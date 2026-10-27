use super::*;
use crate::gateway::test_support::{fixture, native_headers, publish, test_snapshot};

#[test]
fn opencode_first_use_and_resume_keep_the_selected_destination() {
    let (_temporary, runtime, config) = fixture();
    let now = Utc::now();
    let mut headers = HeaderMap::new();
    headers.insert(
        "user-agent",
        "opencode/stable/2.0.19/opencode".parse().unwrap(),
    );
    headers.insert(
        "x-opencode-session",
        "ses_existing_native_conversation".parse().unwrap(),
    );
    headers.insert("x-session-id", headers["x-opencode-session"].clone());
    // A resumed conversation may predate tmax and carry multiple turns.
    let body = json!({"messages":[{"role":"user","content":"first"},{"role":"assistant","content":"answer"},{"role":"user","content":"continue"}]});
    let first = runtime
        .choose(
            &test_snapshot(&config, "fixture-1"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            now,
        )
        .unwrap();
    assert_eq!(first.selection.provider, "p");
    assert_eq!(first.go_session_header, Some("x-opencode-session"));
    let mut preferred = config.clone();
    preferred
        .providers
        .get_mut("q")
        .unwrap()
        .models
        .get_mut("m")
        .unwrap()
        .priority = 100;
    publish(&runtime, &config, &preferred, "priority-change");
    let restarted = Runtime::new(runtime.store.clone(), "restart".into()).unwrap();
    let retained = restarted
        .choose(
            &test_snapshot(&preferred, "priority-change"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            now,
        )
        .unwrap();
    assert_eq!(retained.selection.provider, "p");
    headers.insert(
        "x-opencode-session",
        "ses_second_native_conversation".parse().unwrap(),
    );
    headers.insert("x-session-id", headers["x-opencode-session"].clone());
    let fresh = restarted
        .choose(
            &test_snapshot(&preferred, "priority-change"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            now,
        )
        .unwrap();
    assert_eq!(fresh.selection.provider, "q");
    headers.insert(
        "x-opencode-session",
        "ses_unknown_remote_continuation".parse().unwrap(),
    );
    headers.insert("x-session-id", headers["x-opencode-session"].clone());
    let remote = json!({"messages":[{"role":"user","content":"continue"}],"previous_response_id":"upstream-owned"});
    assert_eq!(
        restarted
            .choose(
                &test_snapshot(&preferred, "priority-change"),
                "dynamic",
                "chat-completions",
                &remote,
                &headers,
                now
            )
            .err()
            .unwrap()
            .code,
        "original_binding_unavailable"
    );
    let files = json!({"messages":[{"role":"user","content":[{"type":"file","file":{"file_id":"upstream-owned"}}]}]});
    assert_eq!(
        restarted
            .choose(
                &test_snapshot(&preferred, "priority-change"),
                "dynamic",
                "chat-completions",
                &files,
                &headers,
                now
            )
            .err()
            .unwrap()
            .code,
        "original_resource_binding_unavailable"
    );
    headers.insert("x-session-id", "conflicting".parse().unwrap());
    assert_eq!(
        restarted
            .choose(
                &test_snapshot(&preferred, "priority-change"),
                "dynamic",
                "chat-completions",
                &body,
                &headers,
                now
            )
            .err()
            .unwrap()
            .code,
        "session_identity_unavailable"
    );
}

#[tokio::test]
async fn retained_session_explanation_survives_deleted_candidates_and_never_clears_blockers() {
    let (directory, runtime, config) = fixture();
    let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
    let choice = runtime
        .choose(
            &test_snapshot(&config, "fixture-1"),
            "dynamic",
            "chat-completions",
            &json!({"messages":[{"role":"user","content":"not-retained-body"}]}),
            &native_headers(at),
            at + chrono::Duration::milliseconds(1),
        )
        .unwrap();
    let handle = choice.selection.evidence["scope_handle"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(handle.starts_with("native-scope:"));
    let mut removed = config.clone();
    removed.providers.remove("p");
    removed.model_aliases.clear();
    removed.agents.insert(
        "changed-client".into(),
        crate::config::AgentTarget {
            agent: "codex".into(),
            scope: directory.path().join("native"),
            providers: [(
                "gateway".into(),
                crate::config::NativeProvider {
                    provider: None,
                    gateway: Some(true),
                    models: vec![],
                },
            )]
            .into(),
            default: None,
            roles: Default::default(),
        },
    );
    publish(&runtime, &config, &removed, "removed");
    runtime
        .store
        .write_raw("tmax.toml", toml::to_string(&removed).unwrap().as_bytes())
        .unwrap();
    let before = std::fs::read(runtime.store.root.join("session-bindings.json")).unwrap();
    let request = crate::Request {
        command: vec!["model".into(), "explain".into()],
        operands: vec!["dynamic".into()],
        arguments: [
            ("protocol".into(), vec!["chat-completions".into()]),
            ("session".into(), vec![handle.clone()]),
            ("target".into(), vec!["changed-client".into()]),
            ("agent-provider".into(), vec!["gateway".into()]),
        ]
        .into(),
        dry_run: false,
        if_revision: None,
    };
    let evidence = crate::operations::explain(&runtime.store, &removed, &request)
        .await
        .unwrap();
    assert_eq!(evidence["selection"]["qualified_model"], "p/m");
    assert_eq!(evidence["continuation"]["reason_code"], "provider_removed");
    assert_eq!(
        evidence["continuation"]["stored_blocker"],
        "provider_removed"
    );
    assert_eq!(evidence["fresh_ranking"], "not_evaluated");
    assert_eq!(evidence["native_assessment"]["state"], "not_reassessed");
    assert!(!evidence.to_string().contains("not-retained-body"));
    assert_eq!(
        std::fs::read(runtime.store.root.join("session-bindings.json")).unwrap(),
        before
    );
    for (id, protocol, session) in [
        ("different", "chat-completions", handle.as_str()),
        ("dynamic", "responses", handle.as_str()),
        ("dynamic", "chat-completions", "unrelated-handle"),
    ] {
        assert_eq!(
            Runtime::inspect_persisted_session(&runtime.store, &removed, id, protocol, session)
                .unwrap_err()
                .code,
            "original_binding_unavailable"
        );
    }
    let restored = Runtime::inspect_persisted_session(
        &runtime.store,
        &config,
        "dynamic",
        "chat-completions",
        &handle,
    )
    .unwrap();
    assert_eq!(
        restored["continuation"]["stored_blocker"],
        "provider_removed"
    );
    assert_eq!(restored["selection"]["qualified_model"], "p/m");
}

#[test]
fn retained_selection_survives_priority_change_history_clear_and_restart() {
    let (_temporary, runtime, config) = fixture();
    let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
    let headers = native_headers(at);
    let body = json!({"messages":[{"role":"user","content":"x"}]});
    let first = runtime
        .choose(
            &test_snapshot(&config, "fixture-1"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            at + chrono::Duration::milliseconds(1),
        )
        .unwrap();
    assert_eq!(first.selection.provider, "p");
    let original = first.selection.evidence.clone();
    let mut updated = config.clone();
    updated
        .providers
        .get_mut("q")
        .unwrap()
        .models
        .get_mut("m")
        .unwrap()
        .priority = 100;
    publish(&runtime, &config, &updated, "fixture-2");
    runtime.history.clear(false);
    let continuation = json!({"messages":[{"role":"user","content":"x"},{"role":"assistant","content":"y"},{"role":"user","content":"z"}]});
    let retained = runtime
        .choose(
            &test_snapshot(&updated, "fixture-2"),
            "dynamic",
            "chat-completions",
            &continuation,
            &headers,
            at + chrono::Duration::seconds(1),
        )
        .unwrap();
    assert_eq!(retained.selection.provider, "p");
    assert_eq!(
        retained.selection.evidence["selection_reason"],
        "retained_destination"
    );
    assert_eq!(first.selection.evidence, original);
    let store = runtime.store.clone();
    drop(runtime);
    let restarted = Runtime::new(store, "restart".into()).unwrap();
    let after = restarted
        .choose(
            &test_snapshot(&updated, "fixture-2"),
            "dynamic",
            "chat-completions",
            &continuation,
            &headers,
            at + chrono::Duration::seconds(2),
        )
        .unwrap();
    assert_eq!(after.selection.provider, "p");
    let fresh = native_headers(at + chrono::Duration::seconds(3));
    let new = restarted
        .choose(
            &test_snapshot(&updated, "fixture-2"),
            "dynamic",
            "chat-completions",
            &body,
            &fresh,
            at + chrono::Duration::seconds(4),
        )
        .unwrap();
    assert_eq!(new.selection.provider, "q");
}
#[test]
fn persisted_binding_preview_reports_blockers_without_publication() {
    let (_temporary, runtime, config) = fixture();
    let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
    let headers = native_headers(at);
    let body = json!({"messages":[{"role":"user","content":"x"}]});
    runtime
        .choose(
            &test_snapshot(&config, "fixture-1"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            at + chrono::Duration::milliseconds(1),
        )
        .unwrap();
    let bindings = std::fs::read(runtime.store.root.join("session-bindings.json")).unwrap();
    let routing = std::fs::read(runtime.store.root.join("routing-state.json")).unwrap();
    let mut disabled = config.clone();
    disabled.model_aliases[0].enabled = false;
    let impact = Runtime::preview_persisted(&runtime.store, &disabled).unwrap();
    assert_eq!(
        impact["blocked_sessions"][0]["reason_code"],
        "alias_disabled"
    );
    assert_eq!(
        std::fs::read(runtime.store.root.join("session-bindings.json")).unwrap(),
        bindings
    );
    assert_eq!(
        std::fs::read(runtime.store.root.join("routing-state.json")).unwrap(),
        routing
    );
    assert_eq!(runtime.binding_summary()["blocked"], 0);
}
#[test]
fn failed_binding_activation_preserves_live_state_and_retry_persists_blockers() {
    let (_temporary, runtime, config) = fixture();
    let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
    let headers = native_headers(at);
    let body = json!({"messages":[{"role":"user","content":"x"}]});
    runtime
        .choose(
            &test_snapshot(&config, "fixture-1"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            at + chrono::Duration::milliseconds(1),
        )
        .unwrap();
    let before = serde_json::to_value(&*runtime.bindings.lock().unwrap()).unwrap();
    let path = runtime.store.root.join("session-bindings.json");
    let original = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let mut disabled = config.clone();
    disabled.model_aliases[0].enabled = false;
    crate::routing::reconcile(&runtime.store, &config, &disabled, &[]).unwrap();
    crate::inputs::activate(&runtime.store, &disabled, "disabled").unwrap();
    assert!(runtime.activate_bindings(&disabled, false).is_err());
    assert_eq!(
        serde_json::to_value(&*runtime.bindings.lock().unwrap()).unwrap(),
        before
    );
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, original).unwrap();
    let impact = runtime.activate_bindings(&disabled, false).unwrap();
    assert_eq!(
        impact["blocked_sessions"][0]["reason_code"],
        "alias_disabled"
    );
    let restarted = Runtime::new(runtime.store.clone(), "retry".into()).unwrap();
    let error = restarted
        .choose(
            &test_snapshot(&config, "fixture-1"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            at + chrono::Duration::seconds(1),
        )
        .err()
        .unwrap();
    assert_eq!(error.code, "alias_disabled");
}
#[test]
fn stateless_activation_cannot_bypass_a_retained_session_block() {
    let (_temporary, runtime, config) = fixture();
    let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
    let headers = native_headers(at);
    let body = json!({"messages":[{"role":"user","content":"x"}]});
    let first = runtime
        .choose(
            &test_snapshot(&config, "fixture-1"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            at + chrono::Duration::milliseconds(1),
        )
        .unwrap();
    assert_eq!(first.selection.provider, "p");
    let mut stateless = config.clone();
    stateless.model_aliases[0].continuity = Some("stateless".into());
    stateless
        .providers
        .get_mut("q")
        .unwrap()
        .models
        .get_mut("m")
        .unwrap()
        .priority = 100;
    publish(&runtime, &config, &stateless, "stateless");
    let error = runtime
        .choose(
            &test_snapshot(&stateless, "stateless"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            at + chrono::Duration::seconds(1),
        )
        .err()
        .unwrap();
    assert_eq!(error.code, "continuity_changed");
    let fresh = native_headers(at + chrono::Duration::seconds(2));
    let choice = runtime
        .choose(
            &test_snapshot(&stateless, "stateless"),
            "dynamic",
            "chat-completions",
            &body,
            &fresh,
            at + chrono::Duration::seconds(3),
        )
        .unwrap();
    assert_eq!(choice.selection.provider, "q");
}
#[test]
fn incompatible_activation_block_is_permanent_without_reassignment() {
    let (_temporary, runtime, config) = fixture();
    let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
    let headers = native_headers(at);
    let body = json!({"messages":[{"role":"user","content":"x"}]});
    runtime
        .choose(
            &test_snapshot(&config, "fixture-1"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            at + chrono::Duration::milliseconds(1),
        )
        .unwrap();
    let mut removed = config.clone();
    removed.model_aliases[0].candidates = vec!["q/m".into()];
    publish(&runtime, &config, &removed, "removed");
    publish(&runtime, &removed, &config, "restored");
    let store = runtime.store.clone();
    let bindings_before = std::fs::read(store.root.join("session-bindings.json")).unwrap();
    let mut legacy = store.read_json::<Value>("routing-state.json").unwrap();
    for view in ["saved", "loaded"] {
        for provider in legacy[view]["config"]["providers"]
            .as_object_mut()
            .unwrap()
            .values_mut()
        {
            provider["models"]["m"]["tier"] = json!("L0");
        }
        legacy[view]["config"]["model_aliases"][0]["tier"] = json!("L0");
        legacy[view]["config"]["model_aliases"][0]["candidates"] = Value::Null;
    }
    store.write_json("routing-state.json", &legacy).unwrap();
    drop(runtime);
    crate::routing::reconcile(&store, &config, &config, &[]).unwrap();
    assert_eq!(
        std::fs::read(store.root.join("session-bindings.json")).unwrap(),
        bindings_before
    );
    let runtime = Runtime::new(store, "after-historical-cutover".into()).unwrap();
    let error = runtime
        .choose(
            &test_snapshot(&config, "restored"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            at + chrono::Duration::seconds(1),
        )
        .err()
        .unwrap();
    assert_eq!(error.code, "candidate_removed");
    assert_eq!(runtime.bindings.lock().unwrap().entries.len(), 1);
}

#[test]
fn omp_chat_history_can_use_stateless_alias_but_not_session_or_remote_state() {
    let (_temporary, runtime, config) = fixture();
    let mut stateless = config.clone();
    stateless.model_aliases[0].continuity = Some("stateless".into());
    publish(&runtime, &config, &stateless, "omp-stateless");
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", "omp/18.12.6".parse().unwrap());
    let body = json!({"messages":[
        {"role":"user","content":"first"},
        {"role":"assistant","content":"answer"},
        {"role":"user","content":"continue"}
    ]});
    let choice = runtime
        .choose(
            &test_snapshot(&stateless, "omp-stateless"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            Utc::now(),
        )
        .unwrap();
    assert_eq!(choice.selection.provider, "p");
    assert!(runtime.bindings.lock().unwrap().entries.is_empty());
    let error = runtime
        .choose(
            &test_snapshot(&config, "omp-session"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            Utc::now(),
        )
        .err()
        .unwrap();
    assert_eq!(error.code, "session_identity_unavailable");
    for rejected in [
        json!({"messages":[]}),
        json!({"messages":[{"role":"user","content":"x"}],"previous_response_id":"resp_1"}),
    ] {
        let error = runtime
            .choose(
                &test_snapshot(&stateless, "omp-stateless"),
                "dynamic",
                "chat-completions",
                &rejected,
                &headers,
                Utc::now(),
            )
            .err()
            .unwrap();
        assert_eq!(error.code, "independent_context_required");
    }
    headers.insert("x-session-id", "unqualified-session".parse().unwrap());
    let error = runtime
        .choose(
            &test_snapshot(&stateless, "omp-stateless"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            Utc::now(),
        )
        .err()
        .unwrap();
    assert_eq!(error.code, "independent_context_required");
}
