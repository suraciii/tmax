//! Protected native capture authority, write-ahead recovery, and restoration.
use super::{
    adapters::File,
    document::{self, Format},
    scope::{DshCredentialsLock, digest, read_native, revision, unavailable},
};
use crate::{
    Error, Request, Result, secrets,
    storage::{self, Store},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use zeroize::Zeroize;

pub(super) const INDEX: &str = "native/recovery.json";
#[derive(Default, Serialize, Deserialize)]
pub(super) struct Index {
    pub(super) records: Vec<Record>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Record {
    pub(super) id: String,
    pub(super) owner: String,
    pub(super) target: String,
    pub(super) agent: String,
    pub(super) scope: PathBuf,
    pub(super) payload_ref: String,
    pub(super) secret_refs: Vec<String>,
    pub(super) state: String,
    pub(super) files: Vec<FileRecord>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct FileRecord {
    pub(super) path: PathBuf,
    pub(super) format: Format,
    pub(super) original_absent: bool,
    pub(super) after_revision: String,
    pub(super) fields: Vec<Field>,
    pub(super) written: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Field {
    pub(super) path: Vec<String>,
    pub(super) after_hash: String,
    pub(super) before_hash: String,
    pub(super) observed_hash: String,
    pub(super) released: bool,
}
#[derive(Serialize, Deserialize)]
pub(super) struct Payload {
    pub(super) files: Vec<Capture>,
}
#[derive(Serialize, Deserialize)]
pub(super) struct Capture {
    pub(super) path: PathBuf,
    pub(super) before: Option<String>,
    pub(super) after: String,
    pub(super) exact_original_safe: bool,
    pub(super) baseline_values: Vec<(Vec<String>, Option<Value>)>,
}

impl Index {
    pub(super) fn load(store: &Store) -> Result<Self> {
        store.read_json(INDEX)
    }

    pub(super) fn rename_target(&mut self, identity: Option<&str>, new: &str) {
        for record in &mut self.records {
            if identity == Some(record.owner.as_str()) {
                record.target = new.into();
            }
        }
    }

    pub(super) fn save(&self, store: &Store) -> Result<()> {
        store.write_json(INDEX, self)
    }
}
pub(super) fn baseline<'a>(
    capture: Option<&'a Capture>,
    original: &'a Value,
    path: &[String],
) -> Option<&'a Value> {
    match capture.and_then(|c| c.baseline_values.iter().find(|(p, _)| p == path)) {
        Some((_, value)) => value.as_ref(),
        None => document::get(original, path),
    }
}
pub(super) fn field_hash(value: Option<&Value>) -> String {
    value
        .map(|v| digest(v.to_string().as_bytes()))
        .unwrap_or_else(|| "absent".into())
}
pub(super) fn owner(store: &Store, target: &str) -> Result<Option<String>> {
    secrets::owner_identity(store, "agent", target)
}
pub(super) fn record_for<'a>(
    index: &'a Index,
    identity: Option<&str>,
    scope: &Path,
) -> Option<&'a Record> {
    identity.and_then(|identity| {
        index.records.iter().rev().find(|r| {
            r.owner == identity
                && r.scope == scope
                && r.state != "restored"
                && r.state != "discarded"
        })
    })
}
fn validate_record(record: &Record) -> Result<()> {
    if !record.scope.is_absolute()
        || record.files.iter().any(|f| {
            f.path.parent() != Some(record.scope.as_path()) || f.path.file_name().is_none()
        })
    {
        return Err(Error::new(
            "recovery_invalid",
            "Native recovery paths do not match their captured scope",
        ));
    }
    Ok(())
}
pub(super) fn field_drift(record: &Record, read_credentials: bool) -> Result<Vec<Value>> {
    validate_record(record)?;
    let mut drift = Vec::new();
    for file in &record.files {
        if file.format == Format::Credential && !read_credentials {
            continue;
        }
        let text = read_native(&file.path)?;
        let value = document::parse(text.as_deref().unwrap_or(""), file.format)?;
        for field in &file.fields {
            let current = field_hash(document::get(&value, &field.path));
            if current != field.after_hash
                && current != field.before_hash
                && current != field.observed_hash
            {
                drift.push(json!({"path":file.path,"field":field.path,"reason_code":"native_owned_field_changed"}));
            }
        }
    }
    Ok(drift)
}
pub(super) fn record_view(store: &Store, record: &Record) -> Value {
    let drift = field_drift(record, false);
    let protection = secrets::check_recovery(store, &record.payload_ref, &record.owner)
        .map(|_| "current_user_files");
    json!({"recovery":record.id,"target":record.target,"agent":record.agent,"path":record.scope,"state":record.state,"protection":protection.as_ref().ok(),"restore_available":protection.is_ok(),"storage_reason_code":protection.as_ref().err().map(|error|error.code.as_str()),"files":record.files.iter().map(|f|json!({"path":f.path,"written":f.written,"owned_fields":f.fields.iter().map(|f|&f.path).collect::<Vec<_>>()})).collect::<Vec<_>>(),"conflicts":drift.as_ref().ok(),"inspection_state":if drift.is_ok(){"available"}else{"unknown"},"reason_code":drift.err().map(|e|e.code),"available_actions":if protection.is_ok(){json!(["agent restore --recovery","agent recovery accept","agent recovery discard"])}else{json!([])}})
}
pub(super) fn protect_payload(
    store: &Store,
    id: &str,
    owner: &str,
    target: &str,
    payload: &Payload,
) -> Result<String> {
    let mut bytes = serde_json::to_vec(payload).map_err(|_| {
        Error::new(
            "recovery_unavailable",
            "Cannot encode protected native recovery",
        )
    })?;
    let result = secrets::put(store, &format!("agent:{owner}:{target}"), &bytes);
    bytes.zeroize();
    result.map_err(|e| {
        Error::new(
            &e.code,
            &format!("Native recovery {id} could not be protected; no native write was attempted"),
        )
    })
}
pub(super) fn decode_payload(store: &Store, record: &Record) -> Result<Payload> {
    secrets::check_recovery(store, &record.payload_ref, &record.owner)?;
    let bytes = secrets::get(store, &record.payload_ref)?;
    let payload: Payload = serde_json::from_slice(&bytes).map_err(|_| {
        Error::new(
            "recovery_invalid",
            "Protected native recovery is not readable",
        )
    })?;
    if payload.files.len() != record.files.len()
        || payload
            .files
            .iter()
            .zip(&record.files)
            .any(|(a, b)| a.path != b.path || revision(Some(&a.after)) != b.after_revision)
    {
        return Err(Error::new(
            "recovery_invalid",
            "Protected native recovery does not match safe retained metadata",
        ));
    }
    Ok(payload)
}
pub(super) fn conflicts(index: &Index, identity: Option<&str>, files: &[File]) -> Result<()> {
    for record in &index.records {
        if matches!(record.state.as_str(), "restored" | "discarded")
            || identity == Some(&record.owner)
        {
            continue;
        }
        if files
            .iter()
            .any(|f| record.files.iter().any(|r| r.path == f.path))
        {
            return Err(Error::new(
                "native_scope_conflict",
                "Another retained target owns settings in this shared native scope; inspect and restore that owner before applying",
            ));
        }
    }
    Ok(())
}
pub(super) fn partial(
    store: &Store,
    index: &mut Index,
    record: &mut Record,
    changed: Vec<PathBuf>,
    unchanged: Vec<PathBuf>,
    code: &str,
    message: &str,
) -> Result<Value> {
    record.state = "recovery_required".into();
    if let Some(r) = index.records.iter_mut().find(|r| r.id == record.id) {
        *r = record.clone();
    }
    let persisted = store.write_json(INDEX, index).is_ok();
    Ok(
        json!({"_code":"partial_native_apply","target":record.target,"state":"recovery_required","changed_files":changed,"unchanged_files":unchanged,"remaining_files":record.files.iter().filter(|f|!f.written).map(|f|&f.path).collect::<Vec<_>>(),"reason_code":code,"message":message,"recovery":record.id,"recovery_metadata_updated":persisted,"native_reload":"not_attempted"}),
    )
}
pub(super) fn prune_empty_parents(
    text: &mut String,
    format: Format,
    original: &Value,
    fields: &[Field],
) -> Result<()> {
    // Only containers absent from the captured baseline may be removed.
    let mut parents: BTreeSet<Vec<String>> = BTreeSet::new();
    for field in fields {
        for n in 1..field.path.len() {
            parents.insert(field.path[..n].to_vec());
        }
    }
    let mut parents: Vec<_> = parents.into_iter().collect();
    parents.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for parent in parents {
        let value = document::parse(text, format)?;
        if document::get(original, &parent).is_none()
            && document::get(&value, &parent)
                .is_some_and(|v| v.as_object().is_some_and(|m| m.is_empty()))
        {
            *text = document::set(text, format, &parent, None)?;
        }
    }
    Ok(())
}

fn restore_text(
    current: Option<&str>,
    file: &FileRecord,
    capture: &Capture,
) -> Result<Option<String>> {
    if file.format == Format::Text {
        let actual = document::parse(current.unwrap_or(""), Format::Text)?;
        let observed = field_hash(document::get(&actual, &["$text".into()]));
        if revision(current) != revision(Some(&capture.after))
            && revision(current) != revision(capture.before.as_deref())
            && !file
                .fields
                .iter()
                .any(|field| field.observed_hash == observed)
        {
            return Err(Error::new(
                "native_conflict",
                "The native plugin artifact changed after application; restoration stopped without overwriting it",
            ));
        }
        return Ok(capture.before.clone());
    }
    if file.format == Format::Credential {
        let actual = document::parse(current.unwrap_or(""), Format::Credential)?;
        let observed = field_hash(document::get(&actual, &["$credential".into()]));
        if revision(current) != revision(Some(&capture.after))
            && revision(current) != revision(capture.before.as_deref())
            && !file.fields.iter().any(|f| f.observed_hash == observed)
        {
            return Err(Error::new(
                "native_conflict",
                "The native credential artifact changed after handoff; its opaque bytes were not imported or overwritten",
            ));
        }
        return Ok(capture.before.clone());
    }
    if current == Some(capture.after.as_str()) && capture.exact_original_safe {
        return Ok(capture.before.clone());
    }
    let mut text = current.unwrap_or("").to_owned();
    let original = document::parse(capture.before.as_deref().unwrap_or(""), file.format)?;
    let actual = document::parse(&text, file.format)?;
    for field in &file.fields {
        let hash = field_hash(document::get(&actual, &field.path));
        if hash != field.after_hash && hash != field.before_hash && hash != field.observed_hash {
            return Err(Error::new("native_conflict","A later edit changed a tmax-owned native field; restoration stopped without overwriting it").field(field.path.join(".")));
        }
    }
    for field in file.fields.iter().rev() {
        let before = baseline(Some(capture), &original, &field.path);
        let value = document::parse(&text, file.format)?;
        if document::get(&value, &field.path) != before {
            text = document::set(&text, file.format, &field.path, before)?;
        }
    }
    // User-added fields survive; comments force a conflict rather than deletion.
    prune_empty_parents(&mut text, file.format, &original, &file.fields)?;
    if file.format == Format::DshPatch
        && original.get("llm-tmax").is_none()
        && document::parse(&text, file.format)?
            .get("llm-tmax")
            .is_some()
    {
        return Err(Error::new(
            "native_conflict",
            "Later edits keep the newly inserted llm-tmax plugin in use; restoration stopped before removing its entry or artifact",
        ));
    }
    if capture.before.is_none()
        && document::parse(&text, file.format)?
            .as_object()
            .is_some_and(|m| m.is_empty())
        && !text.contains('#')
        && !text.contains("//")
        && !text.contains("/*")
    {
        Ok(None)
    } else {
        Ok(Some(text))
    }
}
pub async fn restore_request(store: &Store, request: &Request) -> Result<Value> {
    let id = request
        .arguments
        .get("recovery")
        .and_then(|v| v.first())
        .or_else(|| request.operands.first())
        .ok_or_else(|| {
            Error::new(
                "invalid_arguments",
                "An agent target or exact recovery handle is required",
            )
        })?;
    restore(store, id, request.dry_run).await
}
pub async fn restore(store: &Store, id: &str, dry_run: bool) -> Result<Value> {
    let _lock = if dry_run {
        None
    } else {
        Some(store.lock("native")?)
    };
    let mut index: Index = store.read_json(INDEX)?;
    let selected = if index.records.iter().any(|r| r.id == id) {
        id.to_owned()
    } else {
        let identity=owner(store,id)?.ok_or_else(||Error::new("unknown_resource","No recovery belongs to this target identity; use an exact retained recovery handle"))?;
        index
            .records
            .iter()
            .rev()
            .find(|r| r.owner == identity && r.state != "restored" && r.state != "discarded")
            .map(|r| r.id.clone())
            .ok_or_else(|| {
                Error::new(
                    "unknown_resource",
                    "No remaining native recovery belongs to this target",
                )
            })?
    };
    let mut record = index
        .records
        .iter()
        .find(|r| r.id == selected)
        .cloned()
        .ok_or_else(|| Error::new("unknown_resource", "Native recovery handle is not retained"))?;
    validate_record(&record)?;
    if record.state == "discarded" {
        return Err(Error::new(
            "unknown_resource",
            "Native recovery was explicitly discarded",
        ));
    }
    if record.state == "restored" {
        return Ok(json!({"recovery":selected,"state":"already_restored","native_writes":false}));
    }
    let _credentials_lock = if record.agent == "dsh" && !dry_run {
        Some(DshCredentialsLock::acquire(&record.scope)?)
    } else {
        None
    };
    let payload = if dry_run {
        secrets::check_recovery(store, &record.payload_ref, &record.owner)?;
        None
    } else {
        Some(decode_payload(store, &record)?)
    };
    let conflicts = field_drift(&record, !dry_run)?;
    if !conflicts.is_empty() {
        return Err(Error::new(
            "native_conflict",
            "Owned native fields differ from both their captured originals and tmax writes; restore them, or accept the current native state with agent recovery accept",
        ));
    }
    if dry_run {
        return Ok(
            json!({"target":record.target,"recovery":selected,"dry_run":true,"files":record.files.iter().map(|f|&f.path).collect::<Vec<_>>(),"restoration":"three_way_owned_fields_only","secret_reads":false,"native_writes":false,"native_reload":"required_after_execution"}),
        );
    }
    let payload = payload.expect("executing restoration resolves its payload");
    let mut prepared = Vec::new();
    for (file, capture) in record.files.iter().zip(&payload.files) {
        let before = read_native(&file.path)?;
        let after = restore_text(before.as_deref(), file, capture)?;
        prepared.push((file.path.clone(), before, after));
    }
    record.state = "recovery_required".into();
    if let Some(r) = index.records.iter_mut().find(|r| r.id == selected) {
        *r = record.clone();
    }
    store.write_json(INDEX, &index)?;
    let mut changed = Vec::new();
    let mut unchanged = Vec::new();
    for (path, before, after) in prepared {
        let current = match read_native(&path) {
            Ok(current) => current,
            Err(e) => {
                return partial(
                    store,
                    &mut index,
                    &mut record,
                    changed,
                    unchanged,
                    &e.code,
                    "Native restore input became unavailable after earlier effects",
                );
            }
        };
        if revision(current.as_deref()) != revision(before.as_deref()) {
            return partial(
                store,
                &mut index,
                &mut record,
                changed,
                unchanged,
                "inputs_changed",
                "Native content changed during restoration",
            );
        }
        if before == after {
            unchanged.push(path);
            continue;
        }
        let result = match after {
            Some(text) => storage::atomic_write(&path, text.as_bytes()),
            None => fs::remove_file(&path).map_err(|_| unavailable()),
        };
        if let Err(e) = result {
            return partial(
                store,
                &mut index,
                &mut record,
                changed,
                unchanged,
                &e.code,
                "A native restore write did not complete; earlier restored files remain reported",
            );
        }
        changed.push(path);
    }
    record.state = "restored".into();
    if let Some(r) = index.records.iter_mut().find(|r| r.id == selected) {
        *r = record.clone();
    }
    if store.write_json(INDEX, &index).is_err() {
        return Ok(
            json!({"_code":"partial_native_apply","recovery":selected,"changed_files":changed,"state":"restored_metadata_pending","reason_code":"recovery_metadata_write_failed"}),
        );
    }
    Ok(
        json!({"_code":"awaiting_native_reload","target":record.target,"agent":record.agent,"recovery":selected,"state":"restored_reload_pending","changed_files":changed,"unchanged_files":unchanged,"preserved":"unrelated settings and later unowned edits","credential_logout":false,"gateway_key_revoked":false,"native_reload":"required","recovery_retained":true}),
    )
}

pub async fn recovery(store: &Store, request: &Request) -> Result<Value> {
    let action = request.command.last().map(String::as_str).unwrap_or("");
    let _lock = if matches!(action, "discard" | "accept") && !request.dry_run {
        Some(store.lock("native")?)
    } else {
        None
    };
    let mut index: Index = store.read_json(INDEX)?;
    match action {
        "list" => {
            let selected = request.arguments.get("target").and_then(|v| v.first());
            let stable = selected.map(|id| owner(store, id)).transpose()?.flatten();
            Ok(
                json!({"records":index.records.iter().filter(|r|r.state!="discarded" && selected.is_none_or(|target|stable.as_deref()==Some(&r.owner)||r.target==*target)).map(|r|record_view(store,r)).collect::<Vec<_>>()}),
            )
        }
        "inspect" => {
            let id = request.operands.first().ok_or_else(|| {
                Error::new(
                    "invalid_arguments",
                    "An exact native recovery handle is required",
                )
            })?;
            let record = index
                .records
                .iter()
                .find(|r| r.id == *id && r.state != "discarded")
                .ok_or_else(|| {
                    Error::new("unknown_resource", "Native recovery handle is not retained")
                })?;
            Ok(record_view(store, record))
        }
        "accept" => {
            let requested = request
                .arguments
                .get("recovery")
                .and_then(|v| v.first())
                .cloned()
                .or_else(|| request.operands.first().cloned())
                .ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        "An exact native recovery handle or target ID is required",
                    )
                })?;
            let position = index
                .records
                .iter()
                .position(|record| record.id == requested && record.state != "discarded")
                .or_else(|| {
                    let identity = owner(store, &requested).ok()?;
                    index.records.iter().rposition(|r| {
                        identity.as_deref() == Some(r.owner.as_str())
                            && r.state != "restored"
                            && r.state != "discarded"
                    })
                })
                .ok_or_else(|| {
                    Error::new(
                        "unknown_resource",
                        "No remaining native recovery belongs to this handle or target",
                    )
                })?;
            let mut record = index.records[position].clone();
            validate_record(&record)?;
            if record.state == "restored" {
                return Err(Error::new(
                    "unknown_resource",
                    "Native recovery was already restored; nothing to accept",
                ));
            }
            secrets::check_recovery(store, &record.payload_ref, &record.owner)?;
            let drift = field_drift(&record, false)?;
            if request.dry_run {
                return Ok(
                    json!({"recovery":record.id,"target":record.target,"dry_run":true,"accept":"re-baseline owned fields to the current native state","conflicts":drift,"native_writes":false,"credentials_deleted":false,"credential_ownership":"retained"}),
                );
            }
            let mut payload = decode_payload(store, &record)?;
            for (file, capture) in record.files.iter_mut().zip(payload.files.iter_mut()) {
                let text = read_native(&file.path)?;
                let value = document::parse(text.as_deref().unwrap_or(""), file.format)?;
                for field in file.fields.iter_mut() {
                    let hash = field_hash(document::get(&value, &field.path));
                    field.after_hash = hash.clone();
                    field.before_hash = hash.clone();
                    field.observed_hash = hash;
                }
                capture.baseline_values = file
                    .fields
                    .iter()
                    .map(|field| {
                        (
                            field.path.clone(),
                            document::get(&value, &field.path).cloned(),
                        )
                    })
                    .collect();
                capture.before = text.clone();
                capture.after = text.unwrap_or_default();
                file.after_revision = revision(Some(&capture.after));
            }
            let accepted_ref =
                protect_payload(store, &record.id, &record.owner, &record.target, &payload)?;
            secrets::remove(store, &record.payload_ref)?;
            record.payload_ref = accepted_ref;
            record.state = "accepted".into();
            let fields = record
                .files
                .iter()
                .map(|file| file.fields.len())
                .sum::<usize>();
            index.records[position] = record.clone();
            store.write_json(INDEX, &index)?;
            Ok(
                json!({"recovery":record.id,"target":record.target,"state":"accepted","re_baselined_fields":fields,"native_writes":false,"credentials_deleted":false,"credential_ownership":"retained","restoration_baseline":"current native state"}),
            )
        }
        "discard" => {
            let id = request.operands.first().ok_or_else(|| {
                Error::new(
                    "invalid_arguments",
                    "An exact native recovery handle is required",
                )
            })?;
            let position = index
                .records
                .iter()
                .position(|record| record.id == *id && record.state != "discarded")
                .ok_or_else(|| {
                    Error::new("unknown_resource", "Native recovery handle is not retained")
                })?;
            let record = &index.records[position];
            validate_record(record)?;
            secrets::check_recovery(store, &record.payload_ref, &record.owner)?;
            if index
                .records
                .iter()
                .filter(|other| {
                    other.state != "discarded" && other.payload_ref == record.payload_ref
                })
                .count()
                != 1
            {
                return Err(Error::new(
                    "recovery_invalid",
                    "Recovery payload is shared by inconsistent records; preserve the files before discarding",
                ));
            }
            if request.dry_run {
                return Ok(
                    json!({"recovery":id,"dry_run":true,"discard":"protected restoration information only","native_writes":false,"credentials_deleted":false}),
                );
            }
            secrets::remove(store, &record.payload_ref)?;
            index.records[position].state = "discarded".into();
            // Keep non-secret dependency tombstones: discarding the ability to
            // restore does not prove a plaintext native key was removed.
            store.write_json(INDEX, &index)?;
            Ok(
                json!({"recovery":id,"state":"discarded","native_writes":false,"credentials_deleted":false,"gateway_key_revoked":false}),
            )
        }
        _ => Err(Error::new(
            "unsupported_operation",
            "Unknown native recovery operation",
        )),
    }
}
pub fn secret_references(store: &Store, reference: &str) -> Result<Value> {
    let index = match store.read_json::<Index>(INDEX) {
        Ok(index) => index,
        Err(_) => {
            return Ok(
                json!({"state":"unknown","resources":[],"reason_code":"native_recovery_metadata_unavailable"}),
            );
        }
    };
    let mut resources = Vec::new();
    for record in &index.records {
        if record.payload_ref == reference && record.state != "discarded" {
            resources.push(json!({"kind":"native_recovery","id":record.id,"scope":record.scope}));
        }
        if record.state != "restored" && record.secret_refs.iter().any(|r| r == reference) {
            resources.push(json!({"kind":"native_handoff","id":record.target,"scope":record.scope,"recovery":record.id,"state":record.state}));
        }
    }
    Ok(
        json!({"state":if resources.is_empty(){"unreferenced"}else{"referenced"},"resources":resources,"reason_code":null}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn unsupported_recovery_payload_exposes_no_available_actions_and_stays_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let reference =
            secrets::put(&store, "agent:stable:managed", b"private fixture payload").unwrap();
        let name = format!(
            "secrets/{}.json",
            reference.strip_prefix("secret:").unwrap()
        );
        let mut envelope: Value = store.read_json(&name).unwrap();
        envelope["metadata"]["version"] = json!(99);
        store.write_json(&name, &envelope).unwrap();
        let before = fs::read(store.path(&name).unwrap()).unwrap();
        let record = Record {
            id: "recovery".into(),
            owner: "stable".into(),
            target: "managed".into(),
            agent: "pi".into(),
            scope: directory.path().join("native"),
            payload_ref: reference,
            secret_refs: vec![],
            state: "applied_reload_pending".into(),
            files: vec![],
        };
        let view = record_view(&store, &record);
        assert_eq!(view["available_actions"], json!([]));
        assert_eq!(view["restore_available"], false);
        assert_eq!(view["storage_reason_code"], "storage_version_unsupported");
        assert_eq!(fs::read(store.path(&name).unwrap()).unwrap(), before);
    }
    #[tokio::test]
    async fn recovery_discard_rejects_mismatched_or_shared_payload_without_deleting_secrets() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let gateway_ref = secrets::put(&store, "gateway", b"synthetic-gateway").unwrap();
        let mut record = Record {
            id: "recovery-one".into(),
            owner: "stable".into(),
            target: "managed".into(),
            agent: "pi".into(),
            scope: directory.path().join("native"),
            payload_ref: gateway_ref.clone(),
            secret_refs: vec![],
            state: "applied_reload_pending".into(),
            files: vec![],
        };
        let request = Request {
            command: vec!["agent".into(), "recovery".into(), "discard".into()],
            operands: vec![record.id.clone()],
            arguments: BTreeMap::new(),
            dry_run: false,
            if_revision: None,
        };
        store
            .write_json(
                INDEX,
                &Index {
                    records: vec![record.clone()],
                },
            )
            .unwrap();
        assert_eq!(
            recovery(&store, &request).await.unwrap_err().code,
            "recovery_invalid"
        );
        assert_eq!(
            secrets::get(&store, &gateway_ref).unwrap().as_slice(),
            b"synthetic-gateway"
        );
        record.payload_ref =
            secrets::put(&store, "agent:stable:managed", b"synthetic-recovery").unwrap();
        let mut duplicate = record.clone();
        duplicate.id = "recovery-two".into();
        store
            .write_json(
                INDEX,
                &Index {
                    records: vec![record.clone(), duplicate],
                },
            )
            .unwrap();
        assert_eq!(
            recovery(&store, &request).await.unwrap_err().code,
            "recovery_invalid"
        );
        assert!(secrets::get(&store, &record.payload_ref).is_ok());
        store
            .write_json(
                INDEX,
                &Index {
                    records: vec![record.clone()],
                },
            )
            .unwrap();
        assert_eq!(
            recovery(&store, &request).await.unwrap()["state"],
            "discarded"
        );
        assert!(secrets::get(&store, &record.payload_ref).is_err());
        assert!(secrets::get(&store, &gateway_ref).is_ok());
    }
    fn record(text: &str, before: Option<&str>, format: Format) -> (FileRecord, Capture) {
        let path = PathBuf::from("/isolated/settings.json");
        (
            FileRecord {
                path: path.clone(),
                format,
                original_absent: before.is_none(),
                after_revision: revision(Some(text)),
                fields: vec![Field {
                    path: vec!["model".into()],
                    before_hash: field_hash(
                        before
                            .and_then(|b| document::parse(b, format).ok())
                            .as_ref()
                            .and_then(|v| v.get("model")),
                    ),
                    after_hash: field_hash(document::parse(text, format).unwrap().get("model")),
                    observed_hash: "absent".into(),
                    released: false,
                }],
                written: true,
            },
            Capture {
                path,
                before: before.map(str::to_owned),
                after: text.into(),
                exact_original_safe: true,
                baseline_values: vec![],
            },
        )
    }
    #[test]
    fn plugin_restore_accepts_observed_prewrite_bytes_but_refuses_user_edits() {
        let (mut file, capture) = record("new-plugin", None, Format::Text);
        file.fields = vec![Field {
            path: vec!["$text".into()],
            before_hash: field_hash(None),
            after_hash: field_hash(Some(&json!("new-plugin"))),
            observed_hash: field_hash(Some(&json!("observed-plugin"))),
            released: false,
        }];
        for current in [Some("observed-plugin"), Some("new-plugin"), None] {
            assert_eq!(restore_text(current, &file, &capture).unwrap(), None);
        }
        assert_eq!(
            restore_text(Some("user-edited-plugin"), &file, &capture)
                .unwrap_err()
                .code,
            "native_conflict"
        );
    }
    #[test]
    fn three_way_restore_preserves_later_unrelated_settings_and_comments() {
        let (f, c) = record(
            "{\"model\":\"new\",\"temperature\":0.5}",
            Some("{\"model\":\"old\",\"temperature\":0.5}"),
            Format::Json,
        );
        let restored = restore_text(
            Some("{// user later\n\"model\":\"new\",\"temperature\":0.9}"),
            &f,
            &c,
        )
        .unwrap()
        .unwrap();
        assert!(restored.contains("// user later"));
        assert_eq!(
            document::parse(&restored, Format::Json).unwrap(),
            json!({"model":"old","temperature":0.9})
        );
    }
    #[test]
    fn three_way_restore_refuses_owned_user_edits() {
        let (f, c) = record(
            "{\"model\":\"new\"}",
            Some("{\"model\":\"old\"}"),
            Format::Json,
        );
        assert_eq!(
            restore_text(Some("{\"model\":\"user\"}"), &f, &c)
                .unwrap_err()
                .code,
            "native_conflict"
        );
    }
    #[test]
    fn exact_restore_recovers_original_bytes() {
        let (f, c) = record(
            "{\"model\":\"new\"}",
            Some("{\n  \"model\": \"old\" // keep\n}\n"),
            Format::Json,
        );
        assert_eq!(restore_text(Some(&c.after), &f, &c).unwrap(), c.before);
    }
    #[test]
    fn restore_never_overwrites_a_changed_native_credential_artifact() {
        let path = PathBuf::from("/isolated/native-owned.key");
        let file = FileRecord {
            path: path.clone(),
            format: Format::Credential,
            original_absent: true,
            after_revision: revision(Some("selected-key")),
            fields: vec![],
            written: true,
        };
        let capture = Capture {
            path,
            before: None,
            after: "selected-key".into(),
            exact_original_safe: true,
            baseline_values: vec![],
        };
        let error = restore_text(Some("native-owner-replaced-key"), &file, &capture).unwrap_err();
        assert_eq!(error.code, "native_conflict");
        assert!(!error.message.contains("native-owner-replaced-key"));
        assert_eq!(
            restore_text(Some("selected-key"), &file, &capture).unwrap(),
            None
        );
    }
    #[test]
    fn three_way_restore_keeps_later_owned_container_comments() {
        let path = PathBuf::from("/isolated/settings.json");
        let after = "{\"providers\":{\"managed\":{\"baseURL\":\"https://example.invalid\"}}}";
        let file = FileRecord {
            path: path.clone(),
            format: Format::Json,
            original_absent: true,
            after_revision: revision(Some(after)),
            fields: vec![Field {
                path: vec!["providers".into(), "managed".into(), "baseURL".into()],
                before_hash: field_hash(None),
                after_hash: field_hash(Some(&json!("https://example.invalid"))),
                observed_hash: field_hash(None),
                released: false,
            }],
            written: true,
        };
        let capture = Capture {
            path,
            before: None,
            after: after.into(),
            exact_original_safe: true,
            baseline_values: vec![],
        };
        let error=restore_text(Some("{\"providers\":{\"managed\":{/* later owner note */\"baseURL\":\"https://example.invalid\"}}}"),&file,&capture).unwrap_err();
        assert_eq!(error.code, "native_conflict");
    }
    #[test]
    fn discarded_native_handoff_still_blocks_secret_cleanup() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        let record = Record {
            id: "native-record".into(),
            owner: "stable".into(),
            target: "removed".into(),
            agent: "pi".into(),
            scope: home.path().join("native"),
            payload_ref: "capture".into(),
            secret_refs: vec!["upstream-ref".into()],
            state: "discarded".into(),
            files: vec![],
        };
        store
            .write_json(
                INDEX,
                &Index {
                    records: vec![record],
                },
            )
            .unwrap();
        assert_eq!(
            secret_references(&store, "upstream-ref").unwrap()["state"],
            "referenced"
        );
    }
    #[tokio::test]
    async fn dry_restore_creates_no_locks_and_checks_metadata_without_resolving_payload() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        let native = home.path().join("native");
        fs::create_dir(&native).unwrap();
        let payload_ref = secrets::put(
            &store,
            "agent:stable:removed",
            b"not a decoded recovery payload",
        )
        .unwrap();
        let record = Record {
            id: "native-record".into(),
            owner: "stable".into(),
            target: "removed".into(),
            agent: "pi".into(),
            scope: native,
            payload_ref,
            secret_refs: vec![],
            state: "write_pending".into(),
            files: vec![],
        };
        store
            .write_json(
                INDEX,
                &Index {
                    records: vec![record],
                },
            )
            .unwrap();
        let result = restore(&store, "native-record", true).await.unwrap();
        assert_eq!(result["secret_reads"], false);
        assert!(!store.root.join("locks/native.lock").exists());
    }
}
