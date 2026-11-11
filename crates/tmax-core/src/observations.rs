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

mod quota;
pub(crate) use quota::{
    automatic_quota_tick, quota_epoch_for_binding, quota_suspension_for_binding,
    record_quota_denial, resume_quota_locked,
};
pub use quota::{quota_status, resume_quota};

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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Suspension {
    source: String,
    suspended_at: DateTime<Utc>,
    window: Option<String>,
    reset_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuotaState {
    binding: Binding,
    epoch: String,
    manual_resume_revision: String,
    #[serde(deserialize_with = "required_option")]
    suspension: Option<Suspension>,
    manually_resumed: bool,
    #[serde(deserialize_with = "required_option")]
    suppressed_fact_revision: Option<String>,
    #[serde(deserialize_with = "required_option")]
    latest_denial_at: Option<DateTime<Utc>>,
    #[serde(deserialize_with = "required_option")]
    latest_denial_revision: Option<String>,
    #[serde(deserialize_with = "required_option")]
    latest_available_check_started_at: Option<DateTime<Utc>>,
    #[serde(deserialize_with = "required_option")]
    next_check_at: Option<DateTime<Utc>>,
}

fn required_option<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

impl QuotaState {
    fn new(binding: Binding) -> Self {
        Self {
            binding,
            epoch: "0".into(),
            manual_resume_revision: "0".into(),
            suspension: None,
            manually_resumed: false,
            suppressed_fact_revision: None,
            latest_denial_at: None,
            latest_denial_revision: None,
            latest_available_check_started_at: None,
            next_check_at: None,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Heads {
    heads: BTreeMap<String, Head>,
    #[serde(default)]
    quota: BTreeMap<String, QuotaState>,
    #[serde(skip)]
    unavailable_quota: BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ObservationSnapshot {
    heads: Heads,
    pending: BTreeSet<String>,
}

pub(crate) fn snapshot(store: &Store) -> Result<ObservationSnapshot> {
    let (heads, pending) = quota::effective_heads(store)?;
    Ok(ObservationSnapshot { heads, pending })
}

pub(crate) fn snapshot_saved(store: &Store) -> Result<ObservationSnapshot> {
    Ok(ObservationSnapshot {
        heads: read(store)?,
        pending: BTreeSet::new(),
    })
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
    for (key, state) in heads.quota {
        if key != signature(&state.binding)
            || state.binding.scope != "quota"
            || state.binding.integration != "opencode-go"
            || state.binding.source.as_deref() != Some("go_usage_api")
            || state.binding.provider_identity.is_empty()
            || state.binding.authorization_generation.is_empty()
            || state.epoch.is_empty()
            || state.manual_resume_revision.is_empty()
            || state.suspension.as_ref().is_some_and(|s| {
                !matches!(s.source.as_str(), "go_usage_api" | "go_denial")
                    || s.window
                        .as_deref()
                        .is_some_and(|w| !matches!(w, "rolling" | "weekly" | "monthly"))
                    || s.reset_at
                        .as_deref()
                        .is_some_and(|r| inputs::ExactInstant::parse(r).is_err())
            })
        {
            return Err(invalid_record());
        }
        canonical.quota.insert(key, state);
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
    heads.quota.retain(|k, _| keys.contains(k));
    Ok(())
}

/// Called by the input lifecycle owner with the routing lock already held.
/// Keep only current saved/still-loaded bindings, never an observation archive.
pub(crate) fn retain(store: &Store, runtime: &Runtime) -> Result<()> {
    let _observations = store.lock("observations")?;
    quota::prune_pending(store, runtime)?;
    let (mut heads, _) = quota::effective_heads(store)?;
    let before = (heads.heads.len(), heads.quota.len());
    prune(&mut heads, runtime)?;
    if (heads.heads.len(), heads.quota.len()) != before {
        quota::write_heads(store, &heads)?;
    }
    Ok(())
}

struct Capture {
    binding: Binding,
    head_revision: String,
    quota_epoch: String,
    denial_revision: Option<String>,
    automatic_config: Option<String>,
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
    let (heads, _) = quota::effective_heads(store)?;
    Ok(Capture {
        head_revision: revision(heads.heads.get(&signature(&b))).into(),
        quota_epoch: heads
            .quota
            .get(&signature(&b))
            .map_or("0", |s| s.epoch.as_str())
            .into(),
        denial_revision: heads
            .quota
            .get(&signature(&b))
            .and_then(|s| s.latest_denial_revision.clone()),
        automatic_config: None,
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
    if captured.binding.source.as_deref() == Some("go_usage_api") {
        let state = heads.quota.get(&key);
        if state.map_or("0", |s| s.epoch.as_str()) != captured.quota_epoch
            || state.and_then(|s| s.latest_denial_revision.as_ref())
                != captured.denial_revision.as_ref()
        {
            return Err(Error::new(
                "observations_changed",
                "Quota authority changed during acquisition",
            ));
        }
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
    if outcome == "completed"
        && captured.binding.source.as_deref() == Some("go_usage_api")
        && let Some(access) = fact
            .as_ref()
            .and_then(|f| provider_integration::quota_access("go_usage_api", &f.data))
    {
        let state = heads
            .quota
            .entry(key.clone())
            .or_insert_with(|| QuotaState::new(captured.binding.clone()));
        if let Some(window) = access
            .windows
            .iter()
            .find(|w| w.access_state == "rate-limited")
        {
            state.next_check_at.get_or_insert(finished_at);
            state.suspension = Some(Suspension {
                source: "go_usage_api".into(),
                suspended_at: finished_at,
                window: Some(window.window.clone()),
                reset_at: Some(window.reset_at.clone()),
            });
            state.manually_resumed = false;
        } else if !access.windows.is_empty()
            && access.windows.iter().all(|w| w.access_state == "available")
        {
            state.latest_available_check_started_at = Some(captured.started_at);
            if state.suspension.take().is_some() {
                state.epoch = token();
            }
            state.manually_resumed = false;
        }
    }
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
    if let Some(expected) = &captured.automatic_config {
        let loaded = runtime.loaded.as_ref().map(|view| signature(&view.config));
        if loaded.as_ref() != Some(expected)
            || signature(&runtime.saved.config) != *expected
            || signature(&config::load(store)?.config) != *expected
        {
            return Err(Error::new(
                "observations_changed",
                "Loaded quota authority changed during acquisition",
            ));
        }
    }
    let current = current_saved_binding(store, &runtime, &captured.binding)?;
    let (mut heads, _) = quota::effective_heads(store)?;
    let attempt = publish(&mut heads, captured, current.as_ref(), acquired, Utc::now())?;
    prune(&mut heads, &runtime)?;
    quota::prune_pending(store, &runtime)?;
    quota::write_heads(store, &heads)?;
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
    // Surfaces render this projection instead of interpreting source payloads.
    let display = billing_fact
        .zip((!billing_source.is_empty()).then_some(billing_source))
        .and_then(|(fact, source)| provider_integration::quota_display(source, &fact.data));
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
            "display":display,
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
            "last_attempt_reason_code":s["latest_attempt"]["reason_code"],
            "display":if s["scope"]=="quota"{detail["billing"]["display"].clone()}else{Value::Null}})
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"provider":provider,"observations":scopes,"billing_state":detail["billing"]["state"],"billing_kind":detail["billing"]["kind"],"account_identity":null}),
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

#[cfg(test)]
pub(crate) fn routing_availability_for_binding(
    store: &Store,
    config: &Config,
    provider: &str,
    at: &inputs::ExactInstant,
    scope: (&str, &str),
) -> Result<Option<String>> {
    routing_availability_impl(store, config, provider, at, Some(scope))
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AvailabilityHold {
    pub reason_code: &'static str,
    pub state: &'static str,
    pub window: String,
    pub coverage: &'static str,
    pub observed_at: String,
    pub valid_until: String,
    pub reset_at: String,
    pub max_observation_age_seconds: i64,
}

/// A quota guard hold is derived from the exact retained quota fact captured
/// with the admission snapshot. It never reads a priority input and never
/// mutates the fact.
pub(crate) fn quota_guard_hold_for_binding(
    snapshot: &inputs::Snapshot<'_>,
    config: &Config,
    provider: &str,
    model: &str,
    at: &inputs::ExactInstant,
    captured_scope: (&str, &str),
) -> Option<AvailabilityHold> {
    let provider_config = config.providers.get(provider)?;
    let guard = provider_config.quota_guard.as_ref()?;
    let selected = snapshot.provider_scope_binding(provider)?;
    if selected.0 != captured_scope.0 || selected.1 != captured_scope.1 {
        return None;
    }
    let observations = snapshot.observations()?;
    let current = binding_with_scope(config, provider, "quota", selected).ok()?;
    let head = observations.heads.heads.get(&signature(&current))?;
    let fact = head.fact.as_ref()?;
    if observations
        .heads
        .quota
        .get(&signature(&current))
        .and_then(|state| state.suppressed_fact_revision.as_deref())
        == Some(fact.revision.as_str())
    {
        return None;
    }
    let source = current.source.as_deref()?;
    let access = crate::provider_integration::quota_access(source, &fact.data)?;
    if access
        .coverage
        .as_ref()
        .is_some_and(|models| !models.iter().any(|covered| covered == model))
    {
        return None;
    }
    let observed = inputs::ExactInstant::from_chrono(fact.observed_at);
    if at < &observed {
        return None;
    }
    let age_bound = observed.add_seconds(guard.max_observation_age_seconds)?;
    if at >= &age_bound {
        return None;
    }
    let mut best: Option<(
        inputs::ExactInstant,
        &crate::provider_integration::QuotaAccessWindow,
    )> = None;
    for window in &access.windows {
        if !matches!(window.access_state, "rate-limited" | "exhausted") {
            continue;
        }
        let Ok(reset) = inputs::ExactInstant::parse(&window.reset_at) else {
            continue;
        };
        if at >= &reset {
            continue;
        }
        let valid_until = std::cmp::min(age_bound.clone(), reset);
        if at >= &valid_until {
            continue;
        }
        if best
            .as_ref()
            .is_none_or(|(current, _)| valid_until > *current)
        {
            best = Some((valid_until, window));
        }
    }
    let (valid_until, window) = best?;
    Some(AvailabilityHold {
        reason_code: "quota_guard",
        state: window.access_state,
        window: window.window.clone(),
        coverage: if access.coverage.is_some() {
            "models"
        } else {
            "provider"
        },
        observed_at: inputs::timestamp(fact.observed_at),
        valid_until: valid_until.canonical(),
        reset_at: window.reset_at.clone(),
        max_observation_age_seconds: guard.max_observation_age_seconds,
    })
}

#[cfg(test)]
pub(crate) fn test_fail_quota_denial(
    store: &Store,
    config: &Config,
    provider: &str,
    scope: (&str, &str),
    epoch: &str,
    denial: &provider_integration::QuotaDenial,
) -> Result<()> {
    quota::record_quota_denial_with_writer(store, config, provider, scope, epoch, denial, |_, _| {
        Err(crate::storage::storage_error())
    })
}

#[cfg(test)]
pub(crate) fn test_publish_quota(
    store: &Store,
    config: &Config,
    provider: &str,
    data: Value,
) -> DateTime<Utc> {
    let captured = capture(store, config, provider, "quota").expect("quota capture");
    commit(
        store,
        &captured,
        Ok(AcquiredObservation { status: 200, data }),
    )
    .expect("quota publication");
    read(store)
        .expect("quota heads")
        .heads
        .values()
        .find_map(|head| head.fact.as_ref().map(|fact| fact.observed_at))
        .expect("quota fact")
}

pub(crate) fn routing_availability_for_admission(
    snapshot: &inputs::Snapshot<'_>,
    config: &Config,
    provider: &str,
    at: &inputs::ExactInstant,
    captured_scope: (&str, &str),
) -> Option<String> {
    let selected = snapshot.provider_scope_binding(provider)?;
    if selected.0 != captured_scope.0 || selected.1 != captured_scope.1 {
        return None;
    }
    let observations = snapshot.observations()?;
    routing_availability_selected(config, provider, at, selected, &observations.heads)
        .ok()
        .flatten()
}

fn routing_availability_selected(
    config: &Config,
    provider: &str,
    at: &inputs::ExactInstant,
    selected: (String, String),
    heads: &Heads,
) -> Result<Option<String>> {
    if !config.providers.contains_key(provider) {
        return Err(Error::new("unknown_resource", "Provider is not configured"));
    }
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
    routing_availability_selected(config, provider, at, selected, &heads)
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
            quota_epoch: heads
                .quota
                .get(&signature(&binding))
                .map_or("0", |s| s.epoch.as_str())
                .into(),
            denial_revision: heads
                .quota
                .get(&signature(&binding))
                .and_then(|s| s.latest_denial_revision.clone()),
            automatic_config: None,
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
    fn guarded_go_fixture(
        guard: Option<config::QuotaGuard>,
        status: &str,
        reset: chrono::DateTime<Utc>,
    ) -> (tempfile::TempDir, Store, Config, String) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().to_path_buf());
        let mut config = Config::empty();
        config.providers.insert(
            "go".into(),
            config::Provider {
                integration: Some("opencode-go".into()),
                quota_guard: guard,
                models: [(
                    "m".into(),
                    config::Model {
                        priority: 7,
                        ..Default::default()
                    },
                )]
                .into(),
                ..Default::default()
            },
        );
        config.model_aliases.push(config::ModelAlias {
            candidate_reasoning_effort: Default::default(),
            id: "L0".into(),
            protocol: "chat-completions".into(),
            candidates: vec!["go/m".into()],
            enabled: true,
            continuity: None,
            fallback: None,
        });
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        let initial = config.clone();
        inputs::reconcile(&store, &Config::empty(), &initial, &[]).unwrap();
        let identity = inputs::provider_identity(&store, &initial, "go")
            .unwrap()
            .unwrap();
        let key_ref =
            crate::secrets::put(&store, &format!("provider:{identity}:go"), b"test-key").unwrap();
        config.providers.get_mut("go").unwrap().auth = Some(config::Auth::ApiKey { key_ref });
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(&store, &initial, &config, &[]).unwrap();
        let reset = inputs::timestamp(reset);
        let captured = capture(&store, &config, "go", "quota").unwrap();
        let data = provider_integration::normalize_observation(
            "go_usage_api",
            &json!({"usage":{
                "rolling":{"status":"ok","percent":0,"resetsAt":reset},
                "weekly":{"status":status,"percent":30,"resetsAt":reset},
                "monthly":{"status":"ok","percent":10,"resetsAt":reset}
            }}),
        )
        .unwrap();
        commit(
            &store,
            &captured,
            Ok(AcquiredObservation { status: 200, data }),
        )
        .unwrap();
        (dir, store, config, reset)
    }

    #[test]
    fn quota_suspension_survives_guard_expiry_and_resume_fences_old_evidence() {
        let reset = Utc::now() + chrono::Duration::hours(1);
        let (_dir, store, config, _) = guarded_go_fixture(
            Some(config::QuotaGuard {
                max_observation_age_seconds: 3600,
            }),
            "rate-limited",
            reset,
        );
        let runtime = inputs::read_runtime(&store).unwrap();
        let ids = inputs::provider_scope_binding(&runtime, &config, "go").unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let snapshot = inputs::Snapshot::capture(&store, &config);
        let now = inputs::ExactInstant::from_chrono(Utc::now());
        assert!(quota_guard_hold_for_binding(&snapshot, &config, "go", "m", &now, scope).is_some());
        let after_reset = inputs::ExactInstant::from_chrono(reset + chrono::Duration::seconds(1));
        assert!(
            quota_guard_hold_for_binding(&snapshot, &config, "go", "m", &after_reset, scope)
                .is_none()
        );
        assert!(
            quota_suspension_for_binding(&snapshot, &config, "go", &after_reset, scope).is_some()
        );
        assert_eq!(
            quota_status(&store, &config, "go", false).unwrap()["state"],
            "suspended"
        );
        resume_quota(&store, &config, "go").unwrap();
        let resumed = inputs::Snapshot::capture(&store, &config);
        assert!(quota_guard_hold_for_binding(&resumed, &config, "go", "m", &now, scope).is_none());
        assert!(quota_suspension_for_binding(&resumed, &config, "go", &now, scope).is_none());
        assert_eq!(
            quota_status(&store, &config, "go", false).unwrap()["state"],
            "manually_resumed"
        );
        // An already captured admission remains a consistent historical snapshot.
        assert!(quota_suspension_for_binding(&snapshot, &config, "go", &now, scope).is_some());
    }

    #[test]
    fn historical_rate_limit_is_not_imported_but_resume_suppresses_its_guard() {
        let (_dir, store, config, _) = guarded_go_fixture(
            Some(config::QuotaGuard {
                max_observation_age_seconds: 3600,
            }),
            "rate-limited",
            Utc::now() + chrono::Duration::hours(1),
        );
        let mut historical = read(&store).unwrap();
        historical.quota.clear();
        // A record produced by an older version has no quota field at all.
        store
            .write_json(STATE, &json!({"heads":historical.heads}))
            .unwrap();
        let runtime = inputs::read_runtime(&store).unwrap();
        let ids = inputs::provider_scope_binding(&runtime, &config, "go").unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let before = inputs::Snapshot::capture(&store, &config);
        let at = inputs::ExactInstant::from_chrono(Utc::now());
        assert!(quota_suspension_for_binding(&before, &config, "go", &at, scope).is_none());
        assert!(quota_guard_hold_for_binding(&before, &config, "go", "m", &at, scope).is_some());
        resume_quota(&store, &config, "go").unwrap();
        let after = inputs::Snapshot::capture(&store, &config);
        assert!(quota_guard_hold_for_binding(&after, &config, "go", "m", &at, scope).is_none());
    }

    #[test]
    fn failed_exclusive_locks_retain_only_current_binding_denials() {
        for failed_lock in ["configuration", "routing", "observations"] {
            let (_dir, store, config) = go_fixture();
            inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
            let initial = capture(&store, &config, "go-a", "quota").unwrap();
            commit(&store, &initial, Ok(go_usage())).unwrap();
            let before = std::fs::read(store.path(STATE).unwrap()).unwrap();
            let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
            let a = snapshot.provider_scope_binding("go-a").unwrap();
            let b = snapshot.provider_scope_binding("go-b").unwrap();
            let scope = (a.0.as_str(), a.1.as_str());
            let epoch = quota_epoch_for_binding(&snapshot, &config, "go-a", scope);
            let denial = provider_integration::QuotaDenial {
                window: Some("monthly".into()),
            };
            let result = quota::record_quota_denial_with_locker(
                &store,
                &config,
                "go-a",
                scope,
                &epoch,
                &denial,
                (
                    |store, heads| store.write_json(STATE, heads),
                    |store, name| {
                        if name == failed_lock {
                            Err(crate::storage::storage_error())
                        } else {
                            store.lock(name)
                        }
                    },
                ),
            );
            assert_eq!(result.unwrap_err().code, "storage_unavailable");
            assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), before);
            let current = inputs::Snapshot::capture_loaded(&store, &config, None);
            let at = inputs::ExactInstant::from_chrono(Utc::now());
            let evidence =
                quota_suspension_for_binding(&current, &config, "go-a", &at, scope).unwrap();
            assert_eq!(evidence["state"], "suspended");
            assert_eq!(evidence["persistence"], "pending");
            assert!(
                quota_suspension_for_binding(&current, &config, "go-b", &at, (&b.0, &b.1))
                    .is_none()
            );
            resume_quota(&store, &config, "go-a").unwrap();
            quota::record_quota_denial_with_locker(
                &store,
                &config,
                "go-a",
                scope,
                &epoch,
                &denial,
                (
                    |store, heads| store.write_json(STATE, heads),
                    |store, name| {
                        if name == failed_lock {
                            Err(crate::storage::storage_error())
                        } else {
                            store.lock(name)
                        }
                    },
                ),
            )
            .unwrap_err();
            assert_eq!(
                quota_status(&store, &config, "go-a", true).unwrap()["state"],
                "manually_resumed"
            );
            {
                let _configuration = store.lock("configuration").unwrap();
                let _routing = store.lock("routing").unwrap();
                let _native = store.lock("native").unwrap();
                inputs::authorization_changed_locked(&store, "go-a").unwrap();
            }
            quota::record_quota_denial_with_locker(
                &store,
                &config,
                "go-a",
                scope,
                &epoch,
                &denial,
                (
                    |store, heads| store.write_json(STATE, heads),
                    |store, name| {
                        if name == failed_lock {
                            Err(crate::storage::storage_error())
                        } else {
                            store.lock(name)
                        }
                    },
                ),
            )
            .unwrap_err();
            assert_eq!(
                quota_status(&store, &config, "go-a", true).unwrap()["state"],
                "available"
            );
        }
    }

    #[tokio::test]
    async fn unreadable_validation_is_scoped_and_repaired_after_authority_returns() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let initial = capture(&store, &config, "go-a", "quota").unwrap();
        commit(&store, &initial, Ok(go_usage())).unwrap();
        let before = std::fs::read(store.path(STATE).unwrap()).unwrap();
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let a = snapshot.provider_scope_binding("go-a").unwrap();
        let b = snapshot.provider_scope_binding("go-b").unwrap();
        let scope = (a.0.as_str(), a.1.as_str());
        let epoch = quota_epoch_for_binding(&snapshot, &config, "go-a", scope);
        store.write_raw(STATE, b"invalid").unwrap();
        let denial = provider_integration::QuotaDenial { window: None };
        quota::record_quota_denial_with_locker(
            &store,
            &config,
            "go-a",
            scope,
            &epoch,
            &denial,
            (
                |store, heads| store.write_json(STATE, heads),
                |_, _| Err(crate::storage::storage_error()),
            ),
        )
        .unwrap_err();
        store.write_raw(STATE, &before).unwrap();
        let current = inputs::Snapshot::capture_loaded(&store, &config, None);
        let at = inputs::ExactInstant::from_chrono(Utc::now());
        let unavailable =
            quota_suspension_for_binding(&current, &config, "go-a", &at, scope).unwrap();
        assert_eq!(unavailable["reason_code"], "quota_state_unavailable");
        assert!(
            quota_suspension_for_binding(&current, &config, "go-b", &at, (&b.0, &b.1)).is_none()
        );
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["state"],
            "unavailable"
        );
        let mut drift = config.clone();
        drift.providers.get_mut("go-a").unwrap().enabled = false;
        automatic_quota_tick(&store, &drift).await.unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "suspended"
        );
    }

    #[test]
    fn unreadable_receipt_cannot_override_resume_or_replacement() {
        for replace in [false, true] {
            let (_dir, store, config) = go_fixture();
            inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
            let initial = capture(&store, &config, "go-a", "quota").unwrap();
            commit(&store, &initial, Ok(go_usage())).unwrap();
            let before = std::fs::read(store.path(STATE).unwrap()).unwrap();
            let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
            let ids = snapshot.provider_scope_binding("go-a").unwrap();
            let scope = (ids.0.as_str(), ids.1.as_str());
            let epoch = quota_epoch_for_binding(&snapshot, &config, "go-a", scope);
            store.write_raw(STATE, b"invalid").unwrap();
            let denial = provider_integration::QuotaDenial { window: None };
            quota::record_quota_denial_with_locker(
                &store,
                &config,
                "go-a",
                scope,
                &epoch,
                &denial,
                (
                    |store, heads| store.write_json(STATE, heads),
                    |_, _| Err(crate::storage::storage_error()),
                ),
            )
            .unwrap_err();
            store.write_raw(STATE, &before).unwrap();
            if replace {
                let _configuration = store.lock("configuration").unwrap();
                let _routing = store.lock("routing").unwrap();
                let _native = store.lock("native").unwrap();
                inputs::authorization_changed_locked(&store, "go-a").unwrap();
            } else {
                resume_quota(&store, &config, "go-a").unwrap();
            }
            assert_eq!(
                quota_status(&store, &config, "go-a", true).unwrap()["state"],
                if replace {
                    "available"
                } else {
                    "manually_resumed"
                }
            );
        }
    }

    #[test]
    fn external_fresh_success_but_not_failed_or_stale_check_clears_pending() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let initial = capture(&store, &config, "go-a", "quota").unwrap();
        commit(&store, &initial, Ok(go_usage())).unwrap();
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let ids = snapshot.provider_scope_binding("go-a").unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let epoch = quota_epoch_for_binding(&snapshot, &config, "go-a", scope);
        let denial = provider_integration::QuotaDenial {
            window: Some("monthly".into()),
        };
        test_fail_quota_denial(&store, &config, "go-a", scope, &epoch, &denial).unwrap_err();
        let binding = binding_with_scope(&config, "go-a", "quota", ids).unwrap();
        let mut disk = read(&store).unwrap();
        let mut stale = capture_head(&disk, binding.clone());
        stale.started_at = Utc::now() - chrono::Duration::seconds(5);
        publish(
            &mut disk,
            &stale,
            Some(&binding),
            Ok(go_usage()),
            Utc::now(),
        )
        .unwrap();
        store.write_json(STATE, &disk).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["state"],
            "suspended"
        );
        let mut failed = capture_head(&disk, binding.clone());
        failed.started_at = Utc::now() + chrono::Duration::seconds(1);
        publish(
            &mut disk,
            &failed,
            Some(&binding),
            Err(Error::new("check_failed", "failed")),
            failed.started_at,
        )
        .unwrap();
        store.write_json(STATE, &disk).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["state"],
            "suspended"
        );
        let mut fresh = capture_head(&disk, binding.clone());
        fresh.started_at = Utc::now() + chrono::Duration::seconds(2);
        publish(
            &mut disk,
            &fresh,
            Some(&binding),
            Ok(go_usage()),
            fresh.started_at,
        )
        .unwrap();
        store.write_json(STATE, &disk).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["state"],
            "available"
        );
        let mut later_failure = capture_head(&disk, binding.clone());
        later_failure.started_at = Utc::now() + chrono::Duration::seconds(3);
        publish(
            &mut disk,
            &later_failure,
            Some(&binding),
            Err(Error::new("check_failed", "failed")),
            later_failure.started_at,
        )
        .unwrap();
        store.write_json(STATE, &disk).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["state"],
            "available"
        );
        let mut later_limit = capture_head(&disk, binding.clone());
        later_limit.started_at = Utc::now() + chrono::Duration::seconds(4);
        let data = provider_integration::normalize_observation("go_usage_api", &json!({"usage":{
            "rolling":{"status":"ok","percent":0,"resetsAt":"2026-10-23T19:04:44.687Z"},
            "weekly":{"status":"rate-limited","percent":100,"resetsAt":"2026-10-26T00:00:00.000Z"},
            "monthly":{"status":"ok","percent":62,"resetsAt":"2026-11-09T03:13:27.000Z"}
        }})).unwrap();
        publish(
            &mut disk,
            &later_limit,
            Some(&binding),
            Ok(AcquiredObservation { status: 200, data }),
            later_limit.started_at,
        )
        .unwrap();
        store.write_json(STATE, &disk).unwrap();
        let limited = quota_status(&store, &config, "go-a", true).unwrap();
        assert_eq!(limited["state"], "suspended");
        assert_eq!(limited["source"], "go_usage_api");
        assert_eq!(limited["window"], "weekly");
        assert!(limited.get("persistence").is_none());
        let sibling = capture(&store, &config, "go-b", "quota").unwrap();
        commit(&store, &sibling, Ok(go_usage())).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["window"],
            "weekly"
        );
    }

    #[tokio::test]
    async fn failed_denial_write_blocks_only_binding_and_worker_repairs_during_drift() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let initial = capture(&store, &config, "go-a", "quota").unwrap();
        commit(&store, &initial, Ok(go_usage())).unwrap();
        let before = std::fs::read(store.path(STATE).unwrap()).unwrap();
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let a = snapshot.provider_scope_binding("go-a").unwrap();
        let b = snapshot.provider_scope_binding("go-b").unwrap();
        let scope = (a.0.as_str(), a.1.as_str());
        let epoch = quota_epoch_for_binding(&snapshot, &config, "go-a", scope);
        let denial = provider_integration::QuotaDenial {
            window: Some("monthly".into()),
        };
        assert_eq!(
            test_fail_quota_denial(&store, &config, "go-a", scope, &epoch, &denial)
                .unwrap_err()
                .code,
            "storage_unavailable"
        );
        assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), before);
        let current = inputs::Snapshot::capture_loaded(&store, &config, None);
        let at = inputs::ExactInstant::from_chrono(Utc::now());
        let evidence = quota_suspension_for_binding(&current, &config, "go-a", &at, scope).unwrap();
        assert_eq!(evidence["persistence"], "pending");
        assert_eq!(evidence["durability"], "unconfirmed");
        assert_eq!(evidence["persistence_error"], "storage_unavailable");
        assert!(
            quota_suspension_for_binding(&current, &config, "go-b", &at, (&b.0, &b.1)).is_none()
        );
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["state"],
            "suspended"
        );
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "available"
        );

        // Repair runs even when acquisition is gated by disabled/config drift.
        let mut drift = config.clone();
        drift.providers.get_mut("go-a").unwrap().enabled = false;
        automatic_quota_tick(&store, &drift).await.unwrap();
        let saved = quota_status(&store, &config, "go-a", false).unwrap();
        assert_eq!(saved["state"], "suspended");
        assert!(saved.get("persistence").is_none());
        let failed_check = capture(&store, &config, "go-a", "quota").unwrap();
        commit(
            &store,
            &failed_check,
            Err(Error::new("check_failed", "failed")),
        )
        .unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["state"],
            "suspended"
        );
    }

    #[tokio::test]
    async fn pending_repair_preserves_the_scheduled_recheck_deadline() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let ids = snapshot.provider_scope_binding("go-a").unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let denial = provider_integration::QuotaDenial { window: None };
        record_quota_denial(&store, &config, "go-a", scope, "0", &denial).unwrap();
        let mut disk = read(&store).unwrap();
        let key = signature(&binding_with_scope(&config, "go-a", "quota", ids.clone()).unwrap());
        let deadline = Utc::now() + chrono::Duration::minutes(10);
        disk.quota.get_mut(&key).unwrap().next_check_at = Some(deadline);
        store.write_json(STATE, &disk).unwrap();
        test_fail_quota_denial(&store, &config, "go-a", scope, "0", &denial).unwrap_err();
        automatic_quota_tick(&store, &config).await.unwrap();
        let disk = read(&store).unwrap();
        assert_eq!(disk.quota[&key].next_check_at, Some(deadline));
        assert!(disk.quota[&key].suspension.is_some());
        assert!(!disk.heads.contains_key(&key));
        assert!(
            quota_status(&store, &config, "go-a", true)
                .unwrap()
                .get("persistence")
                .is_none()
        );
    }

    #[test]
    fn independent_stale_recovery_cannot_erase_pending_but_manual_resume_can() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let ids = snapshot.provider_scope_binding("go-a").unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let denial = provider_integration::QuotaDenial {
            window: Some("weekly".into()),
        };
        record_quota_denial(&store, &config, "go-a", scope, "0", &denial).unwrap();
        let stale = capture(&store, &config, "go-a", "quota").unwrap();
        let newer = provider_integration::QuotaDenial {
            window: Some("monthly".into()),
        };
        test_fail_quota_denial(&store, &config, "go-a", scope, "0", &newer).unwrap_err();

        // An independent process sees only disk, so its stale check can commit.
        let mut disk = read(&store).unwrap();
        publish(
            &mut disk,
            &stale,
            Some(&stale.binding),
            Ok(go_usage()),
            Utc::now(),
        )
        .unwrap();
        store.write_json(STATE, &disk).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "available"
        );
        let effective = quota_status(&store, &config, "go-a", true).unwrap();
        assert_eq!(effective["window"], "monthly");
        assert_eq!(effective["persistence"], "pending");

        // Simulate another process resuming and then publishing a later check.
        let key = signature(&stale.binding);
        let state = disk.quota.get_mut(&key).unwrap();
        state.epoch = token();
        state.manual_resume_revision = token();
        state.manually_resumed = true;
        state.suspension = None;
        let after_resume = capture_head(&disk, stale.binding.clone());
        publish(
            &mut disk,
            &after_resume,
            Some(&after_resume.binding),
            Ok(go_usage()),
            Utc::now(),
        )
        .unwrap();
        store.write_json(STATE, &disk).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["state"],
            "available"
        );
        resume_quota(&store, &config, "go-a").unwrap();
    }

    #[test]
    fn unrelated_publication_preserves_pending_and_old_check_is_fenced() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let ids = snapshot.provider_scope_binding("go-a").unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let old_check = capture(&store, &config, "go-a", "quota").unwrap();
        let sibling = capture(&store, &config, "go-b", "quota").unwrap();
        let denial = provider_integration::QuotaDenial {
            window: Some("rolling".into()),
        };
        test_fail_quota_denial(&store, &config, "go-a", scope, "0", &denial).unwrap_err();
        let overlapping = capture(&store, &config, "go-a", "quota").unwrap();
        let later = provider_integration::QuotaDenial {
            window: Some("monthly".into()),
        };
        test_fail_quota_denial(&store, &config, "go-a", scope, "0", &later).unwrap_err();
        assert_eq!(
            commit(&store, &overlapping, Ok(go_usage()))
                .unwrap_err()
                .code,
            "observations_changed"
        );
        assert_eq!(
            commit(&store, &old_check, Ok(go_usage())).unwrap_err().code,
            "observations_changed"
        );
        commit(&store, &sibling, Ok(go_usage())).unwrap();
        let status = quota_status(&store, &config, "go-a", true).unwrap();
        assert_eq!(status["state"], "suspended");
        assert_eq!(status["window"], "monthly");
        assert!(status.get("persistence").is_none());
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "suspended"
        );
    }

    #[test]
    fn invalidated_authorization_prunes_pending_denial() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let ids = snapshot.provider_scope_binding("go-a").unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let key = signature(&binding_with_scope(&config, "go-a", "quota", ids.clone()).unwrap());
        let denial = provider_integration::QuotaDenial { window: None };
        test_fail_quota_denial(&store, &config, "go-a", scope, "0", &denial).unwrap_err();
        assert!(quota::effective_heads(&store).unwrap().1.contains(&key));
        {
            let _configuration = store.lock("configuration").unwrap();
            let _routing = store.lock("routing").unwrap();
            let _native = store.lock("native").unwrap();
            inputs::authorization_changed_locked(&store, "go-a").unwrap();
        }
        assert!(!quota::effective_heads(&store).unwrap().1.contains(&key));
    }

    #[test]
    fn post_rename_error_does_not_claim_durable_suspension() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let ids = snapshot.provider_scope_binding("go-a").unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let denial = provider_integration::QuotaDenial { window: None };
        quota::record_quota_denial_with_writer(
            &store,
            &config,
            "go-a",
            scope,
            "0",
            &denial,
            |store, heads| {
                store.write_json(STATE, heads)?;
                Err(crate::storage::storage_error())
            },
        )
        .unwrap_err();
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["durability"],
            "unconfirmed"
        );
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "suspended"
        );
        resume_quota(&store, &config, "go-a").unwrap();
    }

    #[test]
    fn confirmed_recovery_supersedes_delayed_denials_and_requires_authority_fields() {
        let (_directory, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let ids = snapshot.provider_scope_binding("go-a").unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let epoch = quota_epoch_for_binding(&snapshot, &config, "go-a", scope);
        let denial = provider_integration::QuotaDenial {
            window: Some("monthly".into()),
        };
        let ordinary_check = capture(&store, &config, "go-a", "quota").unwrap();
        commit(&store, &ordinary_check, Ok(go_usage())).unwrap();
        record_quota_denial(&store, &config, "go-a", scope, &epoch, &denial).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["state"],
            "suspended"
        );
        let check = capture(&store, &config, "go-a", "quota").unwrap();
        commit(&store, &check, Ok(go_usage())).unwrap();
        record_quota_denial(&store, &config, "go-a", scope, &epoch, &denial).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["state"],
            "available"
        );

        let intact = serde_json::to_value(read(&store).unwrap()).unwrap();
        for field in [
            "suspension",
            "epoch",
            "manual_resume_revision",
            "manually_resumed",
            "suppressed_fact_revision",
            "latest_denial_at",
            "latest_denial_revision",
            "latest_available_check_started_at",
            "next_check_at",
        ] {
            let mut damaged = intact.clone();
            damaged["quota"]
                .as_object_mut()
                .unwrap()
                .values_mut()
                .next()
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            store.write_json(STATE, &damaged).unwrap();
            assert_eq!(read(&store).unwrap_err().code, "invalid_record");
            let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
            let blocked = quota_suspension_for_binding(
                &snapshot,
                &config,
                "go-a",
                &inputs::ExactInstant::from_chrono(Utc::now()),
                scope,
            )
            .unwrap();
            assert_eq!(blocked["reason_code"], "quota_state_unavailable");
        }
    }

    #[test]
    fn denial_resume_and_observations_are_binding_scoped_and_fenced() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let runtime = inputs::read_runtime(&store).unwrap();
        let ids = inputs::provider_scope_binding_in_view(
            &runtime,
            runtime.loaded.as_ref().unwrap(),
            "go-a",
        )
        .unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let epoch = quota_epoch_for_binding(&snapshot, &config, "go-a", scope);
        let denial = provider_integration::QuotaDenial {
            window: Some("monthly".into()),
        };
        let stale_check = capture(&store, &config, "go-a", "quota").unwrap();
        record_quota_denial(&store, &config, "go-a", scope, &epoch, &denial).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["state"],
            "suspended"
        );
        assert_eq!(
            quota_status(&store, &config, "go-b", false).unwrap()["state"],
            "available"
        );
        assert_eq!(
            commit(&store, &stale_check, Ok(go_usage()))
                .unwrap_err()
                .code,
            "observations_changed"
        );
        // The persisted state is read by a new Store, not an in-memory process cache.
        let restarted = Store::at(store.home.clone());
        assert_eq!(
            quota_status(&restarted, &config, "go-a", false).unwrap()["window"],
            "monthly"
        );
        let failed = capture(&store, &config, "go-a", "quota").unwrap();
        commit(
            &store,
            &failed,
            Err(Error::new("source_format_error", "unknown")),
        )
        .unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "suspended"
        );
        let stale_check = capture(&store, &config, "go-a", "quota").unwrap();
        resume_quota(&store, &config, "go-a").unwrap();
        record_quota_denial(&store, &config, "go-a", scope, &epoch, &denial).unwrap();
        assert_eq!(
            commit(&store, &stale_check, Ok(go_usage()))
                .unwrap_err()
                .code,
            "observations_changed"
        );
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "manually_resumed"
        );
        let fresh = inputs::Snapshot::capture_loaded(&store, &config, None);
        let new_epoch = quota_epoch_for_binding(&fresh, &config, "go-a", scope);
        record_quota_denial(&store, &config, "go-a", scope, &new_epoch, &denial).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "suspended"
        );
        let mut mixed = go_usage();
        mixed.data["usage"]["weekly"]["status"] = json!("rate-limited");
        let captured = capture(&store, &config, "go-a", "quota").unwrap();
        commit(&store, &captured, Ok(mixed)).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "suspended"
        );
        let captured = capture(&store, &config, "go-a", "quota").unwrap();
        commit(&store, &captured, Ok(go_usage())).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "available"
        );
    }

    #[test]
    fn authorization_replacement_prunes_suspension_and_rejects_old_denial() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let runtime = inputs::read_runtime(&store).unwrap();
        let ids = inputs::provider_scope_binding_in_view(
            &runtime,
            runtime.loaded.as_ref().unwrap(),
            "go-a",
        )
        .unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let denial = provider_integration::QuotaDenial { window: None };
        record_quota_denial(&store, &config, "go-a", scope, "0", &denial).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "suspended"
        );
        {
            let _configuration = store.lock("configuration").unwrap();
            let _routing = store.lock("routing").unwrap();
            let _native = store.lock("native").unwrap();
            inputs::authorization_changed_locked(&store, "go-a").unwrap();
        }
        record_quota_denial(&store, &config, "go-a", scope, "0", &denial).unwrap();
        assert!(read(&store).unwrap().quota.is_empty());
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "available"
        );
    }

    #[test]
    fn overlapping_denials_fence_a_check_started_after_the_first_receipt() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let runtime = inputs::read_runtime(&store).unwrap();
        let ids = inputs::provider_scope_binding_in_view(
            &runtime,
            runtime.loaded.as_ref().unwrap(),
            "go-a",
        )
        .unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let first = provider_integration::QuotaDenial {
            window: Some("weekly".into()),
        };
        let second = provider_integration::QuotaDenial {
            window: Some("monthly".into()),
        };
        record_quota_denial(&store, &config, "go-a", scope, "0", &first).unwrap();
        let check = capture(&store, &config, "go-a", "quota").unwrap();
        record_quota_denial(&store, &config, "go-a", scope, "0", &second).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["window"],
            "monthly"
        );
        assert_eq!(
            commit(&store, &check, Ok(go_usage())).unwrap_err().code,
            "observations_changed"
        );
        assert_eq!(
            quota_status(&store, &config, "go-a", true).unwrap()["state"],
            "suspended"
        );
    }

    #[test]
    fn denial_tracks_loaded_binding_across_unrelated_edit_and_explicit_rename() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let runtime = inputs::read_runtime(&store).unwrap();
        let ids = inputs::provider_scope_binding_in_view(
            &runtime,
            runtime.loaded.as_ref().unwrap(),
            "go-a",
        )
        .unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let denial = provider_integration::QuotaDenial { window: None };
        let mut renamed = config.clone();
        let provider = renamed.providers.remove("go-a").unwrap();
        renamed.providers.insert("renamed".into(), provider);
        renamed.providers.get_mut("go-b").unwrap().enabled = false;
        store
            .write_raw("tmax.toml", toml::to_string(&renamed).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(
            &store,
            &config,
            &renamed,
            &[("go-a".into(), "renamed".into())],
        )
        .unwrap();
        inputs::activate(&store, &renamed, &config::raw_revision(&store).unwrap()).unwrap();
        record_quota_denial(&store, &config, "go-a", scope, "0", &denial).unwrap();
        assert_eq!(
            quota_status(&store, &renamed, "renamed", true).unwrap()["state"],
            "suspended"
        );
    }

    #[test]
    fn changed_loaded_endpoint_contract_rejects_old_denial() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let runtime = inputs::read_runtime(&store).unwrap();
        let ids = inputs::provider_scope_binding_in_view(
            &runtime,
            runtime.loaded.as_ref().unwrap(),
            "go-a",
        )
        .unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let mut replaced = config.clone();
        let provider = replaced.providers.get_mut("go-a").unwrap();
        provider.integration = Some("custom".into());
        provider
            .endpoints
            .insert("chat-completions".into(), "https://example.com/v1".into());
        store
            .write_raw("tmax.toml", toml::to_string(&replaced).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(&store, &config, &replaced, &[]).unwrap();
        inputs::activate(&store, &replaced, &config::raw_revision(&store).unwrap()).unwrap();
        record_quota_denial(
            &store,
            &config,
            "go-a",
            scope,
            "0",
            &provider_integration::QuotaDenial { window: None },
        )
        .unwrap();
        assert!(read(&store).unwrap().quota.is_empty());
    }

    #[test]
    fn unavailable_and_historical_snapshots_are_not_presented_as_confirmed_denials() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let runtime = inputs::read_runtime(&store).unwrap();
        let ids = inputs::provider_scope_binding_in_view(
            &runtime,
            runtime.loaded.as_ref().unwrap(),
            "go-a",
        )
        .unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        let at = inputs::ExactInstant::from_chrono(Utc::now());
        let empty = inputs::Snapshot::capture_loaded(&store, &config, None);
        assert!(quota_suspension_for_binding(&empty, &config, "go-a", &at, scope).is_none());
        record_quota_denial(
            &store,
            &config,
            "go-a",
            scope,
            "0",
            &provider_integration::QuotaDenial { window: None },
        )
        .unwrap();
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let before = inputs::ExactInstant::from_chrono(Utc::now() - chrono::Duration::minutes(1));
        let historical =
            quota_suspension_for_binding(&snapshot, &config, "go-a", &before, scope).unwrap();
        assert_eq!(historical["state"], "unavailable");
        assert_eq!(historical["reason_code"], "quota_state_unavailable");
        store.write_raw(STATE, b"not json").unwrap();
        let broken = inputs::Snapshot::capture_loaded(&store, &config, None);
        let unavailable =
            quota_suspension_for_binding(&broken, &config, "go-a", &at, scope).unwrap();
        assert_eq!(unavailable["state"], "unavailable");
        assert_eq!(unavailable["reason_code"], "quota_state_unavailable");
        assert!(
            record_quota_denial(
                &store,
                &config,
                "go-a",
                scope,
                "0",
                &provider_integration::QuotaDenial { window: None }
            )
            .is_err()
        );
    }

    #[test]
    fn directory_in_place_of_observations_is_unavailable() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let runtime = inputs::read_runtime(&store).unwrap();
        let ids = inputs::provider_scope_binding_in_view(
            &runtime,
            runtime.loaded.as_ref().unwrap(),
            "go-a",
        )
        .unwrap();
        std::fs::create_dir_all(store.path(STATE).unwrap()).unwrap();
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let at = inputs::ExactInstant::from_chrono(Utc::now());
        let evidence =
            quota_suspension_for_binding(&snapshot, &config, "go-a", &at, (&ids.0, &ids.1))
                .unwrap();
        assert_eq!(evidence["reason_code"], "quota_state_unavailable");
    }

    #[tokio::test]
    async fn automatic_checks_reserve_the_next_attempt_even_when_credentials_fail() {
        let (_dir, store, config) = go_fixture();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let runtime = inputs::read_runtime(&store).unwrap();
        let ids = inputs::provider_scope_binding_in_view(
            &runtime,
            runtime.loaded.as_ref().unwrap(),
            "go-a",
        )
        .unwrap();
        let scope = (ids.0.as_str(), ids.1.as_str());
        record_quota_denial(
            &store,
            &config,
            "go-a",
            scope,
            "0",
            &provider_integration::QuotaDenial { window: None },
        )
        .unwrap();
        automatic_quota_tick(&store, &config).await.unwrap();
        let first = quota_status(&store, &config, "go-a", false).unwrap();
        assert_eq!(first["state"], "suspended");
        assert!(first["next_check_at"].is_string());
        automatic_quota_tick(&store, &config).await.unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["next_check_at"],
            first["next_check_at"]
        );
        assert_eq!(first["latest_check"]["outcome"], "failed");
        assert_eq!(first["latest_check"]["reason_code"], "invalid_reference");
        let deadline = first["next_check_at"].clone();
        let first_check = first["latest_check"].clone();

        resume_quota(&store, &config, "go-a").unwrap();
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let epoch = quota_epoch_for_binding(&snapshot, &config, "go-a", scope);
        let denial = provider_integration::QuotaDenial { window: None };
        record_quota_denial(&store, &config, "go-a", scope, &epoch, &denial).unwrap();
        automatic_quota_tick(&store, &config).await.unwrap();
        let after_resume = quota_status(&store, &config, "go-a", false).unwrap();
        assert_eq!(after_resume["next_check_at"], deadline);
        assert_eq!(after_resume["latest_check"], first_check);

        let check = capture(&store, &config, "go-a", "quota").unwrap();
        commit(&store, &check, Ok(go_usage())).unwrap();
        assert_eq!(
            quota_status(&store, &config, "go-a", false).unwrap()["state"],
            "available"
        );
        let snapshot = inputs::Snapshot::capture_loaded(&store, &config, None);
        let epoch = quota_epoch_for_binding(&snapshot, &config, "go-a", scope);
        record_quota_denial(&store, &config, "go-a", scope, &epoch, &denial).unwrap();
        let before_tick = quota_status(&store, &config, "go-a", false).unwrap();
        automatic_quota_tick(&store, &config).await.unwrap();
        let after_recovery = quota_status(&store, &config, "go-a", false).unwrap();
        assert_eq!(after_recovery["next_check_at"], deadline);
        assert_eq!(after_recovery["latest_check"], before_tick["latest_check"]);
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
        // The surface-neutral display projection is part of inspection, so no
        // reader has to decode the normalized payload itself.
        assert_eq!(
            before["billing"]["display"]["rows"][2]["label"]["key"],
            "monthly"
        );
        assert_eq!(
            before["billing"]["display"]["rows"][2]["percent"],
            fact["data"]["usage"]["monthly"]["percent"]
        );
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
