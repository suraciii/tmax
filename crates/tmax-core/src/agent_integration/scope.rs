//! Selected native scopes, bounded file access, and filesystem safety.
use super::{
    adapters,
    document::{self, Format},
};
use crate::{
    Error, Result,
    config::{AgentTarget, Config},
    storage::Store,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
};

pub(super) const MAX_NATIVE_BYTES: u64 = 8 * 1024 * 1024;
pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(super) fn revision(text: Option<&str>) -> String {
    text.map(|s| digest(s.as_bytes()))
        .unwrap_or_else(|| "absent".into())
}
pub(super) fn unavailable() -> Error {
    Error::new(
        "native_scope_unavailable",
        "Native scope or configuration is unavailable; inspect the exact selected native path",
    )
}
pub(super) fn scope_evidence(store: &Store, path: &Path) -> Result<(PathBuf, &'static str)> {
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
pub(super) fn scope(store: &Store, path: &Path) -> Result<PathBuf> {
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
pub(super) fn read_native(path: &Path) -> Result<Option<String>> {
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
pub(super) fn target<'a>(config: &'a Config, id: &str) -> Result<&'a AgentTarget> {
    config
        .agents
        .get(id)
        .ok_or_else(|| Error::new("unknown_resource", "Agent target is not configured"))
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
pub(super) fn omp_main_settings(path: &Path) -> &'static str {
    if path.join("config.yml").exists() || !path.join("config.yaml").exists() {
        "config.yml"
    } else {
        "config.yaml"
    }
}
pub(super) fn default_scope(store: &Store, agent: &str) -> PathBuf {
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
        "dsh" => dsh_home(store),
        _ => PathBuf::new(),
    }
}
fn dsh_home(store: &Store) -> PathBuf {
    std::env::var_os("DSH_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| store.home.join(".dsh"))
}
pub(super) struct DshCredentialsLock {
    path: PathBuf,
}
impl DshCredentialsLock {
    pub(super) fn acquire(home: &Path) -> Result<Self> {
        let path = home.join(".credentials.yaml.lock");
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists
                || fs::symlink_metadata(&path).is_ok()
            {
                Error::new(
                    "native_credentials_busy",
                    "DSH credentials have an existing writer lock; retry after its owner releases it",
                )
            } else {
                unavailable()
            }
        })?;
        let guard = Self { path };
        let written = writeln!(file, "{}", std::process::id());
        drop(file);
        written.map_err(|_| unavailable())?;
        Ok(guard)
    }
}
impl Drop for DshCredentialsLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
pub(super) fn dsh_credential_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    if path.exists() {
        use std::os::unix::fs::PermissionsExt;
        if fs::symlink_metadata(path)
            .map_err(|_| unavailable())?
            .permissions()
            .mode()
            & 0o077
            != 0
        {
            return Err(Error::new(
                "unsafe_native_file",
                "DSH credentials require owner-only permissions; no broader credential document was read",
            ));
        }
    }
    Ok(())
}
pub(super) fn dsh_preflight(home: &Path) -> Result<()> {
    let credentials = home.join(".credentials.yaml");
    dsh_credential_permissions(&credentials)?;
    if let Some(text) = read_native(&credentials)? {
        let current = document::parse(&text, Format::Yaml)?;
        adapters::validate_credentials("dsh", &current)?;
    }
    Ok(())
}
pub(super) fn configuration_files(agent: &str, path: &Path) -> Vec<PathBuf> {
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
        "dsh" => &["cordis.patch.yml"],
        _ => &[],
    };
    names.iter().map(|n| path.join(n)).collect()
}
pub(super) fn auth_paths(agent: &str, path: &Path) -> Vec<PathBuf> {
    match agent {
        "codex" | "pi" | "hermes" => vec![path.join("auth.json")],
        "omp" => vec![path.join("auth.json"), path.join("agent.db")],
        "opencode" => vec![path.join("opencode.db")],
        "dsh" => vec![path.join(".credentials.yaml")],
        _ => vec![],
    }
}
pub(super) fn profile_paths(store: &Store, agent: &str) -> Vec<PathBuf> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn dsh_credentials_lock_records_owner_and_releases_on_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(".credentials.yaml.lock");
        let result: Result<()> = (|| {
            let _guard = DshCredentialsLock::acquire(directory.path())?;
            assert_eq!(
                fs::read_to_string(&path).unwrap(),
                format!("{}\n", std::process::id())
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
            assert_eq!(
                DshCredentialsLock::acquire(directory.path())
                    .err()
                    .unwrap()
                    .code,
                "native_credentials_busy"
            );
            assert!(path.exists());
            Err(unavailable())
        })();
        assert!(result.is_err());
        assert!(!path.exists());
        let guard = DshCredentialsLock::acquire(directory.path()).unwrap();
        assert!(path.exists());
        drop(guard);
        assert!(!path.exists());
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
        let files = crate::agent_integration::discovery::safe_files("omp", path);
        assert_eq!(
            files
                .iter()
                .find(|f| f["path"] == json!(path.join("config.yaml")))
                .unwrap()["effective"],
            true
        );
        fs::write(path.join("config.yml"), "modelRoleStorage: global\n").unwrap();
        assert_eq!(omp_main_settings(path), "config.yml");
        let files = crate::agent_integration::discovery::safe_files("omp", path);
        let shadowed = files
            .iter()
            .find(|f| f["path"] == json!(path.join("config.yaml")))
            .unwrap();
        assert_eq!(shadowed["effective"], false);
        assert_eq!(shadowed["shadowed_by"], json!(path.join("config.yml")));
    }
}
