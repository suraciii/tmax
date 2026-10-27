use crate::{
    Error, Result,
    config::{self, Config},
    storage::Store,
};

use super::{
    lineage::{invalidate_changed, make_view, retain, signature, token},
    scheduler::clear_store_schedule,
    store::{read_runtime, write_runtime},
};

/// Authorization replacement or uncertainty advances a lineage even when the
/// desired reference spelling did not change. A qualified token refresh of the
/// same authorization must not call this function.
pub fn authorization_changed(store: &Store, provider: &str) -> Result<()> {
    let _lock = store.lock("routing")?;
    authorization_changed_locked(store, provider)
}

/// Caller holds the routing lock, acquired before any native lock. Native owner
/// operations invalidate synchronously before recording uncertainty or spawning.
pub(crate) fn authorization_changed_locked(store: &Store, provider: &str) -> Result<()> {
    let mut runtime = read_runtime(store)?;
    let previous = runtime.clone();
    let identity = runtime
        .saved
        .providers
        .get(provider)
        .or_else(|| {
            runtime
                .loaded
                .as_ref()
                .and_then(|v| v.providers.get(provider))
        })
        .ok_or_else(|| Error::new("unresolved_reference", "Provider identity is unavailable"))?
        .identity
        .clone();
    let generation = token();
    runtime
        .authorization
        .insert(identity.clone(), generation.clone());
    for view in std::iter::once(&mut runtime.saved).chain(runtime.loaded.iter_mut()) {
        for ids in view.providers.values_mut() {
            if ids.identity == identity {
                ids.authorization = generation.clone();
            }
        }
    }
    invalidate_changed(&mut runtime, &previous);
    retain(&mut runtime);
    write_runtime(store, &runtime)?;
    crate::observations::retain(store, &runtime)
}
/// Reconcile saved intent, preserving identities only for explicit renames.
pub(crate) fn reconcile(
    store: &Store,
    old: &Config,
    new: &Config,
    renames: &[(String, String)],
) -> Result<()> {
    let _lock = store.lock("routing")?;
    let mut runtime = read_runtime(store)?;
    let before = runtime.clone();
    let previous = if signature(&runtime.saved.config) == signature(old) {
        runtime.saved.clone()
    } else {
        let prior = runtime.saved.clone();
        make_view(&mut runtime, &prior, old, &[])
    };
    runtime.saved = make_view(&mut runtime, &previous, new, renames);
    runtime.saved.revision = config::load(store)
        .ok()
        .filter(|file| signature(&file.config) == signature(new))
        .map(|file| file.revision);
    invalidate_changed(&mut runtime, &before);
    retain(&mut runtime);
    write_runtime(store, &runtime)?;
    crate::observations::retain(store, &runtime)
}
/// Called only after successful service activation, not by inspection.
pub fn activate(store: &Store, config: &Config, revision: &str) -> Result<()> {
    let _lock = store.lock("routing")?;
    let mut runtime = read_runtime(store)?;
    let before = runtime.clone();
    let mut view = if signature(&runtime.saved.config) == signature(config) {
        runtime.saved.clone()
    } else if let Some(view) = runtime
        .loaded
        .as_ref()
        .filter(|v| signature(&v.config) == signature(config))
    {
        view.clone()
    } else {
        let old = runtime.loaded.clone().unwrap_or_default();
        make_view(&mut runtime, &old, config, &[])
    };
    view.revision = Some(revision.into());
    runtime.loaded = Some(view);
    invalidate_changed(&mut runtime, &before);
    retain(&mut runtime);
    write_runtime(store, &runtime)?;
    crate::observations::retain(store, &runtime)
}
pub fn deactivate(store: &Store) -> Result<()> {
    let _lock = store.lock("routing")?;
    let mut runtime = read_runtime(store)?;
    clear_store_schedule(store);
    runtime.loaded = None;
    retain(&mut runtime);
    write_runtime(store, &runtime)?;
    crate::observations::retain(store, &runtime)
}

pub(super) fn ensure_saved_view(store: &Store, captured: &Config) -> Result<()> {
    let _configuration = store.lock("configuration")?;
    let current = config::load(store)?;
    if signature(&current.config) != signature(captured) {
        return Err(Error::new(
            "inputs_changed",
            "Saved configuration changed before the input operation",
        ));
    }
    let runtime = read_runtime(store)?;
    if signature(&runtime.saved.config) != signature(captured) {
        reconcile(store, &runtime.saved.config, captured, &[])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{
        exact::ExactInstant,
        snapshot::Snapshot,
        test_support::{fixture, retain_value, save_intent},
    };
    use super::*;
    #[test]
    fn authorization_invalidation_matches_each_saved_and_loaded_declaration() {
        let (_directory, store, config) = fixture();
        retain_value(&store, &config);
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let loaded_declaration = Snapshot::capture_loaded(&store, &config, None)
            .input_revision("p", "i")["declaration_revision"]
            .clone();
        let mut pending = config.clone();
        pending
            .providers
            .get_mut("p")
            .unwrap()
            .inputs
            .get_mut("i")
            .unwrap()
            .max_age_seconds = 1800;
        save_intent(&store, &pending);
        reconcile(&store, &config, &pending, &[]).unwrap();
        authorization_changed(&store, "p").unwrap();
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        let loaded = Snapshot::capture_loaded(&store, &config, None);
        assert_eq!(
            loaded.input_revision("p", "i")["declaration_revision"],
            loaded_declaration
        );
        assert_eq!(
            loaded.input("p", "i", &at)["availability"]["reason_code"],
            "authorization_mismatch"
        );
        assert_eq!(
            Snapshot::capture(&store, &pending).input("p", "i", &at)["availability"]["reason_code"],
            "authorization_mismatch"
        );
        let mut changed = pending.clone();
        let provider = changed.providers.get_mut("p").unwrap();
        provider.inputs.get_mut("i").unwrap().max_age_seconds = 900;
        provider.auth = Some(config::Auth::ApiKey {
            key_ref: "env:REPLACED".into(),
        });
        save_intent(&store, &changed);
        reconcile(&store, &pending, &changed, &[]).unwrap();
        assert_eq!(
            Snapshot::capture(&store, &changed).input("p", "i", &at)["availability"]["reason_code"],
            "declaration_mismatch"
        );
    }

    #[test]
    fn provider_rename_preserves_identity_but_model_coverage_rename_invalidates() {
        let (_directory, store, config) = fixture();
        let revision = retain_value(&store, &config);
        let before = Snapshot::capture(&store, &config)
            .destination_identity("p", "m")
            .unwrap();
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let mut renamed = config.clone();
        let provider = renamed.providers.remove("p").unwrap();
        renamed.providers.insert("renamed".into(), provider);
        save_intent(&store, &renamed);
        reconcile(&store, &config, &renamed, &[("p".into(), "renamed".into())]).unwrap();
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        let snapshot = Snapshot::capture(&store, &renamed);
        assert_eq!(
            snapshot.destination_identity("renamed", "m").unwrap(),
            before
        );
        assert_eq!(
            snapshot.input("renamed", "i", &at)["current_revision"],
            revision
        );
        assert_eq!(
            snapshot.input("renamed", "i", &at)["availability"]["state"],
            "available"
        );
        let mut model_renamed = renamed.clone();
        let p = model_renamed.providers.get_mut("renamed").unwrap();
        let model = p.models.remove("m").unwrap();
        p.models.insert("new-m".into(), model);
        p.inputs.get_mut("i").unwrap().models = vec!["new-m".into()];
        save_intent(&store, &model_renamed);
        reconcile(
            &store,
            &renamed,
            &model_renamed,
            &[("renamed/m".into(), "renamed/new-m".into())],
        )
        .unwrap();
        let after = Snapshot::capture(&store, &model_renamed);
        assert_eq!(
            after.destination_identity("renamed", "new-m").unwrap().2,
            before.2
        );
        assert_eq!(
            after.input("renamed", "i", &at)["availability"]["reason_code"],
            "declaration_mismatch"
        );
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).input("p", "i", &at)["availability"]["state"],
            "available"
        );
    }
    #[test]
    fn pending_declarations_keep_only_saved_and_loaded_heads() {
        let (_directory, store, config) = fixture();
        retain_value(&store, &config);
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let old_snapshot = Snapshot::capture_loaded(&store, &config, None);
        let mut changed = config.clone();
        changed
            .providers
            .get_mut("p")
            .unwrap()
            .inputs
            .get_mut("i")
            .unwrap()
            .max_age_seconds = 1800;
        save_intent(&store, &changed);
        reconcile(&store, &config, &changed, &[]).unwrap();
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(read_runtime(&store).unwrap().channels.len(), 2);
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).input("p", "i", &at)["availability"]["state"],
            "available"
        );
        assert_eq!(
            Snapshot::capture(&store, &changed).input("p", "i", &at)["availability"]["reason_code"],
            "declaration_mismatch"
        );
        activate(&store, &changed, &config::raw_revision(&store).unwrap()).unwrap();
        assert_eq!(read_runtime(&store).unwrap().channels.len(), 1);
        assert_eq!(
            old_snapshot.input("p", "i", &at)["availability"]["state"],
            "available"
        );
        save_intent(&store, &Config::empty());
        reconcile(&store, &changed, &Config::empty(), &[]).unwrap();
        deactivate(&store).unwrap();
        assert!(read_runtime(&store).unwrap().channels.is_empty());
    }
    #[test]
    fn identical_saved_and_loaded_text_never_transfers_recreated_provider_identity() {
        let (_directory, store, config) = fixture();
        retain_value(&store, &config);
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let old = Snapshot::capture_loaded(&store, &config, None)
            .destination_identity("p", "m")
            .unwrap();
        save_intent(&store, &Config::empty());
        reconcile(&store, &config, &Config::empty(), &[]).unwrap();
        save_intent(&store, &config);
        reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        let saved = Snapshot::capture(&store, &config)
            .destination_identity("p", "m")
            .unwrap();
        assert_ne!(old, saved);
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None)
                .destination_identity("p", "m")
                .unwrap(),
            old
        );
    }
    #[test]
    fn interval_and_priority_edits_preserve_declaration_but_authorization_does_not() {
        let (_directory, store, mut config) = fixture();
        retain_value(&store, &config);
        let original = Snapshot::capture(&store, &config).input_revision("p", "i");
        let mut changed = config.clone();
        changed
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .priority = -99;
        save_intent(&store, &changed);
        reconcile(&store, &config, &changed, &[]).unwrap();
        assert_eq!(
            original,
            Snapshot::capture(&store, &changed).input_revision("p", "i")
        );
        authorization_changed(&store, "p").unwrap();
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(
            Snapshot::capture(&store, &changed).input("p", "i", &at)["availability"]["reason_code"],
            "authorization_mismatch"
        );
        config = changed;
        let mut fetched = config.clone();
        fetched
            .providers
            .get_mut("p")
            .unwrap()
            .inputs
            .get_mut("i")
            .unwrap()
            .source = "provider".into();
        save_intent(&store, &fetched);
        reconcile(&store, &config, &fetched, &[]).unwrap();
        let before = Snapshot::capture(&store, &fetched).input_revision("p", "i");
        let mut scheduled = fetched.clone();
        scheduled
            .providers
            .get_mut("p")
            .unwrap()
            .inputs
            .get_mut("i")
            .unwrap()
            .refresh_interval_seconds = Some(60);
        save_intent(&store, &scheduled);
        reconcile(&store, &fetched, &scheduled, &[]).unwrap();
        assert_eq!(
            before,
            Snapshot::capture(&store, &scheduled).input_revision("p", "i")
        );
    }
}
