use crate::{
    Error, Result,
    config::{self, InputDeclaration},
};
use chrono::{DateTime, Utc};
use num_traits::Zero;
use serde_json::{Value, json};

use super::{
    exact::{Decimal, ExactInstant},
    lineage::token,
    model::{AcquisitionAttempt, Binding, Channel, InputHead, InputValue},
};

fn safe_reference(reference: &str) -> Result<()> {
    let url = url::Url::parse(reference).map_err(|_| {
        Error::new(
            "invalid_reference",
            "Provenance must be an absolute HTTPS URL",
        )
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::new(
            "invalid_reference",
            "Provenance must be HTTPS without credentials, query, or fragment",
        ));
    }
    Ok(())
}
pub(super) fn normalize(value: &mut InputValue, declaration: &InputDeclaration) -> Result<()> {
    for text in [
        &mut value.observed_at,
        &mut value.received_at,
        &mut value.valid_until,
        &mut value.reset_at,
    ] {
        *text = ExactInstant::parse(text)?.canonical();
    }
    if !["manual", "source", "retrieval"].contains(&value.observed_at_basis.as_str()) {
        return Err(Error::new("invalid_record", "Unknown observation basis"));
    }
    if declaration.source == "manual" && value.observed_at_basis != "manual"
        || declaration.source != "manual" && value.observed_at_basis == "manual"
    {
        return Err(Error::new(
            "invalid_record",
            "Observation basis does not match source",
        ));
    }
    if let Some(reference) = &value.reference {
        safe_reference(reference)?;
    }
    match (
        declaration.kind.as_str(),
        &value.remaining_percent,
        &value.remaining,
        &value.total,
        &value.unit,
    ) {
        ("reset", None, None, None, None) => {}
        ("quota", Some(percent), None, None, None) => {
            value.remaining_percent = Some(Decimal::percent(percent)?.text);
        }
        ("quota", None, Some(remaining), Some(total), Some(unit)) => {
            let remaining = Decimal::parse(remaining)?;
            let total = Decimal::parse(total)?;
            if total.coefficient.is_zero()
                || remaining.cmp(&total).is_gt()
                || unit.is_empty()
                || unit.chars().any(char::is_control)
            {
                return Err(Error::new(
                    "invalid_record",
                    "Quota counters require comparable quantities, positive total, and a nonempty unit",
                ));
            }
            value.remaining = Some(remaining.text);
            value.total = Some(total.text);
        }
        _ => {
            return Err(Error::new(
                "invalid_record",
                "Quota requires one percentage or complete counters; reset forbids quota fields",
            ));
        }
    }
    let observed = ExactInstant::parse(&value.observed_at)?;
    let received = ExactInstant::parse(&value.received_at)?;
    let expires = ExactInstant::parse(&value.valid_until)?;
    let reset = ExactInstant::parse(&value.reset_at)?;
    let cap = observed.add_seconds(declaration.max_age_seconds);
    if observed > received
        || expires <= observed
        || expires > reset
        || cap.is_some_and(|cap| expires > cap)
    {
        return Err(Error::new(
            "invalid_observation",
            "Observation or expiry violates the declared freshness bounds",
        ));
    }
    Ok(())
}

pub(super) fn same_models(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && a.iter().all(|m| b.contains(m)) && b.iter().all(|m| a.contains(m))
}
pub(super) fn valid_binding(b: &Binding) -> bool {
    config::valid_id(&b.provider, false)
        && config::valid_id(&b.input, false)
        && [
            &b.provider_identity,
            &b.authorization_generation,
            &b.declaration_revision,
        ]
        .iter()
        .all(|s| !s.is_empty() && !s.chars().any(char::is_control))
        && ["quota", "reset"].contains(&b.kind.as_str())
        && ["manual", "provider", "codex-resets"].contains(&b.source.as_str())
        && !b.scope.is_empty()
        && !b.scope.chars().any(char::is_control)
        && !b.models.is_empty()
        && b.models.iter().enumerate().all(|(i, m)| {
            !m.is_empty() && !m.chars().any(char::is_control) && !b.models[..i].contains(m)
        })
}
pub(super) fn availability(
    head: Option<&InputHead>,
    b: &Binding,
    declaration: &InputDeclaration,
    at: DateTime<Utc>,
) -> Value {
    availability_checked(
        head,
        b,
        &ExactInstant::from_chrono(at),
        head.is_none_or(|h| same_models(&h.binding.models, &b.models)),
        Some(declaration),
    )
}
pub(super) fn availability_checked(
    head: Option<&InputHead>,
    b: &Binding,
    at: &ExactInstant,
    coverage: bool,
    declaration: Option<&InputDeclaration>,
) -> Value {
    let reason = match head {
        None => Some("missing"),
        Some(head) => {
            let h = &head.binding;
            if !valid_binding(h)
                || head.revision.is_empty()
                || head.value.is_some() == head.unavailable_reason.is_some()
                || head.unavailable_reason.as_ref().is_some_and(|r| {
                    ![
                        "cleared",
                        "withdrawn",
                        "declaration_mismatch",
                        "authorization_mismatch",
                    ]
                    .contains(&r.as_str())
                })
                || head.value.as_ref().is_some_and(|v| {
                    let max_age = declaration
                        .filter(|_| {
                            h.provider_identity == b.provider_identity
                                && h.declaration_revision == b.declaration_revision
                                && h.kind == b.kind
                                && h.source == b.source
                                && h.scope == b.scope
                                && coverage
                        })
                        .map_or(i64::MAX, |d| d.max_age_seconds);
                    let declaration = InputDeclaration {
                        kind: h.kind.clone(),
                        source: h.source.clone(),
                        scope: h.scope.clone(),
                        models: h.models.clone(),
                        max_age_seconds: max_age,
                        refresh_interval_seconds: None,
                    };
                    let mut normalized = v.clone();
                    normalize(&mut normalized, &declaration).is_err()
                        || head
                            .observation_floor
                            .as_deref()
                            .and_then(|s| ExactInstant::parse(s).ok())
                            != ExactInstant::parse(&v.observed_at).ok()
                })
                || head
                    .observation_floor
                    .as_ref()
                    .is_some_and(|s| ExactInstant::parse(s).is_err())
                || head.value.is_none()
                    && ((head.unavailable_reason.as_deref() == Some("withdrawn"))
                        != head.observation_floor.is_some())
            {
                Some("invalid_record")
            } else if h.provider_identity != b.provider_identity
                || h.declaration_revision != b.declaration_revision
                || h.kind != b.kind
                || h.source != b.source
                || h.scope != b.scope
                || !coverage
            {
                Some("declaration_mismatch")
            } else if h.authorization_generation != b.authorization_generation {
                Some("authorization_mismatch")
            } else if let Some(reason) = &head.unavailable_reason {
                Some(reason.as_str())
            } else if let Some(value) = &head.value {
                if at < &ExactInstant::parse(&value.observed_at).unwrap() {
                    Some("not_yet_observed")
                } else if at >= &ExactInstant::parse(&value.valid_until).unwrap() {
                    Some("expired")
                } else {
                    None
                }
            } else {
                Some("invalid_record")
            }
        }
    };
    json!({"state":if reason.is_none() {"available"} else {"unavailable"},"reason_code":reason})
}
pub(crate) fn quota_matches(basis: &Value, threshold: &str) -> Result<bool> {
    let threshold = Decimal::percent(threshold)?;
    if let Some(percent) = basis["remaining_percent"].as_str() {
        return Ok(!Decimal::percent(percent)?.cmp(&threshold).is_lt());
    }
    let remaining = Decimal::parse(
        basis["remaining"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_record", "Missing remaining counter"))?,
    )?;
    let total = Decimal::parse(
        basis["total"]
            .as_str()
            .ok_or_else(|| Error::new("invalid_record", "Missing total counter"))?,
    )?;
    Ok(!Decimal::parse("100")?
        .product(&remaining)
        .cmp(&threshold.product(&total))
        .is_lt())
}

fn same_observation(a: &InputValue, b: &InputValue) -> bool {
    // Receipt metadata is local, but expiry is part of the source assertion, not a freshness renewal.
    a.observed_at == b.observed_at
        && a.observed_at_basis == b.observed_at_basis
        && a.valid_until == b.valid_until
        && a.reset_at == b.reset_at
        && a.remaining_percent == b.remaining_percent
        && a.remaining == b.remaining
        && a.total == b.total
        && a.unit == b.unit
        && a.reference == b.reference
}
pub(super) fn prepare_observation(
    channel: &Channel,
    declaration: &InputDeclaration,
    value: &mut InputValue,
    at: DateTime<Utc>,
) -> Result<bool> {
    normalize(value, declaration)?;
    let observed = ExactInstant::parse(&value.observed_at)?;
    if let Some(head) = &channel.head
        && let Some(floor) = &head.observation_floor
    {
        let floor = ExactInstant::parse(floor)?;
        if observed < floor {
            return Err(Error::new(
                "older_observation",
                "Observation predates the current ordering watermark",
            ));
        }
        if observed == floor {
            if head
                .value
                .as_ref()
                .is_some_and(|old| same_observation(old, value))
            {
                return Ok(false);
            }
            return Err(Error::new(
                "conflicting_observation",
                "A different assertion already exists at this observation instant",
            ));
        }
    }
    let publication = ExactInstant::from_chrono(at);
    if ExactInstant::parse(&value.received_at)? > publication
        || ExactInstant::parse(&value.reset_at)? <= publication
        || ExactInstant::parse(&value.valid_until)? <= publication
    {
        return Err(Error::new(
            "invalid_observation",
            "A new value must be received, unexpired, and strictly before its reset",
        ));
    }
    Ok(true)
}
pub(super) fn publish(
    channel: &mut Channel,
    b: &Binding,
    declaration: &InputDeclaration,
    mut value: InputValue,
    at: DateTime<Utc>,
) -> Result<bool> {
    if !prepare_observation(channel, declaration, &mut value, at)? {
        return Ok(false);
    }
    channel.head = Some(InputHead {
        binding: b.clone(),
        revision: token(),
        observation_floor: Some(value.observed_at.clone()),
        value: Some(value),
        unavailable_reason: None,
    });
    Ok(true)
}

pub(super) fn valid_attempt(attempt: &AcquisitionAttempt) -> bool {
    if !valid_binding(&attempt.binding)
        || attempt.revision.is_empty()
        || attempt.head_revision_before.is_empty()
        || attempt.head_revision_after.is_empty()
        || !["explicit", "automatic", "provider_refresh"].contains(&attempt.trigger.as_str())
    {
        return false;
    }
    let (Ok(started), Ok(finished)) = (
        ExactInstant::parse(&attempt.started_at),
        ExactInstant::parse(&attempt.finished_at),
    ) else {
        return false;
    };
    if started > finished {
        return false;
    }
    let reason = attempt.reason_code.as_deref();
    match attempt.outcome.as_str() {
        "published" => reason.is_none(),
        "unchanged" => reason == Some("identical_observation"),
        "withdrawn" => reason.is_some_and(|r| {
            [
                "explicit_absence",
                "explicit_withdrawal",
                "reset_time_passed",
            ]
            .contains(&r)
        }),
        "failed" => reason.is_some_and(|r| {
            [
                "transport_error",
                "source_format_error",
                "invalid_observation",
                "older_observation",
                "conflicting_observation",
                "storage_unavailable",
            ]
            .contains(&r)
        }),
        "unavailable" => reason.is_some_and(|r| {
            [
                "unsupported_source",
                "credential_unavailable",
                "noninteractive_unavailable",
            ]
            .contains(&r)
        }),
        "superseded" => reason.is_some_and(|r| {
            [
                "input_changed",
                "declaration_changed",
                "authorization_changed",
                "declaration_removed",
            ]
            .contains(&r)
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        exact::instant,
        model::revision,
        test_support::{declaration, value},
    };
    use super::*;
    fn binding() -> Binding {
        Binding {
            provider: "p".into(),
            input: "i".into(),
            provider_identity: "p1".into(),
            authorization_generation: "a1".into(),
            declaration_revision: "d1".into(),
            kind: "quota".into(),
            scope: "weekly".into(),
            models: vec!["m".into()],
            source: "manual".into(),
        }
    }
    #[test]
    fn fractional_instants_remain_exact_beyond_nanoseconds() {
        let a = ExactInstant::parse("2026-10-23T10:00:00.000000000100000Z").unwrap();
        let equivalent = ExactInstant::parse("2026-10-23T12:00:00.0000000001+02:00").unwrap();
        let later = ExactInstant::parse("2026-10-23T10:00:00.0000000002Z").unwrap();
        assert_eq!(a, equivalent);
        assert!(a < later);
        assert_eq!(a.canonical(), "2026-10-23T10:00:00.0000000001Z");
        let mut channel = Channel::default();
        let d = declaration();
        let b = binding();
        let mut first = value();
        first.observed_at = a.canonical();
        publish(
            &mut channel,
            &b,
            &d,
            first,
            instant("2026-10-23T10:00:02Z").unwrap(),
        )
        .unwrap();
        let mut second = value();
        second.observed_at = later.canonical();
        second.remaining = Some("2".into());
        assert!(
            publish(
                &mut channel,
                &b,
                &d,
                second,
                instant("2026-10-23T10:00:03Z").unwrap()
            )
            .unwrap()
        );
        let between = ExactInstant::parse("2026-10-23T10:00:00.00000000015Z").unwrap();
        assert_eq!(
            availability_checked(channel.head.as_ref(), &b, &between, true, Some(&d))["reason_code"],
            "not_yet_observed"
        );
        let mut retry = value();
        retry.observed_at = "2026-10-23T12:00:00.000000000200000+02:00".into();
        retry.remaining = Some("0002.000".into());
        assert!(
            !publish(
                &mut channel,
                &b,
                &d,
                retry,
                instant("2026-10-23T10:00:04Z").unwrap()
            )
            .unwrap()
        );
    }
    #[test]
    fn malformed_record_precedes_binding_mismatch_and_nulls_are_required() {
        let mut channel = Channel::default();
        let d = declaration();
        let b = binding();
        publish(
            &mut channel,
            &b,
            &d,
            value(),
            instant("2026-10-23T10:00:02Z").unwrap(),
        )
        .unwrap();
        let original = serde_json::to_value(channel.head.as_ref().unwrap()).unwrap();
        assert!(serde_json::from_value::<InputHead>(original.clone()).is_ok());
        let mut missing = original.clone();
        missing
            .as_object_mut()
            .unwrap()
            .remove("unavailable_reason");
        assert!(serde_json::from_value::<InputHead>(missing).is_err());
        let mut unknown = original;
        unknown["source_body"] = json!("never accepted");
        assert!(serde_json::from_value::<InputHead>(unknown).is_err());
        channel
            .head
            .as_mut()
            .unwrap()
            .value
            .as_mut()
            .unwrap()
            .remaining = Some("NaN".into());
        let mut changed = b;
        changed.declaration_revision = "replacement".into();
        assert_eq!(
            availability(
                channel.head.as_ref(),
                &changed,
                &d,
                instant("2026-10-23T13:00:00Z").unwrap()
            )["reason_code"],
            "invalid_record"
        );
    }
    #[test]
    fn recurring_ratio_is_not_rounded() {
        let basis = json!({"remaining":"1","total":"3"});
        assert!(quota_matches(&basis, "33.33").unwrap());
        assert!(!quota_matches(&basis, "33.34").unwrap());
        assert!(
            quota_matches(
                &basis,
                "33.333333333333333333333333333333333333333333333333333333"
            )
            .unwrap()
        );
    }
    #[test]
    fn lexical_variants_are_idempotent_without_freshness_extension() {
        let mut c = Channel::default();
        let d = declaration();
        let b = binding();
        let at = instant("2026-10-23T10:00:02Z").unwrap();
        assert!(publish(&mut c, &b, &d, value(), at).unwrap());
        let revision = revision(Some(&c)).to_owned();
        let mut retry = value();
        retry.observed_at = "2026-10-23T12:00:00.000+02:00".into();
        retry.remaining = Some("0001.000".into());
        retry.received_at = "2026-10-23T10:10:00Z".into();
        assert!(
            !publish(
                &mut c,
                &b,
                &d,
                retry,
                instant("2026-10-23T11:30:00Z").unwrap()
            )
            .unwrap()
        );
        assert_eq!(revision, super::super::model::revision(Some(&c)));
        assert_eq!(
            c.head.unwrap().value.unwrap().received_at,
            "2026-10-23T10:00:01Z"
        );
    }
    #[test]
    fn same_instant_conflicts_and_expiry_is_exclusive() {
        let mut c = Channel::default();
        let d = declaration();
        let b = binding();
        publish(
            &mut c,
            &b,
            &d,
            value(),
            instant("2026-10-23T10:00:02Z").unwrap(),
        )
        .unwrap();
        let mut conflict = value();
        conflict.remaining = Some("2".into());
        assert_eq!(
            publish(
                &mut c,
                &b,
                &d,
                conflict,
                instant("2026-10-23T10:00:03Z").unwrap()
            )
            .unwrap_err()
            .code,
            "conflicting_observation"
        );
        assert_eq!(
            availability(
                c.head.as_ref(),
                &b,
                &d,
                instant("2026-10-23T11:00:00Z").unwrap()
            )["reason_code"],
            "expired"
        );
    }
}
