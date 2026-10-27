//! Shared in-memory and isolated filesystem fixtures for gateway feature tests.
use super::{Runtime, Snapshot};
use crate::{
    config::{Auth, Config},
    storage::Store,
};
use axum::http::HeaderMap;
use chrono::Utc;
use std::sync::Arc;
use zeroize::Zeroizing;

pub(super) const GO_URL: &str = "https://opencode.ai/inference/go/openai/v1";

pub(super) fn native_headers(at: chrono::DateTime<Utc>) -> HeaderMap {
    let millis = at.timestamp_millis();
    let mut bytes = [0u8; 16];
    bytes[..6].copy_from_slice(&millis.to_be_bytes()[2..]);
    bytes[6] = 0x70;
    bytes[8] = 0x80;
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", "pi (linux test; x64)".parse().unwrap());
    headers.insert(
        "x-session-id",
        uuid::Uuid::from_bytes(bytes).to_string().parse().unwrap(),
    );
    headers
}

pub(super) fn fixture() -> (tempfile::TempDir, Arc<Runtime>, Config) {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::at(temporary.path().into());
    let mut config = Config::empty();
    for (id, priority) in [("p", 10), ("q", 1)] {
        let mut provider = crate::config::Provider {
            integration: Some("custom".into()),
            enabled: true,
            ..Default::default()
        };
        provider
            .endpoints
            .insert("chat-completions".into(), "http://127.0.0.1:9/v1".into());
        provider.models.insert(
            "m".into(),
            crate::config::Model {
                enabled: true,
                priority,
                ..Default::default()
            },
        );
        config.providers.insert(id.into(), provider);
    }
    store
        .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
        .unwrap();
    let initial = config.clone();
    crate::routing::reconcile(&store, &Config::empty(), &initial, &[]).unwrap();
    for id in ["p", "q"] {
        let identity = crate::inputs::provider_identity(&store, &initial, id)
            .unwrap()
            .unwrap();
        let reference = crate::secrets::put(
            &store,
            &format!("provider:{identity}:{id}"),
            b"test-provider-key",
        )
        .unwrap();
        config.providers.get_mut(id).unwrap().auth = Some(Auth::ApiKey { key_ref: reference });
    }
    config.model_aliases.push(crate::config::ModelAlias {
        fallback: None,
        candidate_reasoning_effort: Default::default(),
        id: "dynamic".into(),
        protocol: "chat-completions".into(),
        candidates: vec!["p/m".into(), "q/m".into()],
        enabled: true,
        continuity: None,
    });
    crate::routing::reconcile(&store, &initial, &config, &[]).unwrap();
    store
        .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
        .unwrap();
    crate::inputs::activate(&store, &config, "fixture-1").unwrap();
    let runtime = Runtime::new(store, "sessions".into()).unwrap();
    (temporary, runtime, config)
}
pub(super) fn test_snapshot(config: &Config, revision: &str) -> Snapshot {
    Snapshot {
        config: config.clone(),
        revision: revision.into(),
        key: Zeroizing::new(b"test-local-key".to_vec()),
        client: crate::provider_integration::client(config).unwrap(),
        metadata: None,
    }
}
pub(super) fn publish(runtime: &Runtime, old: &Config, new: &Config, revision: &str) {
    crate::routing::reconcile(&runtime.store, old, new, &[]).unwrap();
    let mut authority = runtime.snapshot.write().unwrap();
    crate::inputs::activate(&runtime.store, new, revision).unwrap();
    runtime.activate_bindings(new, false).unwrap();
    *authority = Some(Arc::new(test_snapshot(new, revision)));
}
