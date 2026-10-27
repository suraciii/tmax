//! Loaded gateway state, bounded admission, credentials, and request lifecycle.
use super::{
    ACTIVE_LIMIT, CREDENTIAL_SECONDS,
    bindings::{Bindings, initialize_bindings},
    errors::error_response,
};
use crate::{
    Error, Result,
    config::Config,
    diagnostics::{History, Record},
    storage::Store,
};
use axum::{http::StatusCode, response::Response};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use zeroize::Zeroizing;

static CREDENTIAL_SLOTS: std::sync::LazyLock<Arc<tokio::sync::Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(ACTIVE_LIMIT)));
pub async fn credential(store: Store, reference: String) -> Result<Zeroizing<Vec<u8>>> {
    let slot = CREDENTIAL_SLOTS.clone().try_acquire_owned().map_err(|_| {
        Error::new(
            "credential_capacity",
            "Bounded credential acquisition capacity is unavailable",
        )
    })?;
    let job = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        crate::secrets::get(&store, &reference)
    });
    tokio::time::timeout(Duration::from_secs(CREDENTIAL_SECONDS), job)
        .await
        .map_err(|_| {
            Error::new(
                "credential_acquisition_deadline",
                "Credential acquisition deadline expired",
            )
        })?
        .map_err(|_| Error::new("credential_unavailable", "Credential acquisition failed"))?
}

pub struct Snapshot {
    pub config: Config,
    pub revision: String,
    pub key: Zeroizing<Vec<u8>>,
    pub client: reqwest::Client,
    pub metadata: Option<Arc<crate::catalog::Snapshot>>,
}
struct Admission {
    enabled: bool,
    active: HashMap<String, watch::Sender<bool>>,
}
pub struct Runtime {
    pub store: Store,
    pub snapshot: RwLock<Option<Arc<Snapshot>>>,
    pub history: Arc<History>,
    admission: Mutex<Admission>,
    pub(super) bindings: Mutex<Bindings>,
}

impl Runtime {
    pub fn new(store: Store, process: String) -> Result<Arc<Self>> {
        let bindings = initialize_bindings(&store)?;
        Ok(Arc::new(Self {
            store,
            snapshot: RwLock::new(None),
            history: Arc::new(History::new(process)),
            admission: Mutex::new(Admission {
                enabled: false,
                active: HashMap::new(),
            }),
            bindings: Mutex::new(bindings),
        }))
    }
    pub fn active(&self) -> usize {
        self.admission.lock().expect("admission mutex").active.len()
    }
    pub fn open(&self) {
        self.admission.lock().expect("admission mutex").enabled = true;
    }
    pub fn stop(&self, force: bool) -> Result<usize> {
        let mut a = self.admission.lock().expect("admission mutex");
        if !force && !a.active.is_empty() {
            return Err(Error::new(
                "active_requests",
                "Requests are active; stop refused, or explicitly use --cancel-active",
            ));
        }
        a.enabled = false;
        let count = a.active.len();
        if force {
            for cancel in a.active.values() {
                let _ = cancel.send(true);
            }
        }
        Ok(count)
    }
    pub(super) fn admit(self: &Arc<Self>, record: Record) -> Result<Flight> {
        let mut a = self.admission.lock().expect("admission mutex");
        if !a.enabled {
            return Err(Error::new(
                "listener_stopped",
                "Inference admission is closed",
            ));
        }
        if a.active.len() >= ACTIVE_LIMIT {
            return Err(Error::new(
                "local_capacity",
                "Local active-request capacity is unavailable; no upstream dispatch",
            ));
        }
        let (tx, rx) = watch::channel(false);
        a.active.insert(record.request.clone(), tx);
        Ok(Flight {
            runtime: self.clone(),
            record,
            started: Instant::now(),
            cancel: rx,
            finished: false,
        })
    }
}

pub(super) struct Flight {
    runtime: Arc<Runtime>,
    pub(super) record: Record,
    started: Instant,
    pub(super) cancel: watch::Receiver<bool>,
    pub(super) finished: bool,
}
impl Flight {
    pub(super) fn finish(&mut self, outcome: &str, stage: &str, reason: Option<&str>) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.record.outcome = outcome.into();
        self.record.stage = stage.into();
        self.record.reason_code = reason.map(str::to_owned);
        self.record.duration_ms =
            Some(self.started.elapsed().as_millis().min(u64::MAX as u128) as u64);
        self.runtime.history.update(self.record.clone());
        self.runtime
            .admission
            .lock()
            .expect("admission mutex")
            .active
            .remove(&self.record.request);
    }
    pub(super) fn reject(mut self, status: StatusCode, code: &str, stage: &str) -> Response {
        self.finish("rejected", stage, Some(code));
        error_response(status, code, &self.record.request)
    }
}
impl Drop for Flight {
    fn drop(&mut self) {
        if !self.finished {
            let interrupted = *self.cancel.borrow();
            self.finish(
                if interrupted {
                    "interrupted"
                } else {
                    "cancelled"
                },
                "downstream_delivery",
                Some(if interrupted {
                    "force_stop"
                } else {
                    "downstream_disconnected"
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::{FORWARD_CHUNK_BYTES, INSPECTION_BYTES, REQUEST_BYTES};
    #[test]
    fn immediate_active_admission_and_atomic_stop() {
        let temporary = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(Store::at(temporary.path().into()), "admission".into()).unwrap();
        runtime.open();
        let mut flights = Vec::new();
        for _ in 0..ACTIVE_LIMIT {
            flights.push(
                runtime
                    .admit(runtime.history.begin())
                    .unwrap_or_else(|_| panic!("capacity available")),
            );
        }
        assert_eq!(runtime.active(), ACTIVE_LIMIT);
        assert!(
            matches!(runtime.admit(runtime.history.begin()),Err(error)if error.code=="local_capacity")
        );
        assert!(runtime.stop(false).is_err());
        assert_eq!(runtime.active(), ACTIVE_LIMIT);
        drop(flights);
        assert_eq!(runtime.active(), 0);
        assert_eq!(runtime.stop(false).unwrap(), 0);
        assert!(runtime.admit(runtime.history.begin()).is_err());
        println!(
            "admission_measurement active_limit={} rejected_without_queue=true request_body_bound={} inspection_bound={} chunk_bound={}",
            ACTIVE_LIMIT, REQUEST_BYTES, INSPECTION_BYTES, FORWARD_CHUNK_BYTES
        );
    }
}
