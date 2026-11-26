//! Map terminal request evidence into content-free usage rows.
use crate::{catalog, config::Config, diagnostics::Record, usage::UsageRow};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeMap;

/// Effective price metadata available at the terminal point: the loaded
/// configuration and the accepted catalog snapshot, if any.
#[derive(Clone, Copy)]
pub(crate) struct Pricing<'a> {
    pub config: &'a Config,
    pub metadata: Option<&'a catalog::Snapshot>,
}

/// Content-free facts of one terminal request. The gateway record and an
/// explicit paid probe both map into this shape.
pub(crate) struct CaptureInput<'a> {
    pub origin: &'a str,
    pub received_at: DateTime<Utc>,
    pub admitted_at: Option<DateTime<Utc>>,
    pub requested_model: Option<&'a str>,
    pub model_kind: &'a str,
    pub provider: Option<&'a str>,
    pub qualified_model: Option<&'a str>,
    pub target: Option<&'a str>,
    pub agent_provider: Option<&'a str>,
    pub protocol: Option<&'a str>,
    pub outcome: &'a str,
    pub stage: &'a str,
    pub reason_code: Option<&'a str>,
    pub duration_ms: Option<u64>,
    pub usage: &'a BTreeMap<String, u64>,
    pub usage_state: &'a str,
}

/// Largest token counter this ledger will price or sum. A provider value above
/// it is not usable evidence: the axis is dropped rather than clamped, so a
/// malformed number cannot inflate totals or overflow aggregation.
const MAX_COUNTER: u64 = 1_000_000_000_000;

/// Counter names that belong to a known protocol family. A name from another
/// family that targets the same axis makes that axis ambiguous.
const CHAT_COMPLETIONS: &[&str] = &["prompt_tokens", "completion_tokens"];
const RESPONSES: &[&str] = &["input_tokens", "output_tokens"];
const MESSAGES: &[&str] = &[
    "input_tokens",
    "output_tokens",
    "cache_read_input_tokens",
    "cache_creation_input_tokens",
];
const ALL_COUNTER_NAMES: &[&str] = &[
    "prompt_tokens",
    "completion_tokens",
    "input_tokens",
    "output_tokens",
    "cache_read_input_tokens",
    "cache_creation_input_tokens",
];

#[derive(Debug, Clone, Copy, Default)]
struct AxisPrice {
    value: Option<f64>,
    source: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, Default)]
struct EffectivePrice {
    input: AxisPrice,
    output: AxisPrice,
    cache_read: AxisPrice,
    cache_write: AxisPrice,
}

impl EffectivePrice {
    fn any(self) -> bool {
        self.input.value.is_some()
            || self.output.value.is_some()
            || self.cache_read.value.is_some()
            || self.cache_write.value.is_some()
    }
}

#[derive(Debug, Clone, Default)]
struct Normalized {
    input: Option<i64>,
    output: Option<i64>,
    cache_read: Option<i64>,
    cache_write: Option<i64>,
    /// A counter conflict or a recognized value from another protocol family.
    ambiguous: bool,
    /// A recognized counter exceeded [`MAX_COUNTER`] and was dropped.
    oversized: bool,
    /// At least one recognized counter name was present, even as a zero.
    recognized: bool,
}

impl Normalized {
    fn known(&self) -> usize {
        [self.input, self.output, self.cache_read, self.cache_write]
            .into_iter()
            .flatten()
            .count()
    }

    fn state(&self, captured: &str) -> &'static str {
        if !self.recognized {
            return if self.ambiguous {
                "ambiguous"
            } else {
                "unknown"
            };
        }
        if self.ambiguous {
            return "ambiguous";
        }
        if self.oversized {
            return if self.known() == 0 {
                "unknown"
            } else {
                "partial"
            };
        }
        if captured == "partial" {
            "partial"
        } else {
            "reported"
        }
    }
}

pub(crate) fn capture_record(record: &Record, pricing: Option<Pricing<'_>>) -> UsageRow {
    capture(
        &CaptureInput {
            origin: &record.origin,
            received_at: record.received_at,
            admitted_at: record.admitted_at,
            requested_model: record.model.as_deref(),
            model_kind: &record.model_kind,
            provider: record.provider.as_deref(),
            qualified_model: record.qualified_model.as_deref(),
            target: record.target.as_deref(),
            agent_provider: record.agent_provider.as_deref(),
            protocol: record.protocol.as_deref(),
            outcome: &record.outcome,
            stage: &record.stage,
            reason_code: record.reason_code.as_deref(),
            duration_ms: record.duration_ms,
            usage: &record.usage,
            usage_state: &record.usage_state,
        },
        pricing,
    )
}

/// Facts of one explicit paid probe, which runs outside the daemon.
pub(crate) struct ProbeFacts<'a> {
    pub observed_at: DateTime<Utc>,
    pub provider: &'a str,
    pub model: &'a str,
    pub protocol: &'a str,
    pub upstream_status: Option<u16>,
    pub outcome: &'a str,
    pub duration_ms: Option<u64>,
    pub usage: &'a BTreeMap<String, u64>,
    pub usage_state: &'a str,
}

pub(crate) fn capture_probe(facts: &ProbeFacts<'_>, pricing: Option<Pricing<'_>>) -> UsageRow {
    let qualified = format!("{}/{}", facts.provider, facts.model);
    let _ = facts.upstream_status;
    capture(
        &CaptureInput {
            origin: "probe",
            received_at: facts.observed_at,
            admitted_at: Some(facts.observed_at),
            requested_model: Some(facts.model),
            model_kind: "concrete",
            provider: Some(facts.provider),
            qualified_model: Some(&qualified),
            target: None,
            agent_provider: None,
            protocol: Some(facts.protocol),
            outcome: facts.outcome,
            stage: "upstream_response",
            reason_code: None,
            duration_ms: facts.duration_ms,
            usage: facts.usage,
            usage_state: facts.usage_state,
        },
        pricing,
    )
}

fn capture(input: &CaptureInput<'_>, pricing: Option<Pricing<'_>>) -> UsageRow {
    let normalized = normalize(input.protocol, input.usage);
    let effective = match (input.provider, input.qualified_model, pricing) {
        (Some(provider), Some(qualified), Some(pricing)) => {
            match qualified.strip_prefix(&format!("{provider}/")) {
                Some(model) => price_for(pricing, provider, model),
                None => None,
            }
        }
        _ => None,
    };
    let (cost_micros, price_source, cost_state) =
        estimate(&normalized, input.usage_state, effective.as_ref());
    let billing_kind = match (input.provider, pricing) {
        (Some(provider), Some(pricing)) => billing_kind(pricing.config.providers.get(provider)),
        _ => "unknown",
    };
    let membership_ms = input
        .admitted_at
        .unwrap_or(input.received_at)
        .timestamp_millis();
    UsageRow {
        received_at_ms: input.received_at.timestamp_millis(),
        admitted_at_ms: input.admitted_at.map(|at| at.timestamp_millis()),
        membership_ms,
        origin: bounded_str(Some(input.origin), 32).unwrap_or_else(|| "client".into()),
        requested_model: bounded(input.requested_model),
        model_kind: bounded_str(Some(input.model_kind), 32).unwrap_or_else(|| "unknown".into()),
        provider: bounded(input.provider),
        qualified_model: bounded(input.qualified_model),
        target: bounded(input.target),
        agent_provider: bounded(input.agent_provider),
        protocol: bounded(input.protocol),
        outcome: bounded_str(Some(input.outcome), 32).unwrap_or_else(|| "unknown".into()),
        stage: bounded_str(Some(input.stage), 64).unwrap_or_else(|| "unknown".into()),
        reason_code: bounded(input.reason_code),
        input_tokens: normalized.input,
        output_tokens: normalized.output,
        cache_read_tokens: normalized.cache_read,
        cache_write_tokens: normalized.cache_write,
        usage_state: normalized.state(input.usage_state).to_owned(),
        duration_ms: input
            .duration_ms
            .map(|value| value.min(i64::MAX as u64) as i64),
        cost_micros,
        price_source,
        billing_kind: billing_kind.into(),
        cost_state: cost_state.into(),
    }
}

/// Derive the billing scope label from qualified provider configuration
/// evidence. Missing metadata proves nothing about how a provider bills, so a
/// provider without a recognized integration stays `unknown` rather than being
/// asserted as API-billed.
fn billing_kind(provider: Option<&crate::config::Provider>) -> &'static str {
    let Some(provider) = provider else {
        return "unknown";
    };
    if provider.subscription.is_some() {
        return "subscription";
    }
    match crate::provider_integration::integration_id(provider) {
        Ok("deepseek") => "api",
        Ok(
            "opencode-go"
            | "codex-subscription"
            | "kimi-code-plan-cn"
            | "kimi-code-plan-global"
            | "glm-coding-plan"
            | "stepfun-token-plan"
            | "minimax-cn-coding-plan"
            | "minimax-coding-plan",
        ) => "subscription",
        _ => "unknown",
    }
}

fn bounded(value: Option<&str>) -> Option<String> {
    bounded_str(value, 512)
}

fn bounded_str(value: Option<&str>, limit: usize) -> Option<String> {
    value.map(|value| {
        let mut value = value.to_owned();
        if value.len() > limit {
            let mut end = limit;
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            value.truncate(end);
        }
        value
    })
}

/// Normalize provider counters into exclusive buckets for one native protocol.
///
/// A counter name outside the protocol's recognized family is never used. When
/// a recognized and another protocol's value would both target one axis, that
/// axis is ambiguous and the record reports it. A record with no recognized
/// counter is unknown even if the provider sent unrelated totals.
fn normalize(protocol: Option<&str>, usage: &BTreeMap<String, u64>) -> Normalized {
    let mut out = Normalized::default();
    let recognized: &[&str] = match protocol {
        Some("chat-completions") => CHAT_COMPLETIONS,
        Some("responses") => RESPONSES,
        Some("messages") => MESSAGES,
        _ => &[],
    };
    let present = |name: &str| usage.contains_key(name);
    out.recognized = recognized.iter().any(|name| present(name));
    let foreign: Vec<&str> = usage
        .keys()
        .map(String::as_str)
        .filter(|name| ALL_COUNTER_NAMES.contains(name) && !recognized.contains(name))
        .collect();
    if recognized.is_empty() {
        out.ambiguous = !foreign.is_empty();
        return out;
    }
    if !foreign.is_empty() {
        out.ambiguous = true;
    }
    let mut counter = |name: &str| -> Option<i64> {
        let value = usage.get(name).copied()?;
        if value > MAX_COUNTER {
            out.oversized = true;
            return None;
        }
        Some(value as i64)
    };
    let mut axis = |conflict: bool, value: Option<i64>| -> Option<i64> {
        if conflict {
            out.ambiguous = true;
            return None;
        }
        value
    };
    match protocol {
        Some("chat-completions") => {
            out.input = axis(present("input_tokens"), counter("prompt_tokens"));
            out.output = axis(present("output_tokens"), counter("completion_tokens"));
            if present("cache_read_input_tokens") || present("cache_creation_input_tokens") {
                out.ambiguous = true;
            }
        }
        Some("responses") => {
            out.input = axis(present("prompt_tokens"), counter("input_tokens"));
            out.output = axis(present("completion_tokens"), counter("output_tokens"));
            if present("cache_read_input_tokens") || present("cache_creation_input_tokens") {
                out.ambiguous = true;
            }
        }
        Some("messages") => {
            out.input = axis(present("prompt_tokens"), counter("input_tokens"));
            out.output = axis(present("completion_tokens"), counter("output_tokens"));
            out.cache_read = counter("cache_read_input_tokens");
            out.cache_write = counter("cache_creation_input_tokens");
        }
        _ => unreachable!("recognized protocols are handled"),
    }
    out
}

/// Resolve the effective price for one local model.
///
/// Effective values follow the override > catalog order. Provenance is tracked
/// per axis: an override that sets only `input` must not label a catalog
/// `output` price as an override.
fn price_for(pricing: Pricing<'_>, provider_id: &str, model_id: &str) -> Option<EffectivePrice> {
    let provider = pricing.config.providers.get(provider_id)?;
    let model = provider.models.get(model_id)?;
    let effective =
        catalog::effective_from_snapshot(pricing.metadata, provider, Some((model_id, model)))
            .ok()?;
    let cost = effective.get("effective")?.get("cost")?;
    let override_cost = model.overrides.cost.as_ref();
    let axis = |name: &str| -> AxisPrice {
        let Some(value) = cost
            .get(name)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value >= 0.0)
        else {
            return AxisPrice::default();
        };
        let overridden = override_cost.is_some_and(|cost| match name {
            "input" => cost.input.is_some(),
            "output" => cost.output.is_some(),
            "cache_read" => cost.cache_read.is_some(),
            "cache_write" => cost.cache_write.is_some(),
            _ => false,
        });
        AxisPrice {
            value: Some(value),
            source: Some(if overridden { "override" } else { "catalog" }),
        }
    };
    let price = EffectivePrice {
        input: axis("input"),
        output: axis("output"),
        cache_read: axis("cache_read"),
        cache_write: axis("cache_write"),
    };
    price.any().then_some(price)
}

/// Compute the frozen micro-USD estimate for one record.
///
/// `tokens × USD-per-million-tokens` is already micro-USD, so the sum of the
/// priced axes, rounded once, is an integer micro-USD amount. The source set
/// names where the priced axes came from; unpriced axes are never counted.
fn estimate(
    normalized: &Normalized,
    captured_state: &str,
    price: Option<&EffectivePrice>,
) -> (Option<i64>, Option<String>, &'static str) {
    let known = [
        normalized.input,
        normalized.output,
        normalized.cache_read,
        normalized.cache_write,
    ]
    .into_iter()
    .flatten()
    .count();
    if known == 0 {
        return (None, None, "none");
    }
    let Some(price) = price else {
        return (None, None, "unpriced");
    };
    let mut total = 0f64;
    let mut priced = 0usize;
    let mut sources: Vec<&'static str> = Vec::new();
    for (tokens, axis) in [
        (normalized.input, price.input),
        (normalized.output, price.output),
        (normalized.cache_read, price.cache_read),
        (normalized.cache_write, price.cache_write),
    ] {
        let (Some(tokens), Some(axis_price)) = (tokens, axis.value) else {
            continue;
        };
        total += tokens as f64 * axis_price;
        priced += 1;
        if let Some(source) = axis.source
            && !sources.contains(&source)
        {
            sources.push(source);
        }
    }
    if priced == 0 {
        return (None, None, "unpriced");
    }
    if !total.is_finite() || total < 0.0 || total > i64::MAX as f64 {
        return (None, None, "unpriced");
    }
    sources.sort_unstable();
    let partial = priced < known
        || normalized.ambiguous
        || normalized.oversized
        || captured_state == "partial"
        || captured_state == "ambiguous";
    let state = if partial { "partial" } else { "priced" };
    (Some(total.round() as i64), Some(sources.join(",")), state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Provider;

    fn usage(pairs: &[(&str, u64)]) -> BTreeMap<String, u64> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).into(), *value))
            .collect()
    }

    fn input<'a>(usage: &'a BTreeMap<String, u64>, protocol: &'a str) -> CaptureInput<'a> {
        CaptureInput {
            origin: "client",
            received_at: Utc::now(),
            admitted_at: None,
            requested_model: Some("m"),
            model_kind: "concrete",
            provider: Some("p"),
            qualified_model: Some("p/m"),
            target: None,
            agent_provider: None,
            protocol: Some(protocol),
            outcome: "completed",
            stage: "upstream_response",
            reason_code: None,
            duration_ms: Some(1),
            usage,
            usage_state: "reported",
        }
    }

    fn row(usage: &BTreeMap<String, u64>, protocol: &str) -> UsageRow {
        capture(&input(usage, protocol), None)
    }

    #[test]
    fn total_only_or_foreign_counters_are_not_reported_usage() {
        let total = row(&usage(&[("total_tokens", 4)]), "chat-completions");
        assert_eq!(total.usage_state, "unknown");
        assert!(total.input_tokens.is_none() && total.output_tokens.is_none());

        let foreign = row(&usage(&[("input_tokens", 3)]), "chat-completions");
        assert_eq!(foreign.usage_state, "ambiguous");
        assert!(foreign.input_tokens.is_none());

        let mut both = usage(&[("prompt_tokens", 3), ("input_tokens", 9)]);
        both.insert("completion_tokens".into(), 1);
        let conflict = row(&both, "chat-completions");
        assert_eq!(conflict.usage_state, "ambiguous");
        assert!(conflict.input_tokens.is_none());
        assert_eq!(conflict.output_tokens, Some(1));
    }

    #[test]
    fn oversized_counters_are_dropped_and_reported_unknown_or_partial() {
        let huge_only = row(&usage(&[("prompt_tokens", u64::MAX)]), "chat-completions");
        assert_eq!(huge_only.usage_state, "unknown");
        assert!(huge_only.input_tokens.is_none());
        assert_eq!(huge_only.cost_state, "none");

        let mixed = row(
            &usage(&[("prompt_tokens", u64::MAX), ("completion_tokens", 2)]),
            "chat-completions",
        );
        assert_eq!(mixed.usage_state, "partial");
        assert!(mixed.input_tokens.is_none());
        assert_eq!(mixed.output_tokens, Some(2));
    }

    #[test]
    fn cap_is_exclusive_at_the_documented_maximum() {
        let at_cap = row(
            &usage(&[("prompt_tokens", MAX_COUNTER)]),
            "chat-completions",
        );
        assert_eq!(at_cap.input_tokens, Some(MAX_COUNTER as i64));
        let over = row(
            &usage(&[("prompt_tokens", MAX_COUNTER + 1)]),
            "chat-completions",
        );
        assert!(over.input_tokens.is_none());
    }

    #[test]
    fn messages_cache_buckets_are_exclusive_and_charged_separately() {
        let normalized = normalize(
            Some("messages"),
            &usage(&[
                ("input_tokens", 10),
                ("output_tokens", 2),
                ("cache_read_input_tokens", 5),
                ("cache_creation_input_tokens", 3),
            ]),
        );
        assert_eq!(normalized.input, Some(10));
        assert_eq!(normalized.cache_read, Some(5));
        assert_eq!(normalized.cache_write, Some(3));
        assert!(!normalized.ambiguous);
    }

    fn subscription_config() -> Config {
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            Provider {
                subscription: Some(crate::config::Subscription::default()),
                ..Default::default()
            },
        );
        config
    }

    #[test]
    fn billing_kind_is_unknown_without_pricing_and_names_the_provider_kind_with_it() {
        let usage = usage(&[("prompt_tokens", 3)]);
        let without = capture(&input(&usage, "chat-completions"), None);
        assert_eq!(without.billing_kind, "unknown");
        let mut config = Config::empty();
        config.providers.insert(
            "p".into(),
            Provider {
                integration: Some("deepseek".into()),
                ..Default::default()
            },
        );
        let row = capture(
            &input(&usage, "chat-completions"),
            Some(Pricing {
                config: &config,
                metadata: None,
            }),
        );
        assert_eq!(row.billing_kind, "api");
        let row = capture(
            &input(&usage, "chat-completions"),
            Some(Pricing {
                config: &subscription_config(),
                metadata: None,
            }),
        );
        assert_eq!(row.billing_kind, "subscription");
        let mut unknown = Config::empty();
        unknown.providers.insert("p".into(), Provider::default());
        let row = capture(
            &input(&usage, "chat-completions"),
            Some(Pricing {
                config: &unknown,
                metadata: None,
            }),
        );
        assert_eq!(row.billing_kind, "unknown");
    }

    #[test]
    fn per_axis_price_provenance_is_kept_in_the_source_set() {
        let model = crate::config::Model {
            overrides: crate::config::ModelOverrides {
                cost: Some(crate::config::Cost {
                    input: Some(3.0),
                    cache_read: Some(1.0),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut config = subscription_config();
        config
            .providers
            .get_mut("p")
            .expect("provider")
            .models
            .insert("m".into(), model);
        let usage = usage(&[("prompt_tokens", 1), ("completion_tokens", 1)]);
        let row = capture(
            &input(&usage, "chat-completions"),
            Some(Pricing {
                config: &config,
                metadata: None,
            }),
        );
        assert_eq!(row.price_source.as_deref(), Some("override"));
        assert_eq!(row.cost_micros, Some(3));
        assert_eq!(row.cost_state, "partial");
    }
}
