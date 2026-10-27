//! Finite known request/capability checks, including explicit candidate effort.
use crate::{
    catalog,
    config::{Model, Provider},
};
use serde_json::Value;
use std::collections::BTreeSet;

pub fn mismatch(
    provider: &Provider,
    model_id: &str,
    model: &Model,
    metadata: Option<&catalog::Snapshot>,
    body: &Value,
    candidate_effort: Option<(&str, &str)>,
) -> Option<&'static str> {
    check(
        provider,
        model_id,
        model,
        metadata,
        body,
        candidate_effort,
        true,
    )
}

/// Ranking must never depend on the caller's effort or a candidate's override.
pub fn ranking_mismatch(
    provider: &Provider,
    model_id: &str,
    model: &Model,
    metadata: Option<&catalog::Snapshot>,
    body: &Value,
) -> Option<&'static str> {
    check(provider, model_id, model, metadata, body, None, false)
}

/// Validate the already selected destination without changing its eligibility.
pub fn selected_mismatch(
    provider: &Provider,
    model_id: &str,
    model: &Model,
    metadata: Option<&catalog::Snapshot>,
    body: &Value,
    effort: Option<&str>,
    explicit: bool,
) -> Option<&'static str> {
    let projection =
        match catalog::effective_from_snapshot(metadata, provider, Some((model_id, model))) {
            Ok(value) => value,
            Err(_) => return Some("metadata_unavailable"),
        };
    let effective = &projection["effective"];
    if let Some(reason) = check_effective(effective, body, None, false) {
        return Some(reason);
    }
    let effort = effort?;
    if explicit {
        return crate::reasoning::supported(effective, effort);
    }
    // An identity request keeps the historical unknown-metadata behavior.
    if effort != "none" && effective.get("reasoning").and_then(Value::as_bool) == Some(false) {
        return Some("reasoning_incompatible");
    }
    if crate::reasoning::supported(effective, effort) == Some("reasoning_effort_incompatible") {
        return Some("reasoning_effort_incompatible");
    }
    None
}

fn check(
    provider: &Provider,
    model_id: &str,
    model: &Model,
    metadata: Option<&catalog::Snapshot>,
    body: &Value,
    candidate_effort: Option<(&str, &str)>,
    check_effort: bool,
) -> Option<&'static str> {
    let projection =
        match catalog::effective_from_snapshot(metadata, provider, Some((model_id, model))) {
            Ok(value) => value,
            Err(_) => return Some("metadata_unavailable"),
        };
    let effective = &projection["effective"];
    check_effective(effective, body, candidate_effort, check_effort)
}

fn check_effective(
    effective: &Value,
    body: &Value,
    candidate_effort: Option<(&str, &str)>,
    check_effort: bool,
) -> Option<&'static str> {
    if check_effort
        && let Some((protocol, effort)) = candidate_effort
        && let Some(reason) = crate::reasoning::supported(effective, effort)
            .or_else(|| crate::reasoning::request_mismatch(protocol, body, effort))
    {
        return Some(reason);
    }
    let messages = body.get("messages").and_then(Value::as_array);
    let input = body.get("input").and_then(Value::as_array);
    let tool_use = body
        .get("tools")
        .and_then(Value::as_array)
        .is_some_and(|v| !v.is_empty())
        || body
            .get("functions")
            .and_then(Value::as_array)
            .is_some_and(|v| !v.is_empty())
        || messages.into_iter().flatten().any(|m| {
            m.get("role").and_then(Value::as_str) == Some("tool")
                || m.get("tool_calls")
                    .and_then(Value::as_array)
                    .is_some_and(|v| !v.is_empty())
        })
        || input.into_iter().flatten().any(|m| {
            matches!(
                m.get("type").and_then(Value::as_str),
                Some("function_call" | "function_call_output")
            )
        })
        || messages
            .into_iter()
            .flatten()
            .filter_map(|m| m.get("content").and_then(Value::as_array))
            .flatten()
            .any(|c| {
                matches!(
                    c.get("type").and_then(Value::as_str),
                    Some("tool_use" | "tool_result")
                )
            });
    if tool_use && effective.get("tool_call").and_then(Value::as_bool) == Some(false) {
        return Some("tools_incompatible");
    }
    let effort = candidate_effort.map(|(_, effort)| effort).or_else(|| {
        body.get("reasoning_effort")
            .or_else(|| body.pointer("/reasoning/effort"))
            .and_then(Value::as_str)
    });
    let reasoning = (check_effort
        && candidate_effort.map_or_else(
            || {
                body.get("reasoning_effort")
                    .and_then(Value::as_str)
                    .is_some_and(|v| v != "none")
                    || body
                        .pointer("/reasoning/effort")
                        .and_then(Value::as_str)
                        .is_some_and(|v| v != "none")
            },
            |(_, effort)| effort != "none",
        ))
        || body
            .get("thinking")
            .and_then(|v| v.get("type"))
            .and_then(Value::as_str)
            .is_some_and(|v| matches!(v, "enabled" | "adaptive"));
    if reasoning && effective.get("reasoning").and_then(Value::as_bool) == Some(false) {
        return Some("reasoning_incompatible");
    }
    if body.get("temperature").is_some_and(|v| !v.is_null())
        && effective.get("temperature").and_then(Value::as_bool) == Some(false)
    {
        return Some("temperature_incompatible");
    }
    for field in ["max_tokens", "max_output_tokens", "max_completion_tokens"] {
        if let Some(count) = body.get(field) {
            for limit in [
                effective.pointer("/limit/output").and_then(Value::as_u64),
                effective.pointer("/limit/context").and_then(Value::as_u64),
            ]
            .into_iter()
            .flatten()
            {
                if exceeds(count, limit) {
                    return Some("output_budget_incompatible");
                }
            }
        }
    }
    if check_effort
        && candidate_effort.is_none()
        && let Some(effort) = effort
        && crate::reasoning::supported(effective, effort) == Some("reasoning_effort_incompatible")
    {
        return Some("reasoning_effort_incompatible");
    }
    let structured = body
        .pointer("/response_format/type")
        .or_else(|| body.pointer("/text/format/type"))
        .and_then(Value::as_str)
        == Some("json_schema");
    if structured && effective.get("structured_output").and_then(Value::as_bool) == Some(false) {
        return Some("structured_output_incompatible");
    }
    let mut modalities = BTreeSet::new();
    if body.get("input").is_some_and(Value::is_string) {
        modalities.insert("text");
    }
    for message in messages
        .into_iter()
        .flatten()
        .chain(input.into_iter().flatten())
    {
        if message.get("content").is_some_and(Value::is_string) {
            modalities.insert("text");
        }
        if let Some(content) = message.get("content").and_then(Value::as_array) {
            for part in content {
                modality(part, &mut modalities);
            }
        }
        modality(message, &mut modalities);
    }
    if let Some(allowed) = effective
        .pointer("/modalities/input")
        .and_then(Value::as_array)
        && modalities
            .iter()
            .any(|required| !allowed.iter().any(|value| value.as_str() == Some(required)))
    {
        return Some("input_modality_incompatible");
    }
    if let (Some(requested), Some(allowed)) = (
        body.get("modalities").and_then(Value::as_array),
        effective
            .pointer("/modalities/output")
            .and_then(Value::as_array),
    ) && requested
        .iter()
        .filter_map(Value::as_str)
        .any(|required| !allowed.iter().any(|value| value.as_str() == Some(required)))
    {
        return Some("output_modality_incompatible");
    }
    None
}
fn exceeds(count: &Value, limit: u64) -> bool {
    if let Some(count) = count.as_u64() {
        return count > limit;
    }
    let Some(number) = count.as_number() else {
        return false;
    };
    let text = number.to_string();
    if text.starts_with('-') {
        return false;
    }
    let (mantissa, exponent) = text.split_once(['e', 'E']).unwrap_or((&text, "0"));
    let fractional = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let mut significant = mantissa
        .bytes()
        .filter(|byte| *byte != b'.')
        .skip_while(|byte| *byte == b'0')
        .peekable();
    if significant.peek().is_none() {
        return false;
    }
    if limit == 0 {
        return true;
    }
    let digits = significant.clone().count();
    let exponent = match exponent.parse::<i64>() {
        Ok(value) => value,
        Err(_) => return !exponent.starts_with('-'),
    };
    let integer_digits = (digits as i128) + (exponent as i128) - (fractional as i128);
    let bound = limit.to_string();
    if integer_digits != (bound.len() as i128) {
        return integer_digits > (bound.len() as i128);
    }
    for expected in bound.bytes() {
        match significant.next().unwrap_or(b'0').cmp(&expected) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {}
        }
    }
    significant.any(|byte| byte != b'0')
}
fn modality(part: &Value, modalities: &mut BTreeSet<&'static str>) {
    match part.get("type").and_then(Value::as_str) {
        Some("text" | "input_text") => {
            modalities.insert("text");
        }
        Some("image" | "image_url" | "input_image") => {
            modalities.insert("image");
        }
        Some("audio" | "input_audio") => {
            modalities.insert("audio");
        }
        Some("document" | "input_file")
            if part.pointer("/source/media_type").and_then(Value::as_str)
                == Some("application/pdf") =>
        {
            modalities.insert("pdf");
        }
        Some("video" | "input_video") => {
            modalities.insert("video");
        }
        _ => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn ranking_ignores_effort_but_preserves_other_capability_requirements() {
        let provider = Provider::default();
        let mut model = Model::default();
        model.overrides.reasoning = Some(false);
        let body = json!({"reasoning_effort":"max"});
        assert_eq!(ranking_mismatch(&provider, "m", &model, None, &body), None);
        assert_eq!(
            mismatch(&provider, "m", &model, None, &body, None),
            Some("reasoning_incompatible")
        );
        assert_eq!(
            ranking_mismatch(
                &provider,
                "m",
                &model,
                None,
                &json!({"thinking":{"type":"enabled"}})
            ),
            Some("reasoning_incompatible")
        );
        model.overrides.tool_call = Some(false);
        assert_eq!(
            ranking_mismatch(
                &provider,
                "m",
                &model,
                None,
                &json!({"reasoning_effort":"max","tools":[{}]})
            ),
            Some("tools_incompatible")
        );
    }

    #[test]
    fn identity_effort_preserves_unknown_metadata_but_explicit_targets_require_values() {
        let provider = Provider::default();
        let model = Model::default();
        let body = json!({"reasoning_effort":"high"});
        assert_eq!(
            selected_mismatch(&provider, "m", &model, None, &body, Some("high"), false),
            None
        );
        assert_eq!(
            selected_mismatch(&provider, "m", &model, None, &body, Some("high"), true),
            Some("reasoning_effort_unavailable")
        );
    }

    #[test]
    fn explicit_false_and_zero_limits_are_not_unknown() {
        let p = Provider::default();
        let mut model = Model::default();
        model.overrides.tool_call = Some(false);
        assert_eq!(
            mismatch(
                &p,
                "m",
                &model,
                None,
                &json!({"tools":[{"type":"function"}]}),
                None
            ),
            Some("tools_incompatible")
        );
        model.overrides.limit = Some(crate::config::Limits {
            context: None,
            input: None,
            output: Some(0),
        });
        assert_eq!(
            mismatch(&p, "m", &model, None, &json!({"max_tokens":1}), None),
            Some("output_budget_incompatible")
        );
        assert_eq!(
            mismatch(&p, "m", &model, None, &json!({"max_tokens":0}), None),
            None
        );
    }
    #[test]
    fn opaque_extensions_are_not_interpreted_as_input() {
        let p = Provider::default();
        let mut model = Model::default();
        model.overrides.modalities = Some(crate::config::Modalities {
            input: Some(vec!["text".into()]),
            output: None,
        });
        assert_eq!(
            mismatch(
                &p,
                "m",
                &model,
                None,
                &json!({"messages":[{"role":"user","content":"x"}],"extension":{"type":"image"}}),
                None
            ),
            None
        );
        assert_eq!(
            mismatch(
                &p,
                "m",
                &model,
                None,
                &json!({"messages":[{"role":"user","content":[{"type":"image_url","image_url":{"url":"not retained"}}]}]}),
                None
            ),
            Some("input_modality_incompatible")
        );
    }
    #[test]
    fn separate_effort_declarations_form_one_supported_value_set() {
        let p = Provider::default();
        let mut model = Model::default();
        model.overrides.reasoning_options = Some(vec![
            crate::config::ReasoningOption {
                r#type: "effort".into(),
                values: Some(vec!["low".into()]),
            },
            crate::config::ReasoningOption {
                r#type: "effort".into(),
                values: Some(vec!["high".into()]),
            },
        ]);
        for effort in ["low", "high"] {
            assert_eq!(
                selected_mismatch(&p, "m", &model, None, &json!({}), Some(effort), false),
                None
            );
        }
        assert_eq!(
            selected_mismatch(&p, "m", &model, None, &json!({}), Some("max"), false),
            Some("reasoning_effort_incompatible")
        );
    }

    #[test]
    fn exponent_budget_comparison_is_exact_and_bounded() {
        assert!(exceeds(
            &serde_json::from_str("1e100000000").unwrap(),
            u64::MAX
        ));
        assert!(!exceeds(&serde_json::from_str("1e-100000000").unwrap(), 1));
        assert!(!exceeds(&serde_json::from_str("1.000e3").unwrap(), 1000));
        assert!(exceeds(
            &serde_json::from_str("1.000000000000000001e3").unwrap(),
            1000
        ));
    }
}
