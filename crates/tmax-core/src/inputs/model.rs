use crate::config::Config;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

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
    pub(super) head: Option<InputHead>,
    pub(super) last_attempt: Option<AcquisitionAttempt>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderIdentity {
    pub identity: String,
    pub authorization: String,
    pub(super) auth_definition: Value,
    pub models: BTreeMap<String, String>,
    pub(super) declarations: BTreeMap<String, DeclarationIdentity>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeclarationIdentity {
    pub(super) revision: String,
    pub(super) definition: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct View {
    pub config: Config,
    pub providers: BTreeMap<String, ProviderIdentity>,
    #[serde(default)]
    pub(super) revision: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Runtime {
    pub saved: View,
    pub loaded: Option<View>,
    pub(super) channels: BTreeMap<String, Channel>,
    pub(super) authorization: BTreeMap<String, String>,
}

pub(super) fn revision(channel: Option<&Channel>) -> &str {
    channel
        .and_then(|c| c.head.as_ref())
        .map(|h| h.revision.as_str())
        .unwrap_or("absent")
}
