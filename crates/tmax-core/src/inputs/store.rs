use crate::{Error, Result, config, storage::Store};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use super::model::Runtime;

pub(super) const STATE: &str = "routing-state.json";

fn state_schema_error() -> Error {
    Error::new(
        "invalid_record",
        "Retained input state fails its storage schema",
    )
}

// Stored-data cutover only: never accept these fields in desired configuration.
// Membership comes exclusively from this historical snapshot, including disabled
// models and empty memberships. Reads do not rewrite the retained file.
fn normalize_historical_snapshot(snapshot: &mut Value) -> Result<bool> {
    let snapshot = snapshot.as_object_mut().ok_or_else(state_schema_error)?;
    let mut membership: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut changed = false;
    if let Some(providers) = snapshot.get_mut("providers") {
        for (provider_id, provider) in providers.as_object_mut().ok_or_else(state_schema_error)? {
            let provider = provider.as_object_mut().ok_or_else(state_schema_error)?;
            if let Some(models) = provider.get_mut("models") {
                for (model_id, model) in models.as_object_mut().ok_or_else(state_schema_error)? {
                    let model = model.as_object_mut().ok_or_else(state_schema_error)?;
                    if let Some(tier) = model.remove("tier") {
                        changed = true;
                        match tier {
                            Value::Null => {}
                            Value::String(tier)
                                if matches!(tier.as_str(), "L0" | "L1" | "L2" | "L3") =>
                            {
                                membership
                                    .entry(tier)
                                    .or_default()
                                    .push(format!("{provider_id}/{model_id}"));
                            }
                            _ => return Err(state_schema_error()),
                        }
                    }
                }
            }
        }
    }
    if let Some(aliases) = snapshot.get_mut("model_aliases") {
        for alias in aliases.as_array_mut().ok_or_else(state_schema_error)? {
            let alias = alias.as_object_mut().ok_or_else(state_schema_error)?;
            let Some(tier) = alias.remove("tier") else {
                continue;
            };
            changed = true;
            match tier {
                Value::Null => {
                    if alias
                        .get("candidates")
                        .and_then(Value::as_array)
                        .is_none_or(Vec::is_empty)
                    {
                        return Err(state_schema_error());
                    }
                }
                Value::String(tier) if matches!(tier.as_str(), "L0" | "L1" | "L2" | "L3") => {
                    if alias
                        .get("candidates")
                        .is_some_and(|candidates| !candidates.is_null())
                    {
                        return Err(state_schema_error());
                    }
                    let candidates = membership
                        .get(&tier)
                        .into_iter()
                        .flatten()
                        .cloned()
                        .map(Value::String)
                        .collect();
                    alias.insert("candidates".into(), Value::Array(candidates));
                }
                _ => return Err(state_schema_error()),
            }
        }
    }
    Ok(changed)
}

fn normalize_historical_view(view: &mut Value) -> Result<bool> {
    let view = view.as_object_mut().ok_or_else(state_schema_error)?;
    let mut changed =
        normalize_historical_snapshot(view.get_mut("config").ok_or_else(state_schema_error)?)?;
    if let Some(providers) = view.get_mut("providers") {
        for provider in providers
            .as_object_mut()
            .ok_or_else(state_schema_error)?
            .values_mut()
        {
            let provider = provider.as_object_mut().ok_or_else(state_schema_error)?;
            if let Some(models) = provider.remove("offerings") {
                if provider.contains_key("models") {
                    return Err(state_schema_error());
                }
                provider.insert("models".into(), models);
                changed = true;
            }
        }
    }
    Ok(changed)
}

// Keep serde's duplicate-struct-field rejection when the historical fallback
// materializes JSON values. This validation leaves arbitrary-precision number
// parsing to serde_json and uses its normal recursion bound.
pub(crate) fn check_historical_field_uniqueness(bytes: &[u8]) -> Result<()> {
    struct Fields;
    impl<'de> serde::de::DeserializeSeed<'de> for Fields {
        type Value = ();
        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> std::result::Result<(), D::Error> {
            deserializer.deserialize_any(self)
        }
    }
    impl<'de> serde::de::Visitor<'de> for Fields {
        type Value = ();
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("retained JSON with unique object fields")
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_str<E: serde::de::Error>(self, _: &str) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> std::result::Result<(), A::Error> {
            while sequence.next_element_seed(Fields)?.is_some() {}
            Ok(())
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<(), A::Error> {
            let mut fields = BTreeSet::new();
            while let Some(field) = map.next_key::<String>()? {
                if !fields.insert(field) {
                    return Err(serde::de::Error::custom("Duplicate retained state field"));
                }
                map.next_value_seed(Fields)?;
            }
            Ok(())
        }
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    serde::de::DeserializeSeed::deserialize(Fields, &mut deserializer)
        .map_err(|_| state_schema_error())?;
    deserializer.end().map_err(|_| state_schema_error())
}

fn decode_runtime(bytes: &[u8]) -> Result<Runtime> {
    if let Ok(runtime) = serde_json::from_slice(bytes) {
        return Ok(runtime);
    }
    check_historical_field_uniqueness(bytes)?;
    let mut document: Value = serde_json::from_slice(bytes).map_err(|_| state_schema_error())?;
    let saved =
        normalize_historical_view(document.get_mut("saved").ok_or_else(state_schema_error)?)?;
    let loaded = match document.get_mut("loaded") {
        None | Some(Value::Null) => false,
        Some(view) => normalize_historical_view(view)?,
    };
    if !saved && !loaded {
        return Err(state_schema_error());
    }
    let runtime: Runtime = serde_json::from_value(document).map_err(|_| state_schema_error())?;
    if saved {
        config::validate_retained_snapshot(&runtime.saved.config)
            .map_err(|_| state_schema_error())?;
    }
    if loaded {
        config::validate_retained_snapshot(
            &runtime
                .loaded
                .as_ref()
                .ok_or_else(state_schema_error)?
                .config,
        )
        .map_err(|_| state_schema_error())?;
    }
    Ok(runtime)
}

pub(crate) fn read_runtime(store: &Store) -> Result<Runtime> {
    match crate::storage::read_optional(&store.path(STATE)?)? {
        None => Ok(Runtime::default()),
        Some(bytes) => decode_runtime(&bytes),
    }
}
pub(super) fn write_runtime(store: &Store, state: &Runtime) -> Result<()> {
    store.write_json(STATE, state)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::{
        exact::{ExactInstant, instant},
        lifecycle::{activate, deactivate, reconcile},
        snapshot::Snapshot,
        test_support::{fixture, retain_value, save_intent},
    };
    use super::*;
    use crate::config::Config;
    fn historical_config(config: &Config) -> Value {
        let mut snapshot = serde_json::to_value(config).unwrap();
        for provider in snapshot["providers"].as_object_mut().unwrap().values_mut() {
            for model in provider["models"].as_object_mut().unwrap().values_mut() {
                model["tier"] = Value::Null;
            }
        }
        for alias in snapshot["model_aliases"].as_array_mut().unwrap() {
            alias["tier"] = Value::Null;
        }
        snapshot
    }

    #[test]
    fn historical_model_identity_maps_preserve_both_views_and_retained_facts() {
        let (_directory, store, config) = fixture();
        retain_value(&store, &config);
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let before = read_runtime(&store).unwrap();
        let expected = serde_json::to_value(&before).unwrap();
        let identity = Snapshot::capture_loaded(&store, &config, None)
            .destination_identity("p", "m")
            .unwrap();
        let mut historical = expected.clone();
        for view in ["saved", "loaded"] {
            let provider = historical[view]["providers"]["p"].as_object_mut().unwrap();
            let models = provider.remove("models").unwrap();
            provider.insert("offerings".into(), models);
        }
        store.write_json(STATE, &historical).unwrap();
        let bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(read_runtime(&store).unwrap()).unwrap(),
            expected
        );
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).destination_identity("p", "m"),
            Some(identity.clone())
        );
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).input("p", "i", &at)["availability"]["state"],
            "available"
        );
        assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), bytes);
        reconcile(&store, &config, &config, &[]).unwrap();
        assert_eq!(store.read_json::<Value>(STATE).unwrap(), expected);
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).destination_identity("p", "m"),
            Some(identity)
        );
    }

    #[test]
    fn historical_model_identity_map_rejects_ambiguous_and_duplicate_keys() {
        let (_directory, store, _config) = fixture();
        let current = serde_json::to_value(read_runtime(&store).unwrap()).unwrap();
        let mut ambiguous = current.clone();
        ambiguous["saved"]["providers"]["p"]["offerings"] =
            ambiguous["saved"]["providers"]["p"]["models"].clone();
        assert!(decode_runtime(&serde_json::to_vec(&ambiguous).unwrap()).is_err());
        let mut historical = current;
        let provider = historical["saved"]["providers"]["p"]
            .as_object_mut()
            .unwrap();
        let models = provider.remove("models").unwrap();
        provider.insert("offerings".into(), models);
        provider.insert("unexpected".into(), Value::Null);
        assert!(decode_runtime(&serde_json::to_vec(&historical).unwrap()).is_err());
        historical["saved"]["providers"]["p"]
            .as_object_mut()
            .unwrap()
            .remove("unexpected");
        let encoded = serde_json::to_string(&historical).unwrap();
        let duplicate = encoded.replacen("\"offerings\":", "\"offerings\":{},\"offerings\":", 1);
        assert!(decode_runtime(duplicate.as_bytes()).is_err());
    }

    #[test]
    fn historical_null_fields_preserve_explicit_candidates_lineages_and_input_heads() {
        let (_directory, store, original) = fixture();
        let mut config = original.clone();
        config.model_aliases.push(config::ModelAlias {
            candidate_reasoning_effort: Default::default(),
            id: "custom-name".into(),
            protocol: "chat-completions".into(),
            candidates: vec!["p/m".into()],
            enabled: true,
            continuity: None,
            fallback: None,
        });
        save_intent(&store, &config);
        reconcile(&store, &original, &config, &[]).unwrap();
        retain_value(&store, &config);
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let before = read_runtime(&store).unwrap();
        let expected = serde_json::to_value(&before).unwrap();
        let mut legacy = expected.clone();
        legacy["saved"]["config"] = historical_config(&before.saved.config);
        legacy["loaded"]["config"] = historical_config(&before.loaded.as_ref().unwrap().config);
        store.write_json(STATE, &legacy).unwrap();
        let original_bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(read_runtime(&store).unwrap()).unwrap(),
            expected
        );
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).input("p", "i", &at)["availability"]["state"],
            "available"
        );
        assert_eq!(
            std::fs::read(store.path(STATE).unwrap()).unwrap(),
            original_bytes
        );
        reconcile(&store, &config, &config, &[]).unwrap();
        assert_eq!(store.read_json::<Value>(STATE).unwrap(), expected);
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).input("p", "i", &at)["availability"]["state"],
            "available"
        );
    }

    #[test]
    fn historical_membership_uses_each_snapshot_and_keeps_empty_aliases_unavailable() {
        let (_directory, store, original) = fixture();
        let mut config = original.clone();
        config.providers.get_mut("p").unwrap().models.insert(
            "disabled".into(),
            config::Model {
                enabled: false,
                ..Default::default()
            },
        );
        config
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .insert("other".into(), config::Model::default());
        for id in ["historical", "same-source", "empty"] {
            config.model_aliases.push(config::ModelAlias {
                candidate_reasoning_effort: Default::default(),
                id: id.into(),
                protocol: "chat-completions".into(),
                candidates: vec!["p/m".into()],
                enabled: true,
                continuity: None,
                fallback: None,
            });
        }
        save_intent(&store, &config);
        reconcile(&store, &original, &config, &[]).unwrap();
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let before = read_runtime(&store).unwrap();
        let mut legacy = serde_json::to_value(&before).unwrap();
        for view in ["saved", "loaded"] {
            legacy[view]["config"] = historical_config(&config);
            let snapshot = &mut legacy[view]["config"];
            snapshot["providers"]["p"]["models"]["m"]["tier"] =
                json!(if view == "saved" { "L0" } else { "L1" });
            snapshot["providers"]["p"]["models"]["disabled"]["tier"] = json!("L0");
            snapshot["providers"]["p"]["models"]["other"]["tier"] =
                json!(if view == "saved" { "L1" } else { "L0" });
            if view == "loaded" {
                snapshot["providers"]["p"]["models"]["disabled"]["tier"] = Value::Null;
            }
            for (index, tier) in [(0, "L0"), (1, "L0"), (2, "L3")] {
                snapshot["model_aliases"][index]["tier"] = json!(tier);
                snapshot["model_aliases"][index]["candidates"] = Value::Null;
            }
        }
        store.write_json(STATE, &legacy).unwrap();
        let bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
        let mut future = config.clone();
        future
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .insert("new".into(), config::Model::default());
        future.model_aliases[0].candidates = vec!["p/new".into()];
        save_intent(&store, &future);
        let restored = read_runtime(&store).unwrap();
        assert_eq!(
            restored.saved.config.model_aliases[0].candidates,
            ["p/disabled", "p/m"]
        );
        assert_eq!(
            restored.saved.config.model_aliases[1].candidates,
            ["p/disabled", "p/m"]
        );
        assert_eq!(
            restored.loaded.as_ref().unwrap().config.model_aliases[0].candidates,
            ["p/other"]
        );
        assert!(restored.saved.config.model_aliases[2].candidates.is_empty());
        assert_eq!(
            serde_json::to_value(&restored.saved.providers).unwrap(),
            serde_json::to_value(&before.saved.providers).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&restored.loaded.as_ref().unwrap().providers).unwrap(),
            serde_json::to_value(&before.loaded.as_ref().unwrap().providers).unwrap()
        );
        assert!(config::validate(&restored.saved.config).is_err());
        assert_eq!(
            crate::routing::select(
                &store,
                &restored.saved.config,
                "empty",
                "chat-completions",
                instant("2026-10-23T10:30:00Z").unwrap()
            )
            .unwrap_err()
            .code,
            "model_unavailable"
        );
        assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), bytes);
        deactivate(&store).unwrap();
        let canonical = read_runtime(&store).unwrap();
        assert!(
            canonical.saved.config.model_aliases[2]
                .candidates
                .is_empty()
        );
        assert!(canonical.loaded.is_none());
    }

    #[test]
    fn corrupt_historical_sources_fail_closed_without_rewriting_retained_state() {
        let (_directory, store, original) = fixture();
        let mut config = original.clone();
        config.model_aliases.push(config::ModelAlias {
            candidate_reasoning_effort: Default::default(),
            id: "historical".into(),
            protocol: "chat-completions".into(),
            candidates: vec!["p/m".into()],
            enabled: true,
            continuity: None,
            fallback: None,
        });
        save_intent(&store, &config);
        reconcile(&store, &original, &config, &[]).unwrap();
        let runtime = read_runtime(&store).unwrap();
        let mut legacy = serde_json::to_value(runtime).unwrap();
        legacy["saved"]["config"] = historical_config(&config);
        for (tier, candidates) in [
            (json!("L0"), json!(["p/m"])),
            (Value::Null, Value::Null),
            (Value::Null, json!([])),
            (Value::Null, json!(["p/missing"])),
            (Value::Null, json!(["p/m", "p/m"])),
            (json!("L9"), Value::Null),
            (json!(false), Value::Null),
        ] {
            let mut invalid = legacy.clone();
            invalid["saved"]["config"]["model_aliases"][0]["tier"] = tier;
            invalid["saved"]["config"]["model_aliases"][0]["candidates"] = candidates;
            store.write_json(STATE, &invalid).unwrap();
            let bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
            assert_eq!(read_runtime(&store).unwrap_err().code, "invalid_record");
            assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), bytes);
        }
        for invalid_tier in [json!("L9"), json!(true)] {
            let mut invalid = legacy.clone();
            invalid["saved"]["config"]["providers"]["p"]["models"]["m"]["tier"] = invalid_tier;
            store.write_json(STATE, &invalid).unwrap();
            assert_eq!(read_runtime(&store).unwrap_err().code, "invalid_record");
        }
        let duplicate = serde_json::to_string(&legacy).unwrap().replacen(
            "\"identity\":",
            "\"identity\":\"tampered\",\"identity\":",
            1,
        );
        store.write_raw(STATE, duplicate.as_bytes()).unwrap();
        assert_eq!(read_runtime(&store).unwrap_err().code, "invalid_record");
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(
            Snapshot::capture(&store, &config).input("p", "i", &at)["availability"]["reason_code"],
            "invalid_record"
        );
        assert_eq!(
            std::fs::read(store.path(STATE).unwrap()).unwrap(),
            duplicate.as_bytes()
        );
        legacy["saved"]["config"]["providers"]["p"]["models"]["m"]["unexpected"] = json!(true);
        store.write_json(STATE, &legacy).unwrap();
        assert_eq!(read_runtime(&store).unwrap_err().code, "invalid_record");
    }
}
