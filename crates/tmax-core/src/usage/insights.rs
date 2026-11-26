//! Deterministic, evidence-bound usage insights.
//!
//! Rules read only one usage report snapshot and the dated allowance readings.
//! A rule that cannot fire honestly reports a refusal with the first failing
//! condition in its documented order; a rule with no applicable scope is
//! omitted. Nothing here mutates, observes a provider, or changes routing.
use crate::{
    Result,
    storage::Store,
    usage::{
        Period, ReadingRow,
        report::{self, Duration, Snapshot, Span},
    },
};
use chrono::{DateTime, SecondsFormat, Utc};
use num_bigint::BigUint;
use num_traits::ToPrimitive;
use serde_json::{Map, Value, json};

/// Documented per-result refusal bound; retained refusals keep rule order then
/// scope order.
const REFUSAL_BOUND: usize = 200;
/// Minimum priced records for a cost-concentration claim.
const CONCENTRATION_MINIMUM: i64 = 20;
/// Minimum priced share for a cost-concentration claim, in percent.
const CONCENTRATION_PRICED_PERCENT: i64 = 80;
/// Minimum share of one provider's estimated cost, in percent.
const CONCENTRATION_PAIR_PERCENT: i64 = 50;
/// Minimum failed/rejected records for a failure-cluster claim.
const FAILURE_MINIMUM: i64 = 5;
/// Minimum unpriced records for an unpriced-usage claim.
const UNPRICED_MINIMUM: i64 = 5;
/// Minimum unpriced share for an unpriced-usage claim, in percent.
const UNPRICED_PERCENT: i64 = 10;
/// Minimum requests for an attribution-gap claim.
const ATTRIBUTION_MINIMUM: i64 = 20;
/// Minimum unknown-target share for an attribution-gap claim, in percent.
const ATTRIBUTION_PERCENT: i64 = 50;
/// Minimum p90 regression factor for a latency-regression claim, as a ratio of
/// `current * 2 >= baseline * 3` (1.5x without floating point).
const LATENCY_FACTOR_NUMERATOR: i64 = 3;
const LATENCY_FACTOR_DENOMINATOR: i64 = 2;

fn instant(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn instant_ms(ms: i64) -> Value {
    match DateTime::<Utc>::from_timestamp_millis(ms) {
        Some(at) => json!(instant(at)),
        None => Value::Null,
    }
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn operation_action(operation: &str, arguments: Value) -> Value {
    json!({"operation": operation, "arguments": arguments})
}

fn guidance(message: &str) -> Value {
    json!({"operation": Value::Null, "arguments": [], "message": message})
}

fn eval_scope(
    period: &Value,
    provider: Option<&str>,
    binding: Option<&str>,
    window: Option<&str>,
    model: Option<&str>,
    target: Option<&str>,
) -> Value {
    let mut scope = Map::new();
    scope.insert("period".into(), period.clone());
    for (key, value) in [
        ("provider", provider),
        ("binding", binding),
        ("window", window),
        ("model", model),
        ("target", target),
    ] {
        if let Some(value) = value {
            scope.insert(key.into(), json!(value));
        }
    }
    Value::Object(scope)
}

/// One fired claim before its evidence context is attached.
struct Claim {
    rule: &'static str,
    scope: Value,
    claim: String,
    samples: i64,
    values: Value,
    assumption: Option<&'static str>,
    action: Value,
}

fn fired(claim: Claim, coverage: &Value, generated_at: &str) -> Value {
    let Claim {
        rule,
        scope,
        claim,
        samples,
        values,
        assumption,
        action,
    } = claim;
    json!({
        "rule": rule,
        "scope": scope,
        "claim": claim,
        "evidence": {
            "samples": samples,
            "coverage": coverage.clone(),
            "values": values,
        },
        "assumption": assumption.map(|text| json!(text)).unwrap_or(Value::Null),
        "action": action,
        "generated_at": generated_at,
    })
}

fn refused(rule: &str, scope: Value, reason: &str) -> Value {
    json!({"rule": rule, "scope": scope, "reason": reason})
}

/// Exact non-negative window-share ratio used by `quota_pace`.
#[derive(Clone)]
struct Share {
    numerator: BigUint,
    denominator: BigUint,
}

fn ten_pow(power: usize) -> BigUint {
    BigUint::from(10u8).pow(power as u32)
}

/// Parse an unsigned finite decimal string into an exact coefficient and scale.
fn decimal_parts(text: &str) -> Option<(BigUint, usize)> {
    let mut pieces = text.split('.');
    let integer = pieces.next()?;
    let fraction = pieces.next();
    if pieces.next().is_some() {
        return None;
    }
    if integer.is_empty() || !integer.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fraction = fraction.unwrap_or("");
    if !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let coefficient = BigUint::parse_bytes(format!("{integer}{fraction}").as_bytes(), 10)?;
    Some((coefficient, fraction.len()))
}

impl Share {
    fn from_percent(text: &str) -> Option<Self> {
        let (numerator, scale) = decimal_parts(text)?;
        Some(Self {
            numerator,
            denominator: ten_pow(scale),
        })
    }

    fn from_remaining_percent(text: &str) -> Option<Self> {
        let (remaining, scale) = decimal_parts(text)?;
        let denominator = ten_pow(scale);
        let full = &denominator * BigUint::from(100u8);
        if remaining > full {
            return None;
        }
        Some(Self {
            numerator: full - remaining,
            denominator,
        })
    }

    /// Convert a counter group to its consumed share. Counter axes count the
    /// same unit as the ledger tokens only when the recorded unit says so; a
    /// credit basis uses the same fraction because credits are its own unit.
    fn from_counters(counters: &str, unit: Option<&str>, require_token_unit: bool) -> Option<Self> {
        let value: Value = serde_json::from_str(counters).ok()?;
        if require_token_unit {
            let unit = unit?;
            if !unit.to_ascii_lowercase().contains("token") {
                return None;
            }
        }
        let total_text = ["total", "limit"]
            .iter()
            .find_map(|key| value.get(*key).and_then(Value::as_str))?;
        let used_text = match value.get("remaining").and_then(Value::as_str) {
            Some(remaining) => {
                let (total, total_scale) = decimal_parts(total_text)?;
                let (remaining, remaining_scale) = decimal_parts(remaining)?;
                let scale = total_scale.max(remaining_scale);
                let total = total * ten_pow(scale - total_scale);
                let remaining = remaining * ten_pow(scale - remaining_scale);
                if remaining > total {
                    return None;
                }
                let used = &total - &remaining;
                return Some(Self {
                    numerator: used * BigUint::from(100u8),
                    denominator: total,
                });
            }
            None => value.get("used").and_then(Value::as_str)?,
        };
        let (used, used_scale) = decimal_parts(used_text)?;
        let (total, total_scale) = decimal_parts(total_text)?;
        let scale = used_scale.max(total_scale);
        let used = used * ten_pow(scale - used_scale);
        let total = total * ten_pow(scale - total_scale);
        if total == BigUint::from(0u8) {
            return None;
        }
        Some(Self {
            numerator: used * BigUint::from(100u8),
            denominator: total,
        })
    }

    fn value(&self) -> f64 {
        let numerator = self.numerator.to_f64().unwrap_or(f64::NAN);
        let denominator = self.denominator.to_f64().unwrap_or(f64::NAN);
        numerator / denominator
    }
}

/// True when the consumed share moved by at least one percentage point in
/// either direction, compared exactly with integer arithmetic.
fn movement_at_least_one_point(first: &Share, last: &Share) -> bool {
    let left = &last.numerator * &first.denominator;
    let right = &first.numerator * &last.denominator;
    let difference = if left >= right {
        left - right
    } else {
        right - left
    };
    difference >= &first.denominator * &last.denominator
}

fn reading_share(reading: &ReadingRow, basis: &str) -> Option<Share> {
    match basis {
        "consumed_percent" => reading.percent.as_deref().and_then(Share::from_percent),
        "remaining_percent" => reading
            .percent
            .as_deref()
            .and_then(Share::from_remaining_percent),
        "counters" => {
            Share::from_counters(reading.counters.as_deref()?, reading.unit.as_deref(), true)
        }
        // Credits are comparable within their own unit; the mixed-credit
        // refusal reports the basis after the earlier conditions pass.
        "credit" => reading
            .percent
            .as_deref()
            .and_then(Share::from_percent)
            .or_else(|| Share::from_counters(reading.counters.as_deref()?, None, false)),
        _ => None,
    }
}

fn credit_reading(basis: &str, readings: &[&ReadingRow]) -> bool {
    basis == "credit"
        || readings.iter().any(|reading| {
            reading
                .unit
                .as_deref()
                .is_some_and(|unit| unit.to_ascii_lowercase().contains("credit"))
                || reading
                    .counters
                    .as_deref()
                    .is_some_and(|counters| counters.to_ascii_lowercase().contains("credit"))
        })
}

fn parse_instant_text(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

/// Parse the qualified reset encoding retained with a reading. An absent or
/// unqualified encoding is not a reset clock.
fn reset_instant(reset: &Value) -> Option<DateTime<Utc>> {
    match reset {
        Value::String(text) => parse_instant_text(text),
        Value::Number(number) => {
            let millis = number.as_i64()?;
            DateTime::<Utc>::from_timestamp_millis(millis)
        }
        Value::Object(map) => {
            if map
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.contains("absent") || kind.contains("unqualified"))
            {
                return None;
            }
            if map
                .get("encoding")
                .and_then(Value::as_str)
                .is_some_and(|encoding| encoding.contains("unqualified"))
            {
                return None;
            }
            for key in [
                "at",
                "reset_at",
                "resets_at",
                "resetsAt",
                "resetAt",
                "instant",
                "time",
            ] {
                if let Some(value) = map.get(key) {
                    return reset_instant(value);
                }
            }
            let raw = map.get("raw")?;
            if let Some(millis) = raw.as_i64() {
                return DateTime::<Utc>::from_timestamp_millis(millis);
            }
            let text = raw.as_str()?;
            parse_instant_text(text).or_else(|| {
                text.parse::<i64>()
                    .ok()
                    .and_then(DateTime::<Utc>::from_timestamp_millis)
            })
        }
        _ => None,
    }
}

/// One consistent projection over the report's aggregates, used by the rules.
struct Facts {
    totals: report::Totals,
    costs: Vec<report::Cost>,
    unpriced: (i64, i64),
    failures: Vec<report::FailureGroup>,
    concentration: Vec<report::ConcentrationRow>,
    attribution: (i64, i64),
    readings: Vec<ReadingRow>,
    current: Duration,
    baseline: Option<Duration>,
    coverage_oldest: Option<i64>,
    coverage: report::Coverage,
    now: DateTime<Utc>,
}

impl Facts {
    fn load(snapshot: &Snapshot, now: DateTime<Utc>) -> Result<Self> {
        let conn = snapshot
            .conn()
            .expect("available snapshot has a connection");
        let readings = report::reading_rows(conn, snapshot.reading_span())?;
        let Some(span) = snapshot.usage_span() else {
            return Ok(Self {
                totals: report::Totals::empty(),
                costs: Vec::new(),
                unpriced: (0, 0),
                failures: Vec::new(),
                concentration: Vec::new(),
                attribution: (0, 0),
                readings,
                current: Duration::empty(),
                baseline: None,
                coverage_oldest: snapshot.coverage.oldest_ms(),
                coverage: snapshot.coverage.clone(),
                now,
            });
        };
        let totals = report::Totals::load(conn, span)?;
        let (costs, _) = report::provider_costs(conn, Some(span), None)?;
        let unreserved = report::unpriced_counts(conn, span)?;
        let failures = report::failure_groups(conn, span)?;
        let concentration = report::concentration_rows(conn, span)?;
        let attribution = report::attribution_counts(conn, span)?;
        let current = totals.duration();
        let baseline = match baseline_span(snapshot) {
            Some(span) => Some(report::duration_from_samples(report::duration_samples(
                conn, span,
            )?)),
            None => None,
        };
        Ok(Self {
            totals,
            costs,
            unpriced: unreserved,
            failures,
            concentration,
            attribution,
            readings,
            current,
            baseline,
            coverage_oldest: snapshot.coverage.oldest_ms(),
            coverage: snapshot.coverage.clone(),
            now,
        })
    }

    fn empty(now: DateTime<Utc>) -> Self {
        Self {
            totals: report::Totals::empty(),
            costs: Vec::new(),
            unpriced: (0, 0),
            failures: Vec::new(),
            concentration: Vec::new(),
            attribution: (0, 0),
            readings: Vec::new(),
            current: Duration::empty(),
            baseline: None,
            coverage_oldest: None,
            coverage: report::Coverage::absent(),
            now,
        }
    }
}

/// The preceding equal-length period, when the retained ledger covers it.
fn baseline_span(snapshot: &Snapshot) -> Option<Span> {
    if snapshot.period.kind == "all" {
        return None;
    }
    let since = snapshot.period.since?;
    let until = snapshot.period.until?;
    let length = until - since;
    if length <= chrono::TimeDelta::zero() {
        return None;
    }
    let baseline_since = since - length;
    let baseline_until = since;
    let oldest = snapshot.coverage.oldest_ms()?;
    if oldest > baseline_since.timestamp_millis() {
        return None;
    }
    Some(Span {
        since_ms: baseline_since.timestamp_millis(),
        until_ms: baseline_until.timestamp_millis(),
    })
}

struct Evaluation {
    insights: Vec<Value>,
    refusals: Vec<Value>,
}

impl Evaluation {
    fn new() -> Self {
        Self {
            insights: Vec::new(),
            refusals: Vec::new(),
        }
    }
}

fn quota_pace(
    facts: &Facts,
    period: &Value,
    coverage: &Value,
    generated_at: &str,
    evaluation: &mut Evaluation,
) {
    let mut groups: std::collections::BTreeMap<(String, String), Vec<&ReadingRow>> =
        std::collections::BTreeMap::new();
    for reading in &facts.readings {
        groups
            .entry((reading.provider.clone(), reading.window.clone()))
            .or_default()
            .push(reading);
    }
    for ((provider, window), readings) in groups {
        let base_scope = eval_scope(period, Some(&provider), None, Some(&window), None, None);
        if readings.len() < 2 {
            evaluation
                .refusals
                .push(refused("quota_pace", base_scope, "single_reading"));
            continue;
        }
        let binding = &readings[0].binding;
        if readings.iter().any(|reading| reading.binding != *binding) {
            evaluation
                .refusals
                .push(refused("quota_pace", base_scope, "binding_changed"));
            continue;
        }
        let scope = eval_scope(
            period,
            Some(&provider),
            Some(binding),
            Some(&window),
            None,
            None,
        );
        if facts
            .coverage_oldest
            .is_none_or(|oldest| oldest > readings[0].observed_at_ms)
        {
            evaluation
                .refusals
                .push(refused("quota_pace", scope, "window_before_coverage"));
            continue;
        }
        let first = readings[0];
        let last = readings[readings.len() - 1];
        // A known coverage loss inside the evaluated interval makes the observed
        // movement unverifiable. Unbounded losses (dropped, evicted, restart)
        // could always sit inside it, so they refuse too.
        let evaluated_until = facts
            .coverage
            .read_at_ms()
            .unwrap_or_else(|| facts.now.timestamp_millis());
        if facts
            .coverage
            .has_gap_within(first.observed_at_ms, evaluated_until)
        {
            evaluation
                .refusals
                .push(refused("quota_pace", scope, "coverage_gap"));
            continue;
        }
        let basis = &last.basis;
        if readings.iter().any(|reading| reading.basis != *basis) {
            evaluation
                .refusals
                .push(refused("quota_pace", scope, "unknown_basis"));
            continue;
        }
        let Some(share_first) = reading_share(first, basis) else {
            evaluation
                .refusals
                .push(refused("quota_pace", scope, "unknown_basis"));
            continue;
        };
        let Some(share_last) = reading_share(last, basis) else {
            evaluation
                .refusals
                .push(refused("quota_pace", scope, "unknown_basis"));
            continue;
        };
        if !movement_at_least_one_point(&share_first, &share_last) {
            evaluation
                .refusals
                .push(refused("quota_pace", scope, "low_usage"));
            continue;
        }
        let Some(reset_at) = reset_instant(&last.reset) else {
            evaluation
                .refusals
                .push(refused("quota_pace", scope, "no_reset_clock"));
            continue;
        };
        if reset_at <= facts.now || reset_at.timestamp_millis() <= last.observed_at_ms {
            evaluation
                .refusals
                .push(refused("quota_pace", scope, "no_reset_clock"));
            continue;
        }
        if credit_reading(basis, &readings) {
            evaluation
                .refusals
                .push(refused("quota_pace", scope, "mixed_credits"));
            continue;
        }
        let interval_hours = (i128::from(last.observed_at_ms) - i128::from(first.observed_at_ms))
            as f64
            / 3_600_000.0;
        if interval_hours <= 0.0 {
            evaluation
                .refusals
                .push(refused("quota_pace", scope, "below_threshold"));
            continue;
        }
        let first_percent = share_first.value();
        let last_percent = share_last.value();
        let movement = last_percent - first_percent;
        let rate = movement / interval_hours;
        let projection = if rate > 0.0 && last_percent < 100.0 {
            let hours_to_full = (100.0 - last_percent) / rate;
            let full_at = last.observed_at_ms as f64 + hours_to_full * 3_600_000.0;
            let reset_at_ms = reset_at.timestamp_millis() as f64;
            let offset_hours = (full_at - reset_at_ms).abs() / 3_600_000.0;
            let relation = if full_at < reset_at_ms {
                "before"
            } else {
                "after"
            };
            format!(
                " At that rate the window would reach 100% about {offset_hours:.1} hours {relation} the announced reset at {}.",
                instant(reset_at)
            )
        } else {
            String::new()
        };
        let claim = format!(
            "Provider {provider} window {window} moved the consumed window share from {first_percent:.1}% to {last_percent:.1}% between {} and {}, an observed rate of {rate:+.2} percentage points per hour over {interval_hours:.1} hours.{projection} This is a derived estimate from dated readings and observed gateway usage; it is not a provider limit or a remaining balance, and it fails if the consumption rate or the allowance changes.",
            instant_ms_string(first.observed_at_ms),
            instant_ms_string(last.observed_at_ms),
        );
        let reset_value = instant_ms(reset_at.timestamp_millis());
        let values = json!({
            "provider": provider,
            "binding": binding,
            "window": window,
            "basis": basis,
            "readings": readings.len(),
            "first_observed_at": instant_ms(first.observed_at_ms),
            "last_observed_at": instant_ms(last.observed_at_ms),
            "interval_hours": round2(interval_hours),
            "consumed_share_first_percent": round2(first_percent),
            "consumed_share_last_percent": round2(last_percent),
            "share_delta_percentage_points": round2(movement),
            "rate_percentage_points_per_hour": round2(rate),
            "reset_at": reset_value,
        });
        let action = operation_action(
            "provider refresh",
            json!({"provider": [provider], "scope": ["quota"]}),
        );
        evaluation.insights.push(fired(
            Claim {
                rule: "quota_pace",
                scope,
                claim,
                samples: readings.len() as i64,
                values,
                assumption: Some("The observed consumption rate and the provider-declared allowance remain unchanged until the announced reset."),
                action,
            },
            coverage,
            generated_at,
        ));
    }
}

fn instant_ms_string(ms: i64) -> String {
    match DateTime::<Utc>::from_timestamp_millis(ms) {
        Some(at) => instant(at),
        None => "unknown".into(),
    }
}

fn unpriced_usage(
    facts: &Facts,
    period: &Value,
    coverage: &Value,
    generated_at: &str,
    evaluation: &mut Evaluation,
) {
    let (records, unpriced) = facts.unpriced;
    let fires = unpriced >= UNPRICED_MINIMUM
        || (records > 0
            && i128::from(unpriced) * 100 >= i128::from(records) * i128::from(UNPRICED_PERCENT));
    let scope = eval_scope(period, None, None, None, None, None);
    if !fires {
        evaluation
            .refusals
            .push(refused("unpriced_usage", scope, "below_threshold"));
        return;
    }
    let percent = if records > 0 {
        unpriced as f64 * 100.0 / records as f64
    } else {
        0.0
    };
    let claim = format!(
        "{unpriced} of {records} retained records in this period ({percent:.1}%) have no effective price, so their usage is excluded from every cost estimate."
    );
    let values = json!({
        "records": records,
        "unpriced_records": unpriced,
        "unpriced_percent": round1(percent),
    });
    let action = guidance(
        "No supported operation prices already-recorded requests. Configure an explicit local price override or accept a catalog price for the affected provider models, then record new requests so their estimates freeze at the effective price.",
    );
    evaluation.insights.push(fired(
        Claim {
            rule: "unpriced_usage",
            scope,
            claim,
            samples: records,
            values,
            assumption: Some("A record is unpriced only when no effective price existed at its write time; a later price change never restates it."),
            action,
        },
        coverage,
        generated_at,
    ));
}

fn failure_cluster(
    facts: &Facts,
    period: &Value,
    coverage: &Value,
    generated_at: &str,
    evaluation: &mut Evaluation,
) {
    for group in &facts.failures {
        let scope = eval_scope(period, None, None, None, None, None);
        if group.count < FAILURE_MINIMUM {
            evaluation
                .refusals
                .push(refused("failure_cluster", scope, "below_threshold"));
            continue;
        }
        let claim = format!(
            "{} failed or rejected gateway requests share stage \"{}\" and controlled reason code \"{}\" in this period.",
            group.count, group.stage, group.reason_code
        );
        let values = json!({
            "stage": group.stage,
            "reason_code": group.reason_code,
            "count": group.count,
        });
        let action = guidance(
            "Inspect the affected provider's request outcomes before changing configuration; retries and fallbacks do not create additional records.",
        );
        evaluation.insights.push(fired(
            Claim {
                rule: "failure_cluster",
                scope,
                claim,
                samples: group.count,
                values,
                assumption: None,
                action,
            },
            coverage,
            generated_at,
        ));
    }
}

fn concentrated_pair<'a>(facts: &'a Facts, provider: &str) -> Option<&'a report::ConcentrationRow> {
    facts
        .concentration
        .iter()
        .filter(|row| row.provider == provider)
        .max_by(|left, right| {
            left.cost_micros
                .cmp(&right.cost_micros)
                .then_with(|| right.target.cmp(&left.target))
                .then_with(|| right.model.cmp(&left.model))
        })
}

fn cost_concentration(
    facts: &Facts,
    period: &Value,
    coverage: &Value,
    generated_at: &str,
    evaluation: &mut Evaluation,
) {
    for cost in &facts.costs {
        let Some(provider) = cost.provider.as_deref() else {
            continue;
        };
        let scope = eval_scope(period, Some(provider), None, None, None, None);
        if i128::from(cost.priced) * 100
            < i128::from(cost.records()) * i128::from(CONCENTRATION_PRICED_PERCENT)
        {
            evaluation
                .refusals
                .push(refused("cost_concentration", scope, "low_priced_share"));
            continue;
        }
        if cost.priced < CONCENTRATION_MINIMUM {
            evaluation
                .refusals
                .push(refused("cost_concentration", scope, "low_samples"));
            continue;
        }
        let total = cost.estimate_micros();
        let pair = concentrated_pair(facts, provider);
        let concentrated = pair.is_some_and(|pair| {
            total > 0 && pair.cost_micros * 100 >= total * i128::from(CONCENTRATION_PAIR_PERCENT)
        });
        if !concentrated {
            evaluation
                .refusals
                .push(refused("cost_concentration", scope, "below_threshold"));
            continue;
        }
        let pair = pair.expect("concentrated pair exists");
        let share = pair.cost_micros as f64 * 100.0 / total as f64;
        let target_text = pair.target.as_deref().unwrap_or("unknown");
        let model_text = pair.model.as_deref().unwrap_or("unknown");
        let claim = format!(
            "Target {target_text} with model {model_text} accounts for {share:.1}% of provider {provider}'s estimated cost (USD {} of USD {}) over {} priced records in this period.",
            report::micro_usd_wide(pair.cost_micros),
            report::micro_usd_wide(total),
            cost.priced,
        );
        let fired_scope = eval_scope(
            period,
            Some(provider),
            None,
            None,
            pair.model.as_deref(),
            pair.target.as_deref(),
        );
        let values = json!({
            "provider": provider,
            "target": pair.target,
            "model": pair.model,
            "pair_estimate_usd": report::micro_usd_wide(pair.cost_micros),
            "provider_estimate_usd": report::micro_usd_wide(total),
            "share_percent": round1(share),
            "priced_records": cost.priced,
            "records": cost.records(),
        });
        let action = operation_action("provider inspect", json!({"provider": [provider]}));
        evaluation.insights.push(fired(
            Claim {
                rule: "cost_concentration",
                scope: fired_scope,
                claim,
                samples: cost.priced,
                values,
                assumption: Some("The frozen estimates use the effective price source recorded with each request; they are API-equivalent estimates, not provider charges."),
                action,
            },
            coverage,
            generated_at,
        ));
    }
}

fn latency_regression(
    facts: &Facts,
    period: &Value,
    coverage: &Value,
    generated_at: &str,
    evaluation: &mut Evaluation,
) {
    let scope = eval_scope(period, None, None, None, None, None);
    let Some(baseline) = facts.baseline else {
        evaluation
            .refusals
            .push(refused("latency_regression", scope, "no_baseline"));
        return;
    };
    if facts.current.samples < report::SAMPLE_MINIMUM || baseline.samples < report::SAMPLE_MINIMUM {
        evaluation
            .refusals
            .push(refused("latency_regression", scope, "low_samples"));
        return;
    }
    let (Some(current_p90), Some(baseline_p90)) = (facts.current.p90_ms, baseline.p90_ms) else {
        evaluation
            .refusals
            .push(refused("latency_regression", scope, "low_samples"));
        return;
    };
    let regressed = baseline_p90 > 0
        && current_p90.saturating_mul(LATENCY_FACTOR_DENOMINATOR)
            >= baseline_p90.saturating_mul(LATENCY_FACTOR_NUMERATOR);
    if !regressed {
        evaluation
            .refusals
            .push(refused("latency_regression", scope, "below_threshold"));
        return;
    }
    let ratio = current_p90 as f64 / baseline_p90 as f64;
    let claim = format!(
        "Completed-request p90 duration is {current_p90} ms in this period, {ratio:.2}x the {baseline_p90} ms p90 of the preceding equal period ({} current samples, {} baseline samples).",
        facts.current.samples, baseline.samples,
    );
    let values = json!({
        "current_p50_ms": facts.current.p50_ms,
        "current_p90_ms": current_p90,
        "current_samples": facts.current.samples,
        "baseline_p50_ms": baseline.p50_ms,
        "baseline_p90_ms": baseline_p90,
        "baseline_samples": baseline.samples,
        "ratio": round2(ratio),
        "period_requests": facts.totals.requests(),
    });
    let action = guidance(
        "Compare the affected period's outcomes and provider evidence before changing routing; an observed duration is not a deadline or a failure.",
    );
    evaluation.insights.push(fired(
        Claim {
            rule: "latency_regression",
            scope,
            claim,
            samples: facts.current.samples,
            values,
            assumption: Some("The comparison uses equal-length periods and is not normalized for traffic, model, or provider mix."),
            action,
        },
        coverage,
        generated_at,
    ));
}

fn attribution_gap(
    facts: &Facts,
    period: &Value,
    coverage: &Value,
    generated_at: &str,
    evaluation: &mut Evaluation,
) {
    let (records, unknown) = facts.attribution;
    let fires = records >= ATTRIBUTION_MINIMUM
        && i128::from(unknown) * 100 >= i128::from(records) * i128::from(ATTRIBUTION_PERCENT);
    let scope = eval_scope(period, None, None, None, None, None);
    if !fires {
        evaluation
            .refusals
            .push(refused("attribution_gap", scope, "below_threshold"));
        return;
    }
    let percent = if records > 0 {
        unknown as f64 * 100.0 / records as f64
    } else {
        0.0
    };
    let claim = format!(
        "{unknown} of {records} gateway requests in this period ({percent:.1}%) have no established Agent target, so native attribution is incomplete."
    );
    let values = json!({
        "requests": records,
        "unknown_target": unknown,
        "unknown_percent": round1(percent),
    });
    let action = guidance(
        "Route requests through a configured Agent target so the gateway can establish native attribution; an unknown target is never filled from current configuration.",
    );
    evaluation.insights.push(fired(
        Claim {
            rule: "attribution_gap",
            scope,
            claim,
            samples: records,
            values,
            assumption: Some("A missing Agent target means attribution was not independently established; it is not evidence about any specific Agent or session."),
            action,
        },
        coverage,
        generated_at,
    ));
}

fn evaluate(
    snapshot: &Snapshot,
    facts: &Facts,
    generated_at: &str,
) -> (Vec<Value>, Vec<Value>, i64) {
    let period = snapshot.period_json();
    let coverage = snapshot.coverage.evidence_json();
    let mut evaluation = Evaluation::new();
    quota_pace(facts, &period, &coverage, generated_at, &mut evaluation);
    unpriced_usage(facts, &period, &coverage, generated_at, &mut evaluation);
    failure_cluster(facts, &period, &coverage, generated_at, &mut evaluation);
    cost_concentration(facts, &period, &coverage, generated_at, &mut evaluation);
    latency_regression(facts, &period, &coverage, generated_at, &mut evaluation);
    attribution_gap(facts, &period, &coverage, generated_at, &mut evaluation);
    let omitted = evaluation.refusals.len().saturating_sub(REFUSAL_BOUND);
    evaluation.refusals.truncate(REFUSAL_BOUND);
    (evaluation.insights, evaluation.refusals, omitted as i64)
}

fn result_data(
    snapshot: &Snapshot,
    insights: Vec<Value>,
    refusals: Vec<Value>,
    omitted_refusals: i64,
) -> Value {
    json!({
        "period": snapshot.period_json(),
        "scope": report::scope(),
        "coverage": snapshot.coverage_json(),
        "disclosures": report::disclosures(),
        "insights": insights,
        "refusals": refusals,
        "omitted_refusals": omitted_refusals,
    })
}

fn unavailable_data(period: &Period, reason_code: &str) -> Value {
    let snapshot = Snapshot::unavailable(period, reason_code);
    result_data(&snapshot, Vec::new(), Vec::new(), 0)
}

pub(crate) fn report(store: &Store, period: &Period) -> Result<Value> {
    let snapshot = Snapshot::load(store, period);
    let now = Utc::now();
    let generated_at = instant(now);
    if !snapshot.coverage.is_available() {
        if !snapshot.coverage.is_absent() {
            // Unavailable coverage fabricates no aggregate, insight, or refusal.
            return Ok(result_data(&snapshot, Vec::new(), Vec::new(), 0));
        }
        // An absent store has no applicable reading/provider scopes, but the
        // period-wide rules still refuse with their first failing condition.
        let facts = Facts::empty(now);
        let (insights, refusals, omitted) = evaluate(&snapshot, &facts, &generated_at);
        return Ok(result_data(&snapshot, insights, refusals, omitted));
    }
    let facts = match Facts::load(&snapshot, now) {
        Ok(facts) => facts,
        Err(error) => return Ok(unavailable_data(period, &error.code)),
    };
    let (insights, refusals, omitted) = evaluate(&snapshot, &facts, &generated_at);
    Ok(result_data(&snapshot, insights, refusals, omitted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::report::test_support as support;
    use chrono::Duration;

    fn report_data(store: &Store, period: &Period) -> Value {
        report(store, period).expect("insight report")
    }

    fn fired<'a>(data: &'a Value, rule: &str) -> Option<&'a Value> {
        data["insights"]
            .as_array()?
            .iter()
            .find(|insight| insight["rule"] == rule)
    }

    fn refusal<'a>(data: &'a Value, rule: &str, provider: Option<&str>) -> Option<&'a Value> {
        data["refusals"].as_array()?.iter().find(|refusal| {
            refusal["rule"] == rule
                && match provider {
                    Some(provider) => refusal["scope"]["provider"] == provider,
                    None => refusal["scope"]["provider"].is_null(),
                }
        })
    }

    fn refusal_reason<'a>(data: &'a Value, rule: &str, provider: Option<&str>) -> Option<&'a str> {
        refusal(data, rule, provider)?.get("reason")?.as_str()
    }

    #[test]
    fn quota_pace_refusals_follow_the_documented_condition_order() {
        let (_directory, store) = support::store();
        let now = Utc::now();
        let base = now - Duration::days(30);
        let base_ms = base.timestamp_millis();
        let at = |hours: i64| (base + Duration::hours(hours)).timestamp_millis();
        let before = |days: i64| (base - Duration::days(days)).timestamp_millis();
        let reset = (now + Duration::hours(2)).to_rfc3339();
        let reset_value = json!({"kind": "announced", "at": reset});
        let mut readings = vec![
            support::reading(at(1), "single", "w", "consumed_percent", "10"),
            support::reading(at(1), "bind", "w", "consumed_percent", "10"),
            support::reading(at(2), "bind", "w", "consumed_percent", "20"),
            support::reading(before(2), "before", "w", "consumed_percent", "10"),
            support::reading(before(1), "before", "w", "consumed_percent", "20"),
            support::reading(at(1), "basis", "w", "counters", "10"),
            support::reading(at(2), "basis", "w", "counters", "20"),
            support::reading(at(1), "low", "w", "consumed_percent", "10"),
            support::reading(at(2), "low", "w", "consumed_percent", "10.5"),
            support::reading(at(1), "noclock", "w", "consumed_percent", "10"),
            support::reading(at(2), "noclock", "w", "consumed_percent", "20"),
            support::reading(before(2), "order", "w", "consumed_percent", "10"),
            support::reading(before(1), "order", "w", "consumed_percent", "20"),
        ];
        // Different authorization bindings cannot be compared.
        readings[1].binding = "binding-one".into();
        readings[2].binding = "binding-two".into();
        readings[11].binding = "order-one".into();
        readings[12].binding = "order-two".into();
        // A credit basis with a readable percentage and a reset clock reaches the
        // last documented condition.
        let mut credit_first = support::reading(at(1), "credit", "w", "credit", "10");
        credit_first.reset = reset_value.clone();
        let mut credit_second = support::reading(at(2), "credit", "w", "credit", "20");
        credit_second.reset = reset_value.clone();
        readings.push(credit_first);
        readings.push(credit_second);
        support::append(&store, vec![support::client(base_ms)], readings);
        let period = support::range(
            (now - Duration::days(40)).timestamp_millis(),
            (now + Duration::hours(1)).timestamp_millis(),
        );
        let data = report_data(&store, &period);
        assert_eq!(data["coverage"]["state"], "available");
        assert!(fired(&data, "quota_pace").is_none());
        for (provider, reason) in [
            ("single", "single_reading"),
            ("bind", "binding_changed"),
            ("order", "binding_changed"),
            ("before", "window_before_coverage"),
            ("basis", "unknown_basis"),
            ("low", "low_usage"),
            ("noclock", "no_reset_clock"),
            ("credit", "mixed_credits"),
        ] {
            assert_eq!(
                refusal_reason(&data, "quota_pace", Some(provider)),
                Some(reason),
                "{provider}"
            );
        }
    }

    #[test]
    fn quota_pace_fires_with_two_comparable_readings_and_ledger_usage() {
        let (_directory, store) = support::store();
        let now = Utc::now();
        let base = now - Duration::hours(8);
        let reset = now + Duration::hours(2);
        let mut first = support::reading(
            (now - Duration::hours(4)).timestamp_millis(),
            "go",
            "rolling",
            "consumed_percent",
            "20",
        );
        first.binding = "binding-a".into();
        first.reset = json!({"kind": "announced", "at": reset.to_rfc3339()});
        let mut second = first.clone();
        second.observed_at_ms = (now - Duration::hours(2)).timestamp_millis();
        second.received_at_ms = second.observed_at_ms;
        second.percent = Some("35".into());
        let mut used = support::client((now - Duration::hours(3)).timestamp_millis());
        used.input_tokens = Some(1_000);
        support::append(
            &store,
            vec![support::client(base.timestamp_millis()), used],
            vec![first, second],
        );
        let period = support::range(
            (now - Duration::hours(10)).timestamp_millis(),
            now.timestamp_millis(),
        );
        let data = report_data(&store, &period);
        let insight = fired(&data, "quota_pace").expect("quota pace insight");
        assert_eq!(insight["scope"]["provider"], "go");
        assert_eq!(insight["scope"]["binding"], "binding-a");
        assert_eq!(insight["scope"]["window"], "rolling");
        assert_eq!(insight["evidence"]["samples"], 2);
        assert_eq!(
            insight["evidence"]["values"]["share_delta_percentage_points"],
            15.0
        );
        assert_eq!(
            insight["evidence"]["values"]["rate_percentage_points_per_hour"],
            7.5
        );
        assert_eq!(insight["action"]["operation"], "provider refresh");
        let claim = insight["claim"].as_str().expect("claim text");
        assert!(claim.contains("percentage points"), "{claim}");
        assert!(claim.contains("derived estimate"), "{claim}");
        assert!(insight["assumption"].as_str().is_some());
        assert!(insight["generated_at"].as_str().is_some());
    }

    #[test]
    fn below_threshold_rules_refuse_without_firing() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let mut row = support::client(now - 600_000);
        row.duration_ms = Some(12);
        support::append(&store, vec![row], Vec::new());
        let period = support::range(now - 3_600_000, now + 3_600_000);
        let data = report_data(&store, &period);
        assert!(fired(&data, "unpriced_usage").is_none());
        assert!(fired(&data, "attribution_gap").is_none());
        assert_eq!(
            refusal_reason(&data, "unpriced_usage", None),
            Some("below_threshold")
        );
        assert_eq!(
            refusal_reason(&data, "attribution_gap", None),
            Some("below_threshold")
        );
        assert_eq!(data["omitted_refusals"], 0);
    }

    #[test]
    fn latency_regression_reports_no_baseline_for_all() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let rows: Vec<_> = (0..25)
            .map(|index| {
                let mut row = support::client(now - 60_000 - index * 1_000);
                row.duration_ms = Some(10 + index);
                row
            })
            .collect();
        support::append(&store, rows, Vec::new());
        let data = report_data(&store, &support::all());
        assert!(fired(&data, "latency_regression").is_none());
        assert_eq!(
            refusal_reason(&data, "latency_regression", None),
            Some("no_baseline")
        );
    }

    #[test]
    fn unpriced_and_failure_thresholds_fire_with_their_evidence() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let mut rows = Vec::new();
        for index in 0..5 {
            let mut row = support::client(now - 60_000 - index * 1_000);
            row.provider = Some("p".into());
            row.cost_state = "unpriced".into();
            rows.push(row);
        }
        for index in 0..6 {
            let mut row = support::client(now - 120_000 - index * 1_000);
            row.provider = Some("p".into());
            row.outcome = "failed".into();
            row.stage = "upstream_response".into();
            row.reason_code = Some("rate_limited".into());
            rows.push(row);
        }
        let mut couple = support::client(now - 200_000);
        couple.outcome = "rejected".into();
        couple.reason_code = Some("invalid_request".into());
        rows.push(couple);
        support::append(&store, rows, Vec::new());
        let period = support::range(now - 3_600_000, now + 3_600_000);
        let data = report_data(&store, &period);
        let unpriced = fired(&data, "unpriced_usage").expect("unpriced insight");
        assert_eq!(unpriced["evidence"]["values"]["unpriced_records"], 5);
        assert_eq!(unpriced["evidence"]["values"]["records"], 12);
        assert_eq!(unpriced["action"]["operation"], Value::Null);
        let failure = fired(&data, "failure_cluster").expect("failure insight");
        assert_eq!(failure["evidence"]["values"]["count"], 6);
        assert_eq!(failure["evidence"]["values"]["reason_code"], "rate_limited");
        assert_eq!(failure["evidence"]["samples"], 6);
        // The smaller failure group is an explicit below-threshold refusal.
        let small = refusal(&data, "failure_cluster", None).expect("small group refusal");
        assert_eq!(small["reason"], "below_threshold");
        assert_eq!(failure["evidence"]["coverage"]["state"], "available");
    }

    #[test]
    fn latency_regression_fires_against_the_preceding_equal_period() {
        let (_directory, store) = support::store();
        let now = Utc::now();
        let mut rows = Vec::new();
        for index in 0..20 {
            let mut baseline =
                support::client((now - Duration::hours(3)).timestamp_millis() - index * 1_000);
            baseline.duration_ms = Some(10);
            rows.push(baseline);
            let mut current =
                support::client((now - Duration::hours(1)).timestamp_millis() - index * 1_000);
            current.duration_ms = Some(100);
            rows.push(current);
        }
        rows.push(support::client(
            (now - Duration::hours(6)).timestamp_millis(),
        ));
        support::append(&store, rows, Vec::new());
        let period = support::range(
            (now - Duration::hours(2)).timestamp_millis(),
            now.timestamp_millis(),
        );
        let data = report_data(&store, &period);
        let insight = fired(&data, "latency_regression").expect("latency insight");
        assert_eq!(insight["evidence"]["values"]["current_p90_ms"], 100);
        assert_eq!(insight["evidence"]["values"]["baseline_p90_ms"], 10);
        assert_eq!(insight["evidence"]["values"]["ratio"], 10.0);
        assert_eq!(insight["evidence"]["values"]["current_samples"], 20);
        let claim = insight["claim"].as_str().expect("claim");
        assert!(claim.contains("p90 duration"), "{claim}");
    }

    #[test]
    fn attribution_gap_fires_when_targets_are_unknown() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let rows: Vec<_> = (0..20)
            .map(|index| {
                let mut row = support::client(now - 60_000 - index * 1_000);
                row.provider = Some("p".into());
                row
            })
            .collect();
        support::append(&store, rows, Vec::new());
        let data = report_data(&store, &support::range(now - 3_600_000, now + 3_600_000));
        let insight = fired(&data, "attribution_gap").expect("attribution insight");
        assert_eq!(insight["evidence"]["values"]["requests"], 20);
        assert_eq!(insight["evidence"]["values"]["unknown_target"], 20);
        assert_eq!(insight["evidence"]["values"]["unknown_percent"], 100.0);
    }

    #[test]
    fn cost_concentration_is_evaluated_within_one_provider() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let at = now - 600_000;
        let mut rows = Vec::new();
        for index in 0..20 {
            let mut row = support::client(at - index * 1_000);
            row.provider = Some("p1".into());
            if index < 10 {
                row.target = Some("t1".into());
                row.qualified_model = Some("p1/m1".into());
                row.cost_micros = Some(100);
            } else {
                row.target = Some("t2".into());
                row.qualified_model = Some("p1/m2".into());
                row.cost_micros = Some(10);
            }
            row.cost_state = "priced".into();
            row.price_source = Some("catalog".into());
            rows.push(row);
        }
        for index in 0..20 {
            let mut row = support::client(at - 100_000 - index * 1_000);
            row.provider = Some("p2".into());
            row.target = Some(format!("t{}", index % 3));
            row.qualified_model = Some(format!("p2/m{}", index % 3));
            row.cost_micros = Some(100);
            row.cost_state = "priced".into();
            row.price_source = Some("catalog".into());
            rows.push(row);
        }
        for index in 0..10 {
            let mut row = support::client(at - 200_000 - index * 1_000);
            row.provider = Some("p3".into());
            if index < 4 {
                row.cost_micros = Some(100);
                row.cost_state = "priced".into();
                row.price_source = Some("catalog".into());
            } else {
                row.cost_state = "none".into();
            }
            rows.push(row);
        }
        support::append(&store, rows, Vec::new());
        let period = support::range(now - 3_600_000, now + 3_600_000);
        let data = report_data(&store, &period);
        let insight = fired(&data, "cost_concentration").expect("cost concentration insight");
        assert_eq!(insight["scope"]["provider"], "p1");
        assert_eq!(insight["scope"]["target"], "t1");
        assert_eq!(insight["scope"]["model"], "p1/m1");
        assert_eq!(insight["evidence"]["values"]["share_percent"], 90.9);
        assert_eq!(insight["action"]["operation"], "provider inspect");
        assert_eq!(
            refusal_reason(&data, "cost_concentration", Some("p2")),
            Some("below_threshold")
        );
        assert_eq!(
            refusal_reason(&data, "cost_concentration", Some("p3")),
            Some("low_priced_share")
        );
        assert_eq!(
            fired(&data, "cost_concentration").unwrap()["scope"]["provider"],
            "p1"
        );
    }

    #[test]
    fn refusals_are_bounded_and_omitted_counts_are_reported() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let rows: Vec<_> = (0..201)
            .map(|index| {
                let mut row = support::client(now - 600_000 - index * 1_000);
                row.provider = Some(format!("p{index:03}"));
                row.target = Some("t".into());
                row.cost_state = "none".into();
                row
            })
            .collect();
        support::append(&store, rows, Vec::new());
        let period = support::range(now - 3_600_000, now + 3_600_000);
        let data = report_data(&store, &period);
        assert_eq!(data["refusals"].as_array().unwrap().len(), REFUSAL_BOUND);
        // 201 low_priced_share refusals plus unpriced_usage, latency_regression,
        // and attribution_gap; 204 - 200 are omitted.
        assert_eq!(data["omitted_refusals"], 4);
    }

    #[test]
    fn insight_reads_do_not_mutate_the_ledger() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let mut row = support::client(now - 600_000);
        row.duration_ms = Some(20);
        support::append(
            &store,
            vec![row],
            vec![support::reading(
                now - 300_000,
                "p",
                "w",
                "consumed_percent",
                "10",
            )],
        );
        let path = support::db_path(&store);
        let before = std::fs::read(&path).expect("ledger bytes");
        let data = report_data(&store, &support::range(now - 3_600_000, now + 3_600_000));
        assert_eq!(data["coverage"]["state"], "available");
        let after = std::fs::read(&path).expect("ledger bytes");
        assert_eq!(
            before, after,
            "insight reads leave the ledger bytes unchanged"
        );
    }

    #[test]
    fn unavailable_coverage_yields_no_insights_and_no_refusals() {
        let (_directory, store) = support::store();
        support::write_corrupt(&store);
        let now = support::now_ms();
        let data = report_data(&store, &support::range(now - 3_600_000, now + 3_600_000));
        assert_eq!(data["coverage"]["state"], "unavailable");
        assert!(data["coverage"]["reason_code"].as_str().is_some());
        assert_eq!(data["insights"], json!([]));
        assert_eq!(data["refusals"], json!([]));
        assert_eq!(data["omitted_refusals"], 0);
        assert_eq!(data["period"]["since"], Value::Null);
        assert_eq!(data["scope"]["source"], "gateway_observed");
    }

    #[test]
    fn absent_coverage_evaluates_only_period_wide_rules() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let data = report_data(&store, &support::range(now - 3_600_000, now + 3_600_000));
        assert_eq!(data["coverage"]["state"], "absent");
        assert_eq!(data["insights"], json!([]));
        assert_eq!(
            refusal_reason(&data, "unpriced_usage", None),
            Some("below_threshold")
        );
        assert_eq!(
            refusal_reason(&data, "latency_regression", None),
            Some("no_baseline")
        );
        assert!(refusal(&data, "quota_pace", None).is_none());
        assert!(refusal(&data, "cost_concentration", None).is_none());
    }

    /// Build the same comparable, firable readings used by the firing test.
    fn comparable_readings(now: DateTime<Utc>) -> (Vec<ReadingRow>, i64) {
        let reset = now + Duration::hours(2);
        let mut first = support::reading(
            (now - Duration::hours(4)).timestamp_millis(),
            "go",
            "rolling",
            "consumed_percent",
            "20",
        );
        first.binding = "binding-a".into();
        first.reset = json!({"kind": "announced", "at": reset.to_rfc3339()});
        let mut second = first.clone();
        second.observed_at_ms = (now - Duration::hours(2)).timestamp_millis();
        second.received_at_ms = second.observed_at_ms;
        second.percent = Some("35".into());
        let base = (now - Duration::hours(8)).timestamp_millis();
        (vec![first, second], base)
    }

    #[test]
    fn quota_pace_refuses_when_a_coverage_gap_overlaps_the_reading_interval() {
        for (key, value) in [
            ("dropped", "1"),
            ("evicted", "1"),
            ("restart_gap", "1"),
            ("readings_dropped", "1"),
            ("readings_evicted", "1"),
        ] {
            let (_directory, store) = support::store();
            let now = Utc::now();
            let (readings, base_ms) = comparable_readings(now);
            support::append(&store, vec![support::client(base_ms)], readings);
            support::set_meta(&store, key, value);
            let period = support::range(
                (now - Duration::hours(10)).timestamp_millis(),
                now.timestamp_millis(),
            );
            let data = report_data(&store, &period);
            assert!(
                fired(&data, "quota_pace").is_none(),
                "a known coverage gap must refuse {key}"
            );
            assert_eq!(
                refusal_reason(&data, "quota_pace", Some("go")),
                Some("coverage_gap"),
                "{key}"
            );
        }

        // A clear after the first comparable reading loses part of the interval.
        let (_directory, store) = support::store();
        let now = Utc::now();
        let (readings, base_ms) = comparable_readings(now);
        support::append(&store, vec![support::client(base_ms)], readings);
        support::set_meta(
            &store,
            "cleared_at_ms",
            &(now - Duration::hours(3)).timestamp_millis().to_string(),
        );
        let period = support::range(
            (now - Duration::hours(10)).timestamp_millis(),
            now.timestamp_millis(),
        );
        let data = report_data(&store, &period);
        assert!(fired(&data, "quota_pace").is_none());
        assert_eq!(
            refusal_reason(&data, "quota_pace", Some("go")),
            Some("coverage_gap")
        );
    }

    #[test]
    fn quota_pace_reports_coverage_gap_before_the_later_conditions() {
        let (_directory, store) = support::store();
        let now = Utc::now();
        // Readings whose basis cannot convert observed tokens would refuse with
        // unknown_basis, but the coverage gap is the earlier documented failure.
        let first = support::reading(
            (now - Duration::hours(4)).timestamp_millis(),
            "basis",
            "w",
            "counters",
            "10",
        );
        let mut second = support::reading(
            (now - Duration::hours(2)).timestamp_millis(),
            "basis",
            "w",
            "counters",
            "20",
        );
        second.reset = json!({"kind": "announced", "at": (now + Duration::hours(2)).to_rfc3339()});
        support::append(
            &store,
            vec![support::client(
                (now - Duration::hours(8)).timestamp_millis(),
            )],
            vec![first, second],
        );
        support::set_meta(&store, "dropped", "1");
        let period = support::range(
            (now - Duration::hours(10)).timestamp_millis(),
            now.timestamp_millis(),
        );
        let data = report_data(&store, &period);
        assert_eq!(
            refusal_reason(&data, "quota_pace", Some("basis")),
            Some("coverage_gap")
        );
    }
}
