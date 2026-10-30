use crate::{
    Error, Result,
    config::{self, Config},
    inputs::{self, Runtime},
    provider_integration::{self, AcquiredObservation},
    storage::Store,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const STATE: &str = "observations/heads.json";
const SCOPES: [&str; 3] = ["auth", "models", "quota"];

// Labels are deliberately not part of either the key or retained facts. Renaming a
// provider changes only the projection, not the observation's authorization owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    provider_identity: String,
    authorization_generation: String,
    integration: String,
    endpoint_contract: String,
    source: Option<String>,
    source_contract: Option<String>,
    scope: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fact {
    revision: String,
    observed_at: DateTime<Utc>,
    data: Value,
    status: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attempt {
    revision: String,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    outcome: String,
    reason_code: Option<String>,
    head_revision_before: String,
    head_revision_after: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Head {
    binding: Binding,
    revision: String,
    fact: Option<Fact>,
    last_attempt: Attempt,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Heads {
    heads: BTreeMap<String, Head>,
}

fn signature(value: &impl Serialize) -> String {
    config::revision(&serde_json::to_vec(value).expect("observation metadata serialization"))
}
fn token() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn binding_with_scope(
    config: &Config,
    provider: &str,
    scope: &str,
    identity: (String, String),
) -> Result<Binding> {
    let p = config
        .providers
        .get(provider)
        .ok_or_else(|| Error::new("unknown_resource", "Provider is not configured"))?;
    let destination = provider_integration::observation_destination(p, scope)?;
    Ok(Binding {
        provider_identity: identity.0,
        authorization_generation: identity.1,
        integration: provider_integration::integration_id(p)?.into(),
        endpoint_contract: signature(&provider_integration::resolve(p)?),
        source: destination.map(|d| d.source.into()),
        source_contract: destination.map(|d| d.contract.into()),
        scope: scope.into(),
    })
}
fn binding(
    runtime: &Runtime,
    config: &Config,
    provider: &str,
    scope: &str,
) -> Result<Option<Binding>> {
    inputs::provider_scope_binding(runtime, config, provider)
        .map(|ids| binding_with_scope(config, provider, scope, ids))
        .transpose()
}
fn revision(head: Option<&Head>) -> &str {
    head.map(|h| h.revision.as_str()).unwrap_or("absent")
}
fn invalid_record() -> Error {
    Error::new(
        "invalid_record",
        "Provider observation state fails its storage contract",
    )
}
fn valid_data(source: Option<&str>, data: &Value) -> bool {
    match source {
        Some("official_balance_api") => {
            provider_integration::normalize_observation("official_balance_api", data)
                .is_ok_and(|normalized| normalized == *data)
        }
        Some(
            source @ ("go_usage_api"
            | "kimi_usage_api"
            | "glm_monitor_api"
            | "stepfun_usage_api"
            | "minimax_plan_remains_api"),
        ) => provider_integration::normalize_observation(source, data)
            .is_ok_and(|normalized| normalized == *data),
        Some("official_models_api") => {
            let Some(models) = data.get("models").and_then(Value::as_array) else {
                return false;
            };
            let body = json!({"data":models.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()});
            provider_integration::normalize_observation("official_models_api", &body)
                .is_ok_and(|normalized| normalized == *data)
        }
        _ => false,
    }
}
fn read(store: &Store) -> Result<Heads> {
    let Some(bytes) = crate::storage::read_optional(&store.path(STATE)?)? else {
        return Ok(Heads::default());
    };
    inputs::check_historical_field_uniqueness(&bytes).map_err(|_| invalid_record())?;
    let heads: Heads = serde_json::from_slice(&bytes).map_err(|_| invalid_record())?;
    for (key, head) in &heads.heads {
        let b = &head.binding;
        let a = &head.last_attempt;
        if key != &signature(b)
            || b.provider_identity.is_empty()
            || b.authorization_generation.is_empty()
            || !SCOPES.contains(&b.scope.as_str())
            || head.revision.is_empty()
            || a.revision.is_empty()
            || a.finished_at < a.started_at
            || a.head_revision_after != head.revision
            || a.head_revision_before.is_empty()
            || !["completed", "failed", "unavailable"].contains(&a.outcome.as_str())
            || (a.outcome == "completed") != a.reason_code.is_none()
            || head.fact.as_ref().is_some_and(|f| {
                f.revision.is_empty()
                    || !(200..300).contains(&f.status)
                    || !valid_data(b.source.as_deref(), &f.data)
            })
            || (a.outcome == "completed" && head.fact.is_none())
        {
            return Err(invalid_record());
        }
    }
    let mut canonical = Heads::default();
    for (_, mut head) in heads.heads {
        if let Some(candidates) =
            crate::retained_integration::regional_candidates(&head.binding.integration)
        {
            let mut matching = candidates.iter().filter(|(_, id)| {
                provider_integration::fixed_endpoints(id)
                    .is_ok_and(|endpoints| signature(&endpoints) == head.binding.endpoint_contract)
            });
            let id = matching.next().ok_or_else(invalid_record)?.1;
            if matching.next().is_some() {
                return Err(invalid_record());
            }
            let provider = config::Provider {
                integration: Some(id.into()),
                ..Default::default()
            };
            let destination =
                provider_integration::observation_destination(&provider, &head.binding.scope)
                    .map_err(|_| invalid_record())?;
            if head.binding.source.as_deref() != destination.map(|d| d.source)
                || head.binding.source_contract.as_deref() != destination.map(|d| d.contract)
            {
                return Err(invalid_record());
            }
            head.binding.integration = id.into();
        } else if provider_integration::fixed_endpoints(&head.binding.integration).is_err() {
            return Err(invalid_record());
        }
        if canonical
            .heads
            .insert(signature(&head.binding), head)
            .is_some()
        {
            return Err(invalid_record());
        }
    }
    Ok(canonical)
}
fn retained_bindings(runtime: &Runtime) -> Result<BTreeSet<String>> {
    let mut keys = BTreeSet::new();
    for view in std::iter::once(&runtime.saved).chain(runtime.loaded.iter()) {
        for provider in view.config.providers.keys() {
            let Some(ids) = inputs::provider_scope_binding_in_view(runtime, view, provider) else {
                continue;
            };
            for scope in SCOPES {
                keys.insert(signature(&binding_with_scope(
                    &view.config,
                    provider,
                    scope,
                    ids.clone(),
                )?));
            }
        }
    }
    Ok(keys)
}
fn prune(heads: &mut Heads, runtime: &Runtime) -> Result<()> {
    let keys = retained_bindings(runtime)?;
    heads.heads.retain(|k, _| keys.contains(k));
    Ok(())
}

/// Called by the input lifecycle owner with the routing lock already held.
/// Keep only current saved/still-loaded bindings, never an observation archive.
pub(crate) fn retain(store: &Store, runtime: &Runtime) -> Result<()> {
    let _observations = store.lock("observations")?;
    let mut heads = read(store)?;
    let before = heads.heads.len();
    prune(&mut heads, runtime)?;
    if heads.heads.len() != before {
        store.write_json(STATE, &heads)?;
    }
    Ok(())
}

struct Capture {
    binding: Binding,
    head_revision: String,
    started_at: DateTime<Utc>,
}
fn capture(store: &Store, config: &Config, provider: &str, scope: &str) -> Result<Capture> {
    let _configuration = store.lock("configuration")?;
    let saved = config::load(store)?;
    if signature(&saved.config) != signature(config) {
        return Err(Error::new(
            "observations_changed",
            "Saved provider authority changed before acquisition",
        ));
    }
    // Runtime mutations establish identity through the existing input owner, never
    // through an observation-specific identity registry or an inspection side effect.
    let runtime = inputs::read_runtime(store)?;
    if signature(&runtime.saved.config) != signature(config) {
        inputs::reconcile(store, &runtime.saved.config, config, &[])?;
    }
    let _routing = store.lock("routing")?;
    let _observations = store.lock("observations")?;
    let runtime = inputs::read_runtime(store)?;
    let b = binding(&runtime, config, provider, scope)?.ok_or_else(|| {
        Error::new(
            "credential_unavailable",
            "Provider authorization binding is unavailable",
        )
    })?;
    let heads = read(store)?;
    Ok(Capture {
        head_revision: revision(heads.heads.get(&signature(&b))).into(),
        binding: b,
        started_at: Utc::now(),
    })
}

// This is the publication boundary: a failed attempt changes only last_attempt;
// neither a network result nor a late writer may silently replace a newer head.
fn publish(
    heads: &mut Heads,
    captured: &Capture,
    current: Option<&Binding>,
    acquired: Result<AcquiredObservation>,
    finished_at: DateTime<Utc>,
) -> Result<Attempt> {
    if current != Some(&captured.binding) {
        return Err(Error::new(
            "observations_changed",
            "Provider observation binding changed during acquisition",
        ));
    }
    let key = signature(&captured.binding);
    let prior = heads.heads.get(&key);
    if revision(prior) != captured.head_revision {
        return Err(Error::new(
            "observations_changed",
            "Provider observation head changed during acquisition",
        ));
    }
    let new_revision = token();
    let (fact, outcome, reason_code) = match acquired {
        Ok(value) => (
            Some(Fact {
                revision: token(),
                observed_at: finished_at,
                data: value.data,
                status: value.status,
            }),
            "completed",
            None,
        ),
        Err(error) => (
            prior.and_then(|h| h.fact.clone()),
            if matches!(
                error.code.as_str(),
                "credential_unavailable" | "unsupported_source"
            ) {
                "unavailable"
            } else {
                "failed"
            },
            Some(error.code),
        ),
    };
    let attempt = Attempt {
        revision: token(),
        started_at: captured.started_at,
        finished_at,
        outcome: outcome.into(),
        reason_code,
        head_revision_before: captured.head_revision.clone(),
        head_revision_after: new_revision.clone(),
    };
    heads.heads.insert(
        key,
        Head {
            binding: captured.binding.clone(),
            revision: new_revision,
            fact,
            last_attempt: attempt.clone(),
        },
    );
    Ok(attempt)
}
fn current_saved_binding(
    store: &Store,
    runtime: &Runtime,
    captured: &Binding,
) -> Result<Option<Binding>> {
    let saved = config::load(store)?;
    // Explicit renames preserve identity. Recreating the same label never does.
    for provider in saved.config.providers.keys() {
        if let Some(now) = binding(runtime, &saved.config, provider, &captured.scope)?
            && now.provider_identity == captured.provider_identity
        {
            return Ok(Some(now));
        }
    }
    Ok(None)
}
fn commit(
    store: &Store,
    captured: &Capture,
    acquired: Result<AcquiredObservation>,
) -> Result<Attempt> {
    let _configuration = store.lock("configuration")?;
    let _routing = store.lock("routing")?;
    let _observations = store.lock("observations")?;
    let runtime = inputs::read_runtime(store)?;
    let current = current_saved_binding(store, &runtime, &captured.binding)?;
    let mut heads = read(store)?;
    let attempt = publish(&mut heads, captured, current.as_ref(), acquired, Utc::now())?;
    prune(&mut heads, &runtime)?;
    store.write_json(STATE, &heads)?;
    Ok(attempt)
}

pub async fn refresh(
    store: &Store,
    config: &Config,
    provider_id: &str,
    scope: &str,
    dry_run: bool,
) -> Result<Value> {
    let provider = config
        .providers
        .get(provider_id)
        .ok_or_else(|| Error::new("unknown_resource", "Provider is not configured"))?;
    let scopes = if scope == "all" {
        SCOPES.as_slice()
    } else if SCOPES.contains(&scope) {
        std::slice::from_ref(&scope)
    } else {
        return Err(Error::new("invalid_arguments", "Unknown refresh scope"));
    };
    let mut checks = Vec::with_capacity(scopes.len());
    for scope in scopes {
        let destination = provider_integration::observation_destination(provider, scope)?;
        if dry_run {
            checks.push(
                json!({"scope":scope,"input_name":null,"outcome":"not_executed",
                "reason_code":if destination.is_none(){Some("unsupported_source")}else{None},
                "attempt":null,"publication":"not_performed"}),
            );
            continue;
        }
        let captured = capture(store, config, provider_id, scope)?;
        // No publication/configuration lock is held during credential or source I/O.
        let acquired = match destination {
            Some(destination) => {
                provider_integration::acquire_observation(store, config, provider, destination)
                    .await
            }
            None => Err(Error::new(
                "unsupported_source",
                "No supported observation source for this scope",
            )),
        };
        match commit(store, &captured, acquired) {
            Ok(attempt) => checks.push(json!({"scope":scope,"input_name":null,
                "outcome":attempt.outcome,"reason_code":attempt.reason_code,"attempt":attempt,
                "published":attempt.outcome=="completed","attempt_recorded":true,
                "dashboard":provider_integration::dashboard(provider_integration::integration_id(provider)?)})),
            Err(error) => checks.push(json!({"scope":scope,"input_name":null,"outcome":"failed",
                "reason_code":error.code,"attempt":null,"published":false,"attempt_recorded":false})),
        }
    }
    Ok(json!({"provider":provider_id,"checks":checks}))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TemporalState {
    AsOfObservation,
    Unqualified,
    Missing,
}
#[derive(Serialize)]
struct TemporalApplicability {
    state: TemporalState,
    reason_code: Option<&'static str>,
    as_of: Option<DateTime<Utc>>,
    valid_until: Option<DateTime<Utc>>,
}
fn temporal_applicability(
    fact: Option<&Fact>,
    source: Option<&str>,
    at: &inputs::ExactInstant,
) -> TemporalApplicability {
    let Some(fact) = fact else {
        return TemporalApplicability {
            state: TemporalState::Missing,
            reason_code: Some("missing"),
            as_of: None,
            valid_until: None,
        };
    };
    // The balance contract establishes availability as of this observation,
    // not during any invented interval following retrieval. Discovery has no
    // balance-availability authority even at its observation instant.
    let observed = inputs::ExactInstant::from_chrono(fact.observed_at);
    let as_of_balance = source == Some("official_balance_api") && at == &observed;
    TemporalApplicability {
        state: if as_of_balance {
            TemporalState::AsOfObservation
        } else {
            TemporalState::Unqualified
        },
        reason_code: if as_of_balance {
            None
        } else if at < &observed {
            Some("observation_after_evaluation")
        } else {
            Some("source_validity_unavailable")
        },
        as_of: if matches!(
            source,
            Some("go_usage_api")
                | Some("kimi_usage_api")
                | Some("glm_monitor_api")
                | Some("stepfun_usage_api")
                | Some("minimax_plan_remains_api")
        ) {
            None
        } else {
            Some(fact.observed_at)
        },
        valid_until: None,
    }
}

fn render_scope(
    head: Option<&Head>,
    scope: &str,
    binding_known: bool,
    at: &inputs::ExactInstant,
) -> Value {
    json!({"scope":scope,"state":if head.and_then(|h|h.fact.as_ref()).is_some(){"observed"}else{"unknown"},
        "applicability":{"state":if !binding_known{"unavailable"}else if head.is_some(){"applicable"}else{"absent"},
            "reason_code":if !binding_known{Some("authorization_binding_unavailable")}else{None}},
        "source":head.and_then(|h|h.binding.source.as_deref()),
        "binding":head.map(|h|&h.binding),
        "current_revision":head.map(|h|h.revision.as_str()),
        "fact":head.and_then(|h|h.fact.as_ref()),
        "latest_attempt":head.map(|h|&h.last_attempt),
        "freshness":temporal_applicability(head.and_then(|h|h.fact.as_ref()),head.and_then(|h|h.binding.source.as_deref()),at),
        "observed_at_basis":"retrieval_time","source_provided_timestamp":null,
        "routing_eligibility_effect":"none","account_identity":null,
        "verification":{"models":"unverified","conversation":"unverified"}})
}

pub fn inspect(store: &Store, config: &Config, provider: &str) -> Result<Value> {
    let p = config
        .providers
        .get(provider)
        .ok_or_else(|| Error::new("unknown_resource", "Provider is not configured"))?;
    let _routing = store.read_lock("routing")?;
    let _observations = store.read_lock("observations")?;
    let runtime = inputs::read_runtime(store)?;
    let heads = read(store)?;
    let at = inputs::ExactInstant::from_chrono(Utc::now());
    let mut scopes = Vec::with_capacity(SCOPES.len());
    let mut balance = None;
    let mut go_usage = None;
    let mut plan_usage: Option<(&str, &Fact)> = None;
    for scope in SCOPES {
        let b = binding(&runtime, config, provider, scope)?;
        let head = b.as_ref().and_then(|b| heads.heads.get(&signature(b)));
        if let Some(fact) = head
            .filter(|h| h.binding.source.as_deref() == Some("official_balance_api"))
            .and_then(|h| h.fact.as_ref())
            && balance
                .as_ref()
                .is_none_or(|old: &&Fact| old.observed_at < fact.observed_at)
        {
            balance = Some(fact);
        }
        if scope == "quota" {
            go_usage = head
                .filter(|h| h.binding.source.as_deref() == Some("go_usage_api"))
                .and_then(|h| h.fact.as_ref());
            plan_usage = head
                .filter(|h| {
                    matches!(
                        h.binding.source.as_deref(),
                        Some(
                            "kimi_usage_api"
                                | "glm_monitor_api"
                                | "stepfun_usage_api"
                                | "minimax_plan_remains_api",
                        )
                    )
                })
                .and_then(|h| Some((h.binding.source.as_deref()?, h.fact.as_ref()?)));
        }
        scopes.push(render_scope(head, scope, b.is_some(), &at));
    }
    let billing_fact = balance.or(go_usage).or(plan_usage.map(|(_, fact)| fact));
    let billing_source = if balance.is_some() {
        "official_balance_api"
    } else if go_usage.is_some() {
        "go_usage_api"
    } else {
        plan_usage.map_or("", |(source, _)| source)
    };
    let plan_has_reset = plan_usage.is_some_and(|(source, fact)| {
        source != "kimi_usage_api"
            || fact.data["usage"].get("resetTime").is_some()
            || fact.data["limits"].as_array().is_some_and(|windows| {
                windows
                    .iter()
                    .any(|window| window["detail"].get("resetTime").is_some())
            })
    });
    let integration = provider_integration::integration_id(p)?;
    Ok(
        json!({"provider":provider,"account_identity":null,"observations":scopes,
        "billing":{"kind":match integration{"deepseek"=>"api_balance","opencode-go"=>"go_allowance","kimi-code-plan-cn"|"kimi-code-plan-global"|"glm-coding-plan"|"stepfun-token-plan"|"minimax-cn-coding-plan"|"minimax-coding-plan"=>"plan_allowance",_=>"unknown"},
            "state":if billing_fact.is_some(){"observed"}else{"unknown"},"observation":billing_fact,
            "freshness":temporal_applicability(billing_fact,Some(billing_source),&at),
            "subscription":p.subscription,"account_identity":null,
            "shared_upstream_limits":"unknown; providers may share upstream account limits",
            "aggregation":"not_performed","renewal":"unknown","reset":if go_usage.is_some(){"announced"}else if plan_has_reset{"provider_reported"}else{"unknown"},
            "dashboard":provider_integration::dashboard(integration)}}),
    )
}

pub fn summary(store: &Store, config: &Config, provider: &str) -> Result<Value> {
    let detail = inspect(store, config, provider)?;
    let scopes = detail["observations"]
        .as_array()
        .expect("observation projection")
        .iter()
        .map(|s| {
            json!({"scope":s["scope"],"state":s["state"],"applicability":s["applicability"],
            "source":s["source"],"observed_at":s["fact"]["observed_at"],"freshness":s["freshness"],
            "last_attempt_outcome":s["latest_attempt"]["outcome"],
            "last_attempt_reason_code":s["latest_attempt"]["reason_code"]})
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"provider":provider,"observations":scopes,"billing_state":detail["billing"]["state"],"account_identity":null}),
    )
}

/// Supported sources supply no validity interval establishing current balance
/// availability. A dated is_available:false is not a permanent routing veto.
/// This remains effect-free and never treats missing/corrupt evidence as exhaustion.
pub fn routing_availability(
    store: &Store,
    config: &Config,
    provider: &str,
    at: DateTime<Utc>,
) -> Result<Option<String>> {
    routing_availability_impl(
        store,
        config,
        provider,
        &inputs::ExactInstant::from_chrono(at),
        None,
    )
}

pub(crate) fn routing_availability_for_binding(
    store: &Store,
    config: &Config,
    provider: &str,
    at: &inputs::ExactInstant,
    scope: (&str, &str),
) -> Result<Option<String>> {
    routing_availability_impl(store, config, provider, at, Some(scope))
}

fn routing_availability_impl(
    store: &Store,
    config: &Config,
    provider: &str,
    at: &inputs::ExactInstant,
    captured_scope: Option<(&str, &str)>,
) -> Result<Option<String>> {
    if !config.providers.contains_key(provider) {
        return Err(Error::new("unknown_resource", "Provider is not configured"));
    }
    let _routing = store.read_lock("routing")?;
    let _observations = store.read_lock("observations")?;
    let runtime = inputs::read_runtime(store)?;
    let selected = if let Some((identity, authorization)) = captured_scope {
        // The caller captured an exact loaded destination. Never substitute an
        // identically spelled, recreated provider from the saved configuration.
        std::iter::once(&runtime.saved)
            .chain(runtime.loaded.iter())
            .find_map(|view| {
                inputs::provider_scope_binding_in_view(&runtime, view, provider)
                    .filter(|(i, a)| i == identity && a == authorization)
            })
    } else {
        inputs::provider_scope_binding(&runtime, config, provider)
    };
    let Some(selected) = selected else {
        return Ok(None);
    };
    let heads = read(store)?;
    for scope in ["quota", "auth"] {
        let current = binding_with_scope(config, provider, scope, selected.clone())?;
        let Some(head) = heads.heads.get(&signature(&current)) else {
            continue;
        };
        let Some(fact) = &head.fact else {
            continue;
        };
        let temporal = temporal_applicability(Some(fact), current.source.as_deref(), at);
        if temporal.state == TemporalState::AsOfObservation
            && fact.data.get("is_available").and_then(Value::as_bool) == Some(false)
        {
            return Ok(Some(format!(
                "balance_unavailable:{provider}:{scope}:as_of:{}",
                fact.observed_at.to_rfc3339()
            )));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn b() -> Binding {
        Binding {
            provider_identity: "incarnation".into(),
            authorization_generation: "authorization".into(),
            integration: "deepseek".into(),
            endpoint_contract: "endpoint".into(),
            source: Some("official_balance_api".into()),
            source_contract: Some("deepseek_balance_v1".into()),
            scope: "quota".into(),
        }
    }
    fn capture_head(heads: &Heads, binding: Binding) -> Capture {
        Capture {
            head_revision: revision(heads.heads.get(&signature(&binding))).into(),
            binding,
            started_at: Utc::now(),
        }
    }
    fn balance(available: bool) -> AcquiredObservation {
        AcquiredObservation {
            status: 200,
            data: json!({"is_available":available,"balance_infos":[
            {"currency":"USD","total_balance":"0001.2000","granted_balance":"0.2000","topped_up_balance":"1.0000"},
            {"currency":"CNY","total_balance":"2.000","granted_balance":"0.000","topped_up_balance":"2.000"}]}),
        }
    }
    #[test]
    fn failed_refresh_retains_last_good_dated_fact_and_exact_currencies() {
        let mut heads = Heads::default();
        let captured = capture_head(&heads, b());
        publish(
            &mut heads,
            &captured,
            Some(&b()),
            Ok(balance(false)),
            Utc::now(),
        )
        .unwrap();
        let old = serde_json::to_value(heads.heads.values().next().unwrap().fact.as_ref().unwrap())
            .unwrap();
        let captured = capture_head(&heads, b());
        publish(
            &mut heads,
            &captured,
            Some(&b()),
            Err(Error::new("transport_error", "safe")),
            Utc::now(),
        )
        .unwrap();
        let h = heads.heads.values().next().unwrap();
        assert_eq!(serde_json::to_value(&h.fact).unwrap(), old);
        assert_eq!(h.last_attempt.outcome, "failed");
        assert_eq!(
            old["data"]["balance_infos"][0]["total_balance"],
            "0001.2000"
        );
        assert_eq!(old["data"]["balance_infos"][1]["currency"], "CNY");
        assert_eq!(heads.heads.len(), 1);
    }
    #[test]
    fn late_authorization_removal_source_or_head_change_cannot_publish() {
        let mut heads = Heads::default();
        let captured = capture_head(&heads, b());
        let mut auth = b();
        auth.authorization_generation = "replacement".into();
        let mut source = b();
        source.source_contract = Some("changed".into());
        let mut endpoint = b();
        endpoint.endpoint_contract = "changed".into();
        for current in [Some(&auth), Some(&source), Some(&endpoint), None] {
            assert_eq!(
                publish(
                    &mut heads,
                    &captured,
                    current,
                    Ok(balance(false)),
                    Utc::now()
                )
                .unwrap_err()
                .code,
                "observations_changed"
            );
        }
        publish(
            &mut heads,
            &captured,
            Some(&b()),
            Ok(balance(true)),
            Utc::now(),
        )
        .unwrap();
        assert_eq!(
            publish(
                &mut heads,
                &captured,
                Some(&b()),
                Ok(balance(false)),
                Utc::now()
            )
            .unwrap_err()
            .code,
            "observations_changed"
        );
        assert_eq!(
            heads
                .heads
                .values()
                .next()
                .unwrap()
                .fact
                .as_ref()
                .unwrap()
                .data["is_available"],
            true
        );
    }
    fn fixture() -> (tempfile::TempDir, Store, Config) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().to_path_buf());
        let mut config = Config::empty();
        config.providers.insert(
            "original".into(),
            config::Provider {
                integration: Some("deepseek".into()),
                auth: Some(config::Auth::ApiKey {
                    key_ref: "key".into(),
                }),
                ..Default::default()
            },
        );
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        (dir, store, config)
    }
    #[test]
    fn uncertain_authorization_prunes_saved_and_loaded_facts_not_manual_subscription() {
        let (_dir, store, mut config) = fixture();
        config.providers.get_mut("original").unwrap().subscription = Some(config::Subscription {
            plan: Some("User-maintained plan".into()),
            auto_renew: Some(false),
            ..Default::default()
        });
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        let previous = inputs::read_runtime(&store).unwrap().saved.config;
        inputs::reconcile(&store, &previous, &config, &[]).unwrap();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let auth = capture(&store, &config, "original", "auth").unwrap();
        let quota = capture(&store, &config, "original", "quota").unwrap();
        commit(&store, &auth, Ok(balance(true))).unwrap();
        commit(&store, &quota, Ok(balance(true))).unwrap();
        assert_eq!(read(&store).unwrap().heads.len(), 2);
        let before = inputs::read_runtime(&store).unwrap();
        let original_config = std::fs::read(store.root.join("tmax.toml")).unwrap();
        {
            let _configuration = store.lock("configuration").unwrap();
            let _routing = store.lock("routing").unwrap();
            let _native = store.lock("native").unwrap();
            inputs::authorization_changed_locked(&store, "original").unwrap();
        }
        assert!(read(&store).unwrap().heads.is_empty());
        let after = inputs::read_runtime(&store).unwrap();
        assert_ne!(
            before.saved.providers["original"].authorization,
            after.saved.providers["original"].authorization
        );
        assert_eq!(
            after.saved.providers["original"].authorization,
            after.loaded.as_ref().unwrap().providers["original"].authorization
        );
        assert_eq!(
            std::fs::read(store.root.join("tmax.toml")).unwrap(),
            original_config
        );
        assert_eq!(
            inspect(&store, &config, "original").unwrap()["billing"]["subscription"]["plan"],
            "User-maintained plan"
        );
        assert_eq!(
            commit(&store, &auth, Ok(balance(false))).unwrap_err().code,
            "observations_changed"
        );
    }

    #[test]
    fn rename_projects_identity_without_orphans_and_removal_prunes() {
        let (_dir, store, config) = fixture();
        let captured = capture(&store, &config, "original", "quota").unwrap();
        commit(&store, &captured, Ok(balance(false))).unwrap();
        let mut renamed = config.clone();
        let provider = renamed.providers.remove("original").unwrap();
        renamed.providers.insert("renamed".into(), provider);
        store
            .write_raw("tmax.toml", toml::to_string(&renamed).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(
            &store,
            &config,
            &renamed,
            &[("original".into(), "renamed".into())],
        )
        .unwrap();
        assert_eq!(
            inspect(&store, &renamed, "renamed").unwrap()["billing"]["state"],
            "observed"
        );
        assert_eq!(read(&store).unwrap().heads.len(), 1);
        let empty = Config::empty();
        store
            .write_raw("tmax.toml", toml::to_string(&empty).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(&store, &renamed, &empty, &[]).unwrap();
        let _routing = store.lock("routing").unwrap();
        retain(&store, &inputs::read_runtime(&store).unwrap()).unwrap();
        assert!(read(&store).unwrap().heads.is_empty());
    }
    #[test]
    fn absent_is_unknown_corruption_is_not_absence_and_false_never_assumes_fresh_forever() {
        let (_dir, store, config) = fixture();
        assert_eq!(
            inspect(&store, &config, "original").unwrap()["billing"]["state"],
            "unknown"
        );
        let captured = capture(&store, &config, "original", "quota").unwrap();
        commit(&store, &captured, Ok(balance(false))).unwrap();
        assert_eq!(
            inspect(&store, &config, "original").unwrap()["billing"]["freshness"]["state"],
            "unqualified"
        );
        let observed_at = read(&store)
            .unwrap()
            .heads
            .values()
            .next()
            .unwrap()
            .fact
            .as_ref()
            .unwrap()
            .observed_at;
        assert!(
            routing_availability(&store, &config, "original", observed_at)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            routing_availability(
                &store,
                &config,
                "original",
                observed_at + chrono::Duration::nanoseconds(1)
            )
            .unwrap(),
            None
        );
        assert_eq!(
            routing_availability(
                &store,
                &config,
                "original",
                observed_at - chrono::Duration::nanoseconds(1)
            )
            .unwrap(),
            None
        );
        for at in [Utc::now(), "2099-01-01T00:00:00Z".parse().unwrap()] {
            assert_eq!(
                routing_availability(&store, &config, "original", at).unwrap(),
                None
            );
        }
        store.write_raw(STATE, b"{broken").unwrap();
        assert_eq!(
            inspect(&store, &config, "original").unwrap_err().code,
            "invalid_record"
        );
    }
    #[test]
    fn authorization_changes_after_source_io_reject_publication() {
        let (_dir, store, config) = fixture();
        let captured = capture(&store, &config, "original", "quota").unwrap();
        inputs::authorization_changed(&store, "original").unwrap();
        assert_eq!(
            commit(&store, &captured, Ok(balance(false)))
                .unwrap_err()
                .code,
            "observations_changed"
        );
        assert_eq!(
            inspect(&store, &config, "original").unwrap()["billing"]["state"],
            "unknown"
        );
    }
    #[test]
    fn routing_uses_exact_captured_binding_and_arbitrary_fractional_instants() {
        let (_dir, store, config) = fixture();
        let captured = capture(&store, &config, "original", "quota").unwrap();
        commit(&store, &captured, Ok(balance(false))).unwrap();
        let observed = read(&store)
            .unwrap()
            .heads
            .values()
            .next()
            .unwrap()
            .fact
            .as_ref()
            .unwrap()
            .observed_at;
        let at = inputs::ExactInstant::from_chrono(observed);
        let ids = (
            &*captured.binding.provider_identity,
            &*captured.binding.authorization_generation,
        );
        assert!(
            routing_availability_for_binding(&store, &config, "original", &at, ids)
                .unwrap()
                .is_some()
        );
        for wrong in [("other", ids.1), (ids.0, "other")] {
            assert_eq!(
                routing_availability_for_binding(&store, &config, "original", &at, wrong).unwrap(),
                None
            );
        }
        let whole = observed.format("%Y-%m-%dT%H:%M:%S");
        let later = inputs::ExactInstant::parse(&format!(
            "{whole}.{:09}1Z",
            observed.timestamp_subsec_nanos()
        ))
        .unwrap();
        assert_eq!(
            routing_availability_for_binding(&store, &config, "original", &later, ids).unwrap(),
            None
        );
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let empty = Config::empty();
        store
            .write_raw("tmax.toml", toml::to_string(&empty).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(&store, &config, &empty, &[]).unwrap();
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(&store, &empty, &config, &[]).unwrap();
        assert_eq!(
            routing_availability(&store, &config, "original", observed).unwrap(),
            None
        );
        assert!(
            routing_availability_for_binding(&store, &config, "original", &at, ids)
                .unwrap()
                .is_some()
        );
    }
    fn deepseek_models() -> AcquiredObservation {
        AcquiredObservation {
            status: 200,
            data: provider_integration::normalize_observation(
                "official_models_api",
                &json!({"data":[{"id":"deepseek-flash","owned_by":"not-account-identity"},{"id":"deepseek-v4-pro"}]}),
            )
            .unwrap(),
        }
    }

    #[test]
    fn deepseek_discovery_is_dated_read_only_evidence_not_balance_or_model_verification() {
        let (_dir, store, mut config) = fixture();
        config
            .providers
            .get_mut("original")
            .unwrap()
            .models
            .insert("configured-model".into(), config::Model::default());
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        let previous = inputs::read_runtime(&store).unwrap().saved.config;
        inputs::reconcile(&store, &previous, &config, &[]).unwrap();
        let catalog = crate::catalog::Snapshot {
            revision: "unchanged-directory".into(),
            ..Default::default()
        };
        for snapshot in ["accepted", "latest"] {
            store
                .write_json(&format!("catalog/{snapshot}.json"), &catalog)
                .unwrap();
        }
        let saved = std::fs::read(store.root.join("tmax.toml")).unwrap();
        let accepted = std::fs::read(store.root.join("catalog/accepted.json")).unwrap();
        let latest = std::fs::read(store.root.join("catalog/latest.json")).unwrap();
        let captured = capture(&store, &config, "original", "models").unwrap();
        commit(&store, &captured, Ok(deepseek_models())).unwrap();
        let detail = inspect(&store, &config, "original").unwrap();
        let models = &detail["observations"][1];
        assert_eq!(models["state"], "observed");
        assert_eq!(models["source"], "official_models_api");
        assert_eq!(models["binding"]["source_contract"], "deepseek_models_v1");
        assert_eq!(models["fact"]["data"], deepseek_models().data);
        assert_eq!(models["observed_at_basis"], "retrieval_time");
        assert!(models["source_provided_timestamp"].is_null());
        assert_eq!(models["freshness"]["state"], "unqualified");
        assert!(models["freshness"]["valid_until"].is_null());
        assert_eq!(models["verification"]["models"], "unverified");
        assert_eq!(models["verification"]["conversation"], "unverified");
        assert!(models["account_identity"].is_null());
        assert_eq!(detail["observations"][0]["state"], "unknown");
        assert_eq!(detail["observations"][2]["state"], "unknown");
        assert_eq!(detail["billing"]["state"], "unknown");
        assert_eq!(detail["billing"]["kind"], "api_balance");
        let observed = models["fact"]["observed_at"]
            .as_str()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .unwrap();
        assert_eq!(
            routing_availability(&store, &config, "original", observed).unwrap(),
            None
        );
        let summary = summary(&store, &config, "original").unwrap();
        assert_eq!(summary["observations"][1]["state"], "observed");
        assert_eq!(
            summary["observations"][1]["last_attempt_outcome"],
            "completed"
        );
        assert_eq!(std::fs::read(store.root.join("tmax.toml")).unwrap(), saved);
        assert_eq!(
            std::fs::read(store.root.join("catalog/accepted.json")).unwrap(),
            accepted
        );
        assert_eq!(
            std::fs::read(store.root.join("catalog/latest.json")).unwrap(),
            latest
        );
        assert_eq!(
            config::load(&store).unwrap().config.providers["original"]
                .models
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec!["configured-model"]
        );
    }

    #[test]
    fn deepseek_model_refresh_failures_retain_ids_and_leave_auth_and_quota_balance_unchanged() {
        let (_dir, store, config) = fixture();
        for scope in ["auth", "quota"] {
            let captured = capture(&store, &config, "original", scope).unwrap();
            commit(&store, &captured, Ok(balance(true))).unwrap();
        }
        let captured = capture(&store, &config, "original", "models").unwrap();
        commit(&store, &captured, Ok(deepseek_models())).unwrap();
        let before = inspect(&store, &config, "original").unwrap();
        let fact = before["observations"][1]["fact"].clone();
        for reason in [
            "source_format_error",
            "upstream_rejection",
            "transport_error",
            "observation_deadline",
            "credential_unavailable",
        ] {
            let captured = capture(&store, &config, "original", "models").unwrap();
            let acquired = if reason == "source_format_error" {
                provider_integration::normalize_observation(
                    "official_models_api",
                    &json!({"data":[{"id":"valid"},{"id":"bad\n"}]}),
                )
                .map(|data| AcquiredObservation { data, status: 200 })
            } else {
                Err(Error::new(reason, "safe failure"))
            };
            commit(&store, &captured, acquired).unwrap();
            let after = inspect(&store, &config, "original").unwrap();
            assert_eq!(after["observations"][1]["fact"], fact);
            assert_eq!(
                after["observations"][1]["latest_attempt"]["reason_code"],
                reason
            );
            assert_eq!(
                after["observations"][1]["latest_attempt"]["outcome"],
                if reason == "credential_unavailable" {
                    "unavailable"
                } else {
                    "failed"
                }
            );
            for index in [0, 2] {
                assert_eq!(
                    after["observations"][index]["fact"],
                    before["observations"][index]["fact"]
                );
                assert_eq!(
                    after["observations"][index]["source"],
                    "official_balance_api"
                );
                assert_eq!(
                    after["observations"][index]["binding"]["source_contract"],
                    "deepseek_balance_v1"
                );
            }
            assert_eq!(
                after["billing"]["observation"],
                before["billing"]["observation"]
            );
        }
    }

    #[test]
    fn deepseek_discovery_rejects_late_credentials_and_corrupt_retained_model_ids() {
        let (_dir, store, config) = fixture();
        let first = capture(&store, &config, "original", "models").unwrap();
        let late = capture(&store, &config, "original", "models").unwrap();
        commit(&store, &first, Ok(deepseek_models())).unwrap();
        assert_eq!(
            commit(&store, &late, Ok(deepseek_models()))
                .unwrap_err()
                .code,
            "observations_changed"
        );
        let captured = capture(&store, &config, "original", "models").unwrap();
        inputs::authorization_changed(&store, "original").unwrap();
        let after = inspect(&store, &config, "original").unwrap();
        assert_eq!(after["observations"][1]["state"], "unknown");
        assert!(after["observations"][1]["fact"].is_null());
        assert_eq!(
            commit(&store, &captured, Ok(deepseek_models()))
                .unwrap_err()
                .code,
            "observations_changed"
        );
        let captured = capture(&store, &config, "original", "models").unwrap();
        commit(&store, &captured, Ok(deepseek_models())).unwrap();
        assert_eq!(
            inspect(&store, &config, "original").unwrap()["observations"][1]["state"],
            "observed"
        );
        let mut heads = read(&store).unwrap();
        heads
            .heads
            .values_mut()
            .next()
            .unwrap()
            .fact
            .as_mut()
            .unwrap()
            .data["models"] = json!(["invalid\n"]);
        store.write_json(STATE, &heads).unwrap();
        assert_eq!(
            inspect(&store, &config, "original").unwrap_err().code,
            "invalid_record"
        );
    }
    fn go_usage() -> AcquiredObservation {
        AcquiredObservation {
            status: 200,
            data: provider_integration::normalize_observation(
                "go_usage_api",
                &json!({"usage":{
                    "rolling":{"status":"ok","percent":0,"resetsAt":"2026-10-23T19:04:44.687Z"},
                    "weekly":{"status":"ok","percent":0,"resetsAt":"2026-10-26T00:00:00.000Z"},
                    "monthly":{"status":"ok","percent":62,"resetsAt":"2026-11-09T03:13:27.000Z"}
                }}),
            )
            .unwrap(),
        }
    }
    fn go_fixture() -> (tempfile::TempDir, Store, Config) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().to_path_buf());
        let mut config = Config::empty();
        for id in ["go-a", "go-b"] {
            config.providers.insert(
                id.into(),
                config::Provider {
                    integration: Some("opencode-go".into()),
                    auth: Some(config::Auth::ApiKey {
                        key_ref: format!("key-{id}"),
                    }),
                    ..Default::default()
                },
            );
        }
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        (dir, store, config)
    }
    #[test]
    fn go_quota_failures_retain_exact_fact_without_sharing_or_inventing_availability() {
        let (_dir, store, config) = go_fixture();
        let captured = capture(&store, &config, "go-a", "quota").unwrap();
        commit(&store, &captured, Ok(go_usage())).unwrap();
        let before = inspect(&store, &config, "go-a").unwrap();
        let fact = before["billing"]["observation"].clone();
        assert_eq!(before["billing"]["kind"], "go_allowance");
        assert_eq!(before["billing"]["state"], "observed");
        assert_eq!(fact["data"]["usage"]["monthly"]["percent"], 62);
        assert_eq!(
            fact["data"]["usage"]["monthly"]["resetsAt"],
            "2026-11-09T03:13:27.000Z"
        );
        assert_eq!(before["billing"]["reset"], "announced");
        assert_eq!(before["billing"]["renewal"], "unknown");
        assert_eq!(before["billing"]["freshness"]["state"], "unqualified");
        assert!(before["billing"]["freshness"]["as_of"].is_null());
        assert!(before["billing"]["freshness"]["valid_until"].is_null());
        assert!(before["account_identity"].is_null());
        let quota = &before["observations"][2];
        assert_eq!(quota["source"], "go_usage_api");
        assert_eq!(quota["observed_at_basis"], "retrieval_time");
        assert!(quota["source_provided_timestamp"].is_null());
        assert!(quota["freshness"]["as_of"].is_null());
        for reason in [
            "source_format_error",
            "upstream_rejection",
            "transport_error",
            "observation_deadline",
            "credential_unavailable",
        ] {
            let captured = capture(&store, &config, "go-a", "quota").unwrap();
            commit(&store, &captured, Err(Error::new(reason, "safe failure"))).unwrap();
            let after = inspect(&store, &config, "go-a").unwrap();
            assert_eq!(after["billing"]["observation"], fact);
            assert_eq!(
                after["observations"][2]["latest_attempt"]["reason_code"],
                reason
            );
            assert_eq!(
                after["observations"][2]["latest_attempt"]["outcome"],
                if reason == "credential_unavailable" {
                    "unavailable"
                } else {
                    "failed"
                }
            );
            assert_eq!(
                inspect(&store, &config, "go-b").unwrap()["billing"]["state"],
                "unknown"
            );
            assert_eq!(after["observations"][0]["state"], "unknown");
            assert_eq!(after["observations"][1]["state"], "unknown");
        }
        let observed = fact["observed_at"]
            .as_str()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .unwrap();
        for at in [observed, observed + chrono::Duration::days(365)] {
            assert_eq!(
                routing_availability(&store, &config, "go-a", at).unwrap(),
                None
            );
        }
        inputs::authorization_changed(&store, "go-a").unwrap();
        assert_eq!(
            inspect(&store, &config, "go-a").unwrap()["billing"]["state"],
            "unknown"
        );
    }
    #[test]
    fn malformed_go_refresh_retains_fact_and_corrupt_retained_data_is_rejected() {
        let (_dir, store, config) = go_fixture();
        let captured = capture(&store, &config, "go-a", "quota").unwrap();
        commit(&store, &captured, Ok(go_usage())).unwrap();
        let old = inspect(&store, &config, "go-a").unwrap()["billing"]["observation"].clone();
        let mut malformed = go_usage().data;
        malformed["usage"]["weekly"]["percent"] = json!("not-a-number");
        let acquired = provider_integration::normalize_observation("go_usage_api", &malformed)
            .map(|data| AcquiredObservation { data, status: 200 });
        let captured = capture(&store, &config, "go-a", "quota").unwrap();
        commit(&store, &captured, acquired).unwrap();
        let after = inspect(&store, &config, "go-a").unwrap();
        assert_eq!(after["billing"]["observation"], old);
        assert_eq!(
            after["observations"][2]["latest_attempt"]["reason_code"],
            "source_format_error"
        );
        let mut heads = read(&store).unwrap();
        heads
            .heads
            .values_mut()
            .next()
            .unwrap()
            .fact
            .as_mut()
            .unwrap()
            .data["remaining"] = json!(38);
        store.write_json(STATE, &heads).unwrap();
        assert_eq!(
            inspect(&store, &config, "go-a").unwrap_err().code,
            "invalid_record"
        );
    }
    #[tokio::test]
    async fn go_quota_dry_run_declares_supported_read_without_secret_or_publication() {
        let (_dir, store, config) = go_fixture();
        let result = refresh(&store, &config, "go-a", "quota", true)
            .await
            .unwrap();
        assert_eq!(result["checks"][0]["outcome"], "not_executed");
        assert!(result["checks"][0]["reason_code"].is_null());
        assert!(!store.path(STATE).unwrap().exists());
    }
    #[test]
    fn historical_regional_quota_rebinds_exact_fingerprints_and_rejects_corruption() {
        for (old_id, canonical, source, body) in [
            (
                "kimi-coding-plan",
                "kimi-code-plan-cn",
                "kimi_usage_api",
                json!({"usage":{"limit":"100","used":"20","remaining":"80"}}),
            ),
            (
                "kimi-coding-plan",
                "kimi-code-plan-global",
                "kimi_usage_api",
                json!({"usage":{"limit":"100","used":"20","remaining":"80"}}),
            ),
            (
                "minimax-m-plan",
                "minimax-cn-coding-plan",
                "minimax_plan_remains_api",
                json!({"base_resp":{"status_code":0},"model_remains":[{"model_name":"general","current_interval_remaining_percent":80}]}),
            ),
            (
                "minimax-m-plan",
                "minimax-coding-plan",
                "minimax_plan_remains_api",
                json!({"base_resp":{"status_code":0},"model_remains":[{"model_name":"general","current_interval_remaining_percent":80}]}),
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let store = Store::at(dir.path().to_path_buf());
            let mut config = Config::empty();
            config.providers.insert(
                "plan".into(),
                config::Provider {
                    integration: Some(canonical.into()),
                    auth: Some(config::Auth::ApiKey {
                        key_ref: "fixture-key".into(),
                    }),
                    ..Default::default()
                },
            );
            store
                .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
                .unwrap();
            inputs::reconcile(&store, &Config::empty(), &config, &[]).unwrap();
            inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
            let captured = capture(&store, &config, "plan", "quota").unwrap();
            let data = provider_integration::normalize_observation(source, &body).unwrap();
            commit(
                &store,
                &captured,
                Ok(AcquiredObservation { status: 200, data }),
            )
            .unwrap();
            let expected = read(&store).unwrap();
            let before = inspect(&store, &config, "plan").unwrap();
            let canonical_head = expected.heads.values().next().unwrap().clone();
            let mut legacy_head = canonical_head.clone();
            legacy_head.binding.integration = old_id.into();
            let mut historical = Heads::default();
            historical
                .heads
                .insert(signature(&legacy_head.binding), legacy_head.clone());
            store.write_json(STATE, &historical).unwrap();
            let bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
            let after = inspect(&store, &config, "plan").unwrap();
            assert_eq!(
                after["billing"]["observation"],
                before["billing"]["observation"]
            );
            assert_eq!(
                after["observations"][2]["latest_attempt"],
                before["observations"][2]["latest_attempt"]
            );
            assert_eq!(
                serde_json::to_value(read(&store).unwrap()).unwrap(),
                serde_json::to_value(&expected).unwrap()
            );
            assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), bytes);

            let mut bad_key = Heads::default();
            bad_key
                .heads
                .insert("wrong-key".into(), legacy_head.clone());
            store.write_json(STATE, &bad_key).unwrap();
            assert_eq!(
                inspect(&store, &config, "plan").unwrap_err().code,
                "invalid_record"
            );
            let mut bad_fingerprint = legacy_head.clone();
            bad_fingerprint.binding.endpoint_contract =
                signature(&std::collections::BTreeMap::<String, String>::new());
            let mut invalid = Heads::default();
            invalid
                .heads
                .insert(signature(&bad_fingerprint.binding), bad_fingerprint);
            store.write_json(STATE, &invalid).unwrap();
            assert_eq!(
                inspect(&store, &config, "plan").unwrap_err().code,
                "invalid_record"
            );
            let mut bad_source = legacy_head.clone();
            bad_source.binding.source_contract = Some("different-contract".into());
            let mut invalid = Heads::default();
            invalid
                .heads
                .insert(signature(&bad_source.binding), bad_source);
            store.write_json(STATE, &invalid).unwrap();
            assert_eq!(
                inspect(&store, &config, "plan").unwrap_err().code,
                "invalid_record"
            );
            let mut collision = historical;
            collision
                .heads
                .insert(signature(&canonical_head.binding), canonical_head);
            store.write_json(STATE, &collision).unwrap();
            let collision_bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
            assert_eq!(
                inspect(&store, &config, "plan").unwrap_err().code,
                "invalid_record"
            );
            assert_eq!(
                std::fs::read(store.path(STATE).unwrap()).unwrap(),
                collision_bytes
            );
            let mut historical = Heads::default();
            historical
                .heads
                .insert(signature(&legacy_head.binding), legacy_head);
            store.write_json(STATE, &historical).unwrap();
            let captured = capture(&store, &config, "plan", "quota").unwrap();
            commit(
                &store,
                &captured,
                Err(Error::new("transport_error", "fixture")),
            )
            .unwrap();
            let persisted: Heads = store.read_json(STATE).unwrap();
            assert_eq!(
                persisted.heads.values().next().unwrap().binding.integration,
                canonical
            );
            assert_eq!(
                inspect(&store, &config, "plan").unwrap()["billing"]["observation"],
                before["billing"]["observation"]
            );
        }
    }

    #[test]
    fn plan_quota_projection_reports_plan_kind_without_invented_freshness() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().to_path_buf());
        let mut config = Config::empty();
        config.providers.insert(
            "kimi-a".into(),
            config::Provider {
                integration: Some("kimi-code-plan-cn".into()),
                auth: Some(config::Auth::ApiKey {
                    key_ref: "key-kimi-a".into(),
                }),
                ..Default::default()
            },
        );
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        let captured = capture(&store, &config, "kimi-a", "quota").unwrap();
        let fact = AcquiredObservation {
            status: 200,
            data: provider_integration::normalize_observation(
                "kimi_usage_api",
                &json!({
                    "usage": {"limit":"100","used":"20","remaining":"80","resetTime":"2026-10-29T15:42:16.495050Z"},
                    "limits": [{"window":{"duration":300,"timeUnit":"TIME_UNIT_MINUTE"},
                        "detail":{"limit":"100","used":"43","remaining":"57","resetTime":"2026-10-27T18:42:16.495050Z"}}]
                }),
            )
            .unwrap(),
        };
        commit(&store, &captured, Ok(fact)).unwrap();
        let after = inspect(&store, &config, "kimi-a").unwrap();
        assert_eq!(after["billing"]["kind"], "plan_allowance");
        assert_eq!(after["billing"]["state"], "observed");
        assert_eq!(after["billing"]["reset"], "provider_reported");
        assert_eq!(after["billing"]["renewal"], "unknown");
        assert_eq!(after["billing"]["freshness"]["state"], "unqualified");
        assert!(after["billing"]["freshness"]["as_of"].is_null());
        assert_eq!(
            after["billing"]["observation"]["data"]["usage"]["remaining"],
            "80"
        );
        let quota = &after["observations"][2];
        assert_eq!(quota["source"], "kimi_usage_api");
        assert!(quota["freshness"]["as_of"].is_null());

        let captured = capture(&store, &config, "kimi-a", "quota").unwrap();
        let partial = AcquiredObservation {
            status: 200,
            data: provider_integration::normalize_observation(
                "kimi_usage_api",
                &json!({"usage":{"limit":"100","used":"100"}}),
            )
            .unwrap(),
        };
        commit(&store, &captured, Ok(partial)).unwrap();
        let current = inspect(&store, &config, "kimi-a").unwrap();
        assert_eq!(current["billing"]["reset"], "unknown");
        assert_eq!(
            current["billing"]["observation"]["data"]["usage"],
            json!({"limit":"100","used":"100"})
        );
        assert_eq!(
            current["observations"][2]["latest_attempt"]["outcome"],
            "completed"
        );

        let captured = capture(&store, &config, "kimi-a", "quota").unwrap();
        let unusable = provider_integration::normalize_observation(
            "kimi_usage_api",
            &json!({"usage":{"remaining":"invalid"}}),
        )
        .map(|data| AcquiredObservation { status: 200, data });
        commit(&store, &captured, unusable).unwrap();
        let after_failure = inspect(&store, &config, "kimi-a").unwrap();
        assert_eq!(
            after_failure["billing"]["observation"],
            current["billing"]["observation"]
        );
        assert_eq!(
            after_failure["observations"][2]["latest_attempt"]["outcome"],
            "failed"
        );
    }
}
