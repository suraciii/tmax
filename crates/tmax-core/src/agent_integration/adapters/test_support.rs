use super::Entry;
use crate::config::{AgentTarget, DefaultReference};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn target(agent: &str) -> AgentTarget {
    AgentTarget {
        agent: agent.into(),
        scope: "/explicit/native".into(),
        providers: BTreeMap::new(),
        default: Some(DefaultReference {
            provider: "local".into(),
            id: "m".into(),
            protocol: if agent == "codex" {
                "responses"
            } else {
                "chat-completions"
            }
            .into(),
            effort: None,
            native_effort: None,
        }),
        roles: BTreeMap::new(),
    }
}

pub(super) fn entry(protocol: &str) -> Entry {
    Entry {
        name: "local".into(),
        base: "https://example.invalid/v1".into(),
        protocol: protocol.into(),
        key: Some("protected-reference".into()),
        login_provider: None,
        canonical: None,
        gateway: false,
        models: vec!["m".into()],
        model_routes: BTreeMap::new(),
        model_metadata: BTreeMap::new(),
        model_projection: BTreeMap::new(),
    }
}

pub(super) fn project(entry: &mut Entry, projection: Value) {
    entry.model_projection.insert("m".into(), projection);
}

pub(super) fn effort(target: &mut AgentTarget, effort: &str) {
    target.default.as_mut().expect("test default").effort = Some(effort.into());
}

pub(super) fn native_effort(target: &mut AgentTarget, effort: &str) {
    target.default.as_mut().expect("test default").native_effort = Some(effort.into());
}
