//! Finite known request/capability checks, including explicit candidate effort.
use super::request_facts::RequestFacts;
use crate::{
    catalog,
    config::{Model, Provider},
};
use serde_json::Value;

pub fn mismatch(
    provider: &Provider,
    model_id: &str,
    model: &Model,
    metadata: Option<&catalog::Snapshot>,
    body: &RequestFacts,
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
    body: &RequestFacts,
) -> Option<&'static str> {
    check(provider, model_id, model, metadata, body, None, false)
}

/// Validate the already selected destination without changing its eligibility.
pub fn selected_mismatch(
    provider: &Provider,
    model_id: &str,
    model: &Model,
    metadata: Option<&catalog::Snapshot>,
    body: &RequestFacts,
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
    body: &RequestFacts,
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
    body: &RequestFacts,
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
    let tool_use = body.tool_use;
    if tool_use && effective.get("tool_call").and_then(Value::as_bool) == Some(false) {
        return Some("tools_incompatible");
    }
    let effort = candidate_effort.map(|(_, effort)| effort).or_else(|| {
        if body.reasoning_effort.present() {
            body.reasoning_effort.as_str()
        } else {
            body.nested_effort.as_str()
        }
    });
    let reasoning = (check_effort
        && candidate_effort.map_or_else(
            || {
                body.reasoning_effort.as_str().is_some_and(|v| v != "none")
                    || body.nested_effort.as_str().is_some_and(|v| v != "none")
            },
            |(_, effort)| effort != "none",
        ))
        || body.thinking_enabled;
    if reasoning && effective.get("reasoning").and_then(Value::as_bool) == Some(false) {
        return Some("reasoning_incompatible");
    }
    if body.temperature && effective.get("temperature").and_then(Value::as_bool) == Some(false) {
        return Some("temperature_incompatible");
    }
    if check_effort
        && candidate_effort.is_none()
        && let Some(effort) = effort
        && crate::reasoning::supported(effective, effort) == Some("reasoning_effort_incompatible")
    {
        return Some("reasoning_effort_incompatible");
    }
    let structured = body.structured;
    if structured && effective.get("structured_output").and_then(Value::as_bool) == Some(false) {
        return Some("structured_output_incompatible");
    }
    let modalities = &body.input_modalities;
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
        Some(&body.output_modalities),
        effective
            .pointer("/modalities/output")
            .and_then(Value::as_array),
    ) && requested
        .iter()
        .any(|required| !allowed.iter().any(|value| value.as_str() == Some(required)))
    {
        return Some("output_modality_incompatible");
    }
    None
}
#[cfg(test)]
fn facts(value: serde_json::Value) -> RequestFacts {
    RequestFacts::from_value(value).unwrap()
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
        let body = RequestFacts::from_value(serde_json::json!({"reasoning_effort":"max"})).unwrap();
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
                &facts(json!({"thinking":{"type":"enabled"}}))
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
                &facts(json!({"reasoning_effort":"max","tools":[{}]}))
            ),
            Some("tools_incompatible")
        );
    }

    #[test]
    fn identity_effort_preserves_unknown_metadata_but_explicit_targets_require_values() {
        let provider = Provider::default();
        let model = Model::default();
        let body =
            RequestFacts::from_value(serde_json::json!({"reasoning_effort":"high"})).unwrap();
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
                &facts(json!({"tools":[{"type":"function"}]})),
                None
            ),
            Some("tools_incompatible")
        );
        model.overrides.limit = Some(crate::config::Limits {
            context: None,
            input: None,
            output: Some(0),
        });
        // A requested output budget is forwarded, never pre-rejected: the
        // upstream clamps or rejects it with its own truthful error.
        assert_eq!(
            mismatch(&p, "m", &model, None, &facts(json!({"max_tokens":1})), None),
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
                &facts(
                    json!({"messages":[{"role":"user","content":"x"}],"extension":{"type":"image"}})
                ),
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
                &facts(
                    json!({"messages":[{"role":"user","content":[{"type":"image_url","image_url":{"url":"not retained"}}]}]})
                ),
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
                selected_mismatch(
                    &p,
                    "m",
                    &model,
                    None,
                    &facts(json!({})),
                    Some(effort),
                    false
                ),
                None
            );
        }
        assert_eq!(
            selected_mismatch(&p, "m", &model, None, &facts(json!({})), Some("max"), false),
            Some("reasoning_effort_incompatible")
        );
    }
}
