use crate::{Error, Result, config::Config, storage::Store};
use chrono::Utc;
use serde_json::{Value, json};

use super::{
    codex_resets::{capability, fetch_forecast},
    exact::{ExactInstant, after_seconds, timestamp},
    lineage::{
        binding, current_saved_binding, guard_reason, key, selected_binding, signature, token,
    },
    model::{AcquisitionAttempt, Binding, InputHead, InputValue, revision},
    snapshot::use_projection,
    store::{read_runtime, write_runtime},
    value::publish,
};

// Cross-process acquisition exclusion is distinct from the short publication lock.
// It is held over source I/O; admission and manual record/clear never acquire it.
pub(super) fn acquisition_name(b: &Binding) -> String {
    format!(
        "input-acquisition-{}",
        signature(&(&b.provider_identity, &b.input))
    )
}
pub(super) fn acquisition_lock(store: &Store, b: &Binding) -> Result<crate::storage::LockGuard> {
    store.try_lock(&acquisition_name(b))?.ok_or_else(|| {
        Error::new(
            "acquisition_in_progress",
            "An acquisition is already in progress for this input",
        )
    })
}

fn persistence_failure(
    provider: &str,
    input: &str,
    before: &str,
    acquisition_result: &str,
    uncertain: bool,
    error: &Error,
) -> Value {
    json!({"_code":"storage_unavailable","provider":provider,"input_name":input,
        "input_change":{"outcome":"not_published","current_revision_before":before,"current_revision_after":null,
            "published":if uncertain{None}else{Some(false)},"attempt_recorded":if uncertain{None}else{Some(false)}},
        "attempt":null,"unavailable_reason":"storage_unavailable","acquisition_completed":true,"acquisition_result":acquisition_result,
        "publication_state":if uncertain{"unknown"}else{"not_performed"},"attempt_persistence":if uncertain{"unknown"}else{"not_performed"},"persistence_error":error})
}

pub(super) async fn acquire(
    store: &Store,
    config: &Config,
    provider: &str,
    input: &str,
    loaded: bool,
    trigger: &str,
    running: bool,
) -> Result<Value> {
    let declaration = &config.providers[provider].inputs[input];
    let (mut b, before, _exclusive) = {
        let _configuration = if loaded {
            None
        } else {
            Some(store.lock("configuration")?)
        };
        let _lock = store.lock("routing")?;
        let runtime = read_runtime(store)?;
        let b = selected_binding(&runtime, config, provider, input, loaded)?;
        if !loaded {
            let current = current_saved_binding(store, &runtime, &b)?;
            if let Some(reason) = guard_reason(&b, current.as_ref(), "absent", "absent") {
                return Err(Error::new("inputs_changed", reason));
            }
        }
        let exclusive = acquisition_lock(store, &b)?;
        let before = revision(runtime.channels.get(&key(&b))).to_owned();
        (b, before, exclusive)
    };
    let started = Utc::now();
    let result = if capability(&config.providers[provider], declaration) {
        match tokio::time::timeout(std::time::Duration::from_secs(30), fetch_forecast(config)).await
        {
            Ok(result) => result,
            Err(_) => Err(Error::new(
                "transport_error",
                "Input source deadline elapsed",
            )),
        }
    } else {
        Err(Error::new(
            "unsupported_source",
            "The integration has no qualified acquisition for this input",
        ))
    };
    let received = Utc::now();
    let acquisition_result = match &result {
        Ok(_) => "acquired".to_owned(),
        Err(error) => error.code.clone(),
    };
    let prepared = (|| -> Result<_> {
        let configuration = if loaded {
            None
        } else {
            Some(store.lock("configuration")?)
        };
        let lock = store.lock("routing")?;
        let runtime = read_runtime(store)?;
        Ok((configuration, lock, runtime))
    })();
    let (_configuration, _lock, mut runtime) = match prepared {
        Ok(state) => state,
        Err(error) => {
            return Ok(persistence_failure(
                provider,
                input,
                &before,
                &acquisition_result,
                false,
                &error,
            ));
        }
    };
    let finished = Utc::now();
    let (current, authority_error) = if loaded {
        (
            runtime.loaded.as_ref().and_then(|view| {
                view.providers
                    .iter()
                    .find(|(_, p)| p.identity == b.provider_identity)
                    .and_then(|(p, _)| binding(view, &runtime, p, input))
            }),
            None,
        )
    } else {
        match current_saved_binding(store, &runtime, &b) {
            Ok(current) => (current, None),
            Err(error) => (None, Some(error)),
        }
    };
    let provider = current
        .as_ref()
        .filter(|now| now.provider_identity == b.provider_identity)
        .map(|now| now.provider.as_str())
        .unwrap_or(provider);
    b.provider = provider.into();
    let mut attempt = AcquisitionAttempt {
        binding: b.clone(),
        revision: token(),
        started_at: timestamp(started),
        finished_at: timestamp(finished),
        trigger: trigger.into(),
        outcome: String::new(),
        reason_code: None,
        head_revision_before: before.clone(),
        head_revision_after: revision(runtime.channels.get(&key(&b))).into(),
    };
    let after = current
        .as_ref()
        .map(|b| revision(runtime.channels.get(&key(b))))
        .unwrap_or("absent");
    let superseded = if authority_error.is_some() {
        Some("declaration_changed")
    } else {
        guard_reason(&b, current.as_ref(), &before, after)
    };
    let mut published = false;
    let mut attach = false;
    if let Some(reason) = superseded {
        attempt.outcome = "superseded".into();
        attempt.reason_code = Some(reason.into());
        if let Some(now) = &current
            && key(now) == key(&b)
        {
            attach = true;
            attempt.head_revision_after = revision(runtime.channels.get(&key(now))).into();
        }
    } else {
        attach = true;
        let channel = runtime.channels.entry(key(&b)).or_default();
        match result {
            Err(error) => {
                attempt.outcome = if [
                    "unsupported_source",
                    "credential_unavailable",
                    "noninteractive_unavailable",
                ]
                .contains(&error.code.as_str())
                {
                    "unavailable"
                } else {
                    "failed"
                }
                .into();
                attempt.reason_code = Some(error.code);
            }
            Ok(forecast) => {
                if forecast.reset_at <= ExactInstant::from_chrono(finished) {
                    let floor = channel
                        .head
                        .as_ref()
                        .and_then(|h| h.observation_floor.as_deref())
                        .map(ExactInstant::parse)
                        .transpose()?;
                    let observed = ExactInstant::from_chrono(received);
                    if floor.as_ref().is_some_and(|floor| &observed < floor) {
                        attempt.outcome = "failed".into();
                        attempt.reason_code = Some("older_observation".into());
                    } else if floor.as_ref() == Some(&observed) {
                        attempt.outcome = "failed".into();
                        attempt.reason_code = Some("conflicting_observation".into());
                    } else {
                        channel.head = Some(InputHead {
                            binding: b.clone(),
                            revision: token(),
                            observation_floor: Some(timestamp(received)),
                            value: None,
                            unavailable_reason: Some("withdrawn".into()),
                        });
                        attempt.outcome = "withdrawn".into();
                        attempt.reason_code = Some("reset_time_passed".into());
                        published = true;
                    }
                } else {
                    let expiry = ExactInstant::from_chrono(after_seconds(
                        received,
                        declaration.max_age_seconds,
                    ))
                    .min(forecast.reset_at.clone());
                    let value = InputValue {
                        observed_at: timestamp(received),
                        observed_at_basis: "retrieval".into(),
                        received_at: timestamp(received),
                        valid_until: expiry.canonical(),
                        reset_at: forecast.reset_at.canonical(),
                        remaining_percent: None,
                        remaining: None,
                        total: None,
                        unit: None,
                        reference: Some(forecast.reference),
                    };
                    match publish(channel, &b, declaration, value, finished) {
                        Ok(true) => {
                            attempt.outcome = "published".into();
                            published = true;
                        }
                        Ok(false) => {
                            attempt.outcome = "unchanged".into();
                            attempt.reason_code = Some("identical_observation".into());
                        }
                        Err(error) => {
                            attempt.outcome = "failed".into();
                            attempt.reason_code = Some(error.code);
                        }
                    }
                }
            }
        }
        attempt.head_revision_after = revision(Some(channel)).into();
    }
    if attach {
        runtime.channels.entry(key(&b)).or_default().last_attempt = Some(attempt.clone());
        if let Err(error) = write_runtime(store, &runtime) {
            return Ok(persistence_failure(
                provider,
                input,
                &before,
                &acquisition_result,
                true,
                &error,
            ));
        }
    }
    let runtime_use = use_projection(&runtime, &b, true, running);
    let code = match attempt.outcome.as_str() {
        "failed" | "unavailable" => attempt.reason_code.as_deref(),
        "superseded" => Some("inputs_changed"),
        _ => None,
    };
    Ok(
        json!({"_code":code,"provider":provider,"input_name":input,"input_change":{"outcome":if published{attempt.outcome.as_str()}else if attempt.outcome=="unchanged"{"unchanged"}else{"not_published"},
        "current_revision_before":before,"current_revision_after":attempt.head_revision_after,"published":published,"attempt_recorded":attach},"attempt":attempt,"current_runtime_use":runtime_use}),
    )
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{fixture, retain_value};
    use super::*;
    #[test]
    fn clear_wins_over_in_flight_acquisition_and_attempt_exclusion_is_shared() {
        let (_directory, store, config) = fixture();
        let before = retain_value(&store, &config);
        let runtime = read_runtime(&store).unwrap();
        let b = super::binding(&runtime.saved, &runtime, "p", "i").unwrap();
        let acquisition = acquisition_lock(&store, &b).unwrap();
        assert_eq!(store.is_locked(&acquisition_name(&b)).unwrap(), Some(true));
        assert_eq!(
            acquisition_lock(&store, &b).err().unwrap().code,
            "acquisition_in_progress"
        );
        let writer = store.clone();
        let captured = b.clone();
        std::thread::spawn(move || {
            let _lock = writer.lock("routing").unwrap();
            let mut state = read_runtime(&writer).unwrap();
            let channel_key = key(&captured);
            state.channels.get_mut(&channel_key).unwrap().head = Some(InputHead {
                binding: captured,
                revision: token(),
                observation_floor: None,
                value: None,
                unavailable_reason: Some("cleared".into()),
            });
            write_runtime(&writer, &state).unwrap();
        })
        .join()
        .unwrap();
        let current = read_runtime(&store).unwrap();
        let now = super::binding(&current.saved, &current, "p", "i").unwrap();
        assert_eq!(
            guard_reason(
                &b,
                Some(&now),
                &before,
                revision(current.channels.get(&key(&now)))
            ),
            Some("input_changed")
        );
        assert!(current.channels[&key(&b)].last_attempt.is_none());
        drop(acquisition);
        assert_eq!(store.is_locked(&acquisition_name(&b)).unwrap(), Some(false));
    }
}
