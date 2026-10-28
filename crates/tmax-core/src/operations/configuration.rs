use super::arguments::operand;
use crate::{
    Error, Request, Result,
    config::{self, Config},
    storage::Store,
};
use serde_json::{Value, json};

/// Synchronization after a completed desired-state edit is not a second save.
/// Explicit runtime and revocation operations continue to use strict activation.
pub(crate) async fn activate_saved(
    store: &Store,
    config: &Config,
    revision: &str,
    data: &mut Value,
) {
    let status = crate::service::status(store).await;
    let (activation, loaded_revision) = match status {
        Ok(status) => {
            let loaded_revision = status["daemon"]["identity"]["loaded_revision"].clone();
            let activation = match status["daemon"]["discovery_state"].as_str() {
                Some("compatible") => crate::service::activate(store, config, revision)
                    .await
                    .unwrap_or_else(|error| json!({"activation":"failed","error":error})),
                Some("absent") => json!({"activation":"not_running","revision":revision}),
                _ => {
                    json!({"activation":"unavailable","reason_code":status["daemon"]["reason_code"],"discovery_state":status["daemon"]["discovery_state"]})
                }
            };
            (activation, loaded_revision)
        }
        Err(error) => (
            json!({"activation":"unavailable","error":error}),
            Value::Null,
        ),
    };
    project_saved_activation(data, activation, loaded_revision);
}

fn project_saved_activation(data: &mut Value, mut activation: Value, loaded_revision: Value) {
    let outcome_code = activation
        .as_object_mut()
        .and_then(|object| object.remove("_code"))
        .filter(|code| code.as_str().is_some_and(|code| !code.is_empty()));
    if let Some(code) = &outcome_code {
        activation["code"] = code.clone();
    }
    // Discovery/transport failure can occur after the daemon accepted the call.
    // Its previous revision is a before-fact, not proof of the after-state.
    if matches!(
        activation["error"]["code"].as_str(),
        Some(
            "daemon_identity_unavailable"
                | "daemon_incompatible"
                | "daemon_stopping"
                | "daemon_control_path_invalid"
                | "platform_unavailable"
        )
    ) {
        activation["activation"] = json!("unavailable");
    }
    let active = activation["activation"] == "active";
    let unavailable = activation["activation"] == "unavailable";
    let not_running = activation["activation"] == "not_running";
    data["loaded_revision_before"] = loaded_revision;
    data["loaded_revision_after"] = if active {
        activation["revision"].clone()
    } else {
        // A concurrent activation can replace the sampled before-state even
        // when this request receives an identified rejection.
        Value::Null
    };
    data["runtime"] = json!(if active {
        "loaded"
    } else if not_running {
        "not_running"
    } else if unavailable {
        "unavailable"
    } else {
        "not_loaded"
    });
    if outcome_code.is_some() || (!active && !not_running) {
        let reason = activation["error"]["code"]
            .as_str()
            .or_else(|| activation["reason_code"].as_str())
            .or_else(|| outcome_code.as_ref().and_then(Value::as_str))
            .unwrap_or("saved_not_active");
        let warning = json!({"code":reason,"message":"Desired changes are committed; background activation did not establish ready runtime use. Do not repeat the edit; inspect runtime state and explicitly reload or restart as required","affected":[]});
        data.as_object_mut()
            .unwrap()
            .entry("warnings")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap()
            .push(warning);
        data.as_object_mut().unwrap().entry("boundaries").or_insert_with(|| json!([])).as_array_mut().unwrap().push(json!({"name":"activation","state":if unavailable{"blocked"}else if active{"completed"}else{"failed"},"affected":[],"message":"Desired save completed independently; activation and serving readiness are reported separately"}));
        if data.get("session_impact").is_none() {
            data["session_impact"] =
                json!({"state":"unavailable","reason_code":reason,"sessions":null});
        }
        let activation_operation =
            if matches!(reason, "listener_restart_required" | "restart_required") {
                "gateway restart"
            } else {
                "config reload"
            };
        data["next_actions"] = json!([{"code":"inspect_runtime","operation":"status","arguments":[],"message":"Inspect saved and loaded state; do not replay the completed edit"},{"code":"activate_saved","operation":activation_operation,"arguments":[],"message":"After resolving runtime blockers, explicitly activate the current saved configuration"}]);
    }
    data["activation"] = activation;
}

pub(crate) async fn impact(
    store: &Store,
    candidate: &Config,
    file_revision: &str,
    before: Option<&Config>,
) -> Result<Value> {
    config::validate(candidate)?;
    let (loaded, loaded_error) = match crate::service::loaded(store).await {
        Ok(loaded) => (loaded, None),
        Err(error) => (None, Some(error)),
    };
    let after = serde_json::to_value(candidate)
        .map_err(|_| Error::new("operation_failed", "Cannot project validated configuration"))?;
    let loaded_value = loaded
        .as_ref()
        .and_then(|f| serde_json::to_value(&f.config).ok());
    let changes = if loaded_error.is_none() {
        diff_values(loaded_value.as_ref(), Some(&after), "", "activation")
    } else {
        vec![]
    };
    let mut out = json!({"config_impact":{"file_revision_before":file_revision,"file_revision_after":file_revision,"candidate_revision":null,"loaded_revision_before":loaded.as_ref().map(|f|&f.revision),"activation_diff_state":if loaded_error.is_some(){"unavailable"}else if loaded.is_some(){"complete"}else{"no_loaded_snapshot"},"includes_pending_file_changes":loaded.as_ref().map(|f|f.revision!=file_revision)},"changes":changes,"warnings":[{"code":"external_availability_unverified","message":"Dry-run does not read secrets or verify upstream authorization","affected":[]}],"save":"not_executed","runtime":"not_executed","native_application":"not_requested"});
    out["session_impact"] = match crate::service::preview_activation(store, candidate).await {
        Ok(impact) => impact,
        Err(error) => {
            json!({"state":"unavailable","reason_code":error.code,"sessions":null,"listener":null})
        }
    };
    if let Some(error) = loaded_error {
        out["warnings"].as_array_mut().unwrap().push(json!({"code":error.code,"message":"Loaded configuration and complete activation impact are unavailable; local desired changes remain inspectable","affected":[]}));
    }
    if let Some(before) = before {
        let before = serde_json::to_value(before)
            .map_err(|_| Error::new("operation_failed", "Cannot project prior configuration"))?;
        let saves = diff_values(Some(&before), Some(&after), "", "file_save");
        out["changes"].as_array_mut().unwrap().extend(saves);
    }
    Ok(out)
}
pub(crate) fn diff_values(
    before: Option<&Value>,
    after: Option<&Value>,
    path: &str,
    boundary: &str,
) -> Vec<Value> {
    if before == after {
        return vec![];
    }
    if before.is_some_and(Value::is_object) && after.is_some_and(Value::is_object) {
        let b = before.unwrap().as_object().unwrap();
        let a = after.unwrap().as_object().unwrap();
        let keys: std::collections::BTreeSet<_> = b.keys().chain(a.keys()).collect();
        return keys
            .into_iter()
            .flat_map(|key| {
                diff_values(
                    b.get(key),
                    a.get(key),
                    &if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    },
                    boundary,
                )
            })
            .collect();
    }
    vec![
        json!({"boundary":boundary,"resource":{"kind":"configuration","id":null,"scope":null},"path":path,"kind":if before.is_none(){"add"}else if after.is_none(){"remove"}else{"replace"},"before":before,"after":after}),
    ]
}
pub(super) fn recovery(store: &Store, r: &Request) -> Result<Value> {
    match r.command.last().map(String::as_str) {
        Some("list") => crate::secrets::recovery_list(store),
        Some("inspect") => crate::secrets::recovery_inspect(store, operand(r, 0)?),
        Some("discard") => {
            if !r.dry_run {
                crate::secrets::recovery_discard(store, operand(r, 0)?)?;
            }
            Ok(json!({"recovery":operand(r,0)?,"discarded":!r.dry_run}))
        }
        _ => Err(Error::new(
            "invalid_arguments",
            "Unknown recovery operation",
        )),
    }
}
pub(super) async fn reset(store: &Store, r: &Request) -> Result<Value> {
    let path = store.root.join("tmax.toml");
    let bytes = std::fs::read(&path).map_err(|_| {
        Error::new(
            "invalid_arguments",
            "Reset requires an existing readable invalid configuration",
        )
    })?;
    let expected = config::revision(&bytes);
    if r.if_revision.as_ref().is_some_and(|rev| rev != &expected) {
        return Err(Error::new(
            "inputs_changed",
            "Rejected file revision changed",
        ));
    }
    if std::str::from_utf8(&bytes)
        .ok()
        .is_some_and(|s| config::parse(s).is_ok())
    {
        return Err(Error::new(
            "invalid_arguments",
            "Reset is permitted only for an existing invalid configuration",
        ));
    }
    if r.dry_run {
        let mut preview = impact(store, &Config::empty(), &expected, None).await?;
        preview["recovery"] = json!("protected_capture_not_executed");
        preview["original_revision"] = json!(expected);
        preview["reset"] = json!("not_executed");
        preview["desired_settings"] = json!("all removed");
        preview["native_files"] = json!("unchanged");
        preview["stored_secrets"] = json!("retained");
        return Ok(preview);
    }
    let recovery = crate::secrets::protect_recovery(store, &bytes)?;
    let document = "version = 1\n".parse().expect("constant TOML");
    let revision = match config::save(store, &expected, &document) {
        Ok(revision) => revision,
        Err(error) => {
            return Ok(
                json!({"_code":"saved_not_active","recovery":recovery,"save":"failed","save_error":error,"activation":"not_requested"}),
            );
        }
    };
    let empty = Config::empty();
    let activation = crate::service::activate(store, &empty, &revision).await;
    let autostart = crate::service::reconcile_autostart(store, &empty, false).await;
    Ok(
        json!({"_code":if activation.is_err()||autostart.is_err(){json!("saved_not_active")}else{Value::Null},"recovery":recovery,"save":"saved","file_revision":revision,"activation":activation.unwrap_or_else(|error|json!({"activation":"failed","error":error})),"autostart":autostart.unwrap_or_else(|error|json!({"state":"failed","error":error})),"native_files":"unchanged","stored_secrets":"retained"}),
    )
}

pub(super) fn validate(store: &Store, file: &config::LoadedFile) -> Result<Value> {
    crate::reasoning::validate_saved(store, &file.config)?;
    Ok(
        json!({"snapshot_state":if file.exists{"available"}else{"absent"},"valid":true,"revision":file.revision,"external_availability":"not_verified"}),
    )
}

pub(super) async fn reload(store: &Store, r: &Request) -> Result<Value> {
    let f = config::load(store)?;
    if let Some(expected) = &r.if_revision
        && f.revision != *expected
    {
        return Err(Error::new(
            "inputs_changed",
            "Saved configuration revision differs from the requested guard",
        ));
    }
    crate::reasoning::validate_saved(store, &f.config)?;
    if r.dry_run {
        return impact(store, &f.config, &f.revision, None).await;
    }
    if f.exists {
        crate::routing::reconcile(store, &f.config, &f.config, &[])?;
    }
    let autostart = crate::service::reconcile_autostart(store, &f.config, false).await;
    let activation = crate::service::activate(store, &f.config, &f.revision).await;
    let failed = autostart.is_err() || activation.is_err();
    Ok(
        json!({"_code":if failed{json!("saved_not_active")}else{Value::Null},"file_revision":f.revision,"save":"not_requested","activation":activation.unwrap_or_else(|e|json!({"state":"failed","code":e.code,"message":e.message})),"autostart":autostart.unwrap_or_else(|e|json!({"state":"failed","code":e.code,"message":e.message})),"native_application":"not_requested"}),
    )
}

#[cfg(test)]
mod tests {
    use super::super::results::success_response;
    use super::*;
    #[test]
    fn background_failure_preserves_save_success_but_explicit_activation_stays_strict() {
        let activation = json!({"_code":"saved_not_active","activation":"failed","error":{"code":"listener_restart_required","message":"Explicit restart required"}});
        let mut saved =
            json!({"save":"saved","file_revision":"new","native_application":"not_requested"});
        project_saved_activation(&mut saved, activation.clone(), json!("old"));
        let response = success_response("provider add", false, saved);
        assert_eq!(response.exit_code, 0);
        assert_eq!(response.body["status"], "success");
        let data = &response.body["data"];
        assert_eq!(data["save"], "saved");
        assert_eq!(data["activation"]["activation"], "failed");
        assert_eq!(
            data["activation"]["error"]["code"],
            "listener_restart_required"
        );
        assert_eq!(data["warnings"][0]["code"], "listener_restart_required");
        assert_eq!(data["loaded_revision_before"], "old");
        assert!(data["loaded_revision_after"].is_null());
        assert_eq!(data["boundaries"][0]["state"], "failed");
        assert!(data["session_impact"]["sessions"].is_null());
        assert_eq!(data["next_actions"][1]["operation"], "gateway restart");
        assert_eq!(
            success_response(
                "config reload",
                false,
                json!({"save":"not_requested","activation":activation.clone()})
            )
            .exit_code,
            4
        );
        assert_eq!(
            success_response(
                "gateway key replace",
                false,
                json!({"save":"saved","activation":activation})
            )
            .exit_code,
            4
        );
    }

    #[test]
    fn background_uncertain_reply_never_claims_the_prior_revision_is_still_loaded() {
        let mut data = json!({"save":"saved","file_revision":"new"});
        project_saved_activation(
            &mut data,
            json!({"_code":"saved_not_active","activation":"failed","error":{"code":"daemon_identity_unavailable"}}),
            json!("old"),
        );
        assert_eq!(data["loaded_revision_before"], "old");
        assert!(data["loaded_revision_after"].is_null());
        assert_eq!(data["runtime"], "unavailable");
        assert_eq!(data["activation"]["activation"], "unavailable");
        assert_eq!(
            success_response("provider update", false, data).exit_code,
            0
        );
        let mut stopped = json!({"save":"saved"});
        project_saved_activation(
            &mut stopped,
            json!({"activation":"not_running"}),
            json!("old"),
        );
        assert!(stopped["loaded_revision_after"].is_null());
        assert_eq!(stopped["runtime"], "not_running");
    }

    #[test]
    fn background_concurrent_revision_rejection_reports_no_unproven_after_revision() {
        // R1 was committed by this edit. Status sampled R0, then another writer
        // committed and activated R2 before activation of R1 was rejected.
        let mut saved = json!({"save":"saved","file_revision":"R1"});
        project_saved_activation(
            &mut saved,
            json!({"_code":"saved_not_active","activation":"failed","error":{"code":"inputs_changed"}}),
            json!("R0"),
        );
        let response = success_response("provider update", false, saved);
        assert_eq!(response.exit_code, 0);
        assert_eq!(response.body["status"], "success");
        assert_eq!(response.body["data"]["file_revision"], "R1");
        assert_eq!(response.body["data"]["loaded_revision_before"], "R0");
        assert!(response.body["data"]["loaded_revision_after"].is_null());
        assert_eq!(
            response.body["data"]["activation"]["error"]["code"],
            "inputs_changed"
        );
        assert_eq!(
            response.body["data"]["warnings"][0]["code"],
            "inputs_changed"
        );
    }

    #[test]
    fn background_confirmed_activation_has_no_failed_runtime_warning_for_edit_or_catalog() {
        for operation in ["provider update", "catalog apply"] {
            let mut committed = if operation == "catalog apply" {
                json!({"catalog_apply":"completed","save":"not_requested"})
            } else {
                json!({"save":"saved","file_revision":"R1"})
            };
            project_saved_activation(
                &mut committed,
                json!({"_code":null,"activation":"active","revision":"R1","serving_state":"stopped"}),
                json!("R0"),
            );
            let response = success_response(operation, false, committed);
            assert_eq!(response.exit_code, 0);
            assert_eq!(response.body["code"], "ok");
            assert_eq!(response.body["status"], "success");
            let data = &response.body["data"];
            assert_eq!(data["runtime"], "loaded");
            assert_eq!(data["activation"]["activation"], "active");
            assert_eq!(data["loaded_revision_before"], "R0");
            assert_eq!(data["loaded_revision_after"], "R1");
            assert!(data.get("warnings").is_none());
            assert!(data.get("boundaries").is_none());
            assert!(data.get("session_impact").is_none());
            assert!(data.get("next_actions").is_none());
        }
    }
}
