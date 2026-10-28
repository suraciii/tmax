use crate::{Error, Request, Result, config::Config, storage::Store};
use chrono::Utc;
use serde_json::{Value, json};

use super::{
    acquisition::acquire,
    codex_resets::capability,
    exact::timestamp,
    inspection::InputInspection,
    lifecycle::ensure_saved_view,
    lineage::{
        current_saved_binding, guard_reason, key, project_view, selected_binding, signature, token,
    },
    model::{Channel, InputHead, InputValue, Runtime, revision},
    snapshot::use_projection,
    store::{read_runtime, write_runtime},
    value::{normalize, prepare_observation, publish},
};

fn argument<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request
        .arguments
        .get(name)
        .and_then(|v| v.first())
        .map(String::as_str)
}
fn required<'a>(request: &'a Request, name: &str) -> Result<&'a str> {
    argument(request, name).ok_or_else(|| {
        Error::new("missing_argument", "Required input operand is missing").field(name)
    })
}

pub async fn handle(store: &Store, saved: &Config, request: &Request) -> Result<Value> {
    handle_impl(store, Some(saved), request).await
}
pub async fn handle_unavailable_saved(store: &Store, request: &Request) -> Result<Value> {
    handle_impl(store, None, request).await
}
async fn handle_impl(store: &Store, saved: Option<&Config>, request: &Request) -> Result<Value> {
    let action = request.command.get(2).map(String::as_str).unwrap_or("");
    let provider = request
        .operands
        .first()
        .ok_or_else(|| Error::new("missing_argument", "Provider is required"))?;
    let read_only = matches!(action, "list" | "inspect" | "show");
    let (loaded_info, runtime_error) = match crate::service::loaded(store).await {
        Ok(loaded) => (loaded, None),
        Err(error) if read_only => (None, Some(error.code)),
        Err(error) => return Err(error),
    };
    let running = loaded_info.is_some();
    let loaded = argument(request, "state") == Some("loaded");
    if read_only && loaded && loaded_info.is_none() {
        return Ok(
            json!({"provider":provider,"state":"loaded","snapshot_state":if runtime_error.is_some(){"unavailable"}else{"absent"},"configuration_revision":null,"reason_code":runtime_error.as_deref().unwrap_or("no_loaded_snapshot")}),
        );
    }
    let runtime_view = json!({"snapshot_state":if runtime_error.is_some(){"unavailable"}else if running{"available"}else{"absent"},"configuration_revision":loaded_info.as_ref().map(|file|&file.revision),"reason_code":runtime_error});
    let saved_unavailable = saved.is_none();
    if saved_unavailable && (!loaded || !["list", "inspect", "show"].contains(&action)) {
        return Err(Error::new(
            "invalid_configuration",
            "This operation requires valid saved configuration",
        ));
    }
    let saved = match saved {
        Some(config) => config,
        None => {
            &loaded_info
                .as_ref()
                .ok_or_else(|| {
                    Error::new("no_loaded_snapshot", "No daemon configuration is loaded")
                })?
                .config
        }
    };
    let config = if loaded {
        &loaded_info
            .as_ref()
            .ok_or_else(|| Error::new("no_loaded_snapshot", "No daemon configuration is loaded"))?
            .config
    } else {
        saved
    };
    let p = config
        .providers
        .get(provider)
        .ok_or_else(|| Error::new("unresolved_reference", "Unknown provider"))?;
    let at = Utc::now();
    if action == "list" || action == "inspect" || action == "show" {
        let (mut runtime, read_error) = match read_runtime(store) {
            Ok(state) => (state, None),
            Err(error) => (
                Runtime::default(),
                Some(if error.code == "invalid_record" {
                    "invalid_record"
                } else {
                    "storage_unavailable"
                }),
            ),
        };
        if !loaded && signature(&runtime.saved.config) != signature(config) {
            runtime.saved = project_view(&runtime.saved, config);
        }
        if loaded
            && runtime
                .loaded
                .as_ref()
                .is_none_or(|v| signature(&v.config) != signature(config))
        {
            runtime.loaded = Some(project_view(
                runtime.loaded.as_ref().unwrap_or(&runtime.saved),
                config,
            ));
        }
        let inspection = InputInspection {
            store,
            runtime: &runtime,
            config,
            at,
            loaded,
            running,
        };
        let render = |name: &str| -> Result<Value> {
            let mut detail = inspection.details(provider, name)?;
            if let Some(reason) = read_error {
                detail["availability"] = json!({"state":"unavailable","reason_code":reason});
                detail["current_revision"] = Value::Null;
                detail["current_data"] = Value::Null;
                detail["last_attempt"] = Value::Null;
                detail["last_attempt_state"] = json!("unavailable");
            }
            if saved_unavailable {
                detail["saved_use"] =
                    json!({"state":"unavailable","reason_code":"configuration_unavailable"});
            }
            if let Some(reason) = runtime_error.as_deref() {
                detail["current_runtime_use"] = json!({"state":"unavailable","reason_code":reason});
                detail["acquisition_state"] = json!("unavailable");
                detail["acquisition"] = json!({"state":"unavailable","reason_code":reason,"policy_configuration_revision":null,"active_refresh_interval_seconds":null,"next_due_at":null});
            }
            Ok(detail)
        };
        if action == "list" {
            let mut summaries = Vec::new();
            for name in p.inputs.keys() {
                let detail = render(name)?;
                let mut summary = serde_json::Map::new();
                for field in [
                    "input",
                    "kind",
                    "source",
                    "current_revision",
                    "availability",
                    "acquisition_state",
                    "last_attempt_outcome",
                ] {
                    summary.insert(field.into(), detail[field].clone());
                }
                summaries.push(Value::Object(summary));
            }
            return Ok(
                json!({"provider":provider,"state":if loaded{"loaded"}else{"saved"},"snapshot_state":"known","at":timestamp(at),"inputs":summaries,"runtime":runtime_view}),
            );
        }
        let name = request
            .operands
            .get(1)
            .ok_or_else(|| Error::new("missing_argument", "Input is required"))?;
        return Ok(
            json!({"provider":provider,"state":if loaded{"loaded"}else{"saved"},"snapshot_state":"known","at":timestamp(at),"input":render(name)?,"runtime":runtime_view}),
        );
    }
    let name = request
        .operands
        .get(1)
        .ok_or_else(|| Error::new("missing_argument", "Input is required"))?;
    let declaration = p
        .inputs
        .get(name)
        .ok_or_else(|| Error::new("unresolved_reference", "Unknown input"))?;
    if action == "refresh" && declaration.source == "manual" {
        return Err(Error::new(
            "manual_source",
            "Manual inputs use record, not refresh",
        ));
    }
    if action == "record" && declaration.source != "manual" {
        return Err(Error::new(
            "nonmanual_source",
            "Only manual inputs can be recorded",
        ));
    }
    if !["refresh", "record", "clear"].contains(&action) {
        return Err(Error::new(
            "unknown_operation",
            "Unsupported input operation",
        ));
    }
    if loaded {
        return Err(Error::new(
            "invalid_arguments",
            "Input mutations always select saved declarations",
        ));
    }
    let mut candidate = if action == "record" {
        Some(InputValue {
            observed_at: required(request, "observed-at")?.into(),
            observed_at_basis: "manual".into(),
            received_at: timestamp(at),
            valid_until: required(request, "valid-until")?.into(),
            reset_at: required(request, "reset-at")?.into(),
            remaining_percent: argument(request, "remaining-percent").map(str::to_owned),
            remaining: argument(request, "remaining").map(str::to_owned),
            total: argument(request, "total").map(str::to_owned),
            unit: argument(request, "unit").map(str::to_owned),
            reference: argument(request, "reference").map(str::to_owned),
        })
    } else {
        None
    };
    if let Some(value) = &mut candidate {
        normalize(value, declaration)?;
    }
    let mut preview = read_runtime(store)?;
    let prior = preview.saved.clone();
    preview.saved = project_view(&prior, saved);
    let b = selected_binding(&preview, saved, provider, name, false)?;
    let channel = preview.channels.get(&key(&b));
    if let Some(expected) = argument(request, "if-input-revision") {
        let unbound_replacement = b.declaration_revision.is_empty()
            && prior
                .config
                .providers
                .get(provider)
                .is_some_and(|p| p.inputs.contains_key(name));
        if unbound_replacement || expected != revision(channel) {
            return Err(Error::new(
                "inputs_changed",
                "Input revision does not match a known current head",
            ));
        }
    }
    if let Some(value) = &mut candidate {
        prepare_observation(
            channel.unwrap_or(&Channel::default()),
            declaration,
            value,
            at,
        )?;
    }
    if request.dry_run {
        let mut proposed = serde_json::to_value(&candidate)
            .map_err(|_| Error::new("invalid_record", "Cannot describe proposed input"))?;
        if let Some(object) = proposed.as_object_mut() {
            object.remove("received_at");
        }
        return Ok(
            json!({"provider":provider,"input_name":name,"input_change":{"outcome":"planned","current_revision_before":revision(channel),"current_revision_after":null,"published":false,"attempt_recorded":false},
            "current_data":channel.and_then(|c|c.head.as_ref()),"proposed_data":proposed,"unavailable_reason":if action=="refresh"&&!capability(p,declaration){Some("unsupported_source")}else{None},
            "current_runtime_use":use_projection(&preview,&b,true,running)}),
        );
    }
    let captured_binding = b;
    let captured_revision = revision(channel).to_owned();
    ensure_saved_view(store, saved)?;
    if action == "refresh" {
        return acquire(store, saved, provider, name, false, "explicit", running).await;
    }
    let _configuration = store.lock("configuration")?;
    let _lock = store.lock("routing")?;
    let mut runtime = read_runtime(store)?;
    let b = selected_binding(&runtime, saved, provider, name, false)?;
    let current = current_saved_binding(store, &runtime, &b)?;
    if let Some(reason) = guard_reason(&b, current.as_ref(), "absent", "absent") {
        return Err(Error::new("inputs_changed", reason));
    }
    if !captured_binding.declaration_revision.is_empty()
        && let Some(reason) = guard_reason(
            &captured_binding,
            Some(&b),
            &captured_revision,
            revision(runtime.channels.get(&key(&b))),
        )
    {
        return Err(Error::new("inputs_changed", reason));
    }
    let channel = runtime.channels.entry(key(&b)).or_default();
    let before = revision(Some(channel)).to_owned();
    if argument(request, "if-input-revision").is_some_and(|expected| expected != before) {
        return Err(Error::new(
            "inputs_changed",
            "Input revision does not match",
        ));
    }
    let changed = if let Some(value) = candidate {
        publish(channel, &b, declaration, value, Utc::now())?
    } else {
        channel.head = Some(InputHead {
            binding: b.clone(),
            revision: token(),
            observation_floor: None,
            value: None,
            unavailable_reason: Some("cleared".into()),
        });
        true
    };
    let after = revision(Some(channel)).to_owned();
    let runtime_use = use_projection(&runtime, &b, true, running);
    write_runtime(store, &runtime)?;
    Ok(
        json!({"provider":provider,"input_name":name,"input_change":{"outcome":if !changed{"unchanged"}else if action=="clear"{"cleared"}else{"published"},
        "current_revision_before":before,"current_revision_after":after,"published":changed,"attempt_recorded":false},"current_runtime_use":runtime_use}),
    )
}

#[cfg(test)]
mod tests {
    use super::super::{
        store::STATE,
        test_support::{intent, save_intent},
    };
    use super::*;
    use chrono::Duration;
    use std::collections::BTreeMap;
    #[tokio::test]
    async fn dry_run_validates_manual_data_without_reserving_runtime_state() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_path_buf());
        let config = intent();
        save_intent(&store, &config);
        let now = Utc::now();
        let mut arguments = BTreeMap::new();
        for (name, value) in [
            ("observed-at", timestamp(now - Duration::seconds(10))),
            ("valid-until", timestamp(now + Duration::minutes(10))),
            ("reset-at", timestamp(now + Duration::hours(1))),
            ("remaining", "1".into()),
            ("total", "3".into()),
            ("unit", "requests".into()),
        ] {
            arguments.insert(name.into(), vec![value]);
        }
        let request = Request {
            command: vec!["provider".into(), "input".into(), "record".into()],
            operands: vec!["p".into(), "i".into()],
            arguments,
            dry_run: true,
            if_revision: None,
        };
        let result = handle(&store, &config, &request).await.unwrap();
        assert_eq!(result["input_change"]["outcome"], "planned");
        assert!(result["proposed_data"].get("received_at").is_none());
        assert!(!store.root.join(STATE).exists());
        assert!(!store.root.join("locks").exists());
    }
}
