//! Saved login intent is independent of current native process supervision.
use super::{failure, supervisor};
use crate::{Result, config::Config, storage::Store};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Default, Serialize, Deserialize)]
struct Startup {
    acknowledged: Option<bool>,
    last_outcome: Option<String>,
    last_attempt: Option<chrono::DateTime<chrono::Utc>>,
    // Retain legacy ownership evidence until the platform retires its old entry.
    #[serde(flatten)]
    retained: std::collections::BTreeMap<String, Value>,
}

pub(super) fn autostart_status(store: &Store) -> Result<Value> {
    let state: Startup = store.read_json("autostart.json")?;
    let mut value = supervisor::login_status(store)?;
    value["saved_intent"] = json!(
        crate::config::load(store)
            .ok()
            .and_then(|v| v.config.gateway.map(|v| v.autostart))
    );
    value["acknowledged_intent"] = json!(state.acknowledged);
    value["last_outcome"] = json!(state.last_outcome);
    value["last_attempt"] = json!(state.last_attempt);
    value["reconciliation"] = json!("not_requested");
    value["repair"] = json!("gateway autostart enable|disable");
    Ok(value)
}

pub async fn reconcile_autostart(store: &Store, config: &Config, dry_run: bool) -> Result<Value> {
    let enabled = config.gateway.as_ref().is_some_and(|v| v.autostart);
    if !dry_run {
        let persisted = crate::config::load(store)?;
        if serde_json::to_value(&persisted.config).ok() != serde_json::to_value(config).ok() {
            return Err(failure(
                "inputs_changed",
                "Autostart reconciliation accepts only current validated persisted intent",
            ));
        }
    }
    reconcile(store, enabled, dry_run, false).await
}
pub async fn autostart(store: &Store, enabled: bool, dry_run: bool) -> Result<Value> {
    let file = crate::config::load(store)?;
    let gateway = file.config.gateway.as_ref().ok_or_else(|| {
        failure(
            "gateway_unconfigured",
            "Configure gateway before changing autostart",
        )
    })?;
    if !dry_run && gateway.autostart != enabled {
        return Err(failure(
            "inputs_changed",
            "Explicit registration reconciliation requires validated persisted intent",
        ));
    }
    reconcile(store, enabled, dry_run, true).await
}
async fn reconcile(store: &Store, enabled: bool, dry_run: bool, force: bool) -> Result<Value> {
    let state: Startup = store.read_json("autostart.json")?;
    let unchanged = |state: &Startup| {
        state.acknowledged == Some(enabled) || state.acknowledged.is_none() && !enabled
    };
    if !force && unchanged(&state) {
        return autostart_status(store);
    }
    if dry_run {
        let prerequisite = supervisor::probe(store).await.err();
        return Ok(
            json!({"dry_run":true,"enabled":enabled,"registration":"planned","supervision":"unchanged","starts_inference":false,"blockers":prerequisite.into_iter().collect::<Vec<_>>()}),
        );
    }
    let _guard = supervisor::lifecycle_lock(store).await?;
    let mut state: Startup = store.read_json("autostart.json")?;
    if !force && unchanged(&state) {
        return autostart_status(store);
    }
    state.acknowledged = Some(enabled);
    state.last_attempt = Some(chrono::Utc::now());
    state.last_outcome = Some("pending".into());
    store.write_json("autostart.json", &state)?;
    let result = async {
        let manager = supervisor::probe(store).await?;
        supervisor::initialize(store, &manager)?;
        supervisor::login(store, enabled).await
    }
    .await;
    state.last_outcome = Some(
        if result.is_ok() {
            "reconciled"
        } else {
            "failed"
        }
        .into(),
    );
    let metadata = store.write_json("autostart.json", &state);
    Ok(
        json!({"_code":if result.is_err()||metadata.is_err(){json!("saved_not_active")}else{Value::Null},"reconciliation":if result.is_ok(){"reconciled"}else{"failed"},"acknowledged_intent":enabled,"error":result.err(),"outcome_recording_error":metadata.err(),"status":autostart_status(store).ok(),"repair":"gateway autostart enable|disable","current_listeners":"unchanged"}),
    )
}
