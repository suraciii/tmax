use super::*;
use crate::{
    config::{FailureConditions, FailurePolicy},
    gateway::test_support::{fixture, publish},
};
use std::sync::atomic::{AtomicUsize, Ordering};

async fn upstream(
    status: StatusCode,
    body: &'static str,
    calls: Arc<AtomicUsize>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = Router::new().fallback(move |request: axum::extract::Request| {
        let calls = calls.clone();
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            let bytes = to_bytes(request.into_body(), 4096).await.unwrap();
            let request: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(request["model"], "m");
            assert_eq!(request["temperature"], 0.42);
            (status, [("content-type", "application/json")], body)
        }
    });
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), server)
}
fn conditions() -> FailureConditions {
    FailureConditions {
        http_statuses: vec![429],
        connection_errors: false,
        transport_errors: false,
    }
}
fn request(protocol: &str) -> axum::extract::Request {
    let path = match protocol {
        "responses" => "/v1/responses",
        "messages" => "/v1/messages",
        _ => "/v1/chat/completions",
    };
    axum::http::Request::builder().method("POST").uri(path)
        .header("authorization", "Bearer test-local-key")
        .header("content-type", "application/json")
        .header("user-agent", "deepseek-harness/0.2.1-rc.2 (+https://github.com/deepseek-ai/deepseek-harness)")
        .body(Body::from(r#"{"model":"dynamic","input":"hello","store":false,"temperature":0.42,"messages":[{"role":"user","content":"hello"}]}"#)).unwrap()
}

#[tokio::test]
async fn fallback_and_provider_override_apply_across_protocols_and_survive_restart() {
    for (protocol, success) in [
        (
            "chat-completions",
            r#"{"choices":[{"finish_reason":"stop","message":{"content":"LLM_OK"}}]}"#,
        ),
        (
            "responses",
            r#"{"status":"completed","output":[{"content":[{"type":"output_text","text":"LLM_OK"}]}]}"#,
        ),
        (
            "messages",
            r#"{"type":"message","stop_reason":"end_turn","content":[{"type":"text","text":"LLM_OK"}]}"#,
        ),
    ] {
        let p_calls = Arc::new(AtomicUsize::new(0));
        let q_calls = Arc::new(AtomicUsize::new(0));
        let (p_url, p_server) = upstream(
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"quota rejected"}}"#,
            p_calls.clone(),
        )
        .await;
        let (q_url, q_server) = upstream(StatusCode::OK, success, q_calls.clone()).await;
        let (_temporary, runtime, old) = fixture();
        let mut config = old.clone();
        for (provider, url) in [("p", p_url), ("q", q_url)] {
            config.providers.get_mut(provider).unwrap().endpoints =
                [(protocol.to_owned(), url)].into();
        }
        config.providers.get_mut("p").unwrap().failure_policy = Some(FailurePolicy {
            enabled: true,
            errors: conditions(),
            consecutive_failures: 2,
            priority: -5,
            duration_seconds: 18_000,
        });
        let alias = &mut config.model_aliases[0];
        alias.protocol = protocol.into();
        alias.continuity = Some("stateless".into());
        alias.fallback = Some(conditions());
        publish(&runtime, &old, &config, "fallback");
        runtime.open();
        for _ in 0..3 {
            let response = forward(State(runtime.clone()), request(protocol)).await;
            assert_eq!(response.status(), StatusCode::OK);
            assert!(
                std::str::from_utf8(&to_bytes(response.into_body(), 4096).await.unwrap())
                    .unwrap()
                    .contains("LLM_OK")
            );
        }
        assert_eq!(p_calls.load(Ordering::SeqCst), 2);
        assert_eq!(q_calls.load(Ordering::SeqCst), 3);
        let state = crate::failures::inspect(&runtime.store, &config, "p", Utc::now()).unwrap();
        assert_eq!(state["active_priority"], -5);
        let explained =
            crate::routing::explain(&runtime.store, &config, "dynamic", protocol, None).unwrap();
        assert_eq!(explained["selection"]["provider"], "q");
        let restarted = Runtime::new(runtime.store.clone(), "restarted".into()).unwrap();
        publish(&restarted, &config, &config, "restart");
        restarted.open();
        let response = forward(State(restarted.clone()), request(protocol)).await;
        to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(p_calls.load(Ordering::SeqCst), 2);
        assert_eq!(q_calls.load(Ordering::SeqCst), 4);
        p_server.abort();
        q_server.abort();
    }
}

#[tokio::test]
async fn fallback_exhaustion_preserves_last_rejection_without_looping() {
    let p_calls = Arc::new(AtomicUsize::new(0));
    let q_calls = Arc::new(AtomicUsize::new(0));
    let (p_url, p_server) = upstream(
        StatusCode::TOO_MANY_REQUESTS,
        r#"{"error":"CPA_LIMIT"}"#,
        p_calls.clone(),
    )
    .await;
    let (q_url, q_server) = upstream(
        StatusCode::TOO_MANY_REQUESTS,
        r#"{"error":"LLM_LIMIT"}"#,
        q_calls.clone(),
    )
    .await;
    let (_temporary, runtime, old) = fixture();
    let mut config = old.clone();
    config
        .providers
        .get_mut("p")
        .unwrap()
        .endpoints
        .insert("chat-completions".into(), p_url);
    config
        .providers
        .get_mut("q")
        .unwrap()
        .endpoints
        .insert("chat-completions".into(), q_url);
    config.model_aliases[0].continuity = Some("stateless".into());
    config.model_aliases[0].fallback = Some(conditions());
    publish(&runtime, &old, &config, "exhaustion");
    runtime.open();
    let response = forward(State(runtime.clone()), request("chat-completions")).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        to_bytes(response.into_body(), 4096).await.unwrap(),
        r#"{"error":"LLM_LIMIT"}"#
    );
    assert_eq!(p_calls.load(Ordering::SeqCst), 1);
    assert_eq!(q_calls.load(Ordering::SeqCst), 1);
    p_server.abort();
    q_server.abort();
}

#[tokio::test]
async fn unconfigured_error_does_not_fallback() {
    let p_calls = Arc::new(AtomicUsize::new(0));
    let q_calls = Arc::new(AtomicUsize::new(0));
    let (p_url, p_server) = upstream(
        StatusCode::BAD_REQUEST,
        r#"{"error":"INVALID_REQUEST"}"#,
        p_calls.clone(),
    )
    .await;
    let (q_url, q_server) = upstream(
        StatusCode::OK,
        r#"{"choices":[{"finish_reason":"stop"}]}"#,
        q_calls.clone(),
    )
    .await;
    let (_temporary, runtime, old) = fixture();
    let mut config = old.clone();
    config
        .providers
        .get_mut("p")
        .unwrap()
        .endpoints
        .insert("chat-completions".into(), p_url);
    config
        .providers
        .get_mut("q")
        .unwrap()
        .endpoints
        .insert("chat-completions".into(), q_url);
    config.model_aliases[0].continuity = Some("stateless".into());
    config.model_aliases[0].fallback = Some(conditions());
    publish(&runtime, &old, &config, "nonmatching");
    runtime.open();
    let response = forward(State(runtime.clone()), request("chat-completions")).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    to_bytes(response.into_body(), 4096).await.unwrap();
    assert_eq!(p_calls.load(Ordering::SeqCst), 1);
    assert_eq!(q_calls.load(Ordering::SeqCst), 0);
    p_server.abort();
    q_server.abort();
}

#[tokio::test]
async fn successful_stream_headers_commit_destination_even_when_stream_reports_error() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app=Router::new().fallback(||async {
        ([("content-type","text/event-stream")],"data: {\"choices\":[{\"delta\":{\"content\":\"PARTIAL\"}}]}\n\ndata: {\"error\":{\"code\":429}}\n\n")
    });
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let q_calls = Arc::new(AtomicUsize::new(0));
    let (q_url, q_server) = upstream(
        StatusCode::OK,
        r#"{"choices":[{"finish_reason":"stop"}]}"#,
        q_calls.clone(),
    )
    .await;
    let (_temporary, runtime, old) = fixture();
    let mut config = old.clone();
    config
        .providers
        .get_mut("p")
        .unwrap()
        .endpoints
        .insert("chat-completions".into(), format!("http://{address}"));
    config
        .providers
        .get_mut("q")
        .unwrap()
        .endpoints
        .insert("chat-completions".into(), q_url);
    config.model_aliases[0].continuity = Some("stateless".into());
    config.model_aliases[0].fallback = Some(FailureConditions {
        http_statuses: vec![429, 503],
        connection_errors: true,
        transport_errors: true,
    });
    publish(&runtime, &old, &config, "partial-stream");
    runtime.open();
    let response = forward(State(runtime.clone()), request("chat-completions")).await;
    let id = response.headers()["x-tmax-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(response.status(), StatusCode::OK);
    let output = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(std::str::from_utf8(&output).unwrap().contains("PARTIAL"));
    assert_eq!(q_calls.load(Ordering::SeqCst), 0);
    let inspect = crate::Request {
        command: vec!["request".into(), "inspect".into()],
        operands: vec![id],
        arguments: Default::default(),
        dry_run: false,
        if_revision: None,
    };
    let record = runtime.history.view(&inspect).unwrap();
    assert_eq!(record["record"]["outcome"], "failed");
    assert_eq!(record["record"]["decision"]["attempts"][0]["provider"], "p");
    server.abort();
    q_server.abort();
}

fn latest_record(runtime: &Runtime) -> Value {
    let mut query = crate::Request {
        command: vec!["history".into(), "list".into()],
        operands: Vec::new(),
        arguments: Default::default(),
        dry_run: false,
        if_revision: None,
    };
    let listed = runtime.history.view(&query).unwrap();
    query.command[1] = "inspect".into();
    query.operands = vec![listed["records"][0]["request"].as_str().unwrap().into()];
    runtime.history.view(&query).unwrap()["record"].clone()
}

fn track_all_failures(config: &mut Config) {
    for provider in config.providers.values_mut() {
        provider.failure_policy = Some(FailurePolicy {
            enabled: true,
            errors: FailureConditions {
                http_statuses: vec![429],
                connection_errors: true,
                transport_errors: true,
            },
            consecutive_failures: 2,
            priority: -5,
            duration_seconds: 18_000,
        });
    }
    config.model_aliases[0].continuity = Some("stateless".into());
    config.model_aliases[0].fallback = Some(FailureConditions {
        http_statuses: vec![429],
        connection_errors: true,
        transport_errors: true,
    });
}

#[tokio::test]
async fn invalid_credential_header_is_local_without_attempt_or_fallback() {
    for protocol in ["chat-completions", "responses", "messages"] {
        let calls = Arc::new(AtomicUsize::new(0));
        let (url, server) = upstream(StatusCode::OK, "{}", calls.clone()).await;
        let (_temporary, runtime, old) = fixture();
        let mut config = old.clone();
        track_all_failures(&mut config);
        for provider in config.providers.values_mut() {
            provider.endpoints = [(protocol.to_owned(), url.clone())].into();
        }
        config.model_aliases[0].protocol = protocol.into();
        let identity = crate::inputs::provider_identity(&runtime.store, &old, "p")
            .unwrap()
            .unwrap();
        let reference = crate::secrets::put(
            &runtime.store,
            &format!("provider:{identity}:p"),
            b"invalid\0header",
        )
        .unwrap();
        config.providers.get_mut("p").unwrap().auth = Some(Auth::ApiKey { key_ref: reference });
        publish(&runtime, &old, &config, "invalid-header");
        runtime.open();
        let response = forward(State(runtime.clone()), request(protocol)).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let record = latest_record(&runtime);
        assert_eq!(record["reason_code"], "credential_invalid");
        assert_eq!(record["stage"], "credential_acquisition");
        assert_eq!(record["execution"], "not_dispatched");
        assert_eq!(record["outcome"], "rejected");
        assert_eq!(record["decision"]["attempts"], json!([]));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        for provider in ["p", "q"] {
            assert_eq!(
                crate::failures::inspect(&runtime.store, &config, provider, Utc::now()).unwrap()["consecutive_failures"],
                0
            );
        }
        server.abort();
    }
}

#[tokio::test]
async fn cancellation_during_send_preserves_attempt_chain_without_counting_unknown_failure() {
    for force_stop in [false, true] {
        let started = Arc::new(tokio::sync::Notify::new());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().fallback({
            let started = started.clone();
            move || {
                let started = started.clone();
                async move {
                    started.notify_one();
                    std::future::pending::<StatusCode>().await
                }
            }
        });
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let p_calls = Arc::new(AtomicUsize::new(0));
        let (p_url, p_server) =
            upstream(StatusCode::TOO_MANY_REQUESTS, "{}", p_calls.clone()).await;
        let (_temporary, runtime, old) = fixture();
        let mut config = old.clone();
        track_all_failures(&mut config);
        config
            .providers
            .get_mut("p")
            .unwrap()
            .endpoints
            .insert("chat-completions".into(), p_url);
        config
            .providers
            .get_mut("q")
            .unwrap()
            .endpoints
            .insert("chat-completions".into(), format!("http://{address}"));
        publish(&runtime, &old, &config, "cancel-send");
        runtime.open();
        let task = tokio::spawn(forward(State(runtime.clone()), request("chat-completions")));
        tokio::time::timeout(Duration::from_secs(5), started.notified())
            .await
            .unwrap();
        let active = latest_record(&runtime);
        assert_eq!(active["decision"]["attempts"][0]["outcome"], "failed");
        assert_eq!(active["decision"]["attempts"][1]["provider"], "q");
        assert_eq!(active["decision"]["attempts"][1]["outcome"], "in-progress");
        if force_stop {
            assert_eq!(runtime.stop(true).unwrap(), 1);
            let response = tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        } else {
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        }
        let record = latest_record(&runtime);
        let outcome = if force_stop {
            "interrupted"
        } else {
            "cancelled"
        };
        let reason = if force_stop {
            "force_stop"
        } else {
            "downstream_disconnected"
        };
        assert_eq!(record["provider"], "q");
        assert_eq!(record["outcome"], outcome);
        assert_eq!(record["reason_code"], reason);
        assert_eq!(record["decision"]["attempts"].as_array().unwrap().len(), 2);
        assert_eq!(record["decision"]["attempts"][0]["outcome"], "failed");
        assert_eq!(record["decision"]["attempts"][1]["outcome"], outcome);
        assert_eq!(record["decision"]["attempts"][1]["reason_code"], reason);
        assert_eq!(
            record["decision"]["attempts"][1]["upstream_status"],
            Value::Null
        );
        assert_eq!(runtime.active(), 0);
        assert_eq!(p_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            crate::failures::inspect(&runtime.store, &config, "p", Utc::now()).unwrap()["consecutive_failures"],
            1
        );
        assert_eq!(
            crate::failures::inspect(&runtime.store, &config, "q", Utc::now()).unwrap()["consecutive_failures"],
            0
        );
        p_server.abort();
        server.abort();
    }
}

#[tokio::test]
async fn cancellation_after_success_headers_finalizes_attempt_without_provider_failure() {
    for (force_stop, consume_chunk) in [(false, false), (false, true), (true, false), (true, true)]
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().fallback(|| async {
            let stream = futures_util::stream::once(async {
                Ok::<Bytes, std::io::Error>(Bytes::from_static(
                    b"data: {\"choices\":[{\"delta\":{\"content\":\"PARTIAL\"}}]}\n\n",
                ))
            })
            .chain(futures_util::stream::pending());
            (
                [("content-type", "text/event-stream")],
                Body::from_stream(stream),
            )
        });
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (_temporary, runtime, old) = fixture();
        let mut config = old.clone();
        track_all_failures(&mut config);
        config
            .providers
            .get_mut("p")
            .unwrap()
            .endpoints
            .insert("chat-completions".into(), format!("http://{address}"));
        publish(&runtime, &old, &config, "cancel-stream");
        runtime.open();
        let response = forward(State(runtime.clone()), request("chat-completions")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let mut downstream = response.into_body().into_data_stream();
        if consume_chunk {
            let chunk = tokio::time::timeout(Duration::from_secs(5), downstream.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert!(std::str::from_utf8(&chunk).unwrap().contains("PARTIAL"));
        }
        if force_stop {
            assert_eq!(runtime.stop(true).unwrap(), 1);
            assert!(
                tokio::time::timeout(Duration::from_secs(5), downstream.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .is_err()
            );
        } else {
            drop(downstream);
        }
        let record = latest_record(&runtime);
        let outcome = if force_stop {
            "interrupted"
        } else {
            "cancelled"
        };
        let reason = if force_stop {
            "force_stop"
        } else {
            "downstream_disconnected"
        };
        assert_eq!(record["outcome"], outcome);
        assert_eq!(record["decision"]["attempts"].as_array().unwrap().len(), 1);
        assert_eq!(record["decision"]["attempts"][0]["provider"], "p");
        assert_eq!(record["decision"]["attempts"][0]["upstream_status"], 200);
        assert_eq!(record["decision"]["attempts"][0]["outcome"], outcome);
        assert_eq!(record["decision"]["attempts"][0]["reason_code"], reason);
        assert_eq!(runtime.active(), 0);
        assert_eq!(
            crate::failures::inspect(&runtime.store, &config, "p", Utc::now()).unwrap()["consecutive_failures"],
            0
        );
        server.abort();
    }
}

#[test]
fn attempt_evidence_remains_bounded_when_resolved_and_cancelled() {
    let mut attempts = AttemptEvidence::default();
    for index in 0..64 {
        attempts.push(json!({"provider":format!("provider-{index}"),
            "qualified_model":format!("provider-{index}/{}", "m".repeat(128)),
            "upstream_status":null,"reason_code":null,"outcome":"in-progress"}));
        if index < 63 {
            attempts.resolve(Some(429), None);
        }
    }
    assert!(attempts.omitted);
    assert!(attempts.entries.len() <= 16);
    assert!(attempts.bytes <= crate::diagnostics::RECORD_BYTES / 4);
    let history = crate::diagnostics::History::new("attempt-budget".into());
    let mut record = history.begin();
    attempts.apply(&mut record);
    finish_pending_attempt(&mut record, "cancelled", Some("downstream_disconnected"));
    assert!(record.evidence_gap);
    assert!(
        record.decision["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|attempt| attempt.to_string().len())
            .sum::<usize>()
            <= crate::diagnostics::RECORD_BYTES / 4
    );
    assert_eq!(
        record.decision["attempts"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()["outcome"],
        "cancelled"
    );
    assert!(
        !record.decision["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|attempt| attempt["outcome"] == "in-progress")
    );
}

#[tokio::test]
async fn unknown_stream_completion_is_terminal_without_counting_provider_failure() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (url, server) = upstream(StatusCode::OK, "{}", calls.clone()).await;
    let (_temporary, runtime, old) = fixture();
    let mut config = old.clone();
    track_all_failures(&mut config);
    config
        .providers
        .get_mut("p")
        .unwrap()
        .endpoints
        .insert("chat-completions".into(), url);
    publish(&runtime, &old, &config, "unknown-completion");
    runtime.open();
    let response = forward(State(runtime.clone()), request("chat-completions")).await;
    to_bytes(response.into_body(), 4096).await.unwrap();
    let record = latest_record(&runtime);
    assert_eq!(record["outcome"], "unknown");
    assert_eq!(record["decision"]["attempts"][0]["outcome"], "unknown");
    assert_eq!(
        record["decision"]["attempts"][0]["reason_code"],
        "completion_not_evidenced"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        crate::failures::inspect(&runtime.store, &config, "p", Utc::now()).unwrap()["consecutive_failures"],
        0
    );
    server.abort();
}
