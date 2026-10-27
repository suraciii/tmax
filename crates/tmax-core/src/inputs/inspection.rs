use crate::{Error, Result, config::Config, storage::Store};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::{
    acquisition::acquisition_name,
    codex_resets::capability,
    lineage::{binding, key, selected_binding},
    model::{Runtime, revision},
    scheduler::scheduled_next_due,
    snapshot::use_projection,
    value::{availability, valid_attempt},
};

#[derive(Clone, Copy)]
pub(super) struct InputInspection<'a> {
    pub(super) store: &'a Store,
    pub(super) runtime: &'a Runtime,
    pub(super) config: &'a Config,
    pub(super) at: DateTime<Utc>,
    pub(super) loaded: bool,
    pub(super) running: bool,
}
impl InputInspection<'_> {
    pub(super) fn details(&self, provider: &str, name: &str) -> Result<Value> {
        let Self {
            store,
            runtime,
            config,
            at,
            loaded,
            running,
        } = *self;
        let p = config
            .providers
            .get(provider)
            .ok_or_else(|| Error::new("unresolved_reference", "Unknown provider"))?;
        let d = p
            .inputs
            .get(name)
            .ok_or_else(|| Error::new("unresolved_reference", "Unknown input"))?;
        let b = selected_binding(runtime, config, provider, name, loaded)?;
        let channel = runtime.channels.get(&key(&b));
        let mut head = channel.and_then(|c| c.head.clone());
        if let Some(head) = &mut head {
            head.binding.provider = provider.into();
        }
        let raw_attempt = channel.and_then(|c| c.last_attempt.as_ref());
        let mut attempt = raw_attempt
            .filter(|attempt| valid_attempt(attempt) && key(&attempt.binding) == key(&b))
            .cloned();
        if let Some(attempt) = &mut attempt {
            attempt.binding.provider = provider.into();
        }
        let loaded_declaration = runtime.loaded.as_ref().and_then(|view| {
            view.providers
                .iter()
                .find(|(_, p)| p.identity == b.provider_identity)
                .and_then(|(p, _)| {
                    binding(view, runtime, p, name)
                        .filter(|other| key(other) == key(&b))
                        .and_then(|_| view.config.providers[p].inputs.get(name))
                })
        });
        let interval = loaded_declaration.and_then(|d| d.refresh_interval_seconds);
        let busy = if capability(p, d) {
            store.is_locked(&acquisition_name(&b))
        } else {
            Ok(None)
        };
        let acquisition_state = if d.source == "manual" {
            "manual"
        } else if !capability(p, d) || busy.is_err() {
            "unavailable"
        } else if busy.as_ref().is_ok_and(|state| *state == Some(true)) {
            "in_progress"
        } else if !running && d.refresh_interval_seconds.is_some() {
            "paused"
        } else if running && loaded_declaration.is_none() && d.refresh_interval_seconds.is_some() {
            "not_loaded"
        } else if interval.is_some() {
            "scheduled"
        } else {
            "on_demand"
        };
        let availability = availability(head.as_ref(), &b, d, at);
        let invalid = availability["reason_code"] == "invalid_record";
        if invalid {
            head = None;
        }
        let deps: Vec<_> = p
            .models
            .iter()
            .flat_map(|(m, model)| {
                model.priority_rules.iter().filter(move |r| r.input == name)
        .map(move |r| json!({"qualified_model":format!("{provider}/{m}"),"rule_id":r.id}))
            })
            .collect();
        Ok(
            json!({"input":name,"provider":provider,"kind":d.kind,"source":d.source,"scope":d.scope,"models":d.models,
        "current_revision":if invalid||b.declaration_revision.is_empty(){Value::Null}else{json!(revision(channel))},"availability":availability,
        "acquisition_state":acquisition_state,"last_attempt_outcome":attempt.as_ref().map(|a|&a.outcome),
        "declaration_revision":if b.declaration_revision.is_empty(){Value::Null}else{json!(b.declaration_revision)},"declaration":d,"current_data":head,"last_attempt":attempt,
        "last_attempt_state":if raw_attempt.is_some()&&attempt.is_none(){"unavailable"}else if attempt.is_some(){"known"}else{"absent"},"dependencies":deps,
        "saved_use":use_projection(runtime,&b,false,running),"current_runtime_use":use_projection(runtime,&b,true,running),
        "acquisition":{"state":acquisition_state,"policy_configuration_revision":runtime.loaded.as_ref().filter(|_|running).and_then(|view|view.revision.as_deref()),
            "active_refresh_interval_seconds":if running {interval}else{None},"next_due_at":if running{scheduled_next_due(store,&b)}else{None},
            "reason_code":if d.source!="manual"&&!capability(p,d){Some("unsupported_source")}else if busy.is_err(){Some("storage_unavailable")}else if acquisition_state=="paused"{Some("service_not_running")}else{None}}}),
        )
    }
}
