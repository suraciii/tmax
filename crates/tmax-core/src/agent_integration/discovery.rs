//! Installation and native-scope discovery without importing configuration.
use super::{
    adapters,
    installation::version,
    scope::{
        auth_paths, configuration_files, default_scope, omp_main_settings, profile_paths,
        read_native, revision, scope_evidence,
    },
};
use crate::{Error, Result, storage::Store};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) fn safe_files(agent: &str, path: &Path) -> Vec<Value> {
    configuration_files(agent, path).into_iter().map(|p| {
        let mut file = match read_native(&p) {
            Ok(text) => json!({"path":p,"present":text.is_some(),"revision":revision(text.as_deref()),"content":"not_imported"}),
            Err(e) => json!({"path":p,"state":"unavailable","reason_code":e.code}),
        };
        if agent == "omp" && matches!(p.file_name().and_then(|n| n.to_str()), Some("config.yml" | "config.yaml")) {
            let effective = path.join(omp_main_settings(path));
            file["main_settings"] = json!(true);
            file["effective"] = json!(p == effective);
            file["shadowed_by"] = if p != effective && p.exists() { json!(effective) } else { Value::Null };
        }
        file
    }).collect()
}
pub async fn discover(store: &Store, agent: Option<&str>, path: Option<&str>) -> Result<Value> {
    if path.is_some() && agent.is_none() {
        return Err(Error::new(
            "invalid_arguments",
            "An explicit native path requires --agent",
        ));
    }
    if agent.is_some_and(|a| !adapters::AGENTS.contains(&a)) {
        return Err(Error::new(
            "unsupported_agent",
            "Supported agents are codex, pi, omp, opencode, hermes, and dsh",
        ));
    }
    let mut installations = Vec::new();
    for a in adapters::AGENTS
        .into_iter()
        .filter(|a| agent.is_none_or(|wanted| wanted == *a))
    {
        let selected = path
            .map(PathBuf::from)
            .unwrap_or_else(|| default_scope(store, a));
        let (resolved, reason) = scope_evidence(store, &selected)?;
        let available = reason == "available";
        // Version commands use an existing directory and never select credentials.
        let evidence = version(a, if available { &resolved } else { &store.home }).await;
        let scopes = profile_paths(store, a);
        installations.push(json!({"target":null,"agent":a,"path":resolved,"scope_state":if available{"available"}else{"unavailable"},"scope_reason_code":if available{None}else{Some(reason)},"installation":evidence,"profiles":scopes,"files":if available{safe_files(a,&resolved)}else{Vec::new()},"credential_scopes":if available{auth_paths(a,&resolved).iter().map(|p|json!({"path":p,"present":p.exists(),"contents":"not_read","owner":a})).collect::<Vec<_>>()}else{Vec::new()},"precedence":adapters::precedence(a),"supported_roles":adapters::supported_roles(a),"imported":false}));
    }
    Ok(if path.is_some() {
        installations.remove(0)
    } else {
        json!({"installations":installations})
    })
}
