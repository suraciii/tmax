use super::*;

use std::{
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

// Only the denial delta is retained; the current file remains authoritative for
// every other observation and for the manual-resume fence.
#[derive(Clone)]
struct PendingDenial {
    binding: Binding,
    epoch: String,
    manual_resume_revision: String,
    revision: String,
    suspension: Suspension,
    next_check_at: Option<DateTime<Utc>>,
}

type PendingKey = (PathBuf, String);
static PENDING: LazyLock<Mutex<BTreeMap<PendingKey, PendingDenial>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

#[derive(Clone)]
struct UnvalidatedDenial {
    binding: Binding,
    epoch: String,
    suspension: Suspension,
}

static UNVALIDATED: LazyLock<Mutex<BTreeMap<PendingKey, UnvalidatedDenial>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

fn fresh_recovery(state: Option<&QuotaState>, suspended_at: DateTime<Utc>) -> bool {
    state
        .and_then(|state| state.latest_available_check_started_at)
        .is_some_and(|started| started > suspended_at)
}

fn applies(pending: &PendingDenial, state: Option<&QuotaState>) -> bool {
    if state.is_some_and(|s| s.manual_resume_revision != pending.manual_resume_revision) {
        return false;
    }
    !fresh_recovery(state, pending.suspension.suspended_at)
        && !state.is_some_and(|s| {
            s.epoch != pending.epoch
                && s.latest_denial_revision.as_deref() == Some(pending.revision.as_str())
        })
}

pub(super) fn effective_heads(store: &Store) -> Result<(Heads, BTreeSet<String>)> {
    let mut heads = read(store)?;
    let mut applied = BTreeSet::new();
    let overlay = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    for ((root, key), denial) in overlay.iter() {
        if root != &store.root || !applies(denial, heads.quota.get(key)) {
            continue;
        }
        let state = heads.quota.entry(key.clone()).or_insert_with(|| {
            let mut state = QuotaState::new(denial.binding.clone());
            state.epoch = denial.epoch.clone();
            state.manual_resume_revision = denial.manual_resume_revision.clone();
            state
        });
        state.latest_denial_at = Some(denial.suspension.suspended_at);
        state.latest_denial_revision = Some(denial.revision.clone());
        state.suspension = Some(denial.suspension.clone());
        state.manually_resumed = false;
        if state.next_check_at.is_none() {
            state.next_check_at = denial.next_check_at;
        }
        applied.insert(key.clone());
    }
    drop(overlay);
    let unavailable = UNVALIDATED.lock().unwrap_or_else(|e| e.into_inner());
    for ((root, key), denial) in unavailable.iter() {
        if root == &store.root
            && heads
                .quota
                .get(key)
                .map_or("0", |state| state.epoch.as_str())
                == denial.epoch
            && !fresh_recovery(heads.quota.get(key), denial.suspension.suspended_at)
        {
            heads.unavailable_quota.insert(key.clone());
        }
    }
    Ok((heads, applied))
}

// Call only after a successful atomic write under the observations lock. A
// failed write can have renamed already, and is never an acknowledgment.
fn acknowledge(store: &Store, heads: &Heads) {
    PENDING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(root, key), denial| {
            if root != &store.root {
                return true;
            }
            let state = heads.quota.get(key);
            applies(denial, state)
                && !state.is_some_and(|s| {
                    s.latest_denial_revision.as_deref() == Some(denial.revision.as_str())
                })
        });
}

pub(super) fn write_heads(store: &Store, heads: &Heads) -> Result<()> {
    store.write_json(STATE, heads)?;
    acknowledge(store, heads);
    Ok(())
}

pub(super) fn prune_pending(store: &Store, runtime: &Runtime) -> Result<()> {
    let keys = retained_bindings(runtime)?;
    PENDING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(root, key), _| root != &store.root || keys.contains(key));
    UNVALIDATED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(root, key), _| root != &store.root || keys.contains(key));
    Ok(())
}

fn discard_superseded(store: &Store, heads: &Heads) {
    PENDING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(root, key), denial| root != &store.root || applies(denial, heads.quota.get(key)));
}

fn add_pending_evidence(value: &mut Value) {
    value["persistence"] = json!("pending");
    value["durability"] = json!("unconfirmed");
    value["persistence_error"] = json!("storage_unavailable");
}

fn suspension_value(state: &QuotaState, s: &Suspension) -> Value {
    json!({"state":"suspended", "reason_code":"quota_suspended", "source":s.source,
        "suspended_at":s.suspended_at, "window":s.window, "reset_at":s.reset_at,
        "next_check_at":state.next_check_at})
}

fn selected_quota_key(
    snapshot: &inputs::Snapshot<'_>,
    config: &Config,
    provider: &str,
    scope: (&str, &str),
) -> Option<String> {
    let ids = snapshot.provider_scope_binding(provider)?;
    if ids.0 != scope.0 || ids.1 != scope.1 {
        return None;
    }
    Some(signature(
        &binding_with_scope(config, provider, "quota", ids).ok()?,
    ))
}

pub(crate) fn quota_suspension_for_binding(
    snapshot: &inputs::Snapshot<'_>,
    config: &Config,
    provider: &str,
    at: &inputs::ExactInstant,
    scope: (&str, &str),
) -> Option<Value> {
    let key = selected_quota_key(snapshot, config, provider, scope)?;
    let binding =
        binding_with_scope(config, provider, "quota", (scope.0.into(), scope.1.into())).ok()?;
    if binding.integration != "opencode-go" || binding.source.as_deref() != Some("go_usage_api") {
        return None;
    }
    let unavailable = || json!({"state":"unavailable","reason_code":"quota_state_unavailable"});
    let Some(observations) = snapshot.observations() else {
        return Some(unavailable());
    };
    if observations.heads.unavailable_quota.contains(&key) {
        return Some(unavailable());
    }
    let state = observations.heads.quota.get(&key)?;
    let suspension = state.suspension.as_ref()?;
    if at < &inputs::ExactInstant::from_chrono(suspension.suspended_at) {
        return Some(unavailable());
    }
    let mut value = suspension_value(state, suspension);
    if observations.pending.contains(&key) {
        add_pending_evidence(&mut value);
    }
    Some(value)
}

pub(crate) fn quota_epoch_for_binding(
    snapshot: &inputs::Snapshot<'_>,
    config: &Config,
    provider: &str,
    scope: (&str, &str),
) -> String {
    selected_quota_key(snapshot, config, provider, scope)
        .and_then(|key| {
            snapshot
                .observations()?
                .heads
                .quota
                .get(&key)
                .map(|s| s.epoch.clone())
        })
        .unwrap_or_else(|| "0".into())
}

pub(crate) fn record_quota_denial(
    store: &Store,
    config: &Config,
    provider: &str,
    scope: (&str, &str),
    epoch: &str,
    denial: &provider_integration::QuotaDenial,
) -> Result<()> {
    record_quota_denial_with_writer(
        store,
        config,
        provider,
        scope,
        epoch,
        denial,
        |store, heads| store.write_json(STATE, heads),
    )
}

pub(super) fn record_quota_denial_with_writer(
    store: &Store,
    config: &Config,
    provider: &str,
    scope: (&str, &str),
    epoch: &str,
    denial: &provider_integration::QuotaDenial,
    writer: impl FnOnce(&Store, &Heads) -> Result<()>,
) -> Result<()> {
    record_quota_denial_with_locker(
        store,
        config,
        provider,
        scope,
        epoch,
        denial,
        (writer, |store, name| store.lock(name)),
    )
}

pub(super) fn record_quota_denial_with_locker(
    store: &Store,
    config: &Config,
    provider: &str,
    scope: (&str, &str),
    epoch: &str,
    denial: &provider_integration::QuotaDenial,
    io: (
        impl FnOnce(&Store, &Heads) -> Result<()>,
        impl FnMut(&Store, &str) -> Result<crate::storage::LockGuard>,
    ),
) -> Result<()> {
    let (writer, mut lock) = io;
    let requested =
        binding_with_scope(config, provider, "quota", (scope.0.into(), scope.1.into()))?;
    if requested.integration != "opencode-go" || requested.source.as_deref() != Some("go_usage_api")
    {
        return Ok(());
    }
    let receipt = UnvalidatedDenial {
        binding: requested,
        epoch: epoch.into(),
        suspension: Suspension {
            source: "go_denial".into(),
            suspended_at: Utc::now(),
            window: denial.window.clone(),
            reset_at: None,
        },
    };
    let locked = (|| {
        Ok((
            lock(store, "configuration")?,
            lock(store, "routing")?,
            lock(store, "observations")?,
        ))
    })();
    let (_configuration, _routing, _observations) = match locked {
        Ok(guards) => guards,
        Err(error) => {
            retain_after_lock_failure(store, &receipt);
            return Err(error);
        }
    };
    let prepared = (|| {
        let runtime = inputs::read_runtime(store)?;
        let heads = install_denial(store, &runtime, &receipt)?;
        Ok((runtime, heads))
    })();
    let (runtime, heads) = match prepared {
        Ok(value) => value,
        Err(error) => {
            mark_unvalidated(store, receipt);
            return Err(error);
        }
    };
    let Some(mut heads) = heads else {
        return Ok(());
    };
    prune(&mut heads, &runtime)?;
    prune_pending(store, &runtime)?;
    writer(store, &heads)?;
    acknowledge(store, &heads);
    Ok(())
}

fn mark_unvalidated(store: &Store, receipt: UnvalidatedDenial) {
    let key = (store.root.clone(), signature(&receipt.binding));
    let mut entries = UNVALIDATED.lock().unwrap_or_else(|e| e.into_inner());
    if entries
        .get(&key)
        .is_none_or(|old| old.suspension.suspended_at <= receipt.suspension.suspended_at)
    {
        entries.insert(key, receipt);
    }
}

fn retain_after_lock_failure(store: &Store, receipt: &UnvalidatedDenial) {
    let validated = (|| -> Result<()> {
        let _configuration = store
            .read_lock("configuration")?
            .ok_or_else(crate::storage::storage_error)?;
        let _routing = store
            .read_lock("routing")?
            .ok_or_else(crate::storage::storage_error)?;
        let _observations = store
            .read_lock("observations")?
            .ok_or_else(crate::storage::storage_error)?;
        let runtime = inputs::read_runtime(store)?;
        install_denial(store, &runtime, receipt)?;
        Ok(())
    })();
    if validated.is_err() {
        mark_unvalidated(store, receipt.clone());
    }
}

// The caller holds all three publication locks (shared or exclusive).
fn install_denial(
    store: &Store,
    runtime: &Runtime,
    receipt: &UnvalidatedDenial,
) -> Result<Option<Heads>> {
    let Some(view) = runtime.loaded.as_ref() else {
        return Ok(None);
    };
    let mut matched = false;
    for provider in view.config.providers.keys() {
        let Some(ids) = inputs::provider_scope_binding_in_view(runtime, view, provider) else {
            continue;
        };
        if ids.0 == receipt.binding.provider_identity
            && ids.1 == receipt.binding.authorization_generation
            && binding_with_scope(&view.config, provider, "quota", ids)? == receipt.binding
        {
            matched = true;
            break;
        }
    }
    if !matched {
        return Ok(None);
    }
    let (mut heads, _) = effective_heads(store)?;
    let key = signature(&receipt.binding);
    if fresh_recovery(heads.quota.get(&key), receipt.suspension.suspended_at) {
        return Ok(None);
    }
    let state = heads
        .quota
        .entry(key.clone())
        .or_insert_with(|| QuotaState::new(receipt.binding.clone()));
    if state.epoch != receipt.epoch {
        return Ok(None);
    }
    let now = receipt.suspension.suspended_at;
    state.next_check_at.get_or_insert(now);
    state.latest_denial_at = Some(now);
    state.latest_denial_revision = Some(token());
    state.manually_resumed = false;
    state.suspension = Some(receipt.suspension.clone());
    let pending = PendingDenial {
        binding: state.binding.clone(),
        epoch: state.epoch.clone(),
        manual_resume_revision: state.manual_resume_revision.clone(),
        revision: state
            .latest_denial_revision
            .clone()
            .expect("denial revision was set"),
        suspension: receipt.suspension.clone(),
        next_check_at: state.next_check_at,
    };
    let map_key = (store.root.clone(), key);
    let mut entries = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    if entries
        .get(&map_key)
        .is_none_or(|old| old.suspension.suspended_at <= now)
    {
        entries.insert(map_key.clone(), pending);
    }
    drop(entries);
    let mut unavailable = UNVALIDATED.lock().unwrap_or_else(|e| e.into_inner());
    if unavailable
        .get(&map_key)
        .is_some_and(|old| old.suspension.suspended_at <= now)
    {
        unavailable.remove(&map_key);
    }
    Ok(Some(heads))
}

pub fn quota_status(store: &Store, config: &Config, provider: &str, loaded: bool) -> Result<Value> {
    let p = config
        .providers
        .get(provider)
        .ok_or_else(|| Error::new("unknown_resource", "Provider is not configured"))?;
    if provider_integration::integration_id(p)? != "opencode-go" {
        return Ok(json!({"state":"unsupported"}));
    }
    let _routing = store.read_lock("routing")?;
    let runtime = inputs::read_runtime(store)?;
    let ids = if loaded {
        runtime
            .loaded
            .as_ref()
            .filter(|v| signature(&v.config) == signature(config))
            .and_then(|v| inputs::provider_scope_binding_in_view(&runtime, v, provider))
    } else {
        inputs::provider_scope_binding(&runtime, config, provider)
    };
    let Some(ids) = ids else {
        return Ok(json!({"state":"unavailable","reason_code":"binding_unavailable"}));
    };
    let key = signature(&binding_with_scope(config, provider, "quota", ids)?);
    let (heads, pending) = if loaded {
        effective_heads(store)?
    } else {
        (read(store)?, BTreeSet::new())
    };
    if heads.unavailable_quota.contains(&key) {
        return Ok(json!({"state":"unavailable","reason_code":"quota_state_unavailable"}));
    }
    let mut result = match heads.quota.get(&key) {
        Some(state) if state.suspension.is_some() => {
            suspension_value(state, state.suspension.as_ref().unwrap())
        }
        Some(state) if state.manually_resumed => json!({"state":"manually_resumed"}),
        _ => json!({"state":"available"}),
    };
    if pending.contains(&key) {
        add_pending_evidence(&mut result);
    }
    if let Some(attempt) = heads.heads.get(&key).map(|head| &head.last_attempt) {
        result["latest_check"] = json!({"started_at":attempt.started_at,
            "finished_at":attempt.finished_at, "outcome":attempt.outcome,
            "reason_code":attempt.reason_code});
    }
    Ok(result)
}

pub fn resume_quota(store: &Store, config: &Config, provider: &str) -> Result<Value> {
    let _configuration = store.lock("configuration")?;
    if signature(&config::load(store)?.config) != signature(config) {
        return Err(Error::new(
            "observations_changed",
            "Saved provider authority changed before resume",
        ));
    }
    resume_quota_locked(store, config, provider)
}

/// The caller holds the configuration lock and validates its requested revision.
pub(crate) fn resume_quota_locked(store: &Store, config: &Config, provider: &str) -> Result<Value> {
    let p = config
        .providers
        .get(provider)
        .ok_or_else(|| Error::new("unknown_resource", "Provider is not configured"))?;
    if provider_integration::integration_id(p)? != "opencode-go" {
        return Ok(json!({"state":"unsupported","resumed":false}));
    }
    let runtime = inputs::read_runtime(store)?;
    if signature(&runtime.saved.config) != signature(config) {
        inputs::reconcile(store, &runtime.saved.config, config, &[])?;
    }
    let _routing = store.lock("routing")?;
    let _observations = store.lock("observations")?;
    let runtime = inputs::read_runtime(store)?;
    let Some(saved_ids) = inputs::provider_scope_binding(&runtime, config, provider) else {
        return Ok(
            json!({"state":"unavailable","reason_code":"binding_unavailable","resumed":false}),
        );
    };
    let mut bindings = vec![binding_with_scope(
        config,
        provider,
        "quota",
        saved_ids.clone(),
    )?];
    if let Some(view) = &runtime.loaded
        && let Some(ids) = inputs::provider_scope_binding_in_view(&runtime, view, provider)
        && ids.0 == saved_ids.0
        && view
            .config
            .providers
            .get(provider)
            .is_some_and(|p| provider_integration::integration_id(p).ok() == Some("opencode-go"))
    {
        let b = binding_with_scope(&view.config, provider, "quota", ids)?;
        if !bindings.contains(&b) {
            bindings.push(b);
        }
    }
    let (mut heads, _) = effective_heads(store)?;
    let mut resumed = false;
    for b in bindings {
        let key = signature(&b);
        let fact_revision = heads
            .heads
            .get(&key)
            .and_then(|h| h.fact.as_ref())
            .map(|f| f.revision.clone());
        let state = heads.quota.entry(key).or_insert_with(|| QuotaState::new(b));
        resumed |= state.suspension.is_some();
        state.epoch = token();
        state.manual_resume_revision = token();
        state.suspension = None;
        state.manually_resumed = true;
        state.suppressed_fact_revision = fact_revision;
        state.latest_denial_at = None;
    }
    prune(&mut heads, &runtime)?;
    prune_pending(store, &runtime)?;
    write_heads(store, &heads)?;
    Ok(json!({"state":"manually_resumed","resumed":resumed}))
}

pub(crate) async fn automatic_quota_tick(store: &Store, config: &Config) -> Result<()> {
    // Repair local authority before checking enabled providers or configuration
    // drift. A successful repair does not itself imply quota recovery.
    let has_pending = PENDING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .keys()
        .any(|(root, _)| root == &store.root)
        || UNVALIDATED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .any(|(root, _)| root == &store.root);
    if has_pending {
        let _configuration = store.lock("configuration")?;
        let _routing = store.lock("routing")?;
        let _observations = store.lock("observations")?;
        let runtime = inputs::read_runtime(store)?;
        prune_pending(store, &runtime)?;
        let unresolved: Vec<_> = UNVALIDATED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|((root, _), _)| root == &store.root)
            .map(|(key, receipt)| (key.clone(), receipt.clone()))
            .collect();
        for (key, receipt) in unresolved {
            install_denial(store, &runtime, &receipt)?;
            let mut entries = UNVALIDATED.lock().unwrap_or_else(|e| e.into_inner());
            if entries.get(&key).is_some_and(|current| {
                current.suspension.suspended_at == receipt.suspension.suspended_at
            }) {
                entries.remove(&key);
            }
        }
        discard_superseded(store, &read(store)?);
        let (mut heads, pending) = effective_heads(store)?;
        if !pending.is_empty() {
            prune(&mut heads, &runtime)?;
            write_heads(store, &heads)?;
        }
    }
    let expected = signature(config);
    for (provider, p) in &config.providers {
        if !p.enabled || provider_integration::integration_id(p)? != "opencode-go" {
            continue;
        }
        let Some(destination) = provider_integration::observation_destination(p, "quota")? else {
            continue;
        };
        let captured = {
            let _configuration = store.lock("configuration")?;
            let _routing = store.lock("routing")?;
            let _observations = store.lock("observations")?;
            let runtime = inputs::read_runtime(store)?;
            if signature(&config::load(store)?.config) != expected
                || signature(&runtime.saved.config) != expected
                || runtime
                    .loaded
                    .as_ref()
                    .is_none_or(|v| signature(&v.config) != expected)
            {
                return Ok(());
            }
            let view = runtime.loaded.as_ref().unwrap();
            let Some(ids) = inputs::provider_scope_binding_in_view(&runtime, view, provider) else {
                continue;
            };
            if inputs::provider_scope_binding_in_view(&runtime, &runtime.saved, provider).as_ref()
                != Some(&ids)
            {
                continue;
            }
            let b = binding_with_scope(config, provider, "quota", ids)?;
            let key = signature(&b);
            let (mut heads, _) = effective_heads(store)?;
            let now = Utc::now();
            let Some(state) = heads.quota.get_mut(&key) else {
                continue;
            };
            if state.suspension.is_none() || state.next_check_at.is_some_and(|at| at > now) {
                continue;
            }
            state.next_check_at = Some(now + chrono::Duration::seconds(60));
            let captured = Capture {
                binding: b,
                head_revision: revision(heads.heads.get(&key)).into(),
                quota_epoch: state.epoch.clone(),
                denial_revision: state.latest_denial_revision.clone(),
                automatic_config: Some(expected.clone()),
                started_at: now,
            };
            write_heads(store, &heads)?;
            captured
        };
        let acquired =
            provider_integration::acquire_observation(store, config, p, destination).await;
        if let Err(error) = commit(store, &captured, acquired)
            && error.code != "observations_changed"
        {
            return Err(error);
        }
    }
    Ok(())
}
