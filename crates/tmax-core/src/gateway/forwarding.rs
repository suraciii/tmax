//! Same-protocol routing and pull-driven upstream response forwarding.
use super::{
    FORWARD_CHUNK_BYTES, REQUEST_BYTES, RequestFacts,
    client_identity::{hop_header, upstream_headers},
    errors::error_response,
    request_body,
    response_observer::Observer,
    runtime::{Flight, Runtime, credential},
};
use crate::{
    config::{Auth, Config},
    diagnostics::safe_evidence,
};
use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    extract::State,
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use futures_util::{Stream, StreamExt};
use serde_json::{Value, json};
use std::{pin::Pin, sync::Arc, time::Duration};

pub fn router(runtime: Arc<Runtime>) -> Router {
    Router::new().fallback(forward).with_state(runtime)
}

fn matches_key(supplied: &[u8], key: &[u8]) -> bool {
    let mut difference = supplied.len() ^ key.len();
    for (index, byte) in key.iter().enumerate() {
        difference |= usize::from(*byte ^ supplied.get(index).copied().unwrap_or(0));
    }
    difference == 0
}
fn local_key(headers: &HeaderMap) -> Option<&[u8]> {
    if let Some(value) = headers.get("x-gateway-key") {
        return Some(value.as_bytes());
    }
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::as_bytes)
        .or_else(|| headers.get("x-api-key").map(|v| v.as_bytes()))
}
pub(super) async fn forward(
    State(runtime): State<Arc<Runtime>>,
    request: axum::extract::Request,
) -> Response {
    let record = runtime.history.begin();
    let id = record.request.clone();
    let mut flight = match runtime.admit(record.clone()) {
        Ok(v) => v,
        Err(e) => {
            let mut record = record;
            record.outcome = "rejected".into();
            record.stage = "local_admission".into();
            record.reason_code = Some(e.code.clone());
            runtime.history.update(record);
            return error_response(StatusCode::SERVICE_UNAVAILABLE, &e.code, &id);
        }
    };
    let snapshot = runtime.snapshot.read().expect("snapshot lock").clone();
    let Some(snapshot) = snapshot else {
        return flight.reject(
            StatusCode::SERVICE_UNAVAILABLE,
            "listener_stopped",
            "local_admission",
        );
    };
    let (parts, body) = request.into_parts();
    if parts.headers.contains_key("origin") {
        return flight.reject(
            StatusCode::FORBIDDEN,
            "browser_inference_unavailable",
            "shared_key_authentication",
        );
    }
    if !local_key(&parts.headers).is_some_and(|key| matches_key(key, &snapshot.key)) {
        return flight.reject(
            StatusCode::UNAUTHORIZED,
            "authentication_failed",
            "shared_key_authentication",
        );
    }
    if parts.headers.contains_key("x-gateway-key") {
        return flight.reject(
            StatusCode::CONFLICT,
            "gateway_identity_prerequisite",
            "native_identity",
        );
    }
    if parts.method == Method::GET
        && matches!(parts.uri.path(), "/models" | "/v1/models")
        && parts.uri.query().is_none()
    {
        flight.record.admitted_at = Some(Utc::now());
        flight.record.downstream_complete = true;
        flight.finish("completed", "model_discovery", None);
        return axum::Json(publication(&snapshot.config)).into_response();
    }
    let Some(protocol) = crate::provider_integration::inference_protocol(
        parts.method.as_str(),
        parts.uri.path(),
        parts.uri.query(),
    ) else {
        return flight.reject(
            if parts.uri.query().is_some() {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::NOT_FOUND
            },
            "undeclared_operation",
            "operation_validation",
        );
    };
    flight.record.protocol = Some(protocol.into());
    if parts.headers.contains_key("upgrade") {
        return flight.reject(
            StatusCode::BAD_REQUEST,
            "undeclared_transport",
            "operation_validation",
        );
    }
    let bytes = tokio::select! {_ = flight.cancel.changed()=>{return flight.reject(StatusCode::SERVICE_UNAVAILABLE,"force_stop","local_admission");},result=tokio::time::timeout(Duration::from_secs(30),to_bytes(body,REQUEST_BYTES))=>{match result{Ok(Ok(v))=>v,Ok(Err(_))=>return flight.reject(StatusCode::PAYLOAD_TOO_LARGE,"request_too_large","local_admission"),Err(_)=>return flight.reject(StatusCode::REQUEST_TIMEOUT,"request_acquisition_deadline","local_admission")}}};
    let body = match RequestFacts::parse(&bytes) {
        Ok(body) => body,
        _ => {
            flight.record.model_kind = "malformed".into();
            return flight.reject(StatusCode::BAD_REQUEST, "invalid_json", "model_validation");
        }
    };
    let Some(public_id) = body
        .model
        .as_ref()
        .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
        .cloned()
    else {
        flight.record.model_kind = "malformed".into();
        return flight.reject(StatusCode::BAD_REQUEST, "invalid_model", "model_validation");
    };
    let (snapshot, choice, now) = {
        let authority = runtime.snapshot.read().expect("snapshot lock");
        let Some(snapshot) = authority.as_ref().cloned() else {
            return flight.reject(
                StatusCode::SERVICE_UNAVAILABLE,
                "listener_unavailable",
                "local_admission",
            );
        };
        if !local_key(&parts.headers).is_some_and(|key| matches_key(key, &snapshot.key)) {
            return flight.reject(
                StatusCode::UNAUTHORIZED,
                "authentication_failed",
                "shared_key_authentication",
            );
        }
        let alias = snapshot
            .config
            .model_aliases
            .iter()
            .any(|a| a.id == public_id && a.protocol == protocol);
        let recognized = alias || crate::config::model(&snapshot.config, &public_id).is_ok();
        if !recognized {
            flight.record.model_kind = "unrecognized".into();
            return flight.reject(StatusCode::NOT_FOUND, "unknown_model", "model_validation");
        }
        flight.record.model = Some(public_id.clone());
        flight.record.model_kind = if alias { "alias" } else { "concrete" }.into();
        let now = Utc::now();
        let choice = match runtime.choose_recorded(
            &snapshot,
            (&public_id, protocol),
            &body,
            &parts.headers,
            now,
            &mut flight.record.decision,
        ) {
            Ok(v) => v,
            Err(e) => {
                let selected = flight
                    .record
                    .decision
                    .get("selection")
                    .or_else(|| flight.record.decision.get("selected"));
                flight.record.provider = selected
                    .and_then(|v| v.get("provider"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                flight.record.qualified_model = selected
                    .and_then(|v| v.get("qualified_model"))
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                return flight.reject(StatusCode::CONFLICT, &e.code, "selection");
            }
        };
        (snapshot, choice, now)
    };
    let selection = choice.selection;
    let Some(provider) = snapshot.config.providers.get(&selection.provider) else {
        return flight.reject(StatusCode::BAD_REQUEST, "provider_unavailable", "selection");
    };
    let normalize = body.normalization_required;
    drop(body);
    let endpoint = choice.endpoint;
    let path = match crate::provider_integration::operation_path(protocol) {
        Ok(v) => v,
        Err(e) => return flight.reject(StatusCode::BAD_REQUEST, &e.code, "operation_validation"),
    };
    flight.record.admitted_at = Some(now);
    flight.record.configuration_revision = Some(snapshot.revision.clone());
    flight.record.provider = Some(selection.provider.clone());
    flight.record.qualified_model = Some(format!("{}/{}", selection.provider, selection.model));
    flight.record.authorization_generation = Some(choice.authorization_generation);
    flight.record.decision = safe_evidence(&selection.evidence);
    flight.record.stage = "credential_acquisition".into();
    runtime.history.update(flight.record.clone());
    let Some(Auth::ApiKey { key_ref }) = &provider.auth else {
        return flight.reject(
            StatusCode::SERVICE_UNAVAILABLE,
            "credential_unavailable",
            "credential_acquisition",
        );
    };
    let store = runtime.store.clone();
    let key_ref = key_ref.clone();
    let key = tokio::select! {biased;_ = flight.cancel.changed()=>return flight.reject(StatusCode::SERVICE_UNAVAILABLE,"force_stop","credential_acquisition"),result=credential(store,key_ref)=>match result{Ok(value)=>value,Err(error)=>return flight.reject(StatusCode::SERVICE_UNAVAILABLE,&error.code,"credential_acquisition")}};
    let key = match std::str::from_utf8(&key) {
        Ok(v) => v,
        Err(_) => {
            return flight.reject(
                StatusCode::SERVICE_UNAVAILABLE,
                "credential_invalid",
                "credential_acquisition",
            );
        }
    };
    let bytes = match request_body::rewrite(
        bytes,
        &selection.model,
        protocol,
        choice.effort.as_deref(),
        normalize,
    ) {
        Ok(bytes) => bytes,
        Err(_) => {
            return flight.reject(StatusCode::BAD_REQUEST, "invalid_json", "model_validation");
        }
    };
    let go_session_header =
        if crate::provider_integration::integration_id(provider).ok() == Some("opencode-go") {
            choice.go_session_header
        } else {
            None
        };
    let upstream_url = match parts.uri.query() {
        Some(query) => format!("{}{}?{}", endpoint.trim_end_matches('/'), path, query),
        None => format!("{}{}", endpoint.trim_end_matches('/'), path),
    };
    let mut outbound = snapshot
        .client
        .post(upstream_url.clone())
        .headers(upstream_headers(
            &parts.headers,
            go_session_header,
            &upstream_url,
        ))
        .body(bytes);
    if protocol == "messages"
        && crate::provider_integration::integration_id(provider).ok() != Some("opencode-go")
    {
        outbound = outbound.header("x-api-key", key);
    } else {
        outbound = outbound.bearer_auth(key);
    }
    flight.record.execution = "unknown".into();
    flight.record.stage = "upstream_dispatch".into();
    runtime.history.update(flight.record.clone());
    let response = tokio::select! {biased;_ = flight.cancel.changed()=>{flight.finish("interrupted","upstream_dispatch",Some("force_stop"));return error_response(StatusCode::SERVICE_UNAVAILABLE,"force_stop",&id);},result=outbound.send()=>match result{Ok(v)=>v,Err(e)=>{let code=if e.is_connect(){"upstream_connection_failed"}else{"upstream_transport_failed"};flight.finish("failed",if e.is_connect(){"upstream_connection"}else{"upstream_dispatch"},Some(code));return error_response(StatusCode::BAD_GATEWAY,code,&id);}}};
    let status = response.status();
    let headers = response.headers().clone();
    flight.record.upstream_status = Some(status.as_u16());
    flight.record.stage = "upstream_response".into();
    runtime.history.update(flight.record.clone());
    let sse = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"));
    let state = ForwardStream {
        upstream: Box::pin(response.bytes_stream()),
        pending: Bytes::new(),
        flight,
        observer: Observer::new(protocol, sse, status.is_success()),
        done: false,
    };
    let stream = futures_util::stream::unfold(state, |mut state| async move {
        if state.done {
            return None;
        }
        let interrupted = *state.flight.cancel.borrow();
        if interrupted {
            state.observer.finish(&mut state.flight.record);
            state
                .flight
                .finish("interrupted", "upstream_stream", Some("force_stop"));
            state.done = true;
            return Some((Err(std::io::Error::other("request interrupted")), state));
        }
        if !state.pending.is_empty() {
            let n = state.pending.len().min(FORWARD_CHUNK_BYTES);
            let chunk = state.pending.split_to(n);
            return Some((Ok::<Bytes, std::io::Error>(chunk), state));
        }
        tokio::select! {
            biased;
            _ = state.flight.cancel.changed()=>{state.observer.finish(&mut state.flight.record);state.flight.finish("interrupted","upstream_stream",Some("force_stop"));state.done=true;Some((Err(std::io::Error::other("request interrupted")),state))},
            result=state.upstream.next()=>match result {
                Some(Ok(bytes))=>{state.observer.observe(&bytes);state.pending=bytes;let n=state.pending.len().min(FORWARD_CHUNK_BYTES);let chunk=state.pending.split_to(n);Some((Ok(chunk),state))},
                Some(Err(_))=>{state.observer.finish(&mut state.flight.record);state.flight.finish("failed","upstream_stream",Some("upstream_stream_failed"));state.done=true;Some((Err(std::io::Error::other("upstream stream failed")),state))},
                None=>{state.observer.finish(&mut state.flight.record);state.flight.record.downstream_complete=true;let outcome=if !state.observer.success{"failed"}else if state.observer.complete{"completed"}else{"unknown"};state.flight.finish(outcome,"downstream_delivery",if state.observer.complete{None}else{Some("completion_not_evidenced")});None}
            }
        }
    });
    let mut downstream = Response::new(Body::from_stream(stream));
    *downstream.status_mut() = status;
    for (name, value) in &headers {
        if !hop_header(name.as_str()) && name != "content-length" {
            downstream.headers_mut().insert(name.clone(), value.clone());
        }
    }
    if let Ok(v) = id.parse() {
        downstream.headers_mut().insert("x-tmax-request-id", v);
    }
    downstream
}
struct ForwardStream {
    upstream: Pin<Box<dyn Stream<Item = std::result::Result<Bytes, reqwest::Error>> + Send>>,
    pending: Bytes,
    flight: Flight,
    observer: Observer,
    done: bool,
}
impl Drop for ForwardStream {
    fn drop(&mut self) {
        if !self.flight.finished {
            self.observer.finish(&mut self.flight.record);
            let interrupted = *self.flight.cancel.borrow();
            self.flight.finish(
                if interrupted {
                    "interrupted"
                } else {
                    "cancelled"
                },
                "downstream_delivery",
                Some(if interrupted {
                    "force_stop"
                } else {
                    "downstream_disconnected"
                }),
            );
        }
    }
}

pub fn publication(config: &Config) -> Value {
    let mut data = Vec::new();
    for (provider_id, p) in &config.providers {
        if !p.enabled || p.auth.is_none() {
            continue;
        }
        for (model_id, m) in &p.models {
            if !m.enabled {
                continue;
            }
            if let Ok(protocols) = crate::provider_integration::protocols(p) {
                data.push(json!({"id":format!("{provider_id}/{model_id}"),"object":"model","owned_by":provider_id,"protocols":protocols}));
            }
        }
    }
    for a in &config.model_aliases {
        if a.enabled {
            data.push(json!({"id":a.id,"object":"model","protocols":[a.protocol],"continuity":a.continuity.as_deref().unwrap_or("session")}));
        }
    }
    json!({"object":"list","data":data})
}

#[cfg(test)]
#[path = "forwarding_tests.rs"]
mod tests;
