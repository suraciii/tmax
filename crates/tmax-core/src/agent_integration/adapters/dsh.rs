use super::super::document::{self, Format};
use super::capabilities::{ReasoningProjection, api, catalog_model_metadata};
use super::{Entry, File, file};
use crate::{Error, Result, config::AgentTarget};
use serde_json::{Value, json};
use std::path::Path;

/// The DSH entry whose settings carry a profile's providers.
pub(in crate::agent_integration) const PROFILE_SETTINGS: &str = "llm-pi-ai";

// DSH 0.2.1-rc.2 catalog.ts THINKING_LEVEL_GATE and resolveModelReasoning;
// independently qualified from Pi's thinkingLevelMap carrier.
const DSH_EFFORT_LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

pub(in crate::agent_integration) fn dsh_credential_ref(
    scope: &Path,
    profile: &str,
    entry: &Entry,
) -> String {
    dsh_credential_ref_for(scope, profile, &entry.name, &entry.protocol)
}

/// Stable DSH credential reference name for one profile's route and protocol.
/// The profile is part of the identity: two profiles may select the same entry
/// differently, and a shared reference would make one overwrite the other.
pub(crate) fn dsh_credential_ref_for(
    scope: &Path,
    profile: &str,
    name: &str,
    protocol: &str,
) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(scope.as_os_str().as_encoded_bytes());
    digest.update(b"\0");
    digest.update(profile.as_bytes());
    digest.update(b"\0");
    digest.update(name.as_bytes());
    digest.update(b"\0");
    digest.update(protocol.as_bytes());
    format!("TMAX_{:X}", digest.finalize())
}

pub(super) fn project_reasoning(values: &[String]) -> Result<ReasoningProjection> {
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

pub(super) fn model_metadata(effective: &Value) -> Result<Value> {
    let mut metadata = catalog_model_metadata(effective, false)?;
    // DSH accepts the same positive integer capacities and text/image
    // modalities, but represents reasoning capability only through efforts.
    metadata
        .as_object_mut()
        .expect("native model metadata object")
        .remove("reasoning");
    if effective.get("reasoning").and_then(Value::as_bool) == Some(false) {
        metadata["reasoningEfforts"] = json!(false);
    }
    Ok(metadata)
}

/// The DSH profile whose settings carry this home's providers.
///
/// A provider is a profile-level setting in DSH: its Models page writes one into
/// `profiles/<profile>/cordis.patch.yml`. tmax writes to that same place instead
/// of inventing a home-wide mechanism, so applying produces working routes with
/// no plugin to install. Desktop first, then web, then whatever the caller names.
pub(in crate::agent_integration) fn profile(
    scope: &Path,
    requested: Option<&str>,
) -> Result<String> {
    let available = profiles(scope);
    if let Some(name) = requested {
        return available
            .iter()
            .find(|candidate| *candidate == name)
            .cloned()
            .ok_or_else(|| {
                Error::new(
                    "unknown_resource",
                    &format!(
                        "No DSH profile named \"{name}\" in this home; available: {}",
                        describe_profiles(&available)
                    ),
                )
            });
    }
    for preferred in ["desktop", "web"] {
        if let Some(found) = available.iter().find(|candidate| *candidate == preferred) {
            return Ok(found.clone());
        }
    }
    Err(Error::new(
        "unknown_resource",
        &format!(
            "This DSH home declares no desktop or web profile; available: {}. Name one with --profile",
            describe_profiles(&available)
        ),
    ))
}

fn describe_profiles(available: &[String]) -> String {
    if available.is_empty() {
        "none".to_owned()
    } else {
        available.join(", ")
    }
}

fn profiles(scope: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(scope.join("profiles")) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir() && entry.path().join("package.json").is_file())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect();
    names.sort();
    names
}

/// The provider name tmax writes into DSH's own settings: recognizable, and
/// namespaced so it cannot collide with a provider the user configured.
pub(super) fn provider_name(entry: &Entry) -> String {
    format!("tmax-{}", entry.name)
}

pub(super) fn build(
    target: &AgentTarget,
    scope: &Path,
    entries: &[Entry],
    profile: &str,
) -> Result<Vec<File>> {
    super::validate_default_effort(target, entries)?;
    let mut providers = serde_json::Map::new();
    let mut refs = serde_json::Map::new();
    for entry in entries {
        if entry.login_provider.is_some() {
            return Err(Error::new(
                "incompatible",
                "DSH native login authentication is not qualified; select an API-key provider",
            ));
        }
        let mut models = Vec::new();
        for id in &entry.models {
            if let Some((protocol, base)) = entry.model_routes.get(id)
                && (protocol != &entry.protocol || base != &entry.base)
            {
                return Err(Error::new(
                    "incompatible",
                    "DSH requires one protocol and endpoint per native provider entry; split mixed model routes into separate entries",
                ));
            }
            let mut model = json!({"id":id});
            if let Some(metadata) = entry.model_metadata.get(id).and_then(Value::as_object) {
                model
                    .as_object_mut()
                    .expect("native model object")
                    .extend(metadata.clone());
            }
            models.push(model);
        }
        let credential_ref = dsh_credential_ref(scope, profile, entry);
        providers.insert(
            provider_name(entry),
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
    let mut patch = json!({"llm-pi-ai":{"config":{"providers":providers}}});
    if let Some(default) = &target.default {
        let active = entries
            .iter()
            .find(|entry| entry.name == default.provider)
            .ok_or_else(|| Error::new("unresolved_reference", "Native default does not resolve"))?;
        let mut config = json!({
            "provider": provider_name(active),
            "model": default.id,
        });
        if let Some(effort) = &default.effort {
            config["reasoningEffort"] = json!(effort);
        }
        patch["agent-default-model"] = json!({"config":config});
    }
    let mut files = Vec::new();
    if !refs.is_empty() {
        // The base bundle composes DSH's own credential store at this path. The
        // headless bundle disables that service, and a key-bearing provider is
        // useless without it, so the profile that receives the providers asserts
        // it rather than leaving a route that cannot resolve its key.
        patch["credentials"] = json!({"disabled":false});
        files.push(file(
            scope,
            ".credentials.yaml",
            Format::Yaml,
            json!({"version":1,"refs":refs}),
        ));
    }
    // The patch lives in the profile whose settings a provider belongs to.
    files.push(file(
        &scope.join("profiles").join(profile),
        "cordis.patch.yml",
        Format::DshPatch,
        patch,
    ));
    Ok(files)
}

/// The adapter file an earlier release embedded beside the home patch.
pub(in crate::agent_integration) const LEGACY_PLUGIN_FILE: &str = "tmax-dsh.mjs";

/// Whether this home still declares that embedded plugin: the `llm-tmax` row
/// that loads *this* home's artifact. A row that loads anything else belongs to
/// the user, whatever its id, and is left alone.
pub(in crate::agent_integration) fn legacy_row(current: &Value, scope: &Path) -> bool {
    let artifact = current
        .get("llm-tmax")
        .and_then(|row| row.get("name"))
        .and_then(Value::as_str)
        .is_some_and(|name| portable(Path::new(name)) == portable(&scope.join(LEGACY_PLUGIN_FILE)));
    // A release may also have written a `tmax` row that named this home; that
    // entry configured nothing else, so retiring it loses no user setting.
    let home_entry = current
        .get("tmax")
        .and_then(|row| row.get("config"))
        .and_then(|config| config.get("home"))
        .and_then(Value::as_str)
        .is_some_and(|home| portable(Path::new(home)) == portable(scope));
    artifact || home_entry
}

/// Windows verbatim paths differ textually from the same path written plainly.
fn portable(path: &Path) -> String {
    path.to_string_lossy()
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .unwrap_or_else(|| {
            path.to_string_lossy()
                .strip_prefix(r"\\?\")
                .map(str::to_owned)
                .unwrap_or_else(|| path.to_string_lossy().into_owned())
        })
}

pub(super) fn inline_credential(path: &[String]) -> bool {
    path.len() == 2 && path[0] == "refs"
}

pub(super) fn preserve_models(path: &[String], value: &mut Value, current: &Value) {
    if path.len() == 5
        && path[0] == PROFILE_SETTINGS
        && path[1] == "config"
        && path[2] == "providers"
        && path[4] == "models"
        && let Some(existing) = document::get(current, path).and_then(Value::as_array)
        && let Some(models) = value.as_array_mut()
    {
        // Selection owns membership and projected capabilities, not
        // native model preferences such as maxTokens or thinking budgets.
        for model in models {
            if let Some(native) = existing
                .iter()
                .find(|native| native.get("id") == model.get("id"))
                .and_then(Value::as_object)
            {
                let projected = model.as_object_mut().expect("DSH model object");
                for (key, value) in native {
                    projected
                        .entry(key.clone())
                        .or_insert_with(|| value.clone());
                }
            }
        }
    }
}

pub(super) fn withdraw_input(path: &[String], value: &mut Value, desired: &Value) {
    if path.len() != 5
        || path[0] != PROFILE_SETTINGS
        || path[1] != "config"
        || path[2] != "providers"
        || path[4] != "models"
    {
        return;
    }
    if let Some(models) = value.as_array_mut()
        && let Some(selected) = document::get(desired, path).and_then(Value::as_array)
    {
        for model in models {
            if selected.iter().any(|selected| {
                selected.get("id") == model.get("id") && selected.get("input").is_none()
            }) {
                model
                    .as_object_mut()
                    .expect("DSH model object")
                    .remove("input");
            }
        }
    }
}

pub(super) fn resolve_handoff(
    desired: &mut Value,
    resolve: &mut impl FnMut(&str) -> Result<Value>,
) -> Result<()> {
    if let Some(refs) = desired.get_mut("refs").and_then(Value::as_object_mut) {
        for value in refs.values_mut() {
            if let Some(reference) = value.as_str() {
                *value = resolve(reference)?;
            }
        }
    }
    Ok(())
}

pub(super) fn validate_credentials(current: &Value) -> Result<()> {
    if current.as_object().is_some_and(|object| !object.is_empty())
        && current.get("version") != Some(&json!(1))
    {
        return Err(Error::new(
            "native_schema_incompatible",
            "DSH credential document must use version 1; migrate it through DSH first",
        ));
    }
    Ok(())
}

pub(super) fn validate_existing(file: &File, current: &Value) -> Result<()> {
    if let Some(providers) = file
        .desired
        .pointer("/llm-pi-ai/config/providers")
        .and_then(Value::as_object)
    {
        for name in providers.keys() {
            let route_path = vec![
                PROFILE_SETTINGS.into(),
                "config".into(),
                "providers".into(),
                name.clone(),
            ];
            if document::get(current, &route_path)
                .is_some_and(|route| route.get("modelOverrides").is_some())
            {
                return Err(Error::new("native_config_incompatible", "DSH does not permit modelOverrides beside an explicit models list; migrate this route with DSH or choose a separate native provider entry before applying").field("modelOverrides"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::desired_fields;
    use super::super::test_support::target;
    use super::super::{model_metadata, projection_view};
    use super::*;
    use std::collections::BTreeMap;
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
    fn dsh_model_only_handoff_writes_profile_settings_and_never_installs_anything() {
        let scope = Path::new("/explicit/dsh");
        let mut target = target("dsh");
        target.default = None;
        for (protocol, expected_api) in [
            ("chat-completions", "openai-completions"),
            ("responses", "openai-responses"),
            ("messages", "anthropic-messages"),
        ] {
            let entry = dsh_entry(protocol);
            let credential_ref = dsh_credential_ref(scope, "desktop", &entry);
            let files = build(&target, scope, &[entry], "desktop").unwrap();
            // A DSH provider is a profile setting, so tmax writes the profile's
            // settings document and the credential value, nothing else.
            assert_eq!(files.len(), 2, "{files:?}");
            assert_eq!(files[0].path, scope.join(".credentials.yaml"));
            assert_eq!(files[0].desired["version"], 1);
            assert_eq!(
                files[0].desired["refs"][&credential_ref],
                "protected-key-reference"
            );
            assert_eq!(
                files[1].path,
                scope.join("profiles/desktop/cordis.patch.yml")
            );
            assert!(matches!(files[1].format, Format::DshPatch));
            let providers = &files[1].desired["llm-pi-ai"]["config"]["providers"];
            let provider = &providers["tmax-local"];
            assert_eq!(provider["api"], expected_api);
            assert_eq!(provider["apiKeyEnv"], credential_ref);
            assert_eq!(provider["baseURL"], "https://example.invalid/v1");
            assert_eq!(provider["models"], json!([{"id":"m"}]));
            let patch = &files[1].desired;
            assert!(patch.get("llm-tmax").is_none(), "{patch}");
            assert!(patch.get("tmax").is_none(), "{patch}");
            // A key-bearing provider asserts the service that resolves it, in
            // the same profile that carries the providers.
            assert_eq!(patch["credentials"]["disabled"], false);
            assert!(patch.get("agent-default-model").is_none());
            assert!(!patch.to_string().contains("protected-key-reference"));
        }
        let entry = dsh_entry("responses");
        assert_ne!(
            dsh_credential_ref(scope, "desktop", &entry),
            dsh_credential_ref(Path::new("/another/dsh"), "desktop", &entry)
        );
        // Two profiles may select the same entry differently, so the reference
        // that carries its key cannot be shared across them.
        assert_ne!(
            dsh_credential_ref(scope, "desktop", &entry),
            dsh_credential_ref(scope, "web", &entry)
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
                build(&target("dsh"), scope, &[entry], "desktop")
                    .unwrap_err()
                    .code,
                "incompatible"
            );
        }
        let files = build(
            &target("dsh"),
            scope,
            &[dsh_entry("chat-completions")],
            "desktop",
        )
        .unwrap();
        assert_eq!(
            files[1].desired["agent-default-model"],
            json!({"config":{"provider":"tmax-local","model":"m"}})
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
            let effective = json!({"reasoning_options":[{"type":"effort","values":values}],"modalities":{"input":["text"]}});
            let projection = projection_view("dsh", "responses", &effective).unwrap();
            assert_eq!(projection["state"], "blocked");
            assert_eq!(projection["reason_code"], "native_metadata_unrepresentable");
            assert_eq!(projection["input"]["state"], "projected");
            assert_eq!(projection["input"]["fields"], json!({"input":["text"]}));
            assert_eq!(
                model_metadata("dsh", "responses", &effective)
                    .unwrap_err()
                    .code,
                "native_metadata_unrepresentable"
            );
        }
    }
    #[test]
    fn dsh_selected_models_preserve_native_fields_by_identity() {
        let current = json!({"llm-pi-ai":{"config":{"providers":{"local":{"models":[
            {"id":"second","maxTokens":2048,"name":"Native second"},
            {"id":"first","maxTokens":1234,"contextWindow":4096,"thinkingBudgets":{"high":512}},
            {"id":"removed","maxTokens":99}
        ]}}}}});
        let desired = json!({"llm-pi-ai":{"config":{"providers":{"local":{"models":[
            {"id":"first","contextWindow":8192},
            {"id":"second"},
            {"id":"new"}
        ]}}}}});
        let fields = desired_fields("dsh", &desired, &current);
        let path: Vec<String> = vec![
            PROFILE_SETTINGS.into(),
            "config".into(),
            "providers".into(),
            "local".into(),
            "models".into(),
        ];
        assert_eq!(
            fields.iter().find(|(p, _)| p == &path).unwrap().1,
            Some(json!([
                {"id":"first","maxTokens":1234,"contextWindow":8192,"thinkingBudgets":{"high":512}},
                {"id":"second","maxTokens":2048,"name":"Native second"},
                {"id":"new"}
            ]))
        );
    }

    #[test]
    fn dsh_alias_unknown_input_withdraws_only_from_owned_models() {
        let current = json!({"llm-pi-ai":{"config":{"providers":{"local":{"models":[
            {"id":"shared","input":["text","image"],"maxTokens":1234,"thinkingBudgets":{"high":512}}
        ]}}}}});
        let mut model =
            crate::agent_integration::alias_model_metadata("dsh", "responses", &json!({})).unwrap();
        model["id"] = json!("shared");
        let desired = json!({"llm-pi-ai":{"config":{"providers":{"local":{"models":[model]}}}}});
        let path = vec![
            PROFILE_SETTINGS.into(),
            "config".into(),
            "providers".into(),
            "local".into(),
            "models".into(),
        ];
        let owned = vec![crate::agent_integration::recovery::Field {
            path: path.clone(),
            before_hash: "baseline".into(),
            after_hash: "managed".into(),
            observed_hash: "managed".into(),
            released: false,
        }];
        for (ownership, expected_input) in [
            (&[][..], Some(json!(["text", "image"]))),
            (owned.as_slice(), None),
        ] {
            let fields =
                super::super::desired_fields_with_ownership("dsh", &desired, &current, ownership);
            let models = fields
                .iter()
                .find(|(field, _)| field == &path)
                .unwrap()
                .1
                .as_ref()
                .unwrap();
            assert_eq!(models[0].get("input"), expected_input.as_ref());
            assert_eq!(models[0]["reasoningEfforts"], false);
            assert_eq!(models[0]["maxTokens"], 1234);
            assert_eq!(models[0]["thinkingBudgets"], json!({"high":512}));
        }
    }

    #[test]
    fn dsh_alias_reapply_clears_lost_common_efforts_without_changing_native_preferences() {
        let current = json!({
            "llm-pi-ai":{"config":{"providers":{"local":{"models":[{
                "id":"shared", "reasoningEfforts":{"low":"low","high":"high"},
                "maxTokens":1234,"thinkingBudgets":{"high":512}
            }]}}}},
            "agent-default-model":{"config":{"provider":"local","model":"shared","reasoningEffort":"high"}}
        });
        for (effective, expected) in [
            (
                json!({"reasoning_options":[{"type":"effort","values":["high"]}]}),
                json!({"high":"high"}),
            ),
            (json!({}), json!(false)),
        ] {
            let mut model = crate::agent_integration::alias_model_metadata(
                "dsh",
                "chat-completions",
                &effective,
            )
            .unwrap();
            model["id"] = json!("shared");
            let desired =
                json!({"llm-pi-ai":{"config":{"providers":{"local":{"models":[model]}}}}});
            let mut text = serde_yaml::to_string(&current).unwrap();
            for (path, value) in desired_fields("dsh", &desired, &current) {
                text = document::set(&text, Format::Yaml, &path, value.as_ref()).unwrap();
            }
            let after = document::parse(&text, Format::Yaml).unwrap();
            let row = &after["llm-pi-ai"]["config"]["providers"]["local"]["models"][0];
            assert_eq!(row["reasoningEfforts"], expected);
            assert_eq!(row["maxTokens"], 1234);
            assert_eq!(row["thinkingBudgets"], json!({"high":512}));
            assert_eq!(after["agent-default-model"], current["agent-default-model"]);
        }
    }

    #[test]
    fn dsh_default_effort_writes_reasoning_effort_only_when_configured() {
        use super::super::test_support::{effort, entry, project};
        let scope = Path::new("/explicit/dsh");
        let mut model_entry = entry("chat-completions");
        project(
            &mut model_entry,
            json!({"state":"projected","reason_code":null,"fields":{"reasoningEfforts":{"off":"off","max":"max"}},"omitted":[]}),
        );
        let mut target = target("dsh");
        effort(&mut target, "max");
        let files = build(
            &target,
            scope,
            std::slice::from_ref(&model_entry),
            "desktop",
        )
        .unwrap();
        assert_eq!(
            files[1].desired["agent-default-model"]["config"]["reasoningEffort"],
            "max"
        );
        target.default.as_mut().unwrap().effort = None;
        let files = build(&target, scope, &[model_entry], "desktop").unwrap();
        assert!(
            files[1].desired["agent-default-model"]["config"]
                .get("reasoningEffort")
                .is_none()
        );
    }

    #[test]
    fn dsh_default_effort_must_be_representable() {
        use super::super::test_support::{effort, entry, project};
        let mut model_entry = entry("chat-completions");
        project(
            &mut model_entry,
            json!({"state":"projected","reason_code":null,"fields":{"reasoningEfforts":{"max":"max"}},"omitted":[]}),
        );
        let mut target = target("dsh");
        effort(&mut target, "high");
        let error = build(
            &target,
            Path::new("/explicit/dsh"),
            &[model_entry],
            "desktop",
        )
        .unwrap_err();
        assert_eq!(error.code, "native_default_effort_unrepresentable");
        assert!(error.message.contains("available: max"));
    }

    #[test]
    fn dsh_default_update_preserves_an_unmanaged_reasoning_effort() {
        let current = json!({"agent-default-model":{"config":{
            "provider":"local","model":"m","reasoningEffort":"high"
        }}});
        let desired = json!({"agent-default-model":{"config":{
            "provider":"local","model":"m"
        }}});
        let fields = desired_fields("dsh", &desired, &current);
        assert!(
            !fields
                .iter()
                .any(|(path, _)| path.last().is_some_and(|key| key == "reasoningEffort"))
        );
        let mut text = serde_yaml::to_string(&current).unwrap();
        for (path, value) in fields {
            text = document::set(&text, Format::Yaml, &path, value.as_ref()).unwrap();
        }
        let after = document::parse(&text, Format::Yaml).unwrap();
        assert_eq!(
            after["agent-default-model"]["config"]["reasoningEffort"],
            "high"
        );

        let desired = json!({"agent-default-model":{"config":{
            "provider":"local","model":"m","reasoningEffort":"max"
        }}});
        let fields = desired_fields("dsh", &desired, &current);
        assert!(fields.iter().any(|(path, value)| {
            path.last().is_some_and(|key| key == "reasoningEffort")
                && value.as_ref() == Some(&json!("max"))
        }));
    }
}
