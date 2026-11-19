//! Typed observations from systemd's private user-manager connection.
#[path = "systemd_bus.rs"]
mod native;

use self::native::{Bus, Message};
use super::{UNIT_NAME, conflict, control_failed, unavailable};
use crate::Result;
use std::{
    ffi::{CStr, CString},
    fmt::Write,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const MANAGER_PATH: &CStr = c"/org/freedesktop/systemd1";
const MANAGER: &CStr = c"org.freedesktop.systemd1.Manager";
const PROPERTIES: &CStr = c"org.freedesktop.DBus.Properties";
const DEADLINE: Duration = Duration::from_secs(10);

type ExecStart = (String, Vec<String>, bool);

pub(super) struct Unit {
    pub load_state: String,
    pub fragment_path: String,
    pub drop_in_paths: Vec<String>,
    pub need_daemon_reload: bool,
    pub active_state: String,
    pub sub_state: String,
    pub main_pid: u32,
    pub n_restarts: u32,
    pub exec_main_code: i32,
    pub exec_main_status: i32,
    pub result: String,
    pub job_pending: bool,
    pub exec_start: Vec<ExecStart>,
}
impl Unit {
    pub(super) fn has_job(&self) -> bool {
        self.job_pending
    }

    pub(super) fn idle(&self) -> bool {
        self.main_pid == 0
            && !self.has_job()
            && matches!(self.active_state.as_str(), "inactive" | "failed")
    }
}

fn socket_address(path: &Path) -> Result<CString> {
    if !path.is_absolute() {
        return Err(unavailable(
            "The systemd private socket path is not absolute",
        ));
    }
    let mut address = String::from("unix:path=");
    for &byte in path.as_os_str().as_encoded_bytes() {
        if byte.is_ascii_alphanumeric() || b"/_-.".contains(&byte) {
            address.push(char::from(byte));
        } else {
            let _ = write!(address, "%{byte:02x}");
        }
    }
    CString::new(address).map_err(|_| unavailable("The systemd private socket address is invalid"))
}

pub(super) fn peer_pid(fd: i32, uid: u32) -> Result<u32> {
    let mut credential = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let mut length = size;
    let result = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut credential).cast(),
            &mut length,
        )
    };
    if result != 0 || length != size || credential.pid <= 0 || credential.uid != uid {
        return Err(unavailable(
            "The systemd private socket peer is not the current user's manager",
        ));
    }
    Ok(credential.pid as u32)
}

fn exec_start(reply: &Message) -> Result<Vec<ExecStart>> {
    let mut commands = Vec::new();
    reply.begin(b'a', c"(sasbttttuii)")?;
    while reply.enter(b'r', c"sasbttttuii")? {
        commands.push((reply.text()?.to_owned(), reply.strings()?, reply.boolean()?));
        reply.skip(c"ttttuii")?; // Execution timestamps and exit fields are not part of argv.
        reply.end()?;
    }
    reply.end()?;
    Ok(commands)
}

fn unit_properties(reply: &Message) -> Result<Unit> {
    macro_rules! properties {
        ($($name:literal => $field:ident, $signature:literal, $read:expr);+ $(;)?) => {{
            $(let mut $field = None;)+
            reply.begin(b'a', c"{sv}")?;
            while reply.enter(b'e', c"sv")? {
                match reply.text()? {
                    $($name => {
                        reply.begin(b'v', $signature)?;
                        $field = Some($read);
                        reply.end()?;
                    })+
                    "ExecCondition" | "ExecStartPre" | "ExecStartPost" | "ExecReload"
                    | "ExecStop" | "ExecStopPost" => {
                        // These hooks are not part of the owned unit. The loaded unit
                        // can still contain one even after its file has been restored.
                        reply.begin(b'v', c"a(sasbttttuii)")?;
                        reply.begin(b'a', c"(sasbttttuii)")?;
                        if reply.enter(b'r', c"sasbttttuii")? {
                            return Err(conflict());
                        }
                        reply.end()?;
                        reply.end()?;
                    }
                    _ => reply.skip(c"v")?,
                }
                reply.end()?;
            }
            reply.end()?;
            Ok(Unit { $($field: $field.ok_or_else(|| unavailable(concat!("The systemd unit observation omitted ", $name)))?),+ })
        }};
    }

    properties! {
        "LoadState" => load_state, c"s", reply.text()?.to_owned();
        "FragmentPath" => fragment_path, c"s", reply.text()?.to_owned();
        "DropInPaths" => drop_in_paths, c"as", reply.strings()?;
        "NeedDaemonReload" => need_daemon_reload, c"b", reply.boolean()?;
        "ActiveState" => active_state, c"s", reply.text()?.to_owned();
        "SubState" => sub_state, c"s", reply.text()?.to_owned();
        "MainPID" => main_pid, c"u", reply.unsigned()?;
        "NRestarts" => n_restarts, c"u", reply.unsigned()?;
        "ExecMainCode" => exec_main_code, c"i", reply.signed()?;
        "ExecMainStatus" => exec_main_status, c"i", reply.signed()?;
        "Result" => result, c"s", reply.text()?.to_owned();
        "Job" => job_pending, c"(uo)", {
            reply.begin(b'r', c"uo")?;
            let id = reply.unsigned()?;
            let path = reply.path()?;
            reply.end()?;
            id != 0 || path != c"/"
        };
        "ExecStart" => exec_start, c"a(sasbttttuii)", self::exec_start(reply)?;
    }
}

fn activation(pid: u32, uid: u32) -> Result<String> {
    let identify =
        || unavailable("The systemd manager process or boot identity could not be verified");
    let proc = PathBuf::from(format!("/proc/{pid}"));
    if fs::metadata(&proc).map_err(|_| identify())?.uid() != uid {
        return Err(identify());
    }
    let stat = fs::read_to_string(proc.join("stat")).map_err(|_| identify())?;
    let start = stat
        .rsplit_once(')')
        .and_then(|(_, fields)| fields.split_whitespace().nth(19))
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(identify)?;
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").map_err(|_| identify())?;
    let boot = boot.trim();
    if boot.len() != 36 || !boot.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err(identify());
    }
    Ok(format!("systemd:{uid}:{boot}:{pid}:{start}"))
}

pub(super) struct Peer {
    bus: Bus,
    runtime: PathBuf,
    pub activation: String,
}
impl Peer {
    pub(super) fn connect() -> Result<Self> {
        let uid = unsafe { libc::geteuid() };
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|value| !value.is_empty())
            .map_or_else(|| PathBuf::from(format!("/run/user/{uid}")), PathBuf::from);
        let bus = Bus::connect(&socket_address(&runtime.join("systemd/private"))?)?;
        let activation = activation(peer_pid(bus.fd()?, uid)?, uid)?;
        Ok(Self {
            bus,
            runtime,
            activation,
        })
    }

    pub(super) fn unit_paths(&self) -> Result<Vec<String>> {
        let reply = self.bus.call(
            MANAGER_PATH,
            PROPERTIES,
            c"Get",
            &[MANAGER, c"UnitPath"],
            "The systemd manager did not provide its unit search path",
        )?;
        reply.begin(b'v', c"as")?;
        let paths = reply.strings()?;
        reply.end()?;
        Ok(paths)
    }

    pub(super) fn unit(&self) -> Result<Unit> {
        let path = self.bus.call(
            MANAGER_PATH,
            MANAGER,
            c"LoadUnit",
            &[UNIT_NAME],
            "The systemd manager could not resolve the tmax unit",
        )?;
        // One reply supplies both the exact loaded command and its process/job state.
        let reply = self.bus.call(
            path.path()?,
            PROPERTIES,
            c"GetAll",
            &[c""],
            "The systemd manager did not provide a complete unit observation",
        )?;
        unit_properties(&reply)
    }

    pub(super) fn control(&self, args: &[&str]) -> Result<()> {
        // Leave job completion handling to systemctl. Observations never parse its output.
        let executable = ["/usr/bin/systemctl", "/bin/systemctl"]
            .into_iter()
            .find(|path| Path::new(path).is_file())
            .ok_or_else(control_failed)?;
        let mut command = Command::new(executable);
        let mut child = command
            .args(["--user", "--no-pager", "--no-ask-password"])
            .args(args)
            .env("XDG_RUNTIME_DIR", &self.runtime)
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| control_failed())?;
        let deadline = Instant::now() + DEADLINE;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(control_failed())
                    };
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(control_failed());
                }
            }
        }
    }
}
