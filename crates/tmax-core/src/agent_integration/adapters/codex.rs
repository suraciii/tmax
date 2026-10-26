use super::super::document::Format;
use super::credentials::credential_path;
use super::{Entry, File, file};
use crate::{Error, Result, config::AgentTarget};
use serde_json::json;
use std::path::Path;

pub(super) fn build(target: &AgentTarget, scope: &Path, entries: &[Entry]) -> Result<Vec<File>> {
    let default = target.default.as_ref().ok_or_else(|| {
        Error::new(
            "native_incomplete",
            "Select an explicit native default before applying",
        )
    })?;
    let mut providers = serde_json::Map::new();
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

#[cfg(test)]
mod tests {
    use super::super::build;
    use super::super::test_support::target;
    use super::*;
    use crate::config::ModelReference;
    use std::collections::BTreeMap;
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
}
