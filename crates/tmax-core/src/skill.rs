use crate::{
    Error, Request, Result,
    operations::arg,
    storage::{self, Store},
};
use serde_json::{Value, json};
use std::{
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
};

const SKILL: &[u8] = include_bytes!("../skills/tmax/SKILL.md");

pub(crate) fn install(store: &Store, request: &Request) -> Result<Value> {
    let agent = arg(request, "agent");
    let root = match agent {
        None => store.home.join(".agents"),
        Some("hermes") => hermes_home(store, std::env::var_os("HERMES_HOME").as_deref())?,
        Some(_) => {
            return Err(
                Error::new("invalid_arguments", "Only --agent hermes is supported").field("agent"),
            );
        }
    };
    install_at(&root, agent.unwrap_or("agents"), request.dry_run)
}

fn hermes_home(store: &Store, configured: Option<&OsStr>) -> Result<PathBuf> {
    let Some(configured) = configured.filter(|value| !value.is_empty()) else {
        let suffix = std::env::var_os("HERMES_DATA_DIR_SUFFIX").unwrap_or_default();
        return hermes_default_home(
            &store.home,
            &suffix,
            std::env::var_os("LOCALAPPDATA").as_deref(),
            cfg!(windows),
        );
    };
    let path = Path::new(configured);
    // Hermes expands shell-style expressions. Require those to be expanded by the
    // caller rather than silently installing into a different, literal directory.
    let text = configured.to_string_lossy();
    if text.contains('$') || (cfg!(windows) && text.contains('%')) {
        return Err(Error::new(
            "invalid_arguments",
            "Expand environment-variable expressions in HERMES_HOME before installing the skill",
        )
        .field("HERMES_HOME"));
    }
    let path = if path.starts_with("~") {
        store
            .home
            .join(path.strip_prefix("~").expect("checked prefix"))
    } else if text.starts_with('~') {
        return Err(Error::new(
            "invalid_arguments",
            "Use ~/ or an expanded user path in HERMES_HOME",
        )
        .field("HERMES_HOME"));
    } else if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| storage::storage_error())?
            .join(path)
    };
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(Error::new(
            "invalid_arguments",
            "HERMES_HOME must not contain parent-directory traversal; provide a resolved path",
        )
        .field("HERMES_HOME"));
    }
    Ok(path)
}

fn hermes_default_home(
    home: &Path,
    suffix: &OsStr,
    local_appdata: Option<&OsStr>,
    windows: bool,
) -> Result<PathBuf> {
    let mut name = std::ffi::OsString::from(if windows { "hermes" } else { ".hermes" });
    name.push(suffix);
    let name_path = Path::new(&name);
    if name_path.components().count() != 1
        || !matches!(name_path.components().next(), Some(Component::Normal(_)))
    {
        return Err(Error::new(
            "invalid_arguments",
            "HERMES_DATA_DIR_SUFFIX must keep the native home within one directory name",
        )
        .field("HERMES_DATA_DIR_SUFFIX"));
    }
    let base = if windows {
        local_appdata
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Local"))
    } else {
        home.to_owned()
    };
    if !base.is_absolute() {
        return Err(Error::new(
            "invalid_arguments",
            "LOCALAPPDATA must be an absolute path for Hermes skill installation",
        )
        .field("LOCALAPPDATA"));
    }
    Ok(base.join(name))
}

fn install_at(root: &Path, agent: &str, dry_run: bool) -> Result<Value> {
    let directory = root.join("skills/tmax");
    let path = directory.join("SKILL.md");
    check_directories(root, &directory)?;
    let existing = storage::read_optional(&path)?;
    let state = match existing {
        Some(bytes) if bytes == SKILL => "already_installed",
        Some(_) => return Err(conflict()),
        None if dry_run => "would_install",
        None => {
            storage::ensure_directory(&directory)?;
            check_directories(root, &directory)?;
            if storage::atomic_create(&path, SKILL)? {
                "installed"
            } else {
                // A concurrent creator wins; only identical bytes are accepted.
                match storage::read_optional(&path)? {
                    Some(bytes) if bytes == SKILL => "already_installed",
                    _ => return Err(conflict()),
                }
            }
        }
    };
    Ok(
        json!({"skill":"tmax", "agent":agent, "path":path, "state":state, "changed":state == "installed"}),
    )
}

fn conflict() -> Error {
    Error::new(
        "inputs_changed",
        "An existing tmax skill has different content and was preserved; move it aside explicitly before installing the bundled skill",
    )
}

fn check_directories(root: &Path, directory: &Path) -> Result<()> {
    storage::check_ancestors(directory)?;
    for path in directory
        .ancestors()
        .take_while(|path| path.starts_with(root))
    {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(storage::storage_error()),
        };
        if !metadata.is_dir() {
            return Err(storage::storage_error());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if metadata.uid() != unsafe { libc::geteuid() }
                || metadata.permissions().mode() & 0o022 != 0
            {
                return Err(storage::storage_error());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_is_idempotent_and_preserves_unrelated_files_and_existing_entry() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".agents");
        let directory = root.join("skills/tmax");
        storage::ensure_directory(&directory).unwrap();
        fs::write(directory.join("notes.txt"), b"user notes").unwrap();
        let first = install_at(&root, "agents", false).unwrap();
        assert_eq!(first["state"], "installed");
        assert_eq!(first["changed"], true);
        let path = directory.join("SKILL.md");
        let before = fs::metadata(&path).unwrap();
        let second = install_at(&root, "agents", false).unwrap();
        assert_eq!(second["state"], "already_installed");
        assert_eq!(second["changed"], false);
        assert_eq!(
            fs::metadata(&path).unwrap().modified().unwrap(),
            before.modified().unwrap()
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().readonly(),
            before.permissions().readonly()
        );
        assert_eq!(
            fs::read(directory.join("notes.txt")).unwrap(),
            b"user notes"
        );
    }

    #[test]
    fn dry_run_creates_nothing_and_existing_custom_content_blocks_both_modes() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".hermes");
        assert_eq!(
            install_at(&root, "hermes", true).unwrap()["state"],
            "would_install"
        );
        assert!(!root.exists());
        let directory = root.join("skills/tmax");
        storage::ensure_directory(&directory).unwrap();
        let path = directory.join("SKILL.md");
        fs::write(&path, b"user-authored skill").unwrap();
        for dry_run in [true, false] {
            assert_eq!(
                install_at(&root, "hermes", dry_run).unwrap_err().code,
                "inputs_changed"
            );
            assert_eq!(fs::read(&path).unwrap(), b"user-authored skill");
        }
    }

    #[test]
    fn hermes_explicit_scope_uses_os_home_or_native_cwd() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_owned());
        assert_eq!(
            hermes_home(&store, Some(OsStr::new("~/profile"))).unwrap(),
            home.path().join("profile")
        );
        assert_eq!(
            hermes_home(&store, Some(OsStr::new("relative-profile"))).unwrap(),
            std::env::current_dir().unwrap().join("relative-profile")
        );
        assert!(hermes_home(&store, Some(OsStr::new("../profile"))).is_err());
        assert!(hermes_home(&store, Some(OsStr::new("$HOME/profile"))).is_err());
    }

    #[test]
    fn hermes_native_defaults_honor_platform_directory_and_suffix() {
        let home = tempfile::tempdir().unwrap();
        let local = home.path().join("local-data");
        let empty = OsStr::new("");
        assert_eq!(
            hermes_default_home(home.path(), empty, None, false).unwrap(),
            home.path().join(".hermes")
        );
        assert_eq!(
            hermes_default_home(
                home.path(),
                OsStr::new("-work"),
                Some(local.as_os_str()),
                false
            )
            .unwrap(),
            home.path().join(".hermes-work")
        );
        assert_eq!(
            hermes_default_home(home.path(), empty, None, true).unwrap(),
            home.path().join("AppData/Local/hermes")
        );
        assert_eq!(
            hermes_default_home(
                home.path(),
                OsStr::new("-work"),
                Some(local.as_os_str()),
                true
            )
            .unwrap(),
            local.join("hermes-work")
        );
        assert_eq!(
            hermes_default_home(home.path(), empty, Some(empty), true).unwrap(),
            home.path().join("AppData/Local/hermes")
        );
        assert!(hermes_default_home(home.path(), OsStr::new("/escape"), None, false).is_err());
        assert!(
            hermes_default_home(home.path(), empty, Some(OsStr::new("relative")), true).is_err()
        );
    }

    #[test]
    fn no_clobber_publication_preserves_existing_entry() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("SKILL.md");
        fs::write(&path, b"concurrent user content").unwrap();
        assert!(!storage::atomic_create(&path, SKILL).unwrap());
        assert_eq!(fs::read(&path).unwrap(), b"concurrent user content");
        assert_eq!(fs::read_dir(home.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_directory_or_skill_and_nonregular_files_are_rejected() {
        use std::os::unix::fs::symlink;
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = home.path().join(".agents");
        symlink(outside.path(), &root).unwrap();
        assert!(install_at(&root, "agents", false).is_err());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
        fs::remove_file(&root).unwrap();
        let directory = root.join("skills/tmax");
        storage::ensure_directory(&directory).unwrap();
        let outside_file = outside.path().join("user.txt");
        fs::write(&outside_file, b"preserve").unwrap();
        let path = directory.join("SKILL.md");
        symlink(&outside_file, &path).unwrap();
        assert!(install_at(&root, "agents", false).is_err());
        assert_eq!(fs::read(&outside_file).unwrap(), b"preserve");
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(install_at(&root, "agents", true).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn writable_by_other_users_skill_directory_is_not_modified() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        let root = home.path().join(".agents");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o777)).unwrap();
        for dry_run in [true, false] {
            assert!(install_at(&root, "agents", dry_run).is_err());
            assert!(!root.join("skills").exists());
        }
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o777
        );
    }
}
