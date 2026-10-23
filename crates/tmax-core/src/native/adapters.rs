//! Five independent, version-specific native configuration mappings.
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

pub(super) const AGENTS: [&str; 5] = ["codex", "pi", "omp", "opencode", "hermes"];
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
            json!({"scope":"active profile agent directory", "order":["CLI and session selections", "extra --config overlays", "project .omp/config.yml", "active profile config.yml"], "models":["models.yml", "models.yaml", "legacy models.json migration"], "credentials":"native auth cascade and account policies can precede configured apiKey; unqualified existing native auth blocks handoff", "launch_environment":"PI_CODING_AGENT_DIR with OMP_PROFILE unset for explicit default-profile directory", "native_reload":"Restart omp in the selected profile; existing sessions retain selections"})
        }
        "opencode" => {
            json!({"scope":"global config directory / explicit project config directory; accounts are server-scoped", "order":["managed policy", "system configuration", "project .opencode files", "project direct files", "global configuration"], "credentials":"saved server account precedes environment; explicit settings.apiKey pins a custom package key", "launch_environment":"OPENCODE_CONFIG_DIR; an isolated server also needs explicit OPENCODE_DB", "native_reload":"Run opencode reload on the intended server or start a private --standalone server; tmax does not restart a shared server"})
        }
        "hermes" => {
            json!({"scope":"explicit HERMES_HOME, including an explicitly selected profile", "order":["managed scope", "CLI/environment model overrides", "selected profile config.yaml"], "credentials":"native pools and external login adoption can affect built-in providers; custom entries use the selected literal key", "launch_environment":"HERMES_HOME", "native_reload":"Restart Hermes in the selected HERMES_HOME; active sessions may retain model settings"})
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
    pub proxy: bool,
    pub models: Vec<String>,
    pub model_routes: BTreeMap<String, (String, String)>,
}
impl Entry {
    pub(super) fn native_name(&self) -> &str {
        self.login_provider
            .as_deref()
            .or(self.canonical.as_deref())
            .unwrap_or(&self.name)
    }
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
        Ok(format!(
            "powershell.exe -NoProfile -NonInteractive -Command \"[Console]::Out.Write([IO.File]::ReadAllText('{}'))\"",
            path.replace('\'', "''")
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
    let default = target.default.as_ref().ok_or_else(|| {
        Error::new(
            "native_incomplete",
            "Select an explicit native default before applying",
        )
    })?;
    let mut providers = serde_json::Map::new();
    let mut files: Vec<File> = match target.agent.as_str() {
        "codex" => {
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
                let models: Vec<Value> = entry.models.iter().map(|id| {let (protocol,base)=entry.model_routes.get(id).map(|(p,b)|(p.as_str(),b.as_str())).unwrap_or((&entry.protocol,&entry.base));let mut model=json!({"id":id,"api":api(protocol)?,"baseUrl":base});if entry.proxy && target.agent=="pi" && protocol=="chat-completions"{model["compat"]=json!({"sendSessionAffinityHeaders":true,"sessionAffinityFormat":"openrouter"});}Ok(model)}).collect::<Result<_>>()?;
                providers.insert(native_name.into(), json!({"baseUrl":entry.base,"api":api(&entry.protocol)?,"apiKey":format!("!{}",key_command(&credential_path(scope,entry))?),"models":models}));
                if target.agent == "omp" {
                    providers[native_name]["auth"] = json!("apiKey");
                }
            }
            if target.agent == "pi" {
                let d = entries
                    .iter()
                    .find(|e| e.name == default.provider)
                    .ok_or_else(|| {
                        Error::new("unresolved_reference", "Native default does not resolve")
                    })?;
                Ok(vec![
                    file(
                        scope,
                        "models.json",
                        Format::Json,
                        json!({"providers":providers}),
                    ),
                    file(
                        scope,
                        "settings.json",
                        Format::Json,
                        json!({"defaultProvider":d.native_name(),"defaultModel":default.id}),
                    ),
                ])
            } else {
                let mut roles = serde_json::Map::new();
                roles.insert("default".into(), json!(reference(default, entries)?));
                for (role, r) in &target.roles {
                    roles.insert(role.clone(), json!(reference(r, entries)?));
                }
                let models =
                    if scope.join("models.yml").exists() || !scope.join("models.yaml").exists() {
                        "models.yml"
                    } else {
                        "models.yaml"
                    };
                Ok(vec![
                    file(scope, models, Format::Yaml, json!({"providers":providers})),
                    file(
                        scope,
                        "config.yml",
                        Format::Yaml,
                        json!({"modelRoles":roles}),
                    ),
                ])
            }
        }
        "opencode" => {
            for entry in entries {
                if entry.login_provider.is_some() {
                    continue;
                }
                let package = package(&entry.protocol)?;
                let models: BTreeMap<&str, Value> = entry.models.iter().map(|id| {let (protocol,base)=entry.model_routes.get(id).map(|(p,b)|(p.as_str(),b.as_str())).unwrap_or((&entry.protocol,&entry.base));Ok((id.as_str(),json!({"modelID":id,"package":self::package(protocol)?,"settings":{"baseURL":base}})))}).collect::<Result<_>>()?;
                providers.insert(entry.name.clone(),json!({"name":entry.name,"package":package,"env":[],"settings":{"baseURL":entry.base,"apiKey":format!("{{file:{}}}",credential_path(scope,entry).display())},"models":models}));
            }
            let mut config = json!({"providers":providers,"model":reference(default,entries)?});
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
            for entry in entries {
                if entry.login_provider.is_some() {
                    continue;
                }
                let models: BTreeMap<&str, Value> = entry
                    .models
                    .iter()
                    .map(|id| (id.as_str(), json!({})))
                    .collect();
                providers.insert(entry.name.clone(),json!({"name":entry.name,"base_url":entry.base,"key_cmd":key_command(&credential_path(scope,entry))?,"transport":transport(&entry.protocol)?,"models":models}));
                if entry.proxy {
                    providers[&entry.name]["session_affinity_header"] = json!("x-session-id");
                }
            }
            let active = entries
                .iter()
                .find(|e| e.name == default.provider)
                .ok_or_else(|| {
                    Error::new("unresolved_reference", "Native default does not resolve")
                })?;
            let mut config = json!({"providers":providers,"model":{"provider":active.native_name(),"default":default.id}});
            if active.login_provider.is_none() {
                config["model"]["base_url"] = json!(active.base);
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
            "Supported agents are codex, pi, omp, opencode, and hermes",
        )),
    }?;
    for entry in entries {
        if let Some(key) = &entry.key {
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
    fn codex_never_converts_chat_protocol() {
        let t = target("codex");
        let e = Entry {
            name: "local".into(),
            base: "https://example.invalid".into(),
            protocol: "chat-completions".into(),
            key: Some("secret".into()),
            login_provider: None,
            canonical: None,
            proxy: false,
            models: vec!["m".into()],
            model_routes: BTreeMap::new(),
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
            proxy: false,
            models: vec!["m".into()],
            model_routes: BTreeMap::new(),
        };
        let error = build(
            &target,
            Path::new("/explicit/native"),
            &[entry("local"), entry("other")],
        )
        .unwrap_err();
        assert_eq!(error.code, "incompatible");
    }
}
