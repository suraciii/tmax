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
                Value::Table(toml::map::Map::from_iter([
                    ("mode".into(), string("proxy")),
                    ("proxy_url".into(), string(operand(r, 0)?)),
                ])),
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
        "gateway autostart enable" | "gateway autostart disable" => {
            get(root, &["gateway"])?;
            put(
                root,
                &["gateway", "autostart"],
                Value::Boolean(path.ends_with("enable")),
            )?;
        }
        _ => return Err(error()),
    }
    Ok(())
}
