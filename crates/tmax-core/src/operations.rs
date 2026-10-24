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
        "awaiting_native_reload" if matches!(operation, "agent apply" | "agent restore") => 0,
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
        | "native_not_installed"
        | "native_version_probe_failed"
        | "native_version_unknown"
        | "native_version_unqualified"
        | "native_schema_incompatible"
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
            | "storage_version_unsupported"
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
        || (error.field.as_deref().is_some_and(|field| {
            field.starts_with("model_aliases.") && field.contains(".candidate_reasoning_effort.")
        }) && matches!(
            code,
            "reasoning_incompatible"
                | "reasoning_effort_incompatible"
                | "reasoning_effort_unavailable"
                | "candidate_reasoning_protocol_unsupported"
        ))
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
    if matches!(
        path.as_str(),
        "provider login-start" | "provider login-status" | "provider login-cancel"
    ) {
        return crate::service::browser_login(store, r).await;
    }
    if path == "skill install" {
        return crate::skill::install(store, r);
    }
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
    if path == "gateway status" || path == "web status" {
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
        "gateway start" | "gateway stop" | "web open" | "web stop"
    ) {
        let config = config::load(store)
            .map(|f| f.config)
            .unwrap_or_else(|_| Config::empty());
        return crate::service::control(store, &config, r).await;
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
    let file = match selected(store, r).await {
        Ok(Some(file)) => file,
        Ok(None) => {
            return Ok(
                json!({"state":"loaded","snapshot_state":"absent","reason_code":"no_loaded_snapshot"}),
            );
        }
        Err(error) if arg(r, "state") == Some("loaded") => {
            return Ok(
                json!({"state":"loaded","snapshot_state":"unavailable","configuration_revision":null,"reason_code":error.code}),
            );
        }
        Err(error) => return Err(error),
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
        let loaded = crate::service::loaded(store).await;
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
        return Ok(
            json!({"id":id,"provider":p,"metadata":crate::catalog::effective(store,p,None)?,"endpoints":{"file":endpoint(config,&file.revision),"loaded":loaded_endpoint},"identity":null,"upstream_authentication":"unverified","observations":observations,"renewal":p.subscription.as_ref().and_then(|s|s.renewal_date),"via":match &p.auth{Some(Auth::Login{via,..})=>Some(via),_=>None}}),
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
        for scope in scopes {
            crate::integrations::observation_destination(provider, scope)?;
        }
        let mut data =
            crate::integrations::refresh(store, config, operand(r, 0)?, scope, r.dry_run).await?;
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
            json!({"agents":config.agents.iter().map(|(id,t)|json!({"target":id,"agent":t.agent,"path":t.scope,"provider_count":t.providers.len(),"model_count":t.providers.values().map(|e|e.models.len()).sum::<usize>(),"default_configured":t.default.is_some(),"native_application":"not_verified"})).collect::<Vec<_>>()}),
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
    if path == "gateway key show" {
        return key_show(store, config, r).await;
    }
    if path == "model probe" {
        return crate::gateway::probe(store, config, r).await;
    }
    if path.starts_with("gateway ")
        || path.starts_with("web ")
        || path.starts_with("history ")
        || path == "usage"
    {
        return crate::service::control(store, config, r).await;
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
        crate::native::discover(store, Some(agent), Some(path)).await?;
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
            .map(|agent| crate::native::compatibility::Context {
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
/// Synchronization after a completed desired-state edit is not a second save.
/// Explicit runtime and revocation operations continue to use strict activation.
pub(crate) async fn activate_saved(
    store: &Store,
    config: &Config,
    revision: &str,
    data: &mut Value,
) {
    let status = crate::service::status(store).await;
    let (activation, loaded_revision) = match status {
        Ok(status) => {
            let loaded_revision = status["resident"]["identity"]["loaded_revision"].clone();
            let activation = match status["resident"]["discovery_state"].as_str() {
                Some("compatible") => crate::service::activate(store, config, revision)
                    .await
                    .unwrap_or_else(|error| json!({"activation":"failed","error":error})),
                Some("absent") => json!({"activation":"not_running","revision":revision}),
                _ => {
                    json!({"activation":"unavailable","reason_code":status["resident"]["reason_code"],"discovery_state":status["resident"]["discovery_state"]})
                }
            };
            (activation, loaded_revision)
        }
        Err(error) => (
            json!({"activation":"unavailable","error":error}),
            Value::Null,
        ),
    };
    project_saved_activation(data, activation, loaded_revision);
}

fn project_saved_activation(data: &mut Value, mut activation: Value, loaded_revision: Value) {
    let outcome_code = activation
        .as_object_mut()
        .and_then(|object| object.remove("_code"))
        .filter(|code| code.as_str().is_some_and(|code| !code.is_empty()));
    if let Some(code) = &outcome_code {
        activation["code"] = code.clone();
    }
    // Discovery/transport failure can occur after the resident accepted the call.
    // Its previous revision is a before-fact, not proof of the after-state.
    if matches!(
        activation["error"]["code"].as_str(),
        Some(
            "resident_identity_unavailable"
                | "resident_incompatible"
                | "resident_stopping"
                | "resident_control_path_invalid"
                | "platform_unavailable"
        )
    ) {
        activation["activation"] = json!("unavailable");
    }
    let active = activation["activation"] == "active";
    let unavailable = activation["activation"] == "unavailable";
    let not_running = activation["activation"] == "not_running";
    data["loaded_revision_before"] = loaded_revision;
    data["loaded_revision_after"] = if active {
        activation["revision"].clone()
    } else {
        // A concurrent activation can replace the sampled before-state even
        // when this request receives an identified rejection.
        Value::Null
    };
    data["runtime"] = json!(if active {
        "loaded"
    } else if not_running {
        "not_running"
    } else if unavailable {
        "unavailable"
    } else {
        "not_loaded"
    });
    if outcome_code.is_some() || (!active && !not_running) {
        let reason = activation["error"]["code"]
            .as_str()
            .or_else(|| activation["reason_code"].as_str())
            .or_else(|| outcome_code.as_ref().and_then(Value::as_str))
            .unwrap_or("saved_not_active");
        let warning = json!({"code":reason,"message":"Desired changes are committed; background activation did not establish ready runtime use. Do not repeat the edit; inspect runtime state and explicitly reload or restart as required","affected":[]});
        data.as_object_mut()
            .unwrap()
            .entry("warnings")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap()
            .push(warning);
        data.as_object_mut().unwrap().entry("boundaries").or_insert_with(|| json!([])).as_array_mut().unwrap().push(json!({"name":"activation","state":if unavailable{"blocked"}else if active{"completed"}else{"failed"},"affected":[],"message":"Desired save completed independently; activation and serving readiness are reported separately"}));
        if data.get("session_impact").is_none() {
            data["session_impact"] =
                json!({"state":"unavailable","reason_code":reason,"sessions":null});
        }
        let activation_operation =
            if matches!(reason, "listener_restart_required" | "restart_required") {
                "gateway restart"
            } else {
                "config reload"
            };
        data["next_actions"] = json!([{"code":"inspect_runtime","operation":"status","arguments":[],"message":"Inspect saved and loaded state; do not replay the completed edit"},{"code":"activate_saved","operation":activation_operation,"arguments":[],"message":"After resolving runtime blockers, explicitly activate the current saved configuration"}]);
    }
    data["activation"] = activation;
}

pub(crate) async fn impact(
    store: &Store,
    candidate: &Config,
    file_revision: &str,
    before: Option<&Config>,
) -> Result<Value> {
    config::validate(candidate)?;
    let (loaded, loaded_error) = match crate::service::loaded(store).await {
        Ok(loaded) => (loaded, None),
        Err(error) => (None, Some(error)),
    };
    let after = serde_json::to_value(candidate)
        .map_err(|_| Error::new("operation_failed", "Cannot project validated configuration"))?;
    let loaded_value = loaded
        .as_ref()
        .and_then(|f| serde_json::to_value(&f.config).ok());
    let changes = if loaded_error.is_none() {
        diff_values(loaded_value.as_ref(), Some(&after), "", "activation")
    } else {
        vec![]
    };
    let mut out = json!({"config_impact":{"file_revision_before":file_revision,"file_revision_after":file_revision,"candidate_revision":null,"loaded_revision_before":loaded.as_ref().map(|f|&f.revision),"activation_diff_state":if loaded_error.is_some(){"unavailable"}else if loaded.is_some(){"complete"}else{"no_loaded_snapshot"},"includes_pending_file_changes":loaded.as_ref().map(|f|f.revision!=file_revision)},"changes":changes,"warnings":[{"code":"external_availability_unverified","message":"Dry-run does not read secrets or verify upstream authorization","affected":[]}],"save":"not_executed","runtime":"not_executed","native_application":"not_requested"});
    out["session_impact"] = match crate::service::preview_activation(store, candidate).await {
        Ok(impact) => impact,
        Err(error) => {
            json!({"state":"unavailable","reason_code":error.code,"sessions":null,"listener":null})
        }
    };
    if let Some(error) = loaded_error {
        out["warnings"].as_array_mut().unwrap().push(json!({"code":error.code,"message":"Loaded configuration and complete activation impact are unavailable; local desired changes remain inspectable","affected":[]}));
    }
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
    let recovery = crate::secrets::protect_recovery(store, &bytes)?;
    let document = "version = 1\n".parse().expect("constant TOML");
    let revision = match config::save(store, &expected, &document) {
        Ok(revision) => revision,
        Err(error) => {
            return Ok(
                json!({"_code":"saved_not_active","recovery":recovery,"save":"failed","save_error":error,"activation":"not_requested"}),
            );
        }
    };
    let empty = Config::empty();
    let activation = crate::service::activate(store, &empty, &revision).await;
    let autostart = crate::service::reconcile_autostart(store, &empty, false).await;
    Ok(
        json!({"_code":if activation.is_err()||autostart.is_err(){json!("saved_not_active")}else{Value::Null},"recovery":recovery,"save":"saved","file_revision":revision,"activation":activation.unwrap_or_else(|error|json!({"activation":"failed","error":error})),"autostart":autostart.unwrap_or_else(|error|json!({"state":"failed","error":error})),"native_files":"unchanged","stored_secrets":"retained"}),
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
    if c.gateway.as_ref().is_some_and(|p| p.key_ref == reference) {
        refs.push(json!({"kind":"gateway","id":null,"scope":null}));
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
    let gateway = c
        .gateway
        .as_ref()
        .ok_or_else(|| Error::new("invalid_arguments", "Gateway is not configured"))?;
    let mut output = crate::secrets::qualify_output(r)?;
    if r.dry_run {
        return Ok(
            json!({"reference":gateway.key_ref,"loaded_match":null,"delivery":{"channel":output.channel(),"descriptor":output.descriptor(),"delivered":false}}),
        );
    }
    let key = crate::secrets::get(store, &gateway.key_ref)?;
    let delivery = output.deliver(&key);
    let loaded = crate::service::loaded(store).await.ok().flatten();
    Ok(
        json!({"_code":if delivery.is_err(){json!("secret_delivery_failed")}else{Value::Null},"reference":gateway.key_ref,"loaded_match":loaded.as_ref().map(|f|f.config.gateway.as_ref().is_some_and(|p|p.key_ref==gateway.key_ref)),"delivery":{"channel":output.channel(),"descriptor":output.descriptor(),"delivered":if delivery.is_ok(){json!(true)}else{Value::Null}},"boundaries":[{"name":"secret_delivery","state":if delivery.is_ok(){"completed"}else{"partial"},"affected":[],"message":"Protected delivery may be partial on failure; do not replay automatically"}]}),
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
            let response = success_response("gateway autostart", false, data);
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
    #[test]
    fn candidate_effort_configuration_errors_are_invalid_input_not_runtime_failures() {
        for code in [
            "reasoning_incompatible",
            "reasoning_effort_incompatible",
            "reasoning_effort_unavailable",
            "candidate_reasoning_protocol_unsupported",
        ] {
            let configured = error_response(
                "model alias update",
                Error::new(code, "Unsupported configured candidate effort")
                    .field("model_aliases.fast.candidate_reasoning_effort.p/m"),
                false,
            );
            assert_eq!(configured.exit_code, 2);
            assert_eq!(configured.body["code"], code);
            assert_eq!(configured.body["status"], "error");
            let runtime =
                error_response("gateway probe", Error::new(code, "Runtime mismatch"), false);
            assert_eq!(runtime.exit_code, 4);
        }
    }
    #[test]
    fn completed_native_writes_are_success_with_a_reload_reminder() {
        for operation in ["agent apply", "agent restore"] {
            let response = success_response(
                operation,
                false,
                json!({"_code":"awaiting_native_reload","effective":"not_verified"}),
            );
            assert_eq!(response.exit_code, 0);
            assert_eq!(response.body["status"], "success");
            assert_eq!(response.body["data"]["effective"], "not_verified");
        }
        let login = success_response(
            "provider login",
            false,
            json!({"_code":"awaiting_native_reload"}),
        );
        assert_eq!(login.exit_code, 3);
    }
    #[cfg(unix)]
    fn request(command: &[&str], operands: &[&str], arguments: &[(&str, &[&str])]) -> Request {
        Request {
            command: command.iter().map(|value| (*value).into()).collect(),
            operands: operands.iter().map(|value| (*value).into()).collect(),
            arguments: arguments
                .iter()
                .map(|(key, values)| {
                    (
                        (*key).into(),
                        values.iter().map(|value| (*value).into()).collect(),
                    )
                })
                .collect(),
            dry_run: false,
            if_revision: None,
        }
    }

    #[test]
    fn background_failure_preserves_save_success_but_explicit_activation_stays_strict() {
        let activation = json!({"_code":"saved_not_active","activation":"failed","error":{"code":"listener_restart_required","message":"Explicit restart required"}});
        let mut saved =
            json!({"save":"saved","file_revision":"new","native_application":"not_requested"});
        project_saved_activation(&mut saved, activation.clone(), json!("old"));
        let response = success_response("provider add", false, saved);
        assert_eq!(response.exit_code, 0);
        assert_eq!(response.body["status"], "success");
        let data = &response.body["data"];
        assert_eq!(data["save"], "saved");
        assert_eq!(data["activation"]["activation"], "failed");
        assert_eq!(
            data["activation"]["error"]["code"],
            "listener_restart_required"
        );
        assert_eq!(data["warnings"][0]["code"], "listener_restart_required");
        assert_eq!(data["loaded_revision_before"], "old");
        assert!(data["loaded_revision_after"].is_null());
        assert_eq!(data["boundaries"][0]["state"], "failed");
        assert!(data["session_impact"]["sessions"].is_null());
        assert_eq!(data["next_actions"][1]["operation"], "gateway restart");
        assert_eq!(
            success_response(
                "config reload",
                false,
                json!({"save":"not_requested","activation":activation.clone()})
            )
            .exit_code,
            4
        );
        assert_eq!(
            success_response(
                "gateway key replace",
                false,
                json!({"save":"saved","activation":activation})
            )
            .exit_code,
            4
        );
    }

    #[test]
    fn background_uncertain_reply_never_claims_the_prior_revision_is_still_loaded() {
        let mut data = json!({"save":"saved","file_revision":"new"});
        project_saved_activation(
            &mut data,
            json!({"_code":"saved_not_active","activation":"failed","error":{"code":"resident_identity_unavailable"}}),
            json!("old"),
        );
        assert_eq!(data["loaded_revision_before"], "old");
        assert!(data["loaded_revision_after"].is_null());
        assert_eq!(data["runtime"], "unavailable");
        assert_eq!(data["activation"]["activation"], "unavailable");
        assert_eq!(
            success_response("provider update", false, data).exit_code,
            0
        );
        let mut stopped = json!({"save":"saved"});
        project_saved_activation(
            &mut stopped,
            json!({"activation":"not_running"}),
            json!("old"),
        );
        assert!(stopped["loaded_revision_after"].is_null());
        assert_eq!(stopped["runtime"], "not_running");
    }

    #[test]
    fn background_concurrent_revision_rejection_reports_no_unproven_after_revision() {
        // R1 was committed by this edit. Status sampled R0, then another writer
        // committed and activated R2 before activation of R1 was rejected.
        let mut saved = json!({"save":"saved","file_revision":"R1"});
        project_saved_activation(
            &mut saved,
            json!({"_code":"saved_not_active","activation":"failed","error":{"code":"inputs_changed"}}),
            json!("R0"),
        );
        let response = success_response("provider update", false, saved);
        assert_eq!(response.exit_code, 0);
        assert_eq!(response.body["status"], "success");
        assert_eq!(response.body["data"]["file_revision"], "R1");
        assert_eq!(response.body["data"]["loaded_revision_before"], "R0");
        assert!(response.body["data"]["loaded_revision_after"].is_null());
        assert_eq!(
            response.body["data"]["activation"]["error"]["code"],
            "inputs_changed"
        );
        assert_eq!(
            response.body["data"]["warnings"][0]["code"],
            "inputs_changed"
        );
    }

    #[test]
    fn background_confirmed_activation_has_no_failed_runtime_warning_for_edit_or_catalog() {
        for operation in ["provider update", "catalog apply"] {
            let mut committed = if operation == "catalog apply" {
                json!({"catalog_apply":"completed","save":"not_requested"})
            } else {
                json!({"save":"saved","file_revision":"R1"})
            };
            project_saved_activation(
                &mut committed,
                json!({"_code":null,"activation":"active","revision":"R1","serving_state":"stopped"}),
                json!("R0"),
            );
            let response = success_response(operation, false, committed);
            assert_eq!(response.exit_code, 0);
            assert_eq!(response.body["code"], "ok");
            assert_eq!(response.body["status"], "success");
            let data = &response.body["data"];
            assert_eq!(data["runtime"], "loaded");
            assert_eq!(data["activation"]["activation"], "active");
            assert_eq!(data["loaded_revision_before"], "R0");
            assert_eq!(data["loaded_revision_after"], "R1");
            assert!(data.get("warnings").is_none());
            assert!(data.get("boundaries").is_none());
            assert!(data.get("session_impact").is_none());
            assert!(data.get("next_actions").is_none());
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn offline_management_saves_and_reads_with_unidentified_runtime_without_false_absence() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        store
            .write_raw("resident.sock", b"not a resident endpoint")
            .unwrap();
        let add = request(
            &["provider", "add"],
            &["p"],
            &[("integration", &["deepseek"])],
        );
        let saved = dispatch(&store, &add).await.unwrap();
        let response = success_response("provider add", false, saved);
        assert_eq!(response.exit_code, 0);
        let file = config::load(&store).unwrap();
        assert!(file.config.providers.contains_key("p"));
        assert_eq!(response.body["data"]["file_revision"], file.revision);
        assert_eq!(response.body["data"]["runtime"], "unavailable");
        assert_eq!(
            response.body["data"]["activation"]["reason_code"],
            "resident_identity_unavailable"
        );
        assert!(response.body["data"]["loaded_revision_after"].is_null());
        let bytes = std::fs::read(store.root.join("tmax.toml")).unwrap();
        let inspect = dispatch(&store, &request(&["provider", "inspect"], &["p"], &[]))
            .await
            .unwrap();
        assert_eq!(inspect["provider"]["integration"], "deepseek");
        assert_eq!(
            inspect["endpoints"]["loaded"]["snapshot_state"],
            "unavailable"
        );
        let loaded = dispatch(
            &store,
            &request(&["config", "show"], &[], &[("state", &["loaded"])]),
        )
        .await
        .unwrap();
        assert_eq!(loaded["snapshot_state"], "unavailable");
        let mut update = request(
            &["provider", "update"],
            &["p"],
            &[("set", &["overrides.name", "local name"])],
        );
        update.dry_run = true;
        let preview = dispatch(&store, &update).await.unwrap();
        assert_eq!(
            preview["config_impact"]["activation_diff_state"],
            "unavailable"
        );
        assert!(preview["config_impact"]["includes_pending_file_changes"].is_null());
        assert!(preview["session_impact"]["sessions"].is_null());
        assert!(
            preview["changes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|change| change["boundary"] == "file_save")
        );
        assert!(!preview["changes"].as_array().unwrap().is_empty());
        assert_eq!(std::fs::read(store.root.join("tmax.toml")).unwrap(), bytes);
        let reload = dispatch(&store, &request(&["config", "reload"], &[], &[]))
            .await
            .unwrap();
        assert_eq!(
            success_response("config reload", false, reload).exit_code,
            4
        );
        assert_eq!(
            std::fs::read(store.root.join("resident.sock")).unwrap(),
            b"not a resident endpoint"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn offline_management_keeps_security_failures_and_revision_conflicts_explicit() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        store
            .write_raw("routing-state.json", b"corrupt safety state")
            .unwrap();
        let add = request(
            &["provider", "add"],
            &["p"],
            &[("integration", &["deepseek"])],
        );
        let data = dispatch(&store, &add).await.unwrap();
        let response = success_response("provider add", false, data);
        assert_eq!(response.exit_code, 4);
        assert_eq!(response.body["status"], "incomplete");
        assert_eq!(response.body["data"]["save"], "saved");
        assert_eq!(response.body["data"]["reconciliation"]["state"], "failed");
        let committed = std::fs::read(store.root.join("tmax.toml")).unwrap();
        let mut stale = request(
            &["provider", "update"],
            &["p"],
            &[("set", &["enabled", "false"])],
        );
        stale.if_revision = Some("stale".into());
        assert_eq!(
            dispatch(&store, &stale).await.unwrap_err().code,
            "inputs_changed"
        );
        assert_eq!(
            std::fs::read(store.root.join("tmax.toml")).unwrap(),
            committed
        );
        store
            .write_raw(
                "tmax.toml",
                b"version=1\n[gateway]\nlisten='127.0.0.1:12345'\nkey_ref='retained'\n",
            )
            .unwrap();
        store.write_raw("resident.sock", b"unidentified").unwrap();
        let before = std::fs::read(store.root.join("tmax.toml")).unwrap();
        assert_eq!(
            dispatch(&store, &request(&["gateway", "remove"], &[], &[]))
                .await
                .unwrap_err()
                .code,
            "resident_identity_unavailable"
        );
        assert_eq!(std::fs::read(store.root.join("tmax.toml")).unwrap(), before);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn offline_management_input_inspection_keeps_declarations_and_runtime_unknown() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        store.write_raw("tmax.toml", b"version=1\n[providers.p]\nintegration='deepseek'\n[providers.p.models.m]\n[providers.p.inputs.q]\nkind='quota'\nsource='manual'\nscope='weekly'\nmodels=['m']\nmax_age_seconds=60\n").unwrap();
        store.write_raw("resident.sock", b"unidentified").unwrap();
        let before = std::fs::read(store.root.join("tmax.toml")).unwrap();
        let inspect = dispatch(
            &store,
            &request(&["provider", "input", "inspect"], &["p", "q"], &[]),
        )
        .await
        .unwrap();
        assert_eq!(inspect["input"]["declaration"]["source"], "manual");
        assert_eq!(inspect["runtime"]["snapshot_state"], "unavailable");
        assert_eq!(
            inspect["input"]["current_runtime_use"]["state"],
            "unavailable"
        );
        assert_eq!(inspect["input"]["acquisition"]["state"], "unavailable");
        let loaded = dispatch(
            &store,
            &request(
                &["provider", "input", "list"],
                &["p"],
                &[("state", &["loaded"])],
            ),
        )
        .await
        .unwrap();
        assert_eq!(loaded["snapshot_state"], "unavailable");
        assert!(loaded.get("inputs").is_none());
        assert_eq!(std::fs::read(store.root.join("tmax.toml")).unwrap(), before);
    }

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
            crate::native::apply(&store, &config, "target", true)
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

    #[test]
    fn exact_duration_and_no_document_input() {
        assert_eq!(duration("1d").unwrap(), 86400);
        for bad in ["0s", "1.5h", "1h30m", "10", "1H", "-1s"] {
            assert!(duration(bad).is_err());
        }
    }
}
