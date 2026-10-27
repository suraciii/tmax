//! One detached owner per authority, with independent inference and Web lifecycles.
use crate::{Error, Result};
use std::net::SocketAddr;

mod control;
#[cfg(unix)]
#[path = "service/unix_control.rs"]
mod control_transport;
#[cfg(windows)]
#[path = "service/windows_control.rs"]
mod control_transport;
mod resident;
mod startup;
#[cfg(not(windows))]
mod unix_startup;
mod web;
#[cfg(windows)]
mod windows_startup;
#[cfg(windows)]
mod windows_web;

pub use control::{activate, control, loaded, preview_activation, status};
pub(crate) use control::{browser_login, loaded_with_metadata};
pub use resident::run_resident;
pub use startup::{autostart, reconcile_autostart, run_autostart};

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

#[cfg(test)]
mod tests {
    use super::address;
    #[test]
    fn listener_addresses_are_explicit_loopback() {
        assert!(address("127.0.0.1:1234", false).is_ok());
        assert!(address("[::1]:1234", false).is_ok());
        assert!(address("0.0.0.0:1234", false).is_err());
        assert!(address("localhost:1234", false).is_err());
        assert!(address("127.0.0.1:0", false).is_err());
    }
}
