//! Upstream capability declarations, gateway effort maps, and fixed alias effort.
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
    body: &'a Value,
    fixed: Option<&'a str>,
) -> Result<Option<&'a str>> {
    if protocol == "responses"
        && body
            .get("reasoning")
            .is_some_and(|v| !v.is_object() && !v.is_null())
    {
        return Err(Error::new(
            "reasoning_parameter_invalid",
            "Responses reasoning must be an object or null",
        ));
    }
    let value = match protocol {
        "chat-completions" => body.get("reasoning_effort"),
        "responses" => body.pointer("/reasoning/effort"),
        _ => return Ok(None),
    };
    if let Some(effort) = fixed {
        return Ok(Some(effort));
    }
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
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

pub(crate) fn alias_metadata(
    config: &Config,
    metadata: Option<&catalog::Snapshot>,
    alias: &crate::config::ModelAlias,
) -> Result<Value> {
    let mut common: Option<Vec<String>> = None;
    // Include every configured candidate: eligibility and priority are transient,
    // while native selectors persist until the next explicit application.
    for candidate in &alias.candidates {
        let (provider, model) = crate::config::model(config, candidate)?;
        let (_, model_id) = crate::config::qualified(candidate)?;
        let projection =
            catalog::effective_from_snapshot(metadata, provider, Some((model_id, model)))?;
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
    Ok(match common.filter(|values| !values.is_empty()) {
        Some(values) => json!({
            "reasoning": true,
            "reasoning_options": [{"type": "effort", "values": values}],
        }),
        None => json!({}),
    })
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

pub(crate) fn request_mismatch(protocol: &str, body: &Value, effort: &str) -> Option<&'static str> {
    if !matches!(protocol, "chat-completions" | "responses") {
        return Some("candidate_reasoning_protocol_unsupported");
    }
    if protocol == "responses"
        && body
            .get("reasoning")
            .is_some_and(|value| !value.is_object() && !value.is_null())
    {
        return Some("reasoning_parameter_invalid");
    }
    if effort != "none"
        && body.pointer("/thinking/type").and_then(Value::as_str) == Some("disabled")
    {
        return Some("reasoning_toggle_conflict");
    }
    if effort == "none"
        && body
            .pointer("/thinking/type")
            .and_then(Value::as_str)
            .is_some_and(|kind| matches!(kind, "enabled" | "adaptive"))
    {
        return Some("reasoning_toggle_conflict");
    }
    None
}

pub(crate) fn apply(body: &mut Value, protocol: &str, effort: &str) -> Result<()> {
    if let Some(reason) = request_mismatch(protocol, body, effort) {
        return Err(Error::new(
            reason,
            "Configured candidate effort conflicts with the native request reasoning control",
        ));
    }
    match protocol {
        "chat-completions" => body["reasoning_effort"] = json!(effort),
        "responses" => {
            if body.get("reasoning").is_none_or(Value::is_null) {
                body["reasoning"] = json!({});
            }
            body["reasoning"]["effort"] = json!(effort);
        }
        _ => unreachable!("validated protocol"),
    }
    Ok(())
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
        let mut body = json!({"reasoning":{"effort":"max","summary":"auto"}});
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
                &json!({"reasoning_effort":"max"}),
                None
            )
            .unwrap(),
            Some("max")
        );
        let effort = resolve(&model, "responses", &body, None)
            .unwrap()
            .unwrap()
            .to_owned();
        apply(&mut body, "responses", &effort).unwrap();
        assert_eq!(
            body,
            json!({"reasoning":{"effort":"high","summary":"auto"}})
        );
        for body in [
            json!({}),
            json!({"reasoning":null}),
            json!({"reasoning":{"effort":null}}),
        ] {
            assert_eq!(resolve(&model, "responses", &body, None).unwrap(), None);
        }
        assert_eq!(
            resolve(
                &model,
                "responses",
                &json!({"reasoning":{"effort":42}}),
                None
            )
            .unwrap_err()
            .code,
            "reasoning_parameter_invalid"
        );
    }
}
