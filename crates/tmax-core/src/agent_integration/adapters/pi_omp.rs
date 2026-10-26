use super::super::document::Format;
use super::super::scope::omp_main_settings;
use super::capabilities::{ReasoningProjection, api, catalog_model_metadata};
use super::credentials::{credential_path, key_command};
use super::{Entry, File, file, reference};
use crate::{Error, Result, config::AgentTarget};
use serde_json::{Value, json};
use std::path::Path;

const PI_THINKING_LEVELS: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];
const OMP_EFFORT_LEVELS: [&str; 6] = ["minimal", "low", "medium", "high", "xhigh", "max"];

pub(super) fn pi_reasoning(values: &[String]) -> Result<ReasoningProjection> {
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

pub(super) fn omp_reasoning(values: &[String]) -> Result<ReasoningProjection> {
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

pub(super) fn model_metadata(effective: &Value) -> Result<Value> {
    catalog_model_metadata(effective, true)
}

pub(super) fn build(target: &AgentTarget, scope: &Path, entries: &[Entry]) -> Result<Vec<File>> {
    let mut providers = serde_json::Map::new();
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
        let models = if scope.join("models.yml").exists() || !scope.join("models.yaml").exists() {
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
                omp_main_settings(scope),
                Format::Yaml,
                json!({"modelRoles":roles}),
            ));
        }
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::target;
    use super::super::{build, model_metadata};
    use super::*;
    use std::collections::BTreeMap;
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
        let rewritten = super::super::super::document::set(
            original,
            settings.format,
            &["modelRoles".into()],
            Some(&settings.desired["modelRoles"]),
        )
        .unwrap();
        let effective = super::super::super::document::parse(&rewritten, settings.format).unwrap();
        assert_eq!(effective["modelRoleStorage"], "project");
        assert_eq!(effective["defaultThinkingLevel"], "high");
        assert_eq!(effective["modelRoles"]["default"], "local/m");
        assert!(rewritten.contains("# native policy"));
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
        let native = model_metadata(
            "pi",
            "chat-completions",
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
        assert_eq!(
            model_metadata("pi", "chat-completions", &json!({})).unwrap(),
            json!({})
        );
        assert_eq!(
            model_metadata(
                "pi",
                "chat-completions",
                &json!({"modalities":{"input":[]}})
            )
            .unwrap(),
            json!({"input":[]})
        );
        assert_eq!(
            model_metadata(
                "pi",
                "chat-completions",
                &json!({"modalities":{"input":["video"]}})
            )
            .unwrap(),
            json!({"input":[]})
        );
        for field in ["context", "output"] {
            for value in [json!(0), json!(9_007_199_254_740_992u64)] {
                let mut metadata = json!({"limit":{}});
                metadata["limit"][field] = value;
                let error = model_metadata("pi", "chat-completions", &metadata).unwrap_err();
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
            model_metadata: BTreeMap::from([("m".into(), model_metadata("pi", "chat-completions", &json!({
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
}
