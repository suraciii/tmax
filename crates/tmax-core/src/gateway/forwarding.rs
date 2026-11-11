//! Same-protocol routing and pull-driven upstream response forwarding.
use super::{
    FORWARD_CHUNK_BYTES, INSPECTION_BYTES, QUOTA_ERROR_INSPECTION_SECONDS, REQUEST_BYTES,
    RequestFacts,
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
use futures_util::{FutureExt, Stream, StreamExt};
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
            runtime.log_error(&mut record, &[]);
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
    let snapshot;
    let (choice, now, lineage, failure_epochs) = {
        let authority = runtime.snapshot.read().expect("snapshot lock");
        let Some(loaded) = authority.as_ref().cloned() else {
            return flight.reject(
                StatusCode::SERVICE_UNAVAILABLE,
                "listener_unavailable",
                "local_admission",
            );
        };
        snapshot = loaded;
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
        let lineage = crate::inputs::Snapshot::capture_loaded(
            &runtime.store,
            &snapshot.config,
            snapshot.metadata.as_deref(),
        );
        let failure_epochs =
            match crate::failures::Snapshot::capture(&runtime.store, &snapshot.config) {
                Ok(epochs) => epochs,
                Err(error) => {
                    return flight.reject(
                        StatusCode::SERVICE_UNAVAILABLE,
                        &error.code,
                        "selection",
                    );
                }
            };
        let choice = match runtime.choose_untried(
            &snapshot,
            (&public_id, protocol),
            &body,
            &parts.headers,
            now,
            &mut flight.record.decision,
            &Default::default(),
            &lineage,
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
        (choice, now, lineage, failure_epochs)
    };
    let mut choice = choice;
    let mut attempted = std::collections::BTreeSet::new();
    let mut attempts = AttemptEvidence::default();
    let fallback = snapshot
        .config
        .model_aliases
        .iter()
        .find(|alias| alias.id == public_id && alias.protocol == protocol)
        .and_then(|alias| alias.fallback.as_ref());
    flight.record.admitted_at = Some(now);
    flight.record.configuration_revision = Some(snapshot.revision.clone());
    let path = match crate::provider_integration::operation_path(protocol) {
        Ok(value) => value,
        Err(error) => {
            return flight.reject(StatusCode::BAD_REQUEST, &error.code, "operation_validation");
        }
    };
    let mut original = Some(bytes);
    let mut facts = Some(body);
    let (response, attempt) = loop {
        let selection = &choice.selection;
        attempted.insert(selection.provider.clone());
        let provider = &snapshot.config.providers[&selection.provider];
        flight.record.provider = Some(selection.provider.clone());
        flight.record.qualified_model = Some(format!("{}/{}", selection.provider, selection.model));
        flight.record.authorization_generation = Some(choice.authorization_generation.clone());
        flight.record.decision = safe_evidence(&selection.evidence);
        attempts.apply(&mut flight.record);
        flight.record.upstream_status = None;
        flight.record.stage = "credential_acquisition".into();
        runtime.history.update(flight.record.clone());
        let Some(Auth::ApiKey { key_ref }) = &provider.auth else {
            return flight.reject(
                StatusCode::SERVICE_UNAVAILABLE,
                "credential_unavailable",
                "credential_acquisition",
            );
        };
        let key = tokio::select! {
            biased;
            _ = flight.cancel.changed() => return flight.reject(StatusCode::SERVICE_UNAVAILABLE,"force_stop","credential_acquisition"),
            result = credential(runtime.store.clone(), key_ref.clone()) => match result {
                Ok(value) => value,
                Err(error) => return flight.reject(StatusCode::SERVICE_UNAVAILABLE,&error.code,"credential_acquisition"),
            }
        };
        let key = match std::str::from_utf8(&key) {
            Ok(value) => value,
            Err(_) => {
                return flight.reject(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "credential_invalid",
                    "credential_acquisition",
                );
            }
        };
        let normalize = facts
            .as_ref()
            .expect("request facts before dispatch")
            .normalization_required;
        let input = if fallback.is_some() {
            original.as_ref().expect("retained request").clone()
        } else {
            original.take().expect("single request attempt")
        };
        let outgoing = match request_body::rewrite(
            input,
            &selection.model,
            protocol,
            choice.effort.as_deref(),
            normalize,
        ) {
            Ok(value) => value,
            Err(_) => {
                return flight.reject(StatusCode::BAD_REQUEST, "invalid_json", "model_validation");
            }
        };
        if fallback.is_none() {
            facts.take();
        }
        let go_session_header =
            if crate::provider_integration::integration_id(provider).ok() == Some("opencode-go") {
                choice.go_session_header
            } else {
                None
            };
        let upstream_url = match parts.uri.query() {
            Some(query) => format!(
                "{}{}?{}",
                choice.endpoint.trim_end_matches('/'),
                path,
                query
            ),
            None => format!("{}{}", choice.endpoint.trim_end_matches('/'), path),
        };
        let mut outbound = snapshot
            .client
            .post(&upstream_url)
            .headers(upstream_headers(
                &parts.headers,
                go_session_header,
                &upstream_url,
            ))
            .body(outgoing);
        if protocol == "messages"
            && !matches!(
                crate::provider_integration::integration_id(provider).ok(),
                Some("opencode-go" | "minimax-cn-coding-plan" | "minimax-coding-plan")
            )
        {
            outbound = outbound.header("x-api-key", key);
        } else {
            outbound = outbound.bearer_auth(key);
        }
        let outbound = match outbound.build() {
            Ok(request) => request,
            Err(_) => {
                return flight.reject(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "credential_invalid",
                    "credential_acquisition",
                );
            }
        };
        let interrupted = *flight.cancel.borrow();
        if interrupted {
            flight.finish("interrupted", "upstream_dispatch", Some("force_stop"));
            return error_response(StatusCode::SERVICE_UNAVAILABLE, "force_stop", &id);
        }
        flight.dispatch_attempts += 1;
        let attempt = AttemptContext {
            snapshot: snapshot.clone(),
            provider: selection.provider.clone(),
            identity: choice.provider_identity.clone(),
            authorization: choice.authorization_generation.clone(),
            reset_epoch: failure_epochs.epoch(&choice.provider_identity).to_owned(),
            quota_epoch: crate::observations::quota_epoch_for_binding(
                &lineage,
                &snapshot.config,
                &selection.provider,
                (&choice.provider_identity, &choice.authorization_generation),
            ),
        };
        attempts.push(
            json!({"provider":selection.provider,"qualified_model":flight.record.qualified_model,
            "upstream_status":null,"reason_code":null,"outcome":"in-progress"}),
        );
        attempts.apply(&mut flight.record);
        flight.record.execution = "unknown".into();
        flight.record.stage = "upstream_dispatch".into();
        runtime.history.update(flight.record.clone());
        let result = {
            let mut dispatch = PendingDispatch {
                flight: &mut flight,
                resolved: false,
            };
            let result = tokio::select! {
                biased;
                _ = dispatch.flight.cancel.changed() => {
                    return error_response(StatusCode::SERVICE_UNAVAILABLE,"force_stop",&id);
                },
                result = snapshot.client.execute(outbound) => result,
            };
            dispatch.resolved = true;
            result
        };
        let mut quota_state_failed = false;
        let result = match result.map_err(reqwest::Error::without_url) {
            Ok(response)
                if response.status() == StatusCode::TOO_MANY_REQUESTS
                    && crate::provider_integration::integration_id(provider).ok()
                        == Some("opencode-go") =>
            {
                let inspected = tokio::select! {
                    biased;
                    _ = flight.cancel.changed() => {
                        finish_pending_attempt(&mut flight.record, "interrupted", Some("force_stop"));
                        flight.finish("interrupted", "upstream_response", Some("force_stop"));
                        return error_response(StatusCode::SERVICE_UNAVAILABLE, "force_stop", &id);
                    }
                    inspected = inspect_quota_response(response, provider) => inspected,
                };
                let (response, failed) =
                    persist_inspected_quota(inspected, &attempt, &runtime, &mut flight.record);
                quota_state_failed = failed;
                Ok(response)
            }
            Ok(response) => Ok(UpstreamResponse::Original(response)),
            Err(error) => Err(error),
        };
        let (status, reason) = match &result {
            Ok(response) => (
                Some(response.status().as_u16()),
                response.is_denial().then_some(if quota_state_failed {
                    "quota_state_unavailable"
                } else {
                    "quota_suspended"
                }),
            ),
            Err(error) => (
                None,
                Some(if error.is_connect() {
                    "upstream_connection_failed"
                } else {
                    "upstream_transport_failed"
                }),
            ),
        };
        if status.is_none_or(|status| status >= 400) {
            attempt.record(&runtime, &mut flight.record, false, status, reason);
        }
        attempts.resolve(status, reason);
        attempts.apply(&mut flight.record);
        if !quota_state_failed
            && fallback
                .is_some_and(|conditions| crate::failures::matches(conditions, status, reason))
        {
            let mut evidence = Value::Null;
            match runtime.choose_untried(
                &snapshot,
                (&public_id, protocol),
                facts.as_ref().expect("fallback request facts"),
                &parts.headers,
                Utc::now(),
                &mut evidence,
                &attempted,
                &lineage,
            ) {
                Err(error) if error.code == "fallback_exhausted" => {}
                next => {
                    match result {
                        Ok(response) => record_fallback_error(response, &mut flight),
                        Err(error) => flight.record_error(
                            Utc::now(),
                            if error.is_connect() {
                                "upstream_connection"
                            } else {
                                "upstream_dispatch"
                            },
                            None,
                            None,
                            error.to_string(),
                            false,
                        ),
                    }
                    match next {
                        Ok(next) => {
                            choice = next;
                            continue;
                        }
                        Err(error) => {
                            flight.record.decision["fallback"] = safe_evidence(&evidence);
                            return flight.reject(StatusCode::CONFLICT, &error.code, "selection");
                        }
                    }
                }
            }
        }
        match result {
            Ok(response) => {
                break (
                    response,
                    if status.is_some_and(|s| s < 400) {
                        Some(attempt)
                    } else {
                        None
                    },
                );
            }
            Err(error) => {
                flight.record_error(
                    Utc::now(),
                    if error.is_connect() {
                        "upstream_connection"
                    } else {
                        "upstream_dispatch"
                    },
                    None,
                    None,
                    error.without_url().to_string(),
                    false,
                );
                let reason = reason.expect("transport reason");
                flight.finish(
                    "failed",
                    if reason == "upstream_connection_failed" {
                        "upstream_connection"
                    } else {
                        "upstream_dispatch"
                    },
                    Some(reason),
                );
                return error_response(StatusCode::BAD_GATEWAY, reason, &id);
            }
        }
    };
    drop(facts);
    drop(original);
    let status = response.status();
    let headers = response.headers().clone();
    let request_id = upstream_request_id(&headers);
    let safe_denial = response.is_denial();
    flight.record.upstream_status = Some(status.as_u16());
    flight.record.stage = "upstream_response".into();
    runtime.history.update(flight.record.clone());
    let sse = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"));
    let state = ForwardStream {
        upstream: response.into_stream(),
        pending: Bytes::new(),
        flight,
        observer: Observer::new(protocol, sse, status.is_success()),
        safe_denial,
        error_stage: if sse && status.is_success() {
            "upstream_stream"
        } else {
            "upstream_response"
        },
        done: false,
        attempt,
        runtime: runtime.clone(),
        request_id,
    };
    let stream = futures_util::stream::unfold(state, |mut state| async move {
        if state.done {
            return None;
        }
        let interrupted = *state.flight.cancel.borrow();
        if interrupted {
            state.finish_observation(false);
            finish_pending_attempt(&mut state.flight.record, "interrupted", Some("force_stop"));
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
            _ = state.flight.cancel.changed()=>{state.finish_observation(false);finish_pending_attempt(&mut state.flight.record,"interrupted",Some("force_stop"));state.flight.finish("interrupted","upstream_stream",Some("force_stop"));state.done=true;Some((Err(std::io::Error::other("request interrupted")),state))},
            result=state.upstream.next()=>match result {
                Some(Ok(bytes))=>{state.observer.observe(&bytes);state.pending=bytes;let n=state.pending.len().min(FORWARD_CHUNK_BYTES);let chunk=state.pending.split_to(n);Some((Ok(chunk),state))},
                Some(Err(error))=>{
                    state.finish_observation(false);
                    state.flight.record_error(Utc::now(),"upstream_stream",state.flight.record.upstream_status,state.request_id.clone(),error.without_url().to_string(),false);
                    state.complete_attempt(false,Some("upstream_transport_failed"));
                    state.flight.finish("failed","upstream_stream",Some("upstream_stream_failed"));state.done=true;
                    Some((Err(std::io::Error::other("upstream stream failed")),state))
                },
                None=>{
                    state.finish_observation(true);state.flight.record.downstream_complete=true;
                    let success=state.observer.success && state.observer.complete;
                    state.complete_attempt(success,None);
                    let outcome=if !state.observer.success{"failed"}else if state.observer.complete{"completed"}else{"unknown"};
                    let reason=if !state.observer.success{Some("upstream_error")}else if state.observer.complete{None}else{Some("completion_not_evidenced")};
                    let stage=if !state.observer.success{state.error_stage}else{"downstream_delivery"};
                    state.flight.finish(outcome,stage,reason);None
                }
            }
        }
    });
    let mut downstream = Response::new(Body::from_stream(stream));
    *downstream.status_mut() = status;
    for (name, value) in &headers {
        if !hop_header(name.as_str()) {
            downstream.headers_mut().append(name.clone(), value.clone());
        }
    }
    if let Ok(v) = id.parse() {
        downstream.headers_mut().insert("x-tmax-request-id", v);
    }
    downstream
}
type UpstreamStream =
    Pin<Box<dyn Stream<Item = std::result::Result<Bytes, reqwest::Error>> + Send>>;

enum UpstreamResponse {
    Original(reqwest::Response),
    Inspected {
        status: StatusCode,
        headers: HeaderMap,
        stream: UpstreamStream,
        denial: bool,
    },
}
impl UpstreamResponse {
    fn status(&self) -> StatusCode {
        match self {
            Self::Original(response) => response.status(),
            Self::Inspected { status, .. } => *status,
        }
    }
    fn headers(&self) -> &HeaderMap {
        match self {
            Self::Original(response) => response.headers(),
            Self::Inspected { headers, .. } => headers,
        }
    }
    fn is_denial(&self) -> bool {
        matches!(self, Self::Inspected { denial: true, .. })
    }
    fn into_stream(self) -> UpstreamStream {
        match self {
            Self::Original(response) => Box::pin(response.bytes_stream()),
            Self::Inspected { stream, .. } => stream,
        }
    }
}

// Keep the original chunks and any stream error for downstream; only a
// complete body within the inspection bound can be quota evidence.
async fn inspect_quota_response(
    response: reqwest::Response,
    provider: &crate::config::Provider,
) -> (
    UpstreamResponse,
    Option<crate::provider_integration::QuotaDenial>,
) {
    let status = response.status();
    let headers = response.headers().clone();
    let mut upstream = Box::pin(response.bytes_stream());
    let mut prefix = Vec::new();
    let mut inspected_bytes = 0usize;
    let deadline =
        tokio::time::Instant::now() + Duration::from_secs(QUOTA_ERROR_INSPECTION_SECONDS);
    let complete = loop {
        match tokio::time::timeout_at(deadline, upstream.next()).await {
            Ok(None) => break true,
            Ok(Some(Ok(bytes))) => {
                if bytes.is_empty() {
                    continue;
                }
                inspected_bytes = inspected_bytes.saturating_add(bytes.len());
                let fits = inspected_bytes <= INSPECTION_BYTES;
                prefix.push(Ok(bytes));
                if !fits {
                    break false;
                }
            }
            Ok(Some(Err(error))) => {
                prefix.push(Err(error));
                break false;
            }
            Err(_) => break false,
        }
    };
    let denial = if !complete {
        None
    } else if let [Ok(bytes)] = prefix.as_slice() {
        crate::provider_integration::quota_denial(provider, status.as_u16(), bytes)
    } else {
        let mut body = Vec::with_capacity(inspected_bytes);
        for bytes in prefix.iter().filter_map(|chunk| chunk.as_ref().ok()) {
            body.extend_from_slice(bytes);
        }
        crate::provider_integration::quota_denial(provider, status.as_u16(), &body)
    };
    let stream = Box::pin(futures_util::stream::iter(prefix).chain(upstream));
    (
        UpstreamResponse::Inspected {
            status,
            headers,
            stream,
            denial: denial.is_some(),
        },
        denial,
    )
}

// Persistence is evidence bookkeeping, not permission to replace an upstream response.
fn persist_inspected_quota(
    (response, denial): (
        UpstreamResponse,
        Option<crate::provider_integration::QuotaDenial>,
    ),
    attempt: &AttemptContext,
    runtime: &Runtime,
    record: &mut crate::diagnostics::Record,
) -> (UpstreamResponse, bool) {
    let failed = if let Some(denial) = &denial {
        match crate::observations::record_quota_denial(
            &runtime.store,
            &attempt.snapshot.config,
            &attempt.provider,
            (&attempt.identity, &attempt.authorization),
            &attempt.quota_epoch,
            denial,
        ) {
            Ok(()) => false,
            Err(error) => {
                record.decision["quota_state_error"] = json!(error.code);
                record.evidence_gap = true;
                true
            }
        }
    } else {
        false
    };
    (response, failed)
}

fn upstream_request_id(headers: &HeaderMap) -> Option<(String, bool)> {
    ["request-id", "x-request-id"].iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .map(|value| {
                let end = value
                    .len()
                    .min(crate::diagnostics::error_log::DETAIL_IDENTIFIER_BYTES);
                (value[..end].to_owned(), end < value.len())
            })
    })
}
fn record_fallback_error(response: UpstreamResponse, flight: &mut Flight) {
    let captured_at = Utc::now();
    let status = Some(response.status().as_u16());
    let request_id = upstream_request_id(response.headers());
    if response.is_denial() {
        flight.record_error(
            captured_at,
            "upstream_response",
            status,
            request_id,
            "Upstream confirmed quota exhaustion".into(),
            false,
        );
        return;
    }
    let mut stream = response.into_stream();
    let mut body = Vec::new();
    let mut truncated = true;
    loop {
        match stream.next().now_or_never() {
            Some(None) => {
                truncated = false;
                break;
            }
            Some(Some(Ok(bytes))) if !bytes.is_empty() => {
                let keep = bytes
                    .len()
                    .min(crate::diagnostics::error_log::DETAIL_BYTES - body.len());
                body.extend_from_slice(&bytes[..keep]);
                if keep < bytes.len() || body.len() == crate::diagnostics::error_log::DETAIL_BYTES {
                    break;
                }
            }
            _ => break,
        }
    }
    let error = if body.is_empty() {
        "Upstream HTTP failure; response body unavailable before fallback".into()
    } else {
        String::from_utf8_lossy(&body).into_owned()
    };
    flight.record_error(
        captured_at,
        "upstream_response",
        status,
        request_id,
        error,
        truncated,
    );
}
#[derive(Default)]
struct AttemptEvidence {
    entries: std::collections::VecDeque<Value>,
    bytes: usize,
    omitted: bool,
}
impl AttemptEvidence {
    fn push(&mut self, value: Value) {
        let size = value.to_string().len();
        const BUDGET: usize = crate::diagnostics::RECORD_BYTES / 4;
        if size > BUDGET {
            self.omitted = true;
            return;
        }
        while self.bytes + size > BUDGET || self.entries.len() >= 16 {
            if let Some(first) = self.entries.pop_front() {
                self.bytes -= first.to_string().len();
            }
            self.omitted = true;
        }
        self.bytes += size;
        self.entries.push_back(value);
    }
    fn resolve(&mut self, status: Option<u16>, reason: Option<&str>) {
        if self
            .entries
            .back()
            .is_some_and(|entry| entry["outcome"] == "in-progress")
        {
            let mut entry = self.entries.pop_back().expect("pending attempt");
            self.bytes -= entry.to_string().len();
            entry["upstream_status"] = json!(status);
            entry["reason_code"] = json!(reason);
            if status.is_none_or(|status| status >= 400) {
                entry["outcome"] = json!("failed");
            }
            self.push(entry);
        }
    }
    fn apply(&self, record: &mut crate::diagnostics::Record) {
        record.decision["attempts"] = json!(self.entries);
        record.evidence_gap |= self.omitted;
    }
}

fn finish_pending_attempt(
    record: &mut crate::diagnostics::Record,
    outcome: &str,
    reason: Option<&str>,
) {
    if let Some(attempts) = record
        .decision
        .get_mut("attempts")
        .and_then(Value::as_array_mut)
        && let Some(attempt) = attempts
            .last_mut()
            .filter(|attempt| attempt["outcome"] == "in-progress")
    {
        attempt["outcome"] = json!(outcome);
        attempt["reason_code"] = json!(reason);
        let mut bytes: usize = attempts.iter().map(|entry| entry.to_string().len()).sum();
        while bytes > crate::diagnostics::RECORD_BYTES / 4 {
            bytes -= attempts.remove(0).to_string().len();
            record.evidence_gap = true;
        }
    }
}

struct PendingDispatch<'a> {
    flight: &'a mut Flight,
    resolved: bool,
}
impl Drop for PendingDispatch<'_> {
    fn drop(&mut self) {
        if !self.resolved {
            let interrupted = *self.flight.cancel.borrow();
            let outcome = if interrupted {
                "interrupted"
            } else {
                "cancelled"
            };
            let reason = if interrupted {
                "force_stop"
            } else {
                "downstream_disconnected"
            };
            finish_pending_attempt(&mut self.flight.record, outcome, Some(reason));
            self.flight
                .finish(outcome, "upstream_dispatch", Some(reason));
        }
    }
}

struct AttemptContext {
    snapshot: Arc<super::Snapshot>,
    provider: String,
    identity: String,
    authorization: String,
    reset_epoch: String,
    quota_epoch: String,
}
impl AttemptContext {
    fn record(
        &self,
        runtime: &Runtime,
        record: &mut crate::diagnostics::Record,
        success: bool,
        status: Option<u16>,
        reason: Option<&str>,
    ) {
        if let Err(error) = crate::failures::record(
            &runtime.store,
            &self.snapshot.config,
            &self.provider,
            &self.identity,
            &self.authorization,
            &self.reset_epoch,
            success,
            status,
            reason,
            Utc::now(),
        ) {
            record.decision["failure_state_error"] = json!(error.code);
            record.evidence_gap = true;
        }
    }
}

struct ForwardStream {
    upstream: Pin<Box<dyn Stream<Item = std::result::Result<Bytes, reqwest::Error>> + Send>>,
    pending: Bytes,
    flight: Flight,
    observer: Observer,
    safe_denial: bool,
    error_stage: &'static str,
    done: bool,
    attempt: Option<AttemptContext>,
    runtime: Arc<Runtime>,
    request_id: Option<(String, bool)>,
}
impl ForwardStream {
    fn finish_observation(&mut self, reached_eof: bool) {
        if !reached_eof {
            self.observer.mark_incomplete();
        }
        self.observer.finish(&mut self.flight.record);
        if let Some((captured_at, error, truncated)) = self.observer.take_error() {
            self.flight.record_error(
                captured_at,
                self.error_stage,
                self.flight.record.upstream_status,
                self.request_id.clone(),
                if self.safe_denial {
                    "Upstream confirmed quota exhaustion".into()
                } else {
                    error
                },
                truncated,
            );
        }
    }
    fn complete_attempt(&mut self, success: bool, reason: Option<&str>) {
        if let Some(attempt) = self.attempt.take() {
            let status = if reason.is_none() {
                self.flight.record.upstream_status
            } else {
                None
            };
            if success || reason.is_some() || !self.observer.success {
                attempt.record(
                    &self.runtime,
                    &mut self.flight.record,
                    success,
                    status,
                    reason,
                );
            }
        }
        let outcome = if success {
            "completed"
        } else if reason.is_some() || !self.observer.success {
            "failed"
        } else {
            "unknown"
        };
        finish_pending_attempt(
            &mut self.flight.record,
            outcome,
            reason.or(if !self.observer.success {
                Some("upstream_error")
            } else if outcome == "unknown" {
                Some("completion_not_evidenced")
            } else {
                None
            }),
        );
    }
}
impl Drop for ForwardStream {
    fn drop(&mut self) {
        if !self.flight.finished {
            self.finish_observation(false);
            let interrupted = *self.flight.cancel.borrow();
            finish_pending_attempt(
                &mut self.flight.record,
                if interrupted {
                    "interrupted"
                } else {
                    "cancelled"
                },
                Some(if interrupted {
                    "force_stop"
                } else {
                    "downstream_disconnected"
                }),
            );
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

#[cfg(test)]
#[path = "fallback_tests.rs"]
mod fallback_tests;
