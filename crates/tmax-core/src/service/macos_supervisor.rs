//! User-login launchd supervision. Loading a job and registering it for the next
//! login are deliberately separate operations.
use super::super::failure;
use super::{Manager, Observation, Preparation, RESTART_DELAY_SECONDS};
use crate::{Result, storage::Store};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    ffi::{CString, OsString},
    io::Read,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

const LABEL: &str = "org.tmax.service";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

fn unavailable() -> crate::Error {
    failure(
        "service_manager_unavailable",
        "A usable launchd GUI user domain and its login-session identity are required",
    )
}
fn conflict() -> crate::Error {
    failure(
        "service_registration_conflict",
        "The launchd registration is not owned by this authority; it was not changed",
    )
}
fn control_failed() -> crate::Error {
    failure(
        "service_control_failed",
        "The user launchd manager did not complete the requested service operation",
    )
}
fn executable_unavailable() -> crate::Error {
    failure(
        "service_executable_unavailable",
        "The intended service executable cannot be verified on disk; restore the installed binary before restarting",
    )
}
fn validate_executable(path: &Path) -> Result<()> {
    let mut file = std::fs::File::open(path).map_err(|_| executable_unavailable())?;
    let metadata = file.metadata().map_err(|_| executable_unavailable())?;
    let name = CString::new(path.as_os_str().as_bytes()).map_err(|_| executable_unavailable())?;
    if !path.is_absolute()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o111 == 0
        || unsafe { libc::access(name.as_ptr(), libc::X_OK) } != 0
    {
        return Err(executable_unavailable());
    }
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|_| executable_unavailable())?;
    if !matches!(
        magic,
        [0xfe, 0xed, 0xfa, 0xce]
            | [0xce, 0xfa, 0xed, 0xfe]
            | [0xfe, 0xed, 0xfa, 0xcf]
            | [0xcf, 0xfa, 0xed, 0xfe]
            | [0xca, 0xfe, 0xba, 0xbe]
            | [0xbe, 0xba, 0xfe, 0xca]
            | [0xca, 0xfe, 0xba, 0xbf]
            | [0xbf, 0xba, 0xfe, 0xca]
    ) {
        return Err(executable_unavailable());
    }
    Ok(())
}
fn uid() -> u32 {
    unsafe { libc::geteuid() }
}
fn domain() -> String {
    format!("gui/{}", uid())
}
fn target() -> String {
    format!("{}/{LABEL}", domain())
}
fn definition(store: &Store) -> PathBuf {
    store.root.join("launchd-service.plist")
}
fn login_path(store: &Store) -> PathBuf {
    store
        .home
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

// A digest makes the authority marker safe inside an XML comment even when the
// home directory contains XML metacharacters or non-UTF-8 bytes.
fn marker(store: &Store) -> String {
    let digest = Sha256::digest(store.root.as_os_str().as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    format!("<!-- tmax-owned authority-sha256={hex} -->\n")
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn plist_for(store: &Store, executable: &Path) -> Result<Vec<u8>> {
    let executable = executable.to_str().ok_or_else(unavailable)?;
    if executable.chars().any(char::is_control) || executable.trim() != executable {
        return Err(unavailable());
    }
    Ok(format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>{LABEL}</string>\n<key>ProgramArguments</key><array><string>{}</string><string>__service</string></array>\n<key>RunAtLoad</key><true/>\n<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>\n<key>ThrottleInterval</key><integer>{RESTART_DELAY_SECONDS}</integer>\n</dict></plist>\n", marker(store), xml(executable)).into_bytes())
}
fn plist(store: &Store) -> Result<Vec<u8>> {
    let executable = std::env::current_exe().map_err(|_| unavailable())?;
    plist_for(store, &executable)
}
fn owned(store: &Store, bytes: &[u8]) -> bool {
    let prefix = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}",
        marker(store)
    );
    bytes.starts_with(prefix.as_bytes())
}
fn read_owned(store: &Store, path: &Path) -> Result<Option<Vec<u8>>> {
    let contents = crate::storage::read_optional(path)?;
    if contents.as_ref().is_some_and(|bytes| !owned(store, bytes)) {
        return Err(conflict());
    }
    Ok(contents)
}

async fn command(args: &[OsString]) -> Result<std::process::Output> {
    let mut command = tokio::process::Command::new("/bin/launchctl");
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())
}
async fn launchctl(args: &[&str]) -> Result<std::process::Output> {
    command(&args.iter().map(OsString::from).collect::<Vec<_>>()).await
}
fn field<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    output.lines().find_map(|line| {
        let (key, value) = line.trim().split_once(" = ")?;
        (key == name).then_some(value.trim())
    })
}
fn boot_time() -> Result<String> {
    let mut boot: libc::timeval = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<libc::timeval>();
    let result = unsafe {
        libc::sysctlbyname(
            c"kern.boottime".as_ptr(),
            (&mut boot as *mut libc::timeval).cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 || length != std::mem::size_of::<libc::timeval>() || boot.tv_sec <= 0 {
        return Err(unavailable());
    }
    Ok(format!("{}.{}", boot.tv_sec, boot.tv_usec))
}
async fn manager() -> Result<Manager> {
    // Use the GUI domain's audit session, never the caller's process session.
    let user = format!("user/{}", uid());
    let user_output = launchctl(&["print", &user]).await?;
    let gui_output = launchctl(&["print", &domain()]).await?;
    if !user_output.status.success() || !gui_output.status.success() {
        return Err(unavailable());
    }
    let gui_text = std::str::from_utf8(&gui_output.stdout).map_err(|_| unavailable())?;
    let asid = field(gui_text, "asid")
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|n| *n > 0 && *n != u32::MAX)
        .ok_or_else(unavailable)?;
    Ok(Manager {
        name: "launchd_user",
        activation: format!("macos-login:{}:{}:{}", uid(), boot_time()?, asid),
        survives_logout: Some(false),
        session_scope: "user_login",
    })
}

// launchctl print is explicitly diagnostic rather than a stable API. Treat any
// unrecognized output as unavailable, never as evidence that a job is absent.
async fn job() -> Result<Option<String>> {
    let output = launchctl(&["print", &target()]).await?;
    if output.status.success() {
        return String::from_utf8(output.stdout)
            .map(Some)
            .map_err(|_| unavailable());
    }
    let error = String::from_utf8_lossy(&output.stderr);
    if error.contains("Could not find service")
        || error.contains("Could not find specified service")
    {
        Ok(None)
    } else {
        Err(unavailable())
    }
}
fn owned_job(store: &Store, output: &str) -> Result<()> {
    let path = field(output, "path").ok_or_else(unavailable)?;
    let candidate = Path::new(path);
    if candidate != definition(store) && candidate != login_path(store) {
        return Err(conflict());
    }
    let executable = service_argument(output).ok_or_else(conflict)?;
    if !Path::new(executable).is_absolute() || field(output, "program") != Some(executable) {
        return Err(conflict());
    }
    let expected = plist_for(store, Path::new(executable)).map_err(|_| conflict())?;
    let contents = read_owned(store, candidate)?;
    // Disabling login removes its plist without unloading the job. Only the
    // retained definition can then prove the loaded job's exact identity.
    let contents = if contents.is_none() && candidate == login_path(store) {
        read_owned(store, &definition(store))?
    } else {
        contents
    };
    if contents.as_ref() != Some(&expected) {
        return Err(conflict());
    }
    Ok(())
}
fn service_argument(output: &str) -> Option<&str> {
    let mut lines = output.lines();
    if !lines.any(|line| line.trim() == "arguments = {") {
        return None;
    }
    let executable = lines.next()?.trim();
    if lines.next()?.trim() != "__service" || lines.next()?.trim() != "}" {
        return None;
    }
    Some(executable)
}
fn job_executable(output: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(
        service_argument(output).ok_or_else(conflict)?,
    ))
}
async fn preflight(store: &Store) -> Result<Manager> {
    // LaunchAgents is read by launchd from the account's actual home, not HOME.
    if Store::discover()?.home != store.home {
        return Err(unavailable());
    }
    for path in [definition(store), login_path(store)] {
        let value = path.to_str().ok_or_else(unavailable)?;
        if value.chars().any(char::is_control) {
            return Err(unavailable());
        }
    }
    // Executable validation belongs to preparation, not inspection or stop.
    let manager = manager().await?;
    read_owned(store, &definition(store))?;
    read_owned(store, &login_path(store))?;
    if let Some(output) = job().await? {
        owned_job(store, &output)?;
    }
    Ok(manager)
}

pub(super) async fn probe(store: &Store) -> Result<Manager> {
    preflight(store).await
}

pub(super) async fn inspect(store: &Store) -> Result<Observation> {
    preflight(store).await?;
    let registered = read_owned(store, &definition(store))?.is_some()
        || read_owned(store, &login_path(store))?.is_some();
    let output = job().await?;
    if let Some(output) = output {
        owned_job(store, &output)?;
        let pid = field(&output, "pid")
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|v| *v > 0);
        let exit = field(&output, "last exit code").and_then(|v| v.parse::<i32>().ok());
        let state = if pid.is_some() {
            "running"
        } else {
            match field(&output, "state") {
                Some("running") => "starting",
                Some("spawn scheduled") | Some("waiting") if exit.is_some_and(|code| code != 0) => {
                    "recovering"
                }
                Some("not running") | Some("waiting") => "stopped",
                _ => "unavailable",
            }
        };
        Ok(Observation {
            registration: "owned",
            state,
            pid,
            attempts: None,
            last_exit_code: exit,
            reason_code: None,
            retry_delay_seconds: RESTART_DELAY_SECONDS,
        })
    } else {
        Ok(Observation {
            registration: if registered { "owned" } else { "absent" },
            state: "stopped",
            pid: None,
            attempts: None,
            last_exit_code: None,
            reason_code: None,
            retry_delay_seconds: RESTART_DELAY_SECONDS,
        })
    }
}

pub(super) async fn prepare(store: &Store, preparation: Preparation) -> Result<()> {
    preflight(store).await?;
    let loaded = job().await?;
    if let Some(output) = &loaded {
        owned_job(store, output)?;
    }
    let executable = match (&loaded, preparation) {
        (Some(output), Preparation::Preserve) => job_executable(output)?,
        // A service restarting itself may still execute the old inode after an
        // atomic replacement; use the proven installed path, not current_exe.
        (Some(output), Preparation::Replace)
            if field(output, "pid").and_then(|v| v.parse::<u32>().ok())
                == Some(std::process::id()) =>
        {
            job_executable(output)?
        }
        _ => std::env::current_exe().map_err(|_| executable_unavailable())?,
    };
    validate_executable(&executable)?;
    let bytes = plist_for(store, &executable)?;
    // launchd's loaded definition is immutable. Keep both ownership evidence
    // and login intent intact until bootout, then the caller prepares again.
    if preparation == Preparation::Replace && loaded.is_some() {
        return Ok(());
    }
    let login = login_path(store);
    let login_present = read_owned(store, &login)?.is_some();
    store.write_raw("launchd-service.plist", &bytes)?;
    if login_present {
        crate::storage::atomic_write(&login, &bytes)?;
    }
    Ok(())
}

pub(super) async fn start(store: &Store) -> Result<()> {
    preflight(store).await?;
    let path = definition(store);
    if read_owned(store, &path)?.is_none() {
        return Err(control_failed());
    }
    match job().await? {
        Some(output) => {
            owned_job(store, &output)?;
            // Never replace or kill a running service. An existing loaded job
            // has its own immutable launchd definition until it is stopped.
            if field(&output, "pid")
                .and_then(|v| v.parse::<u32>().ok())
                .is_some_and(|v| v > 0)
            {
                return Ok(());
            }
            if read_owned(store, &path)?.as_ref()
                != Some(&plist_for(store, &job_executable(&output)?)?)
            {
                return Err(control_failed());
            }
            let result = launchctl(&["kickstart", &target()])
                .await
                .map_err(|_| control_failed())?;
            if !result.status.success() {
                return Err(control_failed());
            }
        }
        None => {
            if read_owned(store, &path)?.as_ref() != Some(&plist(store)?) {
                return Err(control_failed());
            }
            let args = vec![
                OsString::from("bootstrap"),
                OsString::from(domain()),
                path.into_os_string(),
            ];
            let result = command(&args).await.map_err(|_| control_failed())?;
            if !result.status.success() {
                return Err(control_failed());
            }
        }
    }
    Ok(())
}

pub(super) async fn stop(store: &Store) -> Result<()> {
    preflight(store).await?;
    let Some(output) = job().await? else {
        return Ok(());
    };
    owned_job(store, &output)?;
    let pid = field(&output, "pid")
        .and_then(|v| v.parse::<libc::pid_t>().ok())
        .filter(|pid| *pid > 0);
    let result = launchctl(&["bootout", &target()])
        .await
        .map_err(|_| control_failed())?;
    if !result.status.success() {
        return Err(control_failed());
    }
    let deadline = tokio::time::Instant::now() + COMMAND_TIMEOUT;
    loop {
        match job().await {
            Ok(None) => {
                // bootout removes the job and disarms KeepAlive; additionally
                // wait for the identified process to actually disappear.
                let gone = pid.is_none_or(|pid| {
                    (unsafe { libc::kill(pid, 0) }) == -1
                        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
                });
                if gone {
                    return Ok(());
                }
            }
            Ok(Some(output)) => owned_job(store, &output)?,
            Err(_) => return Err(control_failed()),
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(control_failed());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub(super) async fn login(store: &Store, enabled: bool) -> Result<()> {
    preflight(store).await?;
    let path = login_path(store);
    let existing = read_owned(store, &path)?;
    if enabled {
        prepare(store, Preparation::Preserve).await?;
        // A persistent launchd disable override survives plist replacement.
        // Enabling the label permits a future load but does not load or stop it.
        let result = launchctl(&["enable", &target()])
            .await
            .map_err(|_| control_failed())?;
        if !result.status.success() {
            return Err(control_failed());
        }
        let definition = read_owned(store, &definition(store))?.ok_or_else(control_failed)?;
        crate::storage::atomic_write(&path, &definition)?;
    } else if existing.is_some() {
        std::fs::remove_file(&path).map_err(|_| control_failed())?;
        // Persist the removal without unloading the current job.
        let dir = std::fs::File::open(path.parent().ok_or_else(control_failed)?)
            .map_err(|_| control_failed())?;
        dir.sync_all().map_err(|_| control_failed())?;
    }
    Ok(())
}

pub(super) fn login_status(store: &Store) -> Result<Value> {
    let expected = plist(store)?;
    let definition = crate::storage::read_optional(&definition(store))?;
    let login = crate::storage::read_optional(&login_path(store))?;
    let foreign = definition.as_ref().is_some_and(|v| !owned(store, v))
        || login.as_ref().is_some_and(|v| !owned(store, v));
    let matches = definition.as_ref() == Some(&expected);
    let registration = if foreign {
        "conflict"
    } else if definition.is_none() && login.is_none() {
        "absent"
    } else if matches && login.as_ref().is_none_or(|v| v == &expected) {
        "present"
    } else {
        "drifted"
    };
    // A plist alone cannot prove that launchd's persistent disable override is
    // clear. Do not turn the existence of the file into an enabled claim.
    Ok(
        json!({"registration": registration, "unit_matches": matches, "login_enabled": if foreign || login.is_some() { Value::Null } else { json!(false) }, "login_plist_present": login.is_some(), "manager": "launchd_user", "session_scope": "user_login"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launchd_arguments_and_replacement_file_are_checked() {
        let output = "arguments = {\n    /tmp/tmax binary\n    __service\n}\n";
        assert_eq!(service_argument(output), Some("/tmp/tmax binary"));
        assert!(service_argument("arguments = {\n /tmp/tmax\n __service\n extra\n}\n").is_none());
        assert!(service_argument("arguments = {\n /tmp/tmax\n __service\n").is_none());
        let home = tempfile::tempdir().unwrap();
        let executable = home.path().join("tmax");
        assert_eq!(
            validate_executable(&executable).unwrap_err().code,
            "service_executable_unavailable"
        );
        std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        assert!(validate_executable(&executable).is_ok());
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(validate_executable(&executable).is_err());
    }

    #[test]
    fn loaded_job_must_match_the_owned_definition_and_executable() {
        let home = tempfile::tempdir().unwrap();
        let store = Store::at(home.path().to_path_buf());
        let plist_a = plist_for(&store, Path::new("/opt/tmax-a")).unwrap();
        let plist_b = plist_for(&store, Path::new("/opt/tmax-b")).unwrap();
        store.write_raw("launchd-service.plist", &plist_a).unwrap();
        let output = |path: &Path, program: &str, args: &str| {
            format!(
                "path = {}\nprogram = {program}\narguments = {{\n{args}\n}}\n",
                path.display()
            )
        };
        let good = output(&definition(&store), "/opt/tmax-a", "/opt/tmax-a\n__service");
        assert!(owned_job(&store, &good).is_ok());
        let wrong_program = output(&definition(&store), "/opt/tmax-b", "/opt/tmax-a\n__service");
        assert_eq!(
            owned_job(&store, &wrong_program).unwrap_err().code,
            "service_registration_conflict"
        );
        let wrong_args = output(
            &definition(&store),
            "/opt/tmax-a",
            "/opt/tmax-a\n__service\nextra",
        );
        assert_eq!(
            owned_job(&store, &wrong_args).unwrap_err().code,
            "service_registration_conflict"
        );
        // The job predates a replacement of the plist at the same path.
        store.write_raw("launchd-service.plist", &plist_b).unwrap();
        assert_eq!(
            owned_job(&store, &good).unwrap_err().code,
            "service_registration_conflict"
        );

        let login = login_path(&store);
        std::fs::create_dir_all(login.parent().unwrap()).unwrap();
        std::fs::write(&login, &plist_a).unwrap();
        let from_login = output(&login, "/opt/tmax-a", "/opt/tmax-a\n__service");
        assert!(owned_job(&store, &from_login).is_ok());
        std::fs::remove_file(&login).unwrap();
        assert_eq!(
            owned_job(&store, &from_login).unwrap_err().code,
            "service_registration_conflict"
        );
        store.write_raw("launchd-service.plist", &plist_a).unwrap();
        assert!(owned_job(&store, &from_login).is_ok());
    }

    #[test]
    fn ownership_is_pinned_and_xml_escaped() {
        let a = Store::at(PathBuf::from("/tmp/a&b"));
        let b = Store::at(PathBuf::from("/tmp/other"));
        let bytes = plist(&a).unwrap();
        assert!(owned(&a, &bytes));
        assert!(!owned(&b, &bytes));
    }
    #[test]
    fn diagnostic_fields_require_exact_keys() {
        let output = "\tstate = running\n\tpid = 17\n\tlast exit code = 75\n";
        assert_eq!(field(output, "pid"), Some("17"));
        assert_eq!(field(output, "id"), None);
        assert_eq!(field(output, "last exit code"), Some("75"));
    }
}
