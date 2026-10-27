use crate::{Error, Result};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use num_bigint::BigUint;

pub(crate) fn instant(text: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .map(|v| v.with_timezone(&Utc))
        .map_err(|_| {
            Error::new(
                "invalid_instant",
                "Expected an explicit-offset RFC3339 instant",
            )
        })
}
pub(crate) fn timestamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

/// RFC3339 fractions have no precision cap. Chrono validates calendars/offsets;
/// this decimal tail prevents silently rounding a publication or rule boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExactInstant {
    seconds: i64,
    leap: bool,
    fraction: String,
}
impl ExactInstant {
    pub(crate) fn parse(text: &str) -> Result<Self> {
        let at = instant(text)?;
        let fraction = if let Some(dot) = text.find('.') {
            let tail = &text[dot + 1..];
            let end = tail
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(tail.len());
            if end == 0 {
                return Err(Error::new("invalid_instant", "Empty fractional second"));
            }
            tail[..end].trim_end_matches('0').to_owned()
        } else {
            String::new()
        };
        Ok(Self {
            seconds: at.timestamp(),
            leap: at.timestamp_subsec_nanos() >= 1_000_000_000,
            fraction,
        })
    }
    pub(crate) fn from_chrono(at: DateTime<Utc>) -> Self {
        Self::parse(&timestamp(at)).expect("UTC timestamp")
    }
    pub(crate) fn canonical(&self) -> String {
        let whole = DateTime::<Utc>::from_timestamp(
            self.seconds,
            if self.leap { 1_000_000_000 } else { 0 },
        )
        .expect("validated timestamp")
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
        if self.fraction.is_empty() {
            format!("{whole}Z")
        } else {
            format!("{whole}.{}Z", self.fraction)
        }
    }
    pub(crate) fn add_seconds(&self, seconds: i64) -> Option<Self> {
        let at = DateTime::<Utc>::from_timestamp(
            self.seconds,
            if self.leap { 1_000_000_000 } else { 0 },
        )?;
        let target = at.checked_add_signed(Duration::try_seconds(seconds)?)?;
        Some(Self {
            seconds: target.timestamp(),
            leap: target.timestamp_subsec_nanos() >= 1_000_000_000,
            fraction: self.fraction.clone(),
        })
    }
}
impl Ord for ExactInstant {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.seconds
            .cmp(&other.seconds)
            .then_with(|| self.leap.cmp(&other.leap))
            .then_with(|| {
                let width = self.fraction.len().max(other.fraction.len());
                self.fraction
                    .bytes()
                    .chain(std::iter::repeat(b'0'))
                    .take(width)
                    .cmp(
                        other
                            .fraction
                            .bytes()
                            .chain(std::iter::repeat(b'0'))
                            .take(width),
                    )
            })
    }
}
impl PartialOrd for ExactInstant {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
pub(crate) fn within_horizon(reset: &str, at: &ExactInstant, seconds: i64) -> Result<bool> {
    let reset = ExactInstant::parse(reset)?;
    Ok(&reset > at && at.add_seconds(seconds).is_none_or(|end| reset <= end))
}
pub(super) fn after_seconds(at: DateTime<Utc>, seconds: i64) -> DateTime<Utc> {
    Duration::try_seconds(seconds)
        .and_then(|duration| at.checked_add_signed(duration))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

/// Arbitrary-precision base-ten quantity. No division or binary floating point is used.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Decimal {
    pub(super) coefficient: BigUint,
    scale: usize,
    pub(super) text: String,
}
impl Decimal {
    pub(crate) fn parse(text: &str) -> Result<Self> {
        let mut pieces = text.split('.');
        let integer = pieces.next().unwrap_or("");
        let fraction = pieces.next();
        if integer.is_empty()
            || !integer.bytes().all(|c| c.is_ascii_digit())
            || fraction.is_some_and(|s| s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()))
            || pieces.next().is_some()
        {
            return Err(Error::new(
                "invalid_decimal",
                "Expected an unsigned finite base-ten decimal string",
            ));
        }
        let integer = integer.trim_start_matches('0');
        let integer = if integer.is_empty() { "0" } else { integer };
        let fraction = fraction.unwrap_or("").trim_end_matches('0');
        let canonical = if fraction.is_empty() {
            integer.to_owned()
        } else {
            format!("{integer}.{fraction}")
        };
        let coefficient = BigUint::parse_bytes(format!("{integer}{fraction}").as_bytes(), 10)
            .ok_or_else(|| Error::new("invalid_decimal", "Invalid decimal quantity"))?;
        Ok(Self {
            coefficient,
            scale: fraction.len(),
            text: canonical,
        })
    }
    fn ten_pow(mut n: usize) -> BigUint {
        let mut result = BigUint::from(1u8);
        let mut base = BigUint::from(10u8);
        while n != 0 {
            if n & 1 != 0 {
                result *= &base;
            }
            n >>= 1;
            if n != 0 {
                base = &base * &base;
            }
        }
        result
    }
    pub(super) fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let scale = self.scale.max(other.scale);
        (&self.coefficient * Self::ten_pow(scale - self.scale))
            .cmp(&(&other.coefficient * Self::ten_pow(scale - other.scale)))
    }
    pub(super) fn product(&self, other: &Self) -> Self {
        Self {
            coefficient: &self.coefficient * &other.coefficient,
            scale: self.scale + other.scale,
            text: String::new(),
        }
    }
    pub(crate) fn percent(text: &str) -> Result<Self> {
        let result = Self::parse(text)?;
        if result.cmp(&Self::parse("100")?) == std::cmp::Ordering::Greater {
            return Err(Error::new(
                "invalid_decimal",
                "Percentage must be between 0 and 100",
            ));
        }
        Ok(result)
    }
}
