use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

fn yes() -> bool {
    true
}
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
            version: 1,
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
            .find(|alias| alias.id == id && alias.protocol == protocol)?
            .candidate_reasoning_effort
            .iter()
            .find(|(name, _)| name.split_once('/') == Some((provider, model)))
            .map(|(_, effort)| effort.as_str())
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
    pub protocol: String,
    pub candidates: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub candidate_reasoning_effort: BTreeMap<String, String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub continuity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<FailureConditions>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentTarget {
    pub agent: String,
    pub scope: PathBuf,
    #[serde(default)]
    pub providers: BTreeMap<String, NativeProvider>,
    pub default: Option<ModelReference>,
    #[serde(default)]
    pub roles: BTreeMap<String, ModelReference>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeProvider {
    pub provider: Option<String>,
    #[serde(alias = "proxy")]
    pub gateway: Option<bool>,
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
    let de = toml::Deserializer::new(text);
    let config: Config = serde_path_to_error::deserialize(de).map_err(|e| {
        Error::new(
            "invalid_configuration",
            "Configuration field has an unknown name, missing value, or invalid type",
        )
        .field(e.path().to_string())
    })?;
    validate(&config)?;
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
                document: "version = 1\n".parse().expect("constant TOML"),
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
    let text = document.to_string();
    let config = parse(&text)?;
    crate::reasoning::validate_saved(store, &config)?;
    let _lock = store.lock("configuration")?;
    guard(store, expected)?;
    crate::storage::atomic_write(&store.root.join("tmax.toml"), text.as_bytes())?;
    Ok(revision(text.as_bytes()))
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
    validate_inner(config, false)
}

// Only historical operational snapshots can retain an unavailable empty membership.
// Desired configuration always requires explicit, nonempty candidates.
pub(crate) fn validate_retained_snapshot(config: &Config) -> Result<()> {
    validate_inner(config, true)
}

fn validate_inner(config: &Config, allow_empty_aliases: bool) -> Result<()> {
    if config.version != 1 {
        return Err(invalid("version"));
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
        if !valid_id(&a.id, true)
            || !PROTOCOLS.contains(&a.protocol.as_str())
            || !alias_ids.insert((&a.protocol, &a.id))
            || a.continuity
                .as_ref()
                .is_some_and(|s| !matches!(s.as_str(), "session" | "stateless"))
        {
            return Err(invalid("model_aliases"));
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
            model(config, name)?;
        }
        for (candidate, effort) in &a.candidate_reasoning_effort {
            if !a.candidates.contains(candidate)
                || effort.is_empty()
                || effort.chars().any(|c| c.is_control() || c.is_whitespace())
            {
                return Err(Error::new(
                    "invalid_configuration",
                    &format!(
                        "Alias '{}' candidate '{}' effort {:?} protocol '{}': candidate must be an exact member of candidates and effort must be nonempty without whitespace or control characters",
                        a.id, candidate, effort, a.protocol
                    ),
                ).field(format!("model_aliases.{}.candidate_reasoning_effort.{candidate}", a.id)));
            }
            if !matches!(a.protocol.as_str(), "chat-completions" | "responses") {
                return Err(Error::new(
                    "candidate_reasoning_protocol_unsupported",
                    &format!(
                        "Alias '{}' candidate '{}' effort {:?} protocol '{}': candidate reasoning effort is supported only for Chat Completions and Responses",
                        a.id, candidate, effort, a.protocol
                    ),
                ).field(format!("model_aliases.{}.candidate_reasoning_effort.{candidate}", a.id)));
            }
        }
        if let Ok((p, _)) = model(config, &a.id)
            && crate::provider_integration::protocols(p)?.contains(&a.protocol)
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
        for (name, entry) in &t.providers {
            if !valid_id(name, true)
                || (entry.provider.is_some() == entry.gateway.is_some())
                || entry.gateway == Some(false)
            {
                return Err(invalid(&format!("{base}.providers")));
            }
            if entry.gateway == Some(true) && config.gateway.is_none() {
                return Err(invalid(&format!("{base}.providers.{name}.gateway")));
            }
            let mut pairs = std::collections::BTreeSet::new();
            for selection in &entry.models {
                if !PROTOCOLS.contains(&selection.protocol.as_str())
                    || !pairs.insert((&selection.id, &selection.protocol))
                {
                    return Err(invalid(&format!("{base}.providers.{name}.models")));
                }
                if let Some(p) = &entry.provider {
                    let provider = config.providers.get(p).ok_or_else(|| invalid(&base))?;
                    if !provider.models.contains_key(&selection.id) {
                        return Err(invalid(&base));
                    }
                } else if model(config, &selection.id).is_err()
                    && !config
                        .model_aliases
                        .iter()
                        .any(|a| a.id == selection.id && a.protocol == selection.protocol)
                {
                    return Err(invalid(&base));
                }
            }
            if let Some(p) = &entry.provider
                && !config.providers.contains_key(p)
            {
                return Err(invalid(&base));
            }
        }
        for reference in t.default.iter().chain(t.roles.values()) {
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
        let base = "version=1\n[providers.p]\nintegration='deepseek'\n";
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
        let base = "version=1\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:39999'\n[providers.p.models.m]\n";
        let alias =
            "[[model_aliases]]\nid='fallback'\nprotocol='chat-completions'\ncandidates=['p/m']\n";
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
        let base = "version=1\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:39999'\n[providers.p.models.m]\n";
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
            parse("version=1\n[agents.dsh-main]\nagent='dsh'\nscope='/native/dsh-home'\n").unwrap();
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
        let text = "# personal settings\nversion=1\n[proxy]\nlisten='127.0.0.1:17840'\nkey_ref='retained-proxy-reference' # keep key\n[network]\nproxy_url='http://127.0.0.1:8080'\n[agents.local]\nagent='pi'\nscope='/native'\n[agents.local.providers.proxy]\nproxy=true # keep source\n";
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
        assert_eq!(
            loaded.config.agents["local"].providers["proxy"].gateway,
            Some(true)
        );
        let value = serde_json::to_value(&loaded.config).unwrap();
        assert!(value.get("proxy").is_none());
        assert_eq!(
            value["agents"]["local"]["providers"]["proxy"]["gateway"],
            true
        );
        save(&store, &loaded.revision, &loaded.document).unwrap();
        let saved = std::fs::read_to_string(store.root.join("tmax.toml")).unwrap();
        assert!(saved.contains("[gateway]"));
        assert!(!saved.contains("[proxy]"));
        assert!(saved.contains("# keep key"));
        assert!(saved.contains("# keep source"));
        assert!(saved.contains("# personal settings"));
        assert!(saved.contains("[agents.local.providers.proxy]"));
        assert_eq!(
            parse(&saved).unwrap().network.unwrap().proxy_url,
            "http://127.0.0.1:8080"
        );
        assert_eq!(serde_json::to_value(parse(&saved).unwrap()).unwrap(), value);
    }

    #[test]
    fn conflicting_legacy_and_canonical_gateway_names_are_rejected() {
        for text in [
            "version=1\n[proxy]\nlisten='127.0.0.1:17840'\nkey_ref='a'\n[gateway]\nlisten='127.0.0.1:17841'\nkey_ref='b'\n",
            "version=1\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='a'\n[agents.local]\nagent='pi'\nscope='/native'\n[agents.local.providers.local]\nproxy=true\ngateway=true\n",
        ] {
            assert!(parse(text).is_err());
            let mut document = text.parse().unwrap();
            assert!(normalize_gateway_names(&mut document).is_err());
        }
    }

    #[test]
    fn required_version_and_enabled_defaults() {
        assert!(parse("").is_err());
        let c=parse("version=1\n[providers.x]\nintegration='custom'\n[providers.x.endpoints]\nchat-completions='http://127.0.0.1:9999'\n[providers.x.models.a]\n").unwrap();
        assert!(c.providers["x"].enabled);
        assert!(c.providers["x"].models["a"].enabled);
        assert!(Provider::default().enabled);
        assert!(Model::default().enabled);
    }

    #[test]
    fn gateway_autostart_defaults_to_enabled_and_explicit_false_opts_out() {
        let gateway = |text: &str| parse(text).unwrap().gateway.unwrap().autostart;
        assert!(gateway(
            "version=1\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='a'\n"
        ));
        assert!(gateway(
            "version=1\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='a'\nautostart=true\n"
        ));
        assert!(!gateway(
            "version=1\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='a'\nautostart=false\n"
        ));
    }
    #[test]
    fn unknown_nested_fields_are_rejected_without_echo() {
        let e = parse("version=1\nprivate_key='accidental-secret-value'\n")
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
            "version=1\n[network]\nmode='direct'\n",
            "version=1\n[network]\nmode='proxy'\nproxy_url='http://127.0.0.1:8080'\n",
            "version=1\n[network]\n",
            "version=1\n[providers.p]\nintegration='deepseek'\n[providers.p.models.m]\ntimezone='Etc/UTC'\nschedule=[{priority=1,weekdays=['mon'],all_day=true}]\n",
        ] {
            assert_eq!(parse(text).unwrap_err().code, "invalid_configuration");
        }
    }

    #[test]
    fn region_and_unqualified_regional_integrations_are_not_desired_syntax() {
        for text in [
            "version=1\n[providers.p]\nintegration='deepseek'\nregion='default'\n",
            "version=1\n[providers.p]\nintegration='kimi-code-plan-cn'\nregion='china'\n",
            "version=1\n[providers.p]\nintegration='kimi-coding-plan'\n",
            "version=1\n[providers.p]\nintegration='minimax-m-plan'\n",
            "version=1\n[providers.p]\nmetadata='kimi-for-coding'\n",
        ] {
            assert_eq!(parse(text).unwrap_err().code, "invalid_configuration");
        }
    }
    #[test]
    fn aliases_require_nonempty_unique_registered_concrete_candidates() {
        let base = "version=1\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:9999/v1'\n[providers.p.models.a]\n[providers.p.models.b]\n";
        let alias = "[[model_aliases]]\nid='L0'\nprotocol='chat-completions'\n";
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
            "{base}{alias}candidates=['p/b','p/a']\n[[model_aliases]]\nid='l0'\nprotocol='chat-completions'\ncandidates=['p/a']\n[[model_aliases]]\nid='custom.L9'\nprotocol='chat-completions'\ncandidates=['p/b']\n"
        ))
        .unwrap();
        assert_eq!(config.model_aliases[0].candidates, ["p/b", "p/a"]);
        assert_eq!(config.model_aliases[1].id, "l0");
        assert_eq!(config.model_aliases[2].id, "custom.L9");
    }

    #[test]
    fn removed_product_tier_fields_are_rejected_instead_of_ignored() {
        let provider = "version=1\n[providers.p]\nintegration='custom'\n[providers.p.endpoints]\nchat-completions='http://127.0.0.1:9999/v1'\n[providers.p.models.a]\n";
        for text in [
            format!("{provider}tier='L0'\n"),
            format!(
                "{provider}[[model_aliases]]\nid='L0'\nprotocol='chat-completions'\ntier='L0'\n"
            ),
            format!(
                "{provider}[[model_aliases]]\nid='L0'\nprotocol='chat-completions'\ncandidates=['p/a']\ntier='L0'\n"
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
}
