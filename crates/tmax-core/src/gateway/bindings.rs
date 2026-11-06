use super::{
    client_identity::{NativeRequirements, client_contract},
    native_context,
    runtime::{Runtime, Snapshot},
};
use crate::{Error, Result, config::Config, diagnostics::safe_evidence, storage::Store};
use axum::http::HeaderMap;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Mutex};

mod store;
pub(super) use store::initialize_bindings;
use store::{load_bindings, save_bindings};

const BINDING_LIMIT: usize = 2048;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Bindings {
    epoch: chrono::DateTime<Utc>,
    entries: BTreeMap<String, Binding>,
}
impl Default for Bindings {
    fn default() -> Self {
        Self {
            epoch: Utc::now(),
            entries: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    context_handle: String,
    provider_identity: String,
    model_identity: String,
    provider: String,
    model: String,
    protocol: String,
    public_id: String,
    endpoint: String,
    authorization_generation: String,
    continuity: String,
    decision: Value,
    blocked: Option<String>,
}
pub(super) struct Choice {
    pub(super) selection: crate::routing::Selection,
    pub(super) provider_identity: String,
    pub(super) authorization_generation: String,
    pub(super) endpoint: String,
    pub(super) go_session_header: Option<&'static str>,
    pub(super) effort: Option<String>,
}

impl Runtime {
    pub fn binding_summary(&self) -> Value {
        let b = self.bindings.lock().expect("bindings mutex");
        json!({"retained":b.entries.len(),"blocked":b.entries.values().filter(|v|v.blocked.is_some()).count(),"session_qualification":"native session identity must be qualified before dynamic session admission"})
    }
    pub(crate) fn inspect_session(
        &self,
        config: &Config,
        public_id: &str,
        protocol: &str,
        session: &str,
        loaded: bool,
    ) -> Result<Value> {
        let identities = if loaded {
            crate::inputs::Snapshot::capture_loaded(&self.store, config, None)
        } else {
            crate::inputs::Snapshot::capture(&self.store, config)
        };
        let bindings = self.bindings.lock().expect("bindings mutex");
        retained_explanation(
            &bindings,
            config,
            &identities,
            public_id,
            protocol,
            session,
            "daemon",
        )
    }
    pub(crate) fn inspect_persisted_session(
        store: &Store,
        config: &Config,
        public_id: &str,
        protocol: &str,
        session: &str,
    ) -> Result<Value> {
        let identities = crate::inputs::Snapshot::capture(store, config);
        retained_explanation(
            &load_bindings(store)?,
            config,
            &identities,
            public_id,
            protocol,
            session,
            "persisted",
        )
    }
    pub fn activate_bindings(&self, config: &Config, dry_run: bool) -> Result<Value> {
        Self::transition_bindings(&self.store, &self.bindings, config, dry_run)
    }
    pub fn preview_persisted(store: &Store, config: &Config) -> Result<Value> {
        let bindings = Mutex::new(load_bindings(store)?);
        Self::transition_bindings(store, &bindings, config, true)
    }
    fn transition_bindings(
        store: &Store,
        retained: &Mutex<Bindings>,
        config: &Config,
        dry_run: bool,
    ) -> Result<Value> {
        let identities = if dry_run {
            crate::inputs::Snapshot::capture(store, config)
        } else {
            crate::inputs::Snapshot::capture_loaded(store, config, None)
        };
        let mut names = BTreeMap::new();
        for (provider_id, provider) in &config.providers {
            for model in provider.models.keys() {
                if let Some((identity, _, model_identity)) =
                    identities.destination_identity(provider_id, model)
                {
                    names.insert(
                        model_identity,
                        (provider_id.as_str(), model.as_str(), identity),
                    );
                }
            }
        }
        let mut bindings = retained.lock().expect("bindings mutex");
        let mut staged = (!dry_run).then(|| bindings.clone());
        let next = staged.as_mut().unwrap_or(&mut *bindings);
        let mut affected = Vec::new();
        let mut renamed = Vec::new();
        let mut rekey = Vec::new();
        for (handle, binding) in &mut next.entries {
            if binding.blocked.is_some() {
                continue;
            }
            let rename =
                names
                    .get(&binding.model_identity)
                    .filter(|(provider, model, identity)| {
                        *provider != binding.provider
                            && *model == binding.model
                            && identity == &binding.provider_identity
                    });
            let candidate = rename.map(|(provider, _, _)| {
                let mut candidate = binding.clone();
                if candidate.public_id == format!("{}/{}", candidate.provider, candidate.model) {
                    candidate.public_id = format!("{provider}/{}", candidate.model);
                }
                candidate.provider = (*provider).into();
                candidate
            });
            let current = candidate.as_ref().unwrap_or(binding);
            let reason = binding_blocker(config, current, &identities);
            if let Some(reason) = reason {
                affected.push(json!({"binding":handle,"reason_code":reason}));
                if !dry_run {
                    binding.blocked = Some(reason.into());
                }
            } else if let Some(candidate) = candidate {
                renamed.push(json!({"binding":handle,"provider":candidate.provider}));
                if !dry_run {
                    binding.provider = candidate.provider;
                    binding.public_id = candidate.public_id;
                    let key = native_context::binding_key(
                        &binding.context_handle,
                        &binding.public_id,
                        &binding.protocol,
                    );
                    if key != *handle {
                        rekey.push((handle.clone(), key));
                    }
                }
            }
        }
        if !dry_run {
            for (old, new) in rekey {
                if let Some(binding) = next.entries.remove(&old)
                    && next.entries.insert(new, binding).is_some()
                {
                    return Err(Error::new(
                        "binding_store_conflict",
                        "Native binding rename collided; affected sessions require inspection",
                    ));
                }
            }
            if !affected.is_empty() || !renamed.is_empty() {
                save_bindings(store, next)?;
                *bindings = staged.expect("executed binding transition");
            }
        }
        Ok(
            json!({"blocked_sessions":affected,"renamed_destinations":renamed,"in_flight":"retain admission snapshots"}),
        )
    }
    #[cfg(test)]
    pub(super) fn choose(
        &self,
        snapshot: &Snapshot,
        public_id: &str,
        protocol: &str,
        body: &Value,
        headers: &HeaderMap,
        now: chrono::DateTime<Utc>,
    ) -> Result<Choice> {
        self.choose_recorded(
            snapshot,
            (public_id, protocol),
            &super::request_facts::RequestFacts::from_value(body.clone())
                .expect("request fixture object"),
            headers,
            now,
            &mut Value::Null,
        )
    }

    #[cfg(test)]
    pub(super) fn choose_recorded(
        &self,
        snapshot: &Snapshot,
        route: (&str, &str),
        body: &super::request_facts::RequestFacts,
        headers: &HeaderMap,
        now: chrono::DateTime<Utc>,
        evidence_out: &mut Value,
    ) -> Result<Choice> {
        let identities = crate::inputs::Snapshot::capture_loaded(
            &self.store,
            &snapshot.config,
            snapshot.metadata.as_deref(),
        );
        self.choose_untried(
            snapshot,
            route,
            body,
            headers,
            now,
            evidence_out,
            &Default::default(),
            &identities,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn choose_untried(
        &self,
        snapshot: &Snapshot,
        route: (&str, &str),
        body: &super::request_facts::RequestFacts,
        headers: &HeaderMap,
        now: chrono::DateTime<Utc>,
        evidence_out: &mut Value,
        attempted: &std::collections::BTreeSet<String>,
        identities: &crate::inputs::Snapshot<'_>,
    ) -> Result<Choice> {
        let (public_id, protocol) = route;
        let config = &snapshot.config;
        let context = native_context::identify(headers, body, protocol);
        let native = NativeRequirements::capture(headers, context.as_ref());
        // The gateway is a plain model provider: any authenticated request is
        // routed and forwarded as-is. A recognized native client context only
        // enables destination affinity; nothing is rejected for being an
        // unrecognized client, an incomplete context, or a continuation that
        // references upstream-owned state.
        let alias = config
            .model_aliases
            .iter()
            .find(|a| a.id == public_id && a.protocol == protocol);
        let continuity = alias
            .map(|a| a.continuity.as_deref().unwrap_or("session"))
            .unwrap_or("fixed");
        let key = context
            .as_ref()
            .and_then(|c| c.binding_key(public_id, protocol));
        let retain = key.is_some()
            && continuity != "stateless"
            && (alias.is_some()
                || native_context::has_remote_state(body)
                || context.as_ref().is_some_and(|c| c.created_at.is_some()));
        let mut bindings = key
            .as_ref()
            .map(|_| self.bindings.lock().expect("bindings mutex"));
        let retained = 'retained: {
            let Some(bindings) = bindings.as_mut() else {
                break 'retained Ok(None);
            };
            let Some(key) = key.as_ref() else {
                break 'retained Ok(None);
            };
            let Some(binding) = bindings.entries.get_mut(key) else {
                break 'retained Ok(None);
            };
            // A blocked or incompatible retained destination only disables
            // destination affinity for this request; the request is still
            // served by normal selection.
            if binding.blocked.is_some() {
                break 'retained Ok(None);
            }
            if let Some(reason) = binding_blocker(config, binding, identities) {
                binding.blocked = Some(reason.into());
                // Persisting the block is housekeeping: a storage failure
                // must not deny a request the gateway can still serve.
                let _ = save_bindings(&self.store, bindings);
                break 'retained Ok(None);
            }
            let provider = &config.providers[&binding.provider];
            let mut evidence = json!({"selection_reason":"retained_destination","scope_handle":binding.context_handle,"original_decision":binding.decision,"selected":{ "provider":binding.provider,"model":binding.model,"qualified_model":format!("{}/{}",binding.provider,binding.model),"model_identity":binding.model_identity,"candidate_reasoning_effort":config.candidate_effort(public_id, protocol, &binding.provider, &binding.model)},"effective_priority":binding.decision.get("selection").and_then(|v|v.get("priority"))});
            let effort = selected_request(
                snapshot,
                public_id,
                protocol,
                (&binding.provider, &binding.model),
                body,
                &mut evidence,
                evidence_out,
            )?;
            client_contract(provider, &native)?;
            Ok(Some(Choice {
                selection: crate::routing::Selection {
                    provider: binding.provider.clone(),
                    model: binding.model.clone(),
                    model_identity: binding.model_identity.clone(),
                    priority: 0,
                    evidence,
                },
                provider_identity: binding.provider_identity.clone(),
                authorization_generation: binding.authorization_generation.clone(),
                endpoint: binding.endpoint.clone(),
                go_session_header: context.as_ref().and_then(|c| c.go_session_header),
                effort,
            }))
        }?;
        if let Some(choice) = retained {
            return Ok(choice);
        }
        // Affinity retention records only genuinely fresh qualified
        // sessions; an unfresh session is still served, just not retained.
        let record = retain
            && context.as_ref().is_some_and(|c| {
                c.can_start_binding(
                    body,
                    protocol,
                    bindings.as_ref().expect("qualified binding lock").epoch,
                    now,
                )
            });
        let mut selection = crate::routing::select_request_admitted(
            &self.store,
            config,
            public_id,
            protocol,
            now,
            crate::routing::RequestRequirements { native: &native },
            identities,
            attempted,
        )?;
        let provider = &config.providers[&selection.provider];
        client_contract(provider, &native)?;
        let effort = selected_request(
            snapshot,
            public_id,
            protocol,
            (&selection.provider, &selection.model),
            body,
            &mut selection.evidence,
            evidence_out,
        )?;
        let endpoints = crate::provider_integration::resolve(provider)?;
        let endpoint = endpoints
            .get(protocol)
            .cloned()
            .ok_or_else(|| Error::new("protocol_unavailable", "Protocol endpoint unavailable"))?;
        let captured = &selection.evidence["selection"];
        let provider_identity = captured["provider_identity"]
            .as_str()
            .ok_or_else(|| {
                Error::new(
                    "identity_unavailable",
                    "Selected provider lineage was not captured",
                )
            })?
            .to_owned();
        let authorization_generation = captured["authorization_generation"]
            .as_str()
            .ok_or_else(|| {
                Error::new(
                    "identity_unavailable",
                    "Selected authorization lineage was not captured",
                )
            })?
            .to_owned();
        let model_identity = selection.model_identity.clone();
        if let Some(selected) = selection.evidence.get("selection") {
            let name = selected.get("qualified_model");
            let selected_evaluation = selection
                .evidence
                .get("priority_evaluations")
                .and_then(Value::as_array)
                .and_then(|values| {
                    values
                        .iter()
                        .find(|value| value.get("qualified_model") == name)
                })
                .cloned();
            if let Some(evaluation) = selected_evaluation {
                selection.evidence["selected_evaluation"] = evaluation;
            }
        }
        if record {
            let bindings = bindings.as_mut().expect("qualified binding lock");
            if bindings.entries.len() >= BINDING_LIMIT {
                // Durable capacity only caps affinity retention; the request
                // itself is still served.
                selection.evidence["binding_retention"] = json!("capacity_reached");
            } else {
                selection.evidence["scope_handle"] = json!(
                    context
                        .as_ref()
                        .and_then(native_context::Context::scope_handle)
                );
                let binding = Binding {
                    context_handle: context
                        .as_ref()
                        .and_then(native_context::Context::scope_handle)
                        .expect("qualified native scope"),
                    provider_identity: provider_identity.clone(),
                    model_identity,
                    provider: selection.provider.clone(),
                    model: selection.model.clone(),
                    protocol: protocol.into(),
                    public_id: public_id.into(),
                    endpoint: endpoint.clone(),
                    authorization_generation: authorization_generation.clone(),
                    continuity: continuity.into(),
                    decision: safe_evidence(&selection.evidence),
                    blocked: None,
                };
                let key = key.expect("qualified binding key");
                // A permanently blocked conversation is never reassigned;
                // recording over it would silently clear the block.
                if bindings
                    .entries
                    .get(&key)
                    .is_some_and(|existing| existing.blocked.is_some())
                {
                    selection.evidence["binding_retention"] = json!("blocked");
                } else {
                    bindings.entries.insert(key.clone(), binding);
                    if save_bindings(&self.store, bindings).is_err() {
                        // Durable retention only improves later affinity; a
                        // storage failure still serves the current request.
                        bindings.entries.remove(&key);
                        selection.evidence["binding_retention"] = json!("save_failed");
                    }
                }
            }
        }
        Ok(Choice {
            selection,
            provider_identity,
            authorization_generation,
            endpoint,
            go_session_header: context.as_ref().and_then(|c| c.go_session_header),
            effort,
        })
    }
}

/// Resolve only after choosing a backend. Rejection cannot enter ranking again
/// or publish a new conversation binding.
fn selected_request(
    snapshot: &Snapshot,
    public_id: &str,
    protocol: &str,
    destination: (&str, &str),
    body: &super::request_facts::RequestFacts,
    evidence: &mut Value,
    evidence_out: &mut Value,
) -> Result<Option<String>> {
    let (provider_id, model_id) = destination;
    let provider = &snapshot.config.providers[provider_id];
    let model = &provider.models[model_id];
    let fixed = snapshot
        .config
        .candidate_effort(public_id, protocol, provider_id, model_id);
    let input = body.effort(protocol).as_str();
    let mapped = input.is_some_and(|input| {
        model
            .effort_map
            .get(protocol)
            .is_some_and(|map| map.contains_key(input))
    });
    let source = if fixed.is_some() {
        "fixed"
    } else if mapped {
        "map"
    } else if input.is_some() {
        "identity"
    } else {
        "omitted"
    };
    evidence["effort_resolution"] =
        json!({"input_effort":input,"upstream_effort":null,"source":source});
    *evidence_out = safe_evidence(evidence);
    let resolved = crate::reasoning::resolve(model, protocol, body, fixed);
    evidence["effort_resolution"]["upstream_effort"] = json!(resolved);
    *evidence_out = safe_evidence(evidence);
    let explicit = fixed.is_some() || mapped;
    let effort = resolved.filter(|effort| explicit && Some(*effort) != input);
    crate::provider_integration::check_request(provider, protocol)?;
    Ok(effort.map(str::to_owned))
}

fn retained_explanation(
    bindings: &Bindings,
    config: &Config,
    identities: &crate::inputs::Snapshot<'_>,
    public_id: &str,
    protocol: &str,
    session: &str,
    source: &str,
) -> Result<Value> {
    let key = native_context::binding_key(session, public_id, protocol);
    let binding = bindings
        .entries
        .get(&key)
        .filter(|binding| {
            binding.context_handle == session
                && binding.public_id == public_id
                && binding.protocol == protocol
        })
        .ok_or_else(|| {
            Error::new(
                "original_binding_unavailable",
                "No retained destination matches this gateway handle, public model ID and protocol",
            )
        })?;
    let blocker = binding
        .blocked
        .as_deref()
        .or_else(|| binding_blocker(config, binding, identities));
    Ok(json!({
        "session":session,"session_source":source,"public_id":public_id,"protocol":protocol,
        "selection_reason":"retained_destination",
        "selection":{"provider":binding.provider,"model":binding.model,"qualified_model":format!("{}/{}",binding.provider,binding.model),"model_identity":binding.model_identity,"candidate_reasoning_effort":config.candidate_effort(public_id, protocol, &binding.provider, &binding.model)},
        "continuation":{"state":if blocker.is_some(){"blocked"}else{"retained"},"reason_code":blocker,"stored_blocker":binding.blocked},
        "original_decision":safe_evidence(&binding.decision),"fresh_ranking":"not_evaluated"
    }))
}

fn binding_blocker(
    config: &Config,
    b: &Binding,
    identities: &crate::inputs::Snapshot<'_>,
) -> Option<&'static str> {
    let Some(p) = config.providers.get(&b.provider) else {
        return Some("provider_removed");
    };
    if !p.enabled {
        return Some("provider_disabled");
    }
    let Some(m) = p.models.get(&b.model) else {
        return Some("model_removed");
    };
    if !m.enabled {
        return Some("model_disabled");
    }
    let Ok(endpoints) = crate::provider_integration::resolve(p) else {
        return Some("endpoint_unavailable");
    };
    if endpoints.get(&b.protocol) != Some(&b.endpoint) {
        return Some("endpoint_changed");
    }
    let Some((identity, generation, model_identity)) =
        identities.destination_identity(&b.provider, &b.model)
    else {
        return Some("destination_identity_unavailable");
    };
    if identity != b.provider_identity || model_identity != b.model_identity {
        return Some("destination_replaced");
    }
    if generation != b.authorization_generation {
        return Some("authorization_changed");
    }
    if let Some(alias) = config
        .model_aliases
        .iter()
        .find(|a| a.id == b.public_id && a.protocol == b.protocol)
    {
        if !alias.enabled {
            return Some("alias_disabled");
        }
        if alias.continuity.as_deref().unwrap_or("session") != b.continuity {
            return Some("continuity_changed");
        }
        if !alias.candidates.iter().any(|candidate| {
            candidate.split_once('/') == Some((b.provider.as_str(), b.model.as_str()))
        }) {
            return Some("candidate_removed");
        }
    } else if b.public_id != format!("{}/{}", b.provider, b.model) {
        return Some("alias_removed");
    }
    None
}

#[cfg(test)]
mod affinity_tests;
#[cfg(test)]
mod continuity_tests;
#[cfg(test)]
mod migration_tests;
#[cfg(test)]
mod selection_tests;
