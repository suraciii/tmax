use crate::{
    Error, Result,
    config::{self, Config, FailureConditions, FailurePolicy},
    inputs::{self, Runtime},
    storage::Store,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const STATE: &str = "provider-failures/state.json";
const LOCK: &str = "provider-failures";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    identity: String,
    authorization: String,
    policy: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Trigger {
    at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    priority: i64,
    status: Option<u16>,
    reason_code: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    binding: Binding,
    consecutive_failures: u32,
    updated_at: DateTime<Utc>,
    trigger: Option<Trigger>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    states: BTreeMap<String, State>,
    #[serde(default)]
    reset_epochs: BTreeMap<String, String>,
}

fn signature(value: &impl Serialize) -> String {
    config::revision(&serde_json::to_vec(value).expect("failure metadata serialization"))
}
fn policy_signature(policy: &FailurePolicy) -> String {
    let mut policy = policy.clone();
    policy.errors.http_statuses.sort_unstable();
    signature(&policy)
}
fn binding(policy: &FailurePolicy, identity: &str, authorization: &str) -> Binding {
    Binding {
        identity: identity.into(),
        authorization: authorization.into(),
        policy: policy_signature(policy),
    }
}
fn invalid_record() -> Error {
    Error::new(
        "invalid_record",
        "Provider failure state fails its storage contract",
    )
}
fn read(store: &Store) -> Result<Snapshot> {
    let Some(bytes) = crate::storage::read_optional(&store.path(STATE)?)? else {
        return Ok(Snapshot::default());
    };
    inputs::check_historical_field_uniqueness(&bytes)?;
    let snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|_| invalid_record())?;
    if snapshot
        .reset_epochs
        .iter()
        .any(|(identity, epoch)| identity.is_empty() || epoch.is_empty())
    {
        return Err(invalid_record());
    }
    for (key, state) in &snapshot.states {
        if key != &signature(&state.binding)
            || state.binding.identity.is_empty()
            || state.binding.authorization.is_empty()
            || state.binding.policy.is_empty()
            || state.trigger.as_ref().is_some_and(|trigger| {
                trigger.expires_at <= trigger.at
                    || trigger
                        .status
                        .is_some_and(|status| !(400..600).contains(&status))
                    || trigger.reason_code.as_deref().is_some_and(|reason| {
                        !["upstream_connection_failed", "upstream_transport_failed"]
                            .contains(&reason)
                    })
                    || trigger.status.is_none() && trigger.reason_code.is_none()
            })
        {
            return Err(invalid_record());
        }
    }
    Ok(snapshot)
}

/// Match only explicitly configured HTTP or transport failure categories.
pub fn matches(conditions: &FailureConditions, status: Option<u16>, reason: Option<&str>) -> bool {
    status.is_some_and(|status| conditions.http_statuses.contains(&status))
        || reason == Some("upstream_connection_failed") && conditions.connection_errors
        || reason == Some("upstream_transport_failed") && conditions.transport_errors
}

impl Snapshot {
    pub(crate) fn capture(store: &Store, config: &Config) -> Result<Self> {
        if !config.providers.values().any(|provider| {
            provider
                .failure_policy
                .as_ref()
                .is_some_and(|policy| policy.enabled)
        }) {
            return Ok(Self::default());
        }
        let _guard = store.read_lock(LOCK)?;
        read(store)
    }

    /// Generation captured with admission; never refresh it for a retained attempt.
    pub(crate) fn epoch(&self, identity: &str) -> &str {
        self.reset_epochs
            .get(identity)
            .map(String::as_str)
            .unwrap_or("0")
    }

    pub(crate) fn inspect(
        &self,
        config: &Config,
        provider: &str,
        identity: Option<(&str, &str)>,
        now: DateTime<Utc>,
    ) -> Result<Value> {
        let provider = config
            .providers
            .get(provider)
            .ok_or_else(|| Error::new("unknown_resource", "Provider is not configured"))?;
        let policy = provider.failure_policy.as_ref();
        let mut result = json!({"state":"not_configured","reason_code":null,
            "policy":policy,"consecutive_failures":0,"active_priority":null,
            "triggered_at":null,"expires_at":null,"trigger_reason":null});
        let Some(policy) = policy else {
            return Ok(result);
        };
        if !policy.enabled {
            result["state"] = json!("disabled");
            return Ok(result);
        }
        let Some((identity, authorization)) = identity else {
            result["state"] = json!("unavailable");
            result["reason_code"] = json!("identity_unavailable");
            return Ok(result);
        };
        result["state"] = json!("idle");
        let Some(state) = self
            .states
            .get(&signature(&binding(policy, identity, authorization)))
        else {
            return Ok(result);
        };
        if now < state.updated_at {
            result["state"] = json!("unavailable");
            result["reason_code"] = json!("historical_state_unavailable");
            result["consecutive_failures"] = Value::Null;
            return Ok(result);
        }
        if let Some(trigger) = &state.trigger {
            let active = trigger.at <= now && now < trigger.expires_at;
            result["state"] = json!(if active { "active" } else { "expired" });
            result["reason_code"] = json!(if active {
                "consecutive_failure_threshold"
            } else {
                "penalty_expired"
            });
            result["consecutive_failures"] = json!(if active {
                state.consecutive_failures
            } else {
                0
            });
            result["active_priority"] = if active {
                json!(trigger.priority)
            } else {
                Value::Null
            };
            result["triggered_at"] = json!(inputs::timestamp(trigger.at));
            result["expires_at"] = json!(inputs::timestamp(trigger.expires_at));
            result["trigger_reason"] =
                json!({"status":trigger.status,"reason_code":trigger.reason_code});
        } else {
            result["consecutive_failures"] = json!(state.consecutive_failures);
            if state.consecutive_failures > 0 {
                result["state"] = json!("tracking");
            }
        }
        Ok(result)
    }
}

fn retained_keys(runtime: &Runtime) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for view in std::iter::once(&runtime.saved).chain(runtime.loaded.iter()) {
        for (name, provider) in &view.config.providers {
            if let Some(policy) = provider
                .failure_policy
                .as_ref()
                .filter(|policy| policy.enabled)
                && let Some((identity, authorization)) =
                    inputs::provider_scope_binding_in_view(runtime, view, name)
            {
                keys.insert(signature(&binding(policy, &identity, &authorization)));
            }
        }
    }
    keys
}

fn prune(snapshot: &mut Snapshot, runtime: &Runtime) {
    let retained = retained_keys(runtime);
    snapshot.states.retain(|key, _| retained.contains(key));
    let identities: BTreeSet<_> = std::iter::once(&runtime.saved)
        .chain(runtime.loaded.iter())
        .flat_map(|view| view.providers.values().map(|ids| ids.identity.as_str()))
        .collect();
    snapshot
        .reset_epochs
        .retain(|identity, _| identities.contains(identity.as_str()));
}

fn effective_policies(runtime: &Runtime) -> BTreeMap<&str, Option<String>> {
    let view = runtime.loaded.as_ref().unwrap_or(&runtime.saved);
    view.providers
        .iter()
        .map(|(name, ids)| {
            let policy = view
                .config
                .providers
                .get(name)
                .and_then(|provider| provider.failure_policy.as_ref())
                .filter(|policy| policy.enabled)
                .map(policy_signature);
            (ids.identity.as_str(), policy)
        })
        .collect()
}

/// Input lifecycle calls this before publication with the routing lock held.
pub(crate) fn retain(store: &Store, previous: &Runtime, runtime: &Runtime) -> Result<()> {
    let before = effective_policies(previous);
    let after = effective_policies(runtime);
    let changed: BTreeSet<_> = before
        .iter()
        .filter(|(identity, policy)| {
            after.get(*identity).and_then(Option::as_ref) != policy.as_ref()
        })
        .map(|(identity, _)| *identity)
        .collect();
    // A first policy needs no state, but its later transition must fence attempts
    // even when no outcome has ever created a failure state file.
    if changed.is_empty() && crate::storage::read_optional(&store.path(STATE)?)?.is_none() {
        return Ok(());
    }
    let _guard = store.lock(LOCK)?;
    let mut snapshot = read(store)?;
    let counts = (snapshot.states.len(), snapshot.reset_epochs.len());
    for identity in &changed {
        snapshot
            .reset_epochs
            .insert((*identity).into(), uuid::Uuid::new_v4().to_string());
    }
    snapshot
        .states
        .retain(|_, state| !changed.contains(state.binding.identity.as_str()));
    prune(&mut snapshot, runtime);
    if !changed.is_empty() || (snapshot.states.len(), snapshot.reset_epochs.len()) != counts {
        store.write_json(STATE, &snapshot)?;
    }
    Ok(())
}

/// Publish an admitted request outcome only while its loaded authority is current.
/// Lock order is routing, then provider-failures; callers must not hold either lock.
#[allow(clippy::too_many_arguments)]
pub fn record(
    store: &Store,
    config: &Config,
    provider: &str,
    identity: &str,
    authorization: &str,
    reset_epoch: &str,
    success: bool,
    status: Option<u16>,
    reason: Option<&str>,
    now: DateTime<Utc>,
) -> Result<()> {
    let Some(policy) = config
        .providers
        .get(provider)
        .and_then(|provider| provider.failure_policy.as_ref())
        .filter(|policy| policy.enabled)
    else {
        return Ok(());
    };
    let captured = binding(policy, identity, authorization);
    let _routing = store.lock("routing")?;
    let runtime = inputs::read_runtime(store)?;
    let view = runtime.loaded.as_ref().unwrap_or(&runtime.saved);
    let current = view
        .providers
        .iter()
        .find(|(_, ids)| ids.identity == identity)
        .and_then(|(name, _)| {
            let policy = view
                .config
                .providers
                .get(name)?
                .failure_policy
                .as_ref()
                .filter(|policy| policy.enabled)?;
            let (identity, authorization) =
                inputs::provider_scope_binding_in_view(&runtime, view, name)?;
            Some(binding(policy, &identity, &authorization))
        });
    if current.as_ref() != Some(&captured) {
        return Err(Error::new(
            "failure_state_changed",
            "Provider authorization or failure policy changed before the request outcome was recorded",
        ));
    }
    let _guard = store.lock(LOCK)?;
    let mut snapshot = read(store)?;
    if snapshot.epoch(identity) != reset_epoch {
        return Err(Error::new(
            "failure_state_changed",
            "Provider failure policy or reset generation changed after this attempt was admitted",
        ));
    }
    prune(&mut snapshot, &runtime);
    let key = signature(&captured);
    let state = snapshot.states.entry(key).or_insert_with(|| State {
        binding: captured,
        consecutive_failures: 0,
        updated_at: now,
        trigger: None,
    });
    if let Some(trigger) = &state.trigger {
        if now < trigger.expires_at {
            return Ok(());
        }
        state.consecutive_failures = 0;
        state.trigger = None;
    }
    if success {
        state.consecutive_failures = 0;
    } else if matches(&policy.errors, status, reason) {
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        if state.consecutive_failures >= policy.consecutive_failures {
            let expires_at = Duration::try_seconds(policy.duration_seconds)
                .and_then(|duration| now.checked_add_signed(duration))
                .ok_or_else(|| {
                    Error::new(
                        "invalid_failure_policy",
                        "Failure penalty expiry is outside the supported timestamp range",
                    )
                })?;
            state.trigger = Some(Trigger {
                at: now,
                expires_at,
                priority: policy.priority,
                status: status.filter(|status| policy.errors.http_statuses.contains(status)),
                reason_code: reason
                    .filter(|reason| matches(&policy.errors, None, Some(reason)))
                    .map(str::to_owned),
            });
        }
    }
    state.updated_at = now;
    store.write_json(STATE, &snapshot)
}

/// Read a saved/projected provider without reserving identities or mutating state.
pub fn inspect(
    store: &Store,
    config: &Config,
    provider: &str,
    now: DateTime<Utc>,
) -> Result<Value> {
    let _routing = store.read_lock("routing")?;
    let runtime = inputs::read_runtime(store)?;
    let identity = inputs::provider_scope_binding(&runtime, config, provider);
    Snapshot::capture(store, config)?.inspect(
        config,
        provider,
        identity
            .as_ref()
            .map(|(identity, authorization)| (identity.as_str(), authorization.as_str())),
        now,
    )
}

pub(crate) fn inspect_loaded(
    store: &Store,
    config: &Config,
    provider: &str,
    now: DateTime<Utc>,
) -> Result<Value> {
    let _routing = store.read_lock("routing")?;
    let runtime = inputs::read_runtime(store)?;
    let identity = runtime
        .loaded
        .as_ref()
        .filter(|view| signature(&view.config) == signature(config))
        .and_then(|view| inputs::provider_scope_binding_in_view(&runtime, view, provider));
    Snapshot::capture(store, config)?.inspect(
        config,
        provider,
        identity
            .as_ref()
            .map(|(identity, authorization)| (identity.as_str(), authorization.as_str())),
        now,
    )
}

/// Capture the policy/reset generation before admission. Reads never initialize state.
pub fn epoch(store: &Store, config: &Config, provider: &str) -> Result<String> {
    let declaration = config
        .providers
        .get(provider)
        .ok_or_else(|| Error::new("unknown_resource", "Provider is not configured"))?;
    if !declaration
        .failure_policy
        .as_ref()
        .is_some_and(|policy| policy.enabled)
    {
        return Ok("0".into());
    }
    let _routing = store.read_lock("routing")?;
    let runtime = inputs::read_runtime(store)?;
    let (identity, _) = if let Some(view) = runtime
        .loaded
        .as_ref()
        .filter(|view| signature(&view.config) == signature(config))
    {
        inputs::provider_scope_binding_in_view(&runtime, view, provider)
    } else {
        inputs::provider_scope_binding(&runtime, config, provider)
    }
    .ok_or_else(|| {
        Error::new(
            "identity_unavailable",
            "The provider's persistent identity is unavailable",
        )
    })?;
    let _guard = store.read_lock(LOCK)?;
    Ok(read(store)?.epoch(&identity).into())
}

/// Clear all retained policy/auth bindings belonging to this stable provider.
pub fn reset(store: &Store, config: &Config, provider: &str) -> Result<Value> {
    if !config.providers.contains_key(provider) {
        return Err(Error::new("unknown_resource", "Provider is not configured"));
    }
    let _routing = store.lock("routing")?;
    let runtime = inputs::read_runtime(store)?;
    let (identity, _) =
        inputs::provider_scope_binding(&runtime, config, provider).ok_or_else(|| {
            Error::new(
                "identity_unavailable",
                "The provider's persistent identity is unavailable",
            )
        })?;
    let _guard = store.lock(LOCK)?;
    let mut snapshot = read(store)?;
    let before = snapshot.states.len();
    snapshot
        .states
        .retain(|_, state| state.binding.identity != identity);
    let cleared = before - snapshot.states.len();
    snapshot
        .reset_epochs
        .insert(identity, uuid::Uuid::new_v4().to_string());
    prune(&mut snapshot, &runtime);
    store.write_json(STATE, &snapshot)?;
    Ok(json!({"provider":provider,"outcome":"reset","cleared_bindings":cleared}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Auth, Model, ModelAlias, Provider};

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + seconds, 0).unwrap()
    }

    fn save(store: &Store, config: &Config) {
        store
            .write_raw("tmax.toml", toml::to_string(config).unwrap().as_bytes())
            .unwrap();
    }

    fn fixture() -> (tempfile::TempDir, Store, Config, String, String) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            Provider {
                enabled: true,
                integration: Some("custom".into()),
                endpoints: [(
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                )]
                .into(),
                auth: Some(Auth::ApiKey {
                    key_ref: "key:first".into(),
                }),
                failure_policy: Some(FailurePolicy {
                    enabled: true,
                    errors: FailureConditions {
                        http_statuses: vec![429, 503],
                        connection_errors: true,
                        transport_errors: false,
                    },
                    consecutive_failures: 2,
                    priority: -40,
                    duration_seconds: 60,
                }),
                models: [(
                    "m".into(),
                    Model {
                        enabled: true,
                        priority: 20,
                        ..Default::default()
                    },
                )]
                .into(),
                ..Default::default()
            },
        );
        save(&store, &config);
        inputs::reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        let initial = config.clone();
        let runtime = inputs::read_runtime(&store).unwrap();
        let identity = runtime.saved.providers["p"].identity.clone();
        let key_ref =
            crate::secrets::put(&store, &format!("provider:{identity}:p"), b"fixture-key").unwrap();
        config.providers.get_mut("p").unwrap().auth = Some(Auth::ApiKey { key_ref });
        save(&store, &config);
        inputs::reconcile(&store, &initial, &config, &[]).unwrap();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let runtime = inputs::read_runtime(&store).unwrap();
        let (identity, authorization) =
            inputs::provider_scope_binding(&runtime, &config, "p").unwrap();
        (directory, store, config, identity, authorization)
    }

    #[allow(clippy::too_many_arguments)]
    fn outcome(
        store: &Store,
        config: &Config,
        identity: &str,
        authorization: &str,
        success: bool,
        status: Option<u16>,
        reason: Option<&str>,
        seconds: i64,
    ) {
        record(
            store,
            config,
            "p",
            identity,
            authorization,
            &epoch(store, config, "p").unwrap(),
            success,
            status,
            reason,
            at(seconds),
        )
        .unwrap();
    }

    #[test]
    fn loaded_policy_aba_rejects_old_admissions_with_or_without_prior_failure_state() {
        for prior_penalty in [false, true] {
            for transition in ["changed", "disabled", "removed"] {
                let (directory, store, original, identity, authorization) = fixture();
                if prior_penalty {
                    for seconds in 0..2 {
                        outcome(
                            &store,
                            &original,
                            &identity,
                            &authorization,
                            false,
                            Some(503),
                            None,
                            seconds,
                        );
                    }
                } else {
                    assert!(!store.path(STATE).unwrap().exists());
                }
                let admitted = Snapshot::capture(&store, &original).unwrap();
                let old_epoch = admitted.epoch(&identity).to_owned();
                let mut intermediate = original.clone();
                let provider = intermediate.providers.get_mut("p").unwrap();
                match transition {
                    "changed" => provider.failure_policy.as_mut().unwrap().priority = -100,
                    "disabled" => provider.failure_policy.as_mut().unwrap().enabled = false,
                    "removed" => provider.failure_policy = None,
                    _ => unreachable!(),
                }
                save(&store, &intermediate);
                inputs::reconcile(&store, &original, &intermediate, &[]).unwrap();
                assert_eq!(epoch(&store, &original, "p").unwrap(), old_epoch);
                inputs::activate(
                    &store,
                    &intermediate,
                    &config::raw_revision(&store).unwrap(),
                )
                .unwrap();
                let intermediate_epoch = read(&store).unwrap().epoch(&identity).to_owned();
                assert_ne!(intermediate_epoch, old_epoch);
                assert!(read(&store).unwrap().states.is_empty());
                save(&store, &original);
                inputs::reconcile(&store, &intermediate, &original, &[]).unwrap();
                inputs::activate(&store, &original, &config::raw_revision(&store).unwrap())
                    .unwrap();
                let reopened = Store::at(directory.path().to_owned());
                let current_epoch = epoch(&reopened, &original, "p").unwrap();
                assert_ne!(current_epoch, old_epoch);
                assert_ne!(current_epoch, intermediate_epoch);
                assert_eq!(admitted.epoch(&identity), old_epoch);
                assert_eq!(
                    inspect(&reopened, &original, "p", at(3)).unwrap()["state"],
                    "idle"
                );
                outcome(
                    &reopened,
                    &original,
                    &identity,
                    &authorization,
                    false,
                    Some(503),
                    None,
                    4,
                );
                for success in [false, true] {
                    assert_eq!(
                        record(
                            &reopened,
                            &original,
                            "p",
                            &identity,
                            &authorization,
                            &old_epoch,
                            success,
                            Some(if success { 200 } else { 503 }),
                            None,
                            at(5),
                        )
                        .unwrap_err()
                        .code,
                        "failure_state_changed"
                    );
                }
                assert_eq!(
                    inspect(&reopened, &original, "p", at(5)).unwrap()["consecutive_failures"],
                    1
                );
            }
        }
    }

    #[test]
    fn pending_edits_rename_and_same_policy_restart_preserve_generation_and_state() {
        for failures in [1, 2] {
            let (directory, store, original, identity, authorization) = fixture();
            for seconds in 0..failures {
                outcome(
                    &store,
                    &original,
                    &identity,
                    &authorization,
                    false,
                    Some(503),
                    None,
                    seconds,
                );
            }
            let captured = epoch(&store, &original, "p").unwrap();
            let state = inspect_loaded(&store, &original, "p", at(2)).unwrap();
            let bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
            let mut previous = original.clone();
            for transition in ["changed", "disabled", "removed", "original"] {
                let mut pending = original.clone();
                let provider = pending.providers.get_mut("p").unwrap();
                match transition {
                    "changed" => provider.failure_policy.as_mut().unwrap().duration_seconds = 120,
                    "disabled" => provider.failure_policy.as_mut().unwrap().enabled = false,
                    "removed" => provider.failure_policy = None,
                    "original" => {}
                    _ => unreachable!(),
                }
                save(&store, &pending);
                inputs::reconcile(&store, &previous, &pending, &[]).unwrap();
                assert_eq!(epoch(&store, &original, "p").unwrap(), captured);
                assert_eq!(
                    inspect_loaded(&store, &original, "p", at(2)).unwrap(),
                    state
                );
                assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), bytes);
                previous = pending;
            }
            let mut renamed = original.clone();
            let mut provider = renamed.providers.remove("p").unwrap();
            provider
                .failure_policy
                .as_mut()
                .unwrap()
                .errors
                .http_statuses
                .reverse();
            provider.models.get_mut("m").unwrap().priority = 99;
            renamed.providers.insert("renamed".into(), provider);
            save(&store, &renamed);
            inputs::reconcile(
                &store,
                &original,
                &renamed,
                &[("p".into(), "renamed".into())],
            )
            .unwrap();
            inputs::activate(&store, &renamed, &config::raw_revision(&store).unwrap()).unwrap();
            inputs::deactivate(&store).unwrap();
            let reopened = Store::at(directory.path().to_owned());
            inputs::activate(
                &reopened,
                &renamed,
                &config::raw_revision(&reopened).unwrap(),
            )
            .unwrap();
            assert_eq!(epoch(&reopened, &renamed, "renamed").unwrap(), captured);
            assert_eq!(std::fs::read(reopened.path(STATE).unwrap()).unwrap(), bytes);
            let preserved = inspect_loaded(&reopened, &renamed, "renamed", at(3)).unwrap();
            assert_eq!(
                preserved["consecutive_failures"],
                state["consecutive_failures"]
            );
            assert_eq!(preserved["expires_at"], state["expires_at"]);
            record(
                &reopened,
                &original,
                "p",
                &identity,
                &authorization,
                &captured,
                false,
                Some(503),
                None,
                at(4),
            )
            .unwrap();
            assert_eq!(
                inspect_loaded(&reopened, &renamed, "renamed", at(4)).unwrap()["state"],
                "active"
            );
        }
    }

    #[test]
    fn provider_remove_and_readd_never_restores_state_or_accepts_old_admission() {
        let (_directory, store, original, identity, authorization) = fixture();
        for seconds in 0..2 {
            outcome(
                &store,
                &original,
                &identity,
                &authorization,
                false,
                Some(503),
                None,
                seconds,
            );
        }
        let captured = epoch(&store, &original, "p").unwrap();
        let removed = Config::empty();
        save(&store, &removed);
        inputs::reconcile(&store, &original, &removed, &[]).unwrap();
        inputs::activate(&store, &removed, &config::raw_revision(&store).unwrap()).unwrap();
        assert!(read(&store).unwrap().states.is_empty());
        save(&store, &original);
        inputs::reconcile(&store, &removed, &original, &[]).unwrap();
        inputs::activate(&store, &original, &config::raw_revision(&store).unwrap()).unwrap();
        let runtime = inputs::read_runtime(&store).unwrap();
        assert_ne!(
            runtime.loaded.as_ref().unwrap().providers["p"].identity,
            identity
        );
        assert_eq!(
            inspect_loaded(&store, &original, "p", at(3)).unwrap()["state"],
            "idle"
        );
        assert_eq!(
            record(
                &store,
                &original,
                "p",
                &identity,
                &authorization,
                &captured,
                false,
                Some(503),
                None,
                at(3),
            )
            .unwrap_err()
            .code,
            "failure_state_changed"
        );
    }

    #[test]
    fn timestamp_overflow_rejects_without_publishing_partial_streak() {
        let (_directory, store, config, identity, authorization) = fixture();
        let now = DateTime::<Utc>::MAX_UTC - Duration::seconds(1);
        let captured = epoch(&store, &config, "p").unwrap();
        record(
            &store,
            &config,
            "p",
            &identity,
            &authorization,
            &captured,
            false,
            Some(503),
            None,
            now,
        )
        .unwrap();
        let before = std::fs::read(store.path(STATE).unwrap()).unwrap();
        assert_eq!(
            record(
                &store,
                &config,
                "p",
                &identity,
                &authorization,
                &captured,
                false,
                Some(503),
                None,
                now,
            )
            .unwrap_err()
            .code,
            "invalid_failure_policy"
        );
        assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), before);
    }

    #[test]
    fn explicit_conditions_success_and_unmatched_outcomes_preserve_consecutive_semantics() {
        let (_directory, store, config, identity, authorization) = fixture();
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            Some(429),
            None,
            0,
        );
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            Some(500),
            None,
            1,
        );
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            None,
            Some("cancelled"),
            2,
        );
        assert_eq!(
            inspect(&store, &config, "p", at(2)).unwrap()["consecutive_failures"],
            1
        );
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            true,
            Some(200),
            None,
            3,
        );
        assert_eq!(
            inspect(&store, &config, "p", at(3)).unwrap()["state"],
            "idle"
        );
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            None,
            Some("upstream_transport_failed"),
            4,
        );
        assert_eq!(
            inspect(&store, &config, "p", at(4)).unwrap()["consecutive_failures"],
            0
        );
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            None,
            Some("upstream_connection_failed"),
            5,
        );
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            6,
        );
        let state = inspect(&store, &config, "p", at(6)).unwrap();
        assert_eq!(state["active_priority"], -40);
        assert_eq!(state["trigger_reason"]["status"], 503);
    }

    #[test]
    fn active_penalty_survives_restart_success_and_errors_without_renewal_then_expires() {
        let (directory, store, config, identity, authorization) = fixture();
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            Some(429),
            None,
            0,
        );
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            1,
        );
        let before = std::fs::read(store.path(STATE).unwrap()).unwrap();
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            true,
            Some(200),
            None,
            20,
        );
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            40,
        );
        assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), before);
        let reopened = Store::at(directory.path().to_owned());
        assert_eq!(
            inspect(&reopened, &config, "p", at(60)).unwrap()["active_priority"],
            -40
        );
        let expired = inspect(&reopened, &config, "p", at(61)).unwrap();
        assert_eq!(expired["state"], "expired");
        assert_eq!(expired["consecutive_failures"], 0);
        assert!(expired["active_priority"].is_null());
        assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), before);
        outcome(
            &reopened,
            &config,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            61,
        );
        assert_eq!(
            inspect(&reopened, &config, "p", at(61)).unwrap()["consecutive_failures"],
            1
        );
        assert_eq!(
            inspect(&reopened, &config, "p", at(61)).unwrap()["state"],
            "tracking"
        );
    }

    #[test]
    fn late_outcomes_reject_authorization_and_loaded_policy_drift_not_pending_file_policy() {
        let (_directory, store, original, identity, authorization) = fixture();
        outcome(
            &store,
            &original,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            0,
        );
        let mut changed = original.clone();
        changed
            .providers
            .get_mut("p")
            .unwrap()
            .failure_policy
            .as_mut()
            .unwrap()
            .priority = -100;
        save(&store, &changed);
        inputs::reconcile(&store, &original, &changed, &[]).unwrap();
        // Saved edits are pending: the still-loaded policy remains authoritative.
        outcome(
            &store,
            &original,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            1,
        );
        assert_eq!(
            inspect_loaded(&store, &original, "p", at(1)).unwrap()["state"],
            "active"
        );
        assert_eq!(
            inspect(&store, &changed, "p", at(1)).unwrap()["state"],
            "idle"
        );
        inputs::activate(&store, &changed, &config::raw_revision(&store).unwrap()).unwrap();
        assert_eq!(
            record(
                &store,
                &original,
                "p",
                &identity,
                &authorization,
                "0",
                false,
                Some(503),
                None,
                at(2)
            )
            .unwrap_err()
            .code,
            "failure_state_changed"
        );
        assert!(read(&store).unwrap().states.is_empty());
        outcome(
            &store,
            &changed,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            3,
        );
        inputs::authorization_changed(&store, "p").unwrap();
        assert!(read(&store).unwrap().states.is_empty());
        assert_eq!(
            record(
                &store,
                &changed,
                "p",
                &identity,
                &authorization,
                "0",
                false,
                Some(503),
                None,
                at(4)
            )
            .unwrap_err()
            .code,
            "failure_state_changed"
        );
    }

    #[test]
    fn status_order_is_semantic_but_replaced_auth_configuration_rejects_late_outcomes() {
        let (_directory, store, original, identity, authorization) = fixture();
        outcome(
            &store,
            &original,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            0,
        );
        let mut reordered = original.clone();
        reordered
            .providers
            .get_mut("p")
            .unwrap()
            .failure_policy
            .as_mut()
            .unwrap()
            .errors
            .http_statuses
            .reverse();
        save(&store, &reordered);
        inputs::reconcile(&store, &original, &reordered, &[]).unwrap();
        inputs::activate(&store, &reordered, &config::raw_revision(&store).unwrap()).unwrap();
        outcome(
            &store,
            &original,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            1,
        );
        assert_eq!(
            inspect(&store, &reordered, "p", at(1)).unwrap()["state"],
            "active"
        );
        let mut replaced = reordered.clone();
        replaced.providers.get_mut("p").unwrap().auth = Some(Auth::ApiKey {
            key_ref: "different-key".into(),
        });
        save(&store, &replaced);
        inputs::reconcile(&store, &reordered, &replaced, &[]).unwrap();
        assert_eq!(
            record(
                &store,
                &original,
                "p",
                &identity,
                &authorization,
                "0",
                false,
                Some(503),
                None,
                at(2)
            )
            .unwrap_err()
            .code,
            "failure_state_changed"
        );
        assert!(read(&store).unwrap().states.is_empty());
    }

    #[test]
    fn rename_preserves_stable_state_manual_reset_and_removal_bound_retention() {
        let (_directory, store, original, identity, authorization) = fixture();
        outcome(
            &store,
            &original,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            0,
        );
        outcome(
            &store,
            &original,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            1,
        );
        let mut renamed = original.clone();
        let provider = renamed.providers.remove("p").unwrap();
        renamed.providers.insert("renamed".into(), provider);
        save(&store, &renamed);
        inputs::reconcile(
            &store,
            &original,
            &renamed,
            &[("p".into(), "renamed".into())],
        )
        .unwrap();
        assert_eq!(
            inspect(&store, &renamed, "renamed", at(2)).unwrap()["state"],
            "active"
        );
        assert_eq!(
            reset(&store, &renamed, "renamed").unwrap()["cleared_bindings"],
            1
        );
        assert_eq!(
            inspect_loaded(&store, &original, "p", at(2)).unwrap()["state"],
            "idle"
        );
        outcome(
            &store,
            &original,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            3,
        );
        save(&store, &Config::empty());
        inputs::reconcile(&store, &renamed, &Config::empty(), &[]).unwrap();
        assert_eq!(read(&store).unwrap().states.len(), 1);
        inputs::deactivate(&store).unwrap();
        assert!(read(&store).unwrap().states.is_empty());
    }

    #[test]
    fn corrupt_failure_storage_errors_surface_without_replacement() {
        let (_directory, store, config, identity, authorization) = fixture();
        store.write_raw(STATE, b"not JSON").unwrap();
        assert_eq!(
            inspect(&store, &config, "p", at(0)).unwrap_err().code,
            "invalid_record"
        );
        assert_eq!(
            record(
                &store,
                &config,
                "p",
                &identity,
                &authorization,
                "0",
                false,
                Some(503),
                None,
                at(0)
            )
            .unwrap_err()
            .code,
            "invalid_record"
        );
        assert_eq!(
            reset(&store, &config, "p").unwrap_err().code,
            "invalid_record"
        );
        assert_eq!(
            std::fs::read(store.path(STATE).unwrap()).unwrap(),
            b"not JSON"
        );
    }

    #[test]
    fn no_policy_needs_no_failure_store_and_explanation_is_read_only() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let mut config = Config::empty();
        config.providers.insert("p".into(), Provider::default());
        record(
            &store,
            &config,
            "p",
            "",
            "",
            "0",
            false,
            Some(503),
            None,
            at(0),
        )
        .unwrap();
        assert_eq!(
            inspect(&store, &config, "p", at(0)).unwrap()["state"],
            "not_configured"
        );
        assert!(!store.root.exists());
    }

    #[test]
    fn reset_epoch_rejects_inflight_outcomes_across_restart_and_is_bounded() {
        let (directory, store, config, identity, authorization) = fixture();
        let captured = epoch(&store, &config, "p").unwrap();
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            0,
        );
        reset(&store, &config, "p").unwrap();
        let reopened = Store::at(directory.path().to_owned());
        let current = epoch(&reopened, &config, "p").unwrap();
        assert_ne!(captured, current);
        assert_eq!(
            record(
                &reopened,
                &config,
                "p",
                &identity,
                &authorization,
                &captured,
                false,
                Some(503),
                None,
                at(1)
            )
            .unwrap_err()
            .code,
            "failure_state_changed"
        );
        assert_eq!(
            inspect(&reopened, &config, "p", at(1)).unwrap()["state"],
            "idle"
        );
        record(
            &reopened,
            &config,
            "p",
            &identity,
            &authorization,
            &current,
            false,
            Some(503),
            None,
            at(2),
        )
        .unwrap();
        assert_eq!(
            inspect(&reopened, &config, "p", at(2)).unwrap()["consecutive_failures"],
            1
        );
        reset(&reopened, &config, "p").unwrap();
        assert_ne!(epoch(&reopened, &config, "p").unwrap(), current);
        save(&reopened, &Config::empty());
        inputs::reconcile(&reopened, &config, &Config::empty(), &[]).unwrap();
        inputs::deactivate(&reopened).unwrap();
        assert!(read(&reopened).unwrap().reset_epochs.is_empty());
    }

    #[test]
    fn historical_projection_never_applies_future_penalty_or_future_ordinary_streak() {
        let (_directory, store, config, identity, authorization) = fixture();
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            10,
        );
        let earlier = inspect(&store, &config, "p", at(9)).unwrap();
        assert_eq!(earlier["reason_code"], "historical_state_unavailable");
        assert!(earlier["consecutive_failures"].is_null());
        assert!(earlier["active_priority"].is_null());
        assert_eq!(
            inspect(&store, &config, "p", at(10)).unwrap()["consecutive_failures"],
            1
        );
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            20,
        );
        let before_trigger = inspect(&store, &config, "p", at(19)).unwrap();
        assert_eq!(
            before_trigger["reason_code"],
            "historical_state_unavailable"
        );
        assert!(before_trigger["active_priority"].is_null());
        let explained = crate::routing::explain_loaded_effective(
            &store,
            &config,
            "p/m",
            "chat-completions",
            Some(&inputs::timestamp(at(19))),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(explained["selection"]["priority"], 20);
        assert_eq!(
            inspect(&store, &config, "p", at(20)).unwrap()["active_priority"],
            -40
        );
    }

    #[test]
    fn final_score_replacement_and_loaded_explanation_agree_with_actual_ranking() {
        let (_directory, store, mut config, identity, authorization) = fixture();
        let original = config.clone();
        let mut second = config.providers["p"].clone();
        second.failure_policy = None;
        second.auth = Some(Auth::ApiKey {
            key_ref: "key:second".into(),
        });
        second.models.get_mut("m").unwrap().priority = -50;
        config.providers.insert("q".into(), second);
        config.model_aliases.push(ModelAlias {
            id: "alias".into(),
            protocol: "chat-completions".into(),
            enabled: true,
            candidates: vec!["p/m".into(), "q/m".into()],
            candidate_reasoning_effort: Default::default(),
            continuity: None,
            fallback: None,
        });
        save(&store, &config);
        inputs::reconcile(&store, &original, &config, &[]).unwrap();
        let prior_auth = config.clone();
        let runtime = inputs::read_runtime(&store).unwrap();
        let q_identity = &runtime.saved.providers["q"].identity;
        let key_ref = crate::secrets::put(
            &store,
            &format!("provider:{q_identity}:q"),
            b"second-fixture-key",
        )
        .unwrap();
        config.providers.get_mut("q").unwrap().auth = Some(Auth::ApiKey { key_ref });
        save(&store, &config);
        inputs::reconcile(&store, &prior_auth, &config, &[]).unwrap();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            0,
        );
        outcome(
            &store,
            &config,
            &identity,
            &authorization,
            false,
            Some(503),
            None,
            1,
        );
        // Replacement is -40, not min(normal, -40): a normal -60 becomes -40.
        config
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .priority = -60;
        let old = inputs::read_runtime(&store).unwrap().saved.config;
        save(&store, &config);
        inputs::reconcile(&store, &old, &config, &[]).unwrap();
        inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
        let selected =
            crate::routing::select(&store, &config, "alias", "chat-completions", at(2)).unwrap();
        let explained = crate::routing::explain_loaded_effective(
            &store,
            &config,
            "alias",
            "chat-completions",
            Some(&inputs::timestamp(at(2))),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(selected.provider, "p");
        assert_eq!(selected.priority, -40);
        assert_eq!(explained["selection"]["priority"], -40);
        assert_eq!(
            explained["priority_evaluations"][0]["priority_evaluation"]["normal_priority"],
            -60
        );
        assert_eq!(
            explained["priority_evaluations"][0]["priority_evaluation"]["failure_state"]["state"],
            "active"
        );
        assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), bytes);
        assert_eq!(
            crate::routing::select(&store, &config, "alias", "chat-completions", at(61))
                .unwrap()
                .provider,
            "q"
        );
    }
}
