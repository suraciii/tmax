use super::*;
use crate::{
    Request,
    gateway::test_support::{fixture, publish},
};
use std::collections::BTreeMap;

fn error_log(store: &crate::storage::Store) -> Vec<Value> {
    std::fs::read_to_string(store.root.join("error.log"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn key_comparison_requires_exact_bytes() {
    assert!(matches_key(b"abc", b"abc"));
    assert!(!matches_key(b"abcx", b"abc"));
    assert!(!matches_key(b"ab", b"abc"));
}

#[tokio::test]
async fn forwarding_preserves_client_content_type_values_without_inventing_one() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (sent, mut received) = tokio::sync::mpsc::channel(1);
    let upstream = Router::new().fallback(move |request: axum::extract::Request| {
        let sent = sent.clone();
        async move {
            let content_types: Vec<_> = request
                .headers()
                .get_all("content-type")
                .iter()
                .map(|value| value.to_str().unwrap().to_owned())
                .collect();
            sent.send(content_types).await.unwrap();
            StatusCode::OK
        }
    });
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let (_temporary, runtime, config) = fixture();
    let mut updated = config.clone();
    for protocol in ["responses", "chat-completions", "messages"] {
        updated
            .providers
            .get_mut("p")
            .unwrap()
            .endpoints
            .insert(protocol.into(), format!("http://{address}"));
    }
    publish(&runtime, &config, &updated, "content-type");
    runtime.open();
    for path in ["/v1/responses", "/v1/chat/completions", "/v1/messages"] {
        for client_types in [
            vec![],
            vec!["application/json"],
            vec!["application/json; charset=utf-8"],
            vec!["application/vnd.example+json"],
            vec!["application/json", "application/vnd.example+json"],
        ] {
            let mut request = axum::http::Request::builder()
                .method(Method::POST)
                .uri(path)
                .header("authorization", "Bearer test-local-key")
                .header("user-agent", "pi (linux test; x64)");
            for &content_type in &client_types {
                request = request.header("content-type", content_type);
            }
            let request = request.body(Body::from(r#"{"model":"p/m","input":"x","store":false,"messages":[{"role":"user","content":"x"}]}"#)).unwrap();
            let response = forward(State(runtime.clone()), request).await;
            let actual = received.recv().await.unwrap();
            assert_eq!(actual, client_types, "path={path}");
            assert_eq!(response.status(), StatusCode::OK);
            to_bytes(response.into_body(), 4096).await.unwrap();
        }
    }
    server.abort();
}

#[tokio::test]
async fn messages_beta_selector_reaches_upstream_and_other_queries_never_dispatch() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (sent, mut received) = tokio::sync::mpsc::channel(4);
    let upstream = Router::new().fallback(move |request: axum::extract::Request| {
        let sent = sent.clone();
        async move {
            let (parts, body) = request.into_parts();
            let body = to_bytes(body, 4096).await.unwrap();
            sent.send((parts.uri.to_string(), parts.headers, serde_json::from_slice::<Value>(&body).unwrap())).await.unwrap();
            axum::Json(json!({"type":"message","stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}))
        }
    });
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let (_temporary, runtime, config) = fixture();
    let mut messages = config.clone();
    let provider = messages.providers.get_mut("p").unwrap();
    provider.endpoints.clear();
    provider
        .endpoints
        .insert("messages".into(), format!("http://{address}"));
    publish(&runtime, &config, &messages, "messages-query");
    runtime.open();
    for path in ["/v1/messages", "/v1/messages?beta=true"] {
        let body = json!({"model":"p/m","messages":[{"role":"user","content":"x"}],"max_tokens":1,"stream":false,"extension":{"enabled":false}});
        let request = axum::http::Request::builder()
            .method(Method::POST)
            .uri(path)
            .header("authorization", "Bearer test-local-key")
            .header("user-agent", "pi (linux test; x64)")
            .header("anthropic-beta", "test-feature")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let response = forward(State(runtime.clone()), request).await;
        assert_eq!(response.status(), StatusCode::OK);
        to_bytes(response.into_body(), 4096).await.unwrap();
        let (actual_path, headers, actual_body) = received.recv().await.unwrap();
        assert_eq!(actual_path, path);
        assert_eq!(headers["user-agent"], "pi (linux test; x64)");
        assert_eq!(headers["anthropic-beta"], "test-feature");
        assert_eq!(headers["x-api-key"], "test-provider-key");
        assert!(!headers.contains_key("authorization"));
        let mut expected = body;
        expected["model"] = json!("m");
        assert_eq!(actual_body, expected);
    }
    let invalid = [
        "/v1/messages?unknown=true",
        "/v1/messages?beta=false",
        "/v1/messages?beta=true&beta=true",
        "/v1/messages?beta=true&beta=false",
        "/v1/messages?beta=true&unknown=true",
        "/v1/messages/count_tokens?beta=true",
        "/v1/responses?beta=true",
        "/v1/chat/completions?beta=true",
    ];
    for path in invalid {
        let request = axum::http::Request::builder()
            .method(Method::POST)
            .uri(path)
            .header("authorization", "Bearer test-local-key")
            .body(Body::from(
                r#"{"model":"p/m","messages":[{"role":"user","content":"x"}]}"#,
            ))
            .unwrap();
        assert_eq!(
            forward(State(runtime.clone()), request).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert!(received.try_recv().is_err());
    let mut query = Request {
        command: vec!["history".into(), "list".into()],
        operands: Vec::new(),
        arguments: BTreeMap::from([("outcome".into(), vec!["rejected".into()])]),
        dry_run: false,
        if_revision: None,
    };
    let history = runtime.history.view(&query).unwrap();
    let rejected = history["records"].as_array().unwrap();
    assert_eq!(rejected.len(), invalid.len());
    query.command[1] = "inspect".into();
    for record in rejected {
        query.operands = vec![record["request"].as_str().unwrap().into()];
        let inspected = runtime.history.view(&query).unwrap();
        assert_eq!(inspected["record"]["reason_code"], "undeclared_operation");
        assert_eq!(inspected["record"]["stage"], "operation_validation");
        assert_eq!(inspected["record"]["execution"], "not_dispatched");
    }
    server.abort();
}

#[tokio::test]
async fn large_responses_body_arrives_intact_and_over_limit_body_never_dispatches() {
    use sha2::{Digest, Sha256};
    use std::convert::Infallible;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (sent, mut received) = tokio::sync::mpsc::channel(2);
    let upstream = Router::new().fallback(move |request: axum::extract::Request| {
        let sent = sent.clone();
        async move {
            let bytes = to_bytes(request.into_body(), 16 * 1024 * 1024).await.unwrap();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            let input = body["input"].as_str().unwrap();
            sent.send((body["model"].clone(), Sha256::digest(input.as_bytes()))).await.unwrap();
            axum::Json(json!({"id":"r1","object":"response","status":"completed","output":[],"usage":{"input_tokens":1,"output_tokens":1}}))
        }
    });
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let (_temporary, runtime, config) = fixture();
    let mut responses = config.clone();
    let provider = responses.providers.get_mut("p").unwrap();
    provider.endpoints.clear();
    provider
        .endpoints
        .insert("responses".into(), format!("http://{address}/v1"));
    publish(&runtime, &config, &responses, "large-responses");
    runtime.open();

    let mut input = "x".repeat(8 * 1024 * 1024);
    input.push_str("end-of-large-input");
    let expected = Sha256::digest(input.as_bytes());
    let body = json!({"model":"p/m","input":input,"store":false,"stream":false});
    let request = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/v1/responses")
        .header("authorization", "Bearer test-local-key")
        .header("user-agent", "pi (linux test; x64)")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let response = forward(State(runtime.clone()), request).await;
    assert_eq!(response.status(), StatusCode::OK);
    to_bytes(response.into_body(), 4096).await.unwrap();
    let (model, actual) = received.recv().await.unwrap();
    assert_eq!(model, "m");
    assert_eq!(actual, expected);

    // Shared chunks exercise the byte bound without allocating a limit-sized fixture.
    let chunk = Bytes::from(vec![b' '; 64 * 1024]);
    let chunks = REQUEST_BYTES / chunk.len() + 1;
    let stream =
        futures_util::stream::iter((0..chunks).map(move |_| Ok::<_, Infallible>(chunk.clone())));
    let request = axum::http::Request::builder()
        .method(Method::POST)
        .uri("/v1/responses")
        .header("authorization", "Bearer test-local-key")
        .body(Body::from_stream(stream))
        .unwrap();
    let response = forward(State(runtime.clone()), request).await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let error: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(error["error"]["type"], "request_too_large");
    assert!(received.try_recv().is_err());
    let query = Request {
        command: vec!["history".into(), "inspect".into()],
        operands: vec![error["error"]["request_id"].as_str().unwrap().into()],
        arguments: BTreeMap::new(),
        dry_run: false,
        if_revision: None,
    };
    let history = runtime.history.view(&query).unwrap();
    assert_eq!(history["record"]["stage"], "local_admission");
    assert_eq!(history["record"]["execution"], "not_dispatched");
    assert_eq!(runtime.active(), 0);
    server.abort();
}

#[tokio::test]
async fn malformed_raw_extensions_never_dispatch_and_duplicate_models_use_last_value() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (sent, mut received) = tokio::sync::mpsc::channel(2);
    let upstream = Router::new().fallback(move |request: axum::extract::Request| {
        let sent = sent.clone();
        async move {
            let (parts, body) = request.into_parts();
            let bytes = to_bytes(body, 4096).await.unwrap();
            sent.send((parts.headers, bytes)).await.unwrap();
            axum::Json(json!({"id":"r1","status":"completed","output":[]}))
        }
    });
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let (_temporary, runtime, original) = fixture();
    let mut config = original.clone();
    config
        .providers
        .get_mut("p")
        .unwrap()
        .endpoints
        .insert("responses".into(), format!("http://{address}/v1"));
    publish(&runtime, &original, &config, "raw-body-admission");
    runtime.open();
    let make_request = |bytes: Vec<u8>| {
        axum::http::Request::builder()
            .method(Method::POST)
            .uri("/v1/responses")
            .header("authorization", "Bearer test-local-key")
            .header("user-agent", "pi (linux test; x64)")
            .header("content-type", "application/json")
            .body(Body::from(bytes))
            .unwrap()
    };
    for malformed in [
        br#"{"model":"p/m","input":"x","store":false,"extension":"\uD800"}"#.as_slice(),
        br#"{"model":"p/m","input":"x","store":false,"extension":{"x":[1,]}}"#.as_slice(),
        br#"{"model":"p/m","input":"x","store":false}{}"#.as_slice(),
        b"{\"model\":\"p/m\",\"input\":\"x\",\"extension\":\"\xff\"}".as_slice(),
    ] {
        let response = forward(State(runtime.clone()), make_request(malformed.to_vec())).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let error: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(error["error"]["type"], "invalid_json");
        assert!(received.try_recv().is_err());
        assert_eq!(runtime.active(), 0);
    }
    let raw = br#"{"model":"missing/model","mo\u0064el":"p/m","input":"x","store":false,"extension":{"off":true,"off":false,"zero":0,"huge":1e400}}"#;
    let response = forward(State(runtime.clone()), make_request(raw.to_vec())).await;
    assert_eq!(response.status(), StatusCode::OK);
    to_bytes(response.into_body(), 4096).await.unwrap();
    let (headers, bytes) = received.recv().await.unwrap();
    assert_eq!(headers["content-type"], "application/json");
    let mut expected: Value = serde_json::from_slice(raw).unwrap();
    expected["model"] = json!("m");
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), expected);
    // The upstream must not have to choose its own duplicate-key policy.
    let wire = std::str::from_utf8(&bytes).unwrap();
    assert_eq!(wire.matches("\"model\"").count(), 1);
    assert_eq!(wire.matches("\"off\"").count(), 1);
    server.abort();
}

#[tokio::test]
async fn fallback_keeps_admitted_lineage_endpoint_and_body_across_reload() {
    let first = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let first_address = first.local_addr().unwrap();
    let release = Arc::new(tokio::sync::Notify::new());
    let (entered, mut entering) = tokio::sync::mpsc::channel(1);
    let first_app = Router::new().fallback({
        let release = release.clone();
        move || {
            let release = release.clone();
            let entered = entered.clone();
            async move {
                entered.send(()).await.unwrap();
                release.notified().await;
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    [("x-request-id", "first-provider-request")],
                    axum::Json(json!({"error":"FIRST_LIMIT"})),
                )
            }
        }
    });
    let first_server = tokio::spawn(async move { axum::serve(first, first_app).await.unwrap() });
    let second = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let second_address = second.local_addr().unwrap();
    let (captured, mut requests) = tokio::sync::mpsc::channel(1);
    let second_app = Router::new().fallback(move |request: axum::extract::Request| {
        let captured = captured.clone();
        async move {
            captured
                .send(to_bytes(request.into_body(), 4096).await.unwrap())
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
            axum::Json(
                json!({"choices":[{"finish_reason":"stop","message":{"content":"SECOND_OK"}}]}),
            )
        }
    });
    let second_server = tokio::spawn(async move { axum::serve(second, second_app).await.unwrap() });
    let (_temporary, runtime, initial) = fixture();
    let mut config = initial.clone();
    config
        .providers
        .get_mut("p")
        .unwrap()
        .endpoints
        .insert("chat-completions".into(), format!("http://{first_address}"));
    config.providers.get_mut("q").unwrap().endpoints.insert(
        "chat-completions".into(),
        format!("http://{second_address}"),
    );
    config.model_aliases[0].continuity = Some("stateless".into());
    config.model_aliases[0].fallback = Some(crate::config::FailureConditions {
        http_statuses: vec![429],
        ..Default::default()
    });
    publish(&runtime, &initial, &config, "admitted-before-reload");
    runtime.open();
    let request = axum::http::Request::builder().method("POST").uri("/v1/chat/completions")
        .header("authorization", "Bearer test-local-key")
        .header("content-type", "application/json")
        .header("user-agent", "deepseek-harness/0.2.1-rc.2 (+https://github.com/deepseek-ai/deepseek-harness)")
        .body(Body::from(r#"{"model":"dynamic","messages":[{"role":"user","content":"original"}],"temperature":0.42}"#)).unwrap();
    let forwarded = tokio::spawn({
        let runtime = runtime.clone();
        async move { forward(State(runtime), request).await }
    });
    tokio::time::timeout(Duration::from_secs(5), entering.recv())
        .await
        .unwrap()
        .unwrap();
    let mut next = config.clone();
    next.providers
        .get_mut("q")
        .unwrap()
        .endpoints
        .insert("chat-completions".into(), "http://127.0.0.1:9".into());
    next.providers
        .get_mut("q")
        .unwrap()
        .models
        .get_mut("m")
        .unwrap()
        .priority = 99;
    publish(&runtime, &config, &next, "activated-during-first-attempt");
    release.notify_one();
    let response = tokio::time::timeout(Duration::from_secs(5), forwarded)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let id = response.headers()["x-tmax-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    let response = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(
        std::str::from_utf8(&response)
            .unwrap()
            .contains("SECOND_OK")
    );
    let request: Value = serde_json::from_slice(&requests.recv().await.unwrap()).unwrap();
    assert_eq!(
        request,
        json!({"model":"m","messages":[{"role":"user","content":"original"}],"temperature":0.42})
    );
    let inspected = runtime
        .history
        .view(&Request {
            command: vec!["request".into(), "inspect".into()],
            operands: vec![id],
            arguments: Default::default(),
            dry_run: false,
            if_revision: None,
        })
        .unwrap();
    assert_eq!(
        inspected["record"]["configuration_revision"],
        "admitted-before-reload"
    );
    assert_eq!(
        inspected["record"]["decision"]["attempts"][0]["provider"],
        "p"
    );
    assert_eq!(
        inspected["record"]["decision"]["attempts"][1]["provider"],
        "q"
    );
    assert_eq!(
        inspected["record"]["decision"]["attempts"][1]["outcome"],
        "completed"
    );
    let log = error_log(&runtime.store);
    let entry = log
        .iter()
        .find(|entry| entry["request_id"] == inspected["record"]["request"])
        .unwrap();
    assert_eq!(entry["event"], "gateway.request_recovered");
    assert_eq!(entry["level"], "warn");
    assert_eq!(entry["outcome"], "completed");
    assert_eq!(entry["provider"], "q");
    assert_eq!(entry["model"], "q/m");
    assert!(entry["upstream_request_id"].is_null());
    assert_eq!(entry["errors"][0]["provider"], "p");
    assert_eq!(
        entry["errors"][0]["upstream_request_id"],
        "first-provider-request"
    );
    assert_eq!(entry["errors"][0]["attempt"], 1);
    assert_eq!(entry["errors"][0]["outcome"], "failed");
    assert_eq!(entry["errors"][0]["stage"], "upstream_response");
    assert_eq!(entry["errors"][0]["http_status"], 429);
    let failure_time =
        chrono::DateTime::parse_from_rfc3339(entry["errors"][0]["time"].as_str().unwrap()).unwrap();
    let completion_time =
        chrono::DateTime::parse_from_rfc3339(entry["time"].as_str().unwrap()).unwrap();
    assert!(failure_time <= completion_time);
    assert!(!entry.to_string().contains("original"));
    first_server.abort();
    second_server.abort();
}

#[tokio::test]
async fn error_logs_preserve_upstream_failures_across_runtime_restarts_without_conversations() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let upstream = Router::new().fallback(|request: axum::extract::Request| async move {
        let bytes = to_bytes(request.into_body(), 4096).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        let (status, content_type, response) = match body["mode"].as_str().unwrap() {
            "http" => (StatusCode::BAD_REQUEST, "application/json", r#"{"error":{"message":"Invalid tool arguments","type":"invalid_request_error"}}"#),
            "plain" => (StatusCode::BAD_GATEWAY, "text/plain", "Upstream connection refused"),
            "event" => (StatusCode::OK, "text/event-stream", "data: {\"type\":\"response.output_text.delta\",\"delta\":\"private-successful-chunk\"}\n\nevent: error\ndata: {\"message\":\"Upstream function arguments must be a JSON object\",\"code\":\"bad_arguments\"}\n\n"),
            "nested" => (StatusCode::OK, "text/event-stream", "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"Malformed function arguments\"},\"output\":[\"private-output\"]}}"),
            _ => (StatusCode::OK, "application/json", r#"{"status":"completed","output":["private-successful-chunk"]}"#),
        };
        (status, [("content-type", content_type), ("x-request-id", "upstream-diagnostic-id")], response)
    });
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let (_temporary, runtime, initial) = fixture();
    let mut config = initial.clone();
    config
        .providers
        .get_mut("p")
        .unwrap()
        .endpoints
        .insert("responses".into(), format!("http://{address}/v1"));
    publish(&runtime, &initial, &config, "error-evidence");
    runtime.open();
    let store = runtime.store.clone();
    let mut handles = Vec::new();
    for mode in ["http", "plain", "event", "nested", "success"] {
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/v1/responses")
            .header("authorization", "Bearer test-local-key")
            .header("user-agent", "pi (linux test; x64)")
            .body(Body::from(
                json!({"model":"p/m","store":false,"input":"private-request-prompt","mode":mode})
                    .to_string(),
            ))
            .unwrap();
        let response = forward(State(runtime.clone()), request).await;
        let status = response.status().as_u16();
        let id = response.headers()["x-tmax-request-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let delivered = to_bytes(response.into_body(), 4096).await.unwrap();
        assert!(
            std::str::from_utf8(&delivered)
                .unwrap()
                .contains(match mode {
                    "http" => "Invalid tool arguments",
                    "plain" => "Upstream connection refused",
                    "event" => "bad_arguments",
                    "nested" => "Malformed function arguments",
                    _ => "private-successful-chunk",
                })
        );
        handles.push((mode, id, status));
    }
    drop(runtime);
    let restarted = Runtime::new(store, "restarted".into()).unwrap();
    let entries = error_log(&restarted.store);
    assert_eq!(entries.len(), 4);
    for (mode, id, status) in handles {
        let entry = entries.iter().find(|entry| entry["request_id"] == id);
        if mode == "success" {
            assert!(entry.is_none());
            continue;
        }
        let entry = entry.unwrap();
        assert_eq!(entry["event"], "gateway.request_failed");
        assert_eq!(entry["level"], "error");
        assert_eq!(entry["outcome"], "failed");
        assert_eq!(entry["http_status"], status);
        assert_eq!(entry["provider"], "p");
        assert_eq!(entry["model"], "p/m");
        assert_eq!(entry["upstream_request_id"], "upstream-diagnostic-id");
        assert_eq!(
            entry["errors"][0]["upstream_request_id"],
            "upstream-diagnostic-id"
        );
        assert_eq!(entry["errors"][0]["attempt"], 1);
        assert_eq!(entry["errors"][0]["outcome"], "failed");
        let (message, stage) = match mode {
            "http" => ("Invalid tool arguments", "upstream_response"),
            "plain" => ("Upstream connection refused", "upstream_response"),
            "event" => (
                "Upstream function arguments must be a JSON object",
                "upstream_stream",
            ),
            _ => ("Malformed function arguments", "upstream_stream"),
        };
        assert_eq!(entry["stage"], stage);
        assert_eq!(entry["errors"][0]["stage"], stage);
        assert_eq!(entry["error_message"], message);
        assert_eq!(entry["errors"][0]["error_message"], message);
        match mode {
            "http" => assert_eq!(
                entry["errors"][0]["error_details"]["error"]["type"],
                "invalid_request_error"
            ),
            "plain" => assert_eq!(entry["errors"][0]["error_details"], message),
            "event" => assert_eq!(entry["errors"][0]["error_details"]["code"], "bad_arguments"),
            _ => assert_eq!(entry["errors"][0]["error_details"]["message"], message),
        }
        assert!(entry.get("record").is_none());
        assert!(entry.get("decision").is_none());
        let encoded = entry.to_string();
        for excluded in [
            "private-request-prompt",
            "private-successful-chunk",
            "private-output",
            "test-provider-key",
            "test-local-key",
        ] {
            assert!(
                !encoded.contains(excluded),
                "unexpected conversation or credential content: {excluded}"
            );
        }
        let query = Request {
            command: vec!["history".into(), "inspect".into()],
            operands: vec![id],
            arguments: Default::default(),
            dry_run: false,
            if_revision: None,
        };
        assert_eq!(
            restarted.history.view(&query).unwrap()["snapshot_state"],
            "unavailable"
        );
    }
    server.abort();
}

#[tokio::test]
async fn response_stream_read_failure_uses_stream_stage() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let upstream = Router::new().fallback(|| async {
        let stream = futures_util::stream::once(async {
            Ok::<_, std::io::Error>(Bytes::from_static(
                b"data: {\"type\":\"response.output_text.delta\"}\n\n",
            ))
        })
        .chain(futures_util::stream::once(async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Err(std::io::Error::other("upstream interrupted"))
        }));
        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/event-stream")
            .header("x-request-id", "stream-request-id")
            .body(Body::from_stream(stream))
            .unwrap()
    });
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let (_temporary, runtime, initial) = fixture();
    let mut config = initial.clone();
    config
        .providers
        .get_mut("p")
        .unwrap()
        .endpoints
        .insert("responses".into(), format!("http://{address}/v1"));
    publish(&runtime, &initial, &config, "stream-error");
    runtime.open();
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/responses")
        .header("authorization", "Bearer test-local-key")
        .header("user-agent", "pi (linux test; x64)")
        .body(Body::from(r#"{"model":"p/m","input":"x"}"#))
        .unwrap();
    let response = forward(State(runtime.clone()), request).await;
    assert_eq!(response.status(), StatusCode::OK);
    let id = response.headers()["x-tmax-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(to_bytes(response.into_body(), 4096).await.is_err());
    let entries = error_log(&runtime.store);
    let entry = entries
        .iter()
        .find(|entry| entry["request_id"] == id)
        .unwrap();
    assert_eq!(entry["event"], "gateway.request_failed");
    assert_eq!(entry["outcome"], "failed");
    assert_eq!(entry["http_status"], 200);
    assert_eq!(entry["stage"], "upstream_stream");
    assert_eq!(entry["errors"][0]["stage"], "upstream_stream");
    assert_eq!(entry["errors"][0]["attempt"], 1);
    let query = Request {
        command: vec!["history".into(), "inspect".into()],
        operands: vec![id],
        arguments: Default::default(),
        dry_run: false,
        if_revision: None,
    };
    assert_eq!(
        runtime.history.view(&query).unwrap()["record"]["stage"],
        "upstream_stream"
    );
    server.abort();
}

#[tokio::test]
async fn local_rejection_logs_only_known_upstream_information() {
    let (_temporary, runtime, config) = fixture();
    publish(&runtime, &config, &config, "local-rejection");
    runtime.open();
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/responses")
        .header("authorization", "Bearer wrong-key")
        .body(Body::empty())
        .unwrap();
    let response = forward(State(runtime.clone()), request).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let entries = error_log(&runtime.store);
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry["event"], "gateway.request_rejected");
    assert_eq!(entry["level"], "warn");
    assert_eq!(entry["outcome"], "rejected");
    assert_eq!(entry["stage"], "shared_key_authentication");
    assert_eq!(entry["reason_code"], "authentication_failed");
    assert!(entry["provider"].is_null());
    assert!(entry["model"].is_null());
    assert!(entry["protocol"].is_null());
    assert!(entry["http_status"].is_null());
    assert!(entry["upstream_request_id"].is_null());
    assert_eq!(entry["errors"], json!([]));
}

#[tokio::test]
async fn error_log_storage_failure_does_not_change_error_delivery() {
    let (_temporary, runtime, config) = fixture();
    publish(&runtime, &config, &config, "unavailable-error-log");
    runtime.open();
    std::fs::create_dir(runtime.store.root.join("error.log")).unwrap();
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/responses")
        .header("authorization", "Bearer wrong-key")
        .body(Body::empty())
        .unwrap();
    let response = forward(State(runtime.clone()), request).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(body["error"]["type"], "authentication_failed");
    assert_eq!(
        body["error"]["message"],
        "Current shared gateway key required"
    );
    let query = Request {
        command: vec!["history".into(), "inspect".into()],
        operands: vec![body["error"]["request_id"].as_str().unwrap().into()],
        arguments: Default::default(),
        dry_run: false,
        if_revision: None,
    };
    assert_eq!(
        runtime.history.view(&query).unwrap()["record"]["evidence_gap"],
        true
    );
    assert!(runtime.store.root.join("error.log").is_dir());
    assert_eq!(runtime.active(), 0);
}

#[tokio::test]
async fn error_detail_bounds_do_not_truncate_forwarded_responses() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let size = crate::diagnostics::error_log::DETAIL_BYTES + 1024;
    let upstream =
        Router::new().fallback(move || async move { (StatusCode::BAD_REQUEST, "界".repeat(size)) });
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let (_temporary, runtime, initial) = fixture();
    let mut config = initial.clone();
    config
        .providers
        .get_mut("p")
        .unwrap()
        .endpoints
        .insert("chat-completions".into(), format!("http://{address}/v1"));
    publish(&runtime, &initial, &config, "error-truncation");
    runtime.open();
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("authorization", "Bearer test-local-key")
        .header("user-agent", "pi (linux test; x64)")
        .body(Body::from(
            r#"{"model":"p/m","messages":[{"role":"user","content":"x"}]}"#,
        ))
        .unwrap();
    let response = forward(State(runtime.clone()), request).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let id = response.headers()["x-tmax-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    let delivered = to_bytes(response.into_body(), size * 3).await.unwrap();
    assert_eq!(delivered.as_ref(), "界".repeat(size).as_bytes());
    let log = error_log(&runtime.store);
    let entry = log.iter().find(|entry| entry["request_id"] == id).unwrap();
    assert_eq!(entry["errors"][0]["truncated"], true);
    let text = entry["errors"][0]["error_details"].as_str().unwrap();
    assert!(text.len() <= crate::diagnostics::error_log::DETAIL_BYTES);
    assert!(text.chars().all(|character| character == '界'));
    server.abort();
}

#[tokio::test]
async fn review_regression_oversized_request_id_is_visible_as_truncated() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let upstream = Router::new().fallback(|| async {
        (
            StatusCode::BAD_REQUEST,
            [("x-request-id", "r".repeat(700))],
            axum::Json(json!({"error":{"message":"Rejected"}})),
        )
    });
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let (_temporary, runtime, initial) = fixture();
    let mut config = initial.clone();
    config
        .providers
        .get_mut("p")
        .unwrap()
        .endpoints
        .insert("chat-completions".into(), format!("http://{address}/v1"));
    publish(&runtime, &initial, &config, "long-request-id");
    runtime.open();
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("authorization", "Bearer test-local-key")
        .header("user-agent", "pi (linux test; x64)")
        .body(Body::from(
            r#"{"model":"p/m","messages":[{"role":"user","content":"x"}]}"#,
        ))
        .unwrap();
    let response = forward(State(runtime.clone()), request).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    to_bytes(response.into_body(), 4096).await.unwrap();
    let entry = error_log(&runtime.store).pop().unwrap();
    assert_eq!(entry["upstream_request_id"], "r".repeat(512));
    assert_eq!(entry["errors"][0]["truncated"], true);
    assert_eq!(entry["truncated"], true);
    assert_eq!(
        entry["errors"][0]["error_details"]["error"]["message"],
        "Rejected"
    );
    server.abort();
}

#[tokio::test]
async fn review_regression_failed_fallback_selection_retains_the_first_error() {
    let (_temporary, runtime, initial) = fixture();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let store = runtime.store.clone();
    let upstream = Router::new().fallback(move || {
        let store = store.clone();
        async move {
            store
                .write_raw("provider-failures/state.json", b"broken failure state")
                .unwrap();
            (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(json!({"error":{"message":"First attempt throttled"}})),
            )
        }
    });
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let mut config = initial.clone();
    let provider = config.providers.get_mut("p").unwrap();
    provider
        .endpoints
        .insert("chat-completions".into(), format!("http://{address}/v1"));
    provider.failure_policy = Some(crate::config::FailurePolicy {
        enabled: true,
        errors: crate::config::FailureConditions {
            http_statuses: vec![429],
            ..Default::default()
        },
        consecutive_failures: 2,
        priority: -1,
        duration_seconds: 60,
    });
    config.model_aliases[0].continuity = Some("stateless".into());
    config.model_aliases[0].fallback = Some(crate::config::FailureConditions {
        http_statuses: vec![429],
        ..Default::default()
    });
    publish(&runtime, &initial, &config, "selection-error-after-refusal");
    runtime.open();
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("authorization", "Bearer test-local-key")
        .header(
            "user-agent",
            "deepseek-harness/0.2.1-rc.2 (+https://github.com/deepseek-ai/deepseek-harness)",
        )
        .body(Body::from(
            r#"{"model":"dynamic","messages":[{"role":"user","content":"x"}]}"#,
        ))
        .unwrap();
    let response = forward(State(runtime.clone()), request).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    to_bytes(response.into_body(), 4096).await.unwrap();
    let entry = error_log(&runtime.store).pop().unwrap();
    assert_eq!(entry["outcome"], "rejected");
    assert_eq!(entry["stage"], "selection");
    assert_eq!(entry["errors"][0]["provider"], "p");
    assert_eq!(entry["errors"][0]["http_status"], 429);
    server.abort();
}

#[tokio::test]
async fn dropping_a_partial_error_response_preserves_text_and_marks_it_incomplete() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let upstream = Router::new().fallback(|| async {
        let prefix = futures_util::stream::once(async {
            Ok::<_, std::io::Error>(Bytes::from_static(
                b"{\"error\":{\"message\":\"Rejected before EOF\"}}",
            ))
        });
        (
            StatusCode::BAD_REQUEST,
            Body::from_stream(prefix.chain(futures_util::stream::pending())),
        )
    });
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let (_temporary, runtime, initial) = fixture();
    let mut config = initial.clone();
    config
        .providers
        .get_mut("p")
        .unwrap()
        .endpoints
        .insert("chat-completions".into(), format!("http://{address}/v1"));
    publish(&runtime, &initial, &config, "incomplete-http-error");
    runtime.open();
    let request = axum::http::Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("authorization", "Bearer test-local-key")
        .header("user-agent", "pi (linux test; x64)")
        .body(Body::from(
            r#"{"model":"p/m","messages":[{"role":"user","content":"x"}]}"#,
        ))
        .unwrap();
    let response = forward(State(runtime.clone()), request).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let id = response.headers()["x-tmax-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    let mut stream = response.into_body().into_data_stream();
    assert_eq!(
        stream.next().await.unwrap().unwrap().as_ref(),
        b"{\"error\":{\"message\":\"Rejected before EOF\"}}"
    );
    drop(stream);
    let entry = error_log(&runtime.store).pop().unwrap();
    assert_eq!(entry["request_id"], id);
    assert_eq!(entry["outcome"], "cancelled");
    assert_eq!(
        entry["errors"][0]["error_details"],
        r#"{"error":{"message":"Rejected before EOF"}}"#
    );
    assert_eq!(entry["errors"][0]["truncated"], true);
    assert_eq!(entry["evidence_gap"], true);
    server.abort();
}
