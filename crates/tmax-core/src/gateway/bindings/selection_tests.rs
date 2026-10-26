use super::*;
use crate::gateway::test_support::{fixture, native_headers, publish, test_snapshot};
use std::time::Duration;

#[test]
fn protocol_eligibility_uses_provider_endpoints_not_model_identity() {
    let (_temporary, runtime, config) = fixture();
    let mut go = config.clone();
    let provider = go.providers.get_mut("p").unwrap();
    provider.integration = Some("opencode-go".into());
    provider.endpoints.clear();
    provider.models = [
        (
            "deepseek-v4.1-flash".into(),
            crate::config::Model::default(),
        ),
        ("unverified-model".into(), crate::config::Model::default()),
    ]
    .into();
    go.model_aliases[0].protocol = "responses".into();
    go.model_aliases[0].continuity = Some("stateless".into());
    go.model_aliases[0].candidates = vec!["p/deepseek-v4.1-flash".into()];
    publish(&runtime, &config, &go, "provider-protocols");
    let body = json!({"input":"x","store":false});
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", "pi (test; test)".parse().unwrap());
    for id in ["p/deepseek-v4.1-flash", "p/unverified-model", "dynamic"] {
        let choice = runtime
            .choose(
                &test_snapshot(&go, "provider-protocols"),
                id,
                "responses",
                &body,
                &headers,
                Utc::now(),
            )
            .unwrap();
        assert_eq!(choice.selection.provider, "p");
        assert_eq!(
            choice.endpoint,
            "https://opencode.ai/inference/go/openai/v1"
        );
    }
    let mut chat_only = go.clone();
    chat_only.providers.get_mut("p").unwrap().integration = Some("custom".into());
    chat_only.providers.get_mut("p").unwrap().endpoints =
        [("chat-completions".into(), "http://127.0.0.1:9/v1".into())].into();
    publish(&runtime, &go, &chat_only, "chat-only");
    assert!(
        runtime
            .choose(
                &test_snapshot(&chat_only, "chat-only"),
                "p/deepseek-v4.1-flash",
                "responses",
                &body,
                &headers,
                Utc::now(),
            )
            .err()
            .unwrap()
            .code
            == "endpoint_unavailable"
    );
}
#[test]
fn alias_ties_report_the_retained_model_identity_used_for_ranking() {
    let (_temporary, runtime, mut config) = fixture();
    let previous = config.clone();
    config
        .providers
        .get_mut("q")
        .unwrap()
        .models
        .get_mut("m")
        .unwrap()
        .priority = 10;
    publish(&runtime, &previous, &config, "tied");
    let identities = crate::inputs::Snapshot::capture_loaded(&runtime.store, &config, None);
    let p = identities.destination_identity("p", "m").unwrap().2;
    let q = identities.destination_identity("q", "m").unwrap().2;
    let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
    let choice = runtime
        .choose(
            &test_snapshot(&config, "tied"),
            "dynamic",
            "chat-completions",
            &json!({"messages":[{"role":"user","content":"x"}]}),
            &native_headers(at),
            at + chrono::Duration::milliseconds(1),
        )
        .unwrap();
    assert_eq!(choice.selection.model_identity, p.min(q));
    assert_eq!(
        choice.selection.evidence["selection_reason"],
        "stable_model_identity"
    );
    assert_eq!(
        choice.selection.evidence["selection"]["model_identity"],
        choice.selection.model_identity
    );
    assert_eq!(
        choice.selection.evidence["selection"]["qualified_model"],
        format!("{}/{}", choice.selection.provider, choice.selection.model)
    );
}

#[test]
fn context_free_selection_does_not_wait_for_session_bindings() {
    let (_temporary, runtime, config) = fixture();
    let snapshot = test_snapshot(&config, "fixture-1");
    let bindings = runtime.bindings.lock().unwrap();
    let worker_runtime = runtime.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let choice = worker_runtime.choose(
            &snapshot,
            "p/m",
            "chat-completions",
            &json!({"messages":[{"role":"user","content":"synthetic"}]}),
            &HeaderMap::new(),
            Utc::now(),
        );
        sender.send(choice).unwrap();
    });
    let choice = receiver.recv_timeout(Duration::from_secs(5));
    drop(bindings);
    worker.join().unwrap();
    let choice = choice
        .expect("context-free selection waited for session state")
        .unwrap();
    assert_eq!(choice.selection.provider, "p");
    assert_eq!(choice.selection.model, "m");
}

#[test]
fn candidate_effort_precedes_filtering_and_retained_requests_without_rerouting() {
    let (_temporary, runtime, original) = fixture();
    let mut config = original.clone();
    for (provider, values) in [("p", vec!["low", "high"]), ("q", vec!["max"])] {
        config
            .providers
            .get_mut(provider)
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .overrides
            .reasoning_options = Some(vec![crate::config::ReasoningOption {
            r#type: "effort".into(),
            values: Some(values.into_iter().map(String::from).collect()),
        }]);
    }
    publish(&runtime, &original, &config, "effort-capabilities");
    let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
    let body = json!({"messages":[{"role":"user","content":"x"}],"reasoning_effort":"max"});
    let first = runtime
        .choose(
            &test_snapshot(&config, "no-override"),
            "dynamic",
            "chat-completions",
            &body,
            &native_headers(at),
            at,
        )
        .unwrap();
    assert_eq!(first.selection.provider, "q");
    let previous = config.clone();
    config.model_aliases[0]
        .candidate_reasoning_effort
        .insert("p/m".into(), "high".into());
    publish(&runtime, &previous, &config, "fixed-high");
    let next = at + chrono::Duration::seconds(1);
    let headers = native_headers(next);
    let fixed = runtime
        .choose(
            &test_snapshot(&config, "fixed-high"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            next,
        )
        .unwrap();
    assert_eq!(fixed.selection.provider, "p");
    assert_eq!(
        fixed.selection.evidence["selection"]["candidate_reasoning_effort"],
        "high"
    );
    let previous = config.clone();
    config.model_aliases[0]
        .candidate_reasoning_effort
        .insert("p/m".into(), "low".into());
    config
        .providers
        .get_mut("q")
        .unwrap()
        .models
        .get_mut("m")
        .unwrap()
        .priority = 100;
    publish(&runtime, &previous, &config, "fixed-low");
    let retained = runtime
        .choose(
            &test_snapshot(&config, "fixed-low"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            next,
        )
        .unwrap();
    assert_eq!(retained.selection.provider, "p");
    assert_eq!(
        retained.selection.evidence["selection_reason"],
        "retained_destination"
    );
    let retained_evidence = safe_evidence(&retained.selection.evidence);
    assert_eq!(
        retained_evidence["selected"]["candidate_reasoning_effort"],
        "low"
    );
    assert_eq!(
        retained_evidence["original_decision"]["selection"]["candidate_reasoning_effort"],
        "high"
    );
    let conflict =
        json!({"messages":[{"role":"user","content":"x"}],"thinking":{"type":"disabled"}});
    assert_eq!(
        runtime
            .choose(
                &test_snapshot(&config, "conflict"),
                "dynamic",
                "chat-completions",
                &conflict,
                &headers,
                next
            )
            .err()
            .unwrap()
            .code,
        "reasoning_toggle_conflict"
    );
    let previous = config.clone();
    config.model_aliases[0]
        .candidate_reasoning_effort
        .insert("p/m".into(), "max".into());
    publish(&runtime, &previous, &config, "unsupported");
    assert_eq!(
        runtime
            .choose(
                &test_snapshot(&config, "unsupported"),
                "dynamic",
                "chat-completions",
                &body,
                &headers,
                next
            )
            .err()
            .unwrap()
            .code,
        "reasoning_effort_incompatible"
    );
    let previous = config.clone();
    config
        .providers
        .get_mut("p")
        .unwrap()
        .models
        .get_mut("m")
        .unwrap()
        .overrides
        .reasoning_options = Some(vec![crate::config::ReasoningOption {
        r#type: "toggle".into(),
        values: None,
    }]);
    publish(&runtime, &previous, &config, "toggle-only");
    assert_eq!(
        runtime
            .choose(
                &test_snapshot(&config, "toggle-only"),
                "dynamic",
                "chat-completions",
                &body,
                &headers,
                next
            )
            .err()
            .unwrap()
            .code,
        "reasoning_effort_unavailable"
    );
    assert_eq!(body["reasoning_effort"], "max");
}
#[test]
fn arbitrary_aliases_use_only_explicit_members_across_calendar_boundaries() {
    let (_temporary, runtime, original) = fixture();
    let mut config = original.clone();
    let provider = config.providers.get_mut("p").unwrap();
    let scheduled = provider.models.get_mut("m").unwrap();
    scheduled.priority = 0;
    scheduled.timezone = Some("Etc/UTC".into());
    scheduled.schedule = vec![crate::config::Schedule {
        priority: 20,
        window: crate::config::TimeWindow {
            weekdays: Some(vec!["sat".into()]),
            start: Some("00:00".into()),
            end: Some("08:00".into()),
            ..Default::default()
        },
    }];
    provider.models.insert(
        "outside".into(),
        crate::config::Model {
            priority: 999,
            ..Default::default()
        },
    );
    for (id, members) in [
        ("L0", vec!["p/m", "q/m"]),
        ("L1", vec!["p/m"]),
        ("my.custom-name", vec!["q/m"]),
    ] {
        config.model_aliases.push(crate::config::ModelAlias {
            candidate_reasoning_effort: Default::default(),
            id: id.into(),
            protocol: "chat-completions".into(),
            candidates: members.into_iter().map(String::from).collect(),
            enabled: true,
            continuity: None,
        });
    }
    crate::config::validate(&config).unwrap();
    publish(&runtime, &original, &config, "explicit-calendar");
    let before = crate::inputs::instant("2026-10-24T07:59:59Z").unwrap();
    let boundary = crate::inputs::instant("2026-10-24T08:00:00Z").unwrap();
    let select = |id, at| {
        crate::routing::select(&runtime.store, &config, id, "chat-completions", at).unwrap()
    };
    let first = select("L0", before);
    assert_eq!(first.provider, "p");
    assert_eq!(first.priority, 20);
    assert_eq!(
        first.evidence["priority_evaluations"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let next = select("L0", boundary);
    assert_eq!(next.provider, "q");
    assert_eq!(next.priority, 1);
    assert_eq!(select("L1", boundary).provider, "p");
    assert_eq!(select("my.custom-name", before).provider, "q");
    assert_eq!(select("p/m", boundary).provider, "p");
    assert_eq!(select("p/outside", boundary).model, "outside");
    assert!(
        crate::routing::select(&runtime.store, &config, "L3", "chat-completions", before).is_err()
    );
    assert!(crate::routing::select(&runtime.store, &config, "L0", "responses", before).is_err());
    let mut disabled = config.clone();
    for provider in ["p", "q"] {
        disabled
            .providers
            .get_mut(provider)
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .enabled = false;
    }
    assert_eq!(
        crate::routing::select(
            &runtime.store,
            &disabled,
            "L0",
            "chat-completions",
            boundary
        )
        .unwrap_err()
        .code,
        "model_unavailable"
    );
    assert_eq!(
        crate::routing::select(
            &runtime.store,
            &disabled,
            "p/outside",
            "chat-completions",
            boundary
        )
        .unwrap()
        .model,
        "outside"
    );
}

#[test]
fn request_capabilities_filter_before_alias_ranking() {
    let (_temporary, runtime, config) = fixture();
    let mut supported = config.clone();
    supported
        .providers
        .get_mut("p")
        .unwrap()
        .models
        .get_mut("m")
        .unwrap()
        .overrides
        .tool_call = Some(false);
    supported
        .providers
        .get_mut("q")
        .unwrap()
        .models
        .get_mut("m")
        .unwrap()
        .overrides
        .tool_call = Some(true);
    publish(&runtime, &config, &supported, "capabilities");
    let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
    let headers = native_headers(at);
    let body = json!({"messages":[{"role":"user","content":"x"}],"tools":[{"type":"function","function":{"name":"test","parameters":{"type":"object"}}}],"temperature":0,"extension":{"enabled":false}});
    let original = body.clone();
    let choice = runtime
        .choose(
            &test_snapshot(&supported, "capabilities"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            at + chrono::Duration::milliseconds(1),
        )
        .unwrap();
    assert_eq!(choice.selection.provider, "q");
    assert!(
        runtime
            .choose(
                &test_snapshot(&supported, "capabilities"),
                "p/m",
                "chat-completions",
                &body,
                &headers,
                at + chrono::Duration::milliseconds(1)
            )
            .is_err()
    );
    assert_eq!(body, original);
}
