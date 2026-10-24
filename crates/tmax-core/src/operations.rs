pub mod commands;
mod edit;
mod references;
use crate::{
    Error, Request, Response, Result,
    config::{self, Auth, Config},
    storage::Store,
};
use serde_json::{Value, json};
pub fn catalog() -> Value {
    commands::catalog()
}
pub async fn execute(request: Request) -> Response {
    let operation = request.command.join(" ");
    let result = async {
        commands::validate(&request)?;
        let store = Store::discover()?;
        dispatch(&store, &request).await
    }
    .await;
    match result {
        Ok(data) => success_response(&operation, request.dry_run, data),
        Err(e) => error_response(&operation, e, request.dry_run),
    }
}
fn success_response(operation: &str, dry_run: bool, mut data: Value) -> Response {
    let nested_code = data
        .get("_code")
        .and_then(Value::as_str)
        .or_else(|| data.pointer("/activation/_code").and_then(Value::as_str))
        .or_else(|| data.pointer("/autostart/_code").and_then(Value::as_str))
        .map(str::to_string);
    let code = nested_code
        .as_deref()
        .unwrap_or(if dry_run { "dry_run_ready" } else { "ok" });
    let exit = match code {
        "awaiting_native_reload"
        | "restart_required"
        | "runtime_blocked"
        | "resident_incompatible"
        | "active_requests"
        | "inputs_changed"
        | "unsupported_source"
        | "credential_unavailable"
        | "noninteractive_unavailable"
        | "interactive_required"
        | "login_source_required"
        | "target_incomplete" => 3,
        "ok" | "dry_run_ready" | "resident_reused" => 0,
        _ => 4,
    };
    let status = if dry_run && exit != 0 {
        "blocked"
    } else if let Some(status) = data
        .get("_status")
        .and_then(Value::as_str)
        .filter(|s| matches!(*s, "blocked" | "incomplete" | "cancelled"))
    {
        status
    } else if exit != 0 {
        "incomplete"
    } else if dry_run {
        "dry_run"
    } else {
        "success"
    };
    let status = status.to_string();
    if let Some(o) = data.as_object_mut() {
        o.remove("_code");
        o.remove("_status");
    }
    Response {
        exit_code: exit,
        body: json!({"schema_version":1,"operation":operation,"code":code,"status":status,"data":data}),
    }
}
fn error_response(operation: &str, error: Error, dry_run: bool) -> Response {
    let code = error.code.as_str();
    let exit = if matches!(
        code,
        "inputs_changed"
            | "credential_unavailable"
            | "login_source_required"
            | "interactive_required"
            | "unsupported_protected_input"
            | "unsupported_protected_output"
            | "target_incomplete"
            | "resident_incompatible"
            | "resident_stopping"
            | "resident_identity_unavailable"
            | "resident_control_path_invalid"
            | "endpoint_occupied"
            | "endpoint_conflict"
            | "active_requests"
            | "restart_required"
            | "model_unavailable"
            | "catalog_unavailable"
            | "compatibility_unavailable"
            | "acquisition_in_progress"
            | "recovery_required"
    ) {
        3
    } else if code == "cancelled" {
        5
    } else if code.starts_with("invalid")
        || matches!(
            code,
            "unresolved_reference" | "unknown_resource" | "unsupported_operation"
        )
    {
        2
    } else {
        4
    };
    Response {
        exit_code: exit,
        body: json!({"schema_version":1,"operation":operation,"code":code,"status":if exit==3{"blocked"}else if exit==5{"cancelled"}else{"error"},"data":if error.field.is_some(){json!({"field":error.field})}else{Value::Null},"blockers":[{"code":code,"message":error.message,"affected":error.affected}],"dry_run":dry_run}),
    }
}
pub(crate) fn arg<'a>(r: &'a Request, key: &str) -> Option<&'a str> {
    r.arguments
        .get(key)
        .and_then(|v| v.first())
        .map(String::as_str)
}
pub(crate) fn args<'a>(r: &'a Request, key: &str) -> &'a [String] {
    r.arguments.get(key).map(Vec::as_slice).unwrap_or(&[])
}
pub(crate) fn operand(r: &Request, n: usize) -> Result<&str> {
    r.operands.get(n).map(String::as_str).ok_or_else(|| {
        Error::new(
            "invalid_arguments",
            "A required positional operand is missing",
        )
    })
}
pub(crate) fn required<'a>(r: &'a Request, key: &str) -> Result<&'a str> {
    arg(r, key).ok_or_else(|| {
        Error::new("invalid_arguments", "A required named option is missing").field(key)
    })
}
pub(crate) fn boolean(s: &str) -> Result<bool> {
    match s {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(Error::new(
            "invalid_arguments",
            "Expected exactly true or false",
        )),
    }
}
pub(crate) fn duration(s: &str) -> Result<i64> {
    let unit = s.as_bytes().last().copied().ok_or_else(|| {
        Error::new(
            "invalid_arguments",
            "Duration requires a positive integer and unit",
        )
    })?;
    let factor = match unit {
        b's' => 1,
        b'm' => 60,
        b'h' => 3600,
        b'd' => 86400,
        _ => {
            return Err(Error::new(
                "invalid_arguments",
                "Duration unit must be s, m, h, or d",
            ));
        }
    };
    let digits = &s[..s.len() - 1];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::new(
            "invalid_arguments",
            "Duration requires a positive integer and s, m, h, or d",
        ));
    }
    let n: i64 = digits.parse().map_err(|_| {
        Error::new(
            "invalid_arguments",
            "Duration exceeds the supported integer range",
        )
    })?;
    n.checked_mul(factor).filter(|n| *n > 0).ok_or_else(|| {
        Error::new(
            "invalid_arguments",
            "Duration must be positive and within signed 64-bit seconds",
        )
    })
}
fn unlock_needed(r: &Request) -> Result<Option<zeroize::Zeroizing<Vec<u8>>>> {
    crate::secrets::unlock(r)
}
async fn selected(store: &Store, r: &Request) -> Result<Option<config::LoadedFile>> {
    if arg(r, "state").unwrap_or("file") == "loaded" {
        if let Some(snapshot) = crate::service::loaded(store).await? {
            return Ok(Some(snapshot));
        }
        return Ok(None);
    }
    Ok(Some(config::load(store)?))
}
async fn dispatch(store: &Store, r: &Request) -> Result<Value> {
    let path = r.command.join(" ");
    if path == "integration list" {
        return Ok(json!({"integrations":crate::integrations::registry()}));
    }
    if path == "integration inspect" {
        return crate::integrations::inspect(operand(r, 0)?);
    }
    if path == "secret list" {
        return secret_inventory(store).await;
    }
    if path.starts_with("config recovery ") {
        return recovery(store, r);
    }
    if path == "agent recovery list"
        || path == "agent recovery inspect"
        || path == "agent recovery discard"
    {
        return crate::native::recovery(store, r).await;
    }
    if path == "agent restore" {
        return crate::native::restore_request(store, r).await;
    }
    if path == "agent discover" {
        return crate::native::discover(store, arg(r, "agent"), None).await;
    }
    if path == "agent inspect" && r.operands.is_empty() {
        return crate::native::discover(
            store,
            Some(required(r, "agent")?),
            Some(required(r, "path")?),
        )
        .await;
    }
    if path == "proxy status" || path == "web status" {
        return crate::service::status(store).await;
    }
    if path == "status" {
        let file = config::load(store);
        let resident = crate::service::status(store).await?;
        return Ok(match file {
            Ok(f) => {
                json!({"config_path":store.root.join("tmax.toml"),"file_revision":f.revision,"configuration_state":if f.exists{"valid"}else{"absent"},"providers":f.config.providers.len(),"targets":f.config.agents.len(),"resident":resident})
            }
            Err(e) => {
                json!({"config_path":store.root.join("tmax.toml"),"configuration_state":"invalid","reason_code":e.code,"field":e.field,"resident":resident,"next_action":"config reset or correct supported fields"})
            }
        });
    }
    if path == "config reset" {
        return reset(store, r).await;
    }
    if path == "secret cleanup" {
        return cleanup(store, r).await;
    }
    // Reuse/stop controls consult the identified resident even when the saved file is invalid.
    if matches!(
        path.as_str(),
        "proxy start" | "proxy stop" | "web open" | "web stop"
    ) {
        let config = config::load(store)
            .map(|f| f.config)
            .unwrap_or_else(|_| Config::empty());
        let unlock = if !r.dry_run
            && path == "proxy start"
            && r.arguments
                .keys()
                .any(|k| k == "unlock-stdin" || k == "unlock-fd")
        {
            unlock_needed(r)?
        } else {
            None
        };
        return crate::service::control(store, &config, r, unlock.as_deref().map(Vec::as_slice))
            .await;
    }
    if edit::is_edit(&path) {
        return edit::execute(store, r).await;
    }
    if path == "config reload" {
        let f = config::load(store)?;
        if let Some(expected) = &r.if_revision
            && f.revision != *expected
        {
            return Err(Error::new(
                "inputs_changed",
                "Saved configuration revision differs from the requested guard",
            ));
        }
        if r.dry_run {
            return impact(store, &f.config, &f.revision, None).await;
        }
        if f.exists {
            crate::routing::reconcile(store, &f.config, &f.config, &[])?;
        }
        let autostart = crate::service::reconcile_autostart(store, &f.config, false).await;
        let activation = crate::service::activate(store, &f.config, &f.revision).await;
        let failed = autostart.is_err() || activation.is_err();
        return Ok(
            json!({"_code":if failed{json!("saved_not_active")}else{Value::Null},"file_revision":f.revision,"save":"not_requested","activation":activation.unwrap_or_else(|e|json!({"state":"failed","code":e.code,"message":e.message})),"autostart":autostart.unwrap_or_else(|e|json!({"state":"failed","code":e.code,"message":e.message})),"native_application":"not_requested"}),
        );
    }
    if path.starts_with("provider input ") {
        return match config::load(store) {
            Ok(file) => crate::inputs::handle(store, &file.config, r).await,
            Err(_) if arg(r, "state") == Some("loaded") => {
                crate::inputs::handle_unavailable_saved(store, r).await
            }
            Err(error) => Err(error),
        };
    }
    let Some(file) = selected(store, r).await? else {
        return Ok(
            json!({"state":"loaded","snapshot_state":"absent","reason_code":"no_loaded_snapshot"}),
        );
    };
    let config = &file.config;
    if path == "config show" {
        return Ok(
            json!({"state":arg(r,"state").unwrap_or("file"),"snapshot_state":if file.exists{"available"}else{"absent"},"configuration_revision":file.revision,"path":store.root.join("tmax.toml"),"configuration":config}),
        );
    }
    if path == "config validate" {
        return Ok(
            json!({"snapshot_state":if file.exists{"available"}else{"absent"},"valid":true,"revision":file.revision,"external_availability":"not_verified"}),
        );
    }
    if path.starts_with("catalog ") {
        return crate::catalog::handle(store, config, r).await;
    }
    if path == "provider list" {
        let providers = config.providers.iter().map(|(id,p)| {
            let observations = crate::observations::summary(store, config, id)
                .unwrap_or_else(|error| json!({"state":"unavailable","reason_code":error.code}));
            json!({"id":id,"name":p.overrides.name,"enabled":p.enabled,"authentication":match p.auth{Some(Auth::ApiKey{..})=>"api-key",Some(Auth::Login{..})=>"login",None=>"not_configured"},"subscription":p.subscription.as_ref().and_then(|s|s.plan.as_ref()),"observations":observations,"affected_targets":config.agents.values().filter(|t|t.providers.values().any(|e|e.provider.as_ref()==Some(id))).count()})
        }).collect::<Vec<_>>();
        return Ok(json!({"providers":providers,"revision":file.revision}));
    }
    if path == "provider inspect" {
        let id = operand(r, 0)?;
        let p = config
            .providers
            .get(id)
            .ok_or_else(|| Error::new("unknown_resource", "Provider is not configured"))?;
        let loaded = crate::service::loaded(store).await?;
        let endpoint = |c: &Config, revision: &str| -> Value {
            match c.providers.get(id) {
                Some(p) => {
                    json!({"snapshot_state":"available","configuration_revision":revision,"configured":{"region":p.region,"endpoints":p.endpoints},"resolution":crate::integrations::endpoint_view(p).ok(),"reason_code":null})
                }
                None => {
                    json!({"snapshot_state":"available","configuration_revision":revision,"configured":null,"resolution":null,"reason_code":"provider_missing"})
                }
            }
        };
        let observations = crate::observations::inspect(store, config, id)
            .unwrap_or_else(|error| json!({"state":"unavailable","reason_code":error.code}));
        return Ok(
            json!({"id":id,"provider":p,"metadata":crate::catalog::effective(store,p,None)?,"endpoints":{"file":endpoint(config,&file.revision),"loaded":loaded.as_ref().map(|f|endpoint(&f.config,&f.revision)).unwrap_or_else(||json!({"snapshot_state":"absent","configuration_revision":null,"configured":null,"resolution":null,"reason_code":"no_loaded_snapshot"}))},"identity":null,"upstream_authentication":"unverified","observations":observations,"renewal":p.subscription.as_ref().and_then(|s|s.renewal_date),"via":match &p.auth{Some(Auth::Login{via,..})=>Some(via),_=>None}}),
        );
    }
    if path == "provider refresh" {
        let provider = config
            .providers
            .get(operand(r, 0)?)
            .ok_or_else(|| Error::new("unknown_resource", "Provider is not configured"))?;
        let scope = required(r, "scope")?;
        let scopes = if scope == "all" {
            &["auth", "models", "quota"][..]
        } else {
            std::slice::from_ref(&scope)
        };
        let mut needs_secret = false;
        for scope in scopes {
            needs_secret |=
                crate::integrations::observation_destination(provider, scope)?.is_some();
        }
        let unlock = if r.dry_run || !needs_secret {
            None
        } else {
            unlock_needed(r)?
        };
        let mut data = crate::integrations::refresh(
            store,
            config,
            operand(r, 0)?,
            scope,
            unlock.as_deref().map(Vec::as_slice),
            r.dry_run,
        )
        .await?;
        if !r.dry_run
            && data["checks"].as_array().is_some_and(|a| {
                a.iter()
                    .any(|c| c["outcome"] == "failed" || c["outcome"] == "unavailable")
            })
        {
            data["_code"] = json!("observation_failed");
        }
        return Ok(data);
    }
    if path == "model list" {
        return model_list(config, r);
    }
    if path == "model inspect" {
        let id = operand(r, 0)?;
        if let Ok((p, m)) = config::model(config, id) {
            return Ok(
                json!({"kind":"concrete","qualified_model":id,"configuration":m,"metadata":crate::catalog::effective(store,p,Some((config::qualified(id)?.1,m)))?,"protocols":crate::integrations::protocols(p,config::qualified(id)?.1)?,"revision":file.revision,"priority_evaluation":crate::routing::explain(store,config,id,arg(r,"protocol").unwrap_or("chat-completions"),None)?}),
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
        return Ok(
            json!({"kind":"alias","definition":aliases[0],"continuity":aliases[0].continuity.as_deref().unwrap_or("session"),"revision":file.revision}),
        );
    }
    if path == "model explain" {
        return explain(store, config, r).await;
    }
    if path == "model rule list" || path == "model rule inspect" {
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
        return Ok(
            json!({"qualified_model":id,"rules":m.priority_rules,"priority_evaluation":evaluation}),
        );
    }
    if path == "agent list" {
        return Ok(
            json!({"agents":config.agents.iter().map(|(id,t)|json!({"target":id,"agent":t.agent,"path":t.scope,"provider_count":t.providers.len(),"model_count":t.providers.values().map(|e|e.models.len()).sum::<usize>(),"default_ready":t.default.is_some(),"native_application":"not_verified"})).collect::<Vec<_>>()}),
        );
    }
    if path == "agent inspect" {
        return crate::native::inspect(store, config, operand(r, 0)?).await;
    }
    if path == "agent apply" {
        return crate::native::apply_request(store, config, r).await;
    }
    if path == "network show" {
        return Ok(network_view(
            config,
            &file.revision,
            file.exists,
            arg(r, "state").unwrap_or("file"),
        ));
    }
    if path == "proxy key show" {
        return key_show(store, config, r).await;
    }
    if path == "model probe" {
        let unlock = if r.dry_run { None } else { unlock_needed(r)? };
        return crate::proxy::probe(store, config, r, unlock.as_deref().map(Vec::as_slice)).await;
    }
    if path.starts_with("proxy ")
        || path.starts_with("web ")
        || path.starts_with("history ")
        || path == "usage"
    {
        let unlock = if !r.dry_run && path == "proxy restart" {
            unlock_needed(r)?
        } else {
            None
        };
        return crate::service::control(store, config, r, unlock.as_deref().map(Vec::as_slice))
            .await;
    }
    Err(Error::new(
        "invalid_arguments",
        "Unknown finite management operation",
    ))
}
fn model_list(config: &Config, r: &Request) -> Result<Value> {
    let mut entries = Vec::new();
    if arg(r, "kind") != Some("alias") {
        for (id, p) in &config.providers {
            if arg(r, "provider").is_some_and(|v| v != id) {
                continue;
            }
            for (mid, m) in &p.models {
                let protocols = crate::integrations::protocols(p, mid)?;
                if arg(r, "protocol").is_some_and(|proto| !protocols.iter().any(|p| p == proto)) {
                    continue;
                }
                entries.push(json!({"id":format!("{id}/{mid}"),"kind":"concrete","tier":m.tier,"enabled":p.enabled&&m.enabled,"published_protocols":if p.enabled&&m.enabled{protocols}else{Vec::new()},"availability":if p.auth.is_none(){"authentication_missing"}else{"unverified"}}));
            }
        }
    }
    if arg(r, "kind") != Some("concrete") && arg(r, "provider").is_none() {
        for a in &config.model_aliases {
            if arg(r, "protocol").is_some_and(|p| p != a.protocol) {
                continue;
            }
            entries.push(json!({"id":a.id,"protocol":a.protocol,"kind":"alias","tier":a.tier,"enabled":a.enabled,"published_protocols":if a.enabled{vec![a.protocol.as_str()]}else{Vec::new()},"continuity":a.continuity.as_deref().unwrap_or("session")}));
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
    let mut id = r.operands.first().cloned();
    let protocol = required(r, "protocol")?;
    let context = if let Some(tier) = arg(r, "tier") {
        if !matches!(tier, "L0" | "L1" | "L2" | "L3") {
            return Err(Error::new(
                "invalid_arguments",
                "Select a configured L0, L1, L2 or L3 tier",
            ));
        }
        if id.is_some() || arg(r, "agent-provider").is_some() || arg(r, "session").is_some() {
            return Err(Error::new(
                "invalid_arguments",
                "Tier advice cannot combine with model ID, entry or session",
            ));
        }
        let context = if let Some(target) = arg(r, "target") {
            let t = c
                .agents
                .get(target)
                .ok_or_else(|| Error::new("unknown_resource", "Target is not configured"))?;
            json!({"kind":"agent_target","target":target,"agent_provider":null,"agent":t.agent,"path":t.scope})
        } else {
            crate::native::discover(
                store,
                Some(required(r, "agent")?),
                Some(required(r, "path")?),
            )
            .await?;
            json!({"kind":"native_scope","target":null,"agent_provider":null,"agent":arg(r,"agent"),"path":arg(r,"path")})
        };
        id = Some(tier.into());
        context
    } else if let Some(target) = arg(r, "target") {
        let t = c
            .agents
            .get(target)
            .ok_or_else(|| Error::new("unknown_resource", "Target is not configured"))?;
        let name = required(r, "agent-provider")?;
        let e = t.providers.get(name).ok_or_else(|| {
            Error::new("unknown_resource", "Native provider entry not configured")
        })?;
        if let Some(provider) = &e.provider {
            id = Some(format!(
                "{provider}/{}",
                id.ok_or_else(|| Error::new("invalid_arguments", "Model ID required"))?
            ));
        }
        json!({"kind":"agent_provider","target":target,"agent_provider":name,"agent":t.agent,"path":t.scope})
    } else {
        if arg(r, "agent-provider").is_some() {
            return Err(Error::new(
                "invalid_arguments",
                "Native provider context requires target",
            ));
        }
        json!({"kind":"proxy","target":null,"agent_provider":null,"agent":null,"path":null})
    };
    let id = id.ok_or_else(|| Error::new("invalid_arguments", "Model ID or tier is required"))?;
    let entry = context["target"]
        .as_str()
        .zip(context["agent_provider"].as_str())
        .and_then(|(target, name)| c.agents.get(target)?.providers.get(name));
    if arg(r, "session").is_some() {
        if entry.is_some_and(|entry| entry.provider.is_some()) {
            return Err(Error::new(
                "invalid_arguments",
                "A retained proxy session cannot be inspected through an upstream-source entry",
            ));
        }
        let mut data = crate::service::control(store, config, r, None).await?;
        data["context"] = context;
        data["requested"] = json!({"id":r.operands.first(),"tier":null,"protocol":protocol});
        data["native_assessment"] = json!({"state":"not_reassessed","reason":"retained_destination_inspection","authorization":"not_verified"});
        return Ok(data);
    }
    let requirements =
        context["agent"]
            .as_str()
            .map(|agent| crate::native::compatibility::Context {
                agent,
                target: context["target"].as_str(),
                provider: entry.and_then(|e| e.provider.as_deref()),
                proxy: entry.is_some_and(|e| e.proxy == Some(true)),
            });
    let advice = crate::routing::Advice {
        native: requirements,
        tier: arg(r, "tier"),
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
    data["requested"] = json!({"id":r.operands.first(),"tier":arg(r,"tier"),"protocol":protocol});
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
pub(crate) async fn impact(
    store: &Store,
    candidate: &Config,
    file_revision: &str,
    before: Option<&Config>,
) -> Result<Value> {
    let loaded = crate::service::loaded(store).await?;
    let after = serde_json::to_value(candidate)
        .map_err(|_| Error::new("operation_failed", "Cannot project validated configuration"))?;
    let loaded_value = loaded
        .as_ref()
        .and_then(|f| serde_json::to_value(&f.config).ok());
    let changes = diff_values(loaded_value.as_ref(), Some(&after), "", "activation");
    let mut out = json!({"config_impact":{"file_revision_before":file_revision,"file_revision_after":file_revision,"candidate_revision":null,"loaded_revision_before":loaded.as_ref().map(|f|&f.revision),"activation_diff_state":if loaded.is_some(){"complete"}else{"no_loaded_snapshot"},"includes_pending_file_changes":loaded.as_ref().map(|f|f.revision!=file_revision)},"changes":changes,"warnings":[{"code":"external_availability_unverified","message":"Dry-run does not read secrets or verify upstream authorization","affected":[]}],"save":"not_executed","runtime":"not_executed","native_application":"not_requested"});
    out["session_impact"] = crate::service::preview_activation(store, candidate).await?;
    if let Some(before) = before {
        let before = serde_json::to_value(before)
            .map_err(|_| Error::new("operation_failed", "Cannot project prior configuration"))?;
        let saves = diff_values(Some(&before), Some(&after), "", "file_save");
        out["changes"].as_array_mut().unwrap().extend(saves);
    }
    Ok(out)
}
pub(crate) fn diff_values(
    before: Option<&Value>,
    after: Option<&Value>,
    path: &str,
    boundary: &str,
) -> Vec<Value> {
    if before == after {
        return vec![];
    }
    if before.is_some_and(Value::is_object) && after.is_some_and(Value::is_object) {
        let b = before.unwrap().as_object().unwrap();
        let a = after.unwrap().as_object().unwrap();
        let keys: std::collections::BTreeSet<_> = b.keys().chain(a.keys()).collect();
        return keys
            .into_iter()
            .flat_map(|key| {
                diff_values(
                    b.get(key),
                    a.get(key),
                    &if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    },
                    boundary,
                )
            })
            .collect();
    }
    vec![
        json!({"boundary":boundary,"resource":{"kind":"configuration","id":null,"scope":null},"path":path,"kind":if before.is_none(){"add"}else if after.is_none(){"remove"}else{"replace"},"before":before,"after":after}),
    ]
}
fn recovery(store: &Store, r: &Request) -> Result<Value> {
    match r.command.last().map(String::as_str) {
        Some("list") => crate::secrets::recovery_list(store),
        Some("inspect") => crate::secrets::recovery_inspect(store, operand(r, 0)?),
        Some("discard") => {
            if !r.dry_run {
                crate::secrets::recovery_discard(store, operand(r, 0)?)?;
            }
            Ok(json!({"recovery":operand(r,0)?,"discarded":!r.dry_run}))
        }
        _ => Err(Error::new(
            "invalid_arguments",
            "Unknown recovery operation",
        )),
    }
}
async fn reset(store: &Store, r: &Request) -> Result<Value> {
    let path = store.root.join("tmax.toml");
    let bytes = std::fs::read(&path).map_err(|_| {
        Error::new(
            "invalid_arguments",
            "Reset requires an existing readable invalid configuration",
        )
    })?;
    let expected = config::revision(&bytes);
    if r.if_revision.as_ref().is_some_and(|rev| rev != &expected) {
        return Err(Error::new(
            "inputs_changed",
            "Rejected file revision changed",
        ));
    }
    if std::str::from_utf8(&bytes)
        .ok()
        .is_some_and(|s| config::parse(s).is_ok())
    {
        return Err(Error::new(
            "invalid_arguments",
            "Reset is permitted only for an existing invalid configuration",
        ));
    }
    if r.dry_run {
        let mut preview = impact(store, &Config::empty(), &expected, None).await?;
        preview["recovery"] = json!("protected_capture_not_executed");
        preview["original_revision"] = json!(expected);
        preview["reset"] = json!("not_executed");
        preview["desired_settings"] = json!("all removed");
        preview["native_files"] = json!("unchanged");
        preview["stored_secrets"] = json!("retained");
        return Ok(preview);
    }
    let unlock = unlock_needed(r)?;
    let recovery =
        crate::secrets::protect_recovery(store, &bytes, unlock.as_deref().map(Vec::as_slice))?;
    let document = "version = 1\n".parse().expect("constant TOML");
    if config::save(store, &expected, &document).is_err() {
        return Ok(
            json!({"_code":"saved_not_active","recovery":recovery,"save":"failed","activation":"not_requested"}),
        );
    }
    let empty = Config::empty();
    let revision = config::raw_revision(store)?;
    let activation = crate::service::activate(store, &empty, &revision).await;
    let autostart = crate::service::reconcile_autostart(store, &empty, false).await;
    Ok(
        json!({"_code":if activation.is_err()||autostart.is_err(){json!("saved_not_active")}else{Value::Null},"recovery":recovery,"save":"saved","file_revision":revision,"activation":activation.ok(),"autostart":autostart.ok(),"native_files":"unchanged","stored_secrets":"retained"}),
    )
}
async fn secret_inventory(store: &Store) -> Result<Value> {
    let mut inventory = crate::secrets::inventory(store)?;
    let desired = config::load(store);
    let loaded = crate::service::loaded(store).await;
    for entry in inventory["entries"].as_array_mut().into_iter().flatten() {
        let reference = entry["reference"].as_str().unwrap_or("");
        let d = match &desired {
            Ok(f) => reference_view(&f.config, reference),
            Err(_) => {
                json!({"state":"unknown","resources":[],"reason_code":"configuration_unavailable"})
            }
        };
        let l = match &loaded {
            Ok(Some(f)) => reference_view(&f.config, reference),
            Ok(None) => json!({"state":"unreferenced","resources":[],"reason_code":null}),
            Err(_) => {
                json!({"state":"unknown","resources":[],"reason_code":"resident_identity_unavailable"})
            }
        };
        let native = crate::native::secret_references(store, reference)?;
        let safe = d["state"] == "unreferenced"
            && l["state"] == "unreferenced"
            && native["state"] == "unreferenced"
            && entry["metadata_state"] == "known";
        entry["references"] = json!({"desired":d,"loaded":l,"native":native});
        entry["cleanup"] = json!({"state":if safe{"eligible"}else{"blocked"},"blockers":if safe{json!([])}else{json!([{"code":"reference_safety_unestablished","message":"Resolve known references and unknown ownership before cleanup","affected":[]}])}});
    }
    Ok(inventory)
}
fn reference_view(c: &Config, reference: &str) -> Value {
    let mut refs = Vec::new();
    for (id, p) in &c.providers {
        if matches!(&p.auth,Some(Auth::ApiKey {key_ref})if key_ref==reference) {
            refs.push(json!({"kind":"provider","id":id,"scope":null}));
        }
    }
    if c.proxy.as_ref().is_some_and(|p| p.key_ref == reference) {
        refs.push(json!({"kind":"proxy","id":null,"scope":null}));
    }
    json!({"state":if refs.is_empty(){"unreferenced"}else{"referenced"},"resources":refs,"reason_code":null})
}
async fn cleanup(store: &Store, r: &Request) -> Result<Value> {
    let reference = operand(r, 0)?;
    let inventory = secret_inventory(store).await?;
    let entry = inventory["entries"]
        .as_array()
        .and_then(|a| a.iter().find(|e| e["reference"] == reference))
        .ok_or_else(|| {
            Error::new(
                "unknown_resource",
                "Secret reference is not in retained inventory",
            )
        })?;
    if entry["cleanup"]["state"] != "eligible" {
        return Err(Error::new(
            "recovery_required",
            "Secret reference safety is unknown or still referenced; inspect retained ownership and references",
        ));
    }
    if !r.dry_run {
        let _guard = store.lock("configuration")?;
        let current = secret_inventory(store).await?;
        if !current["entries"].as_array().is_some_and(|a| {
            a.iter()
                .any(|e| e["reference"] == reference && e["cleanup"]["state"] == "eligible")
        }) {
            return Err(Error::new(
                "inputs_changed",
                "Secret references changed during cleanup",
            ));
        }
        crate::secrets::remove(store, reference)?;
    }
    Ok(json!({"reference":reference,"deleted":!r.dry_run,"cleanup":entry["cleanup"]}))
}
async fn key_show(store: &Store, c: &Config, r: &Request) -> Result<Value> {
    let proxy = c
        .proxy
        .as_ref()
        .ok_or_else(|| Error::new("invalid_arguments", "Proxy is not configured"))?;
    let mut output = crate::secrets::qualify_output(r)?;
    if r.dry_run {
        return Ok(
            json!({"reference":proxy.key_ref,"loaded_match":null,"delivery":{"channel":output.channel(),"descriptor":output.descriptor(),"delivered":false}}),
        );
    }
    let unlock = unlock_needed(r)?;
    let key = crate::secrets::get(store, &proxy.key_ref, unlock.as_deref().map(Vec::as_slice))?;
    let delivery = output.deliver(&key);
    let loaded = crate::service::loaded(store).await.ok().flatten();
    Ok(
        json!({"_code":if delivery.is_err(){json!("secret_delivery_failed")}else{Value::Null},"reference":proxy.key_ref,"loaded_match":loaded.as_ref().map(|f|f.config.proxy.as_ref().is_some_and(|p|p.key_ref==proxy.key_ref)),"delivery":{"channel":output.channel(),"descriptor":output.descriptor(),"delivered":if delivery.is_ok(){json!(true)}else{Value::Null}},"boundaries":[{"name":"secret_delivery","state":if delivery.is_ok(){"completed"}else{"partial"},"affected":[],"message":"Protected delivery may be partial on failure; do not replay automatically"}]}),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn failed_registration_reports_partial_effects_without_losing_saved_intent() {
        for enabled in [true, false] {
            let data = json!({"save":"saved","runtime":"not_running","autostart":{
                "_code":"saved_not_active","reconciliation":"failed",
                "completed_effects":if enabled {vec!["owned_unit_written"]} else {vec![]},
                "status":{"saved_intent":enabled,"login_enabled":false},
                "error":{"code":"autostart_failed"}
            }});
            let response = success_response("proxy autostart", false, data);
            assert_eq!(response.exit_code, 4);
            assert_eq!(response.body["code"], "saved_not_active");
            assert_eq!(response.body["status"], "incomplete");
            assert_eq!(response.body["data"]["save"], "saved");
            assert_eq!(response.body["data"]["runtime"], "not_running");
            assert_eq!(
                response.body["data"]["autostart"]["status"]["saved_intent"],
                enabled
            );
            assert_eq!(
                response.body["data"]["autostart"]["completed_effects"]
                    .as_array()
                    .unwrap()
                    .len(),
                usize::from(enabled)
            );
        }
    }
    use super::*;
    #[tokio::test]
    async fn incomplete_native_entry_and_tier_advice_exclude_the_same_protocol_that_apply_rejects()
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
                models: [(
                    "m".into(),
                    config::Model {
                        tier: Some("L0".into()),
                        ..Default::default()
                    },
                )]
                .into(),
                ..Default::default()
            },
        );
        config.agents.insert(
            "target".into(),
            config::AgentTarget {
                agent: "codex".into(),
                scope: directory.path().join("native"),
                providers: [(
                    "entry".into(),
                    config::NativeProvider {
                        provider: Some("p".into()),
                        proxy: None,
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
        for tier in [false, true] {
            let mut arguments = std::collections::BTreeMap::from([
                ("protocol".into(), vec!["chat-completions".into()]),
                ("target".into(), vec!["target".into()]),
            ]);
            arguments.insert(
                if tier { "tier" } else { "agent-provider" }.into(),
                vec![if tier { "L0" } else { "entry" }.into()],
            );
            let request = Request {
                command: vec!["model".into(), "explain".into()],
                operands: if tier { vec![] } else { vec!["m".into()] },
                arguments,
                dry_run: false,
                if_revision: None,
            };
            let advice = explain(&store, &config, &request).await.unwrap();
            assert!(advice["selection"].is_null());
            assert_eq!(advice["excluded_candidates"][0]["qualified_model"], "p/m");
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
            crate::native::apply(&store, &config, "target", true, None)
                .await
                .unwrap_err()
                .code,
            "incompatible"
        );
    }

    #[test]
    fn exact_duration_and_no_document_input() {
        assert_eq!(duration("1d").unwrap(), 86400);
        for bad in ["0s", "1.5h", "1h30m", "10", "1H", "-1s"] {
            assert!(duration(bad).is_err());
        }
    }
}
