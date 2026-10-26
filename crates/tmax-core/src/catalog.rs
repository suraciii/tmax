use crate::{
    Error, Request, Result,
    config::{Config, Model, ModelOverrides, Provider, ProviderOverrides},
    storage::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub revision: String,
    pub source: String,
    pub retrieved_at: String,
    pub providers: BTreeMap<String, Value>,
}
pub fn read(store: &Store, snapshot: &str) -> Result<Option<Snapshot>> {
    if !matches!(snapshot, "accepted" | "latest") {
        return Err(Error::new(
            "invalid_arguments",
            "Catalog snapshot must be accepted or latest",
        ));
    }
    let path = format!("catalog/{snapshot}.json");
    if !store.root.join(&path).exists() {
        return Ok(None);
    }
    Ok(Some(store.read_json(&path)?))
}
pub fn effective(store: &Store, p: &Provider, model: Option<(&str, &Model)>) -> Result<Value> {
    let snapshot = read(store, "accepted")?;
    effective_from_snapshot(snapshot.as_ref(), p, model)
}
pub fn effective_from_snapshot(
    snapshot: Option<&Snapshot>,
    p: &Provider,
    model: Option<(&str, &Model)>,
) -> Result<Value> {
    let baseline = p
        .metadata
        .as_ref()
        .and_then(|id| snapshot.and_then(|s| s.providers.get(id)));
    let base = if let Some((id, _)) = model {
        baseline
            .and_then(|p| p.get("models"))
            .and_then(|m| m.get(id))
            .cloned()
            .unwrap_or_else(|| json!({}))
    } else {
        json!({"name":baseline.and_then(|p|p.get("name")),"doc":baseline.and_then(|p|p.get("doc"))})
    };
    let overrides = if let Some((_, model)) = model {
        serde_json::to_value(&model.overrides)
    } else {
        serde_json::to_value(&p.overrides)
    }
    .map_err(|_| Error::new("operation_failed", "Cannot project metadata"))?;
    let overrides = without_nulls(overrides);
    let mut result = base.clone();
    merge(&mut result, &overrides);
    Ok(
        json!({"catalog":base,"overrides":overrides,"effective":result,"metadata_revision":snapshot.map(|s|&s.revision),"catalog_reference":p.metadata,"source":snapshot.map(|s|&s.source),"retrieved_at":snapshot.map(|s|&s.retrieved_at),"catalog_state":if snapshot.is_none(){"unavailable"}else if baseline.is_none()&&p.metadata.is_some(){"missing_entry"}else{"accepted"}}),
    )
}
fn without_nulls(v: Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k, without_nulls(v)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(without_nulls).collect()),
        v => v,
    }
}
pub fn merge(base: &mut Value, overrides: &Value) {
    if let (Some(b), Some(o)) = (base.as_object_mut(), overrides.as_object()) {
        for (k, v) in o {
            if let Some(old) = b.get_mut(k) {
                merge(old, v)
            } else {
                b.insert(k.clone(), v.clone());
            }
        }
    } else {
        *base = overrides.clone();
    }
}

fn validate_snapshot(snapshot: &Snapshot) -> Result<()> {
    if snapshot.providers.is_empty()
        || snapshot.providers.iter().any(|(id, provider)| {
            !crate::config::valid_id(id, false)
                || !provider.is_object()
                || !provider.get("models").is_some_and(Value::is_object)
                || provider
                    .get("id")
                    .is_some_and(|value| value.as_str() != Some(id))
        })
    {
        return Err(Error::new(
            "observation_failed",
            "Catalog provider identities or model maps are invalid",
        ));
    }
    Ok(())
}

// The typed local schema defines consumed metadata; other directory fields,
// including endpoint and package guidance, are not adopted settings.
fn model_metadata_shape() -> Result<Value> {
    serde_json::to_value(ModelOverrides {
        limit: Some(crate::config::Limits::default()),
        modalities: Some(crate::config::Modalities::default()),
        cost: Some(crate::config::Cost::default()),
        ..ModelOverrides::default()
    })
    .map_err(|_| Error::new("operation_failed", "Cannot project metadata schema"))
}

fn supported_metadata(value: &Value, shape: &Value) -> Value {
    match (value.as_object(), shape.as_object()) {
        (Some(values), Some(fields)) => Value::Object(
            values
                .iter()
                .filter_map(|(key, value)| {
                    fields
                        .get(key)
                        .map(|field| (key.clone(), supported_metadata(value, field)))
                })
                .collect(),
        ),
        _ => value.clone(),
    }
}

fn metadata_changes(before: &Value, after: &Value, kind: &str, id: &str, path: &str) -> Vec<Value> {
    let mut changes =
        crate::operations::diff_values(Some(before), Some(after), path, "catalog_apply");
    for change in &mut changes {
        change["resource"] = json!({"kind":kind,"id":id,"scope":null});
    }
    changes
}

fn adoption_impact(
    config: &Config,
    accepted: Option<&Snapshot>,
    proposed: &Snapshot,
) -> Result<Value> {
    validate_snapshot(proposed)?;
    let shape = model_metadata_shape()?;
    let mut candidate = config.clone();
    let mut changes = Vec::new();
    let mut providers = BTreeSet::new();
    let mut models = BTreeMap::new();
    for (provider_id, provider) in &config.providers {
        let before = effective_from_snapshot(accepted, provider, None)?;
        let after = effective_from_snapshot(Some(proposed), provider, None)?;
        candidate.providers.get_mut(provider_id).unwrap().overrides =
            serde_json::from_value::<ProviderOverrides>(after["effective"].clone()).map_err(
                |_| {
                    Error::new(
                        "invalid_configuration",
                        "Proposed catalog provider metadata has invalid field types",
                    )
                    .field(format!("providers.{provider_id}.metadata"))
                },
            )?;
        let provider_changes = metadata_changes(
            &before["effective"],
            &after["effective"],
            "provider",
            provider_id,
            &format!("providers.{provider_id}.metadata"),
        );
        if !provider_changes.is_empty() {
            providers.insert(provider_id.clone());
            changes.extend(provider_changes);
        }
        for (model_id, model) in &provider.models {
            if provider
                .metadata
                .as_ref()
                .and_then(|id| proposed.providers.get(id))
                .and_then(|entry| entry.get("models"))
                .and_then(|entries| entries.get(model_id))
                .is_some_and(|entry| !entry.is_object())
            {
                return Err(Error::new(
                    "invalid_configuration",
                    "Proposed catalog model metadata must be an object",
                )
                .field(format!(
                    "providers.{provider_id}.models.{model_id}.metadata"
                )));
            }
            let before = effective_from_snapshot(accepted, provider, Some((model_id, model)))?;
            let after = effective_from_snapshot(Some(proposed), provider, Some((model_id, model)))?;
            let before = without_nulls(supported_metadata(&before["effective"], &shape));
            let after = without_nulls(supported_metadata(&after["effective"], &shape));
            candidate
                .providers
                .get_mut(provider_id)
                .unwrap()
                .models
                .get_mut(model_id)
                .unwrap()
                .overrides =
                serde_json::from_value::<ModelOverrides>(after.clone()).map_err(|_| {
                    Error::new(
                        "invalid_configuration",
                        "Proposed catalog model metadata has invalid field types",
                    )
                    .field(format!(
                        "providers.{provider_id}.models.{model_id}.metadata"
                    ))
                })?;
            let qualified = format!("{provider_id}/{model_id}");
            let model_changes = metadata_changes(
                &before,
                &after,
                "model",
                &qualified,
                &format!("providers.{provider_id}.models.{model_id}.metadata"),
            );
            if !model_changes.is_empty() {
                providers.insert(provider_id.clone());
                changes.extend(model_changes);
                models.insert(qualified, (before, after));
            }
        }
    }
    // Reuse explicit-override constraints without persisting this projection.
    crate::config::validate(&candidate)?;
    crate::reasoning::validate(config, Some(proposed))?;
    let mut aliases = BTreeMap::new();
    for alias in &config.model_aliases {
        let affected: Vec<_> = models
            .keys()
            .filter(|name| alias.candidates.contains(name))
            .cloned()
            .collect();
        if !affected.is_empty() {
            aliases.insert((alias.id.clone(), alias.protocol.clone()), affected);
        }
    }
    let mut routes = Vec::new();
    for name in models.keys() {
        let (provider, _) = crate::config::model(config, name)?;
        let (_, model_id) = crate::config::qualified(name)?;
        for protocol in crate::integrations::protocols(provider, model_id)? {
            routes.push(json!({"kind":"concrete","id":name,"protocol":protocol,"effect":"metadata_changed"}));
        }
    }
    for ((id, protocol), affected) in &aliases {
        routes.push(json!({"kind":"alias","id":id,"protocol":protocol,"models":affected,"effect":"candidate_metadata_changed"}));
    }
    let mut targets = Vec::new();
    for (target_id, target) in &config.agents {
        for (entry_id, entry) in &target.providers {
            let mut selections = Vec::new();
            let mut native_changes = Vec::new();
            let mut blockers = Vec::new();
            for selected in &entry.models {
                let qualified = entry
                    .provider
                    .as_ref()
                    .map(|provider| format!("{provider}/{}", selected.id));
                let name = qualified.as_deref().unwrap_or(&selected.id);
                if let Some((before, after)) = models.get(name) {
                    let old_native =
                        crate::native::model_metadata(&target.agent, &selected.protocol, before);
                    match crate::native::model_metadata(&target.agent, &selected.protocol, after) {
                        Ok(new_native) => {
                            let mut projected = crate::operations::diff_values(
                                old_native.as_ref().ok(),
                                Some(&new_native),
                                &format!("models.{}", selected.id),
                                "native_write",
                            );
                            for change in &mut projected {
                                change["resource"] =
                                    json!({"kind":"agent_target","id":target_id,"scope":entry_id});
                            }
                            if entry.gateway == Some(true) || !projected.is_empty() {
                                selections
                                    .push(json!({"id":selected.id,"protocol":selected.protocol}));
                                native_changes.extend(projected);
                            }
                        }
                        Err(error) => {
                            selections.push(json!({"id":selected.id,"protocol":selected.protocol}));
                            blockers.push(json!({"model":selected.id,"protocol":selected.protocol,"code":error.code,"field":error.field}));
                        }
                    }
                } else if entry.gateway == Some(true)
                    && aliases.contains_key(&(selected.id.clone(), selected.protocol.clone()))
                {
                    selections.push(json!({"id":selected.id,"protocol":selected.protocol}));
                }
            }
            if !selections.is_empty() {
                targets.push(json!({"target":target_id,"agent":target.agent,"agent_provider":entry_id,"models":selections,"native_application":if !blockers.is_empty(){"blocked"}else if !native_changes.is_empty(){"reapply_required"}else{"unchanged"},"native_changes":native_changes,"blockers":blockers,"route_metadata":if entry.gateway==Some(true){"revalidation_required"}else{"not_applicable"}}));
            }
        }
    }
    Ok(json!({
        "catalog_impact":{"accepted_revision_before":accepted.map(|snapshot| &snapshot.revision),"candidate_revision":proposed.revision,"accepted_revision_after":accepted.map(|snapshot| &snapshot.revision),"baseline":"accepted","endpoint_settings":"unchanged"},
        "metadata_changes":changes,
        "affected_providers":providers,
        "affected_models":models.keys().collect::<Vec<_>>(),
        "affected_routes":routes,
        "affected_targets":targets
    }))
}
fn argument<'a>(r: &'a Request, key: &str) -> Option<&'a str> {
    r.arguments
        .get(key)
        .and_then(|v| v.first())
        .map(String::as_str)
}
pub async fn handle(store: &Store, config: &Config, r: &Request) -> Result<Value> {
    let action = r.command.get(1).map(String::as_str).unwrap_or("");
    let snapshot = argument(r, "snapshot").unwrap_or("accepted");
    match action {
        "refresh" => {
            if r.dry_run {
                return Ok(
                    json!({"source":"https://models.dev/api.json","download":"not_executed","accepted":"unchanged"}),
                );
            }
            let client = crate::integrations::client(config)?;
            let response = tokio::time::timeout(
                crate::integrations::OBSERVATION_DEADLINE,
                client.get("https://models.dev/api.json").send(),
            )
            .await
            .map_err(|_| Error::new("observation_failed", "Catalog connection deadline expired"))?
            .map_err(|_| {
                Error::new(
                    "observation_failed",
                    "Catalog download transport failed; previous snapshot retained",
                )
            })?;
            if !response.status().is_success() {
                return Err(Error::new(
                    "observation_failed",
                    "Catalog returned a non-success response; previous snapshot retained",
                ));
            }
            let mut response = response;
            let bytes = tokio::time::timeout(crate::integrations::OBSERVATION_DEADLINE, async {
                let mut bytes = Vec::new();
                while let Some(chunk) = response
                    .chunk()
                    .await
                    .map_err(|_| Error::new("observation_failed", "Catalog download interrupted"))?
                {
                    if bytes.len() + chunk.len() > 64 * 1024 * 1024 {
                        return Err(Error::new(
                            "observation_failed",
                            "Catalog exceeds the supported download bound",
                        ));
                    }
                    bytes.extend_from_slice(&chunk);
                }
                Ok(bytes)
            })
            .await
            .map_err(|_| Error::new("observation_failed", "Catalog response deadline expired"))??;
            let providers: BTreeMap<String, Value> =
                serde_json::from_slice(&bytes).map_err(|_| {
                    Error::new(
                        "observation_failed",
                        "Catalog has an invalid JSON representation",
                    )
                })?;
            let revision = crate::config::revision(&bytes);
            let new = Snapshot {
                revision: revision.clone(),
                source: "https://models.dev/api.json".into(),
                retrieved_at: chrono::Utc::now().to_rfc3339(),
                providers,
            };
            validate_snapshot(&new)?;
            let _guard = store.lock("catalog")?;
            store.write_json("catalog/latest.json", &new)?;
            Ok(
                json!({"revision":revision,"snapshot":"latest","accepted":"unchanged","retrieved_at":new.retrieved_at}),
            )
        }
        "apply" => {
            let file = crate::config::load(store)?;
            if r.if_revision
                .as_ref()
                .is_some_and(|expected| expected != &file.revision)
            {
                return Err(Error::new(
                    "inputs_changed",
                    "Configuration revision differs from the requested guard",
                ));
            }
            let wanted = argument(r, "revision")
                .ok_or_else(|| Error::new("invalid_arguments", "Catalog revision is required"))?;
            let guard = if r.dry_run {
                None
            } else {
                Some(store.lock("catalog")?)
            };
            let latest = read(store, "latest")?.ok_or_else(|| {
                Error::new(
                    "catalog_unavailable",
                    "No validated downloaded catalog exists; refresh explicitly",
                )
            })?;
            if latest.revision != wanted {
                return Err(Error::new(
                    "inputs_changed",
                    "Requested downloaded catalog revision is not retained",
                ));
            }
            let accepted = read(store, "accepted")?;
            let Value::Object(mut metadata) =
                adoption_impact(&file.config, accepted.as_ref(), &latest)?
            else {
                unreachable!()
            };
            let mut impact =
                crate::operations::impact(store, &file.config, &file.revision, None).await?;
            let Value::Array(metadata_changes) = metadata.remove("metadata_changes").unwrap()
            else {
                unreachable!()
            };
            impact["changes"]
                .as_array_mut()
                .unwrap()
                .extend(metadata_changes);
            impact.as_object_mut().unwrap().extend(metadata);
            impact["revision"] = json!(wanted);
            impact["catalog_apply"] = json!("not_executed");
            if r.dry_run {
                return Ok(impact);
            }
            let publication = store.lock("routing")?;
            crate::config::guard(store, &file.revision)?;
            store.write_json("catalog/accepted.json", &latest)?;
            drop(publication);
            drop(guard);
            impact["catalog_apply"] = json!("completed");
            impact["catalog_impact"]["accepted_revision_after"] = json!(wanted);
            impact["save"] = json!("not_requested");
            crate::operations::activate_saved(store, &file.config, &file.revision, &mut impact)
                .await;
            Ok(impact)
        }
        "providers" | "models" | "inspect" => {
            let Some(s) = read(store, snapshot)? else {
                return Ok(
                    json!({"snapshot":snapshot,"snapshot_state":"unavailable","source":"https://models.dev/api.json","reason_code":"missing_snapshot","providers":[]}),
                );
            };
            let common = json!({"snapshot":snapshot,"snapshot_state":"available","revision":s.revision,"source":s.source,"retrieved_at":s.retrieved_at});
            if action == "providers" {
                let mut out = common;
                out["providers"]=json!(s.providers.iter().map(|(id,p)|json!({"id":id,"name":p.get("name"),"models":p.get("models").and_then(Value::as_object).map(|m|m.len()),"integration_choices":choices(id)})).collect::<Vec<_>>());
                return Ok(out);
            }
            let id = r.operands.first().ok_or_else(|| {
                Error::new("invalid_arguments", "Catalog provider operand required")
            })?;
            let p = s.providers.get(id).ok_or_else(|| {
                Error::new(
                    "invalid_arguments",
                    "Catalog provider is absent from selected snapshot",
                )
            })?;
            let mut out = common;
            out["id"] = json!(id);
            if action == "models" {
                out["models"]=json!(p["models"].as_object().into_iter().flat_map(|m|m.iter()).map(|(id,m)|json!({"id":id,"name":m.get("name"),"tool_call":m.get("tool_call"),"reasoning":m.get("reasoning"),"limit":m.get("limit")})).collect::<Vec<_>>());
            } else {
                out["metadata"] = if let Some(model) = argument(r, "model") {
                    p["models"].get(model).cloned().ok_or_else(|| {
                        Error::new(
                            "invalid_arguments",
                            "Catalog model is absent from selected snapshot",
                        )
                    })?
                } else {
                    p.clone()
                };
                let mut suggestion = id.clone();
                let mut n = 2;
                while config.providers.contains_key(&suggestion) {
                    suggestion = format!("{id}-{n}");
                    n += 1;
                }
                out["suggested_provider_id"] = json!(suggestion);
                out["integration_choices"] = choices(id);
            }
            Ok(out)
        }
        _ => Err(Error::new("invalid_arguments", "Unknown catalog operation")),
    }
}
fn choices(id: &str) -> Value {
    let p = Provider {
        metadata: Some(id.into()),
        ..Provider::default()
    };
    match crate::integrations::integration_id(&p) {
        Ok(id) => {
            json!([{"integration":id,"endpoint_setup":if id=="kimi-coding-plan"{"region_or_known_base"}else{"singleton_default"}}])
        }
        Err(_) => json!([{"integration":"custom","endpoint_setup":"explicit_bases"}]),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merge_preserves_false_zero_and_array_replacement() {
        let mut base = json!({"limit":{"context":100,"output":50},"temperature":true,"modalities":{"input":["text","image"],"output":["text"]}});
        merge(
            &mut base,
            &json!({"limit":{"output":0},"temperature":false,"modalities":{"input":[]}}),
        );
        assert_eq!(base["limit"]["context"], 100);
        assert_eq!(base["limit"]["output"], 0);
        assert_eq!(base["temperature"], false);
        assert_eq!(base["modalities"]["input"], json!([]));
        assert_eq!(base["modalities"]["output"], json!(["text"]));
    }

    fn fixture() -> (Config, Snapshot, Snapshot) {
        let mut config = crate::config::parse(
            r#"version = 1
[providers.account]
metadata = 'directory'
integration = 'custom'
[providers.account.endpoints]
chat-completions = 'http://127.0.0.1:9999/v1'
[providers.account.models.a]
priority = 0
[providers.account.models.b]
priority = 100
[gateway]
listen = '127.0.0.1:19876'
key_ref = 'synthetic-gateway-reference'
[[model_aliases]]
id = 'explicit'
protocol = 'chat-completions'
candidates = ['account/a', 'account/b']
[[model_aliases]]
id = 'alternate'
protocol = 'chat-completions'
candidates = ['account/a', 'account/b']
[agents.direct]
agent = 'pi'
scope = '/synthetic/catalog-direct'
[agents.direct.providers.upstream]
provider = 'account'
models = [{id = 'a', protocol = 'chat-completions'}]
[agents.aliases]
agent = 'pi'
scope = '/synthetic/catalog-aliases'
[agents.aliases.providers.local]
gateway = true
models = [{id = 'explicit', protocol = 'chat-completions'}, {id = 'alternate', protocol = 'chat-completions'}]
"#,
        )
        .unwrap();
        let old_model = json!({"limit":{"context":200000,"output":32000},"tool_call":true,"reasoning":true,"modalities":{"input":["text","image"]}});
        let new_model = json!({"limit":{"context":10000,"output":1000},"tool_call":false,"reasoning":false,"modalities":{"input":["text"]}});
        let old = Snapshot {
            revision: "old".into(),
            source: "synthetic-directory".into(),
            retrieved_at: "2026-10-24T00:00:00Z".into(),
            providers: BTreeMap::from([(
                "directory".into(),
                json!({"id":"directory","name":"Directory","api":"https://old.example.invalid","models":{"a":old_model,"b":{"limit":{"context":200000,"output":32000}}}}),
            )]),
        };
        let mut new = old.clone();
        new.revision = "new".into();
        new.providers.get_mut("directory").unwrap()["models"]["a"] = new_model;
        new.providers.get_mut("directory").unwrap()["api"] = json!("https://new.example.invalid");
        new.providers.get_mut("directory").unwrap()["models"]["unregistered"] =
            json!({"tool_call":true});
        let mut masked = config.providers["account"].clone();
        masked.models.get_mut("a").unwrap().overrides =
            serde_json::from_value(old.providers["directory"]["models"]["a"].clone()).unwrap();
        config.providers.insert("masked".into(), masked);
        crate::config::validate(&config).unwrap();
        (config, old, new)
    }

    #[test]
    fn adoption_reports_effective_fields_and_all_alias_dependencies_not_just_the_winner() {
        let (config, old, new) = fixture();
        let impact = adoption_impact(&config, Some(&old), &new).unwrap();
        assert_eq!(impact["affected_providers"], json!(["account"]));
        assert_eq!(impact["affected_models"], json!(["account/a"]));
        let changes = impact["metadata_changes"].as_array().unwrap();
        for (field, before, after) in [
            ("limit.context", json!(200000), json!(10000)),
            ("limit.output", json!(32000), json!(1000)),
            ("tool_call", json!(true), json!(false)),
            ("reasoning", json!(true), json!(false)),
            (
                "modalities.input",
                json!(["text", "image"]),
                json!(["text"]),
            ),
        ] {
            let change = changes
                .iter()
                .find(|change| {
                    change["path"] == format!("providers.account.models.a.metadata.{field}")
                })
                .unwrap();
            assert_eq!(change["before"], before);
            assert_eq!(change["after"], after);
            assert_eq!(change["resource"]["id"], "account/a");
        }
        assert_eq!(changes.len(), 5);
        for alias in ["explicit", "alternate"] {
            let route = impact["affected_routes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|route| route["kind"] == "alias" && route["id"] == alias)
                .unwrap();
            assert_eq!(route["models"], json!(["account/a"]));
        }
        let targets = impact["affected_targets"].as_array().unwrap();
        let direct = targets
            .iter()
            .find(|target| target["target"] == "direct")
            .unwrap();
        assert_eq!(direct["native_application"], "reapply_required");
        let context = direct["native_changes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|change| change["path"] == "models.a.contextWindow")
            .unwrap();
        assert_eq!(context["before"], 200000);
        assert_eq!(context["after"], 10000);
        let aliases = targets
            .iter()
            .find(|target| target["target"] == "aliases")
            .unwrap();
        assert_eq!(aliases["models"].as_array().unwrap().len(), 2);
        assert_eq!(aliases["native_application"], "unchanged");
        assert_eq!(aliases["route_metadata"], "revalidation_required");
    }

    #[test]
    fn adoption_validates_proposed_effective_metadata_but_honors_masking_overrides() {
        let (config, old, new) = fixture();
        for (field, malformed) in [
            ("tool_call", json!("false")),
            ("modalities", json!({"input":["text", false]})),
            ("limit", json!({"context":-1})),
            ("limit", json!({"context":u64::MAX})),
            ("cost", json!({"input":-0.5})),
        ] {
            let mut invalid = new.clone();
            invalid.providers.get_mut("directory").unwrap()["models"]["a"][field] = malformed;
            let error = adoption_impact(&config, Some(&old), &invalid).unwrap_err();
            assert_eq!(error.code, "invalid_configuration");
        }
        let mut masked_only = config.clone();
        masked_only.providers.retain(|id, _| id == "masked");
        masked_only.agents.clear();
        masked_only.model_aliases.clear();
        let mut invalid_baseline = new;
        invalid_baseline.providers.get_mut("directory").unwrap()["models"]["a"]["tool_call"] =
            json!("not a boolean");
        let impact = adoption_impact(&masked_only, Some(&old), &invalid_baseline).unwrap();
        assert!(impact["affected_models"].as_array().unwrap().is_empty());
        assert!(impact["metadata_changes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn endpoint_guidance_and_unregistered_models_do_not_become_adopted_settings() {
        let (config, old, mut new) = fixture();
        new.providers.get_mut("directory").unwrap()["models"]["a"] =
            old.providers["directory"]["models"]["a"].clone();
        let before = serde_json::to_value(&config).unwrap();
        let impact = adoption_impact(&config, Some(&old), &new).unwrap();
        for field in [
            "metadata_changes",
            "affected_providers",
            "affected_models",
            "affected_targets",
        ] {
            assert!(impact[field].as_array().unwrap().is_empty());
        }
        assert_eq!(serde_json::to_value(&config).unwrap(), before);
        assert_eq!(impact["catalog_impact"]["endpoint_settings"], "unchanged");
    }

    #[test]
    fn missing_catalog_models_remove_inherited_defaults_not_local_models() {
        let (config, old, mut new) = fixture();
        new.providers.get_mut("directory").unwrap()["models"] = json!({});
        let impact = adoption_impact(&config, Some(&old), &new).unwrap();
        assert!(
            impact["metadata_changes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|change| {
                    change["resource"]["id"] == "account/a"
                        && change["path"] == "providers.account.models.a.metadata.limit"
                        && change["kind"] == "remove"
                })
        );
        assert!(crate::config::model(&config, "account/a").is_ok());
        assert!(crate::config::model(&config, "account/b").is_ok());
    }

    #[test]
    fn unrepresentable_native_metadata_reports_target_blockers_not_invalid_catalog_facts() {
        let (mut config, old, mut new) = fixture();
        config.agents.get_mut("direct").unwrap().agent = "hermes".into();
        new.providers.get_mut("directory").unwrap()["models"]["a"]["limit"]["context"] = json!(0);
        let impact = adoption_impact(&config, Some(&old), &new).unwrap();
        let target = impact["affected_targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|target| target["target"] == "direct")
            .unwrap();
        assert_eq!(target["native_application"], "blocked");
        assert_eq!(
            target["blockers"][0]["code"],
            "native_metadata_unrepresentable"
        );
    }

    #[tokio::test]
    async fn apply_retains_the_preview_impact_and_publishes_only_on_execution() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().to_path_buf());
        let (mut config, old, new) = fixture();
        let native_scope = temporary.path().join("native");
        std::fs::create_dir(&native_scope).unwrap();
        let native_file = native_scope.join("models.json");
        std::fs::write(&native_file, b"unchanged native configuration").unwrap();
        for target in config.agents.values_mut() {
            target.scope = native_scope.clone();
        }
        let document = toml::to_string(&config).unwrap();
        store.write_raw("tmax.toml", document.as_bytes()).unwrap();
        store.write_json("catalog/accepted.json", &old).unwrap();
        store.write_json("catalog/latest.json", &new).unwrap();
        let revision = crate::config::load(&store).unwrap().revision;
        let mut request = Request {
            command: vec!["catalog".into(), "apply".into()],
            operands: Vec::new(),
            arguments: BTreeMap::from([("revision".into(), vec!["new".into()])]),
            dry_run: true,
            if_revision: Some(revision.clone()),
        };
        let preview = handle(&store, &config, &request).await.unwrap();
        assert_eq!(preview["catalog_apply"], "not_executed");
        assert_eq!(read(&store, "accepted").unwrap().unwrap().revision, "old");
        request.dry_run = false;
        let applied = handle(&store, &config, &request).await.unwrap();
        for field in [
            "affected_providers",
            "affected_models",
            "affected_routes",
            "affected_targets",
        ] {
            assert_eq!(applied[field], preview[field]);
        }
        assert_eq!(preview["catalog_impact"]["accepted_revision_before"], "old");
        assert_eq!(preview["catalog_impact"]["accepted_revision_after"], "old");
        assert_eq!(preview["catalog_impact"]["candidate_revision"], "new");
        assert_eq!(applied["catalog_impact"]["accepted_revision_before"], "old");
        assert_eq!(applied["catalog_impact"]["accepted_revision_after"], "new");
        let metadata_changes = |impact: &Value| {
            impact["changes"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|change| change["boundary"] == "catalog_apply")
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(metadata_changes(&applied), metadata_changes(&preview));
        assert!(!metadata_changes(&applied).is_empty());
        assert_eq!(applied["catalog_apply"], "completed");
        assert_eq!(applied["activation"]["activation"], "not_running");
        assert_eq!(applied["runtime"], "not_running");
        assert_eq!(applied["native_application"], "not_requested");
        assert!(applied.get("session_impact").is_some());
        assert_eq!(read(&store, "accepted").unwrap().unwrap().revision, "new");
        assert_eq!(crate::config::load(&store).unwrap().revision, revision);
        assert_eq!(
            std::fs::read(store.root.join("tmax.toml")).unwrap(),
            document.as_bytes()
        );
        assert_eq!(
            std::fs::read(&native_file).unwrap(),
            b"unchanged native configuration"
        );
    }

    #[tokio::test]
    async fn candidate_effort_catalog_adoption_rejects_removed_values_but_honors_explicit_overrides()
     {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().to_path_buf());
        let (mut config, mut old, mut new) = fixture();
        old.providers.get_mut("directory").unwrap()["models"]["a"]["reasoning_options"] =
            json!([{"type":"effort","values":["high","max"]}]);
        new.providers.get_mut("directory").unwrap()["models"]["a"] =
            old.providers["directory"]["models"]["a"].clone();
        new.providers.get_mut("directory").unwrap()["models"]["a"]["reasoning_options"] =
            json!([{"type":"effort","values":["high"]}]);
        config.model_aliases[0]
            .candidate_reasoning_effort
            .insert("account/a".into(), "max".into());
        let document = toml::to_string(&config).unwrap();
        store.write_raw("tmax.toml", document.as_bytes()).unwrap();
        store.write_json("catalog/accepted.json", &old).unwrap();
        store.write_json("catalog/latest.json", &new).unwrap();
        let accepted_before = std::fs::read(store.root.join("catalog/accepted.json")).unwrap();
        for dry_run in [true, false] {
            let request = Request {
                command: vec!["catalog".into(), "apply".into()],
                operands: Vec::new(),
                arguments: BTreeMap::from([("revision".into(), vec!["new".into()])]),
                dry_run,
                if_revision: None,
            };
            let error = handle(&store, &config, &request).await.unwrap_err();
            assert_eq!(error.code, "reasoning_effort_incompatible");
            assert_eq!(
                error.field.as_deref(),
                Some("model_aliases.explicit.candidate_reasoning_effort.account/a")
            );
            assert_eq!(
                std::fs::read(store.root.join("catalog/accepted.json")).unwrap(),
                accepted_before
            );
            assert_eq!(
                std::fs::read(store.root.join("tmax.toml")).unwrap(),
                document.as_bytes()
            );
        }
        config
            .providers
            .get_mut("account")
            .unwrap()
            .models
            .get_mut("a")
            .unwrap()
            .overrides
            .reasoning_options = Some(vec![crate::config::ReasoningOption {
            r#type: "effort".into(),
            values: Some(vec!["max".into()]),
        }]);
        let impact = adoption_impact(&config, Some(&old), &new).unwrap();
        assert!(
            impact["metadata_changes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|change| {
                    !change["path"].as_str().is_some_and(|path| {
                        path.starts_with("providers.account.models.a.metadata.reasoning_options")
                    })
                })
        );
        assert_eq!(
            config.model_aliases[0].candidate_reasoning_effort["account/a"],
            "max"
        );
    }

    #[tokio::test]
    async fn invalid_apply_preserves_accepted_snapshot_for_preview_and_execution() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().to_path_buf());
        let (config, old, mut new) = fixture();
        new.providers.get_mut("directory").unwrap()["models"]["a"]["reasoning"] = json!("false");
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        store.write_json("catalog/accepted.json", &old).unwrap();
        store.write_json("catalog/latest.json", &new).unwrap();
        for dry_run in [true, false] {
            let request = Request {
                command: vec!["catalog".into(), "apply".into()],
                operands: Vec::new(),
                arguments: BTreeMap::from([("revision".into(), vec!["new".into()])]),
                dry_run,
                if_revision: None,
            };
            let error = handle(&store, &config, &request).await.unwrap_err();
            assert_eq!(error.code, "invalid_configuration");
            assert_eq!(read(&store, "accepted").unwrap().unwrap().revision, "old");
        }
    }
}
