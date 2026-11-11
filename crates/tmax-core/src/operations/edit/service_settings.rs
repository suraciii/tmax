use super::document::{error, get, put, remove, string};
use super::fields::require_change;
use crate::operations::arguments::{arg, args, operand, required};
use crate::{Request, Result};
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
            if remove(root, &["gateway"])?.is_none() {
                return Err(error());
            }
            info["stored_secrets"] = json!("retained");
        }
        _ => return Err(error()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{Request, config, operations::network_view, storage::Store};
    use serde_json::json;

    #[tokio::test]
    async fn proxy_and_direct_save_valid_policy_without_changing_invalid_or_previewed_state() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().into());
        let original = b"# retained settings\nversion = 1\n";
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
}
