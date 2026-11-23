use super::document::{error, get, put, remove, string};
use super::fields::require_change;
use crate::config::GATEWAY_PROVIDER_ID;
use crate::operations::arguments::{arg, args, operand, required};
use crate::{Error, Request, Result};
use serde_json::{Value as Json, json};
use toml::Value;

pub(super) fn mutate(root: &mut Value, r: &Request, path: &str, info: &mut Json) -> Result<()> {
    match path {
        "network direct" => {
            remove(root, &["network"])?;
        }
        "network proxy" => {
            put(
                root,
                &["network"],
                Value::Table(toml::map::Map::from_iter([(
                    "proxy_url".into(),
                    string(operand(r, 0)?),
                )])),
            )?;
        }
        "gateway configure" => {
            require_change(r)?;
            let exists = root.get("gateway").is_some();
            if !exists {
                put(
                    root,
                    &["gateway"],
                    Value::Table(toml::map::Map::from_iter([
                        ("listen".into(), string(required(r, "listen")?)),
                        ("key_ref".into(), string("__credential_provision_pending__")),
                    ])),
                )?;
            } else if let Some(listen) = arg(r, "listen") {
                put(root, &["gateway", "listen"], string(listen))?;
            }
            for chunk in args(r, "set").chunks(2) {
                let [field, value] = chunk else {
                    return Err(error());
                };
                match (field.as_str(), value.as_str()) {
                    ("autostart", "true") => {
                        put(root, &["gateway", "autostart"], Value::Boolean(true))?
                    }
                    ("autostart", "false") => {
                        put(root, &["gateway", "autostart"], Value::Boolean(false))?
                    }
                    _ => return Err(error()),
                }
            }
            for clear in args(r, "clear") {
                if clear != "autostart" {
                    return Err(error());
                }
                remove(root, &["gateway", "autostart"])?;
            }
            // The gateway owns provider `tmax`: report and drop any stored copy so the
            // preview and the saved document describe the same adopted result.
            if let Some(adopted) = stored_gateway_provider(root) {
                remove(root, &["providers", GATEWAY_PROVIDER_ID])?;
                if root
                    .get("providers")
                    .and_then(Value::as_table)
                    .is_some_and(toml::map::Map::is_empty)
                {
                    remove(root, &["providers"])?;
                }
                info["adopted_provider"] = adopted;
            }
            info["provision_gateway_key"] = json!(!exists);
        }
        "gateway key replace" => {
            get(root, &["gateway"])?;
            put(
                root,
                &["gateway", "key_ref"],
                string("__credential_provision_pending__"),
            )?;
        }
        "gateway remove" => {
            if root.get("gateway").is_none() {
                return Err(error());
            }
            if let Some(entries) = gateway_reference_entries(root) {
                return Err(Error::new(
                    "referenced_resource",
                    &format!(
                        "The gateway provider is still referenced by declared agent entries\nRemove the entries first: {}",
                        entries.join(", ")
                    ),
                )
                .field("agents"));
            }
            if let Some(candidates) = gateway_alias_candidates(root) {
                return Err(Error::new(
                    "referenced_resource",
                    &format!(
                        "The gateway provider is still referenced by model alias candidates\nRemove the candidates first: {}",
                        candidates.join(", ")
                    ),
                )
                .field("model_aliases"));
            }
            remove(root, &["gateway"])?;
            info["stored_secrets"] = json!("retained");
        }
        _ => return Err(error()),
    }
    Ok(())
}

/// The stored provider definition under the reserved ID, reported before the gateway adopts it.
fn stored_gateway_provider(root: &Value) -> Option<Json> {
    let stored = root
        .get("providers")
        .and_then(Value::as_table)
        .and_then(|providers| providers.get(GATEWAY_PROVIDER_ID))
        .and_then(Value::as_table)?;
    let previous_models = stored
        .get("models")
        .and_then(Value::as_table)
        .map_or(0, toml::map::Map::len);
    Some(json!({
        "id": GATEWAY_PROVIDER_ID,
        "stored": true,
        "removed": true,
        "previous_integration": stored.get("integration").and_then(Value::as_str),
        "previous_endpoints": stored.get("endpoints"),
        "previous_authentication": stored.get("auth").and_then(|auth| auth.get("method")).and_then(Value::as_str),
        "previous_models": previous_models,
    }))
}

/// Target/entry pairs whose direct provider entry names the derived gateway provider.
fn gateway_reference_entries(root: &Value) -> Option<Vec<String>> {
    let mut entries = Vec::new();
    if let Some(agents) = root.get("agents").and_then(Value::as_table) {
        for (target, declaration) in agents {
            if let Some(providers) = declaration.get("providers").and_then(Value::as_table) {
                for (name, entry) in providers {
                    if entry.get("provider").and_then(Value::as_str) == Some(GATEWAY_PROVIDER_ID) {
                        entries.push(format!("{target}/{name}"));
                    }
                }
            }
        }
    }
    entries.sort();
    (!entries.is_empty()).then_some(entries)
}

/// Alias candidates that cannot resolve once the derived provider is removed.
fn gateway_alias_candidates(root: &Value) -> Option<Vec<String>> {
    let mut candidates = Vec::new();
    if let Some(aliases) = root.get("model_aliases").and_then(Value::as_array) {
        for alias in aliases {
            let Some(names) = alias.get("candidates").and_then(Value::as_array) else {
                continue;
            };
            for name in names {
                if let Some(name) = name.as_str()
                    && name.starts_with(&format!("{GATEWAY_PROVIDER_ID}/"))
                {
                    candidates.push(name.to_string());
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    (!candidates.is_empty()).then_some(candidates)
}

#[cfg(test)]
mod tests {
    use crate::{Request, config, operations::network_view, storage::Store};
    use serde_json::json;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn proxy_and_direct_save_valid_policy_without_changing_invalid_or_previewed_state() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().into());
        let original = b"# retained settings\nversion = 2\n";
        store.write_raw("tmax.toml", original).unwrap();
        let mut request = Request {
            command: vec!["network".into(), "proxy".into()],
            operands: vec!["socks5://127.0.0.1".into()],
            arguments: Default::default(),
            dry_run: true,
            if_revision: None,
        };
        super::super::execute(&store, &request).await.unwrap();
        assert_eq!(
            std::fs::read(store.root.join("tmax.toml")).unwrap(),
            original.as_slice()
        );

        request.dry_run = false;
        super::super::execute(&store, &request).await.unwrap();
        let loaded = config::load(&store).unwrap();
        assert!(loaded.document.to_string().contains("# retained settings"));
        let configured = network_view(&loaded.config, &loaded.revision, true, "file");
        assert_eq!(
            configured["policy"]["configured"],
            json!({"proxy_url":"socks5://127.0.0.1"})
        );
        assert_eq!(configured["policy"]["effective"]["mode"], "proxy");
        assert_eq!(configured["policy"]["effective"]["proxy_port"], 1080);

        request.operands = vec!["socks5://user:password@127.0.0.1".into()];
        assert!(super::super::execute(&store, &request).await.is_err());
        assert_eq!(config::load(&store).unwrap().revision, loaded.revision);

        request.operands = vec!["https://127.0.0.1:8443".into()];
        super::super::execute(&store, &request).await.unwrap();
        let replacement = config::load(&store).unwrap();
        let policy = network_view(&replacement.config, &replacement.revision, true, "file");
        assert_eq!(
            policy["policy"]["configured"],
            json!({"proxy_url":"https://127.0.0.1:8443"})
        );
        assert_eq!(policy["policy"]["effective"]["proxy_port"], 8443);

        request.command = vec!["network".into(), "direct".into()];
        request.operands.clear();
        super::super::execute(&store, &request).await.unwrap();
        let loaded = config::load(&store).unwrap();
        assert!(loaded.config.network.is_none());
        let direct = network_view(&loaded.config, &loaded.revision, true, "file");
        assert_eq!(direct["policy"]["configured"], json!(null));
        assert_eq!(direct["policy"]["effective"]["mode"], "direct");
        assert_eq!(direct["policy"]["effective"]["proxy_url"], json!(null));
    }
    const GATEWAY: &str =
        "version=2\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='gateway-reference'\n";
    const ADOPTED: &str = "# personal settings\nversion=2\n[providers.tmax]\nintegration='custom'\n[providers.tmax.endpoints]\nchat-completions='http://127.0.0.1:39999'\n[providers.tmax.auth]\nmethod='api-key'\nkey_ref='stored-reference'\n[providers.tmax.models.route]\nenabled=true\n";

    fn gateway_request(
        command: &[&str],
        operands: &[&str],
        arguments: &[(&str, &[&str])],
    ) -> Request {
        Request {
            command: command.iter().map(|value| (*value).into()).collect(),
            operands: operands.iter().map(|value| (*value).into()).collect(),
            arguments: arguments
                .iter()
                .map(|(name, values)| {
                    (
                        (*name).into(),
                        values.iter().map(|value| (*value).into()).collect(),
                    )
                })
                .collect::<BTreeMap<_, _>>(),
            dry_run: false,
            if_revision: None,
        }
    }
    fn gateway_store(text: &str) -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        store.write_raw("tmax.toml", text.as_bytes()).unwrap();
        (directory, store)
    }
    fn file_bytes(store: &Store) -> Vec<u8> {
        std::fs::read(store.root.join("tmax.toml")).unwrap()
    }

    #[tokio::test]
    async fn gateway_configure_reports_and_removes_a_stored_reserved_provider() {
        let adopted = json!({
            "id": "tmax",
            "stored": true,
            "removed": true,
            "previous_integration": "custom",
            "previous_endpoints": {"chat-completions": "http://127.0.0.1:39999"},
            "previous_authentication": "api-key",
            "previous_models": 1,
        });
        let (_directory, store) = gateway_store(ADOPTED);
        let mut request = gateway_request(
            &["gateway", "configure"],
            &[],
            &[
                ("listen", &["127.0.0.1:17842"]),
                ("set", &["autostart", "false"]),
            ],
        );
        request.dry_run = true;
        let preview = super::super::execute(&store, &request).await.unwrap();
        assert_eq!(preview["requested"]["adopted_provider"], adopted);
        assert_eq!(preview["requested"]["provision_gateway_key"], json!(true));
        assert_eq!(file_bytes(&store), ADOPTED.as_bytes());
        request.dry_run = false;
        let result = super::super::execute(&store, &request).await.unwrap();
        assert_eq!(result["adopted_provider"], adopted);
        assert_eq!(result["provision_gateway_key"], json!(true));
        let loaded = config::load(&store).unwrap();
        assert_eq!(
            loaded.config.gateway.as_ref().unwrap().listen,
            "127.0.0.1:17842"
        );
        assert_eq!(
            loaded.config.providers["tmax"].integration.as_deref(),
            Some("tmax-gateway")
        );
        let saved = loaded.document.to_string();
        assert!(!saved.contains("stored-reference"), "{saved}");
        assert!(!saved.contains("[providers"), "{saved}");
    }

    #[tokio::test]
    async fn gateway_removal_refuses_while_declared_entries_reference_the_provider() {
        let text = format!(
            "{GATEWAY}[agents.dsh-main]\nagent='dsh'\nscope='/tmp/dsh-main'\n[agents.dsh-main.providers.tmax]\nprovider='tmax'\n[agents.omp-main]\nagent='omp'\nscope='/tmp/omp-main'\n[agents.omp-main.providers.tmax]\nprovider='tmax'\n"
        );
        let (_directory, store) = gateway_store(&text);
        let before = file_bytes(&store);
        let error =
            super::super::execute(&store, &gateway_request(&["gateway", "remove"], &[], &[]))
                .await
                .unwrap_err();
        assert_eq!(error.code, "referenced_resource");
        assert_eq!(error.field.as_deref(), Some("agents"));
        assert_eq!(
            error.message,
            "The gateway provider is still referenced by declared agent entries\nRemove the entries first: dsh-main/tmax, omp-main/tmax"
        );
        assert_eq!(file_bytes(&store), before);
    }

    #[tokio::test]
    async fn gateway_removal_refuses_deterministically_on_alias_candidates() {
        let text = format!(
            "{GATEWAY}[[model_aliases]]\nid='route'\nprotocols=['chat-completions']\ncandidates=['tmax/route']\n"
        );
        let (_directory, store) = gateway_store(&text);
        let before = file_bytes(&store);
        let error =
            super::super::execute(&store, &gateway_request(&["gateway", "remove"], &[], &[]))
                .await
                .unwrap_err();
        assert_eq!(error.code, "referenced_resource");
        assert_eq!(error.field.as_deref(), Some("model_aliases"));
        assert!(
            error
                .message
                .ends_with("Remove the candidates first: tmax/route"),
            "{}",
            error.message
        );
        assert_eq!(file_bytes(&store), before);
    }

    #[tokio::test]
    async fn gateway_removal_is_reachable_after_the_referencing_entries_are_removed() {
        let text = format!(
            "{GATEWAY}[agents.dsh-main]\nagent='dsh'\nscope='/tmp/dsh-main'\n[agents.dsh-main.providers.tmax]\nprovider='tmax'\n"
        );
        let (_directory, store) = gateway_store(&text);
        assert_eq!(
            super::super::execute(&store, &gateway_request(&["gateway", "remove"], &[], &[]))
                .await
                .unwrap_err()
                .code,
            "referenced_resource"
        );
        super::super::execute(
            &store,
            &gateway_request(&["agent", "provider", "remove"], &["dsh-main", "tmax"], &[]),
        )
        .await
        .unwrap();
        let result =
            super::super::execute(&store, &gateway_request(&["gateway", "remove"], &[], &[]))
                .await
                .unwrap();
        assert_eq!(result["save"], json!("saved"));
        let loaded = config::load(&store).unwrap();
        assert!(loaded.config.gateway.is_none());
        assert!(!loaded.config.providers.contains_key("tmax"));
    }
}
