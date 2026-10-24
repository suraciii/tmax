//! Same-user, local-only Windows transport for the existing resident protocol.
use super::{CONTROL_SECONDS, failure};
use crate::{Result, storage::Store};
use sha2::{Digest, Sha256};
use std::{
    ffi::c_void,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    time::Duration,
};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};

pub(super) type Client = NamedPipeClient;
pub(super) struct Server(NamedPipeServer);
impl std::ops::Deref for Server {
    type Target = NamedPipeServer;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Server {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        // A connected client can otherwise retain an old named-pipe instance
        // even after the server handle closes. Explicitly detach that client.
        let _ = self.0.disconnect();
    }
}
pub(super) async fn confirm_client_closed(stream: &mut Server) -> std::io::Result<()> {
    use tokio::io::AsyncReadExt;
    let mut byte = [0];
    match stream.read(&mut byte).await {
        Ok(0) => Ok(()),
        Err(error) if error.raw_os_error() == Some(109) => Ok(()),
        _ => Err(std::io::Error::other(
            "Expected client closure after one complete control reply",
        )),
    }
}
type Handle = *mut c_void;
#[repr(C)]
struct SecurityAttributes {
    length: u32,
    descriptor: Handle,
    inherit: i32,
}
#[repr(C)]
struct SidAndAttributes {
    sid: Handle,
    attributes: u32,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn GetStdHandle(which: u32) -> Handle;
    fn GetFileType(handle: Handle) -> u32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn GetNamedPipeServerProcessId(pipe: Handle, pid: *mut u32) -> i32;
    fn GetNamedPipeClientProcessId(pipe: Handle, pid: *mut u32) -> i32;
    fn LocalFree(value: Handle) -> Handle;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(
        token: Handle,
        class: u32,
        data: Handle,
        length: u32,
        needed: *mut u32,
    ) -> i32;
    fn ConvertSidToStringSidW(sid: Handle, text: *mut *mut u16) -> i32;
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        text: *const u16,
        revision: u32,
        descriptor: *mut Handle,
        length: *mut u32,
    ) -> i32;
}
pub(super) fn stdin_is_pipe() -> bool {
    unsafe { GetFileType(GetStdHandle((-10i32) as u32)) == 3 }
}
fn identity_error() -> crate::Error {
    failure(
        "resident_identity_unavailable",
        "Cannot establish same-user Windows control peer identity; no control action performed",
    )
}

fn process_sid(process: Handle) -> Result<String> {
    let mut raw = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process, 0x0008, &mut raw) } == 0 {
        return Err(identity_error());
    }
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut needed = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            1,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 || needed > 65536 {
        return Err(identity_error());
    }
    // TOKEN_USER contains a pointer and must be pointer-aligned.
    let mut data = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            1,
            data.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err(identity_error());
    }
    let user = unsafe { &*data.as_ptr().cast::<SidAndAttributes>() };
    let mut text = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(user.sid, &mut text) } == 0 {
        return Err(identity_error());
    }
    let mut length = 0;
    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
    }
    let result = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
        .map_err(|_| identity_error());
    unsafe {
        LocalFree(text.cast());
    }
    result
}
fn current_sid() -> Result<String> {
    process_sid(unsafe { GetCurrentProcess() })
}
fn name(store: &Store, sid: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(sid.as_bytes());
    for unit in store.root.as_os_str().encode_wide() {
        hash.update(unit.to_le_bytes());
    }
    format!(r"\\.\pipe\tmax-{:x}", hash.finalize())
}
fn verify_process(pid: u32, access: u32) -> Result<OwnedHandle> {
    if pid == 0 {
        return Err(identity_error());
    }
    let raw = unsafe { OpenProcess(access, 0, pid) };
    if raw.is_null() {
        return Err(identity_error());
    }
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    if process_sid(process.as_raw_handle())? != current_sid()? {
        return Err(identity_error());
    }
    Ok(process)
}
pub(super) struct Peer {
    pub(super) pid: u32,
    process: OwnedHandle,
}
impl Peer {
    pub(super) async fn wait_for_exit(self) -> Result<()> {
        // Web operations call the same resident through private RPC. A process
        // cannot wait for its own exit; external CLI callers provide this last
        // process/namespace completion boundary instead.
        if self.pid == std::process::id() {
            return Ok(());
        }
        // Retain the exact authenticated process object, never reopen a PID after
        // the reply. Named-pipe server instances outlive dropping the pending
        // listener, so final stop completes only after all owner handles close.
        let completed = tokio::task::spawn_blocking(move || unsafe {
            WaitForSingleObject(
                self.process.as_raw_handle(),
                (CONTROL_SECONDS * 1000) as u32,
            )
        })
        .await
        .map_err(|_| {
            failure(
                "resident_control_failed",
                "Cannot await Windows resident completion",
            )
        })?;
        if completed != 0 {
            return Err(failure(
                "resident_control_failed",
                "Windows resident did not complete endpoint release within its finite shutdown deadline",
            ));
        }
        Ok(())
    }
}
pub(super) fn peer(stream: &Client) -> Result<Peer> {
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(stream.as_raw_handle(), &mut pid) } == 0 {
        return Err(identity_error());
    }
    let process = verify_process(pid, 0x1000 | 0x00100000)?;
    Ok(Peer { pid, process })
}
pub(super) fn authorize_client(stream: &Server) -> Result<()> {
    let mut pid = 0;
    if unsafe { GetNamedPipeClientProcessId(stream.as_raw_handle(), &mut pid) } == 0 {
        return Err(identity_error());
    }
    verify_process(pid, 0x1000).map(|_| ())
}
pub(super) async fn connect(store: &Store) -> Result<Option<Client>> {
    let name = name(store, &current_sid()?);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(CONTROL_SECONDS);
    loop {
        // Identification is enough for peer checks; do not grant arbitrary pipe
        // servers permission to impersonate this client for other operations.
        match ClientOptions::new()
            .security_qos_flags(0x00010000)
            .open(&name)
        {
            Ok(stream) => return Ok(Some(stream)),
            Err(error) if error.raw_os_error() == Some(2) => return Ok(None),
            Err(error) if error.raw_os_error() == Some(231) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(failure(
                        "resident_identity_unavailable",
                        "Windows resident control endpoint remained busy within its finite connection deadline",
                    ));
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(_) => {
                return Err(failure(
                    "resident_identity_unavailable",
                    "Cannot connect to the Windows resident control endpoint; no takeover attempted",
                ));
            }
        }
    }
}
struct SecurityDescriptor(Handle);
impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn create(name: &str, sid: &str, first: bool) -> Result<Server> {
    let text: Vec<u16> = format!("O:{sid}D:P(A;;GA;;;{sid})\0")
        .encode_utf16()
        .collect();
    let mut descriptor = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(identity_error());
    }
    let descriptor = SecurityDescriptor(descriptor);
    let mut attributes = SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>() as u32,
        descriptor: descriptor.0,
        inherit: 0,
    };
    unsafe { ServerOptions::new().first_pipe_instance(first).reject_remote_clients(true).create_with_security_attributes_raw(name, (&mut attributes as *mut SecurityAttributes).cast()) }
        .map(Server)
        .map_err(|_| failure("endpoint_occupied", "Windows resident control endpoint is unavailable or already owned; no fallback or takeover attempted"))
}

pub(super) struct Listener {
    pending: Server,
    name: String,
    sid: String,
}
impl Listener {
    pub(super) async fn bind(store: &Store) -> Result<Self> {
        let sid = current_sid()?;
        let name = name(store, &sid);
        let pending = create(&name, &sid, true)?;
        Ok(Self { pending, name, sid })
    }
    pub(super) async fn accept(&mut self) -> Result<Server> {
        self.pending.connect().await.map_err(|_| {
            failure(
                "resident_control_failed",
                "Windows resident control accept failed",
            )
        })?;
        // Keep the namespace continuously owned, including while an accepted
        // connection waits for the shared shutdown completion acknowledgement.
        let next = create(&self.name, &self.sid, false)?;
        Ok(std::mem::replace(&mut self.pending, next))
    }
    pub(super) fn release(self) -> Result<()> {
        drop(self);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_is_short_authority_and_user_specific() {
        let first = Store::at(std::path::PathBuf::from(r"C:\Users\one"));
        let second = Store::at(std::path::PathBuf::from(r"C:\Users\two"));
        assert_ne!(name(&first, "S-1-5-21-1"), name(&second, "S-1-5-21-1"));
        assert_ne!(name(&first, "S-1-5-21-1"), name(&first, "S-1-5-21-2"));
        assert!(name(&first, "S-1-5-21-1").len() < 100);
    }
    #[tokio::test]
    async fn same_user_peer_is_qualified_and_namespace_cannot_be_taken_over() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        let mut listener = Listener::bind(&store).await.unwrap();
        assert!(Listener::bind(&store).await.is_err());
        let client = connect(&store).await.unwrap().unwrap();
        let server = listener.accept().await.unwrap();
        assert_eq!(peer(&client).unwrap().pid, std::process::id());
        // In-process Web control must never await this process's own exit.
        peer(&client).unwrap().wait_for_exit().await.unwrap();
        authorize_client(&server).unwrap();
        drop(client);
        drop(server);
        listener.release().unwrap();
        assert!(connect(&store).await.unwrap().is_none());
    }
    #[tokio::test]
    async fn discarded_server_disconnects_stale_client_before_rebind() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        let mut listener = Listener::bind(&store).await.unwrap();
        let client = connect(&store).await.unwrap().unwrap();
        let server = listener.accept().await.unwrap();
        drop(server);
        listener.release().unwrap();
        // Keep the old client alive: dropping server handles alone is not enough.
        let replacement = Listener::bind(&store).await.unwrap();
        drop(client);
        replacement.release().unwrap();
    }
    #[tokio::test]
    async fn retained_process_guard_observes_exact_child_completion() {
        use std::os::windows::io::AsHandle;
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/c", "exit", "0"])
            .spawn()
            .unwrap();
        let guard = Peer {
            pid: child.id(),
            process: child.as_handle().try_clone_to_owned().unwrap(),
        };
        assert_ne!(guard.pid, std::process::id());
        guard.wait_for_exit().await.unwrap();
        assert!(child.wait().unwrap().success());
    }
}
