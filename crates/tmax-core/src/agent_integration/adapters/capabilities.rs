use super::{dsh, opencode, pi_omp};
use crate::{Error, Result, reasoning::effort_values};
use serde_json::{Value, json};

pub(super) struct ReasoningProjection {
    pub(super) state: &'static str,
    pub(super) reason_code: Option<&'static str>,
    pub(super) fields: Value,
    pub(super) omitted: Vec<String>,
}

fn project_reasoning(
    agent: &str,
    protocol: &str,
    values: &[String],
) -> Result<ReasoningProjection> {
    match agent {
        "pi" => pi_omp::pi_reasoning(values),
        "omp" => pi_omp::omp_reasoning(values),
        "dsh" => dsh::project_reasoning(values),
        "opencode" => opencode::project_reasoning(protocol, values),
        _ => Ok(ReasoningProjection {
            state: "not_supported",
            reason_code: Some("native_reasoning_options_unavailable"),
            fields: json!({}),
            omitted: Vec::new(),
        }),
    }
}

pub(in crate::agent_integration) fn projection_view(
    agent: &str,
    protocol: &str,
    effective: &Value,
) -> Result<Value> {
    if matches!(agent, "codex" | "hermes") {
        return Ok(
            json!({"state":"not_supported","reason_code":"native_reasoning_options_unavailable","fields":{},"omitted":[]}),
        );
    }
    let Some(values) = effort_values(effective) else {
        let reason = if effective.get("reasoning").and_then(Value::as_bool) == Some(false) {
            "reasoning_disabled"
        } else {
            "reasoning_options_unavailable"
        };
        return Ok(json!({"state":"unavailable","reason_code":reason,"fields":{},"omitted":[]}));
    };
    let projection = project_reasoning(agent, protocol, &values)?;
    Ok(json!({
        "state": projection.state,
        "reason_code": projection.reason_code,
        "fields": projection.fields,
        "omitted": projection.omitted,
    }))
}

pub(super) fn catalog_model_metadata(effective: &Value, include_output: bool) -> Result<Value> {
    let mut metadata = json!({});
    for (source, native) in [("context", "contextWindow"), ("output", "maxTokens")] {
        // DSH catalog.ts PiAiModelProfile.maxTokens also chooses a per-request
        // default. Leave output capacity unmanaged rather than change requests.
        if !include_output && source == "output" {
            continue;
        }
        if let Some(value) = effective.get("limit").and_then(|limit| limit.get(source))
            && !value.is_null()
        {
            // Qualified Pi, OMP, and DSH schemas reject nonpositive limits.
            // JS numbers must retain the exact integer.
            let count = value.as_u64().filter(|count| *count > 0 && *count <= 9_007_199_254_740_991)
                .ok_or_else(|| Error::new(
                    "native_metadata_unrepresentable",
                    "This concrete model limit cannot be represented exactly by the qualified native schema; select a representable explicit limit before applying",
                ).field(format!("limit.{source}")))?;
            metadata[native] = json!(count);
        }
    }
    if let Some(reasoning) = effective.get("reasoning").and_then(Value::as_bool) {
        metadata["reasoning"] = json!(reasoning);
    }
    if let Some(input) = effective
        .pointer("/modalities/input")
        .and_then(Value::as_array)
    {
        // Qualified native schemas support only text and image, not the
        // provider's entire modality vocabulary.
        metadata["input"] = json!(
            input
                .iter()
                .filter_map(Value::as_str)
                .filter(|modality| matches!(*modality, "text" | "image"))
                .collect::<Vec<_>>()
        );
    }
    Ok(metadata)
}

pub(super) fn append_reasoning(
    agent: &str,
    protocol: &str,
    effective: &Value,
    metadata: &mut Value,
) -> Result<()> {
    if matches!(agent, "pi" | "omp" | "opencode" | "dsh")
        && let Some(values) = effort_values(effective)
    {
        let projection = project_reasoning(agent, protocol, &values)?;
        if let Value::Object(fields) = projection.fields {
            metadata
                .as_object_mut()
                .expect("native model metadata object")
                .extend(fields);
        }
    }
    Ok(())
}

pub(super) fn api(protocol: &str) -> Result<&'static str> {
    match protocol {
        "chat-completions" => Ok("openai-completions"),
        "responses" => Ok("openai-responses"),
        "messages" => Ok("anthropic-messages"),
        _ => Err(Error::new(
            "unsupported_protocol",
            "Unsupported native protocol",
        )),
    }
}
