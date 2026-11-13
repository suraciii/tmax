//! Session capability model and native-store resolution.
//!
//! Product rules live in `specs/sessions/spec.md`. Native schema knowledge stays
//! in `readers.rs` and `writers.rs`; this module only selects one resolved store
//! and carries decoded metadata.
use crate::{
    Error, Result,
    config::{AgentTarget, Config},
    storage::Store,
};
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(crate) const MAX_SESSION_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_METADATA_FILE_BYTES: u64 = 128 * 1024;
pub(crate) const MAX_NATIVE_ROWS: usize = 50_001;
pub(crate) const MAX_LIST_FILES: usize = 20_000;
pub(crate) const MAX_TITLE_CHARS: usize = 200;
pub(crate) const MAX_PROMPT_CHARS: usize = 160;
pub(crate) const MAX_TOOL_CHARS: usize = 4_096;
pub(crate) const DEFAULT_LIMIT: usize = 50;
pub(crate) const MAX_LIMIT: usize = 500;

#[derive(Clone, Debug)]
pub(crate) struct ResolvedStore {
    pub(crate) agent: String,
    pub(crate) identity: String,
    pub(crate) path: PathBuf,
    pub(crate) kind: StoreKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StoreKind {
    Codex,
    Pi,
    Omp,
    OpenCode,
    Hermes,
    Dsh,
}

#[derive(Clone, Debug)]
pub(crate) struct SessionRow {
    pub(crate) target: Option<String>,
    pub(crate) targets: Vec<String>,
    pub(crate) scope: Option<String>,
    pub(crate) agent: String,
    pub(crate) native_id: String,
    pub(crate) title: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) workspace_roots: Option<Vec<String>>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) archived: Option<bool>,
    pub(crate) hidden: Option<bool>,
    pub(crate) message_count: Option<u64>,
    pub(crate) tool_call_count: Option<u64>,
    pub(crate) state: String,
    pub(crate) store: String,
    pub(crate) imported_from: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CanonicalCounts {
    pub(crate) turns: u64,
    pub(crate) messages: u64,
    pub(crate) tools: u64,
    pub(crate) compactions: u64,
}

#[derive(Clone, Debug)]
pub(crate) enum Entry {
    User {
        text: String,
    },
    Assistant {
        text: String,
    },
    Tool {
        name: String,
        arguments: String,
        outcome: String,
        error: bool,
        changed_paths: Vec<String>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub(crate) revision: String,
    pub(crate) model: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) workspace_roots: Option<Vec<String>>,
    pub(crate) title: String,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) entries: Vec<Entry>,
    pub(crate) counts: CanonicalCounts,
    pub(crate) omitted: BTreeMap<String, u64>,
    pub(crate) state: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TargetScan {
    pub(crate) label: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) agent: String,
    pub(crate) path: PathBuf,
    pub(crate) store: Option<String>,
    pub(crate) state: String,
    pub(crate) reason_code: Option<String>,
    pub(crate) rows: Vec<SessionRow>,
    pub(crate) cwd_unknown: u64,
    pub(crate) timestamp_unknown: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ListKind {
    Scanned,
    Absent,
    Partial,
    Unsupported,
}

impl ListKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Scanned => "scanned",
            Self::Absent => "absent",
            Self::Partial => "partial",
            Self::Unsupported => "unsupported",
        }
    }
}

pub(crate) struct ListOutcome {
    pub(crate) rows: Vec<SessionRow>,
    pub(crate) kind: ListKind,
    pub(crate) reason_code: Option<String>,
}

pub(crate) fn config_target<'a>(config: &'a Config, id: &str) -> Result<&'a AgentTarget> {
    config
        .agents
        .get(id)
        .ok_or_else(|| Error::new("unknown_resource", "Agent target is not configured"))
}

pub(crate) fn scope_path(store: &Store, path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        store.root.join(path)
    };
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(Error::new(
            "invalid_native_scope",
            "Native scope must not contain parent traversal",
        ));
    }
    Ok(path)
}

pub(crate) fn resolve_store(store: &Store, agent: &str, scope: &Path) -> Result<ResolvedStore> {
    let scope = scope_path(store, scope)?;
    if inside_store(store, &scope) {
        return Err(Error::new(
            "invalid_native_scope",
            "Native session stores must remain outside tmax-owned storage",
        ));
    }
    let kind = match agent {
        "codex" => StoreKind::Codex,
        "pi" => StoreKind::Pi,
        "omp" => StoreKind::Omp,
        "opencode" => StoreKind::OpenCode,
        "hermes" => StoreKind::Hermes,
        "dsh" => StoreKind::Dsh,
        _ => {
            return Err(Error::new(
                "unsupported_agent",
                "Supported agents are codex, pi, omp, opencode, hermes, and dsh",
            ));
        }
    };
    let path = match kind {
        StoreKind::Codex => scope.join("sessions"),
        StoreKind::Pi => session_root(&scope, "settings.json")?,
        StoreKind::Omp => omp_session_root(&scope)?,
        StoreKind::OpenCode => opencode_db(store, &scope)?,
        StoreKind::Hermes => scope.join("state.db"),
        StoreKind::Dsh => scope.join("sessions"),
    };
    if inside_store(store, &path) {
        return Err(Error::new(
            "invalid_native_scope",
            "Native session stores must remain outside tmax-owned storage",
        ));
    }
    let identity = identity_path(&path);
    Ok(ResolvedStore {
        agent: agent.to_owned(),
        identity,
        path,
        kind,
    })
}

fn session_root(scope: &Path, settings: &str) -> Result<PathBuf> {
    let selected = std::env::var_os("PI_CODING_AGENT_SESSION_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let selected = match selected {
        Some(path) => Some(path),
        None => {
            let path = scope.join(settings);
            match fs::read_to_string(&path) {
                Ok(text) => {
                    let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
                        Error::new(
                            "session_store_unresolved",
                            "The native session settings are malformed, so the store cannot be resolved",
                        )
                    })?;
                    value
                        .get("sessionDir")
                        .and_then(serde_json::Value::as_str)
                        .map(PathBuf::from)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => {
                    return Err(Error::new(
                        "session_store_unresolved",
                        "The native session settings cannot be read",
                    ));
                }
            }
        }
    };
    match selected {
        Some(path) if path.is_absolute() => Ok(path),
        Some(_) => Err(Error::new(
            "session_store_unresolved",
            "A relative native session-root selector does not identify exactly one store",
        )),
        None => Ok(scope.join("sessions")),
    }
}

fn omp_session_root(scope: &Path) -> Result<PathBuf> {
    let selected = std::env::var_os("PI_CODING_AGENT_SESSION_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let selected = match selected {
        Some(path) => Some(path),
        None => {
            let path = if scope.join("config.yml").exists() {
                scope.join("config.yml")
            } else {
                scope.join("config.yaml")
            };
            match fs::read_to_string(&path) {
                Ok(text) => {
                    let value: serde_yaml::Value = serde_yaml::from_str(&text).map_err(|_| {
                        Error::new(
                            "session_store_unresolved",
                            "The native OMP settings are malformed, so the session store cannot be resolved",
                        )
                    })?;
                    value
                        .get("sessionDir")
                        .and_then(serde_yaml::Value::as_str)
                        .map(PathBuf::from)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => {
                    return Err(Error::new(
                        "session_store_unresolved",
                        "The native OMP settings cannot be read",
                    ));
                }
            }
        }
    };
    match selected {
        Some(path) if path.is_absolute() => Ok(path),
        Some(_) => Err(Error::new(
            "session_store_unresolved",
            "A relative native session-root selector does not identify exactly one store",
        )),
        None => Ok(scope.join("sessions")),
    }
}

fn opencode_db(store: &Store, scope: &Path) -> Result<PathBuf> {
    if scope.is_file()
        || (!scope.exists()
            && scope
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("db")))
    {
        return Ok(scope.to_owned());
    }
    let selected = std::env::var_os("OPENCODE_DB").filter(|value| !value.is_empty());
    if let Some(selected) = &selected {
        let selected = PathBuf::from(selected);
        if selected.is_absolute() {
            return Ok(selected);
        }
        return Err(Error::new(
            "session_store_unresolved",
            "The OpenCode database selector is not absolute",
        ));
    }
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| store.home.join(".local/share"));
    if !data_home.is_absolute() {
        return Err(Error::new(
            "session_store_unresolved",
            "The OpenCode data-home binding is not absolute",
        ));
    }
    match selected {
        Some(selected) => Ok(data_home.join(selected)),
        None => Ok(data_home.join("opencode/opencode.db")),
    }
}

fn identity_path(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_owned())
        .to_string_lossy()
        .into_owned()
}

fn inside_store(store: &Store, path: &Path) -> bool {
    path.starts_with(&store.root)
        || canonical_existing_prefix(path).starts_with(canonical_existing_prefix(&store.root))
}

fn canonical_existing_prefix(path: &Path) -> PathBuf {
    let mut current = path;
    let mut suffix = Vec::new();
    loop {
        if let Ok(mut resolved) = fs::canonicalize(current) {
            for component in suffix.iter().rev() {
                resolved.push(component);
            }
            return resolved;
        }
        let Some(name) = current.file_name() else {
            return path.to_owned();
        };
        suffix.push(name.to_owned());
        let Some(parent) = current.parent() else {
            return path.to_owned();
        };
        current = parent;
    }
}

pub(crate) fn agent_supported(agent: &str) -> bool {
    matches!(
        agent,
        "codex" | "pi" | "omp" | "opencode" | "hermes" | "dsh"
    )
}

pub(crate) fn native_id_from_path(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let stem = name
        .strip_suffix(".jsonl.zst")
        .or_else(|| name.strip_suffix(".jsonl.zstd"))
        .or_else(|| name.strip_suffix(".jsonl"))
        .unwrap_or(name);
    if stem.starts_with("rollout-") {
        let start = stem.len().saturating_sub(36);
        let candidate = &stem[start..];
        if candidate.len() == 36
            && candidate
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
        {
            return Some(candidate.to_owned());
        }
        return None;
    }
    stem.rsplit_once('_')
        .map(|(_, id)| id)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

pub(crate) fn bounded_title(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    value.chars().take(MAX_TITLE_CHARS).collect()
}

pub(crate) fn first_prompt(text: &str) -> String {
    let value: String = text
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect();
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    value.chars().take(MAX_PROMPT_CHARS).collect()
}

pub(crate) fn title_from_prompt(text: &str) -> Option<String> {
    let title = first_prompt(text);
    (!title.is_empty()).then_some(title)
}

pub(crate) fn bounded_tool(text: &str) -> String {
    text.chars().take(MAX_TOOL_CHARS).collect()
}

pub(crate) fn rfc3339_millis(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(crate) fn from_unix_seconds(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    let seconds = value.trunc() as i64;
    let nanos = ((value.fract() * 1_000_000_000.0).round() as u32).min(999_999_999);
    Utc.timestamp_opt(seconds, nanos)
        .single()
        .map(rfc3339_millis)
}

pub(crate) fn from_unix_millis(value: i64) -> Option<String> {
    Utc.timestamp_millis_opt(value).single().map(rfc3339_millis)
}

pub(crate) fn parse_timestamp(value: &serde_json::Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        return DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|value| rfc3339_millis(value.with_timezone(&Utc)));
    }
    if let Some(value) = value.as_i64() {
        return if value.abs() >= 1_000_000_000_000 {
            from_unix_millis(value)
        } else {
            Utc.timestamp_opt(value, 0).single().map(rfc3339_millis)
        };
    }
    value.as_f64().and_then(from_unix_seconds)
}

pub(crate) fn text_from_content(value: &serde_json::Value) -> (String, u64) {
    let Some(items) = value.as_array() else {
        if let Some(text) = value.as_str() {
            return (text.to_owned(), 0);
        }
        return (String::new(), 1);
    };
    let mut text = Vec::new();
    let mut omitted = 0;
    for item in items {
        match item.get("type").and_then(serde_json::Value::as_str) {
            Some("text" | "input_text" | "output_text") => {
                if let Some(value) = item.get("text").and_then(serde_json::Value::as_str) {
                    text.push(value.to_owned());
                }
            }
            Some("thinking" | "reasoning") => omitted += 1,
            Some("image" | "input_image" | "audio" | "input_audio" | "file") => omitted += 1,
            _ => {
                if let Some(value) = item.get("text").and_then(serde_json::Value::as_str) {
                    text.push(value.to_owned());
                } else {
                    omitted += 1;
                }
            }
        }
    }
    (text.join("\n"), omitted)
}

pub(crate) fn files_under(root: &Path, extension: &str, max: usize) -> Result<Vec<PathBuf>> {
    files_under_depth(root, extension, max, 2)
}

pub(crate) fn files_under_depth(
    root: &Path,
    extension: &str,
    max: usize,
    max_depth: usize,
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![(root.to_owned(), 0usize)];
    while let Some((path, depth)) = stack.pop() {
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                return Err(Error::new(
                    "native_store_unreadable",
                    "Native session store cannot be read",
                ));
            }
        };
        for entry in entries {
            let entry = entry.map_err(|_| {
                Error::new(
                    "native_store_unreadable",
                    "Native session store cannot be read",
                )
            })?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|_| {
                Error::new(
                    "native_store_unreadable",
                    "Native session store cannot be read",
                )
            })?;
            if file_type.is_dir() {
                if depth < max_depth {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if file_type.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(extension))
            {
                files.push(path);
                if files.len() >= max {
                    return Ok(files);
                }
            }
        }
    }
    files.sort();
    Ok(files)
}

pub(crate) fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(Error::new(
            "unsafe_native_file",
            "Native session must not be a symlink",
        ));
    }
    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            Error::new("unknown_resource", "Native session no longer exists")
        } else {
            Error::new("native_store_unreadable", "Native session cannot be read")
        }
    })?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err(Error::new(
            "source_oversized",
            "Native session exceeds the reader's bounded size limit",
        ));
    }
    fs::read(path)
        .map_err(|_| Error::new("native_store_unreadable", "Native session cannot be read"))
}

fn read_pipe(mut reader: impl std::io::Read, limit: Option<u64>) -> Vec<u8> {
    use std::io::Read;
    let mut bytes = Vec::new();
    match limit {
        Some(limit) => {
            let _ =
                std::io::Read::take(&mut reader, limit.saturating_add(1)).read_to_end(&mut bytes);
        }
        None => {
            let _ = reader.read_to_end(&mut bytes);
        }
    }
    bytes
}

pub(crate) fn run_output(command: Command, timeout: Duration) -> Result<Output> {
    run_output_with_limit(command, timeout, None, None)
}

fn run_output_with_limit(
    mut command: Command,
    timeout: Duration,
    stdout_limit: Option<u64>,
    stderr_limit: Option<u64>,
) -> Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|_| {
        Error::new(
            "native_reader_unavailable",
            "The required native read-only helper is unavailable",
        )
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        Error::new(
            "native_store_unreadable",
            "Native read-only helper stdout is unavailable",
        )
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        Error::new(
            "native_store_unreadable",
            "Native read-only helper stderr is unavailable",
        )
    })?;
    let stdout_reader = thread::spawn(move || read_pipe(stdout, stdout_limit));
    let stderr_reader = thread::spawn(move || read_pipe(stderr, stderr_limit));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(Error::new(
                    "native_store_unreadable",
                    "Native read-only helper timed out",
                ));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(Error::new(
                    "native_store_unreadable",
                    "Native read-only helper failed",
                ));
            }
        }
    };
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

pub(crate) fn sqlite_json(path: &Path, sql: &str) -> Result<serde_json::Value> {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(Error::new(
            "unsafe_native_file",
            "Native session database must not be a symlink",
        ));
    }
    let mut command = Command::new("sqlite3");
    command.arg("-readonly").arg("-json").arg(path).arg(sql);
    let output = run_output_with_limit(
        command,
        Duration::from_secs(5),
        Some(128 * 1024 * 1024),
        Some(1024 * 1024),
    )?;
    if !output.status.success() || output.stdout.len() > 128 * 1024 * 1024 {
        return Err(Error::new(
            "native_store_unreadable",
            "Native session database cannot be read without migration",
        ));
    }
    if output.stdout.is_empty() {
        return Ok(serde_json::Value::Array(Vec::new()));
    }
    serde_json::from_slice(&output.stdout).map_err(|_| {
        Error::new(
            "native_store_unreadable",
            "Native session database returned an unreadable schema",
        )
    })
}

pub(crate) fn sqlite_json_sets(path: &Path, sql: &str) -> Result<Vec<serde_json::Value>> {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(Error::new(
            "unsafe_native_file",
            "Native session database must not be a symlink",
        ));
    }
    let mut command = Command::new("sqlite3");
    command.arg("-readonly").arg("-json").arg(path).arg(sql);
    let output = run_output_with_limit(
        command,
        Duration::from_secs(5),
        Some(128 * 1024 * 1024),
        Some(1024 * 1024),
    )?;
    if !output.status.success() || output.stdout.len() > 128 * 1024 * 1024 {
        return Err(Error::new(
            "native_store_unreadable",
            "Native session database cannot be read without migration",
        ));
    }
    if output.stdout.is_empty() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    for value in serde_json::Deserializer::from_slice(&output.stdout).into_iter() {
        values.push(value.map_err(|_| {
            Error::new(
                "native_store_unreadable",
                "Native session database returned an unreadable schema",
            )
        })?);
    }
    Ok(values)
}

pub(crate) fn sqlite_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
pub(crate) fn sqlite_exec(path: &Path, sql: &str) -> Result<()> {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(Error::new(
            "unsafe_native_file",
            "Native session database must not be a symlink",
        ));
    }
    let mut command = Command::new("sqlite3");
    command.arg(path).arg(sql);
    let output = run_output(command, Duration::from_secs(10))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("locked") || stderr.contains("busy") {
        return Err(Error::new(
            "destination_locked",
            "The destination session database is locked by another writer",
        ));
    }
    Err(Error::new(
        "native_write_failed",
        "The destination session database transaction failed",
    ))
}

pub(crate) fn table_columns(path: &Path, table: &str) -> Result<Vec<String>> {
    let value = sqlite_json(path, &format!("PRAGMA table_info({table});"))?;
    Ok(value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row.get("name")?.as_str().map(str::to_owned))
        .collect())
}

pub(crate) fn table_exists(path: &Path, table: &str) -> Result<bool> {
    Ok(!table_columns(path, table)?.is_empty())
}

pub(crate) fn normalize_cwd(value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    if !path.is_absolute() {
        return None;
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::Normal(value) => normalized.push(value),
        }
    }
    Some(normalized)
}

pub(crate) fn cwd_matches(recorded: Option<&str>, wanted: &Path) -> bool {
    recorded
        .and_then(normalize_cwd)
        .is_some_and(|path| path == wanted)
}

pub(crate) fn zstd_decompress(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        Error::new(
            "native_store_unreadable",
            "Native compressed session cannot be read",
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(Error::new(
            "unsafe_native_file",
            "Native compressed session must be a regular file, not a symlink",
        ));
    }
    if metadata.len() > MAX_SESSION_BYTES {
        return Err(Error::new(
            "source_oversized",
            "Native compressed session exceeds the reader's bounded size limit",
        ));
    }
    let mut command = Command::new("zstd");
    command.arg("-dc").arg(path);
    let output = run_output_with_limit(
        command,
        Duration::from_secs(10),
        Some(MAX_SESSION_BYTES),
        Some(1024 * 1024),
    )?;
    if !output.status.success() {
        return Err(Error::new(
            "native_store_unreadable",
            "Native compressed session cannot be decompressed",
        ));
    }
    if output.stdout.len() as u64 > MAX_SESSION_BYTES {
        return Err(Error::new(
            "source_oversized",
            "Native compressed session exceeds the reader's bounded size limit",
        ));
    }
    Ok(output.stdout)
}

pub(crate) fn count_omitted(omitted: &mut BTreeMap<String, u64>, key: &str, count: u64) {
    if count != 0 {
        *omitted.entry(key.to_owned()).or_default() += count;
    }
}
