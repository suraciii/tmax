//! Browser orchestration of Codex-owned authentication. No token export/import.
use super::*;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::sync::watch;

const ATTEMPT_SECONDS: u64 = 600;
const RPC_SECONDS: u64 = 20;
const MESSAGE_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct Account {
    pub email: Option<String>,
    pub plan: Option<String>,
}
impl Account {
    fn read(value: &Value) -> Result<Option<Self>> {
        let account = &value["account"];
        if account.is_null() {
            return Ok(None);
        }
        if account["type"] != "chatgpt" {
            return Err(err("native_scope_conflict"));
        }
        Ok(Some(Self {
            email: account["email"].as_str().map(str::to_owned),
            plan: account["planType"].as_str().map(str::to_owned),
        }))
    }
    fn differs(&self, other: &Self) -> bool {
        matches!((&self.email, &other.email), (Some(a), Some(b)) if a != b)
    }
}

fn err(code: &str) -> Error {
    let message = match code {
        "native_not_installed" => {
            "Install the official Codex CLI and make its native executable available on PATH"
        }
        "native_login_unsupported" => {
            "This Codex installation does not support the required authentication interface; update the official Codex CLI and retry"
        }
        "native_ambient_login_not_adopted" => {
            "This scope has an existing login not owned by this provider; choose a new independent scope"
        }
        "native_scope_conflict" => {
            "This scope belongs to another authorization; choose an independent Codex scope"
        }
        "login_source_required" => {
            "Choose an existing Codex target or explicitly create an independent Codex scope"
        }
        "identity_mismatch" => {
            "Codex reported a different account; add a separate provider. The affected native scope needs recovery"
        }
        "inputs_changed" => {
            "Configuration changed during sign-in; no configuration was overwritten. Inspect the retained native scope and sign in again"
        }
        "login_failed" => {
            "Codex could not complete sign-in. Retry, or choose device-code sign-in if the browser callback is unavailable"
        }
        "login_expired" => "Sign-in expired; start another attempt",
        "cancelled" => "Sign-in was cancelled",
        "acquisition_in_progress" => {
            "Another native sign-in is in progress; finish or cancel it first"
        }
        "login_unavailable" => {
            "The Codex authentication process did not return a supported response"
        }
        _ => {
            "The sign-in operation could not complete; inspect its code and native scope before retrying"
        }
    };
    Error::new(code, message)
}

struct Plan {
    provider: String,
    revision: String,
    provider_identity: Option<String>,
    via: String,
    target: AgentTarget,
    path: PathBuf,
    new_scope: bool,
    method: String,
    previous: Option<Account>,
    managed: bool,
}

fn private_root(store: &Store) -> PathBuf {
    store.home.join(".codex/tmax")
}

fn prepare(store: &Store, request: &Request) -> Result<Plan> {
    let loaded = config::load(store)?;
    if request
        .if_revision
        .as_ref()
        .is_some_and(|r| *r != loaded.revision)
    {
        return Err(err("inputs_changed"));
    }
    let provider = crate::operations::operand(request, 0)?.to_owned();
    let p = loaded
        .config
        .providers
        .get(&provider)
        .ok_or_else(|| err("unknown_resource"))?;
    if crate::integrations::integration_id(p)? != "codex-subscription" {
        return Err(err("unsupported_authentication"));
    }
    let method = crate::operations::arg(request, "method").unwrap_or("browser");
    if !matches!(method, "browser" | "device") {
        return Err(err("invalid_arguments"));
    }
    let new_scope = crate::operations::arg(request, "new-codex") == Some("true");
    let supplied = crate::operations::arg(request, "via");
    if new_scope && supplied.is_some() {
        return Err(err("invalid_arguments"));
    }
    let (via, target, path) = if new_scope {
        let id = uuid::Uuid::new_v4().to_string();
        let path = private_root(store).join(&id);
        (
            format!("codex-{id}"),
            AgentTarget {
                agent: "codex".into(),
                scope: path.clone(),
                providers: BTreeMap::new(),
                default: None,
                roles: BTreeMap::new(),
            },
            path,
        )
    } else {
        let via = supplied
            .or(match &p.auth {
                Some(Auth::Login { via, .. }) => Some(via.as_str()),
                _ => None,
            })
            .ok_or_else(|| err("login_source_required"))?;
        let target = if let Some(target) = loaded.config.agents.get(via) {
            target.clone()
        } else {
            // Retry a tmax-created scope whose successful login was never saved.
            let retained: Logins = store.read_json(LOGINS)?;
            let record = retained
                .entries
                .iter()
                .rev()
                .find(|r| r.provider == provider && r.target == via && r.agent == "codex")
                .ok_or_else(|| err("unknown_resource"))?;
            let private_root = scope(store, &private_root(store))?;
            let path = scope(store, &record.scope)?;
            if !path.starts_with(private_root) {
                return Err(err("native_scope_conflict"));
            }
            AgentTarget {
                agent: "codex".into(),
                scope: path,
                providers: BTreeMap::new(),
                default: None,
                roles: BTreeMap::new(),
            }
        };
        if target.agent != "codex" {
            return Err(err("unsupported_authentication"));
        }
        let path = scope(store, &target.scope)?;
        (via.to_owned(), target, path)
    };
    let logins: Logins = store.read_json(LOGINS)?;
    if loaded.config.providers.iter().any(|(id, p)| {
        id != &provider && matches!(&p.auth, Some(Auth::Login { via: other, .. }) if other == &via)
    }) || logins
        .entries
        .iter()
        .any(|l| l.provider != provider && l.scope == path && l.state != "signed_out")
    {
        return Err(err("native_scope_conflict"));
    }
    let managed = logins.entries.iter().any(|l| {
        l.provider == provider
            && l.target == via
            && l.scope == path
            && l.source_revision == login_source_revision(&target, &path)
    });
    if !managed && auth_paths("codex", &path).iter().any(|p| p.exists()) {
        return Err(err("native_ambient_login_not_adopted"));
    }
    let previous = logins
        .entries
        .iter()
        .rev()
        .find(|l| l.provider == provider && l.account.is_some())
        .and_then(|l| l.account.clone());
    let new_scope = new_scope || !loaded.config.agents.contains_key(&via);
    Ok(Plan {
        provider: provider.clone(),
        provider_identity: crate::inputs::provider_identity(store, &loaded.config, &provider)?,
        revision: loaded.revision,
        via,
        target,
        path,
        new_scope,
        method: method.into(),
        previous,
        managed,
    })
}

#[derive(Clone, Serialize)]
struct Progress {
    attempt: String,
    provider: String,
    state: String,
    via: String,
    path: PathBuf,
    credential_owner: &'static str,
    method: String,
    expires_at: String,
    authorization_url: Option<String>,
    user_code: Option<String>,
    account: Option<Account>,
    native_authorization_may_have_changed: bool,
    code: Option<String>,
    message: Option<String>,
    save: &'static str,
    activation: Value,
}
impl Progress {
    fn pending(&self) -> bool {
        matches!(self.state.as_str(), "starting" | "waiting" | "cancelling")
    }
    fn clear_challenge(&mut self) {
        self.authorization_url = None;
        self.user_code = None;
    }
}
struct Flow {
    progress: Mutex<Progress>,
    cancel: watch::Sender<bool>,
}
type Flows = BTreeMap<(PathBuf, String), Arc<Flow>>;
fn flows() -> &'static Mutex<Flows> {
    static FLOWS: OnceLock<Mutex<Flows>> = OnceLock::new();
    FLOWS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(crate) fn command(store: &Store, request: &Request) -> Result<Value> {
    let provider = crate::operations::operand(request, 0)?;
    let key = (store.root.clone(), provider.to_owned());
    let operation = request.command.last().map(String::as_str).unwrap_or("");
    let mut registry = flows().lock().expect("login registry");
    if operation == "login-status" || operation == "login-cancel" {
        if let Some(flow) = registry.get(&key) {
            let mut progress = flow.progress.lock().expect("login progress");
            if operation == "login-cancel" {
                if crate::operations::required(request, "attempt")? != progress.attempt {
                    return Err(err("inputs_changed"));
                }
                if !request.dry_run && progress.pending() {
                    progress.state = "cancelling".into();
                    progress.clear_challenge();
                    let _ = flow.cancel.send(true);
                }
            }
            if operation == "login-status" && progress.state == "succeeded" {
                let retained = retained_status(store, provider)?;
                if retained["state"] != "succeeded" || retained["via"] != progress.via {
                    return Ok(retained);
                }
            }
            return Ok(json!(progress.clone()));
        }
        if operation == "login-cancel" {
            return Err(err("unknown_resource"));
        }
        return retained_status(store, provider);
    }
    if operation != "login-start" {
        return Err(err("unsupported_operation"));
    }
    let mut plan = prepare(store, request)?;
    if request.dry_run {
        return Ok(
            json!({"provider":provider,"new_scope":plan.new_scope,"via":if plan.new_scope {None} else {Some(&plan.via)},"path":if plan.new_scope {None} else {Some(&plan.path)},"credential_owner":"codex","method":plan.method,"processes_started":false,"native_authorization_may_change_on_execution":true}),
        );
    }
    if let Some(flow) = registry.get(&key) {
        let progress = flow.progress.lock().expect("login progress");
        if progress.pending() {
            return Ok(json!(progress.clone()));
        }
    }
    let authentication = store
        .try_lock("native-authentication")?
        .ok_or_else(|| err("acquisition_in_progress"))?;
    let (cancel, mut cancelled) = watch::channel(false);
    let progress = Progress {
        attempt: uuid::Uuid::new_v4().to_string(),
        provider: plan.provider.clone(),
        state: "starting".into(),
        via: plan.via.clone(),
        path: plan.path.clone(),
        credential_owner: "codex",
        method: plan.method.clone(),
        expires_at: (chrono::Utc::now() + chrono::Duration::seconds(ATTEMPT_SECONDS as i64))
            .to_rfc3339(),
        authorization_url: None,
        user_code: None,
        account: None,
        native_authorization_may_have_changed: false,
        code: None,
        message: None,
        save: "not_requested",
        activation: Value::Null,
    };
    let initial = json!(progress);
    let flow = Arc::new(Flow {
        progress: Mutex::new(progress),
        cancel,
    });
    if registry.len() >= 64 {
        registry.retain(|_, flow| flow.progress.lock().expect("login progress").pending());
    }
    registry.insert(key, flow.clone());
    let store = store.clone();
    tokio::spawn(async move {
        let _authentication = authentication;
        let mut rpc = None;
        let result = tokio::select! {
            biased;
            _ = cancelled.changed() => Err(err("cancelled")),
            result = tokio::time::timeout(Duration::from_secs(ATTEMPT_SECONDS), run(&store, &mut plan, &flow, &mut rpc)) => result.unwrap_or_else(|_| Err(err("login_expired"))),
        };
        // Reap the exact native process before advertising a terminal outcome.
        if let Some(mut rpc) = rpc {
            rpc.stop().await;
        }
        let mut p = flow.progress.lock().expect("login progress");
        p.clear_challenge();
        if let Err(error) = result {
            if p.save == "saved" {
                p.state = "incomplete".into();
                p.code = Some("saved_not_active".into());
                p.message = Some("Login reference was saved but runtime reconciliation did not complete; refresh and inspect before retrying".into());
                return;
            }
            p.state = match error.code.as_str() {
                "cancelled" => "cancelled",
                "login_expired" => "expired",
                _ => "failed",
            }
            .into();
            p.code = Some(error.code);
            p.message = Some(error.message);
        } else {
            p.state = "succeeded".into();
        }
    });
    Ok(initial)
}

fn retained_status(store: &Store, provider: &str) -> Result<Value> {
    let loaded = config::load(store)?;
    let p = loaded
        .config
        .providers
        .get(provider)
        .ok_or_else(|| err("unknown_resource"))?;
    let logins: Logins = store.read_json(LOGINS)?;
    let record = logins.entries.iter().rev().find(|l| l.provider == provider);
    let committed = record.is_some_and(|r| r.state == "signed_in" && matches!(&p.auth, Some(Auth::Login{via,session_ref}) if via == &r.target && session_ref.as_deref() == Some(&r.session_ref)));
    Ok(
        json!({"provider":provider,"state":if committed {"succeeded"} else if record.is_some_and(|r| matches!(r.state.as_str(), "native_effect_uncertain" | "authenticated_not_saved")) {"interrupted"} else {"idle"},"attempt":null,"account":record.and_then(|r|r.account.as_ref()),"via":record.map(|r|&r.target),"path":record.map(|r|&r.scope),"credential_owner":"codex","evidence":"retained_login_result_not_live_entitlement"}),
    )
}

struct Rpc {
    child: Option<Child>,
    reader: BufReader<Box<dyn AsyncRead + Unpin + Send>>,
    writer: Box<dyn AsyncWrite + Unpin + Send>,
    sequence: u64,
    completed: Option<Value>,
    login_id: Option<String>,
}
impl Rpc {
    fn spawn(path: &Path) -> Result<Self> {
        let mut command = native_command("codex", path)?;
        command
            .args(["app-server", "--listen", "stdio://"])
            .env_remove("OPENAI_API_KEY")
            .env_remove("CODEX_API_KEY")
            .env_remove("CODEX_ACCESS_TOKEN")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn().map_err(|_| err("native_not_installed"))?;
        Ok(Self {
            reader: BufReader::new(Box::new(child.stdout.take().expect("stdout"))),
            writer: Box::new(child.stdin.take().expect("stdin")),
            child: Some(child),
            sequence: 0,
            completed: None,
            login_id: None,
        })
    }
    async fn send(&mut self, message: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&message).map_err(|_| err("login_unavailable"))?;
        bytes.push(b'\n');
        self.writer
            .write_all(&bytes)
            .await
            .map_err(|_| err("login_unavailable"))?;
        self.writer
            .flush()
            .await
            .map_err(|_| err("login_unavailable"))
    }
    async fn message(&mut self) -> Result<Value> {
        let mut bytes = zeroize::Zeroizing::new(Vec::new());
        let length = (&mut self.reader)
            .take(MESSAGE_BYTES + 1)
            .read_until(b'\n', &mut bytes)
            .await
            .map_err(|_| err("login_unavailable"))?;
        if length == 0 || length as u64 > MESSAGE_BYTES || bytes.last() != Some(&b'\n') {
            return Err(err("login_unavailable"));
        }
        serde_json::from_slice(&bytes).map_err(|_| err("login_unavailable"))
    }
    async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.sequence += 1;
        let id = self.sequence;
        tokio::time::timeout(Duration::from_secs(RPC_SECONDS), async {
            self.send(json!({"id":id,"method":method,"params":params}))
                .await?;
            loop {
                let message = self.message().await?;
                if message["id"] == id {
                    if let Some(error) = message.get("error") {
                        return Err(err(if error["code"] == -32601 || error["code"] == -32602 {
                            "native_login_unsupported"
                        } else {
                            "login_failed"
                        }));
                    }
                    return message
                        .get("result")
                        .cloned()
                        .ok_or_else(|| err("login_unavailable"));
                }
                if message["method"] == "account/login/completed" {
                    self.completed = Some(message["params"].clone());
                }
            }
        })
        .await
        .map_err(|_| err("login_unavailable"))?
    }
    async fn wait_login(&mut self, id: &str) -> Result<()> {
        loop {
            let completion = if let Some(completion) = self.completed.take() {
                completion
            } else {
                let message = self.message().await?;
                if message["method"] != "account/login/completed" {
                    continue;
                }
                message["params"].clone()
            };
            if completion["loginId"] != id {
                continue;
            }
            return if completion["success"] == true {
                Ok(())
            } else {
                Err(err("login_failed"))
            };
        }
    }
    async fn stop(&mut self) {
        if let Some(id) = self.login_id.take() {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                self.call("account/login/cancel", json!({"loginId":id})),
            )
            .await;
        }
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }
}

fn challenge(result: &Value) -> Result<(String, Option<String>, String)> {
    let device = result["type"] == "chatgptDeviceCode";
    if !device && result["type"] != "chatgpt" {
        return Err(err("login_unavailable"));
    }
    let value = result[if device { "verificationUrl" } else { "authUrl" }]
        .as_str()
        .ok_or_else(|| err("login_unavailable"))?;
    let url = url::Url::parse(value).map_err(|_| err("login_unavailable"))?;
    if url.scheme() != "https"
        || !matches!(url.host_str(), Some("auth.openai.com" | "chatgpt.com"))
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(err("login_unavailable"));
    }
    let code = if device {
        Some(
            result["userCode"]
                .as_str()
                .filter(|v| !v.is_empty() && v.len() < 128)
                .ok_or_else(|| err("login_unavailable"))?
                .to_owned(),
        )
    } else {
        None
    };
    let id = result["loginId"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() < 128)
        .ok_or_else(|| err("login_unavailable"))?;
    Ok((value.into(), code, id.into()))
}

async fn run(store: &Store, plan: &mut Plan, flow: &Flow, process: &mut Option<Rpc>) -> Result<()> {
    if plan.new_scope && !plan.path.exists() {
        storage::ensure_directory(&plan.path)?;
        storage::atomic_create(
            &plan.path.join("config.toml"),
            b"cli_auth_credentials_store = \"file\"\n",
        )?;
    }
    plan.path = scope(store, &plan.path)?;
    plan.target.scope = plan.path.clone();
    flow.progress.lock().expect("login progress").path = plan.path.clone();
    // The managed authentication protocol determines support. A CLI version
    // difference alone must not prevent the user from signing in.
    *process = Some(Rpc::spawn(&plan.path)?);
    run_protocol(
        store,
        plan,
        flow,
        process.as_mut().expect("started process"),
    )
    .await
}

async fn run_protocol(store: &Store, plan: &Plan, flow: &Flow, rpc: &mut Rpc) -> Result<()> {
    rpc.call("initialize", json!({"clientInfo":{"name":"tmax","title":"tmax","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":false}})).await?;
    rpc.send(json!({"method":"initialized","params":{}}))
        .await?;
    let before = Account::read(
        &rpc.call("account/read", json!({"refreshToken":false}))
            .await?,
    )?;
    if before.is_some() && !plan.managed {
        return Err(err("native_ambient_login_not_adopted"));
    }
    if let (Some(previous), Some(before)) = (&plan.previous, &before)
        && previous.differs(before)
    {
        return Err(err("identity_mismatch"));
    }
    let session = format!("native-session-{}", uuid::Uuid::new_v4());
    {
        let mut p = flow.progress.lock().expect("login progress");
        if *flow.cancel.borrow() {
            return Err(err("cancelled"));
        }
        let _configuration = store.lock("configuration")?;
        config::guard(store, &plan.revision)?;
        if crate::inputs::provider_identity(store, &config::load(store)?.config, &plan.provider)?
            != plan.provider_identity
        {
            return Err(err("inputs_changed"));
        }
        let _routing = store.lock("routing")?;
        let _native = store.lock("native")?;
        let mut logins: Logins = store.read_json(LOGINS)?;
        logins.entries.retain(|l| {
            !(l.provider == plan.provider && l.target == plan.via && l.scope == plan.path)
        });
        logins.entries.push(LoginRecord {
            provider: plan.provider.clone(),
            target: plan.via.clone(),
            scope: plan.path.clone(),
            agent: "codex".into(),
            native_provider: "openai".into(),
            credential_id: None,
            session_ref: session.clone(),
            state: "native_effect_uncertain".into(),
            source_revision: login_source_revision(&plan.target, &plan.path),
            account: plan.previous.clone().or(before.clone()),
        });
        begin_native_auth(store, &plan.provider, &logins).map_err(|_| err("recovery_required"))?;
        p.native_authorization_may_have_changed = true;
    }
    let login = rpc
        .call(
            "account/login/start",
            json!({"type":if plan.method == "device" {"chatgptDeviceCode"} else {"chatgpt"}}),
        )
        .await?;
    let (url, code, id) = challenge(&login)?;
    rpc.login_id = Some(id.clone());
    {
        let mut p = flow.progress.lock().expect("login progress");
        if *flow.cancel.borrow() {
            return Err(err("cancelled"));
        }
        p.state = "waiting".into();
        p.authorization_url = Some(url);
        p.user_code = code;
    }
    rpc.wait_login(&id).await?;
    rpc.login_id = None;
    let account = Account::read(
        &rpc.call("account/read", json!({"refreshToken":false}))
            .await?,
    )?
    .ok_or_else(|| err("login_failed"))?;
    if plan
        .previous
        .as_ref()
        .or(before.as_ref())
        .is_some_and(|a| a.differs(&account))
    {
        return Err(err("identity_mismatch"));
    }
    let (config, revision, previous) = {
        let mut p = flow.progress.lock().expect("login progress");
        if *flow.cancel.borrow() {
            return Err(err("cancelled"));
        }
        let result = commit(store, plan, &session, &account)?;
        p.save = "saved";
        p.account = Some(account);
        p.clear_challenge();
        result
    };
    {
        let _native = store.lock("native")?;
        let mut logins: Logins = store.read_json(LOGINS)?;
        let record = logins
            .entries
            .iter_mut()
            .find(|r| r.session_ref == session)
            .ok_or_else(|| err("recovery_required"))?;
        record.state = "signed_in".into();
        store.write_json(LOGINS, &logins)?;
    }
    crate::routing::reconcile(store, &previous, &config, &[])?;
    let mut data = json!({});
    crate::operations::activate_saved(store, &config, &revision, &mut data).await;
    let incomplete = data.get("_code").is_some();
    flow.progress.lock().expect("login progress").activation = data;
    if incomplete {
        Err(err("saved_not_active"))
    } else {
        Ok(())
    }
}

fn commit(
    store: &Store,
    plan: &Plan,
    session: &str,
    account: &Account,
) -> Result<(Config, String, Config)> {
    let loaded = config::load(store)?;
    if loaded.revision != plan.revision
        || crate::inputs::provider_identity(store, &loaded.config, &plan.provider)?
            != plan.provider_identity
    {
        return Err(err("inputs_changed"));
    }
    let mut document = loaded.document;
    let mut auth = toml_edit::Table::new();
    auth["method"] = toml_edit::value("login");
    auth["via"] = toml_edit::value(&plan.via);
    auth["session_ref"] = toml_edit::value(session);
    document["providers"][&plan.provider]["auth"] = toml_edit::Item::Table(auth);
    if plan.new_scope {
        let mut target = toml_edit::Table::new();
        target["agent"] = toml_edit::value("codex");
        target["scope"] = toml_edit::value(plan.path.to_string_lossy().as_ref());
        if document.get("agents").is_none() {
            document["agents"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        document["agents"][&plan.via] = toml_edit::Item::Table(target);
    }
    let candidate = config::parse(&document.to_string())?;
    {
        let _native = store.lock("native")?;
        let mut logins: Logins = store.read_json(LOGINS)?;
        let record = logins
            .entries
            .iter_mut()
            .find(|l| l.session_ref == session && l.provider == plan.provider)
            .ok_or_else(|| err("recovery_required"))?;
        record.state = "authenticated_not_saved".into();
        record.account = Some(account.clone());
        store.write_json(LOGINS, &logins)?;
    }
    let revision = config::save(store, &plan.revision, &document)?;
    Ok((candidate, revision, loaded.config))
}

#[cfg(test)]
#[path = "browser_login_tests.rs"]
mod tests;
