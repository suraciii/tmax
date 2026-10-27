use super::super::document::{self, Format};
use super::capabilities::{ReasoningProjection, api, catalog_model_metadata};
use super::{Entry, File, file};
use crate::{Error, Result, config::AgentTarget};
use serde_json::{Value, json};
use std::path::Path;

// DSH 0.2.1-rc.2 catalog.ts THINKING_LEVEL_GATE and resolveModelReasoning;
// independently qualified from Pi's thinkingLevelMap carrier.
const DSH_EFFORT_LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

pub(in crate::agent_integration) fn dsh_credential_ref(scope: &Path, entry: &Entry) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(scope.as_os_str().as_encoded_bytes());
    digest.update(b"\0");
    digest.update(entry.name.as_bytes());
    digest.update(b"\0");
    digest.update(entry.protocol.as_bytes());
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

pub(super) fn build(target: &AgentTarget, scope: &Path, entries: &[Entry]) -> Result<Vec<File>> {
    let mut providers = serde_json::Map::new();
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
    let mut patch = json!({"llm-pi-ai":{"disabled":false,"config":{"providers":providers}}});
    if let Some(default) = &target.default {
        let active = entries
            .iter()
            .find(|entry| entry.name == default.provider)
            .ok_or_else(|| Error::new("unresolved_reference", "Native default does not resolve"))?;
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

pub(super) fn inline_credential(path: &[String]) -> bool {
    path.len() == 2 && path[0] == "refs"
}

pub(super) fn preserve_models(path: &[String], value: &mut Value, current: &Value) {
    if path.len() == 5
        && path[0] == "llm-pi-ai"
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
        || path[0] != "llm-pi-ai"
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
                "llm-pi-ai".into(),
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
    use super::super::{build, model_metadata, projection_view};
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
            "llm-pi-ai".into(),
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
            "llm-pi-ai".into(),
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
}
