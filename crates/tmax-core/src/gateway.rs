//! Same-protocol HTTP forwarding. No retries, resource lookup, or protocol translation.
use crate::{
    Error, Request, Result,
    config::{Auth, Config},
    diagnostics::{History, Record, safe_evidence},
    storage::Store,
};
use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
    extract::State,
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap},
    pin::Pin,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use zeroize::Zeroizing;
#[path = "service/capabilities.rs"]
mod capabilities;
#[path = "service/native_context.rs"]
mod native_context;
pub fn capability_mismatch(
    provider: &crate::config::Provider,
    model_id: &str,
    model: &crate::config::Model,
    metadata: Option<&crate::catalog::Snapshot>,
    body: &Value,
) -> Option<&'static str> {
    capabilities::mismatch(provider, model_id, model, metadata, body)
}
const BINDING_LIMIT: usize = 2048;
const BINDING_FILE_BYTES: u64 = 64 * 1024 * 1024;

// Concrete budgets are surfaced by status and exercised by the bounded-admission tests.
pub const ACTIVE_LIMIT: usize = 16;
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
pub const REQUEST_BYTES: usize = 4 * 1024 * 1024;
pub const INSPECTION_BYTES: usize = 64 * 1024;
pub const FORWARD_CHUNK_BYTES: usize = 16 * 1024;
pub const CREDENTIAL_SECONDS: u64 = 10;

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
    bindings: Mutex<Bindings>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bindings {
    epoch: chrono::DateTime<Utc>,
    entries: BTreeMap<String, Binding>,
}
impl Default for Bindings {
    fn default() -> Self {
        Self {
            epoch: Utc::now(),
            entries: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    context_handle: String,
    provider_identity: String,
    offering_identity: String,
    provider: String,
    model: String,
    protocol: String,
    public_id: String,
    endpoint: String,
    authorization_generation: String,
    continuity: String,
    decision: Value,
    blocked: Option<String>,
}
struct Choice {
    selection: crate::routing::Selection,
    authorization_generation: String,
    endpoint: String,
    go_session_header: Option<&'static str>,
}
fn load_bindings(store: &Store) -> Result<Bindings> {
    let path = store.root.join("session-bindings.json");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > BINDING_FILE_BYTES) {
        return Err(Error::new(
            "binding_store_unavailable",
            "Session binding store exceeds its bounded size; preserve it and inspect recovery",
        ));
    }
    let bindings: Bindings = store.read_json("session-bindings.json")?;
    if bindings.entries.len() > BINDING_LIMIT {
        return Err(Error::new(
            "binding_store_unavailable",
            "Session binding count exceeds its bounded limit",
        ));
    }
    Ok(bindings)
}
impl Runtime {
    pub fn new(store: Store, process: String) -> Result<Arc<Self>> {
        let path = store.root.join("session-bindings.json");
        let bindings = load_bindings(&store)?;
        if !path.exists() {
            store.write_json("session-bindings.json", &bindings)?;
        }
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
    fn admit(self: &Arc<Self>, record: Record) -> Result<Flight> {
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
    pub fn binding_summary(&self) -> Value {
        let b = self.bindings.lock().expect("bindings mutex");
        json!({"retained":b.entries.len(),"blocked":b.entries.values().filter(|v|v.blocked.is_some()).count(),"session_qualification":"native session identity must be qualified before dynamic session admission"})
    }
    pub(crate) fn inspect_session(
        &self,
        config: &Config,
        public_id: &str,
        protocol: &str,
        session: &str,
        loaded: bool,
    ) -> Result<Value> {
        let identities = if loaded {
            crate::inputs::Snapshot::capture_loaded(&self.store, config, None)
        } else {
            crate::inputs::Snapshot::capture(&self.store, config)
        };
        let bindings = self.bindings.lock().expect("bindings mutex");
        retained_explanation(
            &bindings,
            config,
            &identities,
            public_id,
            protocol,
            session,
            "resident",
        )
    }
    pub(crate) fn inspect_persisted_session(
        store: &Store,
        config: &Config,
        public_id: &str,
        protocol: &str,
        session: &str,
    ) -> Result<Value> {
        let identities = crate::inputs::Snapshot::capture(store, config);
        retained_explanation(
            &load_bindings(store)?,
            config,
            &identities,
            public_id,
            protocol,
            session,
            "persisted",
        )
    }
    pub fn activate_bindings(&self, config: &Config, dry_run: bool) -> Result<Value> {
        Self::transition_bindings(&self.store, &self.bindings, config, dry_run)
    }
    pub fn preview_persisted(store: &Store, config: &Config) -> Result<Value> {
        let bindings = Mutex::new(load_bindings(store)?);
        Self::transition_bindings(store, &bindings, config, true)
    }
    fn transition_bindings(
        store: &Store,
        retained: &Mutex<Bindings>,
        config: &Config,
        dry_run: bool,
    ) -> Result<Value> {
        let identities = if dry_run {
            crate::inputs::Snapshot::capture(store, config)
        } else {
            crate::inputs::Snapshot::capture_loaded(store, config, None)
        };
        let mut names = BTreeMap::new();
        for (provider_id, provider) in &config.providers {
            for model in provider.models.keys() {
                if let Some((identity, _, offering)) =
                    identities.destination_identity(provider_id, model)
                {
                    names.insert(offering, (provider_id.as_str(), model.as_str(), identity));
                }
            }
        }
        let mut bindings = retained.lock().expect("bindings mutex");
        let mut staged = (!dry_run).then(|| bindings.clone());
        let next = staged.as_mut().unwrap_or(&mut *bindings);
        let mut affected = Vec::new();
        let mut renamed = Vec::new();
        let mut rekey = Vec::new();
        for (handle, binding) in &mut next.entries {
            if binding.blocked.is_some() {
                continue;
            }
            let rename =
                names
                    .get(&binding.offering_identity)
                    .filter(|(provider, model, identity)| {
                        *provider != binding.provider
                            && *model == binding.model
                            && identity == &binding.provider_identity
                    });
            let candidate = rename.map(|(provider, _, _)| {
                let mut candidate = binding.clone();
                if candidate.public_id == format!("{}/{}", candidate.provider, candidate.model) {
                    candidate.public_id = format!("{provider}/{}", candidate.model);
                }
                candidate.provider = (*provider).into();
                candidate
            });
            let current = candidate.as_ref().unwrap_or(binding);
            let reason = binding_blocker(config, current, &identities);
            if let Some(reason) = reason {
                affected.push(json!({"binding":handle,"reason_code":reason}));
                if !dry_run {
                    binding.blocked = Some(reason.into());
                }
            } else if let Some(candidate) = candidate {
                renamed.push(json!({"binding":handle,"provider":candidate.provider}));
                if !dry_run {
                    binding.provider = candidate.provider;
                    binding.public_id = candidate.public_id;
                    let key = native_context::binding_key(
                        &binding.context_handle,
                        &binding.public_id,
                        &binding.protocol,
                    );
                    if key != *handle {
                        rekey.push((handle.clone(), key));
                    }
                }
            }
        }
        if !dry_run {
            for (old, new) in rekey {
                if let Some(binding) = next.entries.remove(&old)
                    && next.entries.insert(new, binding).is_some()
                {
                    return Err(Error::new(
                        "binding_store_conflict",
                        "Native binding rename collided; affected sessions require inspection",
                    ));
                }
            }
            if !affected.is_empty() || !renamed.is_empty() {
                save_bindings(store, next)?;
                *bindings = staged.expect("executed binding transition");
            }
        }
        Ok(
            json!({"blocked_sessions":affected,"renamed_destinations":renamed,"in_flight":"retain admission snapshots"}),
        )
    }
    fn choose(
        &self,
        snapshot: &Snapshot,
        public_id: &str,
        protocol: &str,
        body: &Value,
        headers: &HeaderMap,
        now: chrono::DateTime<Utc>,
    ) -> Result<Choice> {
        let config = &snapshot.config;
        let context = native_context::identify(headers, body, protocol);
        let native = NativeRequirements::capture(context.as_ref());
        if native_context::has_unbound_resources(body) {
            return Err(Error::new(
                "original_resource_binding_unavailable",
                "This operation contains a provider-owned resource without a qualified original authorization binding",
            ));
        }
        let alias = config
            .model_aliases
            .iter()
            .find(|a| a.id == public_id && a.protocol == protocol);
        let continuity = alias
            .map(|a| a.continuity.as_deref().unwrap_or("session"))
            .unwrap_or("fixed");
        if alias.is_some()
            && continuity == "stateless"
            && !context.as_ref().is_some_and(|c| c.independent)
        {
            return Err(Error::new(
                "independent_context_required",
                "Stateless forwarding requires a qualified native client with complete independent context",
            ));
        }
        let key = context
            .as_ref()
            .and_then(|c| c.binding_key(public_id, protocol));
        if alias.is_some() && continuity == "session" && key.is_none() {
            return Err(Error::new(
                "session_identity_unavailable",
                "This alias requires the qualified native conversation identity",
            ));
        }
        if native_context::has_remote_state(body) && key.is_none() {
            return Err(Error::new(
                "original_binding_unavailable",
                "Upstream-owned continuation requires its qualified original destination binding",
            ));
        }
        let retain = key.is_some()
            && continuity != "stateless"
            && (alias.is_some()
                || native_context::has_remote_state(body)
                || context.as_ref().is_some_and(|c| c.created_at.is_some()));
        let mut bindings = self.bindings.lock().expect("bindings mutex");
        if let Some(key) = &key
            && let Some(binding) = bindings.entries.get_mut(key)
        {
            if let Some(reason) = binding.blocked.as_ref() {
                return Err(Error::new(
                    reason,
                    "This retained native conversation is blocked; use a qualified new native session",
                ));
            }
            let identities = crate::inputs::Snapshot::capture_loaded(
                &self.store,
                config,
                snapshot.metadata.as_deref(),
            );
            if let Some(reason) = binding_blocker(config, binding, &identities) {
                binding.blocked = Some(reason.into());
                save_bindings(&self.store, &bindings)?;
                return Err(Error::new(
                    reason,
                    "Retained destination is incompatible; it will not be reassigned",
                ));
            }
            let provider = &config.providers[&binding.provider];
            let model = &provider.models[&binding.model];
            if let Some(reason) = capability_mismatch(
                provider,
                &binding.model,
                model,
                snapshot.metadata.as_deref(),
                body,
            ) {
                return Err(Error::new(
                    reason,
                    "Request requirements are incompatible with the retained offering; no reassignment",
                ));
            }
            client_contract(provider, &native)?;
            let evidence = json!({"selection_reason":"retained_destination","scope_handle":binding.context_handle,"original_decision":binding.decision,"selected":{ "provider":binding.provider,"model":binding.model,"offering_identity":binding.offering_identity},"effective_priority":binding.decision.get("selection").and_then(|v|v.get("priority"))});
            return Ok(Choice {
                selection: crate::routing::Selection {
                    provider: binding.provider.clone(),
                    model: binding.model.clone(),
                    offering_identity: binding.offering_identity.clone(),
                    priority: 0,
                    evidence,
                },
                authorization_generation: binding.authorization_generation.clone(),
                endpoint: binding.endpoint.clone(),
                go_session_header: context.as_ref().and_then(|c| c.go_session_header),
            });
        }
        if retain
            && !context
                .as_ref()
                .is_some_and(|c| c.trusted_new(body, protocol, bindings.epoch, now))
        {
            return Err(Error::new(
                "original_binding_unavailable",
                "Unknown native session is not a qualified fresh session; prior destinations cannot be reconstructed",
            ));
        }
        let mut selection = crate::routing::select_request_loaded(
            &self.store,
            config,
            public_id,
            protocol,
            now,
            crate::routing::RequestRequirements {
                body,
                native: &native,
            },
            snapshot.metadata.as_deref(),
        )?;
        let provider = &config.providers[&selection.provider];
        client_contract(provider, &native)?;
        crate::integrations::check_request(provider, &selection.model, protocol, body)?;
        let endpoints = crate::integrations::resolve(provider)?;
        let endpoint = endpoints
            .get(protocol)
            .cloned()
            .ok_or_else(|| Error::new("protocol_unavailable", "Protocol endpoint unavailable"))?;
        let captured = &selection.evidence["selection"];
        let provider_identity = captured["provider_identity"]
            .as_str()
            .ok_or_else(|| {
                Error::new(
                    "identity_unavailable",
                    "Selected provider lineage was not captured",
                )
            })?
            .to_owned();
        let authorization_generation = captured["authorization_generation"]
            .as_str()
            .ok_or_else(|| {
                Error::new(
                    "identity_unavailable",
                    "Selected authorization lineage was not captured",
                )
            })?
            .to_owned();
        let offering_identity = selection.offering_identity.clone();
        if let Some(selected) = selection.evidence.get("selection") {
            let name = selected.get("qualified_model");
            let selected_evaluation = selection
                .evidence
                .get("priority_evaluations")
                .and_then(Value::as_array)
                .and_then(|values| {
                    values
                        .iter()
                        .find(|value| value.get("qualified_model") == name)
                })
                .cloned();
            if let Some(evaluation) = selected_evaluation {
                selection.evidence["selected_evaluation"] = evaluation;
            }
        }
        if retain {
            selection.evidence["scope_handle"] = json!(
                context
                    .as_ref()
                    .and_then(native_context::Context::scope_handle)
            );
            if bindings.entries.len() >= BINDING_LIMIT {
                return Err(Error::new(
                    "session_binding_capacity",
                    "Durable native session capacity reached; no existing binding is evicted or reassigned",
                ));
            }
            let binding = Binding {
                context_handle: context
                    .as_ref()
                    .and_then(native_context::Context::scope_handle)
                    .expect("qualified native scope"),
                provider_identity,
                offering_identity,
                provider: selection.provider.clone(),
                model: selection.model.clone(),
                protocol: protocol.into(),
                public_id: public_id.into(),
                endpoint: endpoint.clone(),
                authorization_generation: authorization_generation.clone(),
                continuity: continuity.into(),
                decision: safe_evidence(&selection.evidence),
                blocked: None,
            };
            let key = key.expect("qualified binding key");
            bindings.entries.insert(key.clone(), binding);
            if let Err(error) = save_bindings(&self.store, &bindings) {
                bindings.entries.remove(&key);
                return Err(error);
            }
        }
        Ok(Choice {
            selection,
            authorization_generation,
            endpoint,
            go_session_header: context.as_ref().and_then(|c| c.go_session_header),
        })
    }
}
fn retained_explanation(
    bindings: &Bindings,
    config: &Config,
    identities: &crate::inputs::Snapshot<'_>,
    public_id: &str,
    protocol: &str,
    session: &str,
    source: &str,
) -> Result<Value> {
    let key = native_context::binding_key(session, public_id, protocol);
    let binding = bindings
        .entries
        .get(&key)
        .filter(|binding| {
            binding.context_handle == session
                && binding.public_id == public_id
                && binding.protocol == protocol
        })
        .ok_or_else(|| {
            Error::new(
                "original_binding_unavailable",
                "No retained destination matches this gateway handle, public model ID and protocol",
            )
        })?;
    let blocker = binding
        .blocked
        .as_deref()
        .or_else(|| binding_blocker(config, binding, identities));
    Ok(json!({
        "session":session,"session_source":source,"public_id":public_id,"protocol":protocol,
        "selection_reason":"retained_destination",
        "selection":{"provider":binding.provider,"model":binding.model,"qualified_model":format!("{}/{}",binding.provider,binding.model),"offering_identity":binding.offering_identity},
        "continuation":{"state":if blocker.is_some(){"blocked"}else{"retained"},"reason_code":blocker,"stored_blocker":binding.blocked},
        "original_decision":safe_evidence(&binding.decision),"fresh_ranking":"not_evaluated","authorization":"not_verified"
    }))
}

fn binding_blocker(
    config: &Config,
    b: &Binding,
    identities: &crate::inputs::Snapshot<'_>,
) -> Option<&'static str> {
    let Some(p) = config.providers.get(&b.provider) else {
        return Some("provider_removed");
    };
    if !p.enabled {
        return Some("provider_disabled");
    }
    let Some(m) = p.models.get(&b.model) else {
        return Some("offering_removed");
    };
    if !m.enabled {
        return Some("offering_disabled");
    }
    let Ok(endpoints) = crate::integrations::resolve(p) else {
        return Some("endpoint_unavailable");
    };
    if endpoints.get(&b.protocol) != Some(&b.endpoint) {
        return Some("endpoint_changed");
    }
    let Some((identity, generation, offering)) =
        identities.destination_identity(&b.provider, &b.model)
    else {
        return Some("destination_identity_unavailable");
    };
    if identity != b.provider_identity || offering != b.offering_identity {
        return Some("destination_replaced");
    }
    if generation != b.authorization_generation {
        return Some("authorization_changed");
    }
    if let Some(alias) = config
        .model_aliases
        .iter()
        .find(|a| a.id == b.public_id && a.protocol == b.protocol)
    {
        if !alias.enabled {
            return Some("alias_disabled");
        }
        if alias.continuity.as_deref().unwrap_or("session") != b.continuity {
            return Some("continuity_changed");
        }
        if !alias.candidates.iter().any(|candidate| {
            candidate.split_once('/') == Some((b.provider.as_str(), b.model.as_str()))
        }) {
            return Some("candidate_removed");
        }
    } else if b.public_id != format!("{}/{}", b.provider, b.model) {
        return Some("alias_removed");
    }
    None
}
fn save_bindings(store: &Store, bindings: &Bindings) -> Result<()> {
    struct Buffer(Vec<u8>);
    impl std::io::Write for Buffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) > BINDING_FILE_BYTES as usize {
                return Err(std::io::Error::other("binding capacity"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer(Vec::new());
    serde_json::to_writer(&mut buffer, bindings).map_err(|_| {
        Error::new(
            "session_binding_capacity",
            "Durable native binding byte capacity reached; existing destinations were not evicted",
        )
    })?;
    store.write_raw("session-bindings.json", &buffer.0)
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NativeClient {
    Unknown,
    Pi,
    Hermes,
}
pub struct NativeRequirements {
    pub client: NativeClient,
    pub go_session: bool,
}
impl NativeRequirements {
    fn capture(context: Option<&native_context::Context>) -> Self {
        Self {
            client: match context.map(|c| c.family) {
                Some("pi") => NativeClient::Pi,
                Some("hermes") => NativeClient::Hermes,
                _ => NativeClient::Unknown,
            },
            go_session: context.is_some_and(|c| c.go_session_header.is_some()),
        }
    }
}
pub fn native_mismatch(
    provider: &crate::config::Provider,
    native: &NativeRequirements,
) -> Option<&'static str> {
    match crate::integrations::integration_id(provider) {
        Ok("codex-subscription") => Some("gateway_identity_prerequisite"),
        Ok("kimi-coding-plan") if matches!(provider.auth, Some(Auth::Login { .. })) => {
            Some("native_authorization_unqualified")
        }
        Ok("opencode-go") if native.client == NativeClient::Unknown || !native.go_session => {
            Some("native_session_required")
        }
        Ok("kimi-coding-plan") if native.client == NativeClient::Unknown => {
            Some("native_client_required")
        }
        Ok(_) => None,
        Err(_) => Some("integration_unavailable"),
    }
}
fn client_contract(provider: &crate::config::Provider, native: &NativeRequirements) -> Result<()> {
    if let Some(reason) = native_mismatch(provider, native) {
        Err(Error::new(reason, safe_message(reason)))
    } else {
        Ok(())
    }
}
pub fn limits() -> Value {
    json!({"active_requests":ACTIVE_LIMIT,"request_body_bytes":REQUEST_BYTES,"response_inspection_bytes":INSPECTION_BYTES,"downstream_chunk_bytes":FORWARD_CHUNK_BYTES,"credential_acquisition_seconds":CREDENTIAL_SECONDS,"waiting_queue":0,"inference_total_timeout":null,"inference_idle_timeout":null,"history_records":crate::diagnostics::RECORD_LIMIT,"history_record_bytes":crate::diagnostics::RECORD_BYTES})
}
pub fn router(runtime: Arc<Runtime>) -> Router {
    Router::new().fallback(forward).with_state(runtime)
}
struct Flight {
    runtime: Arc<Runtime>,
    record: Record,
    started: Instant,
    cancel: watch::Receiver<bool>,
    finished: bool,
}
impl Flight {
    fn finish(&mut self, outcome: &str, stage: &str, reason: Option<&str>) {
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
    fn reject(mut self, status: StatusCode, code: &str, stage: &str) -> Response {
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
fn error_response(status: StatusCode, code: &str, id: &str) -> Response {
    (
        status,
        axum::Json(json!({"error":{"type":code,"message":safe_message(code),"request_id":id}})),
    )
        .into_response()
}
fn safe_message(code: &str) -> &'static str {
    match code {
        "local_capacity" => "Local capacity unavailable; request was not dispatched",
        "request_too_large" => "Request exceeds the local body limit",
        "authentication_failed" => "Current shared gateway key required",
        "session_identity_unavailable" => {
            "A qualified native conversation identity is required for session-stable alias forwarding"
        }
        "original_binding_unavailable" => {
            "Original native destination is unavailable; only a qualified new native session can select a new destination"
        }
        "original_resource_binding_unavailable" => {
            "A provider-owned resource lacks its qualified original authorization binding; no lookup or replay was attempted"
        }
        "independent_context_required" => {
            "This alias requires a qualified client with complete independent context and no upstream-owned continuation state"
        }
        "gateway_identity_prerequisite" => {
            "Codex gateway use requires managed-policy permission and qualified native authorization identity; direct native access remains independent"
        }
        "native_session_required" => {
            "Go requires the real qualified native client and its dynamic conversation session header; a static or fabricated ID is not supported"
        }
        "native_client_required" => {
            "Kimi Coding Plan requires the truthful qualified personal-interactive native client"
        }
        "native_authorization_unqualified" => {
            "The native login owner has no qualified forwarding handoff; keep authorization with its supported direct native owner"
        }
        "upstream_transport_failed" => {
            "Upstream transport failed; execution or completion may be unknown"
        }
        _ => "Request unavailable at the indicated stage; inspect local request metadata",
    }
}
fn matches_key(supplied: &[u8], key: &[u8]) -> bool {
    let mut difference = supplied.len() ^ key.len();
    for (index, byte) in key.iter().enumerate() {
        difference |= usize::from(*byte ^ supplied.get(index).copied().unwrap_or(0));
    }
    difference == 0
}
fn local_key(headers: &HeaderMap) -> Option<&[u8]> {
    if let Some(value) = headers.get("x-gateway-key") {
        return Some(value.as_bytes());
    }
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::as_bytes)
        .or_else(|| headers.get("x-api-key").map(|v| v.as_bytes()))
}
async fn forward(State(runtime): State<Arc<Runtime>>, request: axum::extract::Request) -> Response {
    let record = runtime.history.begin();
    let id = record.request.clone();
    let mut flight = match runtime.admit(record.clone()) {
        Ok(v) => v,
        Err(e) => {
            let mut record = record;
            record.outcome = "rejected".into();
            record.stage = "local_admission".into();
            record.reason_code = Some(e.code.clone());
            runtime.history.update(record);
            return error_response(StatusCode::SERVICE_UNAVAILABLE, &e.code, &id);
        }
    };
    let snapshot = runtime.snapshot.read().expect("snapshot lock").clone();
    let Some(snapshot) = snapshot else {
        return flight.reject(
            StatusCode::SERVICE_UNAVAILABLE,
            "listener_stopped",
            "local_admission",
        );
    };
    let (parts, body) = request.into_parts();
    if parts.headers.contains_key("origin") {
        return flight.reject(
            StatusCode::FORBIDDEN,
            "browser_inference_unavailable",
            "shared_key_authentication",
        );
    }
    if !local_key(&parts.headers).is_some_and(|key| matches_key(key, &snapshot.key)) {
        return flight.reject(
            StatusCode::UNAUTHORIZED,
            "authentication_failed",
            "shared_key_authentication",
        );
    }
    if parts.headers.contains_key("x-gateway-key") {
        return flight.reject(
            StatusCode::CONFLICT,
            "gateway_identity_prerequisite",
            "native_identity",
        );
    }
    if parts.method == Method::GET
        && matches!(parts.uri.path(), "/models" | "/v1/models")
        && parts.uri.query().is_none()
    {
        flight.record.admitted_at = Some(Utc::now());
        flight.record.downstream_complete = true;
        flight.finish("completed", "model_discovery", None);
        return axum::Json(publication(&snapshot.config)).into_response();
    }
    let Some(protocol) = crate::integrations::inference_protocol(
        parts.method.as_str(),
        parts.uri.path(),
        parts.uri.query(),
    ) else {
        return flight.reject(
            if parts.uri.query().is_some() {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::NOT_FOUND
            },
            "undeclared_operation",
            "operation_validation",
        );
    };
    flight.record.protocol = Some(protocol.into());
    if parts.headers.contains_key("upgrade") {
        return flight.reject(
            StatusCode::BAD_REQUEST,
            "undeclared_transport",
            "operation_validation",
        );
    }
    let bytes = tokio::select! {_ = flight.cancel.changed()=>{return flight.reject(StatusCode::SERVICE_UNAVAILABLE,"force_stop","local_admission");},result=tokio::time::timeout(Duration::from_secs(30),to_bytes(body,REQUEST_BYTES))=>{match result{Ok(Ok(v))=>v,Ok(Err(_))=>return flight.reject(StatusCode::PAYLOAD_TOO_LARGE,"request_too_large","local_admission"),Err(_)=>return flight.reject(StatusCode::REQUEST_TIMEOUT,"request_acquisition_deadline","local_admission")}}};
    let mut body: Value = match serde_json::from_slice(&bytes) {
        Ok(Value::Object(v)) => Value::Object(v),
        _ => {
            flight.record.model_kind = "malformed".into();
            return flight.reject(StatusCode::BAD_REQUEST, "invalid_json", "model_validation");
        }
    };
    let Some(public_id) = body
        .get("model")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
        .map(str::to_owned)
    else {
        flight.record.model_kind = "malformed".into();
        return flight.reject(StatusCode::BAD_REQUEST, "invalid_model", "model_validation");
    };
    let (snapshot, choice, now) = {
        let authority = runtime.snapshot.read().expect("snapshot lock");
        let Some(snapshot) = authority.as_ref().cloned() else {
            return flight.reject(
                StatusCode::SERVICE_UNAVAILABLE,
                "listener_unavailable",
                "local_admission",
            );
        };
        if !local_key(&parts.headers).is_some_and(|key| matches_key(key, &snapshot.key)) {
            return flight.reject(
                StatusCode::UNAUTHORIZED,
                "authentication_failed",
                "shared_key_authentication",
            );
        }
        let alias = snapshot
            .config
            .model_aliases
            .iter()
            .any(|a| a.id == public_id && a.protocol == protocol);
        let recognized = alias || crate::config::model(&snapshot.config, &public_id).is_ok();
        if !recognized {
            flight.record.model_kind = "unrecognized".into();
            return flight.reject(StatusCode::NOT_FOUND, "unknown_model", "model_validation");
        }
        flight.record.model = Some(public_id.clone());
        flight.record.model_kind = if alias { "alias" } else { "concrete" }.into();
        let now = Utc::now();
        let choice =
            match runtime.choose(&snapshot, &public_id, protocol, &body, &parts.headers, now) {
                Ok(v) => v,
                Err(e) => return flight.reject(StatusCode::CONFLICT, &e.code, "selection"),
            };
        (snapshot, choice, now)
    };
    let selection = choice.selection;
    let Some(provider) = snapshot.config.providers.get(&selection.provider) else {
        return flight.reject(StatusCode::BAD_REQUEST, "provider_unavailable", "selection");
    };
    if let Err(e) = crate::integrations::check_request(provider, &selection.model, protocol, &body)
    {
        return flight.reject(StatusCode::BAD_REQUEST, &e.code, "model_validation");
    }
    let endpoint = choice.endpoint;
    let path = match crate::integrations::operation_path(protocol) {
        Ok(v) => v,
        Err(e) => return flight.reject(StatusCode::BAD_REQUEST, &e.code, "operation_validation"),
    };
    flight.record.admitted_at = Some(now);
    flight.record.configuration_revision = Some(snapshot.revision.clone());
    flight.record.provider = Some(selection.provider.clone());
    flight.record.offering = Some(format!("{}/{}", selection.provider, selection.model));
    flight.record.authorization_generation = Some(choice.authorization_generation);
    flight.record.decision = safe_evidence(&selection.evidence);
    flight.record.stage = "credential_acquisition".into();
    runtime.history.update(flight.record.clone());
    let Some(Auth::ApiKey { key_ref }) = &provider.auth else {
        return flight.reject(
            StatusCode::SERVICE_UNAVAILABLE,
            "credential_unavailable",
            "credential_acquisition",
        );
    };
    let store = runtime.store.clone();
    let key_ref = key_ref.clone();
    let key = tokio::select! {biased;_ = flight.cancel.changed()=>return flight.reject(StatusCode::SERVICE_UNAVAILABLE,"force_stop","credential_acquisition"),result=credential(store,key_ref)=>match result{Ok(value)=>value,Err(error)=>return flight.reject(StatusCode::SERVICE_UNAVAILABLE,&error.code,"credential_acquisition")}};
    let key = match std::str::from_utf8(&key) {
        Ok(v) => v,
        Err(_) => {
            return flight.reject(
                StatusCode::SERVICE_UNAVAILABLE,
                "credential_invalid",
                "credential_acquisition",
            );
        }
    };
    body["model"] = Value::String(selection.model.clone());
    let go_session_header =
        if crate::integrations::integration_id(provider).ok() == Some("opencode-go") {
            choice.go_session_header
        } else {
            None
        };
    let upstream_url = match parts.uri.query() {
        Some(query) => format!("{}{}?{}", endpoint.trim_end_matches('/'), path, query),
        None => format!("{}{}", endpoint.trim_end_matches('/'), path),
    };
    let mut outbound = snapshot
        .client
        .post(upstream_url)
        .headers(upstream_headers(&parts.headers, go_session_header))
        .json(&body);
    if protocol == "messages"
        && crate::integrations::integration_id(provider).ok() != Some("opencode-go")
    {
        outbound = outbound.header("x-api-key", key);
    } else {
        outbound = outbound.bearer_auth(key);
    }
    flight.record.execution = "unknown".into();
    flight.record.stage = "upstream_dispatch".into();
    runtime.history.update(flight.record.clone());
    let response = tokio::select! {biased;_ = flight.cancel.changed()=>{flight.finish("interrupted","upstream_dispatch",Some("force_stop"));return error_response(StatusCode::SERVICE_UNAVAILABLE,"force_stop",&id);},result=outbound.send()=>match result{Ok(v)=>v,Err(e)=>{let code=if e.is_connect(){"upstream_connection_failed"}else{"upstream_transport_failed"};flight.finish("failed",if e.is_connect(){"upstream_connection"}else{"upstream_dispatch"},Some(code));return error_response(StatusCode::BAD_GATEWAY,code,&id);}}};
    let status = response.status();
    let headers = response.headers().clone();
    flight.record.upstream_status = Some(status.as_u16());
    flight.record.stage = "upstream_response".into();
    runtime.history.update(flight.record.clone());
    let sse = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/event-stream"));
    let state = ForwardStream {
        upstream: Box::pin(response.bytes_stream()),
        pending: Bytes::new(),
        flight,
        observer: Observer::new(protocol, sse, status.is_success()),
        done: false,
    };
    let stream = futures_util::stream::unfold(state, |mut state| async move {
        if state.done {
            return None;
        }
        let interrupted = *state.flight.cancel.borrow();
        if interrupted {
            state.observer.finish(&mut state.flight.record);
            state
                .flight
                .finish("interrupted", "upstream_stream", Some("force_stop"));
            state.done = true;
            return Some((Err(std::io::Error::other("request interrupted")), state));
        }
        if !state.pending.is_empty() {
            let n = state.pending.len().min(FORWARD_CHUNK_BYTES);
            let chunk = state.pending.split_to(n);
            return Some((Ok::<Bytes, std::io::Error>(chunk), state));
        }
        tokio::select! {
            biased;
            _ = state.flight.cancel.changed()=>{state.observer.finish(&mut state.flight.record);state.flight.finish("interrupted","upstream_stream",Some("force_stop"));state.done=true;Some((Err(std::io::Error::other("request interrupted")),state))},
            result=state.upstream.next()=>match result {
                Some(Ok(bytes))=>{state.observer.observe(&bytes);state.pending=bytes;let n=state.pending.len().min(FORWARD_CHUNK_BYTES);let chunk=state.pending.split_to(n);Some((Ok(chunk),state))},
                Some(Err(_))=>{state.observer.finish(&mut state.flight.record);state.flight.finish("failed","upstream_stream",Some("upstream_stream_failed"));state.done=true;Some((Err(std::io::Error::other("upstream stream failed")),state))},
                None=>{state.observer.finish(&mut state.flight.record);state.flight.record.downstream_complete=true;let outcome=if !state.observer.success{"failed"}else if state.observer.complete{"completed"}else{"unknown"};state.flight.finish(outcome,"downstream_delivery",if state.observer.complete{None}else{Some("completion_not_evidenced")});None}
            }
        }
    });
    let mut downstream = Response::new(Body::from_stream(stream));
    *downstream.status_mut() = status;
    for (name, value) in &headers {
        if !hop_header(name.as_str()) && name != "content-length" {
            downstream.headers_mut().insert(name.clone(), value.clone());
        }
    }
    if let Ok(v) = id.parse() {
        downstream.headers_mut().insert("x-tmax-request-id", v);
    }
    downstream
}
struct ForwardStream {
    upstream: Pin<Box<dyn Stream<Item = std::result::Result<Bytes, reqwest::Error>> + Send>>,
    pending: Bytes,
    flight: Flight,
    observer: Observer,
    done: bool,
}
impl Drop for ForwardStream {
    fn drop(&mut self) {
        if !self.flight.finished {
            self.observer.finish(&mut self.flight.record);
            let interrupted = *self.flight.cancel.borrow();
            self.flight.finish(
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
fn hop_header(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}
fn upstream_headers(headers: &HeaderMap, go_session_header: Option<&str>) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, value) in headers {
        if !hop_header(name.as_str())
            && !matches!(
                name.as_str(),
                "host"
                    | "authorization"
                    | "x-api-key"
                    | "x-gateway-key"
                    | "content-length"
                    | "origin"
                    | "referer"
                    | "cookie"
                    | "sec-fetch-site"
                    | "sec-fetch-mode"
                    | "sec-fetch-dest"
            )
        {
            out.insert(name.clone(), value.clone());
        }
    }
    if let Some(session) = go_session_header.and_then(|name| headers.get(name)) {
        // This is the actual qualified native session, not a generated or
        // provider-wide identity. Preserve the native header and truthful UA.
        out.insert("x-opencode-session", session.clone());
    }
    out
}

struct Observer {
    protocol: String,
    sse: bool,
    success: bool,
    buffer: Vec<u8>,
    discard: bool,
    complete: bool,
    usage: BTreeMap<String, u64>,
    partial: bool,
}
impl Observer {
    fn new(protocol: &str, sse: bool, success: bool) -> Self {
        Self {
            protocol: protocol.into(),
            sse,
            success,
            buffer: Vec::new(),
            discard: false,
            complete: false,
            usage: BTreeMap::new(),
            partial: false,
        }
    }
    fn observe(&mut self, bytes: &[u8]) {
        if self.sse {
            for byte in bytes {
                if *byte == b'\n' {
                    if !self.discard {
                        let mut line = std::mem::take(&mut self.buffer);
                        self.line(&line);
                        line.clear();
                        self.buffer = line;
                    }
                    self.buffer.clear();
                    self.discard = false;
                } else if !self.discard {
                    if self.buffer.len() == INSPECTION_BYTES {
                        self.buffer.clear();
                        self.discard = true;
                        self.partial = true;
                    } else {
                        self.buffer.push(*byte);
                    }
                }
            }
        } else if !self.discard {
            if self.buffer.len().saturating_add(bytes.len()) <= INSPECTION_BYTES {
                self.buffer.extend_from_slice(bytes);
            } else {
                self.buffer.clear();
                self.discard = true;
                self.partial = true;
            }
        }
    }
    fn line(&mut self, line: &[u8]) {
        let Some(data) = line.strip_prefix(b"data:") else {
            return;
        };
        let data = data.strip_prefix(b" ").unwrap_or(data);
        if data == b"[DONE]" || data == b"[DONE]\r" {
            if self.protocol == "chat-completions" {
                self.complete = true;
            }
            return;
        }
        if let Ok(value) = serde_json::from_slice::<Value>(data) {
            self.value(&value);
        }
    }
    fn value(&mut self, value: &Value) {
        if value.get("error").is_some() {
            self.success = false;
        }
        match value.get("type").and_then(Value::as_str) {
            Some("response.completed") => {
                self.complete = true;
                if let Some(response) = value.get("response") {
                    self.extract_usage(response.get("usage"));
                }
            }
            Some("message_stop") => self.complete = true,
            Some("response.failed" | "response.incomplete" | "error") => {
                self.success = false;
                self.complete = false;
            }
            _ => {}
        }
        if !self.sse {
            self.complete =
                match self.protocol.as_str() {
                    "chat-completions" => value
                        .get("choices")
                        .and_then(Value::as_array)
                        .is_some_and(|choices| {
                            !choices.is_empty()
                                && choices
                                    .iter()
                                    .all(|c| c.get("finish_reason").is_some_and(|r| !r.is_null()))
                        }),
                    "responses" => value.get("status").and_then(Value::as_str) == Some("completed"),
                    "messages" => value.get("stop_reason").is_some_and(|r| !r.is_null()),
                    _ => false,
                };
        }
        self.extract_usage(value.get("usage"));
        if let Some(message) = value.get("message") {
            self.extract_usage(message.get("usage"));
        }
    }
    fn extract_usage(&mut self, value: Option<&Value>) {
        if let Some(value) = value {
            for name in [
                "input_tokens",
                "output_tokens",
                "prompt_tokens",
                "completion_tokens",
                "total_tokens",
                "cache_read_input_tokens",
                "cache_creation_input_tokens",
            ] {
                if let Some(count) = value.get(name).and_then(Value::as_u64) {
                    self.usage.insert(name.into(), count);
                }
            }
        }
    }
    fn finish(&mut self, record: &mut Record) {
        if !self.sse
            && !self.discard
            && let Ok(value) = serde_json::from_slice::<Value>(&self.buffer)
        {
            self.value(&value);
        }
        record.usage = std::mem::take(&mut self.usage);
        record.usage_state = if record.usage.is_empty() {
            "unknown"
        } else if self.partial || !self.complete {
            "partial"
        } else {
            "reported"
        }
        .into();
        if self.complete {
            record.execution = "completion_evidenced".into();
        }
        record.evidence_gap |= self.partial;
    }
}

pub fn publication(config: &Config) -> Value {
    let mut data = Vec::new();
    for (provider_id, p) in &config.providers {
        if !p.enabled || p.auth.is_none() {
            continue;
        }
        for (model_id, m) in &p.models {
            if !m.enabled {
                continue;
            }
            if let Ok(protocols) = crate::integrations::protocols(p, model_id) {
                data.push(json!({"id":format!("{provider_id}/{model_id}"),"object":"model","owned_by":provider_id,"protocols":protocols}));
            }
        }
    }
    for a in &config.model_aliases {
        if a.enabled {
            data.push(json!({"id":a.id,"object":"model","protocols":[a.protocol],"continuity":a.continuity.as_deref().unwrap_or("session")}));
        }
    }
    json!({"object":"list","data":data})
}

/// Explicit paid, fixed-destination diagnostic request. It does not start a resident.
pub async fn probe(store: &Store, config: &Config, request: &Request) -> Result<Value> {
    let id = request
        .operands
        .first()
        .ok_or_else(|| Error::new("missing_operand", "Qualified offering required"))?;
    let arg = |name: &str| {
        request
            .arguments
            .get(name)
            .and_then(|v| v.first())
            .map(String::as_str)
    };
    let protocol =
        arg("protocol").ok_or_else(|| Error::new("missing_argument", "Protocol required"))?;
    let transport =
        arg("transport").ok_or_else(|| Error::new("missing_argument", "Transport required"))?;
    if transport != "http" {
        return Err(Error::new(
            "transport_unavailable",
            "Only the declared HTTP transport is available",
        ));
    }
    let (provider_id, model_id) = crate::config::qualified(id)?;
    let (provider, model) = crate::config::model(config, id)?;
    if !provider.enabled || !model.enabled {
        return Err(Error::new("model_unavailable", "Offering is disabled"));
    }
    let integration = crate::integrations::integration_id(provider)?;
    if matches!(
        integration,
        "codex-subscription" | "opencode-go" | "kimi-coding-plan"
    ) {
        return Err(Error::new(
            "native_identity_unqualified",
            "This probe lacks the qualified real native client identity required by the selected integration",
        ));
    }
    let mut body = match protocol {
        "chat-completions" => {
            json!({"model":model_id,"messages":[{"role":"user","content":"Reply OK."}],"max_tokens":8})
        }
        "responses" => {
            json!({"model":model_id,"input":"Reply OK.","store":false,"max_output_tokens":8})
        }
        "messages" => {
            json!({"model":model_id,"messages":[{"role":"user","content":"Reply OK."}],"max_tokens":8})
        }
        _ => return Err(Error::new("protocol_unavailable", "Unknown protocol")),
    };
    crate::integrations::check_request(provider, model_id, protocol, &body)?;
    let endpoints = crate::integrations::resolve(provider)?;
    let endpoint = endpoints
        .get(protocol)
        .ok_or_else(|| Error::new("protocol_unavailable", "Protocol endpoint unavailable"))?;
    let path = crate::integrations::operation_path(protocol)?;
    if request.dry_run {
        return Ok(
            json!({"dry_run":true,"offering":id,"provider":provider_id,"protocol":protocol,"transport":transport,"cost_risk":"May consume quota or incur unknown charges","request_shape":{"prompt":"fixed short diagnostic","output_token_limit":8},"inference_dispatched":false}),
        );
    }
    let Some(Auth::ApiKey { key_ref }) = &provider.auth else {
        return Err(Error::new(
            "credential_unavailable",
            "API-key authorization required",
        ));
    };
    let key = credential(store.clone(), key_ref.clone()).await?;
    let key = std::str::from_utf8(&key).map_err(|_| {
        Error::new(
            "credential_invalid",
            "Credential is not a supported header value",
        )
    })?;
    let client = crate::integrations::client(config)?;
    let started = Instant::now();
    let at = Utc::now();
    let mut outbound = client
        .post(format!("{}{}", endpoint.trim_end_matches('/'), path))
        .json(&body);
    if protocol == "messages" {
        outbound = outbound
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01");
    } else {
        outbound = outbound.bearer_auth(key);
    }
    body = Value::Null;
    drop(body);
    let response = outbound.send().await.map_err(|_| {
        Error::new(
            "probe_transport_failed",
            "Probe transport failed; upstream execution may be unknown, no retry performed",
        )
    })?;
    let status = response.status().as_u16();
    let mut stream = response.bytes_stream();
    let mut observer = Observer::new(protocol, false, (200..300).contains(&status));
    while let Some(chunk) = stream.next().await {
        let chunk=chunk.map_err(|_|Error::new("probe_stream_failed","Probe response interrupted; execution and usage may be unknown, no replay performed"))?;
        observer.observe(&chunk);
    }
    if !observer.discard
        && let Ok(value) = serde_json::from_slice::<Value>(&observer.buffer)
    {
        observer.value(&value);
    }
    Ok(
        json!({"offering":id,"provider":provider_id,"protocol":protocol,"transport":transport,"observed_at":at,"source":"explicit_inference_probe","upstream_status":status,"result":if observer.success&&observer.complete{"completed"}else if !observer.success{"failed"}else{"unknown"},"duration_ms":started.elapsed().as_millis(),"usage":observer.usage,"usage_state":if observer.partial{"partial"}else if observer.usage.is_empty(){"unknown"}else{"reported"},"cost_risk":"May consume quota or incur unknown charges","account_identity":"unverified","quota":"unknown"}),
    )
}

#[cfg(test)]
mod tests {
    use super::native_context::independent as independent_context;
    use super::*;
    #[test]
    fn usage_observer_preserves_terminal_semantics() {
        let mut observer = Observer::new("responses", true, true);
        observer.observe(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"secret\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":3,\"output_tokens\":2,\"secret\":\"x\"}}}\n\n");
        assert!(observer.complete);
        assert_eq!(observer.usage.len(), 2);
        assert_eq!(observer.usage["input_tokens"], 3);
    }
    #[test]
    fn response_observer_is_bounded() {
        let mut observer = Observer::new("responses", true, true);
        observer.observe(&vec![b'a'; INSPECTION_BYTES + 1]);
        assert!(observer.buffer.len() <= INSPECTION_BYTES);
        assert!(observer.partial);
        observer.observe(b"\ndata: {\"type\":\"response.completed\"}\n");
        assert!(observer.complete);
    }
    #[test]
    fn stateless_context_is_protocol_qualified() {
        assert!(independent_context(
            "chat-completions",
            &json!({"messages":[{"role":"user","content":"x"}]})
        ));
        assert!(!independent_context(
            "responses",
            &json!({"input":"x","previous_response_id":"id","store":false})
        ));
        assert!(!independent_context("responses", &json!({"input":"x"})));
        assert!(independent_context(
            "responses",
            &json!({"input":[],"store":false})
        ));
    }
    #[test]
    fn key_comparison_requires_exact_bytes() {
        assert!(matches_key(b"abc", b"abc"));
        assert!(!matches_key(b"abcx", b"abc"));
        assert!(!matches_key(b"ab", b"abc"));
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
    fn native_headers(at: chrono::DateTime<Utc>) -> HeaderMap {
        let millis = at.timestamp_millis();
        let mut bytes = [0u8; 16];
        bytes[..6].copy_from_slice(&millis.to_be_bytes()[2..]);
        bytes[6] = 0x70;
        bytes[8] = 0x80;
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "pi (linux test; x64)".parse().unwrap());
        headers.insert(
            "x-session-id",
            uuid::Uuid::from_bytes(bytes).to_string().parse().unwrap(),
        );
        headers
    }
    fn fixture() -> (tempfile::TempDir, Arc<Runtime>, Config) {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().into());
        let mut config = Config::empty();
        for (id, priority) in [("p", 10), ("q", 1)] {
            let mut provider = crate::config::Provider {
                integration: Some("custom".into()),
                enabled: true,
                ..Default::default()
            };
            provider
                .endpoints
                .insert("chat-completions".into(), "http://127.0.0.1:9/v1".into());
            provider.models.insert(
                "m".into(),
                crate::config::Model {
                    enabled: true,
                    priority,
                    ..Default::default()
                },
            );
            config.providers.insert(id.into(), provider);
        }
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        let initial = config.clone();
        crate::routing::reconcile(&store, &Config::empty(), &initial, &[]).unwrap();
        for id in ["p", "q"] {
            let identity = crate::inputs::provider_identity(&store, &initial, id)
                .unwrap()
                .unwrap();
            let reference = crate::secrets::put(
                &store,
                &format!("provider:{identity}:{id}"),
                b"test-provider-key",
            )
            .unwrap();
            config.providers.get_mut(id).unwrap().auth = Some(Auth::ApiKey { key_ref: reference });
        }
        config.model_aliases.push(crate::config::ModelAlias {
            id: "dynamic".into(),
            protocol: "chat-completions".into(),
            candidates: vec!["p/m".into(), "q/m".into()],
            enabled: true,
            continuity: None,
        });
        crate::routing::reconcile(&store, &initial, &config, &[]).unwrap();
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        crate::inputs::activate(&store, &config, "fixture-1").unwrap();
        let runtime = Runtime::new(store, "sessions".into()).unwrap();
        (temporary, runtime, config)
    }
    fn test_snapshot(config: &Config, revision: &str) -> Snapshot {
        Snapshot {
            config: config.clone(),
            revision: revision.into(),
            key: Zeroizing::new(b"test-local-key".to_vec()),
            client: crate::integrations::client(config).unwrap(),
            metadata: None,
        }
    }
    fn publish(runtime: &Runtime, old: &Config, new: &Config, revision: &str) {
        crate::routing::reconcile(&runtime.store, old, new, &[]).unwrap();
        let mut authority = runtime.snapshot.write().unwrap();
        crate::inputs::activate(&runtime.store, new, revision).unwrap();
        runtime.activate_bindings(new, false).unwrap();
        *authority = Some(Arc::new(test_snapshot(new, revision)));
    }
    #[test]
    fn arbitrary_aliases_use_only_explicit_members_across_calendar_boundaries() {
        let (_temporary, runtime, original) = fixture();
        let mut config = original.clone();
        let provider = config.providers.get_mut("p").unwrap();
        let scheduled = provider.models.get_mut("m").unwrap();
        scheduled.priority = 0;
        scheduled.timezone = Some("Etc/UTC".into());
        scheduled.schedule = vec![crate::config::Schedule {
            priority: 20,
            window: crate::config::TimeWindow {
                weekdays: Some(vec!["sat".into()]),
                start: Some("00:00".into()),
                end: Some("08:00".into()),
                ..Default::default()
            },
        }];
        provider.models.insert(
            "outside".into(),
            crate::config::Model {
                priority: 999,
                ..Default::default()
            },
        );
        for (id, members) in [
            ("L0", vec!["p/m", "q/m"]),
            ("L1", vec!["p/m"]),
            ("my.custom-name", vec!["q/m"]),
        ] {
            config.model_aliases.push(crate::config::ModelAlias {
                id: id.into(),
                protocol: "chat-completions".into(),
                candidates: members.into_iter().map(String::from).collect(),
                enabled: true,
                continuity: None,
            });
        }
        crate::config::validate(&config).unwrap();
        publish(&runtime, &original, &config, "explicit-calendar");
        let before = crate::inputs::instant("2026-10-24T07:59:59Z").unwrap();
        let boundary = crate::inputs::instant("2026-10-24T08:00:00Z").unwrap();
        let select = |id, at| {
            crate::routing::select(&runtime.store, &config, id, "chat-completions", at).unwrap()
        };
        let first = select("L0", before);
        assert_eq!(first.provider, "p");
        assert_eq!(first.priority, 20);
        assert_eq!(
            first.evidence["priority_evaluations"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let next = select("L0", boundary);
        assert_eq!(next.provider, "q");
        assert_eq!(next.priority, 1);
        assert_eq!(select("L1", boundary).provider, "p");
        assert_eq!(select("my.custom-name", before).provider, "q");
        assert_eq!(select("p/m", boundary).provider, "p");
        assert_eq!(select("p/outside", boundary).model, "outside");
        assert!(
            crate::routing::select(&runtime.store, &config, "L3", "chat-completions", before)
                .is_err()
        );
        assert!(
            crate::routing::select(&runtime.store, &config, "L0", "responses", before).is_err()
        );
        let mut disabled = config.clone();
        for provider in ["p", "q"] {
            disabled
                .providers
                .get_mut(provider)
                .unwrap()
                .models
                .get_mut("m")
                .unwrap()
                .enabled = false;
        }
        assert_eq!(
            crate::routing::select(
                &runtime.store,
                &disabled,
                "L0",
                "chat-completions",
                boundary
            )
            .unwrap_err()
            .code,
            "model_unavailable"
        );
        assert_eq!(
            crate::routing::select(
                &runtime.store,
                &disabled,
                "p/outside",
                "chat-completions",
                boundary
            )
            .unwrap()
            .model,
            "outside"
        );
    }

    #[tokio::test]
    async fn retained_session_explanation_survives_deleted_candidates_and_never_clears_blockers() {
        let (directory, runtime, config) = fixture();
        let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
        let choice = runtime
            .choose(
                &test_snapshot(&config, "fixture-1"),
                "dynamic",
                "chat-completions",
                &json!({"messages":[{"role":"user","content":"not-retained-body"}]}),
                &native_headers(at),
                at + chrono::Duration::milliseconds(1),
            )
            .unwrap();
        let handle = choice.selection.evidence["scope_handle"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(handle.starts_with("native-scope:"));
        let mut removed = config.clone();
        removed.providers.remove("p");
        removed.model_aliases.clear();
        removed.agents.insert(
            "changed-client".into(),
            crate::config::AgentTarget {
                agent: "codex".into(),
                scope: directory.path().join("native"),
                providers: [(
                    "gateway".into(),
                    crate::config::NativeProvider {
                        provider: None,
                        gateway: Some(true),
                        models: vec![],
                    },
                )]
                .into(),
                default: None,
                roles: Default::default(),
            },
        );
        publish(&runtime, &config, &removed, "removed");
        runtime
            .store
            .write_raw("tmax.toml", toml::to_string(&removed).unwrap().as_bytes())
            .unwrap();
        let before = std::fs::read(runtime.store.root.join("session-bindings.json")).unwrap();
        let request = crate::Request {
            command: vec!["model".into(), "explain".into()],
            operands: vec!["dynamic".into()],
            arguments: [
                ("protocol".into(), vec!["chat-completions".into()]),
                ("session".into(), vec![handle.clone()]),
                ("target".into(), vec!["changed-client".into()]),
                ("agent-provider".into(), vec!["gateway".into()]),
            ]
            .into(),
            dry_run: false,
            if_revision: None,
        };
        let evidence = crate::operations::explain(&runtime.store, &removed, &request)
            .await
            .unwrap();
        assert_eq!(evidence["selection"]["qualified_model"], "p/m");
        assert_eq!(evidence["continuation"]["reason_code"], "provider_removed");
        assert_eq!(
            evidence["continuation"]["stored_blocker"],
            "provider_removed"
        );
        assert_eq!(evidence["fresh_ranking"], "not_evaluated");
        assert_eq!(evidence["native_assessment"]["state"], "not_reassessed");
        assert!(!evidence.to_string().contains("not-retained-body"));
        assert_eq!(
            std::fs::read(runtime.store.root.join("session-bindings.json")).unwrap(),
            before
        );
        for (id, protocol, session) in [
            ("different", "chat-completions", handle.as_str()),
            ("dynamic", "responses", handle.as_str()),
            ("dynamic", "chat-completions", "unrelated-handle"),
        ] {
            assert_eq!(
                Runtime::inspect_persisted_session(&runtime.store, &removed, id, protocol, session)
                    .unwrap_err()
                    .code,
                "original_binding_unavailable"
            );
        }
        let restored = Runtime::inspect_persisted_session(
            &runtime.store,
            &config,
            "dynamic",
            "chat-completions",
            &handle,
        )
        .unwrap();
        assert_eq!(
            restored["continuation"]["stored_blocker"],
            "provider_removed"
        );
        assert_eq!(restored["selection"]["qualified_model"], "p/m");
    }

    #[test]
    fn retained_selection_survives_priority_change_history_clear_and_restart() {
        let (_temporary, runtime, config) = fixture();
        let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
        let headers = native_headers(at);
        let body = json!({"messages":[{"role":"user","content":"x"}]});
        let first = runtime
            .choose(
                &test_snapshot(&config, "fixture-1"),
                "dynamic",
                "chat-completions",
                &body,
                &headers,
                at + chrono::Duration::milliseconds(1),
            )
            .unwrap();
        assert_eq!(first.selection.provider, "p");
        let original = first.selection.evidence.clone();
        let mut updated = config.clone();
        updated
            .providers
            .get_mut("q")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .priority = 100;
        publish(&runtime, &config, &updated, "fixture-2");
        runtime.history.clear(false);
        let continuation = json!({"messages":[{"role":"user","content":"x"},{"role":"assistant","content":"y"},{"role":"user","content":"z"}]});
        let retained = runtime
            .choose(
                &test_snapshot(&updated, "fixture-2"),
                "dynamic",
                "chat-completions",
                &continuation,
                &headers,
                at + chrono::Duration::seconds(1),
            )
            .unwrap();
        assert_eq!(retained.selection.provider, "p");
        assert_eq!(
            retained.selection.evidence["selection_reason"],
            "retained_destination"
        );
        assert_eq!(first.selection.evidence, original);
        let store = runtime.store.clone();
        drop(runtime);
        let restarted = Runtime::new(store, "restart".into()).unwrap();
        let after = restarted
            .choose(
                &test_snapshot(&updated, "fixture-2"),
                "dynamic",
                "chat-completions",
                &continuation,
                &headers,
                at + chrono::Duration::seconds(2),
            )
            .unwrap();
        assert_eq!(after.selection.provider, "p");
        let fresh = native_headers(at + chrono::Duration::seconds(3));
        let new = restarted
            .choose(
                &test_snapshot(&updated, "fixture-2"),
                "dynamic",
                "chat-completions",
                &body,
                &fresh,
                at + chrono::Duration::seconds(4),
            )
            .unwrap();
        assert_eq!(new.selection.provider, "q");
    }
    #[test]
    fn persisted_binding_preview_reports_blockers_without_publication() {
        let (_temporary, runtime, config) = fixture();
        let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
        let headers = native_headers(at);
        let body = json!({"messages":[{"role":"user","content":"x"}]});
        runtime
            .choose(
                &test_snapshot(&config, "fixture-1"),
                "dynamic",
                "chat-completions",
                &body,
                &headers,
                at + chrono::Duration::milliseconds(1),
            )
            .unwrap();
        let bindings = std::fs::read(runtime.store.root.join("session-bindings.json")).unwrap();
        let routing = std::fs::read(runtime.store.root.join("routing-state.json")).unwrap();
        let mut disabled = config.clone();
        disabled.model_aliases[0].enabled = false;
        let impact = Runtime::preview_persisted(&runtime.store, &disabled).unwrap();
        assert_eq!(
            impact["blocked_sessions"][0]["reason_code"],
            "alias_disabled"
        );
        assert_eq!(
            std::fs::read(runtime.store.root.join("session-bindings.json")).unwrap(),
            bindings
        );
        assert_eq!(
            std::fs::read(runtime.store.root.join("routing-state.json")).unwrap(),
            routing
        );
        assert_eq!(runtime.binding_summary()["blocked"], 0);
    }
    #[test]
    fn failed_binding_activation_preserves_live_state_and_retry_persists_blockers() {
        let (_temporary, runtime, config) = fixture();
        let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
        let headers = native_headers(at);
        let body = json!({"messages":[{"role":"user","content":"x"}]});
        runtime
            .choose(
                &test_snapshot(&config, "fixture-1"),
                "dynamic",
                "chat-completions",
                &body,
                &headers,
                at + chrono::Duration::milliseconds(1),
            )
            .unwrap();
        let before = serde_json::to_value(&*runtime.bindings.lock().unwrap()).unwrap();
        let path = runtime.store.root.join("session-bindings.json");
        let original = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let mut disabled = config.clone();
        disabled.model_aliases[0].enabled = false;
        crate::routing::reconcile(&runtime.store, &config, &disabled, &[]).unwrap();
        crate::inputs::activate(&runtime.store, &disabled, "disabled").unwrap();
        assert!(runtime.activate_bindings(&disabled, false).is_err());
        assert_eq!(
            serde_json::to_value(&*runtime.bindings.lock().unwrap()).unwrap(),
            before
        );
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, original).unwrap();
        let impact = runtime.activate_bindings(&disabled, false).unwrap();
        assert_eq!(
            impact["blocked_sessions"][0]["reason_code"],
            "alias_disabled"
        );
        let restarted = Runtime::new(runtime.store.clone(), "retry".into()).unwrap();
        let error = restarted
            .choose(
                &test_snapshot(&config, "fixture-1"),
                "dynamic",
                "chat-completions",
                &body,
                &headers,
                at + chrono::Duration::seconds(1),
            )
            .err()
            .unwrap();
        assert_eq!(error.code, "alias_disabled");
    }
    #[test]
    fn stateless_activation_cannot_bypass_a_retained_session_block() {
        let (_temporary, runtime, config) = fixture();
        let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
        let headers = native_headers(at);
        let body = json!({"messages":[{"role":"user","content":"x"}]});
        let first = runtime
            .choose(
                &test_snapshot(&config, "fixture-1"),
                "dynamic",
                "chat-completions",
                &body,
                &headers,
                at + chrono::Duration::milliseconds(1),
            )
            .unwrap();
        assert_eq!(first.selection.provider, "p");
        let mut stateless = config.clone();
        stateless.model_aliases[0].continuity = Some("stateless".into());
        stateless
            .providers
            .get_mut("q")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .priority = 100;
        publish(&runtime, &config, &stateless, "stateless");
        let error = runtime
            .choose(
                &test_snapshot(&stateless, "stateless"),
                "dynamic",
                "chat-completions",
                &body,
                &headers,
                at + chrono::Duration::seconds(1),
            )
            .err()
            .unwrap();
        assert_eq!(error.code, "continuity_changed");
        let fresh = native_headers(at + chrono::Duration::seconds(2));
        let choice = runtime
            .choose(
                &test_snapshot(&stateless, "stateless"),
                "dynamic",
                "chat-completions",
                &body,
                &fresh,
                at + chrono::Duration::seconds(3),
            )
            .unwrap();
        assert_eq!(choice.selection.provider, "q");
    }
    #[test]
    fn incompatible_activation_block_is_permanent_without_reassignment() {
        let (_temporary, runtime, config) = fixture();
        let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
        let headers = native_headers(at);
        let body = json!({"messages":[{"role":"user","content":"x"}]});
        runtime
            .choose(
                &test_snapshot(&config, "fixture-1"),
                "dynamic",
                "chat-completions",
                &body,
                &headers,
                at + chrono::Duration::milliseconds(1),
            )
            .unwrap();
        let mut removed = config.clone();
        removed.model_aliases[0].candidates = vec!["q/m".into()];
        publish(&runtime, &config, &removed, "removed");
        publish(&runtime, &removed, &config, "restored");
        let store = runtime.store.clone();
        let bindings_before = std::fs::read(store.root.join("session-bindings.json")).unwrap();
        let mut legacy = store.read_json::<Value>("routing-state.json").unwrap();
        for view in ["saved", "loaded"] {
            for provider in legacy[view]["config"]["providers"]
                .as_object_mut()
                .unwrap()
                .values_mut()
            {
                provider["models"]["m"]["tier"] = json!("L0");
            }
            legacy[view]["config"]["model_aliases"][0]["tier"] = json!("L0");
            legacy[view]["config"]["model_aliases"][0]["candidates"] = Value::Null;
        }
        store.write_json("routing-state.json", &legacy).unwrap();
        drop(runtime);
        crate::routing::reconcile(&store, &config, &config, &[]).unwrap();
        assert_eq!(
            std::fs::read(store.root.join("session-bindings.json")).unwrap(),
            bindings_before
        );
        let runtime = Runtime::new(store, "after-historical-cutover".into()).unwrap();
        let error = runtime
            .choose(
                &test_snapshot(&config, "restored"),
                "dynamic",
                "chat-completions",
                &body,
                &headers,
                at + chrono::Duration::seconds(1),
            )
            .err()
            .unwrap();
        assert_eq!(error.code, "candidate_removed");
        assert_eq!(runtime.bindings.lock().unwrap().entries.len(), 1);
    }
    #[test]
    fn request_capabilities_filter_before_alias_ranking() {
        let (_temporary, runtime, config) = fixture();
        let mut supported = config.clone();
        supported
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .overrides
            .tool_call = Some(false);
        supported
            .providers
            .get_mut("q")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .overrides
            .tool_call = Some(true);
        publish(&runtime, &config, &supported, "capabilities");
        let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
        let headers = native_headers(at);
        let body = json!({"messages":[{"role":"user","content":"x"}],"tools":[{"type":"function","function":{"name":"test","parameters":{"type":"object"}}}],"temperature":0,"extension":{"enabled":false}});
        let original = body.clone();
        let choice = runtime
            .choose(
                &test_snapshot(&supported, "capabilities"),
                "dynamic",
                "chat-completions",
                &body,
                &headers,
                at + chrono::Duration::milliseconds(1),
            )
            .unwrap();
        assert_eq!(choice.selection.provider, "q");
        assert!(
            runtime
                .choose(
                    &test_snapshot(&supported, "capabilities"),
                    "p/m",
                    "chat-completions",
                    &body,
                    &headers,
                    at + chrono::Duration::milliseconds(1)
                )
                .is_err()
        );
        assert_eq!(body, original);
    }
    #[test]
    fn native_requirements_filter_new_selection_but_never_move_retained_destination() {
        let (_temporary, runtime, config) = fixture();
        let mut go = config.clone();
        let p = go.providers.get_mut("p").unwrap();
        p.integration = Some("opencode-go".into());
        p.endpoints.clear();
        let mut stateless = go.clone();
        stateless.model_aliases[0].continuity = Some("stateless".into());
        publish(&runtime, &config, &stateless, "native-stateless");
        let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
        let mut local = native_headers(at);
        local.remove("x-session-id");
        let body = json!({"messages":[{"role":"user","content":"x"}]});
        let lower = runtime
            .choose(
                &test_snapshot(&stateless, "native-stateless"),
                "dynamic",
                "chat-completions",
                &body,
                &local,
                at + chrono::Duration::milliseconds(1),
            )
            .unwrap();
        assert_eq!(lower.selection.provider, "q");
        publish(&runtime, &stateless, &go, "native-context");
        let mut native = native_headers(at + chrono::Duration::seconds(1));
        let session = native["x-session-id"].clone();
        let high = runtime
            .choose(
                &test_snapshot(&go, "native-context"),
                "dynamic",
                "chat-completions",
                &body,
                &native,
                at + chrono::Duration::seconds(2),
            )
            .unwrap();
        assert_eq!(high.selection.provider, "p");
        native.insert("x-opencode-session", session.clone());
        let retained = runtime
            .choose(
                &test_snapshot(&go, "native-context"),
                "dynamic",
                "chat-completions",
                &body,
                &native,
                at + chrono::Duration::seconds(3),
            )
            .unwrap();
        assert_eq!(retained.selection.provider, "p");
        assert_eq!(
            retained.selection.evidence["selection_reason"],
            "retained_destination"
        );
        native.insert("x-opencode-session", "conflicting-session".parse().unwrap());
        let error = runtime
            .choose(
                &test_snapshot(&go, "native-context"),
                "dynamic",
                "chat-completions",
                &body,
                &native,
                at + chrono::Duration::seconds(3),
            )
            .err()
            .unwrap();
        assert_eq!(error.code, "session_identity_unavailable");
        let bindings = runtime.bindings.lock().unwrap();
        assert_eq!(
            bindings
                .entries
                .values()
                .filter(|binding| binding.provider == "p")
                .count(),
            1
        );
    }
    #[test]
    fn pi_go_forwarding_preserves_native_identity_across_turns_resume_and_new_sessions() {
        let (_temporary, runtime, config) = fixture();
        let mut go = config.clone();
        let provider = go.providers.get_mut("p").unwrap();
        provider.integration = Some("opencode-go".into());
        provider.endpoints.clear();
        publish(&runtime, &config, &go, "go-affinity");
        let at = runtime.bindings.lock().unwrap().epoch + chrono::Duration::milliseconds(2);
        let headers = native_headers(at);
        let initial = json!({"messages":[{"role":"user","content":"x"}],"stream":true});
        let continuation = json!({"messages":[{"role":"user","content":"x"},{"role":"assistant","content":"y"},{"role":"user","content":"z"}],"stream":false});
        let tool_turn = json!({"messages":[{"role":"user","content":"x"},{"role":"assistant","tool_calls":[{"id":"call-1","type":"function","function":{"name":"test","arguments":"{}"}}]},{"role":"tool","tool_call_id":"call-1","content":"ok"}],"stream":true});
        let auxiliary = json!({"messages":[{"role":"user","content":"summarize"}],"stream":false});
        let snapshot = test_snapshot(&go, "go-affinity");
        for body in [&initial, &continuation, &tool_turn, &auxiliary] {
            let choice = runtime
                .choose(&snapshot, "p/m", "chat-completions", body, &headers, at)
                .unwrap();
            assert_eq!(choice.selection.provider, "p");
            assert_eq!(choice.go_session_header, Some("x-session-id"));
            let forwarded = upstream_headers(&headers, choice.go_session_header);
            assert_eq!(forwarded["x-opencode-session"], headers["x-session-id"]);
            assert_eq!(forwarded["x-session-id"], headers["x-session-id"]);
            assert_eq!(forwarded["user-agent"], headers["user-agent"]);
        }
        let store = runtime.store.clone();
        drop(runtime);
        let resumed = Runtime::new(store, "go-resume".into()).unwrap();
        let retained = resumed
            .choose(
                &snapshot,
                "p/m",
                "chat-completions",
                &continuation,
                &headers,
                at,
            )
            .unwrap();
        assert_eq!(
            retained.selection.evidence["selection_reason"],
            "retained_destination"
        );
        let forwarded = upstream_headers(&headers, retained.go_session_header);
        assert_eq!(forwarded["x-opencode-session"], headers["x-session-id"]);
        let fresh = native_headers(at + chrono::Duration::seconds(1));
        let new = resumed
            .choose(
                &snapshot,
                "p/m",
                "chat-completions",
                &initial,
                &fresh,
                at + chrono::Duration::seconds(1),
            )
            .unwrap();
        let forwarded_new = upstream_headers(&fresh, new.go_session_header);
        assert_ne!(
            forwarded_new["x-opencode-session"],
            forwarded["x-opencode-session"]
        );
        assert_eq!(resumed.bindings.lock().unwrap().entries.len(), 2);
        let mut disabled = go.clone();
        disabled.providers.get_mut("p").unwrap().enabled = false;
        publish(&resumed, &go, &disabled, "go-disabled");
        publish(&resumed, &disabled, &go, "go-restored");
        let error = resumed
            .choose(
                &snapshot,
                "p/m",
                "chat-completions",
                &continuation,
                &headers,
                at,
            )
            .err()
            .unwrap();
        assert_eq!(error.code, "provider_disabled");
        assert_eq!(resumed.bindings.lock().unwrap().entries.len(), 2);
    }

    #[tokio::test]
    async fn go_identity_rejections_retain_exact_history_reason_without_dispatch() {
        let (_temporary, runtime, config) = fixture();
        let mut go = config.clone();
        let provider = go.providers.get_mut("p").unwrap();
        provider.integration = Some("opencode-go".into());
        provider.endpoints.clear();
        provider.models.get_mut("m").unwrap().overrides.tool_call = Some(false);
        go.model_aliases[0].candidates = vec!["p/m".into()];
        go.model_aliases[0].continuity = Some("stateless".into());
        publish(&runtime, &config, &go, "go-rejections");
        runtime.open();
        let at = Utc::now();
        runtime.bindings.lock().unwrap().epoch = at - chrono::Duration::seconds(1);
        let qualified = native_headers(at);
        let mut missing = qualified.clone();
        missing.remove("x-session-id");
        let mut unknown = qualified.clone();
        unknown.insert("user-agent", "generic-client".parse().unwrap());
        let mut unknown_recommended = unknown.clone();
        unknown_recommended.insert("x-opencode-session", qualified["x-session-id"].clone());
        let mut conflict = qualified.clone();
        conflict.insert("x-opencode-session", "conflicting-session".parse().unwrap());
        let mut hermes_affinity = qualified.clone();
        hermes_affinity.insert("user-agent", "HermesAgent/0.22.1".parse().unwrap());
        hermes_affinity.insert("x-session-id", "native-hermes-session".parse().unwrap());
        let body = json!({"model":"p/m","messages":[{"role":"user","content":"x"}]});
        let mut cases = Vec::new();
        for headers in [
            HeaderMap::new(),
            missing.clone(),
            unknown,
            unknown_recommended,
            conflict,
            hermes_affinity,
        ] {
            cases.push((
                "/v1/chat/completions",
                body.clone(),
                headers,
                "native_session_required",
            ));
        }
        cases.extend([
            ("/v1/chat/completions", json!({"model":"dynamic","messages":[{"role":"user","content":"x"}]}), missing, "native_session_required"),
            ("/v1/responses", json!({"model":"p/m","input":[{"role":"user","content":"x"}],"store":false}), qualified.clone(), "native_session_required"),
            ("/v1/chat/completions", json!({"model":"p/m","messages":[{"role":"user","content":"x"}],"tools":[{"type":"function","function":{"name":"test","parameters":{"type":"object"}}}]}), qualified, "tools_incompatible"),
        ]);
        let mut expected = Vec::new();
        for (path, body, headers, reason) in cases {
            let mut request = axum::http::Request::builder()
                .method(Method::POST)
                .uri(path)
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap();
            *request.headers_mut() = headers;
            request
                .headers_mut()
                .insert("authorization", "Bearer test-local-key".parse().unwrap());
            assert_eq!(
                forward(State(runtime.clone()), request).await.status(),
                StatusCode::CONFLICT
            );
            expected.push(reason);
        }
        let mut query = Request {
            command: vec!["history".into(), "list".into()],
            operands: Vec::new(),
            arguments: BTreeMap::new(),
            dry_run: false,
            if_revision: None,
        };
        let listed = runtime.history.view(&query).unwrap();
        let records = listed["records"].as_array().unwrap();
        assert_eq!(records.len(), expected.len());
        query.command[1] = "inspect".into();
        for (row, reason) in records.iter().zip(expected) {
            query.operands = vec![row["request"].as_str().unwrap().into()];
            let inspected = runtime.history.view(&query).unwrap();
            let record = &inspected["record"];
            assert_eq!(record["reason_code"], reason);
            assert_eq!(record["outcome"], "rejected");
            assert_eq!(record["stage"], "selection");
            assert_eq!(record["execution"], "not_dispatched");
            assert!(record["upstream_status"].is_null());
        }
        assert!(runtime.bindings.lock().unwrap().entries.is_empty());
    }
    #[tokio::test]
    async fn messages_beta_selector_reaches_upstream_and_other_queries_never_dispatch() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (sent, mut received) = tokio::sync::mpsc::channel(4);
        let upstream = Router::new().fallback(move |request: axum::extract::Request| {
            let sent = sent.clone();
            async move {
                let (parts, body) = request.into_parts();
                let body = to_bytes(body, 4096).await.unwrap();
                sent.send((parts.uri.to_string(), parts.headers, serde_json::from_slice::<Value>(&body).unwrap())).await.unwrap();
                axum::Json(json!({"type":"message","stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}))
            }
        });
        let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
        let (_temporary, runtime, config) = fixture();
        let mut messages = config.clone();
        let provider = messages.providers.get_mut("p").unwrap();
        provider.endpoints.clear();
        provider
            .endpoints
            .insert("messages".into(), format!("http://{address}"));
        publish(&runtime, &config, &messages, "messages-query");
        runtime.open();
        for path in ["/v1/messages", "/v1/messages?beta=true"] {
            let body = json!({"model":"p/m","messages":[{"role":"user","content":"x"}],"max_tokens":1,"stream":false,"extension":{"enabled":false}});
            let request = axum::http::Request::builder()
                .method(Method::POST)
                .uri(path)
                .header("authorization", "Bearer test-local-key")
                .header("user-agent", "pi (linux test; x64)")
                .header("anthropic-beta", "test-feature")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap();
            let response = forward(State(runtime.clone()), request).await;
            assert_eq!(response.status(), StatusCode::OK);
            to_bytes(response.into_body(), 4096).await.unwrap();
            let (actual_path, headers, actual_body) = received.recv().await.unwrap();
            assert_eq!(actual_path, path);
            assert_eq!(headers["user-agent"], "pi (linux test; x64)");
            assert_eq!(headers["anthropic-beta"], "test-feature");
            assert_eq!(headers["x-api-key"], "test-provider-key");
            assert!(!headers.contains_key("authorization"));
            let mut expected = body;
            expected["model"] = json!("m");
            assert_eq!(actual_body, expected);
        }
        let invalid = [
            "/v1/messages?unknown=true",
            "/v1/messages?beta=false",
            "/v1/messages?beta=true&beta=true",
            "/v1/messages?beta=true&beta=false",
            "/v1/messages?beta=true&unknown=true",
            "/v1/messages/count_tokens?beta=true",
            "/v1/responses?beta=true",
            "/v1/chat/completions?beta=true",
        ];
        for path in invalid {
            let request = axum::http::Request::builder()
                .method(Method::POST)
                .uri(path)
                .header("authorization", "Bearer test-local-key")
                .body(Body::from(
                    r#"{"model":"p/m","messages":[{"role":"user","content":"x"}]}"#,
                ))
                .unwrap();
            assert_eq!(
                forward(State(runtime.clone()), request).await.status(),
                StatusCode::BAD_REQUEST
            );
        }
        assert!(received.try_recv().is_err());
        let mut query = Request {
            command: vec!["history".into(), "list".into()],
            operands: Vec::new(),
            arguments: BTreeMap::from([("outcome".into(), vec!["rejected".into()])]),
            dry_run: false,
            if_revision: None,
        };
        let history = runtime.history.view(&query).unwrap();
        let rejected = history["records"].as_array().unwrap();
        assert_eq!(rejected.len(), invalid.len());
        query.command[1] = "inspect".into();
        for record in rejected {
            query.operands = vec![record["request"].as_str().unwrap().into()];
            let inspected = runtime.history.view(&query).unwrap();
            assert_eq!(inspected["record"]["reason_code"], "undeclared_operation");
            assert_eq!(inspected["record"]["stage"], "operation_validation");
            assert_eq!(inspected["record"]["execution"], "not_dispatched");
        }
        server.abort();
    }
}
