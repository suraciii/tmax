//! Versioned, secret-free projection for DSH plugin consumers.
//!
//! The projection describes one declared DSH target's gateway routes and
//! public model capabilities. It never resolves or emits credentials, starts
//! a service, applies native files, or refreshes remote state. Consumers treat
//! `schema_version` as a compatibility gate.

use super::adapters;
use crate::{
    Error, Request, Result,
    config::{self, AgentTarget, Config},
    storage::Store,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::PathBuf};

/// Build the projection for the target selected by TARGET, --home, DSH_HOME,
/// or the sole declared DSH target.
pub async fn projection(store: &Store, request: &Request) -> Result<Value> {
    let file = config::load(store)?;
    let requested = request.operands.first().map(String::as_str);
    let home = request
        .arguments
        .get("home")
        .and_then(|values| values.first())
        .map(String::as_str);
    let (id, target) = select_target(&file.config, requested, home)?;
    build(store, &file, id, target).await
}

fn select_target<'a>(
    config: &'a Config,
    requested: Option<&'a str>,
    home: Option<&str>,
) -> Result<(&'a str, &'a AgentTarget)> {
    if let Some(id) = requested {
        let target = config
            .agents
            .get(id)
            .ok_or_else(|| Error::new("unknown_resource", "DSH target is not configured"))?;
        if target.agent != "dsh" {
            return Err(Error::new(
                "invalid_arguments",
                "The selected target is not a DSH target",
            ));
        }
        return Ok((id, target));
    }
    if let Some(home) = home {
        return match_target_for_home(config, &PathBuf::from(home), true);
    }
    if let Some(home) = std::env::var_os("DSH_HOME").map(PathBuf::from) {
        match match_target_for_home(config, &home, false) {
            Ok(target) => return Ok(target),
            Err(error) if error.code == "invalid_arguments" => return Err(error),
            Err(_) => {}
        }
    }
    let matches: Vec<(&str, &AgentTarget)> = config
        .agents
        .iter()
        .filter(|(_, target)| target.agent == "dsh")
        .map(|(id, target)| (id.as_str(), target))
        .collect();
    match matches.as_slice() {
        [only] => Ok(*only),
        [] => Err(Error::new(
            "unknown_resource",
            "No DSH target is declared; add one or supply --home",
        )),
        _ => Err(Error::new(
            "invalid_arguments",
            "Multiple DSH targets are declared; supply TARGET or --home",
        )),
    }
}

fn match_target_for_home<'a>(
    config: &'a Config,
    home: &PathBuf,
    require_match: bool,
) -> Result<(&'a str, &'a AgentTarget)> {
    let matches: Vec<(&str, &AgentTarget)> = config
        .agents
        .iter()
        .filter(|(_, target)| target.agent == "dsh" && target.scope == *home)
        .map(|(id, target)| (id.as_str(), target))
        .collect();
    match matches.as_slice() {
        [only] => Ok(*only),
        [] if require_match => Err(Error::new(
            "unknown_resource",
            "No declared DSH target matches the requested home",
        )),
        [] => Err(Error::new(
            "unknown_resource",
            "No declared DSH target matches the current DSH home",
        )),
        _ => Err(Error::new(
            "invalid_arguments",
            "Multiple DSH targets match the requested home; supply TARGET",
        )),
    }
}

async fn build(
    store: &Store,
    file: &config::LoadedFile,
    target_id: &str,
    target: &AgentTarget,
) -> Result<Value> {
    let config = &file.config;
    let gateway = config.gateway.as_ref();
    let status = crate::service::status(store).await?;
    let daemon_state = status
        .pointer("/daemon/discovery_state")
        .and_then(Value::as_str)
        .unwrap_or("unidentified");
    let ready = status
        .pointer("/gateway/ready")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let ready_or_unknown = if daemon_state == "compatible" {
        json!(ready)
    } else {
        Value::Null
    };
    let listen = status
        .pointer("/gateway/listen")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| gateway.map(|gateway| gateway.listen.clone()));
    let base_url = listen
        .as_deref()
        .map(|listen| format!("http://{listen}"))
        .unwrap_or_default();
    let snapshot = crate::catalog::read(store, "accepted")?;
    let mut blockers = Vec::new();
    let mut routes = Vec::new();
    let mut route_names = Vec::new();

    if gateway.is_none() {
        blockers.push(json!({
            "code": "gateway_unconfigured",
            "message": "Configure and explicitly start the tmax gateway before this target can use it",
        }));
    }

    for (name, binding) in &target.providers {
        if binding.provider.is_some() {
            continue;
        }
        if binding.models.is_empty() {
            blockers.push(json!({
                "code": "target_incomplete",
                "message": format!("Gateway route \"{name}\" has no selected models"),
            }));
            continue;
        }
        let protocols: BTreeSet<&str> = binding
            .models
            .iter()
            .map(|selection| selection.protocol.as_str())
            .collect();
        let protocol = match (protocols.len(), protocols.iter().next()) {
            (1, Some(protocol)) => *protocol,
            _ => {
                blockers.push(json!({
                    "code": "incompatible",
                    "message": format!("Gateway route \"{name}\" mixes protocols; split it into one route per protocol"),
                }));
                continue;
            }
        };
        route_names.push(name.clone());
        let mut models = Vec::new();
        for selection in &binding.models {
            models.push(
                model_projection(store, config, snapshot.as_ref(), name, protocol, selection)
                    .unwrap_or_else(|error| {
                        json!({
                            "id": selection.id.as_str(),
                            "protocol": selection.protocol.as_str(),
                            "kind": "unknown",
                            "availability": "unknown",
                            "error": {"code": error.code, "message": error.message},
                        })
                    }),
            );
        }
        routes.push(json!({
            "name": name,
            "protocol": protocol,
            "base_url": base_url,
            "credential_ref": gateway.map(|gateway| {
                let _ = gateway;
                adapters::dsh_credential_ref_for(&target.scope, name, protocol)
            }),
            "models": models,
        }));
    }

    if routes.is_empty() && blockers.is_empty() {
        blockers.push(json!({
            "code": "gateway_entry_missing",
            "message": "This DSH target has no gateway provider entry",
        }));
    }

    Ok(json!({
        "schema_version": 1,
        "authority": store.root,
        "tmax_version": env!("CARGO_PKG_VERSION"),
        "configuration_revision": file.revision,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "target": {
            "id": target_id,
            "path": target.scope,
            "default": target.default,
            "routes": route_names,
        },
        "gateway": {
            "configured": gateway.is_some(),
            "ready": ready_or_unknown,
            "listen": listen,
            "daemon_discovery_state": daemon_state,
            "reason_code": if gateway.is_none() {
                Some("gateway_unconfigured")
            } else if daemon_state != "compatible" {
                status.pointer("/daemon/reason_code").and_then(Value::as_str).or(Some("daemon_identity_unavailable"))
            } else if !ready {
                Some("gateway_not_ready")
            } else {
                None
            },
        },
        "routes": routes,
        "blockers": blockers,
    }))
}

fn model_projection(
    store: &Store,
    config: &Config,
    snapshot: Option<&crate::catalog::Snapshot>,
    route: &str,
    protocol: &str,
    selection: &config::ModelSelection,
) -> Result<Value> {
    let _ = (store, route);
    if let Some(alias) = config
        .model_aliases
        .iter()
        .find(|alias| alias.id == selection.id && alias.protocol == protocol && alias.enabled)
    {
        let effective = crate::reasoning::alias_metadata(config, snapshot, alias)?;
        let capabilities = adapters::alias_model_metadata("dsh", protocol, &effective)
            .unwrap_or_else(|error| json!({"state":"unknown","reason_code":error.code}));
        return Ok(json!({
            "id": selection.id.as_str(),
            "protocol": protocol,
            "kind": "alias",
            "continuity": alias.continuity.as_deref(),
            "availability": "unverified",
            "display_name": effective.get("name").and_then(Value::as_str).unwrap_or(selection.id.as_str()),
            "capabilities": capabilities,
            "candidate_count": alias.candidates.len(),
        }));
    }
    let (provider, model) = config::model(config, &selection.id)?;
    let (_, model_id) = config::qualified(&selection.id)?;
    let effective =
        crate::catalog::effective_from_snapshot(snapshot, provider, Some((model_id, model)))?;
    let input_metadata = crate::reasoning::input_metadata(&effective["effective"], model, protocol);
    let capabilities = adapters::model_metadata("dsh", protocol, &input_metadata)
        .unwrap_or_else(|error| json!({"state":"unknown","reason_code":error.code}));
    let availability = if !provider.enabled || !model.enabled {
        "disabled"
    } else if provider.auth.is_none() {
        "authentication_missing"
    } else {
        "unverified"
    };
    Ok(json!({
        "id": selection.id.as_str(),
        "protocol": protocol,
        "kind": "concrete",
        "availability": availability,
        "display_name": effective["effective"].get("name").and_then(Value::as_str).unwrap_or(selection.id.as_str()),
        "capabilities": capabilities,
        "published_protocols": crate::provider_integration::protocols(provider).unwrap_or_default(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgentTarget, Config, NativeProvider};
    use std::collections::BTreeMap;

    fn target(scope: &str) -> AgentTarget {
        AgentTarget {
            agent: "dsh".into(),
            scope: scope.into(),
            providers: BTreeMap::new(),
            default: None,
            roles: BTreeMap::new(),
        }
    }

    #[test]
    fn selects_the_sole_dsh_target_and_rejects_ambiguity() {
        let mut config = Config::empty();
        config.agents.insert("one".into(), target("/tmp/dsh-one"));
        let (id, _) = select_target(&config, None, None).unwrap();
        assert_eq!(id, "one");

        config.agents.insert("two".into(), target("/tmp/dsh-two"));
        let error = select_target(&config, None, None).unwrap_err();
        assert_eq!(error.code, "invalid_arguments");

        let (id, _) = select_target(&config, None, Some("/tmp/dsh-two")).unwrap();
        assert_eq!(id, "two");
    }

    #[test]
    fn rejects_a_non_dsh_target_and_unknown_home() {
        let mut config = Config::empty();
        let mut other = target("/tmp/other");
        other.agent = "pi".into();
        config.agents.insert("other".into(), other);
        let error = select_target(&config, Some("other"), None).unwrap_err();
        assert_eq!(error.code, "invalid_arguments");
        let error = select_target(&config, None, Some("/tmp/missing")).unwrap_err();
        assert_eq!(error.code, "unknown_resource");
    }

    #[tokio::test]
    async fn reports_an_incomplete_gateway_route_without_secrets() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("tmax"));
        let mut config = Config::empty();
        let mut target = target("/tmp/dsh-home");
        target.providers.insert(
            "tmax".into(),
            NativeProvider {
                provider: None,
                gateway: Some(true),
                models: Vec::new(),
            },
        );
        config.agents.insert("dsh-main".into(), target);
        std::fs::create_dir_all(store.root.join("native")).unwrap();
        std::fs::write(
            store.root.join("tmax.toml"),
            "version = 1\n[gateway]\nlisten = \"127.0.0.1:17840\"\nkey_ref = \"secret:test\"\n[agents.dsh-main]\nagent = \"dsh\"\nscope = \"/tmp/dsh-home\"\n[agents.dsh-main.providers.tmax]\ngateway = true\nmodels = []\n",
        )
        .unwrap();
        let file = config::load(&store).unwrap();
        let target = file.config.agents.get("dsh-main").unwrap();
        let value = build(&store, &file, "dsh-main", target).await.unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["gateway"]["configured"], true);
        assert_eq!(value["routes"], json!([]));
        assert_eq!(value["blockers"][0]["code"], "target_incomplete");
        assert!(value.to_string().find("TMAX_").is_none());
    }

    #[test]
    fn credential_reference_is_stable_and_route_scoped() {
        let first =
            adapters::dsh_credential_ref_for(std::path::Path::new("/tmp/dsh"), "tmax", "responses");
        let same =
            adapters::dsh_credential_ref_for(std::path::Path::new("/tmp/dsh"), "tmax", "responses");
        let other =
            adapters::dsh_credential_ref_for(std::path::Path::new("/tmp/dsh"), "tmax", "messages");
        assert_eq!(first, same);
        assert_ne!(first, other);
        assert!(first.starts_with("TMAX_"));
    }
}
