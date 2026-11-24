//! One supervised owner per authority, with independent inference and Web lifecycles.
use crate::{Error, Result};
use std::net::SocketAddr;

mod control;
#[cfg(unix)]
#[path = "service/unix_control.rs"]
mod control_transport;
#[cfg(windows)]
#[path = "service/windows_control.rs"]
mod control_transport;
mod daemon;
mod startup;
mod supervised_control;
mod supervisor;
mod web;
#[cfg(windows)]
mod windows_web;

pub(crate) use control::{LoadedSnapshot, browser_login, loaded_snapshot};
pub use control::{activate, control, loaded, preview_activation, status, usage};
pub use daemon::run_service;
pub use startup::reconcile_autostart;
pub use supervisor::service_exit_code;

fn failure(code: &str, message: &str) -> Error {
    Error::new(code, message)
}
fn address(value: &str, allow_zero: bool) -> Result<SocketAddr> {
    let address: SocketAddr = value.parse().map_err(|_| {
        failure(
            "invalid_argument",
            "Listener must be a numeric loopback host:port",
        )
    })?;
    if !address.ip().is_loopback() || (!allow_zero && address.port() == 0) {
        return Err(failure(
            "invalid_argument",
            "Listener must use numeric loopback and a nonzero port",
        ));
    }
    Ok(address)
}

/// The running executable's path and content digest, read once per process.
/// On Linux the digest reads `/proc/self/exe`, which names the running image
/// even after the file at its path was replaced, so an upgraded on-disk binary
/// cannot pass for the process actually serving.
pub(super) fn running_image() -> Option<&'static (std::path::PathBuf, String)> {
    static IMAGE: std::sync::LazyLock<Option<(std::path::PathBuf, String)>> =
        std::sync::LazyLock::new(|| {
            use sha2::{Digest, Sha256};
            use std::io::Read;
            let path = std::env::current_exe().ok()?;
            #[cfg(target_os = "linux")]
            let source: &std::path::Path = std::path::Path::new("/proc/self/exe");
            #[cfg(not(target_os = "linux"))]
            let source: &std::path::Path = &path;
            let mut file = std::fs::File::open(source).ok()?;
            let mut hasher = Sha256::new();
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = file.read(&mut buffer).ok()?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            Some((path, format!("{:x}", hasher.finalize())))
        });
    IMAGE.as_ref()
}

#[cfg(test)]
mod tests {
    use super::{address, running_image};
    #[test]
    fn listener_addresses_are_explicit_loopback() {
        assert!(address("127.0.0.1:1234", false).is_ok());
        assert!(address("[::1]:1234", false).is_ok());
        assert!(address("0.0.0.0:1234", false).is_err());
        assert!(address("localhost:1234", false).is_err());
        assert!(address("127.0.0.1:0", false).is_err());
    }
    #[test]
    fn running_image_is_an_absolute_path_and_a_sha256_digest() {
        let (path, digest) = running_image().expect("this test binary is readable");
        assert!(path.is_absolute());
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|b| b.is_ascii_hexdigit()));
    }
}
