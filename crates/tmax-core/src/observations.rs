use crate::{
    Error, Result,
    config::{self, Config},
    inputs::{self, Runtime},
    integrations::{self, AcquiredObservation},
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
    let destination = integrations::observation_destination(p, scope)?;
    Ok(Binding {
        provider_identity: identity.0,
        authorization_generation: identity.1,
        integration: integrations::integration_id(p)?.into(),
        endpoint_contract: signature(&integrations::resolve(p)?),
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
            integrations::normalize_observation("official_balance_api", data)
                .is_ok_and(|normalized| normalized == *data)
        }
        Some("official_models_api") => {
            let Some(models) = data.get("models").and_then(Value::as_array) else {
                return false;
            };
            let body = json!({"data":models.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()});
            integrations::normalize_observation("official_models_api", &body)
                .is_ok_and(|normalized| normalized == *data)
        }
        _ => false,
    }
}
fn read(store: &Store) -> Result<Heads> {
    let Some(bytes) = crate::storage::read_optional(&store.path(STATE)?)? else {
        return Ok(Heads::default());
    };
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
    Ok(heads)
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
    unlock: Option<&[u8]>,
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
        let destination = integrations::observation_destination(provider, scope)?;
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
                integrations::acquire_observation(store, config, provider, destination, unlock)
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
                "dashboard":integrations::dashboard(integrations::integration_id(provider)?)})),
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
        as_of: Some(fact.observed_at),
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
        scopes.push(render_scope(head, scope, b.is_some(), &at));
    }
    Ok(
        json!({"provider":provider,"account_identity":null,"observations":scopes,
        "billing":{"kind":if integrations::integration_id(p)?=="deepseek"{"api_balance"}else{"unknown"},
            "state":if balance.is_some(){"observed"}else{"unknown"},"observation":balance,
            "freshness":temporal_applicability(balance,Some("official_balance_api"),&at),
            "subscription":p.subscription,"account_identity":null,
            "shared_upstream_limits":"unknown; providers may share upstream account limits",
            "aggregation":"not_performed","renewal":"unknown","reset":"unknown",
            "dashboard":integrations::dashboard(integrations::integration_id(p)?)}}),
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
    #[test]
    fn discovered_models_are_not_registered() {
        let normalized = integrations::normalize_observation(
            "official_models_api",
            &json!({"data":[{"id":"model"}]}),
        )
        .unwrap();
        assert_eq!(normalized["models"], json!(["model"]));
        assert_eq!(normalized["registered_models"], json!([]));
        assert_eq!(normalized["registration"], "not_performed");
    }
}
