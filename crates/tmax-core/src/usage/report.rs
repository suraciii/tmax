//! Read-time usage reports over the ledger.
//!
//! A report is one read-only projection over the retained usage records and
//! allowance readings. Nothing derived is stored, reads never prune or rewrite
//! the ledger, and an absent, empty, or unavailable store stays distinguishable.
//! See `specs/usage-analytics/spec.md` and `specs/cli/usage/spec.md` for the
//! owned behavior and result data.
use crate::{
    Error, Result,
    storage::Store,
    usage::{Period, ReadingRow, ledger, local_day, micro_usd},
};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{Connection, params};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Maximum group rows in one breakdown result.
pub(super) const ROW_BOUND: usize = 200;
/// Maximum per-provider cost entries in one report.
pub(super) const COST_BOUND: usize = 64;
/// Maximum allowance readings in one report; the newest readings are kept.
pub(super) const ALLOWANCE_BOUND: usize = 200;
/// Documented minimum sample count for a duration percentile.
pub(super) const SAMPLE_MINIMUM: i64 = 20;
/// Maximum coverage gaps in one result; the most severe losses are kept.
pub(super) const GAP_BOUND: usize = 16;
/// Usage-record retention horizon in days. Pruning runs on writes only.
pub(super) const USAGE_RETENTION_DAYS: i64 = 90;
/// Allowance-reading retention horizon in days. Pruning runs on writes only.
pub(super) const READINGS_RETENTION_DAYS: i64 = 45;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

fn storage_unavailable() -> Error {
    Error::new("storage_unavailable", "The usage ledger is unavailable")
}

fn query<T>(result: rusqlite::Result<T>) -> Result<T> {
    result.map_err(|_| storage_unavailable())
}

fn utc(ms: i64) -> Option<DateTime<Utc>> {
    DateTime::<Utc>::from_timestamp_millis(ms)
}

fn instant(ms: i64) -> Value {
    match utc(ms) {
        Some(at) => Value::String(at.to_rfc3339_opts(SecondsFormat::Millis, true)),
        None => Value::Null,
    }
}

fn optional_instant(ms: Option<i64>) -> Value {
    ms.map(instant).unwrap_or(Value::Null)
}

fn instant_at(at: DateTime<Utc>) -> Value {
    instant(at.timestamp_millis())
}

fn nonzero(value: i64) -> Option<i64> {
    (value > 0).then_some(value)
}

/// The coverage contract shared by every report result.
pub(super) fn scope() -> Value {
    json!({
        "source": "gateway_observed",
        "direct_traffic": "unobserved",
        "probes": "separate",
        "diagnostic_history": "excluded",
    })
}

/// Disclosures required on every report and insight result.
pub(super) fn disclosures() -> Value {
    json!({
        "token_comparability": "Token buckets are protocol-scoped and exclusive; raw bucket totals are not comparable across protocols or providers.",
        "cached_input": "Cache-read and cache-write tokens are separate priced buckets and are never added to input; an input cost estimate can overstate when a provider counts cached prompt tokens inside its input counter.",
        "cost_basis": "Cost estimates use effective reference prices frozen when each record was written; subscription-backed records are labeled, and their estimate is API-equivalent rather than a billed amount.",
        "probes": "Probe tokens are included in token totals but excluded from request, outcome, and duration counts.",
        "excluded": ["provider_reported_total_tokens", "foreign_counter_names"],
    })
}

/// One resolved report period, as the result reports it.
#[derive(Debug, Clone)]
pub(super) struct PeriodView {
    pub kind: String,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub timezone: String,
}

impl PeriodView {
    fn to_json(&self) -> Value {
        json!({
            "kind": self.kind,
            "since": self.since.map(instant_at).unwrap_or(Value::Null),
            "until": self.until.map(instant_at).unwrap_or(Value::Null),
            "timezone": self.timezone,
        })
    }
}

/// One half-open query window in epoch milliseconds. [`Coverage::reportable`]
/// reuses it for the inclusive intersection of the requested period with the
/// retained span.
#[derive(Debug, Clone, Copy)]
pub(super) struct Span {
    pub since_ms: i64,
    pub until_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoverageState {
    Available,
    Absent,
    Unavailable,
}

impl CoverageState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Absent => "absent",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Counters {
    oldest_ms: Option<i64>,
    newest_ms: Option<i64>,
    retained_records: i64,
    evicted: i64,
    dropped: i64,
    cleared_at_ms: Option<i64>,
}

impl Counters {
    fn json(&self) -> Value {
        json!({
            "oldest": optional_instant(self.oldest_ms),
            "newest": optional_instant(self.newest_ms),
            "retained_records": self.retained_records,
            "evicted": self.evicted,
            "dropped": self.dropped,
            "cleared_at": optional_instant(self.cleared_at_ms),
        })
    }

    fn null_json() -> Value {
        json!({
            "oldest": Value::Null,
            "newest": Value::Null,
            "retained_records": Value::Null,
            "evicted": Value::Null,
            "dropped": Value::Null,
            "cleared_at": Value::Null,
        })
    }
}

/// One typed coverage loss or limitation; its JSON shape is part of the CLI
/// result contract.
#[derive(Debug, Clone, Copy)]
pub(super) enum Gap {
    Cleared {
        at_ms: i64,
    },
    Evicted {
        count: i64,
    },
    Dropped {
        count: i64,
    },
    Restart,
    OutOfCoverage {
        from_ms: Option<i64>,
        to_ms: Option<i64>,
    },
}

impl Gap {
    /// Severity rank; a broader loss sorts first.
    fn rank(self) -> u8 {
        match self {
            Self::Cleared { .. } => 0,
            Self::Evicted { .. } => 1,
            Self::Dropped { .. } => 2,
            Self::Restart => 3,
            Self::OutOfCoverage { .. } => 4,
        }
    }

    /// Newest-first ordering inside one rank.
    fn recency(self) -> i64 {
        match self {
            Self::Cleared { at_ms } => at_ms,
            Self::OutOfCoverage { to_ms, .. } => to_ms.unwrap_or(i64::MAX),
            Self::Evicted { .. } | Self::Dropped { .. } | Self::Restart => i64::MAX,
        }
    }

    fn json(self) -> Value {
        match self {
            Self::Cleared { at_ms } => json!({"kind": "cleared", "at": instant(at_ms)}),
            Self::Evicted { count } => json!({"kind": "evicted", "count": count}),
            Self::Dropped { count } => json!({"kind": "dropped", "count": count}),
            Self::Restart => json!({"kind": "restart", "loss": "unknown"}),
            Self::OutOfCoverage { from_ms, to_ms } => json!({
                "kind": "out_of_coverage",
                "from": optional_instant(from_ms),
                "to": optional_instant(to_ms),
            }),
        }
    }

    /// True when the loss may fall inside `[since_ms, until_ms]`. A loss with no
    /// recorded interval (dropped, evicted, restart) could have occurred anywhere
    /// in the epoch, so it always counts.
    pub(super) fn intersects(self, since_ms: i64, until_ms: i64) -> bool {
        match self {
            Self::Cleared { at_ms } => at_ms > since_ms,
            Self::Evicted { .. } | Self::Dropped { .. } | Self::Restart => true,
            Self::OutOfCoverage { from_ms, to_ms } => {
                from_ms.unwrap_or(i64::MIN) < until_ms && to_ms.unwrap_or(i64::MAX) > since_ms
            }
        }
    }
}

fn gaps_json(mut gaps: Vec<Gap>) -> Value {
    gaps.sort_unstable_by_key(|gap| (gap.rank(), std::cmp::Reverse(gap.recency())));
    gaps.truncate(GAP_BOUND);
    Value::Array(gaps.into_iter().map(Gap::json).collect())
}

/// Retention horizon disclosure shared by every report and insight result.
#[derive(Debug, Clone, Copy)]
struct Retention {
    usage_days: i64,
    readings_days: i64,
    usage_overdue: Option<bool>,
    readings_overdue: Option<bool>,
}

impl Retention {
    fn json(self) -> Value {
        json!({
            "usage_days": self.usage_days,
            "readings_days": self.readings_days,
            "enforcement": "on_write",
            "usage_overdue": self.usage_overdue.map(Value::Bool).unwrap_or(Value::Null),
            "readings_overdue": self
                .readings_overdue
                .map(Value::Bool)
                .unwrap_or(Value::Null),
        })
    }

    /// Each horizon reports its own delayed enforcement: pruning runs only on a
    /// write or explicit clear, so a read can observe an oldest retained row
    /// that is already past the documented age.
    fn observed(ledger: &Counters, readings: &Counters, now_ms: i64) -> Self {
        Self {
            usage_days: USAGE_RETENTION_DAYS,
            readings_days: READINGS_RETENTION_DAYS,
            usage_overdue: Some(
                ledger
                    .oldest_ms
                    .is_some_and(|oldest| oldest < now_ms - USAGE_RETENTION_DAYS * DAY_MS),
            ),
            readings_overdue: Some(
                readings
                    .oldest_ms
                    .is_some_and(|oldest| oldest < now_ms - READINGS_RETENTION_DAYS * DAY_MS),
            ),
        }
    }

    fn unknown() -> Self {
        Self {
            usage_days: USAGE_RETENTION_DAYS,
            readings_days: READINGS_RETENTION_DAYS,
            usage_overdue: None,
            readings_overdue: None,
        }
    }
}

fn span_json(span: Option<Span>) -> Value {
    match span {
        Some(span) => json!({
            "since": instant(span.since_ms),
            "until": instant(span.until_ms),
        }),
        None => Value::Null,
    }
}

/// `reportable` is the inclusive intersection of the requested period with the
/// retained span. A requested bound outside the covered epoch produces an
/// `out_of_coverage` gap; the live right edge is the read instant, so a period
/// that merely has no recent records is not a gap.
fn coverage_bounds(
    counters: Counters,
    requested: Option<(Option<i64>, Option<i64>)>,
    now_ms: i64,
    gaps: &mut Vec<Gap>,
) -> Option<Span> {
    let (requested_since, requested_until) = requested?;
    let (Some(requested_since), Some(requested_until)) = (requested_since, requested_until) else {
        return None;
    };
    let (Some(oldest), Some(newest)) = (counters.oldest_ms, counters.newest_ms) else {
        return None;
    };
    if requested_since < oldest {
        let to = requested_until.min(oldest);
        if to > requested_since {
            gaps.push(Gap::OutOfCoverage {
                from_ms: Some(requested_since),
                to_ms: Some(to),
            });
        }
    }
    if requested_until > now_ms {
        let from = requested_since.max(now_ms);
        if requested_until > from {
            gaps.push(Gap::OutOfCoverage {
                from_ms: Some(from),
                to_ms: Some(requested_until),
            });
        }
    }
    let since = requested_since.max(oldest);
    let until = requested_until.min(newest);
    if since < until || (since == until && until == newest && newest < requested_until) {
        Some(Span {
            since_ms: since,
            until_ms: until,
        })
    } else {
        None
    }
}

/// Ledger and allowance-reading coverage, reported once per result.
#[derive(Debug, Clone)]
pub(super) struct Coverage {
    state: CoverageState,
    reason_code: Option<String>,
    ledger: Option<Counters>,
    readings: Option<Counters>,
    reportable: Option<Span>,
    readings_reportable: Option<Span>,
    gaps: Vec<Gap>,
    readings_gaps: Vec<Gap>,
    retention: Retention,
    /// Instant of the read that produced this coverage.
    read_at_ms: Option<i64>,
}

impl Coverage {
    fn resolved(
        ledger: Counters,
        readings: Counters,
        restart_gap: bool,
        requested: Option<(Option<i64>, Option<i64>)>,
        now_ms: i64,
    ) -> Self {
        let mut gaps = Vec::new();
        if let Some(at_ms) = ledger.cleared_at_ms {
            gaps.push(Gap::Cleared { at_ms });
        }
        if ledger.evicted > 0 {
            gaps.push(Gap::Evicted {
                count: ledger.evicted,
            });
        }
        if ledger.dropped > 0 {
            gaps.push(Gap::Dropped {
                count: ledger.dropped,
            });
        }
        if restart_gap {
            gaps.push(Gap::Restart);
        }
        let reportable = coverage_bounds(ledger, requested, now_ms, &mut gaps);
        let mut readings_gaps = Vec::new();
        if readings.evicted > 0 {
            readings_gaps.push(Gap::Evicted {
                count: readings.evicted,
            });
        }
        if readings.dropped > 0 {
            readings_gaps.push(Gap::Dropped {
                count: readings.dropped,
            });
        }
        let readings_reportable = coverage_bounds(readings, requested, now_ms, &mut readings_gaps);
        Self {
            state: CoverageState::Available,
            reason_code: None,
            ledger: Some(ledger),
            readings: Some(readings),
            reportable,
            readings_reportable,
            gaps,
            readings_gaps,
            retention: Retention::observed(&ledger, &readings, now_ms),
            read_at_ms: Some(now_ms),
        }
    }

    pub(super) fn absent() -> Self {
        Self {
            state: CoverageState::Absent,
            reason_code: None,
            ledger: Some(Counters::default()),
            readings: Some(Counters::default()),
            reportable: None,
            readings_reportable: None,
            gaps: Vec::new(),
            readings_gaps: Vec::new(),
            retention: Retention {
                usage_days: USAGE_RETENTION_DAYS,
                readings_days: READINGS_RETENTION_DAYS,
                usage_overdue: Some(false),
                readings_overdue: Some(false),
            },
            read_at_ms: None,
        }
    }

    pub(super) fn unavailable(reason_code: &str) -> Self {
        Self {
            state: CoverageState::Unavailable,
            reason_code: Some(reason_code.to_owned()),
            ledger: None,
            readings: None,
            reportable: None,
            readings_reportable: None,
            gaps: Vec::new(),
            readings_gaps: Vec::new(),
            retention: Retention::unknown(),
            read_at_ms: None,
        }
    }

    pub(super) fn is_available(&self) -> bool {
        self.state == CoverageState::Available
    }

    pub(super) fn is_absent(&self) -> bool {
        self.state == CoverageState::Absent
    }

    pub(super) fn oldest_ms(&self) -> Option<i64> {
        self.ledger.and_then(|ledger| ledger.oldest_ms)
    }

    /// Instant of the read that produced this coverage, if it was available.
    pub(super) fn read_at_ms(&self) -> Option<i64> {
        self.read_at_ms
    }

    /// True when a known ledger or reading coverage gap may fall inside
    /// `[since_ms, until_ms]`.
    pub(super) fn has_gap_within(&self, since_ms: i64, until_ms: i64) -> bool {
        self.gaps
            .iter()
            .chain(self.readings_gaps.iter())
            .any(|gap| gap.intersects(since_ms, until_ms))
    }

    fn counters_json(counters: Option<&Counters>) -> Value {
        counters
            .map(Counters::json)
            .unwrap_or_else(Counters::null_json)
    }

    fn to_json(&self) -> Value {
        let ledger = Self::counters_json(self.ledger.as_ref());
        let readings = match Self::counters_json(self.readings.as_ref()) {
            Value::Object(mut map) => {
                map.insert("state".into(), json!(self.state.as_str()));
                map.insert(
                    "reason_code".into(),
                    self.reason_code
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                );
                map.insert("reportable".into(), span_json(self.readings_reportable));
                map.insert("gaps".into(), gaps_json(self.readings_gaps.clone()));
                Value::Object(map)
            }
            other => other,
        };
        json!({
            "state": self.state.as_str(),
            "reason_code": &self.reason_code,
            "oldest": ledger["oldest"].clone(),
            "newest": ledger["newest"].clone(),
            "retained_records": ledger["retained_records"].clone(),
            "evicted": ledger["evicted"].clone(),
            "dropped": ledger["dropped"].clone(),
            "cleared_at": ledger["cleared_at"].clone(),
            "reportable": span_json(self.reportable),
            "gaps": gaps_json(self.gaps.clone()),
            "retention": self.retention.json(),
            "readings": readings,
        })
    }

    /// Compact coverage evidence for an insight's `evidence` object.
    pub(super) fn evidence_json(&self) -> Value {
        let (oldest, newest, retained) = match self.ledger {
            Some(ledger) => (
                optional_instant(ledger.oldest_ms),
                optional_instant(ledger.newest_ms),
                json!(ledger.retained_records),
            ),
            None => (Value::Null, Value::Null, Value::Null),
        };
        let readings = match self.readings {
            Some(readings) => json!({
                "oldest": optional_instant(readings.oldest_ms),
                "newest": optional_instant(readings.newest_ms),
                "retained_records": readings.retained_records,
                "reportable": span_json(self.readings_reportable),
                "gaps": gaps_json(self.readings_gaps.clone()),
            }),
            None => json!({
                "oldest": Value::Null,
                "newest": Value::Null,
                "retained_records": Value::Null,
                "reportable": Value::Null,
                "gaps": Value::Array(Vec::new()),
            }),
        };
        json!({
            "state": self.state.as_str(),
            "oldest": oldest,
            "newest": newest,
            "retained_records": retained,
            "reportable": span_json(self.reportable),
            "gaps": gaps_json(self.gaps.clone()),
            "retention": self.retention.json(),
            "readings": readings,
        })
    }
}

/// One consistency snapshot for a report or insight read.
pub(super) struct Snapshot {
    pub coverage: Coverage,
    pub period: PeriodView,
    conn: Option<Connection>,
    usage_span: Option<Span>,
    reading_span: Option<Span>,
}

impl Snapshot {
    /// Open one explicit read transaction and read the coverage inside it. Every
    /// later aggregate for the result reads the same committed snapshot: a writer
    /// that commits after this call is invisible until the next report.
    pub(super) fn load(store: &Store, period: &Period) -> Self {
        let now = Utc::now();
        let now_ms = now.timestamp_millis();
        match ledger::open_read(store) {
            Ok(None) => Self::absent(period),
            Err(error) => Self::unavailable(period, &error.code),
            Ok(Some(conn)) => {
                // A deferred read-only transaction takes its snapshot at the
                // first read and never mutates the store.
                if conn.execute_batch("BEGIN DEFERRED").is_err() {
                    return Self::unavailable(period, "storage_unavailable");
                }
                match raw_coverage(&conn) {
                    Ok(raw) => {
                        let requested = requested_bounds(period, &raw, now_ms);
                        let coverage = raw.resolve(requested, now_ms);
                        let (period, usage_span, reading_span) =
                            resolve_bounds(period, &coverage, now);
                        Self {
                            coverage,
                            period,
                            conn: Some(conn),
                            usage_span,
                            reading_span,
                        }
                    }
                    Err(error) => {
                        let _ = conn.execute_batch("ROLLBACK");
                        Self::unavailable(period, &error.code)
                    }
                }
            }
        }
    }

    fn absent(period: &Period) -> Self {
        let bounds = if period.kind == "all" {
            (None, None)
        } else {
            (period.since, period.until)
        };
        Self {
            coverage: Coverage::absent(),
            period: PeriodView {
                kind: period.kind.clone(),
                since: bounds.0,
                until: bounds.1,
                timezone: period.timezone.clone(),
            },
            conn: None,
            usage_span: None,
            reading_span: None,
        }
    }

    pub(super) fn unavailable(period: &Period, reason_code: &str) -> Self {
        Self {
            coverage: Coverage::unavailable(reason_code),
            period: PeriodView {
                kind: period.kind.clone(),
                since: None,
                until: None,
                timezone: period.timezone.clone(),
            },
            conn: None,
            usage_span: None,
            reading_span: None,
        }
    }

    pub(super) fn conn(&self) -> Option<&Connection> {
        self.conn.as_ref()
    }

    pub(super) fn usage_span(&self) -> Option<Span> {
        self.usage_span
    }

    pub(super) fn reading_span(&self) -> Option<Span> {
        self.reading_span
    }

    pub(super) fn period_json(&self) -> Value {
        self.period.to_json()
    }

    pub(super) fn coverage_json(&self) -> Value {
        self.coverage.to_json()
    }

    fn base(&self) -> Map<String, Value> {
        let mut data = Map::new();
        data.insert("period".into(), self.period_json());
        data.insert("scope".into(), scope());
        data.insert("coverage".into(), self.coverage.to_json());
        data.insert("disclosures".into(), disclosures());
        data
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.as_ref() {
            // The snapshot is read-only; release its explicit read transaction.
            let _ = conn.execute_batch("ROLLBACK");
        }
    }
}

/// The requested period as epoch milliseconds. `all` requests the retained
/// ledger range; unavailable bounds leave reportability unknown.
fn requested_bounds(
    period: &Period,
    raw: &RawCoverage,
    now_ms: i64,
) -> Option<(Option<i64>, Option<i64>)> {
    if period.kind == "all" {
        return Some((raw.ledger.oldest_ms, Some(now_ms)));
    }
    Some((
        period.since.map(|at| at.timestamp_millis()),
        period.until.map(|at| at.timestamp_millis()),
    ))
}

fn resolve_bounds(
    period: &Period,
    coverage: &Coverage,
    now: DateTime<Utc>,
) -> (PeriodView, Option<Span>, Option<Span>) {
    let now_ms = now.timestamp_millis();
    if period.kind == "all" {
        let oldest = coverage.ledger.and_then(|counters| counters.oldest_ms);
        let view = PeriodView {
            kind: period.kind.clone(),
            since: oldest.and_then(utc),
            until: oldest.map(|_| now),
            timezone: period.timezone.clone(),
        };
        let usage = oldest.map(|since_ms| Span {
            since_ms,
            until_ms: now_ms,
        });
        let reading = Span {
            since_ms: oldest.unwrap_or(i64::MIN),
            until_ms: now_ms,
        };
        (view, usage, Some(reading))
    } else {
        let usage = match (period.since, period.until) {
            (Some(since), Some(until)) => Some(Span {
                since_ms: since.timestamp_millis(),
                until_ms: until.timestamp_millis(),
            }),
            _ => None,
        };
        let view = PeriodView {
            kind: period.kind.clone(),
            since: period.since,
            until: period.until,
            timezone: period.timezone.clone(),
        };
        (view, usage, usage)
    }
}

fn read_counters(conn: &Connection, sql: &str) -> Result<(Option<i64>, Option<i64>, i64)> {
    query(conn.query_row(sql, [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))))
}

/// Epoch counters and retained ranges, read before reportability is resolved.
struct RawCoverage {
    ledger: Counters,
    readings: Counters,
    restart_gap: bool,
}

impl RawCoverage {
    fn resolve(self, requested: Option<(Option<i64>, Option<i64>)>, now_ms: i64) -> Coverage {
        Coverage::resolved(
            self.ledger,
            self.readings,
            self.restart_gap,
            requested,
            now_ms,
        )
    }
}

fn raw_coverage(conn: &Connection) -> Result<RawCoverage> {
    let (ledger_oldest, ledger_newest, ledger_retained) = read_counters(
        conn,
        "SELECT MIN(membership_ms), MAX(membership_ms), COUNT(*) FROM usage_record",
    )?;
    let (readings_oldest, readings_newest, readings_retained) = read_counters(
        conn,
        "SELECT MIN(observed_at_ms), MAX(observed_at_ms), COUNT(*) FROM allowance_reading",
    )?;
    let epoch = ledger::epoch(conn);
    let cleared_at = nonzero(epoch.cleared_at_ms);
    let ledger = Counters {
        oldest_ms: ledger_oldest,
        newest_ms: ledger_newest,
        retained_records: ledger_retained,
        evicted: epoch.evicted,
        dropped: epoch.dropped,
        cleared_at_ms: cleared_at,
    };
    let readings = Counters {
        oldest_ms: readings_oldest,
        newest_ms: readings_newest,
        retained_records: readings_retained,
        evicted: epoch.readings_evicted,
        dropped: epoch.readings_dropped,
        cleared_at_ms: cleared_at,
    };
    Ok(RawCoverage {
        ledger,
        readings,
        restart_gap: epoch.restart_gap,
    })
}

fn coverage_of(conn: &Connection) -> Result<Coverage> {
    let raw = raw_coverage(conn)?;
    Ok(raw.resolve(None, Utc::now().timestamp_millis()))
}

/// One duration projection with its sample count and availability state.
#[derive(Debug, Clone, Copy)]
pub(super) struct Duration {
    pub p50_ms: Option<i64>,
    pub p90_ms: Option<i64>,
    pub samples: i64,
    pub state: &'static str,
}

impl Duration {
    pub(super) fn empty() -> Self {
        Self {
            p50_ms: None,
            p90_ms: None,
            samples: 0,
            state: "no_records",
        }
    }

    fn to_json(self) -> Value {
        json!({
            "p50_ms": self.p50_ms,
            "p90_ms": self.p90_ms,
            "samples": self.samples,
            "sample_minimum": SAMPLE_MINIMUM,
            "state": self.state,
        })
    }
}

/// Nearest-rank percentile over an ascending slice; the caller checks the sample
/// minimum before reporting a value.
fn nearest_rank(sorted: &[i64], percent: usize) -> i64 {
    let rank = ((percent * sorted.len()).div_ceil(100)).max(1) - 1;
    sorted[rank.min(sorted.len() - 1)]
}

pub(super) fn duration_from_samples(mut samples: Vec<i64>) -> Duration {
    samples.sort_unstable();
    let count = samples.len() as i64;
    if count == 0 {
        return Duration::empty();
    }
    if count < SAMPLE_MINIMUM {
        return Duration {
            p50_ms: None,
            p90_ms: None,
            samples: count,
            state: "low_samples",
        };
    }
    Duration {
        p50_ms: Some(nearest_rank(&samples, 50)),
        p90_ms: Some(nearest_rank(&samples, 90)),
        samples: count,
        state: "available",
    }
}

/// One aggregate row: client request counts, probe counts, token sums, and the
/// per-record usage states. Token sums cover every retained record in scope,
/// while request/outcome/duration aggregates cover client traffic only.
struct Aggregate {
    total: i64,
    requests: i64,
    completed: i64,
    failed: i64,
    rejected: i64,
    cancelled: i64,
    interrupted: i64,
    outcome_unknown: i64,
    probes: i64,
    input_sum: Option<i64>,
    input_known: i64,
    output_sum: Option<i64>,
    output_known: i64,
    cache_read_sum: Option<i64>,
    cache_read_known: i64,
    cache_write_sum: Option<i64>,
    cache_write_known: i64,
    usage_reported: i64,
    usage_partial: i64,
    usage_unknown: i64,
    usage_ambiguous: i64,
    admitted: i64,
}

/// Column list shared by the totals and breakdown aggregates. Order is fixed and
/// mirrored by [`Aggregate::read`].
const AGGREGATES: &str = "COUNT(*),
    COALESCE(SUM(CASE WHEN origin <> 'probe' THEN 1 ELSE 0 END),0),
    COALESCE(SUM(CASE WHEN origin <> 'probe' AND outcome = 'completed' THEN 1 ELSE 0 END),0),
    COALESCE(SUM(CASE WHEN origin <> 'probe' AND outcome = 'failed' THEN 1 ELSE 0 END),0),
    COALESCE(SUM(CASE WHEN origin <> 'probe' AND outcome = 'rejected' THEN 1 ELSE 0 END),0),
    COALESCE(SUM(CASE WHEN origin <> 'probe' AND outcome = 'cancelled' THEN 1 ELSE 0 END),0),
    COALESCE(SUM(CASE WHEN origin <> 'probe' AND outcome = 'interrupted' THEN 1 ELSE 0 END),0),
    COALESCE(SUM(CASE WHEN origin <> 'probe' AND outcome = 'unknown' THEN 1 ELSE 0 END),0),
    COALESCE(SUM(CASE WHEN origin = 'probe' THEN 1 ELSE 0 END),0),
    SUM(input_tokens), COUNT(input_tokens),
    SUM(output_tokens), COUNT(output_tokens),
    SUM(cache_read_tokens), COUNT(cache_read_tokens),
    SUM(cache_write_tokens), COUNT(cache_write_tokens),
    COALESCE(SUM(CASE WHEN usage_state = 'reported' THEN 1 ELSE 0 END),0),
    COALESCE(SUM(CASE WHEN usage_state = 'partial' THEN 1 ELSE 0 END),0),
    COALESCE(SUM(CASE WHEN usage_state = 'unknown' THEN 1 ELSE 0 END),0),
    COALESCE(SUM(CASE WHEN usage_state = 'ambiguous' THEN 1 ELSE 0 END),0),
    COUNT(admitted_at_ms)";

impl Aggregate {
    fn read(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            total: row.get(offset)?,
            requests: row.get(offset + 1)?,
            completed: row.get(offset + 2)?,
            failed: row.get(offset + 3)?,
            rejected: row.get(offset + 4)?,
            cancelled: row.get(offset + 5)?,
            interrupted: row.get(offset + 6)?,
            outcome_unknown: row.get(offset + 7)?,
            probes: row.get(offset + 8)?,
            input_sum: row.get(offset + 9)?,
            input_known: row.get(offset + 10)?,
            output_sum: row.get(offset + 11)?,
            output_known: row.get(offset + 12)?,
            cache_read_sum: row.get(offset + 13)?,
            cache_read_known: row.get(offset + 14)?,
            cache_write_sum: row.get(offset + 15)?,
            cache_write_known: row.get(offset + 16)?,
            usage_reported: row.get(offset + 17)?,
            usage_partial: row.get(offset + 18)?,
            usage_unknown: row.get(offset + 19)?,
            usage_ambiguous: row.get(offset + 20)?,
            admitted: row.get(offset + 21)?,
        })
    }

    fn outcomes_json(&self) -> Value {
        json!({
            "completed": self.completed,
            "failed": self.failed,
            "rejected": self.rejected,
            "cancelled": self.cancelled,
            "interrupted": self.interrupted,
            "unknown": self.outcome_unknown,
        })
    }

    fn usage_json(&self) -> Value {
        json!({
            "input_tokens": known_sum(self.input_sum, self.input_known),
            "output_tokens": known_sum(self.output_sum, self.output_known),
            "cache_read_tokens": known_sum(self.cache_read_sum, self.cache_read_known),
            "cache_write_tokens": known_sum(self.cache_write_sum, self.cache_write_known),
        })
    }

    fn usage_states_json(&self) -> Value {
        json!({
            "reported": self.usage_reported,
            "partial": self.usage_partial,
            "unknown": self.usage_unknown,
            "ambiguous": self.usage_ambiguous,
        })
    }

    fn membership_basis(&self) -> &'static str {
        if self.total == 0 || self.admitted == self.total {
            "admission"
        } else if self.admitted == 0 {
            "receipt"
        } else {
            "mixed"
        }
    }
}

/// Keep a known token sum exact and distinct from an unknown value.
///
/// Capture caps each counter at 10^12 and the ledger caps retained rows at
/// 200,000, so an integer `SUM` fits i64 exactly and is never rounded.
fn known_sum(sum: Option<i64>, known: i64) -> Value {
    if known == 0 {
        Value::Null
    } else {
        json!(sum.unwrap_or(0))
    }
}

/// Format an exact micro-USD amount as a decimal string with six places.
///
/// Estimates aggregate in i128 so a sum of 200,000 rows cannot round through
/// floating point; the rendered string is exact.
pub(super) fn micro_usd_wide(micros: i128) -> String {
    if let Ok(value) = i64::try_from(micros) {
        return micro_usd(value);
    }
    let negative = micros < 0;
    let value = micros.unsigned_abs();
    let whole = value / 1_000_000;
    let fraction = value % 1_000_000;
    format!(
        "{}{}.{:06}",
        if negative { "-" } else { "" },
        whole,
        fraction
    )
}

pub(super) fn duration_samples(conn: &Connection, span: Span) -> Result<Vec<i64>> {
    let mut statement = query(conn.prepare(
        "SELECT duration_ms FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2
           AND origin <> 'probe' AND outcome = 'completed' AND duration_ms IS NOT NULL
         ORDER BY duration_ms",
    ))?;
    let samples =
        query(statement.query_map(params![span.since_ms, span.until_ms], |row| row.get(0)))?;
    let mut out = Vec::new();
    for sample in samples {
        out.push(query(sample)?);
    }
    Ok(out)
}

/// Totals for one report scope.
pub(super) struct Totals {
    aggregate: Aggregate,
    duration: Duration,
}

impl Totals {
    pub(super) fn empty() -> Self {
        Self {
            aggregate: Aggregate {
                total: 0,
                requests: 0,
                completed: 0,
                failed: 0,
                rejected: 0,
                cancelled: 0,
                interrupted: 0,
                outcome_unknown: 0,
                probes: 0,
                input_sum: None,
                input_known: 0,
                output_sum: None,
                output_known: 0,
                cache_read_sum: None,
                cache_read_known: 0,
                cache_write_sum: None,
                cache_write_known: 0,
                usage_reported: 0,
                usage_partial: 0,
                usage_unknown: 0,
                usage_ambiguous: 0,
                admitted: 0,
            },
            duration: Duration::empty(),
        }
    }

    pub(super) fn load(conn: &Connection, span: Span) -> Result<Self> {
        let sql = format!(
            "SELECT {AGGREGATES} FROM usage_record
             WHERE membership_ms >= ?1 AND membership_ms < ?2"
        );
        let aggregate = query(conn.query_row(
            &sql,
            params![span.since_ms, span.until_ms],
            |row| Aggregate::read(row, 0),
        ))?;
        let duration = duration_from_samples(duration_samples(conn, span)?);
        Ok(Self {
            aggregate,
            duration,
        })
    }

    pub(super) fn requests(&self) -> i64 {
        self.aggregate.requests
    }

    pub(super) fn duration(&self) -> Duration {
        self.duration
    }

    fn to_json(&self) -> Value {
        json!({
            "requests": self.aggregate.requests,
            "outcomes": self.aggregate.outcomes_json(),
            "probes": self.aggregate.probes,
            "usage": self.aggregate.usage_json(),
            "usage_states": self.aggregate.usage_states_json(),
            "duration": self.duration.to_json(),
            "membership_basis": self.aggregate.membership_basis(),
        })
    }
}

/// Per-provider cost aggregates; the internal record count is not serialized.
#[derive(Debug, Clone, Default)]
pub(super) struct Cost {
    pub provider: Option<String>,
    records: i64,
    pub priced: i64,
    pub partially_priced: i64,
    pub unpriced: i64,
    /// Exact i128 micro-USD total for the single billing scope, if one applies.
    estimate_micros: Option<i128>,
    estimated_records: i64,
    sources: Vec<String>,
    billing_kinds: Vec<String>,
}

impl Cost {
    pub(super) fn records(&self) -> i64 {
        self.records
    }

    pub(super) fn estimate_micros(&self) -> i128 {
        self.estimate_micros.unwrap_or(0)
    }

    /// More than one billing kind cannot be added into one money estimate.
    fn mixed_billing(&self) -> bool {
        self.billing_kinds.len() > 1
    }

    /// Only a subscription-only estimate is API-equivalent. An API or unknown
    /// scope is not, and a mixed scope has no estimate at all.
    fn api_equivalent(&self) -> bool {
        matches!(self.billing_kinds.as_slice(), [kind] if kind == "subscription")
    }

    fn has_estimate(&self) -> bool {
        !self.mixed_billing() && self.estimated_records > 0 && self.estimate_micros.is_some()
    }

    fn estimate_reason(&self) -> Value {
        if self.mixed_billing() {
            return json!("mixed_billing_scope");
        }
        if self.has_estimate() {
            return Value::Null;
        }
        if self.unpriced > 0 {
            json!("unpriced")
        } else {
            json!("no_priced_axes")
        }
    }

    /// Drop a cross-scope estimate; the counts and billing kinds stay reported.
    fn seal_billing_scope(&mut self) {
        if self.mixed_billing() {
            self.estimate_micros = None;
        }
    }

    fn to_json(&self) -> Value {
        let has_estimate = self.has_estimate();
        json!({
            "provider": self.provider,
            "currency": if has_estimate { json!("USD") } else { Value::Null },
            "estimate": if has_estimate {
                json!(micro_usd_wide(self.estimate_micros.unwrap_or(0)))
            } else {
                Value::Null
            },
            "estimate_reason": self.estimate_reason(),
            "priced": self.priced,
            "partially_priced": self.partially_priced,
            "unpriced": self.unpriced,
            "sources": self.sources,
            "billing_kinds": self.billing_kinds,
            "api_equivalent": self.api_equivalent(),
            "unpriceable_axes": ["reasoning", "cache_storage"],
        })
    }
}

/// Cost count columns; order is fixed and mirrored by the reader below. Money
/// is summed exactly in Rust (i128), never by SQLite `SUM`/`TOTAL`.
const COST_AGGREGATES: &str = "COUNT(*),
    COALESCE(SUM(CASE WHEN cost_state = 'priced' THEN 1 ELSE 0 END),0),
    COALESCE(SUM(CASE WHEN cost_state = 'partial' THEN 1 ELSE 0 END),0),
    COALESCE(SUM(CASE WHEN cost_state = 'unpriced' THEN 1 ELSE 0 END),0),
    COUNT(cost_micros)";

fn read_cost(
    provider: Option<String>,
    row: &rusqlite::Row<'_>,
    offset: usize,
) -> rusqlite::Result<Cost> {
    Ok(Cost {
        provider,
        records: row.get(offset)?,
        priced: row.get(offset + 1)?,
        partially_priced: row.get(offset + 2)?,
        unpriced: row.get(offset + 3)?,
        estimate_micros: None,
        estimated_records: row.get(offset + 4)?,
        sources: Vec::new(),
        billing_kinds: Vec::new(),
    })
}

/// Accumulate exact micro-USD in i128. The saturating add is only a guard: the
/// retention caps bound a sum far below the i128 range.
fn add_exact(total: &mut i128, micros: i64) {
    *total = total.saturating_add(i128::from(micros));
}

fn provider_estimates(conn: &Connection, span: Span) -> Result<BTreeMap<String, i128>> {
    let mut statement = query(conn.prepare(
        "SELECT provider, cost_micros FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2
           AND provider IS NOT NULL AND cost_micros IS NOT NULL",
    ))?;
    let rows = query(
        statement.query_map(params![span.since_ms, span.until_ms], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }),
    )?;
    let mut totals: BTreeMap<String, i128> = BTreeMap::new();
    for row in rows {
        let (provider, micros) = query(row)?;
        add_exact(totals.entry(provider).or_default(), micros);
    }
    Ok(totals)
}

fn model_estimates(conn: &Connection, span: Span) -> Result<BTreeMap<(String, String), i128>> {
    let mut statement = query(conn.prepare(
        "SELECT qualified_model, provider, cost_micros FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2
           AND qualified_model IS NOT NULL AND provider IS NOT NULL
           AND cost_micros IS NOT NULL",
    ))?;
    let rows = query(
        statement.query_map(params![span.since_ms, span.until_ms], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        }),
    )?;
    let mut totals: BTreeMap<(String, String), i128> = BTreeMap::new();
    for row in rows {
        let (model, provider, micros) = query(row)?;
        add_exact(totals.entry((model, provider)).or_default(), micros);
    }
    Ok(totals)
}

/// Split one stored comma-separated price-source set into the reported source
/// set. Each record carries only the sources of the axes it priced.
fn add_source_set(set: &mut BTreeSet<String>, price_source: &str) {
    for source in price_source.split(',') {
        let source = source.trim();
        if !source.is_empty() {
            set.insert(source.to_owned());
        }
    }
}

fn sorted_sets(sets: BTreeMap<String, BTreeSet<String>>) -> BTreeMap<String, Vec<String>> {
    sets.into_iter()
        .map(|(key, values)| (key, values.into_iter().collect()))
        .collect()
}

fn provider_sources(conn: &Connection, span: Span) -> Result<BTreeMap<String, Vec<String>>> {
    let mut statement = query(conn.prepare(
        "SELECT DISTINCT provider, price_source FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2
           AND provider IS NOT NULL AND price_source IS NOT NULL
         ORDER BY provider, price_source",
    ))?;
    let rows = query(
        statement.query_map(params![span.since_ms, span.until_ms], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }),
    )?;
    let mut sources: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        let (provider, price_source) = query(row)?;
        add_source_set(sources.entry(provider).or_default(), &price_source);
    }
    Ok(sorted_sets(sources))
}

fn provider_billing_kinds(conn: &Connection, span: Span) -> Result<BTreeMap<String, Vec<String>>> {
    let mut statement = query(conn.prepare(
        "SELECT DISTINCT provider, billing_kind FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2 AND provider IS NOT NULL
         ORDER BY provider, billing_kind",
    ))?;
    let rows = query(
        statement.query_map(params![span.since_ms, span.until_ms], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }),
    )?;
    let mut kinds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        let (provider, kind) = query(row)?;
        let kind = kind.trim();
        if !kind.is_empty() {
            kinds.entry(provider).or_default().insert(kind.to_owned());
        }
    }
    Ok(sorted_sets(kinds))
}

/// Provider cost entries, bounded when a bound is given. The omitted count
/// always covers every provider entry beyond the bound.
pub(super) fn provider_costs(
    conn: &Connection,
    span: Option<Span>,
    bound: Option<usize>,
) -> Result<(Vec<Cost>, i64)> {
    let Some(span) = span else {
        return Ok((Vec::new(), 0));
    };
    let total: i64 = query(conn.query_row(
        "SELECT COUNT(DISTINCT provider) FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2 AND provider IS NOT NULL",
        params![span.since_ms, span.until_ms],
        |row| row.get(0),
    ))?;
    let sql = format!(
        "SELECT provider, {COST_AGGREGATES} FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2 AND provider IS NOT NULL
         GROUP BY provider ORDER BY provider{}",
        match bound {
            Some(limit) => format!(" LIMIT {limit}"),
            None => String::new(),
        }
    );
    let mut statement = query(conn.prepare(&sql))?;
    let rows = query(
        statement.query_map(params![span.since_ms, span.until_ms], |row| {
            read_cost(Some(row.get(0)?), row, 1)
        }),
    )?;
    let sources = provider_sources(conn, span)?;
    let kinds = provider_billing_kinds(conn, span)?;
    let estimates = provider_estimates(conn, span)?;
    let mut costs = Vec::new();
    for row in rows {
        let mut cost = query(row)?;
        if let Some(provider) = cost.provider.as_ref() {
            cost.sources = sources.get(provider).cloned().unwrap_or_default();
            cost.billing_kinds = kinds.get(provider).cloned().unwrap_or_default();
            cost.estimate_micros = estimates.get(provider).copied();
        }
        cost.seal_billing_scope();
        costs.push(cost);
    }
    let omitted = total.saturating_sub(costs.len() as i64);
    Ok((costs, omitted))
}

fn model_sources(conn: &Connection, span: Span) -> Result<BTreeMap<String, Vec<String>>> {
    let mut statement = query(conn.prepare(
        "SELECT DISTINCT qualified_model, price_source FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2
           AND qualified_model IS NOT NULL AND provider IS NOT NULL
           AND price_source IS NOT NULL
         ORDER BY qualified_model, price_source",
    ))?;
    let rows = query(
        statement.query_map(params![span.since_ms, span.until_ms], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }),
    )?;
    let mut sources: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        let (model, price_source) = query(row)?;
        add_source_set(sources.entry(model).or_default(), &price_source);
    }
    Ok(sorted_sets(sources))
}

fn model_billing_kinds(conn: &Connection, span: Span) -> Result<BTreeMap<String, Vec<String>>> {
    let mut statement = query(conn.prepare(
        "SELECT DISTINCT qualified_model, billing_kind FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2
           AND qualified_model IS NOT NULL AND provider IS NOT NULL
         ORDER BY qualified_model, billing_kind",
    ))?;
    let rows = query(
        statement.query_map(params![span.since_ms, span.until_ms], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }),
    )?;
    let mut kinds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        let (model, kind) = query(row)?;
        let kind = kind.trim();
        if !kind.is_empty() {
            kinds.entry(model).or_default().insert(kind.to_owned());
        }
    }
    Ok(sorted_sets(kinds))
}

/// Cost estimates for one model's records, grouped by provider. A model whose
/// records span providers has no single provider-scoped estimate.
fn model_costs(conn: &Connection, span: Option<Span>) -> Result<BTreeMap<String, Vec<Cost>>> {
    let Some(span) = span else {
        return Ok(BTreeMap::new());
    };
    let sql = format!(
        "SELECT qualified_model, provider, {COST_AGGREGATES} FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2
           AND qualified_model IS NOT NULL AND provider IS NOT NULL
         GROUP BY qualified_model, provider ORDER BY qualified_model, provider"
    );
    let mut statement = query(conn.prepare(&sql))?;
    let rows = query(
        statement.query_map(params![span.since_ms, span.until_ms], |row| {
            let model: String = row.get(0)?;
            read_cost(Some(row.get(1)?), row, 2).map(|cost| (model, cost))
        }),
    )?;
    let sources = model_sources(conn, span)?;
    let kinds = model_billing_kinds(conn, span)?;
    let estimates = model_estimates(conn, span)?;
    let mut costs: BTreeMap<String, Vec<Cost>> = BTreeMap::new();
    for row in rows {
        let (model, mut cost) = query(row)?;
        if let Some(model_sources) = sources.get(&model) {
            cost.sources.clone_from(model_sources);
        }
        if let Some(model_kinds) = kinds.get(&model) {
            cost.billing_kinds.clone_from(model_kinds);
        }
        if let Some(provider) = cost.provider.clone() {
            cost.estimate_micros = estimates.get(&(model.clone(), provider)).copied();
        }
        cost.seal_billing_scope();
        costs.entry(model).or_default().push(cost);
    }
    Ok(costs)
}

fn allowance_json(row: &ReadingRow) -> Value {
    json!({
        "provider": row.provider,
        "binding": row.binding,
        "window": row.window,
        "basis": row.basis,
        "percent": row.percent,
        "counters": json_text(row.counters.as_deref()),
        "unit": row.unit,
        "observed_at": instant(row.observed_at_ms),
        "source": row.source,
        "access_state": row.access_state,
        "reset": row.reset,
        "freshness": row.freshness,
    })
}

fn json_text(text: Option<&str>) -> Value {
    match text {
        Some(text) => serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())),
        None => Value::Null,
    }
}

/// Dated allowance readings in the period, newest first, bounded.
pub(super) fn allowances(
    conn: &Connection,
    span: Option<Span>,
    bound: usize,
) -> Result<(Vec<Value>, i64)> {
    let Some(span) = span else {
        return Ok((Vec::new(), 0));
    };
    let total: i64 = query(conn.query_row(
        "SELECT COUNT(*) FROM allowance_reading
         WHERE observed_at_ms >= ?1 AND observed_at_ms < ?2",
        params![span.since_ms, span.until_ms],
        |row| row.get(0),
    ))?;
    let mut statement = query(conn.prepare(
        "SELECT observed_at_ms, received_at_ms, provider, binding, scope_window, basis,
                percent, counters, unit, source, access_state, reset, freshness
         FROM allowance_reading
         WHERE observed_at_ms >= ?1 AND observed_at_ms < ?2
         ORDER BY observed_at_ms DESC, id DESC LIMIT ?3",
    ))?;
    let rows = query(statement.query_map(
        params![span.since_ms, span.until_ms, bound as i64],
        reading_row,
    ))?;
    let mut readings = Vec::new();
    for row in rows {
        let row = query(row)?;
        readings.push(allowance_json(&row));
    }
    let omitted = total.saturating_sub(readings.len() as i64);
    Ok((readings, omitted))
}

fn reading_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReadingRow> {
    Ok(ReadingRow {
        observed_at_ms: row.get(0)?,
        received_at_ms: row.get(1)?,
        provider: row.get(2)?,
        binding: row.get(3)?,
        window: row.get(4)?,
        basis: row.get(5)?,
        percent: row.get(6)?,
        counters: row.get(7)?,
        unit: row.get(8)?,
        source: row.get(9)?,
        access_state: row.get(10)?,
        reset: serde_json::from_str(&row.get::<_, String>(11)?).unwrap_or(Value::Null),
        freshness: row.get(12)?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dimension {
    Provider,
    Model,
    Requested,
    Target,
    Protocol,
    Outcome,
    Origin,
    Day,
}

impl Dimension {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "provider" => Ok(Self::Provider),
            "model" => Ok(Self::Model),
            "requested" => Ok(Self::Requested),
            "target" => Ok(Self::Target),
            "protocol" => Ok(Self::Protocol),
            "outcome" => Ok(Self::Outcome),
            "origin" => Ok(Self::Origin),
            "day" => Ok(Self::Day),
            _ => {
                Err(Error::new("invalid_arguments", "Unsupported breakdown dimension").field("by"))
            }
        }
    }

    fn key_expression(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::Model => "qualified_model",
            Self::Requested => "requested_model",
            Self::Target => "target",
            Self::Protocol => "protocol",
            Self::Outcome => "outcome",
            Self::Origin => "origin",
            // The local calendar day of the membership timestamp. The SQL
            // rendering matches `crate::usage::local_day` for the same instant.
            Self::Day => "strftime('%Y-%m-%d', membership_ms / 1000, 'unixepoch', 'localtime')",
        }
    }

    fn is_provider(self) -> bool {
        matches!(self, Self::Provider)
    }

    fn is_model(self) -> bool {
        matches!(self, Self::Model)
    }
}

struct RowCosts {
    providers: BTreeMap<String, Cost>,
    models: BTreeMap<String, Vec<Cost>>,
}

impl RowCosts {
    fn load(conn: &Connection, span: Option<Span>, dimension: Dimension) -> Result<Self> {
        let providers = if dimension.is_provider() {
            provider_costs(conn, span, None)?
                .0
                .into_iter()
                .filter_map(|cost| cost.provider.clone().map(|provider| (provider, cost)))
                .collect()
        } else {
            BTreeMap::new()
        };
        let models = if dimension.is_model() {
            model_costs(conn, span)?
        } else {
            BTreeMap::new()
        };
        Ok(Self { providers, models })
    }

    fn for_row(&self, key: Option<&str>, dimension: Dimension) -> Option<Value> {
        if dimension.is_provider() {
            return self.providers.get(key?).map(Cost::to_json);
        }
        if dimension.is_model() {
            let costs = self.models.get(key?)?;
            return match costs.as_slice() {
                [cost] => Some(cost.to_json()),
                _ => None,
            };
        }
        None
    }
}

fn row_json(
    key: Option<String>,
    aggregate: &Aggregate,
    duration: Duration,
    cost: Option<Value>,
) -> Value {
    let cost_reason = if cost.is_some() {
        Value::Null
    } else {
        json!("cross_provider")
    };
    json!({
        "key": key,
        "requests": aggregate.requests,
        "outcomes": aggregate.outcomes_json(),
        "probes": aggregate.probes,
        "usage": aggregate.usage_json(),
        "usage_states": aggregate.usage_states_json(),
        "duration": duration.to_json(),
        "cost": cost.unwrap_or(Value::Null),
        "cost_reason": cost_reason,
    })
}

fn duration_samples_by_key(
    conn: &Connection,
    span: Span,
    dimension: Dimension,
) -> Result<BTreeMap<Option<String>, Vec<i64>>> {
    let key = dimension.key_expression();
    let sql = format!(
        "SELECT {key} AS k, duration_ms FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2
           AND origin <> 'probe' AND outcome = 'completed' AND duration_ms IS NOT NULL
         ORDER BY k IS NULL, k, duration_ms"
    );
    let mut statement = query(conn.prepare(&sql))?;
    let rows = query(
        statement.query_map(params![span.since_ms, span.until_ms], |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?))
        }),
    )?;
    let mut samples: BTreeMap<Option<String>, Vec<i64>> = BTreeMap::new();
    for row in rows {
        let (key, duration_ms) = query(row)?;
        samples.entry(key).or_default().push(duration_ms);
    }
    Ok(samples)
}

/// Canonical `day` keys: the SQL grouping expression selects the day buckets,
/// then the retained instant of each bucket is rendered with the same local-day
/// helper the rest of the crate uses.
fn day_keys(conn: &Connection, span: Span, timezone: &str) -> Result<BTreeMap<String, String>> {
    let key = Dimension::Day.key_expression();
    let sql = format!(
        "SELECT {key} AS k, MIN(membership_ms) FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2
         GROUP BY k"
    );
    let mut statement = query(conn.prepare(&sql))?;
    let rows = query(
        statement.query_map(params![span.since_ms, span.until_ms], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }),
    )?;
    let mut keys = BTreeMap::new();
    for row in rows {
        let (sql_key, first_ms) = query(row)?;
        keys.insert(sql_key.clone(), local_day(first_ms, timezone));
    }
    Ok(keys)
}

fn breakdown_rows(
    conn: &Connection,
    span: Span,
    dimension: Dimension,
    timezone: &str,
) -> Result<(Vec<Value>, i64)> {
    let key = dimension.key_expression();
    let count_sql = format!(
        "SELECT COUNT(*) FROM (
           SELECT 1 FROM usage_record
           WHERE membership_ms >= ?1 AND membership_ms < ?2
           GROUP BY {key})"
    );
    let total: i64 = query(conn.query_row(
        &count_sql,
        params![span.since_ms, span.until_ms],
        |row| row.get(0),
    ))?;
    let sql = format!(
        "SELECT {key} AS k, {AGGREGATES} FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2
         GROUP BY k ORDER BY k IS NULL, k LIMIT ?3"
    );
    let mut statement = query(conn.prepare(&sql))?;
    let rows = query(statement.query_map(
        params![span.since_ms, span.until_ms, ROW_BOUND as i64],
        |row| Ok((row.get::<_, Option<String>>(0)?, Aggregate::read(row, 1)?)),
    ))?;
    let costs = RowCosts::load(conn, Some(span), dimension)?;
    let mut durations = duration_samples_by_key(conn, span, dimension)?;
    let day_keys = if dimension == Dimension::Day {
        day_keys(conn, span, timezone)?
    } else {
        BTreeMap::new()
    };
    let mut out = Vec::new();
    for row in rows {
        let (key, aggregate) = query(row)?;
        let samples = durations.remove(&key).unwrap_or_default();
        let cost = costs.for_row(key.as_deref(), dimension);
        let key = match (dimension, key) {
            (Dimension::Day, Some(key)) => day_keys.get(&key).cloned().or(Some(key)),
            (_, key) => key,
        };
        out.push(row_json(
            key,
            &aggregate,
            duration_from_samples(samples),
            cost,
        ));
    }
    let omitted = total.saturating_sub(out.len() as i64);
    Ok((out, omitted))
}

fn unavailable_data(period: &Period, reason_code: &str, breakdown: Option<Dimension>) -> Value {
    let snapshot = Snapshot::unavailable(period, reason_code);
    let mut data = snapshot.base();
    data.insert("totals".into(), Value::Null);
    data.insert("costs".into(), json!([]));
    data.insert("allowances".into(), json!([]));
    data.insert("omitted_costs".into(), json!(0));
    data.insert("omitted_allowances".into(), json!(0));
    if breakdown.is_some() {
        data.insert("rows".into(), json!([]));
        data.insert("omitted_rows".into(), json!(0));
    }
    Value::Object(data)
}

fn summary_data(snapshot: &Snapshot) -> Result<Value> {
    let mut data = snapshot.base();
    match snapshot.coverage.state {
        CoverageState::Unavailable => {
            data.insert("totals".into(), Value::Null);
            data.insert("costs".into(), json!([]));
            data.insert("allowances".into(), json!([]));
            data.insert("omitted_costs".into(), json!(0));
            data.insert("omitted_allowances".into(), json!(0));
        }
        CoverageState::Absent => {
            data.insert("totals".into(), Totals::empty().to_json());
            data.insert("costs".into(), json!([]));
            data.insert("allowances".into(), json!([]));
            data.insert("omitted_costs".into(), json!(0));
            data.insert("omitted_allowances".into(), json!(0));
        }
        CoverageState::Available => {
            let conn = snapshot
                .conn()
                .expect("available snapshot has a connection");
            let totals = match snapshot.usage_span() {
                Some(span) => Totals::load(conn, span)?,
                None => Totals::empty(),
            };
            let (costs, omitted_costs) =
                provider_costs(conn, snapshot.usage_span(), Some(COST_BOUND))?;
            let (allowances, omitted_allowances) =
                allowances(conn, snapshot.reading_span(), ALLOWANCE_BOUND)?;
            data.insert("totals".into(), totals.to_json());
            data.insert(
                "costs".into(),
                Value::Array(costs.iter().map(Cost::to_json).collect()),
            );
            data.insert("allowances".into(), Value::Array(allowances));
            data.insert("omitted_costs".into(), json!(omitted_costs));
            data.insert("omitted_allowances".into(), json!(omitted_allowances));
        }
    }
    Ok(Value::Object(data))
}

fn breakdown_data(snapshot: &Snapshot, dimension: Dimension) -> Result<Value> {
    let mut data = snapshot.base();
    match snapshot.coverage.state {
        CoverageState::Unavailable => {
            data.insert("totals".into(), Value::Null);
            data.insert("costs".into(), json!([]));
            data.insert("allowances".into(), json!([]));
            data.insert("omitted_costs".into(), json!(0));
            data.insert("omitted_allowances".into(), json!(0));
            data.insert("rows".into(), json!([]));
            data.insert("omitted_rows".into(), json!(0));
        }
        CoverageState::Absent => {
            data.insert("totals".into(), Totals::empty().to_json());
            data.insert("costs".into(), json!([]));
            data.insert("allowances".into(), json!([]));
            data.insert("omitted_costs".into(), json!(0));
            data.insert("omitted_allowances".into(), json!(0));
            data.insert("rows".into(), json!([]));
            data.insert("omitted_rows".into(), json!(0));
        }
        CoverageState::Available => {
            let conn = snapshot
                .conn()
                .expect("available snapshot has a connection");
            let totals = match snapshot.usage_span() {
                Some(span) => Totals::load(conn, span)?,
                None => Totals::empty(),
            };
            let (costs, omitted_costs) =
                provider_costs(conn, snapshot.usage_span(), Some(COST_BOUND))?;
            let (allowances, omitted_allowances) =
                allowances(conn, snapshot.reading_span(), ALLOWANCE_BOUND)?;
            let (rows, omitted_rows) = match snapshot.usage_span() {
                Some(span) => breakdown_rows(conn, span, dimension, &snapshot.period.timezone)?,
                None => (Vec::new(), 0),
            };
            data.insert("totals".into(), totals.to_json());
            data.insert(
                "costs".into(),
                Value::Array(costs.iter().map(Cost::to_json).collect()),
            );
            data.insert("allowances".into(), Value::Array(allowances));
            data.insert("omitted_costs".into(), json!(omitted_costs));
            data.insert("omitted_allowances".into(), json!(omitted_allowances));
            data.insert("rows".into(), Value::Array(rows));
            data.insert("omitted_rows".into(), json!(omitted_rows));
        }
    }
    Ok(Value::Object(data))
}

pub(crate) fn summary(store: &Store, period: &Period) -> Result<Value> {
    let snapshot = Snapshot::load(store, period);
    match summary_data(&snapshot) {
        Ok(data) => Ok(data),
        Err(error) => Ok(unavailable_data(period, &error.code, None)),
    }
}

pub(crate) fn breakdown(store: &Store, period: &Period, dimension: &str) -> Result<Value> {
    let dimension = Dimension::parse(dimension)?;
    let snapshot = Snapshot::load(store, period);
    match breakdown_data(&snapshot, dimension) {
        Ok(data) => Ok(data),
        Err(error) => Ok(unavailable_data(period, &error.code, Some(dimension))),
    }
}

fn clear_blocked(error: Error) -> Error {
    if error.code == "storage_version_unsupported" {
        return error;
    }
    Error::new(
        "usage_store_unavailable",
        &format!(
            "The local usage ledger artifact {} cannot be read; preserve it and repair the affected store",
            ledger::DB_RELATIVE
        ),
    )
}

fn clear_failed() -> Error {
    Error::new(
        "usage_clear_failed",
        "The retained usage data could not be removed",
    )
}

fn clear_data(
    dry_run: bool,
    executed_at: Option<DateTime<Utc>>,
    removed: ledger::ClearCounts,
    retained: ledger::ClearCounts,
    coverage: &Coverage,
) -> Value {
    json!({
        "clear": {
            "dry_run": dry_run,
            "executed_at": executed_at.map(instant_at).unwrap_or(Value::Null),
            "removed": {
                "usage_records": removed.usage_records,
                "allowance_readings": removed.allowance_readings,
            },
            "retained": {
                "usage_records": retained.usage_records,
                "allowance_readings": retained.allowance_readings,
            },
        },
        "coverage": coverage.to_json(),
    })
}

pub(crate) fn clear(store: &Store, dry_run: bool) -> Result<Value> {
    let now = Utc::now();
    match ledger::open_read(store) {
        // An absent store is a successful no-op; it must not create the file.
        Ok(None) => Ok(clear_data(
            dry_run,
            None,
            ledger::ClearCounts::default(),
            ledger::ClearCounts::default(),
            &Coverage::absent(),
        )),
        Err(error) => Err(clear_blocked(error)),
        Ok(Some(conn)) => {
            if dry_run {
                let removed = ledger::counts(&conn).map_err(|_| clear_failed())?;
                let coverage = coverage_of(&conn).map_err(|_| clear_failed())?;
                return Ok(clear_data(
                    true,
                    None,
                    removed,
                    ledger::ClearCounts::default(),
                    &coverage,
                ));
            }
            drop(conn);
            let mut connection = ledger::open_write(store).map_err(clear_blocked)?;
            let removed = ledger::clear(&mut connection).map_err(|_| clear_failed())?;
            let retained = ledger::counts(&connection).map_err(|_| clear_failed())?;
            let coverage = coverage_of(&connection).map_err(|_| clear_failed())?;
            Ok(clear_data(false, Some(now), removed, retained, &coverage))
        }
    }
}

/// One grouped failed/rejected condition in the report period.
#[derive(Debug, Clone)]
pub(super) struct FailureGroup {
    pub stage: String,
    pub reason_code: String,
    pub count: i64,
}

/// Failed and rejected client records grouped by stage and controlled reason
/// code, largest group first.
pub(super) fn failure_groups(conn: &Connection, span: Span) -> Result<Vec<FailureGroup>> {
    let mut statement = query(conn.prepare(
        "SELECT stage, reason_code, COUNT(*) FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2
           AND origin <> 'probe' AND outcome IN ('failed', 'rejected')
           AND reason_code IS NOT NULL
         GROUP BY stage, reason_code
         ORDER BY COUNT(*) DESC, reason_code, stage",
    ))?;
    let rows = query(
        statement.query_map(params![span.since_ms, span.until_ms], |row| {
            Ok(FailureGroup {
                stage: row.get(0)?,
                reason_code: row.get(1)?,
                count: row.get(2)?,
            })
        }),
    )?;
    let mut groups = Vec::new();
    for row in rows {
        groups.push(query(row)?);
    }
    Ok(groups)
}

/// Total retained records and unpriced records in the period.
pub(super) fn unpriced_counts(conn: &Connection, span: Span) -> Result<(i64, i64)> {
    query(conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN cost_state = 'unpriced' THEN 1 ELSE 0 END),0)
         FROM usage_record WHERE membership_ms >= ?1 AND membership_ms < ?2",
        params![span.since_ms, span.until_ms],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ))
}

/// Estimated cost summed per provider, target, and qualified model.
#[derive(Debug, Clone)]
pub(super) struct ConcentrationRow {
    pub provider: String,
    pub target: Option<String>,
    pub model: Option<String>,
    pub cost_micros: i128,
}

pub(super) fn concentration_rows(conn: &Connection, span: Span) -> Result<Vec<ConcentrationRow>> {
    let mut statement = query(conn.prepare(
        "SELECT provider, target, qualified_model, cost_micros FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2
           AND provider IS NOT NULL AND cost_micros IS NOT NULL",
    ))?;
    let rows = query(
        statement.query_map(params![span.since_ms, span.until_ms], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        }),
    )?;
    let mut totals: BTreeMap<(String, Option<String>, Option<String>), i128> = BTreeMap::new();
    for row in rows {
        let (provider, target, model, micros) = query(row)?;
        add_exact(totals.entry((provider, target, model)).or_default(), micros);
    }
    Ok(totals
        .into_iter()
        .map(
            |((provider, target, model), cost_micros)| ConcentrationRow {
                provider,
                target,
                model,
                cost_micros,
            },
        )
        .collect())
}

/// Client records and how many of them have no established Agent target.
pub(super) fn attribution_counts(conn: &Connection, span: Span) -> Result<(i64, i64)> {
    query(conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN target IS NULL THEN 1 ELSE 0 END),0)
         FROM usage_record
         WHERE membership_ms >= ?1 AND membership_ms < ?2 AND origin <> 'probe'",
        params![span.since_ms, span.until_ms],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ))
}

/// Dated allowance readings in the period, ordered oldest first.
pub(super) fn reading_rows(conn: &Connection, span: Option<Span>) -> Result<Vec<ReadingRow>> {
    let Some(span) = span else {
        return Ok(Vec::new());
    };
    let mut statement = query(conn.prepare(
        "SELECT observed_at_ms, received_at_ms, provider, binding, scope_window, basis,
                percent, counters, unit, source, access_state, reset, freshness
         FROM allowance_reading
         WHERE observed_at_ms >= ?1 AND observed_at_ms < ?2
         ORDER BY observed_at_ms, id",
    ))?;
    let rows = query(statement.query_map(params![span.since_ms, span.until_ms], reading_row))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(query(row)?);
    }
    Ok(out)
}

#[cfg(test)]
pub(super) mod test_support {
    use crate::{
        storage::Store,
        usage::{Period, ReadingRow, UsageRow, WriteJob, ledger},
    };
    use chrono::{DateTime, Utc};
    use rusqlite::Connection;
    use std::path::PathBuf;

    pub(crate) fn store() -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = Store::at(directory.path().to_owned());
        (directory, store)
    }

    pub(crate) fn now_ms() -> i64 {
        Utc::now().timestamp_millis()
    }

    pub(crate) fn millis(ms: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp_millis(ms).expect("valid instant")
    }

    pub(crate) fn range(since_ms: i64, until_ms: i64) -> Period {
        Period {
            kind: "range".into(),
            since: Some(millis(since_ms)),
            until: Some(millis(until_ms)),
            timezone: "UTC".into(),
        }
    }

    pub(crate) fn all() -> Period {
        Period {
            kind: "all".into(),
            since: None,
            until: None,
            timezone: "UTC".into(),
        }
    }

    pub(crate) fn client(at_ms: i64) -> UsageRow {
        UsageRow {
            received_at_ms: at_ms,
            admitted_at_ms: Some(at_ms),
            membership_ms: at_ms,
            origin: "client".into(),
            model_kind: "concrete".into(),
            outcome: "completed".into(),
            stage: "upstream_response".into(),
            usage_state: "reported".into(),
            billing_kind: "api".into(),
            cost_state: "none".into(),
            ..UsageRow::default()
        }
    }

    pub(crate) fn probe(at_ms: i64) -> UsageRow {
        UsageRow {
            origin: "probe".into(),
            ..client(at_ms)
        }
    }

    pub(crate) fn reading(
        at_ms: i64,
        provider: &str,
        window: &str,
        basis: &str,
        percent: &str,
    ) -> ReadingRow {
        ReadingRow {
            observed_at_ms: at_ms,
            received_at_ms: at_ms,
            provider: provider.into(),
            binding: format!("binding-{provider}"),
            window: window.into(),
            basis: basis.into(),
            percent: Some(percent.into()),
            counters: None,
            unit: None,
            source: "test".into(),
            access_state: Some("available".into()),
            reset: serde_json::Value::Null,
            freshness: None,
        }
    }

    pub(crate) fn append(store: &Store, rows: Vec<UsageRow>, readings: Vec<ReadingRow>) {
        let mut jobs = rows.into_iter().map(WriteJob::Usage).collect::<Vec<_>>();
        jobs.extend(readings.into_iter().map(WriteJob::Reading));
        ledger::append_direct(store, &jobs).expect("append ledger fixtures");
    }

    pub(crate) fn db_path(store: &Store) -> PathBuf {
        ledger::db_path(store).expect("ledger path")
    }

    /// Run fixture SQL directly, for states a write-path prune would prevent.
    pub(crate) fn exec(store: &Store, sql: &str) {
        let connection = Connection::open(db_path(store)).expect("open ledger");
        connection.execute_batch(sql).expect("exec fixture sql");
    }

    pub(crate) fn rfc3339(ms: i64) -> String {
        millis(ms).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    pub(crate) fn set_meta(store: &Store, key: &str, value: &str) {
        let connection = Connection::open(db_path(store)).expect("open ledger");
        connection
            .execute(
                "INSERT INTO meta(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![key, value],
            )
            .expect("set coverage marker");
    }

    pub(crate) fn write_corrupt(store: &Store) {
        let path = db_path(store);
        std::fs::create_dir_all(path.parent().expect("ledger directory"))
            .expect("create ledger directory");
        std::fs::write(path, b"this is not a sqlite database").expect("write corrupt ledger");
    }

    pub(crate) fn write_unsupported(store: &Store) {
        let path = db_path(store);
        std::fs::create_dir_all(path.parent().expect("ledger directory"))
            .expect("create ledger directory");
        let connection = Connection::open(&path).expect("open ledger");
        connection
            .execute_batch(
                "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 PRAGMA user_version = 2;",
            )
            .expect("write unsupported schema");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::report::test_support as support;

    fn row_for<'a>(rows: &'a Value, key: Option<&str>) -> Option<&'a Value> {
        rows.as_array()?.iter().find(|row| {
            let row_key = &row["key"];
            match key {
                Some(key) => row_key == key,
                None => row_key.is_null(),
            }
        })
    }

    #[test]
    fn absent_store_reports_absent_coverage_without_creating_the_ledger() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let period = support::range(now - 3_600_000, now + 3_600_000);
        let data = summary(&store, &period).expect("absent report");
        assert_eq!(data["coverage"]["state"], "absent");
        assert_eq!(data["coverage"]["reason_code"], Value::Null);
        assert_eq!(data["coverage"]["oldest"], Value::Null);
        assert_eq!(data["coverage"]["newest"], Value::Null);
        assert_eq!(data["coverage"]["retained_records"], 0);
        assert_eq!(data["coverage"]["evicted"], 0);
        assert_eq!(data["coverage"]["dropped"], 0);
        assert_eq!(data["coverage"]["cleared_at"], Value::Null);
        assert_eq!(data["coverage"]["gaps"], json!([]));
        assert_eq!(data["coverage"]["reportable"], Value::Null);
        assert_eq!(data["coverage"]["retention"]["usage_days"], 90);
        assert_eq!(data["coverage"]["retention"]["readings_days"], 45);
        assert_eq!(data["coverage"]["retention"]["enforcement"], "on_write");
        assert_eq!(data["coverage"]["retention"]["usage_overdue"], false);
        assert_eq!(data["coverage"]["retention"]["readings_overdue"], false);
        assert_eq!(data["coverage"]["readings"]["state"], "absent");
        assert_eq!(data["coverage"]["readings"]["gaps"], json!([]));
        assert_eq!(data["coverage"]["readings"]["reportable"], Value::Null);
        assert_eq!(data["coverage"]["readings"]["retained_records"], 0);
        assert_eq!(data["totals"]["requests"], 0);
        assert_eq!(data["totals"]["usage"]["input_tokens"], Value::Null);
        assert_eq!(data["totals"]["usage"]["output_tokens"], Value::Null);
        assert_eq!(data["totals"]["duration"]["state"], "no_records");
        assert_eq!(data["totals"]["duration"]["samples"], 0);
        assert_eq!(data["totals"]["duration"]["sample_minimum"], 20);
        assert_eq!(data["totals"]["membership_basis"], "admission");
        assert_eq!(data["costs"], json!([]));
        assert_eq!(data["allowances"], json!([]));
        assert_eq!(data["omitted_costs"], 0);
        assert_eq!(data["omitted_allowances"], 0);
        assert!(data.get("rows").is_none());
        assert!(data.get("omitted_rows").is_none());
        assert_eq!(data["scope"]["source"], "gateway_observed");
        assert_eq!(data["scope"]["direct_traffic"], "unobserved");
        assert_eq!(data["scope"]["probes"], "separate");
        assert_eq!(data["scope"]["diagnostic_history"], "excluded");
        assert!(
            !support::db_path(&store).exists(),
            "an absent read creates no file"
        );
    }

    #[test]
    fn empty_ledger_is_available_coverage_with_zero_counts_and_null_counters() {
        let (_directory, store) = support::store();
        support::append(&store, Vec::new(), Vec::new());
        let now = support::now_ms();
        let data = summary(&store, &support::range(now - 3_600_000, now + 3_600_000))
            .expect("empty report");
        assert_eq!(data["coverage"]["state"], "available");
        assert_eq!(data["coverage"]["retained_records"], 0);
        assert_eq!(data["coverage"]["oldest"], Value::Null);
        assert_eq!(data["coverage"]["gaps"], json!([]));
        assert_eq!(data["coverage"]["reportable"], Value::Null);
        assert_eq!(data["coverage"]["retention"]["usage_overdue"], false);
        assert_eq!(data["coverage"]["retention"]["readings_overdue"], false);
        assert_eq!(data["totals"]["requests"], 0);
        assert_eq!(data["totals"]["probes"], 0);
        assert_eq!(data["totals"]["usage"]["input_tokens"], Value::Null);
        assert_eq!(data["totals"]["duration"]["state"], "no_records");
        assert_eq!(data["costs"], json!([]));
        assert_eq!(data["allowances"], json!([]));
    }

    #[test]
    fn unavailable_ledger_reports_a_typed_reason_instead_of_zero_usage() {
        for (write, expected_reason) in [
            (
                support::write_corrupt as fn(&crate::storage::Store),
                None::<&str>,
            ),
            (
                support::write_unsupported as fn(&crate::storage::Store),
                Some("storage_version_unsupported"),
            ),
        ] {
            let (_directory, store) = support::store();
            write(&store);
            let now = support::now_ms();
            let period = support::range(now - 3_600_000, now + 3_600_000);
            let data = summary(&store, &period).expect("unavailable report is still a read");
            assert_eq!(data["coverage"]["state"], "unavailable");
            let reason = data["coverage"]["reason_code"]
                .as_str()
                .expect("typed reason");
            if let Some(expected) = expected_reason {
                assert_eq!(reason, expected);
            }
            assert_eq!(data["coverage"]["oldest"], Value::Null);
            assert_eq!(data["coverage"]["reportable"], Value::Null);
            assert_eq!(data["coverage"]["gaps"], json!([]));
            assert_eq!(data["coverage"]["retention"]["usage_overdue"], Value::Null);
            assert_eq!(
                data["coverage"]["retention"]["readings_overdue"],
                Value::Null
            );
            assert_eq!(data["coverage"]["retained_records"], Value::Null);
            assert_eq!(
                data["coverage"]["readings"]["retained_records"],
                Value::Null
            );
            assert_eq!(data["totals"], Value::Null);
            assert_eq!(data["costs"], json!([]));
            assert_eq!(data["allowances"], json!([]));
            assert_eq!(data["omitted_costs"], 0);
            assert_eq!(data["omitted_allowances"], 0);
            assert_eq!(data["period"]["since"], Value::Null);
            assert_eq!(data["period"]["until"], Value::Null);
            let breakdown = breakdown(&store, &period, "provider").expect("unavailable rows");
            assert_eq!(breakdown["rows"], json!([]));
            assert_eq!(breakdown["omitted_rows"], 0);
            assert_eq!(breakdown["totals"], Value::Null);
        }
    }

    #[test]
    fn token_totals_keep_known_zero_distinct_from_unknown() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let mut counted = support::client(now - 600_000);
        counted.input_tokens = Some(0);
        counted.output_tokens = None;
        counted.usage_state = "partial".into();
        let mut probed = support::probe(now - 300_000);
        probed.input_tokens = Some(7);
        probed.outcome = "failed".into();
        support::append(&store, vec![counted, probed], Vec::new());
        let data = summary(&store, &support::range(now - 3_600_000, now + 3_600_000))
            .expect("token report");
        let totals = &data["totals"];
        assert_eq!(totals["requests"], 1);
        assert_eq!(totals["probes"], 1);
        assert_eq!(totals["outcomes"]["completed"], 1);
        assert_eq!(
            totals["outcomes"]["failed"], 0,
            "probe outcomes stay separate"
        );
        // Known zero is a value; an absent output counter stays null.
        assert_eq!(totals["usage"]["input_tokens"], 7);
        assert_eq!(totals["usage"]["output_tokens"], Value::Null);
        assert_eq!(totals["usage"]["cache_read_tokens"], Value::Null);
        assert_eq!(totals["usage_states"]["partial"], 1);
        assert_eq!(totals["usage_states"]["reported"], 1);
    }

    #[test]
    fn costs_are_per_provider_and_never_combined() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let priced = |provider: &str, micros: i64, source: &str| {
            let mut row = support::client(now - 600_000);
            row.provider = Some(provider.into());
            row.qualified_model = Some(format!("{provider}/m"));
            row.cost_micros = Some(micros);
            row.cost_state = "priced".into();
            row.price_source = Some(source.into());
            row
        };
        let mut unpriced = support::client(now - 600_000);
        unpriced.provider = Some("p3".into());
        unpriced.qualified_model = Some("p3/m".into());
        unpriced.input_tokens = Some(5);
        unpriced.cost_state = "unpriced".into();
        let mut no_axes = support::client(now - 600_000);
        no_axes.provider = Some("p4".into());
        support::append(
            &store,
            vec![
                priced("p1", 1_500_000, "catalog"),
                priced("p2", 250_000, "override"),
                unpriced,
                no_axes,
            ],
            Vec::new(),
        );
        let data = summary(&store, &support::range(now - 3_600_000, now + 3_600_000))
            .expect("cost report");
        let costs = data["costs"].as_array().expect("cost array");
        assert_eq!(costs.len(), 4);
        assert_eq!(costs[0]["provider"], "p1");
        assert_eq!(costs[0]["currency"], "USD");
        assert_eq!(costs[0]["estimate"], "1.500000");
        assert_eq!(costs[0]["estimate_reason"], Value::Null);
        assert_eq!(costs[0]["priced"], 1);
        assert_eq!(costs[0]["sources"], json!(["catalog"]));
        assert_eq!(
            costs[0]["unpriceable_axes"],
            json!(["reasoning", "cache_storage"])
        );
        assert_eq!(costs[1]["provider"], "p2");
        assert_eq!(costs[1]["estimate"], "0.250000");
        assert_eq!(costs[1]["sources"], json!(["override"]));
        assert_eq!(costs[2]["provider"], "p3");
        assert_eq!(costs[2]["estimate"], Value::Null);
        assert_eq!(costs[2]["estimate_reason"], "unpriced");
        assert_eq!(costs[2]["currency"], Value::Null);
        assert_eq!(costs[2]["priced"], 0);
        assert_eq!(costs[2]["partially_priced"], 0);
        assert_eq!(costs[2]["unpriced"], 1);
        assert_eq!(costs[3]["provider"], "p4");
        assert_eq!(costs[3]["estimate_reason"], "no_priced_axes");
        // Unknown usage (`none` cost state) is not counted as unpriced.
        assert_eq!(costs[3]["priced"], 0);
        assert_eq!(costs[3]["partially_priced"], 0);
        assert_eq!(costs[3]["unpriced"], 0);
        // No combined estimate appears at any level of the report.
        assert!(data.get("estimate").is_none());
        assert!(data["totals"].get("cost").is_none());
        for key in ["total", "combined", "cost"] {
            assert!(data.get(key).is_none(), "no combined {key} key");
        }
    }

    #[test]
    fn breakdown_day_rows_use_local_calendar_keys() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let today = now - 60_000;
        let yesterday = today - 86_400_000;
        support::append(
            &store,
            vec![
                support::client(today),
                support::client(today - 1_000),
                support::client(yesterday),
            ],
            Vec::new(),
        );
        let period = support::range(yesterday - 86_400_000, now + 60_000);
        let data = breakdown(&store, &period, "day").expect("day breakdown");
        let rows = data["rows"].as_array().expect("day rows");
        assert_eq!(rows.len(), 2);
        let expected_today = crate::usage::local_day(today, "UTC");
        let expected_yesterday = crate::usage::local_day(yesterday, "UTC");
        let keys: Vec<&str> = rows.iter().filter_map(|row| row["key"].as_str()).collect();
        assert!(keys.contains(&expected_today.as_str()), "{keys:?}");
        assert!(keys.contains(&expected_yesterday.as_str()), "{keys:?}");
        let today_row = row_for(&data["rows"], Some(&expected_today)).expect("today row");
        assert_eq!(today_row["requests"], 2);
        let yesterday_row =
            row_for(&data["rows"], Some(&expected_yesterday)).expect("yesterday row");
        assert_eq!(yesterday_row["requests"], 1);
    }

    #[test]
    fn breakdown_unknown_provider_group_is_explicit_and_carries_no_combined_cost() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let mut priced = support::client(now - 60_000);
        priced.provider = Some("p".into());
        priced.qualified_model = Some("p/m".into());
        priced.cost_micros = Some(1_000_000);
        priced.cost_state = "priced".into();
        priced.price_source = Some("catalog".into());
        support::append(
            &store,
            vec![
                support::client(now - 120_000),
                support::client(now - 90_000),
                priced,
            ],
            Vec::new(),
        );
        let data = breakdown(
            &store,
            &support::range(now - 3_600_000, now + 3_600_000),
            "provider",
        )
        .expect("provider breakdown");
        let rows = data["rows"].as_array().expect("provider rows");
        assert_eq!(rows.len(), 2);
        let unknown = row_for(&data["rows"], None).expect("unknown group");
        assert_eq!(unknown["key"], Value::Null);
        assert_eq!(unknown["requests"], 2);
        assert_eq!(unknown["cost"], Value::Null);
        assert_eq!(unknown["cost_reason"], "cross_provider");
        let known = row_for(&data["rows"], Some("p")).expect("known group");
        assert_eq!(known["cost"]["provider"], "p");
        assert_eq!(known["cost"]["estimate"], "1.000000");
        assert_eq!(known["cost_reason"], Value::Null);
    }

    #[test]
    fn bounded_arrays_report_their_omitted_counts() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let rows: Vec<_> = (0..201)
            .map(|index| {
                let mut row = support::client(now - 60_000 - index * 1_000);
                let provider = format!("p{index:03}");
                row.provider = Some(provider.clone());
                row.qualified_model = Some(format!("{provider}/m"));
                row.cost_micros = Some(1_000);
                row.cost_state = "priced".into();
                row.price_source = Some("catalog".into());
                row
            })
            .collect();
        let readings: Vec<_> = (0..201)
            .map(|index| {
                support::reading(
                    now - 120_000 - index * 1_000,
                    "p",
                    &format!("w{index:03}"),
                    "consumed_percent",
                    "1",
                )
            })
            .collect();
        support::append(&store, rows, readings);
        let period = support::range(now - 3_600_000, now + 3_600_000);
        let data = summary(&store, &period).expect("bounded report");
        assert_eq!(data["costs"].as_array().unwrap().len(), COST_BOUND);
        assert_eq!(data["omitted_costs"], 201 - COST_BOUND as i64);
        assert_eq!(
            data["allowances"].as_array().unwrap().len(),
            ALLOWANCE_BOUND
        );
        assert_eq!(data["omitted_allowances"], 1);
        // The newest reading is retained first.
        assert_eq!(data["allowances"][0]["window"], "w000");
        let rows = breakdown(&store, &period, "provider").expect("bounded breakdown");
        assert_eq!(rows["rows"].as_array().unwrap().len(), ROW_BOUND);
        assert_eq!(rows["omitted_rows"], 1);
    }

    #[test]
    fn duration_percentiles_require_the_sample_minimum() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let mut low: Vec<_> = (1..=19)
            .map(|index| {
                let mut row = support::client(now - 60_000 - index * 1_000);
                row.duration_ms = Some(index);
                row
            })
            .collect();
        let mut probed = support::probe(now - 300_000);
        probed.duration_ms = Some(9_999);
        low.push(probed);
        support::append(&store, low.clone(), Vec::new());
        low.pop();
        let period = support::range(now - 3_600_000, now + 3_600_000);
        let data = summary(&store, &period).expect("low sample report");
        assert_eq!(data["totals"]["duration"]["state"], "low_samples");
        assert_eq!(data["totals"]["duration"]["samples"], 19);
        assert_eq!(data["totals"]["duration"]["p50_ms"], Value::Null);
        assert_eq!(data["totals"]["duration"]["p90_ms"], Value::Null);

        let mut twentieth = support::client(now - 200_000);
        twentieth.duration_ms = Some(20);
        low.push(twentieth);
        support::append(&store, vec![low.pop().unwrap()], Vec::new());
        let data = summary(&store, &period).expect("full sample report");
        assert_eq!(data["totals"]["duration"]["state"], "available");
        assert_eq!(data["totals"]["duration"]["samples"], 20);
        // Nearest-rank over 1..=20: p50 is the 10th value, p90 is the 18th.
        assert_eq!(data["totals"]["duration"]["p50_ms"], 10);
        assert_eq!(data["totals"]["duration"]["p90_ms"], 18);
    }

    #[test]
    fn coverage_reports_typed_gaps_retention_and_readings_coverage() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        support::append(
            &store,
            vec![support::client(now - 3_000_000)],
            vec![support::reading(
                now - 2_000_000,
                "p",
                "w",
                "consumed_percent",
                "10",
            )],
        );
        let cleared_at = now - 1_000;
        support::set_meta(&store, "evicted", "3");
        support::set_meta(&store, "dropped", "2");
        support::set_meta(&store, "cleared_at_ms", &cleared_at.to_string());
        support::set_meta(&store, "restart_gap", "1");
        support::set_meta(&store, "readings_evicted", "5");
        support::set_meta(&store, "readings_dropped", "4");
        let period = support::range(now - 3_600_000, now);
        let data = summary(&store, &period).expect("coverage report");
        let coverage = &data["coverage"];
        assert_eq!(coverage["evicted"], 3);
        assert_eq!(coverage["dropped"], 2);
        assert_eq!(coverage["cleared_at"], json!(support::rfc3339(cleared_at)));
        assert_eq!(
            coverage["gaps"],
            json!([
                {"kind": "cleared", "at": support::rfc3339(cleared_at)},
                {"kind": "evicted", "count": 3},
                {"kind": "dropped", "count": 2},
                {"kind": "restart", "loss": "unknown"},
                {
                    "kind": "out_of_coverage",
                    "from": support::rfc3339(now - 3_600_000),
                    "to": support::rfc3339(now - 3_000_000),
                },
            ])
        );
        assert_eq!(
            coverage["reportable"],
            json!({
                "since": support::rfc3339(now - 3_000_000),
                "until": support::rfc3339(now - 3_000_000),
            })
        );
        assert_eq!(coverage["retention"]["usage_days"], 90);
        assert_eq!(coverage["retention"]["readings_days"], 45);
        assert_eq!(coverage["retention"]["enforcement"], "on_write");
        assert_eq!(coverage["retention"]["usage_overdue"], false);
        assert_eq!(coverage["retention"]["readings_overdue"], false);
        let readings = &coverage["readings"];
        assert_eq!(readings["evicted"], 5);
        assert_eq!(readings["dropped"], 4);
        assert_eq!(
            readings["gaps"],
            json!([
                {"kind": "evicted", "count": 5},
                {"kind": "dropped", "count": 4},
                {
                    "kind": "out_of_coverage",
                    "from": support::rfc3339(now - 3_600_000),
                    "to": support::rfc3339(now - 2_000_000),
                },
            ])
        );
        assert_eq!(
            readings["reportable"],
            json!({
                "since": support::rfc3339(now - 2_000_000),
                "until": support::rfc3339(now - 2_000_000),
            })
        );
        // A clean epoch marker reports no restart gap.
        support::set_meta(&store, "restart_gap", "0");
        let data = summary(&store, &period).expect("clean coverage report");
        let kinds: Vec<&str> = data["coverage"]["gaps"]
            .as_array()
            .expect("gap array")
            .iter()
            .filter_map(|gap| gap["kind"].as_str())
            .collect();
        assert!(!kinds.contains(&"restart"), "{kinds:?}");
    }

    #[test]
    fn a_period_outside_the_retained_span_reports_out_of_coverage() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        support::append(&store, vec![support::client(now - 60_000)], Vec::new());
        // A 2020-2021 request against 2026 rows reports the clamp instead of an
        // empty report that looks like no traffic.
        let requested_since = 1_609_459_200_000;
        let requested_until = 1_640_995_200_000;
        let data = summary(&store, &support::range(requested_since, requested_until))
            .expect("out of coverage report");
        assert_eq!(data["coverage"]["state"], "available");
        assert_eq!(data["coverage"]["reportable"], Value::Null);
        assert_eq!(
            data["coverage"]["gaps"],
            json!([{
                "kind": "out_of_coverage",
                "from": support::rfc3339(requested_since),
                "to": support::rfc3339(requested_until),
            }])
        );
        assert_eq!(
            data["period"]["since"],
            json!(support::rfc3339(requested_since)),
            "the requested period is reported unchanged"
        );
        assert_eq!(data["totals"]["requests"], 0);
        assert_eq!(data["totals"]["usage"]["input_tokens"], Value::Null);
    }

    #[test]
    fn reportable_clamps_to_the_retained_span_with_split_gaps() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let oldest = now - 900_000;
        let newest = now - 300_000;
        support::append(
            &store,
            vec![support::client(oldest), support::client(newest)],
            Vec::new(),
        );
        let period = support::range(now - 7_200_000, now - 100_000);
        let data = summary(&store, &period).expect("clamped report");
        assert_eq!(
            data["coverage"]["reportable"],
            json!({
                "since": support::rfc3339(oldest),
                "until": support::rfc3339(newest),
            })
        );
        assert_eq!(
            data["coverage"]["gaps"],
            json!([{
                "kind": "out_of_coverage",
                "from": support::rfc3339(now - 7_200_000),
                "to": support::rfc3339(oldest),
            }])
        );
        assert_eq!(data["totals"]["requests"], 2);

        // A future range is out of coverage on the right side only.
        let future = summary(&store, &support::range(now + 1_800_000, now + 3_600_000))
            .expect("future report");
        assert_eq!(future["coverage"]["reportable"], Value::Null);
        assert_eq!(
            future["coverage"]["gaps"],
            json!([{
                "kind": "out_of_coverage",
                "from": support::rfc3339(now + 1_800_000),
                "to": support::rfc3339(now + 3_600_000),
            }])
        );
        assert_eq!(future["totals"]["requests"], 0);
    }

    #[test]
    fn retention_reports_delayed_enforcement_per_horizon() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        support::append(
            &store,
            vec![support::client(now - 60_000)],
            vec![support::reading(
                now - 30_000,
                "p",
                "w",
                "consumed_percent",
                "10",
            )],
        );
        let period = support::range(now - 3_600_000, now);
        let data = summary(&store, &period).expect("retained report");
        assert_eq!(data["coverage"]["retention"]["usage_overdue"], false);
        assert_eq!(data["coverage"]["retention"]["readings_overdue"], false);

        // Pruning runs only on writes, so a read can observe an overdue usage
        // horizon while the readings horizon is still fresh.
        let old_usage = now - 91 * 24 * 60 * 60 * 1000;
        support::exec(
            &store,
            &format!("UPDATE usage_record SET membership_ms = {old_usage}"),
        );
        let data = summary(&store, &period).expect("overdue usage report");
        assert_eq!(data["coverage"]["retention"]["usage_overdue"], true);
        assert_eq!(data["coverage"]["retention"]["readings_overdue"], false);
        assert_eq!(data["coverage"]["retained_records"], 1);
        assert_eq!(data["coverage"]["reportable"], Value::Null);

        // A reading older than the 45-day readings horizon is reported too.
        let old_reading = now - 46 * 24 * 60 * 60 * 1000;
        support::exec(
            &store,
            &format!("UPDATE allowance_reading SET observed_at_ms = {old_reading}"),
        );
        let data = summary(&store, &period).expect("overdue readings report");
        assert_eq!(data["coverage"]["retention"]["usage_overdue"], true);
        assert_eq!(data["coverage"]["retention"]["readings_overdue"], true);
    }

    #[test]
    fn one_read_transaction_prevents_a_mixed_report_snapshot() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        support::append(&store, vec![support::client(now - 120_000)], Vec::new());
        let period = support::range(now - 3_600_000, now + 3_600_000);
        let snapshot = Snapshot::load(&store, &period);
        assert_eq!(snapshot.coverage.state, CoverageState::Available);
        assert!(
            !snapshot.conn().expect("open ledger").is_autocommit(),
            "the snapshot holds one explicit read transaction"
        );
        // A writer commits after the snapshot began.
        support::append(&store, vec![support::client(now - 60_000)], Vec::new());
        let data = summary_data(&snapshot).expect("snapshot report");
        assert_eq!(
            data["totals"]["requests"], 1,
            "the report sees only the committed snapshot"
        );
        assert_eq!(data["coverage"]["retained_records"], 1);
        drop(snapshot);
        let fresh = summary(&store, &period).expect("fresh report");
        assert_eq!(fresh["totals"]["requests"], 2);
        assert_eq!(fresh["coverage"]["retained_records"], 2);
    }

    #[test]
    fn exact_micro_usd_sums_do_not_round_through_floats() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        // This exact sum exceeds f64's exact-integer range (2^53), so a REAL
        // aggregate would render ...000000 instead of ...000001.
        let first_micros = 9_008_000_000_000_000i64;
        let second_micros = 1i64;
        let row = |at_ms: i64, micros: i64| {
            let mut row = support::client(at_ms);
            row.provider = Some("p".into());
            row.qualified_model = Some("p/m".into());
            row.input_tokens = Some(1_000_000_000_000);
            row.cost_micros = Some(micros);
            row.cost_state = "priced".into();
            row.price_source = Some("catalog".into());
            row
        };
        support::append(
            &store,
            vec![
                row(now - 600_000, first_micros),
                row(now - 500_000, second_micros),
            ],
            Vec::new(),
        );
        let data = summary(&store, &support::range(now - 3_600_000, now)).expect("exact report");
        assert_eq!(data["coverage"]["state"], "available");
        assert_eq!(
            data["totals"]["usage"]["input_tokens"],
            2_000_000_000_000i64
        );
        assert_eq!(data["costs"][0]["priced"], 2);
        assert_eq!(data["costs"][0]["estimate"], "9008000000.000001");
        assert_eq!(
            data["costs"][0]["estimate"],
            json!(micro_usd_wide(9_008_000_000_000_001i128))
        );
    }

    #[test]
    fn maximal_counters_and_estimates_stay_available_and_exact() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let cap = 1_000_000_000_000i64;
        let rows: Vec<_> = (0..20)
            .map(|index| {
                let mut row = support::client(now - 600_000 - index * 1_000);
                row.provider = Some("p".into());
                row.qualified_model = Some("p/m".into());
                row.target = Some("t".into());
                row.input_tokens = Some(cap);
                row.output_tokens = Some(cap);
                row.cache_read_tokens = Some(cap);
                row.cache_write_tokens = Some(cap);
                row.cost_micros = Some(i64::MAX);
                row.cost_state = "priced".into();
                row.price_source = Some("catalog".into());
                row
            })
            .collect();
        support::append(&store, rows, Vec::new());
        let period = support::range(now - 3_600_000, now);
        let data = summary(&store, &period).expect("overflow-safe report");
        assert_eq!(data["coverage"]["state"], "available");
        assert_eq!(data["totals"]["usage"]["input_tokens"], 20 * cap);
        assert_eq!(data["totals"]["usage"]["output_tokens"], 20 * cap);
        let exact = i128::from(i64::MAX) * 20;
        assert_eq!(data["costs"][0]["estimate"], micro_usd_wide(exact));
        // Insight ratios and multiplications must not overflow either.
        let insights = crate::usage::insights::report(&store, &period).expect("insights");
        let insight = insights["insights"]
            .as_array()
            .expect("insights")
            .iter()
            .find(|insight| insight["rule"] == "cost_concentration")
            .expect("concentration insight");
        assert_eq!(insight["evidence"]["values"]["share_percent"], 100.0);
        assert_eq!(
            insight["evidence"]["values"]["provider_estimate_usd"],
            json!(micro_usd_wide(exact))
        );
    }

    #[test]
    fn mixed_billing_scope_suppresses_the_cost_estimate() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let priced = |at_ms: i64, kind: &str, micros: i64| {
            let mut row = support::client(at_ms);
            row.provider = Some("mixed".into());
            row.qualified_model = Some("mixed/m".into());
            row.billing_kind = kind.into();
            row.cost_micros = Some(micros);
            row.cost_state = "priced".into();
            row.price_source = Some("catalog".into());
            row
        };
        support::append(
            &store,
            vec![
                priced(now - 600_000, "api", 1_000_000),
                priced(now - 500_000, "subscription", 2_000_000),
            ],
            Vec::new(),
        );
        let period = support::range(now - 3_600_000, now);
        let data = summary(&store, &period).expect("mixed scope report");
        let cost = &data["costs"][0];
        assert_eq!(cost["provider"], "mixed");
        assert_eq!(cost["billing_kinds"], json!(["api", "subscription"]));
        assert_eq!(cost["estimate"], Value::Null);
        assert_eq!(cost["estimate_reason"], "mixed_billing_scope");
        assert_eq!(cost["currency"], Value::Null);
        assert_eq!(cost["api_equivalent"], false);
        assert_eq!(cost["priced"], 2);
        assert_eq!(cost["unpriced"], 0);
        // The suppressed estimate never leaks into an insight.
        let insights =
            crate::usage::insights::report(&store, &period).expect("mixed scope insights");
        assert!(
            !insights["insights"]
                .as_array()
                .expect("insights")
                .iter()
                .any(|insight| {
                    insight["rule"] == "cost_concentration"
                        && insight["scope"]["provider"] == "mixed"
                })
        );
    }

    #[test]
    fn costs_carry_billing_labels_and_split_price_source_sets() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let mut subscription = support::client(now - 300_000);
        subscription.provider = Some("subscription".into());
        subscription.qualified_model = Some("subscription/m".into());
        subscription.billing_kind = "subscription".into();
        subscription.cost_micros = Some(2_000_000);
        subscription.cost_state = "priced".into();
        subscription.price_source = Some("catalog,override".into());
        let mut api = support::client(now - 200_000);
        api.provider = Some("api".into());
        api.qualified_model = Some("api/m".into());
        api.cost_micros = Some(1_000_000);
        api.cost_state = "priced".into();
        api.price_source = Some("override".into());
        support::append(&store, vec![subscription, api], Vec::new());
        let data = summary(&store, &support::range(now - 3_600_000, now)).expect("cost report");
        let costs = data["costs"].as_array().expect("cost entries");
        let api_row = costs
            .iter()
            .find(|cost| cost["provider"] == "api")
            .expect("api entry");
        assert_eq!(api_row["sources"], json!(["override"]));
        assert_eq!(api_row["billing_kinds"], json!(["api"]));
        assert_eq!(api_row["api_equivalent"], false);
        assert_eq!(api_row["estimate"], "1.000000");
        let subscription_row = costs
            .iter()
            .find(|cost| cost["provider"] == "subscription")
            .expect("subscription entry");
        assert_eq!(subscription_row["sources"], json!(["catalog", "override"]));
        assert_eq!(subscription_row["billing_kinds"], json!(["subscription"]));
        assert_eq!(subscription_row["api_equivalent"], true);
        assert_eq!(subscription_row["estimate"], "2.000000");

        // An unknown billing scope is not API-equivalent either.
        let mut unknown = support::client(now - 100_000);
        unknown.provider = Some("unknown".into());
        unknown.qualified_model = Some("unknown/m".into());
        unknown.billing_kind = "unknown".into();
        unknown.cost_micros = Some(3_000_000);
        unknown.cost_state = "priced".into();
        unknown.price_source = Some("catalog".into());
        support::append(&store, vec![unknown], Vec::new());
        let data = summary(&store, &support::range(now - 3_600_000, now)).expect("cost report");
        let unknown_row = data["costs"]
            .as_array()
            .expect("cost entries")
            .iter()
            .find(|cost| cost["provider"] == "unknown")
            .expect("unknown entry");
        assert_eq!(unknown_row["billing_kinds"], json!(["unknown"]));
        assert_eq!(unknown_row["api_equivalent"], false);
        assert_eq!(unknown_row["estimate"], "3.000000");
    }

    #[test]
    fn every_result_carries_the_required_disclosures() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let period = support::range(now - 3_600_000, now);
        for value in [
            summary(&store, &period).expect("summary"),
            breakdown(&store, &period, "provider").expect("breakdown"),
            crate::usage::insights::report(&store, &period).expect("insights"),
        ] {
            let disclosures = &value["disclosures"];
            assert!(
                disclosures["token_comparability"]
                    .as_str()
                    .expect("token comparability")
                    .contains("protocol-scoped")
            );
            let cached_input = disclosures["cached_input"].as_str().expect("cached input");
            assert!(
                cached_input.contains("never added to input"),
                "{cached_input}"
            );
            assert!(cached_input.contains("can overstate"), "{cached_input}");
            assert!(
                cached_input.contains("cached prompt tokens"),
                "{cached_input}"
            );
            assert!(
                disclosures["cost_basis"]
                    .as_str()
                    .expect("cost basis")
                    .contains("API-equivalent")
            );
            assert!(
                disclosures["probes"]
                    .as_str()
                    .expect("probes")
                    .contains("excluded from request")
            );
            assert_eq!(
                disclosures["excluded"],
                json!(["provider_reported_total_tokens", "foreign_counter_names"])
            );
        }
        let (_corrupt_directory, corrupt) = support::store();
        support::write_corrupt(&corrupt);
        let unavailable = summary(&corrupt, &period).expect("unavailable report");
        assert_eq!(unavailable["coverage"]["state"], "unavailable");
        assert!(unavailable["disclosures"]["cost_basis"].as_str().is_some());
    }

    #[test]
    fn report_reads_do_not_mutate_the_ledger() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let mut row = support::client(now - 60_000);
        row.duration_ms = Some(15);
        row.input_tokens = Some(12);
        support::append(
            &store,
            vec![row],
            vec![support::reading(
                now - 30_000,
                "p",
                "w",
                "consumed_percent",
                "10",
            )],
        );
        let path = support::db_path(&store);
        let before = std::fs::read(&path).expect("ledger bytes");
        let period = support::range(now - 3_600_000, now + 3_600_000);
        summary(&store, &period).expect("summary");
        breakdown(&store, &period, "provider").expect("breakdown");
        breakdown(&store, &period, "day").expect("day breakdown");
        let after = std::fs::read(&path).expect("ledger bytes");
        assert_eq!(before, after, "reads leave the ledger bytes unchanged");
    }

    #[test]
    fn clear_dry_run_reports_counts_without_touching_the_ledger() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        support::append(
            &store,
            vec![
                support::client(now - 120_000),
                support::client(now - 60_000),
            ],
            vec![support::reading(
                now - 30_000,
                "p",
                "w",
                "consumed_percent",
                "10",
            )],
        );
        let path = support::db_path(&store);
        let before = std::fs::read(&path).expect("ledger bytes");
        let data = clear(&store, true).expect("dry run");
        assert_eq!(data["clear"]["dry_run"], true);
        assert_eq!(data["clear"]["executed_at"], Value::Null);
        assert_eq!(data["clear"]["removed"]["usage_records"], 2);
        assert_eq!(data["clear"]["removed"]["allowance_readings"], 1);
        assert_eq!(data["clear"]["retained"]["usage_records"], 0);
        assert_eq!(data["clear"]["retained"]["allowance_readings"], 0);
        assert_eq!(data["coverage"]["state"], "available");
        assert_eq!(data["coverage"]["retained_records"], 2);
        assert_eq!(data["coverage"]["readings"]["retained_records"], 1);
        assert!(data.get("period").is_none(), "clear reports coverage only");
        let after = std::fs::read(&path).expect("ledger bytes");
        assert_eq!(before, after, "a dry run leaves the ledger bytes unchanged");
    }

    #[test]
    fn clear_execution_removes_rows_and_starts_a_new_epoch() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        support::append(
            &store,
            vec![
                support::client(now - 120_000),
                support::client(now - 60_000),
            ],
            vec![support::reading(
                now - 30_000,
                "p",
                "w",
                "consumed_percent",
                "10",
            )],
        );
        support::set_meta(&store, "evicted", "5");
        support::set_meta(&store, "dropped", "4");
        let data = clear(&store, false).expect("executed clear");
        assert_eq!(data["clear"]["dry_run"], false);
        assert!(data["clear"]["executed_at"].as_str().is_some());
        assert_eq!(data["clear"]["removed"]["usage_records"], 2);
        assert_eq!(data["clear"]["removed"]["allowance_readings"], 1);
        assert_eq!(data["clear"]["retained"]["usage_records"], 0);
        assert_eq!(data["clear"]["retained"]["allowance_readings"], 0);
        let coverage = &data["coverage"];
        assert_eq!(coverage["state"], "available");
        assert_eq!(coverage["retained_records"], 0);
        assert_eq!(coverage["readings"]["retained_records"], 0);
        assert_eq!(coverage["oldest"], Value::Null);
        assert_eq!(coverage["evicted"], 0);
        assert_eq!(coverage["dropped"], 0);
        assert!(coverage["cleared_at"].as_str().is_some());
        assert_eq!(
            coverage["gaps"],
            json!([{
                "kind": "cleared",
                "at": coverage["cleared_at"].clone(),
            }])
        );
        assert_eq!(coverage["retention"]["usage_days"], 90);
        assert_eq!(coverage["retention"]["usage_overdue"], false);
        assert_eq!(coverage["retention"]["readings_overdue"], false);
        let after = summary(&store, &support::range(now - 3_600_000, now + 3_600_000))
            .expect("post clear summary");
        assert_eq!(after["coverage"]["retained_records"], 0);
        assert_eq!(after["totals"]["requests"], 0);
    }

    #[test]
    fn clear_on_an_absent_store_succeeds_without_creating_the_ledger() {
        let (_directory, store) = support::store();
        for dry_run in [true, false] {
            let data = clear(&store, dry_run).expect("absent clear");
            assert_eq!(data["clear"]["dry_run"], dry_run);
            assert_eq!(data["clear"]["executed_at"], Value::Null);
            assert_eq!(data["clear"]["removed"]["usage_records"], 0);
            assert_eq!(data["clear"]["removed"]["allowance_readings"], 0);
            assert_eq!(data["clear"]["retained"]["usage_records"], 0);
            assert_eq!(data["coverage"]["state"], "absent");
            assert!(!support::db_path(&store).exists());
        }
    }

    #[test]
    fn membership_basis_distinguishes_admission_receipt_and_mixed_rows() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let mut receipt = support::client(now - 60_000);
        receipt.admitted_at_ms = None;
        support::append(&store, vec![receipt], Vec::new());
        let period = support::range(now - 3_600_000, now + 3_600_000);
        let data = summary(&store, &period).expect("receipt basis report");
        assert_eq!(data["totals"]["membership_basis"], "receipt");
        support::append(&store, vec![support::client(now - 30_000)], Vec::new());
        let data = summary(&store, &period).expect("mixed basis report");
        assert_eq!(data["totals"]["membership_basis"], "mixed");
    }

    #[test]
    fn model_breakdown_rows_carry_provider_scoped_cost() {
        let (_directory, store) = support::store();
        let now = support::now_ms();
        let priced = |model: &str, micros: i64| {
            let mut row = support::client(now - 60_000);
            row.provider = Some("p".into());
            row.qualified_model = Some(model.into());
            row.cost_micros = Some(micros);
            row.cost_state = "priced".into();
            row.price_source = Some("catalog".into());
            row
        };
        support::append(
            &store,
            vec![
                priced("p/one", 1_000_000),
                priced("p/two", 250_000),
                priced("p/two", 250_000),
            ],
            Vec::new(),
        );
        let data = breakdown(
            &store,
            &support::range(now - 3_600_000, now + 3_600_000),
            "model",
        )
        .expect("model breakdown");
        let one = row_for(&data["rows"], Some("p/one")).expect("one model row");
        assert_eq!(one["cost"]["provider"], "p");
        assert_eq!(one["cost"]["estimate"], "1.000000");
        assert_eq!(one["cost_reason"], Value::Null);
        let two = row_for(&data["rows"], Some("p/two")).expect("two model row");
        assert_eq!(two["requests"], 2);
        assert_eq!(two["cost"]["estimate"], "0.500000");
    }

    #[tokio::test]
    async fn usage_operations_dispatch_through_the_local_entrypoint() {
        let (_directory, store) = support::store();
        let request = crate::Request {
            command: vec!["usage".into(), "summary".into()],
            operands: Vec::new(),
            arguments: std::collections::BTreeMap::new(),
            dry_run: false,
            if_revision: None,
        };
        let data = crate::operations::test_dispatch(&store, &request)
            .await
            .expect("usage summary dispatch");
        assert_eq!(data["coverage"]["state"], "absent");
        let mut insights = request.clone();
        insights.command = vec!["usage".into(), "insights".into()];
        let data = crate::operations::test_dispatch(&store, &insights)
            .await
            .expect("usage insights dispatch");
        assert_eq!(data["coverage"]["state"], "absent");
        let mut breakdown = request.clone();
        breakdown.command = vec!["usage".into(), "breakdown".into()];
        breakdown.arguments.insert("by".into(), vec!["day".into()]);
        let data = crate::operations::test_dispatch(&store, &breakdown)
            .await
            .expect("usage breakdown dispatch");
        assert_eq!(data["coverage"]["state"], "absent");
        let mut clear = request.clone();
        clear.command = vec!["usage".into(), "clear".into()];
        clear.dry_run = true;
        let data = crate::operations::test_dispatch(&store, &clear)
            .await
            .expect("usage clear dispatch");
        assert_eq!(data["clear"]["dry_run"], true);
    }

    #[test]
    fn clear_blocks_before_deletion_on_corrupt_or_unsupported_stores() {
        for (write, expected) in [
            (
                support::write_corrupt as fn(&crate::storage::Store),
                "usage_store_unavailable",
            ),
            (
                support::write_unsupported as fn(&crate::storage::Store),
                "storage_version_unsupported",
            ),
        ] {
            let (_directory, store) = support::store();
            write(&store);
            let path = support::db_path(&store);
            let before = std::fs::read(&path).expect("ledger bytes");
            let error = clear(&store, false).expect_err("blocked clear");
            assert_eq!(error.code, expected);
            let after = std::fs::read(&path).expect("ledger bytes");
            assert_eq!(before, after, "a blocked clear deletes nothing");
        }
    }
}
