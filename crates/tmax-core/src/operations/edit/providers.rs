use super::document::{error, get, get_mut, new_named, put, remove, string, strings, table};
use super::fields::{apply_fields, require_change};
use super::references::rename_refs;
use crate::config::GATEWAY_PROVIDER_ID;
use crate::operations::arguments::{arg, args, duration, operand, required};
use crate::{Error, Request, Result};
use serde_json::{Value as Json, json};
use toml::Value;

fn gateway_configured(root: &Value) -> bool {
    root.get("gateway")
        .is_some_and(|gateway| gateway.as_table().is_some())
}
/// The configured gateway owns provider `tmax`; provider operations must refuse it.
fn gateway_reserved() -> Error {
    Error::new(
        "reserved_resource",
        &format!(
            "Provider ID \"{GATEWAY_PROVIDER_ID}\" is reserved for the gateway provider; the gateway owns it\nRun `tmax gateway remove` to stop managing the gateway provider"
        ),
    )
    .field("provider")
}
/// Provider `tmax` cannot be created by a provider operation, with or without a gateway.
fn reserved_creation() -> Error {
    Error::new(
        "reserved_resource",
        &format!(
            "Provider ID \"{GATEWAY_PROVIDER_ID}\" is reserved for the gateway provider; create it by configuring the gateway with `tmax gateway configure`"
        ),
    )
    .field("provider")
}
fn guard_gateway_provider(root: &Value, id: &str) -> Result<()> {
    if gateway_configured(root) && id == GATEWAY_PROVIDER_ID {
        return Err(gateway_reserved());
    }
    Ok(())
}
fn guard_provider_creation(root: &Value, id: &str) -> Result<()> {
    if id != GATEWAY_PROVIDER_ID {
        return Ok(());
    }
    if gateway_configured(root) {
        return Err(gateway_reserved());
    }
    Err(reserved_creation())
}

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
            guard_provider_creation(root, id)?;
            let mut p = table();
            apply_fields(&mut p, r, "provider")?;
            endpoints(&mut p, r)?;
            new_named(root, &["providers"], id, p)?;
            info["provider"] = json!(id);
        }
        "provider update" => {
            require_change(r)?;
            let id = operand(r, 0)?;
            guard_gateway_provider(root, id)?;
            let p = get_mut(root, &["providers", id])?;
            apply_fields(p, r, "provider")?;
            endpoints(p, r)?;
        }
        "provider rename" => {
            let old = operand(r, 0)?;
            let new = operand(r, 1)?;
            if gateway_configured(root)
                && (old == GATEWAY_PROVIDER_ID || new == GATEWAY_PROVIDER_ID)
            {
                return Err(gateway_reserved());
            }
            if new == GATEWAY_PROVIDER_ID {
                return Err(reserved_creation());
            }
            let p = remove(root, &["providers", old])?.ok_or_else(error)?;
            new_named(root, &["providers"], new, p)?;
            rename_refs(root, old, new, true)?;
            renames.push((old.into(), new.into()));
        }
        "provider remove" => {
            let id = operand(r, 0)?;
            guard_gateway_provider(root, id)?;
            info["retained_authentication"] = json!(get(root, &["providers", id])?.get("auth"));
            if remove(root, &["providers", id])?.is_none() {
                return Err(error());
            }
            info["native_files"] = json!("unchanged; direct consumers can continue outside tmax");
        }
        "provider input add" | "provider input replace" => {
            let p = operand(r, 0)?;
            guard_gateway_provider(root, p)?;
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
            let p = operand(r, 0)?;
            guard_gateway_provider(root, p)?;
            if remove(root, &["providers", p, "inputs", operand(r, 1)?])?.is_none() {
                return Err(error());
            }
        }
        "provider key set" => {
            let id = operand(r, 0)?;
            guard_gateway_provider(root, id)?;
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
            let id = operand(r, 0)?;
            guard_gateway_provider(root, id)?;
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
            let id = operand(r, 0)?;
            guard_gateway_provider(root, id)?;
            get(root, &["providers", id])?;
            remove(root, &["providers", id, "auth"])?;
        }
        "provider login" => {
            let id = operand(r, 0)?;
            guard_gateway_provider(root, id)?;
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

#[cfg(test)]
mod tests {
    use super::super::execute;
    use crate::storage::Store;
    use crate::{Request, config};
    use std::collections::BTreeMap;

    const GATEWAY: &str =
        "version=2\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='gateway-reference'\n";
    const STORED_RESERVED: &str = "version=2\n[providers.tmax]\nintegration='custom'\n[providers.tmax.endpoints]\nchat-completions='http://127.0.0.1:39999'\n";

    fn request(command: &[&str], operands: &[&str], arguments: &[(&str, &[&str])]) -> Request {
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
    fn store_with(text: &str) -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        store.write_raw("tmax.toml", text.as_bytes()).unwrap();
        (directory, store)
    }
    fn bytes(store: &Store) -> Vec<u8> {
        std::fs::read(store.root.join("tmax.toml")).unwrap()
    }

    #[tokio::test]
    async fn configured_gateway_owns_the_reserved_provider_id() {
        let (_directory, store) = store_with(GATEWAY);
        let before = bytes(&store);
        let cases = [
            request(
                &["provider", "add"],
                &["tmax"],
                &[("set", &["metadata", "retained"])],
            ),
            request(
                &["provider", "update"],
                &["tmax"],
                &[("set", &["metadata", "retained"])],
            ),
            request(&["provider", "rename"], &["tmax", "other"], &[]),
            request(&["provider", "rename"], &["other", "tmax"], &[]),
            request(&["provider", "remove"], &["tmax"], &[]),
            request(&["provider", "key", "set"], &["tmax"], &[]),
            request(
                &["provider", "auth", "set"],
                &["tmax"],
                &[("via", &["native-target"])],
            ),
            request(&["provider", "auth", "clear"], &["tmax"], &[]),
            request(
                &["provider", "login"],
                &["tmax"],
                &[("via", &["native-target"])],
            ),
            request(&["provider", "logout"], &["tmax"], &[]),
            request(
                &["provider", "input", "add"],
                &["tmax", "weekly"],
                &[
                    ("kind", &["quota"]),
                    ("source", &["manual"]),
                    ("scope", &["scope"]),
                    ("model", &["route"]),
                    ("max-age", &["3600"]),
                ],
            ),
            request(&["provider", "input", "replace"], &["tmax", "weekly"], &[]),
            request(&["provider", "input", "remove"], &["tmax", "weekly"], &[]),
        ];
        for case in cases {
            let operation = case.command.join(" ");
            let error = execute(&store, &case).await.unwrap_err();
            assert_eq!(error.code, "reserved_resource", "{operation}");
            assert_eq!(error.field.as_deref(), Some("provider"), "{operation}");
            assert!(
                error.message.contains("the gateway owns it"),
                "{operation}: {}",
                error.message
            );
            assert!(
                error.message.contains("tmax gateway remove"),
                "{operation}: {}",
                error.message
            );
            assert_eq!(bytes(&store), before, "{operation} must not write");
        }
        let dry_run = Request {
            dry_run: true,
            ..request(&["provider", "remove"], &["tmax"], &[])
        };
        assert_eq!(
            execute(&store, &dry_run).await.unwrap_err().code,
            "reserved_resource"
        );
        assert_eq!(bytes(&store), before);
    }

    #[tokio::test]
    async fn provider_creation_refuses_the_reserved_id_without_a_gateway() {
        let (_directory, store) = store_with("version=2\n");
        for case in [
            request(
                &["provider", "add"],
                &["tmax"],
                &[("set", &["integration", "deepseek"])],
            ),
            request(&["provider", "rename"], &["other", "tmax"], &[]),
        ] {
            let operation = case.command.join(" ");
            let error = execute(&store, &case).await.unwrap_err();
            assert_eq!(error.code, "reserved_resource", "{operation}");
            assert_eq!(error.field.as_deref(), Some("provider"), "{operation}");
            assert!(
                error.message.contains("tmax gateway configure"),
                "{operation}: {}",
                error.message
            );
        }
        assert_eq!(bytes(&store), b"version=2\n");
    }

    #[tokio::test]
    async fn stored_provider_named_tmax_remains_user_state_without_a_gateway() {
        let (_directory, store) = store_with(STORED_RESERVED);
        execute(
            &store,
            &request(
                &["provider", "update"],
                &["tmax"],
                &[("set", &["metadata", "mine"])],
            ),
        )
        .await
        .unwrap();
        let loaded = config::load(&store).unwrap();
        assert_eq!(
            loaded.config.providers["tmax"].metadata.as_deref(),
            Some("mine")
        );
        execute(
            &store,
            &request(&["provider", "rename"], &["tmax", "renamed"], &[]),
        )
        .await
        .unwrap();
        let loaded = config::load(&store).unwrap();
        assert!(!loaded.config.providers.contains_key("tmax"));
        assert!(loaded.config.providers.contains_key("renamed"));

        let (_directory, store) = store_with(STORED_RESERVED);
        execute(&store, &request(&["provider", "remove"], &["tmax"], &[]))
            .await
            .unwrap();
        assert!(
            !config::load(&store)
                .unwrap()
                .config
                .providers
                .contains_key("tmax")
        );
    }
}
