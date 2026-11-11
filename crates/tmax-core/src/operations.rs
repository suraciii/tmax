mod arguments;
pub mod commands;
mod configuration;
mod edit;
mod inspection;
mod references;
mod results;
mod secret_commands;

pub(crate) use arguments::{arg, operand, required};
pub(crate) use configuration::{activate_saved, diff_values, impact};
pub(crate) use inspection::{explain, network_view};

use crate::{
    Error, Request, Response, Result,
    config::{self, Config},
    storage::Store,
};
use results::{error_response, success_response};
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
#[cfg(test)]
pub(crate) async fn test_dispatch(store: &Store, request: &Request) -> Result<Value> {
    dispatch(store, request).await
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
        return Ok(json!({"integrations":crate::provider_integration::registry()}));
    }
    if path == "integration inspect" {
        return crate::provider_integration::inspect(operand(r, 0)?);
    }
    if path == "secret list" {
        return secret_commands::secret_inventory(store).await;
    }
    if path.starts_with("config recovery ") {
        return configuration::recovery(store, r);
    }
    if path == "agent recovery list"
        || path == "agent recovery inspect"
        || path == "agent recovery discard"
        || path == "agent recovery accept"
    {
        return crate::agent_integration::recovery(store, r).await;
    }
    if path == "agent restore" {
        return crate::agent_integration::restore_request(store, r).await;
    }
    if path == "agent discover" {
        return crate::agent_integration::discover(store, arg(r, "agent"), None).await;
    }
    if path == "agent inspect" && r.operands.is_empty() {
        return crate::agent_integration::discover(
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
        let daemon = crate::service::status(store).await?;
        return Ok(match file {
            Ok(f) => {
                json!({"config_path":store.root.join("tmax.toml"),"file_revision":f.revision,"configuration_state":if f.exists{"valid"}else{"absent"},"providers":f.config.providers.len(),"targets":f.config.agents.len(),"daemon":daemon})
            }
            Err(e) => {
                json!({"config_path":store.root.join("tmax.toml"),"configuration_state":"invalid","reason_code":e.code,"field":e.field,"daemon":daemon,"next_action":"config reset or correct supported fields"})
            }
        });
    }
    if path == "config reset" {
        return configuration::reset(store, r).await;
    }
    if path == "secret cleanup" {
        return secret_commands::cleanup(store, r).await;
    }
    // Reuse/stop controls consult the identified daemon even when the saved file is invalid.
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
        return configuration::reload(store, r).await;
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
    if path == "dsh projection" {
        return crate::agent_integration::dsh_projection(store, r).await;
    }
    let file = match inspection::selected(store, r).await {
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
        return configuration::validate(store, &file);
    }
    if path.starts_with("catalog ") {
        return crate::catalog::handle(store, config, r).await;
    }
    if path == "provider list" {
        return inspection::provider_list(store, &file, r).await;
    }
    if path == "provider inspect" {
        return inspection::provider_inspect(store, &file, r).await;
    }
    if path == "provider failure-reset" {
        let provider = operand(r, 0)?;
        if r.if_revision
            .as_ref()
            .is_some_and(|expected| expected != &file.revision)
        {
            return Err(Error::new(
                "inputs_changed",
                "Saved configuration differs from the requested revision",
            ));
        }
        if r.dry_run {
            let state = crate::failures::inspect(store, config, provider, chrono::Utc::now())?;
            return Ok(
                json!({"provider":provider,"reset":false,"failure_state":state,"requested":"provider failure-reset"}),
            );
        }
        let _lock = store.lock("configuration")?;
        config::guard(store, &file.revision)?;
        return crate::failures::reset(store, config, provider);
    }
    if path == "provider quota-resume" {
        let provider = operand(r, 0)?;
        if r.if_revision
            .as_ref()
            .is_some_and(|expected| expected != &file.revision)
        {
            return Err(Error::new(
                "inputs_changed",
                "Saved configuration differs from the requested revision",
            ));
        }
        let loaded = crate::service::loaded_snapshot(store).await;
        let mut loaded_state = inspection::loaded_quota_projection(&loaded, provider);
        let mut same_provider = false;
        if let Ok(Some(snapshot)) = &loaded {
            let saved_ids =
                crate::inputs::Snapshot::capture(store, config).provider_scope_binding(provider);
            let loaded_ids =
                crate::inputs::Snapshot::capture_loaded(store, &snapshot.file.config, None)
                    .provider_scope_binding(provider);
            same_provider = saved_ids
                .zip(loaded_ids)
                .is_some_and(|(saved, loaded)| saved.0 == loaded.0)
                && snapshot
                    .file
                    .config
                    .providers
                    .get(provider)
                    .is_some_and(|p| {
                        crate::provider_integration::integration_id(p).ok() == Some("opencode-go")
                    });
            if !same_provider {
                loaded_state = json!({"state":"unavailable","reason_code":"binding_mismatch"});
            }
        }
        if r.dry_run {
            let file_state = crate::observations::quota_status(store, config, provider, false)?;
            let mut state =
                if file_state["state"] == "suspended" || file_state["state"] == "unsupported" {
                    file_state.clone()
                } else {
                    loaded_state.clone()
                };
            state["views"] = json!({"file":file_state,"loaded":loaded_state});
            return Ok(
                json!({"provider":provider,"resumed":false,"quota_suspension":state,"requested":"provider quota-resume"}),
            );
        }
        let _lock = store.lock("configuration")?;
        config::guard(store, &file.revision)?;
        let mut result = crate::observations::resume_quota_locked(store, config, provider)?;
        if same_provider
            && loaded_state["state"] == "suspended"
            && result["state"] == "manually_resumed"
        {
            result["resumed"] = json!(true);
        } else if result["state"] == "manually_resumed"
            && result["resumed"] == false
            && loaded_state["state"] == "unavailable"
            && !matches!(
                loaded_state["reason_code"].as_str(),
                Some("no_loaded_snapshot" | "provider_missing")
            )
        {
            result["resumed"] = Value::Null;
        }
        return Ok(result);
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
            crate::provider_integration::observation_destination(provider, scope)?;
        }
        let mut data =
            crate::provider_integration::refresh(store, config, operand(r, 0)?, scope, r.dry_run)
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
        return inspection::model_list(config, r);
    }
    if path == "model inspect" {
        return inspection::model_inspect(store, &file, r);
    }
    if path == "model explain" {
        return explain(store, config, r).await;
    }
    if path == "model rule list" || path == "model rule inspect" {
        return inspection::model_rules(store, config, r, &path);
    }
    if path == "agent list" {
        return inspection::agent_list(config);
    }
    if path == "agent inspect" {
        return crate::agent_integration::inspect(store, config, operand(r, 0)?).await;
    }
    if path == "agent apply" {
        return crate::agent_integration::apply_request(store, config, r).await;
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
        return secret_commands::key_show(store, config, r).await;
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

#[cfg(test)]
mod tests {
    use super::*;
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
    #[tokio::test]
    async fn provider_quota_resume_previews_guards_revision_and_preserves_disablement() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        let text = "version=1\n[providers.p]\nintegration='opencode-go'\nenabled=false\n";
        store.write_raw("tmax.toml", text.as_bytes()).unwrap();
        let config = config::parse(text).unwrap();
        crate::inputs::reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        crate::observations::test_publish_quota(
            &store,
            &config,
            "p",
            json!({
                "usage": {
                    "rolling":{"status":"ok","percent":1,"resetsAt":"2099-01-01T00:00:00Z"},
                    "weekly":{"status":"rate-limited","percent":100,"resetsAt":"2099-01-01T00:00:00Z"},
                    "monthly":{"status":"ok","percent":1,"resetsAt":"2099-01-01T00:00:00Z"}
                },
                "percent_basis":"consumed","percent_precision":"floored","reset_basis":"announcement"
            }),
        );
        let state_path = store.root.join("observations/heads.json");
        let before = std::fs::read(&state_path).unwrap();
        let revision = config::revision(text.as_bytes());
        let list = dispatch(&store, &request(&["provider", "list"], &[], &[]))
            .await
            .unwrap();
        assert_eq!(
            list["providers"][0]["quota_suspension"]["views"]["file"]["state"],
            "suspended"
        );
        let inspect = dispatch(&store, &request(&["provider", "inspect"], &["p"], &[]))
            .await
            .unwrap();
        assert_eq!(
            inspect["quota_suspension"]["views"]["file"]["state"],
            "suspended"
        );
        assert_eq!(
            inspect["quota_suspension"]["views"]["loaded"]["state"],
            "unavailable"
        );
        assert_eq!(std::fs::read(&state_path).unwrap(), before);
        let mut command = request(&["provider", "quota-resume"], &["p"], &[]);
        command.dry_run = true;
        command.if_revision = Some(revision.clone());
        let preview = dispatch(&store, &command).await.unwrap();
        assert_eq!(preview["quota_suspension"]["state"], "suspended");
        assert_eq!(std::fs::read(&state_path).unwrap(), before);
        command.if_revision = Some("stale".into());
        assert_eq!(
            dispatch(&store, &command).await.unwrap_err().code,
            "inputs_changed"
        );
        assert_eq!(std::fs::read(&state_path).unwrap(), before);
        command.if_revision = Some(revision);
        command.dry_run = false;
        dispatch(&store, &command).await.unwrap();
        assert_eq!(
            crate::observations::quota_status(&store, &config, "p", false).unwrap()["state"],
            "manually_resumed"
        );
        dispatch(&store, &command).await.unwrap();
        assert_eq!(
            std::fs::read(store.root.join("tmax.toml")).unwrap(),
            text.as_bytes()
        );
        assert!(!config::load(&store).unwrap().config.providers["p"].enabled);
    }

    #[tokio::test]
    async fn provider_failure_reset_previews_without_mutation_and_checks_revision() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        let text = "version=1\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:39999'\n[providers.p.failure_policy]\nconsecutive_failures=1\npriority=-100\nduration_seconds=3600\nerrors={http_statuses=[503]}\n";
        store.write_raw("tmax.toml", text.as_bytes()).unwrap();
        let config = config::parse(text).unwrap();
        crate::inputs::reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        let runtime = crate::inputs::read_runtime(&store).unwrap();
        let (identity, authorization) =
            crate::inputs::provider_scope_binding(&runtime, &config, "p").unwrap();
        crate::failures::record(
            &store,
            &config,
            "p",
            &identity,
            &authorization,
            &crate::failures::epoch(&store, &config, "p").unwrap(),
            false,
            Some(503),
            None,
            chrono::Utc::now(),
        )
        .unwrap();
        let state_path = store.root.join("provider-failures/state.json");
        let before = std::fs::read(&state_path).unwrap();
        let mut request = request(&["provider", "failure-reset"], &["p"], &[]);
        request.dry_run = true;
        request.if_revision = Some(config::revision(text.as_bytes()));
        let preview = dispatch(&store, &request).await.unwrap();
        assert_eq!(preview["reset"], false);
        assert_eq!(preview["failure_state"]["active_priority"], -100);
        let response = success_response("provider failure-reset", true, preview);
        assert_eq!(response.exit_code, 0);
        assert_eq!(response.body["status"], "dry_run");
        assert_eq!(std::fs::read(&state_path).unwrap(), before);
        request.if_revision = Some("stale".into());
        assert_eq!(
            dispatch(&store, &request).await.unwrap_err().code,
            "inputs_changed"
        );
        assert_eq!(std::fs::read(&state_path).unwrap(), before);
        request.if_revision = Some(config::revision(text.as_bytes()));
        request.dry_run = false;
        let reset = dispatch(&store, &request).await.unwrap();
        let response = success_response("provider failure-reset", false, reset);
        assert_eq!(response.exit_code, 0);
        assert_eq!(response.body["operation"], "provider failure-reset");
        assert_eq!(response.body["status"], "success");
        let state = crate::failures::inspect(&store, &config, "p", chrono::Utc::now()).unwrap();
        assert!(state["active_priority"].is_null());
        assert_eq!(state["consecutive_failures"], 0);
        assert_eq!(
            std::fs::read(store.root.join("tmax.toml")).unwrap(),
            text.as_bytes()
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn offline_management_saves_and_reads_with_unidentified_runtime_without_false_absence() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        store
            .write_raw("daemon.sock", b"not a daemon endpoint")
            .unwrap();
        let add = request(
            &["provider", "add"],
            &["p"],
            &[("set", &["integration", "deepseek"])],
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
            "daemon_identity_unavailable"
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
            std::fs::read(store.root.join("daemon.sock")).unwrap(),
            b"not a daemon endpoint"
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
            &[("set", &["integration", "deepseek"])],
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
        store.write_raw("daemon.sock", b"unidentified").unwrap();
        let before = std::fs::read(store.root.join("tmax.toml")).unwrap();
        assert_eq!(
            dispatch(&store, &request(&["gateway", "remove"], &[], &[]))
                .await
                .unwrap_err()
                .code,
            "daemon_identity_unavailable"
        );
        assert_eq!(std::fs::read(store.root.join("tmax.toml")).unwrap(), before);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn offline_management_input_inspection_keeps_declarations_and_runtime_unknown() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        store.write_raw("tmax.toml", b"version=1\n[providers.p]\nintegration='deepseek'\n[providers.p.models.m]\n[providers.p.inputs.q]\nkind='quota'\nsource='manual'\nscope='weekly'\nmodels=['m']\nmax_age_seconds=60\n").unwrap();
        store.write_raw("daemon.sock", b"unidentified").unwrap();
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
    async fn invalid_saved_effort_map_rejects_validation_and_reload_before_effects() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("state"));
        let config = b"version=1\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='https://example.invalid/v1'\n[providers.p.models.m.effort_map.chat-completions]\nmax='undeclared'\n[providers.p.models.m.overrides]\nreasoning_options=[{type='effort',values=['high']}]\n";
        store.write_raw("tmax.toml", config).unwrap();
        for (action, dry_run) in [("validate", false), ("reload", true), ("reload", false)] {
            let mut request = request(&["config", action], &[], &[]);
            request.dry_run = dry_run;
            let error = dispatch(&store, &request).await.unwrap_err();
            assert_eq!(error.code, "reasoning_effort_incompatible");
            assert_eq!(
                error.field.as_deref(),
                Some("providers.p.models.m.effort_map.chat-completions.max")
            );
            assert_eq!(std::fs::read(store.root.join("tmax.toml")).unwrap(), config);
            let files: Vec<_> = std::fs::read_dir(&store.root)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            assert_eq!(files, [std::ffi::OsString::from("tmax.toml")]);
        }
    }
}
