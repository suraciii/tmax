//! Native application transaction: preview, schema detection, protected capture, guarded writes.
use super::{
    adapters::{self, File, desired_fields_with_ownership},
    document::{self, Format},
    installation::{schema_blocker, version},
    planning::{entries, fact_gap_warnings, projection_view},
    recovery::{
        Capture, Field, FileRecord, INDEX, Index, Payload, Record, conflicts, field_hash, owner,
        partial, protect_payload, prune_empty_parents, record_for,
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
/// Refresh the recorded delivery facts of an unchanged apply when they are
/// missing or outdated, so a second `agent apply` repairs `unknown` delivery.
#[cfg(test)]
fn record_unchanged_delivery(
    store: &Store,
    config: &Config,
    target: &crate::config::AgentTarget,
    previous: Option<&crate::agent_integration::recovery::Record>,
    index: &mut crate::agent_integration::recovery::Index,
) -> Result<()> {
    if !target
        .providers
        .values()
        .any(|entry| entry.is_relayed(config))
    {
        return Ok(());
    }
    let Some(current) = config::gateway_delivery_for(config, &config.relayed_route_pairs(target))
    else {
        return Ok(());
    };
    let Some(previous) = previous else {
        return Ok(());
    };
    let outdated = previous.delivery.as_ref().is_none_or(|applied| {
        applied.revision != current.revision
            || applied.listen != current.listen
            || applied.key_ref != current.key_ref
    });
    if !outdated {
        return Ok(());
    }
    if let Some(record) = index
        .records
        .iter_mut()
        .find(|record| record.id == previous.id)
    {
        record.delivery = Some(current.into());
        index.save(store)?;
    }
    Ok(())
}

pub async fn apply_request(store: &Store, config: &Config, request: &Request) -> Result<Value> {
    let id = request
        .operands
        .first()
        .ok_or_else(|| Error::new("invalid_arguments", "An agent target is required"))?;
    apply(
        store,
        config,
        id,
        request.dry_run,
        crate::operations::arguments::arg(request, "profile"),
    )
    .await
}
pub async fn apply(
    store: &Store,
    config: &Config,
    id: &str,
    dry_run: bool,
    profile: Option<&str>,
) -> Result<Value> {
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
    let (mut entries, secret_refs) = entries(store, config, id, target, &path, false).await?;
    let fact_gaps = fact_gap_warnings(target, &entries);
    let files = adapters::build_in_profile(target, &path, &entries, profile);
    for entry in &mut entries {
        if let Some(key) = &mut entry.key {
            key.zeroize();
        }
    }
    let mut files = files?;
    conflicts(&index, identity.as_deref(), &files)?;
    if let Some(previous) = &previous {
        for f in &previous.files {
            if files.iter().any(|p| p.path == f.path) {
                continue;
            }
            // A file tmax deleted is not an owned field to release: releasing it
            // would restore the bytes tmax retired on the previous run.
            if f.format == Format::Text
                && !f.fields.is_empty()
                && f.fields.iter().all(|field| field.after_hash == "absent")
            {
                continue;
            }
            files.push(File {
                path: f.path.clone(),
                format: f.format,
                desired: json!({}),
            });
        }
    }
    let mut previews = Vec::new();
    let mut observed_inputs = Vec::new();
    for file in &files {
        if file.format == Format::Text
            && file.path.exists()
            && !previous.as_ref().is_some_and(|record| {
                record
                    .files
                    .iter()
                    .any(|owned| owned.path == file.path && !owned.fields.is_empty())
            })
        {
            return Err(Error::new(
                "native_namespace_conflict",
                "The native plugin artifact already exists and is not owned by this target",
            ));
        }
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
            previews.push(json!({"path":file.path,"present":file.path.exists(),"credential_handoff":"private native-owned credential file, referenced by the documented native key command/file mechanism","content_revision":"checked_only_on_execution","secret_reads":false}));
            continue;
        }
        let before = read_native(&file.path)?;
        let current = document::parse(before.as_deref().unwrap_or(""), file.format)?;
        // A provider is named after its entry so every existing reference keeps
        // working. Taking over a name the user already configured would edit
        // their provider, so a name tmax does not own blocks instead.
        if target.agent == "dsh"
            && file.format == Format::DshPatch
            && let Some(providers) = file
                .desired
                .pointer("/llm-pi-ai/config/providers")
                .and_then(Value::as_object)
        {
            for name in providers.keys() {
                let path = vec![
                    adapters::PROFILE_SETTINGS.to_owned(),
                    "config".into(),
                    "providers".into(),
                    name.clone(),
                ];
                // Owned fields are recorded per leaf, so any field below the
                // provider path means the provider is this target's.
                let owned = previous.as_ref().is_some_and(|record| {
                    record.files.iter().any(|owned| {
                        owned.path == file.path
                            && owned
                                .fields
                                .iter()
                                .any(|field| field.path.starts_with(&path))
                    })
                });
                let matches = document::get(&current, &path) == Some(&providers[name]);
                if !owned
                    && !matches
                    && document::get(&current, &path).is_some_and(|value| !value.is_null())
                {
                    return Err(Error::new(
                        "native_namespace_conflict",
                        &format!(
                            "The DSH profile already configures a provider named \"{name}\" that tmax does not own; rename this entry or remove that provider before applying"
                        ),
                    )
                    .field(path.join(".")));
                }
            }
        }
        adapters::validate_existing(&target.agent, file, &current)?;
        let owned = previous
            .as_ref()
            .and_then(|record| record.files.iter().find(|old| old.path == file.path))
            .map(|file| file.fields.as_slice())
            .unwrap_or_default();
        let fields = desired_fields_with_ownership(&target.agent, &file.desired, &current, owned);
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
    // Environment facts are collected together, never one per run: a reader who
    // fixes one obstacle and reruns must not meet the next only then.
    let mut environment = json!({});
    let mut environment_blockers = Vec::new();
    if matches!(target.agent.as_str(), "opencode" | "dsh") {
        let installed = version(&target.agent, &path).await;
        environment["native"] = installed.clone();
        if let Some(blocker) = schema_blocker(&target.agent, &installed) {
            environment_blockers.push(blocker);
        }
    }
    if !environment_blockers.is_empty() {
        let primary = environment_blockers[0]["code"].clone();
        return Ok(
            json!({"_code":primary,"target":id,"agent":target.agent,"path":path,"dry_run":dry_run,"installation":environment.get("native").cloned(),"plugin":environment.get("plugin").cloned(),"environment":environment,"blockers":environment_blockers,"effects":{"native_writes":false,"secret_reads":false}}),
        );
    }
    if dry_run {
        return Ok(
            json!({"target":id,"agent":target.agent,"path":path,"dry_run":true,"files":previews,"supported_roles":adapters::supported_roles(&target.agent),"native_projection":projection_view(target,&entries),"fact_gaps":fact_gaps,"default":target.default,"roles":target.roles,"credentials":{"resolved":false},"environment":environment,"recovery":{"planned":"exact native bytes in current-user-only files before every external write","retained":previous.as_ref().map(|r|&r.id)},"effects":{"native_writes":false,"secret_reads":false,"processes_started":false,"locks_created":false}}),
        );
    }
    adapters::resolve_handoff(&target.agent, &mut files, |reference| {
        handoff_key(store, reference)
    })?;
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
        // Single-level restore returns the managed values as they were
        // immediately before this apply, not the first pre-tmax baseline.
        let original_text = before.clone();
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
            let exact_original_safe = true;
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
        let owned = previous
            .as_ref()
            .and_then(|record| record.files.iter().find(|old| old.path == file.path))
            .map(|file| file.fields.as_slice())
            .unwrap_or_default();
        let mut desired =
            desired_fields_with_ownership(&target.agent, &file.desired, &current, owned);
        // An absence tmax owns stays absent. The recorded after-state is the
        // desired state until a restore, so a later apply must not mistake it
        // for a field tmax stopped managing and put the old bytes back.
        for field in owned {
            if field.after_hash == "absent" && !desired.iter().any(|(path, _)| *path == field.path)
            {
                desired.push((field.path.clone(), None));
            }
        }
        let mut after = before.clone().unwrap_or_default();
        let mut fields = Vec::new();
        let mut baseline_values = Vec::new();
        if let Some(old) = previous
            .as_ref()
            .and_then(|r| r.files.iter().find(|f| f.path == file.path))
        {
            for old_field in &old.fields {
                if !desired.iter().any(|(p, _)| *p == old_field.path) {
                    // A managed field the expected configuration no longer
                    // declares is removed. The pre-apply snapshot keeps the
                    // previous value for a single-level restore.
                    let prior = document::get(&original_value, &old_field.path);
                    baseline_values.push((old_field.path.clone(), prior.cloned()));
                    if document::get(&current, &old_field.path).is_some() {
                        after = document::set(&after, file.format, &old_field.path, None)?;
                    }
                    fields.push(Field {
                        path: old_field.path.clone(),
                        before_hash: field_hash(prior),
                        after_hash: "absent".into(),
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
            let before_value = document::get(&original_value, &field);
            baseline_values.push((field.clone(), before_value.cloned()));
            // An empty text document is how whole-file absence is expressed, so
            // the recorded state is absence too; recording the empty string would
            // make a later restore disagree with the read-back it is compared to.
            let after_value = match (file.format, value.as_ref()) {
                (Format::Text, Some(empty)) if empty == &json!("") => None,
                _ => value.as_ref(),
            };
            fields.push(Field {
                before_hash: field_hash(before_value),
                after_hash: field_hash(after_value),
                observed_hash: field_hash(document::get(&current, &field)),
                path: field,
                released: false,
            });
        }
        // Removing the last managed leaf must not leave an empty provider or
        // model container behind; unrelated nonempty containers stay.
        let released: Vec<Field> = fields
            .iter()
            .filter(|field| field.released)
            .cloned()
            .collect();
        if !released.is_empty() {
            prune_released_parents(
                &mut after,
                file.format,
                &file.desired,
                &original_value,
                &released,
            )?;
        }
        if file.format == Format::DshPatch {
            prune_empty_parents(&mut after, file.format, &original_value, &fields)?;
        }
        let exact_original_safe = true;
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
        delivery: if entries.iter().any(|entry| entry.gateway) {
            crate::config::gateway_delivery_for(config, &config.relayed_route_pairs(target))
                .map(Into::into)
        } else {
            None
        },
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
            } else if matches!(prepared.file.format, Format::Credential | Format::Text)
                && prepared.after.is_empty()
            {
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
                json!({"_code":"partial_native_apply","target":id,"changed_files":changed,"unchanged_files":unchanged,"recovery":record.id,"state":"recovery_required","reason_code":"recovery_metadata_write_failed"}),
            );
        }
    }
    for file in &mut record.files {
        // Keep released fields as ownership tombstones. Restore can reinstate
        // their values from the baseline map, and a later apply must still know
        // they are tmax-managed so it can remove them again.
        for field in &mut file.fields {
            field.observed_hash = field.after_hash.clone();
        }
    }
    record.state = "applied_reload_pending".into();
    if let Some(r) = index.records.iter_mut().find(|r| r.id == record.id) {
        *r = record.clone();
    }
    // Restore is single-level: the completed apply replaces earlier retained
    // snapshots for this target. A failed apply returns before this point, so
    // its recovery record is not lost.
    let retained_owner = record.owner.clone();
    let mut stale_payloads = Vec::new();
    index.records.retain(|r| {
        if r.owner == retained_owner && r.id != record.id {
            stale_payloads.push(r.payload_ref.clone());
            false
        } else {
            true
        }
    });
    if store.write_json(INDEX, &index).is_err() {
        return Ok(
            json!({"_code":"partial_native_apply","target":id,"changed_files":changed,"recovery":record.id,"reason_code":"recovery_metadata_write_failed","state":"recovery_required"}),
        );
    }
    if let Some(old) = previous
        && old.payload_ref != record.payload_ref
    {
        stale_payloads.push(old.payload_ref.clone());
    }
    stale_payloads.sort();
    stale_payloads.dedup();
    for payload_ref in stale_payloads {
        if payload_ref != record.payload_ref {
            let _ = secrets::remove(store, &payload_ref);
        }
    }
    Ok(
        json!({"_code":"awaiting_native_reload","target":id,"agent":target.agent,"path":path,"state":"applied_reload_pending","written":true,"changed_files":changed,"unchanged_files":unchanged,"recovery":record.id,"fact_gaps":fact_gaps}),
    )
}

fn prune_released_parents(
    text: &mut String,
    format: Format,
    desired: &Value,
    original: &Value,
    released: &[Field],
) -> Result<()> {
    let mut parents: Vec<Vec<String>> = Vec::new();
    for field in released {
        for n in 1..field.path.len() {
            let parent = field.path[..n].to_vec();
            if !parents.contains(&parent) {
                parents.push(parent);
            }
        }
    }
    parents.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for parent in parents {
        if document::get(desired, &parent).is_some() || document::get(original, &parent).is_some() {
            continue;
        }
        let value = document::parse(text, format)?;
        if document::get(&value, &parent)
            .is_some_and(|v| v.as_object().is_some_and(|m| m.is_empty()))
        {
            *text = document::set(text, format, &parent, None)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::adapters::desired_fields;

    #[tokio::test]
    async fn an_unchanged_apply_advances_the_restore_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("authority"));
        let path = directory.path().join("native");
        fs::create_dir_all(&path).unwrap();
        let target = inspection_target(path.clone());
        let reference = secrets::put(&store, "provider:p", b"synthetic-snapshot-key").unwrap();
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("custom".into()),
                auth: Some(Auth::ApiKey { key_ref: reference }),
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
        fs::write(
            path.join("settings.json"),
            "{\"defaultProvider\":\"local\",\"defaultModel\":\"old\",\"unrelated\":{\"keep\":true}}",
        )
        .unwrap();
        apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        let after_first = fs::read_to_string(path.join("settings.json")).unwrap();
        apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        let index: Index = store.read_json(INDEX).unwrap();
        super::super::recovery::restore(&store, &index.records[0].id, false)
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(path.join("settings.json")).unwrap(),
            after_first,
            "the second successful apply replaces the snapshot with its pre-apply value"
        );
    }
    #[tokio::test]
    async fn restore_reinstates_a_removed_managed_field_after_an_unrelated_edit() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("authority"));
        let path = directory.path().join("native");
        fs::create_dir_all(&path).unwrap();
        let target = inspection_target(path.clone());
        let reference = secrets::put(&store, "provider:p", b"synthetic-removed-field-key").unwrap();
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("custom".into()),
                auth: Some(Auth::ApiKey { key_ref: reference }),
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
        apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        let mut without_default = config.clone();
        without_default.agents.get_mut("managed").unwrap().default = None;
        store
            .write_raw(
                "tmax.toml",
                toml::to_string(&without_default).unwrap().as_bytes(),
            )
            .unwrap();
        apply(&store, &without_default, "managed", false, None)
            .await
            .unwrap();
        let removed = fs::read_to_string(path.join("settings.json")).unwrap();
        assert!(!removed.contains("defaultModel"), "{removed}");
        // A later unrelated edit must not prevent the single-level snapshot from
        // putting the removed managed field back.
        let edited = removed.replace('{', "{\"user\":\"later\",");
        fs::write(path.join("settings.json"), &edited).unwrap();
        let index: Index = store.read_json(INDEX).unwrap();
        super::super::recovery::restore(&store, &index.records[0].id, false)
            .await
            .unwrap();
        let restored = fs::read_to_string(path.join("settings.json")).unwrap();
        assert!(restored.contains("\"defaultModel\""), "{restored}");
        assert!(restored.contains("\"user\":\"later\""), "{restored}");
        // Reapply the configuration that removed the field. Restore must also
        // reinstate ownership, not just the value, or the old default survives.
        store
            .write_raw(
                "tmax.toml",
                toml::to_string(&without_default).unwrap().as_bytes(),
            )
            .unwrap();
        apply(&store, &without_default, "managed", false, None)
            .await
            .unwrap();
        let removed_again = fs::read_to_string(path.join("settings.json")).unwrap();
        assert!(!removed_again.contains("defaultModel"), "{removed_again}");
    }
    #[test]
    fn released_managed_leaves_do_not_leave_empty_containers() {
        let fields = vec![Field {
            path: vec!["providers".into(), "local".into(), "baseURL".into()],
            before_hash: "x".into(),
            after_hash: "absent".into(),
            observed_hash: "x".into(),
            released: true,
        }];
        let original = json!({"providers":{"local":{}}});
        let mut text = "{\"providers\":{\"local\":{},\"unmanaged\":{\"keep\":true}}}".to_owned();
        prune_released_parents(&mut text, Format::Json, &json!({}), &original, &fields).unwrap();
        let value = document::parse(&text, Format::Json).unwrap();
        assert_eq!(
            value["providers"]["local"],
            json!({}),
            "a container present before the apply is user-owned and stays"
        );
        assert_eq!(value["providers"]["unmanaged"]["keep"], true);
    }
    #[test]
    fn an_unchanged_apply_records_missing_delivery_facts() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("authority"));
        let path = directory.path().join("native");
        fs::create_dir_all(&path).unwrap();
        let mut config = Config::empty();
        config.gateway = Some(config::Gateway {
            listen: "127.0.0.1:17840".into(),
            key_ref: "secret:gw".into(),
            autostart: false,
        });
        config.providers.insert(
            "up".into(),
            config::Provider {
                integration: Some("custom".into()),
                endpoints: BTreeMap::from([("responses".into(), "http://127.0.0.1:39999".into())]),
                models: BTreeMap::from([("route".into(), config::Model::default())]),
                ..Default::default()
            },
        );
        config.model_aliases.push(config::ModelAlias {
            candidate_reasoning_effort: Default::default(),
            id: "route".into(),
            protocols: vec!["responses".into()],
            candidates: vec!["up/route".into()],
            enabled: true,
            continuity: Some("stateless".into()),
            fallback: None,
        });
        config.agents.insert(
            "pi-main".into(),
            config::AgentTarget {
                agent: "pi".into(),
                scope: path.clone(),
                providers: [(
                    "relay".into(),
                    config::NativeProvider {
                        provider: config::GATEWAY_PROVIDER_ID.into(),
                        models: vec![config::ModelSelection {
                            id: "route".into(),
                            protocol: "responses".into(),
                        }],
                    },
                )]
                .into(),
                default: None,
                roles: Default::default(),
            },
        );
        let target = config.agents["pi-main"].clone();
        let record = super::super::recovery::Record {
            id: "r1".into(),
            owner: "owner".into(),
            target: "pi-main".into(),
            agent: "pi".into(),
            scope: path.clone(),
            payload_ref: "payload".into(),
            secret_refs: Vec::new(),
            state: "applied_reload_pending".into(),
            files: Vec::new(),
            delivery: None,
        };
        let mut index = super::super::recovery::Index {
            records: vec![record.clone()],
        };
        record_unchanged_delivery(&store, &config, &target, Some(&record), &mut index).unwrap();
        let saved: super::super::recovery::Index = store.read_json("native/recovery.json").unwrap();
        let delivery = saved.records[0]
            .delivery
            .as_ref()
            .expect("unchanged apply records delivery facts");
        assert_eq!(
            delivery.revision,
            config::gateway_delivery_for(&config, &config.relayed_route_pairs(&target))
                .unwrap()
                .revision
        );
        // A second unchanged apply is a no-op.
        record_unchanged_delivery(&store, &config, &target, Some(&record), &mut index).unwrap();
        let current =
            config::gateway_delivery_for(&config, &config.relayed_route_pairs(&target)).unwrap();
        assert_eq!(
            index.records[0].delivery.as_ref().unwrap().revision,
            current.revision
        );
    }

    use super::super::test_support::inspection_target;
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
            let preview = apply(&store, &config, "managed", true, None).await.unwrap();
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
    #[cfg(unix)]
    #[tokio::test]
    async fn apply_writes_when_native_executable_is_absent_and_reports_it() {
        const CHILD: &str = "TMAX_MISSING_EXECUTABLE_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let empty = tempfile::tempdir().unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "agent_integration::application::tests::apply_writes_when_native_executable_is_absent_and_reports_it",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("PATH", empty.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "isolated missing-executable apply failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let path = directory.path().join("native");
        fs::create_dir(&path).unwrap();
        let mut target = inspection_target(path.clone());
        target.agent = "pi".into();
        target.default = None;
        fs::write(
            path.join("models.json"),
            "{\"providers\":{\"unrelated\":{\"models\":{\"keep\":{}}}}}\n",
        )
        .unwrap();
        let reference = secrets::put(&store, "provider:p", b"synthetic-missing-tool-key").unwrap();
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("custom".into()),
                auth: Some(Auth::ApiKey { key_ref: reference }),
                endpoints: BTreeMap::from([(
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                )]),
                models: BTreeMap::from([("first".into(), config::Model::default())]),
                ..Default::default()
            },
        );
        config.agents.insert("managed".into(), target);
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        let applied = apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        assert_eq!(applied["_code"], "awaiting_native_reload");
        assert_eq!(applied["written"], true);
        assert!(applied.get("installation").is_none());
        assert!(applied.get("warnings").is_none());
        let written = fs::read_to_string(path.join("models.json")).unwrap();
        assert!(written.contains("local"));
        assert!(path.read_dir().unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("tmax-")
        }));
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn apply_does_not_probe_non_opencode_executables() {
        const CHILD: &str = "TMAX_NO_PROBE_CHILD";
        const MARKER: &str = "TMAX_NO_PROBE_MARKER";
        if std::env::var_os(CHILD).is_none() {
            use std::os::unix::fs::PermissionsExt;
            let bin = tempfile::tempdir().unwrap();
            let marker = bin.path().join("probed");
            let executable = bin.path().join("pi");
            fs::write(
                &executable,
                "#!/bin/sh\n: > \"$TMAX_NO_PROBE_MARKER\"\nexit 1\n",
            )
            .unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "agent_integration::application::tests::apply_does_not_probe_non_opencode_executables",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env(MARKER, &marker)
                .env("PATH", bin.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "isolated no-probe apply failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                !marker.exists(),
                "apply executed a non-OpenCode native executable"
            );
            return;
        }
        let marker = std::env::var_os(MARKER).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let path = directory.path().join("native");
        fs::create_dir(&path).unwrap();
        let mut target = inspection_target(path.clone());
        target.agent = "pi".into();
        target.default = None;
        fs::write(
            path.join("models.json"),
            "{\"providers\":{\"unrelated\":{\"models\":{\"keep\":{}}}}}\n",
        )
        .unwrap();
        let reference = secrets::put(&store, "provider:p", b"synthetic-no-probe-key").unwrap();
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("custom".into()),
                auth: Some(Auth::ApiKey { key_ref: reference }),
                endpoints: BTreeMap::from([(
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                )]),
                models: BTreeMap::from([("first".into(), config::Model::default())]),
                ..Default::default()
            },
        );
        config.agents.insert("managed".into(), target);
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        let applied = apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        assert_eq!(applied["written"], true);
        assert!(!std::path::Path::new(&marker).exists());
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn known_incompatible_schema_blocks_before_writes() {
        const CHILD: &str = "TMAX_INCOMPATIBLE_SCHEMA_CHILD";
        if std::env::var_os(CHILD).is_none() {
            use std::os::unix::fs::PermissionsExt;
            let bin = tempfile::tempdir().unwrap();
            let executable = bin.path().join("opencode");
            fs::write(
                &executable,
                "#!/bin/sh\n[ \"$1\" = --version ] || exit 1\nprintf '1.9.0\\n'\n",
            )
            .unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "agent_integration::application::tests::known_incompatible_schema_blocks_before_writes",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("PATH", bin.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "isolated schema block failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let path = directory.path().join("native");
        fs::create_dir(&path).unwrap();
        let mut target = inspection_target(path.clone());
        target.agent = "opencode".into();
        target.default = None;
        let reference = secrets::put(&store, "provider:p", b"synthetic-schema-key").unwrap();
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("custom".into()),
                auth: Some(Auth::ApiKey { key_ref: reference }),
                endpoints: BTreeMap::from([(
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                )]),
                models: BTreeMap::from([("first".into(), config::Model::default())]),
                ..Default::default()
            },
        );
        config.agents.insert("managed".into(), target);
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        let result = apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        assert_eq!(result["_code"], "native_schema_incompatible");
        assert_eq!(result["effects"]["native_writes"], false);
        assert!(path.read_dir().unwrap().next().is_none());
    }
    #[tokio::test]
    async fn authoritative_apply_overwrites_a_later_native_rewrite() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let path = directory.path().join("native");
        fs::create_dir(&path).unwrap();
        let mut target = inspection_target(path.clone());
        target.agent = "omp".into();
        target.default = None;
        let original =
            json!({"defaultProvider":"existing","providers":{"unrelated":{"models":{"keep":{}}}}});
        let filename = "models.yml";
        fs::write(
            path.join(filename),
            serde_yaml::to_string(&original).unwrap(),
        )
        .unwrap();
        let reference = secrets::put(&store, "provider:p", b"synthetic-rewrite-key").unwrap();
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("custom".into()),
                auth: Some(Auth::ApiKey { key_ref: reference }),
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
        let applied = apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        assert_eq!(applied["_code"], "awaiting_native_reload");
        let gaps = applied["fact_gaps"].as_array().unwrap();
        assert!(gaps.iter().any(|gap| {
            gap["model"] == "first"
                && gap["entry"] == "local"
                && gap["agent_self_completion"] == "undocumented"
                && gap["missing"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("reasoning_efforts"))
                && gap["missing"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("limit.context"))
        }));
        // The tool enriches its own rewritten entry; the authoritative apply
        // must overwrite that managed entry and leave unrelated entries alone.
        let model_path = vec![
            "providers".to_owned(),
            "local".to_owned(),
            "models".to_owned(),
        ];
        let written = fs::read_to_string(path.join(filename)).unwrap();
        let value = document::parse(&written, Format::Yaml).unwrap();
        let mut models = document::get(&value, &model_path)
            .unwrap()
            .as_array()
            .unwrap()
            .clone();
        models[0]["contextWindow"] = json!(1050000);
        models[0]["thinking"] = json!({"mode": "effort", "efforts": ["low", "high"]});
        let enriched = document::set(
            &written,
            Format::Yaml,
            &model_path,
            Some(&Value::Array(models)),
        )
        .unwrap();
        fs::write(path.join(filename), &enriched).unwrap();
        let reapplied = apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        assert_eq!(reapplied["_code"], "awaiting_native_reload");
        let rewritten = fs::read_to_string(path.join(filename)).unwrap();
        assert_ne!(rewritten, enriched);
        let value = document::parse(&rewritten, Format::Yaml).unwrap();
        assert_eq!(
            document::get(&value, &model_path).unwrap()[0]["contextWindow"],
            Value::Null
        );
        assert!(
            value["providers"]["unrelated"]["models"]["keep"].is_object(),
            "unmanaged provider entries stay"
        );
        let index: Index = store.read_json(INDEX).unwrap();
        assert_eq!(
            index.records.len(),
            1,
            "restore keeps one snapshot per target"
        );
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
        adapters::resolve_handoff("hermes", &mut files, |reference| {
            handoff_key(&store, reference)
        })
        .unwrap();
        assert_eq!(
            files[0].desired["providers"]["local"]["api_key"],
            "synthetic-inline-key"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn pi_default_effort_is_field_scoped_and_detects_external_edits() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let path = directory.path().join("native");
        fs::create_dir(&path).unwrap();
        let mut target = inspection_target(path.clone());
        target.agent = "pi".into();
        fs::write(
            path.join("settings.json"),
            "{\"defaultProvider\":\"local\",\"defaultModel\":\"first\",\"defaultThinkingLevel\":\"low\",\"unrelated\":{\"keep\":true}}",
        )
        .unwrap();
        let reference = secrets::put(&store, "provider:p", b"synthetic-pi-effort-key").unwrap();
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("custom".into()),
                auth: Some(Auth::ApiKey { key_ref: reference }),
                endpoints: BTreeMap::from([(
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                )]),
                models: BTreeMap::from([(
                    "first".into(),
                    config::Model {
                        overrides: config::ModelOverrides {
                            limit: Some(config::Limits {
                                context: Some(128000),
                                input: None,
                                output: Some(8192),
                            }),
                            reasoning_options: Some(vec![config::ReasoningOption {
                                r#type: "effort".into(),
                                values: Some(vec!["low".into(), "high".into()]),
                            }]),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            },
        );
        config.agents.insert("managed".into(), target);
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();

        // An unconfigured effort must not be written or removed.
        apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        let settings = fs::read_to_string(path.join("settings.json")).unwrap();
        assert!(
            settings.contains("\"defaultThinkingLevel\": \"low\"")
                || settings.contains("\"defaultThinkingLevel\":\"low\"")
        );
        assert!(settings.contains("unrelated"));

        // An explicit effort is written and owned.
        let mut managed = config.clone();
        managed
            .agents
            .get_mut("managed")
            .unwrap()
            .default
            .as_mut()
            .unwrap()
            .effort = Some("high".into());
        store
            .write_raw("tmax.toml", toml::to_string(&managed).unwrap().as_bytes())
            .unwrap();
        apply(&store, &managed, "managed", false, None)
            .await
            .unwrap();
        let settings = fs::read_to_string(path.join("settings.json")).unwrap();
        assert!(
            settings.contains("\"defaultThinkingLevel\": \"high\"")
                || settings.contains("\"defaultThinkingLevel\":\"high\"")
        );
        assert!(settings.contains("unrelated"));

        // Removing the effort from desired configuration removes the managed
        // field; the single-level snapshot can restore the previous value.
        let mut unmanaged = managed.clone();
        unmanaged
            .agents
            .get_mut("managed")
            .unwrap()
            .default
            .as_mut()
            .unwrap()
            .effort = None;
        store
            .write_raw("tmax.toml", toml::to_string(&unmanaged).unwrap().as_bytes())
            .unwrap();
        apply(&store, &unmanaged, "managed", false, None)
            .await
            .unwrap();
        let settings = fs::read_to_string(path.join("settings.json")).unwrap();
        assert!(!settings.contains("defaultThinkingLevel"), "{settings}");

        // Re-manage it, then verify an external edit is overwritten by the
        // authoritative apply, not treated as a blocker.
        store
            .write_raw("tmax.toml", toml::to_string(&managed).unwrap().as_bytes())
            .unwrap();
        apply(&store, &managed, "managed", false, None)
            .await
            .unwrap();
        let settings = fs::read_to_string(path.join("settings.json")).unwrap();
        let edited = settings.replace("\"high\"", "\"low\"");
        fs::write(path.join("settings.json"), &edited).unwrap();
        apply(&store, &managed, "managed", false, None)
            .await
            .unwrap();
        let settings = fs::read_to_string(path.join("settings.json")).unwrap();
        assert!(
            settings.contains("\"defaultThinkingLevel\": \"high\"")
                || settings.contains("\"defaultThinkingLevel\":\"high\"")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn opencode_default_effort_is_a_composite_field_with_baseline_restore() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let path = directory.path().join("native");
        fs::create_dir(&path).unwrap();
        let native_path = path.join("opencode.jsonc");
        fs::write(
            &native_path,
            "// native comment\n{\"model\":\"local/first\",\"unrelated\":{\"keep\":true}}\n",
        )
        .unwrap();
        let mut target = inspection_target(path.clone());
        target.agent = "opencode".into();
        target.default.as_mut().unwrap().effort = Some("high".into());
        let reference =
            secrets::put(&store, "provider:p", b"synthetic-opencode-effort-key").unwrap();
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("custom".into()),
                auth: Some(Auth::ApiKey { key_ref: reference }),
                endpoints: BTreeMap::from([(
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                )]),
                models: BTreeMap::from([(
                    "first".into(),
                    config::Model {
                        overrides: config::ModelOverrides {
                            limit: Some(config::Limits {
                                context: Some(128000),
                                input: None,
                                output: Some(8192),
                            }),
                            reasoning_options: Some(vec![config::ReasoningOption {
                                r#type: "effort".into(),
                                values: Some(vec!["low".into(), "high".into()]),
                            }]),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            },
        );
        config.agents.insert("managed".into(), target);
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        apply(&store, &config, "managed", false, None)
            .await
            .unwrap();
        let applied = fs::read_to_string(&native_path).unwrap();
        assert!(applied.contains("\"local/first#high\""), "{applied}");
        assert!(applied.contains("native comment"), "{applied}");
        assert!(applied.contains("unrelated"), "{applied}");

        // Removing only the effort leaves the current composite value in place.
        let mut unmanaged = config.clone();
        unmanaged
            .agents
            .get_mut("managed")
            .unwrap()
            .default
            .as_mut()
            .unwrap()
            .effort = None;
        store
            .write_raw("tmax.toml", toml::to_string(&unmanaged).unwrap().as_bytes())
            .unwrap();
        apply(&store, &unmanaged, "managed", false, None)
            .await
            .unwrap();
        let settings = fs::read_to_string(&native_path).unwrap();
        assert!(settings.contains("\"local/first#high\""), "{settings}");

        // Clearing the default removes the managed composite field.
        let mut cleared = unmanaged.clone();
        cleared.agents.get_mut("managed").unwrap().default = None;
        store
            .write_raw("tmax.toml", toml::to_string(&cleared).unwrap().as_bytes())
            .unwrap();
        apply(&store, &cleared, "managed", false, None)
            .await
            .unwrap();
        let restored = fs::read_to_string(&native_path).unwrap();
        assert!(!restored.contains("local/first"), "{restored}");
        assert!(restored.contains("native comment"), "{restored}");
        assert!(restored.contains("unrelated"), "{restored}");
    }
}
