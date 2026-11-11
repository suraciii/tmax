//! Connect one declared DSH target to the tmax gateway without managing DSH.
//!
//! Only stable credential reference names enter the DSH credentials document.
//! The gateway key is resolved at execution time and never returned, logged, or
//! written into a patch. No native route, artifact, or service is created.

use super::{
    adapters,
    document::{self, Format},
    dsh_projection::select_target,
    scope::{self, DshCredentialsLock},
};
use crate::{
    Error, Request, Result,
    config::{self, AgentTarget, Config},
    secrets, storage,
    storage::Store,
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};
use zeroize::Zeroize;

/// Connect the target selected by TARGET, --home, DSH_HOME, or the sole DSH
/// target. Dry-run never reads the DSH credential document or a secret.
pub async fn connect(store: &Store, request: &Request) -> Result<Value> {
    let file = config::load(store)?;
    let requested = request.operands.first().map(String::as_str);
    let home = request
        .arguments
        .get("home")
        .and_then(|values| values.first())
        .map(String::as_str);
    let (id, target) = select_target(&file.config, requested, home)?;
    run(store, &file.config, id, target, request.dry_run).await
}

struct Route {
    name: String,
    protocol: String,
    credential_ref: String,
}

fn routes(target: &AgentTarget) -> Result<Vec<Route>> {
    let mut routes = Vec::new();
    for (name, binding) in &target.providers {
        if binding.provider.is_some() || binding.gateway != Some(true) {
            continue;
        }
        if binding.models.is_empty() {
            return Err(Error::new(
                "target_incomplete",
                "Every DSH gateway route requires an explicit model selection",
            ));
        }
        let protocols: BTreeSet<&str> = binding
            .models
            .iter()
            .map(|selection| selection.protocol.as_str())
            .collect();
        let protocol = match (protocols.len(), protocols.iter().next()) {
            (1, Some(protocol)) => *protocol,
            _ => {
                return Err(Error::new(
                    "incompatible",
                    "DSH requires one protocol per native provider entry; split mixed model routes before connecting",
                ));
            }
        };
        routes.push(Route {
            name: name.clone(),
            protocol: protocol.into(),
            credential_ref: adapters::dsh_credential_ref_for(&target.scope, name, protocol),
        });
    }
    if routes.is_empty() {
        return Err(Error::new(
            "target_incomplete",
            "This DSH target has no gateway route to connect",
        ));
    }
    Ok(routes)
}

fn file_state(credentials: &Path) -> Result<(&'static str, &'static str)> {
    match fs::symlink_metadata(credentials) {
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => Err(Error::new(
            "unsafe_native_file",
            "DSH credentials must be a bounded regular file, not a symlink",
        )),
        Ok(_) => {
            scope::dsh_credential_permissions(credentials)?;
            Ok(("present", "owner_only"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(("absent", "not_applicable"))
        }
        Err(_) => Err(Error::new(
            "native_scope_unavailable",
            "Cannot inspect the DSH credentials document",
        )),
    }
}

fn validate_key(bytes: &[u8]) -> Result<String> {
    let key = std::str::from_utf8(bytes)
        .map_err(|_| Error::new("invalid_credential", "Native handoff requires a UTF-8 key"))?;
    if key.is_empty()
        || key.trim() != key
        || key
            .chars()
            .any(|character| matches!(character, '\r' | '\n' | '\0'))
    {
        return Err(Error::new(
            "invalid_credential",
            "Native key handoff requires a nonempty single-line key without surrounding whitespace",
        ));
    }
    Ok(key.to_owned())
}

async fn run(
    store: &Store,
    config: &Config,
    target_id: &str,
    target: &AgentTarget,
    dry_run: bool,
) -> Result<Value> {
    let gateway = config
        .gateway
        .as_ref()
        .ok_or_else(|| Error::new("gateway_unconfigured", "The sole gateway is not configured"))?;
    let routes = routes(target)?;
    let path = scope::scope(store, &target.scope)?;
    let credentials = path.join(".credentials.yaml");
    let status = crate::service::status(store).await?;
    let ready = status.pointer("/gateway/ready").and_then(Value::as_bool);
    let listen = status
        .pointer("/gateway/listen")
        .and_then(Value::as_str)
        .unwrap_or(&gateway.listen);
    let gateway_view = json!({
        "ready": ready,
        "listen": listen,
        "reason_code": if ready == Some(false) { Some("gateway_not_ready") } else { None },
    });

    if dry_run {
        let (state, permissions) = file_state(&credentials)?;
        return Ok(json!({
            "schema_version": 1,
            "target": {"id": target_id, "path": target.scope},
            "credentials": {
                "path": credentials,
                "state": state,
                "permissions": permissions,
            },
            "references": routes.iter().map(|route| json!({
                "route": route.name,
                "protocol": route.protocol,
                "credential_ref": route.credential_ref,
                "action": "would_write",
            })).collect::<Vec<_>>(),
            "gateway": gateway_view,
            "changed": null,
            "effects": {
                "native_writes": false,
                "secret_reads": false,
                "locks_created": false,
                "processes_started": false,
            },
        }));
    }

    scope::dsh_preflight(&path)?;
    let _lock = DshCredentialsLock::acquire(&path)?;
    scope::dsh_preflight(&path)?;
    let before = scope::read_native(&credentials)?.unwrap_or_default();
    let current = document::parse(&before, Format::Yaml)?;
    let mut after = if before.trim().is_empty() {
        "version: 1\nrefs:\n".to_owned()
    } else {
        before
    };
    let mut key = validate_key(&secrets::get(store, &gateway.key_ref)?)?;
    let mut changed = false;
    if document::get(&current, &["version".into()]) != Some(&Value::from(1)) {
        after = document::set(
            &after,
            Format::Yaml,
            &["version".into()],
            Some(&Value::from(1)),
        )?;
        changed = true;
    }
    let mut references = Vec::new();
    for route in routes {
        let field = vec!["refs".into(), route.credential_ref.clone()];
        let same = document::get(&current, &field) == Some(&Value::String(key.clone()));
        if !same {
            after = document::set(
                &after,
                Format::Yaml,
                &field,
                Some(&Value::String(key.clone())),
            )?;
            changed = true;
        }
        references.push(json!({
            "route": route.name,
            "protocol": route.protocol,
            "credential_ref": route.credential_ref,
            "action": if same { "unchanged" } else { "written" },
        }));
    }
    if changed {
        document::parse(&after, Format::Yaml)?;
        storage::atomic_write(&credentials, after.as_bytes())?;
    }
    key.zeroize();
    after.zeroize();
    Ok(json!({
        "schema_version": 1,
        "target": {"id": target_id, "path": target.scope},
        "credentials": {
            "path": credentials,
            "state": "present",
            "permissions": "owner_only",
        },
        "references": references,
        "gateway": gateway_view,
        "changed": changed,
        "effects": {
            "native_writes": changed,
            "secret_reads": true,
            "locks_created": true,
            "processes_started": false,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Gateway, ModelSelection, NativeProvider};
    use std::{collections::BTreeMap, path::PathBuf};

    fn fixture(directory: &tempfile::TempDir) -> (Store, Config, AgentTarget, PathBuf) {
        let store = Store::at(directory.path().join("tmax"));
        let home = directory.path().join("dsh");
        fs::create_dir_all(&home).unwrap();
        let key_ref = secrets::put(&store, "provider:gateway", b"gateway-key").unwrap();
        let mut config = Config::empty();
        config.gateway = Some(Gateway {
            listen: "127.0.0.1:17840".into(),
            key_ref,
            autostart: false,
        });
        let mut target = AgentTarget {
            agent: "dsh".into(),
            scope: home.clone(),
            providers: BTreeMap::new(),
            default: None,
            roles: BTreeMap::new(),
        };
        target.providers.insert(
            "tmax".into(),
            NativeProvider {
                provider: None,
                gateway: Some(true),
                models: vec![ModelSelection {
                    id: "public-model".into(),
                    protocol: "responses".into(),
                }],
            },
        );
        (store, config, target, home)
    }

    #[tokio::test]
    async fn dry_run_never_reads_a_secret_or_the_credentials_document() {
        let directory = tempfile::tempdir().unwrap();
        let (store, config, target, home) = fixture(&directory);
        let credentials = home.join(".credentials.yaml");
        fs::write(&credentials, "this is not: [valid yaml").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&credentials, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let value = run(&store, &config, "dsh-main", &target, true)
            .await
            .unwrap();
        assert_eq!(value["effects"]["secret_reads"], false);
        assert_eq!(value["effects"]["native_writes"], false);
        assert_eq!(value["changed"], Value::Null);
        assert_eq!(value["references"][0]["action"], "would_write");
        assert_eq!(value["credentials"]["state"], "present");
        assert!(!value.to_string().contains("gateway-key"));
    }

    #[tokio::test]
    async fn connect_preserves_unrelated_refs_and_comments_and_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let (store, config, target, home) = fixture(&directory);
        let credentials = home.join(".credentials.yaml");
        fs::write(
            &credentials,
            "# keep this note\nversion: 1\nrefs:\n  OTHER: keep\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&credentials, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let first = run(&store, &config, "dsh-main", &target, false)
            .await
            .unwrap();
        assert_eq!(first["changed"], true);
        let text = fs::read_to_string(&credentials).unwrap();
        assert!(text.contains("# keep this note"));
        assert!(text.contains("OTHER: keep"));
        let parsed = document::parse(&text, Format::Yaml).unwrap();
        let reference = adapters::dsh_credential_ref_for(&target.scope, "tmax", "responses");
        assert_eq!(parsed["refs"][&reference], "gateway-key");
        assert!(!first.to_string().contains("gateway-key"));

        let second = run(&store, &config, "dsh-main", &target, false)
            .await
            .unwrap();
        assert_eq!(second["changed"], false);
        assert_eq!(second["references"][0]["action"], "unchanged");
        assert_eq!(fs::read_to_string(&credentials).unwrap(), text);
    }

    #[tokio::test]
    async fn refuses_a_target_without_a_configured_gateway() {
        let directory = tempfile::tempdir().unwrap();
        let (store, mut config, target, _home) = fixture(&directory);
        config.gateway = None;
        let error = run(&store, &config, "dsh-main", &target, true)
            .await
            .unwrap_err();
        assert_eq!(error.code, "gateway_unconfigured");
    }

    #[tokio::test]
    async fn connect_adds_the_document_version_to_a_comment_only_file() {
        let directory = tempfile::tempdir().unwrap();
        let (store, config, target, home) = fixture(&directory);
        let credentials = home.join(".credentials.yaml");
        fs::write(&credentials, "# user note\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&credentials, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let value = run(&store, &config, "dsh-main", &target, false)
            .await
            .unwrap();
        assert_eq!(value["changed"], true);
        let parsed =
            document::parse(&fs::read_to_string(&credentials).unwrap(), Format::Yaml).unwrap();
        assert_eq!(parsed["version"], 1);
        let reference = adapters::dsh_credential_ref_for(&target.scope, "tmax", "responses");
        assert_eq!(parsed["refs"][&reference], "gateway-key");
    }

    #[tokio::test]
    async fn connect_refuses_a_credentials_document_without_owner_only_permissions() {
        let directory = tempfile::tempdir().unwrap();
        let (store, config, target, home) = fixture(&directory);
        let credentials = home.join(".credentials.yaml");
        fs::write(&credentials, "version: 1\nrefs: {}\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&credentials, fs::Permissions::from_mode(0o644)).unwrap();
        }
        let error = run(&store, &config, "dsh-main", &target, false)
            .await
            .unwrap_err();
        assert_eq!(error.code, "unsafe_native_file");
        let dry = run(&store, &config, "dsh-main", &target, true).await;
        assert_eq!(dry.unwrap_err().code, "unsafe_native_file");
    }

    #[tokio::test]
    async fn connect_does_not_create_patch_or_artifact_files() {
        let directory = tempfile::tempdir().unwrap();
        let (store, config, target, home) = fixture(&directory);
        run(&store, &config, "dsh-main", &target, false)
            .await
            .unwrap();
        assert!(home.join(".credentials.yaml").is_file());
        assert!(!home.join("cordis.patch.yml").exists());
        assert!(!home.join("tmax-dsh.mjs").exists());
        assert!(!home.join(".credentials.yaml.lock").exists());
    }
}
