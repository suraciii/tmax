use super::arguments::{arg, operand, required};
use crate::{
    Error, Request, Result,
    config::{self, Auth, Config},
    storage::Store,
};
use serde_json::{Value, json};

pub(super) async fn selected(store: &Store, r: &Request) -> Result<Option<config::LoadedFile>> {
    if arg(r, "state").unwrap_or("file") == "loaded" {
        if let Some(snapshot) = crate::service::loaded(store).await? {
            return Ok(Some(snapshot));
        }
        return Ok(None);
    }
    Ok(Some(config::load(store)?))
}
pub(super) fn model_list(config: &Config, r: &Request) -> Result<Value> {
    let mut entries = Vec::new();
    if arg(r, "kind") != Some("alias") {
        for (id, p) in &config.providers {
            if arg(r, "provider").is_some_and(|v| v != id) {
                continue;
            }
            for (mid, m) in &p.models {
                let protocols = crate::provider_integration::protocols(p)?;
                if arg(r, "protocol").is_some_and(|proto| !protocols.iter().any(|p| p == proto)) {
                    continue;
                }
                entries.push(json!({"id":format!("{id}/{mid}"),"kind":"concrete","enabled":p.enabled&&m.enabled,"published_protocols":if p.enabled&&m.enabled{protocols}else{Vec::new()},"availability":if p.auth.is_none(){"authentication_missing"}else{"unverified"}}));
            }
        }
    }
    if arg(r, "kind") != Some("concrete") && arg(r, "provider").is_none() {
        for a in &config.model_aliases {
            if arg(r, "protocol").is_some_and(|p| p != a.protocol) {
                continue;
            }
            entries.push(json!({"id":a.id,"protocol":a.protocol,"kind":"alias","enabled":a.enabled,"published_protocols":if a.enabled{vec![a.protocol.as_str()]}else{Vec::new()},"continuity":a.continuity.as_deref().unwrap_or("session")}));
        }
    }
    Ok(json!({"models":entries}))
}
pub(crate) async fn explain(store: &Store, c: &Config, r: &Request) -> Result<Value> {
    let loaded = if arg(r, "state") == Some("loaded") {
        Some(
            crate::service::loaded_with_metadata(store)
                .await?
                .ok_or_else(|| {
                    Error::new(
                        "runtime_unavailable",
                        "There is no loaded configuration to explain",
                    )
                })?,
        )
    } else {
        None
    };
    // The target, candidates and accepted metadata must belong to the same view.
    let c = loaded.as_ref().map(|(file, _)| &file.config).unwrap_or(c);
    let config = c;
    let mut id = operand(r, 0)?.to_owned();
    let protocol = required(r, "protocol")?;
    let target = arg(r, "target");
    let agent = arg(r, "agent");
    let path = arg(r, "path");
    if target.is_some() && (agent.is_some() || path.is_some())
        || agent.is_some() != path.is_some()
        || arg(r, "agent-provider").is_some() && target.is_none()
    {
        return Err(Error::new(
            "invalid_arguments",
            "Select a declared target with an optional provider entry, or an agent and native path",
        ));
    }
    let context = if let Some(target) = target {
        let t = c
            .agents
            .get(target)
            .ok_or_else(|| Error::new("unknown_resource", "Target is not configured"))?;
        if let Some(name) = arg(r, "agent-provider") {
            let e = t.providers.get(name).ok_or_else(|| {
                Error::new("unknown_resource", "Native provider entry not configured")
            })?;
            if let Some(provider) = &e.provider {
                id = format!("{provider}/{id}");
            }
            json!({"kind":"agent_provider","target":target,"agent_provider":name,"agent":t.agent,"path":t.scope})
        } else {
            json!({"kind":"agent_target","target":target,"agent_provider":null,"agent":t.agent,"path":t.scope})
        }
    } else if let (Some(agent), Some(path)) = (agent, path) {
        crate::agent_integration::discover(store, Some(agent), Some(path)).await?;
        json!({"kind":"native_scope","target":null,"agent_provider":null,"agent":agent,"path":path})
    } else {
        json!({"kind":"gateway","target":null,"agent_provider":null,"agent":null,"path":null})
    };
    let entry = context["target"]
        .as_str()
        .zip(context["agent_provider"].as_str())
        .and_then(|(target, name)| c.agents.get(target)?.providers.get(name));
    if arg(r, "session").is_some() {
        if entry.is_some_and(|entry| entry.provider.is_some()) {
            return Err(Error::new(
                "invalid_arguments",
                "A retained gateway session cannot be inspected through an upstream-source entry",
            ));
        }
        let mut data = crate::service::control(store, config, r).await?;
        data["context"] = context;
        data["requested"] = json!({"id":r.operands.first(),"protocol":protocol});
        data["native_assessment"] = json!({"state":"not_reassessed","reason":"retained_destination_inspection","authorization":"not_verified"});
        return Ok(data);
    }
    let requirements =
        context["agent"]
            .as_str()
            .map(|agent| crate::agent_integration::compatibility::Context {
                agent,
                target: context["target"].as_str(),
                provider: entry.and_then(|e| e.provider.as_deref()),
                gateway: entry.is_some_and(|e| e.gateway == Some(true)),
            });
    let advice = crate::routing::Advice {
        native: requirements,
    };
    let mut data = if let Some((_, metadata)) = &loaded {
        crate::routing::explain_loaded(
            store,
            config,
            &id,
            protocol,
            arg(r, "at"),
            metadata.as_ref(),
            Some(&advice),
        )?
    } else {
        crate::routing::explain_native(store, config, &id, protocol, arg(r, "at"), Some(&advice))?
    };
    if requirements.is_some() {
        data["native_assessment"] = json!({"state":"assessed","scope":"protocol_provider_client_and_credential_handoff","native_login_readiness":"safe_operation_metadata_checked_for_direct_login","installation_and_authorization":"not_verified","ranking_is_live_authorization_proof":false,"secret_reads":false,"native_apply":"not_requested"});
    }
    data["context"] = context;
    data["requested"] = json!({"id":r.operands.first(),"protocol":protocol});
    Ok(data)
}
pub(crate) fn network_view(c: &Config, revision: &str, exists: bool, state: &str) -> Value {
    let n = c.network.as_ref();
    let mode = n.map(|n| n.mode.as_str()).unwrap_or("direct");
    let u = n
        .and_then(|n| n.proxy_url.as_ref())
        .and_then(|u| url::Url::parse(u).ok());
    json!({"state":state,"snapshot_state":if exists{"available"}else{"absent"},"policy":{"snapshot_state":if exists{"available"}else{"absent"},"configuration_revision":revision,"configured":n,"effective":{"mode":mode,"proxy_url":u.as_ref().map(url::Url::as_str),"proxy_scheme":u.as_ref().map(url::Url::scheme),"proxy_host":u.as_ref().and_then(url::Url::host_str),"proxy_port":u.as_ref().and_then(|u|u.port().or_else(||match u.scheme(){"http"=>Some(80),"https"=>Some(443),"socks5"=>Some(1080),_=>None}))},"reason_code":if n.is_none(){json!("default_direct")}else{Value::Null}},"future_work_scope":if state=="loaded"{"resident"}else{"independent_cli"}})
}

pub(super) fn provider_list(store: &Store, file: &config::LoadedFile) -> Result<Value> {
    let config = &file.config;
    let providers = config.providers.iter().map(|(id,p)| {
            let observations = crate::observations::summary(store, config, id)
                .unwrap_or_else(|error| json!({"state":"unavailable","reason_code":error.code}));
            json!({"id":id,"name":p.overrides.name,"enabled":p.enabled,"authentication":match p.auth{Some(Auth::ApiKey{..})=>"api-key",Some(Auth::Login{..})=>"login",None=>"not_configured"},"subscription":p.subscription.as_ref().and_then(|s|s.plan.as_ref()),"observations":observations,"affected_targets":config.agents.values().filter(|t|t.providers.values().any(|e|e.provider.as_ref()==Some(id))).count()})
        }).collect::<Vec<_>>();
    Ok(json!({"providers":providers,"revision":file.revision}))
}

pub(super) async fn provider_inspect(
    store: &Store,
    file: &config::LoadedFile,
    r: &Request,
) -> Result<Value> {
    let config = &file.config;
    let id = operand(r, 0)?;
    let p = config
        .providers
        .get(id)
        .ok_or_else(|| Error::new("unknown_resource", "Provider is not configured"))?;
    let loaded = crate::service::loaded(store).await;
    let endpoint = |c: &Config, revision: &str| -> Value {
        match c.providers.get(id) {
            Some(p) => {
                json!({"snapshot_state":"available","configuration_revision":revision,"configured":{"region":p.region,"endpoints":p.endpoints},"resolution":crate::provider_integration::endpoint_view(p).ok(),"reason_code":null})
            }
            None => {
                json!({"snapshot_state":"available","configuration_revision":revision,"configured":null,"resolution":null,"reason_code":"provider_missing"})
            }
        }
    };
    let loaded_endpoint = match loaded {
        Ok(Some(loaded)) => endpoint(&loaded.config, &loaded.revision),
        Ok(None) => {
            json!({"snapshot_state":"absent","configuration_revision":null,"configured":null,"resolution":null,"reason_code":"no_loaded_snapshot"})
        }
        Err(error) => {
            json!({"snapshot_state":"unavailable","configuration_revision":null,"configured":null,"resolution":null,"reason_code":error.code})
        }
    };
    let observations = crate::observations::inspect(store, config, id)
        .unwrap_or_else(|error| json!({"state":"unavailable","reason_code":error.code}));
    Ok(
        json!({"id":id,"provider":p,"metadata":crate::catalog::effective(store,p,None)?,"endpoints":{"file":endpoint(config,&file.revision),"loaded":loaded_endpoint},"identity":null,"upstream_authentication":"unverified","observations":observations,"renewal":p.subscription.as_ref().and_then(|s|s.renewal_date),"via":match &p.auth{Some(Auth::Login{via,..})=>Some(via),_=>None}}),
    )
}

pub(super) fn model_inspect(
    store: &Store,
    file: &config::LoadedFile,
    r: &Request,
) -> Result<Value> {
    let config = &file.config;
    let id = operand(r, 0)?;
    if let Ok((p, m)) = config::model(config, id) {
        let metadata = crate::catalog::effective(store, p, Some((config::qualified(id)?.1, m)))?;
        let protocols = crate::provider_integration::protocols(p)?;
        let native_projection =
            crate::agent_integration::reasoning_projection(&metadata["effective"], &protocols);
        return Ok(
            json!({"kind":"concrete","qualified_model":id,"configuration":m,"metadata":metadata,"native_projection":native_projection,"protocols":protocols,"revision":file.revision,"priority_evaluation":crate::routing::explain(store,config,id,arg(r,"protocol").unwrap_or("chat-completions"),None)?}),
        );
    }
    let aliases: Vec<_> = config
        .model_aliases
        .iter()
        .filter(|a| a.id == id && arg(r, "protocol").is_none_or(|p| p == a.protocol))
        .collect();
    if aliases.len() != 1 {
        return Err(Error::new(
            "invalid_arguments",
            "Alias identity is unknown or ambiguous; supply its protocol",
        ));
    }
    Ok(
        json!({"kind":"alias","definition":aliases[0],"continuity":aliases[0].continuity.as_deref().unwrap_or("session"),"revision":file.revision}),
    )
}

pub(super) fn model_rules(
    store: &Store,
    config: &Config,
    r: &Request,
    path: &str,
) -> Result<Value> {
    let id = operand(r, 0)?;
    let (_, m) = config::model(config, id)?;
    let evaluation = crate::routing::explain(
        store,
        config,
        id,
        arg(r, "protocol").unwrap_or("chat-completions"),
        arg(r, "at"),
    )?;
    if path.ends_with("inspect") {
        let rule = m
            .priority_rules
            .iter()
            .find(|rule| rule.id == r.operands[1])
            .ok_or_else(|| Error::new("unknown_resource", "Priority rule is not configured"))?;
        return Ok(
            json!({"qualified_model":id,"state":arg(r,"state").unwrap_or("file"),"rule":rule,"priority_evaluation":evaluation}),
        );
    }
    Ok(json!({"qualified_model":id,"rules":m.priority_rules,"priority_evaluation":evaluation}))
}

pub(super) fn agent_list(config: &Config) -> Result<Value> {
    Ok(
        json!({"agents":config.agents.iter().map(|(id,t)|json!({"target":id,"agent":t.agent,"path":t.scope,"provider_count":t.providers.len(),"model_count":t.providers.values().map(|e|e.models.len()).sum::<usize>(),"default_configured":t.default.is_some(),"native_application":"not_verified"})).collect::<Vec<_>>()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn incomplete_native_entry_and_alias_advice_exclude_the_same_protocol_that_apply_rejects()
    {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("state"));
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("custom".into()),
                endpoints: [(
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                )]
                .into(),
                auth: Some(Auth::ApiKey {
                    key_ref: "not-resolved".into(),
                }),
                models: [
                    ("m".into(), config::Model::default()),
                    ("org/model".into(), config::Model::default()),
                ]
                .into(),
                ..Default::default()
            },
        );
        config.model_aliases.push(config::ModelAlias {
            candidate_reasoning_effort: Default::default(),
            id: "custom-name".into(),
            protocol: "chat-completions".into(),
            candidates: vec!["p/m".into()],
            enabled: true,
            continuity: None,
        });
        config.agents.insert(
            "target".into(),
            config::AgentTarget {
                agent: "codex".into(),
                scope: directory.path().join("native"),
                providers: [(
                    "entry".into(),
                    config::NativeProvider {
                        provider: Some("p".into()),
                        gateway: None,
                        models: vec![],
                    },
                )]
                .into(),
                default: None,
                roles: Default::default(),
            },
        );
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        crate::inputs::reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        let original = std::fs::read(store.root.join("tmax.toml")).unwrap();
        for (id, entry, qualified) in [
            ("m", Some("entry"), "p/m"),
            ("org/model", Some("entry"), "p/org/model"),
            ("custom-name", None, "p/m"),
            ("p/m", None, "p/m"),
        ] {
            let mut arguments = std::collections::BTreeMap::from([
                ("protocol".into(), vec!["chat-completions".into()]),
                ("target".into(), vec!["target".into()]),
            ]);
            if let Some(entry) = entry {
                arguments.insert("agent-provider".into(), vec![entry.into()]);
            }
            let request = Request {
                command: vec!["model".into(), "explain".into()],
                operands: vec![id.into()],
                arguments,
                dry_run: false,
                if_revision: None,
            };
            let advice = explain(&store, &config, &request).await.unwrap();
            assert!(advice["selection"].is_null());
            assert_eq!(
                advice["excluded_candidates"][0]["qualified_model"],
                qualified
            );
            assert_eq!(
                advice["excluded_candidates"][0]["reason_code"],
                "incompatible"
            );
            assert!(
                advice["excluded_candidates"][0]["native_requirement"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("Responses")
            );
        }
        assert_eq!(
            std::fs::read(store.root.join("tmax.toml")).unwrap(),
            original
        );
        assert!(!directory.path().join("native").exists());
        let selection = config::ModelSelection {
            id: "m".into(),
            protocol: "chat-completions".into(),
        };
        let target = config.agents.get_mut("target").unwrap();
        target
            .providers
            .get_mut("entry")
            .unwrap()
            .models
            .push(selection);
        target.default = Some(config::ModelReference {
            provider: "entry".into(),
            id: "m".into(),
            protocol: "chat-completions".into(),
        });
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        std::fs::create_dir(directory.path().join("native")).unwrap();
        assert_eq!(
            crate::agent_integration::apply(&store, &config, "target", true)
                .await
                .unwrap_err()
                .code,
            "incompatible"
        );
    }

    #[tokio::test]
    async fn ambiguous_native_explanation_scopes_fail_without_side_effects() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("absent"));
        for selectors in [
            vec![
                ("target", "target"),
                ("agent", "pi"),
                ("path", "/unused/native"),
            ],
            vec![("agent-provider", "entry")],
            vec![("agent", "pi")],
            vec![("path", "/unused/native")],
        ] {
            let mut arguments = std::collections::BTreeMap::from([(
                "protocol".into(),
                vec!["chat-completions".into()],
            )]);
            arguments.extend(
                selectors
                    .into_iter()
                    .map(|(key, value)| (key.into(), vec![value.into()])),
            );
            let request = Request {
                command: vec!["model".into(), "explain".into()],
                operands: vec!["custom-name".into()],
                arguments,
                dry_run: false,
                if_revision: None,
            };
            assert_eq!(
                explain(&store, &Config::empty(), &request)
                    .await
                    .unwrap_err()
                    .code,
                "invalid_arguments"
            );
            assert!(!store.root.exists());
        }
    }
}
