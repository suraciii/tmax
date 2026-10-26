//! Native application transaction: preview, qualification, protected capture, guarded writes.
use super::{
    adapters::{self, File, desired_fields, inline_credential},
    document::{self, Format},
    installation::{version, version_blocker, version_warnings},
    planning::{entries, projection_view},
    recovery::{
        Capture, Field, FileRecord, INDEX, Index, Payload, Record, baseline, conflicts,
        decode_payload, field_hash, owner, partial, protect_payload, record_for,
    },
    scope::{
        DshCredentialsLock, digest, dsh_credential_permissions, read_native, revision, scope,
        target, unavailable,
    },
};
use crate::{
    Error, Request, Result,
    config::{self, Config},
    secrets,
    storage::{self, Store},
};
use serde_json::{Value, json};
use std::{fs, path::Path};
use zeroize::Zeroize;

struct Inputs {
    file: File,
    before: Option<String>,
    observed: Option<String>,
    after: String,
    fields: Vec<Field>,
    exact_original_safe: bool,
    baseline_values: Vec<(Vec<String>, Option<Value>)>,
}
fn source_guard(store: &Store) -> Result<String> {
    Ok(config::load(store)?.revision)
}
fn recheck(store: &Store, expected: &str, path: &Path, before: Option<&str>) -> Result<()> {
    if source_guard(store)? != expected
        || revision(read_native(path)?.as_deref()) != revision(before)
    {
        return Err(Error::new(
            "inputs_changed",
            "Configuration content changed during native execution; no newer content was overwritten",
        ));
    }
    Ok(())
}
fn handoff_key(store: &Store, reference: &str) -> Result<Value> {
    let bytes = secrets::get(store, reference)?;
    let key = std::str::from_utf8(&bytes)
        .map_err(|_| Error::new("invalid_credential", "Native handoff requires a UTF-8 key"))?;
    if key.is_empty()
        || key.trim() != key
        || key.chars().any(|c| c == '\r' || c == '\n' || c == '\0')
    {
        return Err(Error::new(
            "invalid_credential",
            "Native key handoff requires a nonempty single-line key without surrounding whitespace",
        ));
    }
    Ok(Value::String(key.to_owned()))
}
pub async fn apply_request(store: &Store, config: &Config, request: &Request) -> Result<Value> {
    let id = request
        .operands
        .first()
        .ok_or_else(|| Error::new("invalid_arguments", "An agent target is required"))?;
    apply(store, config, id, request.dry_run).await
}
pub async fn apply(store: &Store, config: &Config, id: &str, dry_run: bool) -> Result<Value> {
    let target = target(config, id)?;
    let path = scope(store, &target.scope)?;
    let _configuration = if dry_run {
        None
    } else {
        Some(store.lock("configuration")?)
    };
    let _lock = if dry_run {
        None
    } else {
        Some(store.lock("native")?)
    };
    let saved = config::load(store)?;
    if serde_json::to_value(&saved.config).map_err(|_| unavailable())?
        != serde_json::to_value(config).map_err(|_| unavailable())?
    {
        return Err(Error::new(
            "inputs_changed",
            "The desired configuration changed before native execution; inspect the current authority",
        ));
    }
    let expected = saved.revision;
    let mut index: Index = store.read_json(INDEX)?;
    let identity = owner(store, id)?;
    let previous = record_for(&index, identity.as_deref(), &path).cloned();
    if previous
        .as_ref()
        .is_some_and(|r| r.state == "write_pending" || r.state == "recovery_required")
    {
        return Err(Error::new(
            "recovery_required",
            "An interrupted native operation has retained recovery; restore its exact record before applying again",
        ));
    }
    let _credentials_lock = if target.agent == "dsh" && !dry_run {
        Some(DshCredentialsLock::acquire(&path)?)
    } else {
        None
    };
    let (mut entries, secret_refs) = entries(store, config, id, target, &path, dry_run).await?;
    let files = adapters::build(target, &path, &entries);
    for entry in &mut entries {
        if let Some(key) = &mut entry.key {
            key.zeroize();
        }
    }
    let mut files = files?;
    conflicts(&index, identity.as_deref(), &files)?;
    if let Some(previous) = &previous {
        for f in &previous.files {
            if !files.iter().any(|p| p.path == f.path) {
                files.push(File {
                    path: f.path.clone(),
                    format: f.format,
                    desired: json!({}),
                });
            }
        }
    }
    let mut previews = Vec::new();
    let mut observed_inputs = Vec::new();
    let mut desired_unchanged = previous
        .as_ref()
        .is_some_and(|r| r.secret_refs == secret_refs && r.files.len() == files.len());
    for file in &files {
        if file.format == Format::Credential
            && file.path.exists()
            && !previous.as_ref().is_some_and(|r| {
                r.files
                    .iter()
                    .any(|f| f.path == file.path && !f.fields.is_empty())
            })
        {
            return Err(Error::new(
                "native_namespace_conflict",
                "The native credential artifact already exists and is not currently owned by this target; it was not read",
            ));
        }
        #[cfg(unix)]
        if file.format == Format::Credential && file.path.exists() {
            use std::os::unix::fs::PermissionsExt;
            if fs::symlink_metadata(&file.path)
                .map_err(|_| unavailable())?
                .permissions()
                .mode()
                & 0o077
                != 0
            {
                return Err(Error::new(
                    "unsafe_native_file",
                    "Native credential handoff requires owner-only file permissions; no broader file was read",
                ));
            }
        }
        if target.agent == "dsh"
            && file
                .path
                .file_name()
                .is_some_and(|name| name == ".credentials.yaml")
        {
            dsh_credential_permissions(&file.path)?;
        }
        if dry_run && file.format == Format::Credential {
            previews.push(json!({"path":file.path,"present":file.path.exists(),"credential_handoff":"private native-owned credential file, referenced by the documented native key command/file mechanism","plaintext_native_store":true,"content_revision":"checked_only_on_execution","secret_reads":false}));
            continue;
        }
        let before = read_native(&file.path)?;
        let current = document::parse(before.as_deref().unwrap_or(""), file.format)?;
        adapters::validate_existing(&target.agent, file, &current)?;
        if let Some(old) = previous
            .as_ref()
            .and_then(|r| r.files.iter().find(|f| f.path == file.path))
        {
            for field in &old.fields {
                if field_hash(document::get(&current, &field.path)) != field.after_hash {
                    return Err(Error::new("native_conflict","An owned native setting changed after application; inspect it before apply or restore").field(field.path.join(".")));
                }
            }
        }
        let fields = desired_fields(&target.agent, &file.desired, &current);
        if file.format != Format::Credential {
            let old_fields = previous
                .as_ref()
                .and_then(|r| r.files.iter().find(|f| f.path == file.path))
                .map(|f| &f.fields);
            desired_unchanged &= old_fields.is_some_and(|old| {
                old.len() == fields.len()
                    && fields.iter().all(|(p, v)| {
                        old.iter().any(|f| f.path == *p)
                            && (inline_credential(&target.agent, p)
                                || document::get(&current, p) == v.as_ref())
                    })
            });
        }
        let mut existing_entries = Vec::new();
        for root in ["providers", "model_providers"] {
            if let Some(entries) = file.desired.get(root).and_then(Value::as_object) {
                for name in entries.keys() {
                    if current
                        .get(root)
                        .and_then(|entries| entries.get(name))
                        .is_some()
                    {
                        existing_entries.push(json!({"namespace":root,"name":name}));
                    }
                }
            }
        }
        previews.push(json!({"existing_provider_entries":existing_entries,"path":file.path,"revision":revision(before.as_deref()),"owned_fields":fields.iter().map(|(p,_)|p).collect::<Vec<_>>(),"credential_handoff":match target.agent.as_str() { "hermes" => "plaintext API key in current-user-restricted config.yaml; no token import", "dsh" => "plaintext API key in current-user-restricted DSH .credentials.yaml; home patch contains a native credential reference; no token import", _ => "native command/file reference to a private native-owned credential artifact; no inline config secret or token import" }}));
        observed_inputs.push((file.path.clone(), before));
    }
    if dry_run {
        return Ok(
            json!({"target":id,"agent":target.agent,"path":path,"dry_run":true,"files":previews,"supported_roles":adapters::supported_roles(&target.agent),"native_projection":projection_view(target,&entries),"default":target.default,"roles":target.roles,"precedence":adapters::precedence(&target.agent),"qualification":{"state":"requires_execution_version_check","exact_version":adapters::qualified_version(&target.agent),"source":adapters::source(&target.agent),"inference":"not_verified"},"credentials":{"resolved":false,"plaintext_native_handoff":true},"recovery":{"planned":"exact native bytes in current-user-only files before every external write","retained":previous.as_ref().map(|r|&r.id)},"effects":{"native_writes":false,"secret_reads":false,"processes_started":false,"locks_created":false},"native_reload":"required; see precedence.native_reload"}),
        );
    }
    let installed = version(&target.agent, &path).await;
    if let Some(blocker) = version_blocker(&target.agent, &installed) {
        return Ok(
            json!({"_code":blocker["code"],"_status":"blocked","target":id,"agent":target.agent,"path":path,"installation":installed,"blockers":[blocker],"effects":{"native_writes":false,"secret_reads":false},"qualification":"not_established"}),
        );
    }
    if desired_unchanged {
        return Ok(
            json!({"_code":"awaiting_native_reload","target":id,"agent":target.agent,"state":"applied_reload_pending","changed_files":[],"unchanged_files":files.iter().map(|f|&f.path).collect::<Vec<_>>(),"recovery":previous.as_ref().map(|r|&r.id),"secret_reads":false,"warnings":version_warnings(&installed),"native_reload":"still_required_or_unknown","effective":"not_verified"}),
        );
    }
    adapters::resolve_handoff(&target.agent, &mut files, |reference| {
        handoff_key(store, reference)
    })?;
    let previous_payload = previous
        .as_ref()
        .map(|r| decode_payload(store, r))
        .transpose()?;
    let mut prepared = Vec::new();
    for file in files {
        let before = read_native(&file.path)?;
        let current = document::parse(before.as_deref().unwrap_or(""), file.format)?;
        let observed = observed_inputs
            .iter()
            .find(|(path, _)| *path == file.path)
            .and_then(|(_, text)| text.as_deref());
        if revision(before.as_deref()) != revision(observed) {
            return Err(Error::new(
                "inputs_changed",
                "Native bytes changed while qualifying this operation; no newer content was overwritten",
            ));
        }
        let original = previous_payload
            .as_ref()
            .and_then(|p| p.files.iter().find(|f| f.path == file.path));
        let original_text = original
            .map(|p| p.before.clone())
            .unwrap_or_else(|| before.clone());
        let original_value = document::parse(original_text.as_deref().unwrap_or(""), file.format)?;
        if file.format == Format::Credential {
            let after = file.desired.as_str().unwrap_or("").to_owned();
            let hash_value = if after.is_empty() {
                None
            } else {
                Some(Value::String(digest(after.as_bytes())))
            };
            let fields = vec![Field {
                path: vec!["$credential".into()],
                before_hash: field_hash(document::get(&original_value, &["$credential".into()])),
                after_hash: field_hash(hash_value.as_ref()),
                observed_hash: field_hash(document::get(&current, &["$credential".into()])),
                released: after.is_empty(),
            }];
            let exact_original_safe = original.is_none_or(|p| {
                p.exact_original_safe && before.as_deref() == Some(p.after.as_str())
            });
            prepared.push(Inputs {
                file,
                before: original_text,
                observed: before,
                after,
                fields,
                exact_original_safe,
                baseline_values: vec![],
            });
            continue;
        }
        let desired = desired_fields(&target.agent, &file.desired, &current);
        let mut after = before.clone().unwrap_or_default();
        let mut fields = Vec::new();
        let mut baseline_values = Vec::new();
        if let Some(old) = previous
            .as_ref()
            .and_then(|r| r.files.iter().find(|f| f.path == file.path))
        {
            for old_field in &old.fields {
                if !desired.iter().any(|(p, _)| *p == old_field.path) {
                    let old = baseline(original, &original_value, &old_field.path);
                    baseline_values.push((old_field.path.clone(), old.cloned()));
                    if document::get(&current, &old_field.path) != old {
                        after = document::set(&after, file.format, &old_field.path, old)?;
                    }
                    fields.push(Field {
                        path: old_field.path.clone(),
                        before_hash: field_hash(old),
                        after_hash: field_hash(old),
                        observed_hash: field_hash(document::get(&current, &old_field.path)),
                        released: true,
                    });
                }
            }
        }
        for (field, value) in desired {
            if document::get(&current, &field) != value.as_ref() {
                after = document::set(&after, file.format, &field, value.as_ref())?;
            }
            let inherited = previous
                .as_ref()
                .and_then(|r| r.files.iter().find(|f| f.path == file.path))
                .is_some_and(|f| f.fields.iter().any(|f| f.path == field));
            let before_value = if inherited {
                baseline(original, &original_value, &field)
            } else {
                document::get(&current, &field)
            };
            baseline_values.push((field.clone(), before_value.cloned()));
            fields.push(Field {
                before_hash: field_hash(before_value),
                after_hash: field_hash(value.as_ref()),
                observed_hash: field_hash(document::get(&current, &field)),
                path: field,
                released: false,
            });
        }
        let exact_original_safe = original
            .is_none_or(|p| p.exact_original_safe && before.as_deref() == Some(p.after.as_str()))
            && baseline_values
                .iter()
                .all(|(path, value)| value.as_ref() == document::get(&original_value, path));
        prepared.push(Inputs {
            file,
            before: original_text,
            observed: before,
            after,
            fields,
            exact_original_safe,
            baseline_values,
        });
    }
    let recovery_id = previous
        .as_ref()
        .map(|r| r.id.clone())
        .unwrap_or_else(|| format!("native-{}", uuid::Uuid::new_v4()));
    let stable = identity.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let payload = Payload {
        files: prepared
            .iter()
            .map(|p| Capture {
                path: p.file.path.clone(),
                before: p.before.clone(),
                after: p.after.clone(),
                exact_original_safe: p.exact_original_safe,
                baseline_values: p.baseline_values.clone(),
            })
            .collect(),
    };
    let payload_ref = protect_payload(store, &recovery_id, &stable, id, &payload)?;
    let mut record = Record {
        id: recovery_id.clone(),
        owner: stable,
        target: id.into(),
        agent: target.agent.clone(),
        scope: path.clone(),
        payload_ref,
        secret_refs,
        state: "write_pending".into(),
        files: prepared
            .iter()
            .map(|p| FileRecord {
                path: p.file.path.clone(),
                format: p.file.format,
                original_absent: p.before.is_none(),
                after_revision: revision(Some(&p.after)),
                fields: p.fields.clone(),
                written: false,
            })
            .collect(),
    };
    if let Some(at) = index.records.iter().position(|r| r.id == recovery_id) {
        index.records[at] = record.clone();
    } else {
        index.records.push(record.clone());
    }
    store.write_json(INDEX, &index)?;
    let mut changed = Vec::new();
    let mut unchanged = Vec::new();
    for (n, prepared) in prepared.iter().enumerate() {
        let input = match read_native(&prepared.file.path) {
            Ok(input) => input,
            Err(e) => {
                return partial(
                    store,
                    &mut index,
                    &mut record,
                    changed,
                    unchanged,
                    &e.code,
                    "Native configuration became unavailable after recovery capture",
                );
            }
        };
        if revision(input.as_deref()) != revision(prepared.observed.as_deref()) {
            return partial(
                store,
                &mut index,
                &mut record,
                changed,
                unchanged,
                "inputs_changed",
                "Native bytes changed after recovery capture, including comments",
            );
        }
        let result = recheck(
            store,
            &expected,
            &prepared.file.path,
            prepared.observed.as_deref(),
        )
        .and_then(|_| {
            if input.as_deref() == Some(prepared.after.as_str()) {
                Ok(())
            } else if prepared.file.format == Format::Credential && prepared.after.is_empty() {
                match fs::remove_file(&prepared.file.path) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(_) => Err(unavailable()),
                }
            } else {
                storage::atomic_write(&prepared.file.path, prepared.after.as_bytes())
            }
        });
        if let Err(e) = result {
            return partial(
                store,
                &mut index,
                &mut record,
                changed,
                unchanged,
                &e.code,
                "Native write did not complete; inspect the protected recovery record",
            );
        }
        if input.as_deref() == Some(prepared.after.as_str()) {
            unchanged.push(prepared.file.path.clone());
        } else {
            changed.push(prepared.file.path.clone());
        }
        record.files[n].written = true;
        if let Some(r) = index.records.iter_mut().find(|r| r.id == record.id) {
            *r = record.clone();
        }
        if store.write_json(INDEX, &index).is_err() {
            return Ok(
                json!({"_code":"partial_native_apply","target":id,"changed_files":changed,"unchanged_files":unchanged,"recovery":record.id,"state":"recovery_required","reason_code":"recovery_metadata_write_failed","native_reload":"not_attempted"}),
            );
        }
    }
    for file in &mut record.files {
        file.fields.retain(|field| !field.released);
        for field in &mut file.fields {
            field.observed_hash = field.after_hash.clone();
        }
    }
    record.state = "applied_reload_pending".into();
    if let Some(r) = index.records.iter_mut().find(|r| r.id == record.id) {
        *r = record.clone();
    }
    if store.write_json(INDEX, &index).is_err() {
        return Ok(
            json!({"_code":"partial_native_apply","target":id,"changed_files":changed,"recovery":record.id,"reason_code":"recovery_metadata_write_failed","state":"recovery_required"}),
        );
    }
    if let Some(old) = previous
        && old.payload_ref != record.payload_ref
    {
        let _ = secrets::remove(store, &old.payload_ref);
    }
    Ok(
        json!({"_code":"awaiting_native_reload","target":id,"agent":target.agent,"path":path,"state":"applied_reload_pending","written":true,"effective":"not_verified","changed_files":changed,"unchanged_files":unchanged,"recovery":record.id,"plaintext_native_handoff":true,"native_auth_import":false,"installation":installed,"warnings":version_warnings(&installed),"precedence":adapters::precedence(&target.agent)}),
    )
}

#[cfg(test)]
mod tests {
    use super::super::{planning::pending_native_changes, test_support::inspection_target};
    use super::*;
    use crate::config::Auth;
    use std::collections::BTreeMap;
    #[tokio::test]
    async fn model_only_application_preserves_defaults_roles_and_uses_its_own_scope() {
        for agent in ["pi", "omp", "hermes"] {
            let directory = tempfile::tempdir().unwrap();
            let store = Store::at(directory.path().to_owned());
            let path = directory.path().join("native");
            fs::create_dir(&path).unwrap();
            let mut target = inspection_target(path.clone());
            target.agent = agent.into();
            target.default = None;
            let original = json!({
                "defaultProvider":"existing", "defaultModel":"unchanged",
                "modelRoles":{"default":"existing/unchanged","smol":"existing/small"},
                "model":{"provider":"existing","default":"unchanged","base_url":"https://existing.invalid"},
                "auxiliary":{"vision":{"provider":"existing","model":"vision"}},
                "providers":{"local":{"native_setting":"retained","key_cmd":"obsolete-command"},"unrelated":{"models":{"keep":{}}}}, "theme":"dark"
            });
            let (filename, format) = match agent {
                "pi" => ("models.json", Format::Json),
                "omp" => ("models.yml", Format::Yaml),
                _ => ("config.yaml", Format::Yaml),
            };
            let text = if format == Format::Json {
                serde_json::to_string(&original).unwrap()
            } else {
                serde_yaml::to_string(&original).unwrap()
            };
            fs::write(path.join(filename), &text).unwrap();
            let mut config = Config::empty();
            config.providers.insert(
                "p".into(),
                config::Provider {
                    integration: Some("custom".into()),
                    auth: Some(Auth::ApiKey {
                        key_ref: "opaque-reference-not-read".into(),
                    }),
                    endpoints: BTreeMap::from([(
                        "chat-completions".into(),
                        "https://example.invalid/v1".into(),
                    )]),
                    models: BTreeMap::from([("first".into(), config::Model::default())]),
                    ..Default::default()
                },
            );
            config.agents.insert("managed".into(), target.clone());
            store
                .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
                .unwrap();
            let preview = apply(&store, &config, "managed", true).await.unwrap();
            assert!(preview["files"].as_array().unwrap().iter().any(|f| {
                f["existing_provider_entries"]
                    .as_array()
                    .is_some_and(|entries| !entries.is_empty())
            }));
            assert_eq!(fs::read_to_string(path.join(filename)).unwrap(), text);
            let (entries, _) = entries(&store, &config, "managed", &target, &path, true)
                .await
                .unwrap();
            let files = adapters::build(&target, &path, &entries).unwrap();
            assert_eq!(files.len(), if agent == "hermes" { 1 } else { 2 });
            let file = files
                .iter()
                .find(|f| f.format != Format::Credential)
                .unwrap();
            assert_eq!(file.path, path.join(filename));
            let mut rewritten = text.clone();
            for (field, value) in desired_fields(agent, &file.desired, &original) {
                rewritten = document::set(&rewritten, format, &field, value.as_ref()).unwrap();
            }
            let after = document::parse(&rewritten, format).unwrap();
            for (field, value) in original.as_object().unwrap() {
                if field != "providers" {
                    assert_eq!(&after[field], value);
                }
            }
            assert_eq!(
                after["providers"]["unrelated"],
                original["providers"]["unrelated"]
            );
            assert_eq!(after["providers"]["local"]["native_setting"], "retained");
            if agent == "hermes" {
                assert!(after["providers"]["local"]["api_key"].is_string());
                assert!(after["providers"]["local"].get("key_cmd").is_none());
            } else {
                let credential = files
                    .iter()
                    .find(|f| f.format == Format::Credential)
                    .unwrap();
                assert_eq!(credential.path.parent(), Some(path.as_path()));
                assert!(
                    credential
                        .path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("tmax-")
                );
            }
        }
    }
    #[test]
    fn hermes_inline_handoff_resolves_only_on_execution_and_tracks_key_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let reference = secrets::put(&store, "provider:p", b"synthetic-inline-key").unwrap();
        let path = directory.path().join("config.yaml");
        let mut target = inspection_target(directory.path().to_owned());
        target.agent = "hermes".into();
        target.default = None;
        let mut files = vec![File {
            path: path.clone(),
            format: Format::Yaml,
            desired: json!({"providers":{"local":{"api_key":reference}}}),
        }];
        let preview = files[0].desired.clone();
        adapters::resolve_handoff("hermes", &mut files, |reference| {
            handoff_key(&store, reference)
        })
        .unwrap();
        assert_eq!(
            files[0].desired["providers"]["local"]["api_key"],
            "synthetic-inline-key"
        );
        fs::write(&path, serde_yaml::to_string(&files[0].desired).unwrap()).unwrap();
        let fields = desired_fields("hermes", &files[0].desired, &json!({}))
            .into_iter()
            .map(|(path, value)| Field {
                path,
                before_hash: "absent".into(),
                after_hash: field_hash(value.as_ref()),
                observed_hash: "absent".into(),
                released: false,
            })
            .collect();
        let record = Record {
            id: "recovery".into(),
            owner: "stable".into(),
            target: "managed".into(),
            agent: "hermes".into(),
            scope: directory.path().to_owned(),
            payload_ref: "not-read".into(),
            secret_refs: vec![reference.clone()],
            state: "applied_reload_pending".into(),
            files: vec![FileRecord {
                path: path.clone(),
                format: Format::Yaml,
                original_absent: true,
                after_revision: "retained".into(),
                written: true,
                fields,
            }],
        };
        files[0].desired = preview;
        assert!(
            pending_native_changes(
                &target,
                &files,
                std::slice::from_ref(&reference),
                Some(&record)
            )
            .unwrap()
            .is_empty()
        );
        let changed =
            pending_native_changes(&target, &files, &["new-reference".into()], Some(&record))
                .unwrap();
        assert_eq!(changed[0]["field"], json!(["credential_reference"]));
        fs::write(&path, "providers:\n  local:\n    api_key: user-edit\n").unwrap();
        let drift = pending_native_changes(&target, &files, &[reference], Some(&record)).unwrap();
        assert_eq!(drift[0]["field"], json!(["providers", "local", "api_key"]));
        let diagnostics = serde_json::to_string(&(changed, drift)).unwrap();
        assert!(
            !diagnostics.contains("synthetic-inline-key") && !diagnostics.contains("user-edit")
        );
    }
}
