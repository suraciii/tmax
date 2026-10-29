//! Serialized lifecycle operations through one native process owner.
use super::{
    control::{rpc, status},
    daemon::absent,
    failure, supervisor,
};
use crate::{Error, Request, Result, config::Config, storage::Store};
use serde_json::{Value, json};
use std::{net::SocketAddr, time::Duration};

fn request(group: &str, action: &str) -> Request {
    Request {
        command: vec![group.into(), action.into()],
        operands: Vec::new(),
        arguments: Default::default(),
        dry_run: false,
        if_revision: None,
    }
}
fn listener(value: &Value, name: &str) -> Option<SocketAddr> {
    value["daemon"][name]["listen"]
        .as_str()
        .and_then(|v| v.parse().ok())
}
fn target(operation: &Request) -> (&str, &str) {
    (operation.command[0].as_str(), operation.command[1].as_str())
}
async fn qualified_before(store: &Store) -> Result<Option<Value>> {
    // RPC performs authority, protocol, challenge and peer-process validation.
    rpc(store, "status", None, None).await
}
fn startup_address(store: &Store) -> Result<SocketAddr> {
    let file = crate::config::load(store)?;
    let gateway = file.config.gateway.as_ref().ok_or_else(|| {
        failure(
            "gateway_unconfigured",
            "Configure the gateway before starting inference",
        )
    })?;
    super::address(&gateway.listen, false)
}
fn web_address(operation: &Request) -> Result<Option<SocketAddr>> {
    operation
        .arguments
        .get("listen")
        .and_then(|v| v.first())
        .map(|v| super::address(v, false))
        .transpose()
}
/// Decide readiness against the durable request after the manager observation.
/// The state lock makes this decision serialize with direct daemon stops.
pub(super) fn readiness(
    store: &Store,
    attempt: &supervisor::Activation,
    value: Option<&Value>,
    manager: Option<(&supervisor::Manager, &supervisor::Observation)>,
    inference: bool,
    web: bool,
) -> Result<bool> {
    let _state = supervisor::state_lock(store)?;
    let record = supervisor::read(store)?.ok_or_else(|| {
        failure(
            "inputs_changed",
            "The service activation disappeared before readiness",
        )
    })?;
    if record.activation != attempt.activation || record.generation != attempt.generation {
        return Err(failure(
            "inputs_changed",
            "A newer service lifecycle operation superseded this readiness request",
        ));
    }
    if let (Some(value), Some((manager, observed))) = (value, manager) {
        let identity = &value["daemon"]["identity"];
        let pid = identity["pid"].as_u64().filter(|pid| *pid > 0);
        let process = identity["process_instance"]
            .as_str()
            .filter(|p| !p.is_empty());
        let owned = manager.activation == record.activation
            && observed.registration == "owned"
            && pid.is_some()
            && observed.pid.map(u64::from) == pid
            && process.is_some()
            && process == record.process.as_deref()
            && value["daemon"]["discovery_state"] == "compatible"
            && matches!(record.state.as_str(), "running" | "blocked");
        let inference_ready = !inference
            || record.requested.inference
                && !record.pending.inference
                && value["gateway"]["ready"] == true
                && record.inference_listen.is_some()
                && listener(value, "inference") == record.inference_listen
                && attempt
                    .inference_listen
                    .is_none_or(|address| Some(address) == record.inference_listen);
        let web_ready = !web
            || record.requested.web
                && !record.pending.web
                && value["daemon"]["web"]["state"] == "running"
                && record.web_listen.is_some()
                && listener(value, "web") == record.web_listen
                && attempt
                    .web_listen
                    .is_none_or(|address| Some(address) == record.web_listen);
        if owned && inference_ready && web_ready {
            return Ok(true);
        }
    }
    if matches!(record.state.as_str(), "blocked" | "failed") {
        return Err(failure(
            record
                .reason_code
                .as_deref()
                .unwrap_or("service_readiness_failed"),
            "The supervised service could not establish requested listener readiness; inspect gateway status",
        ));
    }
    Ok(false)
}

async fn ready(
    store: &Store,
    attempt: &supervisor::Activation,
    inference: bool,
    web: bool,
) -> Result<Value> {
    let result = tokio::time::timeout(Duration::from_secs(supervisor::READINESS_SECONDS), async {
        loop {
            let value = qualified_before(store).await?;
            let observation = if value.is_some() {
                Some((
                    supervisor::probe(store).await?,
                    supervisor::inspect(store).await?,
                ))
            } else {
                None
            };
            if readiness(
                store,
                attempt,
                value.as_ref(),
                observation
                    .as_ref()
                    .map(|(manager, observed)| (manager, observed)),
                inference,
                web,
            )? {
                return Ok(value.expect("ready requires a qualified daemon"));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    result.unwrap_or_else(|_|Err(failure("service_readiness_failed", "The supervised service did not establish identified listener readiness before its deadline")))
}
async fn failed_start(
    store: &Store,
    attempt: &supervisor::Activation,
    inference: bool,
    web: bool,
    error: Error,
    effects: &[&str],
) -> Result<Value> {
    let withdrawn = supervisor::withdraw(
        store,
        &attempt.activation,
        attempt.generation,
        inference,
        web,
        &error,
    );
    let mut stop_error = None;
    if matches!(withdrawn, Ok(true)) {
        for (group, selected) in [("gateway", inference), ("web", web)] {
            if selected
                && let Err(error) = rpc(store, "control", Some(request(group, "stop")), None).await
            {
                stop_error = Some(error);
            }
        }
        if supervisor::read(store)?
            .is_some_and(|r| r.activation == attempt.activation && !r.requested.any())
            && let Err(error) = supervisor::stop(store).await
        {
            stop_error = Some(error);
        }
    }
    let mut value = status(store).await.unwrap_or_else(|_| absent());
    value["_code"] = json!("service_readiness_failed");
    value["_status"] = json!("incomplete");
    value["error"] = json!(error);
    value["completed_effects"] = json!(effects);
    value["request_withdrawal"] = json!({"confirmed":matches!(withdrawn,Ok(true))&&stop_error.is_none(),"error":withdrawn.err(),"manager_stop_error":stop_error});
    Ok(value)
}

pub(super) async fn control(store: &Store, _config: &Config, operation: &Request) -> Result<Value> {
    let (group, action) = target(operation);
    #[cfg(unix)]
    if matches!(
        (group, action),
        ("gateway", "start" | "restart") | ("web", "open")
    ) {
        super::control_transport::validate_control_path(&super::control_transport::socket_path(
            store,
        ))?;
    }
    let before = qualified_before(store).await?;
    // Prove manager availability before creating lifecycle state or interrupting a live owner.
    let already_running = before.as_ref().is_some_and(|v| {
        listener(
            v,
            if group == "gateway" {
                "inference"
            } else {
                "web"
            },
        )
        .is_some()
    });
    if action != "restart" && already_running && matches!(action, "start" | "open") {
        let _guard = supervisor::lifecycle_lock(store).await?;
        let mut value = rpc(store, "control", Some(operation.clone()), None)
            .await?
            .ok_or_else(|| {
                failure(
                    "inputs_changed",
                    "The existing daemon stopped before listener reuse",
                )
            })?;
        value["supervision"] = supervisor::view(store, &value["daemon"]).await;
        return Ok(value);
    }
    let observed = status(store).await?;
    let supervised = observed["supervision"]["process_owned"] == true;
    // A compatible existing owner remains usable until an explicit process replacement.
    if before.is_some() && !supervised && action != "restart" && supervisor::read(store).is_ok() {
        let _guard = supervisor::lifecycle_lock(store).await?;
        let mut value = rpc(store, "control", Some(operation.clone()), None)
            .await?
            .ok_or_else(|| {
                failure(
                    "inputs_changed",
                    "The existing daemon stopped before control",
                )
            })?;
        value["supervision"] = supervisor::view(store, &value["daemon"]).await;
        return Ok(value);
    }
    let manager = supervisor::probe(store).await?;
    let inference_listen = if group == "gateway" && matches!(action, "start" | "restart") {
        Some(startup_address(store)?)
    } else {
        None
    };
    if action == "restart" {
        let file = crate::config::load(store)?;
        let gateway = file.config.gateway.as_ref().ok_or_else(|| {
            failure(
                "gateway_unconfigured",
                "Configure the gateway before restarting",
            )
        })?;
        let key = crate::gateway::credential(store.clone(), gateway.key_ref.clone()).await?;
        if key.is_empty() {
            return Err(failure(
                "credential_unavailable",
                "Shared gateway key is unavailable; restart did not interrupt the working owner",
            ));
        }
    }
    let _guard = supervisor::lifecycle_lock(store).await?;
    let current = qualified_before(store).await?;
    if current
        .as_ref()
        .and_then(|v| v["daemon"]["identity"]["process_instance"].as_str())
        != before
            .as_ref()
            .and_then(|v| v["daemon"]["identity"]["process_instance"].as_str())
    {
        return Err(failure(
            "inputs_changed",
            "Daemon ownership changed before the serialized lifecycle operation; inspect current state",
        ));
    }
    if supervisor::read(store).is_err() && matches!(action, "restart" | "stop") {
        supervisor::repair(store, &manager, current.as_ref())?;
    }
    if action == "start"
        && let Some(record) = supervisor::read(store)?
        && record.activation == manager.activation
        && record.requested.inference
        && record
            .inference_listen
            .is_some_and(|v| Some(v) != inference_listen)
    {
        return Err(failure(
            "restart_required",
            "Saved inference address differs from the acknowledged activation; explicitly restart to adopt it",
        ));
    }
    if matches!(action, "stop") {
        let record = supervisor::request(
            store,
            &manager,
            (group == "gateway").then_some(false),
            (group == "web").then_some(false),
            None,
            None,
        )?;
        let mut value =
            if let Some(value) = rpc(store, "control", Some(operation.clone()), None).await? {
                value
            } else {
                absent()
            };
        if !record.requested.any()
            && current
                .as_ref()
                .and_then(|v| v["daemon"]["identity"]["pid"].as_u64())
                != Some(u64::from(std::process::id()))
            && let Err(error) = supervisor::stop(store).await
        {
            value["_code"] = json!("service_control_failed");
            value["_status"] = json!("incomplete");
            value["error"] = json!(error);
        }
        value["supervision"] = supervisor::view(store, &value["daemon"]).await;
        return Ok(value);
    }
    supervisor::prepare(
        store,
        if action == "restart" {
            supervisor::Preparation::Replace
        } else {
            supervisor::Preparation::Preserve
        },
    )
    .await?;
    let mut interrupted = Value::Null;
    let web_was = current.as_ref().and_then(|v| listener(v, "web"));
    if action == "restart"
        && current
            .as_ref()
            .and_then(|v| v["daemon"]["identity"]["pid"].as_u64())
            == Some(u64::from(std::process::id()))
    {
        if !supervised {
            return Err(failure(
                "restart_required",
                "Use the CLI gateway restart to transfer this unsupervised Web owner to the native manager",
            ));
        }
        supervisor::request_restart(
            store,
            &manager,
            web_was.is_some(),
            inference_listen,
            web_was,
        )?;
        let mut value = rpc(store, "restart_process", None, None)
            .await?
            .ok_or_else(|| {
                failure(
                    "service_control_failed",
                    "The existing owner disappeared before accepting restart",
                )
            })?;
        value["old_process_instance"] =
            current.as_ref().unwrap()["daemon"]["identity"]["process_instance"].clone();
        value["restart_completion"] = json!("pending_reconnect");
        value["reconnect_deadline_seconds"] = json!(
            observed["supervision"]["recovery"]["retry_delay_seconds"]
                .as_u64()
                .unwrap_or(supervisor::RESTART_DELAY_SECONDS)
                + supervisor::READINESS_SECONDS
                + super::control::CONTROL_SECONDS
        );
        return Ok(value);
    }
    if action == "restart" {
        // First disarm the old activation; a crash between the two stops cannot revive it.
        supervisor::request(store, &manager, Some(false), Some(false), None, None)?;
        if current.is_some() {
            if let Some(stopped) =
                rpc(store, "control", Some(request("gateway", "stop")), None).await?
            {
                interrupted = stopped["interrupted_requests"].clone();
            }
            if web_was.is_some() {
                rpc(store, "control", Some(request("web", "stop")), None).await?;
            }
        }
        let stopped = supervisor::stop(store).await;
        let manager_stopped = stopped.is_ok();
        let prepared = match stopped {
            Ok(()) => supervisor::prepare(store, supervisor::Preparation::Replace).await,
            Err(error) => Err(error),
        };
        if let Err(error) = prepared {
            let mut value = status(store).await?;
            value["_code"] = json!("service_control_failed");
            value["_status"] = json!("incomplete");
            value["error"] = json!(error);
            value["completed_effects"] = if manager_stopped {
                json!([
                    "recovery_suppressed",
                    "old_listener_stop_requested",
                    "native_manager_stopped"
                ])
            } else {
                json!(["recovery_suppressed", "old_listener_stop_requested"])
            };
            return Ok(value);
        }
    }
    let requested_web = if group == "web" {
        web_address(operation)?
    } else {
        web_was
    };
    let record = supervisor::request(
        store,
        &manager,
        (group == "gateway").then_some(true),
        if group == "web" {
            Some(true)
        } else if action == "restart" {
            Some(web_was.is_some())
        } else {
            None
        },
        inference_listen,
        requested_web,
    )?;
    let new_inference = group == "gateway"
        && current
            .as_ref()
            .is_none_or(|v| listener(v, "inference").is_none())
        || action == "restart";
    let new_web = group == "web"
        && current
            .as_ref()
            .is_none_or(|v| listener(v, "web").is_none())
        || action == "restart" && web_was.is_some();
    if current.is_none() || action == "restart" {
        if let Err(error) = supervisor::start(store).await {
            return failed_start(
                store,
                &record,
                new_inference,
                new_web,
                error,
                &["service_definition_prepared", "listener_request_saved"],
            )
            .await;
        }
    } else if let Err(error) = rpc(store, "control", Some(operation.clone()), None).await {
        return failed_start(
            store,
            &record,
            new_inference,
            new_web,
            error,
            &["listener_request_saved"],
        )
        .await;
    }
    let mut value = match ready(
        store,
        &record,
        group == "gateway",
        group == "web" || action == "restart" && web_was.is_some(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            return failed_start(
                store,
                &record,
                new_inference,
                new_web,
                error,
                &[
                    "service_definition_prepared",
                    "listener_request_saved",
                    "manager_start_requested",
                ],
            )
            .await;
        }
    };
    value["lifecycle"] = json!({"surface":if group=="gateway"{"inference"}else{"web"},"action":if action=="restart"{"restarted"}else{"started"},"requested_listen":if group=="gateway"{inference_listen}else{requested_web}.map(|v|v.to_string()),"actual_listen":listener(&value,if group=="gateway"{"inference"}else{"web"}).map(|v|v.to_string())});
    if action == "restart" {
        value["interrupted_requests"] = interrupted;
        value["recycled"] = json!(true);
    }
    value["supervision"] = supervisor::view(store, &value["daemon"]).await;
    Ok(value)
}
