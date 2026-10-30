use crate::{
    Error, Result,
    config::{self, Config, InputDeclaration, Provider},
    storage::Store,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use super::model::{
    Binding, Channel, DeclarationIdentity, InputHead, ProviderIdentity, Runtime, View,
};

pub(super) fn token() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub(crate) fn signature<T: Serialize>(value: &T) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("serializable domain value"))
    )
}

fn source_definition(provider: &Provider, declaration: &InputDeclaration) -> String {
    source_definition_with_integration(
        provider,
        declaration,
        crate::provider_integration::integration_id(provider).ok(),
    )
}
pub(super) fn source_definition_with_integration(
    provider: &Provider,
    declaration: &InputDeclaration,
    integration: Option<&str>,
) -> String {
    let mut models = declaration.models.clone();
    models.sort();
    signature(
        &json!({"kind":declaration.kind,"source":declaration.source,"scope":declaration.scope,"models":models,
        "max_age_seconds":declaration.max_age_seconds,"integration":integration,
        "endpoints":crate::provider_integration::resolve(provider).ok()}),
    )
}
fn auth_definition(config: &Config, provider: &Provider) -> Value {
    let source = match &provider.auth {
        Some(config::Auth::Login { via, .. }) if via != "tmax" => config
            .agents
            .get(via)
            .map(|t| json!({"agent":t.agent,"scope":t.scope})),
        _ => None,
    };
    json!({"auth":provider.auth,"source":source})
}

// Read-only projections reuse unchanged bindings without reserving identities.
pub(super) fn project_view(old: &View, config: &Config) -> View {
    let mut providers = BTreeMap::new();
    for (name, p) in &config.providers {
        let mut ids = old.providers.get(name).cloned().unwrap_or_default();
        if ids.auth_definition != auth_definition(config, p) {
            ids.authorization.clear();
        }
        ids.models.retain(|m, _| p.models.contains_key(m));
        ids.declarations
            .retain(|input, _| p.inputs.contains_key(input));
        for (input, d) in &p.inputs {
            let definition = source_definition(p, d);
            if ids
                .declarations
                .get(input)
                .is_none_or(|prior| prior.definition != definition)
            {
                ids.declarations.insert(
                    input.clone(),
                    DeclarationIdentity {
                        revision: String::new(),
                        definition,
                    },
                );
            }
        }
        providers.insert(name.clone(), ids);
    }
    View {
        config: config.clone(),
        providers,
        revision: if signature(&old.config) == signature(config) {
            old.revision.clone()
        } else {
            None
        },
    }
}
pub(super) fn make_view(
    runtime: &mut Runtime,
    old: &View,
    config: &Config,
    renames: &[(String, String)],
) -> View {
    let mut previous = old.providers.clone();
    for (from, to) in renames {
        if !from.contains('/') && !to.contains('/') {
            if let Some(identity) = previous.remove(from) {
                previous.insert(to.clone(), identity);
            }
        } else if let (Ok((p, m)), Ok((np, nm))) = (config::qualified(from), config::qualified(to))
            && p == np
            && let Some(identity) = previous.get_mut(p)
            && let Some(id) = identity.models.remove(m)
        {
            identity.models.insert(nm.into(), id);
        }
    }
    let mut providers = BTreeMap::new();
    for (name, provider) in &config.providers {
        let mut ids = previous.remove(name).unwrap_or_else(|| ProviderIdentity {
            identity: token(),
            authorization: token(),
            ..Default::default()
        });
        if let Some(current) = runtime.authorization.get(&ids.identity) {
            ids.authorization = current.clone();
        }
        let auth = auth_definition(config, provider);
        if ids.auth_definition != auth {
            ids.authorization = token();
            ids.auth_definition = auth;
        }
        runtime
            .authorization
            .insert(ids.identity.clone(), ids.authorization.clone());
        ids.models
            .retain(|name, _| provider.models.contains_key(name));
        for model in provider.models.keys() {
            ids.models.entry(model.clone()).or_insert_with(token);
        }
        ids.declarations
            .retain(|name, _| provider.inputs.contains_key(name));
        for (input, declaration) in &provider.inputs {
            let definition = source_definition(provider, declaration);
            if ids
                .declarations
                .get(input)
                .is_none_or(|old| old.definition != definition)
            {
                ids.declarations.insert(
                    input.clone(),
                    DeclarationIdentity {
                        revision: token(),
                        definition,
                    },
                );
            }
        }
        providers.insert(name.clone(), ids);
    }
    View {
        config: config.clone(),
        providers,
        revision: None,
    }
}
pub(super) fn key(binding: &Binding) -> String {
    format!(
        "{}/{}/{}",
        binding.provider_identity, binding.authorization_generation, binding.declaration_revision
    )
}
pub(super) fn binding(
    view: &View,
    runtime: &Runtime,
    provider: &str,
    input: &str,
) -> Option<Binding> {
    let p = view.config.providers.get(provider)?;
    let d = p.inputs.get(input)?;
    let ids = view.providers.get(provider)?;
    let mut models = d.models.clone();
    models.sort();
    Some(Binding {
        provider: provider.into(),
        input: input.into(),
        provider_identity: ids.identity.clone(),
        authorization_generation: if ids.authorization.is_empty() {
            String::new()
        } else {
            runtime
                .authorization
                .get(&ids.identity)
                .unwrap_or(&ids.authorization)
                .clone()
        },
        declaration_revision: ids.declarations.get(input)?.revision.clone(),
        kind: d.kind.clone(),
        scope: d.scope.clone(),
        models,
        source: d.source.clone(),
    })
}

/// Resolve one retained view without acquiring another publication lock.
pub(crate) fn provider_scope_binding_in_view(
    runtime: &Runtime,
    view: &View,
    provider: &str,
) -> Option<(String, String)> {
    let ids = view.providers.get(provider)?;
    if ids.identity.is_empty() || ids.authorization.is_empty() {
        return None;
    }
    let authorization = runtime
        .authorization
        .get(&ids.identity)
        .unwrap_or(&ids.authorization);
    if authorization != &ids.authorization {
        return None;
    }
    Some((ids.identity.clone(), authorization.clone()))
}

/// Resolve saved intent or its read-only projection in an already-locked runtime.
pub(crate) fn provider_scope_binding(
    runtime: &Runtime,
    config: &Config,
    provider: &str,
) -> Option<(String, String)> {
    let hash = signature(config);
    if signature(&runtime.saved.config) == hash {
        provider_scope_binding_in_view(runtime, &runtime.saved, provider)
    } else if let Some(view) = runtime
        .loaded
        .as_ref()
        .filter(|view| signature(&view.config) == hash)
    {
        provider_scope_binding_in_view(runtime, view, provider)
    } else {
        let declaration = config.providers.get(provider)?;
        let ids = runtime.saved.providers.get(provider)?;
        if ids.auth_definition != auth_definition(config, declaration) {
            return None;
        }
        provider_scope_binding_in_view(runtime, &runtime.saved, provider)
    }
}
pub(super) fn retain(runtime: &mut Runtime) {
    let mut keys = BTreeSet::new();
    let mut providers = BTreeSet::new();
    for view in std::iter::once(&runtime.saved).chain(runtime.loaded.iter()) {
        for (p, provider) in &view.config.providers {
            if let Some(ids) = view.providers.get(p) {
                providers.insert(ids.identity.clone());
            }
            for name in provider.inputs.keys() {
                if let Some(b) = binding(view, runtime, p, name) {
                    keys.insert(key(&b));
                }
            }
        }
    }
    runtime.channels.retain(|k, _| keys.contains(k));
    runtime.authorization.retain(|p, _| providers.contains(p));
}

pub(super) fn invalidate_changed(runtime: &mut Runtime, previous: &Runtime) {
    let mut heads = Vec::new();
    for view in std::iter::once(&runtime.saved).chain(runtime.loaded.iter()) {
        for (p, provider) in &view.config.providers {
            for input in provider.inputs.keys() {
                let Some(now) = binding(view, runtime, p, input) else {
                    continue;
                };
                if runtime.channels.contains_key(&key(&now)) {
                    continue;
                }
                let previous_bindings = || {
                    std::iter::once(&previous.saved)
                        .chain(previous.loaded.iter())
                        .filter_map(|old| {
                            old.providers
                                .iter()
                                .find(|(_, ids)| ids.identity == now.provider_identity)
                                .and_then(|(p, _)| binding(old, previous, p, input))
                        })
                };
                let before = previous_bindings()
                    .find(|before| before.declaration_revision == now.declaration_revision)
                    .or_else(|| previous_bindings().next());
                if let Some(before) = before {
                    let reason = if before.declaration_revision != now.declaration_revision {
                        "declaration_mismatch"
                    } else if before.authorization_generation != now.authorization_generation {
                        "authorization_mismatch"
                    } else {
                        continue;
                    };
                    heads.push((
                        key(&now),
                        InputHead {
                            binding: now,
                            revision: token(),
                            observation_floor: None,
                            value: None,
                            unavailable_reason: Some(reason.into()),
                        },
                    ));
                }
            }
        }
    }
    for (key, head) in heads {
        runtime.channels.entry(key).or_insert(Channel {
            head: Some(head),
            last_attempt: None,
        });
    }
}

pub(super) fn selected_binding(
    runtime: &Runtime,
    config: &Config,
    provider: &str,
    input: &str,
    loaded: bool,
) -> Result<Binding> {
    let view = if loaded {
        runtime.loaded.as_ref()
    } else {
        Some(&runtime.saved)
    }
    .filter(|v| signature(&v.config) == signature(config))
    .ok_or_else(|| Error::new("inputs_changed", "Configuration binding is not current"))?;
    binding(view, runtime, provider, input)
        .ok_or_else(|| Error::new("unresolved_reference", "Input declaration does not resolve"))
}

pub(super) fn guard_reason(
    captured: &Binding,
    current: Option<&Binding>,
    before: &str,
    after: &str,
) -> Option<&'static str> {
    match current {
        None => Some("declaration_removed"),
        Some(now) if now.provider_identity != captured.provider_identity => {
            Some("declaration_removed")
        }
        Some(now) if now.authorization_generation != captured.authorization_generation => {
            Some("authorization_changed")
        }
        Some(now) if now.declaration_revision != captured.declaration_revision => {
            Some("declaration_changed")
        }
        Some(_) if after != before => Some("input_changed"),
        _ => None,
    }
}
pub(super) fn current_saved_binding(
    store: &Store,
    runtime: &Runtime,
    captured: &Binding,
) -> Result<Option<Binding>> {
    let saved = config::load(store)?;
    let view = project_view(&runtime.saved, &saved.config);
    Ok(view
        .providers
        .iter()
        .find(|(_, ids)| ids.identity == captured.provider_identity)
        .and_then(|(provider, _)| binding(&view, runtime, provider, &captured.input)))
}

#[cfg(test)]
mod tests {
    use super::super::{
        exact::instant,
        lifecycle::reconcile,
        model::revision,
        store::read_runtime,
        test_support::{fixture, retain_value, save_intent},
        value::publish,
    };
    use super::*;
    #[test]
    fn provider_scope_binding_does_not_require_models_and_rejects_stale_authorization() {
        let (_directory, store, config) = fixture();
        let mut no_models = config.clone();
        let provider = no_models.providers.get_mut("p").unwrap();
        provider.models.clear();
        provider.inputs.clear();
        save_intent(&store, &no_models);
        reconcile(&store, &config, &no_models, &[]).unwrap();
        let mut runtime = read_runtime(&store).unwrap();
        let scope = provider_scope_binding(&runtime, &no_models, "p").unwrap();
        assert_eq!(scope.0, runtime.saved.providers["p"].identity);
        runtime
            .authorization
            .insert(scope.0, "replaced-generation".into());
        assert!(provider_scope_binding_in_view(&runtime, &runtime.saved, "p").is_none());
    }

    #[test]
    fn saved_publication_guard_follows_explicit_rename_but_not_recreated_provider() {
        let (_directory, store, config) = fixture();
        let head_revision = retain_value(&store, &config);
        let runtime = read_runtime(&store).unwrap();
        let captured = super::binding(&runtime.saved, &runtime, "p", "i").unwrap();
        let original = runtime.channels[&key(&captured)]
            .head
            .as_ref()
            .unwrap()
            .value
            .clone()
            .unwrap();
        let mut renamed = config.clone();
        let provider = renamed.providers.remove("p").unwrap();
        renamed.providers.insert("renamed".into(), provider);
        save_intent(&store, &renamed);
        reconcile(&store, &config, &renamed, &[("p".into(), "renamed".into())]).unwrap();
        let mut runtime = read_runtime(&store).unwrap();
        let current = current_saved_binding(&store, &runtime, &captured)
            .unwrap()
            .unwrap();
        assert_eq!(current.provider, "renamed");
        let after = revision(runtime.channels.get(&key(&current)));
        assert_eq!(
            guard_reason(&captured, Some(&current), &head_revision, after),
            None
        );
        let mut retry = original.clone();
        retry.received_at = "2026-10-23T10:10:00Z".into();
        let channel = runtime.channels.get_mut(&key(&current)).unwrap();
        assert!(
            !publish(
                channel,
                &current,
                &renamed.providers["renamed"].inputs["i"],
                retry,
                instant("2026-10-23T10:10:00Z").unwrap()
            )
            .unwrap()
        );
        let retained = channel.head.as_ref().unwrap().value.as_ref().unwrap();
        assert_eq!(retained.observed_at, original.observed_at);
        assert_eq!(retained.received_at, original.received_at);
        assert_eq!(retained.valid_until, original.valid_until);
        save_intent(&store, &Config::empty());
        reconcile(&store, &renamed, &Config::empty(), &[]).unwrap();
        save_intent(&store, &renamed);
        reconcile(&store, &Config::empty(), &renamed, &[]).unwrap();
        let runtime = read_runtime(&store).unwrap();
        let current = current_saved_binding(&store, &runtime, &captured).unwrap();
        assert!(current.is_none());
        assert_eq!(
            guard_reason(&captured, current.as_ref(), &head_revision, "absent"),
            Some("declaration_removed")
        );
    }
}
