//! Native Agent session listing and one-way import.
//!
//! The product contract is `specs/sessions/spec.md`. Native formats are read and
//! written only through the adapters in this module.
mod model;
mod readers;
mod records;
#[cfg(test)]
mod tests;
mod writers;

use crate::{Error, Request, Result, config, storage::Store};
use chrono::{DateTime, Utc};
use model::*;
use records::{ImportRecord, RecordState, Records};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};
use writers::{WriteRequest, Written};

fn arg<'a>(request: &'a Request, key: &str) -> Option<&'a str> {
    request
        .arguments
        .get(key)
        .and_then(|values| values.first())
        .map(String::as_str)
}

fn operand(request: &Request, index: usize) -> Result<&str> {
    request
        .operands
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| Error::new("invalid_arguments", "A required operand is missing"))
}

fn required<'a>(request: &'a Request, key: &str) -> Result<&'a str> {
    arg(request, key)
        .ok_or_else(|| Error::new("invalid_arguments", "A required option is missing").field(key))
}

fn flag(request: &Request, key: &str) -> bool {
    request
        .arguments
        .get(key)
        .is_some_and(|values| values.first().is_some_and(|value| value == "true"))
}

pub fn list(store: &Store, request: &Request) -> Result<Value> {
    let target = arg(request, "target");
    let agent = arg(request, "agent");
    let path = arg(request, "path");
    if target.is_some() && (agent.is_some() || path.is_some()) {
        return Err(Error::new(
            "invalid_arguments",
            "Select one declared target or an explicit agent scope, not both",
        ));
    }
    if agent.is_some() != path.is_some() {
        return Err(Error::new(
            "invalid_arguments",
            "An explicit session scope requires both --agent and --path",
        ));
    }
    let limit = parse_limit(arg(request, "limit"))?;
    let wanted_cwd = match arg(request, "cwd") {
        Some(value) => Some(normalize_cwd(value).ok_or_else(|| {
            Error::new(
                "invalid_arguments",
                "--cwd must be an absolute path without traversal",
            )
            .field("cwd")
        })?),
        None => None,
    };
    let since = parse_instant(arg(request, "since"), "since")?;
    let until = parse_instant(arg(request, "until"), "until")?;
    if let (Some(since), Some(until)) = (since, until)
        && since > until
    {
        return Err(Error::new(
            "invalid_arguments",
            "--since must not be later than --until",
        ));
    }
    let config = if agent.is_some() && path.is_some() {
        None
    } else {
        Some(config::load(store)?.config)
    };
    let mut prepared = Vec::new();
    if let (Some(agent), Some(path)) = (agent, path) {
        if !model::agent_supported(agent) {
            return Err(Error::new(
                "unsupported_agent",
                "Supported agents are codex, pi, omp, opencode, hermes, and dsh",
            ));
        }
        let scope = scope_path(store, &PathBuf::from(path))
            .map_err(|error| Error::new("invalid_arguments", &error.message).field("path"))?;
        let resolved = resolve_store(store, agent, &scope);
        prepared.push(Prepared {
            label: None,
            agent: agent.to_owned(),
            scope,
            declared_targets: Vec::new(),
            resolved,
        });
    } else if let Some(target) = target {
        let declared = config_target(
            config
                .as_ref()
                .ok_or_else(|| Error::new("invalid_arguments", "A target selector is required"))?,
            target,
        )?;
        let resolved = resolve_store(store, &declared.agent, &declared.scope);
        prepared.push(Prepared {
            label: Some(target.to_owned()),
            agent: declared.agent.clone(),
            scope: declared.scope.clone(),
            declared_targets: vec![target.to_owned()],
            resolved,
        });
    } else {
        let config = config
            .as_ref()
            .ok_or_else(|| Error::new("invalid_arguments", "A target selector is required"))?;
        for (label, target) in &config.agents {
            let resolved = resolve_store(store, &target.agent, &target.scope);
            prepared.push(Prepared {
                label: Some(label.clone()),
                agent: target.agent.clone(),
                scope: target.scope.clone(),
                declared_targets: vec![label.clone()],
                resolved,
            });
        }
    }
    // A store is shared by every declared target that resolves to it, even when
    // only one target was selected for scanning. Resolve all declared targets for
    // this metadata projection; scanning remains limited to the selected scope.
    let mut by_identity: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(config) = &config {
        for (label, target) in &config.agents {
            if let Ok(resolved) = resolve_store(store, &target.agent, &target.scope) {
                by_identity
                    .entry(resolved.identity)
                    .or_default()
                    .push(label.clone());
            }
        }
    }
    for item in &mut prepared {
        if item.label.is_none() {
            item.declared_targets.clear();
            continue;
        }
        if let Ok(resolved) = &item.resolved {
            item.declared_targets = by_identity
                .get(&resolved.identity)
                .into_iter()
                .flatten()
                .filter(|label| !label.is_empty())
                .cloned()
                .collect();
            item.declared_targets.sort();
            item.declared_targets.dedup();
        }
    }
    let records = Records::load(store);
    let mut scans = Vec::new();
    for item in prepared {
        scans.push(scan_target(item, &records));
    }
    let mut merged: BTreeMap<(String, String), SessionRow> = BTreeMap::new();
    let mut cwd_unknown_ids: BTreeSet<(String, String)> = BTreeSet::new();
    let mut timestamp_unknown_ids: BTreeSet<(String, String)> = BTreeSet::new();
    for scan in &mut scans {
        let mut scan_cwd_unknown = BTreeSet::new();
        let mut scan_timestamp_unknown = BTreeSet::new();
        scan.rows.retain(|row| {
            let key = || (row.store.clone(), row.native_id.clone());
            if let Some(wanted) = &wanted_cwd
                && !cwd_matches(row.cwd.as_deref(), wanted)
            {
                if row.cwd.is_none() || row.cwd.as_deref().and_then(normalize_cwd).is_none() {
                    scan_cwd_unknown.insert(key());
                }
                return false;
            }
            if since.is_some() || until.is_some() {
                let Some(updated) = row.updated_at.as_deref().and_then(parse_instant_value) else {
                    scan_timestamp_unknown.insert(key());
                    return false;
                };
                if since.as_ref().is_some_and(|since| updated < *since)
                    || until.as_ref().is_some_and(|until| updated > *until)
                {
                    return false;
                }
            }
            true
        });
        scan.cwd_unknown = scan_cwd_unknown.len() as u64;
        scan.timestamp_unknown = scan_timestamp_unknown.len() as u64;
        cwd_unknown_ids.extend(scan_cwd_unknown);
        timestamp_unknown_ids.extend(scan_timestamp_unknown);
        for row in scan.rows.drain(..) {
            let key = (row.store.clone(), row.native_id.clone());
            match merged.get_mut(&key) {
                Some(existing) => {
                    existing.targets.extend(row.targets);
                    existing.targets.sort();
                    existing.targets.dedup();
                    if existing.target.as_deref().is_none_or(|current| {
                        row.target.as_deref().is_some_and(|next| next < current)
                    }) {
                        existing.target = row.target;
                    }
                }
                None => {
                    merged.insert(key, row);
                }
            }
        }
    }
    let mut rows: Vec<SessionRow> = merged.into_values().collect();
    for row in &mut rows {
        if let Some(first) = row.targets.first() {
            row.target = Some(first.clone());
        }
    }
    rows.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.native_id.cmp(&b.native_id))
    });
    let scan_truncated = scans.iter().any(|scan| {
        scan.reason_code
            .as_deref()
            .is_some_and(|code| code.contains("truncated"))
    });
    let truncated = rows.len() > limit || scan_truncated;
    let total_sessions = rows.len() as u64;
    rows.truncate(limit);
    let coverage = if scans.iter().all(|scan| scan.state == "scanned") {
        "complete"
    } else {
        "partial"
    };
    let mut warnings = Vec::new();
    let mut omitted = serde_json::Map::new();
    for scan in &scans {
        let code = match scan.state.as_str() {
            "scanned" => None,
            "absent" => Some("target_absent"),
            "partial" => Some("target_partial"),
            "unsupported" => Some("target_unsupported"),
            "unresolved" => Some("target_unresolved"),
            "unreadable" => Some("target_unreadable"),
            _ => Some("target_incomplete"),
        };
        if let Some(code) = code {
            warnings.push(json!({
                "code": code,
                "target": scan.label,
                "path": scan.path,
                "reason_code": scan.reason_code,
            }));
        }
    }
    let cwd_unknown = cwd_unknown_ids.len() as u64;
    let timestamp_unknown = timestamp_unknown_ids.len() as u64;
    if cwd_unknown > 0 {
        warnings.push(json!({"code": "cwd_unknown", "count": cwd_unknown}));
        omitted.insert(
            "sessions_excluded_by_unknown_cwd".to_owned(),
            Value::from(cwd_unknown),
        );
    }
    if timestamp_unknown > 0 {
        warnings.push(json!({"code": "timestamp_unknown", "count": timestamp_unknown}));
        omitted.insert(
            "sessions_excluded_by_unknown_timestamp".to_owned(),
            Value::from(timestamp_unknown),
        );
    }
    if total_sessions > rows.len() as u64 {
        omitted.insert(
            "sessions_excluded_by_limit".to_owned(),
            Value::from(total_sessions - rows.len() as u64),
        );
    }
    let targets: Vec<Value> = scans
        .into_iter()
        .map(|scan| {
            json!({
                "target": scan.label,
                "scope": scan.scope,
                "agent": scan.agent,
                "path": scan.path,
                "store": scan.store,
                "state": scan.state,
                "reason_code": scan.reason_code,
                "cwd_unknown": scan.cwd_unknown,
                "timestamp_unknown": scan.timestamp_unknown,
            })
        })
        .collect();
    Ok(json!({
        "sessions": rows.iter().map(row_json).collect::<Vec<_>>(),
        "targets": targets,
        "coverage": coverage,
        "counts": {
            "targets": targets.len(),
            "sessions": total_sessions,
            "returned": rows.len(),
        },
        "warnings": warnings,
        "omitted": omitted,
        "limit": limit,
        "truncated": truncated,
    }))
}

struct Prepared {
    label: Option<String>,
    agent: String,
    scope: PathBuf,
    declared_targets: Vec<String>,
    resolved: Result<ResolvedStore>,
}

fn scan_target(item: Prepared, records: &Records) -> TargetScan {
    let scope = item.scope.to_string_lossy().into_owned();
    let resolved = match item.resolved {
        Ok(resolved) => resolved,
        Err(error) => {
            let label = item.label;
            let explicit = label.is_none().then_some(scope);
            return TargetScan {
                label,
                scope: explicit,
                agent: item.agent,
                path: item.scope,
                store: None,
                state: if error.code == "session_store_unresolved" {
                    "unresolved".into()
                } else {
                    "unreadable".into()
                },
                reason_code: Some(error.code),
                rows: Vec::new(),
                cwd_unknown: 0,
                timestamp_unknown: 0,
            };
        }
    };
    let outcome = match readers::list(&resolved) {
        Ok(outcome) => outcome,
        Err(error) => {
            let state = if error.code == "native_schema_unsupported" {
                "unsupported"
            } else {
                "unreadable"
            };
            let label = item.label;
            let explicit = label.is_none().then_some(scope);
            return TargetScan {
                label,
                scope: explicit,
                agent: item.agent,
                path: resolved.path,
                store: Some(resolved.identity),
                state: state.into(),
                reason_code: Some(error.code),
                rows: Vec::new(),
                cwd_unknown: 0,
                timestamp_unknown: 0,
            };
        }
    };
    let mut rows = outcome.rows;
    for row in &mut rows {
        row.target = item.label.clone();
        row.targets = item.declared_targets.clone();
        row.scope = item.label.is_none().then(|| scope.clone());
        row.imported_from = records.imported_from(&row.store, &row.native_id);
    }
    let explicit = item.label.is_none().then_some(scope);
    TargetScan {
        label: item.label,
        scope: explicit,
        agent: item.agent,
        path: resolved.path,
        store: Some(resolved.identity),
        state: outcome.kind.as_str().into(),
        reason_code: outcome.reason_code,
        rows,
        cwd_unknown: 0,
        timestamp_unknown: 0,
    }
}

fn parse_limit(value: Option<&str>) -> Result<usize> {
    let Some(value) = value else {
        return Ok(DEFAULT_LIMIT);
    };
    let parsed = value.parse::<usize>().map_err(|_| {
        Error::new(
            "invalid_arguments",
            "--limit must be a positive integer no greater than 500",
        )
        .field("limit")
    })?;
    if !(1..=MAX_LIMIT).contains(&parsed) {
        return Err(Error::new(
            "invalid_arguments",
            "--limit must be a positive integer no greater than 500",
        )
        .field("limit"));
    }
    Ok(parsed)
}

fn parse_instant(value: Option<&str>, field: &str) -> Result<Option<DateTime<Utc>>> {
    let Some(value) = value else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(value)
        .map(|value| Some(value.with_timezone(&Utc)))
        .map_err(|_| {
            Error::new(
                "invalid_arguments",
                "Time filters require RFC 3339 with an explicit offset",
            )
            .field(field)
        })
}

fn parse_instant_value(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

fn row_json(row: &SessionRow) -> Value {
    let mut map = Map::new();
    map.insert("target".into(), json!(row.target));
    map.insert("targets".into(), json!(row.targets));
    map.insert("scope".into(), json!(row.scope));
    map.insert("agent".into(), json!(row.agent));
    map.insert("native_id".into(), json!(row.native_id));
    map.insert("title".into(), json!(row.title));
    map.insert("cwd".into(), json!(row.cwd));
    map.insert("workspace_roots".into(), json!(row.workspace_roots));
    map.insert("created_at".into(), json!(row.created_at));
    map.insert("updated_at".into(), json!(row.updated_at));
    map.insert("model".into(), json!(row.model));
    map.insert("archived".into(), json!(row.archived));
    map.insert("hidden".into(), json!(row.hidden));
    map.insert("message_count".into(), json!(row.message_count));
    map.insert("tool_call_count".into(), json!(row.tool_call_count));
    map.insert("state".into(), json!(row.state));
    map.insert("imported_from".into(), json!(row.imported_from));
    map.insert("store".into(), json!(row.store));
    Value::Object(map)
}

pub fn import(store: &Store, request: &Request) -> Result<Value> {
    let source_target = operand(request, 0)?;
    let native_id = operand(request, 1)?;
    let destination_target = required(request, "to")?;
    let title_override = arg(request, "title");
    let tail = match arg(request, "tail") {
        Some(value) => Some(value.parse::<usize>().map_err(|_| {
            Error::new("invalid_arguments", "--tail must be a positive integer").field("tail")
        })?),
        None => None,
    };
    if tail == Some(0) {
        return Err(
            Error::new("invalid_arguments", "--tail must be a positive integer").field("tail"),
        );
    }
    let allow_over_budget = flag(request, "allow-over-budget");
    if tail.is_some() && allow_over_budget {
        return Err(Error::new(
            "invalid_arguments",
            "--tail and --allow-over-budget cannot be combined",
        ));
    }
    if let Some(title) = title_override
        && (title.trim().is_empty()
            || title.chars().any(char::is_control)
            || title.chars().count() > MAX_TITLE_CHARS)
    {
        return Err(Error::new(
            "invalid_arguments",
            "--title must be nonempty, bounded, and contain no control characters",
        )
        .field("title"));
    }
    let config = config::load(store)?.config;
    let source = config_target(&config, source_target)?;
    let destination = config_target(&config, destination_target)?;
    let source_store = resolve_store(store, &source.agent, &source.scope).map_err(|error| {
        Error::new(
            "session_store_unresolved",
            &format!(
                "The source target has no single resolved session store: {}",
                error.message
            ),
        )
    })?;
    let destination_store =
        resolve_store(store, &destination.agent, &destination.scope).map_err(|error| {
            Error::new(
                "session_store_unresolved",
                &format!(
                    "The destination target has no single resolved session store: {}",
                    error.message
                ),
            )
        })?;
    if source_store.identity == destination_store.identity {
        return Err(Error::new(
            "same_session_store",
            "The source and destination targets resolve to the same native session store",
        ));
    }
    if writers::writer_for(&destination.agent).is_none() {
        return Err(Error::new(
            "unsupported_destination",
            "No qualified native session writer is available for this destination",
        ));
    }
    let mut snapshot = readers::read_source(&source_store, native_id)?;
    match snapshot.state.as_str() {
        "readable" => {}
        "incomplete" => {
            return Err(Error::new(
                "source_incomplete",
                "The selected source branch is incomplete or unreadable",
            ));
        }
        "unsupported_version" => {
            return Err(Error::new(
                "unsupported_source",
                "The selected source has no qualified reader schema",
            ));
        }
        _ => {
            return Err(Error::new(
                "source_unreadable",
                "The selected source session is not readable",
            ));
        }
    }
    if destination.agent == "opencode"
        && snapshot
            .model
            .as_deref()
            .and_then(|value| value.split_once('/'))
            .is_none()
    {
        return Err(Error::new(
            "model_unknown",
            "The destination requires a recorded provider/model identity",
        ));
    }
    let cwd = effective_cwd(&snapshot, arg(request, "cwd"))?;
    let cwd_text = cwd.to_str().map(str::to_owned).ok_or_else(|| {
        Error::new(
            "destination_cwd_unrepresentable",
            "The destination cannot represent the selected working directory",
        )
    })?;
    let _record_lock = store.lock("session-imports")?;
    let mut records = Records::load(store);
    let mut warnings = Vec::new();
    if records.state == RecordState::Unavailable {
        warnings.push(json!({
            "code": "record_unavailable",
            "message": "Import provenance storage is unavailable; repeat detection is unavailable for this operation",
        }));
    }
    if !flag(request, "again") {
        let existing: Vec<ImportRecord> = records
            .matching(
                &source_store.identity,
                native_id,
                &destination_store.identity,
            )
            .cloned()
            .collect();
        let mut present = Vec::new();
        for record in existing {
            if readers::contains(&destination_store, &record.destination_native_id)? {
                present.push(crate::ResourceRef {
                    kind: "session".into(),
                    id: Some(record.destination_native_id),
                    scope: Some(destination_target.to_owned()),
                });
            }
        }
        if !present.is_empty() {
            let mut error = Error::new(
                "already_imported",
                "This source session was already imported to the destination store",
            );
            error.affected = present;
            return Err(error);
        }
    }
    let workspace_omitted = snapshot
        .workspace_roots
        .as_ref()
        .map(|roots| roots.len() as u64)
        .unwrap_or(0);
    if workspace_omitted > 0 {
        count_omitted(
            &mut snapshot.omitted,
            "workspace_roots_not_imported",
            workspace_omitted,
        );
    }
    let requested_title = title_override
        .map(str::to_owned)
        .unwrap_or_else(|| snapshot.title.clone());
    let title = resolve_title(&destination_store, &requested_title, native_id)?;
    warnings.push(json!({
        "code": "context_unknown",
        "message": "The destination model context limit is unknown; no silent truncation or inferred limit was applied",
    }));
    let mut selected_snapshot = snapshot.clone();
    if let Some(turns) = tail {
        select_tail(&mut selected_snapshot, turns)?;
    }
    let estimated = selected_snapshot
        .entries
        .iter()
        .map(|entry| match entry {
            Entry::User { text } | Entry::Assistant { text } => text.len(),
            Entry::Tool {
                name,
                arguments,
                outcome,
                ..
            } => name.len() + arguments.len() + outcome.len(),
        })
        .sum::<usize>();
    let context = json!({
        "state": "unknown",
        "model": selected_snapshot.model,
        "limit": null,
        "reserve": null,
        "estimated": estimated,
        "selected_turns": selected_snapshot.counts.turns,
        "reason_code": "context_unknown",
    });
    let omitted = json!(selected_snapshot.omitted);
    let counts = json!({
        "turns": selected_snapshot.counts.turns,
        "messages": selected_snapshot.counts.messages,
        "tool_activity": selected_snapshot.counts.tools,
        "compaction_records": selected_snapshot.counts.compactions,
    });
    let source_view = json!({
        "target": source_target,
        "store": source_store.identity,
        "native_id": native_id,
        "revision": snapshot.revision,
        "model": snapshot.model,
        "state": snapshot.state,
        "cwd": snapshot.cwd,
        "workspace_roots": snapshot.workspace_roots,
    });
    if request.dry_run {
        writers::qualify(&destination_store)?;
        return Ok(json!({
            "source": source_view,
            "destination": {
                "target": destination_target,
                "native_id": null,
                "store": destination_store.identity,
                "title": title,
                "cwd": cwd_text,
            },
            "counts": counts,
            "omitted": omitted,
            "context": context,
            "visibility": "not_attempted",
            "record": {"state":"not_written"},
            "resume": null,
            "warnings": warnings,
        }));
    }
    let written = writers::write(
        &destination_store,
        WriteRequest {
            snapshot: &selected_snapshot,
            title: &title,
            cwd: &cwd,
            scope: &scope_path(store, &destination.scope)?,
            home: &store.home,
        },
    )?;
    let readback = readers::read_source(&destination_store, &written.native_id)
        .map_err(|_| verification_failure(&written, destination_target))?;
    if readback.state != "readable"
        || readback.cwd.as_deref() != Some(written.cwd.as_str())
        || readback.title != written.title
        || portable_text(&readback) != portable_text(&selected_snapshot)
    {
        return Err(verification_failure(&written, destination_target));
    }
    let record = ImportRecord {
        source_store: source_store.identity.clone(),
        source_native_id: native_id.to_owned(),
        source_revision: snapshot.revision.clone(),
        destination_store: destination_store.identity.clone(),
        destination_native_id: written.native_id.clone(),
        source_target: source_target.to_owned(),
        destination_target: destination_target.to_owned(),
        source_schema: source_schema(&source.agent).to_owned(),
        destination_schema: written.schema,
        title: title.clone(),
        source_cwd: snapshot.cwd.clone(),
        destination_cwd: Some(written.cwd.clone()),
        workspace_roots_omitted: workspace_omitted,
        counts: counts.clone(),
        imported_at: rfc3339_millis(Utc::now()),
    };
    let mut record_pruned = false;
    let record_state = if records.state == RecordState::Unavailable {
        "unavailable"
    } else {
        record_pruned = records.insert(record);
        match records::save(store, &records) {
            Ok(byte_pruned) => {
                record_pruned |= byte_pruned;
                "created"
            }
            Err(_) => "failed",
        }
    };
    if record_state == "created" && record_pruned {
        warnings.push(json!({
            "code":"record_pruned",
            "message":"The oldest import provenance record was pruned to stay within the bounded record store",
        }));
    }
    if record_state == "failed" {
        warnings.push(json!({
            "code":"record_failed",
            "message":"The destination session is verified, but its provenance record could not be written",
        }));
    }
    let mut value = json!({
        "source": source_view,
        "destination": {
            "target": destination_target,
            "native_id": written.native_id,
            "store": destination_store.identity,
            "title": written.title,
            "cwd": written.cwd,
        },
        "counts": counts,
        "omitted": omitted,
        "context": context,
        "visibility": "verified",
        "record": {"state": record_state},
        "resume": written.resume,
        "warnings": warnings,
    });
    if record_state == "failed" {
        value["_code"] = json!("record_failed");
    }
    Ok(value)
}

fn verification_failure(written: &Written, destination_target: &str) -> Error {
    if let Some(path) = &written.cleanup_path {
        match fs::remove_file(path) {
            Ok(()) => {
                return Error::new(
                    "native_write_failed",
                    "The file-based destination was not verifiable and was removed before import completion",
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Error::new(
                    "native_write_failed",
                    "The file-based destination was not verifiable and is no longer present",
                );
            }
            Err(_) => {}
        }
    }
    let mut error = Error::new(
        "committed_unverified",
        "The destination session was written but did not pass native readback verification; inspect it before retrying",
    );
    error.affected = vec![crate::ResourceRef {
        kind: "session".into(),
        id: Some(written.native_id.clone()),
        scope: Some(destination_target.to_owned()),
    }];
    error
}

fn effective_cwd(snapshot: &Snapshot, explicit: Option<&str>) -> Result<PathBuf> {
    let selected = match explicit {
        Some(value) => PathBuf::from(value),
        None => snapshot.cwd.as_deref().map(PathBuf::from).ok_or_else(|| {
            Error::new(
                "source_cwd_missing",
                "The source records no primary working directory; supply --cwd",
            )
        })?,
    };
    if explicit.is_some() && !selected.is_absolute() {
        return Err(Error::new(
            "invalid_arguments",
            "--cwd must be an absolute existing directory",
        )
        .field("cwd"));
    }
    let normalized = normalize_cwd(&selected.to_string_lossy()).ok_or_else(|| {
        Error::new(
            "destination_cwd_unrepresentable",
            "The selected working directory has no absolute lexical form",
        )
    })?;
    if !normalized.is_dir() {
        return Err(Error::new(
            "destination_cwd_missing",
            "The selected working directory does not exist or is not a directory",
        ));
    }
    Ok(normalized)
}

fn resolve_title(binding: &ResolvedStore, wanted: &str, native_id: &str) -> Result<String> {
    let mut base = if wanted.trim().is_empty() {
        native_id.to_owned()
    } else {
        wanted.chars().take(MAX_TITLE_CHARS).collect()
    };
    base = base
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let base = base.trim().to_owned();
    let base = if base.is_empty() {
        native_id.to_owned()
    } else {
        base
    };
    let mut existing = BTreeSet::new();
    if let Ok(outcome) = readers::list(binding) {
        for row in outcome.rows {
            if let Some(title) = row.title {
                existing.insert(title);
            }
        }
    }
    for suffix in 0..100 {
        let title = if suffix == 0 {
            base.clone()
        } else {
            let suffix = format!(" ({})", suffix + 1);
            let keep = MAX_TITLE_CHARS.saturating_sub(suffix.chars().count());
            format!("{}{}", base.chars().take(keep).collect::<String>(), suffix)
        };
        if !existing.contains(&title) {
            return Ok(title);
        }
    }
    Err(Error::new(
        "title_collision",
        "No unique destination title could be produced within the bounded suffix range",
    ))
}

fn select_tail(snapshot: &mut Snapshot, turns: usize) -> Result<()> {
    let mut starts = Vec::new();
    for (index, entry) in snapshot.entries.iter().enumerate() {
        if matches!(entry, Entry::User { .. }) {
            starts.push(index);
        }
    }
    if starts.is_empty() {
        return Err(Error::new(
            "source_incomplete",
            "The source has no user turn to retain",
        ));
    }
    if starts.len().saturating_sub(turns) <= 1 {
        return Ok(());
    }
    let keep_from = starts[starts.len() - turns];
    let first = starts[0];
    let dropped_turns = starts.len().saturating_sub(turns).saturating_sub(1);
    let mut selected = Vec::new();
    if first < keep_from {
        selected.push(snapshot.entries[first].clone());
        selected.push(Entry::Assistant {
            text: format!("[tmax omission marker: {dropped_turns} earlier turn(s) omitted]"),
        });
    }
    selected.extend(snapshot.entries[keep_from..].iter().cloned());
    let dropped_bytes = snapshot.entries[..keep_from]
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != first)
        .map(|(_, entry)| match entry {
            Entry::User { text } | Entry::Assistant { text } => text.len(),
            Entry::Tool {
                name,
                arguments,
                outcome,
                ..
            } => name.len() + arguments.len() + outcome.len(),
        })
        .sum::<usize>();
    count_omitted(
        &mut snapshot.omitted,
        "turns_omitted_by_tail",
        dropped_turns as u64,
    );
    count_omitted(
        &mut snapshot.omitted,
        "bytes_omitted_by_tail",
        dropped_bytes as u64,
    );
    snapshot.entries = selected;
    let mut counts = CanonicalCounts::default();
    for entry in &snapshot.entries {
        match entry {
            Entry::User { .. } => {
                counts.turns += 1;
                counts.messages += 1;
            }
            Entry::Assistant { .. } => counts.messages += 1,
            Entry::Tool { .. } => counts.tools += 1,
        }
    }
    counts.compactions = snapshot.counts.compactions;
    snapshot.counts = counts;
    Ok(())
}

fn portable_text(snapshot: &Snapshot) -> Vec<String> {
    snapshot
        .entries
        .iter()
        .map(|entry| match entry {
            Entry::User { text } | Entry::Assistant { text } => text.clone(),
            Entry::Tool { .. } => writers::tool_text(entry),
        })
        .collect()
}

fn source_schema(agent: &str) -> &'static str {
    match agent {
        "codex" => "codex.rollout",
        "pi" => "pi.session.v3",
        "omp" => "omp.session.v3",
        "opencode" => "opencode.session_v2",
        "hermes" => "hermes.state.v1",
        "dsh" => "dsh.session.v4",
        _ => "unknown",
    }
}
