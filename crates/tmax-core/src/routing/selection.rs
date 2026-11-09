use super::priority::evaluate;
use crate::{
    Error, Result,
    config::{self, Config, Model, Provider},
    inputs::{self, Snapshot},
    storage::Store,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{borrow::Cow, collections::BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Selection {
    pub provider: String,
    pub model: String,
    pub model_identity: String,
    pub priority: i64,
    pub evidence: Value,
}

fn candidates(config: &Config, id: &str, protocol: &str) -> Result<(bool, Vec<(String, String)>)> {
    if !config::PROTOCOLS.contains(&protocol) {
        return Err(Error::new(
            "unsupported_protocol",
            "Unknown native protocol",
        ));
    }
    if let Some(alias) = config
        .model_aliases
        .iter()
        .find(|a| a.id == id && a.protocol == protocol)
    {
        if !alias.enabled {
            return Err(Error::new("disabled_model", "Model alias is disabled"));
        }
        let mut result = Vec::with_capacity(alias.candidates.len());
        for name in &alias.candidates {
            let (p, m) = config::qualified(name)?;
            config::model(config, name)?;
            result.push((p.into(), m.into()));
        }
        return Ok((true, result));
    }
    let (provider, model) = config::qualified(id)?;
    config::model(config, id)?;
    Ok((false, vec![(provider.into(), model.into())]))
}
fn eligibility(
    provider: &Provider,
    model: &Model,
    protocol: &str,
    inventory: Option<&Value>,
    provider_identity: Option<&str>,
) -> Option<&'static str> {
    if !provider.enabled {
        return Some("provider_disabled");
    }
    if !model.enabled {
        return Some("model_disabled");
    }
    if !crate::provider_integration::resolve(provider)
        .ok()
        .is_some_and(|endpoints| endpoints.contains_key(protocol))
    {
        return Some("endpoint_unavailable");
    }
    match &provider.auth {
        None => Some("credential_unavailable"),
        Some(config::Auth::ApiKey { key_ref }) if key_ref.is_empty() => {
            Some("credential_unavailable")
        }
        Some(config::Auth::ApiKey { key_ref })
            if inventory.is_some_and(|inventory| {
                let entry = inventory["entries"].as_array().and_then(|entries| {
                    entries.iter().find(|entry| entry["reference"] == *key_ref)
                });
                entry.is_some_and(|entry| {
                    entry["metadata_state"] != "known"
                        || provider_identity.is_some_and(|identity| {
                            entry["owner"]["identity"].as_str() != Some(identity)
                        })
                }) || entry.is_none() && inventory["coverage"] == "complete"
            }) =>
        {
            Some("credential_unavailable")
        }
        Some(config::Auth::Login { session_ref, .. }) if session_ref.is_none() => {
            Some("credential_unavailable")
        }
        _ => None,
    }
}
/// Request-scoped native client requirements. Routing never inspects content.
#[derive(Clone, Copy)]
pub struct RequestRequirements<'a> {
    pub native: &'a crate::gateway::NativeRequirements,
}

pub(crate) struct Advice<'a> {
    pub native: Option<crate::agent_integration::compatibility::Context<'a>>,
}

enum SelectionView<'a> {
    Saved {
        context: Option<&'a Advice<'a>>,
    },
    Loaded {
        metadata: Option<&'a crate::catalog::Snapshot>,
        request: Option<RequestRequirements<'a>>,
        context: Option<&'a Advice<'a>>,
    },
    Admitted {
        snapshot: &'a Snapshot<'a>,
        request: RequestRequirements<'a>,
        excluded_providers: &'a BTreeSet<String>,
    },
}
fn decision(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: DateTime<Utc>,
    exact_at: &inputs::ExactInstant,
    view: SelectionView<'_>,
) -> Result<(Value, Option<Selection>)> {
    let (snapshot, native, context, excluded_providers) = match view {
        SelectionView::Saved { context } => (
            Cow::Owned(Snapshot::capture(store, config)),
            None,
            context,
            None,
        ),
        SelectionView::Loaded {
            metadata,
            request,
            context,
        } => (
            Cow::Owned(Snapshot::capture_loaded(store, config, metadata)),
            request.map(|request| request.native),
            context,
            None,
        ),
        SelectionView::Admitted {
            snapshot,
            request,
            excluded_providers,
        } => (
            Cow::Borrowed(snapshot),
            Some(request.native),
            None,
            Some(excluded_providers),
        ),
    };
    let (alias, candidates) = candidates(config, id, protocol)?;
    let inventory = crate::secrets::inventory(store).ok();
    let authorization = context
        .and_then(|context| context.native.as_ref())
        .filter(|context| {
            !context.gateway
                && config
                    .providers
                    .values()
                    .any(|p| matches!(p.auth, Some(config::Auth::Login { .. })))
        })
        .map(|_| crate::agent_integration::compatibility::AuthorizationSnapshot::capture(store));
    let mut evaluations = Vec::new();
    let failures = crate::failures::Snapshot::capture(store, config)?;
    let mut ranked = Vec::new();
    let mut excluded = Vec::new();
    for (provider, name) in candidates {
        let p = &config.providers[&provider];
        let m = &p.models[&name];
        let effort = config.candidate_effort(id, protocol, &provider, &name);
        let evaluation = evaluate(config, &snapshot, &failures, &provider, &name, at, exact_at)?;
        let identity = snapshot.destination_identity(&provider, &name);
        let native_assessment =
            context
                .and_then(|context| context.native.as_ref())
                .map(|context| -> Result<()> {
                    context.assess(config, &provider, protocol, id)?;
                    if !context.gateway
                        && matches!(p.auth, Some(config::Auth::Login { .. }))
                        && let Some(authorization) = &authorization
                    {
                        authorization
                            .as_ref()
                            .map_err(Clone::clone)?
                            .assess(store, config, context, &provider)?;
                    }
                    Ok(())
                });
        let reason = excluded_providers
            .filter(|providers| providers.contains(&provider))
            .map(|_| "provider_already_attempted")
            .or_else(|| {
                native_assessment
                    .as_ref()
                    .and_then(|result| result.as_ref().err())
                    .map(|error| error.code.as_str())
            })
            .or_else(|| {
                eligibility(
                    p,
                    m,
                    protocol,
                    inventory.as_ref(),
                    identity.as_ref().map(|ids| ids.0.as_str()),
                )
            })
            .or_else(|| native.and_then(|native| crate::gateway::native_mismatch(p, native)));
        let observed_unavailability = if reason.is_none()
            && let Some((incarnation, authorization, _)) = &identity
        {
            crate::observations::routing_availability_for_admission(
                snapshot.as_ref(),
                config,
                &provider,
                exact_at,
                (incarnation, authorization),
            )
        } else {
            None
        };
        let quota_hold = if reason.is_none()
            && observed_unavailability.is_none()
            && let Some((incarnation, authorization, _)) = &identity
        {
            crate::observations::quota_guard_hold_for_binding(
                snapshot.as_ref(),
                config,
                &provider,
                &name,
                exact_at,
                (incarnation, authorization),
            )
        } else {
            None
        };
        let reason = reason
            .or(observed_unavailability.as_deref())
            .or(quota_hold.as_ref().map(|hold| hold.reason_code));
        let priority = evaluation["effective_priority"]
            .as_i64()
            .unwrap_or(m.priority);
        let qualified = format!("{provider}/{name}");
        if let Some(reason) = reason {
            let mut candidate = json!({"qualified_model":qualified,"reason_code":reason,"native_requirement":native_assessment.as_ref().and_then(|result| result.as_ref().err()).map(|error| json!({"code":error.code,"message":error.message}))});
            if let Some(hold) = &quota_hold {
                candidate["availability_hold"] = json!(hold);
            }
            excluded.push(candidate);
        } else if let Some((provider_identity, authorization_generation, identity)) = identity {
            ranked.push((
                priority,
                identity,
                provider.clone(),
                name.clone(),
                provider_identity,
                authorization_generation,
            ));
        } else {
            excluded
                .push(json!({"qualified_model":qualified,"reason_code":"identity_unavailable"}));
        }
        evaluations.push(json!({"qualified_model":qualified,"candidate_reasoning_effort":effort,"priority_used_for_ranking":alias,"priority_evaluation":evaluation}));
    }
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let selected=ranked.first().map(|(priority,identity,p,m,provider_identity,authorization_generation)|json!({"provider":p,"model":m,"qualified_model":format!("{p}/{m}"),"provider_identity":provider_identity,"authorization_generation":authorization_generation,"model_identity":identity,"priority":priority,"candidate_reasoning_effort":config.candidate_effort(id, protocol, p, m)}));
    let reason = if !alias {
        "fixed_concrete"
    } else if ranked
        .get(1)
        .is_some_and(|next| Some(next.0) == ranked.first().map(|r| r.0))
    {
        "stable_model_identity"
    } else {
        "highest_effective_priority"
    };
    let evidence = json!({"public_id":id,"protocol":protocol,"at":exact_at.canonical(),"configuration_revision":snapshot.configuration_revision(),"metadata_revision":snapshot.metadata_revision(),
        "priority_evaluations":evaluations,"excluded_candidates":excluded,"selection":selected,"selection_reason":reason});
    let selection = ranked
        .into_iter()
        .next()
        .map(|(priority, identity, provider, model, _, _)| Selection {
            provider,
            model,
            model_identity: identity,
            priority,
            evidence: evidence.clone(),
        });
    Ok((evidence, selection))
}
pub fn explain(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: Option<&str>,
) -> Result<Value> {
    explain_native(store, config, id, protocol, at, None)
}
pub(crate) fn explain_native(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: Option<&str>,
    context: Option<&Advice<'_>>,
) -> Result<Value> {
    let now = Utc::now();
    let text = at
        .map(str::to_owned)
        .unwrap_or_else(|| inputs::timestamp(now));
    let exact_at = inputs::ExactInstant::parse(&text)?;
    let at = inputs::instant(&text)?;
    decision(
        store,
        config,
        id,
        protocol,
        at,
        &exact_at,
        SelectionView::Saved { context },
    )
    .map(|(evidence, _)| evidence)
}
pub fn select(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: DateTime<Utc>,
) -> Result<Selection> {
    let (_, selection) = decision(
        store,
        config,
        id,
        protocol,
        at,
        &inputs::ExactInstant::from_chrono(at),
        SelectionView::Saved { context: None },
    )?;
    selection.ok_or_else(|| {
        Error::new(
            "model_unavailable",
            "No configured candidate has applicable identity, authorization, and protocol access",
        )
    })
}
fn request_unavailable(evidence: &Value) -> Error {
    // A fixed model, or candidates with the same blocker, has a precise
    // rejection reason. Do not erase it before the gateway records history.
    // Different blockers cannot truthfully be reduced to one candidate's cause.
    if let Some(excluded) = evidence["excluded_candidates"].as_array()
        && !excluded.is_empty()
        && excluded
            .iter()
            .all(|candidate| candidate["reason_code"].as_str() == Some("quota_guard"))
    {
        let earliest = excluded
            .iter()
            .filter_map(|candidate| candidate["availability_hold"]["valid_until"].as_str())
            .filter_map(|valid_until| inputs::ExactInstant::parse(valid_until).ok())
            .min()
            .map(|valid_until| valid_until.canonical());
        let message = match earliest {
            Some(valid_until) => format!(
                "All configured candidates are held by quota_guard; earliest recovery at {valid_until}"
            ),
            None => "All configured candidates are held by quota_guard".into(),
        };
        return Error::new("quota_guard", &message);
    }
    let common_reason = evidence["excluded_candidates"]
        .as_array()
        .and_then(|excluded| {
            excluded
                .first()
                .and_then(|candidate| candidate["reason_code"].as_str())
                .filter(|reason| {
                    excluded
                        .iter()
                        .all(|candidate| candidate["reason_code"].as_str() == Some(*reason))
                })
        });
    Error::new(
        common_reason.unwrap_or("model_unavailable"),
        "No configured candidate satisfies the request's native capability, authorization, and protocol requirements",
    )
}

pub fn select_request(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: DateTime<Utc>,
) -> Result<Selection> {
    let (evidence, selection) = decision(
        store,
        config,
        id,
        protocol,
        at,
        &inputs::ExactInstant::from_chrono(at),
        SelectionView::Saved { context: None },
    )?;
    selection.ok_or_else(|| request_unavailable(&evidence))
}

pub fn select_request_loaded(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: DateTime<Utc>,
    request: RequestRequirements<'_>,
    metadata: Option<&crate::catalog::Snapshot>,
) -> Result<Selection> {
    let (evidence, selection) = decision(
        store,
        config,
        id,
        protocol,
        at,
        &inputs::ExactInstant::from_chrono(at),
        SelectionView::Loaded {
            metadata,
            request: Some(request),
            context: None,
        },
    )?;
    selection.ok_or_else(|| request_unavailable(&evidence))
}

/// Re-rank the immutable admitted configuration without revisiting attempted providers.
#[allow(clippy::too_many_arguments)]
pub(crate) fn select_request_admitted(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: DateTime<Utc>,
    request: RequestRequirements<'_>,
    snapshot: &inputs::Snapshot<'_>,
    excluded_providers: &BTreeSet<String>,
    evidence_out: &mut Value,
) -> Result<Selection> {
    let (evidence, selection) = decision(
        store,
        config,
        id,
        protocol,
        at,
        &inputs::ExactInstant::from_chrono(at),
        SelectionView::Admitted {
            snapshot,
            request,
            excluded_providers,
        },
    )?;
    *evidence_out = evidence.clone();
    selection.ok_or_else(|| {
        if excluded_providers.is_empty() {
            request_unavailable(&evidence)
        } else {
            Error::new(
                "fallback_exhausted",
                "No unattempted provider satisfies the admitted request requirements",
            )
        }
    })
}

pub(crate) fn explain_loaded(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: Option<&str>,
    metadata: Option<&crate::catalog::Snapshot>,
    context: Option<&Advice<'_>>,
) -> Result<Value> {
    let now = Utc::now();
    let text = at
        .map(str::to_owned)
        .unwrap_or_else(|| inputs::timestamp(now));
    let exact_at = inputs::ExactInstant::parse(&text)?;
    let at = inputs::instant(&text)?;
    decision(
        store,
        config,
        id,
        protocol,
        at,
        &exact_at,
        SelectionView::Loaded {
            metadata,
            request: None,
            context,
        },
    )
    .map(|(evidence, _)| evidence)
}

#[cfg(test)]
mod tests {
    use super::{Advice, explain_loaded, explain_native, request_unavailable};
    use crate::{
        config::{self, Config, Model, Provider},
        inputs,
        storage::Store,
    };
    use serde_json::json;

    #[test]
    fn loaded_alias_advice_keeps_native_candidates_metadata_and_input_lineage_together() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("state"));
        let mut loaded = Config::empty();
        loaded.providers.insert(
            "old".into(),
            Provider {
                integration: Some("custom".into()),
                metadata: Some("catalog".into()),
                endpoints: [(
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                )]
                .into(),
                auth: Some(config::Auth::ApiKey {
                    key_ref: "not-resolved".into(),
                }),
                models: [(
                    "m".into(),
                    Model {
                        priority: 7,
                        ..Default::default()
                    },
                )]
                .into(),
                ..Default::default()
            },
        );
        loaded.model_aliases.push(config::ModelAlias {
            candidate_reasoning_effort: Default::default(),
            id: "L0".into(),
            protocol: "chat-completions".into(),
            candidates: vec!["old/m".into()],
            enabled: true,
            continuity: None,
            fallback: None,
        });
        store
            .write_raw("tmax.toml", toml::to_string(&loaded).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(&store, &Config::empty(), &loaded, &[]).unwrap();
        let loaded_revision = config::raw_revision(&store).unwrap();
        inputs::activate(&store, &loaded, &loaded_revision).unwrap();
        let old_metadata = crate::catalog::Snapshot {
            revision: "loaded-metadata".into(),
            providers: [(
                "catalog".into(),
                json!({"models":{"m":{"reasoning":false}}}),
            )]
            .into(),
            ..Default::default()
        };
        let new_metadata = crate::catalog::Snapshot {
            revision: "saved-metadata".into(),
            ..old_metadata.clone()
        };
        store
            .write_json("catalog/accepted.json", &new_metadata)
            .unwrap();
        let mut saved = loaded.clone();
        let provider = saved.providers.remove("old").unwrap();
        saved.providers.insert("new".into(), provider);
        saved.model_aliases[0].candidates = vec!["new/m".into()];
        saved
            .providers
            .get_mut("new")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .priority = 99;
        store
            .write_raw("tmax.toml", toml::to_string(&saved).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(&store, &loaded, &saved, &[]).unwrap();
        let context = Advice {
            native: Some(crate::agent_integration::compatibility::Context {
                agent: "codex",
                target: Some("old-target"),
                provider: None,
                gateway: false,
            }),
        };
        let evidence = explain_loaded(
            &store,
            &loaded,
            "L0",
            "chat-completions",
            None,
            Some(&old_metadata),
            Some(&context),
        )
        .unwrap();
        assert_eq!(evidence["metadata_revision"], "loaded-metadata");
        assert_eq!(evidence["configuration_revision"], loaded_revision);
        assert_eq!(
            evidence["excluded_candidates"][0]["qualified_model"],
            "old/m"
        );
        assert_eq!(
            evidence["excluded_candidates"][0]["reason_code"],
            "incompatible"
        );
        assert_eq!(
            evidence["priority_evaluations"][0]["priority_evaluation"]["effective_priority"],
            7
        );
        let saved_context = Advice {
            native: Some(crate::agent_integration::compatibility::Context {
                agent: "pi",
                target: Some("new-target"),
                provider: None,
                gateway: false,
            }),
        };
        let evidence = explain_native(
            &store,
            &saved,
            "L0",
            "chat-completions",
            None,
            Some(&saved_context),
        )
        .unwrap();
        assert_eq!(evidence["metadata_revision"], "saved-metadata");
        assert_eq!(
            evidence["excluded_candidates"][0]["qualified_model"],
            "new/m"
        );
        assert_eq!(
            evidence["priority_evaluations"][0]["priority_evaluation"]["effective_priority"],
            99
        );
    }

    #[test]
    fn request_rejection_preserves_shared_blockers_without_mislabeling_mixed_failures() {
        let fixed = json!({"excluded_candidates":[{"qualified_model":"p/m","reason_code":"native_session_required"}]});
        assert_eq!(request_unavailable(&fixed).code, "native_session_required");
        let shared = json!({"excluded_candidates":[
            {"qualified_model":"p/m","reason_code":"native_session_required"},
            {"qualified_model":"q/m","reason_code":"native_session_required"}
        ]});
        assert_eq!(request_unavailable(&shared).code, "native_session_required");
        let mixed = json!({"excluded_candidates":[
            {"qualified_model":"p/m","reason_code":"native_session_required"},
            {"qualified_model":"q/m","reason_code":"provider_disabled"}
        ]});
        assert_eq!(request_unavailable(&mixed).code, "model_unavailable");
        assert_eq!(
            request_unavailable(&json!({"excluded_candidates":[]})).code,
            "model_unavailable"
        );
        let held = request_unavailable(&json!({"excluded_candidates":[
            {"qualified_model":"p/m","reason_code":"quota_guard","availability_hold":{"valid_until":"2026-11-09T01:00:00Z"}},
            {"qualified_model":"q/m","reason_code":"quota_guard","availability_hold":{"valid_until":"2026-11-09T02:00:00Z"}}
        ]}));
        assert_eq!(held.code, "quota_guard");
        assert!(held.message.contains("2026-11-09T01:00:00Z"));
    }
}
