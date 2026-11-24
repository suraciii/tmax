//! Desired target resolution and safe native projection/comparison.
#[cfg(test)]
use super::adapters::desired_fields;
use super::{
    adapters::{self, Entry, File, inline_credential},
    authentication::auth_namespace_present,
    compatibility,
    discovery::safe_files,
    document::{self, Format},
    installation::{schema_blocker, version},
    recovery::{INDEX, Index, Record, field_drift, field_hash, owner, record_for, record_view},
    scope::{dsh_preflight, read_native, scope_evidence, target, unavailable},
};
use crate::{
    Error, Result,
    config::{self, AgentTarget, Auth, Config},
    storage::Store,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(crate) fn reasoning_projection(effective: &Value, protocols: &[String]) -> Value {
    let view = |agent: &str, protocol: &str| {
        adapters::projection_view(agent, protocol, effective).unwrap_or_else(|error| {
            json!({"state":"blocked","reason_code":error.code,"message":error.message,"fields":{},"omitted":[]})
        })
    };
    Value::Object(
        adapters::AGENTS
            .into_iter()
            .map(|agent| {
                let mut projection = view(
                    agent,
                    protocols
                        .first()
                        .map(String::as_str)
                        .unwrap_or("chat-completions"),
                );
                if agent == "opencode" {
                    projection["protocols"] = Value::Object(
                        protocols
                            .iter()
                            .map(|protocol| (protocol.clone(), view(agent, protocol)))
                            .collect(),
                    );
                }
                (agent.to_owned(), projection)
            })
            .collect(),
    )
}
/// Compare the applied delivery facts of one record with the current facts.
fn applied_delivery(
    config: &Config,
    target: &AgentTarget,
    record: Option<&Record>,
    ready: Option<bool>,
    native_conflict: bool,
) -> Value {
    let Some(current) = config::gateway_delivery_for(config, &config.relayed_route_pairs(target))
    else {
        return json!({"state": "not_configured"});
    };
    let Some(applied) = record.and_then(|record| record.delivery.as_ref()) else {
        return json!({
            "state": if record.is_some() { "unknown" } else { "not_applied" },
            "revision": current.revision,
            "reason_code": if record.is_some() { "delivery_not_recorded" } else { "never_applied" },
        });
    };
    // A record that did not finish its native writes cannot be trusted as
    // applied delivery even though it carries the facts captured before writing.
    if record.is_some_and(|record| record.state != "applied_reload_pending") {
        return json!({
            "state": "blocked",
            "revision": current.revision,
            "applied_revision": applied.revision,
            "reasons": ["apply_incomplete"],
            "routes": {"removed": []},
        });
    }
    let applied_facts = config::GatewayDelivery {
        revision: applied.revision.clone(),
        listen: applied.listen.clone(),
        key_ref: applied.key_ref.clone(),
        routes: applied.routes.clone(),
    };
    let mut reasons = current.drift(&applied_facts);
    let drifted = !reasons.is_empty();
    if !config.missing_relayed_routes(target).is_empty() {
        reasons.push("entry_references_missing_route");
    }
    if native_conflict {
        reasons.push("native_edit_conflict");
    }
    if ready == Some(false) {
        reasons.push("gateway_not_ready");
    }
    let state = if drifted {
        "stale"
    } else if !reasons.is_empty() {
        "blocked"
    } else {
        "current"
    };
    json!({
        "state": state,
        "revision": current.revision,
        "applied_revision": applied.revision,
        "reasons": reasons,
        "routes": {
            "removed": applied.routes.iter().filter(|route| !current.routes.contains(route)).collect::<Vec<_>>(),
        },
    })
}

/// Per-target delivery facts for the configured gateway.
pub async fn delivery(store: &Store, config: &Config) -> Result<Value> {
    let Some(provider) = config::gateway_delivery(config) else {
        return Ok(json!({"state": "not_configured"}));
    };
    let index: Index = store.read_json(INDEX)?;
    let readiness = crate::service::status(store)
        .await
        .ok()
        .and_then(|status| status["gateway"]["ready"].as_bool());
    let mut targets = Vec::new();
    for (id, target) in &config.agents {
        let relayed = target
            .providers
            .iter()
            .filter(|(_, entry)| entry.is_relayed(config))
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if relayed.is_empty() {
            continue;
        }
        let (path, _) = scope_evidence(store, &target.scope)?;
        let identity = owner(store, id)?;
        let record = record_for(&index, identity.as_deref(), &path);
        let native_conflict = record
            .and_then(|record| field_drift(record, false).ok())
            .is_some_and(|drift| !drift.is_empty());
        let applied = applied_delivery(config, target, record, readiness, native_conflict);
        for entry in relayed {
            targets.push(json!({
                "target": id,
                "entry": entry,
                "applied_delivery": applied,
            }));
        }
    }
    Ok(json!({
        "state": "configured",
        "revision": provider.revision,
        "listen": provider.listen,
        "routes": provider.routes,
        "targets": targets,
    }))
}

/// The dimension one blocker code blocks, and the action that clears it.
/// Ordered as a reader meets them: the scope exists, the client is installed,
/// its plugin is present, the gateway answers, then the selections themselves.
fn blocker_dimension(code: &str) -> (&'static str, Option<&'static str>) {
    match code {
        "native_scope_unavailable" => ("scope", None),
        "native_schema_incompatible" => (
            "native_installation",
            Some("install a client version whose native schema tmax projects"),
        ),
        "native_version_unqualified" => (
            "native_installation",
            Some("update this client to the qualified version or newer"),
        ),
        "native_not_installed" => ("native_installation", Some("install the client")),
        "native_version_probe_failed" | "native_version_unknown" => (
            "native_installation",
            Some("run the client once so its version resolves"),
        ),
        "dsh_plugin_not_installed" => (
            "plugin",
            Some("install the tmax bundle into a profile of this DSH home"),
        ),
        "gateway_not_configured" => ("gateway", Some("tmax gateway configure")),
        "gateway_not_ready" => ("gateway", Some("tmax gateway start")),
        "credential_unavailable" => (
            "credential",
            Some("tmax provider key set PROVIDER, or sign the client in"),
        ),
        "integration_unqualified"
        | "unsupported_agent"
        | "compatibility_unavailable"
        | "incompatible"
        | "model_unavailable" => (
            "compatibility",
            Some("choose a protocol, model, or client this native handoff supports"),
        ),
        "target_incomplete" => ("selection", Some("select providers and models")),
        "native_conflict" | "unowned_settings" | "referenced_resource" => (
            "native_settings",
            Some("reconcile the reported native fields, then reapply"),
        ),
        _ => ("delivery", Some("inspect the blocker message")),
    }
}

/// Every dimension a target's current blockers touch, plus the ones that are
/// simply fine. `ready` is true only when nothing at all blocks.
fn readiness_matrix(blockers: &[Value]) -> Value {
    const DIMENSIONS: [&str; 7] = [
        "scope",
        "native_installation",
        "plugin",
        "gateway",
        "credential",
        "compatibility",
        "selection",
    ];
    let mut rows: Vec<(String, Value)> = DIMENSIONS
        .iter()
        .map(|dimension| ((*dimension).to_owned(), json!({"state":"ready"})))
        .collect();
    for blocker in blockers {
        let code = blocker.get("code").and_then(Value::as_str).unwrap_or("");
        let (dimension, next_action) = blocker_dimension(code);
        let row = json!({
            "state": "blocked",
            "code": code,
            "message": blocker.get("message").cloned().unwrap_or(Value::Null),
            "next_action": next_action,
        });
        match rows.iter_mut().find(|(name, _)| name == dimension) {
            Some((_, existing)) if existing["state"] == "blocked" => {
                // Two blockers on one dimension keep the first; the blocker
                // list beside this matrix carries them all.
            }
            Some((_, existing)) => *existing = row,
            None => rows.push((dimension.to_owned(), row)),
        }
    }
    let ready = rows.iter().all(|(_, row)| row["state"] == "ready");
    json!({
        "ready": ready,
        "blocked": rows
            .iter()
            .filter(|(_, row)| row["state"] == "blocked")
            .map(|(dimension, row)| {
                json!({"dimension":dimension,"code":row["code"],"message":row["message"],"next_action":row["next_action"]})
            })
            .collect::<Vec<_>>(),
        "dimensions": rows
            .into_iter()
            .map(|(dimension, row)| json!({"dimension":dimension,"state":row["state"]}))
            .collect::<Vec<_>>(),
    })
}

pub async fn inspect(store: &Store, config: &Config, id: &str) -> Result<Value> {
    let target = target(config, id)?;
    let (path, scope_reason) = scope_evidence(store, &target.scope)?;
    let available = scope_reason == "available";
    let index: Index = store.read_json(INDEX)?;
    let identity = owner(store, id)?;
    let record = record_for(&index, identity.as_deref(), &path);
    let relayed_entry_present = target
        .providers
        .values()
        .any(|entry| entry.is_relayed(config));
    let readiness = if relayed_entry_present && config.gateway.is_some() {
        crate::service::status(store)
            .await
            .ok()
            .and_then(|status| status["gateway"]["ready"].as_bool())
    } else {
        None
    };
    let mut native_conflict = false;
    let mut blockers = Vec::new();
    if !available {
        blockers.push(json!({"code":"native_scope_unavailable","path":path,"reason_code":scope_reason,"message":"Select an existing native configuration directory or explicitly create the intended directory before applying"}));
    }
    if (target.agent == "codex" && target.default.is_none())
        || target.providers.is_empty()
        || target.providers.values().any(|p| p.models.is_empty())
    {
        blockers.push(json!({"code":"target_incomplete","message":"Select providers and models before applying; Codex also requires a default model"}));
    }
    let mut pending = None;
    let mut native_projection = None;
    let mut fact_gaps = Vec::new();
    if available {
        let plan = match entries(store, config, id, target, &path, true).await {
            Ok((entries, references)) => {
                fact_gaps = fact_gap_warnings(target, &entries);
                native_projection = Some(projection_view(target, &entries));
                adapters::build(target, &path, &entries)
                    .and_then(|files| pending_native_changes(target, &files, &references, record))
            }
            Err(error) => Err(error),
        };
        match plan {
            Ok(changes) => pending = Some(changes),
            Err(error) => blockers
                .push(json!({"code":error.code,"field":error.field,"message":error.message})),
        }
        if let Some(record) = record {
            match field_drift(record, false) {
                Ok(drift) => {
                    native_conflict = !drift.is_empty();
                    blockers.extend(drift)
                }
                Err(error) => blockers.push(json!({"code":error.code,"message":error.message})),
            }
        }
    }
    let installed = if available {
        version(&target.agent, &path).await
    } else {
        json!({"state":"not_checked","version":null,"reason_code":"native_scope_unavailable"})
    };
    if available && let Some(blocker) = schema_blocker(&target.agent, &installed) {
        blockers.push(blocker);
    }
    let delivery = applied_delivery(config, target, record, readiness, native_conflict);
    // One row per thing a reader can act on, so a blocked target names every
    // obstacle at once instead of whichever check ran first.
    let readiness_matrix = readiness_matrix(&blockers);
    let applied_state = match &pending {
        None => "unknown",
        Some(_) if record.is_none() => "not_applied",
        Some(changes) if !changes.is_empty() => "pending_native_changes",
        Some(_) => record.map(|r| r.state.as_str()).unwrap_or("not_applied"),
    };
    Ok(
        json!({"target":id,"agent":target.agent,"path":path,"scope_state":if available{"available"}else{"unavailable"},"scope_reason_code":if available{None}else{Some(scope_reason)},"configuration_revision":config::raw_revision(store).ok(),"providers":target.providers.iter().map(|(name,p)|{let relayed=p.is_relayed(config);let base_url=config.gateway.as_ref().filter(|_|relayed).map(|gateway|format!("http://{}",gateway.listen));let protocols=p.models.iter().map(|selection|selection.protocol.as_str()).collect::<BTreeSet<_>>();let credential_ref=if relayed&&target.agent=="dsh"&&protocols.len()==1{protocols.iter().next().map(|protocol|adapters::dsh_credential_ref_for(&target.scope,name,protocol))}else{None};json!({"name":name,"source":{"provider":p.provider},"relayed":relayed,"base_url":base_url,"credential_ref":credential_ref,"applied_delivery":if relayed{delivery.clone()}else{Value::Null},"models":p.models})}).collect::<Vec<_>>(),"default":target.default,"roles":target.roles,"supported_roles":adapters::supported_roles(&target.agent),"native_projection":native_projection,"fact_gaps":fact_gaps,"installation":installed,"precedence":adapters::precedence(&target.agent),"files":if available{safe_files(&target.agent,&path)}else{Vec::new()},"credential_handoff":{"owner":target.agent,"plaintext_in_native_configuration":target.agent == "hermes","plaintext_private_native_credential_file":target.agent != "hermes","reference":if target.agent == "hermes" {"native api_key in current-user-restricted config.yaml"} else {"native command/file helper"},"secret_reads":false,"native_auth_import":false},"applied_state":applied_state,"native_default":super::native_default::view(&target.agent,&path),"pending_native_changes":pending,"blockers":blockers,"readiness":readiness_matrix,"recovery":record.map(|r|record_view(store,r))}),
    )
}

pub(super) fn projection_view(target: &AgentTarget, entries: &[Entry]) -> Vec<Value> {
    entries
        .iter()
        .map(|entry| {
            let source = target
                .providers
                .get(&entry.name)
                .map(|binding| json!({"provider":binding.provider}))
                .unwrap_or(Value::Null);
            json!({
                "name":entry.name,
                "source":source,
                "models":entry.models.iter().map(|id|{
                    let projection=entry.model_projection.get(id).cloned().unwrap_or_else(||json!({"state":"unavailable","reason_code":"native_projection_unavailable","fields":{},"omitted":[]}));
                    let efforts=adapters::representable_efforts(&target.agent,&projection).unwrap_or_default();
                    json!({
                        "id":id,
                        "protocol":entry.model_routes.get(id).map(|(protocol,_)|protocol.as_str()).unwrap_or(entry.protocol.as_str()),
                        "capabilities":entry.model_metadata.get(id).cloned().unwrap_or(Value::Null),
                        "efforts":efforts,
                        "projection":projection
                    })
                }).collect::<Vec<_>>()
            })
        })
        .collect()
}

/// Visible model-fact gaps for one target: models whose native entry will
/// stay bare because no catalog or declared fact supplies them.
pub(super) fn fact_gap_warnings(target: &AgentTarget, entries: &[Entry]) -> Vec<Value> {
    let completion = adapters::native_completion(&target.agent);
    let mut warnings = Vec::new();
    for entry in entries {
        for (id, projection) in &entry.model_projection {
            let mut missing = Vec::new();
            if projection.get("reason_code").and_then(Value::as_str)
                == Some("reasoning_options_unavailable")
            {
                missing.push("reasoning_efforts");
            }
            if projection
                .pointer("/limits/fields/contextWindow")
                .is_none_or(Value::is_null)
            {
                missing.push("limit.context");
            }
            if !missing.is_empty() {
                warnings.push(json!({
                    "code":"model_fact_gap",
                    "entry":entry.name,
                    "model":id,
                    "missing":missing,
                    "agent_self_completion":completion,
                    "message":"No catalog or declared fact supplies the missing fields; the native entry stays bare for them. Declare model overrides or accept the degraded entry.",
                }));
            }
        }
    }
    warnings
}

/// Resolve one relayed entry. The gateway is on the request path, so the
/// destination and credential come from the derived gateway provider and each
/// selected public model is a gateway route.
#[allow(clippy::too_many_arguments)]
async fn relayed_entry(
    store: &Store,
    config: &Config,
    target: &AgentTarget,
    binding: &config::NativeProvider,
    metadata: Option<&crate::catalog::Snapshot>,
    requirements: &compatibility::Context<'_>,
    model_metadata: &mut BTreeMap<String, Value>,
    model_projection: &mut BTreeMap<String, Value>,
    require_ready: bool,
) -> Result<(String, Option<String>, Option<String>)> {
    let provider = config
        .providers
        .get(config::GATEWAY_PROVIDER_ID)
        .ok_or_else(|| {
            Error::new(
                "model_unavailable",
                "The gateway provider is not configured",
            )
        })?;
    let gateway = config
        .gateway
        .as_ref()
        .ok_or_else(|| Error::new("model_unavailable", "The sole gateway is not configured"))?;
    // Inspection describes desired delivery; only application requires a ready
    // listener on the request path.
    if require_ready {
        let status = crate::service::status(store).await?;
        let desired_revision = config::load(store)?.revision;
        let ready = &status["gateway"];
        if ready["ready"] != true
            || ready["listen"] != gateway.listen
            || ready["key_ref"] != gateway.key_ref
            || ready["config_revision"] != desired_revision
        {
            return Err(Error::new(
                "gateway_not_ready",
                "Explicitly start the configured gateway and activate the matching complete snapshot and shared key before native application",
            ));
        }
    }
    for m in &binding.models {
        requirements.assess_public(config, &m.id, &m.protocol)?;
        if let Some(alias) = config
            .model_aliases
            .iter()
            .find(|a| a.id == m.id && a.publishes(&m.protocol) && a.enabled)
        {
            let effective = crate::reasoning::alias_metadata(config, metadata, alias, &m.protocol)?;
            model_metadata.insert(
                m.id.clone(),
                adapters::alias_model_metadata(&target.agent, &m.protocol, &effective)?,
            );
            model_projection.insert(
                m.id.clone(),
                adapters::projection_view(&target.agent, &m.protocol, &effective)?,
            );
        } else {
            let (_, model_id) = config::qualified(&m.id)?;
            let (p, model) = config::model(config, &m.id)?;
            if !p.enabled
                || !model.enabled
                || !crate::provider_integration::protocols(p)?
                    .iter()
                    .any(|p| p == &m.protocol)
            {
                return Err(Error::new(
                    "model_unavailable",
                    "Selected public concrete gateway model is not published for this protocol",
                ));
            }
            if matches!(
                target.agent.as_str(),
                "pi" | "omp" | "opencode" | "hermes" | "dsh"
            ) {
                let projection =
                    crate::catalog::effective_from_snapshot(metadata, p, Some((model_id, model)))?;
                let inputs =
                    crate::reasoning::input_metadata(&projection["effective"], model, &m.protocol);
                model_metadata.insert(
                    m.id.clone(),
                    adapters::model_metadata(&target.agent, &m.protocol, &inputs)?,
                );
                model_projection.insert(
                    m.id.clone(),
                    adapters::projection_view(&target.agent, &m.protocol, &inputs)?,
                );
            }
        }
    }
    let key_ref = match provider.auth.as_ref() {
        Some(Auth::ApiKey { key_ref }) => Some(key_ref.clone()),
        _ => {
            return Err(Error::new(
                "credential_unavailable",
                "The gateway provider has no shared key reference",
            ));
        }
    };
    Ok((format!("http://{}", gateway.listen), key_ref, None))
}

pub(super) async fn entries(
    store: &Store,
    config: &Config,
    id: &str,
    target: &AgentTarget,
    path: &Path,
    inspection: bool,
) -> Result<(Vec<Entry>, Vec<String>)> {
    if target.agent == "dsh" {
        dsh_preflight(path)?;
    }
    let mut entries = Vec::new();
    let mut references = Vec::new();
    let authorization = compatibility::AuthorizationSnapshot::capture(store)?;
    let metadata = if matches!(
        target.agent.as_str(),
        "pi" | "omp" | "opencode" | "hermes" | "dsh"
    ) {
        crate::catalog::read(store, "accepted")?
    } else {
        None
    };
    for (name, binding) in &target.providers {
        if binding.models.is_empty() {
            return Err(Error::new(
                "target_incomplete",
                "Every native provider entry requires an explicit model selection",
            ));
        }
        let protocols: BTreeSet<&str> =
            binding.models.iter().map(|m| m.protocol.as_str()).collect();
        if protocols.len() != 1 && matches!(target.agent.as_str(), "codex" | "hermes" | "dsh") {
            return Err(Error::new(
                "incompatible",
                "This native provider has one effective transport; select separate native entries for different protocols",
            ));
        }
        let protocol = *protocols
            .first()
            .ok_or_else(|| Error::new("target_incomplete", "Select a native protocol"))?;
        let relayed = binding.is_relayed(config);
        let requirements = compatibility::Context {
            agent: &target.agent,
            target: Some(id),
            provider: if relayed {
                None
            } else {
                Some(binding.provider.as_str())
            },
            gateway: relayed,
        };
        requirements.protocol(protocol)?;
        let mut canonical = None;
        let mut model_routes = BTreeMap::new();
        let mut model_metadata = BTreeMap::new();
        let mut model_projection = BTreeMap::new();
        let provider_id = binding.provider.as_str();
        let (base, key_ref, login_provider) = if config.is_gateway_provider(provider_id) {
            relayed_entry(
                store,
                config,
                target,
                binding,
                metadata.as_ref(),
                &requirements,
                &mut model_metadata,
                &mut model_projection,
                !inspection,
            )
            .await?
        } else {
            let provider = config.providers.get(provider_id).ok_or_else(|| {
                Error::new(
                    "unresolved_reference",
                    "Native upstream provider does not resolve",
                )
            })?;
            if !provider.enabled {
                return Err(Error::new(
                    "model_unavailable",
                    "Native upstream provider is disabled",
                ));
            }
            let integration = crate::provider_integration::integration_id(provider)?;
            for selected in &binding.models {
                let model = provider.models.get(&selected.id).ok_or_else(|| {
                    Error::new(
                        "unresolved_reference",
                        "Native upstream model does not resolve",
                    )
                })?;
                if !model.enabled {
                    return Err(Error::new(
                        "model_unavailable",
                        "Native upstream model is disabled",
                    ));
                }
                requirements.assess(config, provider_id, &selected.protocol, &selected.id)?;
                if matches!(
                    target.agent.as_str(),
                    "pi" | "omp" | "opencode" | "hermes" | "dsh"
                ) {
                    let projection = crate::catalog::effective_from_snapshot(
                        metadata.as_ref(),
                        provider,
                        Some((&selected.id, model)),
                    )?;
                    model_metadata.insert(
                        selected.id.clone(),
                        adapters::model_metadata(
                            &target.agent,
                            &selected.protocol,
                            &projection["effective"],
                        )?,
                    );
                    model_projection.insert(
                        selected.id.clone(),
                        adapters::projection_view(
                            &target.agent,
                            &selected.protocol,
                            &projection["effective"],
                        )?,
                    );
                }
            }
            if integration == "opencode-go" && target.agent == "pi" {
                canonical = Some("opencode-go".to_owned());
            }
            let bases = crate::provider_integration::resolve(provider)?;
            for selected in &binding.models {
                let endpoint = bases.get(&selected.protocol).cloned().ok_or_else(|| {
                    Error::new(
                        "incompatible",
                        "The upstream integration has no endpoint for a selected native protocol",
                    )
                })?;
                if model_routes
                    .insert(selected.id.clone(), (selected.protocol.clone(), endpoint))
                    .is_some()
                {
                    return Err(Error::new(
                        "native_namespace_conflict",
                        "One native provider/model ID cannot simultaneously name two protocol variants",
                    ));
                }
            }
            let base = bases.get(protocol).cloned().ok_or_else(|| {
                Error::new(
                    "incompatible",
                    "The upstream integration has no endpoint for this native protocol",
                )
            })?;
            match provider.auth.as_ref() {
                Some(Auth::ApiKey { key_ref }) => (base, Some(key_ref.clone()), None),
                Some(Auth::Login { .. }) => {
                    let native_provider = requirements
                        .login_provider(provider)?
                        .ok_or_else(unavailable)?;
                    authorization.assess(store, config, &requirements, provider_id)?;
                    (base, None, Some(native_provider.to_owned()))
                }
                None => {
                    return Err(Error::new(
                        "credential_unavailable",
                        "Configure the selected provider's API key or native login before application",
                    ));
                }
            }
        };
        if model_routes.is_empty() {
            for selected in &binding.models {
                if model_routes
                    .insert(
                        selected.id.clone(),
                        (selected.protocol.clone(), base.clone()),
                    )
                    .is_some()
                {
                    return Err(Error::new(
                        "native_namespace_conflict",
                        "One native model ID cannot name two protocol variants",
                    ));
                }
            }
        }
        if key_ref.is_some()
            && target.agent == "pi"
            && auth_namespace_present(
                &path.join("auth.json"),
                canonical.as_deref().unwrap_or(name),
            )?
        {
            return Err(Error::new(
                "native_credential_precedence_unknown",
                "The selected exact native provider namespace has saved authentication that can override this key reference; resolve it through the native owner or select an independent scope. Credential values were not materialized",
            ));
        }
        if target.agent == "codex" {
            for selected in &binding.models {
                model_projection.insert(
                    selected.id.clone(),
                    adapters::projection_view(&target.agent, &selected.protocol, &Value::Null)?,
                );
            }
        }
        let key = key_ref.inspect(|reference| {
            references.push(reference.clone());
        });
        entries.push(Entry {
            name: name.clone(),
            base,
            protocol: protocol.into(),
            key,
            login_provider,
            canonical,
            gateway: relayed,
            models: binding.models.iter().map(|m| m.id.clone()).collect(),
            model_routes,
            model_metadata,
            model_projection,
        });
    }
    if (target.agent == "codex" && target.default.is_none()) || entries.is_empty() {
        return Err(Error::new(
            "target_incomplete",
            "Select providers and models before applying; Codex also requires a default model",
        ));
    }
    let mut names = BTreeSet::new();
    for entry in &entries {
        if !names.insert(entry.native_name()) {
            return Err(Error::new(
                "native_scope_conflict",
                "Two desired entries resolve to one native provider/authentication namespace",
            ));
        }
    }
    Ok((entries, references))
}
pub(super) fn pending_native_changes(
    target: &AgentTarget,
    files: &[File],
    references: &[String],
    record: Option<&Record>,
) -> Result<Vec<Value>> {
    let mut changes = Vec::new();
    for file in files
        .iter()
        .filter(|file| file.format != Format::Credential)
    {
        let current = document::parse(
            read_native(&file.path)?.as_deref().unwrap_or(""),
            file.format,
        )?;
        let previous = record.and_then(|record| {
            record
                .files
                .iter()
                .find(|previous| previous.path == file.path)
        });
        let desired = adapters::desired_fields_with_ownership(
            &target.agent,
            &file.desired,
            &current,
            previous.map_or(&[], |file| file.fields.as_slice()),
        );
        for (path, value) in &desired {
            if !previous.is_some_and(|file| {
                file.written
                    && file.fields.iter().any(|field| {
                        !field.released
                            && field.path == *path
                            && field.after_hash == field_hash(document::get(&current, path))
                    })
                    && (inline_credential(&target.agent, path)
                        || document::get(&current, path) == value.as_ref())
            }) {
                changes.push(json!({"path":file.path,"field":path,"change":if value.is_some(){"set"}else{"remove"}}));
            }
        }
        if let Some(previous) = previous {
            for field in previous.fields.iter().filter(|field| !field.released) {
                if !desired.iter().any(|(path, _)| *path == field.path) {
                    changes.push(json!({"path":file.path,"field":field.path,"change":"release"}));
                }
            }
        }
    }
    if let Some(record) = record {
        for file in record.files.iter().filter(|file| {
            file.format != Format::Credential
                && !files.iter().any(|desired| desired.path == file.path)
        }) {
            for field in file.fields.iter().filter(|field| !field.released) {
                changes.push(json!({"path":file.path,"field":field.path,"change":"release"}));
            }
        }
    }
    if record
        .map(|record| record.secret_refs.as_slice())
        .unwrap_or_default()
        != references
    {
        changes.push(
            json!({"field":["credential_reference"],"change":"replace","secret_reads":false}),
        );
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::super::{
        application::apply,
        recovery::{Field, FileRecord},
        test_support::inspection_target,
    };
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    #[tokio::test]
    async fn unavailable_scope_inspection_retains_desired_target_and_apply_stays_blocked() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let mut config = Config::empty();
        let path = directory.path().join("not-created");
        config
            .agents
            .insert("managed".into(), inspection_target(path.clone()));
        let inspected = inspect(&store, &config, "managed").await.unwrap();
        assert_eq!(inspected["path"], json!(path));
        assert_eq!(inspected["scope_reason_code"], "absent");
        assert_eq!(inspected["default"]["id"], "first");
        assert_eq!(inspected["providers"][0]["models"][0]["id"], "first");
        assert_eq!(inspected["applied_state"], "unknown");
        assert!(inspected["pending_native_changes"].is_null());
        assert_eq!(inspected["installation"]["state"], "not_checked");
        assert_eq!(
            inspected["installation"]["reason_code"],
            "native_scope_unavailable"
        );
        for removed in [
            "warnings",
            "desired_state",
            "last_application_state",
            "effective_state",
        ] {
            assert!(inspected.get(removed).is_none(), "{removed} is present");
        }
        let error = apply(&store, &config, "managed", false).await.unwrap_err();
        assert_eq!(error.code, "native_scope_unavailable");
        assert!(error.message.contains(path.to_str().unwrap()));
        assert!(!store.root.exists());
        fs::write(&path, b"not a directory").unwrap();
        let inspected = inspect(&store, &config, "managed").await.unwrap();
        assert_eq!(inspected["scope_reason_code"], "not_directory");
        assert_eq!(inspected["files"], json!([]));
    }
    #[test]
    fn current_native_projection_reports_changed_default_separately_from_last_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let target = inspection_target(directory.path().to_owned());
        fs::write(&path, b"{\"defaultModel\":\"first\"}").unwrap();
        let mut record = Record {
            id: "retained".into(),
            owner: "stable".into(),
            target: "managed".into(),
            agent: "pi".into(),
            scope: target.scope.clone(),
            payload_ref: "not-read".into(),
            secret_refs: vec![],
            state: "applied_reload_pending".into(),
            delivery: None,
            files: vec![FileRecord {
                path: path.clone(),
                format: Format::Json,
                original_absent: false,
                after_revision: "old".into(),
                written: true,
                fields: vec![Field {
                    path: vec!["defaultModel".into()],
                    after_hash: field_hash(Some(&json!("first"))),
                    before_hash: "absent".into(),
                    observed_hash: "absent".into(),
                    released: false,
                }],
            }],
        };
        let files = [File {
            path: path.clone(),
            format: Format::Json,
            desired: json!({"defaultModel":"second"}),
        }];
        let changes = pending_native_changes(&target, &files, &[], Some(&record)).unwrap();
        assert_eq!(
            changes,
            vec![json!({"path":path,"field":["defaultModel"],"change":"set"})]
        );
        assert_eq!(record.state, "applied_reload_pending");
        record.files[0].fields[0].after_hash = field_hash(Some(&json!("second")));
        fs::write(&path, b"{\"defaultModel\":\"second\"}").unwrap();
        assert!(
            pending_native_changes(&target, &files, &[], Some(&record))
                .unwrap()
                .is_empty()
        );
        record.files[0].written = false;
        record.state = "recovery_required".into();
        assert_eq!(
            pending_native_changes(&target, &files, &[], Some(&record)).unwrap()[0]["field"],
            json!(["defaultModel"])
        );
        record.files[0].written = true;
        fs::write(&path, b"{\"defaultModel\":\"first\"}").unwrap();
        assert_eq!(
            pending_native_changes(&target, &files, &[], Some(&record)).unwrap()[0]["change"],
            "set"
        );
        assert_eq!(
            pending_native_changes(&target, &[], &[], Some(&record)).unwrap()[0]["change"],
            "release"
        );
    }
    #[tokio::test]
    async fn entries_use_accepted_effective_concrete_metadata_without_native_preference_edits() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().join("tmax"));
        let accepted = crate::catalog::Snapshot {
            revision: "accepted".into(),
            providers: BTreeMap::from([(
                "catalog".into(),
                json!({"models":{"m":{
                    "limit":{"context":1000000,"output":65536},"reasoning":true,"tool_call":true,
                    "modalities":{"input":["text","image","video"],"output":["text"]}
                }}}),
            )]),
            ..Default::default()
        };
        store
            .write_json("catalog/accepted.json", &accepted)
            .unwrap();
        let mut latest = accepted.clone();
        latest.revision = "not-accepted".into();
        latest.providers.get_mut("catalog").unwrap()["models"]["m"]["limit"]["context"] = json!(17);
        store.write_json("catalog/latest.json", &latest).unwrap();
        let mut provider = config::Provider {
            integration: Some("custom".into()),
            metadata: Some("catalog".into()),
            auth: Some(Auth::ApiKey {
                key_ref: "opaque-reference-not-read".into(),
            }),
            endpoints: BTreeMap::from([(
                "chat-completions".into(),
                "https://example.invalid/v1".into(),
            )]),
            ..Default::default()
        };
        let mut model = config::Model::default();
        model.overrides.reasoning = Some(false);
        model.overrides.tool_call = Some(false);
        model.overrides.limit = Some(config::Limits {
            context: None,
            input: None,
            output: Some(4096),
        });
        model.overrides.modalities = Some(config::Modalities {
            input: Some(vec!["image".into()]),
            output: None,
        });
        provider.models = BTreeMap::from([
            ("m".into(), model),
            ("unknown".into(), config::Model::default()),
        ]);
        let mut config = Config::empty();
        config.providers.insert("p".into(), provider);
        let target = AgentTarget {
            agent: "pi".into(),
            scope: temporary.path().join("native"),
            providers: BTreeMap::from([(
                "local".into(),
                config::NativeProvider {
                    provider: "p".into(),
                    models: ["m", "unknown"]
                        .into_iter()
                        .map(|id| config::ModelSelection {
                            id: id.into(),
                            protocol: "chat-completions".into(),
                        })
                        .collect(),
                },
            )]),
            default: Some(config::DefaultReference {
                provider: "local".into(),
                id: "m".into(),
                protocol: "chat-completions".into(),
                effort: None,
                native_effort: None,
            }),
            roles: BTreeMap::new(),
        };
        let (entries, _) = entries(&store, &config, "target", &target, &target.scope, true)
            .await
            .unwrap();
        assert_eq!(
            entries[0].model_metadata["m"],
            json!({"contextWindow":1000000,"maxTokens":4096,"reasoning":false,"input":["image"]})
        );
        assert_eq!(entries[0].model_metadata["unknown"], json!({}));
        let files = adapters::build(&target, &target.scope, &entries).unwrap();
        let models = files
            .iter()
            .find(|file| file.path == target.scope.join("models.json"))
            .unwrap();
        let settings = files
            .iter()
            .find(|file| file.path == target.scope.join("settings.json"))
            .unwrap();
        assert_eq!(
            models.desired["providers"]["local"]["models"][0]["maxTokens"],
            4096
        );
        let original = r#"{"defaultThinkingLevel":"high","modelThinkingLevels":{"local/m":"low"},"unrelated":{"samplingParams":{"temperature":0.2},"maxTokens":777}}"#;
        let mut rewritten = original.to_owned();
        let fields = desired_fields(
            "pi",
            &settings.desired,
            &document::parse(original, Format::Json).unwrap(),
        );
        for (path, value) in fields {
            rewritten = document::set(&rewritten, Format::Json, &path, value.as_ref()).unwrap();
        }
        let after = document::parse(&rewritten, Format::Json).unwrap();
        let before = document::parse(original, Format::Json).unwrap();
        for field in ["defaultThinkingLevel", "modelThinkingLevels", "unrelated"] {
            assert_eq!(after[field], before[field]);
        }
        for agent in ["opencode", "hermes"] {
            let mut target = target.clone();
            target.agent = agent.into();
            let (prepared, references) =
                super::entries(&store, &config, "target", &target, &target.scope, true)
                    .await
                    .unwrap();
            assert_eq!(references, ["opaque-reference-not-read"]);
            let files = adapters::build(&target, &target.scope, &prepared).unwrap();
            let file = files
                .iter()
                .find(|file| file.format != Format::Credential)
                .unwrap();
            let native = &file.desired["providers"]["local"]["models"];
            if agent == "opencode" {
                assert_eq!(
                    native["m"]["limit"],
                    json!({"context":1000000,"output":4096})
                );
                assert_eq!(
                    native["m"]["capabilities"],
                    json!({"tools":false,"input":["image"],"output":["text"]})
                );
                assert!(native["m"].get("reasoning").is_none());
                assert!(native["unknown"].get("limit").is_none());
                assert!(native["unknown"].get("capabilities").is_none());
            } else {
                assert_eq!(native["m"], json!({"context_length":1000000}));
                assert_eq!(
                    file.desired["model_overrides"]["local"]["m"],
                    json!({
                        "context_window":1000000,"supports_reasoning":false,"supports_tools":false,"supports_vision":true
                    })
                );
                assert_eq!(native["unknown"], json!({}));
                assert!(
                    file.desired["model_overrides"]["local"]
                        .get("unknown")
                        .is_none()
                );
            }
            // Exercise the same leaf writer as apply, including nested provider models.
            // Existing native request bodies/effort/budgets and unrelated models survive.
            let original = json!({
                "providers":{"local":{"models":{"m":{
                    "settings":{"temperature":0.2,"maxTokens":777},
                    "body":{"max_output_tokens":777,"reasoning":{"effort":"high"}}
                },"unknown":{"body":{"max_tokens":444},"capabilities":{"supports_vision":true}},
                "user-model":{"body":{"temperature":0.8}}}}},
                "agent":{"reasoning_effort":"high"},"reasoning_overrides":{"m":"low"}
            });
            let mut rewritten = if file.format == Format::Yaml {
                serde_yaml::to_string(&original).unwrap()
            } else {
                serde_json::to_string(&original).unwrap()
            };
            let fields = desired_fields(agent, &file.desired, &original);
            for (path, value) in fields {
                rewritten = document::set(&rewritten, file.format, &path, value.as_ref()).unwrap();
            }
            let after = document::parse(&rewritten, file.format).unwrap();
            assert_eq!(
                after["providers"]["local"]["models"]["m"]["body"],
                original["providers"]["local"]["models"]["m"]["body"]
            );
            for field in ["temperature", "maxTokens"] {
                assert_eq!(
                    after["providers"]["local"]["models"]["m"]["settings"][field],
                    original["providers"]["local"]["models"]["m"]["settings"][field]
                );
            }
            assert_eq!(
                after["providers"]["local"]["models"]["user-model"],
                original["providers"]["local"]["models"]["user-model"]
            );
            for field in ["body", "capabilities"] {
                assert_eq!(
                    after["providers"]["local"]["models"]["unknown"][field],
                    original["providers"]["local"]["models"]["unknown"][field]
                );
            }
            for field in ["agent", "reasoning_overrides"] {
                assert_eq!(after[field], original[field]);
            }
        }
    }

    #[tokio::test]
    async fn direct_entries_retain_upstream_efforts_while_gateway_projection_uses_mapped_inputs() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().join("tmax"));
        let mut model = config::Model {
            overrides: serde_json::from_value(json!({
                "reasoning":true,
                "reasoning_options":[{"type":"effort","values":["high","max"]}]
            }))
            .unwrap(),
            ..Default::default()
        };
        model.effort_map.insert(
            "chat-completions".into(),
            BTreeMap::from([("xhigh".into(), "high".into())]),
        );
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("custom".into()),
                auth: Some(Auth::ApiKey {
                    key_ref: "opaque-reference-not-read".into(),
                }),
                endpoints: BTreeMap::from([(
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                )]),
                models: BTreeMap::from([("m".into(), model)]),
                ..Default::default()
            },
        );
        let target = AgentTarget {
            agent: "omp".into(),
            scope: temporary.path().join("native"),
            providers: BTreeMap::from([(
                "local".into(),
                config::NativeProvider {
                    provider: "p".into(),
                    models: vec![config::ModelSelection {
                        id: "m".into(),
                        protocol: "chat-completions".into(),
                    }],
                },
            )]),
            default: None,
            roles: BTreeMap::new(),
        };
        let (direct, _) = entries(&store, &config, "target", &target, &target.scope, true)
            .await
            .unwrap();
        assert_eq!(
            direct[0].model_metadata["m"]["thinking"],
            json!({"mode":"effort","efforts":["high","max"]})
        );
        let provider = &config.providers["p"];
        let model = &provider.models["m"];
        let upstream =
            crate::catalog::effective_from_snapshot(None, provider, Some(("m", model))).unwrap();
        let inputs =
            crate::reasoning::input_metadata(&upstream["effective"], model, "chat-completions");
        assert_eq!(
            adapters::model_metadata("omp", "chat-completions", &inputs).unwrap()["thinking"],
            json!({"mode":"effort","efforts":["high","max","xhigh"]})
        );
        assert_eq!(
            adapters::projection_view("omp", "chat-completions", &inputs).unwrap()["fields"]["thinking"],
            json!({"mode":"effort","efforts":["high","max","xhigh"]})
        );
        assert_eq!(
            crate::reasoning::effort_values(&upstream["effective"]).unwrap(),
            ["high", "max"]
        );
    }

    #[test]
    fn alias_projection_intersects_mapped_and_fixed_efforts_of_every_configured_candidate() {
        let mut first = config::Model {
            overrides: serde_json::from_value(json!({
                "reasoning_options":[{"type":"effort","values":["high","max"]}]
            }))
            .unwrap(),
            ..Default::default()
        };
        first.effort_map.insert(
            "chat-completions".into(),
            BTreeMap::from([("xhigh".into(), "max".into())]),
        );
        let mut second = config::Model {
            enabled: false,
            priority: -100,
            ..Default::default()
        };
        second.overrides = serde_json::from_value(json!({
            "reasoning_options":[{"type":"effort","values":["xhigh","max"]}]
        }))
        .unwrap();
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                models: BTreeMap::from([("first".into(), first), ("second".into(), second)]),
                ..Default::default()
            },
        );
        let mut alias = config::ModelAlias {
            id: "shared".into(),
            protocols: vec!["chat-completions".into()],
            candidates: vec!["p/first".into(), "p/second".into()],
            candidate_reasoning_effort: BTreeMap::new(),
            enabled: true,
            continuity: Some("stateless".into()),
            fallback: None,
        };
        let effective =
            crate::reasoning::alias_metadata(&config, None, &alias, "chat-completions").unwrap();
        assert_eq!(
            adapters::alias_model_metadata("dsh", "chat-completions", &effective).unwrap(),
            json!({"reasoningEfforts":{"xhigh":"xhigh","max":"max"}})
        );
        assert_eq!(
            adapters::projection_view("dsh", "chat-completions", &effective).unwrap()["fields"],
            json!({"reasoningEfforts":{"xhigh":"xhigh","max":"max"}})
        );
        alias
            .candidate_reasoning_effort
            .insert("p/second".into(), "xhigh".into());
        let effective =
            crate::reasoning::alias_metadata(&config, None, &alias, "chat-completions").unwrap();
        assert_eq!(
            adapters::alias_model_metadata("dsh", "chat-completions", &effective).unwrap(),
            json!({"reasoningEfforts":{"xhigh":"xhigh"}})
        );
        alias
            .candidate_reasoning_effort
            .insert("p/first".into(), "high".into());
        let effective =
            crate::reasoning::alias_metadata(&config, None, &alias, "chat-completions").unwrap();
        assert_eq!(
            adapters::alias_model_metadata("dsh", "chat-completions", &effective).unwrap(),
            json!({"reasoningEfforts":false})
        );
    }

    #[tokio::test]
    async fn relayed_provider_entries_inspect_without_readiness_and_block_application() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().join("tmax"));
        let mut config = Config::empty();
        config.gateway = Some(config::Gateway {
            listen: "127.0.0.1:17840".into(),
            key_ref: "secret:opaque-not-read".into(),
            autostart: true,
        });
        config.model_aliases.push(config::ModelAlias {
            id: "route".into(),
            protocols: vec!["responses".into()],
            candidates: vec!["up/route".into()],
            candidate_reasoning_effort: BTreeMap::new(),
            enabled: true,
            continuity: Some("stateless".into()),
            fallback: None,
        });
        let mut upstream = config::Provider {
            integration: Some("custom".into()),
            auth: Some(Auth::ApiKey {
                key_ref: "opaque-reference-not-read".into(),
            }),
            endpoints: BTreeMap::from([("responses".into(), "https://example.invalid/v1".into())]),
            models: BTreeMap::from([("route".into(), config::Model::default())]),
            ..Default::default()
        };
        upstream.enabled = true;
        config.providers.insert("up".into(), upstream);
        crate::config::apply_gateway_provider(&mut config);
        let target = AgentTarget {
            agent: "omp".into(),
            scope: temporary.path().join("native"),
            providers: BTreeMap::from([(
                "relayed".into(),
                config::NativeProvider {
                    provider: config::GATEWAY_PROVIDER_ID.into(),
                    models: vec![config::ModelSelection {
                        id: "route".into(),
                        protocol: "responses".into(),
                    }],
                },
            )]),
            default: None,
            roles: BTreeMap::new(),
        };
        // Inspection describes desired delivery without a ready listener.
        let (inspected, _) = entries(&store, &config, "target", &target, &target.scope, true)
            .await
            .unwrap();
        assert_eq!(inspected.len(), 1);
        assert!(inspected[0].gateway);
        assert_eq!(inspected[0].base, "http://127.0.0.1:17840");
        // Application requires the ready listener on the request path.
        let error = entries(&store, &config, "target", &target, &target.scope, false)
            .await
            .unwrap_err();
        assert_eq!(error.code, "gateway_not_ready", "{error:?}");
    }
    #[tokio::test]
    async fn dsh_inspect_reports_relayed_delivery_facts_without_requiring_a_ready_gateway() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("tmax"));
        let scope = directory.path().join("dsh-home");
        std::fs::create_dir_all(&scope).unwrap();
        let text = "version=2\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='secret:gw'\n[providers.up]\nintegration='custom'\n[providers.up.endpoints]\nresponses='http://127.0.0.1:39999'\n[providers.up.auth]\nmethod='api-key'\nkey_ref='secret:up'\n[providers.up.models.route]\n[[model_aliases]]\nid='route'\nprotocols=['responses']\ncontinuity='stateless'\ncandidates=['up/route']\n";
        let target = format!(
            "[agents.dsh-main]\nagent='dsh'\nscope='{}'\n[agents.dsh-main.providers.tmax]\nprovider='tmax'\nmodels=[{{id='route',protocol='responses'}}]\n[agents.dsh-main.default]\nprovider='tmax'\nid='route'\nprotocol='responses'\n",
            scope.display()
        );
        store
            .write_raw("tmax.toml", format!("{text}{target}").as_bytes())
            .unwrap();
        let config = config::load(&store).unwrap().config;
        let inspected = inspect(&store, &config, "dsh-main").await.unwrap();
        let entry = &inspected["providers"][0];
        assert_eq!(entry["relayed"], true);
        assert_eq!(entry["base_url"], "http://127.0.0.1:17840");
        assert!(
            entry["credential_ref"]
                .as_str()
                .is_some_and(|reference| reference.starts_with("TMAX_"))
        );
        assert!(inspected["configuration_revision"].is_string());
        assert!(
            inspected["native_projection"].is_array(),
            "desired projection is reported even without a ready gateway: {inspected}"
        );
        assert!(inspected["blockers"].as_array().is_some_and(Vec::is_empty));
    }

    fn delivery_config(listen: &str, key_ref: &str, routes: &[&str]) -> Config {
        let mut config = Config::empty();
        config.gateway = Some(config::Gateway {
            listen: listen.into(),
            key_ref: key_ref.into(),
            autostart: true,
        });
        config.model_aliases = routes
            .iter()
            .map(|route| config::ModelAlias {
                id: route.to_string(),
                protocols: vec!["responses".into()],
                candidates: vec!["up/route".into()],
                candidate_reasoning_effort: BTreeMap::new(),
                enabled: true,
                continuity: Some("stateless".into()),
                fallback: None,
            })
            .collect();
        config
    }

    fn delivery_target(routes: &[&str]) -> AgentTarget {
        AgentTarget {
            agent: "pi".into(),
            scope: PathBuf::from("/tmp/scope"),
            providers: BTreeMap::from([(
                "relayed".into(),
                config::NativeProvider {
                    provider: config::GATEWAY_PROVIDER_ID.into(),
                    models: routes
                        .iter()
                        .map(|route| config::ModelSelection {
                            id: route.to_string(),
                            protocol: "responses".into(),
                        })
                        .collect(),
                },
            )]),
            default: None,
            roles: BTreeMap::new(),
        }
    }

    fn delivery_record(config: &Config, target: &AgentTarget) -> super::Record {
        let delivery = config::gateway_delivery_for(config, &config.relayed_route_pairs(target))
            .map(Into::into);
        super::Record {
            id: "r".into(),
            owner: "o".into(),
            target: "t".into(),
            agent: "pi".into(),
            scope: PathBuf::from("/tmp/scope"),
            payload_ref: "p".into(),
            secret_refs: Vec::new(),
            state: "applied_reload_pending".into(),
            files: Vec::new(),
            delivery,
        }
    }

    fn state(config: &Config, target: &AgentTarget, record: &super::Record) -> Value {
        applied_delivery(config, target, Some(record), Some(true), false)
    }

    #[test]
    fn applied_delivery_names_drift_and_blocks_without_merging_reasons() {
        let config = delivery_config("127.0.0.1:17840", "secret:gw", &["route"]);
        let target = delivery_target(&["route"]);
        let record = delivery_record(&config, &target);

        assert_eq!(state(&config, &target, &record)["state"], "current");
        assert_eq!(
            applied_delivery(&config, &target, None, None, false)["state"],
            "not_applied"
        );
        let mut legacy = delivery_record(&config, &target);
        legacy.delivery = None;
        let unknown = state(&config, &target, &legacy);
        assert_eq!(unknown["state"], "unknown");
        assert_eq!(unknown["reason_code"], "delivery_not_recorded");

        let listen = delivery_config("127.0.0.1:17841", "secret:gw", &["route"]);
        let listen_state = state(&listen, &target, &record);
        assert_eq!(listen_state["state"], "stale");
        assert_eq!(
            listen_state["reasons"],
            serde_json::json!(["listen_changed"])
        );

        let rotated = delivery_config("127.0.0.1:17840", "secret:rotated", &["route"]);
        assert_eq!(
            state(&rotated, &target, &record)["reasons"],
            serde_json::json!(["key_rotated"])
        );

        // An added route unrelated to the entry is not drift.
        let added = delivery_config("127.0.0.1:17840", "secret:gw", &["route", "other"]);
        let added_state = state(&added, &target, &record);
        assert_eq!(added_state["state"], "current");
        assert_eq!(added_state["reasons"], serde_json::json!([]));

        // Removing a referenced route is named, and the entry now references a missing route.
        let removed = delivery_config("127.0.0.1:17840", "secret:gw", &["other"]);
        let removed_state = state(&removed, &target, &record);
        assert_eq!(removed_state["state"], "stale");
        assert_eq!(
            removed_state["reasons"],
            serde_json::json!(["route_removed", "entry_references_missing_route"])
        );
        assert_eq!(
            removed_state["routes"]["removed"],
            serde_json::json!(["route"])
        );

        // The target's own selection changing needs a re-apply even when the
        // referenced route still exists: the target-scoped revision differs.
        let selected = delivery_config("127.0.0.1:17840", "secret:gw", &["route", "other"]);
        let selected_target = delivery_target(&["route", "other"]);
        let selected_state = state(&selected, &selected_target, &record);
        assert_eq!(selected_state["state"], "stale");
        assert_eq!(
            selected_state["reasons"],
            serde_json::json!(["entry_selection_changed"])
        );

        let blocked = applied_delivery(&config, &target, Some(&record), Some(false), false);
        assert_eq!(blocked["state"], "blocked");
        assert_eq!(blocked["reasons"], serde_json::json!(["gateway_not_ready"]));
        let conflict = applied_delivery(&config, &target, Some(&record), Some(true), true);
        assert_eq!(conflict["state"], "blocked");
        assert_eq!(
            conflict["reasons"],
            serde_json::json!(["native_edit_conflict"])
        );
    }
}
