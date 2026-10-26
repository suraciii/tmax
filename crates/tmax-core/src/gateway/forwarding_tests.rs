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
