//! Same-protocol model gateway: a stable public facade over private feature owners.
//! Explicit bounded alias fallback; no resource lookup or protocol translation.
use serde_json::{Value, json};

mod bindings;
mod client_identity;
mod errors;
mod forwarding;
mod native_context;
mod opencode_session;
mod probe;
mod request_body;
pub(crate) mod request_facts;
mod response_observer;
mod runtime;

#[cfg(test)]
mod test_support;

pub use client_identity::{NativeClient, NativeRequirements, native_mismatch};
pub use forwarding::{publication, router};
pub use probe::probe;
pub use request_facts::RequestFacts;
pub use runtime::{Runtime, Snapshot, credential};

// Concrete budgets are surfaced by status and exercised by the bounded-admission tests.
pub const ACTIVE_LIMIT: usize = 16;
pub const REQUEST_BYTES: usize = 512 * 1024 * 1024;
pub const INSPECTION_BYTES: usize = 64 * 1024;
pub const FORWARD_CHUNK_BYTES: usize = 16 * 1024;
pub const CREDENTIAL_SECONDS: u64 = 10;

pub fn limits() -> Value {
    json!({"active_requests":ACTIVE_LIMIT,"request_body_bytes":REQUEST_BYTES,"response_inspection_bytes":INSPECTION_BYTES,"downstream_chunk_bytes":FORWARD_CHUNK_BYTES,"credential_acquisition_seconds":CREDENTIAL_SECONDS,"waiting_queue":0,"inference_total_timeout":null,"inference_idle_timeout":null,"history_records":crate::diagnostics::RECORD_LIMIT,"history_record_bytes":crate::diagnostics::RECORD_BYTES})
}
