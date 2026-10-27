use super::arguments::operand;
use crate::{
    Error, Request, Result,
    config::{self, Auth, Config},
    storage::Store,
};
use serde_json::{Value, json};

pub(super) async fn secret_inventory(store: &Store) -> Result<Value> {
    let mut inventory = crate::secrets::inventory(store)?;
    let desired = config::load(store);
    let loaded = crate::service::loaded(store).await;
    for entry in inventory["entries"].as_array_mut().into_iter().flatten() {
        let reference = entry["reference"].as_str().unwrap_or("");
        let d = match &desired {
            Ok(f) => reference_view(&f.config, reference),
            Err(_) => {
                json!({"state":"unknown","resources":[],"reason_code":"configuration_unavailable"})
            }
        };
        let l = match &loaded {
            Ok(Some(f)) => reference_view(&f.config, reference),
            Ok(None) => json!({"state":"unreferenced","resources":[],"reason_code":null}),
            Err(_) => {
                json!({"state":"unknown","resources":[],"reason_code":"resident_identity_unavailable"})
            }
        };
        let native = crate::agent_integration::secret_references(store, reference)?;
        let safe = d["state"] == "unreferenced"
            && l["state"] == "unreferenced"
            && native["state"] == "unreferenced"
            && entry["metadata_state"] == "known";
        entry["references"] = json!({"desired":d,"loaded":l,"native":native});
        entry["cleanup"] = json!({"state":if safe{"eligible"}else{"blocked"},"blockers":if safe{json!([])}else{json!([{"code":"reference_safety_unestablished","message":"Resolve known references and unknown ownership before cleanup","affected":[]}])}});
    }
    Ok(inventory)
}
fn reference_view(c: &Config, reference: &str) -> Value {
    let mut refs = Vec::new();
    for (id, p) in &c.providers {
        if matches!(&p.auth,Some(Auth::ApiKey {key_ref})if key_ref==reference) {
            refs.push(json!({"kind":"provider","id":id,"scope":null}));
        }
    }
    if c.gateway.as_ref().is_some_and(|p| p.key_ref == reference) {
        refs.push(json!({"kind":"gateway","id":null,"scope":null}));
    }
    json!({"state":if refs.is_empty(){"unreferenced"}else{"referenced"},"resources":refs,"reason_code":null})
}
pub(super) async fn cleanup(store: &Store, r: &Request) -> Result<Value> {
    let reference = operand(r, 0)?;
    let inventory = secret_inventory(store).await?;
    let entry = inventory["entries"]
        .as_array()
        .and_then(|a| a.iter().find(|e| e["reference"] == reference))
        .ok_or_else(|| {
            Error::new(
                "unknown_resource",
                "Secret reference is not in retained inventory",
            )
        })?;
    if entry["cleanup"]["state"] != "eligible" {
        return Err(Error::new(
            "recovery_required",
            "Secret reference safety is unknown or still referenced; inspect retained ownership and references",
        ));
    }
    if !r.dry_run {
        let _guard = store.lock("configuration")?;
        let current = secret_inventory(store).await?;
        if !current["entries"].as_array().is_some_and(|a| {
            a.iter()
                .any(|e| e["reference"] == reference && e["cleanup"]["state"] == "eligible")
        }) {
            return Err(Error::new(
                "inputs_changed",
                "Secret references changed during cleanup",
            ));
        }
        crate::secrets::remove(store, reference)?;
    }
    Ok(json!({"reference":reference,"deleted":!r.dry_run,"cleanup":entry["cleanup"]}))
}
pub(super) async fn key_show(store: &Store, c: &Config, r: &Request) -> Result<Value> {
    let gateway = c
        .gateway
        .as_ref()
        .ok_or_else(|| Error::new("invalid_arguments", "Gateway is not configured"))?;
    let mut output = crate::secrets::qualify_output(r)?;
    if r.dry_run {
        return Ok(
            json!({"reference":gateway.key_ref,"loaded_match":null,"delivery":{"channel":output.channel(),"descriptor":output.descriptor(),"delivered":false}}),
        );
    }
    let key = crate::secrets::get(store, &gateway.key_ref)?;
    let delivery = output.deliver(&key);
    let loaded = crate::service::loaded(store).await.ok().flatten();
    Ok(
        json!({"_code":if delivery.is_err(){json!("secret_delivery_failed")}else{Value::Null},"reference":gateway.key_ref,"loaded_match":loaded.as_ref().map(|f|f.config.gateway.as_ref().is_some_and(|p|p.key_ref==gateway.key_ref)),"delivery":{"channel":output.channel(),"descriptor":output.descriptor(),"delivered":if delivery.is_ok(){json!(true)}else{Value::Null}},"boundaries":[{"name":"secret_delivery","state":if delivery.is_ok(){"completed"}else{"partial"},"affected":[],"message":"Protected delivery may be partial on failure; do not replay automatically"}]}),
    )
}
