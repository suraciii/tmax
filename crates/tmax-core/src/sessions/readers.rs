//! Version-qualified, read-only native session readers.
//!
//! Each branch is intentionally self-contained: one native schema family, one
//! bounded read. Unknown fields are ignored; only adapter-mapped fields enter a
//! row or canonical snapshot.
use super::model::*;
use crate::{Error, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn list(binding: &ResolvedStore) -> Result<ListOutcome> {
    if !binding.path.exists() {
        return Ok(ListOutcome {
            rows: Vec::new(),
            kind: ListKind::Absent,
            reason_code: None,
        });
    }
    match binding.kind {
        StoreKind::Codex => list_codex(binding),
        StoreKind::Pi | StoreKind::Omp => list_pi_omp(binding),
        StoreKind::OpenCode => list_opencode(binding),
        StoreKind::Hermes => list_hermes(binding),
        StoreKind::Dsh => list_dsh(binding),
    }
}

fn completed(rows: Vec<SessionRow>, reason_code: Option<String>) -> ListOutcome {
    let partial = rows.iter().any(|row| {
        matches!(
            row.state.as_str(),
            "incomplete" | "unsupported_version" | "unreadable"
        )
    });
    let all_unsupported =
        !rows.is_empty() && rows.iter().all(|row| row.state == "unsupported_version");
    let kind = if all_unsupported {
        ListKind::Unsupported
    } else if partial {
        ListKind::Partial
    } else {
        ListKind::Scanned
    };
    ListOutcome {
        rows,
        kind,
        reason_code: if kind == ListKind::Partial && reason_code.is_none() {
            Some("native_rows_unreadable".into())
        } else {
            reason_code
        },
    }
}

fn empty_row(
    binding: &ResolvedStore,
    native_id: String,
    title: Option<String>,
    cwd: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    state: &str,
) -> SessionRow {
    SessionRow {
        target: None,
        targets: Vec::new(),
        scope: None,
        agent: binding.agent.clone(),
        native_id,
        title,
        cwd,
        workspace_roots: None,
        created_at,
        updated_at,
        model: None,
        archived: None,
        hidden: None,
        message_count: None,
        tool_call_count: None,
        state: state.to_owned(),
        store: binding.identity.clone(),
        imported_from: None,
    }
}

pub(crate) fn codex_home(binding: &ResolvedStore) -> PathBuf {
    binding
        .path
        .parent()
        .map(Path::to_owned)
        .unwrap_or_else(|| binding.path.clone())
}

pub(crate) fn codex_index(binding: &ResolvedStore) -> Option<PathBuf> {
    let home = codex_home(binding);
    let mut found: Vec<(u64, PathBuf)> = fs::read_dir(home)
        .ok()?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?;
            let version = name
                .strip_prefix("state_")?
                .strip_suffix(".sqlite")?
                .parse()
                .ok()?;
            Some((version, path))
        })
        .collect();
    found.sort_by_key(|(version, _)| *version);
    found.pop().map(|(_, path)| path)
}

fn codex_required_columns() -> [&'static str; 7] {
    [
        "id",
        "rollout_path",
        "created_at",
        "updated_at",
        "cwd",
        "title",
        "archived",
    ]
}

fn list_codex(binding: &ResolvedStore) -> Result<ListOutcome> {
    if let Some(db) = codex_index(binding) {
        match list_codex_index(binding, &db) {
            Ok(mut outcome) if !outcome.rows.is_empty() => {
                if outcome.kind == ListKind::Partial && outcome.reason_code.is_none() {
                    outcome.reason_code = Some("native_index_partial".into());
                }
                return Ok(outcome);
            }
            Ok(_) => {}
            Err(error) if error.code == "native_schema_unsupported" => {}
            Err(error) => return Err(error),
        }
    }
    list_codex_files(binding)
}

fn list_codex_index(binding: &ResolvedStore, db: &Path) -> Result<ListOutcome> {
    let columns = table_columns(db, "threads")?;
    if columns.is_empty() {
        return Err(Error::new(
            "native_schema_unsupported",
            "The native Codex session index has no recognized schema",
        ));
    }
    if codex_required_columns()
        .iter()
        .any(|name| !columns.iter().any(|column| column == name))
    {
        return Err(Error::new(
            "native_schema_unsupported",
            "The native Codex session index has an unrecognized schema signature",
        ));
    }
    let optional = [
        "model",
        "model_provider",
        "first_user_message",
        "preview",
        "archived_at",
        "history_mode",
    ];
    let mut selected: Vec<&str> = codex_required_columns().to_vec();
    for name in optional {
        if columns.iter().any(|column| column == name) {
            selected.push(name);
        }
    }
    let filter = if columns.iter().any(|column| column == "thread_source") {
        " WHERE (thread_source IS NULL OR thread_source = 'user')"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {} FROM threads{} ORDER BY updated_at DESC, id DESC LIMIT {};",
        selected.join(", "),
        filter,
        MAX_NATIVE_ROWS + 1
    );
    let value = sqlite_json(db, &sql)?;
    let records = value.as_array().cloned().unwrap_or_default();
    let truncated = records.len() > MAX_NATIVE_ROWS;
    let mut rows = Vec::new();
    for record in records.into_iter().take(MAX_NATIVE_ROWS) {
        let native_id = record
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if native_id.is_empty() {
            continue;
        }
        let rollout = record
            .get("rollout_path")
            .and_then(Value::as_str)
            .map(PathBuf::from);
        let state = match rollout.as_ref() {
            Some(path) if codex_rollout_readable(path) => "readable",
            _ => "unreadable",
        };
        let title = record
            .get("title")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .or_else(|| record.get("preview").and_then(Value::as_str))
            .or_else(|| record.get("first_user_message").and_then(Value::as_str))
            .map(bounded_title)
            .filter(|value| !value.is_empty());
        let cwd = record.get("cwd").and_then(Value::as_str).map(str::to_owned);
        let created = record
            .get("created_at")
            .and_then(Value::as_i64)
            .and_then(from_unix_seconds_i64);
        let updated = record
            .get("updated_at")
            .and_then(Value::as_i64)
            .and_then(from_unix_seconds_i64);
        let mut row = empty_row(binding, native_id, title, cwd, created, updated, state);
        row.archived = record
            .get("archived")
            .and_then(Value::as_i64)
            .map(|value| value != 0);
        row.model = match (
            record.get("model_provider").and_then(Value::as_str),
            record.get("model").and_then(Value::as_str),
        ) {
            (Some(provider), Some(model)) if !provider.is_empty() => {
                Some(format!("{provider}/{model}"))
            }
            (_, Some(model)) => Some(model.to_owned()),
            _ => None,
        };
        rows.push(row);
    }
    let reason = truncated.then(|| "native_rows_truncated".to_owned());
    let mut outcome = completed(rows, reason);
    if truncated && outcome.kind == ListKind::Scanned {
        outcome.kind = ListKind::Partial;
    }
    Ok(outcome)
}

fn codex_rollout_readable(path: &Path) -> bool {
    path.is_file() || PathBuf::from(format!("{}.zst", path.to_string_lossy())).is_file()
}

fn from_unix_seconds_i64(value: i64) -> Option<String> {
    Utc.timestamp_opt(value, 0).single().map(rfc3339_millis)
}

fn list_codex_files(binding: &ResolvedStore) -> Result<ListOutcome> {
    let mut files = files_under_depth(&binding.path, ".jsonl", MAX_LIST_FILES, 4)?;
    files.extend(files_under_depth(
        &binding.path,
        ".jsonl.zst",
        MAX_LIST_FILES,
        4,
    )?);
    files.sort();
    files.dedup();
    let mut rows = Vec::new();
    for path in files.into_iter().take(MAX_LIST_FILES) {
        let bytes = match read_listing_bytes(&path, MAX_METADATA_FILE_BYTES) {
            Ok(bytes) => bytes,
            Err(_) => {
                let id = native_id_from_path(&path).unwrap_or_else(|| "unknown".into());
                rows.push(empty_row(
                    binding,
                    id,
                    None,
                    None,
                    None,
                    file_updated(&path),
                    "unreadable",
                ));
                continue;
            }
        };
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(_) => {
                let id = native_id_from_path(&path).unwrap_or_else(|| "unknown".into());
                rows.push(empty_row(
                    binding,
                    id,
                    None,
                    None,
                    None,
                    file_updated(&path),
                    "unreadable",
                ));
                continue;
            }
        };
        let mut header = None;
        let mut title = None;
        let mut first_user = None;
        let mut state = "readable";
        let mut updated = file_updated(&path);
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                state = "incomplete";
                continue;
            };
            match value.get("type").and_then(Value::as_str) {
                Some("session_meta") => {
                    header = value.get("payload").cloned();
                    if let Some(timestamp) = header
                        .as_ref()
                        .and_then(|header| {
                            header.get("timestamp").or_else(|| header.get("created_at"))
                        })
                        .and_then(parse_timestamp)
                    {
                        updated = Some(timestamp);
                    }
                }
                Some("response_item") => {
                    let payload = value.get("payload");
                    if payload
                        .and_then(|payload| payload.get("type"))
                        .and_then(Value::as_str)
                        == Some("message")
                    {
                        let role = payload
                            .and_then(|payload| payload.get("role"))
                            .and_then(Value::as_str);
                        let text = payload
                            .and_then(|payload| payload.get("content"))
                            .map(text_from_content)
                            .map(|(text, _)| text)
                            .unwrap_or_default();
                        if first_user.is_none() && role == Some("user") {
                            first_user = title_from_prompt(&text);
                        }
                    }
                }
                _ => {}
            }
            if let Some(timestamp) = value.get("timestamp").and_then(parse_timestamp) {
                updated = Some(timestamp);
            }
        }
        let Some(header) = header else {
            let id = native_id_from_path(&path).unwrap_or_else(|| "unknown".into());
            rows.push(empty_row(
                binding,
                id,
                None,
                None,
                None,
                updated,
                "unreadable",
            ));
            continue;
        };
        if header
            .get("thread_source")
            .and_then(Value::as_str)
            .is_some_and(|source| source != "user")
        {
            continue;
        }
        let native_id = header
            .get("id")
            .or_else(|| header.get("session_id"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| native_id_from_path(&path))
            .unwrap_or_else(|| "unknown".into());
        let cwd = header.get("cwd").and_then(Value::as_str).map(str::to_owned);
        let created = header.get("timestamp").and_then(parse_timestamp);
        if title.is_none() {
            title = first_user;
        }
        let mut row = empty_row(binding, native_id, title, cwd, created, updated, state);
        row.workspace_roots = header
            .get("runtime_workspace_roots")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|value| row.cwd.as_deref() != Some(*value))
                    .map(str::to_owned)
                    .collect()
            });
        row.model = header
            .get("model_provider")
            .and_then(Value::as_str)
            .map(str::to_owned);
        rows.push(row);
    }
    let reason = (rows.len() >= MAX_LIST_FILES).then(|| "native_files_truncated".to_owned());
    let mut outcome = completed(rows, reason);
    if outcome.kind == ListKind::Scanned && outcome.reason_code.is_some() {
        outcome.kind = ListKind::Partial;
    }
    Ok(outcome)
}

fn file_updated(path: &Path) -> Option<String> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(rfc3339_millis(DateTime::<Utc>::from(modified)))
}

pub(crate) fn read_metadata_bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(Error::new(
            "unsafe_native_file",
            "Native session metadata must not be a symlink",
        ));
    }
    let mut file = fs::File::open(path).map_err(|_| {
        Error::new(
            "native_store_unreadable",
            "Native session metadata cannot be read",
        )
    })?;
    use std::io::Read;
    let mut bytes = vec![0; limit as usize];
    let read = file.read(&mut bytes).map_err(|_| {
        Error::new(
            "native_store_unreadable",
            "Native session metadata cannot be read",
        )
    })?;
    bytes.truncate(read);
    // Listing reads only a bounded prefix. A record cut by the window is not a
    // torn native record: remove the incomplete trailing line and stop.
    if read as u64 == limit
        && let Some(position) = bytes.iter().rposition(|byte| *byte == b'\n')
    {
        bytes.truncate(position + 1);
    }
    Ok(bytes)
}

fn read_listing_bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("zst") {
        return read_metadata_bytes(path, limit);
    }
    let bytes = zstd_decompress(path)?;
    let mut bytes = bytes;
    if bytes.len() as u64 > limit {
        bytes.truncate(limit as usize);
        if let Some(position) = bytes.iter().rposition(|byte| *byte == b'\n') {
            bytes.truncate(position + 1);
        }
    }
    Ok(bytes)
}

fn list_pi_omp(binding: &ResolvedStore) -> Result<ListOutcome> {
    let files = primary_pi_files(binding)?;
    let mut rows = Vec::new();
    for path in files.into_iter().take(MAX_LIST_FILES) {
        if !looks_like_pi_session(&path) {
            continue;
        }
        let bytes = match read_metadata_bytes(&path, MAX_METADATA_FILE_BYTES) {
            Ok(bytes) => bytes,
            Err(_) => {
                let id = native_id_from_path(&path).unwrap_or_else(|| "unknown".into());
                rows.push(empty_row(
                    binding,
                    id,
                    None,
                    None,
                    None,
                    file_updated(&path),
                    "unreadable",
                ));
                continue;
            }
        };
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(_) => {
                let id = native_id_from_path(&path).unwrap_or_else(|| "unknown".into());
                rows.push(empty_row(
                    binding,
                    id,
                    None,
                    None,
                    None,
                    file_updated(&path),
                    "unreadable",
                ));
                continue;
            }
        };
        let mut header = None;
        let mut title = None;
        let mut first_user = None;
        let mut created = None;
        let mut updated = file_updated(&path);
        let mut model = None;
        let mut state = "readable";
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                state = "incomplete";
                continue;
            };
            match value.get("type").and_then(Value::as_str) {
                Some("session") => {
                    header = Some(value.clone());
                    created = header
                        .as_ref()
                        .and_then(|header| header.get("timestamp"))
                        .and_then(parse_timestamp);
                }
                Some("title") | Some("title_change") => {
                    if let Some(value) = value
                        .get("title")
                        .and_then(Value::as_str)
                        .map(bounded_title)
                        .filter(|value| !value.is_empty())
                    {
                        title = Some(value);
                    }
                    if let Some(timestamp) = value.get("updatedAt").and_then(parse_timestamp) {
                        updated = Some(timestamp);
                    }
                }
                Some("model_change") => {
                    if let Some(model_id) = value.get("modelId").and_then(Value::as_str) {
                        let provider = value.get("provider").and_then(Value::as_str);
                        model = Some(match provider {
                            Some(provider) if !provider.is_empty() => {
                                format!("{provider}/{model_id}")
                            }
                            _ => model_id.to_owned(),
                        });
                    }
                }
                Some("message") => {
                    let role = value
                        .get("message")
                        .and_then(|message| message.get("role"))
                        .and_then(Value::as_str);
                    if first_user.is_none() && role == Some("user") {
                        first_user = value
                            .get("message")
                            .and_then(|message| message.get("content"))
                            .map(text_from_content)
                            .map(|(text, _)| text)
                            .and_then(|text| title_from_prompt(&text));
                    }
                }
                _ => {}
            }
            if let Some(timestamp) = value.get("timestamp").and_then(parse_timestamp) {
                updated = Some(timestamp);
            }
        }
        let Some(header) = header else {
            let id = native_id_from_path(&path).unwrap_or_else(|| "unknown".into());
            rows.push(empty_row(
                binding,
                id,
                None,
                None,
                None,
                updated,
                "unreadable",
            ));
            continue;
        };
        let native_id = header
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| native_id_from_path(&path))
            .unwrap_or_else(|| "unknown".into());
        let version = header.get("version").and_then(Value::as_i64);
        if version != Some(3) {
            state = "unsupported_version";
        }
        let cwd = header.get("cwd").and_then(Value::as_str).map(str::to_owned);
        if title.is_none() {
            title = header
                .get("title")
                .and_then(Value::as_str)
                .map(bounded_title)
                .filter(|value| !value.is_empty())
                .or(first_user);
        }
        let mut row = empty_row(binding, native_id, title, cwd, created, updated, state);
        row.workspace_roots = header.get("workspace_roots").map(|value| {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|value| row.cwd.as_deref() != Some(*value))
                .map(str::to_owned)
                .collect()
        });
        row.model = model;
        rows.push(row);
    }
    let reason = (rows.len() >= MAX_LIST_FILES).then(|| "native_files_truncated".to_owned());
    let mut outcome = completed(rows, reason);
    if outcome.kind == ListKind::Partial && outcome.reason_code.is_none() {
        outcome.reason_code = Some("native_rows_unreadable".into());
    }
    if outcome.kind == ListKind::Scanned && outcome.reason_code.is_some() {
        outcome.kind = ListKind::Partial;
    }
    Ok(outcome)
}

pub(crate) fn looks_like_pi_session(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(stem) = name.strip_suffix(".jsonl") else {
        return false;
    };
    let Some((stamp, id)) = stem.rsplit_once('_') else {
        return false;
    };
    stamp.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && id.len() >= 6
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn list_hermes(binding: &ResolvedStore) -> Result<ListOutcome> {
    let columns = table_columns(&binding.path, "sessions")?;
    if columns.is_empty() {
        return Err(Error::new(
            "native_schema_unsupported",
            "The native Hermes session database has no recognized schema",
        ));
    }
    let required = [
        "id",
        "model",
        "started_at",
        "message_count",
        "cwd",
        "title",
        "archived",
        "hidden",
    ];
    if required
        .iter()
        .any(|name| !columns.iter().any(|column| column == name))
    {
        return Err(Error::new(
            "native_schema_unsupported",
            "The native Hermes session database has an unrecognized schema signature",
        ));
    }
    let optional = [
        "ended_at",
        "last_activity_at",
        "tool_call_count",
        "profile_name",
    ];
    let mut selected = required.to_vec();
    for name in optional {
        if columns.iter().any(|column| column == name) {
            selected.push(name);
        }
    }
    let sql = format!(
        "SELECT {} FROM sessions ORDER BY started_at DESC, id DESC LIMIT {};",
        selected.join(", "),
        MAX_NATIVE_ROWS + 1
    );
    let value = sqlite_json(&binding.path, &sql)?;
    let records = value.as_array().cloned().unwrap_or_default();
    let truncated = records.len() > MAX_NATIVE_ROWS;
    let mut rows = Vec::new();
    for record in records.into_iter().take(MAX_NATIVE_ROWS) {
        let Some(native_id) = record.get("id").and_then(Value::as_str) else {
            continue;
        };
        let title = record
            .get("title")
            .and_then(Value::as_str)
            .map(bounded_title)
            .filter(|value| !value.is_empty());
        let cwd = record.get("cwd").and_then(Value::as_str).map(str::to_owned);
        let created = record
            .get("started_at")
            .and_then(Value::as_f64)
            .and_then(from_unix_seconds);
        let updated = record
            .get("last_activity_at")
            .and_then(Value::as_f64)
            .and_then(from_unix_seconds)
            .or_else(|| {
                record
                    .get("ended_at")
                    .and_then(Value::as_f64)
                    .and_then(from_unix_seconds)
            })
            .or_else(|| created.clone());
        let mut row = empty_row(
            binding,
            native_id.to_owned(),
            title,
            cwd,
            created,
            updated,
            "readable",
        );
        row.workspace_roots = Some(Vec::new());
        row.model = record
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned);
        row.archived = record
            .get("archived")
            .and_then(Value::as_i64)
            .map(|value| value != 0);
        row.hidden = record
            .get("hidden")
            .and_then(Value::as_i64)
            .map(|value| value != 0);
        row.message_count = record
            .get("message_count")
            .and_then(Value::as_i64)
            .and_then(|value| u64::try_from(value).ok());
        row.tool_call_count = record
            .get("tool_call_count")
            .and_then(Value::as_i64)
            .and_then(|value| u64::try_from(value).ok());
        rows.push(row);
    }
    let reason = truncated.then(|| "native_rows_truncated".to_owned());
    let mut outcome = completed(rows, reason);
    if truncated && outcome.kind == ListKind::Scanned {
        outcome.kind = ListKind::Partial;
    }
    Ok(outcome)
}

fn list_opencode(binding: &ResolvedStore) -> Result<ListOutcome> {
    let columns = table_columns(&binding.path, "session_v2")?;
    if columns.is_empty() {
        return Err(Error::new(
            "native_schema_unsupported",
            "The native OpenCode session database has no recognized current schema",
        ));
    }
    let required = [
        "id",
        "directory",
        "title",
        "model",
        "time_created",
        "time_updated",
    ];
    if required
        .iter()
        .any(|name| !columns.iter().any(|column| column == name))
    {
        return Err(Error::new(
            "native_schema_unsupported",
            "The native OpenCode session database has an unrecognized schema signature",
        ));
    }
    let optional = ["parent_id", "time_archived", "time_suspended"];
    let mut selected = required.to_vec();
    for name in optional {
        if columns.iter().any(|column| column == name) {
            selected.push(name);
        }
    }
    let sql = format!(
        "SELECT {} FROM session_v2 ORDER BY time_updated DESC, id DESC LIMIT {};",
        selected.join(", "),
        MAX_NATIVE_ROWS + 1
    );
    let value = sqlite_json(&binding.path, &sql)?;
    let records = value.as_array().cloned().unwrap_or_default();
    let truncated = records.len() > MAX_NATIVE_ROWS;
    let mut rows = Vec::new();
    for record in records.into_iter().take(MAX_NATIVE_ROWS) {
        let Some(native_id) = record.get("id").and_then(Value::as_str) else {
            continue;
        };
        let title = record
            .get("title")
            .and_then(Value::as_str)
            .map(bounded_title)
            .filter(|value| !value.is_empty());
        let cwd = record
            .get("directory")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let created = record
            .get("time_created")
            .and_then(Value::as_i64)
            .and_then(from_unix_millis);
        let updated = record
            .get("time_updated")
            .and_then(Value::as_i64)
            .and_then(from_unix_millis);
        let mut row = empty_row(
            binding,
            native_id.to_owned(),
            title,
            cwd,
            created,
            updated,
            "readable",
        );
        row.model = record
            .get("model")
            .and_then(Value::as_str)
            .and_then(|value| serde_json::from_str::<Value>(value).ok())
            .and_then(|value| {
                let provider = value.get("providerID").and_then(Value::as_str)?;
                let model = value.get("id").and_then(Value::as_str)?;
                Some(format!("{provider}/{model}"))
            });
        row.archived = record
            .get("time_archived")
            .and_then(Value::as_i64)
            .map(|value| value != 0);
        row.hidden = record
            .get("time_suspended")
            .and_then(Value::as_i64)
            .map(|value| value != 0);
        rows.push(row);
    }
    let reason = truncated.then(|| "native_rows_truncated".to_owned());
    let mut outcome = completed(rows, reason);
    if truncated && outcome.kind == ListKind::Scanned {
        outcome.kind = ListKind::Partial;
    }
    Ok(outcome)
}

fn list_dsh(binding: &ResolvedStore) -> Result<ListOutcome> {
    let sessions = dsh_session_files(&binding.path, MAX_LIST_FILES)?;
    let mut rows = Vec::new();
    for (dir, path, version) in sessions.into_iter().take(MAX_LIST_FILES) {
        let fallback_id = dir
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("session-"))
            .unwrap_or("unknown")
            .to_owned();
        if version != 4 {
            let mut row = empty_row(
                binding,
                fallback_id,
                None,
                None,
                None,
                file_updated(&path),
                "unsupported_version",
            );
            row.workspace_roots = None;
            rows.push(row);
            continue;
        }
        let bytes = match zstd_decompress(&path) {
            Ok(bytes) => bytes,
            Err(_) => {
                rows.push(empty_row(
                    binding,
                    fallback_id,
                    None,
                    None,
                    None,
                    file_updated(&path),
                    "unreadable",
                ));
                continue;
            }
        };
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(_) => {
                rows.push(empty_row(
                    binding,
                    fallback_id,
                    None,
                    None,
                    None,
                    file_updated(&path),
                    "unreadable",
                ));
                continue;
            }
        };
        let mut header = None;
        let mut title = None;
        let mut first_user = None;
        let mut model = None;
        let mut updated = file_updated(&path);
        let mut state = "readable";
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                state = "incomplete";
                continue;
            };
            match value.get("type").and_then(Value::as_str) {
                Some("session") => {
                    header = Some(value.clone());
                }
                Some("session/title") => {
                    if let Some(value) = value
                        .get("data")
                        .and_then(|data| data.get("title"))
                        .and_then(Value::as_str)
                        .map(bounded_title)
                        .filter(|value| !value.is_empty())
                    {
                        title = Some(value);
                    }
                }
                Some("model/selection") => {
                    let data = value.get("data");
                    model = match (
                        data.and_then(|data| data.get("provider"))
                            .and_then(Value::as_str),
                        data.and_then(|data| data.get("model"))
                            .and_then(Value::as_str),
                    ) {
                        (Some(provider), Some(model)) => Some(format!("{provider}/{model}")),
                        (_, Some(model)) => Some(model.to_owned()),
                        _ => model,
                    };
                }
                Some("user/message") => {
                    first_user = first_user.take().or_else(|| dsh_user_prompt(&value));
                }
                _ => {}
            }
            if let Some(timestamp) = value.get("time").and_then(parse_timestamp) {
                updated = Some(timestamp);
            }
        }
        let Some(header) = header else {
            rows.push(empty_row(
                binding,
                fallback_id,
                None,
                None,
                None,
                updated,
                "unreadable",
            ));
            continue;
        };
        let native_id = header
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or(fallback_id);
        let cwd = header.get("cwd").and_then(Value::as_str).map(str::to_owned);
        let created = header.get("createdAt").and_then(parse_timestamp);
        if title.is_none() {
            title = first_user;
        }
        let mut row = empty_row(binding, native_id, title, cwd, created, updated, state);
        row.model = model;
        rows.push(row);
    }
    let reason = (rows.len() >= MAX_LIST_FILES).then(|| "native_files_truncated".to_owned());
    let mut outcome = completed(rows, reason);
    if outcome.kind == ListKind::Scanned && outcome.reason_code.is_some() {
        outcome.kind = ListKind::Partial;
    }
    Ok(outcome)
}

fn dsh_session_files(root: &Path, max: usize) -> Result<Vec<(PathBuf, PathBuf, u64)>> {
    let mut dirs = Vec::new();
    let entries = fs::read_dir(root).map_err(|_| {
        Error::new(
            "native_store_unreadable",
            "Native DSH session store cannot be read",
        )
    })?;
    let mut project_dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| {
            Error::new(
                "native_store_unreadable",
                "Native DSH session store cannot be read",
            )
        })?;
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            project_dirs.push(entry.path());
        }
    }
    for project in project_dirs {
        let mut candidates = vec![project.clone()];
        if let Ok(children) = fs::read_dir(&project) {
            for child in children.flatten() {
                if child.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                    candidates.push(child.path());
                }
            }
        }
        for dir in candidates {
            if let Some((version, path)) = dsh_best_generation(&dir) {
                dirs.push((dir, path, version));
                if dirs.len() >= max {
                    break;
                }
            }
        }
        if dirs.len() >= max {
            break;
        }
    }
    dirs.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(dirs)
}

fn dsh_best_generation(dir: &Path) -> Option<(u64, PathBuf)> {
    let mut best: Option<(u64, PathBuf)> = None;
    for candidate in fs::read_dir(dir).ok()?.flatten() {
        let path = candidate.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(rest) = name.strip_prefix("session.v") else {
            continue;
        };
        let Some(version) = rest
            .strip_suffix(".jsonl.zstd")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        if best.as_ref().is_none_or(|(current, _)| version > *current) {
            best = Some((version, path));
        }
    }
    best
}

pub(crate) fn primary_pi_files(binding: &ResolvedStore) -> Result<Vec<PathBuf>> {
    let mut files = files_under(&binding.path, ".jsonl", MAX_LIST_FILES)?;
    files.retain(|path| {
        path.strip_prefix(&binding.path)
            .map(|relative| relative.components().count() == 2)
            .unwrap_or(false)
    });
    Ok(files)
}

fn find_pi_omp_file(binding: &ResolvedStore, native_id: &str) -> Result<PathBuf> {
    let files = primary_pi_files(binding)?;
    let mut failure = None;
    for path in files {
        if native_id_from_path(&path).as_deref() == Some(native_id) {
            return Ok(path);
        }
        let bytes = match read_metadata_bytes(&path, MAX_METADATA_FILE_BYTES) {
            Ok(bytes) => bytes,
            Err(error) => {
                failure.get_or_insert(error);
                continue;
            }
        };
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(_) => {
                failure.get_or_insert_with(|| {
                    Error::new(
                        "source_unreadable",
                        "Native session metadata is not valid UTF-8",
                    )
                });
                continue;
            }
        };
        if text.lines().any(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .is_some_and(|value| {
                    value.get("type").and_then(Value::as_str) == Some("session")
                        && value.get("id").and_then(Value::as_str) == Some(native_id)
                })
        }) {
            return Ok(path);
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    Err(Error::new(
        "unknown_resource",
        "The selected native session does not exist",
    ))
}

pub(crate) fn contains(binding: &ResolvedStore, native_id: &str) -> Result<bool> {
    match binding.kind {
        StoreKind::Pi | StoreKind::Omp => match find_pi_omp_file(binding, native_id) {
            Ok(_) => Ok(true),
            Err(error) if error.code == "unknown_resource" => Ok(false),
            Err(error) => Err(error),
        },
        StoreKind::Codex => {
            if let Some(db) = codex_index(binding)
                && table_columns(&db, "threads")?
                    .iter()
                    .any(|column| column == "id")
            {
                let value = sqlite_json(
                    &db,
                    &format!(
                        "SELECT 1 AS present FROM threads WHERE id={} LIMIT 1;",
                        sqlite_quote(native_id)
                    ),
                )?;
                return Ok(value.as_array().is_some_and(|rows| !rows.is_empty()));
            }
            let mut files = files_under_depth(&binding.path, ".jsonl", MAX_LIST_FILES, 4)?;
            files.extend(files_under_depth(
                &binding.path,
                ".jsonl.zst",
                MAX_LIST_FILES,
                4,
            )?);
            Ok(files
                .into_iter()
                .any(|path| native_id_from_path(&path).as_deref() == Some(native_id)))
        }
        StoreKind::Hermes => {
            if !binding.path.exists() {
                return Ok(false);
            }
            let value = sqlite_json(
                &binding.path,
                &format!(
                    "SELECT 1 AS present FROM sessions WHERE id={} LIMIT 1;",
                    sqlite_quote(native_id)
                ),
            )?;
            Ok(value.as_array().is_some_and(|rows| !rows.is_empty()))
        }
        StoreKind::OpenCode => {
            if !binding.path.exists() {
                return Ok(false);
            }
            let value = sqlite_json(
                &binding.path,
                &format!(
                    "SELECT 1 AS present FROM session_v2 WHERE id={} LIMIT 1;",
                    sqlite_quote(native_id)
                ),
            )?;
            Ok(value.as_array().is_some_and(|rows| !rows.is_empty()))
        }
        StoreKind::Dsh => {
            if !binding.path.exists() {
                return Ok(false);
            }
            let files = dsh_session_files(&binding.path, MAX_LIST_FILES)?;
            for (_, path, _) in files {
                let bytes = zstd_decompress(&path)?;
                let text = std::str::from_utf8(&bytes).map_err(|_| {
                    Error::new("source_unreadable", "Native DSH session is not valid UTF-8")
                })?;
                if text.lines().any(|line| {
                    serde_json::from_str::<Value>(line)
                        .ok()
                        .is_some_and(|value| {
                            value.get("type").and_then(Value::as_str) == Some("session")
                                && value.get("id").and_then(Value::as_str) == Some(native_id)
                        })
                }) {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

// ---- canonical source readers ----

pub(crate) fn read_source(binding: &ResolvedStore, native_id: &str) -> Result<Snapshot> {
    match binding.kind {
        StoreKind::Pi | StoreKind::Omp => read_pi_omp(binding, native_id),
        StoreKind::Codex => read_codex(binding, native_id),
        StoreKind::Hermes => read_hermes(binding, native_id),
        StoreKind::OpenCode => read_opencode(binding, native_id),
        StoreKind::Dsh => read_dsh(binding, native_id),
    }
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn empty_snapshot(binding: &ResolvedStore, native_id: &str) -> Snapshot {
    Snapshot {
        revision: format!("{}:{}", binding.identity, native_id),
        model: None,
        cwd: None,
        workspace_roots: None,
        title: String::new(),
        created_at: None,
        updated_at: None,
        entries: Vec::new(),
        counts: CanonicalCounts::default(),
        omitted: BTreeMap::new(),
        state: "readable".to_owned(),
    }
}

fn read_pi_omp(binding: &ResolvedStore, native_id: &str) -> Result<Snapshot> {
    let path = find_pi_omp_file(binding, native_id)?;
    let bytes = read_bounded(&path, MAX_SESSION_BYTES)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error::new("source_unreadable", "Native session is not valid UTF-8"))?;
    let mut snapshot = empty_snapshot(binding, native_id);
    snapshot.revision = hash(&bytes);
    let mut header = None;
    let mut nodes: Vec<PiNode> = Vec::new();
    let mut title = None;
    let mut updated = None;
    let mut incomplete = false;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => {
                incomplete = true;
                continue;
            }
        };
        if let Some(timestamp) = value.get("timestamp").and_then(parse_timestamp) {
            updated = Some(timestamp);
        }
        match value.get("type").and_then(Value::as_str) {
            Some("session") => {
                header = Some(value);
            }
            Some("title") | Some("title_change") => {
                if let Some(value) = value
                    .get("title")
                    .and_then(Value::as_str)
                    .map(bounded_title)
                    .filter(|value| !value.is_empty())
                {
                    title = Some(value);
                }
                if let Some(timestamp) = value.get("updatedAt").and_then(parse_timestamp) {
                    updated = Some(timestamp);
                }
            }
            Some("model_change") => {
                if let Some(model_id) = value.get("modelId").and_then(Value::as_str) {
                    let provider = value.get("provider").and_then(Value::as_str);
                    snapshot.model = Some(match provider {
                        Some(provider) if !provider.is_empty() => format!("{provider}/{model_id}"),
                        _ => model_id.to_owned(),
                    });
                }
            }
            Some("compaction") => {
                snapshot.counts.compactions += 1;
                count_omitted(&mut snapshot.omitted, "compaction_not_representable", 1);
            }
            Some("message") => {
                let Some(message) = value.get("message") else {
                    incomplete = true;
                    continue;
                };
                let role = message.get("role").and_then(Value::as_str).unwrap_or("");
                nodes.push(PiNode {
                    id: value
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    parent: value
                        .get("parentId")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    timestamp: value.get("timestamp").and_then(parse_timestamp),
                    role: role.to_owned(),
                    message: message.clone(),
                });
            }
            _ => {}
        }
    }
    let Some(header) = header else {
        return Err(Error::new(
            "source_incomplete",
            "Native session has no readable session header",
        ));
    };
    if header.get("version").and_then(Value::as_i64) != Some(3) {
        return Err(Error::new(
            "unsupported_source",
            "Native session has no qualified reader for its schema signature",
        ));
    }
    if header
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|header_id| header_id != native_id)
    {
        return Err(Error::new(
            "source_unreadable",
            "The native session header identity does not match the selected session",
        ));
    }
    snapshot.cwd = header.get("cwd").and_then(Value::as_str).map(str::to_owned);
    snapshot.workspace_roots = header.get("workspace_roots").map(|value| {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|value| snapshot.cwd.as_deref() != Some(*value))
            .map(str::to_owned)
            .collect()
    });
    snapshot.created_at = header.get("timestamp").and_then(parse_timestamp);
    snapshot.updated_at = updated;
    if incomplete {
        snapshot.state = "incomplete".to_owned();
    }
    let branch = select_branch(&nodes)?;
    let branch_first_user = branch
        .iter()
        .filter(|node| node.role == "user")
        .find_map(|node| {
            node.message
                .get("content")
                .map(text_from_content)
                .map(|(text, _)| text)
                .and_then(|text| title_from_prompt(&text))
        });
    snapshot.title = title
        .or_else(|| {
            header
                .get("title")
                .and_then(Value::as_str)
                .map(bounded_title)
                .filter(|value| !value.is_empty())
        })
        .or(branch_first_user)
        .unwrap_or_default();
    let total_messages = nodes.len();
    let branch_ids: BTreeSet<&str> = branch
        .iter()
        .filter_map(|node| (!node.id.is_empty()).then_some(node.id.as_str()))
        .collect();
    let other_branches = total_messages.saturating_sub(branch_ids.len());
    if other_branches > 0 {
        count_omitted(
            &mut snapshot.omitted,
            "other_branches",
            other_branches as u64,
        );
    }
    for node in branch {
        let role = node.role.as_str();
        match role {
            "user" => {
                let (text, omitted) = node
                    .message
                    .get("content")
                    .map(text_from_content)
                    .unwrap_or_default();
                count_omitted(&mut snapshot.omitted, "non_text_content", omitted);
                if !text.trim().is_empty() {
                    snapshot.entries.push(Entry::User { text });
                    snapshot.counts.turns += 1;
                    snapshot.counts.messages += 1;
                }
            }
            "assistant" => {
                let (text, omitted) = node
                    .message
                    .get("content")
                    .map(text_from_content)
                    .unwrap_or_default();
                count_omitted(&mut snapshot.omitted, "reasoning", omitted);
                if !text.trim().is_empty() {
                    snapshot.entries.push(Entry::Assistant { text });
                    snapshot.counts.messages += 1;
                }
                if let Some(content) = node.message.get("content").and_then(Value::as_array) {
                    for item in content {
                        if item.get("type").and_then(Value::as_str) == Some("toolCall") {
                            let name = item
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("tool")
                                .to_owned();
                            let arguments = bounded_tool(
                                &item
                                    .get("arguments")
                                    .map(Value::to_string)
                                    .unwrap_or_default(),
                            );
                            snapshot.entries.push(Entry::Tool {
                                name,
                                arguments,
                                outcome: String::new(),
                                error: false,
                                changed_paths: Vec::new(),
                            });
                            snapshot.counts.tools += 1;
                        }
                    }
                }
            }
            "toolResult" => {
                let (text, _) = node
                    .message
                    .get("content")
                    .map(text_from_content)
                    .unwrap_or_default();
                let name = node
                    .message
                    .get("toolName")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_owned();
                snapshot.entries.push(Entry::Tool {
                    name,
                    arguments: String::new(),
                    outcome: bounded_tool(&text),
                    error: node
                        .message
                        .get("isError")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    changed_paths: Vec::new(),
                });
                snapshot.counts.tools += 1;
            }
            "bashExecution" => {
                let name = "bash".to_owned();
                let arguments = bounded_tool(
                    node.message
                        .get("command")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                );
                let outcome = bounded_tool(
                    node.message
                        .get("output")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                );
                snapshot.entries.push(Entry::Tool {
                    name,
                    arguments,
                    outcome,
                    error: node
                        .message
                        .get("exitCode")
                        .and_then(Value::as_i64)
                        .is_some_and(|code| code != 0),
                    changed_paths: Vec::new(),
                });
                snapshot.counts.tools += 1;
            }
            "system" | "developer" => {
                count_omitted(&mut snapshot.omitted, "system_context", 1);
            }
            "custom" | "customMessage" | "custom_message" => {
                count_omitted(&mut snapshot.omitted, "provenance_unknown", 1);
            }
            _ => {
                count_omitted(&mut snapshot.omitted, "unknown_content", 1);
            }
        }
    }
    if snapshot.entries.is_empty() && snapshot.state == "readable" {
        snapshot.state = "incomplete".to_owned();
    }
    Ok(snapshot)
}

#[derive(Clone)]
struct PiNode {
    id: String,
    parent: Option<String>,
    timestamp: Option<String>,
    role: String,
    message: Value,
}

fn select_branch(nodes: &[PiNode]) -> Result<Vec<&PiNode>> {
    if nodes.is_empty() {
        return Err(Error::new(
            "source_incomplete",
            "Native session has no visible conversation branch",
        ));
    }
    let by_id: BTreeMap<&str, &PiNode> = nodes
        .iter()
        .filter(|node| !node.id.is_empty())
        .map(|node| (node.id.as_str(), node))
        .collect();
    let parents: BTreeSet<&str> = nodes
        .iter()
        .filter_map(|node| node.parent.as_deref())
        .filter(|parent| by_id.contains_key(parent))
        .collect();
    // A compacted or pruned native file can retain records whose parent is no
    // longer present. The latest identifiable leaf is still the visible branch;
    // disconnected older roots are counted as omitted branches, not guessed.
    let mut leaves: Vec<&PiNode> = nodes
        .iter()
        .filter(|node| node.id.is_empty() || !parents.contains(node.id.as_str()))
        .collect();
    if leaves.is_empty() {
        return Err(Error::new(
            "source_ambiguous_branch",
            "Native session has no identifiable visible leaf",
        ));
    }
    leaves.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    if leaves.len() > 1 && leaves[0].timestamp == leaves[1].timestamp {
        return Err(Error::new(
            "source_ambiguous_branch",
            "Native session has equally recent visible leaves",
        ));
    }
    let mut branch = Vec::new();
    let mut current = leaves[0];
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(current.id.as_str()) {
            return Err(Error::new(
                "source_incomplete",
                "Native session has a cyclic visible branch",
            ));
        }
        branch.push(current);
        let Some(parent) = current
            .parent
            .as_deref()
            .and_then(|parent| by_id.get(parent).copied())
        else {
            break;
        };
        current = parent;
    }
    branch.reverse();
    Ok(branch)
}

fn read_codex(binding: &ResolvedStore, native_id: &str) -> Result<Snapshot> {
    let mut index = None;
    if let Some(db) = codex_index(binding) {
        let columns = table_columns(&db, "threads")?;
        if columns.iter().any(|column| column == "id")
            && columns.iter().any(|column| column == "rollout_path")
        {
            let mut selected = vec!["id", "rollout_path"];
            for name in [
                "title",
                "preview",
                "first_user_message",
                "cwd",
                "model_provider",
                "model",
                "created_at",
                "updated_at",
                "archived",
            ] {
                if columns.iter().any(|column| column == name) {
                    selected.push(name);
                }
            }
            let value = sqlite_json(
                &db,
                &format!(
                    "SELECT {} FROM threads WHERE id={} LIMIT 1;",
                    selected.join(", "),
                    sqlite_quote(native_id)
                ),
            )?;
            index = value.as_array().and_then(|rows| rows.first()).cloned();
        }
    }
    let path = if let Some(record) = &index {
        record
            .get("rollout_path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
    } else {
        None
    };
    let path = match path {
        Some(path) if path.is_file() => path,
        _ => find_codex_file(binding, native_id)?,
    };
    let bytes = if path.extension().and_then(|ext| ext.to_str()) == Some("zst") {
        zstd_decompress(&path)?
    } else {
        read_bounded(&path, MAX_SESSION_BYTES)?
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        Error::new(
            "source_unreadable",
            "Native Codex session is not valid UTF-8",
        )
    })?;
    let mut snapshot = empty_snapshot(binding, native_id);
    snapshot.revision = hash(&bytes);
    let mut header = None;
    let mut first_user = None;
    let mut updated = None;
    let mut turn_model = None;
    let mut incomplete = false;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => {
                incomplete = true;
                continue;
            }
        };
        if let Some(timestamp) = value.get("timestamp").and_then(parse_timestamp) {
            updated = Some(timestamp);
        }
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") => {
                header = value.get("payload").cloned();
            }
            Some("turn_context") => {
                if turn_model.is_none() {
                    turn_model = value
                        .get("payload")
                        .and_then(|payload| payload.get("model"))
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
            }
            Some("response_item") => {
                let Some(payload) = value.get("payload") else {
                    continue;
                };
                match payload.get("type").and_then(Value::as_str) {
                    Some("message") => {
                        let role = payload.get("role").and_then(Value::as_str).unwrap_or("");
                        let (text, omitted) = payload
                            .get("content")
                            .map(text_from_content)
                            .unwrap_or_default();
                        if omitted > 0 {
                            count_omitted(&mut snapshot.omitted, "non_text_content", omitted);
                        }
                        match role {
                            "user" => {
                                if first_user.is_none() {
                                    first_user = title_from_prompt(&text);
                                }
                                if !text.trim().is_empty() {
                                    snapshot.entries.push(Entry::User { text });
                                    snapshot.counts.turns += 1;
                                    snapshot.counts.messages += 1;
                                }
                            }
                            "assistant" => {
                                if !text.trim().is_empty() {
                                    snapshot.entries.push(Entry::Assistant { text });
                                    snapshot.counts.messages += 1;
                                }
                            }
                            "system" | "developer" => {
                                count_omitted(&mut snapshot.omitted, "system_context", 1);
                            }
                            _ => {
                                count_omitted(&mut snapshot.omitted, "unknown_content", 1);
                            }
                        }
                    }
                    Some("reasoning") => {
                        count_omitted(&mut snapshot.omitted, "reasoning", 1);
                    }
                    Some(
                        "function_call" | "custom_tool_call" | "local_shell_call"
                        | "web_search_call",
                    ) => {
                        let name = payload
                            .get("name")
                            .and_then(Value::as_str)
                            .or_else(|| payload.get("type").and_then(Value::as_str))
                            .unwrap_or("tool")
                            .to_owned();
                        let arguments = bounded_tool(
                            &payload
                                .get("arguments")
                                .or_else(|| payload.get("input"))
                                .map(Value::to_string)
                                .unwrap_or_default(),
                        );
                        snapshot.entries.push(Entry::Tool {
                            name,
                            arguments,
                            outcome: String::new(),
                            error: false,
                            changed_paths: Vec::new(),
                        });
                        snapshot.counts.tools += 1;
                    }
                    Some(
                        "function_call_output"
                        | "custom_tool_call_output"
                        | "local_shell_call_output",
                    ) => {
                        let outcome = bounded_tool(
                            payload.get("output").and_then(Value::as_str).unwrap_or(""),
                        );
                        snapshot.entries.push(Entry::Tool {
                            name: "tool".into(),
                            arguments: String::new(),
                            outcome,
                            error: payload
                                .get("is_error")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            changed_paths: Vec::new(),
                        });
                        snapshot.counts.tools += 1;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    let Some(header) = header else {
        return Err(Error::new(
            "source_incomplete",
            "Native Codex session has no readable session header",
        ));
    };
    if header
        .get("id")
        .or_else(|| header.get("session_id"))
        .and_then(Value::as_str)
        .is_some_and(|header_id| header_id != native_id)
    {
        return Err(Error::new(
            "source_unreadable",
            "The native session header identity does not match the selected session",
        ));
    }
    snapshot.cwd = header.get("cwd").and_then(Value::as_str).map(str::to_owned);
    let provider = header
        .get("model_provider")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            index
                .as_ref()
                .and_then(|record| record.get("model_provider"))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        });
    if let Some(model) = turn_model {
        snapshot.model = Some(match provider {
            Some(provider) if !model.contains('/') => format!("{provider}/{model}"),
            _ => model,
        });
    }
    snapshot.workspace_roots = header.get("runtime_workspace_roots").map(|value| {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|value| snapshot.cwd.as_deref() != Some(*value))
            .map(str::to_owned)
            .collect()
    });
    snapshot.created_at = header.get("timestamp").and_then(parse_timestamp);
    snapshot.updated_at = updated;
    snapshot.title = index
        .as_ref()
        .and_then(|record| {
            record
                .get("title")
                .and_then(Value::as_str)
                .or_else(|| record.get("preview").and_then(Value::as_str))
        })
        .map(bounded_title)
        .filter(|value| !value.is_empty())
        .or(first_user)
        .unwrap_or_default();
    if snapshot.model.is_none() {
        snapshot.model = index.as_ref().and_then(|record| {
            let provider = record.get("model_provider").and_then(Value::as_str);
            let model = record.get("model").and_then(Value::as_str)?;
            Some(match provider {
                Some(provider) if !provider.is_empty() => format!("{provider}/{model}"),
                _ => model.to_owned(),
            })
        });
    }
    if incomplete {
        snapshot.state = "incomplete".to_owned();
    }
    if snapshot.entries.is_empty() {
        snapshot.state = "incomplete".to_owned();
    }
    Ok(snapshot)
}

fn find_codex_file(binding: &ResolvedStore, native_id: &str) -> Result<PathBuf> {
    let mut files = files_under_depth(&binding.path, ".jsonl", MAX_LIST_FILES, 4)?;
    files.extend(files_under_depth(
        &binding.path,
        ".jsonl.zst",
        MAX_LIST_FILES,
        4,
    )?);
    if let Some(path) = files
        .into_iter()
        .find(|path| native_id_from_path(path).as_deref() == Some(native_id))
    {
        return Ok(path);
    }
    Err(Error::new(
        "unknown_resource",
        "The selected native Codex session does not exist",
    ))
}

fn read_hermes(binding: &ResolvedStore, native_id: &str) -> Result<Snapshot> {
    let columns = table_columns(&binding.path, "sessions")?;
    if [
        "id",
        "model",
        "started_at",
        "cwd",
        "title",
        "message_count",
        "archived",
        "hidden",
    ]
    .iter()
    .any(|name| !columns.iter().any(|column| column == name))
    {
        return Err(Error::new(
            "native_schema_unsupported",
            "The native Hermes session database has an unrecognized source schema",
        ));
    }
    let mut selected = vec![
        "id",
        "model",
        "started_at",
        "cwd",
        "title",
        "message_count",
        "archived",
        "hidden",
    ];
    for name in ["tool_call_count", "ended_at", "last_activity_at"] {
        if columns.iter().any(|column| column == name) {
            selected.push(name);
        }
    }
    let message_columns = table_columns(&binding.path, "messages")?;
    let reasoning_columns: Vec<&str> = ["reasoning", "reasoning_content", "reasoning_details"]
        .into_iter()
        .filter(|name| message_columns.iter().any(|column| column == name))
        .collect();
    let session_sql = format!(
        "SELECT {} FROM sessions WHERE id={} LIMIT 1;",
        selected.join(", "),
        sqlite_quote(native_id)
    );
    let messages_sql = format!(
        "SELECT role, content, tool_name, tool_calls, timestamp{} FROM messages WHERE session_id={} AND active=1 ORDER BY id;",
        reasoning_columns
            .iter()
            .map(|name| format!(", {name}"))
            .collect::<String>(),
        sqlite_quote(native_id)
    );
    let sets = sqlite_json_sets(
        &binding.path,
        &format!("BEGIN; {session_sql} {messages_sql} COMMIT;"),
    )?;
    let record = sets
        .first()
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .cloned()
        .ok_or_else(|| {
            Error::new(
                "unknown_resource",
                "The selected native session does not exist",
            )
        })?;
    let mut snapshot = empty_snapshot(binding, native_id);
    snapshot.model = record
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_owned);
    snapshot.cwd = record.get("cwd").and_then(Value::as_str).map(str::to_owned);
    snapshot.created_at = record
        .get("started_at")
        .and_then(Value::as_f64)
        .and_then(from_unix_seconds);
    snapshot.updated_at = record
        .get("last_activity_at")
        .and_then(Value::as_f64)
        .and_then(from_unix_seconds)
        .or_else(|| {
            record
                .get("ended_at")
                .and_then(Value::as_f64)
                .and_then(from_unix_seconds)
        })
        .or_else(|| snapshot.created_at.clone());
    snapshot.revision = format!(
        "sqlite:{}:{}:{}",
        binding.identity,
        native_id,
        snapshot.updated_at.as_deref().unwrap_or("unknown")
    );
    snapshot.title = record
        .get("title")
        .and_then(Value::as_str)
        .map(bounded_title)
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let messages = sets
        .get(1)
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let mut first_user = None;
    for message in messages.as_array().into_iter().flatten() {
        if reasoning_columns.iter().any(|name| {
            message
                .get(*name)
                .is_some_and(|value| !value.is_null() && value.as_str() != Some(""))
        }) {
            count_omitted(&mut snapshot.omitted, "reasoning", 1);
        }
        let role = message.get("role").and_then(Value::as_str).unwrap_or("");
        let content = message.get("content").and_then(Value::as_str).unwrap_or("");
        match role {
            "user" => {
                if first_user.is_none() {
                    first_user = title_from_prompt(content);
                }
                if !content.trim().is_empty() {
                    snapshot.entries.push(Entry::User {
                        text: content.to_owned(),
                    });
                    snapshot.counts.turns += 1;
                    snapshot.counts.messages += 1;
                }
            }
            "assistant" => {
                if !content.trim().is_empty() {
                    snapshot.entries.push(Entry::Assistant {
                        text: content.to_owned(),
                    });
                    snapshot.counts.messages += 1;
                }
                if let Some(calls) = message.get("tool_calls").and_then(Value::as_str)
                    && let Ok(calls) = serde_json::from_str::<Value>(calls)
                {
                    for call in calls.as_array().into_iter().flatten() {
                        let name = call
                            .get("function")
                            .and_then(|function| function.get("name"))
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_owned();
                        let arguments = bounded_tool(
                            &call
                                .get("function")
                                .and_then(|function| function.get("arguments"))
                                .map(Value::to_string)
                                .unwrap_or_default(),
                        );
                        snapshot.entries.push(Entry::Tool {
                            name,
                            arguments,
                            outcome: String::new(),
                            error: false,
                            changed_paths: Vec::new(),
                        });
                        snapshot.counts.tools += 1;
                    }
                }
            }
            "tool" => {
                let name = message
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_owned();
                snapshot.entries.push(Entry::Tool {
                    name,
                    arguments: String::new(),
                    outcome: bounded_tool(content),
                    error: false,
                    changed_paths: Vec::new(),
                });
                snapshot.counts.tools += 1;
            }
            "system" | "developer" => {
                count_omitted(&mut snapshot.omitted, "system_context", 1);
            }
            _ => {
                count_omitted(&mut snapshot.omitted, "unknown_content", 1);
            }
        }
    }
    if snapshot.title.is_empty() {
        snapshot.title = first_user.unwrap_or_default();
    }
    if snapshot.entries.is_empty() {
        snapshot.state = "incomplete".to_owned();
    }
    Ok(snapshot)
}

fn read_opencode(binding: &ResolvedStore, native_id: &str) -> Result<Snapshot> {
    if !table_exists(&binding.path, "session_v2")?
        || !table_exists(&binding.path, "session_message")?
    {
        return Err(Error::new(
            "native_schema_unsupported",
            "The native OpenCode session database has no recognized current schema",
        ));
    }
    let session_columns = table_columns(&binding.path, "session_v2")?;
    if [
        "id",
        "directory",
        "title",
        "model",
        "time_created",
        "time_updated",
    ]
    .iter()
    .any(|name| !session_columns.iter().any(|column| column == name))
    {
        return Err(Error::new(
            "native_schema_unsupported",
            "The native OpenCode session database has an unrecognized source schema",
        ));
    }
    let session_sql = format!(
        "SELECT id, directory, title, model, time_created, time_updated FROM session_v2 WHERE id={} LIMIT 1;",
        sqlite_quote(native_id)
    );
    let messages_sql = format!(
        "SELECT type, data, time_created FROM session_message WHERE session_id={} ORDER BY seq, id;",
        sqlite_quote(native_id)
    );
    let sets = sqlite_json_sets(
        &binding.path,
        &format!("BEGIN; {session_sql} {messages_sql} COMMIT;"),
    )?;
    let record = sets
        .first()
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .cloned()
        .ok_or_else(|| {
            Error::new(
                "unknown_resource",
                "The selected native session does not exist",
            )
        })?;
    let mut snapshot = empty_snapshot(binding, native_id);
    snapshot.cwd = record
        .get("directory")
        .and_then(Value::as_str)
        .map(str::to_owned);
    snapshot.created_at = record
        .get("time_created")
        .and_then(Value::as_i64)
        .and_then(from_unix_millis);
    snapshot.updated_at = record
        .get("time_updated")
        .and_then(Value::as_i64)
        .and_then(from_unix_millis);
    snapshot.revision = format!(
        "sqlite:{}:{}:{}",
        binding.identity,
        native_id,
        snapshot.updated_at.as_deref().unwrap_or("unknown")
    );
    snapshot.title = record
        .get("title")
        .and_then(Value::as_str)
        .map(bounded_title)
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    snapshot.model = record
        .get("model")
        .and_then(Value::as_str)
        .and_then(|value| serde_json::from_str::<Value>(value).ok())
        .and_then(|value| {
            let provider = value.get("providerID").and_then(Value::as_str)?;
            let model = value.get("id").and_then(Value::as_str)?;
            Some(format!("{provider}/{model}"))
        });
    let messages = sets
        .get(1)
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let mut first_user = None;
    for message in messages.as_array().into_iter().flatten() {
        let message_type = message.get("type").and_then(Value::as_str).unwrap_or("");
        let data = match message
            .get("data")
            .and_then(Value::as_str)
            .and_then(|value| serde_json::from_str::<Value>(value).ok())
        {
            Some(data) => data,
            None => {
                snapshot.state = "incomplete".to_owned();
                Value::Null
            }
        };
        match message_type {
            "user" => {
                let text = data.get("text").and_then(Value::as_str).unwrap_or("");
                if first_user.is_none() {
                    first_user = title_from_prompt(text);
                }
                if !text.trim().is_empty() {
                    snapshot.entries.push(Entry::User {
                        text: text.to_owned(),
                    });
                    snapshot.counts.turns += 1;
                    snapshot.counts.messages += 1;
                }
            }
            "assistant" => {
                let content = data
                    .get("content")
                    .cloned()
                    .unwrap_or(Value::Array(Vec::new()));
                let (text, omitted) = text_from_content(&content);
                count_omitted(&mut snapshot.omitted, "reasoning", omitted);
                if !text.trim().is_empty() {
                    snapshot.entries.push(Entry::Assistant { text });
                    snapshot.counts.messages += 1;
                }
                for item in content.as_array().into_iter().flatten() {
                    if item.get("type").and_then(Value::as_str) != Some("tool") {
                        continue;
                    }
                    let name = item
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .to_owned();
                    let state = item.get("state");
                    let arguments = bounded_tool(
                        &state
                            .and_then(|state| state.get("input"))
                            .map(Value::to_string)
                            .unwrap_or_default(),
                    );
                    let outcome = bounded_tool(
                        &state
                            .and_then(|state| state.get("content"))
                            .map(Value::to_string)
                            .unwrap_or_default(),
                    );
                    snapshot.entries.push(Entry::Tool {
                        name,
                        arguments,
                        outcome,
                        error: state
                            .and_then(|state| state.get("status"))
                            .and_then(Value::as_str)
                            .is_some_and(|status| status == "error"),
                        changed_paths: Vec::new(),
                    });
                    snapshot.counts.tools += 1;
                }
            }
            "compaction" => {
                snapshot.counts.compactions += 1;
                count_omitted(&mut snapshot.omitted, "compaction_not_representable", 1);
            }
            "synthetic" => {
                count_omitted(&mut snapshot.omitted, "system_context", 1);
            }
            _ => {
                count_omitted(&mut snapshot.omitted, "unknown_content", 1);
            }
        }
    }
    if snapshot.title.is_empty() {
        snapshot.title = first_user.unwrap_or_default();
    }
    if snapshot.entries.is_empty() {
        snapshot.state = "incomplete".to_owned();
    }
    Ok(snapshot)
}

fn dsh_user_prompt(value: &Value) -> Option<String> {
    if value
        .get("data")
        .and_then(|data| data.get("source"))
        .and_then(|source| source.get("kind"))
        .and_then(Value::as_str)
        != Some("user")
    {
        return None;
    }
    value
        .get("data")
        .and_then(|data| data.get("content"))
        .map(text_from_content)
        .map(|(text, _)| text)
        .and_then(|text| title_from_prompt(&text))
}

fn read_dsh(binding: &ResolvedStore, native_id: &str) -> Result<Snapshot> {
    let sessions = dsh_session_files(&binding.path, MAX_LIST_FILES)?;
    let mut selected = None;
    for (dir, path, version) in sessions {
        if dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == native_id || name == format!("session-{native_id}"))
        {
            selected = Some((path, version));
            break;
        }
    }
    let (path, version) = match selected {
        Some(value) => value,
        None => {
            let mut fallback = None;
            for (_, path, version) in dsh_session_files(&binding.path, MAX_LIST_FILES)? {
                let bytes = zstd_decompress(&path)?;
                let text = std::str::from_utf8(&bytes).map_err(|_| {
                    Error::new("source_unreadable", "Native DSH session is not valid UTF-8")
                })?;
                if text.lines().any(|line| {
                    serde_json::from_str::<Value>(line)
                        .ok()
                        .is_some_and(|value| {
                            value.get("type").and_then(Value::as_str) == Some("session")
                                && value.get("id").and_then(Value::as_str) == Some(native_id)
                        })
                }) {
                    fallback = Some((path, version));
                    break;
                }
            }
            fallback.ok_or_else(|| {
                Error::new(
                    "unknown_resource",
                    "The selected native DSH session does not exist",
                )
            })?
        }
    };
    if version != 4 {
        return Err(Error::new(
            "unsupported_source",
            "Native DSH session has no qualified reader for its schema signature",
        ));
    }
    let bytes = zstd_decompress(&path)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error::new("source_unreadable", "Native DSH session is not valid UTF-8"))?;
    let mut snapshot = empty_snapshot(binding, native_id);
    snapshot.revision = hash(&bytes);
    let mut header = None;
    let mut title = None;
    let mut first_user = None;
    let mut updated = None;
    let mut incomplete = false;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => {
                incomplete = true;
                continue;
            }
        };
        if let Some(timestamp) = value.get("time").and_then(parse_timestamp) {
            updated = Some(timestamp);
        }
        match value.get("type").and_then(Value::as_str) {
            Some("session") => header = Some(value),
            Some("session/title") => {
                if let Some(value) = value
                    .get("data")
                    .and_then(|data| data.get("title"))
                    .and_then(Value::as_str)
                    .map(bounded_title)
                    .filter(|value| !value.is_empty())
                {
                    title = Some(value);
                }
            }
            Some("model/selection") => {
                let data = value.get("data");
                snapshot.model = match (
                    data.and_then(|data| data.get("provider"))
                        .and_then(Value::as_str),
                    data.and_then(|data| data.get("model"))
                        .and_then(Value::as_str),
                ) {
                    (Some(provider), Some(model)) => Some(format!("{provider}/{model}")),
                    (_, Some(model)) => Some(model.to_owned()),
                    _ => snapshot.model,
                };
            }
            Some("user/message") => {
                let data = value.get("data");
                if data
                    .and_then(|data| data.get("source"))
                    .and_then(|source| source.get("kind"))
                    .and_then(Value::as_str)
                    != Some("user")
                {
                    count_omitted(&mut snapshot.omitted, "system_context", 1);
                    continue;
                }
                let (text, omitted) = data
                    .and_then(|data| data.get("content"))
                    .map(text_from_content)
                    .unwrap_or_default();
                count_omitted(&mut snapshot.omitted, "non_text_content", omitted);
                if first_user.is_none() {
                    first_user = title_from_prompt(&text);
                }
                if !text.trim().is_empty() {
                    snapshot.entries.push(Entry::User { text });
                    snapshot.counts.turns += 1;
                    snapshot.counts.messages += 1;
                }
            }
            Some("assistant/message") => {
                let message = value.get("data").and_then(|data| data.get("message"));
                let (text, omitted) = message
                    .and_then(|message| message.get("content"))
                    .map(text_from_content)
                    .unwrap_or_default();
                count_omitted(&mut snapshot.omitted, "reasoning", omitted);
                if !text.trim().is_empty() {
                    snapshot.entries.push(Entry::Assistant { text });
                    snapshot.counts.messages += 1;
                }
            }
            Some("tool/call") => {
                let data = value.get("data");
                let name = data
                    .and_then(|data| data.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_owned();
                let arguments = bounded_tool(
                    &data
                        .and_then(|data| data.get("arguments"))
                        .map(Value::to_string)
                        .unwrap_or_default(),
                );
                snapshot.entries.push(Entry::Tool {
                    name,
                    arguments,
                    outcome: String::new(),
                    error: false,
                    changed_paths: Vec::new(),
                });
                snapshot.counts.tools += 1;
            }
            Some("tool/result") => {
                let (text, _) = value
                    .get("data")
                    .and_then(|data| data.get("message"))
                    .and_then(|message| message.get("content"))
                    .map(text_from_content)
                    .unwrap_or_default();
                snapshot.entries.push(Entry::Tool {
                    name: "tool".into(),
                    arguments: String::new(),
                    outcome: bounded_tool(&text),
                    error: false,
                    changed_paths: Vec::new(),
                });
                snapshot.counts.tools += 1;
            }
            Some("compaction" | "session/compaction") => {
                snapshot.counts.compactions += 1;
                count_omitted(&mut snapshot.omitted, "compaction_not_representable", 1);
            }
            _ => {}
        }
    }
    let Some(header) = header else {
        return Err(Error::new(
            "source_incomplete",
            "Native DSH session has no readable session header",
        ));
    };
    if header
        .get("id")
        .and_then(Value::as_str)
        .is_some_and(|header_id| header_id != native_id)
    {
        return Err(Error::new(
            "source_unreadable",
            "The native session header identity does not match the selected session",
        ));
    }
    snapshot.cwd = header.get("cwd").and_then(Value::as_str).map(str::to_owned);
    snapshot.created_at = header.get("createdAt").and_then(parse_timestamp);
    snapshot.updated_at = updated;
    snapshot.title = title.or(first_user).unwrap_or_default();
    if incomplete {
        snapshot.state = "incomplete".to_owned();
    }
    if snapshot.entries.is_empty() {
        snapshot.state = "incomplete".to_owned();
    }
    Ok(snapshot)
}
