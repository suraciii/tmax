use super::document::{error, get, get_mut, new_named, put, remove, string, table};
use super::fields::require_change;
use crate::operations::arguments::{arg, operand, required};
use crate::{Error, Request, Result};
use serde_json::{Value as Json, json};
use toml::Value;

pub(super) fn mutate(root: &mut Value, r: &Request, path: &str, info: &mut Json) -> Result<()> {
    match path {
        "agent add" => {
            let id = operand(r, 0)?;
            let mut target = table();
            put(&mut target, &["agent"], string(required(r, "agent")?))?;
            put(&mut target, &["scope"], string(required(r, "path")?))?;
            put(&mut target, &["providers"], table())?;
            new_named(root, &["agents"], id, target)?;
        }
        "agent update" => {
            require_change(r)?;
            let t = get_mut(root, &["agents", operand(r, 0)?])?;
            for (flag, key) in [("agent", "agent"), ("path", "scope")] {
                if let Some(v) = arg(r, flag) {
                    put(t, &[key], string(v))?;
                }
            }
        }
        "agent remove" => {
            if remove(root, &["agents", operand(r, 0)?])?.is_none() {
                return Err(error());
            }
            info["native_files"] =
                json!("unchanged; restore information and native credentials retained");
        }
        "agent rename" => {
            let old = operand(r, 0)?;
            let new = operand(r, 1)?;
            let target = remove(root, &["agents", old])?.ok_or_else(error)?;
            new_named(root, &["agents"], new, target)?;
            if let Some(providers) = root.get_mut("providers").and_then(Value::as_table_mut) {
                for p in providers.iter_mut().map(|(_, value)| value) {
                    if p.get("auth")
                        .and_then(|a| a.get("via"))
                        .and_then(Value::as_str)
                        == Some(old)
                    {
                        put(p, &["auth", "via"], string(new))?;
                    }
                }
            }
            info["target_rename"] = json!([old, new]);
        }
        "agent provider add" | "agent provider update" | "agent provider remove" => {
            let target = operand(r, 0)?;
            let name = operand(r, 1)?;
            get(root, &["agents", target])?;
            if path.ends_with("remove") {
                if remove(root, &["agents", target, "providers", name])?.is_none() {
                    return Err(error());
                }
            } else {
                if arg(r, "provider").is_some() == arg(r, "gateway").is_some() {
                    return Err(error());
                }
                let mut entry = if path.ends_with("add") {
                    table()
                } else {
                    get(root, &["agents", target, "providers", name])?.clone()
                };
                remove(&mut entry, &["provider"])?;
                remove(&mut entry, &["gateway"])?;
                if let Some(provider) = arg(r, "provider") {
                    put(&mut entry, &["provider"], string(provider))?;
                } else {
                    put(&mut entry, &["gateway"], Value::Boolean(true))?;
                }
                if path.ends_with("add") {
                    put(&mut entry, &["models"], Value::Array(Vec::new()))?;
                    new_named(root, &["agents", target, "providers"], name, entry)?;
                } else {
                    put(root, &["agents", target, "providers", name], entry)?;
                }
            }
        }
        "agent model add" | "agent model remove" | "agent model clear" => {
            let target = operand(r, 0)?;
            let name = operand(r, 1)?;
            let e = get_mut(root, &["agents", target, "providers", name])?;
            if e.get("models").is_none() {
                put(e, &["models"], Value::Array(Vec::new()))?;
            }
            let list = e
                .get_mut("models")
                .and_then(Value::as_array_mut)
                .ok_or_else(error)?;
            if path.ends_with("clear") {
                list.clear();
            } else {
                let id = operand(r, 2)?;
                let protocol = required(r, "protocol")?;
                let pos = list.iter().position(|m| {
                    m.get("id").and_then(Value::as_str) == Some(id)
                        && m.get("protocol").and_then(Value::as_str) == Some(protocol)
                });
                if path.ends_with("add") {
                    if pos.is_some() {
                        return Err(error());
                    }
                    list.push(selection(id, protocol));
                } else {
                    list.remove(pos.ok_or_else(error)?);
                }
            }
        }
        "agent default set" => {
            put(
                root,
                &["agents", operand(r, 0)?, "default"],
                reference(operand(r, 1)?, operand(r, 2)?, required(r, "protocol")?),
            )?;
        }
        "agent default clear" => {
            get(root, &["agents", operand(r, 0)?])?;
            remove(root, &["agents", operand(r, 0)?, "default"])?;
        }
        "agent role set" => {
            let target = operand(r, 0)?;
            let role = operand(r, 1)?;
            let agent = get(root, &["agents", target, "agent"])?
                .as_str()
                .ok_or_else(error)?;
            if !crate::agent_integration::supported_roles(agent).contains(&role) {
                return Err(Error::new(
                    "invalid_arguments",
                    "This native Agent does not declare that model role",
                ));
            }
            put(
                root,
                &["agents", target, "roles", role],
                reference(operand(r, 2)?, operand(r, 3)?, required(r, "protocol")?),
            )?;
        }
        "agent role remove" => {
            if remove(root, &["agents", operand(r, 0)?, "roles", operand(r, 1)?])?.is_none() {
                return Err(error());
            }
        }
        "agent role clear" => {
            get(root, &["agents", operand(r, 0)?])?;
            remove(root, &["agents", operand(r, 0)?, "roles"])?;
        }
        _ => return Err(error()),
    }
    Ok(())
}
fn selection(id: &str, p: &str) -> Value {
    Value::Table(toml::map::Map::from_iter([
        ("id".into(), string(id)),
        ("protocol".into(), string(p)),
    ]))
}
fn reference(name: &str, id: &str, p: &str) -> Value {
    let mut v = selection(id, p);
    v.as_table_mut()
        .unwrap()
        .insert("provider".into(), string(name));
    v
}
