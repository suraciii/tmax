//! Qualified daemon protocol, bounded exchanges, and management clients.
#[cfg(unix)]
use super::control_transport::{socket_path, validate_control_path};
#[cfg(windows)]
use super::windows_web;
use super::{
    address, control_transport,
    daemon::{Shared, absent, daemon_control},
    failure,
    startup::{autostart, autostart_status},
};
use crate::{Request, Result, config::Config, gateway::Runtime, storage::Store};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[cfg(not(windows))]
use std::process::{Command, Stdio};
use std::{path::PathBuf, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

pub(super) const MANAGEMENT_PROTOCOL: &str = "tmax-control/6";
const CONTROL_REQUEST_BYTES: usize = 1024 * 1024;
const CONTROL_RESPONSE_BYTES: usize = 12 * 1024 * 1024;
pub(super) const CONTROL_SECONDS: u64 = 30;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Call {
    challenge: String,
    authority: PathBuf,
    protocol: String,
    action: String,
    request: Option<Request>,
    revision: Option<String>,
    candidate: Option<Config>,
    expected_process: Option<String>,
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

async fn rpc_with_candidate(
    store: &Store,
    action: &str,
    request: Option<Request>,
    revision: Option<String>,
    candidate: Option<Config>,
) -> Result<Option<Value>> {
    let expected = if action == "status" {
        None
    } else {
        let Some(identity) = rpc_exchange(store, "status", None, None, None, None).await? else {
            return Ok(None);
        };
        if identity["daemon"]["reason_code"] == "daemon_stopping" {
            return Err(failure(
                "daemon_stopping",
                "The identified daemon is completing an explicit stop; wait for that stop's result before starting again",
            ));
        }
        if identity["daemon"]["discovery_state"] != "compatible" {
            return Err(failure(
                "daemon_incompatible",
                "Use the running daemon's compatible CLI or Web controls before starting this executable",
            ));
        }
        Some(
            identity["daemon"]["identity"]["process_instance"]
                .as_str()
                .ok_or_else(|| {
                    failure(
                        "daemon_identity_unavailable",
                        "Daemon process instance is unavailable",
                    )
                })?
                .to_owned(),
        )
    };
    rpc_exchange(store, action, request, revision, expected, candidate).await
}
async fn rpc_exchange(
    store: &Store,
    action: &str,
    request: Option<Request>,
    revision: Option<String>,
    expected_process: Option<String>,
    candidate: Option<Config>,
) -> Result<Option<Value>> {
    let Some(mut stream) = control_transport::connect(store).await? else {
        return Ok(None);
    };
    #[cfg(unix)]
    let peer_pid = control_transport::peer_pid(&stream)?;
    #[cfg(windows)]
    let peer = control_transport::peer(&stream)?;
    #[cfg(windows)]
    let peer_pid = Some(peer.pid);
    let challenge = Uuid::new_v4().to_string();
    let call = Call {
        challenge: challenge.clone(),
        authority: store.root.clone(),
        protocol: MANAGEMENT_PROTOCOL.into(),
        action: action.into(),
        request,
        revision,
        candidate,
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
            "daemon_identity_unavailable",
            "Daemon control did not complete within its finite deadline",
        )
    })?
    .map_err(|_| {
        failure(
            "daemon_identity_unavailable",
            "Daemon control exchange failed",
        )
    })?;
    // Windows handlers retain ownership until the fully-read client closes,
    // then explicitly disconnect stale client references before server drop.
    drop(stream);
    let reply: Reply = serde_json::from_slice(&result).map_err(|_| {
        failure(
            "daemon_identity_unavailable",
            "Endpoint did not return a recognized daemon identity",
        )
    })?;
    if reply.challenge != challenge
        || reply.authority != store.root
        || reply.process.is_empty()
        || reply.process.len() > 128
        || reply.pid == 0
        || peer_pid.is_some_and(|pid| pid != reply.pid)
        || expected_process.is_some_and(|expected| expected != reply.process)
    {
        return Err(failure(
            "daemon_identity_unavailable",
            "Daemon identity challenge, authority, or process did not match",
        ));
    }
    let incompatible = reply.protocol != MANAGEMENT_PROTOCOL;
    if incompatible && action != "status" {
        return Err(failure(
            "daemon_incompatible",
            "Daemon management protocol is incompatible; use its compatible CLI or Web to stop it before starting this executable",
        ));
    }
    let mut value = reply.result?;
    if let Some(identity) = value.pointer("/daemon/identity").filter(|v| !v.is_null())
        && (identity["pid"].as_u64() != Some(u64::from(reply.pid))
            || identity["process_instance"].as_str() != Some(reply.process.as_str()))
    {
        return Err(failure(
            "daemon_identity_unavailable",
            "Nested daemon identity did not match the qualified process peer",
        ));
    }
    #[cfg(windows)]
    if value["shutdown"]["ownership_released"] == true && value["shutdown"]["state"] == "completed"
    {
        peer.wait_for_exit().await?;
    }
    if let Some(daemon) = value.get_mut("daemon") {
        daemon["caller_version"] = json!(env!("CARGO_PKG_VERSION"));
        daemon["version_relation"] = json!(if reply.version == env!("CARGO_PKG_VERSION") {
            "same"
        } else {
            "different"
        });
        if incompatible {
            daemon["discovery_state"] = json!("incompatible");
            daemon["reason_code"] = json!("management_protocol_mismatch");
        }
    }
    Ok(Some(value))
}
pub(super) async fn rpc(
    store: &Store,
    action: &str,
    request: Option<Request>,
    revision: Option<String>,
) -> Result<Option<Value>> {
    rpc_with_candidate(store, action, request, revision, None).await
}

pub async fn status(store: &Store) -> Result<Value> {
    let mut value = match rpc(store, "status", None, None).await {
        Ok(Some(v)) => v,
        Ok(None) => absent(),
        Err(e) => {
            let mut value = absent();
            value["daemon"]["discovery_state"] = json!(if e.code == "daemon_incompatible" {
                "incompatible"
            } else {
                "unidentified"
            });
            value["daemon"]["reason_code"] = json!(e.code);
            value
        }
    };
    value["autostart"]=autostart_status(store).unwrap_or_else(|error|json!({"snapshot_state":"unavailable","reason_code":error.code,"reconciliation":"not_requested","repair":"gateway autostart enable|disable"}));
    value["supervision"] = super::supervisor::view(store, &value["daemon"]).await;
    match crate::config::load(store) {
        Ok(file) => {
            value["shared_key"]["file_reference"] =
                json!(file.config.gateway.as_ref().map(|gateway| &gateway.key_ref));
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
    Ok(loaded_with_metadata(store).await?.map(|(file, _)| file))
}
pub(crate) async fn loaded_with_metadata(
    store: &Store,
) -> Result<Option<(crate::config::LoadedFile, Option<crate::catalog::Snapshot>)>> {
    let Some(value) = rpc(store, "loaded", None, None).await? else {
        return Ok(None);
    };
    if value["configuration"].is_null() {
        return Ok(None);
    }
    let config: Config = serde_json::from_value(value["configuration"].clone()).map_err(|_| {
        failure(
            "daemon_identity_unavailable",
            "Daemon loaded configuration is not recognized",
        )
    })?;
    let revision = value["revision"]
        .as_str()
        .ok_or_else(|| {
            failure(
                "daemon_identity_unavailable",
                "Daemon loaded revision unavailable",
            )
        })?
        .to_owned();
    let document = toml::to_string(&config)
        .map_err(|_| {
            failure(
                "daemon_identity_unavailable",
                "Cannot project loaded configuration",
            )
        })?
        .parse()
        .map_err(|_| {
            failure(
                "daemon_identity_unavailable",
                "Cannot project loaded document",
            )
        })?;
    let metadata = serde_json::from_value(value["metadata"].clone()).map_err(|_| {
        failure(
            "daemon_identity_unavailable",
            "Daemon loaded metadata is not recognized",
        )
    })?;
    Ok(Some((
        crate::config::LoadedFile {
            config,
            exists: revision != "absent",
            revision,
            document,
        },
        metadata,
    )))
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
    let preview = rpc_with_candidate(
        store,
        "preview_activation",
        None,
        None,
        Some(config.clone()),
    )
    .await?;
    if let Some(preview) = preview {
        return Ok(preview);
    }
    let sessions = Runtime::preview_persisted(store, config)?;
    Ok(
        json!({"state":"not_running","session_source":"persisted","sessions":sessions,"listener":null}),
    )
}

pub(super) fn retained_session_arguments(request: &Request) -> Result<(&str, &str, &str)> {
    let id = request.operands.first().map(String::as_str);
    let protocol = request
        .arguments
        .get("protocol")
        .and_then(|v| v.first())
        .map(String::as_str);
    let session = request
        .arguments
        .get("session")
        .and_then(|v| v.first())
        .map(String::as_str);
    id.zip(protocol).zip(session).map(|((id, protocol), session)| (id, protocol, session))
        .ok_or_else(|| failure("invalid_arguments", "Retained inspection requires a public model ID, protocol and existing gateway session handle"))
}

pub(crate) async fn browser_login(store: &Store, request: &Request) -> Result<Value> {
    if request.dry_run && request.command.last().is_some_and(|v| v == "login-start") {
        return crate::agent_integration::browser_login::command(store, request);
    }
    rpc(store, "browser_login", Some(request.clone()), None)
        .await?
        .ok_or_else(|| {
            failure(
                "daemon_required",
                "Open tmax Web before starting or inspecting browser sign-in",
            )
        })
}

pub async fn control(store: &Store, config: &Config, request: &Request) -> Result<Value> {
    let group = request.command.first().map(String::as_str).unwrap_or("");
    let action = request.command.get(1).map(String::as_str).unwrap_or("");
    if action == "status" {
        return status(store).await;
    }
    if group == "model" && action == "probe" {
        return crate::gateway::probe(store, config, request).await;
    }
    if group == "gateway" && action == "autostart" {
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
        return Ok(rpc(store,"control",Some(request.clone()),None).await?.unwrap_or_else(||json!({"dry_run":true,"snapshot_count":0,"retained_in_progress":0,"coverage":"no_daemon_history"})));
    }
    if request.dry_run {
        let before = status(store).await?;
        let mut blockers = Vec::new();
        let discovery = before["daemon"]["discovery_state"]
            .as_str()
            .unwrap_or("unavailable");
        #[cfg(unix)]
        if discovery == "absent"
            && matches!(
                (group, action),
                ("gateway", "start" | "restart") | ("web", "open")
            )
        {
            validate_control_path(&socket_path(store))?;
        }
        if !matches!(discovery, "absent" | "compatible") {
            blockers.push(json!({"code":"daemon_identity_unavailable","state":discovery}));
        }
        let active = before["active_requests"].as_u64().unwrap_or(0);
        let reused = group == "gateway"
            && action == "start"
            && before["daemon"]["inference"]["listen"].is_string();
        if group == "gateway" && matches!(action, "start" | "restart") && !reused {
            let file = crate::config::load(store)?;
            if file.config.gateway.is_none() {
                blockers.push(json!({"code":"gateway_unconfigured"}));
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
            if before["daemon"]["web"]["listen"]
                .as_str()
                .is_some_and(|actual| actual != listen)
            {
                blockers.push(json!({"code":"endpoint_conflict"}));
            }
        }
        if matches!(
            (group, action),
            ("gateway", "start" | "restart") | ("web", "open")
        ) && (discovery == "absent" || action == "restart")
            && let Err(error) = super::supervisor::probe(store).await
        {
            blockers.push(json!(error));
        }
        return Ok(
            json!({"_code":if blockers.is_empty(){Value::Null}else{json!("runtime_blocked")},"dry_run":true,"daemon":before["daemon"],"supervision":before["supervision"],"service_definition":if reused{"unchanged"}else{"prepare_owned_if_needed"},"recovery_generation":"unchanged","planned_action":if reused{"reused"}else{"planned"},"operation":request.command,"blockers":blockers,"active_requests":active,"limits":crate::gateway::limits(),"endpoint_availability":"not_reserved_or_probed","credential_readiness":"not_resolved","reservation":false,"secret_reads":false}),
        );
    }
    if matches!(
        (group, action),
        ("gateway", "start" | "stop" | "restart") | ("web", "open" | "stop")
    ) {
        return super::supervised_control::control(store, config, request)
            .await
            .and_then(|value| maybe_browser(value, request));
    }
    if let Some(value) = rpc(store, "control", Some(request.clone()), None).await? {
        return maybe_browser(value, request);
    }
    if group == "model" && action == "explain" {
        let (id, protocol, session) = retained_session_arguments(request)?;
        let at = request
            .arguments
            .get("at")
            .and_then(|values| values.first())
            .map(String::as_str);
        return Runtime::inspect_persisted_session(store, config, id, protocol, session, at);
    }
    if group == "history" || group == "usage" {
        return Ok(
            json!({"snapshot_state":"absent","coverage":{"prior_process_coverage":"unavailable"},"records":[],"reason_code":"no_daemon_history"}),
        );
    }
    Err(failure(
        "unsupported_operation",
        "Unknown daemon control operation",
    ))
}

fn maybe_browser(mut value: Value, request: &Request) -> Result<Value> {
    if request.command.first().is_some_and(|v| v == "web")
        && request.command.get(1).is_some_and(|v| v == "open")
        && !request.arguments.contains_key("no-browser")
        && let Some(url) = value["daemon"]["web"]["url"].as_str()
    {
        #[cfg(windows)]
        let opened = windows_web::open_browser(url);
        #[cfg(not(windows))]
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

pub(super) async fn serve_control(
    state: Shared,
    mut stream: control_transport::Server,
) -> Result<()> {
    control_transport::authorize_client(&stream)?;
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
        .map_err(|_| failure("invalid_argument", "Malformed daemon control request"))?;
    bytes.zeroize();
    let mut daemon = state.lock().await;
    if call.action != "status" && call.expected_process.as_deref() != Some(daemon.process()) {
        return Err(failure(
            "daemon_identity_unavailable",
            "Daemon changed after discovery; no control action performed",
        ));
    }
    let final_stop = call.action == "control" && call.request.as_ref().is_some_and(|request| matches!(request.command.as_slice(), [surface, action] if matches!(surface.as_str(), "gateway" | "web") && action == "stop"));
    let stopping = daemon.shutdown().is_stopping();
    let result = if stopping && call.action != "status" && !final_stop {
        Err(failure(
            "daemon_stopping",
            "The identified daemon is completing an explicit stop; no new state change was performed",
        ))
    } else if call.authority != daemon.store().root {
        Err(failure(
            "daemon_identity_unavailable",
            "Management authority does not match this daemon",
        ))
    } else if call.protocol != MANAGEMENT_PROTOCOL {
        Err(failure(
            "daemon_incompatible",
            "Use a CLI compatible with the running service or its Web controls",
        ))
    } else {
        match call.action.as_str() {
            "browser_login" => {
                let request = call
                    .request
                    .as_ref()
                    .ok_or_else(|| failure("invalid_arguments", "Missing login request"))?;
                crate::operations::commands::validate(request)?;
                crate::agent_integration::browser_login::command(daemon.store(), request)
            }
            "status" => Ok(daemon.view()),
            "loaded" => Ok(daemon.loaded_view()),
            "restart_process" => {
                let mut value = daemon.view();
                value["interrupted_requests"] = json!(daemon.runtime_stop());
                daemon.shutdown().request_restart();
                value["_code"] = json!("service_restart_requested");
                Ok(value)
            }
            "activate" => daemon.activate(call.revision.as_deref()).await,
            "preview_activation" => daemon.preview_activation(call.candidate.as_ref()),
            "control" => {
                if let Some(request) = call.request.take() {
                    daemon_control(&state, &mut daemon, request).await
                } else {
                    Err(failure(
                        "invalid_argument",
                        "Missing finite management request",
                    ))
                }
            }
            _ => Err(failure("unsupported_operation", "Unknown daemon operation")),
        }
    };
    let shutdown = daemon.shutdown().clone();
    let wait_for_release = final_stop && result.is_ok() && shutdown.is_stopping();
    let mut reply = Reply {
        challenge: call.challenge.clone(),
        authority: daemon.store().root.clone(),
        protocol: MANAGEMENT_PROTOCOL.into(),
        process: daemon.process().to_owned(),
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").into(),
        result,
    };
    drop(daemon);
    if wait_for_release {
        let release = shutdown.wait_released().await;
        if let Ok(value) = &mut reply.result {
            value["daemon_before"] = value["daemon"].clone();
            let absent = absent();
            for field in ["daemon", "gateway", "shared_key", "publication"] {
                value[field] = absent[field].clone();
            }
            value["network"]["loaded"] = json!({"snapshot_state":"absent","configuration_revision":null,"configured":null,"effective":null,"reason_code":"daemon_stopped"});
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
        #[cfg(windows)]
        control_transport::confirm_client_closed(&mut stream).await?;
        #[cfg(unix)]
        stream.shutdown().await?;
        Ok::<_, std::io::Error>(())
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removed_unlock_fields_are_rejected_at_private_protocol_boundary() {
        let mut call = json!({"challenge":"opaque","authority":"/isolated","protocol":"tmax-control/1","action":"status","request":null,"revision":null,"candidate":null,"expected_process":null});
        assert!(serde_json::from_value::<Call>(call.clone()).is_ok());
        call["unlock"] = Value::Null;
        assert!(serde_json::from_value::<Call>(call).is_err());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn unrepresentable_control_path_blocks_start_before_service_effects() {
        let temporary = tempfile::tempdir().unwrap();
        // A valid filesystem component whose UTF-8 bytes exceed Unix socket
        // pathname capacity, without assuming one platform's sun_path size.
        let store = Store::at(temporary.path().join("é".repeat(100)));
        for (group, action) in [
            ("gateway", "start"),
            ("gateway", "restart"),
            ("web", "open"),
        ] {
            for dry_run in [false, true] {
                let request = Request {
                    command: vec![group.into(), action.into()],
                    operands: Vec::new(),
                    arguments: Default::default(),
                    dry_run,
                    if_revision: None,
                };
                let error = control(&store, &Config::empty(), &request)
                    .await
                    .unwrap_err();
                assert_eq!(error.code, "daemon_control_path_invalid");
                assert!(
                    !store.root.exists(),
                    "blocked startup must not create locks or service state"
                );
            }
        }
        assert!(!store.home.exists());
    }
}
