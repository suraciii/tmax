use super::document::{error, get, get_mut, new_named, put, remove, string, strings, table};
use super::fields::{apply_fields, require_change};
use super::references::rename_refs;
use crate::operations::arguments::{arg, args, duration, operand, required};
use crate::{Error, Request, Result};
use serde_json::{Value as Json, json};
use toml::Value;

pub(super) fn mutate(
    root: &mut Value,
    r: &Request,
    path: &str,
    info: &mut Json,
    renames: &mut Vec<(String, String)>,
) -> Result<()> {
    match path {
        "provider add" => {
            let id = operand(r, 0)?;
            let mut p = table();
            for key in ["metadata", "integration", "region"] {
                if let Some(v) = arg(r, key) {
                    if args(r, "set")
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .any(|p| p[0] == key)
                    {
                        return Err(error());
                    }
                    put(&mut p, &[key], string(v))?;
                }
            }
            apply_fields(&mut p, r, "provider")?;
            endpoints(&mut p, r)?;
            new_named(root, &["providers"], id, p)?;
            info["provider"] = json!(id);
        }
        "provider update" => {
            require_change(r)?;
            let p = get_mut(root, &["providers", operand(r, 0)?])?;
            apply_fields(p, r, "provider")?;
            endpoints(p, r)?;
        }
        "provider rename" => {
            let old = operand(r, 0)?;
            let new = operand(r, 1)?;
            let p = remove(root, &["providers", old])?.ok_or_else(error)?;
            new_named(root, &["providers"], new, p)?;
            rename_refs(root, old, new, true)?;
            renames.push((old.into(), new.into()));
        }
        "provider remove" => {
            let id = operand(r, 0)?;
            info["retained_authentication"] = json!(get(root, &["providers", id])?.get("auth"));
            if remove(root, &["providers", id])?.is_none() {
                return Err(error());
            }
            info["native_files"] = json!("unchanged; direct consumers can continue outside tmax");
        }
        "provider input add" | "provider input replace" => {
            let p = operand(r, 0)?;
            let name = operand(r, 1)?;
            let mut d = table();
            for key in ["kind", "source", "scope"] {
                put(&mut d, &[key], string(required(r, key)?))?;
            }
            if args(r, "model").is_empty() {
                return Err(error());
            }
            put(&mut d, &["models"], strings(args(r, "model")))?;
            put(
                &mut d,
                &["max_age_seconds"],
                Value::Integer(duration(required(r, "max-age")?)?),
            )?;
            if let Some(interval) = arg(r, "refresh-every") {
                put(
                    &mut d,
                    &["refresh_interval_seconds"],
                    Value::Integer(duration(interval)?),
                )?;
            }
            get(root, &["providers", p])?;
            if path.ends_with("add") {
                new_named(root, &["providers", p, "inputs"], name, d)?;
            } else {
                get(root, &["providers", p, "inputs", name])?;
                put(root, &["providers", p, "inputs", name], d)?;
            }
        }
        "provider input remove" => {
            if remove(
                root,
                &["providers", operand(r, 0)?, "inputs", operand(r, 1)?],
            )?
            .is_none()
            {
                return Err(error());
            }
        }
        "provider key set" => {
            let id = operand(r, 0)?;
            get(root, &["providers", id])?;
            let reference = arg(r, "key-ref").unwrap_or("__credential_provision_pending__");
            if arg(r, "key-ref").is_some() && arg(r, "key-stdin").is_some() {
                return Err(error());
            }
            put(
                root,
                &["providers", id, "auth"],
                Value::Table(toml::map::Map::from_iter([
                    ("method".into(), string("api-key")),
                    ("key_ref".into(), string(reference)),
                ])),
            )?;
        }
        "provider auth set" => {
            if required(r, "method")? != "login" {
                return Err(error());
            }
            let id = operand(r, 0)?;
            get(root, &["providers", id])?;
            let mut auth = table();
            put(&mut auth, &["method"], string("login"))?;
            put(&mut auth, &["via"], string(required(r, "via")?))?;
            if let Some(s) = arg(r, "session-ref") {
                put(&mut auth, &["session_ref"], string(s))?;
            }
            put(root, &["providers", id, "auth"], auth)?;
        }
        "provider auth clear" | "provider logout" => {
            get(root, &["providers", operand(r, 0)?])?;
            remove(root, &["providers", operand(r, 0)?, "auth"])?;
        }
        "provider login" => {
            let id = operand(r, 0)?;
            let p = get(root, &["providers", id])?;
            let via = arg(r, "via")
                .or_else(|| {
                    p.get("auth")
                        .and_then(|a| a.get("via"))
                        .and_then(Value::as_str)
                })
                .ok_or_else(|| {
                    Error::new(
                        "login_source_required",
                        "Select an explicit supported native login source",
                    )
                })?
                .to_string();
            let auth = Value::Table(toml::map::Map::from_iter([
                ("method".into(), string("login")),
                ("via".into(), string(&via)),
            ]));
            put(root, &["providers", id, "auth"], auth)?;
            info["via"] = json!(via);
        }
        _ => return Err(error()),
    }
    Ok(())
}
fn endpoints(p: &mut Value, r: &Request) -> Result<()> {
    let mut protocols = std::collections::BTreeSet::new();
    for protocol in args(r, "clear-endpoint") {
        if !protocols.insert(protocol) {
            return Err(error());
        }
        remove(p, &["endpoints", protocol])?;
    }
    for pair in args(r, "endpoint").as_chunks::<2>().0 {
        if !protocols.insert(&pair[0]) {
            return Err(error());
        }
        put(p, &["endpoints", &pair[0]], string(&pair[1]))?;
    }
    Ok(())
}
