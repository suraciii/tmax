//! Explicit fixed-destination diagnostic requests, independent of daemon startup.
use super::{response_observer::Observer, runtime::credential};
use crate::{
    Error, Request, Result,
    config::{Auth, Config},
    storage::Store,
};
use chrono::Utc;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::time::Instant;

/// Explicit paid, fixed-destination diagnostic request. It does not start a daemon.
pub async fn probe(store: &Store, config: &Config, request: &Request) -> Result<Value> {
    let id = request
        .operands
        .first()
        .ok_or_else(|| Error::new("missing_operand", "Qualified model required"))?;
    let arg = |name: &str| {
        request
            .arguments
            .get(name)
            .and_then(|v| v.first())
            .map(String::as_str)
    };
    let protocol =
        arg("protocol").ok_or_else(|| Error::new("missing_argument", "Protocol required"))?;
    let transport =
        arg("transport").ok_or_else(|| Error::new("missing_argument", "Transport required"))?;
    if transport != "http" {
        return Err(Error::new(
            "transport_unavailable",
            "Only the declared HTTP transport is available",
        ));
    }
    let (provider_id, model_id) = crate::config::qualified(id)?;
    let (provider, model) = crate::config::model(config, id)?;
    if !provider.enabled || !model.enabled {
        return Err(Error::new("model_unavailable", "Model is disabled"));
    }
    let integration = crate::provider_integration::integration_id(provider)?;
    if matches!(
        integration,
        "codex-subscription" | "opencode-go" | "kimi-coding-plan"
    ) {
        return Err(Error::new(
            "native_identity_unqualified",
            "This probe lacks the qualified real native client identity required by the selected integration",
        ));
    }
    let mut body = match protocol {
        "chat-completions" => {
            json!({"model":model_id,"messages":[{"role":"user","content":"Reply OK."}],"max_tokens":8})
        }
        "responses" => {
            json!({"model":model_id,"input":"Reply OK.","store":false,"max_output_tokens":8})
        }
        "messages" => {
            json!({"model":model_id,"messages":[{"role":"user","content":"Reply OK."}],"max_tokens":8})
        }
        _ => return Err(Error::new("protocol_unavailable", "Unknown protocol")),
    };
    let facts = super::RequestFacts::from_value(body.clone()).expect("generated request object");
    crate::provider_integration::check_request(provider, protocol, &facts)?;
    let endpoints = crate::provider_integration::resolve(provider)?;
    let endpoint = endpoints
        .get(protocol)
        .ok_or_else(|| Error::new("protocol_unavailable", "Protocol endpoint unavailable"))?;
    let path = crate::provider_integration::operation_path(protocol)?;
    if request.dry_run {
        return Ok(
            json!({"dry_run":true,"qualified_model":id,"provider":provider_id,"protocol":protocol,"transport":transport,"cost_risk":"May consume quota or incur unknown charges","request_shape":{"prompt":"fixed short diagnostic","output_token_limit":8},"inference_dispatched":false}),
        );
    }
    let Some(Auth::ApiKey { key_ref }) = &provider.auth else {
        return Err(Error::new(
            "credential_unavailable",
            "API-key authorization required",
        ));
    };
    let key = credential(store.clone(), key_ref.clone()).await?;
    let key = std::str::from_utf8(&key).map_err(|_| {
        Error::new(
            "credential_invalid",
            "Credential is not a supported header value",
        )
    })?;
    let client = crate::provider_integration::client(config)?;
    let started = Instant::now();
    let at = Utc::now();
    let mut outbound = client
        .post(format!("{}{}", endpoint.trim_end_matches('/'), path))
        .json(&body);
    if protocol == "messages" {
        outbound = outbound
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01");
    } else {
        outbound = outbound.bearer_auth(key);
    }
    body = Value::Null;
    drop(body);
    let response = outbound.send().await.map_err(|_| {
        Error::new(
            "probe_transport_failed",
            "Probe transport failed; upstream execution may be unknown, no retry performed",
        )
    })?;
    let status = response.status().as_u16();
    let mut stream = response.bytes_stream();
    let mut observer = Observer::new(protocol, false, (200..300).contains(&status));
    while let Some(chunk) = stream.next().await {
        let chunk=chunk.map_err(|_|Error::new("probe_stream_failed","Probe response interrupted; execution and usage may be unknown, no replay performed"))?;
        observer.observe(&chunk);
    }
    observer.finish_json();
    Ok(
        json!({"qualified_model":id,"provider":provider_id,"protocol":protocol,"transport":transport,"observed_at":at,"source":"explicit_inference_probe","upstream_status":status,"result":if observer.success&&observer.complete{"completed"}else if !observer.success{"failed"}else{"unknown"},"duration_ms":started.elapsed().as_millis(),"usage":observer.usage,"usage_state":if observer.partial{"partial"}else if observer.usage.is_empty(){"unknown"}else{"reported"},"cost_risk":"May consume quota or incur unknown charges","account_identity":"unverified","quota":"unknown"}),
    )
}
