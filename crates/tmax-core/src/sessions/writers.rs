//! Version-qualified native session writers.
//!
//! A writer is enabled only for an exact qualified schema and registration path.
//! There is no generic JSONL or database fallback: an unknown destination schema
//! is rejected before any native effect.
use super::{
    model::{Entry, ResolvedStore, Snapshot, bounded_tool, rfc3339_millis},
    readers,
};
use crate::{Error, ResourceRef, Result};
use chrono::Utc;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use uuid::Uuid;

fn committed_unverified(message: &str, native_id: Option<&str>) -> Error {
    let mut error = Error::new("committed_unverified", message);
    if let Some(native_id) = native_id {
        error.affected = vec![ResourceRef {
            kind: "session".into(),
            id: Some(native_id.to_owned()),
            scope: None,
        }];
    }
    error
}

pub(crate) struct WriteRequest<'a> {
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) title: &'a str,
    pub(crate) cwd: &'a Path,
    pub(crate) scope: &'a Path,
    pub(crate) home: &'a Path,
}

pub(crate) struct Written {
    pub(crate) native_id: String,
    pub(crate) title: String,
    pub(crate) cwd: String,
    pub(crate) schema: String,
    pub(crate) resume: Option<Value>,
    pub(crate) cleanup_path: Option<PathBuf>,
}

pub(crate) fn writer_for(agent: &str) -> Option<&'static str> {
    match agent {
        "pi" => Some("pi.session.v3"),
        "omp" => Some("omp.session.v3"),
        "opencode" => Some("opencode.session_v2.import"),
        "codex" => Some("codex.thread-index.v1"),
        // Hermes and DSH remain read-only in the initial contract.
        _ => None,
    }
}

pub(crate) fn qualify(binding: &ResolvedStore) -> Result<()> {
    match binding.kind {
        super::model::StoreKind::Pi | super::model::StoreKind::Omp => qualify_pi_omp(binding),
        super::model::StoreKind::OpenCode => qualify_opencode(),
        super::model::StoreKind::Codex => codex_writer_index(binding).map(|_| ()),
        _ => Err(Error::new(
            "unsupported_destination",
            "No qualified native session writer is available for this destination",
        )),
    }
}

pub(crate) fn write(binding: &ResolvedStore, request: WriteRequest<'_>) -> Result<Written> {
    qualify(binding)?;
    match binding.kind {
        super::model::StoreKind::Pi | super::model::StoreKind::Omp => {
            write_pi_omp(binding, request)
        }
        super::model::StoreKind::OpenCode => write_opencode(binding, request),
        super::model::StoreKind::Codex => write_codex(binding, request),
        _ => Err(Error::new(
            "unsupported_destination",
            "No qualified native session writer is available for this destination",
        )),
    }
}

fn encode_pi_cwd(cwd: &Path) -> String {
    let text = cwd.to_string_lossy();
    let text = text.trim_start_matches(['/', '\\']);
    let text = text.replace(['/', '\\', ':'], "-");
    format!("--{text}--")
}

fn encode_omp_cwd(cwd: &Path, home: &Path) -> String {
    let cwd = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_owned());
    let home = fs::canonicalize(home).unwrap_or_else(|_| home.to_owned());
    let temp = fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
    if let Ok(relative) = cwd.strip_prefix(&home) {
        let text = relative.to_string_lossy();
        return format!("-{}", text.replace(['/', '\\', ':'], "-"));
    }
    if cwd.starts_with(&temp) {
        let text = cwd.to_string_lossy();
        let text = text.trim_start_matches(['/', '\\']);
        return format!("-{}", text.replace(['/', '\\', ':'], "-"));
    }
    encode_pi_cwd(&cwd)
}

fn same_cwd(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn random_short() -> String {
    Uuid::new_v4().simple().to_string()[..8].to_owned()
}

fn message_record(
    id: &str,
    parent: Option<&str>,
    role: &str,
    text: &str,
    timestamp: &str,
) -> Value {
    let mut message = json!({
        "role": role,
        "content": [{"type": "text", "text": text}],
        "timestamp": timestamp,
    });
    if role == "assistant" {
        message["stopReason"] = json!("stop");
    }
    json!({
        "type": "message",
        "id": id,
        "parentId": parent,
        "timestamp": timestamp,
        "message": message,
    })
}

pub(crate) fn tool_text(entry: &Entry) -> String {
    let Entry::Tool {
        name,
        arguments,
        outcome,
        error,
        changed_paths,
    } = entry
    else {
        return String::new();
    };
    let mut text = format!("[tool activity: {name}]");
    if !arguments.is_empty() {
        text.push_str("\narguments: ");
        text.push_str(arguments);
    }
    if !outcome.is_empty() {
        text.push_str("\nrecorded outcome: ");
        text.push_str(&bounded_tool(outcome));
    }
    if *error {
        text.push_str("\nerror: true");
    }
    if !changed_paths.is_empty() {
        text.push_str("\nchanged paths: ");
        text.push_str(&changed_paths.join(", "));
    }
    text
}

fn qualify_pi_omp(binding: &ResolvedStore) -> Result<()> {
    for path in readers::primary_pi_files(binding)? {
        let bytes = readers::read_metadata_bytes(&path, super::model::MAX_METADATA_FILE_BYTES)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            Error::new(
                "native_schema_unsupported",
                "The destination session store has no recognized JSONL schema",
            )
        })?;
        let header = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .find_map(|line| {
                let value = serde_json::from_str::<Value>(line).ok()?;
                (value.get("type").and_then(Value::as_str) == Some("session")).then_some(value)
            });
        let Some(header) = header else {
            return Err(Error::new(
                "native_schema_unsupported",
                "The destination session store has no recognized JSONL schema",
            ));
        };
        if header.get("version").and_then(Value::as_i64) != Some(3) {
            return Err(Error::new(
                "native_schema_unsupported",
                "The destination session store uses an unqualified JSONL schema version",
            ));
        }
    }
    Ok(())
}

fn qualify_opencode() -> Result<()> {
    let mut command = Command::new("opencode");
    command.arg("--version");
    let output = super::model::run_output(command, Duration::from_secs(5))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout.split_whitespace().last().unwrap_or_default();
    if !output.status.success() || !matches!(version, "2.0.28" | "v2.0.28") {
        return Err(Error::new(
            "unsupported_destination",
            "The installed OpenCode writer version is not qualified",
        ));
    }
    Ok(())
}

fn codex_writer_index(binding: &ResolvedStore) -> Result<(PathBuf, Vec<String>)> {
    let db = readers::codex_index(binding).ok_or_else(|| {
        Error::new(
            "native_schema_unsupported",
            "The destination Codex home has no qualified native thread index",
        )
    })?;
    let columns = super::model::table_columns(&db, "threads")?;
    let required = [
        "id",
        "rollout_path",
        "created_at",
        "updated_at",
        "source",
        "model_provider",
        "cwd",
        "title",
        "sandbox_policy",
        "approval_mode",
    ];
    if required
        .iter()
        .any(|name| !columns.iter().any(|column| column == name))
    {
        return Err(Error::new(
            "native_schema_unsupported",
            "The destination Codex thread index has an unrecognized schema signature",
        ));
    }
    Ok((db, columns))
}

fn write_pi_omp(binding: &ResolvedStore, request: WriteRequest<'_>) -> Result<Written> {
    let root = &binding.path;
    let encoded = match binding.kind {
        super::model::StoreKind::Omp => encode_omp_cwd(request.cwd, request.home),
        _ => encode_pi_cwd(request.cwd),
    };
    let bucket = root.join(encoded);
    fs::create_dir_all(&bucket).map_err(|_| {
        Error::new(
            "destination_store_unwritable",
            "The destination session bucket cannot be created",
        )
    })?;
    let native_id = Uuid::new_v4().to_string();
    let stamp = Utc::now();
    let file_stamp = stamp.format("%Y-%m-%dT%H-%M-%S-%3fZ").to_string();
    let final_path = bucket.join(format!("{file_stamp}_{native_id}.jsonl"));
    let temp_path = bucket.join(format!(".{native_id}.tmp"));
    let timestamp = rfc3339_millis(stamp);
    let mut lines = Vec::new();
    let mut header = json!({
        "type": "session",
        "version": 3,
        "id": native_id,
        "timestamp": timestamp,
        "cwd": request.cwd,
    });
    if binding.kind == super::model::StoreKind::Omp {
        header["title"] = json!(request.title);
        header["titleSource"] = json!("import");
        lines.push(
            json!({
                "type": "title",
                "v": 1,
                "title": request.title,
                "updatedAt": timestamp,
            })
            .to_string(),
        );
    } else {
        header["title"] = json!(request.title);
    }
    lines.push(header.to_string());
    let mut parent: Option<String> = None;
    for entry in &request.snapshot.entries {
        let id = random_short();
        let text = match entry {
            Entry::User { text } => text.clone(),
            Entry::Assistant { text } => text.clone(),
            Entry::Tool { .. } => tool_text(entry),
        };
        if text.is_empty() {
            continue;
        }
        let role = match entry {
            Entry::User { .. } => "user",
            Entry::Assistant { .. } | Entry::Tool { .. } => "assistant",
        };
        lines.push(message_record(&id, parent.as_deref(), role, &text, &timestamp).to_string());
        parent = Some(id);
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp_path).map_err(|_| {
        Error::new(
            "destination_store_unwritable",
            "The destination session file cannot be created",
        )
    })?;
    for line in &lines {
        if writeln!(file, "{line}").is_err() {
            let _ = fs::remove_file(&temp_path);
            return Err(Error::new(
                "native_write_failed",
                "The destination session file could not be written",
            ));
        }
    }
    file.sync_all().map_err(|_| {
        let _ = fs::remove_file(&temp_path);
        Error::new(
            "native_write_failed",
            "The destination session file could not be synchronized",
        )
    })?;
    drop(file);
    if let Err(error) = fs::rename(&temp_path, &final_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(Error::new(
            "native_write_failed",
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "The destination session identity already exists"
            } else {
                "The destination session file could not be published"
            },
        ));
    }
    #[cfg(unix)]
    if let Some(parent) = final_path.parent() {
        let _ = fs::File::open(parent).and_then(|file| file.sync_all());
    }
    Ok(Written {
        native_id,
        title: request.title.to_owned(),
        cwd: request.cwd.to_string_lossy().into_owned(),
        schema: if binding.kind == super::model::StoreKind::Pi {
            "pi.session.v3".into()
        } else {
            "omp.session.v3".into()
        },
        resume: Some(json!({
            "program": if binding.kind == super::model::StoreKind::Pi { "pi" } else { "omp" },
            "arguments": ["--session", final_path.to_string_lossy()],
        })),
        cleanup_path: Some(final_path),
    })
}

fn write_opencode(binding: &ResolvedStore, request: WriteRequest<'_>) -> Result<Written> {
    let (provider, model) = request
        .snapshot
        .model
        .as_deref()
        .and_then(|value| value.split_once('/'))
        .filter(|(provider, model)| !provider.is_empty() && !model.is_empty())
        .ok_or_else(|| {
            Error::new(
                "model_unknown",
                "The destination requires a recorded provider/model identity",
            )
        })?;
    let id = format!("ses_{}", Uuid::new_v4().simple());
    let project = format!("proj_{}", Uuid::new_v4().simple());
    let created = Utc::now().timestamp_millis();
    let updated = created;
    let model_value = json!({"id": model, "providerID": provider, "variant": "default"});
    let mut messages = Vec::new();
    for entry in &request.snapshot.entries {
        let message_id = format!("msg_{}", Uuid::new_v4().simple());
        match entry {
            Entry::User { text } => messages.push(json!({
                "id": message_id,
                "time": {"created": created},
                "text": text,
                "files": [],
                "agents": [],
                "type": "user",
            })),
            Entry::Assistant { text } => messages.push(json!({
                "id": message_id,
                "time": {"created": created, "completed": created},
                "type": "assistant",
                "agent": "build",
                "model": model_value,
                "content": [{"type": "text", "text": text, "time": {"created": created, "completed": created}}],
                "finish": "stop",
                "cost": 0,
                "tokens": {"input": 0, "output": 0, "reasoning": 0, "cache": {"read": 0, "write": 0}},
                "snapshot": {"start": "", "end": "", "files": []},
            })),
            Entry::Tool { .. } => messages.push(json!({
                "id": message_id,
                "time": {"created": created, "completed": created},
                "type": "assistant",
                "agent": "build",
                "model": model_value,
                "content": [{"type": "text", "text": tool_text(entry), "time": {"created": created, "completed": created}}],
                "finish": "stop",
                "cost": 0,
                "tokens": {"input": 0, "output": 0, "reasoning": 0, "cache": {"read": 0, "write": 0}},
                "snapshot": {"start": "", "end": "", "files": []},
            })),
        }
    }
    let export = json!({
        "info": {
            "id": id,
            "projectID": project,
            "agent": "build",
            "model": model_value,
            "cost": 0,
            "tokens": {"input": 0, "output": 0, "reasoning": 0, "cache": {"read": 0, "write": 0}},
            "time": {"created": created, "updated": updated},
            "title": request.title,
            "location": {"directory": request.cwd},
        },
        "messages": messages,
    });
    let temp = std::env::temp_dir().join(format!(".tmax-session-{}.json", Uuid::new_v4().simple()));
    write_private(
        &temp,
        &serde_json::to_vec_pretty(&export).map_err(|_| {
            Error::new(
                "native_write_failed",
                "The destination session document could not be encoded",
            )
        })?,
    )?;
    let mut command = Command::new("opencode");
    command
        .arg("session")
        .arg("import")
        .arg(&temp)
        .arg("--standalone")
        .arg("--directory")
        .arg(request.cwd);
    command.env("OPENCODE_DB", &binding.path);
    if binding.path.file_name().and_then(|name| name.to_str()) == Some("opencode.db")
        && let Some(data_home) = binding.path.parent().and_then(Path::parent)
    {
        command.env("XDG_DATA_HOME", data_home);
    }
    let database_scope = request.scope.is_file()
        || (!request.scope.exists()
            && request
                .scope
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("db")));
    if !database_scope && let Some(config_home) = request.scope.parent() {
        command.env("XDG_CONFIG_HOME", config_home);
    }
    let output = super::model::run_output(command, Duration::from_secs(30));
    let _ = fs::remove_file(&temp);
    let output = output?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("locked") || stderr.contains("busy") {
            return Err(Error::new(
                "destination_locked",
                "The OpenCode session database is locked by another writer",
            ));
        }
        return Err(Error::new(
            "native_write_failed",
            "The official OpenCode session import failed",
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let imported = stdout
        .lines()
        .find_map(|line| line.strip_prefix("Imported session: "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let Some(imported) = imported else {
        return Err(committed_unverified(
            "OpenCode import returned without a destination session identity",
            None,
        ));
    };
    let readback = readers::read_source(binding, &imported).map_err(|_| {
        committed_unverified(
            "The imported OpenCode session is not visible through the destination reader",
            Some(&imported),
        )
    })?;
    let effective_cwd = readback
        .cwd
        .clone()
        .unwrap_or_else(|| request.cwd.to_string_lossy().into_owned());
    if !same_cwd(Path::new(&effective_cwd), request.cwd) || readback.title != request.title {
        return Err(committed_unverified(
            "The imported OpenCode session did not retain its planned cwd and title",
            Some(&imported),
        ));
    }
    Ok(Written {
        native_id: imported.clone(),
        title: request.title.to_owned(),
        cwd: effective_cwd,
        schema: "opencode.session_v2.import".into(),
        resume: Some(json!({"program":"opencode","arguments":["--session", imported]})),
        cleanup_path: None,
    })
}

fn write_private(path: &PathBuf, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| {
        Error::new(
            "destination_store_unwritable",
            "The pending session document cannot be created",
        )
    })?;
    if file.write_all(bytes).is_err() || file.sync_all().is_err() {
        let _ = fs::remove_file(path);
        return Err(Error::new(
            "native_write_failed",
            "The pending session document could not be written",
        ));
    }
    Ok(())
}

fn write_codex(binding: &ResolvedStore, request: WriteRequest<'_>) -> Result<Written> {
    let (db, columns) = codex_writer_index(binding)?;
    let native_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let stamp = now.format("%Y-%m-%dT%H-%M-%S").to_string();
    let day = now.format("%Y/%m/%d").to_string();
    let directory = binding.path.join(day);
    fs::create_dir_all(&directory).map_err(|_| {
        Error::new(
            "destination_store_unwritable",
            "The destination Codex session directory cannot be created",
        )
    })?;
    let final_path = directory.join(format!("rollout-{stamp}-{native_id}.jsonl"));
    let temp_path = directory.join(format!(".{native_id}.tmp"));
    let timestamp = rfc3339_millis(now);
    let provider = request
        .snapshot
        .model
        .as_deref()
        .and_then(|value| value.split_once('/'))
        .map(|(provider, _)| provider)
        .unwrap_or("openai");
    let cwd = request.cwd.to_string_lossy();
    let mut lines = Vec::new();
    let mut ordinal = 0u64;
    lines.push(
        json!({
            "type": "session_meta",
            "timestamp": timestamp,
            "ordinal": ordinal,
            "payload": {
                "id": native_id,
                "session_id": native_id,
                "timestamp": timestamp,
                "cwd": cwd,
                "runtime_workspace_roots": [cwd],
                "source": "exec",
                "thread_source": "user",
                "model_provider": provider,
                "cli_version": "tmax-import",
            }
        })
        .to_string(),
    );
    ordinal += 1;
    for entry in &request.snapshot.entries {
        let (role, kind, text) = match entry {
            Entry::User { text } => ("user", "input_text", text.clone()),
            Entry::Assistant { text } => ("assistant", "output_text", text.clone()),
            Entry::Tool { .. } => ("assistant", "output_text", tool_text(entry)),
        };
        if text.is_empty() {
            continue;
        }
        lines.push(
            json!({
                "type": "response_item",
                "timestamp": timestamp,
                "ordinal": ordinal,
                "payload": {
                    "type": "message",
                    "role": role,
                    "content": [{"type": kind, "text": text}],
                }
            })
            .to_string(),
        );
        ordinal += 1;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp_path).map_err(|_| {
        Error::new(
            "destination_store_unwritable",
            "The destination Codex session file cannot be created",
        )
    })?;
    for line in &lines {
        if writeln!(file, "{line}").is_err() {
            let _ = fs::remove_file(&temp_path);
            return Err(Error::new(
                "native_write_failed",
                "The destination Codex session file could not be written",
            ));
        }
    }
    if file.sync_all().is_err() {
        let _ = fs::remove_file(&temp_path);
        return Err(Error::new(
            "native_write_failed",
            "The destination Codex session file could not be synchronized",
        ));
    }
    drop(file);
    fs::rename(&temp_path, &final_path).map_err(|_| {
        let _ = fs::remove_file(&temp_path);
        Error::new(
            "native_write_failed",
            "The destination Codex session file could not be published",
        )
    })?;
    let created = now.timestamp();
    let title = request.title;
    let threadsource = columns.iter().any(|column| column == "thread_source");
    let sql = format!(
        "BEGIN IMMEDIATE; INSERT INTO threads (id, rollout_path, created_at, updated_at, source, model_provider, cwd, title, sandbox_policy, approval_mode{}) VALUES ({}, {}, {}, {}, 'exec', {}, {}, {}, '{{\"type\":\"disabled\"}}', 'never'{}); COMMIT;",
        if threadsource { ", thread_source" } else { "" },
        super::model::sqlite_quote(&native_id),
        super::model::sqlite_quote(&final_path.to_string_lossy()),
        created,
        created,
        super::model::sqlite_quote(provider),
        super::model::sqlite_quote(&cwd),
        super::model::sqlite_quote(title),
        if threadsource { ", 'user'" } else { "" },
    );
    if let Err(error) = super::model::sqlite_exec(&db, &sql) {
        let _ = fs::remove_file(&final_path);
        return Err(error);
    }
    Ok(Written {
        native_id: native_id.clone(),
        title: title.to_owned(),
        cwd: cwd.into_owned(),
        schema: "codex.thread-index.v1".into(),
        resume: Some(json!({"program":"codex","arguments":["resume", native_id]})),
        cleanup_path: None,
    })
}
