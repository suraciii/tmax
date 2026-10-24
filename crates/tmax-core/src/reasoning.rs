//! Explicit alias-candidate effort; capability metadata never chooses a value.
use crate::{Error, Result, catalog, config::Config};
use serde_json::{Value, json};
pub(crate) fn validate_saved(store: &crate::storage::Store, config: &Config) -> Result<()> {
    if config
        .model_aliases
        .iter()
        .all(|alias| alias.candidate_reasoning_effort.is_empty())
    {
        return Ok(());
    }
    validate(config, catalog::read(store, "accepted")?.as_ref())
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
