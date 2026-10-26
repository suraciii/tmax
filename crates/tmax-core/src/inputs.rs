use crate::{
    Error, Request, Result,
    config::{self, Config, InputDeclaration, Provider},
    storage::Store,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{LazyLock, Mutex},
    time::Instant,
};

const STATE: &str = "routing-state.json";
fn token() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub(crate) fn signature<T: Serialize>(value: &T) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("serializable domain value"))
    )
}
pub(crate) fn instant(text: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|_| {
            Error::new(
                "invalid_instant",
                "Expected an explicit-offset RFC3339 instant",
            )
        })
}
pub(crate) fn timestamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// RFC3339 fractions have no precision cap. Chrono validates calendars/offsets;
/// this decimal tail prevents silently rounding a publication or rule boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExactInstant {
    seconds: i64,
    leap: bool,
    fraction: String,
}
impl ExactInstant {
    pub(crate) fn parse(text: &str) -> Result<Self> {
        let at = instant(text)?;
        let fraction = if let Some(dot) = text.find('.') {
            let tail = &text[dot + 1..];
            let end = tail
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(tail.len());
            if end == 0 {
                return Err(Error::new("invalid_instant", "Empty fractional second"));
            }
            tail[..end].trim_end_matches('0').to_owned()
        } else {
            String::new()
        };
        Ok(Self {
            seconds: at.timestamp(),
            leap: at.timestamp_subsec_nanos() >= 1_000_000_000,
            fraction,
        })
    }
    pub(crate) fn from_chrono(at: DateTime<Utc>) -> Self {
        Self::parse(&timestamp(at)).expect("UTC timestamp")
    }
    pub(crate) fn canonical(&self) -> String {
        let whole = DateTime::<Utc>::from_timestamp(
            self.seconds,
            if self.leap { 1_000_000_000 } else { 0 },
        )
        .expect("validated timestamp")
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
        if self.fraction.is_empty() {
            format!("{whole}Z")
        } else {
            format!("{whole}.{}Z", self.fraction)
        }
    }
    pub(crate) fn add_seconds(&self, seconds: i64) -> Option<Self> {
        let at = DateTime::<Utc>::from_timestamp(
            self.seconds,
            if self.leap { 1_000_000_000 } else { 0 },
        )?;
        let target = at.checked_add_signed(Duration::try_seconds(seconds)?)?;
        Some(Self {
            seconds: target.timestamp(),
            leap: target.timestamp_subsec_nanos() >= 1_000_000_000,
            fraction: self.fraction.clone(),
        })
    }
}
impl Ord for ExactInstant {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.seconds
            .cmp(&other.seconds)
            .then_with(|| self.leap.cmp(&other.leap))
            .then_with(|| {
                let width = self.fraction.len().max(other.fraction.len());
                self.fraction
                    .bytes()
                    .chain(std::iter::repeat(b'0'))
                    .take(width)
                    .cmp(
                        other
                            .fraction
                            .bytes()
                            .chain(std::iter::repeat(b'0'))
                            .take(width),
                    )
            })
    }
}
impl PartialOrd for ExactInstant {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
pub(crate) fn within_horizon(reset: &str, at: &ExactInstant, seconds: i64) -> Result<bool> {
    let reset = ExactInstant::parse(reset)?;
    Ok(&reset > at && at.add_seconds(seconds).is_none_or(|end| reset <= end))
}
fn after_seconds(at: DateTime<Utc>, seconds: i64) -> DateTime<Utc> {
    Duration::try_seconds(seconds)
        .and_then(|duration| at.checked_add_signed(duration))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

/// Arbitrary-precision base-ten quantity. No division or binary floating point is used.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Decimal {
    coefficient: BigUint,
    scale: usize,
    text: String,
}
impl Decimal {
    pub(crate) fn parse(text: &str) -> Result<Self> {
        let mut pieces = text.split('.');
        let integer = pieces.next().unwrap_or("");
        let fraction = pieces.next();
        if integer.is_empty()
            || !integer.bytes().all(|c| c.is_ascii_digit())
            || fraction.is_some_and(|s| s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()))
            || pieces.next().is_some()
        {
            return Err(Error::new(
                "invalid_decimal",
                "Expected an unsigned finite base-ten decimal string",
            ));
        }
        let integer = integer.trim_start_matches('0');
        let integer = if integer.is_empty() { "0" } else { integer };
        let fraction = fraction.unwrap_or("").trim_end_matches('0');
        let canonical = if fraction.is_empty() {
            integer.to_owned()
        } else {
            format!("{integer}.{fraction}")
        };
        let coefficient = BigUint::parse_bytes(format!("{integer}{fraction}").as_bytes(), 10)
            .ok_or_else(|| Error::new("invalid_decimal", "Invalid decimal quantity"))?;
        Ok(Self {
            coefficient,
            scale: fraction.len(),
            text: canonical,
        })
    }
    fn ten_pow(mut n: usize) -> BigUint {
        let mut result = BigUint::from(1u8);
        let mut base = BigUint::from(10u8);
        while n != 0 {
            if n & 1 != 0 {
                result *= &base;
            }
            n >>= 1;
            if n != 0 {
                base = &base * &base;
            }
        }
        result
    }
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let scale = self.scale.max(other.scale);
        (&self.coefficient * Self::ten_pow(scale - self.scale))
            .cmp(&(&other.coefficient * Self::ten_pow(scale - other.scale)))
    }
    fn product(&self, other: &Self) -> Self {
        Self {
            coefficient: &self.coefficient * &other.coefficient,
            scale: self.scale + other.scale,
            text: String::new(),
        }
    }
    pub(crate) fn percent(text: &str) -> Result<Self> {
        let result = Self::parse(text)?;
        if result.cmp(&Self::parse("100")?) == std::cmp::Ordering::Greater {
            return Err(Error::new(
                "invalid_decimal",
                "Percentage must be between 0 and 100",
            ));
        }
        Ok(result)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputValue {
    pub observed_at: String,
    pub observed_at_basis: String,
    pub received_at: String,
    pub valid_until: String,
    pub reset_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_percent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    pub provider: String,
    pub input: String,
    pub provider_identity: String,
    pub authorization_generation: String,
    pub declaration_revision: String,
    pub kind: String,
    pub scope: String,
    pub models: Vec<String>,
    pub source: String,
}
fn required_option<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "HeadRecord")]
pub struct InputHead {
    #[serde(flatten)]
    pub(crate) binding: Binding,
    pub revision: String,
    pub observation_floor: Option<String>,
    pub value: Option<InputValue>,
    pub unavailable_reason: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "AttemptRecord")]
pub struct AcquisitionAttempt {
    #[serde(flatten)]
    pub(crate) binding: Binding,
    pub revision: String,
    pub started_at: String,
    pub finished_at: String,
    pub trigger: String,
    pub outcome: String,
    pub reason_code: Option<String>,
    pub head_revision_before: String,
    pub head_revision_after: String,
}

// Strict wire structs avoid serde's unsupported deny_unknown_fields + flatten
// combination. Conversion moves fields into the shared operational binding.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeadRecord {
    provider: String,
    input: String,
    provider_identity: String,
    authorization_generation: String,
    declaration_revision: String,
    kind: String,
    scope: String,
    models: Vec<String>,
    source: String,
    revision: String,
    #[serde(deserialize_with = "required_option")]
    observation_floor: Option<String>,
    #[serde(deserialize_with = "required_option")]
    value: Option<InputValue>,
    #[serde(deserialize_with = "required_option")]
    unavailable_reason: Option<String>,
}
impl From<HeadRecord> for InputHead {
    fn from(record: HeadRecord) -> Self {
        Self {
            binding: Binding {
                provider: record.provider,
                input: record.input,
                provider_identity: record.provider_identity,
                authorization_generation: record.authorization_generation,
                declaration_revision: record.declaration_revision,
                kind: record.kind,
                scope: record.scope,
                models: record.models,
                source: record.source,
            },
            revision: record.revision,
            observation_floor: record.observation_floor,
            value: record.value,
            unavailable_reason: record.unavailable_reason,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttemptRecord {
    provider: String,
    input: String,
    provider_identity: String,
    authorization_generation: String,
    declaration_revision: String,
    kind: String,
    scope: String,
    models: Vec<String>,
    source: String,
    revision: String,
    started_at: String,
    finished_at: String,
    trigger: String,
    outcome: String,
    #[serde(deserialize_with = "required_option")]
    reason_code: Option<String>,
    head_revision_before: String,
    head_revision_after: String,
}
impl From<AttemptRecord> for AcquisitionAttempt {
    fn from(record: AttemptRecord) -> Self {
        Self {
            binding: Binding {
                provider: record.provider,
                input: record.input,
                provider_identity: record.provider_identity,
                authorization_generation: record.authorization_generation,
                declaration_revision: record.declaration_revision,
                kind: record.kind,
                scope: record.scope,
                models: record.models,
                source: record.source,
            },
            revision: record.revision,
            started_at: record.started_at,
            finished_at: record.finished_at,
            trigger: record.trigger,
            outcome: record.outcome,
            reason_code: record.reason_code,
            head_revision_before: record.head_revision_before,
            head_revision_after: record.head_revision_after,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Channel {
    head: Option<InputHead>,
    last_attempt: Option<AcquisitionAttempt>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderIdentity {
    pub identity: String,
    pub authorization: String,
    auth_definition: Value,
    pub models: BTreeMap<String, String>,
    declarations: BTreeMap<String, DeclarationIdentity>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclarationIdentity {
    revision: String,
    definition: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct View {
    pub config: Config,
    pub providers: BTreeMap<String, ProviderIdentity>,
    #[serde(default)]
    revision: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Runtime {
    pub saved: View,
    pub loaded: Option<View>,
    channels: BTreeMap<String, Channel>,
    authorization: BTreeMap<String, String>,
}

fn source_definition(provider: &Provider, declaration: &InputDeclaration) -> String {
    let mut models = declaration.models.clone();
    models.sort();
    signature(
        &json!({"kind":declaration.kind,"source":declaration.source,"scope":declaration.scope,"models":models,
        "max_age_seconds":declaration.max_age_seconds,"integration":crate::provider_integration::integration_id(provider).ok(),
        "endpoints":crate::provider_integration::resolve(provider).ok()}),
    )
}
fn auth_definition(config: &Config, provider: &Provider) -> Value {
    let source = match &provider.auth {
        Some(config::Auth::Login { via, .. }) if via != "tmax" => config
            .agents
            .get(via)
            .map(|t| json!({"agent":t.agent,"scope":t.scope})),
        _ => None,
    };
    json!({"auth":provider.auth,"source":source})
}

// Read-only projections reuse unchanged bindings without reserving identities.
fn project_view(old: &View, config: &Config) -> View {
    let mut providers = BTreeMap::new();
    for (name, p) in &config.providers {
        let mut ids = old.providers.get(name).cloned().unwrap_or_default();
        if ids.auth_definition != auth_definition(config, p) {
            ids.authorization.clear();
        }
        ids.models.retain(|m, _| p.models.contains_key(m));
        ids.declarations
            .retain(|input, _| p.inputs.contains_key(input));
        for (input, d) in &p.inputs {
            let definition = source_definition(p, d);
            if ids
                .declarations
                .get(input)
                .is_none_or(|prior| prior.definition != definition)
            {
                ids.declarations.insert(
                    input.clone(),
                    DeclarationIdentity {
                        revision: String::new(),
                        definition,
                    },
                );
            }
        }
        providers.insert(name.clone(), ids);
    }
    View {
        config: config.clone(),
        providers,
        revision: if signature(&old.config) == signature(config) {
            old.revision.clone()
        } else {
            None
        },
    }
}
fn make_view(
    runtime: &mut Runtime,
    old: &View,
    config: &Config,
    renames: &[(String, String)],
) -> View {
    let mut previous = old.providers.clone();
    for (from, to) in renames {
        if !from.contains('/') && !to.contains('/') {
            if let Some(identity) = previous.remove(from) {
                previous.insert(to.clone(), identity);
            }
        } else if let (Ok((p, m)), Ok((np, nm))) = (config::qualified(from), config::qualified(to))
            && p == np
            && let Some(identity) = previous.get_mut(p)
            && let Some(id) = identity.models.remove(m)
        {
            identity.models.insert(nm.into(), id);
        }
    }
    let mut providers = BTreeMap::new();
    for (name, provider) in &config.providers {
        let mut ids = previous.remove(name).unwrap_or_else(|| ProviderIdentity {
            identity: token(),
            authorization: token(),
            ..Default::default()
        });
        if let Some(current) = runtime.authorization.get(&ids.identity) {
            ids.authorization = current.clone();
        }
        let auth = auth_definition(config, provider);
        if ids.auth_definition != auth {
            ids.authorization = token();
            ids.auth_definition = auth;
        }
        runtime
            .authorization
            .insert(ids.identity.clone(), ids.authorization.clone());
        ids.models
            .retain(|name, _| provider.models.contains_key(name));
        for model in provider.models.keys() {
            ids.models.entry(model.clone()).or_insert_with(token);
        }
        ids.declarations
            .retain(|name, _| provider.inputs.contains_key(name));
        for (input, declaration) in &provider.inputs {
            let definition = source_definition(provider, declaration);
            if ids
                .declarations
                .get(input)
                .is_none_or(|old| old.definition != definition)
            {
                ids.declarations.insert(
                    input.clone(),
                    DeclarationIdentity {
                        revision: token(),
                        definition,
                    },
                );
            }
        }
        providers.insert(name.clone(), ids);
    }
    View {
        config: config.clone(),
        providers,
        revision: None,
    }
}
fn key(binding: &Binding) -> String {
    format!(
        "{}/{}/{}",
        binding.provider_identity, binding.authorization_generation, binding.declaration_revision
    )
}
fn binding(view: &View, runtime: &Runtime, provider: &str, input: &str) -> Option<Binding> {
    let p = view.config.providers.get(provider)?;
    let d = p.inputs.get(input)?;
    let ids = view.providers.get(provider)?;
    let mut models = d.models.clone();
    models.sort();
    Some(Binding {
        provider: provider.into(),
        input: input.into(),
        provider_identity: ids.identity.clone(),
        authorization_generation: if ids.authorization.is_empty() {
            String::new()
        } else {
            runtime
                .authorization
                .get(&ids.identity)
                .unwrap_or(&ids.authorization)
                .clone()
        },
        declaration_revision: ids.declarations.get(input)?.revision.clone(),
        kind: d.kind.clone(),
        scope: d.scope.clone(),
        models,
        source: d.source.clone(),
    })
}

/// Resolve one retained view without acquiring another publication lock.
pub(crate) fn provider_scope_binding_in_view(
    runtime: &Runtime,
    view: &View,
    provider: &str,
) -> Option<(String, String)> {
    let ids = view.providers.get(provider)?;
    if ids.identity.is_empty() || ids.authorization.is_empty() {
        return None;
    }
    let authorization = runtime
        .authorization
        .get(&ids.identity)
        .unwrap_or(&ids.authorization);
    if authorization != &ids.authorization {
        return None;
    }
    Some((ids.identity.clone(), authorization.clone()))
}

/// Resolve saved intent or its read-only projection in an already-locked runtime.
pub(crate) fn provider_scope_binding(
    runtime: &Runtime,
    config: &Config,
    provider: &str,
) -> Option<(String, String)> {
    let hash = signature(config);
    if signature(&runtime.saved.config) == hash {
        provider_scope_binding_in_view(runtime, &runtime.saved, provider)
    } else if let Some(view) = runtime
        .loaded
        .as_ref()
        .filter(|view| signature(&view.config) == hash)
    {
        provider_scope_binding_in_view(runtime, view, provider)
    } else {
        let declaration = config.providers.get(provider)?;
        let ids = runtime.saved.providers.get(provider)?;
        if ids.auth_definition != auth_definition(config, declaration) {
            return None;
        }
        provider_scope_binding_in_view(runtime, &runtime.saved, provider)
    }
}
fn retain(runtime: &mut Runtime) {
    let mut keys = BTreeSet::new();
    let mut providers = BTreeSet::new();
    for view in std::iter::once(&runtime.saved).chain(runtime.loaded.iter()) {
        for (p, provider) in &view.config.providers {
            if let Some(ids) = view.providers.get(p) {
                providers.insert(ids.identity.clone());
            }
            for name in provider.inputs.keys() {
                if let Some(b) = binding(view, runtime, p, name) {
                    keys.insert(key(&b));
                }
            }
        }
    }
    runtime.channels.retain(|k, _| keys.contains(k));
    runtime.authorization.retain(|p, _| providers.contains(p));
}
fn state_schema_error() -> Error {
    Error::new(
        "invalid_record",
        "Retained input state fails its storage schema",
    )
}

// Stored-data cutover only: never accept these fields in desired configuration.
// Membership comes exclusively from this historical snapshot, including disabled
// models and empty memberships. Reads do not rewrite the retained file.
fn normalize_historical_snapshot(snapshot: &mut Value) -> Result<bool> {
    let snapshot = snapshot.as_object_mut().ok_or_else(state_schema_error)?;
    let mut membership: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut changed = false;
    if let Some(providers) = snapshot.get_mut("providers") {
        for (provider_id, provider) in providers.as_object_mut().ok_or_else(state_schema_error)? {
            let provider = provider.as_object_mut().ok_or_else(state_schema_error)?;
            if let Some(models) = provider.get_mut("models") {
                for (model_id, model) in models.as_object_mut().ok_or_else(state_schema_error)? {
                    let model = model.as_object_mut().ok_or_else(state_schema_error)?;
                    if let Some(tier) = model.remove("tier") {
                        changed = true;
                        match tier {
                            Value::Null => {}
                            Value::String(tier)
                                if matches!(tier.as_str(), "L0" | "L1" | "L2" | "L3") =>
                            {
                                membership
                                    .entry(tier)
                                    .or_default()
                                    .push(format!("{provider_id}/{model_id}"));
                            }
                            _ => return Err(state_schema_error()),
                        }
                    }
                }
            }
        }
    }
    if let Some(aliases) = snapshot.get_mut("model_aliases") {
        for alias in aliases.as_array_mut().ok_or_else(state_schema_error)? {
            let alias = alias.as_object_mut().ok_or_else(state_schema_error)?;
            let Some(tier) = alias.remove("tier") else {
                continue;
            };
            changed = true;
            match tier {
                Value::Null => {
                    if alias
                        .get("candidates")
                        .and_then(Value::as_array)
                        .is_none_or(Vec::is_empty)
                    {
                        return Err(state_schema_error());
                    }
                }
                Value::String(tier) if matches!(tier.as_str(), "L0" | "L1" | "L2" | "L3") => {
                    if alias
                        .get("candidates")
                        .is_some_and(|candidates| !candidates.is_null())
                    {
                        return Err(state_schema_error());
                    }
                    let candidates = membership
                        .get(&tier)
                        .into_iter()
                        .flatten()
                        .cloned()
                        .map(Value::String)
                        .collect();
                    alias.insert("candidates".into(), Value::Array(candidates));
                }
                _ => return Err(state_schema_error()),
            }
        }
    }
    Ok(changed)
}

fn normalize_historical_view(view: &mut Value) -> Result<bool> {
    let view = view.as_object_mut().ok_or_else(state_schema_error)?;
    let mut changed =
        normalize_historical_snapshot(view.get_mut("config").ok_or_else(state_schema_error)?)?;
    if let Some(providers) = view.get_mut("providers") {
        for provider in providers
            .as_object_mut()
            .ok_or_else(state_schema_error)?
            .values_mut()
        {
            let provider = provider.as_object_mut().ok_or_else(state_schema_error)?;
            if let Some(models) = provider.remove("offerings") {
                if provider.contains_key("models") {
                    return Err(state_schema_error());
                }
                provider.insert("models".into(), models);
                changed = true;
            }
        }
    }
    Ok(changed)
}

// Keep serde's duplicate-struct-field rejection when the historical fallback
// materializes JSON values. This validation leaves arbitrary-precision number
// parsing to serde_json and uses its normal recursion bound.
pub(crate) fn check_historical_field_uniqueness(bytes: &[u8]) -> Result<()> {
    struct Fields;
    impl<'de> serde::de::DeserializeSeed<'de> for Fields {
        type Value = ();
        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> std::result::Result<(), D::Error> {
            deserializer.deserialize_any(self)
        }
    }
    impl<'de> serde::de::Visitor<'de> for Fields {
        type Value = ();
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("retained JSON with unique object fields")
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_str<E: serde::de::Error>(self, _: &str) -> std::result::Result<(), E> {
            Ok(())
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> std::result::Result<(), A::Error> {
            while sequence.next_element_seed(Fields)?.is_some() {}
            Ok(())
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<(), A::Error> {
            let mut fields = BTreeSet::new();
            while let Some(field) = map.next_key::<String>()? {
                if !fields.insert(field) {
                    return Err(serde::de::Error::custom("Duplicate retained state field"));
                }
                map.next_value_seed(Fields)?;
            }
            Ok(())
        }
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    serde::de::DeserializeSeed::deserialize(Fields, &mut deserializer)
        .map_err(|_| state_schema_error())?;
    deserializer.end().map_err(|_| state_schema_error())
}

fn decode_runtime(bytes: &[u8]) -> Result<Runtime> {
    if let Ok(runtime) = serde_json::from_slice(bytes) {
        return Ok(runtime);
    }
    check_historical_field_uniqueness(bytes)?;
    let mut document: Value = serde_json::from_slice(bytes).map_err(|_| state_schema_error())?;
    let saved =
        normalize_historical_view(document.get_mut("saved").ok_or_else(state_schema_error)?)?;
    let loaded = match document.get_mut("loaded") {
        None | Some(Value::Null) => false,
        Some(view) => normalize_historical_view(view)?,
    };
    if !saved && !loaded {
        return Err(state_schema_error());
    }
    let runtime: Runtime = serde_json::from_value(document).map_err(|_| state_schema_error())?;
    if saved {
        config::validate_retained_snapshot(&runtime.saved.config)
            .map_err(|_| state_schema_error())?;
    }
    if loaded {
        config::validate_retained_snapshot(
            &runtime
                .loaded
                .as_ref()
                .ok_or_else(state_schema_error)?
                .config,
        )
        .map_err(|_| state_schema_error())?;
    }
    Ok(runtime)
}

pub(crate) fn read_runtime(store: &Store) -> Result<Runtime> {
    match crate::storage::read_optional(&store.path(STATE)?)? {
        None => Ok(Runtime::default()),
        Some(bytes) => decode_runtime(&bytes),
    }
}
fn write_runtime(store: &Store, state: &Runtime) -> Result<()> {
    store.write_json(STATE, state)
}

fn invalidate_changed(runtime: &mut Runtime, previous: &Runtime) {
    let mut heads = Vec::new();
    for view in std::iter::once(&runtime.saved).chain(runtime.loaded.iter()) {
        for (p, provider) in &view.config.providers {
            for input in provider.inputs.keys() {
                let Some(now) = binding(view, runtime, p, input) else {
                    continue;
                };
                if runtime.channels.contains_key(&key(&now)) {
                    continue;
                }
                let previous_bindings = || {
                    std::iter::once(&previous.saved)
                        .chain(previous.loaded.iter())
                        .filter_map(|old| {
                            old.providers
                                .iter()
                                .find(|(_, ids)| ids.identity == now.provider_identity)
                                .and_then(|(p, _)| binding(old, previous, p, input))
                        })
                };
                let before = previous_bindings()
                    .find(|before| before.declaration_revision == now.declaration_revision)
                    .or_else(|| previous_bindings().next());
                if let Some(before) = before {
                    let reason = if before.declaration_revision != now.declaration_revision {
                        "declaration_mismatch"
                    } else if before.authorization_generation != now.authorization_generation {
                        "authorization_mismatch"
                    } else {
                        continue;
                    };
                    heads.push((
                        key(&now),
                        InputHead {
                            binding: now,
                            revision: token(),
                            observation_floor: None,
                            value: None,
                            unavailable_reason: Some(reason.into()),
                        },
                    ));
                }
            }
        }
    }
    for (key, head) in heads {
        runtime.channels.entry(key).or_insert(Channel {
            head: Some(head),
            last_attempt: None,
        });
    }
}

/// Authorization replacement or uncertainty advances a lineage even when the
/// desired reference spelling did not change. A qualified token refresh of the
/// same authorization must not call this function.
pub fn authorization_changed(store: &Store, provider: &str) -> Result<()> {
    let _lock = store.lock("routing")?;
    authorization_changed_locked(store, provider)
}

/// Caller holds the routing lock, acquired before any native lock. Native owner
/// operations invalidate synchronously before recording uncertainty or spawning.
pub(crate) fn authorization_changed_locked(store: &Store, provider: &str) -> Result<()> {
    let mut runtime = read_runtime(store)?;
    let previous = runtime.clone();
    let identity = runtime
        .saved
        .providers
        .get(provider)
        .or_else(|| {
            runtime
                .loaded
                .as_ref()
                .and_then(|v| v.providers.get(provider))
        })
        .ok_or_else(|| Error::new("unresolved_reference", "Provider identity is unavailable"))?
        .identity
        .clone();
    let generation = token();
    runtime
        .authorization
        .insert(identity.clone(), generation.clone());
    for view in std::iter::once(&mut runtime.saved).chain(runtime.loaded.iter_mut()) {
        for ids in view.providers.values_mut() {
            if ids.identity == identity {
                ids.authorization = generation.clone();
            }
        }
    }
    invalidate_changed(&mut runtime, &previous);
    retain(&mut runtime);
    write_runtime(store, &runtime)?;
    crate::observations::retain(store, &runtime)
}
/// Reconcile saved intent, preserving identities only for explicit renames.
pub(crate) fn reconcile(
    store: &Store,
    old: &Config,
    new: &Config,
    renames: &[(String, String)],
) -> Result<()> {
    let _lock = store.lock("routing")?;
    let mut runtime = read_runtime(store)?;
    let before = runtime.clone();
    let previous = if signature(&runtime.saved.config) == signature(old) {
        runtime.saved.clone()
    } else {
        let prior = runtime.saved.clone();
        make_view(&mut runtime, &prior, old, &[])
    };
    runtime.saved = make_view(&mut runtime, &previous, new, renames);
    runtime.saved.revision = config::load(store)
        .ok()
        .filter(|file| signature(&file.config) == signature(new))
        .map(|file| file.revision);
    invalidate_changed(&mut runtime, &before);
    retain(&mut runtime);
    write_runtime(store, &runtime)?;
    crate::observations::retain(store, &runtime)
}
/// Called only after successful service activation, not by inspection.
pub fn activate(store: &Store, config: &Config, revision: &str) -> Result<()> {
    let _lock = store.lock("routing")?;
    let mut runtime = read_runtime(store)?;
    let before = runtime.clone();
    let mut view = if signature(&runtime.saved.config) == signature(config) {
        runtime.saved.clone()
    } else if let Some(view) = runtime
        .loaded
        .as_ref()
        .filter(|v| signature(&v.config) == signature(config))
    {
        view.clone()
    } else {
        let old = runtime.loaded.clone().unwrap_or_default();
        make_view(&mut runtime, &old, config, &[])
    };
    view.revision = Some(revision.into());
    runtime.loaded = Some(view);
    invalidate_changed(&mut runtime, &before);
    retain(&mut runtime);
    write_runtime(store, &runtime)?;
    crate::observations::retain(store, &runtime)
}
pub fn deactivate(store: &Store) -> Result<()> {
    let _lock = store.lock("routing")?;
    let mut runtime = read_runtime(store)?;
    SCHEDULES
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&store.root);
    runtime.loaded = None;
    retain(&mut runtime);
    write_runtime(store, &runtime)?;
    crate::observations::retain(store, &runtime)
}

#[derive(Clone)]
pub(crate) struct Snapshot<'a> {
    runtime: Runtime,
    view: Option<View>,
    error: Option<&'static str>,
    metadata: Option<std::borrow::Cow<'a, crate::catalog::Snapshot>>,
}
impl<'a> Snapshot<'a> {
    pub(crate) fn capture(store: &Store, config: &Config) -> Self {
        Self::capture_view(store, config, false, None)
    }
    pub(crate) fn capture_loaded(
        store: &Store,
        config: &Config,
        metadata: Option<&'a crate::catalog::Snapshot>,
    ) -> Self {
        Self::capture_view(store, config, true, metadata)
    }
    fn capture_view(
        store: &Store,
        config: &Config,
        loaded: bool,
        supplied_metadata: Option<&'a crate::catalog::Snapshot>,
    ) -> Self {
        // Readers never initialize storage. If a first writer creates the lock
        // while we read restored files, recapture once under that existing lock.
        let captured = (|| -> Result<Self> {
            let capture = || -> Result<Self> {
                let saved_revision = if loaded {
                    None
                } else {
                    Some(config::raw_revision(store)?)
                };
                let saved_file = if loaded {
                    None
                } else {
                    config::load(store).ok()
                };
                let runtime = read_runtime(store)?;
                let metadata = if loaded {
                    supplied_metadata.map(std::borrow::Cow::Borrowed)
                } else {
                    crate::catalog::read(store, "accepted")
                        .ok()
                        .flatten()
                        .map(std::borrow::Cow::Owned)
                };
                if let Some(revision) = &saved_revision {
                    config::guard(store, revision)?;
                }
                let hash = signature(config);
                let view = if loaded {
                    runtime
                        .loaded
                        .as_ref()
                        .filter(|view| signature(&view.config) == hash)
                        .cloned()
                } else {
                    let mut view = project_view(&runtime.saved, config);
                    view.revision = saved_file
                        .filter(|file| {
                            Some(&file.revision) == saved_revision.as_ref()
                                && signature(&file.config) == hash
                        })
                        .map(|file| file.revision);
                    Some(view)
                };
                Ok(Self {
                    runtime,
                    view,
                    error: None,
                    metadata,
                })
            };
            let mut guard = store.read_lock("routing")?;
            let mut snapshot = capture()?;
            if guard.is_none() {
                guard = store.read_lock("routing")?;
                if guard.is_some() {
                    snapshot = capture()?;
                }
            }
            Ok(snapshot)
        })();
        captured.unwrap_or_else(|error| Self {
            runtime: Runtime::default(),
            view: None,
            error: Some(match error.code.as_str() {
                "invalid_record" => "invalid_record",
                "inputs_changed" => "inputs_changed",
                _ => "storage_unavailable",
            }),
            metadata: None,
        })
    }
    pub(crate) fn configuration_revision(&self) -> Option<&str> {
        self.view.as_ref()?.revision.as_deref()
    }
    pub(crate) fn metadata_revision(&self) -> Option<&str> {
        self.metadata
            .as_ref()
            .map(|snapshot| snapshot.revision.as_str())
    }
    pub(crate) fn metadata(&self) -> Option<&crate::catalog::Snapshot> {
        self.metadata.as_deref()
    }
    pub(crate) fn input_revision(&self, provider: &str, input: &str) -> Value {
        let b = self
            .view
            .as_ref()
            .and_then(|view| binding(view, &self.runtime, provider, input));
        let current_revision = b
            .as_ref()
            .filter(|b| !b.declaration_revision.is_empty())
            .map(|b| revision(self.runtime.channels.get(&key(b))));
        json!({"provider":provider,"input":input,
            "provider_identity":b.as_ref().map(|b|b.provider_identity.as_str()).filter(|s|!s.is_empty()),
            "authorization_generation":b.as_ref().map(|b|b.authorization_generation.as_str()).filter(|s|!s.is_empty()),
            "declaration_revision":b.as_ref().map(|b|b.declaration_revision.as_str()).filter(|s|!s.is_empty()),"current_revision":current_revision})
    }
    pub(crate) fn destination_identity(
        &self,
        provider: &str,
        model: &str,
    ) -> Option<(String, String, String)> {
        let (identity, authorization) =
            provider_scope_binding_in_view(&self.runtime, self.view.as_ref()?, provider)?;
        let model_identity = self
            .view
            .as_ref()?
            .providers
            .get(provider)?
            .models
            .get(model)?;
        Some((identity, authorization, model_identity.clone()))
    }
    pub(crate) fn input(&self, provider: &str, input: &str, at: &ExactInstant) -> Value {
        let Some(view) = &self.view else {
            return json!({"provider":provider,"input":input,"current_revision":null,
            "availability":{"state":"unavailable","reason_code":self.error.unwrap_or("missing")}});
        };
        let Some(b) = binding(view, &self.runtime, provider, input) else {
            return json!({"provider":provider,"input":input,"current_revision":"absent","availability":{"state":"unavailable","reason_code":"missing"}});
        };
        let channel = self.runtime.channels.get(&key(&b));
        let head = channel.and_then(|c| c.head.as_ref());
        let availability = availability_checked(
            head,
            &b,
            at,
            head.is_none_or(|head| same_models(&head.binding.models, &b.models)),
            Some(&view.config.providers[provider].inputs[input]),
        );
        let mut basis = serde_json::to_value(&b).expect("binding serialization");
        basis["current_revision"] = if b.declaration_revision.is_empty() {
            Value::Null
        } else {
            json!(
                channel
                    .and_then(|c| c.head.as_ref())
                    .map(|h| h.revision.as_str())
                    .unwrap_or("absent")
            )
        };
        for field in [
            "provider_identity",
            "authorization_generation",
            "declaration_revision",
        ] {
            if basis[field] == "" {
                basis[field] = Value::Null;
            }
        }
        basis["availability"] = availability;
        let unsafe_basis = basis["availability"]["reason_code"]
            .as_str()
            .is_some_and(|reason| {
                [
                    "invalid_record",
                    "declaration_mismatch",
                    "authorization_mismatch",
                    "storage_unavailable",
                ]
                .contains(&reason)
            });
        if !unsafe_basis
            && let Some(value) = channel
                .and_then(|c| c.head.as_ref())
                .and_then(|h| h.value.as_ref())
        {
            let mut values = serde_json::to_value(value).expect("value serialization");
            values.as_object_mut().unwrap().remove("reference");
            values.as_object_mut().unwrap().remove("received_at");
            basis
                .as_object_mut()
                .unwrap()
                .extend(values.as_object().unwrap().clone());
        }
        basis
    }
}

fn safe_reference(reference: &str) -> Result<()> {
    let url = url::Url::parse(reference).map_err(|_| {
        Error::new(
            "invalid_reference",
            "Provenance must be an absolute HTTPS URL",
        )
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::new(
            "invalid_reference",
            "Provenance must be HTTPS without credentials, query, or fragment",
        ));
    }
    Ok(())
}
fn normalize(value: &mut InputValue, declaration: &InputDeclaration) -> Result<()> {
    for text in [
        &mut value.observed_at,
        &mut value.received_at,
        &mut value.valid_until,
        &mut value.reset_at,
    ] {
        *text = ExactInstant::parse(text)?.canonical();
    }
    if !["manual", "source", "retrieval"].contains(&value.observed_at_basis.as_str()) {
        return Err(Error::new("invalid_record", "Unknown observation basis"));
    }
    if declaration.source == "manual" && value.observed_at_basis != "manual"
        || declaration.source != "manual" && value.observed_at_basis == "manual"
    {
        return Err(Error::new(
            "invalid_record",
            "Observation basis does not match source",
        ));
    }
    if let Some(reference) = &value.reference {
        safe_reference(reference)?;
    }
    match (
        declaration.kind.as_str(),
        &value.remaining_percent,
        &value.remaining,
        &value.total,
        &value.unit,
    ) {
        ("reset", None, None, None, None) => {}
        ("quota", Some(percent), None, None, None) => {
            value.remaining_percent = Some(Decimal::percent(percent)?.text);
        }
        ("quota", None, Some(remaining), Some(total), Some(unit)) => {
            let remaining = Decimal::parse(remaining)?;
            let total = Decimal::parse(total)?;
            if total.coefficient.is_zero()
                || remaining.cmp(&total).is_gt()
                || unit.is_empty()
                || unit.chars().any(char::is_control)
            {
                return Err(Error::new(
                    "invalid_record",
                    "Quota counters require comparable quantities, positive total, and a nonempty unit",
                ));
            }
            value.remaining = Some(remaining.text);
            value.total = Some(total.text);
        }
        _ => {
            return Err(Error::new(
                "invalid_record",
                "Quota requires one percentage or complete counters; reset forbids quota fields",
            ));
        }
    }
    let observed = ExactInstant::parse(&value.observed_at)?;
    let received = ExactInstant::parse(&value.received_at)?;
    let expires = ExactInstant::parse(&value.valid_until)?;
    let reset = ExactInstant::parse(&value.reset_at)?;
    let cap = observed.add_seconds(declaration.max_age_seconds);
    if observed > received
        || expires <= observed
        || expires > reset
        || cap.is_some_and(|cap| expires > cap)
    {
        return Err(Error::new(
            "invalid_observation",
            "Observation or expiry violates the declared freshness bounds",
        ));
    }
    Ok(())
}

fn same_models(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().all(|m| b.contains(m)) && b.iter().all(|m| a.contains(m))
}
fn valid_binding(b: &Binding) -> bool {
    config::valid_id(&b.provider, false)
        && config::valid_id(&b.input, false)
        && [
            &b.provider_identity,
            &b.authorization_generation,
            &b.declaration_revision,
        ]
        .iter()
        .all(|s| !s.is_empty() && !s.chars().any(char::is_control))
        && ["quota", "reset"].contains(&b.kind.as_str())
        && ["manual", "provider", "codex-resets"].contains(&b.source.as_str())
        && !b.scope.is_empty()
        && !b.scope.chars().any(char::is_control)
        && !b.models.is_empty()
        && b.models.iter().enumerate().all(|(i, m)| {
            !m.is_empty() && !m.chars().any(char::is_control) && !b.models[..i].contains(m)
        })
}
fn availability(
    head: Option<&InputHead>,
    b: &Binding,
    declaration: &InputDeclaration,
    at: DateTime<Utc>,
) -> Value {
    availability_checked(
        head,
        b,
        &ExactInstant::from_chrono(at),
        head.is_none_or(|h| same_models(&h.binding.models, &b.models)),
        Some(declaration),
    )
}
fn availability_checked(
    head: Option<&InputHead>,
    b: &Binding,
    at: &ExactInstant,
    coverage: bool,
    declaration: Option<&InputDeclaration>,
) -> Value {
    let reason = match head {
        None => Some("missing"),
        Some(head) => {
            let h = &head.binding;
            if !valid_binding(h)
                || head.revision.is_empty()
                || head.value.is_some() == head.unavailable_reason.is_some()
                || head.unavailable_reason.as_ref().is_some_and(|r| {
                    ![
                        "cleared",
                        "withdrawn",
                        "declaration_mismatch",
                        "authorization_mismatch",
                    ]
                    .contains(&r.as_str())
                })
                || head.value.as_ref().is_some_and(|v| {
                    let max_age = declaration
                        .filter(|_| {
                            h.provider_identity == b.provider_identity
                                && h.declaration_revision == b.declaration_revision
                                && h.kind == b.kind
                                && h.source == b.source
                                && h.scope == b.scope
                                && coverage
                        })
                        .map_or(i64::MAX, |d| d.max_age_seconds);
                    let declaration = InputDeclaration {
                        kind: h.kind.clone(),
                        source: h.source.clone(),
                        scope: h.scope.clone(),
                        models: h.models.clone(),
                        max_age_seconds: max_age,
                        refresh_interval_seconds: None,
                    };
                    let mut normalized = v.clone();
                    normalize(&mut normalized, &declaration).is_err()
                        || head
                            .observation_floor
                            .as_deref()
                            .and_then(|s| ExactInstant::parse(s).ok())
                            != ExactInstant::parse(&v.observed_at).ok()
                })
                || head
                    .observation_floor
                    .as_ref()
                    .is_some_and(|s| ExactInstant::parse(s).is_err())
                || head.value.is_none()
                    && ((head.unavailable_reason.as_deref() == Some("withdrawn"))
                        != head.observation_floor.is_some())
            {
                Some("invalid_record")
            } else if h.provider_identity != b.provider_identity
                || h.declaration_revision != b.declaration_revision
                || h.kind != b.kind
                || h.source != b.source
                || h.scope != b.scope
                || !coverage
            {
                Some("declaration_mismatch")
            } else if h.authorization_generation != b.authorization_generation {
                Some("authorization_mismatch")
            } else if let Some(reason) = &head.unavailable_reason {
                Some(reason.as_str())
            } else if let Some(value) = &head.value {
                if at < &ExactInstant::parse(&value.observed_at).unwrap() {
                    Some("not_yet_observed")
                } else if at >= &ExactInstant::parse(&value.valid_until).unwrap() {
                    Some("expired")
                } else {
                    None
                }
            } else {
                Some("invalid_record")
            }
        }
    };
    json!({"state":if reason.is_none() {"available"} else {"unavailable"},"reason_code":reason})
}
pub(crate) fn quota_matches(basis: &Value, threshold: &str) -> Result<bool> {
    let threshold = Decimal::percent(threshold)?;
    if let Some(percent) = basis["remaining_percent"].as_str() {
        return Ok(!Decimal::percent(percent)?.cmp(&threshold).is_lt());
    }
    let remaining = Decimal::parse(
        basis["remaining"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_record", "Missing remaining counter"))?,
    )?;
    let total = Decimal::parse(
        basis["total"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_record", "Missing total counter"))?,
    )?;
    Ok(!Decimal::parse("100")?
        .product(&remaining)
        .cmp(&threshold.product(&total))
        .is_lt())
}

fn revision(channel: Option<&Channel>) -> &str {
    channel
        .and_then(|c| c.head.as_ref())
        .map(|h| h.revision.as_str())
        .unwrap_or("absent")
}
fn same_observation(a: &InputValue, b: &InputValue) -> bool {
    // Receipt metadata is local, but expiry is part of the source assertion, not a freshness renewal.
    a.observed_at == b.observed_at
        && a.observed_at_basis == b.observed_at_basis
        && a.valid_until == b.valid_until
        && a.reset_at == b.reset_at
        && a.remaining_percent == b.remaining_percent
        && a.remaining == b.remaining
        && a.total == b.total
        && a.unit == b.unit
        && a.reference == b.reference
}
fn prepare_observation(
    channel: &Channel,
    declaration: &InputDeclaration,
    value: &mut InputValue,
    at: DateTime<Utc>,
) -> Result<bool> {
    normalize(value, declaration)?;
    let observed = ExactInstant::parse(&value.observed_at)?;
    if let Some(head) = &channel.head
        && let Some(floor) = &head.observation_floor
    {
        let floor = ExactInstant::parse(floor)?;
        if observed < floor {
            return Err(Error::new(
                "older_observation",
                "Observation predates the current ordering watermark",
            ));
        }
        if observed == floor {
            if head
                .value
                .as_ref()
                .is_some_and(|old| same_observation(old, value))
            {
                return Ok(false);
            }
            return Err(Error::new(
                "conflicting_observation",
                "A different assertion already exists at this observation instant",
            ));
        }
    }
    let publication = ExactInstant::from_chrono(at);
    if ExactInstant::parse(&value.received_at)? > publication
        || ExactInstant::parse(&value.reset_at)? <= publication
        || ExactInstant::parse(&value.valid_until)? <= publication
    {
        return Err(Error::new(
            "invalid_observation",
            "A new value must be received, unexpired, and strictly before its reset",
        ));
    }
    Ok(true)
}
fn publish(
    channel: &mut Channel,
    b: &Binding,
    declaration: &InputDeclaration,
    mut value: InputValue,
    at: DateTime<Utc>,
) -> Result<bool> {
    if !prepare_observation(channel, declaration, &mut value, at)? {
        return Ok(false);
    }
    channel.head = Some(InputHead {
        binding: b.clone(),
        revision: token(),
        observation_floor: Some(value.observed_at.clone()),
        value: Some(value),
        unavailable_reason: None,
    });
    Ok(true)
}

fn argument<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request
        .arguments
        .get(name)
        .and_then(|v| v.first())
        .map(String::as_str)
}
fn required<'a>(request: &'a Request, name: &str) -> Result<&'a str> {
    argument(request, name).ok_or_else(|| {
        Error::new("missing_argument", "Required input operand is missing").field(name)
    })
}
fn selected_binding(
    runtime: &Runtime,
    config: &Config,
    provider: &str,
    input: &str,
    loaded: bool,
) -> Result<Binding> {
    let view = if loaded {
        runtime.loaded.as_ref()
    } else {
        Some(&runtime.saved)
    }
    .filter(|v| signature(&v.config) == signature(config))
    .ok_or_else(|| Error::new("inputs_changed", "Configuration binding is not current"))?;
    binding(view, runtime, provider, input)
        .ok_or_else(|| Error::new("unresolved_reference", "Input declaration does not resolve"))
}

fn guard_reason(
    captured: &Binding,
    current: Option<&Binding>,
    before: &str,
    after: &str,
) -> Option<&'static str> {
    match current {
        None => Some("declaration_removed"),
        Some(now) if now.provider_identity != captured.provider_identity => {
            Some("declaration_removed")
        }
        Some(now) if now.authorization_generation != captured.authorization_generation => {
            Some("authorization_changed")
        }
        Some(now) if now.declaration_revision != captured.declaration_revision => {
            Some("declaration_changed")
        }
        Some(_) if after != before => Some("input_changed"),
        _ => None,
    }
}
fn current_saved_binding(
    store: &Store,
    runtime: &Runtime,
    captured: &Binding,
) -> Result<Option<Binding>> {
    let saved = config::load(store)?;
    let view = project_view(&runtime.saved, &saved.config);
    Ok(view
        .providers
        .iter()
        .find(|(_, ids)| ids.identity == captured.provider_identity)
        .and_then(|(provider, _)| binding(&view, runtime, provider, &captured.input)))
}

fn ensure_saved_view(store: &Store, captured: &Config) -> Result<()> {
    let _configuration = store.lock("configuration")?;
    let current = config::load(store)?;
    if signature(&current.config) != signature(captured) {
        return Err(Error::new(
            "inputs_changed",
            "Saved configuration changed before the input operation",
        ));
    }
    let runtime = read_runtime(store)?;
    if signature(&runtime.saved.config) != signature(captured) {
        reconcile(store, &runtime.saved.config, captured, &[])?;
    }
    Ok(())
}
fn capability(provider: &Provider, declaration: &InputDeclaration) -> bool {
    declaration.source == "codex-resets"
        && declaration.kind == "reset"
        && declaration.scope == "codex"
        && crate::provider_integration::integration_id(provider).ok() == Some("codex-subscription")
}
fn use_projection(
    runtime: &Runtime,
    selected: &Binding,
    loaded: bool,
    service_running: bool,
) -> Value {
    if loaded && !service_running {
        return json!({"state":"no_loaded_snapshot","reason_code":null});
    }
    let view = if loaded {
        runtime.loaded.as_ref()
    } else {
        Some(&runtime.saved)
    };
    let Some(view) = view else {
        return json!({"state":if loaded {"no_loaded_snapshot"} else {"unavailable"},"reason_code":null});
    };
    let other = view
        .providers
        .iter()
        .find(|(_, p)| p.identity == selected.provider_identity)
        .and_then(|(p, _)| binding(view, runtime, p, &selected.input));
    let matches = other.as_ref().is_some_and(|b| key(b) == key(selected));
    json!({"state":if matches {"applicable"} else if loaded {"pending"} else if other.is_none() {"no_saved_declaration"} else {"not_applicable"},"reason_code":if matches {None} else {Some("declaration_mismatch")}})
}

fn valid_attempt(attempt: &AcquisitionAttempt) -> bool {
    if !valid_binding(&attempt.binding)
        || attempt.revision.is_empty()
        || attempt.head_revision_before.is_empty()
        || attempt.head_revision_after.is_empty()
        || !["explicit", "automatic", "provider_refresh"].contains(&attempt.trigger.as_str())
    {
        return false;
    }
    let (Ok(started), Ok(finished)) = (
        ExactInstant::parse(&attempt.started_at),
        ExactInstant::parse(&attempt.finished_at),
    ) else {
        return false;
    };
    if started > finished {
        return false;
    }
    let reason = attempt.reason_code.as_deref();
    match attempt.outcome.as_str() {
        "published" => reason.is_none(),
        "unchanged" => reason == Some("identical_observation"),
        "withdrawn" => reason.is_some_and(|r| {
            [
                "explicit_absence",
                "explicit_withdrawal",
                "reset_time_passed",
            ]
            .contains(&r)
        }),
        "failed" => reason.is_some_and(|r| {
            [
                "transport_error",
                "source_format_error",
                "invalid_observation",
                "older_observation",
                "conflicting_observation",
                "storage_unavailable",
            ]
            .contains(&r)
        }),
        "unavailable" => reason.is_some_and(|r| {
            [
                "unsupported_source",
                "credential_unavailable",
                "noninteractive_unavailable",
            ]
            .contains(&r)
        }),
        "superseded" => reason.is_some_and(|r| {
            [
                "input_changed",
                "declaration_changed",
                "authorization_changed",
                "declaration_removed",
            ]
            .contains(&r)
        }),
        _ => false,
    }
}
#[derive(Clone, Copy)]
struct InputInspection<'a> {
    store: &'a Store,
    runtime: &'a Runtime,
    config: &'a Config,
    at: DateTime<Utc>,
    loaded: bool,
    running: bool,
}
impl InputInspection<'_> {
    fn details(&self, provider: &str, name: &str) -> Result<Value> {
        let Self {
            store,
            runtime,
            config,
            at,
            loaded,
            running,
        } = *self;
        let p = config
            .providers
            .get(provider)
            .ok_or_else(|| Error::new("unresolved_reference", "Unknown provider"))?;
        let d = p
            .inputs
            .get(name)
            .ok_or_else(|| Error::new("unresolved_reference", "Unknown input"))?;
        let b = selected_binding(runtime, config, provider, name, loaded)?;
        let channel = runtime.channels.get(&key(&b));
        let mut head = channel.and_then(|c| c.head.clone());
        if let Some(head) = &mut head {
            head.binding.provider = provider.into();
        }
        let raw_attempt = channel.and_then(|c| c.last_attempt.as_ref());
        let mut attempt = raw_attempt
            .filter(|attempt| valid_attempt(attempt) && key(&attempt.binding) == key(&b))
            .cloned();
        if let Some(attempt) = &mut attempt {
            attempt.binding.provider = provider.into();
        }
        let loaded_declaration = runtime.loaded.as_ref().and_then(|view| {
            view.providers
                .iter()
                .find(|(_, p)| p.identity == b.provider_identity)
                .and_then(|(p, _)| {
                    binding(view, runtime, p, name)
                        .filter(|other| key(other) == key(&b))
                        .and_then(|_| view.config.providers[p].inputs.get(name))
                })
        });
        let interval = loaded_declaration.and_then(|d| d.refresh_interval_seconds);
        let busy = if capability(p, d) {
            store.is_locked(&acquisition_name(&b))
        } else {
            Ok(None)
        };
        let acquisition_state = if d.source == "manual" {
            "manual"
        } else if !capability(p, d) || busy.is_err() {
            "unavailable"
        } else if busy.as_ref().is_ok_and(|state| *state == Some(true)) {
            "in_progress"
        } else if !running && d.refresh_interval_seconds.is_some() {
            "paused"
        } else if running && loaded_declaration.is_none() && d.refresh_interval_seconds.is_some() {
            "not_loaded"
        } else if interval.is_some() {
            "scheduled"
        } else {
            "on_demand"
        };
        let availability = availability(head.as_ref(), &b, d, at);
        let invalid = availability["reason_code"] == "invalid_record";
        if invalid {
            head = None;
        }
        let deps: Vec<_> = p
            .models
            .iter()
            .flat_map(|(m, model)| {
                model.priority_rules.iter().filter(move |r| r.input == name)
        .map(move |r| json!({"qualified_model":format!("{provider}/{m}"),"rule_id":r.id}))
            })
            .collect();
        Ok(
            json!({"input":name,"provider":provider,"kind":d.kind,"source":d.source,"scope":d.scope,"models":d.models,
        "current_revision":if invalid||b.declaration_revision.is_empty(){Value::Null}else{json!(revision(channel))},"availability":availability,
        "acquisition_state":acquisition_state,"last_attempt_outcome":attempt.as_ref().map(|a|&a.outcome),
        "declaration_revision":if b.declaration_revision.is_empty(){Value::Null}else{json!(b.declaration_revision)},"declaration":d,"current_data":head,"last_attempt":attempt,
        "last_attempt_state":if raw_attempt.is_some()&&attempt.is_none(){"unavailable"}else if attempt.is_some(){"known"}else{"absent"},"dependencies":deps,
        "saved_use":use_projection(runtime,&b,false,running),"current_runtime_use":use_projection(runtime,&b,true,running),
        "acquisition":{"state":acquisition_state,"policy_configuration_revision":runtime.loaded.as_ref().filter(|_|running).and_then(|view|view.revision.as_deref()),
            "active_refresh_interval_seconds":if running {interval}else{None},"next_due_at":if running{scheduled_next_due(store,&b)}else{None},
            "reason_code":if d.source!="manual"&&!capability(p,d){Some("unsupported_source")}else if busy.is_err(){Some("storage_unavailable")}else if acquisition_state=="paused"{Some("service_not_running")}else{None}}}),
        )
    }
}

pub async fn handle(store: &Store, saved: &Config, request: &Request) -> Result<Value> {
    handle_impl(store, Some(saved), request).await
}
pub async fn handle_unavailable_saved(store: &Store, request: &Request) -> Result<Value> {
    handle_impl(store, None, request).await
}
async fn handle_impl(store: &Store, saved: Option<&Config>, request: &Request) -> Result<Value> {
    let action = request.command.get(2).map(String::as_str).unwrap_or("");
    let provider = request
        .operands
        .first()
        .ok_or_else(|| Error::new("missing_argument", "Provider is required"))?;
    let read_only = matches!(action, "list" | "inspect" | "show");
    let (loaded_info, runtime_error) = match crate::service::loaded(store).await {
        Ok(loaded) => (loaded, None),
        Err(error) if read_only => (None, Some(error.code)),
        Err(error) => return Err(error),
    };
    let running = loaded_info.is_some();
    let loaded = argument(request, "state") == Some("loaded");
    if read_only && loaded && loaded_info.is_none() {
        return Ok(
            json!({"provider":provider,"state":"loaded","snapshot_state":if runtime_error.is_some(){"unavailable"}else{"absent"},"configuration_revision":null,"reason_code":runtime_error.as_deref().unwrap_or("no_loaded_snapshot")}),
        );
    }
    let runtime_view = json!({"snapshot_state":if runtime_error.is_some(){"unavailable"}else if running{"available"}else{"absent"},"configuration_revision":loaded_info.as_ref().map(|file|&file.revision),"reason_code":runtime_error});
    let saved_unavailable = saved.is_none();
    if saved_unavailable && (!loaded || !["list", "inspect", "show"].contains(&action)) {
        return Err(Error::new(
            "invalid_configuration",
            "This operation requires valid saved configuration",
        ));
    }
    let saved = match saved {
        Some(config) => config,
        None => {
            &loaded_info
                .as_ref()
                .ok_or_else(|| {
                    Error::new("no_loaded_snapshot", "No resident configuration is loaded")
                })?
                .config
        }
    };
    let config = if loaded {
        &loaded_info
            .as_ref()
            .ok_or_else(|| Error::new("no_loaded_snapshot", "No resident configuration is loaded"))?
            .config
    } else {
        saved
    };
    let p = config
        .providers
        .get(provider)
        .ok_or_else(|| Error::new("unresolved_reference", "Unknown provider"))?;
    let at = Utc::now();
    if action == "list" || action == "inspect" || action == "show" {
        let (mut runtime, read_error) = match read_runtime(store) {
            Ok(state) => (state, None),
            Err(error) => (
                Runtime::default(),
                Some(if error.code == "invalid_record" {
                    "invalid_record"
                } else {
                    "storage_unavailable"
                }),
            ),
        };
        if !loaded && signature(&runtime.saved.config) != signature(config) {
            runtime.saved = project_view(&runtime.saved, config);
        }
        if loaded
            && runtime
                .loaded
                .as_ref()
                .is_none_or(|v| signature(&v.config) != signature(config))
        {
            runtime.loaded = Some(project_view(
                runtime.loaded.as_ref().unwrap_or(&runtime.saved),
                config,
            ));
        }
        let inspection = InputInspection {
            store,
            runtime: &runtime,
            config,
            at,
            loaded,
            running,
        };
        let render = |name: &str| -> Result<Value> {
            let mut detail = inspection.details(provider, name)?;
            if let Some(reason) = read_error {
                detail["availability"] = json!({"state":"unavailable","reason_code":reason});
                detail["current_revision"] = Value::Null;
                detail["current_data"] = Value::Null;
                detail["last_attempt"] = Value::Null;
                detail["last_attempt_state"] = json!("unavailable");
            }
            if saved_unavailable {
                detail["saved_use"] =
                    json!({"state":"unavailable","reason_code":"configuration_unavailable"});
            }
            if let Some(reason) = runtime_error.as_deref() {
                detail["current_runtime_use"] = json!({"state":"unavailable","reason_code":reason});
                detail["acquisition_state"] = json!("unavailable");
                detail["acquisition"] = json!({"state":"unavailable","reason_code":reason,"policy_configuration_revision":null,"active_refresh_interval_seconds":null,"next_due_at":null});
            }
            Ok(detail)
        };
        if action == "list" {
            let mut summaries = Vec::new();
            for name in p.inputs.keys() {
                let detail = render(name)?;
                let mut summary = serde_json::Map::new();
                for field in [
                    "input",
                    "kind",
                    "source",
                    "current_revision",
                    "availability",
                    "acquisition_state",
                    "last_attempt_outcome",
                ] {
                    summary.insert(field.into(), detail[field].clone());
                }
                summaries.push(Value::Object(summary));
            }
            return Ok(
                json!({"provider":provider,"state":if loaded{"loaded"}else{"saved"},"snapshot_state":"known","at":timestamp(at),"inputs":summaries,"runtime":runtime_view}),
            );
        }
        let name = request
            .operands
            .get(1)
            .ok_or_else(|| Error::new("missing_argument", "Input is required"))?;
        return Ok(
            json!({"provider":provider,"state":if loaded{"loaded"}else{"saved"},"snapshot_state":"known","at":timestamp(at),"input":render(name)?,"runtime":runtime_view}),
        );
    }
    let name = request
        .operands
        .get(1)
        .ok_or_else(|| Error::new("missing_argument", "Input is required"))?;
    let declaration = p
        .inputs
        .get(name)
        .ok_or_else(|| Error::new("unresolved_reference", "Unknown input"))?;
    if action == "refresh" && declaration.source == "manual" {
        return Err(Error::new(
            "manual_source",
            "Manual inputs use record, not refresh",
        ));
    }
    if action == "record" && declaration.source != "manual" {
        return Err(Error::new(
            "nonmanual_source",
            "Only manual inputs can be recorded",
        ));
    }
    if !["refresh", "record", "clear"].contains(&action) {
        return Err(Error::new(
            "unknown_operation",
            "Unsupported input operation",
        ));
    }
    if loaded {
        return Err(Error::new(
            "invalid_arguments",
            "Input mutations always select saved declarations",
        ));
    }
    let mut candidate = if action == "record" {
        Some(InputValue {
            observed_at: required(request, "observed-at")?.into(),
            observed_at_basis: "manual".into(),
            received_at: timestamp(at),
            valid_until: required(request, "valid-until")?.into(),
            reset_at: required(request, "reset-at")?.into(),
            remaining_percent: argument(request, "remaining-percent").map(str::to_owned),
            remaining: argument(request, "remaining").map(str::to_owned),
            total: argument(request, "total").map(str::to_owned),
            unit: argument(request, "unit").map(str::to_owned),
            reference: argument(request, "reference").map(str::to_owned),
        })
    } else {
        None
    };
    if let Some(value) = &mut candidate {
        normalize(value, declaration)?;
    }
    let mut preview = read_runtime(store)?;
    let prior = preview.saved.clone();
    preview.saved = project_view(&prior, saved);
    let b = selected_binding(&preview, saved, provider, name, false)?;
    let channel = preview.channels.get(&key(&b));
    if let Some(expected) = argument(request, "if-input-revision") {
        let unbound_replacement = b.declaration_revision.is_empty()
            && prior
                .config
                .providers
                .get(provider)
                .is_some_and(|p| p.inputs.contains_key(name));
        if unbound_replacement || expected != revision(channel) {
            return Err(Error::new(
                "inputs_changed",
                "Input revision does not match a known current head",
            ));
        }
    }
    if let Some(value) = &mut candidate {
        prepare_observation(
            channel.unwrap_or(&Channel::default()),
            declaration,
            value,
            at,
        )?;
    }
    if request.dry_run {
        let mut proposed = serde_json::to_value(&candidate)
            .map_err(|_| Error::new("invalid_record", "Cannot describe proposed input"))?;
        if let Some(object) = proposed.as_object_mut() {
            object.remove("received_at");
        }
        return Ok(
            json!({"provider":provider,"input_name":name,"input_change":{"outcome":"planned","current_revision_before":revision(channel),"current_revision_after":null,"published":false,"attempt_recorded":false},
            "current_data":channel.and_then(|c|c.head.as_ref()),"proposed_data":proposed,"unavailable_reason":if action=="refresh"&&!capability(p,declaration){Some("unsupported_source")}else{None},
            "current_runtime_use":use_projection(&preview,&b,true,running)}),
        );
    }
    let captured_binding = b;
    let captured_revision = revision(channel).to_owned();
    ensure_saved_view(store, saved)?;
    if action == "refresh" {
        return acquire(store, saved, provider, name, false, "explicit", running).await;
    }
    let _configuration = store.lock("configuration")?;
    let _lock = store.lock("routing")?;
    let mut runtime = read_runtime(store)?;
    let b = selected_binding(&runtime, saved, provider, name, false)?;
    let current = current_saved_binding(store, &runtime, &b)?;
    if let Some(reason) = guard_reason(&b, current.as_ref(), "absent", "absent") {
        return Err(Error::new("inputs_changed", reason));
    }
    if !captured_binding.declaration_revision.is_empty()
        && let Some(reason) = guard_reason(
            &captured_binding,
            Some(&b),
            &captured_revision,
            revision(runtime.channels.get(&key(&b))),
        )
    {
        return Err(Error::new("inputs_changed", reason));
    }
    let channel = runtime.channels.entry(key(&b)).or_default();
    let before = revision(Some(channel)).to_owned();
    if argument(request, "if-input-revision").is_some_and(|expected| expected != before) {
        return Err(Error::new(
            "inputs_changed",
            "Input revision does not match",
        ));
    }
    let changed = if let Some(value) = candidate {
        publish(channel, &b, declaration, value, Utc::now())?
    } else {
        channel.head = Some(InputHead {
            binding: b.clone(),
            revision: token(),
            observation_floor: None,
            value: None,
            unavailable_reason: Some("cleared".into()),
        });
        true
    };
    let after = revision(Some(channel)).to_owned();
    let runtime_use = use_projection(&runtime, &b, true, running);
    write_runtime(store, &runtime)?;
    Ok(
        json!({"provider":provider,"input_name":name,"input_change":{"outcome":if !changed{"unchanged"}else if action=="clear"{"cleared"}else{"published"},
        "current_revision_before":before,"current_revision_after":after,"published":changed,"attempt_recorded":false},"current_runtime_use":runtime_use}),
    )
}

pub fn snapshot(store: &Store, config: &Config) -> Result<Value> {
    let snapshot = Snapshot::capture(store, config);
    let at = ExactInstant::from_chrono(Utc::now());
    let mut providers = BTreeMap::new();
    for (name, p) in &config.providers {
        let mut inputs = BTreeMap::new();
        for input in p.inputs.keys() {
            inputs.insert(input.clone(), snapshot.input(name, input, &at));
        }
        providers.insert(name.clone(), inputs);
    }
    Ok(
        json!({"at":at.canonical(),"configuration_revision":snapshot.configuration_revision(),"metadata_revision":snapshot.metadata_revision(),"providers":providers}),
    )
}

pub fn provider_identity(store: &Store, config: &Config, provider: &str) -> Result<Option<String>> {
    let snapshot = Snapshot::capture(store, config);
    if let Some(reason) = snapshot.error {
        return Err(Error::new(
            reason,
            "Provider incarnation state is unavailable",
        ));
    }
    Ok(snapshot
        .view
        .as_ref()
        .and_then(|view| view.providers.get(provider))
        .filter(|ids| !ids.identity.is_empty())
        .map(|ids| ids.identity.clone()))
}

// Cross-process acquisition exclusion is distinct from the short publication lock.
// It is held over source I/O; admission and manual record/clear never acquire it.
fn acquisition_name(b: &Binding) -> String {
    format!(
        "input-acquisition-{}",
        signature(&(&b.provider_identity, &b.input))
    )
}
fn acquisition_lock(store: &Store, b: &Binding) -> Result<crate::storage::LockGuard> {
    store.try_lock(&acquisition_name(b))?.ok_or_else(|| {
        Error::new(
            "acquisition_in_progress",
            "An acquisition is already in progress for this input",
        )
    })
}
fn scheduled_next_due(store: &Store, b: &Binding) -> Option<String> {
    let schedules = SCHEDULES.lock().unwrap_or_else(|p| p.into_inner());
    let next = schedules.get(&store.root)?.due.get(&key(b))?.next?;
    let remaining = Duration::from_std(next.saturating_duration_since(Instant::now())).ok()?;
    Utc::now().checked_add_signed(remaining).map(timestamp)
}

#[derive(Debug)]
struct Forecast {
    reset_at: ExactInstant,
    reference: String,
}
fn parse_markdown(text: &str) -> Result<Forecast> {
    use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
    let fail = || {
        Error::new(
            "source_format_error",
            "Unsupported Codex Resets Markdown representation",
        )
    };
    let mut title = 0;
    let mut sections = 0;
    let mut in_section = false;
    let mut heading = None;
    let mut heading_text = String::new();
    let mut quote_depth = 0usize;
    let mut list_depth = 0usize;
    let mut timestamps = 0usize;
    let mut paragraph = false;
    let mut paragraph_text = String::new();
    let mut codes = Vec::new();
    let mut paragraphs = Vec::new();
    let mut links = Vec::new();
    let mut current_link: Option<(String, String)> = None;
    for event in Parser::new(text) {
        match event {
            Event::Start(Tag::BlockQuote(_)) => quote_depth += 1,
            Event::End(TagEnd::BlockQuote(_)) => quote_depth = quote_depth.saturating_sub(1),
            Event::Start(Tag::List(_)) => list_depth += 1,
            Event::End(TagEnd::List(_)) => list_depth = list_depth.saturating_sub(1),
            Event::Start(Tag::Heading { level, .. }) if quote_depth == 0 && list_depth == 0 => {
                heading = Some(level);
                heading_text.clear();
                if level == HeadingLevel::H2 {
                    in_section = false;
                }
            }
            Event::End(TagEnd::Heading(level)) if heading == Some(level) => {
                if level == HeadingLevel::H1 && heading_text == "Codex Resets" {
                    title += 1;
                }
                if level == HeadingLevel::H2 && heading_text == "Reset scheduled" {
                    sections += 1;
                    in_section = true;
                }
                heading = None;
            }
            Event::Start(Tag::Paragraph) if in_section && quote_depth == 0 && list_depth == 0 => {
                paragraph = true;
                paragraph_text.clear();
                codes.clear();
            }
            Event::End(TagEnd::Paragraph) if paragraph => {
                if paragraph_text
                    .split('·')
                    .any(|p| p.trim() == "Upcoming reset")
                {
                    paragraphs.push((paragraph_text.clone(), codes.clone()));
                }
                paragraph = false;
            }
            Event::Start(Tag::Link { dest_url, .. })
                if in_section && quote_depth == 0 && list_depth == 0 =>
            {
                if current_link.is_some() {
                    return Err(fail());
                }
                current_link = Some((dest_url.to_string(), String::new()));
            }
            Event::End(TagEnd::Link) => {
                if let Some((url, label)) = current_link.take()
                    && label == "View announcement"
                {
                    links.push(url);
                }
            }
            Event::Text(text) => {
                if heading.is_some() {
                    heading_text.push_str(&text);
                }
                if paragraph {
                    paragraph_text.push_str(&text);
                }
                if let Some((_, label)) = &mut current_link {
                    label.push_str(&text);
                }
            }
            Event::Code(text) => {
                if in_section
                    && quote_depth == 0
                    && list_depth == 0
                    && text.ends_with('Z')
                    && ExactInstant::parse(&text).is_ok()
                {
                    timestamps += 1;
                }
                if paragraph {
                    paragraph_text.push_str(&text);
                    codes.push(text.to_string());
                }
            }
            Event::SoftBreak | Event::HardBreak if paragraph => {
                paragraph_text.push(' ');
            }
            _ => {}
        }
    }
    if title != 1 || sections != 1 || paragraphs.len() != 1 || links.len() != 1 || timestamps != 1 {
        return Err(fail());
    }
    let (paragraph, codes) = paragraphs.pop().unwrap();
    if codes.len() != 1
        || !codes[0].ends_with('Z')
        || paragraph
            .split('·')
            .filter(|part| part.trim() == codes[0])
            .count()
            != 1
    {
        return Err(fail());
    }
    let reset_at = ExactInstant::parse(&codes[0]).map_err(|_| fail())?;
    let reference = links.pop().unwrap();
    if !reference
        .strip_prefix("https://x.com/thsottiaux/status/")
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err(fail());
    }
    Ok(Forecast {
        reset_at,
        reference,
    })
}
async fn fetch_forecast(config: &Config) -> Result<Forecast> {
    use futures_util::StreamExt;
    let client = crate::provider_integration::client(config).map_err(|_| {
        Error::new(
            "transport_error",
            "Cannot initialize the configured input transport",
        )
    })?;
    let response = client
        .get("https://codex-resets.com/index.md")
        .header(reqwest::header::ACCEPT, "text/markdown")
        .send()
        .await
        .map_err(|_| Error::new("transport_error", "Codex announcement transport failed"))?;
    if !response.status().is_success()
        || response.url().as_str() != "https://codex-resets.com/index.md"
    {
        return Err(Error::new(
            "transport_error",
            "Codex announcement response or redirect was refused",
        ));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    if content_type != "text/markdown" {
        return Err(Error::new(
            "source_format_error",
            "Codex announcement is not qualified Markdown",
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| {
            Error::new("transport_error", "Codex announcement body was interrupted")
        })?;
        if bytes.len().saturating_add(chunk.len()) > 1024 * 1024 {
            return Err(Error::new(
                "source_format_error",
                "Codex announcement exceeds the bounded source size",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| Error::new("source_format_error", "Codex announcement is not UTF-8"))?;
    parse_markdown(text)
}

fn persistence_failure(
    provider: &str,
    input: &str,
    before: &str,
    acquisition_result: &str,
    uncertain: bool,
    error: &Error,
) -> Value {
    json!({"_code":"storage_unavailable","provider":provider,"input_name":input,
        "input_change":{"outcome":"not_published","current_revision_before":before,"current_revision_after":null,
            "published":if uncertain{None}else{Some(false)},"attempt_recorded":if uncertain{None}else{Some(false)}},
        "attempt":null,"unavailable_reason":"storage_unavailable","acquisition_completed":true,"acquisition_result":acquisition_result,
        "publication_state":if uncertain{"unknown"}else{"not_performed"},"attempt_persistence":if uncertain{"unknown"}else{"not_performed"},"persistence_error":error})
}

async fn acquire(
    store: &Store,
    config: &Config,
    provider: &str,
    input: &str,
    loaded: bool,
    trigger: &str,
    running: bool,
) -> Result<Value> {
    let declaration = &config.providers[provider].inputs[input];
    let (mut b, before, _exclusive) = {
        let _configuration = if loaded {
            None
        } else {
            Some(store.lock("configuration")?)
        };
        let _lock = store.lock("routing")?;
        let runtime = read_runtime(store)?;
        let b = selected_binding(&runtime, config, provider, input, loaded)?;
        if !loaded {
            let current = current_saved_binding(store, &runtime, &b)?;
            if let Some(reason) = guard_reason(&b, current.as_ref(), "absent", "absent") {
                return Err(Error::new("inputs_changed", reason));
            }
        }
        let exclusive = acquisition_lock(store, &b)?;
        let before = revision(runtime.channels.get(&key(&b))).to_owned();
        (b, before, exclusive)
    };
    let started = Utc::now();
    let result = if capability(&config.providers[provider], declaration) {
        match tokio::time::timeout(std::time::Duration::from_secs(30), fetch_forecast(config)).await
        {
            Ok(result) => result,
            Err(_) => Err(Error::new(
                "transport_error",
                "Input source deadline elapsed",
            )),
        }
    } else {
        Err(Error::new(
            "unsupported_source",
            "The integration has no qualified acquisition for this input",
        ))
    };
    let received = Utc::now();
    let acquisition_result = match &result {
        Ok(_) => "acquired".to_owned(),
        Err(error) => error.code.clone(),
    };
    let prepared = (|| -> Result<_> {
        let configuration = if loaded {
            None
        } else {
            Some(store.lock("configuration")?)
        };
        let lock = store.lock("routing")?;
        let runtime = read_runtime(store)?;
        Ok((configuration, lock, runtime))
    })();
    let (_configuration, _lock, mut runtime) = match prepared {
        Ok(state) => state,
        Err(error) => {
            return Ok(persistence_failure(
                provider,
                input,
                &before,
                &acquisition_result,
                false,
                &error,
            ));
        }
    };
    let finished = Utc::now();
    let (current, authority_error) = if loaded {
        (
            runtime.loaded.as_ref().and_then(|view| {
                view.providers
                    .iter()
                    .find(|(_, p)| p.identity == b.provider_identity)
                    .and_then(|(p, _)| binding(view, &runtime, p, input))
            }),
            None,
        )
    } else {
        match current_saved_binding(store, &runtime, &b) {
            Ok(current) => (current, None),
            Err(error) => (None, Some(error)),
        }
    };
    let provider = current
        .as_ref()
        .filter(|now| now.provider_identity == b.provider_identity)
        .map(|now| now.provider.as_str())
        .unwrap_or(provider);
    b.provider = provider.into();
    let mut attempt = AcquisitionAttempt {
        binding: b.clone(),
        revision: token(),
        started_at: timestamp(started),
        finished_at: timestamp(finished),
        trigger: trigger.into(),
        outcome: String::new(),
        reason_code: None,
        head_revision_before: before.clone(),
        head_revision_after: revision(runtime.channels.get(&key(&b))).into(),
    };
    let after = current
        .as_ref()
        .map(|b| revision(runtime.channels.get(&key(b))))
        .unwrap_or("absent");
    let superseded = if authority_error.is_some() {
        Some("declaration_changed")
    } else {
        guard_reason(&b, current.as_ref(), &before, after)
    };
    let mut published = false;
    let mut attach = false;
    if let Some(reason) = superseded {
        attempt.outcome = "superseded".into();
        attempt.reason_code = Some(reason.into());
        if let Some(now) = &current
            && key(now) == key(&b)
        {
            attach = true;
            attempt.head_revision_after = revision(runtime.channels.get(&key(now))).into();
        }
    } else {
        attach = true;
        let channel = runtime.channels.entry(key(&b)).or_default();
        match result {
            Err(error) => {
                attempt.outcome = if [
                    "unsupported_source",
                    "credential_unavailable",
                    "noninteractive_unavailable",
                ]
                .contains(&error.code.as_str())
                {
                    "unavailable"
                } else {
                    "failed"
                }
                .into();
                attempt.reason_code = Some(error.code);
            }
            Ok(forecast) => {
                if forecast.reset_at <= ExactInstant::from_chrono(finished) {
                    let floor = channel
                        .head
                        .as_ref()
                        .and_then(|h| h.observation_floor.as_deref())
                        .map(ExactInstant::parse)
                        .transpose()?;
                    let observed = ExactInstant::from_chrono(received);
                    if floor.as_ref().is_some_and(|floor| &observed < floor) {
                        attempt.outcome = "failed".into();
                        attempt.reason_code = Some("older_observation".into());
                    } else if floor.as_ref() == Some(&observed) {
                        attempt.outcome = "failed".into();
                        attempt.reason_code = Some("conflicting_observation".into());
                    } else {
                        channel.head = Some(InputHead {
                            binding: b.clone(),
                            revision: token(),
                            observation_floor: Some(timestamp(received)),
                            value: None,
                            unavailable_reason: Some("withdrawn".into()),
                        });
                        attempt.outcome = "withdrawn".into();
                        attempt.reason_code = Some("reset_time_passed".into());
                        published = true;
                    }
                } else {
                    let expiry = ExactInstant::from_chrono(after_seconds(
                        received,
                        declaration.max_age_seconds,
                    ))
                    .min(forecast.reset_at.clone());
                    let value = InputValue {
                        observed_at: timestamp(received),
                        observed_at_basis: "retrieval".into(),
                        received_at: timestamp(received),
                        valid_until: expiry.canonical(),
                        reset_at: forecast.reset_at.canonical(),
                        remaining_percent: None,
                        remaining: None,
                        total: None,
                        unit: None,
                        reference: Some(forecast.reference),
                    };
                    match publish(channel, &b, declaration, value, finished) {
                        Ok(true) => {
                            attempt.outcome = "published".into();
                            published = true;
                        }
                        Ok(false) => {
                            attempt.outcome = "unchanged".into();
                            attempt.reason_code = Some("identical_observation".into());
                        }
                        Err(error) => {
                            attempt.outcome = "failed".into();
                            attempt.reason_code = Some(error.code);
                        }
                    }
                }
            }
        }
        attempt.head_revision_after = revision(Some(channel)).into();
    }
    if attach {
        runtime.channels.entry(key(&b)).or_default().last_attempt = Some(attempt.clone());
        if let Err(error) = write_runtime(store, &runtime) {
            return Ok(persistence_failure(
                provider,
                input,
                &before,
                &acquisition_result,
                true,
                &error,
            ));
        }
    }
    let runtime_use = use_projection(&runtime, &b, true, running);
    let code = match attempt.outcome.as_str() {
        "failed" | "unavailable" => attempt.reason_code.as_deref(),
        "superseded" => Some("inputs_changed"),
        _ => None,
    };
    Ok(
        json!({"_code":code,"provider":provider,"input_name":input,"input_change":{"outcome":if published{attempt.outcome.as_str()}else if attempt.outcome=="unchanged"{"unchanged"}else{"not_published"},
        "current_revision_before":before,"current_revision_after":attempt.head_revision_after,"published":published,"attempt_recorded":attach},"attempt":attempt,"current_runtime_use":runtime_use}),
    )
}

struct Due {
    interval: i64,
    next: Option<Instant>,
}
#[derive(Default)]
struct ScheduleState {
    due: BTreeMap<String, Due>,
    cursor: String,
}
static SCHEDULES: LazyLock<Mutex<BTreeMap<std::path::PathBuf, ScheduleState>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
pub async fn automatic_tick(store: &Store, config: &Config) -> Result<()> {
    let runtime = read_runtime(store)?;
    let mut work = Vec::new();
    let mut retained = BTreeSet::new();
    let now = Instant::now();
    {
        let mut schedules = SCHEDULES.lock().unwrap_or_else(|p| p.into_inner());
        let state = schedules.entry(store.root.clone()).or_default();
        for (p, provider) in &config.providers {
            for (name, d) in &provider.inputs {
                if let Some(interval) = d.refresh_interval_seconds {
                    if d.source == "manual" {
                        continue;
                    }
                    let b = selected_binding(&runtime, config, p, name, true)?;
                    let key = key(&b);
                    retained.insert(key.clone());
                    let entry = state.due.entry(key.clone()).or_insert(Due {
                        interval,
                        next: Some(now),
                    });
                    if entry.interval != interval {
                        entry.interval = interval;
                        entry.next = Some(now);
                    }
                    if entry.next.is_some_and(|next| now >= next) {
                        entry.next =
                            now.checked_add(std::time::Duration::from_secs(interval as u64));
                        work.push((key, p.clone(), name.clone()));
                    }
                }
            }
        }
        state.due.retain(|key, _| retained.contains(key));
        // Excess due ticks are skipped, not queued. Rotate to avoid starving a
        // channel when more declarations are due than this bounded batch admits.
        work.sort_by(|a, b| {
            (a.0 <= state.cursor)
                .cmp(&(b.0 <= state.cursor))
                .then_with(|| a.0.cmp(&b.0))
        });
        work.truncate(4);
        if let Some(last) = work.last() {
            state.cursor = last.0.clone();
        }
    }
    use futures_util::{StreamExt, stream};
    let mut pending = stream::iter(work.into_iter().map(|(_, p, name)| async move {
        acquire(store, config, &p, &name, true, "automatic", true).await
    }))
    .buffer_unordered(4);
    let mut failure = None;
    while let Some(result) = pending.next().await {
        if let Err(error) = result
            && error.code != "acquisition_in_progress"
            && failure.is_none()
        {
            failure = Some(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn intent() -> Config {
        let mut config = Config::empty();
        let mut provider = Provider {
            enabled: true,
            integration: Some("custom".into()),
            ..Default::default()
        };
        provider.endpoints.insert(
            "chat-completions".into(),
            "http://127.0.0.1:19090/v1".into(),
        );
        provider.models.insert(
            "m".into(),
            config::Model {
                enabled: true,
                ..Default::default()
            },
        );
        provider.inputs.insert("i".into(), declaration());
        config.providers.insert("p".into(), provider);
        config
    }
    fn save_intent(store: &Store, config: &Config) {
        store
            .write_raw("tmax.toml", toml::to_string(config).unwrap().as_bytes())
            .unwrap();
    }
    fn fixture() -> (tempfile::TempDir, Store, Config) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_path_buf());
        let config = intent();
        save_intent(&store, &config);
        reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        (directory, store, config)
    }
    fn retain_value(store: &Store, config: &Config) -> String {
        let _lock = store.lock("routing").unwrap();
        let mut runtime = read_runtime(store).unwrap();
        let b = super::binding(&runtime.saved, &runtime, "p", "i").unwrap();
        let channel = runtime.channels.entry(key(&b)).or_default();
        publish(
            channel,
            &b,
            &config.providers["p"].inputs["i"],
            value(),
            instant("2026-10-23T10:00:02Z").unwrap(),
        )
        .unwrap();
        let revision = revision(Some(channel)).to_owned();
        write_runtime(store, &runtime).unwrap();
        revision
    }
    fn historical_config(config: &Config) -> Value {
        let mut snapshot = serde_json::to_value(config).unwrap();
        for provider in snapshot["providers"].as_object_mut().unwrap().values_mut() {
            for model in provider["models"].as_object_mut().unwrap().values_mut() {
                model["tier"] = Value::Null;
            }
        }
        for alias in snapshot["model_aliases"].as_array_mut().unwrap() {
            alias["tier"] = Value::Null;
        }
        snapshot
    }

    #[test]
    fn historical_model_identity_maps_preserve_both_views_and_retained_facts() {
        let (_directory, store, config) = fixture();
        retain_value(&store, &config);
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let before = read_runtime(&store).unwrap();
        let expected = serde_json::to_value(&before).unwrap();
        let identity = Snapshot::capture_loaded(&store, &config, None)
            .destination_identity("p", "m")
            .unwrap();
        let mut historical = expected.clone();
        for view in ["saved", "loaded"] {
            let provider = historical[view]["providers"]["p"].as_object_mut().unwrap();
            let models = provider.remove("models").unwrap();
            provider.insert("offerings".into(), models);
        }
        store.write_json(STATE, &historical).unwrap();
        let bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(read_runtime(&store).unwrap()).unwrap(),
            expected
        );
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).destination_identity("p", "m"),
            Some(identity.clone())
        );
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).input("p", "i", &at)["availability"]["state"],
            "available"
        );
        assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), bytes);
        reconcile(&store, &config, &config, &[]).unwrap();
        assert_eq!(store.read_json::<Value>(STATE).unwrap(), expected);
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).destination_identity("p", "m"),
            Some(identity)
        );
    }

    #[test]
    fn historical_model_identity_map_rejects_ambiguous_and_duplicate_keys() {
        let (_directory, store, _config) = fixture();
        let current = serde_json::to_value(read_runtime(&store).unwrap()).unwrap();
        let mut ambiguous = current.clone();
        ambiguous["saved"]["providers"]["p"]["offerings"] =
            ambiguous["saved"]["providers"]["p"]["models"].clone();
        assert!(decode_runtime(&serde_json::to_vec(&ambiguous).unwrap()).is_err());
        let mut historical = current;
        let provider = historical["saved"]["providers"]["p"]
            .as_object_mut()
            .unwrap();
        let models = provider.remove("models").unwrap();
        provider.insert("offerings".into(), models);
        provider.insert("unexpected".into(), Value::Null);
        assert!(decode_runtime(&serde_json::to_vec(&historical).unwrap()).is_err());
        historical["saved"]["providers"]["p"]
            .as_object_mut()
            .unwrap()
            .remove("unexpected");
        let encoded = serde_json::to_string(&historical).unwrap();
        let duplicate = encoded.replacen("\"offerings\":", "\"offerings\":{},\"offerings\":", 1);
        assert!(decode_runtime(duplicate.as_bytes()).is_err());
    }

    #[test]
    fn historical_null_fields_preserve_explicit_candidates_lineages_and_input_heads() {
        let (_directory, store, original) = fixture();
        let mut config = original.clone();
        config.model_aliases.push(config::ModelAlias {
            candidate_reasoning_effort: Default::default(),
            id: "custom-name".into(),
            protocol: "chat-completions".into(),
            candidates: vec!["p/m".into()],
            enabled: true,
            continuity: None,
        });
        save_intent(&store, &config);
        reconcile(&store, &original, &config, &[]).unwrap();
        retain_value(&store, &config);
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let before = read_runtime(&store).unwrap();
        let expected = serde_json::to_value(&before).unwrap();
        let mut legacy = expected.clone();
        legacy["saved"]["config"] = historical_config(&before.saved.config);
        legacy["loaded"]["config"] = historical_config(&before.loaded.as_ref().unwrap().config);
        store.write_json(STATE, &legacy).unwrap();
        let original_bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(read_runtime(&store).unwrap()).unwrap(),
            expected
        );
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).input("p", "i", &at)["availability"]["state"],
            "available"
        );
        assert_eq!(
            std::fs::read(store.path(STATE).unwrap()).unwrap(),
            original_bytes
        );
        reconcile(&store, &config, &config, &[]).unwrap();
        assert_eq!(store.read_json::<Value>(STATE).unwrap(), expected);
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).input("p", "i", &at)["availability"]["state"],
            "available"
        );
    }

    #[test]
    fn historical_membership_uses_each_snapshot_and_keeps_empty_aliases_unavailable() {
        let (_directory, store, original) = fixture();
        let mut config = original.clone();
        config.providers.get_mut("p").unwrap().models.insert(
            "disabled".into(),
            config::Model {
                enabled: false,
                ..Default::default()
            },
        );
        config
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .insert("other".into(), config::Model::default());
        for id in ["historical", "same-source", "empty"] {
            config.model_aliases.push(config::ModelAlias {
                candidate_reasoning_effort: Default::default(),
                id: id.into(),
                protocol: "chat-completions".into(),
                candidates: vec!["p/m".into()],
                enabled: true,
                continuity: None,
            });
        }
        save_intent(&store, &config);
        reconcile(&store, &original, &config, &[]).unwrap();
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let before = read_runtime(&store).unwrap();
        let mut legacy = serde_json::to_value(&before).unwrap();
        for view in ["saved", "loaded"] {
            legacy[view]["config"] = historical_config(&config);
            let snapshot = &mut legacy[view]["config"];
            snapshot["providers"]["p"]["models"]["m"]["tier"] =
                json!(if view == "saved" { "L0" } else { "L1" });
            snapshot["providers"]["p"]["models"]["disabled"]["tier"] = json!("L0");
            snapshot["providers"]["p"]["models"]["other"]["tier"] =
                json!(if view == "saved" { "L1" } else { "L0" });
            if view == "loaded" {
                snapshot["providers"]["p"]["models"]["disabled"]["tier"] = Value::Null;
            }
            for (index, tier) in [(0, "L0"), (1, "L0"), (2, "L3")] {
                snapshot["model_aliases"][index]["tier"] = json!(tier);
                snapshot["model_aliases"][index]["candidates"] = Value::Null;
            }
        }
        store.write_json(STATE, &legacy).unwrap();
        let bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
        let mut future = config.clone();
        future
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .insert("new".into(), config::Model::default());
        future.model_aliases[0].candidates = vec!["p/new".into()];
        save_intent(&store, &future);
        let restored = read_runtime(&store).unwrap();
        assert_eq!(
            restored.saved.config.model_aliases[0].candidates,
            ["p/disabled", "p/m"]
        );
        assert_eq!(
            restored.saved.config.model_aliases[1].candidates,
            ["p/disabled", "p/m"]
        );
        assert_eq!(
            restored.loaded.as_ref().unwrap().config.model_aliases[0].candidates,
            ["p/other"]
        );
        assert!(restored.saved.config.model_aliases[2].candidates.is_empty());
        assert_eq!(
            serde_json::to_value(&restored.saved.providers).unwrap(),
            serde_json::to_value(&before.saved.providers).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&restored.loaded.as_ref().unwrap().providers).unwrap(),
            serde_json::to_value(&before.loaded.as_ref().unwrap().providers).unwrap()
        );
        assert!(config::validate(&restored.saved.config).is_err());
        assert_eq!(
            crate::routing::select(
                &store,
                &restored.saved.config,
                "empty",
                "chat-completions",
                instant("2026-10-23T10:30:00Z").unwrap()
            )
            .unwrap_err()
            .code,
            "model_unavailable"
        );
        assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), bytes);
        deactivate(&store).unwrap();
        let canonical = read_runtime(&store).unwrap();
        assert!(
            canonical.saved.config.model_aliases[2]
                .candidates
                .is_empty()
        );
        assert!(canonical.loaded.is_none());
    }

    #[test]
    fn corrupt_historical_sources_fail_closed_without_rewriting_retained_state() {
        let (_directory, store, original) = fixture();
        let mut config = original.clone();
        config.model_aliases.push(config::ModelAlias {
            candidate_reasoning_effort: Default::default(),
            id: "historical".into(),
            protocol: "chat-completions".into(),
            candidates: vec!["p/m".into()],
            enabled: true,
            continuity: None,
        });
        save_intent(&store, &config);
        reconcile(&store, &original, &config, &[]).unwrap();
        let runtime = read_runtime(&store).unwrap();
        let mut legacy = serde_json::to_value(runtime).unwrap();
        legacy["saved"]["config"] = historical_config(&config);
        for (tier, candidates) in [
            (json!("L0"), json!(["p/m"])),
            (Value::Null, Value::Null),
            (Value::Null, json!([])),
            (Value::Null, json!(["p/missing"])),
            (Value::Null, json!(["p/m", "p/m"])),
            (json!("L9"), Value::Null),
            (json!(false), Value::Null),
        ] {
            let mut invalid = legacy.clone();
            invalid["saved"]["config"]["model_aliases"][0]["tier"] = tier;
            invalid["saved"]["config"]["model_aliases"][0]["candidates"] = candidates;
            store.write_json(STATE, &invalid).unwrap();
            let bytes = std::fs::read(store.path(STATE).unwrap()).unwrap();
            assert_eq!(read_runtime(&store).unwrap_err().code, "invalid_record");
            assert_eq!(std::fs::read(store.path(STATE).unwrap()).unwrap(), bytes);
        }
        for invalid_tier in [json!("L9"), json!(true)] {
            let mut invalid = legacy.clone();
            invalid["saved"]["config"]["providers"]["p"]["models"]["m"]["tier"] = invalid_tier;
            store.write_json(STATE, &invalid).unwrap();
            assert_eq!(read_runtime(&store).unwrap_err().code, "invalid_record");
        }
        let duplicate = serde_json::to_string(&legacy).unwrap().replacen(
            "\"identity\":",
            "\"identity\":\"tampered\",\"identity\":",
            1,
        );
        store.write_raw(STATE, duplicate.as_bytes()).unwrap();
        assert_eq!(read_runtime(&store).unwrap_err().code, "invalid_record");
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(
            Snapshot::capture(&store, &config).input("p", "i", &at)["availability"]["reason_code"],
            "invalid_record"
        );
        assert_eq!(
            std::fs::read(store.path(STATE).unwrap()).unwrap(),
            duplicate.as_bytes()
        );
        legacy["saved"]["config"]["providers"]["p"]["models"]["m"]["unexpected"] = json!(true);
        store.write_json(STATE, &legacy).unwrap();
        assert_eq!(read_runtime(&store).unwrap_err().code, "invalid_record");
    }

    #[test]
    fn restored_runtime_and_catalog_survive_missing_routing_lock_without_mutation() {
        let (_directory, store, config) = fixture();
        let head = retain_value(&store, &config);
        let revision = config::raw_revision(&store).unwrap();
        activate(&store, &config, &revision).unwrap();
        let metadata = crate::catalog::Snapshot {
            revision: "accepted-restored".into(),
            ..Default::default()
        };
        store
            .write_json("catalog/accepted.json", &metadata)
            .unwrap();
        std::fs::remove_file(store.root.join("locks/routing.lock")).unwrap();
        let bytes = std::fs::read(store.root.join(STATE)).unwrap();
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        let saved = Snapshot::capture(&store, &config);
        assert_eq!(saved.metadata_revision(), Some("accepted-restored"));
        assert_eq!(saved.configuration_revision(), Some(revision.as_str()));
        assert!(saved.destination_identity("p", "m").is_some());
        assert_eq!(saved.input("p", "i", &at)["current_revision"], head);
        assert_eq!(
            saved.input("p", "i", &at)["availability"]["state"],
            "available"
        );
        let loaded = Snapshot::capture_loaded(&store, &config, Some(&metadata));
        assert_eq!(
            loaded.destination_identity("p", "m"),
            saved.destination_identity("p", "m")
        );
        assert_eq!(loaded.metadata_revision(), Some("accepted-restored"));
        assert_eq!(std::fs::read(store.root.join(STATE)).unwrap(), bytes);
        assert!(!store.root.join("locks/routing.lock").exists());
    }

    #[test]
    fn comment_only_saved_edit_reports_current_bytes_without_rewriting_runtime() {
        let (_directory, store, config) = fixture();
        let before = config::raw_revision(&store).unwrap();
        activate(&store, &config, &before).unwrap();
        let bytes = std::fs::read(store.root.join(STATE)).unwrap();
        let edited = format!(
            "# Pending manual comment\n{}",
            toml::to_string(&config).unwrap()
        );
        store.write_raw("tmax.toml", edited.as_bytes()).unwrap();
        let after = config::raw_revision(&store).unwrap();
        assert_ne!(before, after);
        assert_eq!(
            Snapshot::capture(&store, &config).configuration_revision(),
            Some(after.as_str())
        );
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).configuration_revision(),
            Some(before.as_str())
        );
        assert_eq!(std::fs::read(store.root.join(STATE)).unwrap(), bytes);
        let mut hypothetical = config.clone();
        hypothetical
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .priority = 5;
        assert!(
            Snapshot::capture(&store, &hypothetical)
                .configuration_revision()
                .is_none()
        );
    }

    #[test]
    fn loaded_capture_ignores_invalid_pending_file_and_saved_revision_is_unqualified() {
        let (_directory, store, config) = fixture();
        retain_value(&store, &config);
        let revision = config::raw_revision(&store).unwrap();
        activate(&store, &config, &revision).unwrap();
        let destination =
            Snapshot::capture_loaded(&store, &config, None).destination_identity("p", "m");
        store
            .write_raw("tmax.toml", b"this is not valid TOML [")
            .unwrap();
        let loaded = Snapshot::capture_loaded(&store, &config, None);
        assert_eq!(loaded.configuration_revision(), Some(revision.as_str()));
        assert_eq!(loaded.destination_identity("p", "m"), destination);
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(
            loaded.input("p", "i", &at)["availability"]["state"],
            "available"
        );
        assert!(
            Snapshot::capture(&store, &config)
                .configuration_revision()
                .is_none()
        );
    }

    #[test]
    fn authorization_invalidation_matches_each_saved_and_loaded_declaration() {
        let (_directory, store, config) = fixture();
        retain_value(&store, &config);
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let loaded_declaration = Snapshot::capture_loaded(&store, &config, None)
            .input_revision("p", "i")["declaration_revision"]
            .clone();
        let mut pending = config.clone();
        pending
            .providers
            .get_mut("p")
            .unwrap()
            .inputs
            .get_mut("i")
            .unwrap()
            .max_age_seconds = 1800;
        save_intent(&store, &pending);
        reconcile(&store, &config, &pending, &[]).unwrap();
        authorization_changed(&store, "p").unwrap();
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        let loaded = Snapshot::capture_loaded(&store, &config, None);
        assert_eq!(
            loaded.input_revision("p", "i")["declaration_revision"],
            loaded_declaration
        );
        assert_eq!(
            loaded.input("p", "i", &at)["availability"]["reason_code"],
            "authorization_mismatch"
        );
        assert_eq!(
            Snapshot::capture(&store, &pending).input("p", "i", &at)["availability"]["reason_code"],
            "authorization_mismatch"
        );
        let mut changed = pending.clone();
        let provider = changed.providers.get_mut("p").unwrap();
        provider.inputs.get_mut("i").unwrap().max_age_seconds = 900;
        provider.auth = Some(config::Auth::ApiKey {
            key_ref: "env:REPLACED".into(),
        });
        save_intent(&store, &changed);
        reconcile(&store, &pending, &changed, &[]).unwrap();
        assert_eq!(
            Snapshot::capture(&store, &changed).input("p", "i", &at)["availability"]["reason_code"],
            "declaration_mismatch"
        );
    }

    #[test]
    fn provider_scope_binding_does_not_require_models_and_rejects_stale_authorization() {
        let (_directory, store, config) = fixture();
        let mut no_models = config.clone();
        let provider = no_models.providers.get_mut("p").unwrap();
        provider.models.clear();
        provider.inputs.clear();
        save_intent(&store, &no_models);
        reconcile(&store, &config, &no_models, &[]).unwrap();
        let mut runtime = read_runtime(&store).unwrap();
        let scope = provider_scope_binding(&runtime, &no_models, "p").unwrap();
        assert_eq!(scope.0, runtime.saved.providers["p"].identity);
        runtime
            .authorization
            .insert(scope.0, "replaced-generation".into());
        assert!(provider_scope_binding_in_view(&runtime, &runtime.saved, "p").is_none());
    }

    #[test]
    fn saved_publication_guard_follows_explicit_rename_but_not_recreated_provider() {
        let (_directory, store, config) = fixture();
        let head_revision = retain_value(&store, &config);
        let runtime = read_runtime(&store).unwrap();
        let captured = super::binding(&runtime.saved, &runtime, "p", "i").unwrap();
        let original = runtime.channels[&key(&captured)]
            .head
            .as_ref()
            .unwrap()
            .value
            .clone()
            .unwrap();
        let mut renamed = config.clone();
        let provider = renamed.providers.remove("p").unwrap();
        renamed.providers.insert("renamed".into(), provider);
        save_intent(&store, &renamed);
        reconcile(&store, &config, &renamed, &[("p".into(), "renamed".into())]).unwrap();
        let mut runtime = read_runtime(&store).unwrap();
        let current = current_saved_binding(&store, &runtime, &captured)
            .unwrap()
            .unwrap();
        assert_eq!(current.provider, "renamed");
        let after = revision(runtime.channels.get(&key(&current)));
        assert_eq!(
            guard_reason(&captured, Some(&current), &head_revision, after),
            None
        );
        let mut retry = original.clone();
        retry.received_at = "2026-10-23T10:10:00Z".into();
        let channel = runtime.channels.get_mut(&key(&current)).unwrap();
        assert!(
            !publish(
                channel,
                &current,
                &renamed.providers["renamed"].inputs["i"],
                retry,
                instant("2026-10-23T10:10:00Z").unwrap()
            )
            .unwrap()
        );
        let retained = channel.head.as_ref().unwrap().value.as_ref().unwrap();
        assert_eq!(retained.observed_at, original.observed_at);
        assert_eq!(retained.received_at, original.received_at);
        assert_eq!(retained.valid_until, original.valid_until);
        save_intent(&store, &Config::empty());
        reconcile(&store, &renamed, &Config::empty(), &[]).unwrap();
        save_intent(&store, &renamed);
        reconcile(&store, &Config::empty(), &renamed, &[]).unwrap();
        let runtime = read_runtime(&store).unwrap();
        let current = current_saved_binding(&store, &runtime, &captured).unwrap();
        assert!(current.is_none());
        assert_eq!(
            guard_reason(&captured, current.as_ref(), &head_revision, "absent"),
            Some("declaration_removed")
        );
    }

    #[test]
    fn fractional_instants_remain_exact_beyond_nanoseconds() {
        let a = ExactInstant::parse("2026-10-23T10:00:00.000000000100000Z").unwrap();
        let equivalent = ExactInstant::parse("2026-10-23T12:00:00.0000000001+02:00").unwrap();
        let later = ExactInstant::parse("2026-10-23T10:00:00.0000000002Z").unwrap();
        assert_eq!(a, equivalent);
        assert!(a < later);
        assert_eq!(a.canonical(), "2026-10-23T10:00:00.0000000001Z");
        let mut channel = Channel::default();
        let d = declaration();
        let b = binding();
        let mut first = value();
        first.observed_at = a.canonical();
        publish(
            &mut channel,
            &b,
            &d,
            first,
            instant("2026-10-23T10:00:02Z").unwrap(),
        )
        .unwrap();
        let mut second = value();
        second.observed_at = later.canonical();
        second.remaining = Some("2".into());
        assert!(
            publish(
                &mut channel,
                &b,
                &d,
                second,
                instant("2026-10-23T10:00:03Z").unwrap()
            )
            .unwrap()
        );
        let between = ExactInstant::parse("2026-10-23T10:00:00.00000000015Z").unwrap();
        assert_eq!(
            availability_checked(channel.head.as_ref(), &b, &between, true, Some(&d))["reason_code"],
            "not_yet_observed"
        );
        let mut retry = value();
        retry.observed_at = "2026-10-23T12:00:00.000000000200000+02:00".into();
        retry.remaining = Some("0002.000".into());
        assert!(
            !publish(
                &mut channel,
                &b,
                &d,
                retry,
                instant("2026-10-23T10:00:04Z").unwrap()
            )
            .unwrap()
        );
    }
    #[test]
    fn malformed_record_precedes_binding_mismatch_and_nulls_are_required() {
        let mut channel = Channel::default();
        let d = declaration();
        let b = binding();
        publish(
            &mut channel,
            &b,
            &d,
            value(),
            instant("2026-10-23T10:00:02Z").unwrap(),
        )
        .unwrap();
        let original = serde_json::to_value(channel.head.as_ref().unwrap()).unwrap();
        assert!(serde_json::from_value::<InputHead>(original.clone()).is_ok());
        let mut missing = original.clone();
        missing
            .as_object_mut()
            .unwrap()
            .remove("unavailable_reason");
        assert!(serde_json::from_value::<InputHead>(missing).is_err());
        let mut unknown = original;
        unknown["source_body"] = json!("never accepted");
        assert!(serde_json::from_value::<InputHead>(unknown).is_err());
        channel
            .head
            .as_mut()
            .unwrap()
            .value
            .as_mut()
            .unwrap()
            .remaining = Some("NaN".into());
        let mut changed = b;
        changed.declaration_revision = "replacement".into();
        assert_eq!(
            availability(
                channel.head.as_ref(),
                &changed,
                &d,
                instant("2026-10-23T13:00:00Z").unwrap()
            )["reason_code"],
            "invalid_record"
        );
    }
    #[test]
    fn provider_rename_preserves_identity_but_model_coverage_rename_invalidates() {
        let (_directory, store, config) = fixture();
        let revision = retain_value(&store, &config);
        let before = Snapshot::capture(&store, &config)
            .destination_identity("p", "m")
            .unwrap();
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let mut renamed = config.clone();
        let provider = renamed.providers.remove("p").unwrap();
        renamed.providers.insert("renamed".into(), provider);
        save_intent(&store, &renamed);
        reconcile(&store, &config, &renamed, &[("p".into(), "renamed".into())]).unwrap();
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        let snapshot = Snapshot::capture(&store, &renamed);
        assert_eq!(
            snapshot.destination_identity("renamed", "m").unwrap(),
            before
        );
        assert_eq!(
            snapshot.input("renamed", "i", &at)["current_revision"],
            revision
        );
        assert_eq!(
            snapshot.input("renamed", "i", &at)["availability"]["state"],
            "available"
        );
        let mut model_renamed = renamed.clone();
        let p = model_renamed.providers.get_mut("renamed").unwrap();
        let model = p.models.remove("m").unwrap();
        p.models.insert("new-m".into(), model);
        p.inputs.get_mut("i").unwrap().models = vec!["new-m".into()];
        save_intent(&store, &model_renamed);
        reconcile(
            &store,
            &renamed,
            &model_renamed,
            &[("renamed/m".into(), "renamed/new-m".into())],
        )
        .unwrap();
        let after = Snapshot::capture(&store, &model_renamed);
        assert_eq!(
            after.destination_identity("renamed", "new-m").unwrap().2,
            before.2
        );
        assert_eq!(
            after.input("renamed", "i", &at)["availability"]["reason_code"],
            "declaration_mismatch"
        );
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).input("p", "i", &at)["availability"]["state"],
            "available"
        );
    }
    #[test]
    fn pending_declarations_keep_only_saved_and_loaded_heads() {
        let (_directory, store, config) = fixture();
        retain_value(&store, &config);
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let old_snapshot = Snapshot::capture_loaded(&store, &config, None);
        let mut changed = config.clone();
        changed
            .providers
            .get_mut("p")
            .unwrap()
            .inputs
            .get_mut("i")
            .unwrap()
            .max_age_seconds = 1800;
        save_intent(&store, &changed);
        reconcile(&store, &config, &changed, &[]).unwrap();
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(read_runtime(&store).unwrap().channels.len(), 2);
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None).input("p", "i", &at)["availability"]["state"],
            "available"
        );
        assert_eq!(
            Snapshot::capture(&store, &changed).input("p", "i", &at)["availability"]["reason_code"],
            "declaration_mismatch"
        );
        activate(&store, &changed, &config::raw_revision(&store).unwrap()).unwrap();
        assert_eq!(read_runtime(&store).unwrap().channels.len(), 1);
        assert_eq!(
            old_snapshot.input("p", "i", &at)["availability"]["state"],
            "available"
        );
        save_intent(&store, &Config::empty());
        reconcile(&store, &changed, &Config::empty(), &[]).unwrap();
        deactivate(&store).unwrap();
        assert!(read_runtime(&store).unwrap().channels.is_empty());
    }
    #[test]
    fn identical_saved_and_loaded_text_never_transfers_recreated_provider_identity() {
        let (_directory, store, config) = fixture();
        retain_value(&store, &config);
        activate(&store, &config, &config::raw_revision(&store).unwrap()).unwrap();
        let old = Snapshot::capture_loaded(&store, &config, None)
            .destination_identity("p", "m")
            .unwrap();
        save_intent(&store, &Config::empty());
        reconcile(&store, &config, &Config::empty(), &[]).unwrap();
        save_intent(&store, &config);
        reconcile(&store, &Config::empty(), &config, &[]).unwrap();
        let saved = Snapshot::capture(&store, &config)
            .destination_identity("p", "m")
            .unwrap();
        assert_ne!(old, saved);
        assert_eq!(
            Snapshot::capture_loaded(&store, &config, None)
                .destination_identity("p", "m")
                .unwrap(),
            old
        );
    }
    #[test]
    fn interval_and_priority_edits_preserve_declaration_but_authorization_does_not() {
        let (_directory, store, mut config) = fixture();
        retain_value(&store, &config);
        let original = Snapshot::capture(&store, &config).input_revision("p", "i");
        let mut changed = config.clone();
        changed
            .providers
            .get_mut("p")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .priority = -99;
        save_intent(&store, &changed);
        reconcile(&store, &config, &changed, &[]).unwrap();
        assert_eq!(
            original,
            Snapshot::capture(&store, &changed).input_revision("p", "i")
        );
        authorization_changed(&store, "p").unwrap();
        let at = ExactInstant::parse("2026-10-23T10:30:00Z").unwrap();
        assert_eq!(
            Snapshot::capture(&store, &changed).input("p", "i", &at)["availability"]["reason_code"],
            "authorization_mismatch"
        );
        config = changed;
        let mut fetched = config.clone();
        fetched
            .providers
            .get_mut("p")
            .unwrap()
            .inputs
            .get_mut("i")
            .unwrap()
            .source = "provider".into();
        save_intent(&store, &fetched);
        reconcile(&store, &config, &fetched, &[]).unwrap();
        let before = Snapshot::capture(&store, &fetched).input_revision("p", "i");
        let mut scheduled = fetched.clone();
        scheduled
            .providers
            .get_mut("p")
            .unwrap()
            .inputs
            .get_mut("i")
            .unwrap()
            .refresh_interval_seconds = Some(60);
        save_intent(&store, &scheduled);
        reconcile(&store, &fetched, &scheduled, &[]).unwrap();
        assert_eq!(
            before,
            Snapshot::capture(&store, &scheduled).input_revision("p", "i")
        );
    }
    #[test]
    fn clear_wins_over_in_flight_acquisition_and_attempt_exclusion_is_shared() {
        let (_directory, store, config) = fixture();
        let before = retain_value(&store, &config);
        let runtime = read_runtime(&store).unwrap();
        let b = super::binding(&runtime.saved, &runtime, "p", "i").unwrap();
        let acquisition = acquisition_lock(&store, &b).unwrap();
        assert_eq!(store.is_locked(&acquisition_name(&b)).unwrap(), Some(true));
        assert_eq!(
            acquisition_lock(&store, &b).err().unwrap().code,
            "acquisition_in_progress"
        );
        let writer = store.clone();
        let captured = b.clone();
        std::thread::spawn(move || {
            let _lock = writer.lock("routing").unwrap();
            let mut state = read_runtime(&writer).unwrap();
            let channel_key = key(&captured);
            state.channels.get_mut(&channel_key).unwrap().head = Some(InputHead {
                binding: captured,
                revision: token(),
                observation_floor: None,
                value: None,
                unavailable_reason: Some("cleared".into()),
            });
            write_runtime(&writer, &state).unwrap();
        })
        .join()
        .unwrap();
        let current = read_runtime(&store).unwrap();
        let now = super::binding(&current.saved, &current, "p", "i").unwrap();
        assert_eq!(
            guard_reason(
                &b,
                Some(&now),
                &before,
                revision(current.channels.get(&key(&now)))
            ),
            Some("input_changed")
        );
        assert!(current.channels[&key(&b)].last_attempt.is_none());
        drop(acquisition);
        assert_eq!(store.is_locked(&acquisition_name(&b)).unwrap(), Some(false));
    }
    #[tokio::test]
    async fn dry_run_validates_manual_data_without_reserving_runtime_state() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_path_buf());
        let config = intent();
        save_intent(&store, &config);
        let now = Utc::now();
        let mut arguments = BTreeMap::new();
        for (name, value) in [
            ("observed-at", timestamp(now - Duration::seconds(10))),
            ("valid-until", timestamp(now + Duration::minutes(10))),
            ("reset-at", timestamp(now + Duration::hours(1))),
            ("remaining", "1".into()),
            ("total", "3".into()),
            ("unit", "requests".into()),
        ] {
            arguments.insert(name.into(), vec![value]);
        }
        let request = Request {
            command: vec!["provider".into(), "input".into(), "record".into()],
            operands: vec!["p".into(), "i".into()],
            arguments,
            dry_run: true,
            if_revision: None,
        };
        let result = handle(&store, &config, &request).await.unwrap();
        assert_eq!(result["input_change"]["outcome"], "planned");
        assert!(result["proposed_data"].get("received_at").is_none());
        assert!(!store.root.join(STATE).exists());
        assert!(!store.root.join("locks").exists());
    }
    fn declaration() -> InputDeclaration {
        InputDeclaration {
            kind: "quota".into(),
            source: "manual".into(),
            scope: "weekly".into(),
            models: vec!["m".into()],
            max_age_seconds: 3600,
            refresh_interval_seconds: None,
        }
    }
    fn binding() -> Binding {
        Binding {
            provider: "p".into(),
            input: "i".into(),
            provider_identity: "p1".into(),
            authorization_generation: "a1".into(),
            declaration_revision: "d1".into(),
            kind: "quota".into(),
            scope: "weekly".into(),
            models: vec!["m".into()],
            source: "manual".into(),
        }
    }
    fn value() -> InputValue {
        InputValue {
            observed_at: "2026-10-23T10:00:00Z".into(),
            observed_at_basis: "manual".into(),
            received_at: "2026-10-23T10:00:01Z".into(),
            valid_until: "2026-10-23T11:00:00Z".into(),
            reset_at: "2026-10-23T12:00:00Z".into(),
            remaining_percent: None,
            remaining: Some("1".into()),
            total: Some("3".into()),
            unit: Some("requests".into()),
            reference: None,
        }
    }
    #[test]
    fn recurring_ratio_is_not_rounded() {
        let basis = json!({"remaining":"1","total":"3"});
        assert!(quota_matches(&basis, "33.33").unwrap());
        assert!(!quota_matches(&basis, "33.34").unwrap());
        assert!(
            quota_matches(
                &basis,
                "33.333333333333333333333333333333333333333333333333333333"
            )
            .unwrap()
        );
    }
    #[test]
    fn lexical_variants_are_idempotent_without_freshness_extension() {
        let mut c = Channel::default();
        let d = declaration();
        let b = binding();
        let at = instant("2026-10-23T10:00:02Z").unwrap();
        assert!(publish(&mut c, &b, &d, value(), at).unwrap());
        let revision = revision(Some(&c)).to_owned();
        let mut retry = value();
        retry.observed_at = "2026-10-23T12:00:00.000+02:00".into();
        retry.remaining = Some("0001.000".into());
        retry.received_at = "2026-10-23T10:10:00Z".into();
        assert!(
            !publish(
                &mut c,
                &b,
                &d,
                retry,
                instant("2026-10-23T11:30:00Z").unwrap()
            )
            .unwrap()
        );
        assert_eq!(revision, super::revision(Some(&c)));
        assert_eq!(
            c.head.unwrap().value.unwrap().received_at,
            "2026-10-23T10:00:01Z"
        );
    }
    #[test]
    fn same_instant_conflicts_and_expiry_is_exclusive() {
        let mut c = Channel::default();
        let d = declaration();
        let b = binding();
        publish(
            &mut c,
            &b,
            &d,
            value(),
            instant("2026-10-23T10:00:02Z").unwrap(),
        )
        .unwrap();
        let mut conflict = value();
        conflict.remaining = Some("2".into());
        assert_eq!(
            publish(
                &mut c,
                &b,
                &d,
                conflict,
                instant("2026-10-23T10:00:03Z").unwrap()
            )
            .unwrap_err()
            .code,
            "conflicting_observation"
        );
        assert_eq!(
            availability(
                c.head.as_ref(),
                &b,
                &d,
                instant("2026-10-23T11:00:00Z").unwrap()
            )["reason_code"],
            "expired"
        );
    }
    #[test]
    fn qualified_markdown_rejects_ambiguity_and_quoted_forecasts() {
        let page = "# Codex Resets\n\n## Reset scheduled\n\n**in 9 hours** · `2026-10-23T17:00:00.000Z` · Upcoming reset · Regular reset\n\n> `2026-10-24T00:00:00Z` · Upcoming reset\n\n[View announcement](https://x.com/thsottiaux/status/123)\n\n## History\n\n`2020-01-01T00:00:00Z`\n";
        assert_eq!(
            parse_markdown(page).unwrap().reset_at.canonical(),
            "2026-10-23T17:00:00Z"
        );
        assert!(parse_markdown(&page.replace("## History", "## Reset scheduled")).is_err());
        assert!(
            parse_markdown(&page.replace(" · Regular reset", " · `2026-10-24T00:00:00Z`")).is_err()
        );
        assert!(parse_markdown(&page.replace("status/123", "status/123?token=x")).is_err());
        assert!(parse_markdown(&page.replace("https://x.com/", "https://x.com:443/")).is_err());
    }
}
