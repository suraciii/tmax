//! Unix endpoint ownership and peer qualification for the resident protocol.
use super::{failure, socket_path, validate_control_path};
use crate::{Result, storage::Store};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
pub(super) type Client = tokio::net::UnixStream;
pub(super) type Server = tokio::net::UnixStream;

pub(super) async fn connect(store: &Store) -> Result<Option<Client>> {
    let path = socket_path(store);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(meta) if meta.file_type().is_socket() && meta.uid() == unsafe { libc::geteuid() } => {}
        _ => {
            return Err(failure(
                "resident_identity_unavailable",
                "Control endpoint is not an inspectable owned Unix socket; no takeover attempted",
            ));
        }
    }
    match Client::connect(path).await {
        Ok(stream) => Ok(Some(stream)),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ) =>
        {
            Ok(None)
        }
        Err(_) => Err(failure(
            "resident_identity_unavailable",
            "Cannot connect to the resident control endpoint",
        )),
    }
}

pub(super) fn peer_pid(stream: &Client) -> Result<Option<u32>> {
    let peer = stream.peer_cred().map_err(|_| {
        failure(
            "resident_identity_unavailable",
            "Cannot establish resident peer identity",
        )
    })?;
    if peer.uid() != unsafe { libc::geteuid() } {
        return Err(failure(
            "resident_identity_unavailable",
            "Resident control peer is not this authority owner",
        ));
    }
    Ok(peer.pid().map(|pid| pid as u32))
}

pub(super) fn authorize_client(stream: &Server) -> Result<()> {
    peer_pid(stream).map(|_| ())
}

pub(super) struct Listener {
    listener: tokio::net::UnixListener,
    path: std::path::PathBuf,
}
impl Listener {
    pub(super) async fn bind(store: &Store) -> Result<Self> {
        let path = socket_path(store);
        validate_control_path(&path)?;
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if !meta.file_type().is_socket() || meta.uid() != unsafe { libc::geteuid() } {
                return Err(failure(
                    "resident_identity_unavailable",
                    "Refusing to replace an unidentified control endpoint",
                ));
            }
            match Client::connect(&path).await {
                Ok(_) => {
                    return Err(failure(
                        "resident_identity_unavailable",
                        "Control endpoint is occupied without matching ownership; no takeover",
                    ));
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                    ) =>
                {
                    std::fs::remove_file(&path).map_err(|_| {
                        failure(
                            "resident_identity_unavailable",
                            "Cannot remove this authority's stale socket",
                        )
                    })?;
                }
                Err(_) => {
                    return Err(failure(
                        "resident_identity_unavailable",
                        "Cannot establish control endpoint availability",
                    ));
                }
            }
        }
        let listener = tokio::net::UnixListener::bind(&path).map_err(|_| {
            failure(
                "endpoint_occupied",
                "Control endpoint is unavailable; no fallback attempted",
            )
        })?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(|_| {
            failure(
                "resident_identity_unavailable",
                "Cannot protect resident socket",
            )
        })?;
        Ok(Self { listener, path })
    }
    pub(super) async fn accept(&mut self) -> Result<Server> {
        self.listener
            .accept()
            .await
            .map(|(stream, _)| stream)
            .map_err(|_| failure("resident_control_failed", "Resident control accept failed"))
    }
    pub(super) fn release(self) -> Result<()> {
        let Self { listener, path } = self;
        drop(listener);
        std::fs::remove_file(path).map_err(|_| {
            failure(
                "resident_control_failed",
                "Cannot release owned resident socket",
            )
        })
    }
}
