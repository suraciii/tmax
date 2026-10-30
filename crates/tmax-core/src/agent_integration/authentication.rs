//! Native authorization lineage, durable login metadata, and owner commands.
use super::{
    compatibility::Context,
    document::{self, Format},
    installation::{self, native_command, version},
    scope::{
        MAX_NATIVE_BYTES, auth_paths, digest, omp_main_settings, read_native, scope, target,
        unavailable,
    },
};
use crate::{
    Error, Result,
    config::{self, AgentTarget, Auth, Config},
    storage::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

pub(super) const LOGINS: &str = "native/logins.json";
#[derive(Default, Serialize, Deserialize)]
pub(super) struct Logins {
    pub(super) entries: Vec<LoginRecord>,
}
#[derive(Serialize, Deserialize)]
pub(super) struct LoginRecord {
    pub(super) provider: String,
    pub(super) target: String,
    pub(super) scope: PathBuf,
    pub(super) agent: String,
    pub(super) native_provider: String,
    pub(super) credential_id: Option<String>,
    pub(super) session_ref: String,
    pub(super) state: String,
    pub(super) source_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) account: Option<Account>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct Account {
    pub email: Option<String>,
    pub plan: Option<String>,
}

impl Logins {
    pub(super) fn load(store: &Store) -> Result<Self> {
        store.read_json(LOGINS)
    }

    pub(super) fn rename_target(&mut self, old: &str, new: &str) {
        for login in &mut self.entries {
            if login.target == old {
                login.target = new.into();
            }
        }
    }

    pub(super) fn remove_target(&mut self, id: &str) {
        for login in &mut self.entries {
            if login.target == id {
                login.state = "source_removed".into();
            }
        }
    }

    pub(super) fn save(&self, store: &Store) -> Result<()> {
        store.write_json(LOGINS, self)
    }
}

/// Pure native provider naming; eligibility and error contracts stay with callers.
pub(super) fn native_login_provider(integration: &str, agent: &str) -> Option<&'static str> {
    match (integration, agent) {
        ("codex-subscription", "codex" | "opencode") => Some("openai"),
        ("codex-subscription", "pi" | "omp" | "hermes") => Some("openai-codex"),
        ("kimi-code-plan-cn" | "kimi-code-plan-global", "omp") => Some("kimi-code"),
        _ => None,
    }
}
fn auth_namespace_from_reader(reader: impl std::io::Read, selected: &str) -> Result<bool> {
    struct Namespace<'a>(&'a str);
    impl<'de> serde::de::Visitor<'de> for Namespace<'_> {
        type Value = bool;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a native credential namespace mapping")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<bool, M::Error> {
            let mut present = false;
            while let Some(name) = map.next_key::<String>()? {
                present |= name == self.0;
                map.next_value::<serde::de::IgnoredAny>()?;
            }
            Ok(present)
        }
    }
    let mut deserializer = serde_json::Deserializer::from_reader(reader);
    let found=serde::de::Deserializer::deserialize_map(&mut deserializer,Namespace(selected)).map_err(|_|Error::new("native_auth_metadata_unavailable","Native credential namespace metadata is malformed; credential values were not materialized"))?;
    deserializer.end().map_err(|_| {
        Error::new(
            "native_auth_metadata_unavailable",
            "Native credential namespace metadata contains trailing data",
        )
    })?;
    Ok(found)
}
pub(super) fn auth_namespace_present(path: &Path, selected: &str) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(unavailable()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_NATIVE_BYTES
    {
        return Err(Error::new(
            "native_auth_metadata_unavailable",
            "Native credential namespace metadata is not a bounded regular file",
        ));
    }
    let file = fs::File::open(path).map_err(|_| unavailable())?;
    auth_namespace_from_reader(std::io::Read::take(file, MAX_NATIVE_BYTES + 1), selected)
}
pub fn login_source_agents(integration: &str) -> &'static [&'static str] {
    match integration {
        "codex-subscription" => &["codex", "pi", "omp", "opencode", "hermes"],
        "kimi-code-plan-cn" | "kimi-code-plan-global" => &["omp"],
        _ => &[],
    }
}
pub(super) fn login_source_revision(target: &AgentTarget, path: &Path) -> String {
    digest(format!("{}\0{}", target.agent, path.display()).as_bytes())
}
fn login_provider(config: &Config, provider: &str, target: &AgentTarget) -> Result<&'static str> {
    let provider = config
        .providers
        .get(provider)
        .ok_or_else(|| Error::new("unknown_resource", "Provider is not configured"))?;
    native_login_provider(
        crate::provider_integration::integration_id(provider)?,
        &target.agent,
    ).ok_or_else(|| Error::new(
        "unsupported_authentication",
        "This access product has no qualified native login for the selected agent. API-key entry is not login authentication",
    ))
}
// Version-qualified, read-only metadata projection. Never SELECT `data` or
// identity_key: those columns contain native credential/session material.
async fn omp_oauth_count(path: &Path, provider: &str) -> Result<u64> {
    if installation::executable("sqlite3").is_none() {
        return Err(Error::new(
            "native_metadata_reader_unavailable",
            "Qualified omp account isolation requires the read-only sqlite3 metadata reader",
        ));
    }
    let db = path.join("agent.db");
    if !db.exists() {
        return Ok(0);
    }
    let mut cmd = tokio::process::Command::new("sqlite3");
    cmd.args(["-readonly"]).arg(db).arg(format!("SELECT count(*) FROM auth_credentials WHERE provider='{}' AND credential_type='oauth' AND disabled_cause IS NULL;",provider.replace('\'',"''"))).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(5), cmd.output())
        .await
        .map_err(|_| {
            Error::new(
                "native_metadata_unavailable",
                "Native account metadata inspection timed out",
            )
        })?
        .map_err(|_| {
            Error::new(
                "native_metadata_unavailable",
                "Cannot inspect native account metadata",
            )
        })?;
    if !output.status.success() || output.stdout.len() > 64 {
        return Err(Error::new(
            "native_metadata_unavailable",
            "The scoped native account schema is unavailable; no credential values were selected",
        ));
    }
    std::str::from_utf8(&output.stdout)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .ok_or_else(|| {
            Error::new(
                "native_metadata_unavailable",
                "Native account count is unavailable",
            )
        })
}
async fn opencode_accounts(path: &Path) -> Result<Vec<String>> {
    let mut cmd = native_command("opencode", path)?;
    cmd.args(["auth", "list", "--standalone", "--format", "json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let output = tokio::time::timeout(Duration::from_secs(30), cmd.output())
        .await
        .map_err(|_| {
            Error::new(
                "native_metadata_unavailable",
                "Native private-server account inspection timed out",
            )
        })?
        .map_err(|_| unavailable())?;
    if !output.status.success() || output.stdout.len() > 1024 * 1024 {
        return Err(Error::new(
            "native_metadata_unavailable",
            "Native private-server account metadata is unavailable",
        ));
    }
    let summary: Value = serde_json::from_slice(&output.stdout).map_err(|_| {
        Error::new(
            "native_metadata_unavailable",
            "Native account summary does not match the researched version",
        )
    })?;
    let integrations = summary.as_array().ok_or_else(|| {
        Error::new(
            "native_metadata_unavailable",
            "Native account summary has an unknown shape",
        )
    })?;
    let mut accounts = Vec::new();
    for integration in integrations.iter().filter(|i| i["id"] == "openai") {
        let connections = integration["connections"].as_array().ok_or_else(|| {
            Error::new(
                "native_metadata_unavailable",
                "Native connection metadata is unavailable",
            )
        })?;
        for connection in connections.iter().filter(|c| c["type"] == "credential") {
            if connection["method"] != "oauth" {
                return Err(Error::new(
                    "native_scope_conflict",
                    "The native OpenAI integration has a different saved authentication method",
                ));
            }
            accounts.push(
                connection["id"]
                    .as_str()
                    .ok_or_else(|| {
                        Error::new(
                            "native_metadata_unavailable",
                            "Native credential ID is unavailable",
                        )
                    })?
                    .to_owned(),
            );
        }
    }
    Ok(accounts)
}
async fn codex_authenticated(path: &Path) -> Result<bool> {
    let mut cmd = native_command("codex", path)?;
    cmd.args(["login", "status"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    tokio::time::timeout(Duration::from_secs(10), cmd.status())
        .await
        .map_err(|_| {
            Error::new(
                "native_metadata_unavailable",
                "Native auth status timed out",
            )
        })?
        .map(|s| s.success())
        .map_err(|_| unavailable())
}
pub async fn login(
    store: &Store,
    config: &Config,
    provider: &str,
    via: &str,
    dry_run: bool,
) -> Result<Value> {
    native_auth(store, config, provider, via, dry_run, false).await
}
pub async fn logout(
    store: &Store,
    config: &Config,
    provider: &str,
    via: &str,
    dry_run: bool,
) -> Result<Value> {
    native_auth(store, config, provider, via, dry_run, true).await
}
// No await or owner side effect may separate invalidation from the write-ahead
// record. Invalidation comes first so an interruption after that record can never
// leave facts from the old authorization current. Caller holds routing and native.
pub(super) fn begin_native_auth(
    store: &Store,
    provider: &str,
    logins: &Logins,
) -> std::result::Result<(), Value> {
    crate::inputs::authorization_changed_locked(store, provider).map_err(|error| {
        json!({"_code":"recovery_required","state":"native_not_attempted",
            "native_login_attempted":false,"native_authorization_may_have_changed":false,
            "authorization_invalidation":{"state":"failed","persistence":"unknown","code":error.code},
            "recovery":"Repair authorization-lineage storage before another explicit native owner operation; no native authorization operation was started"})
    })?;
    store.write_json(LOGINS, logins).map_err(|error| {
        json!({"_code":"recovery_required","state":"native_not_attempted",
            "native_login_attempted":false,"native_authorization_may_have_changed":false,
            "authorization_invalidation":{"state":"completed"},
            "native_record":{"state":"unknown","code":error.code},
            "recovery":"Inspect native login metadata before another explicit owner operation; authorization facts were invalidated but no native authorization operation was started"})
    })
}

async fn native_auth(
    store: &Store,
    config: &Config,
    provider: &str,
    via: &str,
    dry_run: bool,
    logout: bool,
) -> Result<Value> {
    let _authentication = if dry_run {
        None
    } else {
        Some(store.try_lock("native-authentication")?.ok_or_else(|| {
            Error::new(
                "acquisition_in_progress",
                "Another native sign-in is in progress",
            )
        })?)
    };
    let target = target(config, via)?;
    let path = scope(store, &target.scope)?;
    let native_provider = login_provider(config, provider, target)?;
    let _configuration = if dry_run {
        None
    } else {
        Some(store.lock("configuration")?)
    };
    let routing = if dry_run {
        None
    } else {
        let saved = config::load(store)?;
        if serde_json::to_value(&saved.config).map_err(|_| unavailable())?
            != serde_json::to_value(config).map_err(|_| unavailable())?
        {
            return Err(Error::new(
                "inputs_changed",
                "The desired configuration changed before native authentication; inspect the current authority",
            ));
        }
        Some(store.lock("routing")?)
    };
    let _lock = if dry_run {
        None
    } else {
        Some(store.lock("native")?)
    };
    let mut logins: Logins = store.read_json(LOGINS)?;
    let source_revision = login_source_revision(target, &path);
    if config.providers.iter().any(|(id, p)| {
        id != provider && matches!(p.auth.as_ref(),Some(Auth::Login{via:other,..}) if other==via)
    }) || logins.entries.iter().any(|l| {
        l.provider != provider
            && l.scope == path
            && l.agent == target.agent
            && l.state != "signed_out"
    }) {
        return Err(Error::new(
            "native_scope_conflict",
            "This native authorization scope is already selected by another provider instance; use an independent supported native scope",
        ));
    }
    let existing = logins.entries.iter().find(|l| {
        l.provider == provider
            && l.target == via
            && l.scope == path
            && l.source_revision == source_revision
    });
    if !logout && existing.is_some_and(|record| record.account.is_some()) {
        return Err(Error::new(
            "browser_login_required",
            "Reauthenticate this account through provider login-start to preserve account comparison",
        ));
    }
    if !logout
        && existing.is_none()
        && target.agent != "opencode"
        && auth_paths(&target.agent, &path).iter().any(|p| p.exists())
    {
        return Err(Error::new(
            "native_ambient_login_not_adopted",
            "This scope already contains native authentication. Use an independent scope for an explicit sign-in; tmax neither reads nor adopts the existing credentials",
        ));
    }
    if logout && existing.is_none() {
        return Err(Error::new(
            "native_ambient_login_not_adopted",
            "No explicit tmax-managed native login belongs to this provider and scope; ambient credentials are not signed out",
        ));
    }
    let mut command = Vec::<String>::new();
    match (target.agent.as_str(), logout) {
        ("codex", false) => command.extend(["login", "--device-auth"].map(String::from)),
        ("codex", true) => command.push("logout".into()),
        ("opencode", false) => command.extend(
            [
                "auth",
                "login",
                "openai",
                "--method",
                "chatgpt-headless",
                "--standalone",
            ]
            .map(String::from),
        ),
        ("opencode", true) => {
            let credential=existing.and_then(|l|l.credential_id.as_deref()).ok_or_else(||Error::new("recovery_required","The selected native OpenCode account ID is unavailable; inspect the owner scope"))?;
            command
                .extend(["auth", "logout", "openai", credential, "--standalone"].map(String::from));
        }
        ("omp", false) => command.extend(["login", native_provider].map(String::from)),
        ("hermes", false) => command.extend(
            [
                "auth",
                "add",
                native_provider,
                "--type",
                "oauth",
                "--no-browser",
            ]
            .map(String::from),
        ),
        ("hermes", true) => {
            command.extend(["logout", "--provider", native_provider].map(String::from))
        }
        ("pi", _) | ("omp", true) => command.push("--no-session".into()),
        _ => {
            return Err(Error::new(
                "native_login_unqualified",
                "No supported native owner command is qualified",
            ));
        }
    }
    if dry_run {
        return Ok(
            json!({"provider":provider,"via":via,"agent":target.agent,"path":path,"dry_run":true,"native_command":command,"native_owner":target.agent,"credential_scope":auth_paths(&target.agent,&path),"identity":"unknown","shared_consumers":config.agents.iter().filter(|(_,a)|a.agent==target.agent && scope(store,&a.scope).is_ok_and(|p|p==path)).map(|(id,_)|id).collect::<Vec<_>>(),"effects":{"native_authorization_may_change_on_execution":true,"native_token_import":false,"processes_started":false},"recovery":"Repeat independent sign-in through the same native owner to reestablish intended authorization; no token rollback or ambient adoption"}),
        );
    }
    let installed = version(&target.agent, &path).await;
    if installed["compatibility"] != "documented" {
        return Err(Error::new(
            "native_version_unknown",
            "Native login commit and recovery are qualified only for the exact researched owner version",
        ));
    }
    if target.agent == "codex" && existing.is_none() && codex_authenticated(&path).await? {
        return Err(Error::new(
            "native_ambient_login_not_adopted",
            "The native configured keyring/file already has authorization for this CODEX_HOME; an ambient session is not adopted",
        ));
    }
    if target.agent == "hermes" && !logout {
        let config_file = path.join("config.yaml");
        let text = read_native(&config_file)?;
        let cfg = document::parse(text.as_deref().unwrap_or(""), Format::Yaml)?;
        if cfg
            .get("auth")
            .and_then(|a| a.get("adopt_external_logins"))
            .is_some_and(|v| !v.is_boolean())
        {
            return Err(Error::new(
                "native_config_invalid",
                "Hermes external-login adoption setting has an invalid type",
            ));
        }
        if existing.is_some_and(|l| l.state == "signed_in") {
            return Err(Error::new(
                "native_login_recovery_unqualified",
                "Hermes auth add creates a rotating credential pool. Sign out this selected independent scope first; adding another pooled login cannot qualify a pin",
            ));
        }
    }
    if target.agent == "omp" {
        let cfg = document::parse(
            read_native(&path.join(omp_main_settings(&path)))?
                .as_deref()
                .unwrap_or(""),
            Format::Yaml,
        )?;
        if std::env::var_os("OMP_AUTH_BROKER_URL").is_some()
            || cfg.pointer("/auth/broker/url").is_some()
        {
            return Err(Error::new(
                "native_login_unqualified",
                "This target uses an auth broker rather than its local native credential scope; select an independent local profile",
            ));
        }
        if path == store.home.join(".omp/agent") && std::env::var_os("XDG_DATA_HOME").is_some() {
            return Err(Error::new(
                "native_scope_ambiguous",
                "The default omp database can follow XDG migration; use an independent explicit agent directory for qualified login ownership",
            ));
        }
        if omp_oauth_count(&path, native_provider).await? > 1 {
            return Err(Error::new(
                "native_scope_conflict",
                "Several native OAuth accounts can rotate in this scope; a single selected authorization is required",
            ));
        }
    }
    let session_ref = existing
        .map(|l| l.session_ref.clone())
        .unwrap_or_else(|| format!("native-session-{}", uuid::Uuid::new_v4()));
    let prior_credential = existing.and_then(|l| l.credential_id.clone());
    if target.agent == "opencode" {
        let accounts = opencode_accounts(&path).await?;
        if accounts.len() > 1
            || accounts
                .first()
                .is_some_and(|id| Some(id) != prior_credential.as_ref())
        {
            return Err(Error::new(
                "native_ambient_login_not_adopted",
                "Use a fresh independent OpenCode native server database; no discovered saved account is adopted",
            ));
        }
    }
    let mut native_credential_id = prior_credential.clone();
    let mut cmd = native_command(&target.agent, &path)?;
    let interactive_ui = target.agent == "pi" || (target.agent == "omp" && logout);
    // Native login remains interactive and owns its browser/device flow. Route
    // native UI to the terminal, never the machine-readable management result.
    #[cfg(unix)]
    {
        let mut terminal = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .map_err(|_| {
                Error::new(
                    "interactive_required",
                    "Native sign-in/sign-out requires an interactive controlling terminal",
                )
            })?;
        if interactive_ui {
            use std::io::Write;
            let action = if logout {
                "/logout".to_owned()
            } else {
                format!("/login {native_provider}")
            };
            writeln!(terminal,"tmax selected native scope {}. Complete {action} in the native owner, select only {native_provider}, then exit the native UI. tmax will not import tokens.",path.display()).map_err(|_|unavailable())?;
        }
        cmd.stdin(Stdio::from(
            terminal.try_clone().map_err(|_| unavailable())?,
        ))
        .stdout(Stdio::from(
            terminal.try_clone().map_err(|_| unavailable())?,
        ))
        .stderr(Stdio::from(terminal));
    }
    #[cfg(windows)]
    {
        let (input, mut output) = crate::secrets::native_console_stdio()?;
        if interactive_ui {
            let action = if logout {
                "/logout".to_owned()
            } else {
                format!("/login {native_provider}")
            };
            crate::secrets::write_console_output(
                &mut output,
                &format!(
                    "tmax selected native scope {}. Complete {action} in the native owner, select only {native_provider}, then exit the native UI. tmax will not import tokens.\r\n",
                    path.display()
                ),
            )?;
        }
        cmd.stdin(Stdio::from(input))
            .stderr(Stdio::from(output.try_clone().map_err(|_| unavailable())?))
            .stdout(Stdio::from(output));
    }
    let prior_account = existing.and_then(|record| record.account.clone());
    logins
        .entries
        .retain(|l| !(l.provider == provider && l.target == via && l.scope == path));
    logins.entries.push(LoginRecord {
        provider: provider.into(),
        target: via.into(),
        scope: path.clone(),
        agent: target.agent.clone(),
        native_provider: native_provider.into(),
        credential_id: prior_credential.clone(),
        session_ref: session_ref.clone(),
        state: "native_effect_uncertain".into(),
        source_revision,
        account: prior_account,
    });
    if let Err(mut failure) = begin_native_auth(store, provider, &logins) {
        failure["provider"] = json!(provider);
        failure["via"] = json!(via);
        failure["agent"] = json!(target.agent);
        failure["path"] = json!(path);
        return Ok(failure);
    }
    // The durable generations and tombstones now cover every cancellation and
    // partial return below. Do not acquire routing while holding native.
    drop(routing);
    let describe = |mut effect: Value| {
        effect["authorization_invalidation"] = json!({"state":"completed"});
        effect["native_authorization_may_have_changed"] = json!(true);
        effect
    };
    if target.agent == "hermes" && !logout {
        let mut guard = match native_command("hermes", &path) {
            Ok(cmd) => cmd,
            Err(e) => {
                return Ok(describe(
                    json!({"_code":"partial_native_apply","provider":provider,"via":via,"state":"native_effect_uncertain","reason_code":e.code,"native_login_attempted":false,"managed_use_blocked":true}),
                ));
            }
        };
        guard
            .args(["config", "set", "auth.adopt_external_logins", "false"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if !guard.status().await.is_ok_and(|s| s.success()) {
            return Ok(describe(
                json!({"_code":"partial_native_apply","provider":provider,"via":via,"path":path,"state":"native_effect_uncertain","native_login_attempted":false,"native_settings_guard":"may_have_changed","managed_use_blocked":true,"recovery":"Inspect native auth.adopt_external_logins before independent owner sign-in; no external tokens are adopted"}),
            ));
        }
    }
    if target.agent == "opencode"
        && !logout
        && let Some(credential) = &prior_credential
    {
        let mut remove = match native_command("opencode", &path) {
            Ok(cmd) => cmd,
            Err(e) => {
                return Ok(describe(
                    json!({"_code":"partial_native_apply","provider":provider,"state":"native_effect_uncertain","reason_code":e.code,"managed_use_blocked":true}),
                ));
            }
        };
        remove
            .args(["auth", "logout", "openai", credential, "--standalone"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if !remove.status().await.is_ok_and(|s| s.success()) {
            return Ok(describe(
                json!({"_code":"partial_native_apply","provider":provider,"via":via,"state":"native_effect_uncertain","managed_use_blocked":true,"recovery":"Complete sign-out of the recorded native account before independent sign-in"}),
            ));
        }
    }
    cmd.args(&command);
    let result = cmd.status().await;
    let mut success = result.as_ref().is_ok_and(|s| s.success());
    if target.agent == "pi" && success {
        success = if let Ok(mut check) = native_command("pi", &path) {
            check
                .args([
                    "auth",
                    "check",
                    "--provider",
                    native_provider,
                    "--json",
                    "--no-refresh",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            match tokio::time::timeout(Duration::from_secs(10), check.output()).await {
                Ok(Ok(output)) if output.stdout.len() < 4096 => {
                    serde_json::from_slice::<Value>(&output.stdout).is_ok_and(|v| {
                        if logout {
                            v["status"] == "not_ready"
                                || (v["status"] == "ready" && v["authType"] != "oauth")
                        } else {
                            v["status"] == "ready"
                                && v["authType"] == "oauth"
                                && path.join("auth.json").exists()
                        }
                    })
                }
                _ => false,
            }
        } else {
            false
        };
    }
    if target.agent == "omp" && success {
        success = omp_oauth_count(&path, native_provider)
            .await
            .is_ok_and(|count| count == if logout { 0 } else { 1 });
    }
    if target.agent == "codex" && success {
        success = codex_authenticated(&path)
            .await
            .is_ok_and(|ready| ready != logout);
    }
    if target.agent == "opencode" && success {
        match opencode_accounts(&path).await {
            Ok(accounts) if logout => success = accounts.is_empty(),
            Ok(accounts) if accounts.len() == 1 => {
                native_credential_id = accounts.into_iter().next()
            }
            _ => success = false,
        }
    }
    let state = if success {
        if logout { "signed_out" } else { "signed_in" }
    } else {
        "native_effect_uncertain"
    };
    if let Some(login) = logins
        .entries
        .iter_mut()
        .find(|l| l.session_ref == session_ref)
    {
        login.state = state.into();
        login.credential_id = native_credential_id;
    }
    let committed = store.write_json(LOGINS, &logins).is_ok();
    Ok(describe(
        json!({"_code":if success&&committed{"awaiting_native_reload"}else{"partial_native_apply"},"provider":provider,"via":via,"agent":target.agent,"path":path,"state":if committed{state}else{"native_changed_local_commit_failed"},"session_ref":if success&&!logout&&committed{Some(session_ref)}else{None},"identity":"unknown","native_owner":target.agent,"native_session_imported":false,"native_authorization_may_have_changed":true,"managed_use_blocked":!success||!committed||logout,"native_consumers":"May already be affected outside tmax; unchanged desired configuration is not rollback evidence","recovery":"Reestablish the intended authorization through this exact native owner and scope; do not copy tokens or adopt another ambient login"}),
    ))
}

/// Only tmax's safe login-operation metadata, never a native credential store.
pub(crate) struct AuthorizationSnapshot {
    logins: Logins,
}
impl AuthorizationSnapshot {
    pub(crate) fn capture(store: &crate::storage::Store) -> Result<Self> {
        Ok(Self {
            logins: store.read_json(LOGINS)?,
        })
    }
    pub(crate) fn assess(
        &self,
        store: &crate::storage::Store,
        config: &Config,
        context: &Context<'_>,
        provider: &str,
    ) -> Result<()> {
        if context.gateway {
            return Ok(());
        }
        let Some(Auth::Login { session_ref, .. }) = &config.providers[provider].auth else {
            return Ok(());
        };
        let id = context.target.ok_or_else(|| {
            Error::new(
                "credential_unavailable",
                "No declared native login owner is selected",
            )
        })?;
        let target = config.agents.get(id).ok_or_else(|| {
            Error::new(
                "credential_unavailable",
                "The native login owner is not configured",
            )
        })?;
        let path = scope(store, &target.scope)?;
        let revision = login_source_revision(target, &path);
        if self.logins.entries.iter().any(|record| {
            record.provider == provider
                && record.target == id
                && record.scope == path
                && record.source_revision == revision
                && record.state == "signed_in"
                && session_ref.as_deref() == Some(record.session_ref.as_str())
        }) {
            Ok(())
        } else {
            Err(Error::new(
                "credential_unavailable",
                "Native authorization is absent, changed or uncertain; explicitly reestablish it through the declared owner",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::compatibility;
    use super::*;
    use crate::Request;
    use std::collections::BTreeMap;
    #[test]
    fn both_kimi_products_keep_native_login_and_hermes_restrictions() {
        for integration in ["kimi-code-plan-cn", "kimi-code-plan-global"] {
            let mut config = Config::empty();
            config.providers.insert(
                "p".into(),
                config::Provider {
                    integration: Some(integration.into()),
                    auth: Some(Auth::ApiKey {
                        key_ref: "test-key".into(),
                    }),
                    ..Default::default()
                },
            );
            let hermes = compatibility::Context {
                agent: "hermes",
                target: None,
                provider: None,
                gateway: false,
            };
            assert_eq!(
                hermes
                    .assess(&config, "p", "messages", "p/m")
                    .unwrap_err()
                    .code,
                "incompatible"
            );
            assert!(
                hermes
                    .assess(&config, "p", "chat-completions", "p/m")
                    .is_ok()
            );
            assert_eq!(login_source_agents(integration), &["omp"]);
            assert_eq!(native_login_provider(integration, "omp"), Some("kimi-code"));
            assert_eq!(native_login_provider(integration, "hermes"), None);
        }
    }

    async fn auth_fixture() -> (tempfile::TempDir, Store, Config, Logins) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let mut config = Config::empty();
        let target = AgentTarget {
            agent: "codex".into(),
            scope: directory.path().join("native"),
            providers: BTreeMap::new(),
            default: None,
            roles: BTreeMap::new(),
        };
        fs::create_dir(&target.scope).unwrap();
        // Login writers retain the resolved scope and fingerprint, not the
        // spelling originally entered in the target configuration.
        let path = scope(&store, &target.scope).unwrap();
        let record = LoginRecord {
            provider: "p".into(),
            target: "owner".into(),
            scope: path.clone(),
            agent: target.agent.clone(),
            native_provider: "openai".into(),
            credential_id: None,
            session_ref: "native-session-test".into(),
            state: "signed_in".into(),
            source_revision: login_source_revision(&target, &path),
            account: None,
        };
        config.agents.insert("owner".into(), target);
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("codex-subscription".into()),
                auth: Some(Auth::Login {
                    via: "owner".into(),
                    session_ref: Some(record.session_ref.clone()),
                }),
                subscription: Some(config::Subscription {
                    plan: Some("Manual plan".into()),
                    auto_renew: Some(false),
                    ..Default::default()
                }),
                models: BTreeMap::from([("m".into(), config::Model::default())]),
                inputs: BTreeMap::from([(
                    "weekly".into(),
                    config::InputDeclaration {
                        kind: "quota".into(),
                        source: "manual".into(),
                        scope: "weekly".into(),
                        models: vec!["m".into()],
                        max_age_seconds: 3600,
                        refresh_interval_seconds: None,
                    },
                )]),
                ..Default::default()
            },
        );
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        crate::inputs::reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        let now = chrono::Utc::now();
        let request = Request {
            command: vec!["provider".into(), "input".into(), "record".into()],
            operands: vec!["p".into(), "weekly".into()],
            arguments: BTreeMap::from([
                (
                    "observed-at".into(),
                    vec![(now - chrono::Duration::seconds(1)).to_rfc3339()],
                ),
                (
                    "valid-until".into(),
                    vec![(now + chrono::Duration::minutes(10)).to_rfc3339()],
                ),
                (
                    "reset-at".into(),
                    vec![(now + chrono::Duration::hours(1)).to_rfc3339()],
                ),
                ("remaining-percent".into(), vec!["75".into()]),
            ]),
            dry_run: false,
            if_revision: None,
        };
        crate::inputs::handle(&store, &config, &request)
            .await
            .unwrap();
        crate::inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let logins = Logins {
            entries: vec![record],
        };
        store.write_json(LOGINS, &logins).unwrap();
        (directory, store, config, logins)
    }

    #[tokio::test]
    async fn native_advice_never_treats_retained_session_reference_as_recovered_authorization() {
        let (_directory, store, config, mut logins) = auth_fixture().await;
        let advice = crate::routing::Advice {
            native: Some(compatibility::Context {
                agent: "codex",
                target: Some("owner"),
                provider: Some("p"),
                gateway: false,
            }),
        };
        let before = crate::routing::explain_native(
            &store,
            &config,
            "p/m",
            "responses",
            None,
            Some(&advice),
        )
        .unwrap();
        assert_eq!(before["selection"]["qualified_model"], "p/m");
        logins.entries[0].state = "native_effect_uncertain".into();
        {
            let _configuration = store.lock("configuration").unwrap();
            let _routing = store.lock("routing").unwrap();
            let _native = store.lock("native").unwrap();
            begin_native_auth(&store, "p", &logins).unwrap();
        }
        assert!(matches!(
            &config.providers["p"].auth,
            Some(Auth::Login {
                session_ref: Some(_),
                ..
            })
        ));
        for evidence in [
            crate::routing::explain_native(
                &store,
                &config,
                "p/m",
                "responses",
                None,
                Some(&advice),
            )
            .unwrap(),
            crate::routing::explain_loaded(
                &store,
                &config,
                "p/m",
                "responses",
                None,
                None,
                Some(&advice),
            )
            .unwrap(),
        ] {
            assert!(evidence["selection"].is_null());
            assert_eq!(
                evidence["excluded_candidates"][0]["reason_code"],
                "credential_unavailable"
            );
            assert!(
                evidence["excluded_candidates"][0]["native_requirement"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("uncertain")
            );
        }
    }

    #[tokio::test]
    async fn dropping_native_owner_future_keeps_saved_and_loaded_facts_invalidated() {
        let (directory, store, config, mut logins) = auth_fixture().await;
        let before = crate::inputs::read_runtime(&store).unwrap();
        let at = crate::inputs::ExactInstant::from_chrono(chrono::Utc::now());
        let old_head = crate::inputs::Snapshot::capture(&store, &config).input("p", "weekly", &at);
        assert_eq!(old_head["availability"]["state"], "available");
        assert_eq!(old_head["remaining_percent"], "75");
        let original_config = fs::read(store.root.join("tmax.toml")).unwrap();
        logins.entries[0].state = "native_effect_uncertain".into();
        let mut operation = Box::pin(async {
            let _configuration = store.lock("configuration").unwrap();
            let routing = store.lock("routing").unwrap();
            let _native = store.lock("native").unwrap();
            begin_native_auth(&store, "p", &logins).unwrap();
            drop(routing);
            // Stand in for the owner awaiting user input: cancellation never
            // reaches an explicit Err callback or the normal return path.
            std::future::pending::<()>().await;
        });
        assert!(futures_util::poll!(operation.as_mut()).is_pending());
        drop(operation);
        let reopened = Store::at(directory.path().to_owned());
        let after = crate::inputs::read_runtime(&reopened).unwrap();
        assert_eq!(
            after.saved.providers["p"].identity,
            before.saved.providers["p"].identity
        );
        assert_ne!(
            after.saved.providers["p"].authorization,
            before.saved.providers["p"].authorization
        );
        assert_eq!(
            after.loaded.as_ref().unwrap().providers["p"].authorization,
            after.saved.providers["p"].authorization
        );
        for snapshot in [
            crate::inputs::Snapshot::capture(&reopened, &config),
            crate::inputs::Snapshot::capture_loaded(&reopened, &config, None),
        ] {
            let head = snapshot.input("p", "weekly", &at);
            assert_eq!(
                head["availability"]["reason_code"],
                "authorization_mismatch"
            );
            assert_ne!(head["current_revision"], old_head["current_revision"]);
            assert!(head.get("remaining_percent").is_none());
        }
        assert_eq!(
            reopened.read_json::<Logins>(LOGINS).unwrap().entries[0].state,
            "native_effect_uncertain"
        );
        assert_eq!(
            fs::read(reopened.root.join("tmax.toml")).unwrap(),
            original_config
        );
        assert_eq!(
            serde_json::to_value(&after.saved.config.providers["p"].subscription).unwrap(),
            serde_json::to_value(&config.providers["p"].subscription).unwrap()
        );
    }

    #[tokio::test]
    async fn failed_lineage_persistence_never_records_or_starts_native_effects() {
        let (_directory, store, _config, mut logins) = auth_fixture().await;
        let original_logins = fs::read(store.root.join(LOGINS)).unwrap();
        // Failure occurs after runtime invalidation, during observation pruning.
        fs::create_dir_all(store.root.join("observations/heads.json")).unwrap();
        logins.entries[0].state = "native_effect_uncertain".into();
        let _configuration = store.lock("configuration").unwrap();
        let _routing = store.lock("routing").unwrap();
        let _native = store.lock("native").unwrap();
        let failure = begin_native_auth(&store, "p", &logins).unwrap_err();
        assert_eq!(failure["_code"], "recovery_required");
        assert_eq!(failure["authorization_invalidation"]["state"], "failed");
        assert_eq!(
            failure["authorization_invalidation"]["persistence"],
            "unknown"
        );
        assert_eq!(failure["state"], "native_not_attempted");
        assert_eq!(failure["native_login_attempted"], false);
        assert_eq!(failure["native_authorization_may_have_changed"], false);
        assert_eq!(fs::read(store.root.join(LOGINS)).unwrap(), original_logins);
    }

    #[tokio::test]
    async fn failed_write_ahead_record_distinguishes_completed_invalidation() {
        let (_directory, store, _config, mut logins) = auth_fixture().await;
        let before = crate::inputs::read_runtime(&store).unwrap();
        fs::remove_file(store.root.join(LOGINS)).unwrap();
        fs::create_dir(store.root.join(LOGINS)).unwrap();
        logins.entries[0].state = "native_effect_uncertain".into();
        let _configuration = store.lock("configuration").unwrap();
        let _routing = store.lock("routing").unwrap();
        let _native = store.lock("native").unwrap();
        let failure = begin_native_auth(&store, "p", &logins).unwrap_err();
        assert_eq!(failure["authorization_invalidation"]["state"], "completed");
        assert_eq!(failure["native_record"]["state"], "unknown");
        assert_eq!(failure["state"], "native_not_attempted");
        assert_eq!(failure["native_authorization_may_have_changed"], false);
        assert_ne!(
            crate::inputs::read_runtime(&store).unwrap().saved.providers["p"].authorization,
            before.saved.providers["p"].authorization
        );
    }

    #[tokio::test]
    async fn native_auth_preflight_rejection_preserves_authorization_facts() {
        let (_directory, store, mut config, _logins) = auth_fixture().await;
        let original = config.clone();
        config.providers.insert(
            "other".into(),
            config::Provider {
                integration: Some("codex-subscription".into()),
                auth: Some(Auth::Login {
                    via: "owner".into(),
                    session_ref: None,
                }),
                ..Default::default()
            },
        );
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        crate::inputs::reconcile(&store, &original, &config, &[]).unwrap();
        let runtime_before = fs::read(store.root.join("routing-state.json")).unwrap();
        let logins_before = fs::read(store.root.join(LOGINS)).unwrap();
        for logout in [false, true] {
            let error = native_auth(&store, &config, "p", "owner", false, logout)
                .await
                .unwrap_err();
            assert_eq!(error.code, "native_scope_conflict");
            assert_eq!(
                fs::read(store.root.join("routing-state.json")).unwrap(),
                runtime_before
            );
            assert_eq!(fs::read(store.root.join(LOGINS)).unwrap(), logins_before);
            let at = crate::inputs::ExactInstant::from_chrono(chrono::Utc::now());
            assert_eq!(
                crate::inputs::Snapshot::capture(&store, &config).input("p", "weekly", &at)["availability"]
                    ["state"],
                "available"
            );
        }
    }
    #[test]
    fn credential_namespace_projection_ignores_all_values() {
        let metadata=br#"{"unrelated":{"type":"oauth","nested":[{"access":"never-materialized"}]},"selected":{"key":"never-materialized"}}"#;
        assert!(auth_namespace_from_reader(metadata.as_slice(), "selected").unwrap());
        assert!(!auth_namespace_from_reader(metadata.as_slice(), "absent").unwrap());
        assert!(!auth_namespace_from_reader(b"{}".as_slice(), "selected").unwrap());
        assert!(
            auth_namespace_from_reader(b"{\"selected\":\"incomplete".as_slice(), "selected")
                .is_err()
        );
    }
}
