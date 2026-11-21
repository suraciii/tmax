//! Read-only view of the default selection a tool currently persists.
//!
//! Inspection reports the raw native values; it never imports them into desired
//! configuration or treats them as tmax-managed state.
use super::document::{self, Format};
use super::scope::{omp_main_settings, read_native};
use crate::{Error, Result};
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn view(agent: &str, scope: &Path) -> Value {
    match read(agent, scope) {
        Ok(fields) => json!({"state": "available", "fields": fields}),
        Err(error) => json!({"state": "unavailable", "reason_code": error.code}),
    }
}

fn read(agent: &str, scope: &Path) -> Result<Value> {
    let (path, format) = match agent {
        "dsh" => (scope.join("cordis.patch.yml"), Format::DshPatch),
        "pi" => (scope.join("settings.json"), Format::Json),
        "omp" => (scope.join(omp_main_settings(scope)), Format::Yaml),
        "opencode" => {
            let path =
                if scope.join("opencode.jsonc").exists() || !scope.join("opencode.json").exists() {
                    scope.join("opencode.jsonc")
                } else {
                    scope.join("opencode.json")
                };
            (path, Format::Json)
        }
        "codex" => (scope.join("config.toml"), Format::Toml),
        "hermes" => (scope.join("config.yaml"), Format::Yaml),
        _ => {
            return Err(Error::new(
                "unsupported_agent",
                "This Agent has no inspected native default selection",
            ));
        }
    };
    let Some(text) = read_native(&path)? else {
        return Ok(json!({"present": false}));
    };
    let value = document::parse(&text, format)?;
    let fields = match agent {
        "dsh" => {
            let config = value.pointer("/agent-default-model/config");
            json!({
                "present": config.and_then(|config| config.get("model")).is_some(),
                "provider": config.and_then(|config| config.get("provider")),
                "model": config.and_then(|config| config.get("model")),
                "effort": config.and_then(|config| config.get("reasoningEffort")),
            })
        }
        "pi" => json!({
            "present": value.get("defaultModel").is_some(),
            "provider": value.get("defaultProvider"),
            "model": value.get("defaultModel"),
            "effort": value.get("defaultThinkingLevel"),
        }),
        "omp" => json!({
            "present": value.pointer("/modelRoles/default").is_some(),
            "provider": Value::Null,
            "model": value.pointer("/modelRoles/default"),
            "effort": value.get("defaultThinkingLevel"),
        }),
        "opencode" => {
            let model = value.get("model").and_then(Value::as_str);
            let (model, effort) = model
                .map(|model| model.split_once('#').unwrap_or((model, "")))
                .unwrap_or(("", ""));
            json!({
                "present": !model.is_empty(),
                "provider": Value::Null,
                "model": if model.is_empty() { Value::Null } else { json!(model) },
                "effort": if effort.is_empty() { Value::Null } else { json!(effort) },
            })
        }
        "codex" => json!({
            "present": value.get("model").is_some(),
            "provider": value.get("model_provider"),
            "model": value.get("model"),
            "effort": value.get("model_reasoning_effort"),
        }),
        "hermes" => json!({
            "present": value.pointer("/model/default").is_some(),
            "provider": value.pointer("/model/provider"),
            "model": value.pointer("/model/default"),
            "effort": value.pointer("/agent/reasoning_effort"),
        }),
        _ => Value::Null,
    };
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::view;
    use serde_json::{Value, json};
    use std::fs;

    #[test]
    fn reports_each_native_default_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let scope = directory.path();
        let cases = [
            (
                "dsh",
                "cordis.patch.yml",
                "- id: agent-default-model\n  config: {provider: local, model: m, reasoningEffort: high}\n",
                json!("local"),
                json!("m"),
                json!("high"),
            ),
            (
                "pi",
                "settings.json",
                "{\"defaultProvider\":\"local\",\"defaultModel\":\"m\",\"defaultThinkingLevel\":\"low\"}",
                json!("local"),
                json!("m"),
                json!("low"),
            ),
            (
                "omp",
                "config.yml",
                "modelRoles:\n  default: local/m\ndefaultThinkingLevel: medium\n",
                Value::Null,
                json!("local/m"),
                json!("medium"),
            ),
            (
                "opencode",
                "opencode.jsonc",
                "{\"model\":\"local/m#max\"}",
                Value::Null,
                json!("local/m"),
                json!("max"),
            ),
            (
                "codex",
                "config.toml",
                "model = \"m\"\nmodel_provider = \"local\"\nmodel_reasoning_effort = \"xhigh\"\n",
                json!("local"),
                json!("m"),
                json!("xhigh"),
            ),
            (
                "hermes",
                "config.yaml",
                "model:\n  provider: local\n  default: m\nagent:\n  reasoning_effort: high\n",
                json!("local"),
                json!("m"),
                json!("high"),
            ),
        ];
        for (agent, file, text, provider, model, effort) in cases {
            let path = scope.join(file);
            fs::write(&path, text).unwrap();
            let before = fs::read(&path).unwrap();
            let view = view(agent, scope);
            assert_eq!(view["state"], "available", "{agent}");
            assert_eq!(view["fields"].get("provider"), Some(&provider), "{agent}");
            assert_eq!(view["fields"].get("model"), Some(&model), "{agent}");
            assert_eq!(view["fields"].get("effort"), Some(&effort), "{agent}");
            assert_eq!(fs::read(&path).unwrap(), before, "{agent}");
            fs::remove_file(path).unwrap();
        }
    }
}
