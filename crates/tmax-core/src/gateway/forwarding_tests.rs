use super::*;
use crate::{
    Request,
    gateway::test_support::{fixture, publish},
};
use std::collections::BTreeMap;

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
