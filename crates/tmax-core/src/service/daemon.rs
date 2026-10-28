//! Authoritative loaded state, surface lifecycle, and the sole-owner loop.
#[cfg(windows)]
use super::control::CONTROL_SECONDS;
#[cfg(unix)]
use super::control_transport::{socket_path, validate_control_path};
#[cfg(windows)]
use super::windows_startup;
use super::{
    address,
    control::{MANAGEMENT_PROTOCOL, retained_session_arguments, serve_control},
    control_transport, failure, web,
};
#[cfg(windows)]
use crate::Error;
use crate::{
    Request, Result,
    config::Config,
    gateway::{Runtime, Snapshot},
    storage::Store,
};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{Mutex, Notify, watch},
    task::{JoinHandle, JoinSet},
};
use uuid::Uuid;

struct InferenceListener(TcpListener);
impl axum::serve::Listener for InferenceListener {
    type Io = tokio::net::TcpStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, address) = axum::serve::Listener::accept(&mut self.0).await;
            match stream.set_nodelay(true) {
                Ok(()) => return (stream, address),
                Err(error) => {
                    eprintln!(
                        "tmax: rejecting inference connection: cannot set TCP_NODELAY: {error}"
                    );
                }
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.0.local_addr()
    }
}

pub(super) struct Surface {
    pub(super) address: SocketAddr,
    pub(super) shutdown: watch::Sender<bool>,
    pub(super) task: JoinHandle<()>,
    #[cfg(windows)]
    pub(super) listener_closed: Option<watch::Receiver<bool>>,
}
impl Surface {
    async fn stop(self) {
        let _ = self.shutdown.send(true);
        self.task.abort();
        let _ = self.task.await;
    }
    #[cfg(windows)]
    async fn stop_web(self, owner: &Shutdown) -> Result<()> {
        let _ = self.shutdown.send(true);
        let mut closed = self.listener_closed.expect("Web listener completion");
        let release = tokio::time::timeout(Duration::from_secs(CONTROL_SECONDS), async {
            while !*closed.borrow_and_update() {
                closed.changed().await.map_err(|_| {
                    failure(
                        "daemon_control_failed",
                        "Cannot establish Web listener release",
                    )
                })?;
            }
            Ok::<_, Error>(())
        })
        .await;
        if !matches!(release, Ok(Ok(()))) {
            self.task.abort();
            let _ = self.task.await;
            return Err(failure(
                "daemon_control_failed",
                "Web listener did not release within its finite shutdown deadline",
            ));
        }
        // Do not await the response-serving task from its own Web stop request.
        // The owner drains it after private-control instances have closed.
        let mut tasks = owner.web_tasks.lock().expect("Web shutdown tasks");
        tasks.retain(|task| !task.is_finished());
        tasks.push(self.task);
        Ok(())
    }
}
pub(super) struct Shutdown {
    requested: Notify,
    stopping: AtomicBool,
    released: watch::Sender<Option<Result<()>>>,
    #[cfg(windows)]
    endpoints_drained: watch::Sender<Option<Result<()>>>,
    #[cfg(windows)]
    web_tasks: std::sync::Mutex<Vec<JoinHandle<()>>>,
}
impl Shutdown {
    pub(super) fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }
    pub(super) fn new() -> Self {
        let (released, _) = watch::channel(None);
        Self {
            requested: Notify::new(),
            stopping: AtomicBool::new(false),
            released,
            #[cfg(windows)]
            endpoints_drained: watch::channel(None).0,
            #[cfg(windows)]
            web_tasks: std::sync::Mutex::new(Vec::new()),
        }
    }
    pub(super) async fn wait_released(&self) -> Result<()> {
        let mut released = self.released.subscribe();
        loop {
            if let Some(result) = released.borrow_and_update().clone() {
                return result;
            }
            released.changed().await.map_err(|_| {
                failure(
                    "daemon_control_failed",
                    "Daemon ownership release could not be established",
                )
            })?;
        }
    }
    #[cfg(windows)]
    pub(super) async fn wait_endpoints_drained(&self) -> Result<()> {
        let mut complete = self.endpoints_drained.subscribe();
        loop {
            if let Some(result) = complete.borrow_and_update().clone() {
                return result;
            }
            complete.changed().await.map_err(|_| {
                failure(
                    "daemon_control_failed",
                    "Cannot establish Windows control endpoint completion",
                )
            })?;
        }
    }
}
pub(super) struct Daemon {
    store: Store,
    process: String,
    runtime: Arc<Runtime>,
    inference: Option<Surface>,
    web: Option<Surface>,
    loaded: Option<(Config, String)>,
    loaded_metadata: Option<Arc<crate::catalog::Snapshot>>,
    key_state: String,
    shutdown: Arc<Shutdown>,
}
pub(super) type Shared = Arc<Mutex<Daemon>>;

pub(super) fn absent() -> Value {
    json!({"daemon":{"discovery_state":"absent","reason_code":null,"identity":null,"inference":null,"web":null},"gateway":{"ready":false,"listen":null,"key_ref":null,"config_revision":null},"shared_key":{"loaded_reference":null,"loaded_state":"absent"},"publication":{"snapshot_state":"absent","model_count":null},"limits":crate::gateway::limits(),"unmanaged_clients":"unknown"})
}
impl Daemon {
    pub(super) fn new(
        store: Store,
        process: String,
        runtime: Arc<Runtime>,
        shutdown: Arc<Shutdown>,
    ) -> Self {
        Self {
            store,
            process,
            runtime,
            inference: None,
            web: None,
            loaded: None,
            loaded_metadata: None,
            key_state: "absent".into(),
            shutdown,
        }
    }
    pub(super) fn store(&self) -> &Store {
        &self.store
    }
    pub(super) fn process(&self) -> &str {
        &self.process
    }
    pub(super) fn shutdown(&self) -> &Arc<Shutdown> {
        &self.shutdown
    }
    pub(super) fn loaded_view(&self) -> Value {
        json!({"configuration":self.loaded.as_ref().map(|v|&v.0),"revision":self.loaded.as_ref().map(|v|&v.1),"metadata":self.loaded_metadata.as_deref()})
    }
    pub(super) fn preview_activation(&self, candidate: Option<&Config>) -> Result<Value> {
        if let Some(candidate) = candidate {
            crate::config::validate(candidate)?;
            let sessions = self.runtime.activate_bindings(candidate, true)?;
            let restart = match (&self.inference, &candidate.gateway) {
                (Some(listener), Some(gateway)) => {
                    address(&gateway.listen, false)? != listener.address
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
                        .any(|entry| entry.gateway == Some(true))
                })
                .map(|(id, _)| id)
                .collect::<Vec<_>>();
            Ok(
                json!({"state":"available","session_source":"daemon","loaded_revision":self.loaded.as_ref().map(|(_, revision)|revision),"sessions":sessions,"listener":{"state":if restart{"restart_required"}else{"unchanged"},"managed_dependencies":managed,"unmanaged_clients":"unknown"}}),
            )
        } else {
            Err(failure(
                "invalid_argument",
                "Missing typed activation candidate",
            ))
        }
    }
    pub(super) fn view(&self) -> Value {
        let identity = json!({"config_path":self.store.root.join("tmax.toml"),"process_instance":self.process,"pid":std::process::id(),"executable_version":env!("CARGO_PKG_VERSION"),"management_protocol_version":MANAGEMENT_PROTOCOL,"loaded_revision":self.loaded.as_ref().map(|v|&v.1)});
        let surface = |s: &Option<Surface>| {
            s.as_ref().map_or_else(||json!({"state":"stopped","listen":null,"url":null}),|s|json!({"state":if s.task.is_finished(){"unavailable"}else{"running"},"listen":s.address.to_string(),"url":format!("http://{}",s.address)}))
        };
        let key_ref = self
            .loaded
            .as_ref()
            .and_then(|v| v.0.gateway.as_ref())
            .map(|p| &p.key_ref);
        let count = self.loaded.as_ref().map(|v| {
            crate::gateway::publication(&v.0)["data"]
                .as_array()
                .map_or(0, Vec::len)
        });
        let loaded_network=self.loaded.as_ref().map(|(config,revision)|crate::operations::network_view(config,revision,revision!="absent","loaded")["policy"].clone()).unwrap_or_else(||json!({"snapshot_state":"absent","configuration_revision":null,"configured":null,"effective":null,"reason_code":"no_loaded_snapshot"}));
        let mut inference = surface(&self.inference);
        if inference["state"] == "running" && self.key_state != "ready" {
            inference["state"] = json!("unavailable");
        }
        let dependencies=self.loaded.as_ref().map(|(config,_)|config.agents.iter().filter(|(_,target)|target.providers.values().any(|entry|entry.gateway==Some(true))).map(|(id,_)|json!({"kind":"agent_target","id":id,"scope":"configured_gateway_entry","native_effective_state":"unknown"})).collect::<Vec<_>>()).unwrap_or_default();
        let stopping = self.shutdown.stopping.load(Ordering::Acquire);
        json!({"daemon":{"discovery_state":if stopping{"unavailable"}else{"compatible"},"reason_code":if stopping{json!("daemon_stopping")}else{Value::Null},"identity":identity,"inference":inference,"web":surface(&self.web),"caller_version":env!("CARGO_PKG_VERSION"),"version_relation":"same"},"gateway":{"ready":!stopping&&self.inference.as_ref().is_some_and(|s|!s.task.is_finished())&&self.key_state=="ready","listen":self.inference.as_ref().map(|s|s.address.to_string()),"key_ref":key_ref,"config_revision":self.loaded.as_ref().map(|v|&v.1)},"shared_key":{"loaded_reference":key_ref,"loaded_state":self.key_state},"publication":{"snapshot_state":if self.loaded.is_some(){"available"}else{"absent"},"model_count":count},"network":{"loaded":loaded_network},"dependencies":dependencies,"active_requests":self.runtime.active(),"sessions":self.runtime.binding_summary(),"limits":crate::gateway::limits(),"unmanaged_clients":"unknown"})
    }
    pub(super) async fn activate(&mut self, revision: Option<&str>) -> Result<Value> {
        let loaded = crate::config::load(&self.store)?;
        if revision.is_some_and(|r| r != loaded.revision) {
            return Err(failure(
                "inputs_changed",
                "Configuration changed before activation",
            ));
        }
        if let (Some(listener), Some(gateway)) = (&self.inference, &loaded.config.gateway)
            && address(&gateway.listen, false)? != listener.address
        {
            return Err(failure(
                "listener_restart_required",
                "Saved listener address requires explicit gateway restart",
            ));
        }
        let previous = self.runtime.snapshot.read().expect("snapshot lock").clone();
        let metadata = crate::catalog::read(&self.store, "accepted")?.map(Arc::new);
        crate::reasoning::validate(&loaded.config, metadata.as_deref())?;
        let snapshot = if let Some(gateway) = &loaded.config.gateway {
            let retained = previous
                .as_ref()
                .filter(|s| {
                    s.config
                        .gateway
                        .as_ref()
                        .is_some_and(|p| p.key_ref == gateway.key_ref)
                })
                .map(|s| s.key.clone());
            let key = if retained.is_some() {
                retained
            } else {
                crate::gateway::credential(self.store.clone(), gateway.key_ref.clone())
                    .await
                    .ok()
            };
            self.key_state = if key.as_ref().is_some_and(|k| !k.is_empty()) {
                "ready"
            } else {
                "unavailable"
            }
            .into();
            key.filter(|key| !key.is_empty())
                .map(|key| {
                    crate::provider_integration::client(&loaded.config).map(|client| {
                        Arc::new(Snapshot {
                            client,
                            config: loaded.config.clone(),
                            revision: loaded.revision.clone(),
                            key,
                            metadata: metadata.clone(),
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
        self.loaded_metadata = metadata;
        let view = self.view();
        let blocked = self.inference.is_some()
            && self
                .loaded
                .as_ref()
                .is_some_and(|(config, _)| config.gateway.is_some())
            && self.key_state != "ready";
        Ok(
            json!({"_code":if blocked{json!("runtime_blocked")}else{Value::Null},"activation":"active","revision":self.loaded.as_ref().map(|v|&v.1),"serving_state":if self.inference.is_none(){"stopped"}else if self.key_state=="ready"{"ready"}else{"blocked"},"impact":changes,"daemon":view["daemon"],"gateway":view["gateway"],"shared_key":view["shared_key"],"next_action":if blocked{json!("Inspect stored shared-key availability; explicitly replace missing or unsupported key material")}else{Value::Null}}),
        )
    }
    async fn start_inference(&mut self) -> Result<Value> {
        if let Some(listener) = self.inference.as_ref() {
            if listener.task.is_finished() {
                return Err(failure(
                    "listener_unavailable",
                    "Inference listener is unavailable; inspect the owned service and explicitly restart",
                ));
            }
            return Ok(self.lifecycle("inference", "reused", None));
        }
        let current = crate::config::load(&self.store)?;
        let gateway = current.config.gateway.as_ref().ok_or_else(|| {
            failure(
                "gateway_unconfigured",
                "Configure the gateway explicitly before starting it",
            )
        })?;
        let requested = address(&gateway.listen, false)?;
        // Validate cold-start credentials before touching an existing listener.
        let key = crate::gateway::credential(self.store.clone(), gateway.key_ref.clone()).await?;
        if key.is_empty() {
            return Err(failure(
                "credential_unavailable",
                "Shared key is unavailable; explicitly replace it",
            ));
        }
        let client = crate::provider_integration::client(&current.config)?;
        let metadata = crate::catalog::read(&self.store, "accepted")?.map(Arc::new);
        crate::reasoning::validate(&current.config, metadata.as_deref())?;
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
            self.runtime.stop()
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
                        json!({"_code":"saved_not_active","lifecycle":{"surface":"inference","action":"stopped"},"interrupted_requests":interrupted,"error":{"code":"endpoint_occupied","message":"Inference stopped but requested endpoint could not be rebound; inspect conflict and explicitly start"},"daemon":self.view()["daemon"]}),
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
        self.loaded = Some((current.config, current.revision));
        self.key_state = "ready".into();
        self.runtime.open();
        let (shutdown, mut cancelled) = watch::channel(false);
        let runtime = self.runtime.clone();
        let task = tokio::spawn(async move {
            let _ = axum::serve(InferenceListener(listener), crate::gateway::router(runtime))
                .with_graceful_shutdown(async move {
                    let _ = cancelled.changed().await;
                })
                .await;
        });
        self.inference = Some(Surface {
            address: requested,
            shutdown,
            task,
            #[cfg(windows)]
            listener_closed: None,
        });
        let mut value = self.lifecycle("inference", "started", Some(requested));
        value["interrupted_requests"] = json!(interrupted);
        Ok(value)
    }
    fn lifecycle(&self, surface: &str, action: &str, requested: Option<SocketAddr>) -> Value {
        let mut value = self.view();
        value["lifecycle"] = json!({"surface":surface,"action":action,"requested_listen":requested.map(|v|v.to_string()),"actual_listen":if surface=="inference"{self.inference.as_ref()}else{self.web.as_ref()}.map(|v|v.address.to_string())});
        if action == "reused" {
            value["outcome"] = json!("daemon_reused");
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

pub async fn run_daemon() -> Result<()> {
    run_owner(false).await
}
#[cfg(unix)]
pub(super) async fn run_owner(autostart: bool) -> Result<()> {
    let store = Store::discover()?;
    validate_control_path(&socket_path(&store))?;
    let Some(ownership) = store.try_lock("daemon-owner")? else {
        return Ok(());
    };
    run_owned(store, ownership, autostart).await
}

#[cfg(windows)]
pub(super) async fn run_owner(autostart: bool) -> Result<()> {
    let store = Store::discover()?;
    let Some(ownership) = store.try_lock("daemon-owner")? else {
        return if autostart {
            Err(failure(
                "daemon_identity_unavailable",
                "Daemon ownership changed before autostart; inspect the actual owner before retrying",
            ))
        } else {
            Ok(())
        };
    };
    run_owned(store, ownership, autostart).await
}
async fn run_owned(
    store: Store,
    ownership: crate::storage::LockGuard,
    autostart: bool,
) -> Result<()> {
    let mut listener = control_transport::Listener::bind(&store).await?;
    let process = Uuid::new_v4().to_string();
    let runtime = Runtime::new(store.clone(), process.clone())?;
    let shutdown = Arc::new(Shutdown::new());
    let state = Arc::new(Mutex::new(Daemon::new(
        store.clone(),
        process,
        runtime,
        shutdown.clone(),
    )));
    if autostart {
        let result = state.lock().await.start_inference().await;
        match result {
            Ok(value) if value["gateway"]["ready"] == true => {
                #[cfg(windows)]
                if let Err(error) = windows_startup::record_launch(&store, None) {
                    let mut daemon = state.lock().await;
                    let _ = daemon.runtime.stop();
                    if let Some(surface) = daemon.inference.take() {
                        surface.stop().await;
                    }
                    drop(daemon);
                    let _ = crate::inputs::deactivate(&store);
                    let _ = listener.release();
                    return Err(error);
                }
            }
            Ok(_) => {
                let _ = listener.release();
                return Err(failure(
                    "autostart_blocked",
                    "Autostart did not establish a ready inference listener; inspect saved intent, stored key availability, and endpoint state",
                ));
            }
            Err(error) => {
                let _ = listener.release();
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
            _=interval.tick()=>{if acquisition.as_ref().is_none_or(JoinHandle::is_finished){let captured={let daemon=state.lock().await;daemon.loaded.as_ref().map(|v|(daemon.store.clone(),v.0.clone()))};if let Some((store,config))=captured {acquisition=Some(tokio::spawn(async move{let _=crate::inputs::automatic_tick(&store,&config).await;}));}}},
            connection=listener.accept()=>{let stream=connection?;let Ok(slot)=connection_slots.clone().try_acquire_owned()else{drop(stream);continue};let state=state.clone();controls.spawn(async move{let _slot=slot;serve_control(state,stream).await});}
        }
    }
    if let Some(task) = acquisition {
        task.abort();
        let _ = task.await;
    }
    let daemon = state.lock().await;
    let _ = daemon.runtime.stop();
    let inputs_released = crate::inputs::deactivate(&store);
    drop(daemon);
    let endpoint_released = listener.release();
    // A stop acknowledgement is a completion boundary, not a shutdown request.
    // No old runtime publication can follow release of the one owner lock.
    drop(ownership);
    let released = inputs_released.and(endpoint_released);
    let _ = shutdown.released.send_replace(Some(released.clone()));
    while controls.join_next().await.is_some() {}
    #[cfg(windows)]
    {
        let _ = shutdown
            .endpoints_drained
            .send_replace(Some(released.clone()));
        let tasks = std::mem::take(&mut *shutdown.web_tasks.lock().expect("Web shutdown tasks"));
        for mut task in tasks {
            // Connections can now finish their self-stop response. Do not wait
            // on them before signalling the private endpoint drain boundary.
            if tokio::time::timeout(Duration::from_secs(CONTROL_SECONDS), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
    }
    released
}

pub(super) async fn daemon_control(
    state: &Shared,
    daemon: &mut Daemon,
    request: Request,
) -> Result<Value> {
    let group = request.command.first().map(String::as_str).unwrap_or("");
    let action = request.command.get(1).map(String::as_str).unwrap_or("");
    match (group, action) {
        ("gateway", "start") => daemon.start_inference().await,
        ("gateway", "stop") => {
            let interrupted = daemon.runtime.stop();
            if let Some(surface) = daemon.inference.take() {
                surface.stop().await;
            }
            let mut value = daemon.lifecycle("inference", "stopped", None);
            value["interrupted_requests"] = json!(interrupted);
            daemon.maybe_shutdown();
            Ok(value)
        }
        ("web", "open") => {
            let requested = request
                .arguments
                .get("listen")
                .and_then(|v| v.first())
                .map(|s| address(s, false))
                .transpose()?;
            if let Some(surface) = &daemon.web {
                if requested.is_some_and(|v| v != surface.address) {
                    return Err(failure(
                        "endpoint_conflict",
                        "Web listener already runs at another address; explicitly stop it first",
                    ));
                }
                return Ok(daemon.lifecycle("web", "reused", requested));
            }
            daemon.web = Some(web::start(state.clone(), requested).await?);
            Ok(daemon.lifecycle("web", "started", requested))
        }
        ("web", "stop") => {
            if let Some(surface) = daemon.web.take() {
                #[cfg(windows)]
                surface.stop_web(&daemon.shutdown).await?;
                #[cfg(unix)]
                surface.stop().await;
            }
            let value = daemon.lifecycle("web", "stopped", None);
            daemon.maybe_shutdown();
            Ok(value)
        }
        ("model", "explain") => {
            let (id, protocol, session) = retained_session_arguments(&request)?;
            let loaded = request
                .arguments
                .get("state")
                .and_then(|v| v.first())
                .is_some_and(|v| v == "loaded");
            let saved;
            let config = if loaded {
                &daemon
                    .loaded
                    .as_ref()
                    .ok_or_else(|| {
                        failure(
                            "runtime_unavailable",
                            "There is no loaded configuration to inspect",
                        )
                    })?
                    .0
            } else {
                saved = crate::config::load(&daemon.store)?;
                &saved.config
            };
            daemon
                .runtime
                .inspect_session(config, id, protocol, session, loaded)
        }
        ("history", "clear") => Ok(daemon.runtime.history.clear(request.dry_run)),
        ("history", _) => daemon.runtime.history.view(&request),
        ("usage", _) => Ok(daemon.runtime.history.usage()),
        _ => Err(failure(
            "unsupported_operation",
            "Unsupported daemon command",
        )),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::super::control::rpc;
    use super::*;
    #[cfg(unix)]
    use tokio::io::AsyncWriteExt;
    #[cfg(windows)]
    use {super::super::windows_web, axum::Router};
    #[tokio::test]
    async fn loaded_advice_retains_accepted_metadata_without_an_inference_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("state"));
        store
            .write_raw(
                "tmax.toml",
                toml::to_string(&Config::empty()).unwrap().as_bytes(),
            )
            .unwrap();
        let accepted = crate::catalog::Snapshot {
            revision: "loaded-metadata".into(),
            ..Default::default()
        };
        store
            .write_json("catalog/accepted.json", &accepted)
            .unwrap();
        let runtime = Runtime::new(store.clone(), "advice".into()).unwrap();
        let mut daemon = Daemon {
            store: store.clone(),
            process: "advice".into(),
            runtime,
            inference: None,
            web: None,
            loaded: None,
            loaded_metadata: None,
            key_state: "absent".into(),
            shutdown: Arc::new(Shutdown::new()),
        };
        daemon.activate(None).await.unwrap();
        assert!(daemon.runtime.snapshot.read().unwrap().is_none());
        assert_eq!(
            daemon.loaded_metadata.as_ref().unwrap().revision,
            "loaded-metadata"
        );
        store
            .write_json(
                "catalog/accepted.json",
                &crate::catalog::Snapshot {
                    revision: "pending-metadata".into(),
                    ..accepted
                },
            )
            .unwrap();
        assert_eq!(
            daemon.loaded_metadata.as_ref().unwrap().revision,
            "loaded-metadata"
        );
        daemon.activate(None).await.unwrap();
        assert_eq!(
            daemon.loaded_metadata.as_ref().unwrap().revision,
            "pending-metadata"
        );
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn windows_web_stop_releases_listener_before_draining_its_response() {
        let owner = Arc::new(Shutdown::new());
        let entered = Arc::new(Notify::new());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (listener, closed) = windows_web::Listener::new(listener);
        let (shutdown, mut cancelled) = watch::channel(false);
        let handler_owner = owner.clone();
        let handler_entered = entered.clone();
        let router = Router::new().route(
            "/",
            axum::routing::get(move || {
                let owner = handler_owner.clone();
                let entered = handler_entered.clone();
                async move {
                    entered.notify_one();
                    owner.wait_endpoints_drained().await.unwrap();
                    "completed"
                }
            }),
        );
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = cancelled.changed().await;
                })
                .await
                .unwrap();
        });
        let surface = Surface {
            address,
            shutdown,
            task,
            listener_closed: Some(closed),
        };
        let request = tokio::spawn(async move {
            reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap()
                .get(format!("http://{address}"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap()
        });
        entered.notified().await;
        surface.stop_web(&owner).await.unwrap();
        let replacement = TcpListener::bind(address).await.unwrap();
        assert!(!owner.web_tasks.lock().unwrap()[0].is_finished());
        let _ = owner.endpoints_drained.send_replace(Some(Ok(())));
        assert_eq!(request.await.unwrap(), "completed");
        let tasks = std::mem::take(&mut *owner.web_tasks.lock().unwrap());
        for task in tasks {
            task.await.unwrap();
        }
        drop(replacement);
    }
    #[tokio::test]
    async fn repeated_start_reuses_actual_listener_without_reading_pending_config() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().into());
        let reserved = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listen = reserved.local_addr().unwrap().to_string();
        drop(reserved);
        let reference = crate::secrets::put(&store, "gateway", b"test-shared-gateway-key").unwrap();
        let mut config = Config::empty();
        config.gateway = Some(crate::config::Gateway {
            listen,
            key_ref: reference,
            autostart: false,
        });
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        let runtime = Runtime::new(store.clone(), "reuse".into()).unwrap();
        let mut daemon = Daemon {
            store: store.clone(),
            process: "reuse".into(),
            runtime,
            inference: None,
            web: None,
            loaded: None,
            loaded_metadata: None,
            key_state: "absent".into(),
            shutdown: Arc::new(Shutdown::new()),
        };
        let first = daemon.start_inference().await.unwrap();
        assert_eq!(first["gateway"]["ready"], true);
        store
            .write_raw("tmax.toml", b"invalid pending configuration")
            .unwrap();
        let repeated = daemon.start_inference().await.unwrap();
        assert_eq!(repeated["lifecycle"]["action"], "reused");
        assert_eq!(
            repeated["gateway"]["config_revision"],
            first["gateway"]["config_revision"]
        );
        daemon.runtime.stop();
        daemon.inference.take().unwrap().stop().await;
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
        fn request(action: &str) -> Request {
            Request {
                command: vec!["gateway".into(), action.into()],
                operands: Vec::new(),
                arguments: Default::default(),
                dry_run: false,
                if_revision: None,
            }
        }
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().into());
        let reserved = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let listen = reserved.local_addr().unwrap();
        drop(reserved);
        let reference = crate::secrets::put(&store, "gateway", b"stop-barrier-test-key").unwrap();
        let mut config = Config::empty();
        config.gateway = Some(crate::config::Gateway {
            listen: listen.to_string(),
            key_ref: reference,
            autostart: false,
        });
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        let owner_store = store.clone();
        let ownership = store.try_lock("daemon-owner").unwrap().unwrap();
        let first_owner = tokio::spawn(run_owned(owner_store, ownership, false));
        let first = ready(&store).await;
        let started = rpc(&store, "control", Some(request("start")), None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(started["gateway"]["ready"], true);
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
        let stopped = rpc(&store, "control", Some(request("stop")), None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stopped["interrupted_requests"], 1);
        assert_eq!(stopped["shutdown"]["ownership_released"], true);
        assert_eq!(stopped["daemon"]["discovery_state"], "absent");
        // These assertions are immediate: no sleep, discovery retry, or process
        // exit polling may hide a premature stop acknowledgement.
        #[cfg(unix)]
        assert!(!socket_path(&store).exists());
        assert!(rpc(&store, "status", None, None).await.unwrap().is_none());
        let ownership = store
            .try_lock("daemon-owner")
            .unwrap()
            .expect("stop reply must release owner lock");
        let second_owner = tokio::spawn(run_owned(store.clone(), ownership, false));
        let second = ready(&store).await;
        assert_ne!(
            first["daemon"]["identity"]["process_instance"],
            second["daemon"]["identity"]["process_instance"]
        );
        let restarted = rpc(&store, "control", Some(request("start")), None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(restarted["gateway"]["ready"], true);
        let stopped_again = rpc(&store, "control", Some(request("stop")), None)
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
