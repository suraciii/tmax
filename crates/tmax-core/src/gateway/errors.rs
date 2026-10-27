//! Finite, content-free gateway error responses.
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

pub(super) fn error_response(status: StatusCode, code: &str, id: &str) -> Response {
    (
        status,
        axum::Json(json!({"error":{"type":code,"message":safe_message(code),"request_id":id}})),
    )
        .into_response()
}
pub(super) fn safe_message(code: &str) -> &'static str {
    match code {
        "local_capacity" => "Local capacity unavailable; request was not dispatched",
        "request_too_large" => "Request exceeds the local body limit",
        "authentication_failed" => "Current shared gateway key required",
        "session_identity_unavailable" => {
            "A qualified native conversation identity is required for session-stable alias forwarding"
        }
        "original_binding_unavailable" => {
            "Original native destination is unavailable; only a qualified new native session can select a new destination"
        }
        "original_resource_binding_unavailable" => {
            "A provider-owned resource lacks its qualified original authorization binding; no lookup or replay was attempted"
        }
        "independent_context_required" => {
            "This alias requires a qualified client with complete independent context and no upstream-owned continuation state"
        }
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
        _ => "Request unavailable at the indicated stage; inspect local request metadata",
    }
}
