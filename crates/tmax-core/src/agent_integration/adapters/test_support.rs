use crate::config::{AgentTarget, ModelReference};
use std::collections::BTreeMap;

pub(super) fn target(agent: &str) -> AgentTarget {
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
