use super::{arg, operand};
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
                    Some(alias.protocol.clone()),
                );
                break;
            }
        }
    }
    for (target_id, target) in &config.agents {
        for (name, entry) in &target.providers {
            if path == "provider remove" && entry.provider.as_deref() == Some(id) {
                add("agent_provider", name.clone(), Some(target_id.clone()));
            }
            for selected in &entry.models {
                let direct = entry.provider.as_deref();
                let relevant = match path.as_str() {
                    "provider remove" => {
                        entry.proxy == Some(true)
                            && qualified(&selected.id)
                                .ok()
                                .is_some_and(|(provider, _)| provider == id)
                    }
                    "model remove" => {
                        concrete.is_some_and(|(p, m)| direct == Some(p) && selected.id == m)
                            || (entry.proxy == Some(true) && selected.id == id)
                    }
                    "model alias remove" => {
                        entry.proxy == Some(true)
                            && selected.id == id
                            && arg(request, "protocol") == Some(selected.protocol.as_str())
                    }
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
        if matches!(
            path.as_str(),
            "agent provider remove" | "agent model remove" | "agent model clear"
        ) && target_id == id
        {
            let entry = operand(request, 1)?;
            let dependent = |reference: &crate::config::ModelReference| {
                reference.provider == entry
                    && (path != "agent model remove"
                        || (request.operands.get(2) == Some(&reference.id)
                            && arg(request, "protocol") == Some(reference.protocol.as_str())))
            };
            if target.default.as_ref().is_some_and(dependent) {
                add("agent_default", target_id.clone(), Some(entry.into()));
            }
            for (role, reference) in &target.roles {
                if dependent(reference) {
                    add("agent_role", role.clone(), Some(target_id.clone()));
                }
            }
        }
    }
    for (provider_id, provider) in &config.providers {
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
            id: "alias".into(),
            protocol: "chat-completions".into(),
            candidates: vec!["p/model".into()],
            enabled: true,
            continuity: None,
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
}
