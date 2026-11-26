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
        protocols: vec!["chat-completions".into()],
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

/// Ledger rows of one isolated store, read after the runtime writer has flushed.
pub(super) fn usage_rows(store: &Store) -> Vec<serde_json::Value> {
    let Some(connection) = crate::usage::ledger::open_read(store).unwrap() else {
        return Vec::new();
    };
    let mut statement = connection
        .prepare(
            "SELECT received_at_ms, admitted_at_ms, membership_ms, origin, requested_model,
                    model_kind, provider, qualified_model, target, agent_provider, protocol,
                    outcome, stage, reason_code, input_tokens, output_tokens,
                    cache_read_tokens, cache_write_tokens, usage_state, duration_ms,
                    cost_micros, price_source, cost_state
             FROM usage_record ORDER BY id",
        )
        .unwrap();
    let columns: Vec<String> = statement
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let mut rows = Vec::new();
    let mut query = statement.query([]).unwrap();
    while let Some(row) = query.next().unwrap() {
        let mut values = serde_json::Map::new();
        for (index, column) in columns.iter().enumerate() {
            let value = match row.get_ref(index).unwrap() {
                rusqlite::types::ValueRef::Null => serde_json::Value::Null,
                rusqlite::types::ValueRef::Integer(number) => serde_json::json!(number),
                rusqlite::types::ValueRef::Real(number) => serde_json::json!(number),
                rusqlite::types::ValueRef::Text(text) => {
                    serde_json::Value::String(String::from_utf8_lossy(text).into_owned())
                }
                rusqlite::types::ValueRef::Blob(_) => serde_json::Value::Null,
            };
            values.insert(column.clone(), value);
        }
        rows.push(serde_json::Value::Object(values));
    }
    rows
}
