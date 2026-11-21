//! Shared native-target fixtures; compiled only for tests.
use crate::config::{self, AgentTarget};
use std::{collections::BTreeMap, path::PathBuf};

pub(super) fn inspection_target(scope: PathBuf) -> AgentTarget {
    AgentTarget {
        agent: "pi".into(),
        scope,
        providers: BTreeMap::from([(
            "local".into(),
            config::NativeProvider {
                provider: Some("p".into()),
                gateway: None,
                models: vec![config::ModelSelection {
                    id: "first".into(),
                    protocol: "chat-completions".into(),
                }],
            },
        )]),
        default: Some(config::DefaultReference {
            provider: "local".into(),
            id: "first".into(),
            protocol: "chat-completions".into(),
            effort: None,
            native_effort: None,
        }),
        roles: BTreeMap::new(),
    }
}
