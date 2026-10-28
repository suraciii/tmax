mod agents;
mod aliases;
mod document;
pub(super) mod fields;
mod models;
mod providers;
mod references;
mod service_settings;
#[cfg(test)]
mod transaction_tests;

use super::arguments::{arg, operand, required};
use super::configuration::{activate_saved, impact};
use crate::{
    Error, Request, Result,
    config::{self, Config},
    storage::Store,
};
use document::{error, put, string};
use serde_json::{Value as Json, json};
use toml::Value;

pub fn is_edit(path: &str) -> bool {
    matches!(
        path,
        "provider add"
            | "provider update"
            | "provider rename"
            | "provider remove"
            | "provider key set"
            | "provider auth set"
            | "provider auth clear"
            | "provider login"
            | "provider logout"
            | "provider input add"
            | "provider input replace"
            | "provider input remove"
            | "model add"
            | "model update"
            | "model rename"
            | "model remove"
            | "model override list replace"
            | "model reasoning add"
            | "model reasoning replace"
            | "model reasoning remove"
            | "model reasoning clear"
            | "model effort-map set"
            | "model effort-map remove"
            | "model effort-map clear"
            | "model schedule add"
            | "model schedule replace"
            | "model schedule remove"
            | "model schedule clear"
            | "model rule add"
            | "model rule replace"
            | "model rule remove"
            | "model rule move"
            | "model rule enable"
            | "model rule disable"
            | "model rule clear"
            | "model alias add"
            | "model alias update"
            | "model alias candidates add"
            | "model alias candidates remove"
            | "model alias rename"
            | "model alias remove"
            | "agent add"
            | "agent update"
            | "agent rename"
            | "agent remove"
            | "agent provider add"
            | "agent provider update"
            | "agent provider remove"
            | "agent model add"
            | "agent model remove"
            | "agent model clear"
            | "agent default set"
            | "agent default clear"
            | "agent role set"
            | "agent role remove"
            | "agent role clear"
            | "network proxy"
            | "network direct"
            | "gateway configure"
            | "gateway key replace"
            | "gateway remove"
            | "gateway autostart enable"
            | "gateway autostart disable"
    )
}
fn mutate(root: &mut Value, r: &Request) -> Result<(Json, Vec<(String, String)>)> {
    let path = r.command.join(" ");
    let mut renames = Vec::new();
    let mut info = json!({"requested":path});
    match path.as_str() {
        "provider add"
        | "provider update"
        | "provider rename"
        | "provider remove"
        | "provider key set"
        | "provider auth set"
        | "provider auth clear"
        | "provider login"
        | "provider logout"
        | "provider input add"
        | "provider input replace"
        | "provider input remove" => providers::mutate(root, r, &path, &mut info, &mut renames)?,
        "model add"
        | "model update"
        | "model rename"
        | "model remove"
        | "model override list replace"
        | "model reasoning add"
        | "model reasoning replace"
        | "model reasoning remove"
        | "model reasoning clear"
        | "model effort-map set"
        | "model effort-map remove"
        | "model effort-map clear"
        | "model schedule add"
        | "model schedule replace"
        | "model schedule remove"
        | "model schedule clear"
        | "model rule add"
        | "model rule replace"
        | "model rule remove"
        | "model rule move"
        | "model rule enable"
        | "model rule disable"
        | "model rule clear" => models::mutate(root, r, &path, &mut renames)?,
        "model alias add"
        | "model alias update"
        | "model alias candidates add"
        | "model alias candidates remove"
        | "model alias rename"
        | "model alias remove" => aliases::mutate(root, r)?,
        "agent add"
        | "agent update"
        | "agent rename"
        | "agent remove"
        | "agent provider add"
        | "agent provider update"
        | "agent provider remove"
        | "agent model add"
        | "agent model remove"
        | "agent model clear"
        | "agent default set"
        | "agent default clear"
        | "agent role set"
        | "agent role remove"
        | "agent role clear" => agents::mutate(root, r, &path, &mut info)?,
        "network proxy"
        | "network direct"
        | "gateway configure"
        | "gateway key replace"
        | "gateway remove"
        | "gateway autostart enable"
        | "gateway autostart disable" => service_settings::mutate(root, r, &path, &mut info)?,
        _ => return Err(error()),
    }
    Ok((info, renames))
}

pub async fn execute(store: &Store, r: &Request) -> Result<Json> {
    let path = store.root.join("tmax.toml");
    let bytes = match std::fs::read(&path) {
        Ok(v) => Some(v),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Err(Error::new(
                "configuration_unavailable",
                "Cannot read personal configuration",
            ));
        }
    };
    let revision = bytes
        .as_ref()
        .map(|b| config::revision(b))
        .unwrap_or_else(|| "absent".into());
    if r.if_revision.as_ref().is_some_and(|rev| rev != &revision) {
        return Err(Error::new(
            "inputs_changed",
            "Saved configuration differs from the requested revision",
        ));
    }
    let text = bytes
        .as_ref()
        .map(|b| {
            std::str::from_utf8(b)
                .map_err(|_| Error::new("invalid_configuration", "Configuration must be UTF-8"))
        })
        .transpose()?
        .unwrap_or("version = 1\n");
    let mut document: toml_edit::DocumentMut = text.parse().map_err(|_| {
        Error::new(
            "invalid_configuration",
            "Configuration cannot be parsed; inspect recovery options",
        )
    })?;
    let migrated_gateway_names = config::normalize_gateway_names(&mut document)?;
    let mut root: Value = toml::from_str(&document.to_string()).map_err(|_| {
        Error::new(
            "invalid_configuration",
            "Configuration cannot be parsed; inspect recovery options",
        )
    })?;
    let before = root.clone();
    let old = config::parse(text).ok();
    if let Some(old) = &old {
        super::references::guard(old, r)?;
    }
    let (mut data, renames) = mutate(&mut root, r)?;
    if migrated_gateway_names {
        data["configuration_migration"] = json!("gateway_names");
    }
    let path = r.command.join(" ");
    let candidate_text = toml::to_string(&root).map_err(|_| error())?;
    let mut candidate = config::parse(&candidate_text)?;
    crate::reasoning::validate_saved(store, &candidate)?;
    if path == "model add" {
        let qualified = operand(r, 0)?;
        let (provider_id, model_id) = config::qualified(qualified)?;
        if let Some(provider) = candidate.providers.get(provider_id)
            && let Some(model) = provider.models.get(model_id)
        {
            let snapshot = crate::catalog::read(store, "accepted")?;
            let projection = crate::catalog::effective_from_snapshot(
                snapshot.as_ref(),
                provider,
                Some((model_id, model)),
            )?;
            let effective = &projection["effective"];
            let mut missing = Vec::new();
            if crate::reasoning::effort_values(effective).is_none()
                && effective.get("reasoning").and_then(Json::as_bool) != Some(false)
            {
                missing.push("reasoning_efforts");
            }
            if effective
                .pointer("/limit/context")
                .is_none_or(Json::is_null)
            {
                missing.push("limit.context");
            }
            if !missing.is_empty() {
                data["fact_gap"] = json!({
                    "code":"model_fact_gap",
                    "model":qualified,
                    "missing":missing,
                    "message":"No catalog or declared fact supplies the missing fields; native entries for this model stay bare for them. Declare model overrides (tmax model update / tmax model reasoning) or accept the degraded entry.",
                });
            }
        }
    }
    if path == "gateway remove" {
        let state = crate::service::status(store).await?;
        if !matches!(
            state["daemon"]["discovery_state"].as_str(),
            Some("absent" | "compatible")
        ) {
            return Err(Error::new(
                "daemon_identity_unavailable",
                "Establish daemon identity before removing gateway settings; unknown runtime state is not a stopped listener",
            ));
        }
        if state["daemon"]["inference"]["listen"].is_string() {
            return Err(Error::new(
                "active_requests",
                "Stop inference explicitly before removing gateway settings",
            ));
        }
    }
    if r.dry_run {
        let mut impact = impact(store, &candidate, &revision, old.as_ref()).await?;
        impact["requested"] = data;
        if path == "provider login" || path == "provider logout" {
            let source = if path == "provider login" {
                candidate.providers[operand(r, 0)?].auth.as_ref()
            } else {
                old.as_ref()
                    .and_then(|c| c.providers.get(operand(r, 0).ok()?))
                    .and_then(|p| p.auth.as_ref())
            };
            if let Some(config::Auth::Login { via, .. }) = source {
                impact["credential"] = crate::agent_integration::login(
                    store,
                    old.as_ref().unwrap_or(&candidate),
                    operand(r, 0)?,
                    via,
                    true,
                )
                .await?;
            }
        }
        return Ok(impact);
    }
    config::guard(store, &revision)?;
    let needs_key = (path == "gateway configure" && data["provision_gateway_key"] == true)
        || path == "gateway key replace"
        || (path == "provider key set" && arg(r, "key-ref").is_none());
    let mut created = None;
    if needs_key {
        let value: zeroize::Zeroizing<Vec<u8>> = if path == "provider key set" {
            zeroize::Zeroizing::new(crate::secrets::read_protected_input(r, "key")?)
        } else {
            use rand::RngCore;
            let mut b = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut b);
            use base64::Engine;
            zeroize::Zeroizing::new(
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(b)
                    .into_bytes(),
            )
        };
        let owner = if path == "provider key set" {
            provider_owner(store, old.as_ref().unwrap_or(&candidate), operand(r, 0)?)?
        } else {
            "gateway".into()
        };
        let reference = crate::secrets::put(store, &owner, &value)?;
        if path == "provider key set" {
            put(
                &mut root,
                &["providers", operand(r, 0)?, "auth", "key_ref"],
                string(&reference),
            )?;
        } else {
            let previous = old
                .as_ref()
                .and_then(|c| c.gateway.as_ref())
                .map(|p| p.key_ref.clone());
            put(&mut root, &["gateway", "key_ref"], string(&reference))?;
            data["gateway_key"] = json!({"action":if path=="gateway key replace"{"replaced"}else{"provisioned"},"reference":reference,"previous_reference":previous});
            data["unmanaged_clients"] = json!("unknown");
        }
        created = Some(reference);
    }
    if path == "provider key set" && arg(r, "key-ref").is_some() {
        let inventory = crate::secrets::inventory(store)?;
        let reference = required(r, "key-ref")?;
        let id = operand(r, 0)?;
        let identity =
            crate::inputs::provider_identity(store, old.as_ref().unwrap_or(&candidate), id)?
                .ok_or_else(error)?;
        let valid = inventory["entries"].as_array().is_some_and(|a| {
            a.iter().any(|e| {
                e["reference"] == reference
                    && e["owner"]["kind"] == "provider"
                    && e["owner"]["identity"] == identity
                    && e["metadata_state"] == "known"
            })
        });
        if !valid {
            return Err(Error::new(
                "credential_unavailable",
                "Select an existing protected reference owned by this provider incarnation",
            ));
        }
    }
    if path == "provider login" || path == "provider logout" {
        if arg(r, "non-interactive").is_some() {
            return Err(Error::new(
                "interactive_required",
                "Native sign-in requires an explicit interactive owner operation",
            ));
        }
        let original = old.as_ref().unwrap_or(&candidate);
        let provider = operand(r, 0)?;
        let auth = if path == "provider login" {
            candidate.providers[provider].auth.as_ref()
        } else {
            original.providers[provider].auth.as_ref()
        };
        let Some(config::Auth::Login { via, .. }) = auth else {
            return Err(error());
        };
        let effect = if path == "provider login" {
            crate::agent_integration::login(store, original, provider, via, false).await?
        } else {
            crate::agent_integration::logout(store, original, provider, via, false).await?
        };
        data["authorization_invalidation"] = effect["authorization_invalidation"].clone();
        if let Some(code) = effect
            .get("_code")
            .and_then(Json::as_str)
            .filter(|code| *code != "awaiting_native_reload")
        {
            data["_code"] = json!(code);
            data["_status"] = json!("blocked");
            data["credential"] = effect;
            data["save"] = json!("not_requested");
            return Ok(data);
        }
        if effect.get("_code").and_then(Json::as_str) == Some("awaiting_native_reload") {
            data["_code"] = json!("awaiting_native_reload");
        }
        data["credential"] = effect.clone();
        if path == "provider login" {
            let Some(session) = effect.get("session_ref").and_then(Json::as_str) else {
                data["_code"] = json!("recovery_required");
                data["save"] = json!("not_requested");
                return Ok(data);
            };
            put(
                &mut root,
                &["providers", provider, "auth", "session_ref"],
                string(session),
            )?;
        }
    }
    candidate = config::parse(&toml::to_string(&root).map_err(|_| error())?)?;
    document::move_renamed(&mut document, &renames)?;
    document::sync_table(document.as_table_mut(), Some(&before), &root)?;
    let saved = match config::save(store, &revision, &document) {
        Ok(revision) => revision,
        Err(error) => {
            if created.is_none() && data.get("credential").is_none() {
                return Err(error);
            }
            data["_code"] = json!("saved_not_active");
            data["save"] = json!("failed");
            data["save_error"] =
                json!({"code":error.code,"message":error.message,"field":error.field});
            data["retained_secret_reference"] = json!(created);
            data["native_application"] = json!("not_requested");
            return Ok(data);
        }
    };
    data["file_revision"] = json!(saved);
    data["save"] = json!("saved");
    let reconciliation = (|| -> Result<()> {
        crate::routing::reconcile(
            store,
            old.as_ref().unwrap_or(&Config::empty()),
            &candidate,
            &renames,
        )?;
        for (old, new) in &renames {
            if !old.contains('/') {
                crate::secrets::rename_owner(store, "provider", old, new)?;
            }
        }
        if let Some(pair) = data["target_rename"].as_array()
            && let (Some(old), Some(new)) = (
                pair.first().and_then(Json::as_str),
                pair.get(1).and_then(Json::as_str),
            )
        {
            crate::agent_integration::rename_target(store, old, new)?;
        }
        if path == "agent remove" {
            crate::agent_integration::remove_target(store, operand(r, 0)?)?;
        }
        if path == "provider remove" {
            crate::secrets::detach_owner(store, "provider", operand(r, 0)?)?;
        }
        Ok(())
    })();
    if let Err(e) = reconciliation {
        data["_code"] = json!("saved_not_active");
        data["runtime"] = json!("not_loaded");
        data["reconciliation"] = json!({"state":"failed","code":e.code,"message":e.message});
        return Ok(data);
    }
    data["file_revision"] = json!(saved);
    data["save"] = json!(if root == before && !migrated_gateway_names {
        "unchanged"
    } else {
        "saved"
    });
    data["native_application"] = json!("not_requested");
    if matches!(
        path.as_str(),
        "provider login" | "provider logout" | "gateway key replace"
    ) {
        // These operations request native authorization effects or key revocation,
        // not only a desired-state edit. Their activation failure remains partial.
        match crate::service::activate(store, &candidate, &saved).await {
            Ok(activation) => {
                data["runtime"] = json!(if activation["activation"] == "not_running" {
                    "not_running"
                } else if activation["activation"] == "active" {
                    "loaded"
                } else {
                    "not_loaded"
                });
                data["activation"] = activation;
            }
            Err(error) => {
                data["_code"] = json!("saved_not_active");
                data["runtime"] = json!("not_loaded");
                data["activation"] = json!({"activation":"failed","error":error});
            }
        }
    } else {
        activate_saved(store, &candidate, &saved, &mut data).await;
    }
    let autostart = if path.starts_with("gateway autostart ") {
        crate::service::autostart(store, path.ends_with("enable"), false).await
    } else {
        crate::service::reconcile_autostart(store, &candidate, false).await
    };
    match autostart {
        Ok(v) => data["autostart"] = v,
        Err(e) => {
            data["_code"] = json!("saved_not_active");
            data["autostart"] = json!({"state":"failed","code":e.code,"message":e.message});
        }
    }
    Ok(data)
}

fn provider_owner(store: &Store, config: &Config, id: &str) -> Result<String> {
    let identity=crate::inputs::provider_identity(store,config,id)?.ok_or_else(||Error::new("credential_unavailable","Provider incarnation is unavailable; explicitly reload the valid authority before credential provisioning"))?;
    Ok(format!("provider:{identity}:{id}"))
}
