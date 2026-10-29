//! Current-user Task Scheduler integration. The task owns the foreground
//! `__service` process; there is no detached launcher or tmax watchdog.
//!
//! Native recovery must be qualified on Windows for both on-demand and logon
//! starts. In particular, XML alone is not evidence of recovery after an action
//! has started. The scheduler's minimum failure-restart interval is one minute.
use super::{Manager, Observation, Preparation, RESTART_LIMIT};
use crate::{Error, Result, storage::Store};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    ffi::{OsString, c_void},
    io::{Read, Seek, SeekFrom},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

type Handle = *mut c_void;
#[repr(C)]
struct Luid {
    low: u32,
    high: i32,
}
#[repr(C)]
struct TokenStatistics {
    token_id: Luid,
    authentication_id: Luid,
    expiration_time: i64,
    token_type: u32,
    impersonation_level: u32,
    dynamic_charged: u32,
    dynamic_available: u32,
    group_count: u32,
    privilege_count: u32,
    modified_id: Luid,
}
#[repr(C)]
struct TimeOfDay {
    boot_time: i64,
    current_time: i64,
    time_zone_bias: i64,
    time_zone_id: u32,
    reserved: u32,
    boot_time_bias: u64,
    sleep_time_bias: u64,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcessId() -> u32;
    fn ProcessIdToSessionId(process: u32, session: *mut u32) -> i32;
    fn GetCurrentProcess() -> Handle;
    fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(
        token: Handle,
        class: u32,
        data: *mut c_void,
        size: u32,
        needed: *mut u32,
    ) -> i32;
}
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQuerySystemInformation(class: u32, data: *mut c_void, size: u32, needed: *mut u32) -> i32;
}
fn unavailable() -> Error {
    Error::new(
        "service_manager_unavailable",
        "Cannot access the current-user Task Scheduler or establish this Windows logon session; no elevation or detached fallback was attempted",
    )
}
fn failed() -> Error {
    Error::new(
        "service_control_failed",
        "Windows Task Scheduler could not complete the requested operation; inspect the task and retry",
    )
}
fn conflict() -> Error {
    Error::new(
        "service_registration_conflict",
        "A Windows task or legacy Run value is not owned by this authority; it was not changed",
    )
}
fn executable_unavailable() -> Error {
    Error::new(
        "service_executable_unavailable",
        "The intended service executable cannot be verified on disk; restore the installed binary before restarting",
    )
}
fn validate_executable(path: &Path) -> Result<()> {
    let mut file = std::fs::File::open(path).map_err(|_| executable_unavailable())?;
    if !path.is_absolute()
        || !file
            .metadata()
            .map_err(|_| executable_unavailable())?
            .is_file()
    {
        return Err(executable_unavailable());
    }
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)
        .map_err(|_| executable_unavailable())?;
    if &dos[..2] != b"MZ" {
        return Err(executable_unavailable());
    }
    let offset = u32::from_le_bytes([dos[60], dos[61], dos[62], dos[63]]);
    file.seek(SeekFrom::Start(u64::from(offset)))
        .map_err(|_| executable_unavailable())?;
    let mut pe = [0u8; 6];
    file.read_exact(&mut pe)
        .map_err(|_| executable_unavailable())?;
    let machine = u16::from_le_bytes([pe[4], pe[5]]);
    let expected = if cfg!(target_arch = "x86_64") {
        0x8664
    } else if cfg!(target_arch = "x86") {
        0x014c
    } else if cfg!(target_arch = "aarch64") {
        0xaa64
    } else if cfg!(target_arch = "arm") {
        0x01c4
    } else {
        return Err(executable_unavailable());
    };
    if &pe[..4] != b"PE\0\0" || machine != expected {
        return Err(executable_unavailable());
    }
    Ok(())
}
fn activation() -> Result<String> {
    let mut raw = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), 0x8, &mut raw) } == 0 {
        return Err(unavailable());
    }
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut stats: TokenStatistics = unsafe { std::mem::zeroed() };
    let mut needed = 0;
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            10,
            (&mut stats as *mut TokenStatistics).cast(),
            std::mem::size_of::<TokenStatistics>() as u32,
            &mut needed,
        )
    } == 0
    {
        return Err(unavailable());
    }
    let mut time: TimeOfDay = unsafe { std::mem::zeroed() };
    if unsafe {
        NtQuerySystemInformation(
            3,
            (&mut time as *mut TimeOfDay).cast(),
            std::mem::size_of::<TimeOfDay>() as u32,
            &mut needed,
        )
    } < 0
        || time.boot_time <= 0
    {
        return Err(unavailable());
    }
    Ok(format!(
        "windows-logon-{:x}-{:08x}{:08x}",
        time.boot_time, stats.authentication_id.high as u32, stats.authentication_id.low
    ))
}
fn task_name(store: &Store) -> String {
    let mut hash = Sha256::new();
    for unit in store.root.as_os_str().encode_wide() {
        hash.update(unit.to_le_bytes());
    }
    format!("tmax-{:x}", hash.finalize())
}
fn powershell() -> Result<OsString> {
    let mut path = vec![0u16; 32768];
    let len = unsafe { GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) } as usize;
    if len == 0 || len >= path.len() {
        return Err(unavailable());
    }
    use std::os::windows::ffi::OsStringExt;
    path.truncate(len);
    let mut path = std::path::PathBuf::from(OsString::from_wide(&path));
    path.push(r"WindowsPowerShell\v1.0\powershell.exe");
    Ok(path.into_os_string())
}
fn session_id() -> Result<u32> {
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 || session == 0 {
        return Err(unavailable());
    }
    Ok(session)
}

// All data enters through environment variables, never through script text or
// shell interpolation. The COM service runs as the invoking, non-elevated user.
// Only command lines that already match the expected service command are
// returned by WMI; no unrelated process arguments are read or emitted.
const PID_FILTER: &str = r#"
function Select-TaskActionPid($nodes, $candidates, $engines, $exe, $sid) {
  $byId = @{}
  foreach ($node in $nodes) { $byId[[uint32]$node.ProcessId] = $node }
  $allowed = @('"' + $exe + '" __service')
  if ($exe -notmatch '[\s"]') { $allowed += $exe + ' __service' }
  foreach ($candidate in $candidates) {
    $id = [uint32]$candidate.ProcessId
    $node = $byId[$id]
    if ($null -eq $node -or $candidate.ExecutablePath -ine $exe -or
        $candidate.OwnerSid -ine $sid -or $allowed -cnotcontains $candidate.CommandLine -or
        $null -eq $node.CreationDate -or $candidate.CreationDate -ne $node.CreationDate -or
        [uint32]$candidate.ParentProcessId -ne [uint32]$node.ParentProcessId) { continue }
    $seen = @{}; $current = $node
    while ($null -ne $current) {
      $currentId = [uint32]$current.ProcessId
      if ($seen.ContainsKey($currentId)) { break }
      $seen[$currentId] = $true
      if ($engines -contains $currentId) { $id; break }
      $parent = $byId[[uint32]$current.ParentProcessId]
      if ($null -eq $parent -or $null -eq $parent.CreationDate -or $parent.CreationDate -gt $current.CreationDate) { break }
      $current = $parent
    }
  }
}
"#;
const PREPARATION_POLICY: &str = r#"
function Get-TaskExecutable($op, $ownedExe, $desiredExe) {
  if ($op -eq 'replace') { return $desiredExe }
  return $ownedExe
}
function Get-TaskUpdatePolicy($op, $state, $matches, $compatible) {
  if ($state -ne 4) { return 'update' }
  if (!$compatible) { return 'deny' }
  if ($op -eq 'replace' -and !$matches) { return 'defer' }
  if (!$matches) { return 'deny' }
  return 'update'
}
"#;
const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false
try {
  $svc = New-Object -ComObject 'Schedule.Service'
  $svc.Connect()
  $folder = $svc.GetFolder('\')
  $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
  $name = $env:TMAX_TASK_NAME
  $marker = 'tmax-owned authority-sha256=' + $name.Substring(5)
  $task = $null
  try { $task = $folder.GetTask($name) } catch {
    $errorCode = $_.Exception.HResult
    if ($null -ne $_.Exception.InnerException) { $errorCode = $_.Exception.InnerException.HResult }
    if (($errorCode -band 0xffffffffL) -ne 0x80070002L) { throw }
  }
  $registration = 'absent'; $enabled = $false; $triggerEnabled = $false; $taskEnabled = $true; $matches = $false; $state = 0
  if ($null -ne $task) {
    $def = $task.Definition
    if ($def.RegistrationInfo.Description -cne $marker -or $def.Principal.UserId -ine $sid -or [int]$def.Principal.LogonType -ne 3) {
      @{ result='conflict' } | ConvertTo-Json -Compress; exit 0
    }
    $registration = 'owned'
    $state = [int]$task.State
    $taskEnabled = [bool]$task.Enabled
    if ($def.Triggers.Count -gt 1) { @{ result='conflict' } | ConvertTo-Json -Compress; exit 0 }
    if ($def.Triggers.Count -eq 1) {
      $trigger = $def.Triggers.Item(1)
      if ([int]$trigger.Type -ne 9 -or $trigger.UserId -ine $sid) { @{ result='conflict' } | ConvertTo-Json -Compress; exit 0 }
      $triggerEnabled = [bool]$trigger.Enabled
      $enabled = $triggerEnabled -and $taskEnabled
    }
    # A marker alone does not authorize a different action. Preserve only an
    # exact service action, and reject malformed actions before stop or update.
    if ($def.Actions.Count -ne 1 -or [int]$def.Actions.Item(1).Type -ne 0 -or
        $def.Actions.Item(1).Arguments -cne '__service' -or
        ![string]::IsNullOrEmpty($def.Actions.Item(1).WorkingDirectory) -or
        [string]::IsNullOrWhiteSpace($def.Actions.Item(1).Path) -or
        ![System.IO.Path]::IsPathRooted($def.Actions.Item(1).Path)) {
      @{ result='conflict' } | ConvertTo-Json -Compress; exit 0
    }
    $ownedExe = $def.Actions.Item(1).Path
    $env:TMAX_TASK_EXE = Get-TaskExecutable $env:TMAX_TASK_OP $ownedExe $env:TMAX_TASK_EXE
    $compatible = [int]$def.Principal.RunLevel -eq 0 -and [int]$def.Settings.MultipleInstances -eq 2 -and [bool]$def.Settings.AllowDemandStart -and $def.Settings.ExecutionTimeLimit -eq 'PT0S' -and !$def.Settings.DisallowStartIfOnBatteries -and !$def.Settings.StopIfGoingOnBatteries -and !$def.Settings.RunOnlyIfNetworkAvailable -and $def.Settings.RestartInterval -eq 'PT1M' -and [int]$def.Settings.RestartCount -eq [int]$env:TMAX_RESTART_LIMIT
    $matches = $compatible -and $ownedExe -ieq $env:TMAX_TASK_EXE
  }
  $op = $env:TMAX_TASK_OP
  if ($op -eq 'prepare' -or $op -eq 'replace' -or $op -eq 'login') {
    $want = if ($op -eq 'login') { $env:TMAX_LOGIN -eq '1' } elseif ($registration -eq 'owned') { $triggerEnabled } else { $env:TMAX_LEGACY_ENABLED -eq '1' }
    $loginChange = if ($op -eq 'login') { $want -ne $enabled } else { $want -ne $triggerEnabled }
    if ($registration -eq 'owned') {
      $policy = Get-TaskUpdatePolicy $op $state $matches $compatible
      if ($policy -eq 'deny') { @{result='running_drift'} | ConvertTo-Json -Compress; exit 0 }
      if ($policy -eq 'defer') { @{result='ok'; deferred=$true} | ConvertTo-Json -Compress; exit 0 }
    }
    if ($registration -eq 'absent' -or !$matches -or $loginChange) {
      $new = $svc.NewTask(0)
      $new.RegistrationInfo.Description = $marker
      $new.Principal.UserId = $sid; $new.Principal.LogonType = 3; $new.Principal.RunLevel = 0
      $new.Settings.Enabled = if ($op -eq 'login' -and $want) { $true } else { $taskEnabled }; $new.Settings.AllowDemandStart = $true
      $new.Settings.MultipleInstances = 2; $new.Settings.ExecutionTimeLimit = 'PT0S'
      $new.Settings.DisallowStartIfOnBatteries = $false; $new.Settings.StopIfGoingOnBatteries = $false
      $new.Settings.RestartInterval = 'PT1M'; $new.Settings.RestartCount = [int]$env:TMAX_RESTART_LIMIT
      if ($want) { $trigger = $new.Triggers.Create(9); $trigger.UserId = $sid; $trigger.Enabled = $true }
      $action = $new.Actions.Create(0); $action.Path = $env:TMAX_TASK_EXE; $action.Arguments = '__service'
      $flags = if ($registration -eq 'absent') { 2 } else { 4 }
      $task = $folder.RegisterTaskDefinition($name, $new, ($flags -bor 32), $sid, $null, 3, $null)
      $registration = 'owned'; $enabled = $want; $matches = $true
    }
  } elseif ($op -eq 'start') {
    if ($registration -ne 'owned' -or !$matches) { @{result='not_prepared'} | ConvertTo-Json -Compress; exit 0 }
    if ($state -ne 4) { $null = $task.RunEx($null, 2, [int]$env:TMAX_TASK_SESSION, $null) }
  } elseif ($op -eq 'stop') {
    if ($registration -eq 'owned') {
      # Disarm pending scheduler attempts before stopping the running action.
      $task.Enabled = $false
      try { $task.Stop(0) } catch { if ($task.GetInstances(0).Count -ne 0) { throw } }
      $deadline = [DateTime]::UtcNow.AddSeconds(5)
      while ($task.GetInstances(0).Count -ne 0 -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
      if ($task.GetInstances(0).Count -ne 0) { throw 'task still running' }
      $task.Enabled = $taskEnabled
    }
  }
  $actionPid = $null
  if ($op -eq 'inspect' -and $registration -eq 'owned' -and $matches -and $state -eq 4) {
    try {
      $collection = $task.GetInstances(0)
      $instances = @(for ($i = 1; $i -le $collection.Count; $i++) { $collection.Item($i) })
      $engines = @($instances | ForEach-Object { [uint32]$_.EnginePID } | Where-Object { $_ -ne 0 })
      $instanceKeys = @($instances | ForEach-Object { [string]$_.InstanceGuid + ':' + [string][uint32]$_.EnginePID })
      if ($engines.Count -gt 0) {
        $nodes = @(Get-CimInstance -Query 'SELECT ProcessId,ParentProcessId,CreationDate FROM Win32_Process')
        $commands = @('"' + $env:TMAX_TASK_EXE + '" __service')
        if ($env:TMAX_TASK_EXE -notmatch '[\s"]') { $commands += $env:TMAX_TASK_EXE + ' __service' }
        $candidates = @()
        foreach ($command in $commands) {
          $literal = $command.Replace('\','\\').Replace("'", "\'")
          $query = "SELECT ProcessId,ParentProcessId,CreationDate,ExecutablePath,CommandLine FROM Win32_Process WHERE CommandLine = '$literal'"
          foreach ($process in @(Get-CimInstance -Query $query)) {
            if ($process.CommandLine -cne $command -or $process.ExecutablePath -ine $env:TMAX_TASK_EXE) { continue }
            $owner = Invoke-CimMethod -InputObject $process -MethodName GetOwnerSid
            if ($owner.ReturnValue -eq 0) {
              $candidates += [pscustomobject]@{ ProcessId=$process.ProcessId; ParentProcessId=$process.ParentProcessId; CreationDate=$process.CreationDate; ExecutablePath=$process.ExecutablePath; CommandLine=$process.CommandLine; OwnerSid=$owner.Sid }
            }
          }
        }
        $selected = @(Select-TaskActionPid $nodes $candidates $engines $env:TMAX_TASK_EXE $sid | Select-Object -Unique)
        $againCollection = $task.GetInstances(0)
        $againInstances = @(for ($i = 1; $i -le $againCollection.Count; $i++) { $againCollection.Item($i) })
        $againKeys = @($againInstances | ForEach-Object { [string]$_.InstanceGuid + ':' + [string][uint32]$_.EnginePID })
        $againNodes = @(Get-CimInstance -Query 'SELECT ProcessId,ParentProcessId,CreationDate FROM Win32_Process')
        $againSelected = @(Select-TaskActionPid $againNodes $candidates $engines $env:TMAX_TASK_EXE $sid | Select-Object -Unique)
        if ($selected.Count -eq 1 -and $againSelected.Count -eq 1 -and $selected[0] -eq $againSelected[0] -and $instanceKeys.Count -eq $againKeys.Count -and @($instanceKeys | Where-Object { $againKeys -notcontains $_ }).Count -eq 0) { $actionPid = $selected[0] }
      }
    } catch { $actionPid = $null }
  }
  @{ result='ok'; registration=$registration; state=$state; matches=$matches; login_enabled=$enabled; pid=$actionPid } | ConvertTo-Json -Compress
} catch { @{result='unavailable'} | ConvertTo-Json -Compress }
"#;

fn invoke(store: &Store, op: &str, login: bool, legacy: bool) -> Result<Value> {
    let exe = std::env::current_exe().map_err(|_| unavailable())?;
    if op == "prepare" || op == "replace" {
        validate_executable(&exe)?;
    }
    let mut child = Command::new(powershell()?)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("{PID_FILTER}\n{PREPARATION_POLICY}\n{SCRIPT}"),
        ])
        .env("TMAX_TASK_NAME", task_name(store))
        .env("TMAX_TASK_EXE", exe)
        .env("TMAX_TASK_OP", op)
        .env("TMAX_LOGIN", if login { "1" } else { "0" })
        .env("TMAX_LEGACY_ENABLED", if legacy { "1" } else { "0" })
        .env("TMAX_RESTART_LIMIT", RESTART_LIMIT.to_string())
        .env("TMAX_TASK_SESSION", session_id()?.to_string())
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| unavailable())?;
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return Err(unavailable()),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(unavailable());
            }
        }
    }
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .ok_or_else(unavailable)?
        .take(16385)
        .read_to_end(&mut output)
        .map_err(|_| unavailable())?;
    if output.len() > 16384 {
        return Err(unavailable());
    }
    let value: Value = serde_json::from_slice(&output).map_err(|_| unavailable())?;
    match value["result"].as_str() {
        Some("ok") => Ok(value),
        Some("conflict") => Err(conflict()),
        Some("running_drift" | "not_prepared") => Err(failed()),
        _ if op == "inspect" || op == "probe" => Err(unavailable()),
        _ => Err(failed()),
    }
}

async fn call(store: &Store, op: &'static str, login: bool, legacy: bool) -> Result<Value> {
    let store = store.clone();
    tokio::task::spawn_blocking(move || invoke(&store, op, login, legacy))
        .await
        .map_err(|_| unavailable())?
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Legacy {
    last_owned: Option<LegacyCommand>,
}
#[derive(Deserialize)]
struct LegacyCommand {
    authority: std::path::PathBuf,
    command: String,
}

fn legacy_state(store: &Store) -> Result<&'static str> {
    type Hkey = *mut c_void;
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(
            key: Hkey,
            subkey: *const u16,
            options: u32,
            access: u32,
            result: *mut Hkey,
        ) -> i32;
        fn RegQueryValueExW(
            key: Hkey,
            name: *const u16,
            reserved: *mut u32,
            kind: *mut u32,
            data: *mut u8,
            size: *mut u32,
        ) -> i32;
        fn RegCloseKey(key: Hkey) -> i32;
    }
    struct Key(Hkey);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    let wide = |value: &str| value.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut raw = std::ptr::null_mut();
    let result = unsafe {
        RegOpenKeyExW(
            0x80000001u32 as i32 as isize as Hkey,
            wide(r"Software\Microsoft\Windows\CurrentVersion\Run").as_ptr(),
            0,
            0x20019,
            &mut raw,
        )
    };
    if result == 2 {
        return Ok("absent");
    }
    if result != 0 || raw.is_null() {
        return Err(unavailable());
    }
    let key = Key(raw);
    let name = wide("tmax");
    let mut size = 0;
    let mut kind = 0;
    let result = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if result == 2 {
        return Ok("absent");
    }
    if result != 0 || kind != 1 || size > 65536 {
        return Ok("conflict");
    }
    let mut bytes = vec![0; size as usize];
    if unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            bytes.as_mut_ptr(),
            &mut size,
        )
    } != 0
        || kind != 1
        || size as usize > bytes.len()
    {
        return Err(unavailable());
    }
    bytes.truncate(size as usize);
    let executable = std::env::current_exe().map_err(|_| unavailable())?;
    let current = format!(
        "\"{}\" __autostart",
        executable.to_str().ok_or_else(unavailable)?
    );
    let metadata: Legacy = store.read_json("autostart.json")?;
    let owned = metadata
        .last_owned
        .filter(|v| v.authority == store.root)
        .map(|v| v.command);
    let equal = |value: &str| {
        bytes.iter().copied().eq(value
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes))
    };
    Ok(if equal(&current) || owned.as_deref().is_some_and(equal) {
        "owned"
    } else {
        "conflict"
    })
}

// Remove exactly the legacy Run value, after rechecking its exact command. The
// registry API has no compare-and-delete, so reject any observed intervening write.
fn retire_legacy(store: &Store) -> Result<()> {
    type Hkey = *mut c_void;
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(
            key: Hkey,
            subkey: *const u16,
            options: u32,
            access: u32,
            result: *mut Hkey,
        ) -> i32;
        fn RegQueryValueExW(
            key: Hkey,
            name: *const u16,
            reserved: *mut u32,
            kind: *mut u32,
            data: *mut u8,
            size: *mut u32,
        ) -> i32;
        fn RegDeleteValueW(key: Hkey, name: *const u16) -> i32;
        fn RegCloseKey(key: Hkey) -> i32;
    }
    struct Key(Hkey);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut raw = std::ptr::null_mut();
    let rc = unsafe {
        RegOpenKeyExW(
            0x80000001u32 as i32 as isize as Hkey,
            w(r"Software\Microsoft\Windows\CurrentVersion\Run").as_ptr(),
            0,
            0x0103,
            &mut raw,
        )
    };
    if rc == 2 {
        return Ok(());
    }
    if rc != 0 || raw.is_null() {
        return Err(failed());
    }
    let key = Key(raw);
    let name = w("tmax");
    let read = || -> Result<Option<Vec<u8>>> {
        let mut size = 0;
        let mut kind = 0;
        let rc = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if rc == 2 {
            return Ok(None);
        }
        if rc != 0 || kind != 1 || size > 65536 {
            return Err(conflict());
        }
        let mut bytes = vec![0; size as usize];
        if unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                bytes.as_mut_ptr(),
                &mut size,
            )
        } != 0
            || kind != 1
            || size as usize > bytes.len()
        {
            return Err(conflict());
        }
        bytes.truncate(size as usize);
        Ok(Some(bytes))
    };
    let Some(existing) = read()? else {
        return Ok(());
    };
    let exe = std::env::current_exe().map_err(|_| unavailable())?;
    let exe = exe.to_str().ok_or_else(unavailable)?;
    let current = format!("\"{exe}\" __autostart");
    let legacy: Legacy = store.read_json("autostart.json")?;
    let owned = legacy
        .last_owned
        .filter(|item| item.authority == store.root)
        .map(|item| item.command);
    let equal = |s: &str| {
        existing
            .iter()
            .copied()
            .eq(s.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes))
    };
    if !equal(&current) && !owned.as_deref().is_some_and(equal) {
        return Err(conflict());
    }
    if read()?.as_deref() != Some(existing.as_slice()) {
        return Err(conflict());
    }
    let rc = unsafe { RegDeleteValueW(key.0, name.as_ptr()) };
    if rc != 0 && rc != 2 {
        return Err(failed());
    }
    Ok(())
}
fn cutover_error(error: Error) -> Error {
    Error::new(
        &error.code,
        "The scheduled task was prepared, but the legacy Run value could not be safely retired; both launch paths may remain. Inspect the registration before retrying",
    )
}

pub(super) async fn probe(store: &Store) -> Result<Manager> {
    let identity = activation()?;
    call(store, "probe", false, false).await?;
    Ok(Manager {
        name: "windows_task",
        activation: identity,
        survives_logout: Some(false),
        session_scope: "user_login",
    })
}
pub(super) async fn inspect(store: &Store) -> Result<Observation> {
    let status = call(store, "inspect", false, false).await?;
    let registration = if status["registration"] == "owned" {
        "owned"
    } else {
        "absent"
    };
    // Ready does not distinguish an idle task from a pending native retry.
    let state = if registration == "absent" {
        "stopped"
    } else {
        match status["state"].as_i64() {
            Some(4) => "running",
            Some(2) => "starting",
            Some(1) => "stopped",
            _ => "unavailable",
        }
    };
    let pid = status["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid != 0);
    Ok(Observation {
        registration,
        state,
        pid,
        attempts: None,
        last_exit_code: None,
        reason_code: None,
        retry_delay_seconds: 60,
    })
}
pub(super) async fn prepare(store: &Store, preparation: Preparation) -> Result<()> {
    let legacy = legacy_state(store)?;
    if legacy == "conflict" {
        return Err(conflict());
    }
    let op = if preparation == Preparation::Replace {
        "replace"
    } else {
        "prepare"
    };
    let result = call(store, op, false, legacy == "owned").await?;
    if result["deferred"] == true {
        return Ok(());
    }
    if legacy == "owned" {
        retire_legacy(store).map_err(cutover_error)?;
    }
    Ok(())
}
pub(super) async fn start(store: &Store) -> Result<()> {
    call(store, "start", false, false).await.map(|_| ())
}
pub(super) async fn stop(store: &Store) -> Result<()> {
    call(store, "stop", false, false).await.map(|_| ())
}
pub(super) async fn login(store: &Store, enabled: bool) -> Result<()> {
    let legacy = legacy_state(store)?;
    if legacy == "conflict" {
        return Err(conflict());
    }
    call(store, "login", enabled, legacy == "owned").await?;
    if legacy == "owned" {
        retire_legacy(store).map_err(cutover_error)?;
    }
    Ok(())
}
pub(super) fn login_status(store: &Store) -> Result<Value> {
    let status = invoke(store, "inspect", false, false)?;
    Ok(
        json!({ "registration": if status["registration"] == "owned" { if status["matches"] == true { "present" } else { "drifted" } } else { "absent" }, "registration_backend": "windows_task", "unit_matches": status["matches"], "login_enabled": status["login_enabled"], "legacy_run": legacy_state(store).ok(), "restart_interval_seconds": 60, "restart_limit": RESTART_LIMIT }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};

    #[test]
    fn replacement_must_be_an_available_native_executable() {
        let home = tempfile::tempdir().unwrap();
        let executable = home.path().join("tmax.exe");
        assert_eq!(
            validate_executable(&executable).unwrap_err().code,
            "service_executable_unavailable"
        );
        std::fs::write(&executable, b"not an executable").unwrap();
        assert!(validate_executable(&executable).is_err());
        std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        assert!(validate_executable(&executable).is_ok());
    }

    #[test]
    fn running_task_preserves_action_until_explicit_restart_stops_it() {
        let checks = r#"
$ErrorActionPreference='Stop'
$a='C:\old\tmax.exe'; $b='C:\new\tmax.exe'
foreach ($op in @('prepare','login','start','inspect','stop')) {
  if ((Get-TaskExecutable $op $a $b) -ne $a) { throw "$op changed the existing action" }
}
if ((Get-TaskExecutable 'replace' $a $b) -ne $b) { throw 'replacement lost desired action' }
if ((Get-TaskUpdatePolicy 'prepare' 4 $true $true) -ne 'update') { throw 'ordinary prepare rejected' }
if ((Get-TaskUpdatePolicy 'login' 4 $true $true) -ne 'update') { throw 'login reconciliation rejected' }
if ((Get-TaskUpdatePolicy 'replace' 4 $false $true) -ne 'defer') { throw 'live replacement was not deferred' }
if ((Get-TaskUpdatePolicy 'replace' 3 $false $true) -ne 'update') { throw 'stopped replacement was not applied' }
if ((Get-TaskUpdatePolicy 'replace' 4 $false $false) -ne 'deny') { throw 'live policy drift was accepted' }
if ((Get-TaskUpdatePolicy 'login' 4 $false $false) -ne 'deny') { throw 'login recycled a drifted task' }
"#;
        let status = Command::new(powershell().unwrap())
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!("{PREPARATION_POLICY}\n{checks}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn action_identity_rejects_ambiguous_wrong_command_and_wrong_ancestry() {
        let checks = r#"
$ErrorActionPreference='Stop'
$exe='C:\tmax.exe'; $sid='S-1-5-21-123'
$nodes=@(
 [pscustomobject]@{ProcessId=10;ParentProcessId=1;CreationDate=1},
 [pscustomobject]@{ProcessId=20;ParentProcessId=10;CreationDate=2},
 [pscustomobject]@{ProcessId=21;ParentProcessId=10;CreationDate=3},
 [pscustomobject]@{ProcessId=30;ParentProcessId=1;CreationDate=2})
$good=[pscustomobject]@{ProcessId=20;ParentProcessId=10;CreationDate=2;ExecutablePath=$exe;CommandLine='"C:\tmax.exe" __service';OwnerSid=$sid}
$other=[pscustomobject]@{ProcessId=21;ParentProcessId=10;CreationDate=3;ExecutablePath=$exe;CommandLine='"C:\tmax.exe" __service';OwnerSid=$sid}
$wrongArg=[pscustomobject]@{ProcessId=20;ParentProcessId=10;CreationDate=2;ExecutablePath=$exe;CommandLine='"C:\tmax.exe" __service extra';OwnerSid=$sid}
$wrongParent=[pscustomobject]@{ProcessId=30;ParentProcessId=1;CreationDate=2;ExecutablePath=$exe;CommandLine='"C:\tmax.exe" __service';OwnerSid=$sid}
$wrongSid=[pscustomobject]@{ProcessId=20;ParentProcessId=10;CreationDate=2;ExecutablePath=$exe;CommandLine='"C:\tmax.exe" __service';OwnerSid='S-1-5-21-456'}
$reused=[pscustomobject]@{ProcessId=20;ParentProcessId=10;CreationDate=9;ExecutablePath=$exe;CommandLine='"C:\tmax.exe" __service';OwnerSid=$sid}
if (@(Select-TaskActionPid $nodes @($good) @(10) $exe $sid).Count -ne 1) { throw 'valid action not selected' }
if (@(Select-TaskActionPid $nodes @($good,$other) @(10) $exe $sid).Count -ne 2) { throw 'ambiguity lost' }
foreach ($candidate in @($wrongArg,$wrongParent,$wrongSid,$reused)) {
 if (@(Select-TaskActionPid $nodes @($candidate) @(10) $exe $sid).Count -ne 0) { throw 'invalid action selected' }
}
"#;
        let status = Command::new(powershell().unwrap())
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!("{PID_FILTER}\n{checks}"),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .unwrap();
        assert!(status.success());
    }

    // This intentionally uses an isolated temporary task, not the user's tmax
    // task. It takes over two minutes and must be run in an interactive Windows
    // session. A passing result qualifies this scheduler's on-demand behavior;
    // a real logon-triggered recovery still requires a separate logon scenario.
    #[test]
    #[ignore = "requires an interactive Windows Task Scheduler session and takes over two minutes"]
    fn scheduler_recovers_an_on_demand_crash_and_cancels_pending_recovery() {
        let temp = tempfile::tempdir().unwrap();
        let log = temp.path().join("attempts.txt");
        let ps = powershell().unwrap();
        let quoted_log = log.to_string_lossy().replace('\'', "''");
        let action = format!(
            "[IO.File]::AppendAllText('{quoted_log}', \"attempt`n\"); [Diagnostics.Process]::GetCurrentProcess().Kill()"
        );
        let encoded = STANDARD.encode(
            action
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let script = r#"
$ErrorActionPreference='Stop'
$svc=New-Object -ComObject 'Schedule.Service'; $svc.Connect(); $folder=$svc.GetFolder('\')
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$name=$env:TMAX_QUALIFICATION_TASK
try {
  $definition=$svc.NewTask(0)
  $definition.Principal.UserId=$sid; $definition.Principal.LogonType=3
  $definition.Settings.AllowDemandStart=$true; $definition.Settings.ExecutionTimeLimit='PT0S'
  $definition.Settings.DisallowStartIfOnBatteries=$false; $definition.Settings.StopIfGoingOnBatteries=$false
  $definition.Settings.MultipleInstances=2; $definition.Settings.RestartInterval='PT1M'; $definition.Settings.RestartCount=5
  $action=$definition.Actions.Create(0); $action.Path=$env:TMAX_QUALIFICATION_PS
  $action.Arguments='-NoLogo -NoProfile -NonInteractive -EncodedCommand '+$env:TMAX_QUALIFICATION_ACTION
  $task=$folder.RegisterTaskDefinition($name,$definition,2,$sid,$null,3,$null)
  $null=$task.RunEx($null,2,[int]$env:TMAX_QUALIFICATION_SESSION,$null)
  $deadline=[DateTime]::UtcNow.AddSeconds(100)
  do {
    Start-Sleep -Milliseconds 250
    $count=if (Test-Path -LiteralPath $env:TMAX_QUALIFICATION_LOG) { @(Get-Content -LiteralPath $env:TMAX_QUALIFICATION_LOG).Count } else { 0 }
  } while ($count -lt 2 -and [DateTime]::UtcNow -lt $deadline)
  if ($count -lt 2) { throw 'on-demand crash did not recover' }
  Start-Sleep -Seconds 3
  $task.Enabled=$false
  try { $task.Stop(0) } catch { if ([int]$task.State -eq 4) { throw } }
  $task.Enabled=$true
  $before=@(Get-Content -LiteralPath $env:TMAX_QUALIFICATION_LOG).Count
  Start-Sleep -Seconds 75
  $after=@(Get-Content -LiteralPath $env:TMAX_QUALIFICATION_LOG).Count
  if ($after -ne $before) { throw 'pending recovery survived explicit stop' }
} finally {
  try { $task.Enabled=$false; $task.Stop(0) } catch { }
  try { $folder.DeleteTask($name,0) } catch { }
}
"#;
        let status = Command::new(ps.clone())
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ])
            .env(
                "TMAX_QUALIFICATION_TASK",
                format!("tmax-qualification-{}", uuid::Uuid::new_v4()),
            )
            .env("TMAX_QUALIFICATION_PS", ps)
            .env("TMAX_QUALIFICATION_ACTION", encoded)
            .env(
                "TMAX_QUALIFICATION_SESSION",
                session_id().unwrap().to_string(),
            )
            .env("TMAX_QUALIFICATION_LOG", log)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status()
            .unwrap();
        assert!(
            status.success(),
            "Task Scheduler did not satisfy recovery and stop suppression"
        );
    }
}
