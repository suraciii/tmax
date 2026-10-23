//! One detached owner per authority, with independent inference and Web lifecycles.
use crate::{
    Error, Request, Result,
    config::Config,
    proxy::{Runtime, Snapshot},
    storage::Store,
};
use axum::{
    Router,
    body::to_bytes,
    extract::State,
    http::{HeaderMap, Method, StatusCode},
    response::{Html, IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::future::Future;
use std::{
    net::SocketAddr,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{Mutex, Notify, watch},
    task::{JoinHandle, JoinSet},
};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

const MANAGEMENT_PROTOCOL: &str = "tmax-control/1";
const CONTROL_REQUEST_BYTES: usize = 1024 * 1024;
const CONTROL_RESPONSE_BYTES: usize = 12 * 1024 * 1024;
const WEB_REQUEST_BYTES: usize = 1024 * 1024;
const CONTROL_SECONDS: u64 = 30;
#[derive(Serialize, Deserialize)]
struct Call {
    challenge: String,
    authority: PathBuf,
    protocol: String,
    action: String,
    request: Option<Request>,
    revision: Option<String>,
    candidate: Option<Config>,
    unlock: Option<Vec<u8>>,
    expected_process: Option<String>,
}
impl Drop for Call {
    fn drop(&mut self) {
        if let Some(unlock) = &mut self.unlock {
            unlock.zeroize();
        }
    }
}
#[derive(Serialize, Deserialize)]
struct Reply {
    challenge: String,
    authority: PathBuf,
    protocol: String,
    process: String,
    pid: u32,
    version: String,
    result: Result<Value>,
}
struct Surface {
    address: SocketAddr,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}
impl Surface {
    async fn stop(self) {
        let _ = self.shutdown.send(true);
        self.task.abort();
        let _ = self.task.await;
    }
}
struct Shutdown {
    requested: Notify,
    stopping: AtomicBool,
    released: watch::Sender<Option<Result<()>>>,
}
impl Shutdown {
    fn new() -> Self {
        let (released, _) = watch::channel(None);
        Self {
            requested: Notify::new(),
            stopping: AtomicBool::new(false),
            released,
        }
    }
    async fn wait_released(&self) -> Result<()> {
        let mut released = self.released.subscribe();
        loop {
            if let Some(result) = released.borrow_and_update().clone() {
                return result;
            }
            released.changed().await.map_err(|_| {
                failure(
                    "resident_control_failed",
                    "Resident ownership release could not be established",
                )
            })?;
        }
    }
}
struct Resident {
    store: Store,
    process: String,
    runtime: Arc<Runtime>,
    inference: Option<Surface>,
    web: Option<Surface>,
    loaded: Option<(Config, String)>,
    key_state: String,
    shutdown: Arc<Shutdown>,
}
type Shared = Arc<Mutex<Resident>>;
fn failure(code: &str, message: &str) -> Error {
    Error::new(code, message)
}
fn address(value: &str, allow_zero: bool) -> Result<SocketAddr> {
    let address: SocketAddr = value.parse().map_err(|_| {
        failure(
            "invalid_argument",
            "Listener must be a numeric loopback host:port",
        )
    })?;
    if !address.ip().is_loopback() || (!allow_zero && address.port() == 0) {
        return Err(failure(
            "invalid_argument",
            "Listener must use numeric loopback and a nonzero port",
        ));
    }
    Ok(address)
}
fn socket_path(store: &Store) -> PathBuf {
    store.root.join("resident.sock")
}
fn absent() -> Value {
    json!({"resident":{"discovery_state":"absent","reason_code":null,"identity":null,"inference":null,"web":null},"proxy":{"ready":false,"listen":null,"key_ref":null,"config_revision":null},"shared_key":{"loaded_reference":null,"loaded_state":"absent"},"publication":{"snapshot_state":"absent","model_count":null},"limits":crate::proxy::limits(),"unmanaged_clients":"unknown"})
}
impl Resident {
    fn view(&self) -> Value {
        let identity = json!({"config_path":self.store.root.join("tmax.toml"),"process_instance":self.process,"pid":std::process::id(),"executable_version":env!("CARGO_PKG_VERSION"),"management_protocol_version":MANAGEMENT_PROTOCOL,"loaded_revision":self.loaded.as_ref().map(|v|&v.1)});
        let surface = |s: &Option<Surface>| {
            s.as_ref().map_or_else(||json!({"state":"stopped","listen":null,"url":null}),|s|json!({"state":if s.task.is_finished(){"unavailable"}else{"running"},"listen":s.address.to_string(),"url":format!("http://{}",s.address)}))
        };
        let key_ref = self
            .loaded
            .as_ref()
            .and_then(|v| v.0.proxy.as_ref())
            .map(|p| &p.key_ref);
        let count = self.loaded.as_ref().map(|v| {
            crate::proxy::publication(&v.0)["data"]
                .as_array()
                .map_or(0, Vec::len)
        });
        let loaded_network=self.loaded.as_ref().map(|(config,revision)|crate::operations::network_view(config,revision,revision!="absent","loaded")["policy"].clone()).unwrap_or_else(||json!({"snapshot_state":"absent","configuration_revision":null,"configured":null,"effective":null,"reason_code":"no_loaded_snapshot"}));
        let mut inference = surface(&self.inference);
        if inference["state"] == "running" && self.key_state != "ready" {
            inference["state"] = json!("unavailable");
        }
        let dependencies=self.loaded.as_ref().map(|(config,_)|config.agents.iter().filter(|(_,target)|target.providers.values().any(|entry|entry.proxy==Some(true))).map(|(id,_)|json!({"kind":"agent_target","id":id,"scope":"configured_proxy_entry","native_effective_state":"unknown"})).collect::<Vec<_>>()).unwrap_or_default();
        let stopping = self.shutdown.stopping.load(Ordering::Acquire);
        json!({"resident":{"discovery_state":if stopping{"unavailable"}else{"compatible"},"reason_code":if stopping{json!("resident_stopping")}else{Value::Null},"identity":identity,"inference":inference,"web":surface(&self.web),"caller_version":env!("CARGO_PKG_VERSION"),"version_relation":"same"},"proxy":{"ready":!stopping&&self.inference.as_ref().is_some_and(|s|!s.task.is_finished())&&self.key_state=="ready","listen":self.inference.as_ref().map(|s|s.address.to_string()),"key_ref":key_ref,"config_revision":self.loaded.as_ref().map(|v|&v.1)},"shared_key":{"loaded_reference":key_ref,"loaded_state":self.key_state},"publication":{"snapshot_state":if self.loaded.is_some(){"available"}else{"absent"},"model_count":count},"network":{"loaded":loaded_network},"dependencies":dependencies,"active_requests":self.runtime.active(),"sessions":self.runtime.binding_summary(),"limits":crate::proxy::limits(),"unmanaged_clients":"unknown"})
    }
    async fn activate(&mut self, revision: Option<&str>) -> Result<Value> {
        let loaded = crate::config::load(&self.store)?;
        if revision.is_some_and(|r| r != loaded.revision) {
            return Err(failure(
                "inputs_changed",
                "Configuration changed before activation",
            ));
        }
        if let (Some(listener), Some(proxy)) = (&self.inference, &loaded.config.proxy)
            && address(&proxy.listen, false)? != listener.address
        {
            return Err(failure(
                "listener_restart_required",
                "Saved listener address requires explicit proxy restart",
            ));
        }
        let previous = self.runtime.snapshot.read().expect("snapshot lock").clone();
        let metadata = crate::catalog::read(&self.store, "accepted")?.map(Arc::new);
        let snapshot = if let Some(proxy) = &loaded.config.proxy {
            let retained = previous
                .as_ref()
                .filter(|s| {
                    s.config
                        .proxy
                        .as_ref()
                        .is_some_and(|p| p.key_ref == proxy.key_ref)
                })
                .map(|s| s.key.clone());
            let unlock = self.runtime.unlock.read().expect("unlock lock").clone();
            let key = if retained.is_some() {
                retained
            } else if unlock.is_some() {
                crate::proxy::credential(self.store.clone(), proxy.key_ref.clone(), unlock.clone())
                    .await
                    .ok()
            } else {
                None
            };
            self.key_state = if key.as_ref().is_some_and(|k| !k.is_empty()) {
                "ready"
            } else if unlock.is_none() {
                "locked"
            } else {
                "unavailable"
            }
            .into();
            key.filter(|key| !key.is_empty())
                .map(|key| {
                    crate::integrations::client(&loaded.config).map(|client| {
                        Arc::new(Snapshot {
                            client,
                            config: loaded.config.clone(),
                            revision: loaded.revision.clone(),
                            key,
                            metadata,
                        })
                    })
                })
                .transpose()?
        } else {
            self.key_state = "absent".into();
            None
        };
        let runtime = self.runtime.clone();
        let mut authority = runtime.snapshot.write().expect("snapshot lock");
        let changes = runtime.activate_bindings(&loaded.config, true)?;
        crate::inputs::activate(&self.store, &loaded.config, &loaded.revision)?;
        runtime.activate_bindings(&loaded.config, false)?;
        *authority = snapshot;
        drop(authority);
        self.loaded = Some((loaded.config, loaded.revision));
        let view = self.view();
        let blocked = self.inference.is_some()
            && self
                .loaded
                .as_ref()
                .is_some_and(|(config, _)| config.proxy.is_some())
            && self.key_state != "ready";
        Ok(
            json!({"_code":if blocked{json!("runtime_blocked")}else{Value::Null},"activation":"active","revision":self.loaded.as_ref().map(|v|&v.1),"serving_state":if self.inference.is_none(){"stopped"}else if self.key_state=="ready"{"ready"}else{"blocked"},"impact":changes,"resident":view["resident"],"proxy":view["proxy"],"shared_key":view["shared_key"],"next_action":if blocked{json!("Explicitly restart with protected unlock input, or explicitly replace missing shared-key material")}else{Value::Null}}),
        )
    }
    async fn start_inference(
        &mut self,
        restart: bool,
        force: bool,
        provided_unlock: Option<Zeroizing<Vec<u8>>>,
    ) -> Result<Value> {
        if let Some(listener) = self.inference.as_ref().filter(|_| !restart) {
            if listener.task.is_finished() {
                return Err(failure(
                    "listener_unavailable",
                    "Inference listener is unavailable; inspect the owned service and explicitly restart",
                ));
            }
            return Ok(self.lifecycle("inference", "reused", None));
        }
        let current = crate::config::load(&self.store)?;
        let proxy = current.config.proxy.as_ref().ok_or_else(|| {
            failure(
                "proxy_unconfigured",
                "Configure the proxy explicitly before starting it",
            )
        })?;
        let requested = address(&proxy.listen, false)?;
        // Validate cold-start credentials before touching an existing listener.
        let unlock =
            provided_unlock.or_else(|| self.runtime.unlock.read().expect("unlock lock").clone());
        let key =
            crate::proxy::credential(self.store.clone(), proxy.key_ref.clone(), unlock.clone())
                .await?;
        if key.is_empty() {
            return Err(failure(
                "credential_unavailable",
                "Shared key is unavailable; explicitly replace it",
            ));
        }
        let client = crate::integrations::client(&current.config)?;
        let metadata = crate::catalog::read(&self.store, "accepted")?.map(Arc::new);
        let same = self
            .inference
            .as_ref()
            .is_some_and(|s| s.address == requested);
        let prepared = if same {
            None
        } else {
            Some(TcpListener::bind(requested).await.map_err(|_|failure("endpoint_occupied","Requested inference endpoint is occupied or unavailable; no fallback attempted"))?)
        };
        let interrupted = if self.inference.is_some() {
            self.runtime.stop(force)?
        } else {
            0
        };
        if let Some(listener) = self.inference.take() {
            listener.stop().await;
        }
        let listener = if let Some(listener) = prepared {
            listener
        } else {
            match TcpListener::bind(requested).await {
                Ok(listener) => listener,
                Err(_) => {
                    return Ok(
                        json!({"_code":"saved_not_active","lifecycle":{"surface":"inference","action":"stopped"},"interrupted_requests":interrupted,"error":{"code":"endpoint_occupied","message":"Inference stopped but requested endpoint could not be rebound; inspect conflict and explicitly start"},"resident":self.view()["resident"]}),
                    );
                }
            }
        };
        {
            let mut authority = self.runtime.snapshot.write().expect("snapshot lock");
            if let Err(error) =
                crate::inputs::activate(&self.store, &current.config, &current.revision).and_then(
                    |_| {
                        self.runtime
                            .activate_bindings(&current.config, false)
                            .map(|_| ())
                    },
                )
            {
                return Ok(
                    json!({"_code":"saved_not_active","lifecycle":{"surface":"inference","action":"stopped"},"interrupted_requests":interrupted,"error":error}),
                );
            }
            *authority = Some(Arc::new(Snapshot {
                config: current.config.clone(),
                revision: current.revision.clone(),
                key,
                client,
                metadata,
            }));
        }
        *self.runtime.unlock.write().expect("unlock lock") = unlock;
        self.loaded = Some((current.config, current.revision));
        self.key_state = "ready".into();
        self.runtime.open();
        let (shutdown, mut cancelled) = watch::channel(false);
        let runtime = self.runtime.clone();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, crate::proxy::router(runtime))
                .with_graceful_shutdown(async move {
                    let _ = cancelled.changed().await;
                })
                .await;
        });
        self.inference = Some(Surface {
            address: requested,
            shutdown,
            task,
        });
        let mut value = self.lifecycle(
            "inference",
            if restart { "restarted" } else { "started" },
            Some(requested),
        );
        value["interrupted_requests"] = json!(interrupted);
        Ok(value)
    }
    fn lifecycle(&self, surface: &str, action: &str, requested: Option<SocketAddr>) -> Value {
        let mut value = self.view();
        value["lifecycle"] = json!({"surface":surface,"action":action,"requested_listen":requested.map(|v|v.to_string()),"actual_listen":if surface=="inference"{self.inference.as_ref()}else{self.web.as_ref()}.map(|v|v.address.to_string())});
        if action == "reused" {
            value["outcome"] = json!("resident_reused");
        }
        value
    }
    fn maybe_shutdown(&self) {
        if self.inference.is_none() && self.web.is_none() {
            self.shutdown.stopping.store(true, Ordering::Release);
            self.shutdown.requested.notify_one();
        }
    }
}

#[cfg(unix)]
async fn rpc_with_unlock(
    store: &Store,
    action: &str,
    request: Option<Request>,
    revision: Option<String>,
    unlock: Option<&[u8]>,
) -> Result<Option<Value>> {
    rpc_with_candidate(store, action, request, revision, unlock, None).await
}
#[cfg(unix)]
async fn rpc_with_candidate(
    store: &Store,
    action: &str,
    request: Option<Request>,
    revision: Option<String>,
    unlock: Option<&[u8]>,
    candidate: Option<Config>,
) -> Result<Option<Value>> {
    let expected = if action == "status" {
        None
    } else {
        let Some(identity) = rpc_exchange(store, "status", None, None, None, None, None).await?
        else {
            return Ok(None);
        };
        if identity["resident"]["reason_code"] == "resident_stopping" {
            return Err(failure(
                "resident_stopping",
                "The identified resident is completing an explicit stop; wait for that stop's result before starting again",
            ));
        }
        if identity["resident"]["discovery_state"] != "compatible" {
            return Err(failure(
                "resident_incompatible",
                "Use the running resident's compatible CLI or Web controls before starting this executable",
            ));
        }
        Some(
            identity["resident"]["identity"]["process_instance"]
                .as_str()
                .ok_or_else(|| {
                    failure(
                        "resident_identity_unavailable",
                        "Resident process instance is unavailable",
                    )
                })?
                .to_owned(),
        )
    };
    rpc_exchange(
        store, action, request, revision, unlock, expected, candidate,
    )
    .await
}
#[cfg(unix)]
async fn rpc_exchange(
    store: &Store,
    action: &str,
    request: Option<Request>,
    revision: Option<String>,
    unlock: Option<&[u8]>,
    expected_process: Option<String>,
    candidate: Option<Config>,
) -> Result<Option<Value>> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let path = socket_path(store);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(meta) if meta.file_type().is_socket() && meta.uid() == unsafe { libc::geteuid() } => {}
        Ok(_) => {
            return Err(failure(
                "resident_identity_unavailable",
                "Control endpoint is not an owned Unix socket; no takeover attempted",
            ));
        }
        Err(_) => {
            return Err(failure(
                "resident_identity_unavailable",
                "Cannot inspect resident control endpoint",
            ));
        }
    }
    let stream = tokio::net::UnixStream::connect(path).await;
    let mut stream = match stream {
        Ok(v) => v,
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ) =>
        {
            return Ok(None);
        }
        Err(_) => {
            return Err(failure(
                "resident_identity_unavailable",
                "Cannot connect to the resident control endpoint",
            ));
        }
    };
    let peer = stream.peer_cred().map_err(|_| {
        failure(
            "resident_identity_unavailable",
            "Cannot establish resident peer identity",
        )
    })?;
    if peer.uid() != unsafe { libc::geteuid() } {
        return Err(failure(
            "resident_identity_unavailable",
            "Resident socket peer is not the authority owner",
        ));
    }
    let challenge = Uuid::new_v4().to_string();
    let call = Call {
        challenge: challenge.clone(),
        authority: store.root.clone(),
        protocol: MANAGEMENT_PROTOCOL.into(),
        action: action.into(),
        request,
        revision,
        candidate,
        unlock: unlock.map(<[u8]>::to_vec),
        expected_process: expected_process.clone(),
    };
    let bytes = Zeroizing::new(
        serde_json::to_vec(&call)
            .map_err(|_| failure("invalid_argument", "Cannot encode management request"))?,
    );
    if bytes.len() > CONTROL_REQUEST_BYTES {
        return Err(failure(
            "request_too_large",
            "Management request exceeds its bounded size",
        ));
    }
    let result = tokio::time::timeout(Duration::from_secs(CONTROL_SECONDS), async {
        stream.write_u32(bytes.len() as u32).await?;
        stream.write_all(&bytes).await?;
        let count = stream.read_u32().await? as usize;
        if count > CONTROL_RESPONSE_BYTES {
            return Err(std::io::Error::other("control response limit"));
        }
        let mut bytes = vec![0; count];
        stream.read_exact(&mut bytes).await?;
        Ok::<_, std::io::Error>(bytes)
    })
    .await
    .map_err(|_| {
        failure(
            "resident_identity_unavailable",
            "Resident control did not complete within its finite deadline",
        )
    })?
    .map_err(|_| {
        failure(
            "resident_identity_unavailable",
            "Resident control exchange failed",
        )
    })?;
    let reply: Reply = serde_json::from_slice(&result).map_err(|_| {
        failure(
            "resident_identity_unavailable",
            "Endpoint did not return a recognized resident identity",
        )
    })?;
    if reply.challenge != challenge
        || reply.authority != store.root
        || reply.process.is_empty()
        || reply.process.len() > 128
        || reply.pid == 0
        || peer.pid().is_some_and(|pid| pid as u32 != reply.pid)
        || expected_process.is_some_and(|expected| expected != reply.process)
    {
        return Err(failure(
            "resident_identity_unavailable",
            "Resident identity challenge, authority, or process did not match",
        ));
    }
    let incompatible = reply.protocol != MANAGEMENT_PROTOCOL;
    if incompatible && action != "status" {
        return Err(failure(
            "resident_incompatible",
            "Resident management protocol is incompatible; use its compatible CLI or Web to stop it before starting this executable",
        ));
    }
    let mut value = reply.result?;
    if let Some(resident) = value.get_mut("resident") {
        resident["caller_version"] = json!(env!("CARGO_PKG_VERSION"));
        resident["version_relation"] = json!(if reply.version == env!("CARGO_PKG_VERSION") {
            "same"
        } else {
            "different"
        });
        if incompatible {
            resident["discovery_state"] = json!("incompatible");
            resident["reason_code"] = json!("management_protocol_mismatch");
        }
    }
    Ok(Some(value))
}
#[cfg(unix)]
async fn rpc(
    store: &Store,
    action: &str,
    request: Option<Request>,
    revision: Option<String>,
) -> Result<Option<Value>> {
    rpc_with_unlock(store, action, request, revision, None).await
}
#[cfg(not(unix))]
async fn rpc_with_unlock(
    store: &Store,
    action: &str,
    request: Option<Request>,
    revision: Option<String>,
    _unlock: Option<&[u8]>,
) -> Result<Option<Value>> {
    rpc(store, action, request, revision).await
}
#[cfg(not(unix))]
async fn rpc(
    _store: &Store,
    _action: &str,
    _request: Option<Request>,
    _revision: Option<String>,
) -> Result<Option<Value>> {
    Err(failure(
        "platform_unavailable",
        "Resident control requires the supported Unix local-IPC platform",
    ))
}

pub async fn status(store: &Store) -> Result<Value> {
    let mut value = match rpc(store, "status", None, None).await {
        Ok(Some(v)) => v,
        Ok(None) => absent(),
        Err(e) => {
            let mut value = absent();
            value["resident"]["discovery_state"] = json!(if e.code == "resident_incompatible" {
                "incompatible"
            } else {
                "unidentified"
            });
            value["resident"]["reason_code"] = json!(e.code);
            value
        }
    };
    value["autostart"]=autostart_status(store).unwrap_or_else(|error|json!({"snapshot_state":"unavailable","reason_code":error.code,"reconciliation":"not_requested","repair":"proxy autostart enable|disable"}));
    match crate::config::load(store) {
        Ok(file) => {
            value["shared_key"]["file_reference"] =
                json!(file.config.proxy.as_ref().map(|proxy| &proxy.key_ref));
            value["saved_revision"] = json!(file.revision);
            value["network"]["file"] =
                crate::operations::network_view(&file.config, &file.revision, file.exists, "file")
                    ["policy"]
                    .clone();
        }
        Err(error) => {
            value["saved_configuration"] = json!({"snapshot_state":"unavailable","error":error});
        }
    }
    if value["network"]["loaded"].is_null() {
        value["network"]["loaded"] = json!({"snapshot_state":"absent","configuration_revision":null,"configured":null,"effective":null,"reason_code":"no_loaded_snapshot"});
    }
    Ok(value)
}
pub async fn loaded(store: &Store) -> Result<Option<crate::config::LoadedFile>> {
    let Some(value) = rpc(store, "loaded", None, None).await? else {
        return Ok(None);
    };
    if value["configuration"].is_null() {
        return Ok(None);
    }
    let config: Config = serde_json::from_value(value["configuration"].clone()).map_err(|_| {
        failure(
            "resident_identity_unavailable",
            "Resident loaded configuration is not recognized",
        )
    })?;
    let revision = value["revision"]
        .as_str()
        .ok_or_else(|| {
            failure(
                "resident_identity_unavailable",
                "Resident loaded revision unavailable",
            )
        })?
        .to_owned();
    let document = toml::to_string(&config)
        .map_err(|_| {
            failure(
                "resident_identity_unavailable",
                "Cannot project loaded configuration",
            )
        })?
        .parse()
        .map_err(|_| {
            failure(
                "resident_identity_unavailable",
                "Cannot project loaded document",
            )
        })?;
    Ok(Some(crate::config::LoadedFile {
        config,
        exists: revision != "absent",
        revision,
        document,
    }))
}
pub async fn activate(store: &Store, _config: &Config, revision: &str) -> Result<Value> {
    match rpc(store, "activate", None, Some(revision.into())).await {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Ok(json!({"activation":"not_running","revision":revision})),
        Err(error) => Ok(
            json!({"_code":"saved_not_active","activation":"failed","saved_revision":revision,"error":error,"next_action":"Fix the blocker and explicitly reload; the prior loaded snapshot remains active"}),
        ),
    }
}
pub async fn preview_activation(store: &Store, config: &Config) -> Result<Value> {
    crate::config::validate(config)?;
    #[cfg(unix)]
    let preview = rpc_with_candidate(
        store,
        "preview_activation",
        None,
        None,
        None,
        Some(config.clone()),
    )
    .await?;
    #[cfg(not(unix))]
    let preview: Option<Value> = None;
    if let Some(preview) = preview {
        return Ok(preview);
    }
    let sessions = Runtime::preview_persisted(store, config)?;
    Ok(
        json!({"state":"not_running","session_source":"persisted","sessions":sessions,"listener":null}),
    )
}

pub async fn control(
    store: &Store,
    config: &Config,
    request: &Request,
    unlock: Option<&[u8]>,
) -> Result<Value> {
    let group = request.command.first().map(String::as_str).unwrap_or("");
    let action = request.command.get(1).map(String::as_str).unwrap_or("");
    if action == "status" {
        return status(store).await;
    }
    if group == "model" && action == "probe" {
        return crate::proxy::probe(store, config, request, unlock).await;
    }
    if group == "proxy" && action == "autostart" {
        let enabled = match request
            .command
            .get(2)
            .or_else(|| request.operands.first())
            .map(String::as_str)
        {
            Some("enable") => true,
            Some("disable") => false,
            _ => {
                return Err(failure(
                    "invalid_argument",
                    "Autostart requires enable or disable",
                ));
            }
        };
        return autostart(store, enabled, request.dry_run).await;
    }
    if group == "history" && action == "clear" && request.dry_run {
        return Ok(rpc(store,"control",Some(request.clone()),None).await?.unwrap_or_else(||json!({"dry_run":true,"snapshot_count":0,"retained_in_progress":0,"coverage":"no_resident_history"})));
    }
    if request.dry_run {
        let before = status(store).await?;
        let mut blockers = Vec::new();
        let discovery = before["resident"]["discovery_state"]
            .as_str()
            .unwrap_or("unavailable");
        if !matches!(discovery, "absent" | "compatible") {
            blockers.push(json!({"code":"resident_identity_unavailable","state":discovery}));
        }
        let active = before["active_requests"].as_u64().unwrap_or(0);
        if group == "proxy"
            && matches!(action, "stop" | "restart")
            && active > 0
            && !request.arguments.contains_key("cancel-active")
        {
            blockers.push(json!({"code":"active_requests","active_count":active}));
        }
        let reused = group == "proxy"
            && action == "start"
            && before["resident"]["inference"]["listen"].is_string();
        if group == "proxy" && matches!(action, "start" | "restart") && !reused {
            let file = crate::config::load(store)?;
            if file.config.proxy.is_none() {
                blockers.push(json!({"code":"proxy_unconfigured"}));
            }
        }
        if group == "web"
            && action == "open"
            && let Some(listen) = request
                .arguments
                .get("listen")
                .and_then(|values| values.first())
        {
            address(listen, false)?;
            if before["resident"]["web"]["listen"]
                .as_str()
                .is_some_and(|actual| actual != listen)
            {
                blockers.push(json!({"code":"endpoint_conflict"}));
            }
        }
        return Ok(
            json!({"_code":if blockers.is_empty(){Value::Null}else{json!("runtime_blocked")},"dry_run":true,"resident":before["resident"],"planned_action":if reused{"reused"}else{"planned"},"operation":request.command,"blockers":blockers,"active_requests":active,"limits":crate::proxy::limits(),"endpoint_availability":"not_reserved_or_probed","credential_readiness":"not_resolved; protected unlock may be required","reservation":false,"secret_reads":false}),
        );
    }
    if let Some(value) =
        rpc_with_unlock(store, "control", Some(request.clone()), None, unlock).await?
    {
        return maybe_browser(value, request);
    }
    if group == "history" || group == "usage" {
        return Ok(
            json!({"snapshot_state":"absent","coverage":{"prior_process_coverage":"unavailable"},"records":[],"reason_code":"no_resident_history"}),
        );
    }
    if matches!((group, action), ("proxy", "stop") | ("web", "stop")) {
        return Ok(absent());
    }
    if !matches!(
        (group, action),
        ("proxy", "start" | "restart") | ("web", "open")
    ) {
        return Err(failure(
            "unsupported_operation",
            "Unknown resident control operation",
        ));
    }
    launch(store, unlock).await?;
    rpc_with_unlock(store, "control", Some(request.clone()), None, unlock)
        .await?
        .ok_or_else(|| {
            failure(
                "resident_identity_unavailable",
                "Detached resident did not establish its control endpoint",
            )
        })
        .and_then(|v| maybe_browser(v, request))
}
fn maybe_browser(mut value: Value, request: &Request) -> Result<Value> {
    if request.command.first().is_some_and(|v| v == "web")
        && request.command.get(1).is_some_and(|v| v == "open")
        && !request.arguments.contains_key("no-browser")
        && let Some(url) = value["resident"]["web"]["url"].as_str()
    {
        let opened = Command::new("xdg-open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok();
        value["browser"] = json!({"opened":opened,"reason_code":if opened{Value::Null}else{json!("browser_launcher_unavailable")}});
    }
    Ok(value)
}

#[cfg(unix)]
async fn launch(store: &Store, unlock: Option<&[u8]>) -> Result<()> {
    use std::{
        io::Write,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::process::CommandExt,
        },
    };
    let _launch = store.lock("resident-launch")?;
    if rpc(store, "status", None, None).await?.is_some() {
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|_| {
        failure(
            "resident_launch_failed",
            "Cannot locate the current executable",
        )
    })?;
    let (mut reader, mut writer) = if let Some(unlock) = unlock {
        if unlock.len() > 4096 {
            return Err(failure(
                "invalid_argument",
                "Unlock material exceeds the protected-input limit",
            ));
        }
        let mut fds = [-1; 2];
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
            return Err(failure(
                "resident_launch_failed",
                "Cannot allocate inherited anonymous unlock pipe",
            ));
        }
        (
            Some(unsafe { std::fs::File::from_raw_fd(fds[0]) }),
            Some(unsafe { std::fs::File::from_raw_fd(fds[1]) }),
        )
    } else {
        (None, None)
    };
    let fd = reader.as_ref().map(AsRawFd::as_raw_fd);
    let mut command = Command::new(executable);
    command
        .arg("__resident")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // Only async-signal-safe libc calls between fork and exec. FD 3 is the sole
    // inherited unlock channel and is never named in argv or the environment.
    unsafe {
        command.pre_exec(move || {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if let Some(fd) = fd {
                if fd != 3 && libc::dup2(fd, 3) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::fcntl(3, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            } else {
                libc::close(3);
            }
            Ok(())
        });
    }
    let mut child = command.spawn().map_err(|_| {
        failure(
            "resident_launch_failed",
            "Cannot start the detached resident executable",
        )
    })?;
    reader.take();
    if let (Some(mut writer), Some(unlock)) = (writer.take(), unlock) {
        writer.write_all(unlock).map_err(|_| {
            failure(
                "resident_launch_failed",
                "Cannot deliver protected unlock material to resident",
            )
        })?;
    }
    // Bounded startup handshake, not a restart/retry policy. The one child either
    // establishes its owned endpoint or returns an actionable incomplete result.
    for _ in 0..100 {
        if rpc(store, "status", None, None).await?.is_some() {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(());
        }
        if child
            .try_wait()
            .map_err(|_| failure("resident_launch_failed", "Cannot inspect launched resident"))?
            .is_some()
        {
            return Err(failure(
                "resident_launch_failed",
                "Detached resident exited before establishing ownership",
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(failure(
        "resident_launch_failed",
        "Resident startup did not establish identity within five seconds; inspect endpoint and executable compatibility",
    ))
}
#[cfg(not(unix))]
async fn launch(_store: &Store, _unlock: Option<&[u8]>) -> Result<()> {
    Err(failure(
        "platform_unavailable",
        "Detached resident launch requires the supported Unix platform",
    ))
}

#[cfg(unix)]
pub async fn run_resident() -> Result<()> {
    run_owner(false).await
}
#[cfg(unix)]
async fn run_owner(autostart: bool) -> Result<()> {
    use std::{io::Read, os::fd::FromRawFd};
    let store = Store::discover()?;
    let Some(ownership) = store.try_lock("resident-owner")? else {
        return Ok(());
    };
    let unlock = {
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        let valid = unsafe { libc::fstat(3, stat.as_mut_ptr()) } == 0
            && unsafe { stat.assume_init() }.st_mode & libc::S_IFMT == libc::S_IFIFO;
        if valid {
            let mut file = unsafe { std::fs::File::from_raw_fd(3) };
            let mut bytes = Zeroizing::new(Vec::new());
            file.by_ref()
                .take(4097)
                .read_to_end(&mut bytes)
                .map_err(|_| {
                    failure(
                        "credential_unavailable",
                        "Cannot read inherited unlock channel",
                    )
                })?;
            if bytes.len() > 4096 {
                return Err(failure(
                    "credential_unavailable",
                    "Inherited unlock material exceeds its limit",
                ));
            }
            Some(bytes)
        } else {
            None
        }
    };
    run_owned(store, ownership, unlock, autostart).await
}
#[cfg(unix)]
async fn run_owned(
    store: Store,
    ownership: crate::storage::LockGuard,
    unlock: Option<Zeroizing<Vec<u8>>>,
    autostart: bool,
) -> Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    let path = socket_path(&store);
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if !meta.file_type().is_socket() || meta.uid() != unsafe { libc::geteuid() } {
            return Err(failure(
                "resident_identity_unavailable",
                "Refusing to replace an unidentified control endpoint",
            ));
        }
        match tokio::net::UnixStream::connect(&path).await {
            Ok(_) => {
                return Err(failure(
                    "resident_identity_unavailable",
                    "Control endpoint is occupied without matching ownership; no takeover",
                ));
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                ) =>
            {
                std::fs::remove_file(&path).map_err(|_| {
                    failure(
                        "resident_identity_unavailable",
                        "Cannot remove this authority's stale socket",
                    )
                })?
            }
            Err(_) => {
                return Err(failure(
                    "resident_identity_unavailable",
                    "Cannot establish control endpoint availability",
                ));
            }
        }
    }
    let listener = tokio::net::UnixListener::bind(&path).map_err(|_| {
        failure(
            "endpoint_occupied",
            "Control endpoint is unavailable; no fallback attempted",
        )
    })?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(|_| {
        failure(
            "resident_identity_unavailable",
            "Cannot protect resident socket",
        )
    })?;
    let process = Uuid::new_v4().to_string();
    let runtime = Runtime::new(store.clone(), process.clone(), unlock)?;
    let shutdown = Arc::new(Shutdown::new());
    let state = Arc::new(Mutex::new(Resident {
        store: store.clone(),
        process,
        runtime,
        inference: None,
        web: None,
        loaded: None,
        key_state: "absent".into(),
        shutdown: shutdown.clone(),
    }));
    if autostart {
        let result = state.lock().await.start_inference(false, false, None).await;
        match result {
            Ok(value) if value["proxy"]["ready"] == true => {}
            Ok(_) => {
                let _ = std::fs::remove_file(&path);
                return Err(failure(
                    "autostart_blocked",
                    "Autostart did not establish a ready inference listener; inspect saved intent, protected unlock, and endpoint state",
                ));
            }
            Err(error) => {
                let _ = std::fs::remove_file(&path);
                return Err(error);
            }
        }
    }
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut acquisition: Option<JoinHandle<()>> = None;
    let connection_slots = Arc::new(tokio::sync::Semaphore::new(16));
    let mut controls = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            _ = shutdown.requested.notified() => break,
            _ = controls.join_next(), if !controls.is_empty() => {},
            _=interval.tick()=>{if acquisition.as_ref().is_none_or(JoinHandle::is_finished){let captured={let resident=state.lock().await;resident.loaded.as_ref().map(|v|(resident.store.clone(),v.0.clone()))};if let Some((store,config))=captured {acquisition=Some(tokio::spawn(async move{let _=crate::inputs::automatic_tick(&store,&config).await;}));}}},
            connection=listener.accept()=>{let(stream,_)=connection.map_err(|_|failure("resident_control_failed","Resident control accept failed"))?;let Ok(slot)=connection_slots.clone().try_acquire_owned()else{drop(stream);continue};let state=state.clone();controls.spawn(async move{let _slot=slot;serve_control(state,stream).await});}
        }
    }
    if let Some(task) = acquisition {
        task.abort();
        let _ = task.await;
    }
    let resident = state.lock().await;
    let _ = resident.runtime.stop(true);
    let inputs_released = crate::inputs::deactivate(&store);
    drop(resident);
    drop(listener);
    let socket_released = std::fs::remove_file(path).map_err(|_| {
        failure(
            "resident_control_failed",
            "Cannot release owned resident socket",
        )
    });
    // A stop acknowledgement is a completion boundary, not a shutdown request.
    // No old runtime publication can follow release of the one owner lock.
    drop(ownership);
    let released = inputs_released.and(socket_released);
    let _ = shutdown.released.send_replace(Some(released.clone()));
    while controls.join_next().await.is_some() {}
    released
}
#[cfg(not(unix))]
pub async fn run_resident() -> Result<()> {
    Err(failure(
        "platform_unavailable",
        "Resident requires supported Unix local IPC",
    ))
}

#[cfg(unix)]
async fn serve_control(state: Shared, mut stream: tokio::net::UnixStream) -> Result<()> {
    let credentials = stream.peer_cred().map_err(|_| {
        failure(
            "resident_identity_unavailable",
            "Cannot identify local management peer",
        )
    })?;
    if credentials.uid() != unsafe { libc::geteuid() } {
        return Err(failure(
            "resident_identity_unavailable",
            "Local management peer is not this authority owner",
        ));
    }
    let bytes = tokio::time::timeout(Duration::from_secs(CONTROL_SECONDS), async {
        let size = stream.read_u32().await? as usize;
        if size > CONTROL_REQUEST_BYTES {
            return Err(std::io::Error::other("request size"));
        }
        let mut bytes = vec![0; size];
        stream.read_exact(&mut bytes).await?;
        Ok::<_, std::io::Error>(bytes)
    })
    .await
    .map_err(|_| failure("control_deadline", "Control request acquisition expired"))?
    .map_err(|_| failure("control_failed", "Control request could not be read"))?;
    let mut bytes = Zeroizing::new(bytes);
    let mut call: Call = serde_json::from_slice(&bytes)
        .map_err(|_| failure("invalid_argument", "Malformed resident control request"))?;
    bytes.zeroize();
    let mut resident = state.lock().await;
    if call.action != "status"
        && call.expected_process.as_deref() != Some(resident.process.as_str())
    {
        return Err(failure(
            "resident_identity_unavailable",
            "Resident changed after discovery; no control action performed",
        ));
    }
    if call
        .unlock
        .as_ref()
        .is_some_and(|unlock| unlock.len() > 4096)
    {
        return Err(failure(
            "invalid_argument",
            "Protected unlock exceeds its limit",
        ));
    }
    let final_stop = call.action == "control" && call.request.as_ref().is_some_and(|request| matches!(request.command.as_slice(), [surface, action] if matches!(surface.as_str(), "proxy" | "web") && action == "stop"));
    let stopping = resident.shutdown.stopping.load(Ordering::Acquire);
    let result = if stopping && call.action != "status" && !final_stop {
        Err(failure(
            "resident_stopping",
            "The identified resident is completing an explicit stop; no new state change was performed",
        ))
    } else if call.authority != resident.store.root {
        Err(failure(
            "resident_identity_unavailable",
            "Management authority does not match this resident",
        ))
    } else if call.protocol != MANAGEMENT_PROTOCOL {
        Err(failure(
            "resident_incompatible",
            "Use a CLI compatible with the running service or its Web controls",
        ))
    } else {
        match call.action.as_str() {
            "status" => Ok(resident.view()),
            "loaded" => Ok(
                json!({"configuration":resident.loaded.as_ref().map(|v|&v.0),"revision":resident.loaded.as_ref().map(|v|&v.1)}),
            ),
            "activate" => resident.activate(call.revision.as_deref()).await,
            "preview_activation" => (|| -> Result<Value> {
                if let Some(candidate) = call.candidate.as_ref() {
                    crate::config::validate(candidate)?;
                    let sessions = resident.runtime.activate_bindings(candidate, true)?;
                    let restart = match (&resident.inference, &candidate.proxy) {
                        (Some(listener), Some(proxy)) => {
                            address(&proxy.listen, false)? != listener.address
                        }
                        _ => false,
                    };
                    let managed = candidate
                        .agents
                        .iter()
                        .filter(|(_, target)| {
                            target
                                .providers
                                .values()
                                .any(|entry| entry.proxy == Some(true))
                        })
                        .map(|(id, _)| id)
                        .collect::<Vec<_>>();
                    Ok(
                        json!({"state":"available","session_source":"resident","loaded_revision":resident.loaded.as_ref().map(|(_, revision)|revision),"sessions":sessions,"listener":{"state":if restart{"restart_required"}else{"unchanged"},"managed_dependencies":managed,"unmanaged_clients":"unknown"}}),
                    )
                } else {
                    Err(failure(
                        "invalid_argument",
                        "Missing typed activation candidate",
                    ))
                }
            })(),
            "control" => {
                if let Some(request) = call.request.take() {
                    resident_control(
                        &state,
                        &mut resident,
                        request,
                        call.unlock.take().map(Zeroizing::new),
                    )
                    .await
                } else {
                    Err(failure(
                        "invalid_argument",
                        "Missing finite management request",
                    ))
                }
            }
            _ => Err(failure(
                "unsupported_operation",
                "Unknown resident operation",
            )),
        }
    };
    let shutdown = resident.shutdown.clone();
    let wait_for_release =
        final_stop && result.is_ok() && shutdown.stopping.load(Ordering::Acquire);
    let mut reply = Reply {
        challenge: call.challenge.clone(),
        authority: resident.store.root.clone(),
        protocol: MANAGEMENT_PROTOCOL.into(),
        process: resident.process.clone(),
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").into(),
        result,
    };
    drop(resident);
    if wait_for_release {
        let release = shutdown.wait_released().await;
        if let Ok(value) = &mut reply.result {
            value["resident_before"] = value["resident"].clone();
            let absent = absent();
            for field in ["resident", "proxy", "shared_key", "publication"] {
                value[field] = absent[field].clone();
            }
            value["network"]["loaded"] = json!({"snapshot_state":"absent","configuration_revision":null,"configured":null,"effective":null,"reason_code":"resident_stopped"});
            value["shutdown"] = json!({"ownership_released":true,"state":if release.is_ok(){"completed"}else{"incomplete"},"error":release.as_ref().err()});
            if release.is_err() {
                value["_code"] = json!("runtime_blocked");
            }
        }
    }
    let bytes = serde_json::to_vec(&reply)
        .map_err(|_| failure("control_failed", "Cannot encode control result"))?;
    if bytes.len() > CONTROL_RESPONSE_BYTES {
        return Err(failure(
            "response_too_large",
            "Control result exceeds bounded output",
        ));
    }
    tokio::time::timeout(Duration::from_secs(CONTROL_SECONDS), async {
        stream.write_u32(bytes.len() as u32).await?;
        stream.write_all(&bytes).await?;
        stream.shutdown().await
    })
    .await
    .map_err(|_| {
        failure(
            "control_deadline",
            "Control result delivery deadline expired",
        )
    })?
    .map_err(|_| failure("control_failed", "Cannot deliver control result"))?;
    Ok(())
}
async fn resident_control(
    state: &Shared,
    resident: &mut Resident,
    request: Request,
    unlock: Option<Zeroizing<Vec<u8>>>,
) -> Result<Value> {
    let group = request.command.first().map(String::as_str).unwrap_or("");
    let action = request.command.get(1).map(String::as_str).unwrap_or("");
    let force = request.arguments.contains_key("cancel-active");
    match (group, action) {
        ("proxy", "start") => resident.start_inference(false, force, unlock).await,
        ("proxy", "restart") => resident.start_inference(true, force, unlock).await,
        ("proxy", "stop") => {
            let interrupted = resident.runtime.stop(force)?;
            if let Some(surface) = resident.inference.take() {
                surface.stop().await;
            }
            let mut value = resident.lifecycle("inference", "stopped", None);
            value["interrupted_requests"] = json!(interrupted);
            resident.maybe_shutdown();
            Ok(value)
        }
        ("web", "open") => {
            let requested = request
                .arguments
                .get("listen")
                .and_then(|v| v.first())
                .map(|s| address(s, false))
                .transpose()?;
            if let Some(surface) = &resident.web {
                if requested.is_some_and(|v| v != surface.address) {
                    return Err(failure(
                        "endpoint_conflict",
                        "Web listener already runs at another address; explicitly stop it first",
                    ));
                }
                return Ok(resident.lifecycle("web", "reused", requested));
            }
            let listener = TcpListener::bind(
                requested.unwrap_or_else(|| "127.0.0.1:0".parse().expect("constant address")),
            )
            .await
            .map_err(|_| {
                failure(
                    "endpoint_occupied",
                    "Requested Web endpoint is occupied; no alternate port attempted",
                )
            })?;
            let actual = listener.local_addr().map_err(|_| {
                failure("listener_unavailable", "Cannot inspect actual Web address")
            })?;
            let (shutdown, mut cancelled) = watch::channel(false);
            let web = WebState {
                resident: state.clone(),
                authority: actual.to_string(),
                origin: format!("http://{actual}"),
            };
            let task = tokio::spawn(async move {
                let _ = axum::serve(
                    listener,
                    Router::new().fallback(web_request).with_state(web),
                )
                .with_graceful_shutdown(async move {
                    let _ = cancelled.changed().await;
                })
                .await;
            });
            resident.web = Some(Surface {
                address: actual,
                shutdown,
                task,
            });
            Ok(resident.lifecycle("web", "started", requested))
        }
        ("web", "stop") => {
            if let Some(surface) = resident.web.take() {
                surface.stop().await;
            }
            let value = resident.lifecycle("web", "stopped", None);
            resident.maybe_shutdown();
            Ok(value)
        }
        ("history", "clear") => Ok(resident.runtime.history.clear(request.dry_run)),
        ("history", _) => resident.runtime.history.view(&request),
        ("usage", _) => Ok(resident.runtime.history.usage()),
        _ => Err(failure(
            "unsupported_operation",
            "Unsupported resident command",
        )),
    }
}

#[derive(Clone)]
struct WebState {
    resident: Shared,
    authority: String,
    origin: String,
}
fn same_origin(headers: &HeaderMap, web: &WebState, mutation: bool) -> bool {
    if headers.get_all("host").iter().count() != 1
        || headers.get("host").and_then(|v| v.to_str().ok()) != Some(web.authority.as_str())
    {
        return false;
    }
    if headers
        .get("sec-fetch-site")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| !matches!(v, "same-origin" | "none"))
    {
        return false;
    }
    match headers.get("origin").and_then(|v| v.to_str().ok()) {
        Some(origin) => origin == web.origin,
        None => !mutation,
    }
}
async fn web_request(State(web): State<WebState>, request: axum::extract::Request) -> Response {
    let (parts, body) = request.into_parts();
    let mutation = parts.method != Method::GET;
    if !same_origin(&parts.headers, &web, mutation) {
        return(StatusCode::FORBIDDEN,axum::Json(json!({"error":{"code":"untrusted_browser_origin","message":"Exact local Host and same-origin JSON required"}}))).into_response();
    }
    let mut response = match (parts.method.as_str(), parts.uri.path()) {
        ("GET", "/") => Html(include_str!("service/index.html")).into_response(),
        ("GET", "/api/catalog") => axum::Json(crate::operations::catalog()).into_response(),
        ("GET", "/api/status") => {
            let resident = web.resident.lock().await;
            axum::Json(resident.view()).into_response()
        }
        ("POST", "/api/execute") => {
            if parts
                .headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .is_none_or(|v| v.split(';').next() != Some("application/json"))
            {
                return (
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "Same-origin application/json required",
                )
                    .into_response();
            }
            let bytes = match tokio::time::timeout(
                Duration::from_secs(30),
                to_bytes(body, WEB_REQUEST_BYTES),
            )
            .await
            {
                Ok(Ok(v)) => v,
                _ => {
                    return (
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "Bounded JSON request required",
                    )
                        .into_response();
                }
            };
            let mut envelope:WebOperation=match serde_json::from_slice(&bytes){Ok(v)=>v,Err(_)=>return(StatusCode::BAD_REQUEST,axum::Json(json!({"error":{"code":"invalid_request","message":"Finite structured operation and separate protected inputs required"}}))).into_response()};
            let mut request = envelope.request.clone();
            if request.arguments.keys().any(|key| {
                key.ends_with("-fd") || key.ends_with("-stdin") || key == "non-interactive"
            }) {
                return (
                    StatusCode::BAD_REQUEST,
                    "CLI descriptors and terminal input are unavailable through Web requests",
                )
                    .into_response();
            }
            request
                .arguments
                .insert("non-interactive".into(), vec!["true".into()]);
            let path = request.command.join(" ");
            if envelope.key.is_some() && path != "provider key set" {
                return (
                    StatusCode::BAD_REQUEST,
                    "Supplied keys belong only to the explicit provider key action",
                )
                    .into_response();
            }
            let mut channels = Vec::new();
            if !request.dry_run {
                if let Some(key) = envelope.key.take() {
                    let key = Zeroizing::new(key);
                    match protected_form_input(&key) {
                        Ok((file, fd)) => {
                            request
                                .arguments
                                .insert("key-fd".into(), vec![fd.to_string()]);
                            channels.push(file);
                        }
                        Err(error) => {
                            return (StatusCode::BAD_REQUEST, axum::Json(json!({"error":error})))
                                .into_response();
                        }
                    }
                }
                if let Some(unlock) = envelope.unlock.take() {
                    let unlock = Zeroizing::new(unlock);
                    match protected_form_input(&unlock) {
                        Ok((file, fd)) => {
                            request
                                .arguments
                                .insert("unlock-fd".into(), vec![fd.to_string()]);
                            channels.push(file);
                        }
                        Err(error) => {
                            return (StatusCode::BAD_REQUEST, axum::Json(json!({"error":error})))
                                .into_response();
                        }
                    }
                }
            }
            // Explicit reveal is a separate protected surface; ordinary operation JSON
            // cannot accidentally expose the shared key to inspection or caching.
            if request.command == ["proxy", "key", "show"] {
                return(StatusCode::BAD_REQUEST,axum::Json(json!({"error":{"code":"protected_reveal_required","message":"Use the explicit Reveal shared key action"}}))).into_response();
            }
            let response = execute_web(request).await;
            drop(channels);
            axum::Json(response).into_response()
        }
        ("POST", "/api/proxy-key/reveal") => {
            if parts
                .headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                != Some("application/json")
            {
                return (
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "application/json required",
                )
                    .into_response();
            }
            let bytes = match to_bytes(body, 64).await {
                Ok(v) => v,
                Err(_) => {
                    return (StatusCode::BAD_REQUEST, "Empty JSON object required").into_response();
                }
            };
            if serde_json::from_slice::<Value>(&bytes).ok() != Some(json!({})) {
                return (
                    StatusCode::BAD_REQUEST,
                    "Explicit empty JSON reveal action required",
                )
                    .into_response();
            }
            let resident = web.resident.lock().await;
            let config = match crate::config::load(&resident.store) {
                Ok(v) => v,
                Err(e) => return axum::Json(json!({"error":e})).into_response(),
            };
            let Some(proxy) = config.config.proxy else {
                return (StatusCode::CONFLICT, "Proxy not configured").into_response();
            };
            let unlock = resident.runtime.unlock.read().expect("unlock lock").clone();
            match crate::secrets::get(
                &resident.store,
                &proxy.key_ref,
                unlock.as_deref().map(Vec::as_slice),
            ) {
                Ok(key) => match std::str::from_utf8(&key) {
                    Ok(key) => axum::Json(json!({"key":key})).into_response(),
                    Err(_) => (StatusCode::CONFLICT, "Key unavailable").into_response(),
                },
                Err(e) => (StatusCode::CONFLICT, axum::Json(json!({"error":e}))).into_response(),
            }
        }
        _ => (StatusCode::NOT_FOUND, "Undeclared management operation").into_response(),
    };
    response.headers_mut().insert(
        "cache-control",
        "no-store".parse().expect("constant header"),
    );
    response.headers_mut().insert(
        "x-content-type-options",
        "nosniff".parse().expect("constant header"),
    );
    response.headers_mut().insert(
        "referrer-policy",
        "no-referrer".parse().expect("constant header"),
    );
    response.headers_mut().insert("content-security-policy","default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'".parse().expect("constant header"));
    response
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WebOperation {
    request: Request,
    key: Option<String>,
    unlock: Option<String>,
}
impl Drop for WebOperation {
    fn drop(&mut self) {
        if let Some(value) = &mut self.key {
            value.zeroize();
        }
        if let Some(value) = &mut self.unlock {
            value.zeroize();
        }
    }
}
fn execute_web(
    request: Request,
) -> std::pin::Pin<Box<dyn Future<Output = crate::Response> + Send>> {
    Box::pin(crate::execute(request))
}
#[cfg(unix)]
fn protected_form_input(value: &str) -> Result<(std::fs::File, i32)> {
    use std::{
        io::Write,
        os::fd::{AsRawFd, FromRawFd},
    };
    if value.is_empty()
        || value.len() > 2048
        || value.bytes().any(|b| matches!(b, 0 | b'\n' | b'\r'))
    {
        return Err(failure(
            "invalid_protected_input",
            "Protected form input must be a nonempty bounded UTF-8 line",
        ));
    }
    let mut fds = [-1; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(failure(
            "protected_input_unavailable",
            "Cannot create protected anonymous input channel",
        ));
    }
    let reader = unsafe { std::fs::File::from_raw_fd(fds[0]) };
    let mut writer = unsafe { std::fs::File::from_raw_fd(fds[1]) };
    writer
        .write_all(value.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .map_err(|_| {
            failure(
                "protected_input_unavailable",
                "Cannot populate protected input channel",
            )
        })?;
    drop(writer);
    let fd = reader.as_raw_fd();
    Ok((reader, fd))
}
#[cfg(not(unix))]
fn protected_form_input(_value: &str) -> Result<(std::fs::File, i32)> {
    Err(failure(
        "platform_unavailable",
        "Protected Web input requires supported anonymous pipes",
    ))
}
#[derive(Default, Serialize, Deserialize)]
struct Startup {
    acknowledged: Option<bool>,
    last_outcome: Option<String>,
    last_attempt: Option<chrono::DateTime<chrono::Utc>>,
}
fn registration_path(store: &Store) -> PathBuf {
    store.home.join(".config/systemd/user/tmax.service")
}
fn registration(store: &Store) -> Result<Vec<u8>> {
    let executable = std::env::current_exe().map_err(|_| {
        failure(
            "autostart_unavailable",
            "Cannot locate executable for startup registration",
        )
    })?;
    let path = executable.to_str().ok_or_else(|| {
        failure(
            "autostart_unavailable",
            "Executable path cannot be represented for systemd",
        )
    })?;
    if path.contains(['\n', '\r', '%', '"', '\\']) {
        return Err(failure(
            "autostart_unavailable",
            "Executable path cannot be safely registered with systemd",
        ));
    }
    Ok(format!("# tmax-owned authority={}\n[Unit]\nDescription=tmax personal model proxy\n[Service]\nType=simple\nExecStart=\"{}\" __autostart\n[Install]\nWantedBy=default.target\n",store.root.display(),path).into_bytes())
}
fn autostart_status(store: &Store) -> Result<Value> {
    let state: Startup = store.read_json("autostart.json")?;
    let expected = registration(store)?;
    let unit = registration_path(store);
    let actual = crate::storage::read_optional(&unit)?;
    let owned = actual.as_ref().is_some_and(|value| {
        value.starts_with(format!("# tmax-owned authority={}\n", store.root.display()).as_bytes())
    });
    let link = unit
        .parent()
        .expect("registration parent")
        .join("default.target.wants/tmax.service");
    let link_state = match std::fs::symlink_metadata(&link) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "absent",
        Ok(metadata) if metadata.file_type().is_symlink() => {
            if std::fs::canonicalize(&link)
                .ok()
                .zip(std::fs::canonicalize(&unit).ok())
                .is_some_and(|(target, unit)| target == unit)
            {
                "owned"
            } else {
                "conflict"
            }
        }
        Ok(_) => "conflict",
        Err(_) => "unavailable",
    };
    let desired = crate::config::load(store)
        .ok()
        .and_then(|config| config.config.proxy.map(|proxy| proxy.autostart));
    let unit_matches = actual.as_ref() == Some(&expected);
    let registered = owned && unit_matches && link_state == "owned";
    Ok(
        json!({"saved_intent":desired,"acknowledged_intent":state.acknowledged,"registration":if actual.is_none()&&link_state=="absent"{"absent"}else if !owned||matches!(link_state,"conflict"|"unavailable"){"conflict"}else if registered{"present"}else{"drifted"},"unit_matches":unit_matches,"login_link":link_state,"login_enabled":if link_state=="unavailable"{Value::Null}else{json!(registered)},"last_outcome":state.last_outcome,"last_attempt":state.last_attempt,"unlock_requirement":"Encrypted credentials require explicit protected unlock; no plaintext unattended fallback","repair":"proxy autostart enable|disable","reconciliation":"not_requested"}),
    )
}
async fn reconcile(store: &Store, enabled: bool, dry_run: bool, force: bool) -> Result<Value> {
    let mut state: Startup = store.read_json("autostart.json")?;
    let unchanged =
        |s: &Startup| s.acknowledged == Some(enabled) || (s.acknowledged.is_none() && !enabled);
    if !force && unchanged(&state) {
        return autostart_status(store);
    }
    if dry_run {
        return Ok(
            json!({"dry_run":true,"enabled":enabled,"registration":"planned","unlock_requirement":"Protected credentials must be unlocked at startup"}),
        );
    }
    let _lock = store.lock("autostart")?;
    state = store.read_json("autostart.json")?;
    if !force && unchanged(&state) {
        return autostart_status(store);
    }
    let target = registration_path(store);
    let expected = registration(store)?;
    let existing = crate::storage::read_optional(&target)?;
    let marker = format!("# tmax-owned authority={}\n", store.root.display());
    state.acknowledged = Some(enabled);
    state.last_attempt = Some(chrono::Utc::now());
    state.last_outcome = Some("pending".into());
    store.write_json("autostart.json", &state)?;
    let mut effects = Vec::<&str>::new();
    let result:Result<()>=async {
        if existing.as_ref().is_some_and(|v|!v.starts_with(marker.as_bytes())){return Err(failure("autostart_conflict","Startup registration is not owned by this authority; unrelated entries were not changed"));}
        if !enabled&&existing.is_none()&&std::fs::symlink_metadata(target.parent().expect("registration parent").join("default.target.wants/tmax.service")).is_ok(){return Err(failure("autostart_conflict","Login registration exists without this authority's owned unit; ownership must be repaired before removal"));}
        if enabled {
            crate::storage::atomic_write(&target,&expected)?;effects.push("owned_unit_written");
            startup_command(store,&["daemon-reload"]).await?;effects.push("manager_reloaded");
            startup_command(store,&["enable","tmax.service"]).await?;effects.push("login_start_enabled");
        }else if existing.is_some(){
            // Disable while the ownership-proven unit still exists; never remove it
            // first and then ask the manager to guess what should be disabled.
            startup_command(store,&["disable","tmax.service"]).await?;effects.push("login_start_disabled");
            std::fs::remove_file(&target).map_err(|_|failure("autostart_failed","Cannot remove this authority's startup registration"))?;effects.push("owned_unit_removed");
            startup_command(store,&["daemon-reload"]).await?;effects.push("manager_reloaded");
        }
        Ok(())
    }.await;
    state.last_outcome = Some(
        if result.is_ok() {
            "reconciled"
        } else {
            "failed"
        }
        .into(),
    );
    let metadata = store.write_json("autostart.json", &state);
    Ok(
        json!({"_code":if result.is_err()||metadata.is_err(){json!("saved_not_active")}else{Value::Null},"reconciliation":if result.is_ok(){"reconciled"}else{"failed"},"acknowledged_intent":enabled,"completed_effects":effects,"error":result.err(),"outcome_recording_error":metadata.err(),"status":autostart_status(store).ok(),"repair":"proxy autostart enable|disable"}),
    )
}
async fn startup_command(store: &Store, args: &[&str]) -> Result<()> {
    let mut command = tokio::process::Command::new("systemctl");
    command
        .arg("--user")
        .args(args)
        .env("XDG_CONFIG_HOME", store.home.join(".config"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = tokio::time::timeout(Duration::from_secs(10), command.kill_on_drop(true).status())
        .await
        .map_err(|_| {
            failure(
                "autostart_failed",
                "User startup reconciliation deadline expired",
            )
        })?
        .map_err(|_| {
            failure(
                "autostart_unavailable",
                "User systemd startup manager is unavailable",
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(failure(
            "autostart_failed",
            "User startup manager did not accept the requested registration operation",
        ))
    }
}
pub async fn reconcile_autostart(store: &Store, config: &Config, dry_run: bool) -> Result<Value> {
    if dry_run {
        return reconcile(
            store,
            config.proxy.as_ref().is_some_and(|proxy| proxy.autostart),
            true,
            false,
        )
        .await;
    }
    let persisted = crate::config::load(store)?;
    if serde_json::to_value(&persisted.config).ok() != serde_json::to_value(config).ok() {
        return Err(failure(
            "inputs_changed",
            "Autostart reconciliation accepts only current validated persisted intent",
        ));
    }
    reconcile(
        store,
        persisted.config.proxy.as_ref().is_some_and(|p| p.autostart),
        false,
        false,
    )
    .await
}
pub async fn autostart(store: &Store, enabled: bool, dry_run: bool) -> Result<Value> {
    let loaded = crate::config::load(store)?;
    if loaded.config.proxy.is_none() {
        return Err(failure(
            "proxy_unconfigured",
            "Configure proxy before changing autostart",
        ));
    }
    if !dry_run && loaded.config.proxy.as_ref().map(|proxy| proxy.autostart) != Some(enabled) {
        return Err(failure(
            "inputs_changed",
            "Explicit registration reconciliation requires the requested intent to be already validated and persisted",
        ));
    }
    reconcile(store, enabled, dry_run, true).await
}

/// Invoked only by owned OS registration. Disabled saved intent is authoritative.
pub async fn run_autostart() -> Result<Value> {
    let store = Store::discover()?;
    let loaded = crate::config::load(&store)?;
    if !loaded.config.proxy.as_ref().is_some_and(|p| p.autostart) {
        return Ok(json!({"autostart":"disabled","started":false}));
    }
    #[cfg(unix)]
    {
        run_owner(true).await?;
        Ok(json!({"autostart":"stopped"}))
    }
    #[cfg(not(unix))]
    {
        Err(failure(
            "platform_unavailable",
            "Autostart resident requires supported Unix local IPC",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listener_addresses_are_explicit_loopback() {
        assert!(address("127.0.0.1:1234", false).is_ok());
        assert!(address("[::1]:1234", false).is_ok());
        assert!(address("0.0.0.0:1234", false).is_err());
        assert!(address("localhost:1234", false).is_err());
        assert!(address("127.0.0.1:0", false).is_err());
    }
    #[tokio::test]
    async fn web_origin_checks_reject_rebinding_and_forms() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().into());
        let runtime = Runtime::new(store.clone(), "test".into(), None).unwrap();
        let resident = Arc::new(Mutex::new(Resident {
            store,
            process: "test".into(),
            runtime,
            inference: None,
            web: None,
            loaded: None,
            key_state: "absent".into(),
            shutdown: Arc::new(Shutdown::new()),
        }));
        let web = WebState {
            resident,
            authority: "127.0.0.1:1234".into(),
            origin: "http://127.0.0.1:1234".into(),
        };
        let mut headers = HeaderMap::new();
        headers.insert("host", "127.0.0.1:1234".parse().unwrap());
        assert!(same_origin(&headers, &web, false));
        assert!(!same_origin(&headers, &web, true));
        headers.insert("origin", "http://evil.test".parse().unwrap());
        assert!(!same_origin(&headers, &web, true));
        headers.insert("origin", "http://127.0.0.1:1234".parse().unwrap());
        assert!(same_origin(&headers, &web, true));
        headers.insert("host", "evil.test:1234".parse().unwrap());
        assert!(!same_origin(&headers, &web, false));
    }
    #[tokio::test]
    async fn absent_status_and_disabled_unacknowledged_intent_create_nothing() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().into());
        assert_eq!(
            status(&store).await.unwrap()["resident"]["discovery_state"],
            "absent"
        );
        assert!(!store.root.exists());
        let result = reconcile(&store, false, false, false).await.unwrap();
        assert_eq!(result["reconciliation"], "not_requested");
        assert!(!store.root.exists());
        let unit = registration_path(&store);
        crate::storage::atomic_write(&unit, b"foreign startup unit").unwrap();
        let result = reconcile(&store, false, false, false).await.unwrap();
        assert_eq!(result["registration"], "conflict");
        assert_eq!(std::fs::read(&unit).unwrap(), b"foreign startup unit");
        assert!(!store.root.exists());
    }
    #[tokio::test]
    async fn repeated_start_reuses_actual_listener_without_reading_pending_config_or_unlock() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().into());
        let reserved = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listen = reserved.local_addr().unwrap().to_string();
        drop(reserved);
        let reference = crate::secrets::put(
            &store,
            "proxy",
            b"test-shared-proxy-key",
            Some(b"test-unlock"),
        )
        .unwrap();
        let mut config = Config::empty();
        config.proxy = Some(crate::config::Proxy {
            listen,
            key_ref: reference,
            autostart: false,
        });
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        let runtime = Runtime::new(store.clone(), "reuse".into(), None).unwrap();
        let mut resident = Resident {
            store: store.clone(),
            process: "reuse".into(),
            runtime,
            inference: None,
            web: None,
            loaded: None,
            key_state: "absent".into(),
            shutdown: Arc::new(Shutdown::new()),
        };
        let first = resident
            .start_inference(false, false, Some(Zeroizing::new(b"test-unlock".to_vec())))
            .await
            .unwrap();
        assert_eq!(first["proxy"]["ready"], true);
        store
            .write_raw("tmax.toml", b"invalid pending configuration")
            .unwrap();
        let repeated = resident
            .start_inference(false, false, Some(Zeroizing::new(b"wrong-unlock".to_vec())))
            .await
            .unwrap();
        assert_eq!(repeated["lifecycle"]["action"], "reused");
        assert_eq!(
            repeated["proxy"]["config_revision"],
            first["proxy"]["config_revision"]
        );
        assert_eq!(
            resident
                .runtime
                .unlock
                .read()
                .unwrap()
                .as_ref()
                .unwrap()
                .as_slice(),
            b"test-unlock"
        );
        resident.runtime.stop(false).unwrap();
        resident.inference.take().unwrap().stop().await;
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn final_force_stop_releases_ownership_before_reply_and_immediate_restart() {
        async fn ready(store: &Store) -> Value {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    // Only initial creation may still be absent. An unidentified or
                    // incompatible endpoint is a failure, never a retry condition.
                    if let Some(value) = rpc(store, "status", None, None).await.unwrap() {
                        return value;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap()
        }
        fn request(action: &str, force: bool) -> Request {
            Request {
                command: vec!["proxy".into(), action.into()],
                operands: Vec::new(),
                arguments: if force {
                    std::collections::BTreeMap::from([(
                        "cancel-active".into(),
                        vec!["true".into()],
                    )])
                } else {
                    Default::default()
                },
                dry_run: false,
                if_revision: None,
            }
        }
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().into());
        let reserved = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listen = reserved.local_addr().unwrap();
        drop(reserved);
        let reference = crate::secrets::put(
            &store,
            "proxy",
            b"stop-barrier-test-key",
            Some(b"stop-barrier-unlock"),
        )
        .unwrap();
        let mut config = Config::empty();
        config.proxy = Some(crate::config::Proxy {
            listen: listen.to_string(),
            key_ref: reference,
            autostart: false,
        });
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        let owner_store = store.clone();
        let ownership = store.try_lock("resident-owner").unwrap().unwrap();
        let first_owner = tokio::spawn(run_owned(owner_store, ownership, None, false));
        let first = ready(&store).await;
        let started = rpc_with_unlock(
            &store,
            "control",
            Some(request("start", false)),
            None,
            Some(b"stop-barrier-unlock"),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(started["proxy"]["ready"], true);
        let mut active = tokio::net::TcpStream::connect(listen).await.unwrap();
        active.write_all(b"POST /v1/chat/completions HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer stop-barrier-test-key\r\nContent-Type: application/json\r\nContent-Length: 1024\r\n\r\n{").await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if rpc(&store, "status", None, None).await.unwrap().unwrap()["active_requests"] == 1
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let stopped = rpc(&store, "control", Some(request("stop", true)), None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stopped["interrupted_requests"], 1);
        assert_eq!(stopped["shutdown"]["ownership_released"], true);
        assert_eq!(stopped["resident"]["discovery_state"], "absent");
        // These assertions are immediate: no sleep, discovery retry, or process
        // exit polling may hide a premature stop acknowledgement.
        assert!(!socket_path(&store).exists());
        assert!(rpc(&store, "status", None, None).await.unwrap().is_none());
        let ownership = store
            .try_lock("resident-owner")
            .unwrap()
            .expect("stop reply must release owner lock");
        let second_owner = tokio::spawn(run_owned(store.clone(), ownership, None, false));
        let second = ready(&store).await;
        assert_ne!(
            first["resident"]["identity"]["process_instance"],
            second["resident"]["identity"]["process_instance"]
        );
        let restarted = rpc_with_unlock(
            &store,
            "control",
            Some(request("start", false)),
            None,
            Some(b"stop-barrier-unlock"),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(restarted["proxy"]["ready"], true);
        let stopped_again = rpc(&store, "control", Some(request("stop", false)), None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stopped_again["shutdown"]["ownership_released"], true);
        drop(active);
        tokio::time::timeout(Duration::from_secs(5), first_owner)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), second_owner)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
