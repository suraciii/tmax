//! Native process ownership and the current activation's recoverable listener requests.
use super::{address, failure};
use crate::{Error, Result, storage::Store};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{net::SocketAddr, path::PathBuf};

#[cfg(target_os = "linux")]
#[path = "linux_supervisor.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "macos_supervisor.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows_supervisor.rs"]
mod platform;

pub(super) const RESTART_DELAY_SECONDS: u64 = 2;
pub(super) const RESTART_LIMIT: u32 = 5;
pub(super) const RESTART_WINDOW_SECONDS: u64 = 60;
pub(super) const READINESS_SECONDS: u64 = 15;
const RECORD: &str = "service-activation.json";
const RECORD_VERSION: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Preparation {
    Preserve,
    Replace,
}

pub(super) struct Manager {
    pub name: &'static str,
    pub activation: String,
    pub survives_logout: Option<bool>,
    pub session_scope: &'static str,
}
pub(super) struct Observation {
    pub registration: &'static str,
    pub state: &'static str,
    pub pid: Option<u32>,
    pub attempts: Option<u32>,
    pub last_exit_code: Option<i32>,
    pub reason_code: Option<String>,
    pub retry_delay_seconds: u64,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Requested {
    pub inference: bool,
    pub web: bool,
}
impl Requested {
    pub fn any(&self) -> bool {
        self.inference || self.web
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Activation {
    version: u32,
    authority: PathBuf,
    pub activation: String,
    pub generation: u64,
    pub requested: Requested,
    pub pending: Requested,
    pub inference_listen: Option<SocketAddr>,
    pub web_listen: Option<SocketAddr>,
    pub process: Option<String>,
    pub state: String,
    pub reason_code: Option<String>,
    reason_surface: Option<String>,
    pub attempts: u32,
    attempt_window: DateTime<Utc>,
    pub last_exit_code: Option<i32>,
}
impl Activation {
    fn new(store: &Store, manager: &Manager) -> Self {
        Self {
            version: RECORD_VERSION,
            authority: store.root.clone(),
            activation: manager.activation.clone(),
            generation: 0,
            requested: Requested::default(),
            pending: Requested::default(),
            inference_listen: None,
            web_listen: None,
            process: None,
            state: "stopped".into(),
            reason_code: None,
            reason_surface: None,
            attempts: 0,
            attempt_window: Utc::now(),
            last_exit_code: None,
        }
    }
    pub(super) fn advance(&mut self) -> Result<()> {
        self.generation = self.generation.checked_add(1).ok_or_else(|| {
            failure(
                "service_recovery_unavailable",
                "Service lifecycle generation is exhausted",
            )
        })?;
        Ok(())
    }
}

pub(super) fn read(store: &Store) -> Result<Option<Activation>> {
    let Some(bytes) = crate::storage::read_optional_bounded(&store.path(RECORD)?, 32 * 1024)
        .map_err(|error| {
            if error.code == "storage_limit_exceeded" {
                failure(
                    "service_recovery_unavailable",
                    "Service activation metadata exceeds its bound",
                )
            } else {
                error
            }
        })?
    else {
        return Ok(None);
    };
    let record: Activation = serde_json::from_slice(&bytes).map_err(|_| {
        failure(
            "service_recovery_unavailable",
            "Service activation metadata is malformed; explicitly restart the owned service",
        )
    })?;
    if record.version != RECORD_VERSION
        || record.authority != store.root
        || record.activation.is_empty()
        || !matches!(
            record.state.as_str(),
            "starting" | "running" | "stopped" | "blocked" | "failed"
        )
        || record
            .inference_listen
            .is_some_and(|v| address(&v.to_string(), false).is_err())
        || record
            .web_listen
            .is_some_and(|v| address(&v.to_string(), false).is_err())
    {
        return Err(failure(
            "service_recovery_unavailable",
            "Service activation metadata does not match this authority or supported schema",
        ));
    }
    Ok(Some(record))
}
pub(super) fn save(store: &Store, record: &Activation) -> Result<()> {
    store.write_json(RECORD, record)
}

pub(super) async fn probe(store: &Store) -> Result<Manager> {
    platform::probe(store).await
}
pub(super) async fn inspect(store: &Store) -> Result<Observation> {
    platform::inspect(store).await
}
pub(super) async fn prepare(store: &Store, preparation: Preparation) -> Result<()> {
    platform::prepare(store, preparation).await
}
pub(super) async fn start(store: &Store) -> Result<()> {
    platform::start(store).await
}
pub(super) async fn stop(store: &Store) -> Result<()> {
    platform::stop(store).await
}
pub(super) async fn login(store: &Store, enabled: bool) -> Result<()> {
    platform::login(store, enabled).await
}
pub(super) fn login_status(store: &Store) -> Result<Value> {
    platform::login_status(store)
}

/// Serialize CLI operations without blocking a Tokio executor thread on a file lock.
pub(super) async fn lifecycle_lock(store: &Store) -> Result<crate::storage::LockGuard> {
    let store = store.clone();
    tokio::task::spawn_blocking(move || store.lock("service-lifecycle"))
        .await
        .map_err(|_| {
            failure(
                "service_control_failed",
                "Service lifecycle coordination failed",
            )
        })?
}

/// Record a current operation before asking the native manager to run it.
pub(super) fn request(
    store: &Store,
    manager: &Manager,
    inference: Option<bool>,
    web: Option<bool>,
    inference_listen: Option<SocketAddr>,
    web_listen: Option<SocketAddr>,
) -> Result<Activation> {
    request_inner(
        store,
        manager,
        inference,
        web,
        inference_listen,
        web_listen,
        false,
    )
}

/// A self-restart must withdraw all requested listeners if replacement fails.
pub(super) fn request_restart(
    store: &Store,
    manager: &Manager,
    web: bool,
    inference_listen: Option<SocketAddr>,
    web_listen: Option<SocketAddr>,
) -> Result<Activation> {
    request_inner(
        store,
        manager,
        Some(true),
        Some(web),
        inference_listen,
        web_listen,
        true,
    )
}

fn request_inner(
    store: &Store,
    manager: &Manager,
    inference: Option<bool>,
    web: Option<bool>,
    inference_listen: Option<SocketAddr>,
    web_listen: Option<SocketAddr>,
    restart: bool,
) -> Result<Activation> {
    let _guard = store.lock("service-state")?;
    let mut record = read(store)?
        .filter(|v| v.activation == manager.activation)
        .unwrap_or_else(|| Activation::new(store, manager));
    record.advance()?;
    if let Some(requested) = inference {
        record.pending.inference = requested && !record.requested.inference;
        record.requested.inference = requested;
        record.inference_listen = if requested {
            inference_listen.or(record.inference_listen)
        } else {
            None
        };
    }
    if let Some(requested) = web {
        record.pending.web = requested && !record.requested.web;
        record.requested.web = requested;
        record.web_listen = if requested {
            web_listen.or(record.web_listen)
        } else {
            None
        };
    }
    if restart {
        record.pending = record.requested;
    }
    record.state = if record.requested.any() {
        "starting"
    } else {
        "stopped"
    }
    .into();
    if inference.is_some() && record.reason_surface.as_deref() == Some("inference")
        || web.is_some() && record.reason_surface.as_deref() == Some("web")
    {
        record.reason_code = None;
        record.reason_surface = None;
    }
    record.attempts = 0;
    record.attempt_window = Utc::now();
    save(store, &record)?;
    Ok(record)
}

/// Withdraw only this operation's request; a newer generation always wins.
pub(super) fn withdraw(
    store: &Store,
    activation: &str,
    generation: u64,
    inference: bool,
    web: bool,
    error: &Error,
) -> Result<bool> {
    let _guard = store.lock("service-state")?;
    let Some(mut record) = read(store)? else {
        return Ok(false);
    };
    if record.activation != activation || record.generation != generation {
        return Ok(false);
    }
    record.advance()?;
    if inference {
        record.requested.inference = false;
        record.pending.inference = false;
        record.inference_listen = None;
    }
    if web {
        record.requested.web = false;
        record.pending.web = false;
        record.web_listen = None;
    }
    record.state = if record.requested.any() {
        "running"
    } else {
        "failed"
    }
    .into();
    record.reason_code = Some(error.code.clone());
    save(store, &record)?;
    Ok(true)
}

pub(super) fn initialize(store: &Store, manager: &Manager) -> Result<()> {
    let _guard = store.lock("service-state")?;
    if read(store)?.is_none() {
        if store.is_locked("daemon-owner")? == Some(true) {
            return Err(failure(
                "service_recovery_unavailable",
                "The running owner's recovery metadata is missing; explicitly restart before repairing login activation",
            ));
        }
        save(store, &Activation::new(store, manager))?;
    }
    Ok(())
}

/// Explicit lifecycle repair reconstructs only currently observed listeners.
pub(super) fn repair(store: &Store, manager: &Manager, current: Option<&Value>) -> Result<()> {
    let _guard = store.lock("service-state")?;
    let mut record = Activation::new(store, manager);
    if let Some(value) = current {
        record.inference_listen = value
            .pointer("/daemon/inference/listen")
            .and_then(Value::as_str)
            .and_then(|v| v.parse().ok());
        record.web_listen = value
            .pointer("/daemon/web/listen")
            .and_then(Value::as_str)
            .and_then(|v| v.parse().ok());
        record.requested.inference = record.inference_listen.is_some();
        record.requested.web = record.web_listen.is_some();
        record.process = value
            .pointer("/daemon/identity/process_instance")
            .and_then(Value::as_str)
            .map(str::to_owned);
        record.state = if record.requested.any() {
            "running"
        } else {
            "stopped"
        }
        .into();
    }
    save(store, &record)
}

/// Foreground entrypoint: a fresh manager activation follows login intent once.
pub(super) async fn enter(store: &Store) -> Result<Option<Activation>> {
    let manager = probe(store).await?;
    let observed = platform::inspect(store).await?;
    if observed.registration != "owned" || observed.pid != Some(std::process::id()) {
        return Err(failure(
            "service_manager_unavailable",
            "The native manager did not establish ownership of this foreground service process",
        ));
    }
    let _guard = store.lock("service-state")?;
    let mut record = match read(store)? {
        Some(record) if record.activation == manager.activation => record,
        Some(_) => {
            let mut record = Activation::new(store, &manager);
            let file = crate::config::load(store)?;
            if let Some(gateway) = file.config.gateway.filter(|v| v.autostart) {
                record.requested.inference = true;
                record.pending.inference = true;
                record.inference_listen = Some(address(&gateway.listen, false)?);
                record.advance()?;
            }
            record
        }
        None => {
            return Err(failure(
                "service_recovery_unavailable",
                "Service activation metadata is missing; explicitly restart rather than infer a new login activation",
            ));
        }
    };
    if !record.requested.any() {
        record.state = "stopped".into();
        save(store, &record)?;
        return Ok(None);
    }
    let now = Utc::now();
    if now
        .signed_duration_since(record.attempt_window)
        .num_seconds()
        >= RESTART_WINDOW_SECONDS.max(
            observed
                .retry_delay_seconds
                .saturating_mul(u64::from(RESTART_LIMIT)),
        ) as i64
    {
        record.attempt_window = now;
        record.attempts = 0;
    }
    if record.attempts >= RESTART_LIMIT {
        record.state = "failed".into();
        record.reason_code = Some("service_recovery_exhausted".into());
        record.last_exit_code = Some(78);
        save(store, &record)?;
        return Err(failure(
            "service_recovery_exhausted",
            "Native service recovery reached its finite attempt limit; explicitly start or restart after inspecting the cause",
        ));
    }
    record.attempts += 1;
    record.process = None;
    record.state = "starting".into();
    save(store, &record)?;
    Ok(Some(record))
}

/// Called with the daemon mutex held, before/after binding; the state lock fences recovery.
pub(super) fn state_lock(store: &Store) -> Result<crate::storage::LockGuard> {
    store.lock("service-state")
}
pub(super) fn current(store: &Store, activation: &Activation) -> Result<Activation> {
    read(store)?
        .filter(|v| v.activation == activation.activation)
        .ok_or_else(|| {
            failure(
                "service_recovery_unavailable",
                "Service activation changed before listener startup",
            )
        })
}
pub(super) fn acknowledge(
    store: &Store,
    mut record: Activation,
    process: &str,
    inference: Option<SocketAddr>,
    web: Option<SocketAddr>,
    error: Option<&Error>,
) -> Result<()> {
    record.process = Some(process.into());
    if record.requested.inference && inference.is_some() {
        record.inference_listen = inference;
    }
    if record.requested.web && web.is_some() {
        record.web_listen = web;
    }
    let missing_surface = if record.requested.inference && inference.is_none() {
        Some("inference")
    } else if record.requested.web && web.is_none() {
        Some("web")
    } else {
        None
    };
    if record.pending.inference && inference.is_none() {
        record.requested.inference = false;
        record.inference_listen = None;
    }
    if record.pending.web && web.is_none() {
        record.requested.web = false;
        record.web_listen = None;
    }
    record.pending = Requested::default();
    let unavailable =
        record.requested.inference && inference.is_none() || record.requested.web && web.is_none();
    record.state = if unavailable || missing_surface.is_some() && !record.requested.any() {
        "blocked"
    } else if record.requested.any() {
        "running"
    } else {
        "stopped"
    }
    .into();
    if let Some(surface) = missing_surface {
        if record.reason_surface.as_deref() != Some(surface) {
            record.reason_code = None;
        }
        record.reason_code = record.reason_code.or_else(|| error.map(|v| v.code.clone()));
        record.reason_surface = Some(surface.into());
    } else {
        record.reason_code = None;
        record.reason_surface = None;
    }
    save(store, &record)
}

pub(super) fn record_exit(
    store: &Store,
    activation: &str,
    generation: u64,
    error: &Error,
) -> Result<()> {
    let _guard = store.lock("service-state")?;
    let Some(mut record) = read(store)? else {
        return Ok(());
    };
    if record.activation != activation || record.generation != generation {
        return Ok(());
    }
    record.last_exit_code = Some(i32::from(service_exit_code(error)));
    if error.code != "service_restart_requested" {
        record.reason_code = Some(error.code.clone());
        record.state = if service_exit_code(error) == 75 {
            "starting"
        } else {
            "blocked"
        }
        .into();
    }
    save(store, &record)
}

pub(super) fn local_view(store: &Store, process: &str) -> Value {
    match read(store) {
        Ok(Some(record)) if record.process.as_deref() == Some(process) => json!({
            "state":if record.state=="blocked"{"blocked"}else{"supervised"},
            "requested":record.requested,"reason_code":record.reason_code,
            "recovery":{"attempts":record.attempts,"limit":RESTART_LIMIT,"retry_delay_seconds":RESTART_DELAY_SECONDS,"last_exit_code":record.last_exit_code}
        }),
        Ok(_) => {
            json!({"state":"unsupervised","requested":{"inference":null,"web":null},"reason_code":"service_not_supervised"})
        }
        Err(error) => json!({"state":"unavailable","reason_code":error.code}),
    }
}

pub(super) async fn view(store: &Store, daemon: &Value) -> Value {
    let manager = probe(store).await;
    let observation = platform::inspect(store).await;
    let record = read(store);
    let live_pid = daemon.pointer("/identity/pid").and_then(Value::as_u64);
    let identity_known = matches!(
        daemon["discovery_state"].as_str(),
        Some("compatible" | "absent")
    );
    let mut value = json!({"manager":manager.as_ref().ok().map(|v|v.name),"registration":"unavailable","state":"unavailable","reason_code":null,"process_owned":null,
        "requested":{"inference":null,"web":null},
        "recovery":{"attempts":null,"limit":RESTART_LIMIT,"retry_delay_seconds":RESTART_DELAY_SECONDS,"last_exit_code":null},
        "session":{"scope":manager.as_ref().ok().map(|v|v.session_scope),"survives_logout":manager.as_ref().ok().and_then(|v|v.survives_logout)}});
    if let Ok(observed) = &observation {
        value["registration"] = json!(observed.registration);
        if identity_known && observed.registration != "unavailable" {
            value["process_owned"] = json!(
                live_pid.is_some()
                    && observed.pid.map(u64::from) == live_pid
                    && observed.registration == "owned"
            );
        }
        value["state"] = json!(if !identity_known {
            "unavailable"
        } else if let Some(pid) = live_pid {
            if observed.pid.map(u64::from) == Some(pid) && observed.registration == "owned" {
                "supervised"
            } else if observed.registration == "absent" || observed.pid.is_some() {
                "unsupervised"
            } else {
                "unavailable"
            }
        } else {
            observed.state
        });
        value["recovery"]["attempts"] = json!(observed.attempts);
        value["recovery"]["retry_delay_seconds"] = json!(observed.retry_delay_seconds);
        value["recovery"]["last_exit_code"] = json!(observed.last_exit_code);
        value["reason_code"] = json!(observed.reason_code);
    }
    match record {
        Ok(Some(record))
            if manager
                .as_ref()
                .is_ok_and(|m| m.activation == record.activation) =>
        {
            value["requested"] = json!(record.requested);
            value["recovery"]["attempts"] = json!(record.attempts);
            if record.last_exit_code.is_some() {
                value["recovery"]["last_exit_code"] = json!(record.last_exit_code);
            }
            if matches!(record.state.as_str(), "blocked" | "failed")
                && identity_known
                && (live_pid.is_none()
                    && observation
                        .as_ref()
                        .is_ok_and(|v| v.registration == "owned")
                    || value["process_owned"] == true
                        && daemon
                            .pointer("/identity/process_instance")
                            .and_then(Value::as_str)
                            == record.process.as_deref())
            {
                value["state"] = json!(record.state);
                value["reason_code"] = json!(record.reason_code);
            }
        }
        Err(error) => {
            value["state"] = json!("unavailable");
            value["reason_code"] = json!(error.code);
        }
        _ => {}
    }
    if let Err(error) = manager {
        value["reason_code"] = json!(error.code);
    }
    if let Err(error) = observation {
        value["reason_code"] = json!(error.code);
    }
    value
}

pub fn service_exit_code(error: &Error) -> u8 {
    let permanent = !matches!(
        error.code.as_str(),
        "service_restart_requested"
            | "service_control_failed"
            | "daemon_control_failed"
            | "control_failed"
            | "storage_unavailable"
    );
    if permanent {
        if cfg!(any(target_os = "macos", windows)) {
            0
        } else {
            78
        }
    } else {
        75
    }
}

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod tests;
