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
fn project_limits(effective: &Value) -> Value {
    let context = effective
        .pointer("/limit/context")
        .filter(|v| !v.is_null())
        .cloned();
    let output = effective
        .pointer("/limit/output")
        .filter(|v| !v.is_null())
        .cloned();
    let known = [&context, &output].iter().filter(|v| v.is_some()).count();
    json!({
        "state": match known {
            2 => "projected",
            1 => "partial",
            _ => "unavailable",
        },
        "reason_code": if known == 2 { None } else { Some("limits_unknown") },
        "fields": {"contextWindow": context, "maxTokens": output},
        "omitted": [],
    })
}

pub(in crate::agent_integration) fn projection_view(
    agent: &str,
    protocol: &str,
    effective: &Value,
) -> Result<Value> {
    let mut view = if matches!(agent, "codex" | "hermes") {
        json!({"state":"not_supported","reason_code":"native_reasoning_options_unavailable","fields":{},"omitted":[]})
    } else if let Some(values) = effort_values(effective) {
        match project_reasoning(agent, protocol, &values) {
            Ok(projection) => json!({
                "state": projection.state,
                "reason_code": projection.reason_code,
                "fields": projection.fields,
                "omitted": projection.omitted,
            }),
            Err(error) => json!({
                "state":"blocked", "reason_code":error.code,
                "fields":{}, "omitted":values, "message":error.message,
            }),
        }
    } else {
        let reason = if effective.get("reasoning").and_then(Value::as_bool) == Some(false) {
            "reasoning_disabled"
        } else {
            "reasoning_options_unavailable"
        };
        json!({"state":"unavailable","reason_code":reason,"fields":{},"omitted":[]})
    };
    view["limits"] = project_limits(effective);
    view["input"] = project_input(agent, effective);
    Ok(view)
}

/// How far the target tool is known to complete unknown model facts itself.
/// No agent's self-completion is a tmax contract; "none" marks a static
/// configuration surface that keeps bare entries bare.
pub(in crate::agent_integration) fn native_completion(agent: &str) -> &'static str {
    match agent {
        "dsh" => "none",
        "pi" | "omp" | "opencode" | "hermes" | "codex" => "undocumented",
        _ => "unknown",
    }
}

fn project_input(agent: &str, effective: &Value) -> Value {
    if !matches!(agent, "pi" | "omp" | "dsh" | "opencode" | "hermes") {
        return json!({"state":"not_supported","reason_code":"native_input_not_supported","fields":{},"omitted":[],"message":"The qualified native adapter has no input-capability field."});
    }
    let Some(input) = effective
        .pointer("/modalities/input")
        .and_then(Value::as_array)
    else {
        return json!({"state":"unavailable","reason_code":"input_modalities_unknown","fields":{},"omitted":[],"message":"Input support is unknown. No capability is inferred; previously owned declarations are withdrawn. Native defaults or unowned declarations may still apply."});
    };
    if agent == "opencode" {
        return json!({"state":"projected","reason_code":null,"fields":{"capabilities":{"input":input}},"omitted":[]});
    }
    let supported = ["text", "image"];
    let represented = input
        .iter()
        .filter(|value| {
            value
                .as_str()
                .is_some_and(|value| supported.contains(&value))
        })
        .cloned()
        .collect::<Vec<_>>();
    let omitted = input
        .iter()
        .filter(|value| {
            !value
                .as_str()
                .is_some_and(|value| supported.contains(&value))
        })
        .cloned()
        .collect::<Vec<_>>();
    if agent == "dsh" && represented.is_empty() {
        return json!({"state":"blocked","reason_code":"native_input_unrepresentable","fields":{},"omitted":omitted,"message":"DSH treats an empty input list as native inheritance; this declared input capability cannot be represented."});
    }
    if agent == "hermes" {
        let has_text = input.iter().any(|value| value.as_str() == Some("text"));
        let partial = !omitted.is_empty() || !has_text;
        return json!({"state":if partial {"partial"} else {"projected"},"reason_code":if partial {Some("native_input_partial")} else {None},"fields":{"model_overrides":{"supports_vision":input.iter().any(|value| value.as_str() == Some("image"))}},"omitted":omitted,"message":"Hermes maps image to model_overrides.<provider>.<model>.supports_vision. Text handling is native and cannot be disabled; an empty input set cannot be enforced."});
    }
    json!({"state":if omitted.is_empty() {"projected"} else {"partial"},"reason_code":if omitted.is_empty() {None} else {Some("native_input_partial")},"fields":{"input":represented},"omitted":omitted})
}

pub(super) fn append_input(agent: &str, effective: &Value, metadata: &mut Value) -> Result<()> {
    let projection = project_input(agent, effective);
    if projection["state"] == "blocked" {
        return Err(Error::new(
            "native_metadata_unrepresentable",
            projection["message"]
                .as_str()
                .expect("blocked input explanation"),
        )
        .field("modalities.input"));
    }
    if let Some(fields) = projection["fields"].as_object() {
        for (key, value) in fields {
            if let (Some(existing), Some(additional)) = (
                metadata.get_mut(key).and_then(Value::as_object_mut),
                value.as_object(),
            ) {
                existing.extend(additional.clone());
            } else {
                metadata[key] = value.clone();
            }
        }
    }
    Ok(())
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
