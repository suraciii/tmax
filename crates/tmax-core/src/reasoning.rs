//! Upstream capabilities, common alias inputs, gateway effort maps, and fixed alias effort.
use crate::gateway::{RequestFacts, request_facts::Effort};
use crate::{Error, Result, catalog, config::Config};
use serde_json::{Value, json};
pub(crate) fn validate_saved(store: &crate::storage::Store, config: &Config) -> Result<()> {
    if config
        .model_aliases
        .iter()
        .all(|alias| alias.candidate_reasoning_effort.is_empty())
        && config.providers.values().all(|provider| {
            provider
                .models
                .values()
                .all(|model| model.effort_map.is_empty())
        })
    {
        return Ok(());
    }
    validate(config, catalog::read(store, "accepted")?.as_ref())
}

// Reasoning options are capability declarations. They are projected into the
// native agent's own selector vocabulary; they never choose a default level or
// rewrite a request. Inexact names are not translated.
pub(crate) fn effort_values(effective: &Value) -> Option<Vec<String>> {
    if effective.get("reasoning").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let options = effective.get("reasoning_options")?.as_array()?;
    let mut values: Vec<String> = Vec::new();
    for option in options {
        if option.get("type").and_then(Value::as_str) != Some("effort") {
            continue;
        }
        let Some(list) = option.get("values").and_then(Value::as_array) else {
            continue;
        };
        for value in list {
            let Some(value) = value.as_str().filter(|value| !value.is_empty()) else {
                continue;
            };
            if !values.iter().any(|existing| existing == value) {
                values.push(value.to_owned());
            }
        }
    }
    (!values.is_empty()).then_some(values)
}

/// Gateway-facing input names; mappings are explicit, single-step overrides.
pub(crate) fn input_metadata(
    effective: &Value,
    model: &crate::config::Model,
    protocol: &str,
) -> Value {
    let mut metadata = effective.clone();
    let Some(mapping) = model.effort_map.get(protocol) else {
        return metadata;
    };
    let upstream = effort_values(effective).unwrap_or_default();
    let mut inputs: Vec<String> = upstream
        .iter()
        .filter(|input| {
            mapping
                .get(*input)
                .is_none_or(|target| upstream.contains(target))
        })
        .cloned()
        .collect();
    for (input, target) in mapping {
        if upstream.contains(target) && !inputs.contains(input) {
            inputs.push(input.clone());
        }
    }
    if let Some(options) = metadata
        .get_mut("reasoning_options")
        .and_then(Value::as_array_mut)
    {
        options.retain(|option| option.get("type").and_then(Value::as_str) != Some("effort"));
        if !inputs.is_empty() {
            options.push(json!({"type":"effort","values":inputs}));
        }
    }
    metadata
}

/// Return the selected backend's upstream effort, borrowing existing request/config data.
pub(crate) fn resolve<'a>(
    model: &'a crate::config::Model,
    protocol: &str,
    body: &'a RequestFacts,
    fixed: Option<&'a str>,
) -> Result<Option<&'a str>> {
    if protocol == "responses" && body.reasoning_invalid {
        return Err(Error::new(
            "reasoning_parameter_invalid",
            "Responses reasoning must be an object or null",
        ));
    }
    if !matches!(protocol, "chat-completions" | "responses") {
        return Ok(None);
    }
    let value = body.effort(protocol);
    if let Some(effort) = fixed {
        return Ok(Some(effort));
    }
    if matches!(value, Effort::Missing | Effort::Null) {
        return Ok(None);
    }
    let input = value.as_str().filter(|s| !s.is_empty()).ok_or_else(|| {
        Error::new(
            "reasoning_parameter_invalid",
            "Reasoning effort must be a nonempty string or null",
        )
    })?;
    Ok(Some(
        model
            .effort_map
            .get(protocol)
            .and_then(|map| map.get(input))
            .map_or(input, String::as_str),
    ))
}

enum CommonInput {
    Unseen,
    Declared(Vec<String>),
    Unknown,
}

impl CommonInput {
    fn include(&mut self, input: Option<&[Value]>) {
        let Some(input) = input else {
            *self = Self::Unknown;
            return;
        };
        match self {
            Self::Unseen => {
                let mut common = Vec::new();
                for value in input {
                    let value = value.as_str().expect("declared input is a string");
                    if !common.iter().any(|existing| existing == value) {
                        common.push(value.to_owned());
                    }
                }
                *self = Self::Declared(common);
            }
            Self::Declared(common) => {
                common.retain(|value| {
                    input
                        .iter()
                        .any(|item| item.as_str() == Some(value.as_str()))
                });
            }
            Self::Unknown => {}
        }
    }

    fn values(self) -> Option<Vec<String>> {
        match self {
            Self::Declared(values) => Some(values),
            Self::Unseen | Self::Unknown => None,
        }
    }
}

fn declared_input(effective: &Value) -> Option<&[Value]> {
    let input = effective.pointer("/modalities/input")?.as_array()?;
    input
        .iter()
        .all(Value::is_string)
        .then_some(input.as_slice())
}

/// Explain the same all-candidate input intersection used by native projection.
pub(crate) fn alias_input_capability(
    config: &Config,
    metadata: Option<&catalog::Snapshot>,
    alias: &crate::config::ModelAlias,
) -> Result<Value> {
    let mut common = CommonInput::Unseen;
    let mut candidates = Vec::with_capacity(alias.candidates.len());
    for candidate in &alias.candidates {
        let (provider, model) = crate::config::model(config, candidate)?;
        let (_, model_id) = crate::config::qualified(candidate)?;
        let projection =
            catalog::effective_from_snapshot(metadata, provider, Some((model_id, model)))?;
        let input = declared_input(&projection["effective"]);
        common.include(input);
        candidates.push(json!({"model": candidate, "input": input}));
    }
    let common = common.values();
    Ok(json!({
        "state": if common.is_some() { "declared" } else { "unknown" },
        "common": common,
        "candidates": candidates,
    }))
}

pub(crate) fn alias_metadata(
    config: &Config,
    metadata: Option<&catalog::Snapshot>,
    alias: &crate::config::ModelAlias,
) -> Result<Value> {
    let mut common: Option<Vec<String>> = None;
    let mut common_input = CommonInput::Unseen;
    // Include every configured candidate: eligibility and priority are transient,
    // while native selectors persist until the next explicit application.
    for candidate in &alias.candidates {
        let (provider, model) = crate::config::model(config, candidate)?;
        let (_, model_id) = crate::config::qualified(candidate)?;
        let projection =
            catalog::effective_from_snapshot(metadata, provider, Some((model_id, model)))?;
        common_input.include(declared_input(&projection["effective"]));
        let values = if let Some(fixed) = alias.candidate_reasoning_effort.get(candidate) {
            // A fixed upstream value bypasses the model map, including a map
            // whose input has the same name as this fixed value.
            if supported(&projection["effective"], fixed).is_none() {
                vec![fixed.clone()]
            } else {
                Vec::new()
            }
        } else {
            let inputs = input_metadata(&projection["effective"], model, &alias.protocol);
            effort_values(&inputs).unwrap_or_default()
        };
        if let Some(common) = &mut common {
            common.retain(|value| values.contains(value));
        } else {
            common = Some(values);
        }
    }
    let mut effective = match common.filter(|values| !values.is_empty()) {
        Some(values) => json!({
            "reasoning": true,
            "reasoning_options": [{"type": "effort", "values": values}],
        }),
        None => json!({}),
    };
    if let Some(input) = common_input.values() {
        effective["modalities"] = json!({"input": input});
    }
    Ok(effective)
}

pub(crate) fn supported(effective: &Value, effort: &str) -> Option<&'static str> {
    if effective.get("reasoning").and_then(Value::as_bool) == Some(false) {
        return Some("reasoning_incompatible");
    }
    let Some(options) = effective.get("reasoning_options").and_then(Value::as_array) else {
        return Some("reasoning_effort_unavailable");
    };
    let mut known = false;
    for option in options {
        if option.get("type").and_then(Value::as_str) != Some("effort") {
            continue;
        }
        if let Some(values) = option.get("values").and_then(Value::as_array) {
            known = true;
            if values.iter().any(|value| value.as_str() == Some(effort)) {
                return None;
            }
        }
    }
    Some(if known {
        "reasoning_effort_incompatible"
    } else {
        "reasoning_effort_unavailable"
    })
}

pub(crate) fn validate(config: &Config, metadata: Option<&catalog::Snapshot>) -> Result<()> {
    for (provider_id, provider) in &config.providers {
        for (model_id, model) in &provider.models {
            if model.effort_map.is_empty() {
                continue;
            }
            let projection =
                catalog::effective_from_snapshot(metadata, provider, Some((model_id, model)))?;
            for (protocol, mapping) in &model.effort_map {
                for (input, target) in mapping {
                    if let Some(reason) = supported(&projection["effective"], target) {
                        return Err(Error::new(reason, &format!("Model '{provider_id}/{model_id}' protocol '{protocol}' maps '{input}' to undeclared upstream effort '{target}'"))
                            .field(format!("providers.{provider_id}.models.{model_id}.effort_map.{protocol}.{input}")));
                    }
                }
            }
        }
    }
    for alias in &config.model_aliases {
        for (candidate, effort) in &alias.candidate_reasoning_effort {
            let (provider, model) = crate::config::model(config, candidate)?;
            let (_, model_id) = crate::config::qualified(candidate)?;
            let projection =
                catalog::effective_from_snapshot(metadata, provider, Some((model_id, model)))?;
            if let Some(reason) = supported(&projection["effective"], effort) {
                let requirement = match reason {
                    "reasoning_incompatible" => {
                        "effective reasoning=false forbids configured reasoning effort"
                    }
                    "reasoning_effort_incompatible" => {
                        "chosen effort is not listed in effective reasoning_options[type=effort].values"
                    }
                    _ => {
                        "effective reasoning_options must declare type=effort with explicit values; reasoning=true or toggle-only metadata does not declare supported effort levels"
                    }
                };
                return Err(Error::new(
                    reason,
                    &format!(
                        "Alias '{}' candidate '{}' effort '{}' protocol '{}': {requirement}",
                        alias.id, candidate, effort, alias.protocol
                    ),
                )
                .field(format!(
                    "model_aliases.{}.candidate_reasoning_effort.{candidate}",
                    alias.id
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn request_mismatch(
    protocol: &str,
    body: &RequestFacts,
    effort: &str,
) -> Option<&'static str> {
    if !matches!(protocol, "chat-completions" | "responses") {
        return Some("candidate_reasoning_protocol_unsupported");
    }
    if protocol == "responses" && body.reasoning_invalid {
        return Some("reasoning_parameter_invalid");
    }
    if effort != "none" && body.thinking_disabled {
        return Some("reasoning_toggle_conflict");
    }
    if effort == "none" && body.thinking_enabled {
        return Some("reasoning_toggle_conflict");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Model, ModelAlias, ModelOverrides, Provider};
    use std::collections::BTreeMap;

    fn fixture() -> (Config, catalog::Snapshot, ModelAlias) {
        let config = Config {
            providers: BTreeMap::from([(
                "p".into(),
                Provider {
                    metadata: Some("directory".into()),
                    models: BTreeMap::from([
                        ("a".into(), Model::default()),
                        (
                            "b".into(),
                            Model {
                                enabled: false,
                                priority: -100,
                                ..Default::default()
                            },
                        ),
                    ]),
                    ..Default::default()
                },
            )]),
            ..Config::empty()
        };
        let snapshot = catalog::Snapshot {
            providers: BTreeMap::from([(
                "directory".into(),
                json!({"models": {
                    "a": {"reasoning": true, "limit": {"context": 1000000},
                        "reasoning_options": [{"type": "effort", "values": ["low", "high", "max", "low"]}]},
                    "b": {"reasoning": true,
                        "reasoning_options": [{"type": "effort", "values": ["max", "high"]}]}
                }}),
            )]),
            ..Default::default()
        };
        let alias = ModelAlias {
            id: "shared".into(),
            protocol: "chat-completions".into(),
            candidates: vec!["p/a".into(), "p/b".into()],
            candidate_reasoning_effort: BTreeMap::new(),
            enabled: true,
            continuity: Some("stateless".into()),
        };
        (config, snapshot, alias)
    }

    #[test]
    fn alias_inputs_intersect_in_first_candidate_order_without_other_modalities() {
        let (config, mut snapshot, alias) = fixture();
        for (first, second, expected) in [
            (
                json!(["text", "image", "audio", "text", "image"]),
                json!(["image", "text", "image"]),
                json!(["text", "image"]),
            ),
            (
                json!(["image", "text", "image"]),
                json!(["text", "image"]),
                json!(["image", "text"]),
            ),
            (json!(["image"]), json!(["image"]), json!(["image"])),
        ] {
            snapshot.providers.get_mut("directory").unwrap()["models"]["a"]["modalities"] =
                json!({"input": first, "output": ["image"]});
            snapshot.providers.get_mut("directory").unwrap()["models"]["b"]["modalities"] =
                json!({"input": second, "output": ["text"]});
            assert_eq!(
                alias_metadata(&config, Some(&snapshot), &alias).unwrap(),
                json!({
                    "reasoning": true,
                    "reasoning_options": [{"type": "effort", "values": ["high", "max"]}],
                    "modalities": {"input": expected},
                })
            );
            assert_eq!(
                alias_input_capability(&config, Some(&snapshot), &alias).unwrap(),
                json!({
                    "state": "declared",
                    "common": expected,
                    "candidates": [
                        {"model": "p/a", "input": first},
                        {"model": "p/b", "input": second},
                    ],
                })
            );
        }
    }

    #[test]
    fn disabled_candidates_restrict_alias_inputs_independently_of_effort() {
        let (config, mut snapshot, mut alias) = fixture();
        assert!(!config.providers["p"].models["b"].enabled);
        snapshot.providers.get_mut("directory").unwrap()["models"]["a"]["modalities"] =
            json!({"input": ["text", "image"]});
        snapshot.providers.get_mut("directory").unwrap()["models"]["b"]["modalities"] =
            json!({"input": ["text"]});
        alias
            .candidate_reasoning_effort
            .insert("p/a".into(), "low".into());
        alias
            .candidate_reasoning_effort
            .insert("p/b".into(), "high".into());
        assert_eq!(
            alias_metadata(&config, Some(&snapshot), &alias).unwrap(),
            json!({"modalities": {"input": ["text"]}})
        );
        assert_eq!(
            alias_input_capability(&config, Some(&snapshot), &alias).unwrap(),
            json!({
                "state": "declared", "common": ["text"],
                "candidates": [
                    {"model": "p/a", "input": ["text", "image"]},
                    {"model": "p/b", "input": ["text"]},
                ],
            })
        );
    }

    #[test]
    fn unknown_alias_inputs_are_not_text_or_known_empty() {
        let (config, snapshot, alias) = fixture();
        for unknown in [
            json!({}),
            json!({"output": ["text"]}),
            json!({"input": null}),
            json!({"input": "text"}),
            json!({"input": ["text", false]}),
        ] {
            for known in [json!(["text", "image"]), json!([])] {
                for unknown_first in [false, true] {
                    let mut snapshot = snapshot.clone();
                    let (unknown_id, known_id) = if unknown_first {
                        ("a", "b")
                    } else {
                        ("b", "a")
                    };
                    snapshot.providers.get_mut("directory").unwrap()["models"][unknown_id]["modalities"] =
                        unknown.clone();
                    snapshot.providers.get_mut("directory").unwrap()["models"][known_id]["modalities"] =
                        json!({"input": known});
                    let effective = alias_metadata(&config, Some(&snapshot), &alias).unwrap();
                    assert!(effective.get("modalities").is_none());
                    assert_eq!(effort_values(&effective).unwrap(), ["high", "max"]);
                    let inputs = if unknown_first {
                        json!([{"model": "p/a", "input": null}, {"model": "p/b", "input": known}])
                    } else {
                        json!([{"model": "p/a", "input": known}, {"model": "p/b", "input": null}])
                    };
                    assert_eq!(
                        alias_input_capability(&config, Some(&snapshot), &alias).unwrap(),
                        json!({"state": "unknown", "common": null, "candidates": inputs})
                    );
                }
            }
        }
        assert_eq!(
            alias_input_capability(&config, None, &alias).unwrap(),
            json!({
                "state": "unknown", "common": null,
                "candidates": [
                    {"model": "p/a", "input": null},
                    {"model": "p/b", "input": null},
                ],
            })
        );
    }

    #[test]
    fn explicit_empty_and_disjoint_alias_inputs_remain_declared() {
        let (config, mut snapshot, alias) = fixture();
        for (first, second) in [
            (json!([]), json!(["text", "image"])),
            (json!(["text", "image"]), json!([])),
            (json!([]), json!([])),
            (json!(["image"]), json!(["text"])),
        ] {
            snapshot.providers.get_mut("directory").unwrap()["models"]["a"]["modalities"] =
                json!({"input": first});
            snapshot.providers.get_mut("directory").unwrap()["models"]["b"]["modalities"] =
                json!({"input": second});
            assert_eq!(
                alias_metadata(&config, Some(&snapshot), &alias).unwrap(),
                json!({
                    "reasoning": true,
                    "reasoning_options": [{"type": "effort", "values": ["high", "max"]}],
                    "modalities": {"input": []},
                })
            );
            assert_eq!(
                alias_input_capability(&config, Some(&snapshot), &alias).unwrap(),
                json!({
                    "state": "declared", "common": [],
                    "candidates": [
                        {"model": "p/a", "input": first},
                        {"model": "p/b", "input": second},
                    ],
                })
            );
        }
    }

    #[test]
    fn alias_inputs_use_accepted_snapshot_and_local_replacement_overrides() {
        let (mut config, mut snapshot, alias) = fixture();
        snapshot.providers.get_mut("directory").unwrap()["models"]["a"]["modalities"] =
            json!({"input": ["text", "image"]});
        snapshot.providers.get_mut("directory").unwrap()["models"]["b"]["modalities"] =
            json!({"input": ["text"]});
        config
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("b")
            .unwrap()
            .overrides =
            serde_json::from_value::<ModelOverrides>(json!({"modalities": {"input": ["image"]}}))
                .unwrap();
        assert_eq!(
            alias_metadata(&config, Some(&snapshot), &alias).unwrap()["modalities"],
            json!({"input": ["image"]})
        );
        assert_eq!(
            alias_input_capability(&config, Some(&snapshot), &alias).unwrap(),
            json!({
                "state": "declared", "common": ["image"],
                "candidates": [
                    {"model": "p/a", "input": ["text", "image"]},
                    {"model": "p/b", "input": ["image"]},
                ],
            })
        );
        config
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("a")
            .unwrap()
            .overrides = serde_json::from_value::<ModelOverrides>(
            json!({"modalities": {"input": ["image", "audio"]}}),
        )
        .unwrap();
        assert_eq!(
            alias_metadata(&config, None, &alias).unwrap(),
            json!({"modalities": {"input": ["image"]}})
        );
        assert_eq!(
            alias_input_capability(&config, None, &alias).unwrap(),
            json!({
                "state": "declared", "common": ["image"],
                "candidates": [
                    {"model": "p/a", "input": ["image", "audio"]},
                    {"model": "p/b", "input": ["image"]},
                ],
            })
        );
        config
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("b")
            .unwrap()
            .overrides =
            serde_json::from_value::<ModelOverrides>(json!({"modalities": {"input": []}})).unwrap();
        assert_eq!(
            alias_metadata(&config, Some(&snapshot), &alias).unwrap()["modalities"],
            json!({"input": []})
        );
        assert_eq!(
            alias_input_capability(&config, Some(&snapshot), &alias).unwrap()["common"],
            json!([])
        );
    }

    #[test]
    fn alias_efforts_intersect_all_candidates_and_honor_local_overrides() {
        let (mut config, snapshot, alias) = fixture();
        assert_eq!(
            alias_metadata(&config, Some(&snapshot), &alias).unwrap(),
            json!({
                "reasoning": true,
                "reasoning_options": [{"type": "effort", "values": ["high", "max"]}]
            })
        );
        config
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("b")
            .unwrap()
            .overrides = serde_json::from_value::<ModelOverrides>(json!({"reasoning_options": [
            {"type": "effort", "values": ["low", "high"]}
        ]}))
        .unwrap();
        assert_eq!(
            alias_metadata(&config, Some(&snapshot), &alias).unwrap(),
            json!({
                "reasoning": true,
                "reasoning_options": [{"type": "effort", "values": ["low", "high"]}]
            })
        );
    }

    #[test]
    fn alias_efforts_never_infer_support_from_unknown_toggle_or_disabled_reasoning() {
        let (config, snapshot, alias) = fixture();
        for metadata in [
            json!({}),
            json!({"reasoning": true}),
            json!({"reasoning_options": [{"type": "toggle", "values": ["high"]}]}),
            json!({"reasoning": false, "reasoning_options": [{"type": "effort", "values": ["high"]}]}),
            json!({"reasoning_options": [{"type": "effort", "values": ["medium"]}]}),
        ] {
            let mut snapshot = snapshot.clone();
            snapshot.providers.get_mut("directory").unwrap()["models"]["b"] = metadata;
            assert_eq!(
                alias_metadata(&config, Some(&snapshot), &alias).unwrap(),
                json!({})
            );
        }
        assert_eq!(alias_metadata(&config, None, &alias).unwrap(), json!({}));
    }

    #[test]
    fn fixed_candidate_efforts_cannot_advertise_choices_the_gateway_would_replace() {
        let (config, snapshot, mut alias) = fixture();
        alias
            .candidate_reasoning_effort
            .insert("p/b".into(), "high".into());
        assert_eq!(
            alias_metadata(&config, Some(&snapshot), &alias).unwrap(),
            json!({
                "reasoning": true,
                "reasoning_options": [{"type": "effort", "values": ["high"]}]
            })
        );
        alias
            .candidate_reasoning_effort
            .insert("p/a".into(), "max".into());
        assert_eq!(
            alias_metadata(&config, Some(&snapshot), &alias).unwrap(),
            json!({})
        );
    }

    #[test]
    fn fixed_effort_bypasses_same_named_map_in_alias_projection() {
        let (mut config, snapshot, mut alias) = fixture();
        config
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("b")
            .unwrap()
            .effort_map
            .insert(
                "chat-completions".into(),
                BTreeMap::from([("high".into(), "max".into())]),
            );
        alias
            .candidate_reasoning_effort
            .insert("p/b".into(), "high".into());
        assert_eq!(
            effort_values(&alias_metadata(&config, Some(&snapshot), &alias).unwrap()).unwrap(),
            ["high"]
        );
    }

    #[test]
    fn mapped_inputs_intersect_without_replacing_upstream_declarations() {
        let (mut config, snapshot, alias) = fixture();
        let model = config
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("b")
            .unwrap();
        model.effort_map.insert(
            "chat-completions".into(),
            BTreeMap::from([("low".into(), "high".into()), ("high".into(), "max".into())]),
        );
        let effective = &catalog::effective_from_snapshot(
            Some(&snapshot),
            &config.providers["p"],
            Some(("b", &config.providers["p"].models["b"])),
        )
        .unwrap()["effective"];
        assert_eq!(effort_values(effective).unwrap(), ["max", "high"]);
        assert_eq!(
            effort_values(&alias_metadata(&config, Some(&snapshot), &alias).unwrap()).unwrap(),
            ["low", "high", "max"]
        );
        assert_eq!(
            effort_values(&input_metadata(
                effective,
                &config.providers["p"].models["b"],
                "responses"
            ))
            .unwrap(),
            ["max", "high"]
        );
    }

    #[test]
    fn maps_are_single_step_and_do_not_choose_effort_when_omitted() {
        let mut model = Model::default();
        model.effort_map.insert(
            "responses".into(),
            BTreeMap::from([("max".into(), "high".into()), ("high".into(), "low".into())]),
        );
        let body = RequestFacts::from_value(json!({"reasoning":{"effort":"max","summary":"auto"}}))
            .unwrap();
        assert_eq!(
            resolve(&model, "responses", &body, None).unwrap(),
            Some("high")
        );
        assert_eq!(
            resolve(&model, "responses", &body, Some("low")).unwrap(),
            Some("low")
        );
        assert_eq!(
            resolve(
                &model,
                "chat-completions",
                &RequestFacts::from_value(json!({"reasoning_effort":"max"})).unwrap(),
                None
            )
            .unwrap(),
            Some("max")
        );
        for body in [
            json!({}),
            json!({"reasoning":null}),
            json!({"reasoning":{"effort":null}}),
        ] {
            let body = RequestFacts::from_value(body).unwrap();
            assert_eq!(resolve(&model, "responses", &body, None).unwrap(), None);
        }
        assert_eq!(
            resolve(
                &model,
                "responses",
                &RequestFacts::from_value(json!({"reasoning":{"effort":42}})).unwrap(),
                None
            )
            .unwrap_err()
            .code,
            "reasoning_parameter_invalid"
        );
    }
}
