use crate::{
    Error, Request, Result,
    config::{Config, Model, Provider},
    storage::Store,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

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
            if providers.is_empty()
                || providers.iter().any(|(id, p)| {
                    !crate::config::valid_id(id, false)
                        || !p.is_object()
                        || !p.get("models").is_some_and(Value::is_object)
                        || p.get("id").is_some_and(|i| i.as_str() != Some(id))
                })
            {
                return Err(Error::new(
                    "observation_failed",
                    "Catalog provider identities or model maps are invalid",
                ));
            }
            let revision = crate::config::revision(&bytes);
            let new = Snapshot {
                revision: revision.clone(),
                source: "https://models.dev/api.json".into(),
                retrieved_at: chrono::Utc::now().to_rfc3339(),
                providers,
            };
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
            if r.dry_run {
                let mut impact =
                    crate::operations::impact(store, config, &file.revision, None).await?;
                impact["revision"] = json!(wanted);
                impact["catalog_apply"] = json!("not_executed");
                impact["affected_providers"] = json!(config.providers.keys().collect::<Vec<_>>());
                return Ok(impact);
            }
            let guard = store.lock("catalog")?;
            if read(store, "latest")?
                .as_ref()
                .is_none_or(|s| s.revision != wanted)
            {
                return Err(Error::new(
                    "inputs_changed",
                    "Downloaded catalog changed during application",
                ));
            }
            let publication = store.lock("routing")?;
            crate::config::guard(store, &file.revision)?;
            store.write_json("catalog/accepted.json", &latest)?;
            drop(publication);
            drop(guard);
            let activation = crate::service::activate(store, config, &file.revision).await;
            Ok(match activation {
                Ok(activation) => {
                    json!({"revision":wanted,"catalog_apply":"completed","activation":activation,"native_application":"not_requested"})
                }
                Err(error) => {
                    json!({"_code":"saved_not_active","revision":wanted,"catalog_apply":"completed","activation":{"state":"failed","code":error.code},"native_application":"not_requested"})
                }
            })
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
}
