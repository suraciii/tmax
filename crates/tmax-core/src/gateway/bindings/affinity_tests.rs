use super::*;
use crate::gateway::test_support::{fixture, native_headers, publish, test_snapshot};
use crate::{
    Request,
    gateway::{client_identity::upstream_headers, forwarding::forward, test_support::GO_URL},
};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::State,
    http::{Method, StatusCode},
};

#[test]
fn native_requirements_filter_new_selection_but_never_move_retained_destination() {
    let (_temporary, runtime, config) = fixture();
    let mut go = config.clone();
    let p = go.providers.get_mut("p").unwrap();
    p.integration = Some("opencode-go".into());
    p.endpoints.clear();
    let mut stateless = go.clone();
    stateless.model_aliases[0].continuity = Some("stateless".into());
    publish(&runtime, &config, &stateless, "native-stateless");
    let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
    let mut local = native_headers(at);
    local.remove("x-session-id");
    let body = json!({"messages":[{"role":"user","content":"x"}]});
    let lower = runtime
        .choose(
            &test_snapshot(&stateless, "native-stateless"),
            "dynamic",
            "chat-completions",
            &body,
            &local,
            at + chrono::Duration::milliseconds(1),
        )
        .unwrap();
    // Pi's affinity header is gone, but the gateway can attribute the request
    // itself for a Go destination, so p stays eligible instead of falling back.
    assert_eq!(lower.selection.provider, "p");
    publish(&runtime, &stateless, &go, "native-context");
    let mut native = native_headers(at + chrono::Duration::seconds(1));
    let session = native["x-session-id"].clone();
    let high = runtime
        .choose(
            &test_snapshot(&go, "native-context"),
            "dynamic",
            "chat-completions",
            &body,
            &native,
            at + chrono::Duration::seconds(2),
        )
        .unwrap();
    assert_eq!(high.selection.provider, "p");
    native.insert("x-opencode-session", session.clone());
    let retained = runtime
        .choose(
            &test_snapshot(&go, "native-context"),
            "dynamic",
            "chat-completions",
            &body,
            &native,
            at + chrono::Duration::seconds(3),
        )
        .unwrap();
    assert_eq!(retained.selection.provider, "p");
    assert_eq!(
        retained.selection.evidence["selection_reason"],
        "retained_destination"
    );
    native.insert("x-opencode-session", "conflicting-session".parse().unwrap());
    let error = runtime
        .choose(
            &test_snapshot(&go, "native-context"),
            "dynamic",
            "chat-completions",
            &body,
            &native,
            at + chrono::Duration::seconds(3),
        )
        .err()
        .unwrap();
    assert_eq!(error.code, "session_identity_unavailable");
    let bindings = runtime.bindings.lock().unwrap();
    assert_eq!(
        bindings
            .entries
            .values()
            .filter(|binding| binding.provider == "p")
            .count(),
        1
    );
}
#[test]
fn pi_go_forwarding_preserves_native_identity_across_turns_resume_and_new_sessions() {
    let (_temporary, runtime, config) = fixture();
    let mut go = config.clone();
    let provider = go.providers.get_mut("p").unwrap();
    provider.integration = Some("opencode-go".into());
    provider.endpoints.clear();
    publish(&runtime, &config, &go, "go-affinity");
    let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
    let headers = native_headers(at);
    let initial = json!({"messages":[{"role":"user","content":"x"}],"stream":true});
    let continuation = json!({"messages":[{"role":"user","content":"x"},{"role":"assistant","content":"y"},{"role":"user","content":"z"}],"stream":false});
    let tool_turn = json!({"messages":[{"role":"user","content":"x"},{"role":"assistant","tool_calls":[{"id":"call-1","type":"function","function":{"name":"test","arguments":"{}"}}]},{"role":"tool","tool_call_id":"call-1","content":"ok"}],"stream":true});
    let auxiliary = json!({"messages":[{"role":"user","content":"summarize"}],"stream":false});
    let snapshot = test_snapshot(&go, "go-affinity");
    for body in [&initial, &continuation, &tool_turn, &auxiliary] {
        let choice = runtime
            .choose(&snapshot, "p/m", "chat-completions", body, &headers, at)
            .unwrap();
        assert_eq!(choice.selection.provider, "p");
        assert_eq!(choice.go_session_header, Some("x-session-id"));
        let forwarded = upstream_headers(&headers, choice.go_session_header, GO_URL);
        assert_eq!(forwarded["x-opencode-session"], headers["x-session-id"]);
        assert_eq!(forwarded["x-session-id"], headers["x-session-id"]);
        assert_eq!(forwarded["user-agent"], headers["user-agent"]);
    }
    let store = runtime.store.clone();
    drop(runtime);
    let resumed = Runtime::new(store, "go-resume".into()).unwrap();
    let retained = resumed
        .choose(
            &snapshot,
            "p/m",
            "chat-completions",
            &continuation,
            &headers,
            at,
        )
        .unwrap();
    assert_eq!(
        retained.selection.evidence["selection_reason"],
        "retained_destination"
    );
    let forwarded = upstream_headers(&headers, retained.go_session_header, GO_URL);
    assert_eq!(forwarded["x-opencode-session"], headers["x-session-id"]);
    let fresh = native_headers(at + chrono::Duration::seconds(1));
    let new = resumed
        .choose(
            &snapshot,
            "p/m",
            "chat-completions",
            &initial,
            &fresh,
            at + chrono::Duration::seconds(1),
        )
        .unwrap();
    let forwarded_new = upstream_headers(&fresh, new.go_session_header, GO_URL);
    assert_ne!(
        forwarded_new["x-opencode-session"],
        forwarded["x-opencode-session"]
    );
    assert_eq!(resumed.bindings.lock().unwrap().entries.len(), 2);
    let mut disabled = go.clone();
    disabled.providers.get_mut("p").unwrap().enabled = false;
    publish(&resumed, &go, &disabled, "go-disabled");
    publish(&resumed, &disabled, &go, "go-restored");
    let error = resumed
        .choose(
            &snapshot,
            "p/m",
            "chat-completions",
            &continuation,
            &headers,
            at,
        )
        .err()
        .unwrap();
    assert_eq!(error.code, "provider_disabled");
    assert_eq!(resumed.bindings.lock().unwrap().entries.len(), 2);
}

#[tokio::test]
async fn go_identity_rejections_retain_exact_history_reason_without_dispatch() {
    let (_temporary, runtime, config) = fixture();
    let mut go = config.clone();
    let provider = go.providers.get_mut("p").unwrap();
    provider.integration = Some("opencode-go".into());
    provider.endpoints.clear();
    provider.models.get_mut("m").unwrap().overrides.tool_call = Some(false);
    go.model_aliases[0].candidates = vec!["p/m".into()];
    go.model_aliases[0].continuity = Some("stateless".into());
    publish(&runtime, &config, &go, "go-rejections");
    runtime.open();
    let at = Utc::now();
    runtime.bindings.lock().unwrap().epoch = at - chrono::Duration::seconds(1);
    let qualified = native_headers(at);
    let mut conflict = qualified.clone();
    conflict.insert("x-opencode-session", "conflicting-session".parse().unwrap());
    let mut hermes_affinity = qualified.clone();
    hermes_affinity.insert("user-agent", "HermesAgent/0.22.1".parse().unwrap());
    hermes_affinity.insert("x-session-id", "native-hermes-session".parse().unwrap());
    let body = json!({"model":"p/m","messages":[{"role":"user","content":"x"}]});
    let mut cases = Vec::new();
    for headers in [conflict, hermes_affinity] {
        cases.push((
            "/v1/chat/completions",
            body.clone(),
            headers,
            "native_session_required",
        ));
    }
    cases.extend([
        ("/v1/responses", json!({"model":"p/m","input":[{"role":"user","content":"x"}],"store":false}), qualified.clone(), "native_session_required"),
        ("/v1/chat/completions", json!({"model":"p/m","messages":[{"role":"user","content":"x"}],"tools":[{"type":"function","function":{"name":"test","parameters":{"type":"object"}}}]}), qualified, "tools_incompatible"),
    ]);
    let mut expected = Vec::new();
    for (path, body, headers, reason) in cases {
        let mut request = axum::http::Request::builder()
            .method(Method::POST)
            .uri(path)
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        *request.headers_mut() = headers;
        request
            .headers_mut()
            .insert("authorization", "Bearer test-local-key".parse().unwrap());
        let response = forward(State(runtime.clone()), request).await;
        assert_eq!(
            response.status(),
            StatusCode::CONFLICT,
            "case {path} {body} {reason}"
        );
        expected.push(reason);
    }
    let mut query = Request {
        command: vec!["history".into(), "list".into()],
        operands: Vec::new(),
        arguments: BTreeMap::new(),
        dry_run: false,
        if_revision: None,
    };
    let listed = runtime.history.view(&query).unwrap();
    let records = listed["records"].as_array().unwrap();
    assert_eq!(records.len(), expected.len());
    query.command[1] = "inspect".into();
    for (row, reason) in records.iter().zip(expected) {
        query.operands = vec![row["request"].as_str().unwrap().into()];
        let inspected = runtime.history.view(&query).unwrap();
        let record = &inspected["record"];
        assert_eq!(record["reason_code"], reason);
        assert_eq!(record["outcome"], "rejected");
        assert_eq!(record["stage"], "selection");
        assert_eq!(record["execution"], "not_dispatched");
        assert!(record["upstream_status"].is_null());
    }
    assert!(runtime.bindings.lock().unwrap().entries.is_empty());
}

#[tokio::test]
async fn effort_maps_reach_upstream_without_fallback_or_omitted_effort_synthesis() {
    for (protocol, path) in [
        ("chat-completions", "/v1/chat/completions"),
        ("responses", "/v1/responses"),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (sent, mut received) = tokio::sync::mpsc::channel(4);
        let upstream = Router::new().fallback(move |request: axum::extract::Request| {
            let sent = sent.clone();
            async move {
                let body = to_bytes(request.into_body(), 4096).await.unwrap();
                sent.send(serde_json::from_slice::<Value>(&body).unwrap()).await.unwrap();
                axum::Json(json!({"id":"response","choices":[],"usage":{"input_tokens":1,"output_tokens":1}}))
            }
        });
        let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
        let (_temporary, runtime, original) = fixture();
        let mut config = original.clone();
        config.model_aliases[0].protocol = protocol.into();
        for (id, values) in [("p", vec!["low", "high"]), ("q", vec!["max"])] {
            let provider = config.providers.get_mut(id).unwrap();
            provider
                .endpoints
                .insert(protocol.into(), format!("http://{address}"));
            provider
                .models
                .get_mut("m")
                .unwrap()
                .overrides
                .reasoning_options = Some(vec![crate::config::ReasoningOption {
                r#type: "effort".into(),
                values: Some(values.into_iter().map(String::from).collect()),
            }]);
        }
        publish(&runtime, &original, &config, "wire-effort");
        runtime.open();
        let at = Utc::now();
        runtime.bindings.lock().unwrap().epoch = at - chrono::Duration::seconds(1);
        let headers = native_headers(at);
        let mut body = if protocol == "responses" {
            json!({"model":"dynamic","input":[{"role":"user","content":"x"}],"store":false,"reasoning":{"effort":"max","summary":"auto"},"extension":{"keep":true}})
        } else {
            json!({"model":"dynamic","messages":[{"role":"user","content":"x"}],"reasoning_effort":"max","extension":{"keep":true}})
        };
        let make_request = |body: &Value| {
            let mut request = axum::http::Request::builder()
                .method(Method::POST)
                .uri(path)
                .body(Body::from(serde_json::to_vec(body).unwrap()))
                .unwrap();
            *request.headers_mut() = headers.clone();
            request
                .headers_mut()
                .insert("authorization", "Bearer test-local-key".parse().unwrap());
            request
        };
        let rejected = forward(State(runtime.clone()), make_request(&body)).await;
        assert_eq!(rejected.status(), StatusCode::CONFLICT);
        assert!(received.try_recv().is_err());
        assert!(runtime.bindings.lock().unwrap().entries.is_empty());
        let mut query = Request {
            command: vec!["history".into(), "list".into()],
            operands: vec![],
            arguments: BTreeMap::new(),
            dry_run: false,
            if_revision: None,
        };
        let listed = runtime.history.view(&query).unwrap();
        query.command[1] = "inspect".into();
        query.operands = vec![listed["records"][0]["request"].as_str().unwrap().into()];
        let record = runtime.history.view(&query).unwrap();
        assert_eq!(record["record"]["qualified_model"], "p/m");
        assert_eq!(
            record["record"]["reason_code"],
            "reasoning_effort_incompatible"
        );
        assert_eq!(
            record["record"]["decision"]["effort_resolution"]["input_effort"],
            "max"
        );

        let previous = config.clone();
        config
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .effort_map
            .insert(
                protocol.into(),
                BTreeMap::from([("max".into(), "high".into())]),
            );
        publish(&runtime, &previous, &config, "wire-mapped");
        let response = forward(State(runtime.clone()), make_request(&body)).await;
        assert_eq!(response.status(), StatusCode::OK);
        to_bytes(response.into_body(), 4096).await.unwrap();
        let actual = received.recv().await.unwrap();
        let mut expected = body.clone();
        expected["model"] = json!("m");
        crate::reasoning::apply(&mut expected, protocol, "high").unwrap();
        assert_eq!(actual, expected);
        if protocol == "responses" {
            body["reasoning"].as_object_mut().unwrap().remove("effort");
        } else {
            body.as_object_mut().unwrap().remove("reasoning_effort");
        }
        let response = forward(State(runtime.clone()), make_request(&body)).await;
        assert_eq!(response.status(), StatusCode::OK);
        to_bytes(response.into_body(), 4096).await.unwrap();
        let actual = received.recv().await.unwrap();
        body["model"] = json!("m");
        assert_eq!(actual, body);
        assert_eq!(runtime.bindings.lock().unwrap().entries.len(), 1);
        server.abort();
    }
}
