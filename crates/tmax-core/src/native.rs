//! Native application is deliberately separate from desired-state persistence.
//!
//! Each adapter declares a concrete native schema, credential owner, precedence,
//! and exact researched version. Config files are edited at owned paths, never
//! regenerated from imported native state. Exact originals and written bytes are
//! stored in current-user-only files before the first external write. A write-ahead
//! record survives interruption, and three-way restoration preserves unowned later edits.
//! Native auth inspection projects root namespace presence only, skipping every
//! credential value without materializing, retaining, or exporting it.
mod adapters;
pub(crate) mod browser_login;
pub(crate) mod compatibility;
mod document;
pub use adapters::supported_roles;

use crate::{
    Error, Request, Result,
    config::{self, AgentTarget, Auth, Config},
    secrets,
    storage::{self, Store},
};
use adapters::{Entry, File};
use document::Format;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use zeroize::Zeroize;

pub(crate) fn model_metadata(agent: &str, effective: &Value) -> Result<Value> {
    adapters::model_metadata(agent, effective)
}

const INDEX: &str = "native/recovery.json";
const LOGINS: &str = "native/logins.json";
const MAX_NATIVE_BYTES: u64 = 8 * 1024 * 1024;
#[derive(Default, Serialize, Deserialize)]
struct Index {
    records: Vec<Record>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Record {
    id: String,
    owner: String,
    target: String,
    agent: String,
    scope: PathBuf,
    payload_ref: String,
    secret_refs: Vec<String>,
    state: String,
    files: Vec<FileRecord>,
}
#[derive(Clone, Serialize, Deserialize)]
struct FileRecord {
    path: PathBuf,
    format: Format,
    original_absent: bool,
    after_revision: String,
    fields: Vec<Field>,
    written: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Field {
    path: Vec<String>,
    after_hash: String,
    before_hash: String,
    observed_hash: String,
    released: bool,
}
#[derive(Serialize, Deserialize)]
struct Payload {
    files: Vec<Capture>,
}
#[derive(Serialize, Deserialize)]
struct Capture {
    path: PathBuf,
    before: Option<String>,
    after: String,
    exact_original_safe: bool,
    baseline_values: Vec<(Vec<String>, Option<Value>)>,
}
#[derive(Default, Serialize, Deserialize)]
struct Logins {
    entries: Vec<LoginRecord>,
}
#[derive(Serialize, Deserialize)]
struct LoginRecord {
    provider: String,
    target: String,
    scope: PathBuf,
    agent: String,
    native_provider: String,
    credential_id: Option<String>,
    session_ref: String,
    state: String,
    source_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account: Option<browser_login::Account>,
}
struct Inputs {
    file: File,
    before: Option<String>,
    observed: Option<String>,
    after: String,
    fields: Vec<Field>,
    exact_original_safe: bool,
    baseline_values: Vec<(Vec<String>, Option<Value>)>,
}

fn baseline<'a>(
    capture: Option<&'a Capture>,
    original: &'a Value,
    path: &[String],
) -> Option<&'a Value> {
    match capture.and_then(|c| c.baseline_values.iter().find(|(p, _)| p == path)) {
        Some((_, value)) => value.as_ref(),
        None => document::get(original, path),
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn field_hash(value: Option<&Value>) -> String {
    value
        .map(|v| digest(v.to_string().as_bytes()))
        .unwrap_or_else(|| "absent".into())
}
fn revision(text: Option<&str>) -> String {
    text.map(|s| digest(s.as_bytes()))
        .unwrap_or_else(|| "absent".into())
}
fn unavailable() -> Error {
    Error::new(
        "native_scope_unavailable",
        "Native scope or configuration is unavailable; inspect the exact selected native path",
    )
}
fn scope_evidence(store: &Store, path: &Path) -> Result<(PathBuf, &'static str)> {
    let path = if path.is_absolute() {
        path.to_owned()
    } else {
        store.root.join(path)
    };
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(Error::new(
            "invalid_native_scope",
            "Native scope must not contain parent traversal",
        ));
    }
    let canonical = match fs::canonicalize(&path) {
        Ok(path) => path,
        Err(error) => {
            return Ok((
                path,
                match error.kind() {
                    std::io::ErrorKind::NotFound => "absent",
                    std::io::ErrorKind::PermissionDenied => "unreadable",
                    _ => "unavailable",
                },
            ));
        }
    };
    if !canonical.is_dir() {
        return Ok((canonical, "not_directory"));
    }
    // Windows canonicalization adds a verbatim prefix. Compare both directories
    // in the same filesystem form, including aliases of the storage root.
    let root = match fs::canonicalize(&store.root) {
        Ok(root) => root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => store.root.clone(),
        Err(_) => return Err(unavailable()),
    };
    if canonical.starts_with(root) {
        return Err(Error::new(
            "invalid_native_scope",
            "Native scopes must remain outside tmax-owned storage",
        ));
    }
    Ok((canonical, "available"))
}
fn scope(store: &Store, path: &Path) -> Result<PathBuf> {
    let (path, state) = scope_evidence(store, path)?;
    if state != "available" {
        return Err(Error::new(
            "native_scope_unavailable",
            &format!(
                "Native scope {} is {state}; select an existing native configuration directory or create the intended directory explicitly before applying",
                path.display()
            ),
        ));
    }
    Ok(path)
}
fn read_native(path: &Path) -> Result<Option<String>> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(unavailable()),
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_NATIVE_BYTES {
        return Err(Error::new(
            "unsafe_native_file",
            "Native configuration must be a bounded regular file, not a symlink",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() > 1 {
            return Err(Error::new(
                "unsafe_native_file",
                "Hard-linked native configuration cannot be independently managed",
            ));
        }
    }
    let bytes = fs::read(path).map_err(|_| unavailable())?;
    String::from_utf8(bytes).map(Some).map_err(|_| {
        Error::new(
            "native_config_invalid",
            "Native configuration must be UTF-8; no content was imported",
        )
    })
}
fn target<'a>(config: &'a Config, id: &str) -> Result<&'a AgentTarget> {
    config
        .agents
        .get(id)
        .ok_or_else(|| Error::new("unknown_resource", "Agent target is not configured"))
}
struct NativeExecutable {
    program: PathBuf,
    script: Option<PathBuf>,
}
impl From<PathBuf> for NativeExecutable {
    fn from(program: PathBuf) -> Self {
        Self {
            program,
            script: None,
        }
    }
}
#[cfg(windows)]
fn npm_pi(directory: &Path, paths: &[PathBuf]) -> Option<NativeExecutable> {
    if !directory.join("pi.cmd").is_file() {
        return None;
    }
    let node = std::iter::once(directory.to_owned())
        .chain(paths.iter().cloned())
        .map(|directory| directory.join("node.exe"))
        .find(|path| path.is_file())?;
    for package in [
        "@earendil-works/pi-coding-agent",
        "@mariozechner/pi-coding-agent",
    ] {
        let root = directory.join("node_modules").join(package);
        let manifest = read_native(&root.join("package.json"))
            .ok()
            .flatten()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        let Some(manifest) = manifest else { continue };
        if manifest["name"] != package {
            continue;
        }
        let Some(bin) = manifest["bin"]["pi"].as_str() else {
            continue;
        };
        // Read npm metadata; never execute or interpolate the batch/PowerShell shim.
        let bin = Path::new(bin);
        if !bin
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        {
            continue;
        }
        let script = root.join(bin);
        if script.is_file() {
            return Some(NativeExecutable {
                program: node,
                script: Some(script),
            });
        }
    }
    None
}
fn executable(agent: &str) -> Option<NativeExecutable> {
    let paths = std::env::var_os("PATH")?;
    let paths: Vec<_> = std::env::split_paths(&paths).collect();
    for directory in &paths {
        #[cfg(not(windows))]
        if directory.join(agent).is_file() {
            return Some(directory.join(agent).into());
        }
        #[cfg(windows)]
        {
            let binary = directory.join(format!("{agent}.exe"));
            if binary.is_file() {
                return Some(binary.into());
            }
            if agent == "pi"
                && let Some(executable) = npm_pi(directory, &paths)
            {
                return Some(executable);
            }
            // npm's official package includes a native binary. Never execute its
            // POSIX shim as a Windows executable or interpolate a shell command.
            if agent == "codex" && directory.join("codex.cmd").is_file() {
                let (package, triple) = if cfg!(target_arch = "aarch64") {
                    ("codex-win32-arm64", "aarch64-pc-windows-msvc")
                } else {
                    ("codex-win32-x64", "x86_64-pc-windows-msvc")
                };
                for prefix in [
                    directory.join("node_modules/@openai"),
                    directory.join("node_modules/@openai/codex/node_modules/@openai"),
                ] {
                    for suffix in ["bin/codex.exe", "codex/codex.exe"] {
                        let binary = prefix
                            .join(package)
                            .join("vendor")
                            .join(triple)
                            .join(suffix);
                        if binary.is_file() {
                            return Some(binary.into());
                        }
                    }
                }
            }
        }
    }
    None
}
fn native_command(agent: &str, scope: &Path) -> Result<tokio::process::Command> {
    let executable = executable(agent).ok_or_else(|| {
        Error::new(
            "native_not_installed",
            "The selected native executable is not on PATH",
        )
    })?;
    let mut cmd = tokio::process::Command::new(executable.program);
    if let Some(script) = executable.script {
        cmd.arg(script);
    }
    cmd.current_dir(scope).kill_on_drop(true);
    match agent {
        "codex" => {
            cmd.env("CODEX_HOME", scope);
        }
        "pi" => {
            cmd.env("PI_CODING_AGENT_DIR", scope);
        }
        "omp" => {
            cmd.env("PI_CODING_AGENT_DIR", scope)
                .env_remove("OMP_PROFILE")
                .env_remove("PI_PROFILE");
        }
        "opencode" => {
            cmd.env("OPENCODE_CONFIG_DIR", scope)
                .env("OPENCODE_DB", scope.join("opencode.db"))
                .env("XDG_DATA_HOME", scope.join("native-data"))
                .env("XDG_CONFIG_HOME", scope.join("native-config"))
                .env("XDG_CACHE_HOME", scope.join("native-cache"))
                .env("XDG_STATE_HOME", scope.join("native-state"));
        }
        "hermes" => {
            cmd.env("HERMES_HOME", scope);
        }
        _ => return Err(Error::new("unsupported_agent", "Unknown native adapter")),
    }
    Ok(cmd)
}
fn detected_version(agent: &str, text: &str) -> Option<String> {
    text.split_whitespace().find_map(|word| {
        let word = if agent == "omp" {
            word.strip_prefix("omp/").unwrap_or(word)
        } else {
            word
        };
        let word = word.trim_start_matches('v');
        (word.starts_with(|c: char| c.is_ascii_digit())
            && word
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".+-_".contains(c)))
        .then(|| word.to_owned())
    })
}
fn hermes_provenance_runtime(executable: &Path) -> Option<(PathBuf, PathBuf)> {
    // Resolve only the concrete researched shell-launcher forms as inert code
    // metadata. Neither the repair-capable wrapper nor its bootstrap runs.
    let mut launcher = fs::canonicalize(executable).ok()?;
    for _ in 0..4 {
        let text = read_native(&launcher).ok()??;
        let exec = text.lines().find_map(|line| line.strip_prefix("exec "))?;
        if let Some(next) = exec.strip_suffix(" \"$@\"") {
            let next = next.trim_matches('"');
            if !Path::new(next).is_absolute() || next.chars().any(char::is_whitespace) {
                return None;
            }
            launcher = fs::canonicalize(next).ok()?;
            continue;
        }
        let (python, _) = exec.split_once(" -I -c '")?;
        if !Path::new(python).is_absolute() || python.chars().any(char::is_whitespace) {
            return None;
        }
        let decoded = text.replace("'\"'\"'", "'");
        let root = decoded.lines().find_map(|line| {
            line.strip_prefix("sys.path.insert(0, '")
                .and_then(|rest| rest.strip_suffix("')"))
        })?;
        if !Path::new(root).is_absolute() || !Path::new(root).is_dir() {
            return None;
        }
        return Some((PathBuf::from(python), PathBuf::from(root)));
    }
    None
}
async fn hermes_version(path: &Path) -> Option<Vec<u8>> {
    // --version performs network update checks; --print-runtime-command can
    // repair interrupted source updates. Read launcher metadata, then invoke
    // only the native stdlib provenance API, with no bootstrap or bytecode write.
    let (program, root) = hermes_provenance_runtime(&executable("hermes")?.program)?;
    let root = serde_json::to_string(root.to_str()?).ok()?;
    let code = format!(
        "import sys;sys.dont_write_bytecode=True;sys.path.insert(0,{root});from hermes_cli.version_info import get_version_info; print(get_version_info().derived_version)"
    );
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(["-I", "-B", "-c", &code])
        .env("HERMES_HOME", path)
        .env("HERMES_DISABLE_LAZY_INSTALLS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .current_dir(path)
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let output = tokio::time::timeout(Duration::from_secs(10), cmd.output())
        .await
        .ok()?
        .ok()?;
    (output.status.success() && output.stdout.len() < 4096).then_some(output.stdout)
}
fn auth_namespace_from_reader(reader: impl std::io::Read, selected: &str) -> Result<bool> {
    struct Namespace<'a>(&'a str);
    impl<'de> serde::de::Visitor<'de> for Namespace<'_> {
        type Value = bool;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a native credential namespace mapping")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<bool, M::Error> {
            let mut present = false;
            while let Some(name) = map.next_key::<String>()? {
                present |= name == self.0;
                map.next_value::<serde::de::IgnoredAny>()?;
            }
            Ok(present)
        }
    }
    let mut deserializer = serde_json::Deserializer::from_reader(reader);
    let found=serde::de::Deserializer::deserialize_map(&mut deserializer,Namespace(selected)).map_err(|_|Error::new("native_auth_metadata_unavailable","Native credential namespace metadata is malformed; credential values were not materialized"))?;
    deserializer.end().map_err(|_| {
        Error::new(
            "native_auth_metadata_unavailable",
            "Native credential namespace metadata contains trailing data",
        )
    })?;
    Ok(found)
}
fn auth_namespace_present(path: &Path, selected: &str) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(unavailable()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_NATIVE_BYTES
    {
        return Err(Error::new(
            "native_auth_metadata_unavailable",
            "Native credential namespace metadata is not a bounded regular file",
        ));
    }
    let file = fs::File::open(path).map_err(|_| unavailable())?;
    auth_namespace_from_reader(std::io::Read::take(file, MAX_NATIVE_BYTES + 1), selected)
}
async fn version(agent: &str, path: &Path) -> Value {
    let Some(exe) = executable(agent) else {
        return json!({"state":"not_installed","version":null,"compatibility":"unknown","qualified_version":adapters::qualified_version(agent)});
    };
    let output = if agent == "hermes" {
        hermes_version(path).await
    } else {
        match native_command(agent, path) {
            Ok(mut cmd) => {
                cmd.arg("--version")
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());
                match tokio::time::timeout(Duration::from_secs(10), cmd.output()).await {
                    Ok(Ok(output)) if output.status.success() && output.stdout.len() < 4096 => {
                        Some(output.stdout)
                    }
                    _ => None,
                }
            }
            Err(_) => None,
        }
    };
    let found = output
        .as_deref()
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|text| detected_version(agent, text));
    let exact = found.as_deref() == Some(adapters::qualified_version(agent));
    json!({"executable":exe.script.as_ref().unwrap_or(&exe.program),"version":found,"state":if found.is_some(){"detected"}else if output.is_none(){"probe_failed"}else{"version_unknown"},"compatibility":if exact{"documented"}else{"unknown"},"qualified_version":adapters::qualified_version(agent),"version_range":null,"source":adapters::source(agent),"runtime_inference":"not_verified"})
}
fn version_blocker(agent: &str, installation: &Value) -> Option<Value> {
    if installation["state"] == "not_installed" {
        return Some(
            json!({"code":"native_not_installed","message":"Install the tool or correct PATH before applying"}),
        );
    }
    if agent == "opencode"
        && installation["version"]
            .as_str()
            .is_some_and(|version| version.starts_with("1."))
    {
        return Some(
            json!({"code":"native_schema_incompatible","message":"OpenCode 1 uses a different configuration format. Upgrade to OpenCode 2 before applying"}),
        );
    }
    None
}
fn version_warnings(installation: &Value) -> Vec<Value> {
    if installation["compatibility"] == "documented" || installation["state"] == "not_installed" {
        return Vec::new();
    }
    vec![
        json!({"code":"native_version_unverified","message":"This tool version has not been verified. Configuration can still be applied; reload the tool to check it"}),
    ]
}
fn omp_root(store: &Store) -> PathBuf {
    std::env::var_os("PI_CONFIG_DIR")
        .map(PathBuf::from)
        .filter(|p| p.components().all(|c| matches!(c, Component::Normal(_))))
        .map(|p| store.home.join(p))
        .unwrap_or_else(|| store.home.join(".omp"))
}
fn omp_scope(
    root: &Path,
    primary: Option<&std::ffi::OsStr>,
    fallback: Option<&std::ffi::OsStr>,
    agent_dir: Option<&std::ffi::OsStr>,
) -> PathBuf {
    match primary
        .or(fallback)
        .filter(|p| !p.is_empty() && *p != "default")
    {
        Some(profile) => root.join("profiles").join(profile).join("agent"),
        None => agent_dir
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("agent")),
    }
}
// OMP loads the canonical spelling first, but updates an existing .yaml in place.
// Application/recovery capture this resolved path rather than introducing a shadow.
fn omp_main_settings(path: &Path) -> &'static str {
    if path.join("config.yml").exists() || !path.join("config.yaml").exists() {
        "config.yml"
    } else {
        "config.yaml"
    }
}
fn default_scope(store: &Store, agent: &str) -> PathBuf {
    let env = match agent {
        "codex" => "CODEX_HOME",
        "pi" => "PI_CODING_AGENT_DIR",
        "hermes" => "HERMES_HOME",
        "opencode" => "OPENCODE_CONFIG_DIR",
        _ => "",
    };
    if let Some(path) = std::env::var_os(env).filter(|v| !v.is_empty()) {
        return PathBuf::from(path);
    }
    match agent {
        "codex" => store.home.join(".codex"),
        "pi" => store.home.join(".pi/agent"),
        "omp" => omp_scope(
            &omp_root(store),
            std::env::var_os("OMP_PROFILE").as_deref(),
            std::env::var_os("PI_PROFILE").as_deref(),
            std::env::var_os("PI_CODING_AGENT_DIR").as_deref(),
        ),
        "opencode" => std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| store.home.join(".config"))
            .join("opencode"),
        "hermes" => store.home.join(".hermes"),
        _ => PathBuf::new(),
    }
}
fn configuration_files(agent: &str, path: &Path) -> Vec<PathBuf> {
    let names: &[&str] = match agent {
        "codex" => &["config.toml"],
        "pi" => &["models.json", "settings.json"],
        "omp" => &[
            "models.yml",
            "models.yaml",
            "models.json",
            "config.yml",
            "config.yaml",
        ],
        "opencode" => &["opencode.jsonc", "opencode.json"],
        "hermes" => &["config.yaml"],
        _ => &[],
    };
    names.iter().map(|n| path.join(n)).collect()
}
fn auth_paths(agent: &str, path: &Path) -> Vec<PathBuf> {
    match agent {
        "codex" | "pi" | "hermes" => vec![path.join("auth.json")],
        "omp" => vec![path.join("auth.json"), path.join("agent.db")],
        "opencode" => vec![path.join("opencode.db")],
        _ => vec![],
    }
}
fn safe_files(agent: &str, path: &Path) -> Vec<Value> {
    configuration_files(agent, path).into_iter().map(|p| {
        let mut file = match read_native(&p) {
            Ok(text) => json!({"path":p,"present":text.is_some(),"revision":revision(text.as_deref()),"content":"not_imported"}),
            Err(e) => json!({"path":p,"state":"unavailable","reason_code":e.code}),
        };
        if agent == "omp" && matches!(p.file_name().and_then(|n| n.to_str()), Some("config.yml" | "config.yaml")) {
            let effective = path.join(omp_main_settings(path));
            file["main_settings"] = json!(true);
            file["effective"] = json!(p == effective);
            file["shadowed_by"] = if p != effective && p.exists() { json!(effective) } else { Value::Null };
        }
        file
    }).collect()
}
fn profile_paths(store: &Store, agent: &str) -> Vec<PathBuf> {
    let root = match agent {
        "omp" => omp_root(store).join("profiles"),
        "hermes" => store.home.join(".hermes/profiles"),
        _ => return vec![],
    };
    let mut profiles = Vec::new();
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                let path = if agent == "omp" {
                    entry.path().join("agent")
                } else {
                    entry.path()
                };
                if path.is_dir() {
                    profiles.push(path);
                }
            }
        }
    }
    profiles.sort();
    profiles
}

pub async fn discover(store: &Store, agent: Option<&str>, path: Option<&str>) -> Result<Value> {
    if path.is_some() && agent.is_none() {
        return Err(Error::new(
            "invalid_arguments",
            "An explicit native path requires --agent",
        ));
    }
    if agent.is_some_and(|a| !adapters::AGENTS.contains(&a)) {
        return Err(Error::new(
            "unsupported_agent",
            "Supported agents are codex, pi, omp, opencode, and hermes",
        ));
    }
    let mut installations = Vec::new();
    for a in adapters::AGENTS
        .into_iter()
        .filter(|a| agent.is_none_or(|wanted| wanted == *a))
    {
        let selected = path
            .map(PathBuf::from)
            .unwrap_or_else(|| default_scope(store, a));
        let (resolved, reason) = scope_evidence(store, &selected)?;
        let available = reason == "available";
        // Version commands use an existing directory and never select credentials.
        let evidence = version(a, if available { &resolved } else { &store.home }).await;
        let scopes = profile_paths(store, a);
        installations.push(json!({"target":null,"agent":a,"path":resolved,"scope_state":if available{"available"}else{"unavailable"},"scope_reason_code":if available{None}else{Some(reason)},"installation":evidence,"profiles":scopes,"files":if available{safe_files(a,&resolved)}else{Vec::new()},"credential_scopes":if available{auth_paths(a,&resolved).iter().map(|p|json!({"path":p,"present":p.exists(),"contents":"not_read","owner":a})).collect::<Vec<_>>()}else{Vec::new()},"precedence":adapters::precedence(a),"supported_roles":supported_roles(a),"imported":false}));
    }
    Ok(if path.is_some() {
        installations.remove(0)
    } else {
        json!({"installations":installations})
    })
}
fn owner(store: &Store, target: &str) -> Result<Option<String>> {
    secrets::owner_identity(store, "agent", target)
}
fn record_for<'a>(index: &'a Index, identity: Option<&str>, scope: &Path) -> Option<&'a Record> {
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
fn field_drift(record: &Record, read_credentials: bool) -> Result<Vec<Value>> {
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
fn record_view(store: &Store, record: &Record) -> Value {
    let drift = field_drift(record, false);
    let protection = secrets::check_recovery(store, &record.payload_ref, &record.owner)
        .map(|_| "current_user_files");
    json!({"recovery":record.id,"target":record.target,"agent":record.agent,"path":record.scope,"state":record.state,"protection":protection.as_ref().ok(),"restore_available":protection.is_ok(),"storage_reason_code":protection.as_ref().err().map(|error|error.code.as_str()),"files":record.files.iter().map(|f|json!({"path":f.path,"written":f.written,"owned_fields":f.fields.iter().map(|f|&f.path).collect::<Vec<_>>()})).collect::<Vec<_>>(),"conflicts":drift.as_ref().ok(),"inspection_state":if drift.is_ok(){"available"}else{"unknown"},"reason_code":drift.err().map(|e|e.code),"available_actions":if protection.is_ok(){json!(["agent restore --recovery","agent recovery discard"])}else{json!([])}})
}

pub async fn inspect(store: &Store, config: &Config, id: &str) -> Result<Value> {
    let target = target(config, id)?;
    let (path, scope_reason) = scope_evidence(store, &target.scope)?;
    let available = scope_reason == "available";
    let index: Index = store.read_json(INDEX)?;
    let identity = owner(store, id)?;
    let record = record_for(&index, identity.as_deref(), &path);
    let mut blockers = Vec::new();
    if !available {
        blockers.push(json!({"code":"native_scope_unavailable","path":path,"reason_code":scope_reason,"message":"Select an existing native configuration directory or explicitly create the intended directory before applying"}));
    }
    if (target.agent == "codex" && target.default.is_none())
        || target.providers.is_empty()
        || target.providers.values().any(|p| p.models.is_empty())
    {
        blockers.push(json!({"code":"target_incomplete","message":"Select providers and models before applying; Codex also requires a default model"}));
    }
    let mut pending = None;
    if available {
        let plan = match entries(store, config, id, target, &path, true).await {
            Ok((entries, references)) => adapters::build(target, &path, &entries)
                .and_then(|files| pending_native_changes(target, &files, &references, record)),
            Err(error) => Err(error),
        };
        match plan {
            Ok(changes) => pending = Some(changes),
            Err(error) => blockers
                .push(json!({"code":error.code,"field":error.field,"message":error.message})),
        }
        if let Some(record) = record {
            match field_drift(record, false) {
                Ok(drift) => blockers.extend(drift),
                Err(error) => blockers.push(json!({"code":error.code,"message":error.message})),
            }
        }
    }
    let installed = if available {
        version(&target.agent, &path).await
    } else {
        json!({"state":"not_checked","version":null,"compatibility":"unknown","reason_code":"native_scope_unavailable"})
    };
    if available && let Some(blocker) = version_blocker(&target.agent, &installed) {
        blockers.push(blocker);
    }
    let applied_state = match &pending {
        None => "unknown",
        Some(_) if record.is_none() => "not_applied",
        Some(changes) if !changes.is_empty() => "pending_native_changes",
        Some(_) => record.map(|r| r.state.as_str()).unwrap_or("not_applied"),
    };
    Ok(
        json!({"target":id,"agent":target.agent,"path":path,"scope_state":if available{"available"}else{"unavailable"},"scope_reason_code":if available{None}else{Some(scope_reason)},"providers":target.providers.iter().map(|(name,p)|json!({"name":name,"source":if let Some(p)=&p.provider{json!({"provider":p})}else{json!({"gateway":true})},"models":p.models})).collect::<Vec<_>>(),"default":target.default,"roles":target.roles,"supported_roles":supported_roles(&target.agent),"installation":installed,"warnings":version_warnings(&installed),"precedence":adapters::precedence(&target.agent),"files":if available{safe_files(&target.agent,&path)}else{Vec::new()},"credential_handoff":{"owner":target.agent,"plaintext_in_native_configuration":target.agent == "hermes","plaintext_private_native_credential_file":target.agent != "hermes","reference":if target.agent == "hermes" {"native api_key in current-user-restricted config.yaml"} else {"native command/file helper"},"secret_reads":false,"native_auth_import":false},"desired_state":"saved","applied_state":applied_state,"last_application_state":record.map(|r|r.state.as_str()),"pending_native_changes":pending,"effective_state":"native_reload_and_inference_not_verified","blockers":blockers,"recovery":record.map(|r|record_view(store,r))}),
    )
}

async fn entries(
    store: &Store,
    config: &Config,
    id: &str,
    target: &AgentTarget,
    path: &Path,
    _dry_run: bool,
) -> Result<(Vec<Entry>, Vec<String>)> {
    let mut entries = Vec::new();
    let mut references = Vec::new();
    let authorization = compatibility::AuthorizationSnapshot::capture(store)?;
    let metadata = if matches!(target.agent.as_str(), "pi" | "omp" | "opencode" | "hermes") {
        crate::catalog::read(store, "accepted")?
    } else {
        None
    };
    for (name, binding) in &target.providers {
        if binding.models.is_empty() {
            return Err(Error::new(
                "target_incomplete",
                "Every native provider entry requires an explicit model selection",
            ));
        }
        let protocols: BTreeSet<&str> =
            binding.models.iter().map(|m| m.protocol.as_str()).collect();
        if protocols.len() != 1 && matches!(target.agent.as_str(), "codex" | "hermes") {
            return Err(Error::new(
                "incompatible",
                "This native provider has one effective transport; select separate native entries for different protocols",
            ));
        }
        let protocol = *protocols
            .first()
            .ok_or_else(|| Error::new("target_incomplete", "Select a native protocol"))?;
        let requirements = compatibility::Context {
            agent: &target.agent,
            target: Some(id),
            provider: binding.provider.as_deref(),
            gateway: binding.gateway == Some(true),
        };
        requirements.protocol(protocol)?;
        let mut canonical = None;
        let mut model_routes = BTreeMap::new();
        let mut model_metadata = BTreeMap::new();
        let (base, key_ref, login_provider) = if let Some(provider_id) = &binding.provider {
            let provider = config.providers.get(provider_id).ok_or_else(|| {
                Error::new(
                    "unresolved_reference",
                    "Native upstream provider does not resolve",
                )
            })?;
            if !provider.enabled {
                return Err(Error::new(
                    "model_unavailable",
                    "Native upstream provider is disabled",
                ));
            }
            let integration = crate::integrations::integration_id(provider)?;
            for selected in &binding.models {
                let model = provider.models.get(&selected.id).ok_or_else(|| {
                    Error::new(
                        "unresolved_reference",
                        "Native upstream model does not resolve",
                    )
                })?;
                if !model.enabled {
                    return Err(Error::new(
                        "model_unavailable",
                        "Native upstream model is disabled",
                    ));
                }
                requirements.assess(
                    config,
                    provider_id,
                    &selected.id,
                    &selected.protocol,
                    &selected.id,
                )?;
                if matches!(target.agent.as_str(), "pi" | "omp" | "opencode" | "hermes") {
                    let projection = crate::catalog::effective_from_snapshot(
                        metadata.as_ref(),
                        provider,
                        Some((&selected.id, model)),
                    )?;
                    model_metadata.insert(
                        selected.id.clone(),
                        adapters::model_metadata(&target.agent, &projection["effective"])?,
                    );
                }
            }
            if integration == "opencode-go" && target.agent == "pi" {
                canonical = Some("opencode-go".to_owned());
            }
            let bases = crate::integrations::resolve(provider)?;
            for selected in &binding.models {
                let endpoint = bases.get(&selected.protocol).cloned().ok_or_else(|| {
                    Error::new(
                        "incompatible",
                        "The upstream integration has no endpoint for a selected native protocol",
                    )
                })?;
                if model_routes
                    .insert(selected.id.clone(), (selected.protocol.clone(), endpoint))
                    .is_some()
                {
                    return Err(Error::new(
                        "native_namespace_conflict",
                        "One native provider/model ID cannot simultaneously name two protocol variants",
                    ));
                }
            }
            let base = bases.get(protocol).cloned().ok_or_else(|| {
                Error::new(
                    "incompatible",
                    "The upstream integration has no endpoint for this native protocol",
                )
            })?;
            match provider.auth.as_ref() {
                Some(Auth::ApiKey { key_ref }) => (base, Some(key_ref.clone()), None),
                Some(Auth::Login { .. }) => {
                    let native_provider = requirements
                        .login_provider(provider)?
                        .ok_or_else(unavailable)?;
                    authorization.assess(store, config, &requirements, provider_id)?;
                    (base, None, Some(native_provider.to_owned()))
                }
                None => {
                    return Err(Error::new(
                        "credential_unavailable",
                        "Configure the selected provider's API key or native login before application",
                    ));
                }
            }
        } else if binding.gateway == Some(true) {
            let gateway = config.gateway.as_ref().ok_or_else(|| {
                Error::new("model_unavailable", "The sole gateway is not configured")
            })?;
            let status = crate::service::status(store).await?;
            let desired_revision = config::load(store)?.revision;
            let ready = &status["gateway"];
            if ready["ready"] != true
                || ready["listen"] != gateway.listen
                || ready["key_ref"] != gateway.key_ref
                || ready["config_revision"] != desired_revision
            {
                return Err(Error::new(
                    "gateway_not_ready",
                    "Explicitly start the configured gateway and activate the matching complete snapshot and shared key before native application",
                ));
            }
            for m in &binding.models {
                requirements.assess_public(config, &m.id, &m.protocol)?;
                if !config
                    .model_aliases
                    .iter()
                    .any(|a| a.id == m.id && a.protocol == m.protocol && a.enabled)
                {
                    let (_, model_id) = config::qualified(&m.id)?;
                    let (p, model) = config::model(config, &m.id)?;
                    if !p.enabled
                        || !model.enabled
                        || !crate::integrations::protocols(p, model_id)?
                            .iter()
                            .any(|p| p == &m.protocol)
                    {
                        return Err(Error::new(
                            "model_unavailable",
                            "Selected public concrete gateway model is not published for this protocol",
                        ));
                    }
                    if matches!(target.agent.as_str(), "pi" | "omp" | "opencode" | "hermes") {
                        let projection = crate::catalog::effective_from_snapshot(
                            metadata.as_ref(),
                            p,
                            Some((model_id, model)),
                        )?;
                        model_metadata.insert(
                            m.id.clone(),
                            adapters::model_metadata(&target.agent, &projection["effective"])?,
                        );
                    }
                }
            }
            (
                format!("http://{}", gateway.listen),
                Some(gateway.key_ref.clone()),
                None,
            )
        } else {
            return Err(Error::new(
                "invalid_configuration",
                "Native entry requires exactly one upstream or gateway source",
            ));
        };
        if model_routes.is_empty() {
            for selected in &binding.models {
                if model_routes
                    .insert(
                        selected.id.clone(),
                        (selected.protocol.clone(), base.clone()),
                    )
                    .is_some()
                {
                    return Err(Error::new(
                        "native_namespace_conflict",
                        "One native model ID cannot name two protocol variants",
                    ));
                }
            }
        }
        if key_ref.is_some()
            && target.agent == "pi"
            && auth_namespace_present(
                &path.join("auth.json"),
                canonical.as_deref().unwrap_or(name),
            )?
        {
            return Err(Error::new(
                "native_credential_precedence_unknown",
                "The selected exact native provider namespace has saved authentication that can override this key reference; resolve it through the native owner or select an independent scope. Credential values were not materialized",
            ));
        }
        let key = key_ref.inspect(|reference| {
            references.push(reference.clone());
        });
        entries.push(Entry {
            name: name.clone(),
            base,
            protocol: protocol.into(),
            key,
            login_provider,
            canonical,
            gateway: binding.gateway == Some(true),
            models: binding.models.iter().map(|m| m.id.clone()).collect(),
            model_routes,
            model_metadata,
        });
    }
    if (target.agent == "codex" && target.default.is_none()) || entries.is_empty() {
        return Err(Error::new(
            "target_incomplete",
            "Select providers and models before applying; Codex also requires a default model",
        ));
    }
    let mut names = BTreeSet::new();
    for entry in &entries {
        if !names.insert(entry.native_name()) {
            return Err(Error::new(
                "native_scope_conflict",
                "Two desired entries resolve to one native provider/authentication namespace",
            ));
        }
    }
    Ok((entries, references))
}
fn leaves(value: &Value, path: &mut Vec<String>, out: &mut Vec<(Vec<String>, Value)>) {
    if let Some(map) = value.as_object().filter(|m| !m.is_empty()) {
        for (k, v) in map {
            path.push(k.clone());
            leaves(v, path, out);
            path.pop();
        }
    } else {
        out.push((path.clone(), value.clone()));
    }
}
fn inline_credential(agent: &str, path: &[String]) -> bool {
    agent == "hermes" && path.len() == 3 && path[0] == "providers" && path[2] == "api_key"
}
fn desired_fields(
    agent: &str,
    desired: &Value,
    current: &Value,
) -> Vec<(Vec<String>, Option<Value>)> {
    let mut fields = Vec::new();
    leaves(desired, &mut Vec::new(), &mut fields);
    fields.retain(|(path, value)| {
        if path.is_empty() {
            return false;
        }
        !(agent == "hermes"
            && path.len() == 4
            && path[0] == "providers"
            && path[2] == "models"
            && value.as_object().is_some_and(|model| model.is_empty())
            && document::get(current, path)
                .and_then(Value::as_object)
                .is_some_and(|model| !model.is_empty()))
    });
    let mut fields: Vec<_> = fields
        .into_iter()
        .map(|(path, value)| (path, Some(value)))
        .collect();
    if agent == "hermes" {
        // Remove obsolete command references and competing environment keys for
        // providers whose API key tmax now owns. Record these removals so restore
        // can recover the original fields without changing unrelated providers.
        if let Some(providers) = desired.get("providers").and_then(Value::as_object) {
            for (name, provider) in providers {
                if provider.get("api_key").is_some() {
                    for obsolete in ["key_cmd", "key_env", "api_key_env"] {
                        fields.push((
                            vec!["providers".into(), name.clone(), obsolete.into()],
                            None,
                        ));
                    }
                }
            }
        }
    }
    fields
}

fn pending_native_changes(
    target: &AgentTarget,
    files: &[File],
    references: &[String],
    record: Option<&Record>,
) -> Result<Vec<Value>> {
    let mut changes = Vec::new();
    for file in files
        .iter()
        .filter(|file| file.format != Format::Credential)
    {
        let current = document::parse(
            read_native(&file.path)?.as_deref().unwrap_or(""),
            file.format,
        )?;
        let desired = desired_fields(&target.agent, &file.desired, &current);
        let previous = record.and_then(|record| {
            record
                .files
                .iter()
                .find(|previous| previous.path == file.path)
        });
        for (path, value) in &desired {
            if !previous.is_some_and(|file| {
                file.written
                    && file.fields.iter().any(|field| {
                        !field.released
                            && field.path == *path
                            && field.after_hash == field_hash(document::get(&current, path))
                    })
                    && (inline_credential(&target.agent, path)
                        || document::get(&current, path) == value.as_ref())
            }) {
                changes.push(json!({"path":file.path,"field":path,"change":if value.is_some(){"set"}else{"remove"}}));
            }
        }
        if let Some(previous) = previous {
            for field in previous.fields.iter().filter(|field| !field.released) {
                if !desired.iter().any(|(path, _)| *path == field.path) {
                    changes.push(json!({"path":file.path,"field":field.path,"change":"release"}));
                }
            }
        }
    }
    if let Some(record) = record {
        for file in record.files.iter().filter(|file| {
            file.format != Format::Credential
                && !files.iter().any(|desired| desired.path == file.path)
        }) {
            for field in file.fields.iter().filter(|field| !field.released) {
                changes.push(json!({"path":file.path,"field":field.path,"change":"release"}));
            }
        }
    }
    if record
        .map(|record| record.secret_refs.as_slice())
        .unwrap_or_default()
        != references
    {
        changes.push(
            json!({"field":["credential_reference"],"change":"replace","secret_reads":false}),
        );
    }
    Ok(changes)
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
fn protect_payload(
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
fn decode_payload(store: &Store, record: &Record) -> Result<Payload> {
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
fn conflicts(index: &Index, identity: Option<&str>, files: &[File]) -> Result<()> {
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

fn resolve_handoff(store: &Store, agent: &str, files: &mut [File]) -> Result<()> {
    for file in files {
        if file.format == Format::Credential {
            if let Some(reference) = file.desired.as_str() {
                file.desired = handoff_key(store, reference)?;
            }
        } else if agent == "hermes"
            && let Some(providers) = file
                .desired
                .get_mut("providers")
                .and_then(Value::as_object_mut)
        {
            for provider in providers.values_mut() {
                if let Some(reference) = provider.get("api_key").and_then(Value::as_str) {
                    provider["api_key"] = handoff_key(store, reference)?;
                }
            }
        }
    }
    Ok(())
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
        if dry_run && file.format == Format::Credential {
            previews.push(json!({"path":file.path,"present":file.path.exists(),"credential_handoff":"private native-owned credential file, referenced by the documented native key command/file mechanism","plaintext_native_store":true,"content_revision":"checked_only_on_execution","secret_reads":false}));
            continue;
        }
        let before = read_native(&file.path)?;
        let current = document::parse(before.as_deref().unwrap_or(""), file.format)?;
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
        previews.push(json!({"existing_provider_entries":existing_entries,"path":file.path,"revision":revision(before.as_deref()),"owned_fields":fields.iter().map(|(p,_)|p).collect::<Vec<_>>(),"credential_handoff":if target.agent == "hermes" {"plaintext API key in current-user-restricted config.yaml; no token import"} else {"native command/file reference to a private native-owned credential artifact; no inline config secret or token import"}}));
        observed_inputs.push((file.path.clone(), before));
    }
    if dry_run {
        return Ok(
            json!({"target":id,"agent":target.agent,"path":path,"dry_run":true,"files":previews,"supported_roles":supported_roles(&target.agent),"default":target.default,"roles":target.roles,"precedence":adapters::precedence(&target.agent),"qualification":{"state":"requires_execution_version_check","exact_version":adapters::qualified_version(&target.agent),"source":adapters::source(&target.agent),"inference":"not_verified"},"credentials":{"resolved":false,"plaintext_native_handoff":true},"recovery":{"planned":"exact native bytes in current-user-only files before every external write","retained":previous.as_ref().map(|r|&r.id)},"effects":{"native_writes":false,"secret_reads":false,"processes_started":false,"locks_created":false},"native_reload":"required; see precedence.native_reload"}),
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
    resolve_handoff(store, &target.agent, &mut files)?;
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
fn partial(
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

fn restore_text(
    current: Option<&str>,
    file: &FileRecord,
    capture: &Capture,
) -> Result<Option<String>> {
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
    // Remove only containers tmax introduced and which now have no settings.
    // User-added fields survive; comments force a conflict rather than deletion.
    let mut parents: BTreeSet<Vec<String>> = BTreeSet::new();
    for field in &file.fields {
        for n in 1..field.path.len() {
            parents.insert(field.path[..n].to_vec());
        }
    }
    let mut parents: Vec<_> = parents.into_iter().collect();
    parents.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for parent in parents {
        let value = document::parse(&text, file.format)?;
        if document::get(&original, &parent).is_none()
            && document::get(&value, &parent)
                .is_some_and(|v| v.as_object().is_some_and(|m| m.is_empty()))
        {
            text = document::set(&text, file.format, &parent, None)?;
        }
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
            "Owned native fields differ from both their captured originals and tmax writes; inspect recovery conflicts before restoration",
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
    let _lock = if action == "discard" && !request.dry_run {
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

pub fn login_source_agents(integration: &str) -> &'static [&'static str] {
    match integration {
        "codex-subscription" => &["codex", "pi", "omp", "opencode", "hermes"],
        "kimi-coding-plan" => &["omp"],
        _ => &[],
    }
}
fn login_source_revision(target: &AgentTarget, path: &Path) -> String {
    digest(format!("{}\0{}", target.agent, path.display()).as_bytes())
}
fn login_provider(config: &Config, provider: &str, target: &AgentTarget) -> Result<&'static str> {
    let provider = config
        .providers
        .get(provider)
        .ok_or_else(|| Error::new("unknown_resource", "Provider is not configured"))?;
    match (
        crate::integrations::integration_id(provider)?,
        target.agent.as_str(),
    ) {
        ("codex-subscription", "codex" | "opencode") => Ok("openai"),
        ("codex-subscription", "pi" | "omp" | "hermes") => Ok("openai-codex"),
        ("kimi-coding-plan", "omp") => Ok("kimi-code"),
        _ => Err(Error::new(
            "unsupported_authentication",
            "This access product has no qualified native login for the selected agent. API-key entry is not login authentication",
        )),
    }
}
// Version-qualified, read-only metadata projection. Never SELECT `data` or
// identity_key: those columns contain native credential/session material.
async fn omp_oauth_count(path: &Path, provider: &str) -> Result<u64> {
    if executable("sqlite3").is_none() {
        return Err(Error::new(
            "native_metadata_reader_unavailable",
            "Qualified omp account isolation requires the read-only sqlite3 metadata reader",
        ));
    }
    let db = path.join("agent.db");
    if !db.exists() {
        return Ok(0);
    }
    let mut cmd = tokio::process::Command::new("sqlite3");
    cmd.args(["-readonly"]).arg(db).arg(format!("SELECT count(*) FROM auth_credentials WHERE provider='{}' AND credential_type='oauth' AND disabled_cause IS NULL;",provider.replace('\'',"''"))).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(5), cmd.output())
        .await
        .map_err(|_| {
            Error::new(
                "native_metadata_unavailable",
                "Native account metadata inspection timed out",
            )
        })?
        .map_err(|_| {
            Error::new(
                "native_metadata_unavailable",
                "Cannot inspect native account metadata",
            )
        })?;
    if !output.status.success() || output.stdout.len() > 64 {
        return Err(Error::new(
            "native_metadata_unavailable",
            "The scoped native account schema is unavailable; no credential values were selected",
        ));
    }
    std::str::from_utf8(&output.stdout)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .ok_or_else(|| {
            Error::new(
                "native_metadata_unavailable",
                "Native account count is unavailable",
            )
        })
}
async fn opencode_accounts(path: &Path) -> Result<Vec<String>> {
    let mut cmd = native_command("opencode", path)?;
    cmd.args(["auth", "list", "--standalone", "--format", "json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let output = tokio::time::timeout(Duration::from_secs(30), cmd.output())
        .await
        .map_err(|_| {
            Error::new(
                "native_metadata_unavailable",
                "Native private-server account inspection timed out",
            )
        })?
        .map_err(|_| unavailable())?;
    if !output.status.success() || output.stdout.len() > 1024 * 1024 {
        return Err(Error::new(
            "native_metadata_unavailable",
            "Native private-server account metadata is unavailable",
        ));
    }
    let summary: Value = serde_json::from_slice(&output.stdout).map_err(|_| {
        Error::new(
            "native_metadata_unavailable",
            "Native account summary does not match the researched version",
        )
    })?;
    let integrations = summary.as_array().ok_or_else(|| {
        Error::new(
            "native_metadata_unavailable",
            "Native account summary has an unknown shape",
        )
    })?;
    let mut accounts = Vec::new();
    for integration in integrations.iter().filter(|i| i["id"] == "openai") {
        let connections = integration["connections"].as_array().ok_or_else(|| {
            Error::new(
                "native_metadata_unavailable",
                "Native connection metadata is unavailable",
            )
        })?;
        for connection in connections.iter().filter(|c| c["type"] == "credential") {
            if connection["method"] != "oauth" {
                return Err(Error::new(
                    "native_scope_conflict",
                    "The native OpenAI integration has a different saved authentication method",
                ));
            }
            accounts.push(
                connection["id"]
                    .as_str()
                    .ok_or_else(|| {
                        Error::new(
                            "native_metadata_unavailable",
                            "Native credential ID is unavailable",
                        )
                    })?
                    .to_owned(),
            );
        }
    }
    Ok(accounts)
}
async fn codex_authenticated(path: &Path) -> Result<bool> {
    let mut cmd = native_command("codex", path)?;
    cmd.args(["login", "status"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    tokio::time::timeout(Duration::from_secs(10), cmd.status())
        .await
        .map_err(|_| {
            Error::new(
                "native_metadata_unavailable",
                "Native auth status timed out",
            )
        })?
        .map(|s| s.success())
        .map_err(|_| unavailable())
}
pub fn rename_target(store: &Store, old: &str, new: &str) -> Result<()> {
    let _lock = store.lock("native")?;
    let stable = owner(store, old)?;
    let mut index: Index = store.read_json(INDEX)?;
    for record in &mut index.records {
        if stable.as_deref() == Some(&record.owner) {
            record.target = new.into();
        }
    }
    let mut logins: Logins = store.read_json(LOGINS)?;
    for login in &mut logins.entries {
        if login.target == old {
            login.target = new.into();
        }
    }
    store.write_json(INDEX, &index)?;
    store.write_json(LOGINS, &logins)?;
    secrets::rename_owner(store, "agent", old, new)
}
pub fn remove_target(store: &Store, id: &str) -> Result<()> {
    let _lock = store.lock("native")?;
    let mut logins: Logins = store.read_json(LOGINS)?;
    for login in &mut logins.entries {
        if login.target == id {
            login.state = "source_removed".into();
        }
    }
    store.write_json(LOGINS, &logins)?;
    secrets::detach_owner(store, "agent", id)
}
pub async fn login(
    store: &Store,
    config: &Config,
    provider: &str,
    via: &str,
    dry_run: bool,
) -> Result<Value> {
    native_auth(store, config, provider, via, dry_run, false).await
}
pub async fn logout(
    store: &Store,
    config: &Config,
    provider: &str,
    via: &str,
    dry_run: bool,
) -> Result<Value> {
    native_auth(store, config, provider, via, dry_run, true).await
}
// No await or owner side effect may separate invalidation from the write-ahead
// record. Invalidation comes first so an interruption after that record can never
// leave facts from the old authorization current. Caller holds routing and native.
fn begin_native_auth(
    store: &Store,
    provider: &str,
    logins: &Logins,
) -> std::result::Result<(), Value> {
    crate::inputs::authorization_changed_locked(store, provider).map_err(|error| {
        json!({"_code":"recovery_required","state":"native_not_attempted",
            "native_login_attempted":false,"native_authorization_may_have_changed":false,
            "authorization_invalidation":{"state":"failed","persistence":"unknown","code":error.code},
            "recovery":"Repair authorization-lineage storage before another explicit native owner operation; no native authorization operation was started"})
    })?;
    store.write_json(LOGINS, logins).map_err(|error| {
        json!({"_code":"recovery_required","state":"native_not_attempted",
            "native_login_attempted":false,"native_authorization_may_have_changed":false,
            "authorization_invalidation":{"state":"completed"},
            "native_record":{"state":"unknown","code":error.code},
            "recovery":"Inspect native login metadata before another explicit owner operation; authorization facts were invalidated but no native authorization operation was started"})
    })
}

async fn native_auth(
    store: &Store,
    config: &Config,
    provider: &str,
    via: &str,
    dry_run: bool,
    logout: bool,
) -> Result<Value> {
    let _authentication = if dry_run {
        None
    } else {
        Some(store.try_lock("native-authentication")?.ok_or_else(|| {
            Error::new(
                "acquisition_in_progress",
                "Another native sign-in is in progress",
            )
        })?)
    };
    let target = target(config, via)?;
    let path = scope(store, &target.scope)?;
    let native_provider = login_provider(config, provider, target)?;
    let _configuration = if dry_run {
        None
    } else {
        Some(store.lock("configuration")?)
    };
    let routing = if dry_run {
        None
    } else {
        let saved = config::load(store)?;
        if serde_json::to_value(&saved.config).map_err(|_| unavailable())?
            != serde_json::to_value(config).map_err(|_| unavailable())?
        {
            return Err(Error::new(
                "inputs_changed",
                "The desired configuration changed before native authentication; inspect the current authority",
            ));
        }
        Some(store.lock("routing")?)
    };
    let _lock = if dry_run {
        None
    } else {
        Some(store.lock("native")?)
    };
    let mut logins: Logins = store.read_json(LOGINS)?;
    let source_revision = login_source_revision(target, &path);
    if config.providers.iter().any(|(id, p)| {
        id != provider && matches!(p.auth.as_ref(),Some(Auth::Login{via:other,..}) if other==via)
    }) || logins.entries.iter().any(|l| {
        l.provider != provider
            && l.scope == path
            && l.agent == target.agent
            && l.state != "signed_out"
    }) {
        return Err(Error::new(
            "native_scope_conflict",
            "This native authorization scope is already selected by another provider instance; use an independent supported native scope",
        ));
    }
    let existing = logins.entries.iter().find(|l| {
        l.provider == provider
            && l.target == via
            && l.scope == path
            && l.source_revision == source_revision
    });
    if !logout && existing.is_some_and(|record| record.account.is_some()) {
        return Err(Error::new(
            "browser_login_required",
            "Reauthenticate this account through provider login-start to preserve account comparison",
        ));
    }
    if !logout
        && existing.is_none()
        && target.agent != "opencode"
        && auth_paths(&target.agent, &path).iter().any(|p| p.exists())
    {
        return Err(Error::new(
            "native_ambient_login_not_adopted",
            "This scope already contains native authentication. Use an independent scope for an explicit sign-in; tmax neither reads nor adopts the existing credentials",
        ));
    }
    if logout && existing.is_none() {
        return Err(Error::new(
            "native_ambient_login_not_adopted",
            "No explicit tmax-managed native login belongs to this provider and scope; ambient credentials are not signed out",
        ));
    }
    let mut command = Vec::<String>::new();
    match (target.agent.as_str(), logout) {
        ("codex", false) => command.extend(["login", "--device-auth"].map(String::from)),
        ("codex", true) => command.push("logout".into()),
        ("opencode", false) => command.extend(
            [
                "auth",
                "login",
                "openai",
                "--method",
                "chatgpt-headless",
                "--standalone",
            ]
            .map(String::from),
        ),
        ("opencode", true) => {
            let credential=existing.and_then(|l|l.credential_id.as_deref()).ok_or_else(||Error::new("recovery_required","The selected native OpenCode account ID is unavailable; inspect the owner scope"))?;
            command
                .extend(["auth", "logout", "openai", credential, "--standalone"].map(String::from));
        }
        ("omp", false) => command.extend(["login", native_provider].map(String::from)),
        ("hermes", false) => command.extend(
            [
                "auth",
                "add",
                native_provider,
                "--type",
                "oauth",
                "--no-browser",
            ]
            .map(String::from),
        ),
        ("hermes", true) => {
            command.extend(["logout", "--provider", native_provider].map(String::from))
        }
        ("pi", _) | ("omp", true) => command.push("--no-session".into()),
        _ => {
            return Err(Error::new(
                "native_login_unqualified",
                "No supported native owner command is qualified",
            ));
        }
    }
    if dry_run {
        return Ok(
            json!({"provider":provider,"via":via,"agent":target.agent,"path":path,"dry_run":true,"native_command":command,"native_owner":target.agent,"credential_scope":auth_paths(&target.agent,&path),"identity":"unknown","shared_consumers":config.agents.iter().filter(|(_,a)|a.agent==target.agent && scope(store,&a.scope).is_ok_and(|p|p==path)).map(|(id,_)|id).collect::<Vec<_>>(),"effects":{"native_authorization_may_change_on_execution":true,"native_token_import":false,"processes_started":false},"recovery":"Repeat independent sign-in through the same native owner to reestablish intended authorization; no token rollback or ambient adoption"}),
        );
    }
    let installed = version(&target.agent, &path).await;
    if installed["compatibility"] != "documented" {
        return Err(Error::new(
            "native_version_unknown",
            "Native login commit and recovery are qualified only for the exact researched owner version",
        ));
    }
    if target.agent == "codex" && existing.is_none() && codex_authenticated(&path).await? {
        return Err(Error::new(
            "native_ambient_login_not_adopted",
            "The native configured keyring/file already has authorization for this CODEX_HOME; an ambient session is not adopted",
        ));
    }
    if target.agent == "hermes" && !logout {
        let config_file = path.join("config.yaml");
        let text = read_native(&config_file)?;
        let cfg = document::parse(text.as_deref().unwrap_or(""), Format::Yaml)?;
        if cfg
            .get("auth")
            .and_then(|a| a.get("adopt_external_logins"))
            .is_some_and(|v| !v.is_boolean())
        {
            return Err(Error::new(
                "native_config_invalid",
                "Hermes external-login adoption setting has an invalid type",
            ));
        }
        if existing.is_some_and(|l| l.state == "signed_in") {
            return Err(Error::new(
                "native_login_recovery_unqualified",
                "Hermes auth add creates a rotating credential pool. Sign out this selected independent scope first; adding another pooled login cannot qualify a pin",
            ));
        }
    }
    if target.agent == "omp" {
        let cfg = document::parse(
            read_native(&path.join(omp_main_settings(&path)))?
                .as_deref()
                .unwrap_or(""),
            Format::Yaml,
        )?;
        if std::env::var_os("OMP_AUTH_BROKER_URL").is_some()
            || cfg.pointer("/auth/broker/url").is_some()
        {
            return Err(Error::new(
                "native_login_unqualified",
                "This target uses an auth broker rather than its local native credential scope; select an independent local profile",
            ));
        }
        if path == store.home.join(".omp/agent") && std::env::var_os("XDG_DATA_HOME").is_some() {
            return Err(Error::new(
                "native_scope_ambiguous",
                "The default omp database can follow XDG migration; use an independent explicit agent directory for qualified login ownership",
            ));
        }
        if omp_oauth_count(&path, native_provider).await? > 1 {
            return Err(Error::new(
                "native_scope_conflict",
                "Several native OAuth accounts can rotate in this scope; a single selected authorization is required",
            ));
        }
    }
    let session_ref = existing
        .map(|l| l.session_ref.clone())
        .unwrap_or_else(|| format!("native-session-{}", uuid::Uuid::new_v4()));
    let prior_credential = existing.and_then(|l| l.credential_id.clone());
    if target.agent == "opencode" {
        let accounts = opencode_accounts(&path).await?;
        if accounts.len() > 1
            || accounts
                .first()
                .is_some_and(|id| Some(id) != prior_credential.as_ref())
        {
            return Err(Error::new(
                "native_ambient_login_not_adopted",
                "Use a fresh independent OpenCode native server database; no discovered saved account is adopted",
            ));
        }
    }
    let mut native_credential_id = prior_credential.clone();
    let mut cmd = native_command(&target.agent, &path)?;
    let interactive_ui = target.agent == "pi" || (target.agent == "omp" && logout);
    // Native login remains interactive and owns its browser/device flow. Route
    // native UI to the terminal, never the machine-readable management result.
    #[cfg(unix)]
    {
        let mut terminal = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .map_err(|_| {
                Error::new(
                    "interactive_required",
                    "Native sign-in/sign-out requires an interactive controlling terminal",
                )
            })?;
        if interactive_ui {
            use std::io::Write;
            let action = if logout {
                "/logout".to_owned()
            } else {
                format!("/login {native_provider}")
            };
            writeln!(terminal,"tmax selected native scope {}. Complete {action} in the native owner, select only {native_provider}, then exit the native UI. tmax will not import tokens.",path.display()).map_err(|_|unavailable())?;
        }
        cmd.stdin(Stdio::from(
            terminal.try_clone().map_err(|_| unavailable())?,
        ))
        .stdout(Stdio::from(
            terminal.try_clone().map_err(|_| unavailable())?,
        ))
        .stderr(Stdio::from(terminal));
    }
    #[cfg(windows)]
    {
        let (input, mut output) = crate::secrets::native_console_stdio()?;
        if interactive_ui {
            let action = if logout {
                "/logout".to_owned()
            } else {
                format!("/login {native_provider}")
            };
            crate::secrets::write_console_output(
                &mut output,
                &format!(
                    "tmax selected native scope {}. Complete {action} in the native owner, select only {native_provider}, then exit the native UI. tmax will not import tokens.\r\n",
                    path.display()
                ),
            )?;
        }
        cmd.stdin(Stdio::from(input))
            .stderr(Stdio::from(output.try_clone().map_err(|_| unavailable())?))
            .stdout(Stdio::from(output));
    }
    let prior_account = existing.and_then(|record| record.account.clone());
    logins
        .entries
        .retain(|l| !(l.provider == provider && l.target == via && l.scope == path));
    logins.entries.push(LoginRecord {
        provider: provider.into(),
        target: via.into(),
        scope: path.clone(),
        agent: target.agent.clone(),
        native_provider: native_provider.into(),
        credential_id: prior_credential.clone(),
        session_ref: session_ref.clone(),
        state: "native_effect_uncertain".into(),
        source_revision,
        account: prior_account,
    });
    if let Err(mut failure) = begin_native_auth(store, provider, &logins) {
        failure["provider"] = json!(provider);
        failure["via"] = json!(via);
        failure["agent"] = json!(target.agent);
        failure["path"] = json!(path);
        return Ok(failure);
    }
    // The durable generations and tombstones now cover every cancellation and
    // partial return below. Do not acquire routing while holding native.
    drop(routing);
    let describe = |mut effect: Value| {
        effect["authorization_invalidation"] = json!({"state":"completed"});
        effect["native_authorization_may_have_changed"] = json!(true);
        effect
    };
    if target.agent == "hermes" && !logout {
        let mut guard = match native_command("hermes", &path) {
            Ok(cmd) => cmd,
            Err(e) => {
                return Ok(describe(
                    json!({"_code":"partial_native_apply","provider":provider,"via":via,"state":"native_effect_uncertain","reason_code":e.code,"native_login_attempted":false,"managed_use_blocked":true}),
                ));
            }
        };
        guard
            .args(["config", "set", "auth.adopt_external_logins", "false"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if !guard.status().await.is_ok_and(|s| s.success()) {
            return Ok(describe(
                json!({"_code":"partial_native_apply","provider":provider,"via":via,"path":path,"state":"native_effect_uncertain","native_login_attempted":false,"native_settings_guard":"may_have_changed","managed_use_blocked":true,"recovery":"Inspect native auth.adopt_external_logins before independent owner sign-in; no external tokens are adopted"}),
            ));
        }
    }
    if target.agent == "opencode"
        && !logout
        && let Some(credential) = &prior_credential
    {
        let mut remove = match native_command("opencode", &path) {
            Ok(cmd) => cmd,
            Err(e) => {
                return Ok(describe(
                    json!({"_code":"partial_native_apply","provider":provider,"state":"native_effect_uncertain","reason_code":e.code,"managed_use_blocked":true}),
                ));
            }
        };
        remove
            .args(["auth", "logout", "openai", credential, "--standalone"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if !remove.status().await.is_ok_and(|s| s.success()) {
            return Ok(describe(
                json!({"_code":"partial_native_apply","provider":provider,"via":via,"state":"native_effect_uncertain","managed_use_blocked":true,"recovery":"Complete sign-out of the recorded native account before independent sign-in"}),
            ));
        }
    }
    cmd.args(&command);
    let result = cmd.status().await;
    let mut success = result.as_ref().is_ok_and(|s| s.success());
    if target.agent == "pi" && success {
        success = if let Ok(mut check) = native_command("pi", &path) {
            check
                .args([
                    "auth",
                    "check",
                    "--provider",
                    native_provider,
                    "--json",
                    "--no-refresh",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            match tokio::time::timeout(Duration::from_secs(10), check.output()).await {
                Ok(Ok(output)) if output.stdout.len() < 4096 => {
                    serde_json::from_slice::<Value>(&output.stdout).is_ok_and(|v| {
                        if logout {
                            v["status"] == "not_ready"
                                || (v["status"] == "ready" && v["authType"] != "oauth")
                        } else {
                            v["status"] == "ready"
                                && v["authType"] == "oauth"
                                && path.join("auth.json").exists()
                        }
                    })
                }
                _ => false,
            }
        } else {
            false
        };
    }
    if target.agent == "omp" && success {
        success = omp_oauth_count(&path, native_provider)
            .await
            .is_ok_and(|count| count == if logout { 0 } else { 1 });
    }
    if target.agent == "codex" && success {
        success = codex_authenticated(&path)
            .await
            .is_ok_and(|ready| ready != logout);
    }
    if target.agent == "opencode" && success {
        match opencode_accounts(&path).await {
            Ok(accounts) if logout => success = accounts.is_empty(),
            Ok(accounts) if accounts.len() == 1 => {
                native_credential_id = accounts.into_iter().next()
            }
            _ => success = false,
        }
    }
    let state = if success {
        if logout { "signed_out" } else { "signed_in" }
    } else {
        "native_effect_uncertain"
    };
    if let Some(login) = logins
        .entries
        .iter_mut()
        .find(|l| l.session_ref == session_ref)
    {
        login.state = state.into();
        login.credential_id = native_credential_id;
    }
    let committed = store.write_json(LOGINS, &logins).is_ok();
    Ok(describe(
        json!({"_code":if success&&committed{"awaiting_native_reload"}else{"partial_native_apply"},"provider":provider,"via":via,"agent":target.agent,"path":path,"state":if committed{state}else{"native_changed_local_commit_failed"},"session_ref":if success&&!logout&&committed{Some(session_ref)}else{None},"identity":"unknown","native_owner":target.agent,"native_session_imported":false,"native_authorization_may_have_changed":true,"managed_use_blocked":!success||!committed||logout,"native_consumers":"May already be affected outside tmax; unchanged desired configuration is not rollback evidence","recovery":"Reestablish the intended authorization through this exact native owner and scope; do not copy tokens or adopt another ambient login"}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn inspection_target(scope: PathBuf) -> AgentTarget {
        AgentTarget {
            agent: "pi".into(),
            scope,
            providers: BTreeMap::from([(
                "local".into(),
                config::NativeProvider {
                    provider: Some("p".into()),
                    gateway: None,
                    models: vec![config::ModelSelection {
                        id: "first".into(),
                        protocol: "chat-completions".into(),
                    }],
                },
            )]),
            default: Some(config::ModelReference {
                provider: "local".into(),
                id: "first".into(),
                protocol: "chat-completions".into(),
            }),
            roles: BTreeMap::new(),
        }
    }
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
        resolve_handoff(&store, "hermes", &mut files).unwrap();
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
    #[cfg(windows)]
    #[test]
    fn npm_pi_resolves_official_package_without_executing_shims() {
        let directory = tempfile::tempdir().unwrap();
        let prefix = directory.path().join("npm with spaces");
        let node_dir = directory.path().join("node with spaces");
        fs::create_dir_all(&node_dir).unwrap();
        fs::create_dir_all(&prefix).unwrap();
        fs::write(prefix.join("pi.cmd"), "exit /b 1").unwrap();
        for package in [
            "@earendil-works/pi-coding-agent",
            "@mariozechner/pi-coding-agent",
        ] {
            let root = prefix.join("node_modules").join(package);
            fs::create_dir_all(root.join("dist/bundle")).unwrap();
            fs::write(root.join("dist/bundle/cli.js"), "console.log('1.4.0')").unwrap();
            fs::write(
                root.join("package.json"),
                serde_json::to_vec(&json!({"name":package,"bin":{"pi":"dist/bundle/cli.js"}}))
                    .unwrap(),
            )
            .unwrap();
            assert!(npm_pi(&prefix, &[]).is_none());
            fs::write(node_dir.join("node.exe"), "fixture").unwrap();
            let found = npm_pi(&prefix, std::slice::from_ref(&node_dir)).unwrap();
            assert_eq!(found.program, node_dir.join("node.exe"));
            assert_eq!(found.script, Some(root.join("dist/bundle/cli.js")));
            fs::write(prefix.join("node.exe"), "sibling").unwrap();
            assert_eq!(
                npm_pi(&prefix, std::slice::from_ref(&node_dir))
                    .unwrap()
                    .program,
                prefix.join("node.exe")
            );
            fs::write(
                root.join("package.json"),
                serde_json::to_vec(&json!({"name":package,"bin":{"pi":"../outside.js"}})).unwrap(),
            )
            .unwrap();
            assert!(npm_pi(&prefix, std::slice::from_ref(&node_dir)).is_none());
            fs::remove_file(prefix.join("node.exe")).unwrap();
            fs::remove_file(node_dir.join("node.exe")).unwrap();
        }
    }
    #[tokio::test]
    async fn unavailable_scope_inspection_retains_desired_target_and_apply_stays_blocked() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let mut config = Config::empty();
        let path = directory.path().join("not-created");
        config
            .agents
            .insert("managed".into(), inspection_target(path.clone()));
        let inspected = inspect(&store, &config, "managed").await.unwrap();
        assert_eq!(inspected["path"], json!(path));
        assert_eq!(inspected["scope_reason_code"], "absent");
        assert_eq!(inspected["default"]["id"], "first");
        assert_eq!(inspected["providers"][0]["models"][0]["id"], "first");
        assert_eq!(inspected["applied_state"], "unknown");
        assert!(inspected["pending_native_changes"].is_null());
        let error = apply(&store, &config, "managed", false).await.unwrap_err();
        assert_eq!(error.code, "native_scope_unavailable");
        assert!(error.message.contains(path.to_str().unwrap()));
        assert!(!store.root.exists());
        fs::write(&path, b"not a directory").unwrap();
        let inspected = inspect(&store, &config, "managed").await.unwrap();
        assert_eq!(inspected["scope_reason_code"], "not_directory");
        assert_eq!(inspected["files"], json!([]));
    }
    #[test]
    fn current_native_projection_reports_changed_default_separately_from_last_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let target = inspection_target(directory.path().to_owned());
        fs::write(&path, b"{\"defaultModel\":\"first\"}").unwrap();
        let mut record = Record {
            id: "retained".into(),
            owner: "stable".into(),
            target: "managed".into(),
            agent: "pi".into(),
            scope: target.scope.clone(),
            payload_ref: "not-read".into(),
            secret_refs: vec![],
            state: "applied_reload_pending".into(),
            files: vec![FileRecord {
                path: path.clone(),
                format: Format::Json,
                original_absent: false,
                after_revision: "old".into(),
                written: true,
                fields: vec![Field {
                    path: vec!["defaultModel".into()],
                    after_hash: field_hash(Some(&json!("first"))),
                    before_hash: "absent".into(),
                    observed_hash: "absent".into(),
                    released: false,
                }],
            }],
        };
        let files = [File {
            path: path.clone(),
            format: Format::Json,
            desired: json!({"defaultModel":"second"}),
        }];
        let changes = pending_native_changes(&target, &files, &[], Some(&record)).unwrap();
        assert_eq!(
            changes,
            vec![json!({"path":path,"field":["defaultModel"],"change":"set"})]
        );
        assert_eq!(record.state, "applied_reload_pending");
        record.files[0].fields[0].after_hash = field_hash(Some(&json!("second")));
        fs::write(&path, b"{\"defaultModel\":\"second\"}").unwrap();
        assert!(
            pending_native_changes(&target, &files, &[], Some(&record))
                .unwrap()
                .is_empty()
        );
        record.files[0].written = false;
        record.state = "recovery_required".into();
        assert_eq!(
            pending_native_changes(&target, &files, &[], Some(&record)).unwrap()[0]["field"],
            json!(["defaultModel"])
        );
        record.files[0].written = true;
        fs::write(&path, b"{\"defaultModel\":\"first\"}").unwrap();
        assert_eq!(
            pending_native_changes(&target, &files, &[], Some(&record)).unwrap()[0]["change"],
            "set"
        );
        assert_eq!(
            pending_native_changes(&target, &[], &[], Some(&record)).unwrap()[0]["change"],
            "release"
        );
    }
    #[test]
    fn application_versions_are_advisory_except_known_incompatible_schemas() {
        for state in ["probe_failed", "version_unknown", "detected"] {
            let evidence = json!({"state":state,"version":"2.0.19","compatibility":"unknown"});
            assert!(version_blocker("opencode", &evidence).is_none());
            assert_eq!(version_warnings(&evidence).len(), 1);
        }
        assert_eq!(
            version_blocker("opencode", &json!({"version":"1.9.0"})).unwrap()["code"],
            "native_schema_incompatible"
        );
        assert_eq!(
            version_blocker("pi", &json!({"state":"not_installed"})).unwrap()["code"],
            "native_not_installed"
        );
        assert!(version_warnings(&json!({"compatibility":"documented"})).is_empty());
    }
    #[test]
    fn omp_profile_selection_matches_native_default_and_fallback_without_global_environment() {
        use std::ffi::OsStr;
        let root = Path::new("/private/custom-omp");
        let relocated = OsStr::new("/private/relocated");
        assert_eq!(omp_scope(root, None, None, None), root.join("agent"));
        assert_eq!(
            omp_scope(root, Some(OsStr::new("default")), None, None),
            root.join("agent")
        );
        assert_eq!(
            omp_scope(root, None, Some(OsStr::new("work")), Some(relocated)),
            root.join("profiles/work/agent")
        );
        assert_eq!(
            omp_scope(
                root,
                Some(OsStr::new("work")),
                Some(OsStr::new("other")),
                Some(relocated)
            ),
            root.join("profiles/work/agent")
        );
        assert_eq!(
            omp_scope(
                root,
                Some(OsStr::new("default")),
                Some(OsStr::new("work")),
                Some(relocated)
            ),
            PathBuf::from(relocated)
        );
        // Presence, not truthiness, controls whether PI_PROFILE is consulted.
        assert_eq!(
            omp_scope(root, Some(OsStr::new("")), Some(OsStr::new("work")), None),
            root.join("agent")
        );
    }

    #[test]
    fn omp_existing_yaml_settings_remain_effective_and_conflicting_spelling_is_disclosed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        assert_eq!(omp_main_settings(path), "config.yml");
        fs::write(path.join("config.yaml"), "modelRoleStorage: project\n").unwrap();
        assert_eq!(omp_main_settings(path), "config.yaml");
        let files = safe_files("omp", path);
        assert_eq!(
            files
                .iter()
                .find(|f| f["path"] == json!(path.join("config.yaml")))
                .unwrap()["effective"],
            true
        );
        fs::write(path.join("config.yml"), "modelRoleStorage: global\n").unwrap();
        assert_eq!(omp_main_settings(path), "config.yml");
        let files = safe_files("omp", path);
        let shadowed = files
            .iter()
            .find(|f| f["path"] == json!(path.join("config.yaml")))
            .unwrap();
        assert_eq!(shadowed["effective"], false);
        assert_eq!(shadowed["shadowed_by"], json!(path.join("config.yml")));
    }

    async fn auth_fixture() -> (tempfile::TempDir, Store, Config, Logins) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let mut config = Config::empty();
        let target = AgentTarget {
            agent: "codex".into(),
            scope: directory.path().join("native"),
            providers: BTreeMap::new(),
            default: None,
            roles: BTreeMap::new(),
        };
        fs::create_dir(&target.scope).unwrap();
        // Login writers retain the resolved scope and fingerprint, not the
        // spelling originally entered in the target configuration.
        let path = scope(&store, &target.scope).unwrap();
        let record = LoginRecord {
            provider: "p".into(),
            target: "owner".into(),
            scope: path.clone(),
            agent: target.agent.clone(),
            native_provider: "openai".into(),
            credential_id: None,
            session_ref: "native-session-test".into(),
            state: "signed_in".into(),
            source_revision: login_source_revision(&target, &path),
            account: None,
        };
        config.agents.insert("owner".into(), target);
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("codex-subscription".into()),
                auth: Some(Auth::Login {
                    via: "owner".into(),
                    session_ref: Some(record.session_ref.clone()),
                }),
                subscription: Some(config::Subscription {
                    plan: Some("Manual plan".into()),
                    auto_renew: Some(false),
                    ..Default::default()
                }),
                models: BTreeMap::from([("m".into(), config::Model::default())]),
                inputs: BTreeMap::from([(
                    "weekly".into(),
                    config::InputDeclaration {
                        kind: "quota".into(),
                        source: "manual".into(),
                        scope: "weekly".into(),
                        models: vec!["m".into()],
                        max_age_seconds: 3600,
                        refresh_interval_seconds: None,
                    },
                )]),
                ..Default::default()
            },
        );
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        crate::inputs::reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        let now = chrono::Utc::now();
        let request = Request {
            command: vec!["provider".into(), "input".into(), "record".into()],
            operands: vec!["p".into(), "weekly".into()],
            arguments: BTreeMap::from([
                (
                    "observed-at".into(),
                    vec![(now - chrono::Duration::seconds(1)).to_rfc3339()],
                ),
                (
                    "valid-until".into(),
                    vec![(now + chrono::Duration::minutes(10)).to_rfc3339()],
                ),
                (
                    "reset-at".into(),
                    vec![(now + chrono::Duration::hours(1)).to_rfc3339()],
                ),
                ("remaining-percent".into(), vec!["75".into()]),
            ]),
            dry_run: false,
            if_revision: None,
        };
        crate::inputs::handle(&store, &config, &request)
            .await
            .unwrap();
        crate::inputs::activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let logins = Logins {
            entries: vec![record],
        };
        store.write_json(LOGINS, &logins).unwrap();
        (directory, store, config, logins)
    }

    #[tokio::test]
    async fn native_advice_never_treats_retained_session_reference_as_recovered_authorization() {
        let (_directory, store, config, mut logins) = auth_fixture().await;
        let advice = crate::routing::Advice {
            native: Some(compatibility::Context {
                agent: "codex",
                target: Some("owner"),
                provider: Some("p"),
                gateway: false,
            }),
        };
        let before = crate::routing::explain_native(
            &store,
            &config,
            "p/m",
            "responses",
            None,
            Some(&advice),
        )
        .unwrap();
        assert_eq!(before["selection"]["qualified_model"], "p/m");
        logins.entries[0].state = "native_effect_uncertain".into();
        {
            let _configuration = store.lock("configuration").unwrap();
            let _routing = store.lock("routing").unwrap();
            let _native = store.lock("native").unwrap();
            begin_native_auth(&store, "p", &logins).unwrap();
        }
        assert!(matches!(
            &config.providers["p"].auth,
            Some(Auth::Login {
                session_ref: Some(_),
                ..
            })
        ));
        for evidence in [
            crate::routing::explain_native(
                &store,
                &config,
                "p/m",
                "responses",
                None,
                Some(&advice),
            )
            .unwrap(),
            crate::routing::explain_loaded(
                &store,
                &config,
                "p/m",
                "responses",
                None,
                None,
                Some(&advice),
            )
            .unwrap(),
        ] {
            assert!(evidence["selection"].is_null());
            assert_eq!(
                evidence["excluded_candidates"][0]["reason_code"],
                "credential_unavailable"
            );
            assert!(
                evidence["excluded_candidates"][0]["native_requirement"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("uncertain")
            );
        }
    }

    #[tokio::test]
    async fn dropping_native_owner_future_keeps_saved_and_loaded_facts_invalidated() {
        let (directory, store, config, mut logins) = auth_fixture().await;
        let before = crate::inputs::read_runtime(&store).unwrap();
        let at = crate::inputs::ExactInstant::from_chrono(chrono::Utc::now());
        let old_head = crate::inputs::Snapshot::capture(&store, &config).input("p", "weekly", &at);
        assert_eq!(old_head["availability"]["state"], "available");
        assert_eq!(old_head["remaining_percent"], "75");
        let original_config = fs::read(store.root.join("tmax.toml")).unwrap();
        logins.entries[0].state = "native_effect_uncertain".into();
        let mut operation = Box::pin(async {
            let _configuration = store.lock("configuration").unwrap();
            let routing = store.lock("routing").unwrap();
            let _native = store.lock("native").unwrap();
            begin_native_auth(&store, "p", &logins).unwrap();
            drop(routing);
            // Stand in for the owner awaiting user input: cancellation never
            // reaches an explicit Err callback or the normal return path.
            std::future::pending::<()>().await;
        });
        assert!(futures_util::poll!(operation.as_mut()).is_pending());
        drop(operation);
        let reopened = Store::at(directory.path().to_owned());
        let after = crate::inputs::read_runtime(&reopened).unwrap();
        assert_eq!(
            after.saved.providers["p"].identity,
            before.saved.providers["p"].identity
        );
        assert_ne!(
            after.saved.providers["p"].authorization,
            before.saved.providers["p"].authorization
        );
        assert_eq!(
            after.loaded.as_ref().unwrap().providers["p"].authorization,
            after.saved.providers["p"].authorization
        );
        for snapshot in [
            crate::inputs::Snapshot::capture(&reopened, &config),
            crate::inputs::Snapshot::capture_loaded(&reopened, &config, None),
        ] {
            let head = snapshot.input("p", "weekly", &at);
            assert_eq!(
                head["availability"]["reason_code"],
                "authorization_mismatch"
            );
            assert_ne!(head["current_revision"], old_head["current_revision"]);
            assert!(head.get("remaining_percent").is_none());
        }
        assert_eq!(
            reopened.read_json::<Logins>(LOGINS).unwrap().entries[0].state,
            "native_effect_uncertain"
        );
        assert_eq!(
            fs::read(reopened.root.join("tmax.toml")).unwrap(),
            original_config
        );
        assert_eq!(
            serde_json::to_value(&after.saved.config.providers["p"].subscription).unwrap(),
            serde_json::to_value(&config.providers["p"].subscription).unwrap()
        );
    }

    #[tokio::test]
    async fn failed_lineage_persistence_never_records_or_starts_native_effects() {
        let (_directory, store, _config, mut logins) = auth_fixture().await;
        let original_logins = fs::read(store.root.join(LOGINS)).unwrap();
        // Failure occurs after runtime invalidation, during observation pruning.
        fs::create_dir_all(store.root.join("observations/heads.json")).unwrap();
        logins.entries[0].state = "native_effect_uncertain".into();
        let _configuration = store.lock("configuration").unwrap();
        let _routing = store.lock("routing").unwrap();
        let _native = store.lock("native").unwrap();
        let failure = begin_native_auth(&store, "p", &logins).unwrap_err();
        assert_eq!(failure["_code"], "recovery_required");
        assert_eq!(failure["authorization_invalidation"]["state"], "failed");
        assert_eq!(
            failure["authorization_invalidation"]["persistence"],
            "unknown"
        );
        assert_eq!(failure["state"], "native_not_attempted");
        assert_eq!(failure["native_login_attempted"], false);
        assert_eq!(failure["native_authorization_may_have_changed"], false);
        assert_eq!(fs::read(store.root.join(LOGINS)).unwrap(), original_logins);
    }

    #[tokio::test]
    async fn failed_write_ahead_record_distinguishes_completed_invalidation() {
        let (_directory, store, _config, mut logins) = auth_fixture().await;
        let before = crate::inputs::read_runtime(&store).unwrap();
        fs::remove_file(store.root.join(LOGINS)).unwrap();
        fs::create_dir(store.root.join(LOGINS)).unwrap();
        logins.entries[0].state = "native_effect_uncertain".into();
        let _configuration = store.lock("configuration").unwrap();
        let _routing = store.lock("routing").unwrap();
        let _native = store.lock("native").unwrap();
        let failure = begin_native_auth(&store, "p", &logins).unwrap_err();
        assert_eq!(failure["authorization_invalidation"]["state"], "completed");
        assert_eq!(failure["native_record"]["state"], "unknown");
        assert_eq!(failure["state"], "native_not_attempted");
        assert_eq!(failure["native_authorization_may_have_changed"], false);
        assert_ne!(
            crate::inputs::read_runtime(&store).unwrap().saved.providers["p"].authorization,
            before.saved.providers["p"].authorization
        );
    }

    #[tokio::test]
    async fn native_auth_preflight_rejection_preserves_authorization_facts() {
        let (_directory, store, mut config, _logins) = auth_fixture().await;
        let original = config.clone();
        config.providers.insert(
            "other".into(),
            config::Provider {
                integration: Some("codex-subscription".into()),
                auth: Some(Auth::Login {
                    via: "owner".into(),
                    session_ref: None,
                }),
                ..Default::default()
            },
        );
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        crate::inputs::reconcile(&store, &original, &config, &[]).unwrap();
        let runtime_before = fs::read(store.root.join("routing-state.json")).unwrap();
        let logins_before = fs::read(store.root.join(LOGINS)).unwrap();
        for logout in [false, true] {
            let error = native_auth(&store, &config, "p", "owner", false, logout)
                .await
                .unwrap_err();
            assert_eq!(error.code, "native_scope_conflict");
            assert_eq!(
                fs::read(store.root.join("routing-state.json")).unwrap(),
                runtime_before
            );
            assert_eq!(fs::read(store.root.join(LOGINS)).unwrap(), logins_before);
            let at = crate::inputs::ExactInstant::from_chrono(chrono::Utc::now());
            assert_eq!(
                crate::inputs::Snapshot::capture(&store, &config).input("p", "weekly", &at)["availability"]
                    ["state"],
                "available"
            );
        }
    }

    #[tokio::test]
    async fn entries_use_accepted_effective_concrete_metadata_without_native_preference_edits() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().join("tmax"));
        let accepted = crate::catalog::Snapshot {
            revision: "accepted".into(),
            providers: BTreeMap::from([(
                "catalog".into(),
                json!({"models":{"m":{
                    "limit":{"context":1000000,"output":65536},"reasoning":true,"tool_call":true,
                    "modalities":{"input":["text","image","video"],"output":["text"]}
                }}}),
            )]),
            ..Default::default()
        };
        store
            .write_json("catalog/accepted.json", &accepted)
            .unwrap();
        let mut latest = accepted.clone();
        latest.revision = "not-accepted".into();
        latest.providers.get_mut("catalog").unwrap()["models"]["m"]["limit"]["context"] = json!(17);
        store.write_json("catalog/latest.json", &latest).unwrap();
        let mut provider = config::Provider {
            integration: Some("custom".into()),
            metadata: Some("catalog".into()),
            auth: Some(Auth::ApiKey {
                key_ref: "opaque-reference-not-read".into(),
            }),
            endpoints: BTreeMap::from([(
                "chat-completions".into(),
                "https://example.invalid/v1".into(),
            )]),
            ..Default::default()
        };
        let mut model = config::Model::default();
        model.overrides.reasoning = Some(false);
        model.overrides.tool_call = Some(false);
        model.overrides.limit = Some(config::Limits {
            context: None,
            input: None,
            output: Some(4096),
        });
        model.overrides.modalities = Some(config::Modalities {
            input: Some(vec!["image".into()]),
            output: None,
        });
        provider.models = BTreeMap::from([
            ("m".into(), model),
            ("unknown".into(), config::Model::default()),
        ]);
        let mut config = Config::empty();
        config.providers.insert("p".into(), provider);
        let target = AgentTarget {
            agent: "pi".into(),
            scope: temporary.path().join("native"),
            providers: BTreeMap::from([(
                "local".into(),
                config::NativeProvider {
                    provider: Some("p".into()),
                    gateway: None,
                    models: ["m", "unknown"]
                        .into_iter()
                        .map(|id| config::ModelSelection {
                            id: id.into(),
                            protocol: "chat-completions".into(),
                        })
                        .collect(),
                },
            )]),
            default: Some(config::ModelReference {
                provider: "local".into(),
                id: "m".into(),
                protocol: "chat-completions".into(),
            }),
            roles: BTreeMap::new(),
        };
        let (entries, _) = entries(&store, &config, "target", &target, &target.scope, true)
            .await
            .unwrap();
        assert_eq!(
            entries[0].model_metadata["m"],
            json!({"contextWindow":1000000,"maxTokens":4096,"reasoning":false,"input":["image"]})
        );
        assert_eq!(entries[0].model_metadata["unknown"], json!({}));
        let files = adapters::build(&target, &target.scope, &entries).unwrap();
        let models = files
            .iter()
            .find(|file| file.path == target.scope.join("models.json"))
            .unwrap();
        let settings = files
            .iter()
            .find(|file| file.path == target.scope.join("settings.json"))
            .unwrap();
        assert_eq!(
            models.desired["providers"]["local"]["models"][0]["maxTokens"],
            4096
        );
        let original = r#"{"defaultThinkingLevel":"high","modelThinkingLevels":{"local/m":"low"},"unrelated":{"samplingParams":{"temperature":0.2},"maxTokens":777}}"#;
        let mut rewritten = original.to_owned();
        let mut fields = Vec::new();
        leaves(&settings.desired, &mut Vec::new(), &mut fields);
        for (path, value) in fields {
            rewritten = document::set(&rewritten, Format::Json, &path, Some(&value)).unwrap();
        }
        let after = document::parse(&rewritten, Format::Json).unwrap();
        let before = document::parse(original, Format::Json).unwrap();
        for field in ["defaultThinkingLevel", "modelThinkingLevels", "unrelated"] {
            assert_eq!(after[field], before[field]);
        }
        for agent in ["opencode", "hermes"] {
            let mut target = target.clone();
            target.agent = agent.into();
            let (prepared, references) =
                super::entries(&store, &config, "target", &target, &target.scope, true)
                    .await
                    .unwrap();
            assert_eq!(references, ["opaque-reference-not-read"]);
            let files = adapters::build(&target, &target.scope, &prepared).unwrap();
            let file = files
                .iter()
                .find(|file| file.format != Format::Credential)
                .unwrap();
            let native = &file.desired["providers"]["local"]["models"];
            if agent == "opencode" {
                assert_eq!(
                    native["m"]["limit"],
                    json!({"context":1000000,"output":4096})
                );
                assert_eq!(
                    native["m"]["capabilities"],
                    json!({"tools":false,"input":["image"],"output":["text"]})
                );
                assert!(native["m"].get("reasoning").is_none());
                assert!(native["unknown"].get("limit").is_none());
                assert!(native["unknown"].get("capabilities").is_none());
            } else {
                assert_eq!(native["m"], json!({"context_length":1000000}));
                assert_eq!(
                    file.desired["model_overrides"]["local"]["m"],
                    json!({
                        "context_window":1000000,"supports_reasoning":false,"supports_tools":false,"supports_vision":true
                    })
                );
                assert_eq!(native["unknown"], json!({}));
                assert!(
                    file.desired["model_overrides"]["local"]
                        .get("unknown")
                        .is_none()
                );
            }
            // Exercise the same leaf writer as apply, including nested provider models.
            // Existing native request bodies/effort/budgets and unrelated models survive.
            let original = json!({
                "providers":{"local":{"models":{"m":{
                    "settings":{"temperature":0.2,"maxTokens":777},
                    "body":{"max_output_tokens":777,"reasoning":{"effort":"high"}}
                },"unknown":{"body":{"max_tokens":444},"capabilities":{"supports_vision":true}},
                "user-model":{"body":{"temperature":0.8}}}}},
                "agent":{"reasoning_effort":"high"},"reasoning_overrides":{"m":"low"}
            });
            let mut rewritten = if file.format == Format::Yaml {
                serde_yaml::to_string(&original).unwrap()
            } else {
                serde_json::to_string(&original).unwrap()
            };
            let fields = desired_fields(agent, &file.desired, &original);
            for (path, value) in fields {
                rewritten = document::set(&rewritten, file.format, &path, value.as_ref()).unwrap();
            }
            let after = document::parse(&rewritten, file.format).unwrap();
            assert_eq!(
                after["providers"]["local"]["models"]["m"]["body"],
                original["providers"]["local"]["models"]["m"]["body"]
            );
            for field in ["temperature", "maxTokens"] {
                assert_eq!(
                    after["providers"]["local"]["models"]["m"]["settings"][field],
                    original["providers"]["local"]["models"]["m"]["settings"][field]
                );
            }
            assert_eq!(
                after["providers"]["local"]["models"]["user-model"],
                original["providers"]["local"]["models"]["user-model"]
            );
            for field in ["body", "capabilities"] {
                assert_eq!(
                    after["providers"]["local"]["models"]["unknown"][field],
                    original["providers"]["local"]["models"]["unknown"][field]
                );
            }
            for field in ["agent", "reasoning_overrides"] {
                assert_eq!(after[field], original[field]);
            }
        }
    }

    #[test]
    fn hermes_provenance_resolution_never_executes_repair_capable_launcher() {
        let directory = tempfile::tempdir().unwrap();
        let outer = directory.path().join("hermes");
        let inner = directory.path().join("native-launcher");
        let marker = directory.path().join("wrapper-executed");
        let python = directory.path().join("unexecuted-python");
        fs::write(
            &outer,
            format!(
                "#!/bin/sh\ntouch '{}'\nexec {} \"$@\"\n",
                marker.display(),
                inner.display()
            ),
        )
        .unwrap();
        fs::write(&inner,format!("#!/bin/sh\nexec {} -I -c 'import sys\nsys.path.insert(0, '\"'\"'{}'\"'\"')\nimport hermes_bootstrap\n' \"$@\"\n",python.display(),directory.path().display())).unwrap();
        let (resolved_python, source) = hermes_provenance_runtime(&outer).unwrap();
        assert_eq!(resolved_python, python);
        assert_eq!(source, directory.path());
        assert!(!marker.exists());
    }
    #[test]
    fn hermes_provenance_refuses_unqualified_dynamic_launcher_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let launcher = directory.path().join("hermes");
        fs::write(&launcher, "#!/bin/sh\nexec \"$HERMES_PYTHON\" \"$@\"\n").unwrap();
        assert!(hermes_provenance_runtime(&launcher).is_none());
    }
    #[test]
    fn credential_namespace_projection_ignores_all_values() {
        let metadata=br#"{"unrelated":{"type":"oauth","nested":[{"access":"never-materialized"}]},"selected":{"key":"never-materialized"}}"#;
        assert!(auth_namespace_from_reader(metadata.as_slice(), "selected").unwrap());
        assert!(!auth_namespace_from_reader(metadata.as_slice(), "absent").unwrap());
        assert!(!auth_namespace_from_reader(b"{}".as_slice(), "selected").unwrap());
        assert!(
            auth_namespace_from_reader(b"{\"selected\":\"incomplete".as_slice(), "selected")
                .is_err()
        );
    }
    #[test]
    fn installed_version_prefixes_remain_exact() {
        assert_eq!(
            detected_version("omp", "omp/18.12.6\n").as_deref(),
            Some("18.12.6")
        );
        assert_eq!(
            detected_version("codex", "codex-cli 0.162.1\n").as_deref(),
            Some("0.162.1")
        );
        assert_eq!(
            detected_version("hermes", "Hermes v0.22.1+683.g848fc0a\n").as_deref(),
            Some("0.22.1+683.g848fc0a")
        );
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
