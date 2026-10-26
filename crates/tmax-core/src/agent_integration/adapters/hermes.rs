use super::super::document;
use super::super::document::Format;
use super::{Entry, File, file};
use crate::{Error, Result, config::AgentTarget};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

fn transport(protocol: &str) -> Result<&'static str> {
    match protocol {
        "chat-completions" => Ok("chat_completions"),
        "responses" => Ok("codex_responses"),
        "messages" => Ok("anthropic_messages"),
        _ => Err(Error::new(
            "unsupported_protocol",
            "Unsupported native protocol",
        )),
    }
}

pub(super) fn model_metadata(effective: &Value) -> Result<Value> {
    let mut metadata = json!({});
    if let Some(value) = effective.pointer("/limit/context")
        && !value.is_null()
    {
        let count = value.as_u64().filter(|count| *count > 0).ok_or_else(|| {
            Error::new(
                "native_metadata_unrepresentable",
                "Hermes requires a positive concrete model context length",
            )
            .field("limit.context")
        })?;
        metadata["context_length"] = json!(count);
        metadata["model_overrides"]["context_window"] = json!(count);
    }
    for (source, native) in [
        ("reasoning", "supports_reasoning"),
        ("tool_call", "supports_tools"),
    ] {
        if let Some(value) = effective.get(source).and_then(Value::as_bool) {
            metadata["model_overrides"][native] = json!(value);
        }
    }
    if let Some(input) = effective
        .pointer("/modalities/input")
        .and_then(Value::as_array)
    {
        metadata["model_overrides"]["supports_vision"] = json!(
            input
                .iter()
                .any(|modality| modality.as_str() == Some("image"))
        );
    }
    Ok(metadata)
}

pub(super) fn build(target: &AgentTarget, scope: &Path, entries: &[Entry]) -> Result<Vec<File>> {
    let mut providers = serde_json::Map::new();
    let mut overrides = serde_json::Map::new();
    for entry in entries {
        if entry.login_provider.is_some() {
            continue;
        }
        let models: BTreeMap<&str, Value> = entry
            .models
            .iter()
            .map(|id| {
                let mut model = json!({});
                if let Some(metadata) = entry.model_metadata.get(id) {
                    if let Some(context) = metadata.get("context_length") {
                        model["context_length"] = context.clone();
                    }
                    if let Some(patch) = metadata.get("model_overrides") {
                        overrides
                            .entry(entry.native_name().to_owned())
                            .or_insert_with(|| json!({}))[id] = patch.clone();
                    }
                }
                (id.as_str(), model)
            })
            .collect();
        providers.insert(entry.name.clone(),json!({"name":entry.name,"base_url":entry.base,"api_key":entry.key,"transport":transport(&entry.protocol)?,"models":models}));
        if entry.gateway {
            providers[&entry.name]["session_affinity_header"] = json!("x-session-id");
        }
    }
    let mut config = json!({"providers":providers});
    if let Some(default) = &target.default {
        let active = entries
            .iter()
            .find(|e| e.name == default.provider)
            .ok_or_else(|| Error::new("unresolved_reference", "Native default does not resolve"))?;
        config["model"] = json!({"provider":active.native_name(),"default":default.id});
        if active.login_provider.is_none() {
            config["model"]["base_url"] = json!(active.base);
        }
    }
    if !overrides.is_empty() {
        config["model_overrides"] = Value::Object(overrides);
    }
    for (role, r) in &target.roles {
        let e = entries
            .iter()
            .find(|e| e.name == r.provider)
            .ok_or_else(|| Error::new("unresolved_reference", "Native role does not resolve"))?;
        config["auxiliary"][role] = json!({"provider":e.native_name(),"model":r.id});
    }
    // Required to keep the native refresh owner independent. This is not
    // token import and does not make pooled accounts independently pinned.
    if entries.iter().any(|e| e.login_provider.is_some()) {
        config["auth"]["adopt_external_logins"] = json!(false);
    }
    Ok(vec![file(scope, "config.yaml", Format::Yaml, config)])
}

pub(super) fn inline_credential(path: &[String]) -> bool {
    path.len() == 3 && path[0] == "providers" && path[2] == "api_key"
}

pub(super) fn preserve_model(path: &[String], value: &Value, current: &Value) -> bool {
    path.len() == 4
        && path[0] == "providers"
        && path[2] == "models"
        && value.as_object().is_some_and(|model| model.is_empty())
        && document::get(current, path)
            .and_then(Value::as_object)
            .is_some_and(|model| !model.is_empty())
}

pub(super) fn obsolete_fields(desired: &Value, fields: &mut Vec<(Vec<String>, Option<Value>)>) {
    // Remove obsolete command references and competing environment keys for
    // providers whose API key tmax now owns. Record these removals so restore
    // can recover the original fields without changing unrelated providers.
    if let Some(providers) = desired.get("providers").and_then(Value::as_object) {
        for (name, provider) in providers {
            if provider.get("api_key").is_some() {
                for obsolete in ["key_cmd", "key_env", "api_key_env"] {
                    fields.push((
                        vec!["providers".into(), name.clone(), obsolete.into()],
                        None,
                    ));
                }
            }
        }
    }
}

pub(super) fn resolve_handoff(
    desired: &mut Value,
    resolve: &mut impl FnMut(&str) -> Result<Value>,
) -> Result<()> {
    if let Some(providers) = desired.get_mut("providers").and_then(Value::as_object_mut) {
        for provider in providers.values_mut() {
            if let Some(reference) = provider.get("api_key").and_then(Value::as_str) {
                provider["api_key"] = resolve(reference)?;
            }
        }
    }
    Ok(())
}
