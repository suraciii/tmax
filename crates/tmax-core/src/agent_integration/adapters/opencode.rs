use super::super::document::Format;
use super::capabilities::ReasoningProjection;
use super::credentials::credential_path;
use super::{Entry, File, default_reference, file, reference};
use crate::{Error, Result, config::AgentTarget};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

fn package(protocol: &str) -> Result<&'static str> {
    match protocol {
        "chat-completions" => Ok("@opencode/ai/providers/openai-compatible"),
        "responses" => Ok("@opencode/ai/providers/openai-compatible/responses"),
        "messages" => Ok("@opencode/ai/providers/anthropic-compatible"),
        _ => Err(Error::new(
            "unsupported_protocol",
            "Unknown native package protocol",
        )),
    }
}

pub(super) fn project_reasoning(protocol: &str, values: &[String]) -> Result<ReasoningProjection> {
    let setting = match protocol {
        "chat-completions" | "responses" => "reasoningEffort",
        "messages" => "effort",
        _ => {
            return Err(Error::new(
                "incompatible",
                "Unknown native reasoning protocol",
            ));
        }
    };
    Ok(ReasoningProjection {
        state: "projected",
        reason_code: None,
        fields: json!({"variants": values.iter().map(|value| json!({
            "id": value,
            "settings": {setting: value},
        })).collect::<Vec<_>>()}),
        omitted: Vec::new(),
    })
}

/// OpenCode encodes the default model and its effort variant in one string.
/// When tmax does not manage the effort, keep the current variant on the same
/// base model; changing the model is an explicit replacement of that reference.
pub(super) fn preserve_variant(path: &[String], value: &mut Value, current: &Value) {
    if path != ["model"] {
        return;
    }
    let Some(desired) = value.as_str() else {
        return;
    };
    if desired.contains('#') {
        return;
    }
    let Some(current) = current.get("model").and_then(Value::as_str) else {
        return;
    };
    let Some((base, _variant)) = current.split_once('#') else {
        return;
    };
    if base == desired {
        *value = json!(current);
    }
}

pub(super) fn model_metadata(effective: &Value) -> Result<Value> {
    let mut metadata = json!({});
    for field in ["context", "input", "output"] {
        if let Some(value) = effective.get("limit").and_then(|limit| limit.get(field))
            && !value.is_null()
        {
            let count = value.as_u64().filter(|count| *count <= 9_007_199_254_740_991)
                .ok_or_else(|| Error::new(
                    "native_metadata_unrepresentable",
                    "This concrete model limit cannot be represented exactly by OpenCode's integer schema",
                ).field(format!("limit.{field}")))?;
            metadata["limit"][field] = json!(count);
        }
    }
    if let Some(tools) = effective.get("tool_call").and_then(Value::as_bool) {
        metadata["capabilities"]["tools"] = json!(tools);
    }
    if let Some(modalities) = effective.pointer("/modalities/output")
        && modalities.is_array()
    {
        metadata["capabilities"]["output"] = modalities.clone();
    }
    Ok(metadata)
}

pub(super) fn build(target: &AgentTarget, scope: &Path, entries: &[Entry]) -> Result<Vec<File>> {
    let mut providers = serde_json::Map::new();
    for entry in entries {
        if entry.login_provider.is_some() {
            continue;
        }
        let package = package(&entry.protocol)?;
        let models: BTreeMap<&str, Value> = entry.models.iter().map(|id| {
            let (protocol, base) = entry.model_routes.get(id)
                .map(|(p, b)| (p.as_str(), b.as_str()))
                .unwrap_or((&entry.protocol, &entry.base));
            let mut model = json!({"modelID":id,"package":self::package(protocol)?,"settings":{"baseURL":base}});
            if let Some(metadata) = entry.model_metadata.get(id).and_then(Value::as_object) {
                model.as_object_mut().expect("native model object").extend(metadata.clone());
            }
            Ok((id.as_str(), model))
        }).collect::<Result<_>>()?;
        providers.insert(entry.name.clone(),json!({"name":entry.name,"package":package,"env":[],"settings":{"baseURL":entry.base,"apiKey":format!("{{file:{}}}",credential_path(scope,entry).file_name().expect("credential filename").to_string_lossy())},"models":models}));
    }
    let mut config = json!({"providers":providers});
    if let Some(default) = &target.default {
        let mut model = default_reference(default, entries)?;
        if let Some(effort) = &default.effort {
            model.push('#');
            model.push_str(effort);
        }
        config["model"] = json!(model);
    }
    for (role, r) in &target.roles {
        config["agents"][role]["model"] = json!(reference(r, entries)?);
    }
    let name = if scope.join("opencode.jsonc").exists() || !scope.join("opencode.json").exists() {
        "opencode.jsonc"
    } else {
        "opencode.json"
    };
    Ok(vec![file(scope, name, Format::Json, config)])
}

#[cfg(test)]
mod tests {
    use super::super::build;
    use super::super::test_support::target;
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn opencode_adds_models_without_overriding_the_native_default() {
        let mut target = target("opencode");
        target.default = None;
        let entry = Entry {
            name: "local".into(),
            base: "http://127.0.0.1:8317/v1".into(),
            protocol: "chat-completions".into(),
            key: None,
            login_provider: None,
            canonical: None,
            gateway: true,
            models: vec!["m".into()],
            model_routes: BTreeMap::new(),
            model_metadata: BTreeMap::new(),
            model_projection: BTreeMap::new(),
        };
        let files = build(&target, Path::new("/explicit/native"), &[entry]).unwrap();
        assert_eq!(
            files[0].desired["providers"]["local"]["models"]["m"]["modelID"],
            "m"
        );
        let key = files[0].desired["providers"]["local"]["settings"]["apiKey"]
            .as_str()
            .unwrap();
        assert!(key.starts_with("{file:tmax-"));
        assert!(!key.contains(['/', '\\']));
        assert!(files[0].desired.get("model").is_none());
        assert!(files[0].desired.get("agents").is_none());
    }

    #[test]
    fn opencode_default_effort_is_a_variant_suffix() {
        use super::super::test_support::{effort, entry, project};
        let scope = Path::new("/explicit/native");
        let mut model_entry = entry("chat-completions");
        project(
            &mut model_entry,
            json!({"fields":{"variants":[{"id":"high","settings":{"reasoningEffort":"high"}}]}}),
        );
        let mut target = target("opencode");
        effort(&mut target, "high");
        let files = build(&target, scope, std::slice::from_ref(&model_entry)).unwrap();
        let config = files
            .iter()
            .find(|file| file.path == scope.join("opencode.jsonc"))
            .unwrap();
        assert_eq!(config.desired["model"], "local/m#high");
        target.default.as_mut().unwrap().effort = None;
        let files = build(&target, scope, &[model_entry]).unwrap();
        let config = files
            .iter()
            .find(|file| file.path == scope.join("opencode.jsonc"))
            .unwrap();
        assert_eq!(config.desired["model"], "local/m");
    }

    #[test]
    fn opencode_rejects_variant_ids_that_would_break_the_model_reference() {
        use super::super::test_support::{effort, entry, project};
        let mut model_entry = entry("chat-completions");
        project(
            &mut model_entry,
            json!({"fields":{"variants":[{"id":"a#b"},{"id":"high"}]}}),
        );
        let mut target = target("opencode");
        effort(&mut target, "a#b");
        let error = build(&target, Path::new("/explicit/native"), &[model_entry]).unwrap_err();
        assert_eq!(error.code, "native_default_effort_unrepresentable");
        assert!(error.message.contains("available: high"));
    }

    #[test]
    fn opencode_unmanaged_variant_is_preserved_on_the_same_model() {
        use super::super::desired_fields;
        let current = json!({"providers":{},"model":"local/m#high"});
        assert_eq!(
            desired_fields("opencode", &json!({"model":"local/m"}), &current),
            vec![(vec!["model".into()], Some(json!("local/m#high")))]
        );
        assert_eq!(
            desired_fields("opencode", &json!({"model":"local/other"}), &current),
            vec![(vec!["model".into()], Some(json!("local/other")))]
        );
    }
}
