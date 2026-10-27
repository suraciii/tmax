//! Owned systemd user registration and acknowledgement metadata.
use super::failure;
use crate::{Result, storage::Store};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::PathBuf, process::Stdio, time::Duration};

#[derive(Default, Serialize, Deserialize)]
struct Startup {
    acknowledged: Option<bool>,
    last_outcome: Option<String>,
    last_attempt: Option<chrono::DateTime<chrono::Utc>>,
}
pub(super) fn registration_path(store: &Store) -> PathBuf {
    store.home.join(".config/systemd/user/tmax.service")
}
fn registration(store: &Store) -> Result<Vec<u8>> {
    let executable = std::env::current_exe().map_err(|_| {
        failure(
            "autostart_unavailable",
            "Cannot locate executable for startup registration",
        )
    })?;
    let path = executable.to_str().ok_or_else(|| {
        failure(
            "autostart_unavailable",
            "Executable path cannot be represented for systemd",
        )
    })?;
    if path.contains(['\n', '\r', '%', '"', '\\']) {
        return Err(failure(
            "autostart_unavailable",
            "Executable path cannot be safely registered with systemd",
        ));
    }
    Ok(format!("# tmax-owned authority={}\n[Unit]\nDescription=tmax personal model gateway\n[Service]\nType=simple\nExecStart=\"{}\" __autostart\n[Install]\nWantedBy=default.target\n",store.root.display(),path).into_bytes())
}
pub(super) fn status(store: &Store) -> Result<Value> {
    let state: Startup = store.read_json("autostart.json")?;
    let expected = registration(store)?;
    let unit = registration_path(store);
    let actual = crate::storage::read_optional(&unit)?;
    let owned = actual.as_ref().is_some_and(|value| {
        value.starts_with(format!("# tmax-owned authority={}\n", store.root.display()).as_bytes())
    });
    let link = unit
        .parent()
        .expect("registration parent")
        .join("default.target.wants/tmax.service");
    let link_state = match std::fs::symlink_metadata(&link) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "absent",
        Ok(metadata) if metadata.file_type().is_symlink() => {
            if std::fs::canonicalize(&link)
                .ok()
                .zip(std::fs::canonicalize(&unit).ok())
                .is_some_and(|(target, unit)| target == unit)
            {
                "owned"
            } else {
                "conflict"
            }
        }
        Ok(_) => "conflict",
        Err(_) => "unavailable",
    };
    let desired = crate::config::load(store)
        .ok()
        .and_then(|config| config.config.gateway.map(|gateway| gateway.autostart));
    let unit_matches = actual.as_ref() == Some(&expected);
    let registered = owned && unit_matches && link_state == "owned";
    Ok(
        json!({"saved_intent":desired,"acknowledged_intent":state.acknowledged,"registration":if actual.is_none()&&link_state=="absent"{"absent"}else if !owned||matches!(link_state,"conflict"|"unavailable"){"conflict"}else if registered{"present"}else{"drifted"},"unit_matches":unit_matches,"login_link":link_state,"login_enabled":if link_state=="unavailable"{Value::Null}else{json!(registered)},"last_outcome":state.last_outcome,"last_attempt":state.last_attempt,"repair":"gateway autostart enable|disable","reconciliation":"not_requested"}),
    )
}
pub(super) async fn reconcile(
    store: &Store,
    enabled: bool,
    dry_run: bool,
    force: bool,
) -> Result<Value> {
    let mut state: Startup = store.read_json("autostart.json")?;
    let unchanged =
        |s: &Startup| s.acknowledged == Some(enabled) || (s.acknowledged.is_none() && !enabled);
    if !force && unchanged(&state) {
        return status(store);
    }
    if dry_run {
        return Ok(json!({"dry_run":true,"enabled":enabled,"registration":"planned"}));
    }
    let _lock = store.lock("autostart")?;
    state = store.read_json("autostart.json")?;
    if !force && unchanged(&state) {
        return status(store);
    }
    let target = registration_path(store);
    let expected = registration(store)?;
    let existing = crate::storage::read_optional(&target)?;
    let marker = format!("# tmax-owned authority={}\n", store.root.display());
    state.acknowledged = Some(enabled);
    state.last_attempt = Some(chrono::Utc::now());
    state.last_outcome = Some("pending".into());
    store.write_json("autostart.json", &state)?;
    let mut effects = Vec::<&str>::new();
    let result:Result<()>=async {
        if existing.as_ref().is_some_and(|v|!v.starts_with(marker.as_bytes())){return Err(failure("autostart_conflict","Startup registration is not owned by this authority; unrelated entries were not changed"));}
        if !enabled&&existing.is_none()&&std::fs::symlink_metadata(target.parent().expect("registration parent").join("default.target.wants/tmax.service")).is_ok(){return Err(failure("autostart_conflict","Login registration exists without this authority's owned unit; ownership must be repaired before removal"));}
        if enabled {
            crate::storage::atomic_write(&target,&expected)?;effects.push("owned_unit_written");
            startup_command(store,&["daemon-reload"]).await?;effects.push("manager_reloaded");
            startup_command(store,&["enable","tmax.service"]).await?;effects.push("login_start_enabled");
        }else if existing.is_some(){
            // Disable while the ownership-proven unit still exists; never remove it
            // first and then ask the manager to guess what should be disabled.
            startup_command(store,&["disable","tmax.service"]).await?;effects.push("login_start_disabled");
            std::fs::remove_file(&target).map_err(|_|failure("autostart_failed","Cannot remove this authority's startup registration"))?;effects.push("owned_unit_removed");
            startup_command(store,&["daemon-reload"]).await?;effects.push("manager_reloaded");
        }
        Ok(())
    }.await;
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
        json!({"_code":if result.is_err()||metadata.is_err(){json!("saved_not_active")}else{Value::Null},"reconciliation":if result.is_ok(){"reconciled"}else{"failed"},"acknowledged_intent":enabled,"completed_effects":effects,"error":result.err(),"outcome_recording_error":metadata.err(),"status":status(store).ok(),"repair":"gateway autostart enable|disable"}),
    )
}
async fn startup_command(store: &Store, args: &[&str]) -> Result<()> {
    let mut command = tokio::process::Command::new("systemctl");
    command
        .arg("--user")
        .args(args)
        .env("XDG_CONFIG_HOME", store.home.join(".config"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = tokio::time::timeout(Duration::from_secs(10), command.kill_on_drop(true).status())
        .await
        .map_err(|_| {
            failure(
                "autostart_failed",
                "User startup reconciliation deadline expired",
            )
        })?
        .map_err(|_| {
            failure(
                "autostart_unavailable",
                "User systemd startup manager is unavailable",
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(failure(
            "autostart_failed",
            "User startup manager did not accept the requested registration operation",
        ))
    }
}
