//! Durable, content-free usage ledger, reports, and deterministic insights.
//!
//! The gateway records typed usage facts for terminal requests; reports are
//! read-time projections over that ledger. See `specs/usage-analytics/spec.md`.
pub(crate) mod capture;
pub(crate) mod insights;
pub(crate) mod ledger;
pub(crate) mod readings;
pub(crate) mod report;

use crate::{Error, Request, Result, storage::Store};
use chrono::{DateTime, Datelike, Duration, Local, LocalResult, TimeZone, Utc};
use serde_json::Value;

pub(crate) use ledger::UsageWriter;

/// One gateway request's typed, content-free usage facts.
#[derive(Debug, Clone, Default)]
pub(crate) struct UsageRow {
    pub received_at_ms: i64,
    pub admitted_at_ms: Option<i64>,
    pub membership_ms: i64,
    pub origin: String,
    pub requested_model: Option<String>,
    pub model_kind: String,
    pub provider: Option<String>,
    pub qualified_model: Option<String>,
    pub target: Option<String>,
    pub agent_provider: Option<String>,
    pub protocol: Option<String>,
    pub outcome: String,
    pub stage: String,
    pub reason_code: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub usage_state: String,
    pub duration_ms: Option<i64>,
    pub cost_micros: Option<i64>,
    pub price_source: Option<String>,
    pub billing_kind: String,
    pub cost_state: String,
}

/// One dated provider allowance reading.
#[derive(Debug, Clone, Default)]
pub(crate) struct ReadingRow {
    pub observed_at_ms: i64,
    pub received_at_ms: i64,
    pub provider: String,
    pub binding: String,
    pub window: String,
    pub basis: String,
    pub percent: Option<String>,
    pub counters: Option<String>,
    pub unit: Option<String>,
    pub source: String,
    pub access_state: Option<String>,
    pub reset: Value,
    pub freshness: Option<String>,
}

pub(crate) enum WriteJob {
    Usage(UsageRow),
    Reading(ReadingRow),
}

/// One resolved report period. `all` leaves `since` unset until coverage is read.
#[derive(Debug, Clone)]
pub(crate) struct Period {
    pub kind: String,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub timezone: String,
}

pub(crate) const DIMENSIONS: [&str; 8] = [
    "provider",
    "model",
    "requested",
    "target",
    "protocol",
    "outcome",
    "origin",
    "day",
];

pub(crate) async fn handle(store: &Store, r: &Request) -> Result<Value> {
    let path = r.command.join(" ");
    let period = resolve_period(r)?;
    match path.as_str() {
        "usage summary" => report::summary(store, &period),
        "usage breakdown" => report::breakdown(store, &period, dimension(r)?),
        "usage insights" => insights::report(store, &period),
        "usage clear" => report::clear(store, r.dry_run),
        _ => Err(Error::new(
            "unsupported_operation",
            "Unknown usage operation",
        )),
    }
}

fn arg<'a>(r: &'a Request, name: &str) -> Option<&'a str> {
    r.arguments
        .get(name)
        .and_then(|values| values.first())
        .map(String::as_str)
}

fn dimension(r: &Request) -> Result<&'static str> {
    let Some(value) = arg(r, "by") else {
        return Err(Error::new("invalid_arguments", "Missing breakdown dimension").field("by"));
    };
    DIMENSIONS
        .into_iter()
        .find(|known| *known == value)
        .ok_or_else(|| {
            Error::new("invalid_arguments", "Unsupported breakdown dimension").field("by")
        })
}

/// Resolve the requested period from command arguments.
///
/// Day-based windows start at the local midnight that begins the trailing
/// calendar-day window and end at the read instant. `all` has no start until
/// the ledger coverage is read. An explicit range is `[since, until)`.
pub(crate) fn resolve_period(r: &Request) -> Result<Period> {
    let period = arg(r, "period");
    let since = arg(r, "since");
    let until = arg(r, "until");
    let timezone = local_timezone_name();
    if since.is_some() || until.is_some() {
        if period.is_some() {
            return Err(Error::new(
                "invalid_arguments",
                "Period and explicit range are mutually exclusive",
            )
            .field("period"));
        }
        let (Some(since), Some(until)) = (since, until) else {
            return Err(Error::new(
                "invalid_arguments",
                "An explicit range requires both since and until",
            ));
        };
        let since = parse_instant(since)?;
        let until = parse_instant(until)?;
        if since >= until {
            return Err(Error::new(
                "invalid_arguments",
                "The since bound must precede the until bound",
            ));
        }
        return Ok(Period {
            kind: "range".into(),
            since: Some(since),
            until: Some(until),
            timezone,
        });
    }
    let now = Utc::now();
    let kind = period.unwrap_or("today");
    let days = match kind {
        "today" => Some(1),
        "7d" => Some(7),
        "30d" => Some(30),
        "90d" => Some(90),
        "all" => None,
        _ => {
            return Err(Error::new("invalid_arguments", "Unsupported period").field("period"));
        }
    };
    Ok(Period {
        kind: kind.into(),
        since: days.map(|days| {
            let start = local_midnight(now);
            let start_local = start.with_timezone(&Local);
            let date = start_local.date_naive() - Duration::days(days - 1);
            local_midnight_for(date)
        }),
        until: Some(now),
        timezone,
    })
}

fn parse_instant(text: &str) -> Result<DateTime<Utc>> {
    let at = DateTime::parse_from_rfc3339(text)
        .map_err(|_| Error::new("invalid_instant", "Expected an RFC 3339 instant"))?;
    Ok(at.with_timezone(&Utc))
}

fn local_midnight(at: DateTime<Utc>) -> DateTime<Utc> {
    let local = at.with_timezone(&Local);
    local_midnight_for(local.date_naive())
}

fn local_midnight_for(date: chrono::NaiveDate) -> DateTime<Utc> {
    let naive = date.and_hms_opt(0, 0, 0).expect("valid midnight");
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(at) | LocalResult::Ambiguous(at, _) => at.with_timezone(&Utc),
        LocalResult::None => {
            // A daylight-saving jump may skip local midnight. Use the first
            // representable instant of that local day.
            let mut hour = 1;
            while hour < 24 {
                if let Some(candidate) = naive.checked_add_signed(Duration::hours(hour))
                    && let LocalResult::Single(at) | LocalResult::Ambiguous(at, _) =
                        Local.from_local_datetime(&candidate)
                {
                    return at.with_timezone(&Utc);
                }
                hour += 1;
            }
            Local.from_utc_datetime(&naive).with_timezone(&Utc)
        }
    }
}

fn local_timezone_name() -> String {
    if let Ok(tz) = std::env::var("TZ")
        && !tz.trim().is_empty()
    {
        return tz.trim().to_owned();
    }
    #[cfg(unix)]
    if let Ok(path) = std::fs::read_link("/etc/localtime")
        && let Some(name) = path.file_name().and_then(|name| name.to_str())
    {
        return name.to_owned();
    }
    "local".into()
}

/// Format an exact micro-USD integer as a decimal string with six places.
pub(crate) fn micro_usd(micros: i64) -> String {
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

/// The local calendar day (`YYYY-MM-DD`) containing an instant in the report
/// time zone.
pub(crate) fn local_day(at_ms: i64, _timezone: &str) -> String {
    let at = DateTime::<Utc>::from_timestamp_millis(at_ms).unwrap_or_else(Utc::now);
    at.with_timezone(&Local)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

/// Current local calendar year, used to qualify day labels only when needed.
#[allow(dead_code)]
fn current_year() -> i32 {
    Utc::now().with_timezone(&Local).year()
}
