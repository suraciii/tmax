use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

fn yes() -> bool {
    true
}
/// Agents whose native default effort is a caller-facing name shaped by the model projection.
pub(crate) const PROJECTED_EFFORT_AGENTS: &[&str] = &["dsh", "pi", "omp", "opencode"];
/// Agents whose native default effort is a verbatim native value with no selector validation.
pub(crate) const NATIVE_EFFORT_AGENTS: &[&str] = &["codex", "hermes"];
/// Reserved provider ID for the one provider derived from the configured gateway.
/// The gateway owns its definition; it is never stored in the desired configuration.
pub const GATEWAY_PROVIDER_ID: &str = "tmax";
/// Current desired-configuration format.
pub const CONFIG_VERSION: i64 = 2;
/// Format written before routes merged their per-protocol records.
pub const LEGACY_CONFIG_VERSION: i64 = 1;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: i64,
    #[serde(default)]
    pub providers: BTreeMap<String, Provider>,
    #[serde(default)]
    pub model_aliases: Vec<ModelAlias>,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentTarget>,
    #[serde(alias = "proxy")]
    pub gateway: Option<Gateway>,
    pub network: Option<Network>,
}
impl Config {
    pub fn empty() -> Self {
        Self {
            version: CONFIG_VERSION,
            ..Self::default()
        }
    }
}
impl Config {
    pub(crate) fn candidate_effort(
        &self,
        id: &str,
        protocol: &str,
        provider: &str,
        model: &str,
    ) -> Option<&str> {
        self.model_aliases
            .iter()
            .find(|alias| alias.id == id && alias.publishes(protocol))?
            .candidate_reasoning_effort
            .iter()
            .find(|(name, _)| name.split_once('/') == Some((provider, model)))
            .map(|(_, effort)| effort.as_str())
    }
}
impl Config {
    /// Whether `id` names the derived gateway provider for this configuration.
    pub fn is_gateway_provider(&self, id: &str) -> bool {
        self.gateway.is_some() && id == GATEWAY_PROVIDER_ID
    }
    /// The user-authored desired configuration: derived providers are excluded.
    pub fn desired(&self) -> Config {
        let mut desired = self.clone();
        if self.gateway.is_some() {
            desired.providers.remove(GATEWAY_PROVIDER_ID);
        }
        desired
    }
}
/// Delivery-relevant facts of the configured gateway.
///
/// Relayed native entries embed a revision of these facts, so a change is
/// reported as named drift instead of being recomputed at each read.
#[derive(Debug, Clone, Serialize)]
pub struct GatewayDelivery {
    pub revision: String,
    pub listen: String,
    pub key_ref: String,
    /// Enabled public route names, sorted.
    pub routes: Vec<String>,
}

impl GatewayDelivery {
    /// Named drift reasons for one applied target.
    ///
    /// `applied.routes` holds the routes that target referenced when it was
    /// applied, so an added unrelated route does not make an applied target
    /// stale while a removed one does.
    pub fn drift(&self, applied: &Self) -> Vec<&'static str> {
        let mut reasons = Vec::new();
        if self.listen != applied.listen {
            reasons.push("listen_changed");
        }
        if self.key_ref != applied.key_ref {
            reasons.push("key_rotated");
        }
        if applied
            .routes
            .iter()
            .any(|route| !self.routes.contains(route))
        {
            reasons.push("route_removed");
        }
        // The revision is target-scoped, so a difference without a removed
        // route means the target's own selection changed since it was applied
        // (added or dropped route, or a changed selection protocol).
        if reasons.is_empty() && self.revision != applied.revision {
            reasons.push("entry_selection_changed");
        }
        reasons
    }
}

impl Config {
    /// `route/protocol` selections referenced by one target's relayed entries.
    ///
    /// Route names cannot contain `/` (enforced by alias validation), so the
    /// pair spelling is unambiguous; a protocol change therefore changes the
    /// target's delivery revision even though the route name stays the same.
    pub fn relayed_route_pairs(&self, target: &AgentTarget) -> Vec<String> {
        let mut pairs = target
            .providers
            .values()
            .filter(|entry| entry.is_relayed(self))
            .flat_map(|entry| {
                entry
                    .models
                    .iter()
                    .map(|selection| format!("{}/{}", selection.id, selection.protocol))
            })
            .collect::<Vec<_>>();
        pairs.sort();
        pairs.dedup();
        pairs
    }

    /// Route selections of one target whose route is not an enabled route
    /// publishing the selection's protocol.
    pub fn missing_relayed_routes(&self, target: &AgentTarget) -> Vec<(String, String)> {
        target
            .providers
            .values()
            .filter(|entry| entry.is_relayed(self))
            .flat_map(|entry| {
                entry.models.iter().filter_map(|selection| {
                    let published = self.model_aliases.iter().any(|alias| {
                        alias.id == selection.id
                            && alias.enabled
                            && alias.publishes(&selection.protocol)
                    });
                    (!published).then(|| (selection.id.clone(), selection.protocol.clone()))
                })
            })
            .collect()
    }
}

/// Revision over delivery facts and the routes one consumer depends on.
fn delivery_revision(listen: &str, key_ref: &str, routes: &[String]) -> String {
    let mut digest = Sha256::new();
    digest.update(listen.as_bytes());
    digest.update(b"\0");
    digest.update(key_ref.as_bytes());
    digest.update(b"\0");
    for route in routes {
        digest.update(route.as_bytes());
        digest.update(b"\0");
    }
    format!("{:x}", digest.finalize())
}

/// Current provider-level delivery facts, or `None` when no gateway is configured.
pub fn gateway_delivery(config: &Config) -> Option<GatewayDelivery> {
    let gateway = config.gateway.as_ref()?;
    let mut routes = config
        .model_aliases
        .iter()
        .filter(|alias| alias.enabled)
        .map(|alias| alias.id.clone())
        .collect::<Vec<_>>();
    routes.sort();
    Some(GatewayDelivery {
        revision: delivery_revision(&gateway.listen, &gateway.key_ref, &routes),
        listen: gateway.listen.clone(),
        key_ref: gateway.key_ref.clone(),
        routes,
    })
}

/// Current delivery facts of the route/protocol pairs one target references.
///
/// The revision is target-scoped over those pairs, so adding an unrelated route
/// does not make an applied target stale, while changing the target's own
/// selection does.
pub fn gateway_delivery_for(config: &Config, referenced: &[String]) -> Option<GatewayDelivery> {
    let mut delivery = gateway_delivery(config)?;
    let enabled = config
        .model_aliases
        .iter()
        .filter(|alias| alias.enabled)
        .flat_map(|alias| {
            alias
                .protocols
                .iter()
                .map(move |protocol| format!("{}/{}", alias.id, protocol))
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut pairs = referenced
        .iter()
        .filter(|pair| enabled.contains(*pair))
        .cloned()
        .collect::<Vec<_>>();
    pairs.sort();
    pairs.dedup();
    delivery.routes = pairs
        .iter()
        .filter_map(|pair| pair.split_once('/').map(|(route, _)| route.to_owned()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    delivery.revision = delivery_revision(&delivery.listen, &delivery.key_ref, &pairs);
    Some(delivery)
}

/// The one provider the configured gateway contributes to the inventory.
///
/// It is derived from the gateway and the declared routes, never stored as
/// desired configuration. Its models are the gateway's public route names.
pub fn gateway_provider(config: &Config) -> Option<Provider> {
    let gateway = config.gateway.as_ref()?;
    let base = format!("http://{}", gateway.listen);
    let endpoints = PROTOCOLS
        .iter()
        .map(|protocol| ((*protocol).to_string(), base.clone()))
        .collect();
    let mut models: BTreeMap<String, Model> = BTreeMap::new();
    for alias in &config.model_aliases {
        models
            .entry(alias.id.clone())
            .and_modify(|model| model.enabled |= alias.enabled)
            .or_insert_with(|| Model {
                enabled: alias.enabled,
                ..Model::default()
            });
    }
    Some(Provider {
        metadata: None,
        integration: Some(crate::provider_integration::GATEWAY_INTEGRATION.into()),
        enabled: true,
        endpoints,
        overrides: ProviderOverrides::default(),
        auth: Some(Auth::ApiKey {
            key_ref: gateway.key_ref.clone(),
        }),
        subscription: None,
        models,
        inputs: BTreeMap::new(),
        failure_policy: None,
        quota_guard: None,
    })
}
pub(crate) fn apply_gateway_provider(config: &mut Config) {
    if let Some(provider) = gateway_provider(config) {
        config
            .providers
            .insert(GATEWAY_PROVIDER_ID.into(), provider);
    }
}
/// A gateway provider definition is never user state; remove any stored copy
/// when the gateway owns the ID.
pub(crate) fn strip_gateway_provider(document: &mut toml_edit::DocumentMut) {
    if document.get("gateway").is_none() {
        return;
    }
    if let Some(providers) = document
        .get_mut("providers")
        .and_then(toml_edit::Item::as_table_like_mut)
    {
        providers.remove(GATEWAY_PROVIDER_ID);
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provider {
    pub metadata: Option<String>,
    pub integration: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub endpoints: BTreeMap<String, String>,
    #[serde(default)]
    pub overrides: ProviderOverrides,
    pub auth: Option<Auth>,
    pub subscription: Option<Subscription>,
    #[serde(default)]
    pub models: BTreeMap<String, Model>,
    #[serde(default)]
    pub inputs: BTreeMap<String, InputDeclaration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_policy: Option<FailurePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_guard: Option<QuotaGuard>,
}
impl Default for Provider {
    fn default() -> Self {
        Self {
            metadata: None,
            integration: None,
            enabled: true,
            endpoints: BTreeMap::new(),
            overrides: ProviderOverrides::default(),
            auth: None,
            subscription: None,
            models: BTreeMap::new(),
            inputs: BTreeMap::new(),
            failure_policy: None,
            quota_guard: None,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FailureConditions {
    #[serde(default)]
    pub http_statuses: Vec<u16>,
    #[serde(default)]
    pub connection_errors: bool,
    #[serde(default)]
    pub transport_errors: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FailurePolicy {
    #[serde(default = "yes")]
    pub enabled: bool,
    pub errors: FailureConditions,
    pub consecutive_failures: u32,
    pub priority: i64,
    pub duration_seconds: i64,
}
fn default_quota_guard_age() -> i64 {
    3600
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QuotaGuard {
    #[serde(default = "default_quota_guard_age")]
    pub max_observation_age_seconds: i64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderOverrides {
    pub name: Option<String>,
    pub doc: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Auth {
    ApiKey {
        key_ref: String,
    },
    Login {
        via: String,
        session_ref: Option<String>,
    },
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subscription {
    pub plan: Option<String>,
    pub billing_period: Option<String>,
    pub renewal_date: Option<toml::value::Date>,
    pub expiry_date: Option<toml::value::Date>,
    pub auto_renew: Option<bool>,
    pub charge: Option<Charge>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Charge {
    pub amount: String,
    pub currency: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputDeclaration {
    pub kind: String,
    pub source: String,
    pub scope: String,
    pub models: Vec<String>,
    pub max_age_seconds: i64,
    pub refresh_interval_seconds: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub overrides: ModelOverrides,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub effort_map: BTreeMap<String, BTreeMap<String, String>>,
    pub timezone: Option<String>,
    #[serde(default)]
    pub schedule: Vec<Schedule>,
    #[serde(default)]
    pub priority_rules: Vec<PriorityRule>,
}
impl Default for Model {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: 0,
            overrides: ModelOverrides::default(),
            effort_map: BTreeMap::new(),
            timezone: None,
            schedule: Vec::new(),
            priority_rules: Vec::new(),
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelOverrides {
    pub name: Option<String>,
    pub description: Option<String>,
    pub family: Option<String>,
    pub knowledge: Option<String>,
    pub release_date: Option<String>,
    pub last_updated: Option<String>,
    pub attachment: Option<bool>,
    pub reasoning: Option<bool>,
    pub tool_call: Option<bool>,
    pub structured_output: Option<bool>,
    pub temperature: Option<bool>,
    pub open_weights: Option<bool>,
    pub limit: Option<Limits>,
    pub modalities: Option<Modalities>,
    pub cost: Option<Cost>,
    pub reasoning_options: Option<Vec<ReasoningOption>>,
    pub interleaved: Option<Interleaved>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub context: Option<u64>,
    pub input: Option<u64>,
    pub output: Option<u64>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modalities {
    pub input: Option<Vec<String>>,
    pub output: Option<Vec<String>>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cost {
    pub input: Option<f64>,
    pub output: Option<f64>,
    pub reasoning: Option<f64>,
    pub cache_read: Option<f64>,
    pub cache_write: Option<f64>,
    pub cache_storage: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningOption {
    pub r#type: String,
    pub values: Option<Vec<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Interleaved {
    Enabled(bool),
    Field(InterleavedField),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterleavedField {
    pub field: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeWindow {
    pub weekdays: Option<Vec<String>>,
    pub dates: Option<Vec<toml::value::Date>>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub effective_from: Option<toml::value::Date>,
    pub effective_until: Option<toml::value::Date>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    pub priority: i64,
    #[serde(flatten)]
    pub window: TimeWindow,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriorityRule {
    pub id: String,
    pub input: String,
    pub priority: i64,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub reset_within_seconds: i64,
    pub remaining_at_least_percent: Option<String>,
    pub time_window: Option<TimeWindow>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelAlias {
    pub id: String,
    /// Declared native protocols this route publishes, in canonical order.
    pub protocols: Vec<String>,
    pub candidates: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub candidate_reasoning_effort: BTreeMap<String, String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub continuity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<FailureConditions>,
}
impl ModelAlias {
    /// Whether this route declares a protocol.
    pub fn publishes(&self, protocol: &str) -> bool {
        self.protocols.iter().any(|declared| declared == protocol)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentTarget {
    pub agent: String,
    pub scope: PathBuf,
    #[serde(default)]
    pub providers: BTreeMap<String, NativeProvider>,
    pub default: Option<DefaultReference>,
    #[serde(default)]
    pub roles: BTreeMap<String, ModelReference>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultReference {
    pub provider: String,
    pub id: String,
    pub protocol: String,
    /// Caller-facing effort name for Agents with a validated native selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Verbatim native effort value for Agents without a validated selector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_effort: Option<String>,
}
impl DefaultReference {
    pub(crate) fn model(&self) -> ModelReference {
        ModelReference {
            provider: self.provider.clone(),
            id: self.id.clone(),
            protocol: self.protocol.clone(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProvider {
    /// The configured provider this entry selects; `tmax` names the derived
    /// gateway provider.
    pub provider: String,
    #[serde(default)]
    pub models: Vec<ModelSelection>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelSelection {
    pub id: String,
    pub protocol: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelReference {
    pub provider: String,
    pub id: String,
    pub protocol: String,
}
impl NativeProvider {
    /// Whether this native entry reaches inference through the gateway.
    pub fn is_relayed(&self, config: &Config) -> bool {
        config.is_gateway_provider(&self.provider)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gateway {
    pub listen: String,
    pub key_ref: String,
    #[serde(default = "yes")]
    pub autostart: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub proxy_url: String,
}
pub const PROTOCOLS: [&str; 3] = ["chat-completions", "responses", "messages"];
pub fn qualified(name: &str) -> Result<(&str, &str)> {
    let (p, m) = name.split_once('/').ok_or_else(|| {
        Error::new(
            "invalid_reference",
            "Expected a qualified provider/model reference",
        )
    })?;
    if valid_id(p, false) && !m.is_empty() && !m.chars().any(char::is_control) {
        Ok((p, m))
    } else {
        Err(Error::new(
            "invalid_reference",
            "Invalid qualified model reference",
        ))
    }
}
pub fn valid_id(s: &str, slash: bool) -> bool {
    !s.is_empty()
        && !s
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || (!slash && c == '/'))
}
pub fn model<'a>(config: &'a Config, name: &str) -> Result<(&'a Provider, &'a Model)> {
    let (p, m) = qualified(name)?;
    let p = config.providers.get(p).ok_or_else(|| {
        Error::new(
            "unresolved_reference",
            "Provider reference does not resolve",
        )
    })?;
    let m = p
        .models
        .get(m)
        .ok_or_else(|| Error::new("unresolved_reference", "Model reference does not resolve"))?;
    Ok((p, m))
}

pub struct LoadedFile {
    pub config: Config,
    pub revision: String,
    pub document: toml_edit::DocumentMut,
    pub exists: bool,
}
pub fn revision(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(bytes))
}
pub fn parse(text: &str) -> Result<Config> {
    // Report an unsupported format before typed deserialization: a version-1
    // document fails field-level parsing and would otherwise look corrupt.
    #[derive(Deserialize)]
    struct Header {
        version: i64,
    }
    if let Ok(header) = Header::deserialize(toml::Deserializer::new(text))
        && header.version == LEGACY_CONFIG_VERSION
    {
        return Err(unsupported_legacy_version());
    }
    let de = toml::Deserializer::new(text);
    let mut config: Config = serde_path_to_error::deserialize(de).map_err(|e| {
        Error::new(
            "invalid_configuration",
            "Configuration field has an unknown name, missing value, or invalid type",
        )
        .field(e.path().to_string())
    })?;
    for alias in &mut config.model_aliases {
        alias.protocols.sort();
    }
    apply_gateway_provider(&mut config);
    validate_inner(&config, false)?;
    Ok(config)
}
/// Canonicalize only the two historical model-gateway field names in memory.
/// Network proxies and user-selected map keys are separate namespaces.
pub(crate) fn normalize_gateway_names(document: &mut toml_edit::DocumentMut) -> Result<bool> {
    fn rename(table: &mut dyn toml_edit::TableLike) -> Result<bool> {
        if !table.contains_key("proxy") {
            return Ok(false);
        }
        if table.contains_key("gateway") {
            return Err(invalid("gateway"));
        }
        let value = table.remove("proxy").expect("checked legacy field");
        table.insert("gateway", value);
        Ok(true)
    }
    let mut changed = rename(document.as_table_mut())?;
    if let Some(agents) = document
        .get_mut("agents")
        .and_then(toml_edit::Item::as_table_like_mut)
    {
        for (_, target) in agents.iter_mut() {
            if let Some(providers) = target
                .get_mut("providers")
                .and_then(toml_edit::Item::as_table_like_mut)
            {
                for (_, provider) in providers.iter_mut() {
                    if let Some(entry) = provider.as_table_like_mut() {
                        changed |= rename(entry)?;
                    }
                }
            }
        }
    }
    Ok(changed)
}

pub fn load(store: &crate::storage::Store) -> Result<LoadedFile> {
    let path = store.root.join("tmax.toml");
    let bytes = match std::fs::read(path) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LoadedFile {
                config: Config::empty(),
                revision: "absent".into(),
                document: format!("version = {CONFIG_VERSION}\n")
                    .parse()
                    .expect("constant TOML"),
                exists: false,
            });
        }
        Err(_) => {
            return Err(Error::new(
                "configuration_unavailable",
                "Cannot read the personal configuration",
            ));
        }
    };
    let revision = revision(&bytes);
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error::new("invalid_configuration", "Configuration must be UTF-8"))?;
    let config = parse(text)?;
    let mut document = text.parse().map_err(|_| {
        Error::new(
            "invalid_configuration",
            "Configuration is not valid TOML 1.0",
        )
    })?;
    normalize_gateway_names(&mut document)?;
    Ok(LoadedFile {
        config,
        revision,
        document,
        exists: true,
    })
}
/// A stable revision of one target's desired configuration, independent of file
/// formatting or unrelated targets. The page compares it with the revision the
/// last successful apply captured.
pub fn target_revision(target: &AgentTarget) -> String {
    revision(&serde_json::to_vec(target).unwrap_or_default())
}
pub fn raw_revision(store: &crate::storage::Store) -> Result<String> {
    match std::fs::read(store.root.join("tmax.toml")) {
        Ok(v) => Ok(revision(&v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("absent".into()),
        Err(_) => Err(Error::new(
            "configuration_unavailable",
            "Cannot read configuration revision",
        )),
    }
}
pub fn guard(store: &crate::storage::Store, expected: &str) -> Result<()> {
    if raw_revision(store)? != expected {
        Err(Error::new(
            "inputs_changed",
            "Configuration changed; inspect current inputs and retry the intended operation",
        ))
    } else {
        Ok(())
    }
}
pub fn save(
    store: &crate::storage::Store,
    expected: &str,
    document: &toml_edit::DocumentMut,
) -> Result<String> {
    let mut document = document.clone();
    normalize_gateway_names(&mut document)?;
    strip_gateway_provider(&mut document);
    let text = document.to_string();
    let config = parse(&text)?;
    crate::reasoning::validate_saved(store, &config)?;
    let _lock = store.lock("configuration")?;
    guard(store, expected)?;
    crate::storage::atomic_write(&store.root.join("tmax.toml"), text.as_bytes())?;
    Ok(revision(text.as_bytes()))
}
fn unsupported_legacy_version() -> Error {
    Error::new(
        "invalid_configuration",
        "Configuration format version 1 is unsupported; run `tmax config migrate`",
    )
    .field("version")
}
fn invalid(path: &str) -> Error {
    Error::new(
        "invalid_configuration",
        "Configuration field violates its documented constraints",
    )
    .field(path)
}
pub fn decimal(s: &str) -> bool {
    let mut dots = 0;
    !s.is_empty()
        && s.bytes().all(|b| {
            if b == b'.' {
                dots += 1;
                dots == 1
            } else {
                b.is_ascii_digit()
            }
        })
        && !s.starts_with('.')
        && !s.ends_with('.')
}
fn unique<T: Ord>(values: &[T]) -> bool {
    let set: std::collections::BTreeSet<_> = values.iter().collect();
    set.len() == values.len()
}
fn validate_failure_conditions(conditions: &FailureConditions, path: &str) -> Result<()> {
    if conditions.http_statuses.is_empty()
        && !conditions.connection_errors
        && !conditions.transport_errors
    {
        return Err(invalid(path));
    }
    if !unique(&conditions.http_statuses)
        || conditions
            .http_statuses
            .iter()
            .any(|status| !(400..=599).contains(status))
    {
        return Err(invalid(&format!("{path}.http_statuses")));
    }
    Ok(())
}
pub fn validate(config: &Config) -> Result<()> {
    if config.gateway.is_some() {
        let mut normalized = config.clone();
        apply_gateway_provider(&mut normalized);
        return validate_inner(&normalized, false);
    }
    validate_inner(config, false)
}

// Only historical operational snapshots can retain an unavailable empty membership.
// Desired configuration always requires explicit, nonempty candidates.
pub(crate) fn validate_retained_snapshot(config: &Config) -> Result<()> {
    if config.gateway.is_some() {
        let mut normalized = config.clone();
        apply_gateway_provider(&mut normalized);
        return validate_inner(&normalized, true);
    }
    validate_inner(config, true)
}

fn validate_inner(config: &Config, allow_empty_aliases: bool) -> Result<()> {
    if config.version != CONFIG_VERSION {
        return Err(if config.version == LEGACY_CONFIG_VERSION {
            unsupported_legacy_version()
        } else {
            invalid("version")
        });
    }
    for (id, p) in &config.providers {
        let base = format!("providers.{id}");
        if !valid_id(id, false) {
            return Err(invalid("providers"));
        }
        if let Some(policy) = &p.failure_policy {
            let path = format!("{base}.failure_policy");
            validate_failure_conditions(&policy.errors, &format!("{path}.errors"))?;
            if policy.consecutive_failures == 0 {
                return Err(invalid(&format!("{path}.consecutive_failures")));
            }
            if policy.duration_seconds <= 0
                || chrono::Duration::try_seconds(policy.duration_seconds)
                    .and_then(|duration| chrono::Utc::now().checked_add_signed(duration))
                    .is_none()
            {
                return Err(invalid(&format!("{path}.duration_seconds")));
            }
        }
        if let Some(guard) = &p.quota_guard
            && (guard.max_observation_age_seconds <= 0
                || chrono::Duration::try_seconds(guard.max_observation_age_seconds)
                    .and_then(|duration| chrono::Utc::now().checked_add_signed(duration))
                    .is_none())
        {
            return Err(invalid(&format!(
                "{base}.quota_guard.max_observation_age_seconds"
            )));
        }
        let endpoints = crate::provider_integration::resolve(p).map_err(|error| {
            let field = format!("{base}.{}", error.field.as_deref().unwrap_or("endpoints"));
            Error::new("invalid_configuration", &error.message).field(field)
        })?;
        if let Some(auth) = &p.auth {
            match auth {
                Auth::ApiKey { key_ref } => {
                    if key_ref.is_empty()
                        || crate::provider_integration::integration_id(p)? == "codex-subscription"
                    {
                        return Err(invalid(&format!("{base}.auth")));
                    }
                }
                Auth::Login { via, session_ref } => {
                    let integration = crate::provider_integration::integration_id(p)?;
                    if via == "tmax"
                        || !config.agents.contains_key(via)
                        || !matches!(
                            integration,
                            "codex-subscription" | "kimi-code-plan-cn" | "kimi-code-plan-global"
                        )
                        || session_ref.as_ref().is_some_and(|v| v.is_empty())
                    {
                        return Err(invalid(&format!("{base}.auth.via")));
                    }
                }
            }
        }
        if let Some(s) = &p.subscription
            && let Some(c) = &s.charge
            && (!decimal(&c.amount)
                || c.currency.len() != 3
                || !c.currency.bytes().all(|b| b.is_ascii_uppercase()))
        {
            return Err(invalid(&format!("{base}.subscription.charge")));
        }
        for (name, input) in &p.inputs {
            let path = format!("{base}.inputs.{name}");
            if !valid_id(name, false)
                || !matches!(input.kind.as_str(), "quota" | "reset")
                || !matches!(
                    input.source.as_str(),
                    "manual" | "provider" | "codex-resets"
                )
                || input.scope.is_empty()
                || input.scope.chars().any(char::is_control)
                || input.max_age_seconds <= 0
                || input.refresh_interval_seconds.is_some_and(|i| i <= 0)
                || input.models.is_empty()
                || !unique(&input.models)
                || input.models.iter().any(|m| !p.models.contains_key(m))
                || (input.source == "manual" && input.refresh_interval_seconds.is_some())
                || (input.source == "codex-resets"
                    && (input.kind != "reset" || input.scope != "codex"))
            {
                return Err(invalid(&path));
            }
        }
        for (name, m) in &p.models {
            let path = format!("{base}.models.{name}");
            if name.is_empty() || name.chars().any(char::is_control) {
                return Err(invalid(&path));
            }
            for (protocol, mappings) in &m.effort_map {
                let field = format!("{path}.effort_map.{protocol}");
                if !matches!(protocol.as_str(), "chat-completions" | "responses")
                    || !endpoints.contains_key(protocol)
                {
                    return Err(Error::new(
                        "invalid_configuration",
                        "Model effort maps require a supported Chat Completions or Responses endpoint; Messages is not supported",
                    )
                    .field(field));
                }
                for (input, upstream) in mappings {
                    if [input, upstream].into_iter().any(|effort| {
                        effort.is_empty()
                            || effort.chars().any(|c| c.is_control() || c.is_whitespace())
                    }) {
                        return Err(Error::new(
                            "invalid_configuration",
                            "Effort map input and upstream names must be nonempty without whitespace or control characters",
                        )
                        .field(format!("{field}.{input}")));
                    }
                }
            }
            if let Some(l) = &m.overrides.limit
                && [l.context, l.input, l.output]
                    .into_iter()
                    .flatten()
                    .any(|n| n > i64::MAX as u64)
            {
                return Err(invalid(&format!("{path}.overrides.limit")));
            }
            if let Some(c) = &m.overrides.cost
                && [
                    c.input,
                    c.output,
                    c.reasoning,
                    c.cache_read,
                    c.cache_write,
                    c.cache_storage,
                ]
                .into_iter()
                .flatten()
                .any(|n| !n.is_finite() || n < 0.0)
            {
                return Err(invalid(&format!("{path}.overrides.cost")));
            }
            if let Some(Interleaved::Field(f)) = &m.overrides.interleaved
                && f.field.is_empty()
            {
                return Err(invalid(&format!("{path}.overrides.interleaved.field")));
            }
            crate::routing::validate_model(m, p).map_err(|e| e.field(path))?;
        }
    }
    let mut alias_ids = std::collections::BTreeSet::new();
    for a in &config.model_aliases {
        if !valid_id(&a.id, false)
            || a.protocols.is_empty()
            || !unique(&a.protocols)
            || a.protocols
                .iter()
                .any(|protocol| !PROTOCOLS.contains(&protocol.as_str()))
            || !alias_ids.insert(&a.id)
            || a.continuity
                .as_ref()
                .is_some_and(|s| !matches!(s.as_str(), "session" | "stateless"))
        {
            return Err(invalid(&format!("model_aliases.{}", a.id)));
        }
        if let Some(conditions) = &a.fallback {
            let path = format!("model_aliases.{}.fallback", a.id);
            validate_failure_conditions(conditions, &path)?;
            if a.continuity.as_deref() != Some("stateless") {
                return Err(Error::new(
                    "invalid_configuration",
                    "Alias fallback requires stateless continuity; established sessions cannot migrate",
                )
                .field(path));
            }
        }
        if (!allow_empty_aliases && a.candidates.is_empty()) || !unique(&a.candidates) {
            return Err(invalid("model_aliases.candidates"));
        }
        for name in &a.candidates {
            let (candidate_provider, _) = qualified(name)?;
            if config.is_gateway_provider(candidate_provider) {
                return Err(Error::new(
                    "invalid_configuration",
                    "The gateway provider is not an alias candidate; reference an upstream provider model",
                )
                .field(format!("model_aliases.{}.candidates", a.id)));
            }
            let (provider, _) = model(config, name)?;
            let published = crate::provider_integration::protocols(provider)?;
            for declared in &a.protocols {
                if !published.iter().any(|protocol| protocol == declared) {
                    return Err(Error::new(
                        "invalid_configuration",
                        &format!(
                            "Alias '{}' candidate '{}' does not publish declared protocol '{}'; split the route or drop the protocol",
                            a.id, name, declared
                        ),
                    )
                    .field(format!("model_aliases.{}.candidates", a.id)));
                }
            }
        }
        let protocols = a
            .protocols
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        for (candidate, effort) in &a.candidate_reasoning_effort {
            if !a.candidates.contains(candidate)
                || effort.is_empty()
                || effort.chars().any(|c| c.is_control() || c.is_whitespace())
            {
                return Err(Error::new(
                    "invalid_configuration",
                    &format!(
                        "Alias '{}' candidate '{}' effort {:?} protocols [{}]: candidate must be an exact member of candidates and effort must be nonempty without whitespace or control characters",
                        a.id, candidate, effort, protocols
                    ),
                ).field(format!("model_aliases.{}.candidate_reasoning_effort.{candidate}", a.id)));
            }
            if !a
                .protocols
                .iter()
                .any(|protocol| matches!(protocol.as_str(), "chat-completions" | "responses"))
            {
                return Err(Error::new(
                    "candidate_reasoning_protocol_unsupported",
                    &format!(
                        "Alias '{}' candidate '{}' effort {:?} protocols [{}]: candidate reasoning effort is supported only for Chat Completions and Responses",
                        a.id, candidate, effort, protocols
                    ),
                ).field(format!("model_aliases.{}.candidate_reasoning_effort.{candidate}", a.id)));
            }
        }
        if let Ok((p, _)) = model(config, &a.id)
            && a.protocols.iter().any(|protocol| {
                crate::provider_integration::protocols(p).is_ok_and(|p| p.contains(protocol))
            })
        {
            return Err(invalid("model_aliases.id"));
        }
    }
    for (id, t) in &config.agents {
        let base = format!("agents.{id}");
        if !valid_id(id, true)
            || id == "tmax"
            || !matches!(
                t.agent.as_str(),
                "codex" | "pi" | "omp" | "opencode" | "hermes" | "dsh"
            )
            || t.scope.as_os_str().is_empty()
        {
            return Err(invalid(&base));
        }
        if t.roles.keys().any(|role| {
            !crate::agent_integration::supported_roles(&t.agent).contains(&role.as_str())
        }) {
            return Err(invalid(&format!("{base}.roles")));
        }
        if let Some(default) = &t.default {
            if default.effort.is_some() && default.native_effort.is_some() {
                return Err(Error::new(
                    "invalid_configuration",
                    "A native default declares either a validated effort or a native effort, not both",
                )
                .field(format!("{base}.default")));
            }
            for (field, value) in [
                ("effort", default.effort.as_deref()),
                ("native_effort", default.native_effort.as_deref()),
            ] {
                if value.is_some_and(|value| {
                    value.is_empty()
                        || value
                            .chars()
                            .any(|character| character.is_control() || character.is_whitespace())
                }) {
                    return Err(Error::new(
                        "invalid_configuration",
                        "A native default effort must be nonempty without whitespace or control characters",
                    )
                    .field(format!("{base}.default.{field}")));
                }
            }
            let projected = PROJECTED_EFFORT_AGENTS.contains(&t.agent.as_str());
            if t.agent == "opencode"
                && default
                    .effort
                    .as_deref()
                    .is_some_and(|effort| effort.contains('#'))
            {
                return Err(Error::new(
                    "invalid_configuration",
                    "An OpenCode model reference uses one # separator; the default effort cannot contain #",
                )
                .field(format!("{base}.default.effort")));
            }
            if default.effort.is_some() && !projected {
                return Err(Error::new(
                    "invalid_configuration",
                    "This native Agent has no validated reasoning selector; declare native_effort instead",
                )
                .field(format!("{base}.default.effort")));
            }
            if default.native_effort.is_some() && projected {
                return Err(Error::new(
                    "invalid_configuration",
                    "This native Agent uses caller-facing effort names; declare effort instead of native_effort",
                )
                .field(format!("{base}.default.native_effort")));
            }
        }
        for (name, entry) in &t.providers {
            if !valid_id(name, true) || entry.provider.is_empty() {
                return Err(invalid(&format!("{base}.providers")));
            }
            if entry.provider == GATEWAY_PROVIDER_ID && config.gateway.is_none() {
                return Err(Error::new(
                    "gateway_not_configured",
                    "A relayed native entry selects provider \"tmax\", which exists only while a gateway is configured; run `tmax gateway configure --listen 127.0.0.1:8317` first",
                )
                .field(format!("{base}.providers.{name}")));
            }
            let mut pairs = std::collections::BTreeSet::new();
            for selection in &entry.models {
                if !PROTOCOLS.contains(&selection.protocol.as_str())
                    || !pairs.insert((&selection.id, &selection.protocol))
                {
                    return Err(invalid(&format!("{base}.providers.{name}.models")));
                }
                if !entry.provider.is_empty() {
                    let provider = config
                        .providers
                        .get(&entry.provider)
                        .ok_or_else(|| invalid(&base))?;
                    if !provider.models.contains_key(&selection.id) {
                        return Err(invalid(&base));
                    }
                    if config.is_gateway_provider(&entry.provider) {
                        let route = config
                            .model_aliases
                            .iter()
                            .find(|a| a.id == selection.id && a.publishes(&selection.protocol));
                        let Some(route) = route else {
                            return Err(Error::new(
                                "invalid_configuration",
                                "A relayed native selection protocol must be declared by its route",
                            )
                            .field(format!("{base}.providers.{name}.models")));
                        };
                        if !route.enabled {
                            return Err(Error::new(
                                "invalid_configuration",
                                "A relayed native selection references a disabled route; enable the route or remove the selection",
                            )
                            .field(format!("{base}.providers.{name}.models")));
                        }
                    }
                } else if model(config, &selection.id).is_err()
                    && !config
                        .model_aliases
                        .iter()
                        .any(|a| a.id == selection.id && a.publishes(&selection.protocol))
                {
                    return Err(invalid(&base));
                }
            }
            if !config.providers.contains_key(&entry.provider) {
                return Err(invalid(&base));
            }
        }
        let default = t.default.as_ref().map(DefaultReference::model);
        for reference in default.iter().chain(t.roles.values()) {
            let entry = t
                .providers
                .get(&reference.provider)
                .ok_or_else(|| invalid(&base))?;
            if !entry
                .models
                .iter()
                .any(|m| m.id == reference.id && m.protocol == reference.protocol)
            {
                return Err(invalid(&base));
            }
        }
    }
    if let Some(gateway) = &config.gateway {
        let addr: std::net::SocketAddr = gateway
            .listen
            .parse()
            .map_err(|_| invalid("gateway.listen"))?;
        if !addr.ip().is_loopback() || addr.port() == 0 || gateway.key_ref.is_empty() {
            return Err(invalid("gateway"));
        }
    }
    if let Some(n) = &config.network {
        let url = url::Url::parse(&n.proxy_url).map_err(|_| invalid("network.proxy_url"))?;
        if !matches!(url.scheme(), "http" | "https" | "socks5")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(invalid("network.proxy_url"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_guard_defaults_validates_and_rejects_unknown_fields() {
        let base = "version=2\n[providers.p]\nintegration='deepseek'\n";
        let absent = parse(base).unwrap();
        assert!(absent.providers["p"].quota_guard.is_none());
        assert!(Provider::default().quota_guard.is_none());
        assert!(
            serde_json::to_value(&absent).unwrap()["providers"]["p"]
                .get("quota_guard")
                .is_none()
        );

        let default = parse(&format!("{base}[providers.p.quota_guard]\n")).unwrap();
        assert_eq!(
            default.providers["p"]
                .quota_guard
                .as_ref()
                .unwrap()
                .max_observation_age_seconds,
            3600
        );
        let explicit = parse(&format!(
            "{base}[providers.p.quota_guard]\nmax_observation_age_seconds=120\n"
        ))
        .unwrap();
        assert_eq!(
            explicit.providers["p"]
                .quota_guard
                .as_ref()
                .unwrap()
                .max_observation_age_seconds,
            120
        );
        for age in ["0", "-1", "9223372036854775807", "10000000000000"] {
            let error = parse(&format!(
                "{base}[providers.p.quota_guard]\nmax_observation_age_seconds={age}\n"
            ))
            .unwrap_err();
            assert_eq!(error.code, "invalid_configuration");
            assert_eq!(
                error.field.as_deref(),
                Some("providers.p.quota_guard.max_observation_age_seconds")
            );
        }
        assert!(parse(&format!("{base}[providers.p.quota_guard]\nunknown=1\n")).is_err());
    }

    #[test]
    fn failure_policies_validate_and_reject_invalid_saves_atomically() {
        let base = "version=2\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:39999'\n[providers.p.models.m]\n";
        let alias = "[[model_aliases]]\nid='fallback'\nprotocols=['chat-completions']\ncandidates=['p/m']\n";
        let old = parse(&format!("{base}{alias}")).unwrap();
        assert!(old.providers["p"].failure_policy.is_none());
        assert!(old.model_aliases[0].fallback.is_none());
        let serialized = serde_json::to_value(&old).unwrap();
        assert!(serialized["providers"]["p"].get("failure_policy").is_none());
        assert!(serialized["model_aliases"][0].get("fallback").is_none());

        let policy = "[providers.p.failure_policy]\nconsecutive_failures=3\npriority=-100\nduration_seconds=3600\nerrors={http_statuses=[429,503],connection_errors=true}\n";
        let valid = format!(
            "{base}{policy}{alias}continuity='stateless'\nfallback={{http_statuses=[429],transport_errors=true}}\n"
        );
        let parsed = parse(&valid).unwrap();
        let failure = parsed.providers["p"].failure_policy.as_ref().unwrap();
        assert!(failure.enabled);
        assert_eq!(failure.priority, -100);
        let directory = tempfile::tempdir().unwrap();
        let store = crate::storage::Store::at(directory.path().into());
        store.write_raw("tmax.toml", valid.as_bytes()).unwrap();
        let revision = revision(valid.as_bytes());
        for invalid in [
            valid.replace("consecutive_failures=3", "consecutive_failures=0"),
            valid.replace("consecutive_failures=3", "consecutive_failures=-1"),
            valid.replace("consecutive_failures=3", "consecutive_failures=4294967296"),
            valid.replace("priority=-100\n", ""),
            valid.replace("duration_seconds=3600", "duration_seconds=0"),
            valid.replace(
                "duration_seconds=3600",
                "duration_seconds=9223372036854775807",
            ),
            valid.replace("duration_seconds=3600", "duration_seconds=10000000000000"),
            valid.replace(
                "errors={http_statuses=[429,503],connection_errors=true}",
                "errors={}",
            ),
            valid.replace(
                "errors={http_statuses=[429,503],connection_errors=true}",
                "enabled=false\nerrors={}",
            ),
            valid.replace("[429,503]", "[429,429]"),
            valid.replace("[429,503]", "[399,503]"),
            valid.replace("[429,503]", "[429,600]"),
            valid.replace("continuity='stateless'", "continuity='session'"),
            valid.replace("continuity='stateless'\n", ""),
            valid.replace(
                "fallback={http_statuses=[429],transport_errors=true}",
                "fallback={}",
            ),
            valid.replace("transport_errors=true", "unknown_error=true"),
        ] {
            let document = invalid.parse().unwrap();
            assert_eq!(
                save(&store, &revision, &document).unwrap_err().code,
                "invalid_configuration"
            );
            assert_eq!(
                std::fs::read(store.root.join("tmax.toml")).unwrap(),
                valid.as_bytes()
            );
        }
    }

    #[test]
    fn effort_maps_default_empty_and_validate_protocols_and_names() {
        let base = "version=2\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:39999'\n[providers.p.models.m]\n";
        let config = parse(base).unwrap();
        let model = &config.providers["p"].models["m"];
        assert!(model.effort_map.is_empty());
        assert!(
            serde_json::to_value(model)
                .unwrap()
                .get("effort_map")
                .is_none()
        );
        assert!(!toml::to_string(model).unwrap().contains("effort_map"));
        let mapped = parse(&format!(
            "{base}effort_map={{chat-completions={{xhigh='high'}}}}\n"
        ))
        .unwrap();
        assert_eq!(
            mapped.providers["p"].models["m"].effort_map["chat-completions"]["xhigh"],
            "high"
        );
        for entry in [
            "messages={high='high'}",
            "responses={high='high'}",
            "chat-completions={''='high'}",
            "chat-completions={'two words'='high'}",
            "chat-completions={high=''}",
            "chat-completions={high='two words'}",
        ] {
            let error = parse(&format!("{base}effort_map={{{entry}}}\n")).unwrap_err();
            assert_eq!(error.code, "invalid_configuration");
            assert!(
                error
                    .field
                    .unwrap()
                    .starts_with("providers.p.models.m.effort_map.")
            );
        }
    }

    #[test]
    fn dsh_target_accepts_model_only_incremental_configuration() {
        let config =
            parse("version=2\n[agents.dsh-main]\nagent='dsh'\nscope='/native/dsh-home'\n").unwrap();
        let target = &config.agents["dsh-main"];
        assert_eq!(target.agent, "dsh");
        assert!(target.providers.is_empty());
        assert!(target.default.is_none());
        assert!(target.roles.is_empty());
    }

    #[test]
    fn legacy_gateway_names_read_without_writes_and_save_canonically() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::storage::Store::at(dir.path().into());
        let text = "# personal settings\nversion=2\n[proxy]\nlisten='127.0.0.1:17840'\nkey_ref='retained-proxy-reference' # keep key\n[network]\nproxy_url='http://127.0.0.1:8080'\n[agents.local]\nagent='pi'\nscope='/native'\n";
        store.write_raw("tmax.toml", text.as_bytes()).unwrap();
        let loaded = load(&store).unwrap();
        assert_eq!(
            std::fs::read(store.root.join("tmax.toml")).unwrap(),
            text.as_bytes()
        );
        assert_eq!(
            loaded.config.gateway.as_ref().unwrap().key_ref,
            "retained-proxy-reference"
        );

        let value = serde_json::to_value(&loaded.config).unwrap();
        assert!(value.get("proxy").is_none());
        save(&store, &loaded.revision, &loaded.document).unwrap();
        let saved = std::fs::read_to_string(store.root.join("tmax.toml")).unwrap();
        assert!(saved.contains("[gateway]"));
        assert!(!saved.contains("[proxy]"));
        assert!(saved.contains("# keep key"));
        assert!(saved.contains("# personal settings"));
        assert_eq!(
            parse(&saved).unwrap().network.unwrap().proxy_url,
            "http://127.0.0.1:8080"
        );
        assert_eq!(serde_json::to_value(parse(&saved).unwrap()).unwrap(), value);
    }

    #[test]
    fn conflicting_legacy_and_canonical_gateway_names_are_rejected() {
        let text = "version=2\n[proxy]\nlisten='127.0.0.1:17840'\nkey_ref='a'\n[gateway]\nlisten='127.0.0.1:17841'\nkey_ref='b'\n";
        assert!(parse(text).is_err());
        let mut document = text.parse().unwrap();
        assert!(normalize_gateway_names(&mut document).is_err());
    }

    #[test]
    fn required_version_and_enabled_defaults() {
        assert!(parse("").is_err());
        let c=parse("version=2\n[providers.x]\nintegration='custom'\n[providers.x.endpoints]\nchat-completions='http://127.0.0.1:9999'\n[providers.x.models.a]\n").unwrap();
        assert!(c.providers["x"].enabled);
        assert!(c.providers["x"].models["a"].enabled);
        assert!(Provider::default().enabled);
        assert!(Model::default().enabled);
    }

    #[test]
    fn gateway_autostart_defaults_to_enabled_and_explicit_false_opts_out() {
        let gateway = |text: &str| parse(text).unwrap().gateway.unwrap().autostart;
        assert!(gateway(
            "version=2\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='a'\n"
        ));
        assert!(gateway(
            "version=2\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='a'\nautostart=true\n"
        ));
        assert!(!gateway(
            "version=2\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='a'\nautostart=false\n"
        ));
    }
    #[test]
    fn unknown_nested_fields_are_rejected_without_echo() {
        let e = parse("version=2\nprivate_key='accidental-secret-value'\n")
            .err()
            .unwrap();
        assert!(!e.to_string().contains("accidental-secret-value"));
    }
    #[test]
    fn qualified_split_preserves_upstream_slashes() {
        assert_eq!(
            qualified("local/org/model").unwrap(),
            ("local", "org/model")
        );
        assert!(qualified("bare").is_err());
    }
    #[test]
    fn unknown_schedule_fields_reject() {
        let s = "priority=1\nweekdays=['mon']\nsecret='not-a-field'";
        assert!(toml::from_str::<Schedule>(s).is_err());
    }

    #[test]
    fn removed_network_and_calendar_fields_are_not_desired_configuration_aliases() {
        for text in [
            "version=2\n[network]\nmode='direct'\n",
            "version=2\n[network]\nmode='proxy'\nproxy_url='http://127.0.0.1:8080'\n",
            "version=2\n[network]\n",
            "version=2\n[providers.p]\nintegration='deepseek'\n[providers.p.models.m]\ntimezone='Etc/UTC'\nschedule=[{priority=1,weekdays=['mon'],all_day=true}]\n",
        ] {
            assert_eq!(parse(text).unwrap_err().code, "invalid_configuration");
        }
    }

    #[test]
    fn region_and_unqualified_regional_integrations_are_not_desired_syntax() {
        for text in [
            "version=2\n[providers.p]\nintegration='deepseek'\nregion='default'\n",
            "version=2\n[providers.p]\nintegration='kimi-code-plan-cn'\nregion='china'\n",
            "version=2\n[providers.p]\nintegration='kimi-coding-plan'\n",
            "version=2\n[providers.p]\nintegration='minimax-m-plan'\n",
            "version=2\n[providers.p]\nmetadata='kimi-for-coding'\n",
        ] {
            assert_eq!(parse(text).unwrap_err().code, "invalid_configuration");
        }
    }
    #[test]
    fn aliases_require_nonempty_unique_registered_concrete_candidates() {
        let base = "version=2\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:9999/v1'\n[providers.p.models.a]\n[providers.p.models.b]\n";
        let alias = "[[model_aliases]]\nid='L0'\nprotocols=['chat-completions']\n";
        for candidates in [
            "",
            "candidates=[]\n",
            "candidates=['p/a','p/a']\n",
            "candidates=['p/missing']\n",
            "candidates=['missing/a']\n",
            "candidates=['p']\n",
            "candidates=['L0']\n",
        ] {
            assert!(parse(&format!("{base}{alias}{candidates}")).is_err());
        }
        let config = parse(&format!(
            "{base}{alias}candidates=['p/b','p/a']\n[[model_aliases]]\nid='l0'\nprotocols=['chat-completions']\ncandidates=['p/a']\n[[model_aliases]]\nid='custom.L9'\nprotocols=['chat-completions']\ncandidates=['p/b']\n"
        ))
        .unwrap();
        assert_eq!(config.model_aliases[0].candidates, ["p/b", "p/a"]);
        assert_eq!(config.model_aliases[1].id, "l0");
        assert_eq!(config.model_aliases[2].id, "custom.L9");
    }

    #[test]
    fn removed_product_tier_fields_are_rejected_instead_of_ignored() {
        let provider = "version=2\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:9999/v1'\n[providers.p.models.a]\n";
        for text in [
            format!("{provider}tier='L0'\n"),
            format!(
                "{provider}[[model_aliases]]\nid='L0'\nprotocols=['chat-completions']\ntier='L0'\n"
            ),
            format!(
                "{provider}[[model_aliases]]\nid='L0'\nprotocols=['chat-completions']\ncandidates=['p/a']\ntier='L0'\n"
            ),
        ] {
            assert_eq!(parse(&text).unwrap_err().code, "invalid_configuration");
        }
    }

    #[test]
    fn documented_configuration_example_has_resolvable_explicit_alias_members() {
        let example = parse(include_str!("../../../specs/configuration/example.toml")).unwrap();
        assert_eq!(
            example.providers["opencode-go"]
                .quota_guard
                .as_ref()
                .unwrap()
                .max_observation_age_seconds,
            3600
        );
    }

    #[test]
    fn default_reference_declares_one_optional_effort() {
        let base = "version=2\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:39999'\n[providers.p.models.m]\n[agents.local]\nagent='dsh'\nscope='/native'\n[agents.local.providers.local]\nprovider='p'\n[[agents.local.providers.local.models]]\nid='m'\nprotocol='chat-completions'\n[agents.local.default]\nprovider='local'\nid='m'\nprotocol='chat-completions'\n";
        let with_effort = parse(&format!("{base}effort='max'\n")).unwrap();
        let default = with_effort.agents["local"].default.as_ref().unwrap();
        assert_eq!(default.effort.as_deref(), Some("max"));
        assert!(default.native_effort.is_none());
        assert!(
            toml::to_string(&with_effort)
                .unwrap()
                .contains("effort = \"max\"")
        );

        let codex = || {
            base.replace("agent='dsh'", "agent='codex'")
                .replace(
                    "chat-completions='http://127.0.0.1:39999'",
                    "responses='http://127.0.0.1:39999'",
                )
                .replace("protocol='chat-completions'", "protocol='responses'")
        };
        assert_eq!(
            parse(&format!("{}native_effort='xhigh'\n", codex()))
                .unwrap()
                .agents["local"]
                .default
                .as_ref()
                .unwrap()
                .native_effort
                .as_deref(),
            Some("xhigh")
        );
        for invalid in [
            format!("{base}effort='max'\nnative_effort='max'\n"),
            format!("{base}native_effort='max'\n"),
            format!("{base}effort=' max'\n"),
            format!("{base}effort=''\n"),
            format!("{}effort='max'\n", codex()),
            format!(
                "{}effort='a#b'\n",
                base.replace("agent='dsh'", "agent='opencode'")
            ),
        ] {
            assert_eq!(
                parse(&invalid).unwrap_err().code,
                "invalid_configuration",
                "{invalid}"
            );
        }
    }

    #[test]
    fn configured_gateway_derives_one_reserved_provider() {
        let base = "version=2\n[providers.up]\nintegration='custom'\n[providers.up.endpoints]\nresponses='http://127.0.0.1:9999'\n[providers.up.models.'glm-5.4']\n[providers.up.auth]\nmethod='api-key'\nkey_ref='secret:up'\n";
        let without_gateway = parse(base).unwrap();
        assert!(!without_gateway.providers.contains_key(GATEWAY_PROVIDER_ID));
        let gateway = format!(
            "{base}[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='secret:gw'\n[[model_aliases]]\nid='glm-5.4'\nprotocols=['responses']\ncandidates=['up/glm-5.4']\n"
        );
        let config = parse(&gateway).unwrap();
        assert!(config.is_gateway_provider(GATEWAY_PROVIDER_ID));
        let derived = &config.providers[GATEWAY_PROVIDER_ID];
        assert_eq!(
            derived.integration.as_deref(),
            Some(crate::provider_integration::GATEWAY_INTEGRATION)
        );
        assert_eq!(derived.endpoints["responses"], "http://127.0.0.1:17840");
        assert_eq!(derived.endpoints.len(), 3);
        assert!(matches!(
            derived.auth.as_ref(),
            Some(Auth::ApiKey { key_ref }) if key_ref == "secret:gw"
        ));
        let names: Vec<&String> = derived.models.keys().collect();
        assert_eq!(names, vec!["glm-5.4"]);
        assert!(model(&config, "tmax/glm-5.4").is_ok());
    }

    #[test]
    fn stored_gateway_provider_is_never_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::storage::Store::at(dir.path().into());
        let text = "version=2\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='secret:gw'\n[providers.tmax]\nintegration='custom'\n[providers.tmax.endpoints]\nresponses='http://127.0.0.1:1'\n";
        store.write_raw("tmax.toml", text.as_bytes()).unwrap();
        let loaded = load(&store).expect("a stored reserved definition is replaced, not rejected");
        assert_eq!(
            loaded.config.providers[GATEWAY_PROVIDER_ID]
                .integration
                .as_deref(),
            Some(crate::provider_integration::GATEWAY_INTEGRATION)
        );
        let revision = loaded.revision.clone();
        let saved = save(&store, &revision, &loaded.document).unwrap();
        let written = std::fs::read_to_string(store.root.join("tmax.toml")).unwrap();
        assert!(!written.contains("providers.tmax"), "{written}");
        assert_eq!(crate::config::revision(written.as_bytes()), saved);
    }

    #[test]
    fn gateway_provider_is_never_an_alias_candidate() {
        let text = "version=2\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='secret:gw'\n[providers.up]\nintegration='custom'\n[providers.up.endpoints]\nresponses='http://127.0.0.1:9999'\n[providers.up.models.'glm-5.4']\n[providers.up.auth]\nmethod='api-key'\nkey_ref='secret:up'\n[[model_aliases]]\nid='glm-5.4'\nprotocols=['responses']\ncandidates=['up/glm-5.4']\n[[model_aliases]]\nid='loop'\nprotocols=['responses']\ncandidates=['tmax/glm-5.4']\n";
        let error = parse(text).unwrap_err();
        assert_eq!(error.code, "invalid_configuration");
        assert!(
            error
                .field
                .as_deref()
                .unwrap_or("")
                .contains("model_aliases.loop")
        );
    }

    #[test]
    fn format_version_one_is_rejected_with_the_migration_action() {
        let error = parse("version=1\n").unwrap_err();
        assert_eq!(error.code, "invalid_configuration");
        assert_eq!(error.field.as_deref(), Some("version"));
        assert!(error.message.contains("tmax config migrate"), "{error:?}");
        assert_eq!(
            parse("version=3\n").unwrap_err().code,
            "invalid_configuration"
        );
        let legacy_alias = "version=1\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:39999'\n[providers.p.models.m]\n[[model_aliases]]\nid='route'\nprotocol='chat-completions'\ncandidates=['p/m']\n";
        let error = parse(legacy_alias).unwrap_err();
        assert_eq!(error.field.as_deref(), Some("version"));
        assert!(error.message.contains("tmax config migrate"), "{error:?}");
    }

    #[test]
    fn one_route_declares_all_its_protocols_and_publishes_each_of_them() {
        let provider = "version=2\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:39999'\nresponses='http://127.0.0.1:39999'\n[providers.p.models.m]\n";
        let route = "[[model_aliases]]\nid='route'\nprotocols=['responses','chat-completions']\ncandidates=['p/m']\n";
        let config = parse(&format!("{provider}{route}")).unwrap();
        let alias = &config.model_aliases[0];
        assert_eq!(alias.protocols, ["chat-completions", "responses"]);
        assert!(alias.publishes("responses") && alias.publishes("chat-completions"));
        assert!(!alias.publishes("messages"));
        let value = serde_json::to_value(alias).unwrap();
        assert_eq!(
            value["protocols"],
            serde_json::json!(["chat-completions", "responses"])
        );
        assert!(value.get("protocol").is_none());
    }

    #[test]
    fn route_validation_rejects_bad_protocol_lists_and_duplicate_names() {
        let provider = "version=2\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:39999'\n[providers.p.models.m]\n";
        for protocols in [
            "protocols=[]",
            "protocols=['chat-completions','chat-completions']",
            "protocols=['unified']",
            "",
        ] {
            let text =
                format!("{provider}[[model_aliases]]\nid='r'\n{protocols}\ncandidates=['p/m']\n");
            assert_eq!(
                parse(&text).unwrap_err().code,
                "invalid_configuration",
                "{protocols}"
            );
        }
        let duplicate = format!(
            "{provider}[[model_aliases]]\nid='r'\nprotocols=['chat-completions']\ncandidates=['p/m']\n[[model_aliases]]\nid='r'\nprotocols=['responses']\ncandidates=['p/m']\n"
        );
        assert_eq!(parse(&duplicate).unwrap_err().code, "invalid_configuration");
    }

    #[test]
    fn route_candidate_effort_requires_a_supporting_protocol() {
        let provider = "version=2\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:39999'\nresponses='http://127.0.0.1:39999'\nmessages='http://127.0.0.1:39999'\n[providers.p.models.m]\n[providers.p.models.m.overrides]\nreasoning_options=[{type='effort',values=['high']}]\n";
        let supported = format!(
            "{provider}[[model_aliases]]\nid='r'\nprotocols=['chat-completions','messages']\ncandidates=['p/m']\ncandidate_reasoning_effort={{'p/m'='high'}}\n"
        );
        assert!(parse(&supported).is_ok());
        let unsupported = format!(
            "{provider}[[model_aliases]]\nid='r'\nprotocols=['messages']\ncandidates=['p/m']\ncandidate_reasoning_effort={{'p/m'='high'}}\n"
        );
        let error = parse(&unsupported).unwrap_err();
        assert_eq!(error.code, "candidate_reasoning_protocol_unsupported");
    }

    #[test]
    fn relayed_selection_protocol_must_be_declared_by_its_route() {
        let base = "version=2\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='secret:gw'\n[providers.up]\nintegration='custom'\n[providers.up.endpoints]\nresponses='http://127.0.0.1:39999'\n[providers.up.auth]\nmethod='api-key'\nkey_ref='secret:up'\n[providers.up.models.route]\n[[model_aliases]]\nid='route'\nprotocols=['responses']\ncandidates=['up/route']\n[agents.t]\nagent='omp'\nscope='/native'\n[agents.t.providers.relay]\nprovider='tmax'\n";
        let declared = format!("{base}models=[{{id='route',protocol='responses'}}]\n");
        assert!(parse(&declared).is_ok());
        let undeclared = format!("{base}models=[{{id='route',protocol='messages'}}]\n");
        let error = parse(&undeclared).unwrap_err();
        assert_eq!(error.code, "invalid_configuration");
        assert_eq!(
            error.field.as_deref(),
            Some("agents.t.providers.relay.models")
        );
    }

    #[test]
    fn route_candidates_must_publish_every_declared_protocol() {
        let base = "version=2\n[providers.only-chat]\nintegration='custom'\n[providers.only-chat.endpoints]\nchat-completions='http://127.0.0.1:39991'\n[providers.only-chat.models.m]\n[providers.wide]\nintegration='custom'\n[providers.wide.endpoints]\nchat-completions='http://127.0.0.1:39992'\nresponses='http://127.0.0.1:39992'\n[providers.wide.models.m]\n";
        let covered = format!(
            "{base}[[model_aliases]]\nid='route'\nprotocols=['chat-completions','responses']\ncandidates=['wide/m']\n"
        );
        assert!(parse(&covered).is_ok());
        let uncovered = format!(
            "{base}[[model_aliases]]\nid='route'\nprotocols=['chat-completions','responses']\ncandidates=['only-chat/m']\n"
        );
        let error = parse(&uncovered).unwrap_err();
        assert_eq!(error.code, "invalid_configuration");
        assert_eq!(
            error.field.as_deref(),
            Some("model_aliases.route.candidates")
        );
        assert!(
            error
                .message
                .contains("does not publish declared protocol 'responses'"),
            "{error:?}"
        );
        // A candidate that publishes more protocols than the route declares stays legal.
        let subset = format!(
            "{base}[[model_aliases]]\nid='route'\nprotocols=['chat-completions']\ncandidates=['wide/m']\n"
        );
        assert!(parse(&subset).is_ok());
    }

    #[test]
    fn route_names_cannot_contain_a_slash() {
        let text = "version=2\n[providers.up]\nintegration='custom'\n[providers.up.endpoints]\nresponses='http://127.0.0.1:39999'\n[providers.up.models.route]\n[[model_aliases]]\nid='a/b'\nprotocols=['responses']\ncandidates=['up/route']\n";
        let error = parse(text).unwrap_err();
        assert_eq!(error.code, "invalid_configuration");
        assert_eq!(error.field.as_deref(), Some("model_aliases.a/b"));
    }

    #[test]
    fn relayed_selection_must_reference_an_enabled_route() {
        let base = "version=2\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='secret:gw'\n[providers.up]\nintegration='custom'\n[providers.up.endpoints]\nresponses='http://127.0.0.1:39999'\n[providers.up.auth]\nmethod='api-key'\nkey_ref='secret:up'\n[providers.up.models.route]\n[[model_aliases]]\nid='route'\nprotocols=['responses']\nenabled=false\ncandidates=['up/route']\n[agents.t]\nagent='pi'\nscope='/native/t'\n[agents.t.providers.relay]\nprovider='tmax'\nmodels=[{id='route',protocol='responses'}]\n";
        let error = parse(base).unwrap_err();
        assert_eq!(error.code, "invalid_configuration");
        assert!(error.message.contains("disabled route"), "{error:?}");
        // Enabling the route makes the same selection valid.
        let enabled = base.replace("enabled=false\n", "enabled=true\n");
        assert!(parse(&enabled).is_ok(), "{:?}", parse(&enabled).err());
    }

    #[test]
    fn relayed_entry_without_models_still_requires_a_configured_gateway() {
        // This is the entry shape `agent provider add --provider tmax` creates.
        let text = "version=2\n[agents.t]\nagent='pi'\nscope='/native/t'\n[agents.t.providers.relay]\nprovider='tmax'\n";
        let error = parse(text).unwrap_err();
        assert_eq!(error.code, "gateway_not_configured");
        assert_eq!(error.field.as_deref(), Some("agents.t.providers.relay"));
        // With a gateway configured the same empty entry is valid.
        let with_gateway = "version=2\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='secret:gw'\n[agents.t]\nagent='pi'\nscope='/native/t'\n[agents.t.providers.relay]\nprovider='tmax'\n";
        assert!(parse(with_gateway).is_ok());
    }
}
