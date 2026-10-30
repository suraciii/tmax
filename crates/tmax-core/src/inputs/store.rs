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

fn normalize_historical_window(window: &mut Value) -> Result<bool> {
    let window = window.as_object_mut().ok_or_else(state_schema_error)?;
    let Some(all_day) = window.remove("all_day") else {
        return Ok(false);
    };
    match all_day {
        Value::Null
            if ["start", "end"]
                .into_iter()
                .all(|key| window.get(key).is_some_and(Value::is_string)) => {}
        Value::Bool(true)
            if ["start", "end"]
                .into_iter()
                .all(|key| window.get(key).is_none_or(Value::is_null)) => {}
        _ => return Err(state_schema_error()),
    }
    Ok(true)
}

// Stored-data cutover only: never accept these fields in desired configuration.
// Membership comes exclusively from this historical snapshot, including disabled
// models and empty memberships. Reads do not rewrite the retained file.
fn normalize_historical_snapshot(
    snapshot: &mut Value,
    legacy: &mut BTreeMap<String, String>,
) -> Result<bool> {
    let snapshot = snapshot.as_object_mut().ok_or_else(state_schema_error)?;
    let mut membership: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut changed = false;
    if let Some(network) = snapshot.get_mut("network").filter(|value| !value.is_null()) {
        let policy = network.as_object_mut().ok_or_else(state_schema_error)?;
        let mode = policy.remove("mode");
        let direct = mode.as_ref().is_some_and(|value| value == "direct")
            || (mode.is_none() && policy.get("proxy_url").is_none_or(Value::is_null));
        if direct {
            if policy
                .get("proxy_url")
                .is_some_and(|value| !value.is_null())
                || policy.keys().any(|key| key != "proxy_url")
            {
                return Err(state_schema_error());
            }
            *network = Value::Null;
            changed = true;
        } else if let Some(mode) = mode {
            if mode != "proxy" || !policy.get("proxy_url").is_some_and(Value::is_string) {
                return Err(state_schema_error());
            }
            changed = true;
        }
    }
    if let Some(providers) = snapshot.get_mut("providers") {
        for (provider_id, provider) in providers.as_object_mut().ok_or_else(state_schema_error)? {
            let provider = provider.as_object_mut().ok_or_else(state_schema_error)?;
            if let Some(models) = provider.get_mut("models") {
                for (model_id, model) in models.as_object_mut().ok_or_else(state_schema_error)? {
                    let model = model.as_object_mut().ok_or_else(state_schema_error)?;
                    if let Some(windows) = model.get_mut("schedule") {
                        for window in windows.as_array_mut().ok_or_else(state_schema_error)? {
                            changed |= normalize_historical_window(window)?;
                        }
                    }
                    if let Some(rules) = model.get_mut("priority_rules") {
                        for rule in rules.as_array_mut().ok_or_else(state_schema_error)? {
                            if let Some(window) =
                                rule.get_mut("time_window").filter(|value| !value.is_null())
                            {
                                changed |= normalize_historical_window(window)?;
                            }
                        }
                    }
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
            let region_value = provider.remove("region");
            let region = match region_value.as_ref() {
                None | Some(Value::Null) => None,
                Some(Value::String(region)) => Some(region.as_str()),
                _ => return Err(state_schema_error()),
            };
            let previous: config::Provider =
                serde_json::from_value(Value::Object(provider.clone()))
                    .map_err(|_| state_schema_error())?;
            let historical_id = match previous.integration.as_deref() {
                Some(id) => id,
                None if matches!(
                    previous.metadata.as_deref(),
                    Some("kimi-for-coding" | "kimi-code")
                ) =>
                {
                    "kimi-coding-plan"
                }
                None => crate::provider_integration::integration_id(&previous)
                    .map_err(|_| state_schema_error())?,
            };
            if crate::retained_integration::regional_candidates(historical_id).is_some() {
                let canonical = crate::retained_integration::regional_selection(
                    historical_id,
                    region,
                    &previous.endpoints,
                )
                .ok_or_else(state_schema_error)?;
                legacy.insert(provider_id.clone(), historical_id.to_owned());
                provider.insert("integration".into(), Value::String(canonical.into()));
                provider.insert("endpoints".into(), Value::Object(Default::default()));
                changed = true;
            } else {
                let fixed = crate::provider_integration::fixed_endpoints(historical_id)
                    .map_err(|_| state_schema_error())?;
                if historical_id == "custom" {
                    if region.is_some() {
                        return Err(state_schema_error());
                    }
                } else {
                    if region.is_some_and(|value| {
                        value != "default"
                            || !matches!(
                                historical_id,
                                "deepseek"
                                    | "opencode-go"
                                    | "codex-subscription"
                                    | "glm-coding-plan"
                                    | "stepfun-token-plan"
                            )
                    }) || !previous
                        .endpoints
                        .iter()
                        .all(|(protocol, base)| fixed.get(protocol) == Some(base))
                    {
                        return Err(state_schema_error());
                    }
                    if !previous.endpoints.is_empty() {
                        provider.insert("endpoints".into(), Value::Object(Default::default()));
                        changed = true;
                    }
                }
            }
            changed |= region_value.is_some();
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

fn normalize_historical_view(
    view: &mut Value,
    legacy: &mut BTreeMap<String, String>,
) -> Result<bool> {
    let view = view.as_object_mut().ok_or_else(state_schema_error)?;
    let mut changed = normalize_historical_snapshot(
        view.get_mut("config").ok_or_else(state_schema_error)?,
        legacy,
    )?;
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

fn rebind_declarations(view: &mut super::model::View, legacy: &BTreeMap<String, String>) {
    for (name, old_id) in legacy {
        let Some(provider) = view.config.providers.get(name) else {
            continue;
        };
        let Some(identity) = view.providers.get_mut(name) else {
            continue;
        };
        for (name, declaration) in &provider.inputs {
            let Some(retained) = identity.declarations.get_mut(name) else {
                continue;
            };
            let old = super::lineage::source_definition_with_integration(
                provider,
                declaration,
                Some(old_id),
            );
            if retained.definition == old {
                retained.definition = super::lineage::source_definition_with_integration(
                    provider,
                    declaration,
                    crate::provider_integration::integration_id(provider).ok(),
                );
            }
        }
    }
}

fn decode_runtime(bytes: &[u8]) -> Result<Runtime> {
    if let Ok(runtime) = serde_json::from_slice::<Runtime>(bytes) {
        let known_bases = std::iter::once(&runtime.saved)
            .chain(runtime.loaded.iter())
            .any(|view| {
                view.config.providers.values().any(|provider| {
                    provider.integration.as_deref().is_some_and(|id| {
                        crate::retained_integration::regional_candidates(id).is_some()
                    }) || (provider.integration.is_none()
                        && matches!(
                            provider.metadata.as_deref(),
                            Some("kimi-for-coding" | "kimi-code")
                        ))
                        || crate::provider_integration::integration_id(provider).is_err()
                        || (!provider.endpoints.is_empty()
                            && crate::provider_integration::integration_id(provider)
                                .is_ok_and(|id| id != "custom"))
                })
            });
        if !known_bases {
            return Ok(runtime);
        }
    }
    check_historical_field_uniqueness(bytes)?;
    let mut document: Value = serde_json::from_slice(bytes).map_err(|_| state_schema_error())?;
    let mut saved_legacy = BTreeMap::new();
    let mut loaded_legacy = BTreeMap::new();
    let saved = normalize_historical_view(
        document.get_mut("saved").ok_or_else(state_schema_error)?,
        &mut saved_legacy,
    )?;
    let loaded = match document.get_mut("loaded") {
        None | Some(Value::Null) => false,
        Some(view) => normalize_historical_view(view, &mut loaded_legacy)?,
    };
    if !saved && !loaded {
        return Err(state_schema_error());
    }
    let mut runtime: Runtime =
        serde_json::from_value(document).map_err(|_| state_schema_error())?;
    rebind_declarations(&mut runtime.saved, &saved_legacy);
    if let Some(view) = runtime.loaded.as_mut() {
        rebind_declarations(view, &loaded_legacy);
    }
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

    #[test]
    fn regional_cutover_preserves_routing_rules_input_heads_and_lineage() {
        for (old_id, region, canonical) in [
            ("kimi-coding-plan", "china", "kimi-code-plan-cn"),
            ("kimi-coding-plan", "overseas", "kimi-code-plan-global"),
            ("minimax-m-plan", "china", "minimax-cn-coding-plan"),
            ("minimax-m-plan", "global", "minimax-coding-plan"),
        ] {
            let (_directory, store, original) = fixture();
            let mut config = original.clone();
            let p = config.providers.get_mut("p").unwrap();
            p.integration = Some(canonical.into());
            p.endpoints.clear();
            p.models
                .get_mut("m")
                .unwrap()
                .priority_rules
                .push(config::PriorityRule {
                    id: "quota-rule".into(),
                    input: "i".into(),
                    priority: 9,
                    enabled: true,
                    reset_within_seconds: 10800,
                    remaining_at_least_percent: Some("1".into()),
                    time_window: None,
                });
            save_intent(&store, &config);
            reconcile(&store, &original, &config, &[]).unwrap();
            retain_value(&store, &config);
            activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
            let expected = serde_json::to_value(read_runtime(&store).unwrap()).unwrap();
            let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
            let before = Snapshot::capture_loaded(&store, &config, None);
            let input = before.input("p", "i", &at);
            let identity = before.destination_identity("p", "m");
            assert_eq!(input["availability"]["state"], "available");
            let before_rule = crate::routing::explain(
                &store,
                &config,
                "p/m",
                "chat-completions",
                Some("2026-10-23T10:30:00Z"),
            )
            .unwrap();
            assert_eq!(
                before_rule["priority_evaluations"][0]["priority_evaluation"]["selected_rule_id"],
                "quota-rule"
            );
            let old_hash = super::super::lineage::source_definition_with_integration(
                &config.providers["p"],
                &config.providers["p"].inputs["i"],
                Some(old_id),
            );
            let mut historical = expected.clone();
            for view in ["saved", "loaded"] {
                historical[view]["config"]["providers"]["p"]["integration"] = json!(old_id);
                historical[view]["config"]["providers"]["p"]["region"] = json!(region);
                historical[view]["providers"]["p"]["declarations"]["i"]["definition"] =
                    json!(old_hash);
            }
            store.write_json(STATE, &historical).unwrap();
            let bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
            assert_eq!(
                serde_json::to_value(read_runtime(&store).unwrap()).unwrap(),
                expected
            );
            let after = Snapshot::capture_loaded(&store, &config, None);
            assert_eq!(after.input("p", "i", &at), input);
            assert_eq!(after.destination_identity("p", "m"), identity);
            let after_rule = crate::routing::explain(
                &store,
                &config,
                "p/m",
                "chat-completions",
                Some("2026-10-23T10:30:00Z"),
            )
            .unwrap();
            assert_eq!(
                after_rule["priority_evaluations"][0]["priority_evaluation"],
                before_rule["priority_evaluations"][0]["priority_evaluation"]
            );
            assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), bytes);
            let mut unmatched = historical.clone();
            unmatched["saved"]["providers"]["p"]["declarations"]["i"]["definition"] =
                json!("unmatched");
            let unmatched = decode_runtime(&serde_json::to_vec(&unmatched).unwrap()).unwrap();
            assert_eq!(
                unmatched.saved.providers["p"].declarations["i"].definition,
                "unmatched"
            );
            assert_eq!(
                serde_json::to_value(&unmatched.channels).unwrap(),
                expected["channels"]
            );
            reconcile(&store, &config, &config, &[]).unwrap();
            assert_eq!(store.read_json::<Value>(STATE).unwrap(), expected);
        }
    }

    #[test]
    fn retained_regional_selection_requires_unique_consistent_evidence() {
        let (_directory, store, original) = fixture();
        let mut config = original.clone();
        let provider = config.providers.get_mut("p").unwrap();
        provider.integration = Some("kimi-code-plan-cn".into());
        provider.endpoints.clear();
        reconcile(&store, &original, &config, &[]).unwrap();
        let mut historical = serde_json::to_value(read_runtime(&store).unwrap()).unwrap();
        let provider = &mut historical["saved"]["config"]["providers"]["p"];
        provider["integration"] = json!("kimi-coding-plan");
        provider["region"] = Value::Null;
        provider["endpoints"] = json!({"messages":"https://api.kimi.com/coding/"});
        assert!(decode_runtime(&serde_json::to_vec(&historical).unwrap()).is_ok());
        for (region, endpoints) in [
            (Value::Null, json!({})),
            (
                json!("overseas"),
                json!({"messages":"https://api.kimi.com/coding/"}),
            ),
            (json!("unknown"), json!({})),
            (json!(false), json!({})),
            (
                json!("china"),
                json!({"messages":"https://unrecognized.example/"}),
            ),
        ] {
            let mut invalid = historical.clone();
            invalid["saved"]["config"]["providers"]["p"]["region"] = region;
            invalid["saved"]["config"]["providers"]["p"]["endpoints"] = endpoints;
            store.write_json(STATE, &invalid).unwrap();
            let bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
            assert_eq!(read_runtime(&store).unwrap_err().code, "invalid_record");
            let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
            assert_eq!(
                Snapshot::capture(&store, &config).input("p", "i", &at)["availability"]["reason_code"],
                "invalid_record"
            );
            assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), bytes);
        }
        let mut inferred = historical.clone();
        inferred["saved"]["config"]["providers"]["p"]["integration"] = Value::Null;
        inferred["saved"]["config"]["providers"]["p"]["metadata"] = json!("kimi-for-coding");
        let restored = decode_runtime(&serde_json::to_vec(&inferred).unwrap()).unwrap();
        assert_eq!(
            restored.saved.config.providers["p"].integration.as_deref(),
            Some("kimi-code-plan-cn")
        );
        assert_eq!(
            restored.saved.config.providers["p"].metadata.as_deref(),
            Some("kimi-for-coding")
        );
        inferred["saved"]["config"]["providers"]["p"]["endpoints"] = json!({});
        assert!(decode_runtime(&serde_json::to_vec(&inferred).unwrap()).is_err());

        let mut singleton = historical.clone();
        singleton["saved"]["config"]["providers"]["p"]["integration"] = json!("deepseek");
        singleton["saved"]["config"]["providers"]["p"]["endpoints"] = json!({});
        singleton["saved"]["config"]["providers"]["p"]["region"] = json!("default");
        assert!(decode_runtime(&serde_json::to_vec(&singleton).unwrap()).is_ok());
        singleton["saved"]["config"]["providers"]["p"]["region"] = json!("china");
        assert!(decode_runtime(&serde_json::to_vec(&singleton).unwrap()).is_err());
        singleton["saved"]["config"]["providers"]["p"]["integration"] = json!("custom");
        singleton["saved"]["config"]["providers"]["p"]["endpoints"] =
            json!({"chat-completions":"http://127.0.0.1:19090/v1"});
        singleton["saved"]["config"]["providers"]["p"]["region"] = json!("default");
        assert!(decode_runtime(&serde_json::to_vec(&singleton).unwrap()).is_err());
        singleton["saved"]["config"]["providers"]["p"]["region"] = Value::Null;
        assert!(decode_runtime(&serde_json::to_vec(&singleton).unwrap()).is_ok());
        singleton["saved"]["config"]["providers"]["p"]["integration"] = json!("unknown");
        assert!(decode_runtime(&serde_json::to_vec(&singleton).unwrap()).is_err());
    }

    #[test]
    fn removed_dsl_fields_normalize_only_retained_snapshots_without_losing_lineage_or_facts() {
        let (_directory, store, original) = fixture();
        let mut config = original.clone();
        config.network = Some(config::Network {
            proxy_url: "http://127.0.0.1:8080".into(),
        });
        let window = config::TimeWindow {
            weekdays: Some(vec!["fri".into()]),
            ..Default::default()
        };
        let model = config
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap();
        model.timezone = Some("Etc/UTC".into());
        model.schedule.push(config::Schedule {
            priority: 7,
            window: window.clone(),
        });
        model.priority_rules.push(config::PriorityRule {
            id: "weekly".into(),
            input: "i".into(),
            priority: 9,
            enabled: true,
            reset_within_seconds: 10800,
            remaining_at_least_percent: Some("1".into()),
            time_window: Some(window),
        });
        config.providers.insert(
            "known".into(),
            config::Provider {
                integration: Some("deepseek".into()),
                ..Default::default()
            },
        );
        save_intent(&store, &config);
        reconcile(&store, &original, &config, &[]).unwrap();
        retain_value(&store, &config);
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let expected = serde_json::to_value(read_runtime(&store).unwrap()).unwrap();
        let mut historical = expected.clone();
        for view in ["saved", "loaded"] {
            let snapshot = &mut historical[view]["config"];
            snapshot["network"]["mode"] = json!("proxy");
            snapshot["providers"]["p"]["models"]["m"]["schedule"][0]["all_day"] = json!(true);
            snapshot["providers"]["p"]["models"]["m"]["priority_rules"][0]["time_window"]["all_day"] =
                json!(true);
            snapshot["providers"]["known"]["region"] = Value::Null;
            snapshot["providers"]["known"]["endpoints"] =
                json!({"messages":"https://api.deepseek.com/anthropic"});
        }
        store.write_json(STATE, &historical).unwrap();
        let bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(read_runtime(&store).unwrap()).unwrap(),
            expected
        );
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).input("p", "i", &at)["availability"]["state"],
            "available"
        );
        assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), bytes);
        reconcile(&store, &config, &config, &[]).unwrap();
        assert_eq!(store.read_json::<Value>(STATE).unwrap(), expected);

        for (pointer, value) in [
            ("/saved/config/network/mode", json!(false)),
            (
                "/saved/config/providers/p/models/m/schedule/0/all_day",
                json!(false),
            ),
            (
                "/saved/config/providers/p/models/m/schedule/0/all_day",
                Value::Null,
            ),
        ] {
            let mut invalid = historical.clone();
            *invalid.pointer_mut(pointer).unwrap() = value;
            assert!(decode_runtime(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        let mut invalid = historical.clone();
        invalid["saved"]["config"]["providers"]["known"]["endpoints"]["messages"] =
            json!("https://unrecognized.example/v1");
        assert!(decode_runtime(&serde_json::to_vec(&invalid).unwrap()).is_err());
        let mut timed = historical.clone();
        let mut timed_expected = expected.clone();
        for view in ["saved", "loaded"] {
            let window = &mut timed[view]["config"]["providers"]["p"]["models"]["m"]["schedule"][0];
            window["all_day"] = Value::Null;
            window["start"] = json!("08:00");
            window["end"] = json!("20:00");
            let window =
                &mut timed_expected[view]["config"]["providers"]["p"]["models"]["m"]["schedule"][0];
            window["start"] = json!("08:00");
            window["end"] = json!("20:00");
        }
        assert_eq!(
            serde_json::to_value(decode_runtime(&serde_json::to_vec(&timed).unwrap()).unwrap())
                .unwrap(),
            timed_expected
        );
        let mut direct_expected = expected;
        for view in ["saved", "loaded"] {
            direct_expected[view]["config"]["network"] = Value::Null;
        }
        for policy in [
            json!({"mode":"direct","proxy_url":null}),
            json!({}),
            json!({"proxy_url":null}),
        ] {
            let mut direct = historical.clone();
            for view in ["saved", "loaded"] {
                direct[view]["config"]["network"] = policy.clone();
            }
            assert_eq!(
                serde_json::to_value(
                    decode_runtime(&serde_json::to_vec(&direct).unwrap()).unwrap()
                )
                .unwrap(),
                direct_expected
            );
        }
    }

    #[test]
    fn known_base_only_snapshots_are_normalized_even_without_removed_fields() {
        let (_directory, store, original) = fixture();
        let mut config = original.clone();
        config.providers.insert(
            "known".into(),
            config::Provider {
                integration: Some("deepseek".into()),
                ..Default::default()
            },
        );
        reconcile(&store, &original, &config, &[]).unwrap();
        let expected = serde_json::to_value(read_runtime(&store).unwrap()).unwrap();
        let mut historical = expected.clone();
        historical["saved"]["config"]["providers"]["known"]["region"] = Value::Null;
        historical["saved"]["config"]["providers"]["known"]["endpoints"] =
            json!({"messages":"https://api.deepseek.com/anthropic"});
        assert_eq!(
            serde_json::to_value(
                decode_runtime(&serde_json::to_vec(&historical).unwrap()).unwrap()
            )
            .unwrap(),
            expected
        );
    }
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
