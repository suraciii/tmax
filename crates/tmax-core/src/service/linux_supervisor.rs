//! systemd user-manager supervision of the foreground daemon.
//!
//! The manager's own unit search path, rather than the invoking shell's XDG
//! environment, determines both the definition and the login activation link.
#[path = "systemd_peer.rs"]
mod systemd_peer;

use self::systemd_peer::{Peer, Unit};
use super::super::failure;
use super::{
    Manager, Observation, Preparation, RESTART_DELAY_SECONDS, RESTART_LIMIT,
    RESTART_WINDOW_SECONDS, Snapshot,
};
use crate::{Result, storage::Store};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    ffi::{CStr, CString},
    fs,
    io::{self, Read},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, symlink},
    },
    path::{Component, Path, PathBuf},
};

const UNIT_NAME: &CStr = c"tmax.service";
const UNIT: &str = match std::str::from_utf8(UNIT_NAME.to_bytes()) {
    Ok(name) => name,
    Err(_) => panic!("the tmax unit name is not UTF-8"),
};

fn unavailable(message: &str) -> crate::Error {
    failure("service_manager_unavailable", message)
}
fn conflict() -> crate::Error {
    failure(
        "service_registration_conflict",
        "The systemd unit, loaded command, or login link does not match the tmax-owned registration",
    )
}
fn control_failed() -> crate::Error {
    failure(
        "service_control_failed",
        "The systemd user manager could not complete the requested operation; inspect its state before retrying",
    )
}

fn nearest_owner(path: &Path) -> Option<u32> {
    for part in path.ancestors() {
        match fs::symlink_metadata(part) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => return Some(m.uid()),
            Ok(_) => return None,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return None,
        }
    }
    None
}

struct Paths {
    unit: PathBuf,
    legacy: PathBuf,
    search: Vec<PathBuf>,
}
fn paths(store: &Store, search: Vec<String>) -> Result<Paths> {
    let search: Vec<_> = search.into_iter().map(PathBuf::from).collect();
    if search.is_empty()
        || search
            .iter()
            .any(|p| !p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)))
    {
        return Err(unavailable("The systemd unit search path is invalid"));
    }
    // UnitPath is ordered by the manager itself. Select its first persistent,
    // user-owned configuration directory, not a runtime or vendor directory.
    let uid = unsafe { libc::geteuid() };
    let directory = search
        .iter()
        .find(|p| {
            p.ends_with("systemd/user")
                && !p.starts_with("/run")
                && !p.starts_with("/usr")
                && !p.starts_with("/etc")
                && nearest_owner(p) == Some(uid)
        })
        .ok_or_else(|| {
            unavailable("The systemd manager has no owned persistent user unit directory")
        })?;
    Ok(Paths {
        unit: directory.join(UNIT),
        legacy: store.home.join(".config/systemd/user").join(UNIT),
        search,
    })
}

fn marker(store: &Store) -> Result<Vec<u8>> {
    let root = store
        .root
        .to_str()
        .ok_or_else(|| unavailable("The tmax authority cannot be registered with systemd"))?;
    if root.contains(['\n', '\r']) {
        return Err(unavailable(
            "The tmax authority cannot be registered with systemd",
        ));
    }
    Ok(format!("# tmax-owned authority={root}\n").into_bytes())
}
fn unit_data_for(store: &Store, exe: &Path) -> Result<Vec<u8>> {
    let exe = exe
        .to_str()
        .ok_or_else(|| unavailable("The tmax executable path cannot be registered with systemd"))?;
    if exe.contains(['\n', '\r', '%', '"', '\\']) {
        return Err(unavailable(
            "The tmax executable path cannot be registered with systemd",
        ));
    }
    let mut data = marker(store)?;
    data.extend_from_slice(format!("[Unit]\nDescription=tmax personal model gateway\nStartLimitIntervalSec={RESTART_WINDOW_SECONDS}\nStartLimitBurst={RESTART_LIMIT}\n[Service]\nType=simple\nExecStart=\"{exe}\" __service\nRestart=on-failure\nRestartSec={RESTART_DELAY_SECONDS}\nRestartPreventExitStatus=78\n[Install]\nWantedBy=default.target\n").as_bytes());
    Ok(data)
}

fn unit_data(store: &Store) -> Result<Vec<u8>> {
    let exe = std::env::current_exe().map_err(|_| executable_unavailable())?;
    unit_data_for(store, &exe)
}

fn executable_unavailable() -> crate::Error {
    failure(
        "service_executable_unavailable",
        "The intended service executable cannot be verified on disk; restore the installed binary before restarting",
    )
}

// The marker alone does not prove that the service command or policy is ours.
fn definition_executable(store: &Store, data: &[u8]) -> Result<PathBuf> {
    let text = std::str::from_utf8(data).map_err(|_| conflict())?;
    for line in text.lines() {
        let Some(path) = line
            .strip_prefix("ExecStart=\"")
            .and_then(|v| v.strip_suffix("\" __service"))
        else {
            continue;
        };
        let path = Path::new(path);
        if path.is_absolute() && unit_data_for(store, path).ok().as_deref() == Some(data) {
            return Ok(path.to_path_buf());
        }
    }
    Err(conflict())
}

// Only an exact definition generated for this authority can nominate a path.
// Compare the kernel's entire deleted-inode name against that path plus the
// kernel suffix; never derive a candidate by stripping an untrusted name.
fn select_executable(
    store: &Store,
    current: &Path,
    running: &fs::Metadata,
    definitions: &[Option<&[u8]>],
) -> Result<PathBuf> {
    if fs::metadata(current).is_ok_and(|m| m.dev() == running.dev() && m.ino() == running.ino()) {
        return Ok(current.to_path_buf());
    }
    for data in definitions.iter().flatten() {
        let Ok(path) = definition_executable(store, data) else {
            continue;
        };
        let mut deleted = path.as_os_str().to_os_string();
        deleted.push(" (deleted)");
        if current.as_os_str() == deleted {
            return Ok(path);
        }
    }
    Err(executable_unavailable())
}

fn prepare_executable(store: &Store, registration: &Registration) -> Result<PathBuf> {
    let current = std::env::current_exe().map_err(|_| executable_unavailable())?;
    let running = fs::metadata("/proc/self/exe").map_err(|_| executable_unavailable())?;
    let path = select_executable(
        store,
        &current,
        &running,
        &[
            registration.current.as_deref(),
            registration.legacy.as_deref(),
        ],
    )?;
    validate_executable(&path)?;
    Ok(path)
}

fn validate_executable(path: &Path) -> Result<()> {
    let mut file = fs::File::open(path).map_err(|_| executable_unavailable())?;
    let metadata = file.metadata().map_err(|_| executable_unavailable())?;
    let name = CString::new(path.as_os_str().as_bytes()).map_err(|_| executable_unavailable())?;
    if !metadata.is_file()
        || metadata.mode() & 0o111 == 0
        || unsafe { libc::faccessat(libc::AT_FDCWD, name.as_ptr(), libc::X_OK, libc::AT_EACCESS) }
            != 0
    {
        return Err(executable_unavailable());
    }
    // A replacement may have a different inode and version, but must still be
    // a native executable for the same ELF class, byte order and machine.
    let mut replacement = [0u8; 20];
    let mut original = [0u8; 20];
    file.read_exact(&mut replacement)
        .map_err(|_| executable_unavailable())?;
    fs::File::open("/proc/self/exe")
        .and_then(|mut f| f.read_exact(&mut original))
        .map_err(|_| executable_unavailable())?;
    if &replacement[..4] != b"\x7fELF"
        || replacement[..7] != original[..7]
        || replacement[18..20] != original[18..20]
        || !matches!(
            if replacement[5] == 1 {
                u16::from_le_bytes([replacement[16], replacement[17]])
            } else {
                u16::from_be_bytes([replacement[16], replacement[17]])
            },
            2 | 3
        )
    {
        return Err(executable_unavailable());
    }
    Ok(())
}
fn read_unit(path: &Path, store: &Store) -> Result<Option<Vec<u8>>> {
    let expected = marker(store)?;
    let value = crate::storage::read_optional(path).map_err(|_| conflict())?;
    if value.as_ref().is_some_and(|b| !b.starts_with(&expected)) {
        return Err(conflict());
    }
    Ok(value)
}
// An old-path entry outside the manager's search path is unrelated unless its
// marker proves ownership. Never change a foreign or unreadable entry there.
fn legacy_unit(store: &Store, p: &Paths) -> Result<Option<Vec<u8>>> {
    if p.legacy == p.unit {
        return Ok(None);
    }
    if p.search.iter().any(|dir| dir.join(UNIT) == p.legacy) {
        return read_unit(&p.legacy, store);
    }
    let expected = marker(store)?;
    match crate::storage::read_optional(&p.legacy) {
        Ok(Some(data)) if data.starts_with(&expected) => Ok(Some(data)),
        _ => Ok(None),
    }
}
fn link_path(unit: &Path) -> PathBuf {
    unit.parent()
        .expect("unit has parent")
        .join("default.target.wants")
        .join(UNIT)
}
fn normalized(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}
fn link_owned(link: &Path, unit: &Path) -> Result<bool> {
    crate::storage::check_ancestors(link.parent().expect("link has parent"))
        .map_err(|_| conflict())?;
    match fs::symlink_metadata(link) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Ok(m) if m.file_type().is_symlink() => {
            let target = fs::read_link(link).map_err(|_| conflict())?;
            let target = if target.is_absolute() {
                target
            } else {
                link.parent().expect("link has parent").join(target)
            };
            if normalized(&target) == normalized(unit) {
                Ok(true)
            } else {
                Err(conflict())
            }
        }
        _ => Err(conflict()),
    }
}

struct Registration {
    current: Option<Vec<u8>>,
    legacy: Option<Vec<u8>>,
    current_link: bool,
    legacy_link: bool,
}
impl Registration {
    fn installed(&self) -> bool {
        self.current.is_some() || self.legacy.is_some()
    }

    fn enabled(&self) -> bool {
        self.current_link || self.legacy_link
    }
}

// Detect competing definitions or activation links throughout the manager's
// search path. Only the former adapter's exact marker proves legacy ownership.
fn preflight(store: &Store, p: &Paths) -> Result<Registration> {
    let current = read_unit(&p.unit, store)?;
    let legacy = legacy_unit(store, p)?;
    let current_link = link_owned(&link_path(&p.unit), &p.unit)?;
    let legacy_link = (legacy.is_some()
        || p.legacy != p.unit && p.search.iter().any(|dir| dir.join(UNIT) == p.legacy))
        && link_owned(&link_path(&p.legacy), &p.legacy)?;
    if current_link && current.is_none() || legacy_link && legacy.is_none() {
        return Err(conflict());
    }
    // Drop-ins added since the last daemon-reload are not in DropInPaths yet.
    // Check both unit-specific and type-wide directories before any mutation.
    for dir in &p.search {
        for name in ["tmax.service.d", "service.d"] {
            match fs::read_dir(dir.join(name)) {
                Ok(entries) => {
                    for entry in entries {
                        let entry = entry.map_err(|_| conflict())?;
                        if Path::new(&entry.file_name())
                            .extension()
                            .is_some_and(|ext| ext == "conf")
                        {
                            return Err(conflict());
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(_) => return Err(conflict()),
            }
        }
    }
    for dir in &p.search {
        let candidate = dir.join(UNIT);
        if candidate == p.unit || candidate == p.legacy {
            continue;
        }
        match fs::symlink_metadata(&candidate) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            _ => return Err(conflict()),
        }
        match fs::symlink_metadata(link_path(&candidate)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            _ => return Err(conflict()),
        }
    }
    Ok(Registration {
        current,
        legacy,
        current_link,
        legacy_link,
    })
}
fn set_link(unit: &Path, enabled: bool) -> Result<bool> {
    let link = link_path(unit);
    if link_owned(&link, unit)? == enabled {
        return Ok(false);
    }
    if enabled {
        crate::storage::ensure_directory(link.parent().expect("link has parent"))?;
        symlink(unit, &link).map_err(|_| control_failed())?;
    } else {
        fs::remove_file(link).map_err(|_| control_failed())?;
    }
    Ok(true)
}

fn check_loaded(
    store: &Store,
    p: &Paths,
    registration: &Registration,
    unit: &Unit,
) -> Result<Option<PathBuf>> {
    if !unit.drop_in_paths.is_empty() {
        return Err(conflict());
    }
    if unit.load_state == "not-found"
        && unit.fragment_path.is_empty()
        && unit.exec_start.is_empty()
        && unit.idle()
    {
        return Ok(None);
    }
    if unit.load_state != "loaded" {
        return Err(conflict());
    }
    let fragment = Path::new(&unit.fragment_path);
    let definition = if fragment == p.unit {
        registration.current.as_deref()
    } else if fragment == p.legacy {
        registration.legacy.as_deref()
    } else {
        None
    }
    .ok_or_else(conflict)?;
    let executable = definition_executable(store, definition)?;
    let [command] = unit.exec_start.as_slice() else {
        return Err(conflict());
    };
    let expected = executable.to_str().ok_or_else(conflict)?;
    if command.0 != expected || command.1 != [expected, "__service"] || command.2 {
        return Err(conflict());
    }
    Ok(Some(executable))
}

struct QualifiedUnit {
    paths: Paths,
    registration: Registration,
    unit: Unit,
    executable: Option<PathBuf>,
}
impl QualifiedUnit {
    fn read(store: &Store, peer: &Peer) -> Result<Self> {
        let paths = paths(store, peer.unit_paths()?)?;
        let registration = preflight(store, &paths)?;
        let unit = peer.unit()?;
        let executable = check_loaded(store, &paths, &registration, &unit)?;
        Ok(Self {
            paths,
            registration,
            unit,
            executable,
        })
    }

    fn observation(&self) -> Observation {
        let unit = &self.unit;
        let installed = self.registration.installed();
        let observed = self.executable.is_some();
        let active = unit.active_state.as_str();
        let pid = (observed && unit.main_pid != 0 && matches!(active, "active" | "activating"))
            .then_some(unit.main_pid);
        let exit =
            (observed && unit.exec_main_code == libc::CLD_EXITED).then_some(unit.exec_main_status);
        let (state, reason) = match (active, unit.sub_state.as_str()) {
            ("active", _) if pid.is_some() => ("running", None),
            ("activating", "auto-restart" | "auto-restart-queued") => ("recovering", None),
            ("activating", _) => ("starting", None),
            (_, _) if unit.has_job() => ("starting", None),
            ("failed", _) if unit.result == "start-limit-hit" => {
                ("failed", Some("retry_limit_exhausted"))
            }
            ("failed", _) if exit == Some(78) => ("blocked", Some("permanent_startup_failure")),
            ("failed", _) => ("failed", Some("service_process_failed")),
            ("inactive", _) => ("stopped", None),
            _ => ("unavailable", None),
        };
        Observation {
            registration: if installed { "owned" } else { "absent" },
            state,
            pid,
            attempts: observed.then_some(unit.n_restarts),
            last_exit_code: exit,
            reason_code: reason.map(str::to_owned),
            retry_delay_seconds: RESTART_DELAY_SECONDS,
        }
    }

    fn login_status(&self, store: &Store) -> Result<Value> {
        let current = self.registration.current.as_ref();
        let matches = current == Some(&unit_data(store)?);
        let registration = if !self.registration.installed() {
            "absent"
        } else if matches && self.registration.legacy.is_none() {
            "present"
        } else {
            "drifted"
        };
        let enabled = self.registration.enabled();
        Ok(
            json!({"registration":registration,"unit_matches":matches,"login_enabled":enabled,"login_link":if enabled {"owned"} else {"absent"}}),
        )
    }
}

fn desired_definition<'a>(
    store: &Store,
    registration: &'a Registration,
    preparation: Preparation,
) -> Result<Cow<'a, [u8]>> {
    if preparation == Preparation::Preserve
        && let Some(data) = registration
            .current
            .as_deref()
            .or(registration.legacy.as_deref())
    {
        validate_executable(&definition_executable(store, data)?)?;
        return Ok(Cow::Borrowed(data));
    }
    unit_data_for(store, &prepare_executable(store, registration)?).map(Cow::Owned)
}

fn reconcile_files<'a>(
    store: &Store,
    snapshot: &'a QualifiedUnit,
    preparation: Preparation,
    login: Option<bool>,
) -> Result<(Option<&'a Path>, bool)> {
    let QualifiedUnit {
        paths: p,
        registration,
        unit,
        ..
    } = snapshot;
    let mut changed = false;
    let mut migrate = false;
    let fragment = Path::new(&unit.fragment_path);
    let target = if login == Some(false) {
        None
    } else if unit.idle() {
        let expected = desired_definition(store, registration, preparation)?;
        if registration.current.as_deref() != Some(expected.as_ref()) {
            crate::storage::atomic_write(&p.unit, &expected)?;
            changed = true;
        }
        migrate = registration.legacy.is_some();
        Some(p.unit.as_path())
    } else {
        let executable = snapshot.executable.as_deref().ok_or_else(conflict)?;
        if preparation == Preparation::Replace {
            prepare_executable(store, registration)?;
        } else {
            validate_executable(executable)?;
        }
        if login.is_none() {
            return Ok((Some(fragment), false));
        }
        Some(fragment)
    };

    let enabled = login.unwrap_or_else(|| registration.enabled());
    if target == Some(p.unit.as_path()) || registration.current.is_some() {
        changed |= set_link(&p.unit, enabled && target == Some(p.unit.as_path()))?;
    }
    if registration.legacy.is_some() {
        changed |= set_link(
            &p.legacy,
            enabled && !migrate && target == Some(p.legacy.as_path()),
        )?;
        if migrate {
            // Never remove a replaced definition or an unrelated legacy file.
            if let Some(data) = read_unit(&p.legacy, store)? {
                definition_executable(store, &data)?;
                fs::remove_file(&p.legacy).map_err(|_| control_failed())?;
                changed = true;
            }
        }
    }
    let needs_reload = changed
        || unit.need_daemon_reload && (target.is_some() || registration.installed())
        || target.is_some_and(|path| fragment != path || snapshot.executable.is_none());
    Ok((target, needs_reload))
}

fn reconcile(store: &Store, preparation: Preparation, login: Option<bool>) -> Result<()> {
    let (peer, before) = snapshot(store)?;
    let (target, needs_reload) = reconcile_files(store, &before, preparation, login)?;
    if needs_reload {
        peer.control(&["daemon-reload"])?;
        let after = QualifiedUnit::read(store, &peer)?;
        if target.is_some_and(|target| {
            after.executable.is_none() || Path::new(&after.unit.fragment_path) != target
        }) || login.is_some_and(|enabled| after.registration.enabled() != enabled)
        {
            return Err(control_failed());
        }
    }
    Ok(())
}

fn snapshot(store: &Store) -> Result<(Peer, QualifiedUnit)> {
    let peer = Peer::connect()?;
    let unit = QualifiedUnit::read(store, &peer)?;
    Ok((peer, unit))
}

fn observation(peer: Peer, unit: &QualifiedUnit) -> Snapshot {
    Snapshot {
        manager: Manager {
            name: "systemd_user",
            activation: peer.activation,
            survives_logout: None,
            session_scope: "user_manager",
        },
        observed: unit.observation(),
    }
}

// A native bus handle and all of its replies remain on this worker.
async fn blocking<T: Send + 'static>(
    store: &Store,
    operation: impl FnOnce(&Store) -> Result<T> + Send + 'static,
) -> Result<T> {
    let store = store.clone();
    tokio::task::spawn_blocking(move || operation(&store))
        .await
        .map_err(|_| control_failed())?
}

pub(super) async fn observe(store: &Store) -> Result<Snapshot> {
    blocking(store, |store| {
        let (peer, unit) = snapshot(store)?;
        Ok(observation(peer, &unit))
    })
    .await
}

pub(super) async fn observe_status(store: &Store) -> (Result<Snapshot>, Result<Value>) {
    let observed = blocking(store, |store| {
        let (peer, unit) = snapshot(store)?;
        Ok((observation(peer, &unit), unit.login_status(store)))
    })
    .await;
    match observed {
        Ok((snapshot, registration)) => (Ok(snapshot), registration),
        Err(error) => {
            let registration = if error.code == "service_registration_conflict" {
                "conflict"
            } else {
                "unavailable"
            };
            (
                Err(error),
                Ok(json!({"registration":registration,"unit_matches":false,"login_enabled":null})),
            )
        }
    }
}

pub(super) async fn prepare(store: &Store, preparation: Preparation) -> Result<()> {
    blocking(store, move |store| reconcile(store, preparation, None)).await
}

pub(super) async fn start(store: &Store) -> Result<()> {
    blocking(store, |store| {
        let (peer, observed) = snapshot(store)?;
        validate_executable(observed.executable.as_deref().ok_or_else(control_failed)?)?;
        if observed.unit.active_state == "failed" || observed.unit.result == "start-limit-hit" {
            peer.control(&["reset-failed", UNIT])?;
        }
        peer.control(&["start", UNIT])
    })
    .await
}

pub(super) async fn stop(store: &Store) -> Result<()> {
    blocking(store, |store| {
        let (peer, observed) = snapshot(store)?;
        if !observed.registration.installed() {
            return Ok(());
        }
        peer.control(&["stop", UNIT])?;
        if !QualifiedUnit::read(store, &peer)?.unit.idle() {
            return Err(control_failed());
        }
        Ok(())
    })
    .await
}

pub(super) async fn login(store: &Store, enabled: bool) -> Result<()> {
    blocking(store, move |store| {
        reconcile(store, Preparation::Preserve, Some(enabled))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_uses_the_private_manager_socket() {
        use std::os::{
            fd::AsRawFd,
            unix::net::{UnixListener, UnixStream},
        };

        let runtime = tempfile::tempdir().unwrap();
        let path = runtime.path().join("private");
        let _listener = UnixListener::bind(&path).unwrap();
        let socket = UnixStream::connect(&path).unwrap();
        let uid = unsafe { libc::geteuid() };
        assert_eq!(
            systemd_peer::peer_pid(socket.as_raw_fd(), uid).unwrap(),
            std::process::id()
        );
        assert_eq!(
            systemd_peer::peer_pid(socket.as_raw_fd(), uid.wrapping_add(1))
                .unwrap_err()
                .code,
            "service_manager_unavailable"
        );
    }

    fn loaded_unit(fragment: &Path, executable: &Path) -> Unit {
        let exe = executable.to_str().unwrap();
        Unit {
            load_state: "loaded".into(),
            fragment_path: fragment.to_str().unwrap().into(),
            drop_in_paths: vec![],
            need_daemon_reload: false,
            active_state: "inactive".into(),
            sub_state: "dead".into(),
            main_pid: 0,
            n_restarts: 0,
            exec_main_code: 0,
            exec_main_status: 0,
            result: "success".into(),
            job_pending: false,
            exec_start: vec![(exe.into(), vec![exe.into(), "__service".into()], false)],
        }
    }

    fn fixture(store: &Store, executable: &Path) -> QualifiedUnit {
        let fragment = store.home.join(UNIT);
        fs::write(&fragment, unit_data_for(store, executable).unwrap()).unwrap();
        let paths = Paths {
            unit: fragment.clone(),
            legacy: fragment.clone(),
            search: vec![store.home.clone()],
        };
        let registration = preflight(store, &paths).unwrap();
        let unit = loaded_unit(&fragment, executable);
        let executable = check_loaded(store, &paths, &registration, &unit).unwrap();
        QualifiedUnit {
            paths,
            registration,
            unit,
            executable,
        }
    }

    #[test]
    fn loaded_command_must_match_the_exact_owned_argv() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_path_buf());
        let executable = Path::new("/home/a user/bin/tmax");
        let mut snapshot = fixture(&store, executable);
        let expected = check_loaded(
            &store,
            &snapshot.paths,
            &snapshot.registration,
            &snapshot.unit,
        )
        .unwrap();
        assert_eq!(expected.as_deref(), Some(executable));

        // An argv with one argument can render exactly like two arguments in systemctl show.
        snapshot.unit.exec_start[0].1 = vec![format!("{} __service", executable.display())];
        assert_eq!(
            check_loaded(
                &store,
                &snapshot.paths,
                &snapshot.registration,
                &snapshot.unit
            )
            .unwrap_err()
            .code,
            "service_registration_conflict"
        );
        snapshot.unit = loaded_unit(&snapshot.paths.unit, Path::new("/unrelated/tmax"));
        assert_eq!(
            check_loaded(
                &store,
                &snapshot.paths,
                &snapshot.registration,
                &snapshot.unit
            )
            .unwrap_err()
            .code,
            "service_registration_conflict"
        );
    }

    #[test]
    fn retry_wait_is_not_a_terminal_failure() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_path_buf());
        let mut observed = fixture(&store, Path::new("/proc/self/exe"));
        observed.unit.active_state = "failed".into();
        observed.unit.result = "exit-code".into();
        observed.unit.exec_main_code = libc::CLD_EXITED;
        observed.unit.exec_main_status = 78;
        assert_eq!(observed.observation().state, "blocked");
        observed.unit.job_pending = true;
        assert_eq!(observed.observation().state, "starting");
        observed.unit.active_state = "activating".into();
        observed.unit.sub_state = "auto-restart".into();
        assert_eq!(observed.observation().state, "recovering");
    }

    #[test]
    fn login_link_reconciliation_requests_reload_only_when_needed() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_path_buf());
        let mut snapshot = fixture(&store, Path::new("/proc/self/exe"));
        let enabled = |snapshot: &QualifiedUnit, enable| {
            reconcile_files(&store, snapshot, Preparation::Preserve, Some(enable))
                .unwrap()
                .1
        };
        assert!(enabled(&snapshot, true));
        snapshot.registration = preflight(&store, &snapshot.paths).unwrap();
        assert!(snapshot.registration.enabled());
        assert!(!enabled(&snapshot, true));

        snapshot.unit.need_daemon_reload = true;
        assert!(enabled(&snapshot, true));
        snapshot.unit.need_daemon_reload = false;
        assert!(enabled(&snapshot, false));
        snapshot.registration = preflight(&store, &snapshot.paths).unwrap();
        assert!(!snapshot.registration.enabled());
        assert!(!enabled(&snapshot, false));
        assert!(snapshot.paths.unit.is_file());
    }

    #[test]
    fn loaded_and_pending_drop_ins_are_registration_conflicts() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_path_buf());
        let mut snapshot = fixture(&store, Path::new("/proc/self/exe"));
        snapshot
            .unit
            .drop_in_paths
            .push("/home/user/.config/systemd/user/tmax.service.d/override.conf".into());
        assert_eq!(
            check_loaded(
                &store,
                &snapshot.paths,
                &snapshot.registration,
                &snapshot.unit
            )
            .unwrap_err()
            .code,
            "service_registration_conflict"
        );

        let drop_ins = home.path().join("tmax.service.d");
        fs::create_dir(&drop_ins).unwrap();
        fs::write(
            drop_ins.join("override.conf"),
            b"[Service]\nExecStartPre=/unrelated\n",
        )
        .unwrap();
        assert_eq!(
            preflight(&store, &snapshot.paths).err().unwrap().code,
            "service_registration_conflict"
        );
    }

    #[test]
    fn preparation_preserves_existing_executable_until_explicit_stopped_replacement() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_path_buf());
        let first = home.path().join("first");
        symlink("/proc/self/exe", &first).unwrap();
        let mut snapshot = fixture(&store, &first);
        let original = fs::read(&snapshot.paths.unit).unwrap();
        snapshot.unit.active_state = "active".into();
        snapshot.unit.main_pid = std::process::id();
        assert!(
            !reconcile_files(&store, &snapshot, Preparation::Replace, None)
                .unwrap()
                .1
        );
        assert_eq!(fs::read(&snapshot.paths.unit).unwrap(), original);

        snapshot.unit.active_state = "inactive".into();
        snapshot.unit.main_pid = 0;
        assert!(
            !reconcile_files(&store, &snapshot, Preparation::Preserve, None)
                .unwrap()
                .1
        );
        assert_eq!(fs::read(&snapshot.paths.unit).unwrap(), original);

        assert!(
            reconcile_files(&store, &snapshot, Preparation::Replace, None)
                .unwrap()
                .1
        );
        assert_eq!(
            fs::read(&snapshot.paths.unit).unwrap(),
            unit_data(&store).unwrap()
        );
    }
    #[test]
    fn deleted_executable_uses_only_exact_owned_definition_and_requires_replacement() {
        use std::os::unix::fs::PermissionsExt;

        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_path_buf());
        let executable = home.path().join("tmax");
        fs::copy("/proc/self/exe", &executable).unwrap();
        let running = fs::metadata(&executable).unwrap();
        let definition = unit_data_for(&store, &executable).unwrap();
        fs::rename(&executable, home.path().join("old")).unwrap();
        let mut deleted = executable.as_os_str().to_os_string();
        deleted.push(" (deleted)");
        let deleted = PathBuf::from(deleted);
        let selected = select_executable(&store, &deleted, &running, &[Some(&definition)]).unwrap();
        assert_eq!(selected, executable);
        assert_eq!(
            validate_executable(&selected).unwrap_err().code,
            "service_executable_unavailable"
        );
        fs::copy("/proc/self/exe", &executable).unwrap();
        assert!(validate_executable(&selected).is_ok());
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(validate_executable(&selected).is_err());
        let other = Store::at(home.path().join("other"));
        assert!(select_executable(&other, &deleted, &running, &[Some(&definition)]).is_err());
        let mut altered = definition;
        altered.extend_from_slice(b"ExecStart=/unrelated\n");
        assert!(select_executable(&store, &deleted, &running, &[Some(&altered)]).is_err());
        assert!(
            select_executable(
                &store,
                Path::new("/unrelated (deleted)"),
                &running,
                &[Some(&unit_data_for(&store, &executable).unwrap())]
            )
            .is_err()
        );
    }

    #[test]
    fn links_are_independent_and_foreign_links_are_preserved() {
        let home = tempfile::tempdir().unwrap();
        let unit = home.path().join("systemd/user/tmax.service");
        fs::create_dir_all(unit.parent().unwrap()).unwrap();
        set_link(&unit, true).unwrap();
        assert!(link_owned(&link_path(&unit), &unit).unwrap());
        set_link(&unit, false).unwrap();
        symlink("/unrelated", link_path(&unit)).unwrap();
        assert!(set_link(&unit, false).is_err());
        assert_eq!(
            fs::read_link(link_path(&unit)).unwrap(),
            Path::new("/unrelated")
        );
    }
    #[test]
    fn legacy_marker_is_authority_specific() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_path_buf());
        let path = home.path().join("tmax.service");
        fs::write(&path, marker(&store).unwrap()).unwrap();
        assert!(read_unit(&path, &store).unwrap().is_some());
        fs::write(&path, b"# tmax-owned authority=/other\n").unwrap();
        assert_eq!(
            read_unit(&path, &store).unwrap_err().code,
            "service_registration_conflict"
        );
    }
    #[test]
    fn manager_unit_paths_preserve_spaces_and_reject_relative_paths() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_path_buf());
        let directory = home.path().join("a user/systemd/user");
        fs::create_dir_all(&directory).unwrap();
        let search = vec![
            "/run/user/1000/systemd/user".into(),
            directory.to_str().unwrap().into(),
        ];
        assert_eq!(paths(&store, search).unwrap().unit, directory.join(UNIT));
        assert!(paths(&store, vec!["relative/systemd/user".into()]).is_err());
    }
    #[test]
    fn unrelated_legacy_outside_manager_path_is_ignored() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_path_buf());
        let legacy = home.path().join(".config/systemd/user/tmax.service");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, b"foreign service").unwrap();
        let p = Paths {
            unit: home.path().join("other/systemd/user/tmax.service"),
            legacy: legacy.clone(),
            search: vec![home.path().join("other/systemd/user")],
        };
        assert!(legacy_unit(&store, &p).unwrap().is_none());
        assert_eq!(fs::read(legacy).unwrap(), b"foreign service");
    }
}
