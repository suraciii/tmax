//! Loaded gateway state, bounded admission, credentials, and request lifecycle.
use super::{
    ACTIVE_LIMIT, CREDENTIAL_SECONDS,
    bindings::{Bindings, initialize_bindings},
    errors::error_response,
};
use crate::{
    Error, Result,
    config::Config,
    diagnostics::{
        History, Record,
        error_log::{self, ErrorDetail},
    },
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
    /// Close admission and cancel every active request. Stop never waits for
    /// in-flight work: interruption is the contract for both stop and restart.
    pub fn stop(&self) -> usize {
        let mut a = self.admission.lock().expect("admission mutex");
        a.enabled = false;
        let count = a.active.len();
        for cancel in a.active.values() {
            let _ = cancel.send(true);
        }
        count
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
            errors: Vec::new(),
            dispatch_attempts: 0,
        })
    }
    pub(super) fn log_error(&self, record: &mut Record, errors: &[ErrorDetail]) {
        if let Err(error) = error_log::append(&self.store, record, errors) {
            record.evidence_gap = true;
            eprintln!(
                "Unable to retain gateway error {}: {}",
                record.request, error.code
            );
        }
    }
}

pub(super) struct Flight {
    runtime: Arc<Runtime>,
    pub(super) record: Record,
    started: Instant,
    pub(super) cancel: watch::Receiver<bool>,
    pub(super) finished: bool,
    errors: Vec<ErrorDetail>,
    pub(super) dispatch_attempts: usize,
}
impl Flight {
    pub(super) fn record_error(
        &mut self,
        time: chrono::DateTime<chrono::Utc>,
        stage: &str,
        status: Option<u16>,
        request_id: Option<(String, bool)>,
        mut error: String,
        mut truncated: bool,
    ) {
        if self.errors.len() >= error_log::ERROR_DETAIL_LIMIT {
            self.record.evidence_gap = true;
            if let Some(last) = self.errors.last_mut() {
                last.truncated = true;
            }
            return;
        }
        let used: usize = self.errors.iter().map(|detail| detail.error.len()).sum();
        let remaining = error_log::DETAIL_BYTES.saturating_sub(used);
        if error.len() > remaining {
            let mut end = remaining;
            while !error.is_char_boundary(end) {
                end -= 1;
            }
            error.truncate(end);
            error.shrink_to_fit();
            truncated = true;
        }
        let (request_id, request_id_truncated) = match request_id {
            Some((id, truncated)) => (Some(id), truncated),
            None => (None, false),
        };
        self.record.evidence_gap |= truncated || request_id_truncated;
        self.errors.push(ErrorDetail {
            time,
            attempt: (self.dispatch_attempts != 0).then_some(self.dispatch_attempts),
            stage: stage.to_owned(),
            provider: self.record.provider.clone(),
            qualified_model: self.record.qualified_model.clone(),
            upstream_status: status,
            request_id,
            request_id_truncated,
            error,
            truncated,
        });
    }
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
        self.runtime.log_error(&mut self.record, &self.errors);
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
    fn error_capture_keeps_dispatch_ordinals_and_marks_omissions() {
        let temporary = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(Store::at(temporary.path().into()), "capture".into()).unwrap();
        runtime.open();
        let mut flight = runtime.admit(runtime.history.begin()).unwrap();
        flight.record_error(
            chrono::Utc::now(),
            "selection",
            None,
            None,
            "before dispatch".into(),
            false,
        );
        assert_eq!(flight.errors[0].attempt, None);
        for attempt in 1..=error_log::ERROR_DETAIL_LIMIT {
            flight.dispatch_attempts = attempt;
            flight.record_error(
                chrono::Utc::now(),
                "upstream_dispatch",
                None,
                None,
                "failed".into(),
                false,
            );
        }
        assert_eq!(flight.errors.len(), error_log::ERROR_DETAIL_LIMIT);
        assert_eq!(
            flight.errors.last().unwrap().attempt,
            Some(error_log::ERROR_DETAIL_LIMIT - 1)
        );
        assert!(flight.errors.last().unwrap().truncated);
        assert!(flight.record.evidence_gap);
        flight.finish("failed", "upstream_dispatch", None);
    }

    #[test]
    fn error_capture_preserves_time_and_request_id_loss() {
        let temporary = tempfile::tempdir().unwrap();
        let runtime = Runtime::new(Store::at(temporary.path().into()), "capture".into()).unwrap();
        runtime.open();
        let mut flight = runtime.admit(runtime.history.begin()).unwrap();
        let time = chrono::DateTime::parse_from_rfc3339("2000-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        flight.record_error(
            time,
            "upstream_response",
            Some(429),
            Some(("bounded-id".into(), true)),
            r#"{"message":"complete"}"#.into(),
            false,
        );
        assert_eq!(flight.errors[0].time, time);
        assert_eq!(flight.errors[0].request_id.as_deref(), Some("bounded-id"));
        assert!(flight.errors[0].request_id_truncated);
        assert!(!flight.errors[0].truncated);
        assert!(flight.record.evidence_gap);
        flight.finish("failed", "upstream_response", None);
    }

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
        // Stop never waits for in-flight work: it closes admission, cancels
        // the active set, and reports how many were interrupted.
        assert_eq!(runtime.stop(), ACTIVE_LIMIT);
        assert_eq!(runtime.active(), ACTIVE_LIMIT);
        assert!(flights.iter().all(|flight| *flight.cancel.borrow()));
        drop(flights);
        assert_eq!(runtime.active(), 0);
        assert_eq!(runtime.stop(), 0);
        assert!(runtime.admit(runtime.history.begin()).is_err());
        println!(
            "admission_measurement active_limit={} rejected_without_queue=true request_body_bound={} inspection_bound={} chunk_bound={}",
            ACTIVE_LIMIT, REQUEST_BYTES, INSPECTION_BYTES, FORWARD_CHUNK_BYTES
        );
    }
}
