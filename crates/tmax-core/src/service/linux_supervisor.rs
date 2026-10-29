//! systemd user-manager supervision of the foreground daemon.
//!
//! The manager's own unit search path, rather than the invoking shell's XDG
//! environment, determines both the definition and the login activation link.
use super::super::failure;
use super::{
    Manager, Observation, Preparation, RESTART_DELAY_SECONDS, RESTART_LIMIT, RESTART_WINDOW_SECONDS,
};
use crate::{Result, storage::Store};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    collections::HashMap,
    ffi::CString,
    fs,
    io::{self, Read},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, symlink},
        io::AsRawFd,
    },
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const UNIT: &str = "tmax.service";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const OUTPUT_LIMIT: usize = 256 * 1024;

fn unavailable() -> crate::Error {
    failure(
        "service_manager_unavailable",
        "The current user's systemd manager is unavailable or its identity and unit search path cannot be established",
    )
}
fn conflict() -> crate::Error {
    failure(
        "service_registration_conflict",
        "The systemd registration or login link belongs to another authority; no unrelated entry was changed",
    )
}
fn control_failed() -> crate::Error {
    failure(
        "service_control_failed",
        "The systemd user manager could not complete the requested operation; inspect its state before retrying",
    )
}

// Read a bounded amount without waiting on a pipe held by a descendant. All
// manager calls have a deadline, no shell, no stdin, and no diagnostic output.
fn command(binary: &str, args: &[&str], operation: bool) -> Result<String> {
    let error = || {
        if operation {
            control_failed()
        } else {
            unavailable()
        }
    };
    let executable = [format!("/usr/bin/{binary}"), format!("/bin/{binary}")]
        .into_iter()
        .find(|p| Path::new(p).is_file())
        .ok_or_else(error)?;
    let mut child = Command::new(executable)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("LC_ALL", "C")
        .env("SYSTEMD_PAGER", "cat")
        .spawn()
        .map_err(|_| error())?;
    let mut output = child.stdout.take().ok_or_else(error)?;
    let fd = output.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error());
    }
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    let mut finished: Option<std::process::ExitStatus> = None;
    loop {
        loop {
            match output.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) if bytes.len() + n <= OUTPUT_LIMIT => bytes.extend_from_slice(&buffer[..n]),
                Ok(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error());
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error());
                }
            }
        }
        if let Some(status) = finished {
            if !status.success() {
                return Err(error());
            }
            return String::from_utf8(bytes).map_err(|_| error());
        }
        match child.try_wait() {
            Ok(Some(status)) => finished = Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error());
            }
        }
    }
}

fn systemctl(args: &[&str], operation: bool) -> Result<String> {
    let mut all = vec!["--user", "--no-pager", "--no-ask-password"];
    all.extend_from_slice(args);
    command("systemctl", &all, operation)
}

fn manager_pid() -> Result<u32> {
    let reply = command(
        "busctl",
        &[
            "--user",
            "--no-pager",
            "call",
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "GetConnectionUnixProcessID",
            "s",
            "org.freedesktop.systemd1",
        ],
        false,
    )?;
    let mut parts = reply.split_whitespace();
    if parts.next() != Some("u") {
        return Err(unavailable());
    }
    let pid = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|p| *p > 0)
        .ok_or_else(unavailable)?;
    if parts.next().is_some() {
        return Err(unavailable());
    }
    Ok(pid)
}

fn manager_identity() -> Result<String> {
    let pid = manager_pid()?;
    let proc = PathBuf::from(format!("/proc/{pid}"));
    if fs::metadata(&proc).map_err(|_| unavailable())?.uid() != unsafe { libc::geteuid() } {
        return Err(unavailable());
    }
    let stat = fs::read_to_string(proc.join("stat")).map_err(|_| unavailable())?;
    // The parenthesized command may itself contain spaces or parentheses.
    let mut fields = stat
        .rsplit_once(')')
        .ok_or_else(unavailable)?
        .1
        .split_whitespace();
    let start = fields
        .nth(19)
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .ok_or_else(unavailable)?;
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").map_err(|_| unavailable())?;
    let boot = boot.trim();
    if boot.len() != 36
        || !boot.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
        || manager_pid()? != pid
    {
        return Err(unavailable());
    }
    Ok(format!("systemd:{}:{boot}:{pid}:{start}", unsafe {
        libc::geteuid()
    }))
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
// systemctl renders the D-Bus string array using double-quoted shell words.
// Interpret only that representation; reject control escapes rather than
// guessing at paths whose bytes cannot be safely represented.
fn parse_unit_paths(raw: &str) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for ch in raw.chars() {
        if escaped {
            if !matches!(ch, '\\' | '"' | '$' | '`') {
                return Err(unavailable());
            }
            word.push(ch);
            escaped = false;
        } else if ch == '\\' && quoted {
            escaped = true;
        } else if ch == '"' {
            quoted = !quoted;
        } else if ch.is_whitespace() && !quoted {
            if !word.is_empty() {
                paths.push(PathBuf::from(std::mem::take(&mut word)));
            }
        } else if ch.is_control() {
            return Err(unavailable());
        } else {
            word.push(ch);
        }
    }
    if quoted || escaped {
        return Err(unavailable());
    }
    if !word.is_empty() {
        paths.push(PathBuf::from(word));
    }
    Ok(paths)
}
fn paths(store: &Store) -> Result<Paths> {
    let raw = systemctl(&["show", "--property=UnitPath", "--value"], false)?;
    let search = parse_unit_paths(&raw)?;
    if search.is_empty()
        || search
            .iter()
            .any(|p| !p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)))
    {
        return Err(unavailable());
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
        .ok_or_else(unavailable)?;
    Ok(Paths {
        unit: directory.join(UNIT),
        legacy: store.home.join(".config/systemd/user").join(UNIT),
        search,
    })
}

fn marker(store: &Store) -> Result<Vec<u8>> {
    let root = store.root.to_str().ok_or_else(unavailable)?;
    if root.contains(['\n', '\r']) {
        return Err(unavailable());
    }
    Ok(format!("# tmax-owned authority={root}\n").into_bytes())
}
fn unit_data_for(store: &Store, exe: &Path) -> Result<Vec<u8>> {
    let exe = exe.to_str().ok_or_else(unavailable)?;
    if exe.contains(['\n', '\r', '%', '"', '\\']) {
        return Err(unavailable());
    }
    let mut data = marker(store)?;
    data.extend_from_slice(format!("[Unit]\nDescription=tmax personal model gateway\nStartLimitIntervalSec={RESTART_WINDOW_SECONDS}\nStartLimitBurst={RESTART_LIMIT}\n[Service]\nType=simple\nExecStart=\"{exe}\" __service\nRestart=on-failure\nRestartSec={RESTART_DELAY_SECONDS}\nRestartPreventExitStatus=78\n[Install]\nWantedBy=default.target\n").as_bytes());
    Ok(data)
}

fn unit_data(store: &Store) -> Result<Vec<u8>> {
    let exe = std::env::current_exe().map_err(|_| unavailable())?;
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
    definitions: &[Option<Vec<u8>>],
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

fn prepare_executable(store: &Store, p: &Paths) -> Result<PathBuf> {
    let current = std::env::current_exe().map_err(|_| executable_unavailable())?;
    let running = fs::metadata("/proc/self/exe").map_err(|_| executable_unavailable())?;
    let path = select_executable(
        store,
        &current,
        &running,
        &[read_unit(&p.unit, store)?, legacy_unit(store, p)?],
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

// Detect competing definitions or activation links throughout the manager's
// search path. Only the former adapter's exact marker proves legacy ownership.
fn preflight(store: &Store, p: &Paths) -> Result<()> {
    let _ = marker(store)?;
    let own = read_unit(&p.unit, store)?;
    let legacy = legacy_unit(store, p)?;
    if link_owned(&link_path(&p.unit), &p.unit)? && own.is_none() {
        return Err(conflict());
    }
    if legacy.is_some() {
        let _ = link_owned(&link_path(&p.legacy), &p.legacy)?;
    } else if p.legacy != p.unit
        && p.search.iter().any(|dir| dir.join(UNIT) == p.legacy)
        && link_owned(&link_path(&p.legacy), &p.legacy)?
    {
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
    Ok(())
}
fn set_link(unit: &Path, enabled: bool) -> Result<()> {
    let link = link_path(unit);
    let present = link_owned(&link, unit)?;
    if enabled && !present {
        crate::storage::ensure_directory(link.parent().expect("link has parent"))?;
        symlink(unit, &link).map_err(|_| control_failed())?;
    } else if !enabled && present {
        fs::remove_file(link).map_err(|_| control_failed())?;
    }
    Ok(())
}
fn property_map(output: &str) -> HashMap<&str, &str> {
    output
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect()
}
fn unit_properties() -> Result<String> {
    systemctl(
        &[
            "show",
            UNIT,
            "--property=LoadState,FragmentPath,DropInPaths,ActiveState,SubState,MainPID,NRestarts,ExecMainCode,ExecMainStatus,Result,Job",
            "--all",
        ],
        false,
    )
}
fn check_loaded(store: &Store, p: &Paths, props: &HashMap<&str, &str>) -> Result<()> {
    match props.get("DropInPaths") {
        Some(paths) if paths.trim().is_empty() => {}
        Some(_) => return Err(conflict()),
        None => return Err(unavailable()),
    }
    if let Some(fragment) = props.get("FragmentPath").filter(|s| !s.is_empty()) {
        let fragment = Path::new(fragment);
        if fragment != p.unit && fragment != p.legacy {
            return Err(conflict());
        }
        if read_unit(fragment, store)?.is_none() {
            return Err(conflict());
        }
    } else if props
        .get("LoadState")
        .is_some_and(|state| *state != "not-found")
    {
        return Err(conflict());
    }
    Ok(())
}
fn manager(store: &Store) -> Result<(Manager, Paths)> {
    let activation = manager_identity()?;
    let p = paths(store)?;
    preflight(store, &p)?;
    let raw = unit_properties()?;
    check_loaded(store, &p, &property_map(&raw))?;
    Ok((
        Manager {
            name: "systemd_user",
            activation,
            survives_logout: None,
            session_scope: "user_manager",
        },
        p,
    ))
}

// A live unit must keep its definition until the manager has stopped it.
// Preserve also retains an existing definition when the unit is inactive.
fn selected_definition<'a>(
    store: &Store,
    existing: Option<&'a [u8]>,
    desired: Option<&Path>,
    preparation: Preparation,
    live: bool,
) -> Result<Cow<'a, [u8]>> {
    if let Some(data) = existing.filter(|_| preparation == Preparation::Preserve || live) {
        let executable = definition_executable(store, data)?;
        if preparation == Preparation::Preserve {
            validate_executable(&executable)?;
        }
        return Ok(Cow::Borrowed(data));
    }
    if live {
        return Err(conflict());
    }
    unit_data_for(store, desired.ok_or_else(control_failed)?).map(Cow::Owned)
}

fn prepare_sync(store: &Store, preparation: Preparation) -> Result<()> {
    let (_, p) = manager(store)?;
    let raw = unit_properties()?;
    let props = property_map(&raw);
    check_loaded(store, &p, &props)?;
    let live = !matches!(props.get("ActiveState"), Some(&"inactive" | &"failed"));
    let current_data = read_unit(&p.unit, store)?;
    let legacy_data = legacy_unit(store, &p)?;
    let existing = if live {
        match props.get("FragmentPath").copied() {
            Some(path) if Some(path) == p.unit.to_str() => current_data.as_deref(),
            Some(path) if Some(path) == p.legacy.to_str() => legacy_data.as_deref(),
            _ => None,
        }
    } else {
        current_data.as_deref().or(legacy_data.as_deref())
    };
    let desired = if preparation == Preparation::Replace || existing.is_none() {
        Some(prepare_executable(store, &p)?)
    } else {
        None
    };
    let expected = selected_definition(store, existing, desired.as_deref(), preparation, live)?;
    if live {
        return Ok(());
    }
    let legacy_enabled = legacy_data.is_some() && link_owned(&link_path(&p.legacy), &p.legacy)?;
    if read_unit(&p.unit, store)?.as_deref() != Some(expected.as_ref()) {
        crate::storage::atomic_write(&p.unit, expected.as_ref())?;
    }
    if legacy_enabled {
        set_link(&p.unit, true)?;
    }
    if legacy_data.is_some() {
        set_link(&p.legacy, false)?;
        // Recheck the ownership marker immediately before removing the old path.
        if read_unit(&p.legacy, store)?.is_some() {
            fs::remove_file(&p.legacy).map_err(|_| control_failed())?;
        }
    }
    systemctl(&["daemon-reload"], true)?;
    let raw = unit_properties()?;
    let props = property_map(&raw);
    check_loaded(store, &p, &props)?;
    if props.get("LoadState") != Some(&"loaded")
        || props.get("FragmentPath").copied() != p.unit.to_str()
    {
        return Err(control_failed());
    }
    Ok(())
}

fn inspect_sync(store: &Store) -> Result<Observation> {
    let (_, p) = manager(store)?;
    let raw = unit_properties()?;
    let props = property_map(&raw);
    check_loaded(store, &p, &props)?;
    let owned = read_unit(&p.unit, store)?.is_some() || legacy_unit(store, &p)?.is_some();
    if !owned {
        return Ok(Observation {
            registration: "absent",
            state: "stopped",
            pid: None,
            attempts: None,
            last_exit_code: None,
            reason_code: None,
            retry_delay_seconds: RESTART_DELAY_SECONDS,
        });
    }
    let active = props.get("ActiveState").copied().unwrap_or("");
    let sub = props.get("SubState").copied().unwrap_or("");
    let result = props.get("Result").copied().unwrap_or("");
    let pid = props
        .get("MainPID")
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|p| *p > 0 && matches!(active, "active" | "activating"));
    let code = props
        .get("ExecMainCode")
        .and_then(|v| v.parse::<i32>().ok());
    let status = props
        .get("ExecMainStatus")
        .and_then(|v| v.parse::<i32>().ok());
    let exit = if code == Some(1) { status } else { None };
    let (state, reason) = if result == "start-limit-hit" {
        ("failed", Some("retry_limit_exhausted"))
    } else if active == "failed" && exit == Some(78) {
        ("blocked", Some("permanent_startup_failure"))
    } else {
        match (active, sub) {
            ("active", _) if pid.is_some() => ("running", None),
            ("activating", "auto-restart") | ("activating", "auto-restart-queued") => {
                ("recovering", None)
            }
            ("activating", _) => ("starting", None),
            ("inactive", _) => ("stopped", None),
            ("failed", _) => ("failed", Some("service_process_failed")),
            _ => ("unavailable", None),
        }
    };
    Ok(Observation {
        registration: "owned",
        state,
        pid,
        attempts: props.get("NRestarts").and_then(|v| v.parse().ok()),
        last_exit_code: exit,
        reason_code: reason.map(str::to_owned),
        retry_delay_seconds: RESTART_DELAY_SECONDS,
    })
}

async fn blocking<T: Send + 'static>(
    store: &Store,
    f: impl FnOnce(&Store) -> Result<T> + Send + 'static,
) -> Result<T> {
    let store = store.clone();
    tokio::task::spawn_blocking(move || f(&store))
        .await
        .map_err(|_| control_failed())?
}

pub(super) async fn probe(store: &Store) -> Result<Manager> {
    blocking(store, |s| manager(s).map(|(m, _)| m)).await
}
pub(super) async fn inspect(store: &Store) -> Result<Observation> {
    match blocking(store, inspect_sync).await {
        Ok(observation) => Ok(observation),
        Err(e)
            if e.code == "service_registration_conflict"
                || e.code == "service_manager_unavailable" =>
        {
            Ok(Observation {
                registration: if e.code == "service_registration_conflict" {
                    "conflict"
                } else {
                    "unavailable"
                },
                state: "unavailable",
                pid: None,
                attempts: None,
                last_exit_code: None,
                reason_code: Some(e.code),
                retry_delay_seconds: RESTART_DELAY_SECONDS,
            })
        }
        Err(e) => Err(e),
    }
}
pub(super) async fn prepare(store: &Store, preparation: Preparation) -> Result<()> {
    blocking(store, move |s| prepare_sync(s, preparation)).await
}
pub(super) async fn start(store: &Store) -> Result<()> {
    blocking(store, |s| {
        let (_, p) = manager(s)?;
        let raw = unit_properties()?;
        let props = property_map(&raw);
        check_loaded(s, &p, &props)?;
        if props.get("LoadState") != Some(&"loaded") {
            return Err(control_failed());
        }
        let fragment = props
            .get("FragmentPath")
            .filter(|path| !path.is_empty())
            .ok_or_else(control_failed)?;
        let definition = read_unit(Path::new(fragment), s)?.ok_or_else(control_failed)?;
        validate_executable(&definition_executable(s, &definition)?)?;
        if props.get("ActiveState") == Some(&"failed")
            || props.get("Result") == Some(&"start-limit-hit")
        {
            systemctl(&["reset-failed", UNIT], true)?;
        }
        systemctl(&["start", UNIT], true)?;
        Ok(())
    })
    .await
}
pub(super) async fn stop(store: &Store) -> Result<()> {
    blocking(store, |s| {
        let (_, p) = manager(s)?;
        if read_unit(&p.unit, s)?.is_none() && legacy_unit(s, &p)?.is_none() {
            return Ok(());
        }
        systemctl(&["stop", UNIT], true)?;
        let raw = unit_properties()?;
        let props = property_map(&raw);
        check_loaded(s, &p, &props)?;
        let active = props.get("ActiveState").copied().unwrap_or("");
        let pid = props.get("MainPID").and_then(|v| v.parse::<u32>().ok());
        let job = props.get("Job").copied().unwrap_or("");
        if !matches!(active, "inactive" | "failed")
            || pid != Some(0)
            || (!job.is_empty() && job != "0")
        {
            return Err(control_failed());
        }
        Ok(())
    })
    .await
}
pub(super) async fn login(store: &Store, enabled: bool) -> Result<()> {
    blocking(store, move |s| {
        if enabled {
            prepare_sync(s, Preparation::Preserve)?;
        }
        let (_, p) = manager(s)?;
        let raw = unit_properties()?;
        let props = property_map(&raw);
        check_loaded(s, &p, &props)?;
        let use_legacy =
            props.get("FragmentPath").copied() == p.legacy.to_str() && p.legacy != p.unit;
        if read_unit(&p.unit, s)?.is_some() {
            set_link(&p.unit, enabled && !use_legacy)?;
        }
        if legacy_unit(s, &p)?.is_some() {
            set_link(&p.legacy, enabled && use_legacy)?;
        }
        systemctl(&["daemon-reload"], true)?;
        let raw = unit_properties()?;
        check_loaded(s, &p, &property_map(&raw))?;
        Ok(())
    })
    .await
}
pub(super) fn login_status(store: &Store) -> Result<Value> {
    let (_, p) = match manager(store) {
        Ok(v) => v,
        Err(e) if e.code == "service_registration_conflict" => {
            return Ok(
                json!({"registration":"conflict", "unit_matches":false, "login_enabled":null}),
            );
        }
        Err(_) => {
            return Ok(
                json!({"registration":"unavailable", "unit_matches":false, "login_enabled":null}),
            );
        }
    };
    let current = read_unit(&p.unit, store)?;
    let legacy = legacy_unit(store, &p)?;
    let enabled = link_owned(&link_path(&p.unit), &p.unit)?
        || (legacy.is_some() && link_owned(&link_path(&p.legacy), &p.legacy)?);
    let matches = current.as_ref() == Some(&unit_data(store)?);
    let registration = if current.is_none() && legacy.is_none() {
        "absent"
    } else if matches && legacy.is_none() {
        "present"
    } else {
        "drifted"
    };
    Ok(
        json!({"registration":registration, "unit_matches":matches, "login_enabled":enabled, "login_link":if enabled {"owned"} else {"absent"}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_and_pending_drop_ins_are_registration_conflicts() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_path_buf());
        let unit = home.path().join(UNIT);
        fs::write(
            &unit,
            unit_data_for(&store, Path::new("/usr/bin/tmax")).unwrap(),
        )
        .unwrap();
        let p = Paths {
            unit: unit.clone(),
            legacy: unit.clone(),
            search: vec![home.path().to_path_buf()],
        };
        let raw = format!(
            "LoadState=loaded\nFragmentPath={}\nDropInPaths=\n",
            unit.display()
        );
        assert!(check_loaded(&store, &p, &property_map(&raw)).is_ok());
        let raw = format!(
            "LoadState=loaded\nFragmentPath={}\nDropInPaths=/home/user/.config/systemd/user/tmax.service.d/override.conf\n",
            unit.display()
        );
        assert_eq!(
            check_loaded(&store, &p, &property_map(&raw))
                .unwrap_err()
                .code,
            "service_registration_conflict"
        );
        let missing = format!("LoadState=loaded\nFragmentPath={}\n", unit.display());
        assert_eq!(
            check_loaded(&store, &p, &property_map(&missing))
                .unwrap_err()
                .code,
            "service_manager_unavailable"
        );

        let drop_ins = home.path().join("tmax.service.d");
        fs::create_dir(&drop_ins).unwrap();
        fs::write(
            drop_ins.join("override.conf"),
            b"[Service]\nExecStartPre=/unrelated\n",
        )
        .unwrap();
        assert_eq!(
            preflight(&store, &p).unwrap_err().code,
            "service_registration_conflict"
        );
    }

    #[test]
    fn preparation_preserves_existing_executable_until_explicit_stopped_replacement() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_path_buf());
        let first = home.path().join("first");
        let second = home.path().join("second");
        fs::copy("/proc/self/exe", &first).unwrap();
        fs::copy("/proc/self/exe", &second).unwrap();
        let original = unit_data_for(&store, &first).unwrap();
        let replacement = unit_data_for(&store, &second).unwrap();
        assert_eq!(
            selected_definition(
                &store,
                Some(&original),
                Some(&second),
                Preparation::Preserve,
                true
            )
            .unwrap()
            .as_ref(),
            original.as_slice()
        );
        assert_eq!(
            selected_definition(
                &store,
                Some(&original),
                Some(&second),
                Preparation::Preserve,
                false
            )
            .unwrap()
            .as_ref(),
            original.as_slice()
        );
        assert_eq!(
            selected_definition(
                &store,
                Some(&original),
                Some(&second),
                Preparation::Replace,
                true
            )
            .unwrap()
            .as_ref(),
            original.as_slice()
        );
        assert_eq!(
            selected_definition(
                &store,
                Some(&original),
                Some(&second),
                Preparation::Replace,
                false
            )
            .unwrap()
            .as_ref(),
            replacement.as_slice()
        );
        let mut altered = original.clone();
        altered.extend_from_slice(b"ExecStartPre=/unrelated\n");
        assert_eq!(
            selected_definition(
                &store,
                Some(&altered),
                Some(&second),
                Preparation::Preserve,
                true
            )
            .unwrap_err()
            .code,
            "service_registration_conflict"
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
        let selected =
            select_executable(&store, &deleted, &running, &[Some(definition.clone())]).unwrap();
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
        assert!(
            select_executable(&other, &deleted, &running, &[Some(definition.clone())]).is_err()
        );
        let mut altered = definition;
        altered.extend_from_slice(b"ExecStart=/unrelated\n");
        assert!(select_executable(&store, &deleted, &running, &[Some(altered)]).is_err());
        assert!(
            select_executable(
                &store,
                Path::new("/unrelated (deleted)"),
                &running,
                &[Some(unit_data_for(&store, &executable).unwrap())]
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
    fn manager_unit_paths_support_quoted_user_directories() {
        let paths = parse_unit_paths("/run/user/1000/systemd/user \"/home/a user/.config/systemd/user\" /usr/lib/systemd/user\n").unwrap();
        assert_eq!(paths[1], Path::new("/home/a user/.config/systemd/user"));
        assert!(parse_unit_paths("\"/unfinished").is_err());
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
