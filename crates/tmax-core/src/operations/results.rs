use crate::{Error, Response};
use serde_json::{Value, json};

pub(super) fn success_response(operation: &str, dry_run: bool, mut data: Value) -> Response {
    let nested_code = data
        .get("_code")
        .and_then(Value::as_str)
        .or_else(|| data.pointer("/activation/_code").and_then(Value::as_str))
        .or_else(|| data.pointer("/autostart/_code").and_then(Value::as_str))
        .map(str::to_string);
    let code = nested_code
        .as_deref()
        .unwrap_or(if dry_run { "dry_run_ready" } else { "ok" });
    let exit = match code {
        "awaiting_native_reload" if matches!(operation, "agent apply" | "agent restore") => 0,
        "awaiting_native_reload"
        | "restart_required"
        | "runtime_blocked"
        | "daemon_incompatible"
        | "active_requests"
        | "inputs_changed"
        | "unsupported_source"
        | "credential_unavailable"
        | "noninteractive_unavailable"
        | "interactive_required"
        | "login_source_required"
        | "native_not_installed"
        | "native_version_probe_failed"
        | "native_version_unknown"
        | "native_version_unqualified"
        | "native_schema_incompatible"
        | "target_incomplete" => 3,
        "ok" | "dry_run_ready" | "daemon_reused" | "service_restart_requested" => 0,
        _ => 4,
    };
    let status = if code == "service_restart_requested" {
        "pending"
    } else if dry_run && exit != 0 {
        "blocked"
    } else if let Some(status) = data
        .get("_status")
        .and_then(Value::as_str)
        .filter(|s| matches!(*s, "blocked" | "incomplete" | "cancelled"))
    {
        status
    } else if exit == 3
        && data
            .get("blockers")
            .and_then(Value::as_array)
            .is_some_and(|blockers| !blockers.is_empty())
    {
        "blocked"
    } else if exit != 0 {
        "incomplete"
    } else if dry_run {
        "dry_run"
    } else {
        "success"
    };
    let status = status.to_string();
    if let Some(o) = data.as_object_mut() {
        o.remove("_code");
        o.remove("_status");
    }
    Response {
        exit_code: exit,
        body: json!({"schema_version":1,"operation":operation,"code":code,"status":status,"data":data}),
    }
}
pub(super) fn error_response(operation: &str, error: Error, dry_run: bool) -> Response {
    let code = error.code.as_str();
    let exit = if matches!(
        code,
        "inputs_changed"
            | "credential_unavailable"
            | "storage_version_unsupported"
            | "login_source_required"
            | "interactive_required"
            | "unsupported_protected_input"
            | "unsupported_protected_output"
            | "target_incomplete"
            | "source_incomplete"
            | "source_oversized"
            | "source_unreadable"
            | "unsafe_native_file"
            | "source_cwd_missing"
            | "source_ambiguous_branch"
            | "same_session_store"
            | "session_store_unresolved"
            | "destination_cwd_missing"
            | "destination_cwd_unrepresentable"
            | "destination_store_unwritable"
            | "destination_locked"
            | "context_budget_exceeded"
            | "unsupported_destination"
            | "title_collision"
            | "model_unknown"
            | "already_imported"
            | "native_store_unreadable"
            | "native_reader_unavailable"
            | "daemon_incompatible"
            | "daemon_stopping"
            | "daemon_identity_unavailable"
            | "daemon_control_path_invalid"
            | "service_manager_unavailable"
            | "service_registration_conflict"
            | "endpoint_occupied"
            | "endpoint_conflict"
            | "active_requests"
            | "restart_required"
            | "model_unavailable"
            | "catalog_unavailable"
            | "compatibility_unavailable"
            | "acquisition_in_progress"
            | "recovery_required"
    ) {
        3
    } else if code == "cancelled" {
        5
    } else if code.starts_with("invalid")
        || matches!(
            code,
            "unresolved_reference"
                | "unknown_resource"
                | "unsupported_operation"
                | "unsupported_source"
                | "native_schema_unsupported"
                | "unsupported_agent"
        )
        || (error.field.as_deref().is_some_and(|field| {
            (field.starts_with("model_aliases.") && field.contains(".candidate_reasoning_effort."))
                || (field.starts_with("providers.") && field.contains(".effort_map."))
        }) && matches!(
            code,
            "reasoning_incompatible"
                | "reasoning_effort_incompatible"
                | "reasoning_effort_unavailable"
                | "candidate_reasoning_protocol_unsupported"
        ))
    {
        2
    } else {
        4
    };
    Response {
        exit_code: exit,
        body: json!({"schema_version":1,"operation":operation,"code":code,"status":if exit==3{"blocked"}else if exit==5{"cancelled"}else{"error"},"data":if error.field.is_some(){json!({"field":error.field})}else{Value::Null},"blockers":[{"code":code,"message":error.message,"affected":error.affected}],"dry_run":dry_run}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_registration_reports_partial_effects_without_losing_saved_intent() {
        for enabled in [true, false] {
            let data = json!({"save":"saved","runtime":"not_running","autostart":{
                "_code":"saved_not_active","reconciliation":"failed",
                "completed_effects":if enabled {vec!["owned_unit_written"]} else {vec![]},
                "status":{"saved_intent":enabled,"login_enabled":false},
                "error":{"code":"autostart_failed"}
            }});
            let response = success_response("gateway autostart", false, data);
            assert_eq!(response.exit_code, 4);
            assert_eq!(response.body["code"], "saved_not_active");
            assert_eq!(response.body["status"], "incomplete");
            assert_eq!(response.body["data"]["save"], "saved");
            assert_eq!(response.body["data"]["runtime"], "not_running");
            assert_eq!(
                response.body["data"]["autostart"]["status"]["saved_intent"],
                enabled
            );
            assert_eq!(
                response.body["data"]["autostart"]["completed_effects"]
                    .as_array()
                    .unwrap()
                    .len(),
                usize::from(enabled)
            );
        }
    }
    #[test]
    fn candidate_effort_configuration_errors_are_invalid_input_not_runtime_failures() {
        for code in [
            "reasoning_incompatible",
            "reasoning_effort_incompatible",
            "reasoning_effort_unavailable",
            "candidate_reasoning_protocol_unsupported",
        ] {
            let configured = error_response(
                "model alias update",
                Error::new(code, "Unsupported configured candidate effort")
                    .field("model_aliases.fast.candidate_reasoning_effort.p/m"),
                false,
            );
            assert_eq!(configured.exit_code, 2);
            assert_eq!(configured.body["code"], code);
            assert_eq!(configured.body["status"], "error");
            let mapped = error_response(
                "model effort-map set",
                Error::new(code, "Unsupported mapped effort")
                    .field("providers.p.models.m.effort_map.responses.xhigh"),
                false,
            );
            assert_eq!(mapped.exit_code, 2);
            assert_eq!(mapped.body["code"], code);
            let runtime =
                error_response("gateway probe", Error::new(code, "Runtime mismatch"), false);
            assert_eq!(runtime.exit_code, 4);
        }
    }
    #[test]
    fn session_import_preconditions_are_blocked_and_native_failures_are_runtime_errors() {
        for code in [
            "source_incomplete",
            "source_oversized",
            "source_unreadable",
            "source_cwd_missing",
            "source_ambiguous_branch",
            "same_session_store",
            "session_store_unresolved",
            "destination_cwd_missing",
            "destination_cwd_unrepresentable",
            "destination_store_unwritable",
            "destination_locked",
            "context_budget_exceeded",
            "unsupported_destination",
            "title_collision",
            "model_unknown",
            "already_imported",
        ] {
            let response = error_response("session import", Error::new(code, "blocked"), false);
            assert_eq!(response.exit_code, 3, "{code}");
            assert_eq!(response.body["status"], "blocked");
        }
        for code in [
            "unsupported_source",
            "native_schema_unsupported",
            "unsupported_agent",
        ] {
            let response = error_response("session import", Error::new(code, "invalid"), false);
            assert_eq!(response.exit_code, 2, "{code}");
        }
        for code in [
            "committed_unverified",
            "native_write_failed",
            "record_failed",
        ] {
            let response = error_response("session import", Error::new(code, "runtime"), false);
            assert_eq!(response.exit_code, 4, "{code}");
        }
    }
    #[test]
    fn completed_native_writes_are_success_with_a_reload_reminder() {
        for operation in ["agent apply", "agent restore"] {
            let response =
                success_response(operation, false, json!({"_code":"awaiting_native_reload"}));
            assert_eq!(response.exit_code, 0);
            assert_eq!(response.body["status"], "success");
        }
        let login = success_response(
            "provider login",
            false,
            json!({"_code":"awaiting_native_reload"}),
        );
        assert_eq!(login.exit_code, 3);
    }
    #[test]
    fn blocker_evidence_sets_blocked_status_without_a_private_marker() {
        let response = success_response(
            "agent apply",
            false,
            json!({"_code":"native_schema_incompatible","blockers":[{"code":"native_schema_incompatible"}]}),
        );
        assert_eq!(response.exit_code, 3);
        assert_eq!(response.body["status"], "blocked");
        assert!(response.body["data"].get("_status").is_none());
    }
}
