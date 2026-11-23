use super::arguments::{arg, args, operand};
use crate::{
    Error, Request, ResourceRef, Result,
    config::{Auth, Config, qualified},
};

/// Dependencies outside a resource's explicitly requested containment boundary.
pub(super) fn guard(config: &Config, request: &Request) -> Result<()> {
    let path = request.command.join(" ");
    if !matches!(
        path.as_str(),
        "provider remove"
            | "model remove"
            | "model alias remove"
            | "agent remove"
            | "agent provider remove"
            | "provider input remove"
            | "agent model remove"
            | "agent model clear"
            | "model alias update"
    ) {
        return Ok(());
    }
    let id = operand(request, 0)?;
    let mut refs = Vec::new();
    let mut add = |kind: &str, name: String, scope: Option<String>| {
        refs.push(ResourceRef {
            kind: kind.into(),
            id: Some(name),
            scope,
        });
    };
    let concrete = if path == "model remove" {
        Some(qualified(id)?)
    } else {
        None
    };
    for alias in &config.model_aliases {
        for candidate in &alias.candidates {
            let relevant = if path == "provider remove" {
                qualified(candidate)?.0 == id
            } else {
                path == "model remove" && candidate == id
            };
            if relevant {
                add(
                    "model_alias",
                    alias.id.clone(),
                    Some(alias.protocols.join(",")),
                );
                break;
            }
        }
    }
    if path == "model alias update" {
        // Narrowing declared protocols or disabling a route leaves relayed
        // selections that can no longer resolve; name them before the save.
        let disabled = args(request, "set")
            .chunks(2)
            .any(|pair| pair == ["enabled", "false"]);
        let declared = args(request, "protocol");
        let removed = config
            .model_aliases
            .iter()
            .find(|alias| alias.id == id)
            .map(|alias| {
                alias
                    .protocols
                    .iter()
                    .filter(|protocol| {
                        !declared.is_empty() && !declared.iter().any(|value| value == *protocol)
                    })
                    .map(String::as_str)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let affects = |protocol: &str| disabled || removed.contains(&protocol);
        if !disabled && removed.is_empty() {
            // Nothing narrows the published surface.
        } else {
            for (target_id, target) in &config.agents {
                for (name, entry) in &target.providers {
                    if !entry.is_relayed(config) {
                        continue;
                    }
                    for selected in &entry.models {
                        if selected.id == id && affects(&selected.protocol) {
                            add(
                                "agent_model",
                                selected.id.clone(),
                                Some(format!("{target_id}/{name}")),
                            );
                        }
                    }
                }
                let relayed = |provider: &str| {
                    target
                        .providers
                        .get(provider)
                        .is_some_and(|entry| entry.is_relayed(config))
                };
                if let Some(reference) = &target.default
                    && reference.id == id
                    && relayed(&reference.provider)
                    && affects(&reference.protocol)
                {
                    add(
                        "agent_default",
                        target_id.clone(),
                        Some(reference.provider.clone()),
                    );
                }
                for (role, reference) in &target.roles {
                    if reference.id == id
                        && relayed(&reference.provider)
                        && affects(&reference.protocol)
                    {
                        add("agent_role", role.clone(), Some(target_id.clone()));
                    }
                }
            }
        }
    }
    for (target_id, target) in &config.agents {
        for (name, entry) in &target.providers {
            if path == "provider remove" && entry.provider == *id {
                add("agent_provider", name.clone(), Some(target_id.clone()));
            }
            for selected in &entry.models {
                let direct = Some(entry.provider.as_str());
                let relevant = match path.as_str() {
                    "provider remove" => {
                        entry.is_relayed(config)
                            && qualified(&selected.id)
                                .ok()
                                .is_some_and(|(provider, _)| provider == id)
                    }
                    "model remove" => {
                        concrete.is_some_and(|(p, m)| direct == Some(p) && selected.id == m)
                            || (entry.is_relayed(config) && selected.id == id)
                    }
                    "model alias remove" => entry.is_relayed(config) && selected.id == id,
                    _ => false,
                };
                if relevant {
                    add(
                        "agent_model",
                        selected.id.clone(),
                        Some(format!("{target_id}/{name}")),
                    );
                }
            }
        }
        if path == "model alias remove" {
            let relayed = |entry: &str| {
                target
                    .providers
                    .get(entry)
                    .is_some_and(|entry| entry.is_relayed(config))
            };
            if let Some(reference) = &target.default
                && reference.id == id
                && relayed(&reference.provider)
            {
                add(
                    "agent_default",
                    target_id.clone(),
                    Some(reference.provider.clone()),
                );
            }
            for (role, reference) in &target.roles {
                if reference.id == id && relayed(&reference.provider) {
                    add("agent_role", role.clone(), Some(target_id.clone()));
                }
            }
        }
        if matches!(
            path.as_str(),
            "agent provider remove" | "agent model remove" | "agent model clear"
        ) && target_id == id
        {
            let entry = operand(request, 1)?;
            let dependent = |provider: &str, id: &str, protocol: &str| {
                provider == entry
                    && (path != "agent model remove"
                        || (request.operands.get(2).map(String::as_str) == Some(id)
                            && arg(request, "protocol") == Some(protocol)))
            };
            if target.default.as_ref().is_some_and(|reference| {
                dependent(&reference.provider, &reference.id, &reference.protocol)
            }) {
                add("agent_default", target_id.clone(), Some(entry.into()));
            }
            for (role, reference) in &target.roles {
                if dependent(&reference.provider, &reference.id, &reference.protocol) {
                    add("agent_role", role.clone(), Some(target_id.clone()));
                }
            }
        }
    }
    for (provider_id, provider) in &config.providers {
        // The derived provider is not a stored resource; it cannot block a deletion.
        if config.is_gateway_provider(provider_id) {
            continue;
        }
        if path == "agent remove" && matches!(&provider.auth,Some(Auth::Login{via,..}) if via==id) {
            add(
                "provider_authentication",
                provider_id.clone(),
                Some("login_source".into()),
            );
        }
        if let Some((p, m)) = concrete
            && provider_id == p
        {
            for (name, input) in &provider.inputs {
                if input.models.iter().any(|covered| covered == m) {
                    add("provider_input", name.clone(), Some(provider_id.clone()));
                }
            }
        }
        if path == "provider input remove" && provider_id == id {
            let input = operand(request, 1)?;
            for (model_id, model) in &provider.models {
                for rule in &model.priority_rules {
                    if rule.input == input {
                        add(
                            "priority_rule",
                            rule.id.clone(),
                            Some(format!("{provider_id}/{model_id}")),
                        );
                    }
                }
            }
        }
    }
    if refs.is_empty() {
        return Ok(());
    }
    refs.sort_by(|a, b| (&a.kind, &a.id, &a.scope).cmp(&(&b.kind, &b.id, &b.scope)));
    refs.dedup();
    let mut error = Error::new(
        "unresolved_reference",
        "Explicitly remove or replace these dependent settings before removing the requested resource",
    );
    error.affected = refs;
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reports_external_alias_but_allows_contained_models() {
        let mut config = Config::empty();
        let mut provider = crate::config::Provider {
            integration: Some("deepseek".into()),
            ..Default::default()
        };
        provider.models.insert("model".into(), Default::default());
        config.providers.insert("p".into(), provider);
        let request = Request {
            command: vec!["provider".into(), "remove".into()],
            operands: vec!["p".into()],
            arguments: Default::default(),
            dry_run: false,
            if_revision: None,
        };
        guard(&config, &request).unwrap();
        config.model_aliases.push(crate::config::ModelAlias {
            candidate_reasoning_effort: Default::default(),
            id: "alias".into(),
            protocols: vec!["chat-completions".into()],
            candidates: vec!["p/model".into()],
            enabled: true,
            continuity: None,
            fallback: None,
        });
        let error = guard(&config, &request).unwrap_err();
        assert_eq!(
            error.affected,
            [ResourceRef {
                kind: "model_alias".into(),
                id: Some("alias".into()),
                scope: Some("chat-completions".into())
            }]
        );
    }

    #[test]
    fn relayed_entries_and_their_defaults_block_route_removal() {
        let mut config = Config::empty();
        config.gateway = Some(crate::config::Gateway {
            listen: "127.0.0.1:17840".into(),
            key_ref: "secret:gw".into(),
            autostart: false,
        });
        let mut provider = crate::config::Provider {
            integration: Some("custom".into()),
            auth: Some(Auth::ApiKey {
                key_ref: "secret:up".into(),
            }),
            endpoints: [("responses".into(), "http://127.0.0.1:39999".into())].into(),
            ..Default::default()
        };
        provider.models.insert("route".into(), Default::default());
        config.providers.insert("up".into(), provider);
        config.model_aliases.push(crate::config::ModelAlias {
            candidate_reasoning_effort: Default::default(),
            id: "route".into(),
            protocols: vec!["responses".into()],
            candidates: vec!["up/route".into()],
            enabled: true,
            continuity: None,
            fallback: None,
        });
        crate::config::apply_gateway_provider(&mut config);
        config.agents.insert(
            "target".into(),
            crate::config::AgentTarget {
                agent: "omp".into(),
                scope: "/native/omp".into(),
                providers: [(
                    "tmax".into(),
                    crate::config::NativeProvider {
                        provider: crate::config::GATEWAY_PROVIDER_ID.into(),
                        models: vec![crate::config::ModelSelection {
                            id: "route".into(),
                            protocol: "responses".into(),
                        }],
                    },
                )]
                .into(),
                default: Some(crate::config::DefaultReference {
                    provider: "tmax".into(),
                    id: "route".into(),
                    protocol: "responses".into(),
                    effort: None,
                    native_effort: None,
                }),
                roles: Default::default(),
            },
        );
        let request = Request {
            command: vec!["model".into(), "alias".into(), "remove".into()],
            operands: vec!["route".into()],
            arguments: Default::default(),
            dry_run: false,
            if_revision: None,
        };
        let error = guard(&config, &request).unwrap_err();
        assert_eq!(error.code, "unresolved_reference");
        assert_eq!(
            error.affected,
            [
                ResourceRef {
                    kind: "agent_default".into(),
                    id: Some("target".into()),
                    scope: Some("tmax".into())
                },
                ResourceRef {
                    kind: "agent_model".into(),
                    id: Some("route".into()),
                    scope: Some("target/tmax".into())
                },
            ]
        );
    }

    fn alias_update_config() -> Config {
        let mut config = Config::empty();
        config.gateway = Some(crate::config::Gateway {
            listen: "127.0.0.1:17840".into(),
            key_ref: "secret:gw".into(),
            autostart: false,
        });
        let mut provider = crate::config::Provider {
            integration: Some("custom".into()),
            ..Default::default()
        };
        provider
            .endpoints
            .insert("responses".into(), "http://127.0.0.1:39999".into());
        provider.models.insert("route".into(), Default::default());
        config.providers.insert("up".into(), provider);
        crate::config::apply_gateway_provider(&mut config);
        config.model_aliases.push(crate::config::ModelAlias {
            candidate_reasoning_effort: Default::default(),
            id: "route".into(),
            protocols: vec!["responses".into()],
            candidates: vec!["up/route".into()],
            enabled: true,
            continuity: Some("stateless".into()),
            fallback: None,
        });
        config.agents.insert(
            "pi-main".into(),
            crate::config::AgentTarget {
                agent: "pi".into(),
                scope: "/native/pi-main".into(),
                providers: [(
                    "relay".into(),
                    crate::config::NativeProvider {
                        provider: crate::config::GATEWAY_PROVIDER_ID.into(),
                        models: vec![crate::config::ModelSelection {
                            id: "route".into(),
                            protocol: "responses".into(),
                        }],
                    },
                )]
                .into(),
                default: None,
                roles: Default::default(),
            },
        );
        config
    }

    #[test]
    fn alias_update_names_relayed_selections_before_narrowing_or_disabling() {
        let config = alias_update_config();
        let narrow = Request {
            command: vec!["model".into(), "alias".into(), "update".into()],
            operands: vec!["route".into()],
            arguments: [("protocol".into(), vec!["chat-completions".into()])].into(),
            dry_run: false,
            if_revision: None,
        };
        let error = guard(&config, &narrow).unwrap_err();
        assert_eq!(error.code, "unresolved_reference");
        assert!(
            error
                .affected
                .iter()
                .any(|reference| reference.kind == "agent_model"),
            "{error:?}"
        );
        // Keeping the protocol, or changing an unrelated field, is allowed.
        let keep = Request {
            command: vec!["model".into(), "alias".into(), "update".into()],
            operands: vec!["route".into()],
            arguments: [("protocol".into(), vec!["responses".into()])].into(),
            dry_run: false,
            if_revision: None,
        };
        guard(&config, &keep).unwrap();
        let disable = Request {
            command: vec!["model".into(), "alias".into(), "update".into()],
            operands: vec!["route".into()],
            arguments: [("set".into(), vec!["enabled".into(), "false".into()])].into(),
            dry_run: false,
            if_revision: None,
        };
        let error = guard(&config, &disable).unwrap_err();
        assert_eq!(error.code, "unresolved_reference");
    }
}
