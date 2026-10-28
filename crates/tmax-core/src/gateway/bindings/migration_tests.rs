use super::*;
use crate::gateway::test_support::{fixture, native_headers, test_snapshot};

#[test]
fn historical_names_preserve_session_identity_evidence_and_permanent_blockers() {
    for blocker in [None, Some("offering_removed"), Some("offering_disabled")] {
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
        let mut historical = serde_json::to_value(&*runtime.bindings.lock().unwrap()).unwrap();
        let entry = historical["entries"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        let selected_identity = entry["model_identity"].clone();
        let authorization = entry["authorization_generation"].clone();
        let provider_identity = entry["provider_identity"].clone();
        let original_decision = entry["decision"].clone();
        let entry_object = entry.as_object_mut().unwrap();
        let identity = entry_object.remove("model_identity").unwrap();
        entry_object.insert("offering_identity".into(), identity);
        entry_object.insert("blocked".into(), json!(blocker));
        let selection = entry_object.get_mut("decision").unwrap()["selection"]
            .as_object_mut()
            .unwrap();
        let identity = selection.remove("model_identity").unwrap();
        selection.insert("offering_identity".into(), identity);
        runtime
            .store
            .write_json("session-bindings.json", &historical)
            .unwrap();
        let bytes = std::fs::read(runtime.store.root.join("session-bindings.json")).unwrap();
        let mut reranked = config.clone();
        reranked
            .providers
            .get_mut("q")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .priority = 100;
        crate::routing::reconcile(&runtime.store, &config, &reranked, &[]).unwrap();
        crate::inputs::activate(&runtime.store, &reranked, "reranked").unwrap();
        let store = runtime.store.clone();
        drop(runtime);
        let restarted = Runtime::new(store, "migrated".into()).unwrap();
        assert_eq!(
            std::fs::read(restarted.store.root.join("session-bindings.json")).unwrap(),
            bytes
        );
        let normalized = serde_json::to_value(&*restarted.bindings.lock().unwrap()).unwrap();
        let entry = normalized["entries"]
            .as_object()
            .unwrap()
            .values()
            .next()
            .unwrap();
        assert_eq!(entry["model_identity"], selected_identity);
        assert_eq!(entry["provider_identity"], provider_identity);
        assert_eq!(entry["authorization_generation"], authorization);
        assert_eq!(entry["decision"], original_decision);
        assert_eq!(entry["decision"]["selection"]["qualified_model"], "p/m");
        let result = restarted.choose(
            &test_snapshot(&reranked, "reranked"),
            "dynamic",
            "chat-completions",
            &body,
            &headers,
            at + chrono::Duration::seconds(1),
        );
        match blocker {
            None => {
                let retained = result.unwrap();
                assert_eq!(retained.selection.provider, first.selection.provider);
                assert_eq!(
                    retained.selection.model_identity,
                    first.selection.model_identity
                );
                assert_eq!(
                    retained.selection.evidence["selected"]["qualified_model"],
                    "p/m"
                );
            }
            Some(old) => {
                let expected = if old == "offering_removed" {
                    "model_removed"
                } else {
                    "model_disabled"
                };
                // The persisted block keeps affinity away from the removed
                // destination, but the request is still served by normal
                // selection over the remaining candidates.
                let choice = result.unwrap();
                assert_ne!(choice.selection.provider, "p");
                assert_eq!(entry["blocked"], expected);
            }
        }
        let bindings = restarted.bindings.lock().unwrap();
        save_bindings(&restarted.store, &bindings).unwrap();
        assert_eq!(
            restarted
                .store
                .read_json::<Value>("session-bindings.json")
                .unwrap(),
            normalized
        );
    }
}
