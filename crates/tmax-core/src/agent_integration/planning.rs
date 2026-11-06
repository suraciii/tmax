//! Desired target resolution and safe native projection/comparison.
#[cfg(test)]
use super::adapters::desired_fields;
use super::{
    adapters::{self, Entry, File, inline_credential},
    authentication::auth_namespace_present,
    compatibility,
    discovery::safe_files,
    document::{self, Format},
    installation::{version, version_blocker, version_warnings},
    recovery::{INDEX, Index, Record, field_drift, field_hash, owner, record_for, record_view},
    scope::{dsh_preflight, read_native, scope_evidence, target, unavailable},
};
use crate::{
    Error, Result,
    config::{self, AgentTarget, Auth, Config},
    storage::Store,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

pub(crate) fn reasoning_projection(effective: &Value, protocols: &[String]) -> Value {
    let view = |agent: &str, protocol: &str| {
        adapters::projection_view(agent, protocol, effective).unwrap_or_else(|error| {
            json!({"state":"blocked","reason_code":error.code,"message":error.message,"fields":{},"omitted":[]})
        })
    };
    Value::Object(
        adapters::AGENTS
            .into_iter()
            .map(|agent| {
                let mut projection = view(
                    agent,
                    protocols
                        .first()
                        .map(String::as_str)
                        .unwrap_or("chat-completions"),
                );
                if agent == "opencode" {
                    projection["protocols"] = Value::Object(
                        protocols
                            .iter()
                            .map(|protocol| (protocol.clone(), view(agent, protocol)))
                            .collect(),
                    );
                }
                (agent.to_owned(), projection)
            })
            .collect(),
    )
}
pub async fn inspect(store: &Store, config: &Config, id: &str) -> Result<Value> {
    let target = target(config, id)?;
    let (path, scope_reason) = scope_evidence(store, &target.scope)?;
    let available = scope_reason == "available";
    let index: Index = store.read_json(INDEX)?;
    let identity = owner(store, id)?;
    let record = record_for(&index, identity.as_deref(), &path);
    let mut blockers = Vec::new();
    if !available {
        blockers.push(json!({"code":"native_scope_unavailable","path":path,"reason_code":scope_reason,"message":"Select an existing native configuration directory or explicitly create the intended directory before applying"}));
    }
    if (target.agent == "codex" && target.default.is_none())
        || target.providers.is_empty()
        || target.providers.values().any(|p| p.models.is_empty())
    {
        blockers.push(json!({"code":"target_incomplete","message":"Select providers and models before applying; Codex also requires a default model"}));
    }
    let mut pending = None;
    let mut native_projection = None;
    let mut fact_gaps = Vec::new();
    if available {
        let plan = match entries(store, config, id, target, &path, true).await {
            Ok((entries, references)) => {
                fact_gaps = fact_gap_warnings(target, &entries);
                native_projection = Some(projection_view(target, &entries));
                adapters::build(target, &path, &entries)
                    .and_then(|files| pending_native_changes(target, &files, &references, record))
            }
            Err(error) => Err(error),
        };
        match plan {
            Ok(changes) => pending = Some(changes),
            Err(error) => blockers
                .push(json!({"code":error.code,"field":error.field,"message":error.message})),
        }
        if let Some(record) = record {
            match field_drift(record, false) {
                Ok(drift) => blockers.extend(drift),
                Err(error) => blockers.push(json!({"code":error.code,"message":error.message})),
            }
        }
    }
    let installed = if available {
        version(&target.agent, &path).await
    } else {
        json!({"state":"not_checked","version":null,"compatibility":"unknown","reason_code":"native_scope_unavailable"})
    };
    if available && let Some(blocker) = version_blocker(&target.agent, &installed) {
        blockers.push(blocker);
    }
    let applied_state = match &pending {
        None => "unknown",
        Some(_) if record.is_none() => "not_applied",
        Some(changes) if !changes.is_empty() => "pending_native_changes",
        Some(_) => record.map(|r| r.state.as_str()).unwrap_or("not_applied"),
    };
    Ok(
        json!({"target":id,"agent":target.agent,"path":path,"scope_state":if available{"available"}else{"unavailable"},"scope_reason_code":if available{None}else{Some(scope_reason)},"providers":target.providers.iter().map(|(name,p)|json!({"name":name,"source":if let Some(p)=&p.provider{json!({"provider":p})}else{json!({"gateway":true})},"models":p.models})).collect::<Vec<_>>(),"default":target.default,"roles":target.roles,"supported_roles":adapters::supported_roles(&target.agent),"native_projection":native_projection,"fact_gaps":fact_gaps,"installation":installed,"warnings":version_warnings(&target.agent,&installed),"precedence":adapters::precedence(&target.agent),"files":if available{safe_files(&target.agent,&path)}else{Vec::new()},"credential_handoff":{"owner":target.agent,"plaintext_in_native_configuration":target.agent == "hermes","plaintext_private_native_credential_file":target.agent != "hermes","reference":if target.agent == "hermes" {"native api_key in current-user-restricted config.yaml"} else {"native command/file helper"},"secret_reads":false,"native_auth_import":false},"desired_state":"saved","applied_state":applied_state,"last_application_state":record.map(|r|r.state.as_str()),"pending_native_changes":pending,"blockers":blockers,"recovery":record.map(|r|record_view(store,r))}),
    )
}

pub(super) fn projection_view(target: &AgentTarget, entries: &[Entry]) -> Vec<Value> {
    entries
        .iter()
        .map(|entry| {
            let source = target
                .providers
                .get(&entry.name)
                .map(|binding| {
                    if let Some(provider) = &binding.provider {
                        json!({"provider":provider})
                    } else {
                        json!({"gateway":true})
                    }
                })
                .unwrap_or(Value::Null);
            json!({
                "name":entry.name,
                "source":source,
                "models":entry.models.iter().map(|id|json!({
                    "id":id,
                    "protocol":entry.model_routes.get(id).map(|(protocol,_)|protocol.as_str()).unwrap_or(entry.protocol.as_str()),
                    "projection":entry.model_projection.get(id).cloned().unwrap_or_else(||json!({"state":"unavailable","reason_code":"native_projection_unavailable","fields":{},"omitted":[]}))
                })).collect::<Vec<_>>()
            })
        })
        .collect()
}

/// Visible model-fact gaps for one target: models whose native entry will
/// stay bare because no catalog or declared fact supplies them.
pub(super) fn fact_gap_warnings(target: &AgentTarget, entries: &[Entry]) -> Vec<Value> {
    let completion = adapters::native_completion(&target.agent);
    let mut warnings = Vec::new();
    for entry in entries {
        for (id, projection) in &entry.model_projection {
            let mut missing = Vec::new();
            if projection.get("reason_code").and_then(Value::as_str)
                == Some("reasoning_options_unavailable")
            {
                missing.push("reasoning_efforts");
            }
            if projection
                .pointer("/limits/fields/contextWindow")
                .is_none_or(Value::is_null)
            {
                missing.push("limit.context");
            }
            if !missing.is_empty() {
                warnings.push(json!({
                    "code":"model_fact_gap",
                    "entry":entry.name,
                    "model":id,
                    "missing":missing,
                    "agent_self_completion":completion,
                    "message":"No catalog or declared fact supplies the missing fields; the native entry stays bare for them. Declare model overrides or accept the degraded entry.",
                }));
            }
        }
    }
    warnings
}

pub(super) async fn entries(
    store: &Store,
    config: &Config,
    id: &str,
    target: &AgentTarget,
    path: &Path,
    _dry_run: bool,
) -> Result<(Vec<Entry>, Vec<String>)> {
    if target.agent == "dsh" {
        dsh_preflight(path)?;
    }
    let mut entries = Vec::new();
    let mut references = Vec::new();
    let authorization = compatibility::AuthorizationSnapshot::capture(store)?;
    let metadata = if matches!(
        target.agent.as_str(),
        "pi" | "omp" | "opencode" | "hermes" | "dsh"
    ) {
        crate::catalog::read(store, "accepted")?
    } else {
        None
    };
    for (name, binding) in &target.providers {
        if binding.models.is_empty() {
            return Err(Error::new(
                "target_incomplete",
                "Every native provider entry requires an explicit model selection",
            ));
        }
        let protocols: BTreeSet<&str> =
            binding.models.iter().map(|m| m.protocol.as_str()).collect();
        if protocols.len() != 1 && matches!(target.agent.as_str(), "codex" | "hermes" | "dsh") {
            return Err(Error::new(
                "incompatible",
                "This native provider has one effective transport; select separate native entries for different protocols",
            ));
        }
        let protocol = *protocols
            .first()
            .ok_or_else(|| Error::new("target_incomplete", "Select a native protocol"))?;
        let requirements = compatibility::Context {
            agent: &target.agent,
            target: Some(id),
            provider: binding.provider.as_deref(),
            gateway: binding.gateway == Some(true),
        };
        requirements.protocol(protocol)?;
        let mut canonical = None;
        let mut model_routes = BTreeMap::new();
        let mut model_metadata = BTreeMap::new();
        let mut model_projection = BTreeMap::new();
        let (base, key_ref, login_provider) = if let Some(provider_id) = &binding.provider {
            let provider = config.providers.get(provider_id).ok_or_else(|| {
                Error::new(
                    "unresolved_reference",
                    "Native upstream provider does not resolve",
                )
            })?;
            if !provider.enabled {
                return Err(Error::new(
                    "model_unavailable",
                    "Native upstream provider is disabled",
                ));
            }
            let integration = crate::provider_integration::integration_id(provider)?;
            for selected in &binding.models {
                let model = provider.models.get(&selected.id).ok_or_else(|| {
                    Error::new(
                        "unresolved_reference",
                        "Native upstream model does not resolve",
                    )
                })?;
                if !model.enabled {
                    return Err(Error::new(
                        "model_unavailable",
                        "Native upstream model is disabled",
                    ));
                }
                requirements.assess(config, provider_id, &selected.protocol, &selected.id)?;
                if matches!(
                    target.agent.as_str(),
                    "pi" | "omp" | "opencode" | "hermes" | "dsh"
                ) {
                    let projection = crate::catalog::effective_from_snapshot(
                        metadata.as_ref(),
                        provider,
                        Some((&selected.id, model)),
                    )?;
                    model_metadata.insert(
                        selected.id.clone(),
                        adapters::model_metadata(
                            &target.agent,
                            &selected.protocol,
                            &projection["effective"],
                        )?,
                    );
                    model_projection.insert(
                        selected.id.clone(),
                        adapters::projection_view(
                            &target.agent,
                            &selected.protocol,
                            &projection["effective"],
                        )?,
                    );
                }
            }
            if integration == "opencode-go" && target.agent == "pi" {
                canonical = Some("opencode-go".to_owned());
            }
            let bases = crate::provider_integration::resolve(provider)?;
            for selected in &binding.models {
                let endpoint = bases.get(&selected.protocol).cloned().ok_or_else(|| {
                    Error::new(
                        "incompatible",
                        "The upstream integration has no endpoint for a selected native protocol",
                    )
                })?;
                if model_routes
                    .insert(selected.id.clone(), (selected.protocol.clone(), endpoint))
                    .is_some()
                {
                    return Err(Error::new(
                        "native_namespace_conflict",
                        "One native provider/model ID cannot simultaneously name two protocol variants",
                    ));
                }
            }
            let base = bases.get(protocol).cloned().ok_or_else(|| {
                Error::new(
                    "incompatible",
                    "The upstream integration has no endpoint for this native protocol",
                )
            })?;
            match provider.auth.as_ref() {
                Some(Auth::ApiKey { key_ref }) => (base, Some(key_ref.clone()), None),
                Some(Auth::Login { .. }) => {
                    let native_provider = requirements
                        .login_provider(provider)?
                        .ok_or_else(unavailable)?;
                    authorization.assess(store, config, &requirements, provider_id)?;
                    (base, None, Some(native_provider.to_owned()))
                }
                None => {
                    return Err(Error::new(
                        "credential_unavailable",
                        "Configure the selected provider's API key or native login before application",
                    ));
                }
            }
        } else if binding.gateway == Some(true) {
            let gateway = config.gateway.as_ref().ok_or_else(|| {
                Error::new("model_unavailable", "The sole gateway is not configured")
            })?;
            let status = crate::service::status(store).await?;
            let desired_revision = config::load(store)?.revision;
            let ready = &status["gateway"];
            if ready["ready"] != true
                || ready["listen"] != gateway.listen
                || ready["key_ref"] != gateway.key_ref
                || ready["config_revision"] != desired_revision
            {
                return Err(Error::new(
                    "gateway_not_ready",
                    "Explicitly start the configured gateway and activate the matching complete snapshot and shared key before native application",
                ));
            }
            for m in &binding.models {
                requirements.assess_public(config, &m.id, &m.protocol)?;
                if let Some(alias) = config
                    .model_aliases
                    .iter()
                    .find(|a| a.id == m.id && a.protocol == m.protocol && a.enabled)
                {
                    let effective =
                        crate::reasoning::alias_metadata(config, metadata.as_ref(), alias)?;
                    model_metadata.insert(
                        m.id.clone(),
                        adapters::alias_model_metadata(&target.agent, &m.protocol, &effective)?,
                    );
                    model_projection.insert(
                        m.id.clone(),
                        adapters::projection_view(&target.agent, &m.protocol, &effective)?,
                    );
                } else {
                    let (_, model_id) = config::qualified(&m.id)?;
                    let (p, model) = config::model(config, &m.id)?;
                    if !p.enabled
                        || !model.enabled
                        || !crate::provider_integration::protocols(p)?
                            .iter()
                            .any(|p| p == &m.protocol)
                    {
                        return Err(Error::new(
                            "model_unavailable",
                            "Selected public concrete gateway model is not published for this protocol",
                        ));
                    }
                    if matches!(
                        target.agent.as_str(),
                        "pi" | "omp" | "opencode" | "hermes" | "dsh"
                    ) {
                        let projection = crate::catalog::effective_from_snapshot(
                            metadata.as_ref(),
                            p,
                            Some((model_id, model)),
                        )?;
                        let inputs = crate::reasoning::input_metadata(
                            &projection["effective"],
                            model,
                            &m.protocol,
                        );
                        model_metadata.insert(
                            m.id.clone(),
                            adapters::model_metadata(&target.agent, &m.protocol, &inputs)?,
                        );
                        model_projection.insert(
                            m.id.clone(),
                            adapters::projection_view(&target.agent, &m.protocol, &inputs)?,
                        );
                    }
                }
            }
            (
                format!("http://{}", gateway.listen),
                Some(gateway.key_ref.clone()),
                None,
            )
        } else {
            return Err(Error::new(
                "invalid_configuration",
                "Native entry requires exactly one upstream or gateway source",
            ));
        };
        if model_routes.is_empty() {
            for selected in &binding.models {
                if model_routes
                    .insert(
                        selected.id.clone(),
                        (selected.protocol.clone(), base.clone()),
                    )
                    .is_some()
                {
                    return Err(Error::new(
                        "native_namespace_conflict",
                        "One native model ID cannot name two protocol variants",
                    ));
                }
            }
        }
        if key_ref.is_some()
            && target.agent == "pi"
            && auth_namespace_present(
                &path.join("auth.json"),
                canonical.as_deref().unwrap_or(name),
            )?
        {
            return Err(Error::new(
                "native_credential_precedence_unknown",
                "The selected exact native provider namespace has saved authentication that can override this key reference; resolve it through the native owner or select an independent scope. Credential values were not materialized",
            ));
        }
        if target.agent == "codex" {
            for selected in &binding.models {
                model_projection.insert(
                    selected.id.clone(),
                    adapters::projection_view(&target.agent, &selected.protocol, &Value::Null)?,
                );
            }
        }
        let key = key_ref.inspect(|reference| {
            references.push(reference.clone());
        });
        entries.push(Entry {
            name: name.clone(),
            base,
            protocol: protocol.into(),
            key,
            login_provider,
            canonical,
            gateway: binding.gateway == Some(true),
            models: binding.models.iter().map(|m| m.id.clone()).collect(),
            model_routes,
            model_metadata,
            model_projection,
        });
    }
    if (target.agent == "codex" && target.default.is_none()) || entries.is_empty() {
        return Err(Error::new(
            "target_incomplete",
            "Select providers and models before applying; Codex also requires a default model",
        ));
    }
    let mut names = BTreeSet::new();
    for entry in &entries {
        if !names.insert(entry.native_name()) {
            return Err(Error::new(
                "native_scope_conflict",
                "Two desired entries resolve to one native provider/authentication namespace",
            ));
        }
    }
    Ok((entries, references))
}
pub(super) fn pending_native_changes(
    target: &AgentTarget,
    files: &[File],
    references: &[String],
    record: Option<&Record>,
) -> Result<Vec<Value>> {
    let mut changes = Vec::new();
    for file in files
        .iter()
        .filter(|file| file.format != Format::Credential)
    {
        let current = document::parse(
            read_native(&file.path)?.as_deref().unwrap_or(""),
            file.format,
        )?;
        let previous = record.and_then(|record| {
            record
                .files
                .iter()
                .find(|previous| previous.path == file.path)
        });
        let desired = adapters::desired_fields_with_ownership(
            &target.agent,
            &file.desired,
            &current,
            previous.map_or(&[], |file| file.fields.as_slice()),
        );
        for (path, value) in &desired {
            if !previous.is_some_and(|file| {
                file.written
                    && file.fields.iter().any(|field| {
                        !field.released
                            && field.path == *path
                            && field.after_hash == field_hash(document::get(&current, path))
                    })
                    && (inline_credential(&target.agent, path)
                        || document::get(&current, path) == value.as_ref())
            }) {
                changes.push(json!({"path":file.path,"field":path,"change":if value.is_some(){"set"}else{"remove"}}));
            }
        }
        if let Some(previous) = previous {
            for field in previous.fields.iter().filter(|field| !field.released) {
                if !desired.iter().any(|(path, _)| *path == field.path) {
                    changes.push(json!({"path":file.path,"field":field.path,"change":"release"}));
                }
            }
        }
    }
    if let Some(record) = record {
        for file in record.files.iter().filter(|file| {
            file.format != Format::Credential
                && !files.iter().any(|desired| desired.path == file.path)
        }) {
            for field in file.fields.iter().filter(|field| !field.released) {
                changes.push(json!({"path":file.path,"field":field.path,"change":"release"}));
            }
        }
    }
    if record
        .map(|record| record.secret_refs.as_slice())
        .unwrap_or_default()
        != references
    {
        changes.push(
            json!({"field":["credential_reference"],"change":"replace","secret_reads":false}),
        );
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::super::{
        application::apply,
        recovery::{Field, FileRecord},
        test_support::inspection_target,
    };
    use super::*;
    use std::fs;
    #[tokio::test]
    async fn unavailable_scope_inspection_retains_desired_target_and_apply_stays_blocked() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_owned());
        let mut config = Config::empty();
        let path = directory.path().join("not-created");
        config
            .agents
            .insert("managed".into(), inspection_target(path.clone()));
        let inspected = inspect(&store, &config, "managed").await.unwrap();
        assert_eq!(inspected["path"], json!(path));
        assert_eq!(inspected["scope_reason_code"], "absent");
        assert_eq!(inspected["default"]["id"], "first");
        assert_eq!(inspected["providers"][0]["models"][0]["id"], "first");
        assert_eq!(inspected["applied_state"], "unknown");
        assert!(inspected["pending_native_changes"].is_null());
        let error = apply(&store, &config, "managed", false).await.unwrap_err();
        assert_eq!(error.code, "native_scope_unavailable");
        assert!(error.message.contains(path.to_str().unwrap()));
        assert!(!store.root.exists());
        fs::write(&path, b"not a directory").unwrap();
        let inspected = inspect(&store, &config, "managed").await.unwrap();
        assert_eq!(inspected["scope_reason_code"], "not_directory");
        assert_eq!(inspected["files"], json!([]));
    }
    #[test]
    fn current_native_projection_reports_changed_default_separately_from_last_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let target = inspection_target(directory.path().to_owned());
        fs::write(&path, b"{\"defaultModel\":\"first\"}").unwrap();
        let mut record = Record {
            id: "retained".into(),
            owner: "stable".into(),
            target: "managed".into(),
            agent: "pi".into(),
            scope: target.scope.clone(),
            payload_ref: "not-read".into(),
            secret_refs: vec![],
            state: "applied_reload_pending".into(),
            files: vec![FileRecord {
                path: path.clone(),
                format: Format::Json,
                original_absent: false,
                after_revision: "old".into(),
                written: true,
                fields: vec![Field {
                    path: vec!["defaultModel".into()],
                    after_hash: field_hash(Some(&json!("first"))),
                    before_hash: "absent".into(),
                    observed_hash: "absent".into(),
                    released: false,
                }],
            }],
        };
        let files = [File {
            path: path.clone(),
            format: Format::Json,
            desired: json!({"defaultModel":"second"}),
        }];
        let changes = pending_native_changes(&target, &files, &[], Some(&record)).unwrap();
        assert_eq!(
            changes,
            vec![json!({"path":path,"field":["defaultModel"],"change":"set"})]
        );
        assert_eq!(record.state, "applied_reload_pending");
        record.files[0].fields[0].after_hash = field_hash(Some(&json!("second")));
        fs::write(&path, b"{\"defaultModel\":\"second\"}").unwrap();
        assert!(
            pending_native_changes(&target, &files, &[], Some(&record))
                .unwrap()
                .is_empty()
        );
        record.files[0].written = false;
        record.state = "recovery_required".into();
        assert_eq!(
            pending_native_changes(&target, &files, &[], Some(&record)).unwrap()[0]["field"],
            json!(["defaultModel"])
        );
        record.files[0].written = true;
        fs::write(&path, b"{\"defaultModel\":\"first\"}").unwrap();
        assert_eq!(
            pending_native_changes(&target, &files, &[], Some(&record)).unwrap()[0]["change"],
            "set"
        );
        assert_eq!(
            pending_native_changes(&target, &[], &[], Some(&record)).unwrap()[0]["change"],
            "release"
        );
    }
    #[tokio::test]
    async fn entries_use_accepted_effective_concrete_metadata_without_native_preference_edits() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().join("tmax"));
        let accepted = crate::catalog::Snapshot {
            revision: "accepted".into(),
            providers: BTreeMap::from([(
                "catalog".into(),
                json!({"models":{"m":{
                    "limit":{"context":1000000,"output":65536},"reasoning":true,"tool_call":true,
                    "modalities":{"input":["text","image","video"],"output":["text"]}
                }}}),
            )]),
            ..Default::default()
        };
        store
            .write_json("catalog/accepted.json", &accepted)
            .unwrap();
        let mut latest = accepted.clone();
        latest.revision = "not-accepted".into();
        latest.providers.get_mut("catalog").unwrap()["models"]["m"]["limit"]["context"] = json!(17);
        store.write_json("catalog/latest.json", &latest).unwrap();
        let mut provider = config::Provider {
            integration: Some("custom".into()),
            metadata: Some("catalog".into()),
            auth: Some(Auth::ApiKey {
                key_ref: "opaque-reference-not-read".into(),
            }),
            endpoints: BTreeMap::from([(
                "chat-completions".into(),
                "https://example.invalid/v1".into(),
            )]),
            ..Default::default()
        };
        let mut model = config::Model::default();
        model.overrides.reasoning = Some(false);
        model.overrides.tool_call = Some(false);
        model.overrides.limit = Some(config::Limits {
            context: None,
            input: None,
            output: Some(4096),
        });
        model.overrides.modalities = Some(config::Modalities {
            input: Some(vec!["image".into()]),
            output: None,
        });
        provider.models = BTreeMap::from([
            ("m".into(), model),
            ("unknown".into(), config::Model::default()),
        ]);
        let mut config = Config::empty();
        config.providers.insert("p".into(), provider);
        let target = AgentTarget {
            agent: "pi".into(),
            scope: temporary.path().join("native"),
            providers: BTreeMap::from([(
                "local".into(),
                config::NativeProvider {
                    provider: Some("p".into()),
                    gateway: None,
                    models: ["m", "unknown"]
                        .into_iter()
                        .map(|id| config::ModelSelection {
                            id: id.into(),
                            protocol: "chat-completions".into(),
                        })
                        .collect(),
                },
            )]),
            default: Some(config::ModelReference {
                provider: "local".into(),
                id: "m".into(),
                protocol: "chat-completions".into(),
            }),
            roles: BTreeMap::new(),
        };
        let (entries, _) = entries(&store, &config, "target", &target, &target.scope, true)
            .await
            .unwrap();
        assert_eq!(
            entries[0].model_metadata["m"],
            json!({"contextWindow":1000000,"maxTokens":4096,"reasoning":false,"input":["image"]})
        );
        assert_eq!(entries[0].model_metadata["unknown"], json!({}));
        let files = adapters::build(&target, &target.scope, &entries).unwrap();
        let models = files
            .iter()
            .find(|file| file.path == target.scope.join("models.json"))
            .unwrap();
        let settings = files
            .iter()
            .find(|file| file.path == target.scope.join("settings.json"))
            .unwrap();
        assert_eq!(
            models.desired["providers"]["local"]["models"][0]["maxTokens"],
            4096
        );
        let original = r#"{"defaultThinkingLevel":"high","modelThinkingLevels":{"local/m":"low"},"unrelated":{"samplingParams":{"temperature":0.2},"maxTokens":777}}"#;
        let mut rewritten = original.to_owned();
        let fields = desired_fields(
            "pi",
            &settings.desired,
            &document::parse(original, Format::Json).unwrap(),
        );
        for (path, value) in fields {
            rewritten = document::set(&rewritten, Format::Json, &path, value.as_ref()).unwrap();
        }
        let after = document::parse(&rewritten, Format::Json).unwrap();
        let before = document::parse(original, Format::Json).unwrap();
        for field in ["defaultThinkingLevel", "modelThinkingLevels", "unrelated"] {
            assert_eq!(after[field], before[field]);
        }
        for agent in ["opencode", "hermes"] {
            let mut target = target.clone();
            target.agent = agent.into();
            let (prepared, references) =
                super::entries(&store, &config, "target", &target, &target.scope, true)
                    .await
                    .unwrap();
            assert_eq!(references, ["opaque-reference-not-read"]);
            let files = adapters::build(&target, &target.scope, &prepared).unwrap();
            let file = files
                .iter()
                .find(|file| file.format != Format::Credential)
                .unwrap();
            let native = &file.desired["providers"]["local"]["models"];
            if agent == "opencode" {
                assert_eq!(
                    native["m"]["limit"],
                    json!({"context":1000000,"output":4096})
                );
                assert_eq!(
                    native["m"]["capabilities"],
                    json!({"tools":false,"input":["image"],"output":["text"]})
                );
                assert!(native["m"].get("reasoning").is_none());
                assert!(native["unknown"].get("limit").is_none());
                assert!(native["unknown"].get("capabilities").is_none());
            } else {
                assert_eq!(native["m"], json!({"context_length":1000000}));
                assert_eq!(
                    file.desired["model_overrides"]["local"]["m"],
                    json!({
                        "context_window":1000000,"supports_reasoning":false,"supports_tools":false,"supports_vision":true
                    })
                );
                assert_eq!(native["unknown"], json!({}));
                assert!(
                    file.desired["model_overrides"]["local"]
                        .get("unknown")
                        .is_none()
                );
            }
            // Exercise the same leaf writer as apply, including nested provider models.
            // Existing native request bodies/effort/budgets and unrelated models survive.
            let original = json!({
                "providers":{"local":{"models":{"m":{
                    "settings":{"temperature":0.2,"maxTokens":777},
                    "body":{"max_output_tokens":777,"reasoning":{"effort":"high"}}
                },"unknown":{"body":{"max_tokens":444},"capabilities":{"supports_vision":true}},
                "user-model":{"body":{"temperature":0.8}}}}},
                "agent":{"reasoning_effort":"high"},"reasoning_overrides":{"m":"low"}
            });
            let mut rewritten = if file.format == Format::Yaml {
                serde_yaml::to_string(&original).unwrap()
            } else {
                serde_json::to_string(&original).unwrap()
            };
            let fields = desired_fields(agent, &file.desired, &original);
            for (path, value) in fields {
                rewritten = document::set(&rewritten, file.format, &path, value.as_ref()).unwrap();
            }
            let after = document::parse(&rewritten, file.format).unwrap();
            assert_eq!(
                after["providers"]["local"]["models"]["m"]["body"],
                original["providers"]["local"]["models"]["m"]["body"]
            );
            for field in ["temperature", "maxTokens"] {
                assert_eq!(
                    after["providers"]["local"]["models"]["m"]["settings"][field],
                    original["providers"]["local"]["models"]["m"]["settings"][field]
                );
            }
            assert_eq!(
                after["providers"]["local"]["models"]["user-model"],
                original["providers"]["local"]["models"]["user-model"]
            );
            for field in ["body", "capabilities"] {
                assert_eq!(
                    after["providers"]["local"]["models"]["unknown"][field],
                    original["providers"]["local"]["models"]["unknown"][field]
                );
            }
            for field in ["agent", "reasoning_overrides"] {
                assert_eq!(after[field], original[field]);
            }
        }
    }

    #[tokio::test]
    async fn direct_entries_retain_upstream_efforts_while_gateway_projection_uses_mapped_inputs() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().join("tmax"));
        let mut model = config::Model {
            overrides: serde_json::from_value(json!({
                "reasoning":true,
                "reasoning_options":[{"type":"effort","values":["high","max"]}]
            }))
            .unwrap(),
            ..Default::default()
        };
        model.effort_map.insert(
            "chat-completions".into(),
            BTreeMap::from([("xhigh".into(), "high".into())]),
        );
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                integration: Some("custom".into()),
                auth: Some(Auth::ApiKey {
                    key_ref: "opaque-reference-not-read".into(),
                }),
                endpoints: BTreeMap::from([(
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                )]),
                models: BTreeMap::from([("m".into(), model)]),
                ..Default::default()
            },
        );
        let target = AgentTarget {
            agent: "omp".into(),
            scope: temporary.path().join("native"),
            providers: BTreeMap::from([(
                "local".into(),
                config::NativeProvider {
                    provider: Some("p".into()),
                    gateway: None,
                    models: vec![config::ModelSelection {
                        id: "m".into(),
                        protocol: "chat-completions".into(),
                    }],
                },
            )]),
            default: None,
            roles: BTreeMap::new(),
        };
        let (direct, _) = entries(&store, &config, "target", &target, &target.scope, true)
            .await
            .unwrap();
        assert_eq!(
            direct[0].model_metadata["m"]["thinking"],
            json!({"mode":"effort","efforts":["high","max"]})
        );
        let provider = &config.providers["p"];
        let model = &provider.models["m"];
        let upstream =
            crate::catalog::effective_from_snapshot(None, provider, Some(("m", model))).unwrap();
        let inputs =
            crate::reasoning::input_metadata(&upstream["effective"], model, "chat-completions");
        assert_eq!(
            adapters::model_metadata("omp", "chat-completions", &inputs).unwrap()["thinking"],
            json!({"mode":"effort","efforts":["high","max","xhigh"]})
        );
        assert_eq!(
            adapters::projection_view("omp", "chat-completions", &inputs).unwrap()["fields"]["thinking"],
            json!({"mode":"effort","efforts":["high","max","xhigh"]})
        );
        assert_eq!(
            crate::reasoning::effort_values(&upstream["effective"]).unwrap(),
            ["high", "max"]
        );
    }

    #[test]
    fn alias_projection_intersects_mapped_and_fixed_efforts_of_every_configured_candidate() {
        let mut first = config::Model {
            overrides: serde_json::from_value(json!({
                "reasoning_options":[{"type":"effort","values":["high","max"]}]
            }))
            .unwrap(),
            ..Default::default()
        };
        first.effort_map.insert(
            "chat-completions".into(),
            BTreeMap::from([("xhigh".into(), "max".into())]),
        );
        let mut second = config::Model {
            enabled: false,
            priority: -100,
            ..Default::default()
        };
        second.overrides = serde_json::from_value(json!({
            "reasoning_options":[{"type":"effort","values":["xhigh","max"]}]
        }))
        .unwrap();
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            config::Provider {
                models: BTreeMap::from([("first".into(), first), ("second".into(), second)]),
                ..Default::default()
            },
        );
        let mut alias = config::ModelAlias {
            id: "shared".into(),
            protocol: "chat-completions".into(),
            candidates: vec!["p/first".into(), "p/second".into()],
            candidate_reasoning_effort: BTreeMap::new(),
            enabled: true,
            continuity: Some("stateless".into()),
            fallback: None,
        };
        let effective = crate::reasoning::alias_metadata(&config, None, &alias).unwrap();
        assert_eq!(
            adapters::alias_model_metadata("dsh", &alias.protocol, &effective).unwrap(),
            json!({"reasoningEfforts":{"xhigh":"xhigh","max":"max"}})
        );
        assert_eq!(
            adapters::projection_view("dsh", &alias.protocol, &effective).unwrap()["fields"],
            json!({"reasoningEfforts":{"xhigh":"xhigh","max":"max"}})
        );
        alias
            .candidate_reasoning_effort
            .insert("p/second".into(), "xhigh".into());
        let effective = crate::reasoning::alias_metadata(&config, None, &alias).unwrap();
        assert_eq!(
            adapters::alias_model_metadata("dsh", &alias.protocol, &effective).unwrap(),
            json!({"reasoningEfforts":{"xhigh":"xhigh"}})
        );
        alias
            .candidate_reasoning_effort
            .insert("p/first".into(), "high".into());
        let effective = crate::reasoning::alias_metadata(&config, None, &alias).unwrap();
        assert_eq!(
            adapters::alias_model_metadata("dsh", &alias.protocol, &effective).unwrap(),
            json!({"reasoningEfforts":false})
        );
    }
}
