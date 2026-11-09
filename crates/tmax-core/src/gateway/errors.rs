//! Finite, content-free gateway error responses.
use crate::inputs::ExactInstant;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

pub(super) fn error_response(status: StatusCode, code: &str, id: &str) -> Response {
    error_response_with_message(status, code, safe_message(code), id)
}

pub(super) fn error_response_with_message(
    status: StatusCode,
    code: &str,
    message: &str,
    id: &str,
) -> Response {
    (
        status,
        axum::Json(json!({"error":{"type":code,"message":message,"request_id":id}})),
    )
        .into_response()
}

fn hold_boundary(hold: &Value) -> Option<ExactInstant> {
    hold.get("valid_until")
        .and_then(Value::as_str)
        .and_then(|text| ExactInstant::parse(text).ok())
}

/// Render only the locally derived recovery boundary, never an upstream error.
pub(super) fn quota_guard_message(decision: &Value) -> Option<String> {
    let decision = decision
        .get("fallback")
        .filter(|value| value.is_object())
        .unwrap_or(decision);
    let retained = decision
        .get("continuation")
        .and_then(|continuation| continuation.get("availability_hold"))
        .and_then(hold_boundary);
    let excluded = decision
        .get("excluded_candidates")
        .and_then(Value::as_array);
    let earliest = excluded.and_then(|candidates| {
        candidates
            .iter()
            .filter_map(|candidate| candidate.get("availability_hold"))
            .filter_map(hold_boundary)
            .min()
    });
    let (scope, boundary) = if retained.is_some() {
        ("Retained destination", retained)
    } else if excluded.is_some_and(|candidates| !candidates.is_empty()) {
        ("All eligible candidates", earliest)
    } else {
        return None;
    };
    Some(match boundary {
        Some(boundary) => format!(
            "{scope} held by the local quota guard; earliest recovery at {}; inspect local request metadata for details",
            boundary.canonical()
        ),
        None => format!(
            "{scope} held by the local quota guard; inspect local request metadata for details"
        ),
    })
}
pub(super) fn safe_message(code: &str) -> &'static str {
    match code {
        "local_capacity" => "Local capacity unavailable; request was not dispatched",
        "request_too_large" => "Request exceeds the local body limit",
        "authentication_failed" => "Current shared gateway key required",
        "gateway_identity_prerequisite" => {
            "Codex gateway use requires managed-policy permission and qualified native authorization identity; direct native access remains independent"
        }
        "native_session_required" => {
            "Go requires the real qualified native client and its dynamic conversation session header; a static or fabricated ID is not supported"
        }
        "native_client_required" => {
            "Kimi Coding Plan requires the truthful qualified personal-interactive native client"
        }
        "native_authorization_unqualified" => {
            "The native login owner has no qualified forwarding handoff; keep authorization with its supported direct native owner"
        }
        "upstream_transport_failed" => {
            "Upstream transport failed; execution or completion may be unknown"
        }
        "quota_guard" => {
            "A destination is held by the local quota guard; inspect local request metadata for the recovery boundary"
        }
        _ => "Request unavailable at the indicated stage; inspect local request metadata",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_guard_messages_name_scope_and_earliest_boundary() {
        let decision = json!({
            "excluded_candidates": [
                {"availability_hold": {"valid_until": "2026-11-09T02:00:00Z"}},
                {"availability_hold": {"valid_until": "2026-11-09T01:00:00.5Z"}},
                {"availability_hold": {"valid_until": "2026-11-09T01:00:00Z"}}
            ]
        });
        let message = quota_guard_message(&decision).unwrap();
        assert!(message.contains("All eligible candidates"));
        assert!(message.contains("earliest recovery at 2026-11-09T01:00:00Z"));
        assert!(!message.contains("01:00:00.5Z"));

        let retained = json!({
            "continuation": {
                "availability_hold": {"valid_until": "2026-11-09T03:00:00Z"}
            }
        });
        let message = quota_guard_message(&retained).unwrap();
        assert!(message.contains("Retained destination"));
        assert!(message.contains("2026-11-09T03:00:00Z"));

        let fallback = json!({
            "fallback": {
                "continuation": {
                    "availability_hold": {"valid_until": "2026-11-09T04:00:00Z"}
                }
            }
        });
        assert!(
            quota_guard_message(&fallback)
                .unwrap()
                .contains("2026-11-09T04:00:00Z")
        );
    }
}
