//! Independent, version-specific native configuration mappings.
//! Evidence is pinned to the concrete installations below, not a version range.
use super::document::{self, Format};
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

mod capabilities;
mod codex;
mod credentials;
mod dsh;
mod hermes;
mod opencode;
mod pi_omp;
#[cfg(test)]
mod test_support;

pub(super) use capabilities::native_completion;
pub(super) use capabilities::projection_view;
pub(super) use credentials::resolve_handoff;

// Hermes consumes route context and catalog capability patches separately.
// `model_overrides` is an intermediate projection, never a provider-model key.
pub(crate) fn model_metadata(agent: &str, protocol: &str, effective: &Value) -> Result<Value> {
    let mut metadata = match agent {
        "pi" | "omp" => pi_omp::model_metadata(effective)?,
        "dsh" => dsh::model_metadata(effective)?,
        "opencode" => opencode::model_metadata(effective)?,
        "hermes" => hermes::model_metadata(effective)?,
        _ => json!({}),
    };
    capabilities::append_input(agent, effective, &mut metadata)?;
    capabilities::append_reasoning(agent, protocol, effective, &mut metadata)?;
    Ok(metadata)
}

pub(crate) fn alias_model_metadata(
    agent: &str,
    protocol: &str,
    effective: &Value,
) -> Result<Value> {
    let mut metadata = model_metadata(agent, protocol, effective)?;
    if agent == "dsh" && metadata.get("reasoningEfforts").is_none() {
        // DSH merges retained model rows, so omission would preserve an obsolete
        // selector. This alias has no shared selectable effort; candidates may
        // still support reasoning independently.
        metadata["reasoningEfforts"] = json!(false);
    }
    Ok(metadata)
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
    let mut files = match target.agent.as_str() {
        "codex" => codex::build(target, scope, entries),
        "pi" | "omp" => pi_omp::build(target, scope, entries),
        "dsh" => dsh::build(target, scope, entries),
        "opencode" => opencode::build(target, scope, entries),
        "hermes" => hermes::build(target, scope, entries),
        _ => Err(Error::new(
            "unsupported_agent",
            "Supported agents are codex, pi, omp, opencode, hermes, and dsh",
        )),
    }?;
    credentials::prepend_files(&target.agent, scope, entries, &mut files);
    Ok(files)
}

fn leaves(value: &Value, path: &mut Vec<String>, out: &mut Vec<(Vec<String>, Value)>) {
    if let Some(map) = value.as_object().filter(|m| !m.is_empty()) {
        for (k, v) in map {
            path.push(k.clone());
            leaves(v, path, out);
            path.pop();
        }
    } else {
        out.push((path.clone(), value.clone()));
    }
}

pub(super) fn desired_fields(
    agent: &str,
    desired: &Value,
    current: &Value,
) -> Vec<(Vec<String>, Option<Value>)> {
    let mut fields = Vec::new();
    leaves(desired, &mut Vec::new(), &mut fields);
    fields.retain(|(path, value)| {
        !path.is_empty() && !(agent == "hermes" && hermes::preserve_model(path, value, current))
    });
    let mut fields: Vec<_> = fields
        .into_iter()
        .map(|(path, mut value)| {
            if agent == "dsh" {
                dsh::preserve_models(&path, &mut value, current);
            }
            (path, Some(value))
        })
        .collect();
    if agent == "hermes" {
        hermes::obsolete_fields(desired, &mut fields);
    }
    fields
}

pub(super) fn desired_fields_with_ownership(
    agent: &str,
    desired: &Value,
    current: &Value,
    owned: &[super::recovery::Field],
) -> Vec<(Vec<String>, Option<Value>)> {
    let mut fields = desired_fields(agent, desired, current);
    if agent == "dsh" {
        for (path, value) in &mut fields {
            if owned.iter().any(|field| field.path == *path)
                && let Some(value) = value
            {
                dsh::withdraw_input(path, value, desired);
            }
        }
    }
    for field in owned {
        let path = &field.path;
        let selected_input = match agent {
            "opencode" => {
                path.len() == 6
                    && path[0] == "providers"
                    && path[2] == "models"
                    && path[4] == "capabilities"
                    && path[5] == "input"
                    && document::get(desired, &path[..4]).is_some()
            }
            "hermes" => {
                path.len() == 4
                    && path[0] == "model_overrides"
                    && path[3] == "supports_vision"
                    && desired
                        .get("providers")
                        .and_then(|providers| providers.get(&path[1]))
                        .and_then(|provider| provider.get("models"))
                        .is_some_and(|models| models.get(&path[2]).is_some())
            }
            _ => false,
        };
        if selected_input && !fields.iter().any(|(desired, _)| desired == path) {
            // Keep owning the withdrawal: restoring the original declaration
            // here would reintroduce support absent from the common input set.
            fields.push((path.clone(), None));
        }
    }
    fields
}

pub(super) fn inline_credential(agent: &str, path: &[String]) -> bool {
    match agent {
        "hermes" => hermes::inline_credential(path),
        "dsh" => dsh::inline_credential(path),
        _ => false,
    }
}

pub(super) fn validate_credentials(agent: &str, current: &Value) -> Result<()> {
    if agent == "dsh" {
        dsh::validate_credentials(current)?;
    }
    Ok(())
}

pub(super) fn validate_existing(agent: &str, file: &File, current: &Value) -> Result<()> {
    if agent == "dsh" {
        dsh::validate_existing(file, current)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::test_support::target;
    use super::*;
    #[test]
    fn input_projection_distinguishes_unknown_empty_and_schema_limits() {
        for agent in AGENTS {
            let unknown = projection_view(agent, "chat-completions", &json!({})).unwrap();
            assert_eq!(
                unknown["input"]["state"],
                if agent == "codex" {
                    "not_supported"
                } else {
                    "unavailable"
                }
            );
            assert_eq!(unknown["limits"]["state"], "unavailable");
            assert_eq!(unknown["limits"]["reason_code"], "limits_unknown");
            let empty = json!({"modalities":{"input":[]}});
            let view = projection_view(agent, "chat-completions", &empty).unwrap();
            match agent {
                "pi" | "omp" => {
                    assert_eq!(view["input"]["state"], "projected");
                    assert_eq!(
                        model_metadata(agent, "chat-completions", &empty).unwrap()["input"],
                        json!([])
                    );
                }
                "dsh" => {
                    assert_eq!(view["input"]["state"], "blocked");
                    assert_eq!(view["input"]["reason_code"], "native_input_unrepresentable");
                    assert_eq!(
                        model_metadata(agent, "chat-completions", &empty)
                            .unwrap_err()
                            .field
                            .as_deref(),
                        Some("modalities.input")
                    );
                }
                "opencode" => {
                    assert_eq!(
                        view["input"]["fields"],
                        json!({"capabilities":{"input":[]}})
                    );
                    assert_eq!(
                        model_metadata(agent, "chat-completions", &empty).unwrap()["capabilities"]
                            ["input"],
                        json!([])
                    );
                }
                "hermes" => {
                    assert_eq!(view["input"]["state"], "partial");
                    assert_eq!(
                        view["input"]["fields"],
                        json!({"model_overrides":{"supports_vision":false}})
                    );
                }
                _ => assert_eq!(view["input"]["state"], "not_supported"),
            }
        }
        let effective = json!({"modalities":{"input":["text","image","audio","video"]}});
        let partial_limits = json!({"modalities":{"input":["text"]},"limit":{"context":1000}});
        assert_eq!(
            projection_view("pi", "responses", &partial_limits).unwrap()["limits"],
            json!({"state":"partial","reason_code":"limits_unknown","fields":{"contextWindow":1000,"maxTokens":null},"omitted":[]})
        );
        let known_limits = json!({"limit":{"context":1000,"output":128}});
        assert_eq!(
            projection_view("dsh", "responses", &known_limits).unwrap()["limits"],
            json!({"state":"projected","reason_code":null,"fields":{"contextWindow":1000,"maxTokens":128},"omitted":[]})
        );
        for agent in ["pi", "omp", "dsh", "hermes"] {
            let view = projection_view(agent, "responses", &effective).unwrap();
            assert_eq!(view["input"]["state"], "partial");
            assert_eq!(view["input"]["omitted"], json!(["audio", "video"]));
            let metadata = model_metadata(agent, "responses", &effective).unwrap();
            for (key, value) in view["input"]["fields"].as_object().unwrap() {
                assert_eq!(metadata[key], *value);
            }
        }
        let view = projection_view("opencode", "messages", &effective).unwrap();
        assert_eq!(view["input"]["state"], "projected");
        assert_eq!(
            view["input"]["fields"]["capabilities"]["input"],
            effective["modalities"]["input"]
        );
    }

    #[test]
    fn vanished_owned_input_is_deleted_not_restored_from_native_baseline() {
        for (agent, desired, current, path) in [
            (
                "opencode",
                json!({"providers":{"local":{"models":{"shared":{"modelID":"shared"}}}}}),
                json!({"providers":{"local":{"models":{"shared":{"capabilities":{"input":["text","image"],"tools":true}}}}}}),
                vec![
                    "providers",
                    "local",
                    "models",
                    "shared",
                    "capabilities",
                    "input",
                ],
            ),
            (
                "hermes",
                json!({"providers":{"local":{"models":{"shared":{}}}}}),
                json!({"model_overrides":{"local":{"shared":{"supports_vision":true,"supports_tools":true}}}}),
                vec!["model_overrides", "local", "shared", "supports_vision"],
            ),
        ] {
            let path = path.into_iter().map(str::to_owned).collect::<Vec<_>>();
            let owned = vec![super::super::recovery::Field {
                path: path.clone(),
                before_hash: "native-image-baseline".into(),
                after_hash: "managed-image".into(),
                observed_hash: "managed-image".into(),
                released: false,
            }];
            let fields = desired_fields_with_ownership(agent, &desired, &current, &owned);
            assert_eq!(
                fields.iter().find(|(field, _)| field == &path).unwrap().1,
                None
            );
            assert!(
                !desired_fields_with_ownership(agent, &desired, &current, &[])
                    .iter()
                    .any(|(field, _)| field == &path)
            );
            let mut text = serde_json::to_string(&current).unwrap();
            for (field, value) in fields {
                text = document::set(&text, Format::Json, &field, value.as_ref()).unwrap();
            }
            let after = document::parse(&text, Format::Json).unwrap();
            assert!(document::get(&after, &path).is_none());
            let sibling = if agent == "hermes" {
                "supports_tools"
            } else {
                "tools"
            };
            let mut sibling_path = path[..path.len() - 1].to_vec();
            sibling_path.push(sibling.into());
            assert_eq!(document::get(&after, &sibling_path), Some(&json!(true)));
        }
    }
    #[test]
    fn pi_and_omp_reapplication_replaces_input_without_inventing_text() {
        for agent in ["pi", "omp"] {
            let current = json!({"providers":{"local":{"models":[{"id":"shared","input":["text","image"]}],"unrelated_native":"retained"}},"theme":"dark"});
            for (effective, expected) in [
                (
                    json!({"modalities":{"input":["text"]}}),
                    Some(json!(["text"])),
                ),
                (json!({"modalities":{"input":[]}}), Some(json!([]))),
                (json!({}), None),
            ] {
                let mut model = alias_model_metadata(agent, "responses", &effective).unwrap();
                model["id"] = json!("shared");
                let desired = json!({"providers":{"local":{"models":[model]}}});
                let mut text = serde_json::to_string(&current).unwrap();
                for (path, value) in desired_fields_with_ownership(agent, &desired, &current, &[]) {
                    text = document::set(&text, Format::Json, &path, value.as_ref()).unwrap();
                }
                let after = document::parse(&text, Format::Json).unwrap();
                assert_eq!(
                    after["providers"]["local"]["models"][0].get("input"),
                    expected.as_ref()
                );
                assert_eq!(after["providers"]["local"]["unrelated_native"], "retained");
                assert_eq!(after["theme"], "dark");
            }
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
}
