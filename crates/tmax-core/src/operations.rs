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
        return configuration::reset(store, r).await;
    }
    if path == "secret cleanup" {
        return secret_commands::cleanup(store, r).await;
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
        return Ok(
            json!({"snapshot_state":if file.exists{"available"}else{"absent"},"valid":true,"revision":file.revision,"external_availability":"not_verified"}),
        );
    }
    if path.starts_with("catalog ") {
        return crate::catalog::handle(store, config, r).await;
    }
    if path == "provider list" {
        return inspection::provider_list(store, &file);
    }
    if path == "provider inspect" {
        return inspection::provider_inspect(store, &file, r).await;
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
    #[cfg(unix)]
    use super::*;
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
}
