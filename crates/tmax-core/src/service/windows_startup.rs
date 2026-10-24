//! Current-user Windows startup registration and browser launching.
//!
//! The only registry mutation is the `tmax` value in HKCU's Run key. A command
//! must match this executable or this authority's last successfully owned command
//! before it can be replaced or removed.
use crate::{Error, Result, storage::Store};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{ffi::c_void, net::SocketAddr, path::PathBuf, ptr};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "tmax";
const REG_SZ: u32 = 1;
const ERROR_FILE_NOT_FOUND: i32 = 2;
const MAX_VALUE_BYTES: usize = 64 * 1024;
// Run's documented 260-character command limit excludes the registry string's
// terminating NUL. Count UTF-16 code units, including executable quotes and args.
const MAX_COMMAND_UNITS: usize = 260;
const REPAIR: &str = "proxy autostart enable|disable";

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
    fn RegCreateKeyExW(
        key: Hkey,
        subkey: *const u16,
        reserved: u32,
        class: *mut u16,
        options: u32,
        access: u32,
        security: *const c_void,
        result: *mut Hkey,
        disposition: *mut u32,
    ) -> i32;
    fn RegQueryValueExW(
        key: Hkey,
        name: *const u16,
        reserved: *mut u32,
        kind: *mut u32,
        data: *mut u8,
        size: *mut u32,
    ) -> i32;
    fn RegSetValueExW(
        key: Hkey,
        name: *const u16,
        reserved: u32,
        kind: u32,
        data: *const u8,
        size: u32,
    ) -> i32;
    fn RegDeleteValueW(key: Hkey, name: *const u16) -> i32;
    fn RegCloseKey(key: Hkey) -> i32;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        window: *mut c_void,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> *mut c_void;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn FreeConsole() -> i32;
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn unavailable() -> Error {
    Error::new(
        "autostart_unavailable",
        "Cannot access the current user's bounded Windows startup registration",
    )
}

fn conflict() -> Error {
    Error::new(
        "autostart_conflict",
        "Startup registration is not owned by this authority; unrelated entries were not changed",
    )
}

struct Key(Hkey);
impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: this key is the unique owner of a successful registry open.
        unsafe { RegCloseKey(self.0) };
    }
}
impl Key {
    fn open(write: bool, create: bool) -> Result<Option<Self>> {
        // Use the same registry view from both 32-bit and 64-bit executables.
        // KEY_WOW64_64KEY is ignored on 32-bit Windows.
        let access = 0x0100 | if write { 0x0003 } else { 0x0001 };
        let name = wide(RUN_KEY);
        let mut key = ptr::null_mut();
        // Win32 predefined HKEY values are sign-extended pointer-sized constants.
        let current_user = 0x8000_0001_u32 as i32 as isize as Hkey;
        // SAFETY: terminated input and writable output live across the FFI call.
        let result = unsafe {
            if create {
                RegCreateKeyExW(
                    current_user,
                    name.as_ptr(),
                    0,
                    ptr::null_mut(),
                    0,
                    access,
                    ptr::null(),
                    &mut key,
                    ptr::null_mut(),
                )
            } else {
                RegOpenKeyExW(current_user, name.as_ptr(), 0, access, &mut key)
            }
        };
        match result {
            0 if !key.is_null() => Ok(Some(Self(key))),
            ERROR_FILE_NOT_FOUND if !create => Ok(None),
            _ => Err(unavailable()),
        }
    }

    fn read(&self) -> Result<Option<RegistryValue>> {
        let name = wide(RUN_VALUE);
        let mut kind = 0;
        let mut size = 0;
        // SAFETY: the key is live, and query size/type outputs are valid.
        let result = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                ptr::null_mut(),
                &mut kind,
                ptr::null_mut(),
                &mut size,
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if result != 0 || size as usize > MAX_VALUE_BYTES {
            return Err(unavailable());
        }
        let mut bytes = vec![0_u8; size as usize];
        // SAFETY: bytes has exactly the advertised writable capacity. A value
        // growing between queries fails closed rather than allocating unboundedly.
        let result = unsafe {
            RegQueryValueExW(
                self.0,
                name.as_ptr(),
                ptr::null_mut(),
                &mut kind,
                bytes.as_mut_ptr(),
                &mut size,
            )
        };
        if result != 0 || size as usize > bytes.len() {
            return Err(unavailable());
        }
        bytes.truncate(size as usize);
        Ok(Some(RegistryValue { kind, bytes }))
    }

    fn write(&self, command: &str) -> Result<()> {
        let name = wide(RUN_VALUE);
        let value = RegistryValue::command(command);
        if value.bytes.len() > MAX_VALUE_BYTES {
            return Err(unavailable());
        }
        // SAFETY: both buffers live for the call; the byte count includes NUL.
        if unsafe {
            RegSetValueExW(
                self.0,
                name.as_ptr(),
                0,
                REG_SZ,
                value.bytes.as_ptr(),
                value.bytes.len() as u32,
            )
        } != 0
        {
            return Err(Error::new(
                "autostart_failed",
                "Cannot write this authority's Windows startup registration",
            ));
        }
        Ok(())
    }

    fn delete(&self) -> Result<()> {
        let name = wide(RUN_VALUE);
        // SAFETY: this deletes just the named value, never the key or its peers.
        let result = unsafe { RegDeleteValueW(self.0, name.as_ptr()) };
        if result != 0 && result != ERROR_FILE_NOT_FOUND {
            return Err(Error::new(
                "autostart_failed",
                "Cannot remove this authority's Windows startup registration",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
struct RegistryValue {
    kind: u32,
    bytes: Vec<u8>,
}
impl RegistryValue {
    fn command(command: &str) -> Self {
        Self {
            kind: REG_SZ,
            bytes: command
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes)
                .collect(),
        }
    }

    fn matches(&self, command: &str) -> bool {
        self.kind == REG_SZ
            && !command.contains('\0')
            && self.bytes.iter().copied().eq(command
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_le_bytes))
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Startup {
    acknowledged: Option<bool>,
    last_outcome: Option<String>,
    last_attempt: Option<chrono::DateTime<chrono::Utc>>,
    last_owned: Option<OwnedCommand>,
    last_launch: Option<Launch>,
}

#[derive(Serialize, Deserialize)]
struct OwnedCommand {
    authority: PathBuf,
    command: String,
}

#[derive(Serialize, Deserialize)]
struct Launch {
    attempted_at: chrono::DateTime<chrono::Utc>,
    outcome: String,
    reason_code: Option<String>,
    next_action: Option<String>,
}

fn expected_command() -> Result<String> {
    let executable = std::env::current_exe().map_err(|_| unavailable())?;
    let path = executable.to_str().ok_or_else(unavailable)?;
    quote_command(path)
}

fn quote_command(path: &str) -> Result<String> {
    // Windows executable paths cannot contain quotes. Reject controls instead
    // of ever creating an ambiguous Run command; backslashes remain literal.
    if path.is_empty()
        || path.chars().any(|ch| ch == '"' || ch.is_control())
        || path.ends_with('\\')
    {
        return Err(Error::new(
            "autostart_unavailable",
            "Executable path cannot be safely quoted for Windows startup registration",
        ));
    }
    let command = format!("\"{path}\" __autostart");
    if command.encode_utf16().count() > MAX_COMMAND_UNITS {
        return Err(unavailable());
    }
    Ok(command)
}

fn unchanged(state: &Startup, enabled: bool, force: bool) -> bool {
    !force && (state.acknowledged == Some(enabled) || (state.acknowledged.is_none() && !enabled))
}

fn registration_state(
    existing: Option<&RegistryValue>,
    expected: &str,
    state: &Startup,
    store: &Store,
) -> &'static str {
    let Some(existing) = existing else {
        return "absent";
    };
    if existing.matches(expected) {
        "present"
    } else if state
        .last_owned
        .as_ref()
        .is_some_and(|last| last.authority == store.root && existing.matches(&last.command))
    {
        "drifted"
    } else {
        "conflict"
    }
}

pub(super) fn status(store: &Store) -> Result<Value> {
    let state: Startup = store.read_json("autostart.json")?;
    let expected = expected_command()?;
    let existing = Key::open(false, false)?
        .map(|key| key.read())
        .transpose()?
        .flatten();
    let registration = registration_state(existing.as_ref(), &expected, &state, store);
    let desired = crate::config::load(store)
        .ok()
        .and_then(|config| config.config.proxy.map(|proxy| proxy.autostart));
    Ok(json!({
        "saved_intent": desired,
        "acknowledged_intent": state.acknowledged,
        "registration": registration,
        "registration_backend": "hkcu_run",
        "command_matches": registration == "present",
        "login_enabled": registration == "present",
        "last_outcome": state.last_outcome,
        "last_attempt": state.last_attempt,
        "last_launch": state.last_launch,
        "repair": REPAIR,
        "reconciliation": "not_requested"
    }))
}

pub(super) async fn reconcile(
    store: &Store,
    enabled: bool,
    dry_run: bool,
    force: bool,
) -> Result<Value> {
    let mut state: Startup = store.read_json("autostart.json")?;
    if unchanged(&state, enabled, force) {
        return status(store);
    }
    if dry_run {
        return Ok(json!({
            "dry_run": true,
            "enabled": enabled,
            "registration": "planned"
        }));
    }
    let _lock = store.lock("autostart")?;
    state = store.read_json("autostart.json")?;
    if unchanged(&state, enabled, force) {
        return status(store);
    }
    let expected = expected_command()?;
    state.acknowledged = Some(enabled);
    state.last_attempt = Some(chrono::Utc::now());
    state.last_outcome = Some("pending".into());
    store.write_json("autostart.json", &state)?;
    let mut effects = Vec::new();
    let result = (|| -> Result<()> {
        // Disabling never creates a registry key. Opening the existing Run key
        // does not enumerate or touch any other startup value.
        let Some(key) = Key::open(true, enabled)? else {
            return Ok(());
        };
        let existing = key.read()?;
        if registration_state(existing.as_ref(), &expected, &state, store) == "conflict" {
            return Err(conflict());
        }
        // Detect an observed intervening writer before applying our operation.
        // Win32 has no compare-and-set operation for a registry value.
        if key.read()? != existing {
            return Err(conflict());
        }
        if enabled {
            key.write(&expected)?;
            effects.push("owned_run_value_written");
            state.last_owned = Some(OwnedCommand {
                authority: store.root.clone(),
                command: expected,
            });
        } else if existing.is_some() {
            key.delete()?;
            effects.push("owned_run_value_removed");
            state.last_owned = None;
        }
        Ok(())
    })();
    state.last_outcome = Some(
        if result.is_ok() {
            "reconciled"
        } else {
            "failed"
        }
        .into(),
    );
    let metadata = store.write_json("autostart.json", &state);
    Ok(json!({
        "_code": if result.is_err() || metadata.is_err() { json!("saved_not_active") } else { Value::Null },
        "reconciliation": if result.is_ok() { "reconciled" } else { "failed" },
        "acknowledged_intent": enabled,
        "completed_effects": effects,
        "error": result.err(),
        "outcome_recording_error": metadata.err(),
        "status": status(store).ok(),
        "repair": REPAIR
    }))
}

fn launch_outcome(error: Option<&Error>) -> Launch {
    let (outcome, reason, action) = match error.map(|error| error.code.as_str()) {
        None => ("ready", None, None),
        Some("credential_unavailable") => (
            "blocked",
            Some("credential_unavailable"),
            Some("Inspect stored key availability; re-enter or replace missing key material"),
        ),
        Some("storage_version_unsupported") => (
            "blocked",
            Some("storage_version_unsupported"),
            Some(
                "Preserve the unsupported stored record; replace the shared key through proxy key replace",
            ),
        ),
        Some("storage_record_invalid") => (
            "blocked",
            Some("storage_record_invalid"),
            Some(
                "Preserve the corrupt or unsupported stored key file; explicitly replace the shared key",
            ),
        ),
        Some("autostart_blocked") => (
            "blocked",
            Some("autostart_blocked"),
            Some(
                "Inspect saved intent, stored key availability, and endpoint state, then explicitly start proxy",
            ),
        ),
        Some("endpoint_occupied") => (
            "blocked",
            Some("endpoint_occupied"),
            Some("Resolve the configured endpoint conflict, then explicitly start proxy"),
        ),
        Some(_) => (
            "failed",
            Some("autostart_failed"),
            Some("Inspect configuration and resident status, then explicitly start proxy"),
        ),
    };
    // Only fixed safe categories are persisted, never error messages, fields,
    // affected-resource labels or user-provided strings.
    Launch {
        attempted_at: chrono::Utc::now(),
        outcome: outcome.into(),
        reason_code: reason.map(str::to_owned),
        next_action: action.map(str::to_owned),
    }
}

/// Record immediately after startup readiness is proven, or when it fails.
/// Successful registration alone is not evidence of a ready inference listener.
pub(super) fn record_launch(store: &Store, error: Option<&Error>) -> Result<()> {
    let _lock = store.lock("autostart")?;
    let mut state: Startup = store.read_json("autostart.json")?;
    state.last_launch = Some(launch_outcome(error));
    store.write_json("autostart.json", &state)
}

fn generated_loopback_url(url: &str) -> bool {
    let Some(authority) = url.strip_prefix("http://") else {
        return false;
    };
    let Ok(address) = authority.parse::<SocketAddr>() else {
        return false;
    };
    address.ip().is_loopback() && address.port() != 0 && authority == address.to_string()
}

pub(super) fn open_browser(url: &str) -> bool {
    if !generated_loopback_url(url) {
        return false;
    }
    let operation = wide("open");
    let url = wide(url);
    // SAFETY: the verb and validated URL are terminated strings. No shell,
    // parameters, environment expansion or executable interpolation is involved.
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            url.as_ptr(),
            ptr::null(),
            ptr::null(),
            1,
        )
    };
    result as isize > 32
}

/// Detach only the hidden startup entrypoint, preserving normal CLI consoles.
pub(super) fn detach_console() {
    // SAFETY: FreeConsole has no pointer arguments; an absent console is harmless.
    unsafe { FreeConsole() };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::at(PathBuf::from(r"C:\Users\startup-test"))
    }

    #[test]
    fn startup_command_quotes_exactly_one_unicode_executable() {
        assert_eq!(
            quote_command(r"C:\Program Files\模型\tmax.exe").unwrap(),
            "\"C:\\Program Files\\模型\\tmax.exe\" __autostart"
        );
        for invalid in [
            "",
            "C:\\tmax.exe\n",
            "C:\\bad\"name.exe",
            "C:\\path\\",
            "x\0y",
        ] {
            assert!(quote_command(invalid).is_err());
        }
        assert!(quote_command(&"x".repeat(MAX_VALUE_BYTES)).is_err());
    }

    #[test]
    fn startup_command_respects_run_limit_in_utf16_units_excluding_terminator() {
        let overhead = "\"\" __autostart".encode_utf16().count();
        let path = "x".repeat(MAX_COMMAND_UNITS - overhead);
        let command = quote_command(&path).unwrap();
        assert_eq!(command.encode_utf16().count(), MAX_COMMAND_UNITS);
        assert_eq!(wide(&command).len(), MAX_COMMAND_UNITS + 1);
        assert!(quote_command(&format!("{path}x")).is_err());

        let unicode_path = format!("{}😀", "x".repeat(MAX_COMMAND_UNITS - overhead - 2));
        let command = quote_command(&unicode_path).unwrap();
        assert_eq!(command.encode_utf16().count(), MAX_COMMAND_UNITS);
        assert!(quote_command(&format!("{unicode_path}x")).is_err());
    }

    #[test]
    fn browser_accepts_only_canonical_generated_numeric_loopback_urls() {
        for url in [
            "http://127.0.0.1:17840",
            "http://127.3.2.1:1",
            "http://[::1]:65535",
        ] {
            assert!(generated_loopback_url(url), "{url}");
        }
        for url in [
            "http://localhost:17840",
            "https://127.0.0.1:17840",
            "http://127.0.0.1:0",
            "http://192.168.0.1:17840",
            "http://[::]:17840",
            "http://127.0.0.1:17840/",
            "http://127.0.0.1:17840?run=command",
            "http://127.0.0.1:17840#fragment",
            "http://user@127.0.0.1:17840",
            "http://127.0.0.1:17840\0extra",
            "http://127.0.0.1:17840 & calc.exe",
        ] {
            assert!(!generated_loopback_url(url), "{url}");
        }
    }

    #[test]
    fn ownership_requires_exact_command_and_matching_metadata_authority() {
        let store = store();
        let expected = quote_command(r"C:\new\tmax.exe").unwrap();
        let old = quote_command(r"C:\old\tmax.exe").unwrap();
        let mut state = Startup {
            last_owned: Some(OwnedCommand {
                authority: store.root.clone(),
                command: old.clone(),
            }),
            ..Startup::default()
        };
        assert_eq!(
            registration_state(None, &expected, &state, &store),
            "absent"
        );
        assert_eq!(
            registration_state(
                Some(&RegistryValue::command(&expected)),
                &expected,
                &state,
                &store
            ),
            "present"
        );
        assert_eq!(
            registration_state(
                Some(&RegistryValue::command(&old)),
                &expected,
                &state,
                &store
            ),
            "drifted"
        );
        for foreign in [
            format!("{old} extra"),
            "tmax.exe __autostart".into(),
            "unrelated startup command".into(),
        ] {
            assert_eq!(
                registration_state(
                    Some(&RegistryValue::command(&foreign)),
                    &expected,
                    &state,
                    &store
                ),
                "conflict"
            );
        }
        state.last_owned.as_mut().unwrap().authority = PathBuf::from(r"C:\Users\another\.tmax");
        assert_eq!(
            registration_state(
                Some(&RegistryValue::command(&old)),
                &expected,
                &state,
                &store
            ),
            "conflict"
        );
    }

    #[test]
    fn malformed_registry_values_never_prove_ownership() {
        let valid = RegistryValue::command("tmax");
        assert!(valid.matches("tmax"));
        let mut unterminated = valid.bytes.clone();
        unterminated.truncate(unterminated.len() - 2);
        let mut extra_terminator = valid.bytes.clone();
        extra_terminator.extend_from_slice(&[0, 0]);
        for value in [
            RegistryValue {
                kind: 2,
                bytes: valid.bytes.clone(),
            },
            RegistryValue {
                kind: REG_SZ,
                bytes: unterminated,
            },
            RegistryValue {
                kind: REG_SZ,
                bytes: extra_terminator,
            },
            RegistryValue {
                kind: REG_SZ,
                bytes: vec![],
            },
            RegistryValue {
                kind: REG_SZ,
                bytes: vec![0],
            },
            RegistryValue {
                kind: REG_SZ,
                bytes: vec![0, 0xd8, 0, 0],
            },
        ] {
            assert!(!value.matches("tmax"));
        }
        assert!(!RegistryValue::command("tmax\0ignored").matches("tmax\0ignored"));
    }

    #[test]
    fn acknowledged_intent_does_not_implicitly_repair_drift_or_failure() {
        let mut state = Startup::default();
        assert!(unchanged(&state, false, false));
        assert!(!unchanged(&state, true, false));
        state.acknowledged = Some(true);
        state.last_outcome = Some("failed".into());
        assert!(unchanged(&state, true, false));
        assert!(!unchanged(&state, true, true));
        assert!(!unchanged(&state, false, false));
        state.acknowledged = Some(false);
        assert!(unchanged(&state, false, false));
        assert!(!unchanged(&state, false, true));
    }

    #[test]
    fn launch_record_is_blocked_for_unavailable_credentials_and_never_keeps_error_details() {
        let unavailable = Error::new("credential_unavailable", "sensitive supplied bytes");
        let launch = launch_outcome(Some(&unavailable));
        assert_eq!(launch.outcome, "blocked");
        assert_eq!(
            launch.reason_code.as_deref(),
            Some("credential_unavailable")
        );
        assert!(
            launch
                .next_action
                .as_deref()
                .unwrap()
                .contains("stored key availability")
        );
        let arbitrary = Error::new("sensitive-code", "sensitive-message").field("sensitive-field");
        let serialized = serde_json::to_string(&launch_outcome(Some(&arbitrary))).unwrap();
        assert!(!serialized.contains("sensitive"));
        let ready = launch_outcome(None);
        assert_eq!(ready.outcome, "ready");
        assert!(ready.reason_code.is_none());
        assert!(ready.next_action.is_none());
    }

    #[test]
    fn existing_acknowledgment_metadata_remains_readable() {
        let state: Startup = serde_json::from_str(
            r#"{"acknowledged":true,"last_outcome":"reconciled","last_attempt":null}"#,
        )
        .unwrap();
        assert_eq!(state.acknowledged, Some(true));
        assert!(state.last_owned.is_none());
        assert!(state.last_launch.is_none());
    }
}
