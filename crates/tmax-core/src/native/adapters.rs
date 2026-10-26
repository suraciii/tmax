//! Independent, version-specific native configuration mappings.
//! Evidence is pinned to the concrete installations below, not a version range.
use super::document::Format;
use crate::{
    Error, Result,
    config::{AgentTarget, ModelReference},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(super) const AGENTS: [&str; 6] = ["codex", "pi", "omp", "opencode", "hermes", "dsh"];
pub fn supported_roles(agent: &str) -> &'static [&'static str] {
    match agent {
        "codex" => &["review"],
        "pi" => &[],
        "omp" => &[
            "smol", "slow", "vision", "plan", "commit", "tiny", "memory", "task", "advisor",
            "widget", "judge",
        ],
        "opencode" => &[
            "build",
            "plan",
            "general",
            "explore",
            "compaction",
            "title",
            "summary",
        ],
        "hermes" => &[
            "vision",
            "compression",
            "skills_hub",
            "approval",
            "review",
            "mcp",
            "title_generation",
            "memory_query_rewrite",
            "tts_audio_tags",
            "voice_chat",
            "triage_specifier",
            "kanban_decomposer",
            "profile_describer",
            "goal_judge",
            "curator",
            "monitor",
            "background_review",
            "moa_reference",
            "moa_aggregator",
        ],
        _ => &[],
    }
}
pub(super) fn qualified_version(agent: &str) -> &'static str {
    match agent {
        "codex" => "0.162.1",
        "pi" => "1.2.0",
        "omp" => "18.12.6",
        "opencode" => "2.0.28",
        "hermes" => "0.22.1+683.g848fc0a",
        "dsh" => "0.2.1-rc.2",
        _ => "",
    }
}
pub(super) fn source(agent: &str) -> &'static str {
    match agent {
        "codex" => "https://github.com/openai/codex/tree/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d",
        "pi" => {
            "https://github.com/earendil-works/pi/tree/06a466935536f8c9bcacb2d5efc0fc021be1bdff"
        }
        "omp" => "https://github.com/can1357/oh-my-pi/tree/v18.12.6",
        "opencode" => {
            "https://github.com/anomalyco/opencode/tree/026b106bc5d41b679ffc413264229c340871e4d3"
        }
        "hermes" => "https://github.com/NousResearch/hermes-agent/tree/848fc0a",
        "dsh" => {
            "https://github.com/deepseek-ai/deepseek-harness/tree/687f282e8625c6b8d11ff58211a90524be2e4317"
        }
        _ => "",
    }
}
pub(super) fn precedence(agent: &str) -> Value {
    match agent {
        "codex" => {
            json!({"scope":"CODEX_HOME user configuration; CLI and IDE share authentication", "order":["managed requirements and restrictions", "CLI overrides", "selected adjacent profile.config.toml", "user config.toml", "trusted project settings (provider/auth keys excluded)"], "launch_environment":"CODEX_HOME", "native_reload":"Restart Codex and other consumers of this CODEX_HOME; existing sessions may retain their model"})
        }
        "pi" => {
            json!({"scope":"PI_CODING_AGENT_DIR", "order":["CLI and session selections", "trusted project settings/extensions", "agent settings.json and models.json"], "credentials":["runtime api-key", "native auth.json", "models.json apiKey", "provider environment"], "launch_environment":"PI_CODING_AGENT_DIR", "native_reload":"Open /model to reload models; restart for persisted startup defaults"})
        }
        "omp" => {
            json!({"scope":"active profile agent directory", "order":["CLI and session selections", "extra --config overlays", "project .omp/config.yml", "active profile config.yml"], "models":["models.yml", "models.yaml", "legacy models.json migration"], "credentials":"runtime api-key precedes a pinned models.yml apiKey, which precedes stored OAuth and login keys; extension fallback, environment, and other stored keys follow; account restrictions still apply", "launch_environment":"PI_CODING_AGENT_DIR with OMP_PROFILE unset for explicit default-profile directory", "native_reload":"Restart omp in the selected profile; existing sessions retain selections"})
        }
        "opencode" => {
            json!({"scope":"global config directory / explicit project config directory; accounts are server-scoped", "order_direction":"highest_to_lowest", "order":["inline configuration", "project .opencode files (cwd toward root)", "project direct files (cwd toward root)", "explicit configuration file", "global configuration", "well-known configuration", "system configuration", "MDM preferences"], "policy_evaluation":"File-based policy evaluation reverses ordinary configuration order; MDM preferences and system policy retain their separate authority", "credentials":"saved server account precedes environment; explicit settings.apiKey pins a custom package key", "launch_environment":"OPENCODE_CONFIG_DIR; an isolated server also needs explicit OPENCODE_DB", "native_reload":"Run opencode reload on the intended server or start a private --standalone server; tmax does not restart a shared server"})
        }
        "hermes" => {
            json!({"scope":"explicit HERMES_HOME, including an explicitly selected profile", "order":["managed scope", "CLI/environment model overrides", "selected profile config.yaml"], "credentials":"native pools and external login adoption can affect built-in providers; custom entries use the selected literal key", "launch_environment":"HERMES_HOME", "native_reload":"Restart Hermes in the selected HERMES_HOME; active sessions may retain model settings"})
        }
        "dsh" => {
            json!({"scope":"DSH_HOME common configuration", "order":["CLI --patch overlays and flag-derived patches", "DSH_HOME/cordis.patch.yml", "native profile and bundle patches"], "credentials":"native DSH_HOME/.credentials.yaml refs precede environment fallback for the selected apiKeyEnv reference", "launch_environment":"DSH_HOME; profile selection remains DSH-owned", "native_reload":"Restart DSH consumers of this home; active sessions may retain their selections"})
        }
        _ => Value::Null,
    }
}
#[derive(Debug)]
pub(super) struct Entry {
    pub name: String,
    pub base: String,
    pub protocol: String,
    pub key: Option<String>,
    pub login_provider: Option<String>,
    pub canonical: Option<String>,
    pub gateway: bool,
    pub models: Vec<String>,
    pub model_routes: BTreeMap<String, (String, String)>,
    pub model_metadata: BTreeMap<String, Value>,
    pub model_projection: BTreeMap<String, Value>,
}
impl Entry {
    pub(super) fn native_name(&self) -> &str {
        self.login_provider
            .as_deref()
            .or(self.canonical.as_deref())
            .unwrap_or(&self.name)
    }
}
const PI_THINKING_LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
const OMP_EFFORT_LEVELS: [&str; 6] = ["minimal", "low", "medium", "high", "xhigh", "max"];
// DSH 0.2.1-rc.2 catalog.ts THINKING_LEVEL_GATE and resolveModelReasoning;
// independently qualified from Pi's thinkingLevelMap carrier.
const DSH_EFFORT_LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

// Reasoning options are capability declarations. They are projected into the
// native agent's own selector vocabulary; they never choose a default level or
// rewrite a request. Inexact names are not translated.
fn effort_values(effective: &Value) -> Option<Vec<String>> {
    if effective.get("reasoning").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let options = effective.get("reasoning_options")?.as_array()?;
    let mut values: Vec<String> = Vec::new();
    for option in options {
        if option.get("type").and_then(Value::as_str) != Some("effort") {
            continue;
        }
        let Some(list) = option.get("values").and_then(Value::as_array) else {
            continue;
        };
        for value in list {
            let Some(value) = value.as_str().filter(|value| !value.is_empty()) else {
                continue;
            };
            if !values.iter().any(|existing| existing == value) {
                values.push(value.to_owned());
            }
        }
    }
    (!values.is_empty()).then_some(values)
}

struct ReasoningProjection {
    state: &'static str,
    reason_code: Option<&'static str>,
    fields: Value,
    omitted: Vec<String>,
}

fn project_reasoning(
    agent: &str,
    protocol: &str,
    values: &[String],
) -> Result<ReasoningProjection> {
    match agent {
        "pi" => {
            let mut map = serde_json::Map::new();
            let mut positive = false;
            let mut represented: Vec<&str> = Vec::new();
            for level in PI_THINKING_LEVELS {
                if values.iter().any(|value| value == level) {
                    map.insert(level.to_owned(), json!(level));
                    represented.push(level);
                    positive |= level != "off";
                } else {
                    map.insert(level.to_owned(), Value::Null);
                }
            }
            if !positive {
                return Err(Error::new(
                    "native_metadata_unrepresentable",
                    "This model's declared reasoning efforts cannot be represented by Pi's thinking levels; adjust reasoning_options or leave native reasoning metadata unmanaged",
                )
                .field("reasoning_options"));
            }
            let omitted = values
                .iter()
                .filter(|value| !represented.contains(&value.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            Ok(ReasoningProjection {
                state: if omitted.is_empty() {
                    "projected"
                } else {
                    "partial"
                },
                reason_code: if omitted.is_empty() {
                    None
                } else {
                    Some("native_reasoning_options_partial")
                },
                fields: json!({"thinkingLevelMap": map}),
                omitted,
            })
        }
        "omp" => {
            let efforts = values
                .iter()
                .filter(|value| OMP_EFFORT_LEVELS.contains(&value.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            if efforts.is_empty() {
                return Err(Error::new(
                    "native_metadata_unrepresentable",
                    "This model's declared reasoning efforts cannot be represented by OMP's thinking effort levels; adjust reasoning_options or leave native reasoning metadata unmanaged",
                )
                .field("reasoning_options"));
            }
            let omitted = values
                .iter()
                .filter(|value| !OMP_EFFORT_LEVELS.contains(&value.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            Ok(ReasoningProjection {
                state: if omitted.is_empty() {
                    "projected"
                } else {
                    "partial"
                },
                reason_code: if omitted.is_empty() {
                    None
                } else {
                    Some("native_reasoning_options_partial")
                },
                fields: json!({"thinking": {"mode": "effort", "efforts": efforts}}),
                omitted,
            })
        }
        "dsh" => {
            let mut efforts = serde_json::Map::new();
            for level in DSH_EFFORT_LEVELS {
                if values.iter().any(|value| value == level) {
                    efforts.insert(level.to_owned(), json!(level));
                }
            }
            if !efforts.keys().any(|level| level != "off") {
                return Err(Error::new(
                    "native_metadata_unrepresentable",
                    "This model's declared reasoning efforts cannot be represented by DSH's reasoningEfforts; declare an exact supported level beyond off or leave native reasoning metadata unmanaged",
                )
                .field("reasoning_options"));
            }
            let omitted = values
                .iter()
                .filter(|value| !DSH_EFFORT_LEVELS.contains(&value.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            Ok(ReasoningProjection {
                state: if omitted.is_empty() {
                    "projected"
                } else {
                    "partial"
                },
                reason_code: if omitted.is_empty() {
                    None
                } else {
                    Some("native_reasoning_options_partial")
                },
                fields: json!({"reasoningEfforts": efforts}),
                omitted,
            })
        }
        "opencode" => {
            let setting = match protocol {
                "chat-completions" | "responses" => "reasoningEffort",
                "messages" => "effort",
                _ => {
                    return Err(Error::new(
                        "incompatible",
                        "Unknown native reasoning protocol",
                    ));
                }
            };
            Ok(ReasoningProjection {
                state: "projected",
                reason_code: None,
                fields: json!({"variants": values.iter().map(|value| json!({
                    "id": value,
                    "settings": {setting: value},
                })).collect::<Vec<_>>()}),
                omitted: Vec::new(),
            })
        }
        _ => Ok(ReasoningProjection {
            state: "not_supported",
            reason_code: Some("native_reasoning_options_unavailable"),
            fields: json!({}),
            omitted: Vec::new(),
        }),
    }
}

pub(super) fn projection_view(agent: &str, protocol: &str, effective: &Value) -> Result<Value> {
    if matches!(agent, "codex" | "hermes") {
        return Ok(
            json!({"state":"not_supported","reason_code":"native_reasoning_options_unavailable","fields":{},"omitted":[]}),
        );
    }
    let Some(values) = effort_values(effective) else {
        let reason = if effective.get("reasoning").and_then(Value::as_bool) == Some(false) {
            "reasoning_disabled"
        } else {
            "reasoning_options_unavailable"
        };
        return Ok(json!({"state":"unavailable","reason_code":reason,"fields":{},"omitted":[]}));
    };
    let projection = project_reasoning(agent, protocol, &values)?;
    Ok(json!({
        "state": projection.state,
        "reason_code": projection.reason_code,
        "fields": projection.fields,
        "omitted": projection.omitted,
    }))
}

fn catalog_model_metadata(agent: &str, effective: &Value) -> Result<Value> {
    let mut metadata = json!({});
    for (source, native) in [("context", "contextWindow"), ("output", "maxTokens")] {
        // DSH catalog.ts PiAiModelProfile.maxTokens also chooses a per-request
        // default. Leave output capacity unmanaged rather than change requests.
        if agent == "dsh" && source == "output" {
            continue;
        }
        if let Some(value) = effective.get("limit").and_then(|limit| limit.get(source))
            && !value.is_null()
        {
            // Qualified Pi, OMP, and DSH schemas reject nonpositive limits.
            // JS numbers must retain the exact integer.
            let count = value.as_u64().filter(|count| *count > 0 && *count <= 9_007_199_254_740_991)
                .ok_or_else(|| Error::new(
                    "native_metadata_unrepresentable",
                    "This concrete model limit cannot be represented exactly by the qualified native schema; select a representable explicit limit before applying",
                ).field(format!("limit.{source}")))?;
            metadata[native] = json!(count);
        }
    }
    if let Some(reasoning) = effective.get("reasoning").and_then(Value::as_bool) {
        metadata["reasoning"] = json!(reasoning);
    }
    if let Some(input) = effective
        .pointer("/modalities/input")
        .and_then(Value::as_array)
    {
        // Qualified native schemas support only text and image, not the
        // provider's entire modality vocabulary.
        metadata["input"] = json!(
            input
                .iter()
                .filter_map(Value::as_str)
                .filter(|modality| matches!(*modality, "text" | "image"))
                .collect::<Vec<_>>()
        );
    }
    Ok(metadata)
}
// Hermes consumes route context and catalog capability patches separately.
// `model_overrides` here is an intermediate projection, never a provider-model key.
pub(super) fn model_metadata(agent: &str, protocol: &str, effective: &Value) -> Result<Value> {
    let mut metadata = if matches!(agent, "pi" | "omp" | "dsh") {
        catalog_model_metadata(agent, effective)?
    } else {
        json!({})
    };
    if agent == "dsh" {
        // DSH accepts the same positive integer capacities and text/image
        // modalities, but represents reasoning capability only through efforts.
        metadata
            .as_object_mut()
            .expect("native model metadata object")
            .remove("reasoning");
        if effective.get("reasoning").and_then(Value::as_bool) == Some(false) {
            metadata["reasoningEfforts"] = json!(false);
        }
        // Unlike Pi/OMP, DSH treats [] as inheritance, not an explicit empty
        // capability. Refuse rather than silently acquire native modalities.
        if metadata
            .get("input")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            return Err(Error::new(
                "native_metadata_unrepresentable",
                "DSH treats an empty input list as native inheritance; this declared input capability cannot be represented",
            ).field("modalities.input"));
        }
    }
    if agent == "opencode" {
        for field in ["context", "input", "output"] {
            if let Some(value) = effective.get("limit").and_then(|limit| limit.get(field))
                && !value.is_null()
            {
                let count = value.as_u64().filter(|count| *count <= 9_007_199_254_740_991)
                    .ok_or_else(|| Error::new(
                        "native_metadata_unrepresentable",
                        "This concrete model limit cannot be represented exactly by OpenCode's integer schema",
                    ).field(format!("limit.{field}")))?;
                metadata["limit"][field] = json!(count);
            }
        }
        if let Some(tools) = effective.get("tool_call").and_then(Value::as_bool) {
            metadata["capabilities"]["tools"] = json!(tools);
        }
        for field in ["input", "output"] {
            if let Some(modalities) = effective.get("modalities").and_then(|m| m.get(field))
                && modalities.is_array()
            {
                metadata["capabilities"][field] = modalities.clone();
            }
        }
    } else if agent == "hermes" {
        if let Some(value) = effective.pointer("/limit/context")
            && !value.is_null()
        {
            let count = value.as_u64().filter(|count| *count > 0).ok_or_else(|| {
                Error::new(
                    "native_metadata_unrepresentable",
                    "Hermes requires a positive concrete model context length",
                )
                .field("limit.context")
            })?;
            metadata["context_length"] = json!(count);
            metadata["model_overrides"]["context_window"] = json!(count);
        }
        for (source, native) in [
            ("reasoning", "supports_reasoning"),
            ("tool_call", "supports_tools"),
        ] {
            if let Some(value) = effective.get(source).and_then(Value::as_bool) {
                metadata["model_overrides"][native] = json!(value);
            }
        }
        if let Some(input) = effective
            .pointer("/modalities/input")
            .and_then(Value::as_array)
        {
            metadata["model_overrides"]["supports_vision"] = json!(
                input
                    .iter()
                    .any(|modality| modality.as_str() == Some("image"))
            );
        }
    }
    if matches!(agent, "pi" | "omp" | "opencode" | "dsh")
        && let Some(values) = effort_values(effective)
    {
        let projection = project_reasoning(agent, protocol, &values)?;
        if let Value::Object(fields) = projection.fields {
            metadata
                .as_object_mut()
                .expect("native model metadata object")
                .extend(fields);
        }
    }
    Ok(metadata)
}
#[derive(Debug)]
pub(super) struct File {
    pub path: PathBuf,
    pub format: Format,
    pub desired: Value,
}
fn file(scope: &Path, name: &str, format: Format, desired: Value) -> File {
    File {
        path: scope.join(name),
        format,
        desired,
    }
}
fn reference(r: &ModelReference, entries: &[Entry]) -> Result<String> {
    let entry = entries
        .iter()
        .find(|e| e.name == r.provider)
        .ok_or_else(|| {
            Error::new(
                "unresolved_reference",
                "Native model selection does not resolve",
            )
        })?;
    Ok(format!("{}/{}", entry.native_name(), r.id))
}
fn api(protocol: &str) -> Result<&'static str> {
    match protocol {
        "chat-completions" => Ok("openai-completions"),
        "responses" => Ok("openai-responses"),
        "messages" => Ok("anthropic-messages"),
        _ => Err(Error::new(
            "unsupported_protocol",
            "Unsupported native protocol",
        )),
    }
}
fn transport(protocol: &str) -> Result<&'static str> {
    match protocol {
        "chat-completions" => Ok("chat_completions"),
        "responses" => Ok("codex_responses"),
        "messages" => Ok("anthropic_messages"),
        _ => Err(Error::new(
            "unsupported_protocol",
            "Unsupported native protocol",
        )),
    }
}
fn package(protocol: &str) -> Result<&'static str> {
    match protocol {
        "chat-completions" => Ok("@opencode/ai/providers/openai-compatible"),
        "responses" => Ok("@opencode/ai/providers/openai-compatible/responses"),
        "messages" => Ok("@opencode/ai/providers/anthropic-compatible"),
        _ => Err(Error::new(
            "unsupported_protocol",
            "Unknown native package protocol",
        )),
    }
}

pub(super) fn dsh_credential_ref(scope: &Path, entry: &Entry) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(scope.as_os_str().as_encoded_bytes());
    digest.update(b"\0");
    digest.update(entry.name.as_bytes());
    digest.update(b"\0");
    digest.update(entry.protocol.as_bytes());
    format!("TMAX_{:X}", digest.finalize())
}

fn credential_path(scope: &Path, entry: &Entry) -> PathBuf {
    use sha2::{Digest, Sha256};
    scope.join(format!(
        "tmax-{:x}.key",
        Sha256::digest(format!("{}\0{}", entry.name, entry.protocol).as_bytes())
    ))
}
fn key_command(path: &Path) -> Result<String> {
    let path = path.to_str().ok_or_else(|| {
        Error::new(
            "invalid_native_scope",
            "Native credential references require a UTF-8 path",
        )
    })?;
    #[cfg(unix)]
    {
        Ok(format!("/bin/cat '{}'", path.replace('\'', "'\\''")))
    }
    #[cfg(not(unix))]
    {
        use base64::Engine;
        // Pi runs credential commands through Bash on Windows, while other
        // tools use cmd.exe. Transport the PowerShell expression without outer
        // shell expansion of backslashes, dollar signs, or quotes in the path.
        let script = format!(
            "[Console]::Out.Write([IO.File]::ReadAllText('{}'))",
            path.replace('\'', "''")
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        Ok(format!(
            "powershell.exe -NoProfile -NonInteractive -EncodedCommand {encoded}"
        ))
    }
}

pub(super) fn build(target: &AgentTarget, scope: &Path, entries: &[Entry]) -> Result<Vec<File>> {
    for role in target.roles.keys() {
        if !supported_roles(&target.agent).contains(&role.as_str()) {
            return Err(Error::new(
                "unsupported_native_role",
                "This native role is not supported by the selected adapter",
            )
            .field(format!("roles.{role}")));
        }
    }
    let required_default = || {
        target.default.as_ref().ok_or_else(|| {
            Error::new(
                "native_incomplete",
                "Select an explicit native default before applying",
            )
        })
    };
    let mut providers = serde_json::Map::new();
    let mut files: Vec<File> = match target.agent.as_str() {
        "codex" => {
            let default = required_default()?;
            for entry in entries {
                if entry.protocol != "responses" {
                    return Err(Error::new(
                        "incompatible",
                        "Codex 0.162.1 custom model providers use Responses, not protocol conversion",
                    ));
                }
                let native_name = entry.native_name();
                if entry.login_provider.is_none() {
                    let mut provider = json!({"name":entry.name,"base_url":entry.base,"wire_api":"responses","requires_openai_auth":false});
                    #[cfg(unix)]
                    {
                        provider["auth"] =
                            json!({"command":"/bin/cat","args":[credential_path(scope,entry)]});
                    }
                    #[cfg(not(unix))]
                    {
                        provider["auth"] = json!({"command":"powershell.exe","args":["-NoProfile","-NonInteractive","-Command",format!("[Console]::Out.Write([IO.File]::ReadAllText('{}'))",credential_path(scope,entry).display().to_string().replace('\'',"''"))]});
                    }
                    providers.insert(native_name.into(), provider);
                }
            }
            let active = entries
                .iter()
                .find(|e| e.name == default.provider)
                .ok_or_else(|| {
                    Error::new(
                        "unresolved_reference",
                        "Native default provider does not resolve",
                    )
                })?;
            // Codex owns its model metadata and instructions. An explicit custom
            // model selection does not require a synthetic model_catalog_json.
            let mut config = json!({"model":default.id,"model_provider":active.native_name(),"model_providers":providers});
            for (role, r) in &target.roles {
                if r.provider != default.provider {
                    return Err(Error::new(
                        "incompatible",
                        "Codex review_model shares the active model_provider; it cannot select another provider independently",
                    ));
                }
                if role == "review" {
                    config["review_model"] = json!(r.id);
                }
            }
            Ok(vec![file(scope, "config.toml", Format::Toml, config)])
        }
        "pi" | "omp" => {
            for entry in entries {
                let native_name = entry.native_name();
                if entry.login_provider.is_some() {
                    continue;
                }
                // Pi 1.2.0's generic Chat Completions path emits its actual
                // session as x-session-id for this option, including local URLs.
                // The Go gateway qualifies and forwards that dynamic identity;
                // do not replace it with a provider-wide static headers object.
                let models: Vec<Value> = entry.models.iter().map(|id| {
                    let (protocol, base) = entry.model_routes.get(id)
                        .map(|(p, b)| (p.as_str(), b.as_str()))
                        .unwrap_or((&entry.protocol, &entry.base));
                    let mut model = json!({"id":id,"api":api(protocol)?,"baseUrl":base});
                    if let Some(metadata) = entry.model_metadata.get(id).and_then(Value::as_object) {
                        model.as_object_mut().expect("native model object").extend(metadata.clone());
                    }
                    if target.agent == "pi" && entry.gateway && protocol == "chat-completions" {
                        model["compat"] = json!({"sendSessionAffinityHeaders":true,"sessionAffinityFormat":"openrouter"});
                    }
                    Ok(model)
                }).collect::<Result<_>>()?;
                providers.insert(native_name.into(), json!({"baseUrl":entry.base,"api":api(&entry.protocol)?,"apiKey":format!("!{}",key_command(&credential_path(scope,entry))?),"models":models}));
                if target.agent == "omp" {
                    providers[native_name]["auth"] = json!("apiKey");
                }
            }
            if target.agent == "pi" {
                let mut files = vec![file(
                    scope,
                    "models.json",
                    Format::Json,
                    json!({"providers":providers}),
                )];
                if let Some(default) = &target.default {
                    let d = entries
                        .iter()
                        .find(|e| e.name == default.provider)
                        .ok_or_else(|| {
                            Error::new("unresolved_reference", "Native default does not resolve")
                        })?;
                    files.push(file(
                        scope,
                        "settings.json",
                        Format::Json,
                        json!({"defaultProvider":d.native_name(),"defaultModel":default.id}),
                    ));
                }
                Ok(files)
            } else {
                let mut roles = serde_json::Map::new();
                if let Some(default) = &target.default {
                    roles.insert("default".into(), json!(reference(default, entries)?));
                }
                for (role, r) in &target.roles {
                    roles.insert(role.clone(), json!(reference(r, entries)?));
                }
                let models =
                    if scope.join("models.yml").exists() || !scope.join("models.yaml").exists() {
                        "models.yml"
                    } else {
                        "models.yaml"
                    };
                let mut files = vec![file(
                    scope,
                    models,
                    Format::Yaml,
                    json!({"providers":providers}),
                )];
                if !roles.is_empty() {
                    files.push(file(
                        scope,
                        super::omp_main_settings(scope),
                        Format::Yaml,
                        json!({"modelRoles":roles}),
                    ));
                }
                Ok(files)
            }
        }
        "dsh" => {
            let mut refs = serde_json::Map::new();
            for entry in entries {
                if entry.login_provider.is_some() {
                    return Err(Error::new(
                        "incompatible",
                        "DSH native login authentication is not qualified; select an API-key provider",
                    ));
                }
                let models = entry.models.iter().map(|id| {
                    if let Some((protocol, base)) = entry.model_routes.get(id)
                        && (protocol != &entry.protocol || base != &entry.base)
                    {
                        // Qualified modelFields has no api or baseURL; the
                        // provider route owns one protocol and endpoint.
                        return Err(Error::new(
                            "incompatible",
                            "DSH requires one protocol and endpoint per native provider entry; split mixed model routes into separate entries",
                        ));
                    }
                    let mut model = json!({"id":id});
                    if let Some(metadata) = entry.model_metadata.get(id).and_then(Value::as_object) {
                        model.as_object_mut().expect("native model object").extend(metadata.clone());
                    }
                    Ok(model)
                }).collect::<Result<Vec<_>>>()?;
                let credential_ref = dsh_credential_ref(scope, entry);
                providers.insert(
                    entry.native_name().into(),
                    json!({
                        "apiKeyEnv":credential_ref,
                        "api":api(&entry.protocol)?,
                        "baseURL":entry.base,
                        "models":models,
                    }),
                );
                if let Some(key) = &entry.key {
                    refs.insert(credential_ref, json!(key));
                }
            }
            let mut patch =
                json!({"llm-pi-ai":{"disabled":false,"config":{"providers":providers}}});
            if let Some(default) = &target.default {
                let active = entries
                    .iter()
                    .find(|entry| entry.name == default.provider)
                    .ok_or_else(|| {
                        Error::new("unresolved_reference", "Native default does not resolve")
                    })?;
                patch["agent-default-model"] = json!({"config":{
                    "provider":active.native_name(),
                    "model":default.id,
                }});
            }
            let mut files = Vec::new();
            if !refs.is_empty() {
                patch["credentials"] =
                    json!({"disabled":false,"config":{"path":scope.join(".credentials.yaml")}});
                files.push(file(
                    scope,
                    ".credentials.yaml",
                    Format::Yaml,
                    json!({"version":1,"refs":refs}),
                ));
            }
            files.push(file(scope, "cordis.patch.yml", Format::DshPatch, patch));
            Ok(files)
        }
        "opencode" => {
            for entry in entries {
                if entry.login_provider.is_some() {
                    continue;
                }
                let package = package(&entry.protocol)?;
                let models: BTreeMap<&str, Value> = entry.models.iter().map(|id| {
                    let (protocol, base) = entry.model_routes.get(id)
                        .map(|(p, b)| (p.as_str(), b.as_str()))
                        .unwrap_or((&entry.protocol, &entry.base));
                    let mut model = json!({"modelID":id,"package":self::package(protocol)?,"settings":{"baseURL":base}});
                    if let Some(metadata) = entry.model_metadata.get(id).and_then(Value::as_object) {
                        model.as_object_mut().expect("native model object").extend(metadata.clone());
                    }
                    Ok((id.as_str(), model))
                }).collect::<Result<_>>()?;
                providers.insert(entry.name.clone(),json!({"name":entry.name,"package":package,"env":[],"settings":{"baseURL":entry.base,"apiKey":format!("{{file:{}}}",credential_path(scope,entry).file_name().expect("credential filename").to_string_lossy())},"models":models}));
            }
            let mut config = json!({"providers":providers});
            if let Some(default) = &target.default {
                config["model"] = json!(reference(default, entries)?);
            }
            for (role, r) in &target.roles {
                config["agents"][role]["model"] = json!(reference(r, entries)?);
            }
            let name =
                if scope.join("opencode.jsonc").exists() || !scope.join("opencode.json").exists() {
                    "opencode.jsonc"
                } else {
                    "opencode.json"
                };
            Ok(vec![file(scope, name, Format::Json, config)])
        }
        "hermes" => {
            let mut overrides = serde_json::Map::new();
            for entry in entries {
                if entry.login_provider.is_some() {
                    continue;
                }
                let models: BTreeMap<&str, Value> = entry
                    .models
                    .iter()
                    .map(|id| {
                        let mut model = json!({});
                        if let Some(metadata) = entry.model_metadata.get(id) {
                            if let Some(context) = metadata.get("context_length") {
                                model["context_length"] = context.clone();
                            }
                            if let Some(patch) = metadata.get("model_overrides") {
                                overrides
                                    .entry(entry.native_name().to_owned())
                                    .or_insert_with(|| json!({}))[id] = patch.clone();
                            }
                        }
                        (id.as_str(), model)
                    })
                    .collect();
                providers.insert(entry.name.clone(),json!({"name":entry.name,"base_url":entry.base,"api_key":entry.key,"transport":transport(&entry.protocol)?,"models":models}));
                if entry.gateway {
                    providers[&entry.name]["session_affinity_header"] = json!("x-session-id");
                }
            }
            let mut config = json!({"providers":providers});
            if let Some(default) = &target.default {
                let active = entries
                    .iter()
                    .find(|e| e.name == default.provider)
                    .ok_or_else(|| {
                        Error::new("unresolved_reference", "Native default does not resolve")
                    })?;
                config["model"] = json!({"provider":active.native_name(),"default":default.id});
                if active.login_provider.is_none() {
                    config["model"]["base_url"] = json!(active.base);
                }
            }
            if !overrides.is_empty() {
                config["model_overrides"] = Value::Object(overrides);
            }
            for (role, r) in &target.roles {
                let e = entries
                    .iter()
                    .find(|e| e.name == r.provider)
                    .ok_or_else(|| {
                        Error::new("unresolved_reference", "Native role does not resolve")
                    })?;
                config["auxiliary"][role] = json!({"provider":e.native_name(),"model":r.id});
            }
            // Required to keep the native refresh owner independent. This is not
            // token import and does not make pooled accounts independently pinned.
            if entries.iter().any(|e| e.login_provider.is_some()) {
                config["auth"]["adopt_external_logins"] = json!(false);
            }
            Ok(vec![file(scope, "config.yaml", Format::Yaml, config)])
        }
        _ => Err(Error::new(
            "unsupported_agent",
            "Supported agents are codex, pi, omp, opencode, hermes, and dsh",
        )),
    }?;
    for entry in entries {
        if !matches!(target.agent.as_str(), "hermes" | "dsh")
            && let Some(key) = &entry.key
        {
            files.insert(
                0,
                File {
                    path: credential_path(scope, entry),
                    format: Format::Credential,
                    desired: Value::String(key.clone()),
                },
            );
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn windows_credential_command_survives_native_shells_and_literal_paths() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("key with '$value & space.key");
        std::fs::write(&path, b"synthetic-credential").unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        let command = key_command(&path).unwrap();
        let mut shells = vec![("cmd.exe".to_owned(), "/c")];
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            let bash = PathBuf::from(program_files).join("Git/bin/bash.exe");
            if bash.is_file() {
                shells.push((bash.to_string_lossy().into(), "-c"));
            }
        }
        for (shell, flag) in shells {
            let output = std::process::Command::new(shell)
                .args([flag, &command])
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"synthetic-credential");
        }
    }
    fn target(agent: &str) -> AgentTarget {
        AgentTarget {
            agent: agent.into(),
            scope: "/explicit/native".into(),
            providers: BTreeMap::new(),
            default: Some(ModelReference {
                provider: "local".into(),
                id: "m".into(),
                protocol: if agent == "codex" {
                    "responses"
                } else {
                    "chat-completions"
                }
                .into(),
            }),
            roles: BTreeMap::new(),
        }
    }
    #[test]
    fn opencode_adds_models_without_overriding_the_native_default() {
        let mut target = target("opencode");
        target.default = None;
        let entry = Entry {
            name: "local".into(),
            base: "http://127.0.0.1:8317/v1".into(),
            protocol: "chat-completions".into(),
            key: None,
            login_provider: None,
            canonical: None,
            gateway: true,
            models: vec!["m".into()],
            model_routes: BTreeMap::new(),
            model_metadata: BTreeMap::new(),
            model_projection: BTreeMap::new(),
        };
        let files = build(&target, Path::new("/explicit/native"), &[entry]).unwrap();
        assert_eq!(
            files[0].desired["providers"]["local"]["models"]["m"]["modelID"],
            "m"
        );
        let key = files[0].desired["providers"]["local"]["settings"]["apiKey"]
            .as_str()
            .unwrap();
        assert!(key.starts_with("{file:tmax-"));
        assert!(!key.contains(['/', '\\']));
        assert!(files[0].desired.get("model").is_none());
        assert!(files[0].desired.get("agents").is_none());
    }
    #[test]
    fn omp_roles_update_existing_yaml_without_shadowing_unrelated_settings() {
        let directory = tempfile::tempdir().unwrap();
        let scope = directory.path();
        let original = "# native policy\nmodelRoleStorage: project\ndefaultThinkingLevel: high\n";
        std::fs::write(scope.join("config.yaml"), original).unwrap();
        let entry = Entry {
            name: "local".into(),
            base: "https://example.invalid/v1".into(),
            protocol: "chat-completions".into(),
            key: Some("not-read".into()),
            login_provider: None,
            canonical: None,
            gateway: false,
            models: vec!["m".into()],
            model_routes: BTreeMap::new(),
            model_metadata: BTreeMap::new(),
            model_projection: BTreeMap::new(),
        };
        let files = build(&target("omp"), scope, &[entry]).unwrap();
        assert!(!files.iter().any(|f| f.path == scope.join("config.yml")));
        let settings = files
            .iter()
            .find(|f| f.path == scope.join("config.yaml"))
            .unwrap();
        let rewritten = super::super::document::set(
            original,
            settings.format,
            &["modelRoles".into()],
            Some(&settings.desired["modelRoles"]),
        )
        .unwrap();
        let effective = super::super::document::parse(&rewritten, settings.format).unwrap();
        assert_eq!(effective["modelRoleStorage"], "project");
        assert_eq!(effective["defaultThinkingLevel"], "high");
        assert_eq!(effective["modelRoles"]["default"], "local/m");
        assert!(rewritten.contains("# native policy"));
    }

    #[test]
    fn codex_never_converts_chat_protocol() {
        let t = target("codex");
        let e = Entry {
            name: "local".into(),
            base: "https://example.invalid".into(),
            protocol: "chat-completions".into(),
            key: Some("secret".into()),
            login_provider: None,
            canonical: None,
            gateway: false,
            models: vec!["m".into()],
            model_routes: BTreeMap::new(),
            model_metadata: BTreeMap::new(),
            model_projection: BTreeMap::new(),
        };
        assert_eq!(
            build(&t, Path::new("/explicit/native"), &[e])
                .unwrap_err()
                .code,
            "incompatible"
        );
    }
    #[test]
    fn codex_review_cannot_silently_use_a_different_provider() {
        let mut target = target("codex");
        target.roles.insert(
            "review".into(),
            ModelReference {
                provider: "other".into(),
                id: "review-model".into(),
                protocol: "responses".into(),
            },
        );
        let entry = |name: &str| Entry {
            name: name.into(),
            base: "https://example.invalid/v1".into(),
            protocol: "responses".into(),
            key: Some("protected-reference".into()),
            login_provider: None,
            canonical: None,
            gateway: false,
            models: vec!["m".into()],
            model_routes: BTreeMap::new(),
            model_metadata: BTreeMap::new(),
            model_projection: BTreeMap::new(),
        };
        let error = build(
            &target,
            Path::new("/explicit/native"),
            &[entry("local"), entry("other")],
        )
        .unwrap_err();
        assert_eq!(error.code, "incompatible");
    }
    #[test]
    fn pi_gateway_affinity_uses_native_session_option_only_on_qualified_api() {
        let target = target("pi");
        let entry = Entry {
            name: "local".into(),
            base: "http://127.0.0.1:3210/v1".into(),
            protocol: "chat-completions".into(),
            key: None,
            login_provider: None,
            canonical: None,
            gateway: true,
            models: vec!["m".into(), "r".into()],
            model_routes: BTreeMap::from([(
                "r".into(),
                ("responses".into(), "http://127.0.0.1:3210/v1".into()),
            )]),
            model_metadata: BTreeMap::new(),
            model_projection: BTreeMap::new(),
        };
        let files = build(&target, Path::new("/explicit/native"), &[entry]).unwrap();
        let provider = &files[0].desired["providers"]["local"];
        let models = provider["models"].as_array().unwrap();
        assert_eq!(
            models[0]["compat"],
            json!({"sendSessionAffinityHeaders":true,"sessionAffinityFormat":"openrouter"})
        );
        assert!(models[0]["headers"].is_null());
        assert!(models[1]["compat"].is_null());
        assert!(provider["headers"].is_null());
        assert_eq!(files[1].desired["defaultModel"], "m");
        let entry = Entry {
            name: "local".into(),
            base: "https://example.invalid/v1".into(),
            protocol: "chat-completions".into(),
            key: None,
            login_provider: None,
            canonical: None,
            gateway: false,
            models: vec!["m".into()],
            model_routes: BTreeMap::new(),
            model_metadata: BTreeMap::new(),
            model_projection: BTreeMap::new(),
        };
        let direct = build(&target, Path::new("/explicit/native"), &[entry]).unwrap();
        assert!(direct[0].desired["providers"]["local"]["models"][0]["compat"].is_null());
    }
    #[test]
    fn pi_metadata_preserves_false_empty_and_unknown_without_invalid_limit_defaults() {
        let native = catalog_model_metadata(
            "pi",
            &json!({
                "limit":{"context":1000000,"output":65536},
                "reasoning":false,
                "modalities":{"input":["text","image","video","audio"]},
                "cost":{"input":0},
                "samplingParams":{"temperature":0.7},
            }),
        )
        .unwrap();
        assert_eq!(
            native,
            json!({"contextWindow":1000000,"maxTokens":65536,"reasoning":false,"input":["text","image"]})
        );
        assert_eq!(catalog_model_metadata("pi", &json!({})).unwrap(), json!({}));
        assert_eq!(
            catalog_model_metadata("pi", &json!({"modalities":{"input":[]}})).unwrap(),
            json!({"input":[]})
        );
        assert_eq!(
            catalog_model_metadata("pi", &json!({"modalities":{"input":["video"]}})).unwrap(),
            json!({"input":[]})
        );
        for field in ["context", "output"] {
            for value in [json!(0), json!(9_007_199_254_740_992u64)] {
                let mut metadata = json!({"limit":{}});
                metadata["limit"][field] = value;
                let error = catalog_model_metadata("pi", &metadata).unwrap_err();
                assert_eq!(error.code, "native_metadata_unrepresentable");
                assert_eq!(error.field, Some(format!("limit.{field}")));
            }
        }
    }

    #[test]
    fn qualified_metadata_is_concrete_only_and_never_changes_native_preferences() {
        let entry = || {
            Entry {
            name: "local".into(),
            base: "http://127.0.0.1:3210".into(),
            protocol: "chat-completions".into(),
            key: None,
            login_provider: None,
            canonical: None,
            gateway: true,
            models: vec!["m".into(), "dynamic".into()],
            model_routes: BTreeMap::new(),
            model_metadata: BTreeMap::from([("m".into(), catalog_model_metadata("pi", &json!({
                "limit":{"context":1000000,"output":65536},"reasoning":true,"modalities":{"input":["text","image","video"]}
            })).unwrap())]),
            model_projection: BTreeMap::new(),
        }
        };
        let pi = build(&target("pi"), Path::new("/explicit/native"), &[entry()]).unwrap();
        let models = &pi[0].desired["providers"]["local"]["models"];
        assert_eq!(models[0]["contextWindow"], 1000000);
        assert_eq!(models[0]["maxTokens"], 65536);
        assert_eq!(models[0]["reasoning"], true);
        assert_eq!(models[0]["input"], json!(["text", "image"]));
        for field in ["contextWindow", "maxTokens", "reasoning", "input"] {
            assert!(models[1].get(field).is_none());
        }
        for field in [
            "cost",
            "samplingParams",
            "samplingParamsByThinkingLevel",
            "thinkingLevelMap",
            "max_tokens",
            "max_output_tokens",
        ] {
            assert!(models[0].get(field).is_none());
        }
        assert_eq!(
            pi[1].desired,
            json!({"defaultProvider":"local","defaultModel":"m"})
        );
        let omp = build(&target("omp"), Path::new("/explicit/native"), &[entry()]).unwrap();
        for field in ["contextWindow", "maxTokens", "reasoning", "input"] {
            assert_eq!(
                omp[0].desired["providers"]["local"]["models"][0][field],
                models[0][field]
            );
            assert!(
                omp[0].desired["providers"]["local"]["models"][1]
                    .get(field)
                    .is_none()
            );
        }
    }
    #[test]
    fn opencode_integer_limits_preserve_zero_but_never_round_known_limits() {
        assert_eq!(
            model_metadata(
                "opencode",
                "chat-completions",
                &json!({"limit":{"context":0,"input":0,"output":0}})
            )
            .unwrap(),
            json!({"limit":{"context":0,"input":0,"output":0}})
        );
        assert_eq!(
            model_metadata("opencode", "chat-completions", &json!({})).unwrap(),
            json!({})
        );
        for field in ["context", "input", "output"] {
            let mut effective = json!({"limit":{}});
            effective["limit"][field] = json!(9_007_199_254_740_992u64);
            let error = model_metadata("opencode", "chat-completions", &effective).unwrap_err();
            assert_eq!(error.code, "native_metadata_unrepresentable");
            assert_eq!(error.field, Some(format!("limit.{field}")));
        }
        assert_eq!(
            model_metadata(
                "hermes",
                "chat-completions",
                &json!({"limit":{"context":0}})
            )
            .unwrap_err()
            .code,
            "native_metadata_unrepresentable"
        );
        assert_eq!(
            model_metadata(
                "hermes",
                "chat-completions",
                &json!({"limit":{"context":9_007_199_254_740_992u64}})
            )
            .unwrap()["context_length"],
            json!(9_007_199_254_740_992u64)
        );
    }

    #[test]
    fn reasoning_options_project_only_capability_selectors_never_defaults() {
        let effective = json!({
            "reasoning":true,
            "reasoning_options":[{"type":"effort","values":["low","high","max"]}]
        });
        assert_eq!(
            model_metadata("pi", "chat-completions", &effective).unwrap()["thinkingLevelMap"],
            json!({"off":null,"minimal":null,"low":"low","medium":null,"high":"high","xhigh":null,"max":"max"})
        );
        assert_eq!(
            model_metadata("omp", "chat-completions", &effective).unwrap()["thinking"],
            json!({"mode":"effort","efforts":["low","high","max"]})
        );
        assert_eq!(
            model_metadata("opencode", "chat-completions", &effective).unwrap()["variants"],
            json!([
                {"id":"low","settings":{"reasoningEffort":"low"}},
                {"id":"high","settings":{"reasoningEffort":"high"}},
                {"id":"max","settings":{"reasoningEffort":"max"}}
            ])
        );
        assert_eq!(
            model_metadata("opencode", "messages", &effective).unwrap()["variants"],
            json!([
                {"id":"low","settings":{"effort":"low"}},
                {"id":"high","settings":{"effort":"high"}},
                {"id":"max","settings":{"effort":"max"}}
            ])
        );
        let view = projection_view("pi", "chat-completions", &effective).unwrap();
        assert_eq!(view["state"], "projected");
        assert_eq!(view["omitted"], json!([]));
        assert_eq!(view["fields"]["thinkingLevelMap"]["off"], Value::Null);
        for agent in ["codex", "hermes"] {
            assert_eq!(
                projection_view(agent, "chat-completions", &effective).unwrap()["state"],
                "not_supported"
            );
        }
        let no_options = json!({"reasoning":true});
        assert!(
            model_metadata("pi", "chat-completions", &no_options)
                .unwrap()
                .get("thinkingLevelMap")
                .is_none()
        );
        assert_eq!(
            projection_view("pi", "chat-completions", &no_options).unwrap()["reason_code"],
            "reasoning_options_unavailable"
        );
        let disabled =
            json!({"reasoning":false,"reasoning_options":[{"type":"effort","values":["high"]}]});
        assert!(
            model_metadata("pi", "chat-completions", &disabled)
                .unwrap()
                .get("thinkingLevelMap")
                .is_none()
        );
        assert_eq!(
            projection_view("pi", "chat-completions", &disabled).unwrap()["reason_code"],
            "reasoning_disabled"
        );
    }

    #[test]
    fn reasoning_options_report_partial_and_block_unrepresentable_vocabularies() {
        let partial = json!({
            "reasoning":true,
            "reasoning_options":[{"type":"effort","values":["low","ultra"]}]
        });
        assert_eq!(
            model_metadata("pi", "chat-completions", &partial).unwrap()["thinkingLevelMap"],
            json!({"off":null,"minimal":null,"low":"low","medium":null,"high":null,"xhigh":null,"max":null})
        );
        let view = projection_view("pi", "chat-completions", &partial).unwrap();
        assert_eq!(view["state"], "partial");
        assert_eq!(view["omitted"], json!(["ultra"]));

        let unrepresentable = json!({
            "reasoning":true,
            "reasoning_options":[{"type":"effort","values":["ultra"]}]
        });
        for agent in ["pi", "omp"] {
            let error = model_metadata(agent, "chat-completions", &unrepresentable).unwrap_err();
            assert_eq!(error.code, "native_metadata_unrepresentable");
            assert_eq!(error.field.as_deref(), Some("reasoning_options"));
        }
        assert_eq!(
            model_metadata("opencode", "chat-completions", &unrepresentable).unwrap()["variants"],
            json!([{"id":"ultra","settings":{"reasoningEffort":"ultra"}}])
        );
    }

    #[test]
    fn supported_false_empty_and_unknown_capabilities_do_not_become_alias_winner_metadata() {
        for agent in ["opencode", "hermes"] {
            let effective = json!({
                "limit":{"context":123456,"output":8192},"reasoning":false,"tool_call":false,
                "modalities":{"input":[],"output":[]},
                "reasoning_options":[{"type":"effort","values":["high"]}],"temperature":true
            });
            let entry = Entry {
                name: "local".into(),
                base: "http://127.0.0.1:3210".into(),
                protocol: "chat-completions".into(),
                key: None,
                login_provider: None,
                canonical: None,
                gateway: true,
                models: vec!["m".into(), "dynamic".into(), "unknown".into()],
                model_routes: BTreeMap::new(),
                model_metadata: BTreeMap::from([
                    (
                        "m".into(),
                        model_metadata(agent, "chat-completions", &effective).unwrap(),
                    ),
                    (
                        "unknown".into(),
                        model_metadata(agent, "chat-completions", &json!({"reasoning":false}))
                            .unwrap(),
                    ),
                ]),
                model_projection: BTreeMap::new(),
            };
            let files = build(&target(agent), Path::new("/explicit/native"), &[entry]).unwrap();
            let config = &files[0].desired;
            let models = &config["providers"]["local"]["models"];
            if agent == "opencode" {
                assert_eq!(
                    models["m"]["capabilities"],
                    json!({"tools":false,"input":[],"output":[]})
                );
                assert!(models["m"].get("reasoning").is_none());
                for id in ["dynamic", "unknown"] {
                    assert!(models[id].get("limit").is_none());
                    assert!(models[id].get("capabilities").is_none());
                }
            } else {
                assert_eq!(models["m"], json!({"context_length":123456}));
                let overrides = &config["model_overrides"]["local"];
                assert_eq!(
                    overrides["m"],
                    json!({"context_window":123456,"supports_reasoning":false,"supports_tools":false,"supports_vision":false})
                );
                assert_eq!(overrides["unknown"], json!({"supports_reasoning":false}));
                assert!(overrides.get("dynamic").is_none());
                assert_eq!(models["dynamic"], json!({}));
            }
            for field in [
                "max_tokens",
                "max_output_tokens",
                "reasoning_effort",
                "reasoning_options",
                "temperature",
                "cost",
            ] {
                assert!(models["m"].get(field).is_none());
            }
            assert!(config.get("agent").is_none());
            assert!(config.get("reasoning_overrides").is_none());
        }
    }

    fn dsh_entry(protocol: &str) -> Entry {
        Entry {
            name: "local".into(),
            base: "https://example.invalid/v1".into(),
            protocol: protocol.into(),
            key: Some("protected-key-reference".into()),
            login_provider: None,
            canonical: None,
            gateway: false,
            models: vec!["m".into()],
            model_routes: BTreeMap::new(),
            model_metadata: BTreeMap::new(),
            model_projection: BTreeMap::new(),
        }
    }

    #[test]
    fn dsh_model_only_handoff_keeps_secrets_and_native_preferences_out_of_patch() {
        let scope = Path::new("/explicit/dsh");
        let mut target = target("dsh");
        target.default = None;
        for (protocol, expected_api) in [
            ("chat-completions", "openai-completions"),
            ("responses", "openai-responses"),
            ("messages", "anthropic-messages"),
        ] {
            let entry = dsh_entry(protocol);
            let credential_ref = dsh_credential_ref(scope, &entry);
            let files = build(&target, scope, &[entry]).unwrap();
            assert_eq!(files.len(), 2);
            assert_eq!(files[0].path, Path::new("/explicit/dsh/.credentials.yaml"));
            assert_eq!(files[0].desired["version"], 1);
            assert_eq!(
                files[0].desired["refs"][&credential_ref],
                "protected-key-reference"
            );
            assert_eq!(files[1].path, scope.join("cordis.patch.yml"));
            assert!(matches!(files[1].format, Format::DshPatch));
            let patch = &files[1].desired;
            let provider = &patch["llm-pi-ai"]["config"]["providers"]["local"];
            assert_eq!(provider["api"], expected_api);
            assert_eq!(provider["apiKeyEnv"], credential_ref);
            assert_eq!(provider["models"], json!([{"id":"m"}]));
            assert!(patch.get("agent-default-model").is_none());
            assert!(provider.get("reasoning").is_none());
            assert!(provider.get("headers").is_none());
            assert!(!patch.to_string().contains("protected-key-reference"));
        }
        let entry = dsh_entry("responses");
        assert_ne!(
            dsh_credential_ref(scope, &entry),
            dsh_credential_ref(Path::new("/another/dsh"), &entry)
        );
    }

    #[test]
    fn dsh_rejects_unrepresentable_model_routes_instead_of_changing_protocols() {
        let scope = Path::new("/explicit/dsh");
        for route in [
            ("responses", "https://example.invalid/v1"),
            ("chat-completions", "https://other.invalid/v1"),
        ] {
            let mut entry = dsh_entry("chat-completions");
            entry
                .model_routes
                .insert("m".into(), (route.0.into(), route.1.into()));
            assert_eq!(
                build(&target("dsh"), scope, &[entry]).unwrap_err().code,
                "incompatible"
            );
        }
        let files = build(&target("dsh"), scope, &[dsh_entry("chat-completions")]).unwrap();
        assert_eq!(
            files[1].desired["agent-default-model"],
            json!({"config":{"provider":"local","model":"m"}})
        );
    }

    #[test]
    fn dsh_capabilities_do_not_select_thinking_or_output_request_defaults() {
        let effective = json!({
            "limit":{"context":128000,"output":8192},
            "reasoning":true,
            "reasoning_options":[{"type":"effort","values":["off","low","max","ultra"]}],
            "modalities":{"input":["text","image","video"]},
        });
        for protocol in ["chat-completions", "responses", "messages"] {
            let metadata = model_metadata("dsh", protocol, &effective).unwrap();
            assert_eq!(
                metadata,
                json!({
                    "contextWindow":128000,
                    "input":["text","image"],
                    "reasoningEfforts":{"off":"off","low":"low","max":"max"},
                })
            );
            let projection = projection_view("dsh", protocol, &effective).unwrap();
            assert_eq!(projection["state"], "partial");
            assert_eq!(projection["omitted"], json!(["ultra"]));
            assert_eq!(
                projection["fields"],
                json!({"reasoningEfforts":{"off":"off","low":"low","max":"max"}})
            );
        }
        assert_eq!(
            model_metadata("dsh", "responses", &json!({"reasoning":true})).unwrap(),
            json!({})
        );
        assert_eq!(
            model_metadata("dsh", "responses", &json!({"reasoning":false})).unwrap(),
            json!({"reasoningEfforts":false})
        );
        for input in [json!([]), json!(["video"])] {
            assert_eq!(
                model_metadata("dsh", "responses", &json!({"modalities":{"input":input}}))
                    .unwrap_err()
                    .code,
                "native_metadata_unrepresentable"
            );
        }
        for values in [json!(["off"]), json!(["ultra"])] {
            assert_eq!(
                projection_view(
                    "dsh",
                    "responses",
                    &json!({"reasoning_options":[{"type":"effort","values":values}]})
                )
                .unwrap_err()
                .code,
                "native_metadata_unrepresentable"
            );
        }
    }
}
