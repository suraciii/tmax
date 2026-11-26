//! Explicit fixed-destination diagnostic requests, independent of daemon startup.
use super::{response_observer::Observer, runtime::credential};
use crate::{
    Error, Request, Result,
    config::{Auth, Config},
    storage::Store,
};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Instant};

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
        "codex-subscription" | "opencode-go" | "kimi-code-plan-cn" | "kimi-code-plan-global"
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
    crate::provider_integration::check_request(provider, protocol)?;
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
    let response = match outbound.send().await {
        Ok(response) => response,
        Err(_) => {
            append_probe_row(
                store,
                config,
                provider_id,
                model_id,
                protocol,
                at,
                started,
                None,
                "failed",
                &BTreeMap::new(),
                "unknown",
            );
            return Err(Error::new(
                "probe_transport_failed",
                "Probe transport failed; upstream execution may be unknown, no retry performed",
            ));
        }
    };
    let status = response.status().as_u16();
    let mut stream = response.bytes_stream();
    let mut observer = Observer::new(protocol, false, (200..300).contains(&status));
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            append_probe_row(
                store,
                config,
                provider_id,
                model_id,
                protocol,
                at,
                started,
                Some(status),
                "failed",
                &observer.usage,
                observer_usage_state(&observer),
            );
            return Err(Error::new(
                "probe_stream_failed",
                "Probe response interrupted; execution and usage may be unknown, no replay performed",
            ));
        };
        observer.observe(&chunk);
    }
    observer.finish_json();
    let result = if observer.success && observer.complete {
        "completed"
    } else if !observer.success {
        "failed"
    } else {
        "unknown"
    };
    let usage_state = observer_usage_state(&observer);
    append_probe_row(
        store,
        config,
        provider_id,
        model_id,
        protocol,
        at,
        started,
        Some(status),
        result,
        &observer.usage,
        usage_state,
    );
    let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    Ok(
        json!({"qualified_model":id,"provider":provider_id,"protocol":protocol,"transport":transport,"observed_at":at,"source":"explicit_inference_probe","upstream_status":status,"result":result,"duration_ms":duration_ms,"usage":observer.usage,"usage_state":usage_state,"cost_risk":"May consume quota or incur unknown charges","account_identity":"unverified","quota":"unknown"}),
    )
}

fn observer_usage_state(observer: &Observer) -> &'static str {
    if observer.partial {
        "partial"
    } else if observer.usage.is_empty() {
        "unknown"
    } else {
        "reported"
    }
}

/// Append exactly one ledger row for a dispatched paid probe, whatever its
/// terminal outcome. A failed append is a coverage gap and never changes the
/// probe result, so it only increments the dropped marker.
#[allow(clippy::too_many_arguments)]
fn append_probe_row(
    store: &Store,
    config: &Config,
    provider_id: &str,
    model_id: &str,
    protocol: &str,
    at: DateTime<Utc>,
    started: Instant,
    upstream_status: Option<u16>,
    outcome: &str,
    usage: &BTreeMap<String, u64>,
    usage_state: &str,
) {
    let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let metadata = crate::catalog::read(store, "accepted").ok().flatten();
    let row = crate::usage::capture::capture_probe(
        &crate::usage::capture::ProbeFacts {
            observed_at: at,
            provider: provider_id,
            model: model_id,
            protocol,
            upstream_status,
            outcome,
            duration_ms: Some(duration_ms),
            usage,
            usage_state,
        },
        Some(crate::usage::capture::Pricing {
            config,
            metadata: metadata.as_ref(),
        }),
    );
    if crate::usage::ledger::append_direct(store, &[crate::usage::WriteJob::Usage(row)]).is_err() {
        let _ = crate::usage::ledger::record_dropped(store, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Model, Provider};
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn both_kimi_products_reject_unqualified_probes_before_dispatch() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_path_buf());
        let request = Request {
            command: vec!["gateway".into(), "probe".into()],
            operands: vec!["p/m".into()],
            arguments: BTreeMap::from([
                ("protocol".into(), vec!["messages".into()]),
                ("transport".into(), vec!["http".into()]),
            ]),
            dry_run: false,
            if_revision: None,
        };
        for integration in ["kimi-code-plan-cn", "kimi-code-plan-global"] {
            let mut config = Config::empty();
            config.providers.insert(
                "p".into(),
                Provider {
                    integration: Some(integration.into()),
                    models: BTreeMap::from([("m".into(), Model::default())]),
                    ..Default::default()
                },
            );
            assert_eq!(
                probe(&store, &config, &request).await.unwrap_err().code,
                "native_identity_unqualified"
            );
        }
    }
    #[tokio::test]
    async fn successful_paid_probe_appends_one_probe_origin_usage_row() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let upstream = axum::Router::new().fallback(|| async {
                axum::Json(json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"OK"}}],"usage":{"prompt_tokens":3,"completion_tokens":1}}))
            });
        let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_path_buf());
        let key_ref = crate::secrets::put(&store, "gateway", b"probe-secret").unwrap();
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            Provider {
                integration: Some("custom".into()),
                endpoints: BTreeMap::from([(
                    "chat-completions".into(),
                    format!("http://{address}"),
                )]),
                auth: Some(Auth::ApiKey { key_ref }),
                models: BTreeMap::from([("m".into(), Model::default())]),
                ..Default::default()
            },
        );
        let request = Request {
            command: vec!["gateway".into(), "probe".into()],
            operands: vec!["p/m".into()],
            arguments: BTreeMap::from([
                ("protocol".into(), vec!["chat-completions".into()]),
                ("transport".into(), vec!["http".into()]),
            ]),
            dry_run: false,
            if_revision: None,
        };
        let result = probe(&store, &config, &request).await.unwrap();
        assert_eq!(result["result"], "completed");
        assert_eq!(result["usage_state"], "reported");
        let rows = crate::gateway::test_support::usage_rows(&store);
        assert_eq!(rows.len(), 1, "one ledger row per explicit paid probe");
        let row = &rows[0];
        assert_eq!(row["origin"], "probe");
        assert_eq!(row["outcome"], result["result"]);
        assert_eq!(row["provider"], "p");
        assert_eq!(row["requested_model"], "m");
        assert_eq!(row["qualified_model"], "p/m");
        assert_eq!(row["protocol"], "chat-completions");
        assert_eq!(row["input_tokens"], 3);
        assert_eq!(row["output_tokens"], 1);
        assert_eq!(row["usage_state"], result["usage_state"]);
        assert_eq!(row["cost_state"], "unpriced");
        assert!(row["admitted_at_ms"].as_i64().is_some());
        assert!(!row.to_string().contains("probe-secret"));
        server.abort();
    }

    fn custom_config(store: &Store, endpoint: String) -> (Config, Request) {
        let key_ref = crate::secrets::put(store, "gateway", b"probe-secret").unwrap();
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            Provider {
                integration: Some("custom".into()),
                endpoints: BTreeMap::from([("chat-completions".into(), endpoint)]),
                auth: Some(Auth::ApiKey { key_ref }),
                models: BTreeMap::from([("m".into(), Model::default())]),
                ..Default::default()
            },
        );
        let request = Request {
            command: vec!["gateway".into(), "probe".into()],
            operands: vec!["p/m".into()],
            arguments: BTreeMap::from([
                ("protocol".into(), vec!["chat-completions".into()]),
                ("transport".into(), vec!["http".into()]),
            ]),
            dry_run: false,
            if_revision: None,
        };
        (config, request)
    }

    #[tokio::test]
    async fn transport_failure_still_appends_one_failed_probe_row() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_path_buf());
        let (config, request) = custom_config(&store, format!("http://{address}"));
        let error = probe(&store, &config, &request).await.unwrap_err();
        assert_eq!(error.code, "probe_transport_failed");
        let rows = crate::gateway::test_support::usage_rows(&store);
        assert_eq!(rows.len(), 1, "a dispatched probe always appends one row");
        let row = &rows[0];
        assert_eq!(row["origin"], "probe");
        assert_eq!(row["outcome"], "failed");
        assert_eq!(row["usage_state"], "unknown");
        assert_eq!(row["cost_state"], "none");
        assert!(row["provider"].as_str() == Some("p"));
    }

    #[tokio::test]
    async fn interrupted_probe_stream_appends_one_failed_probe_row() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let upstream = axum::Router::new().fallback(|| async {
            let chunks = futures_util::stream::unfold(0u8, |state| async move {
                match state {
                    0 => Some((
                        Ok::<_, std::io::Error>(axum::body::Bytes::from(
                            "data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n",
                        )),
                        1,
                    )),
                    1 => {
                        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                        Some((Err(std::io::Error::other("broken stream")), 2))
                    }
                    _ => None,
                }
            });
            (
                [(
                    axum::http::header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static("text/event-stream"),
                )],
                axum::body::Body::from_stream(chunks),
            )
        });
        let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_path_buf());
        let (config, request) = custom_config(&store, format!("http://{address}"));
        let error = probe(&store, &config, &request).await.unwrap_err();
        assert_eq!(error.code, "probe_stream_failed");
        let rows = crate::gateway::test_support::usage_rows(&store);
        assert_eq!(rows.len(), 1, "an interrupted probe still appends one row");
        let row = &rows[0];
        assert_eq!(row["origin"], "probe");
        assert_eq!(row["outcome"], "failed");
        assert_eq!(row["usage_state"], "unknown");
        server.abort();
    }
}
