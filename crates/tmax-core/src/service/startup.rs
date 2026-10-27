//! Persisted startup intent and the platform registration policy.
#[cfg(not(windows))]
use super::unix_startup::reconcile;
#[cfg(not(windows))]
pub(super) use super::unix_startup::status as autostart_status;
#[cfg(windows)]
use super::windows_startup::reconcile;
#[cfg(windows)]
pub(super) use super::windows_startup::status as autostart_status;
#[cfg(windows)]
use super::{control::rpc, windows_startup};
use super::{failure, resident::run_owner};
#[cfg(windows)]
use crate::Request;
use crate::{Result, config::Config, storage::Store};
use serde_json::{Value, json};

pub async fn reconcile_autostart(store: &Store, config: &Config, dry_run: bool) -> Result<Value> {
    if dry_run {
        return reconcile(
            store,
            config
                .gateway
                .as_ref()
                .is_some_and(|gateway| gateway.autostart),
            true,
            false,
        )
        .await;
    }
    let persisted = crate::config::load(store)?;
    if serde_json::to_value(&persisted.config).ok() != serde_json::to_value(config).ok() {
        return Err(failure(
            "inputs_changed",
            "Autostart reconciliation accepts only current validated persisted intent",
        ));
    }
    reconcile(
        store,
        persisted
            .config
            .gateway
            .as_ref()
            .is_some_and(|p| p.autostart),
        false,
        false,
    )
    .await
}
pub async fn autostart(store: &Store, enabled: bool, dry_run: bool) -> Result<Value> {
    let loaded = crate::config::load(store)?;
    if loaded.config.gateway.is_none() {
        return Err(failure(
            "gateway_unconfigured",
            "Configure gateway before changing autostart",
        ));
    }
    if !dry_run
        && loaded
            .config
            .gateway
            .as_ref()
            .map(|gateway| gateway.autostart)
            != Some(enabled)
    {
        return Err(failure(
            "inputs_changed",
            "Explicit registration reconciliation requires the requested intent to be already validated and persisted",
        ));
    }
    reconcile(store, enabled, dry_run, true).await
}

/// Invoked only by owned OS registration. Disabled saved intent is authoritative.
pub async fn run_autostart() -> Result<Value> {
    let store = Store::discover()?;
    let loaded = crate::config::load(&store)?;
    if !loaded.config.gateway.as_ref().is_some_and(|p| p.autostart) {
        return Ok(json!({"autostart":"disabled","started":false}));
    }
    #[cfg(unix)]
    {
        run_owner(true).await?;
        Ok(json!({"autostart":"stopped"}))
    }
    #[cfg(windows)]
    {
        windows_startup::detach_console();
        let result = async {
            if let Some(current) = rpc(&store, "status", None, None).await? {
                if current["resident"]["discovery_state"] != "compatible" {
                    return Err(failure("resident_incompatible", "Autostart cannot control an incompatible resident; use its compatible CLI or Web controls"));
                }
                let request = Request {
                    command: vec!["gateway".into(), "start".into()],
                    operands: Vec::new(),
                    arguments: Default::default(),
                    dry_run: false,
                    if_revision: None,
                };
                let started = rpc(&store, "control", Some(request), None).await?
                    .ok_or_else(|| failure("resident_identity_unavailable", "Resident disappeared before autostart control"))?;
                if started["gateway"]["ready"] != true {
                    return Err(failure("autostart_blocked", "Autostart did not establish ready inference; inspect stored key availability and endpoint state"));
                }
                windows_startup::record_launch(&store, None)?;
                return Ok(json!({"autostart":"reused","started":true}));
            }
            run_owner(true).await?;
            Ok(json!({"autostart":"stopped"}))
        }.await;
        if let Err(error) = &result {
            // Preserve the startup failure if safe outcome recording also fails.
            let _ = windows_startup::record_launch(&store, Some(error));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::control::status;
    #[cfg(unix)]
    use super::super::unix_startup::registration_path;
    use super::*;
    #[tokio::test]
    async fn absent_status_and_disabled_unacknowledged_intent_create_nothing() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().into());
        assert_eq!(
            status(&store).await.unwrap()["resident"]["discovery_state"],
            "absent"
        );
        assert!(!store.root.exists());
        let result = reconcile(&store, false, false, false).await.unwrap();
        assert_eq!(result["reconciliation"], "not_requested");
        assert!(!store.root.exists());
        #[cfg(unix)]
        {
            let unit = registration_path(&store);
            crate::storage::atomic_write(&unit, b"foreign startup unit").unwrap();
            let result = reconcile(&store, false, false, false).await.unwrap();
            assert_eq!(result["registration"], "conflict");
            assert_eq!(std::fs::read(&unit).unwrap(), b"foreign startup unit");
            assert!(!store.root.exists());
        }
    }
}
