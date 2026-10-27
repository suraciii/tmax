use crate::{
    Error, Result,
    config::{Model, TimeWindow},
};
use chrono::{
    DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, Offset,
    TimeZone, Utc,
};
use chrono_tz::Tz;
use std::collections::BTreeSet;

pub(super) fn invalid(message: &str) -> Error {
    Error::new("invalid_calendar", message)
}
fn date(value: &toml::value::Date) -> Result<NaiveDate> {
    NaiveDate::from_ymd_opt(
        i32::from(value.year),
        u32::from(value.month),
        u32::from(value.day),
    )
    .ok_or_else(|| invalid("Invalid local calendar date"))
}
fn time(text: &str) -> Result<NaiveTime> {
    let bytes = text.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || ![bytes[0], bytes[1], bytes[3], bytes[4]]
            .iter()
            .all(u8::is_ascii_digit)
    {
        return Err(invalid("Local time requires 24-hour HH:MM"));
    }
    NaiveTime::from_hms_opt(
        text[..2].parse().unwrap_or(24),
        text[3..].parse().unwrap_or(60),
        0,
    )
    .ok_or_else(|| invalid("Local time requires 24-hour HH:MM"))
}
fn weekday(text: &str) -> Option<u32> {
    ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
        .iter()
        .position(|v| *v == text)
        .map(|v| v as u32)
}

#[derive(Clone)]
pub(super) struct Window {
    weekdays: Option<BTreeSet<u32>>,
    dates: Option<BTreeSet<NaiveDate>>,
    start: NaiveTime,
    end: NaiveTime,
    all_day: bool,
    from: Option<NaiveDate>,
    until: Option<NaiveDate>,
}
impl Window {
    pub(super) fn parse(window: &TimeWindow) -> Result<Self> {
        let weekdays = window
            .weekdays
            .as_ref()
            .map(|days| {
                let values: Result<BTreeSet<_>> = days
                    .iter()
                    .map(|s| weekday(s).ok_or_else(|| invalid("Unknown weekday")))
                    .collect();
                let values = values?;
                if values.is_empty() || values.len() != days.len() {
                    return Err(invalid("Weekdays must be nonempty and unique"));
                }
                Ok(values)
            })
            .transpose()?;
        let dates = window
            .dates
            .as_ref()
            .map(|days| {
                let values: Result<BTreeSet<_>> = days.iter().map(date).collect();
                let values = values?;
                if values.is_empty() || values.len() != days.len() {
                    return Err(invalid("Dates must be nonempty and unique"));
                }
                Ok(values)
            })
            .transpose()?;
        if weekdays.is_some() == dates.is_some() {
            return Err(invalid("Choose exactly one of weekdays or explicit dates"));
        }
        let all_day = window.all_day == Some(true);
        let midnight = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        let (start, end) = if all_day {
            if window.start.is_some() || window.end.is_some() {
                return Err(invalid("All-day windows forbid start and end"));
            }
            (midnight, midnight)
        } else {
            if window.all_day.is_some() {
                return Err(invalid(
                    "A timed window omits all_day; all_day=false is invalid",
                ));
            }
            let start = time(
                window
                    .start
                    .as_deref()
                    .ok_or_else(|| invalid("Timed window requires start"))?,
            )?;
            let end = time(
                window
                    .end
                    .as_deref()
                    .ok_or_else(|| invalid("Timed window requires end"))?,
            )?;
            if start == end {
                return Err(invalid(
                    "Start and end must differ; use explicit all_day=true",
                ));
            }
            (start, end)
        };
        let from = window.effective_from.as_ref().map(date).transpose()?;
        let until = window.effective_until.as_ref().map(date).transpose()?;
        if from.zip(until).is_some_and(|(from, until)| until <= from) {
            return Err(invalid("Effective end is exclusive and must follow start"));
        }
        Ok(Self {
            weekdays,
            dates,
            start,
            end,
            all_day,
            from,
            until,
        })
    }
    fn selected(&self, day: NaiveDate) -> bool {
        self.weekdays
            .as_ref()
            .is_some_and(|days| days.contains(&day.weekday().num_days_from_monday()))
            || self.dates.as_ref().is_some_and(|days| days.contains(&day))
    }
    pub(super) fn contains(&self, local: NaiveDateTime) -> bool {
        let day = local.date();
        let t = local.time();
        if self.from.is_some_and(|from| day < from) || self.until.is_some_and(|until| day >= until)
        {
            return false;
        }
        if self.all_day {
            return self.selected(day);
        }
        if self.start < self.end {
            return self.selected(day) && t >= self.start && t < self.end;
        }
        self.selected(day) && t >= self.start
            || day
                .pred_opt()
                .is_some_and(|previous| self.selected(previous) && t < self.end)
    }
    fn segments(&self) -> Vec<(u32, NaiveTime, Option<NaiveTime>)> {
        let mut segments = Vec::new();
        if let Some(days) = &self.weekdays {
            for day in days {
                if self.all_day {
                    segments.push((*day, self.start, None));
                } else if self.start < self.end {
                    segments.push((*day, self.start, Some(self.end)));
                } else {
                    segments.push((*day, self.start, None));
                    if self.end != NaiveTime::MIN {
                        segments.push(((*day + 1) % 7, NaiveTime::MIN, Some(self.end)));
                    }
                }
            }
        }
        segments
    }
    fn intervals(&self) -> Vec<(NaiveDateTime, NaiveDateTime)> {
        let mut result = Vec::new();
        if let Some(days) = &self.dates {
            for day in days {
                let start = day.and_time(self.start);
                let end = if self.all_day || self.end < self.start {
                    day.succ_opt().map(|d| d.and_time(self.end))
                } else {
                    Some(day.and_time(self.end))
                };
                if let Some(end) = end {
                    let start = self
                        .from
                        .map(|d| d.and_time(NaiveTime::MIN))
                        .map_or(start, |from| start.max(from));
                    let end = self
                        .until
                        .map(|d| d.and_time(NaiveTime::MIN))
                        .map_or(end, |until| end.min(until));
                    if start < end {
                        result.push((start, end));
                    }
                }
            }
        }
        result
    }
    pub(super) fn overlaps(&self, other: &Self, zone: Tz) -> bool {
        if self.weekdays.is_some() != other.weekdays.is_some() {
            return false;
        }
        if self.dates.is_some() {
            let other_intervals = other.intervals();
            return self.intervals().iter().any(|(s, e)| {
                other_intervals
                    .iter()
                    .any(|(os, oe)| interval_has_instant(zone, (*s).max(*os), (*e).min(*oe)))
            });
        }
        let from = self
            .from
            .into_iter()
            .chain(other.from)
            .max()
            .unwrap_or(NaiveDate::MIN);
        let until = self.until.into_iter().chain(other.until).min();
        if until.is_some_and(|until| from >= until) {
            return false;
        }
        for (day, start, end) in self.segments() {
            for (oday, ostart, oend) in other.segments() {
                if day != oday
                    || end.is_some_and(|end| ostart >= end)
                    || oend.is_some_and(|end| start >= end)
                {
                    continue;
                }
                let distance = (day + 7 - from.weekday().num_days_from_monday()) % 7;
                let mut candidate = from.checked_add_signed(Duration::days(i64::from(distance)));
                while let Some(day) = candidate.filter(|day| until.is_none_or(|until| *day < until))
                {
                    let local_start = day.and_time(start.max(ostart));
                    let end = match (end, oend) {
                        (Some(a), Some(b)) => Some(day.and_time(a.min(b))),
                        (Some(a), None) | (None, Some(a)) => Some(day.and_time(a)),
                        (None, None) => day.succ_opt().map(|day| day.and_time(NaiveTime::MIN)),
                    };
                    if end.is_some_and(|end| interval_has_instant(zone, local_start, end)) {
                        return true;
                    }
                    candidate = day.checked_add_signed(Duration::days(7));
                }
            }
        }
        false
    }
    fn candidate_dates(&self, local: NaiveDate) -> BTreeSet<NaiveDate> {
        let mut dates = BTreeSet::new();
        for offset in -1..=8 {
            if let Some(day) = local.checked_add_signed(Duration::days(offset)) {
                dates.insert(day);
            }
        }
        for bound in self.from.into_iter().chain(self.until) {
            for offset in -1..=8 {
                if let Some(day) = bound.checked_add_signed(Duration::days(offset))
                    && day >= local
                {
                    dates.insert(day);
                }
            }
        }
        if let Some(explicit) = &self.dates {
            for day in explicit {
                if *day >= local.pred_opt().unwrap_or(local) {
                    dates.insert(*day);
                    if let Some(next) = day.succ_opt() {
                        dates.insert(next);
                    }
                }
            }
        }
        dates
    }
    pub(super) fn boundaries(&self, zone: Tz, at: DateTime<Utc>) -> Vec<DateTime<Utc>> {
        let mut candidates = BTreeSet::new();
        for day in self.candidate_dates(at.with_timezone(&zone).date_naive()) {
            for t in [NaiveTime::MIN, self.start, self.end] {
                let local = day.and_time(t);
                candidates.extend(local_instants(zone, local).filter(|utc| *utc > at));
                // If a rollback changes membership, a selector edge lies in its
                // repeated local range. Its two mapped instants bracket the
                // adjacent offset transition exactly; no sampled clock scan.
                if let LocalResult::Ambiguous(a, b) = zone.from_local_datetime(&local) {
                    let (left, right) = if a < b { (a, b) } else { (b, a) };
                    let offset = left.offset().fix();
                    let mut low = left.timestamp();
                    let mut high = right.timestamp();
                    while high - low > 1 {
                        let middle = low + (high - low) / 2;
                        let point = DateTime::from_timestamp(middle, 0).unwrap();
                        if point.with_timezone(&zone).offset().fix() == offset {
                            low = middle;
                        } else {
                            high = middle;
                        }
                    }
                    if let Some(transition) =
                        DateTime::from_timestamp(high, 0).filter(|transition| *transition > at)
                    {
                        candidates.insert(transition);
                    }
                }
            }
        }
        candidates
            .into_iter()
            .filter(|boundary| {
                let before = boundary
                    .checked_sub_signed(Duration::nanoseconds(1))
                    .unwrap_or(*boundary);
                self.contains(before.with_timezone(&zone).naive_local())
                    != self.contains(boundary.with_timezone(&zone).naive_local())
            })
            .collect()
    }
}
fn local_instants(zone: Tz, local: NaiveDateTime) -> impl Iterator<Item = DateTime<Utc>> {
    let points = match zone.from_local_datetime(&local) {
        LocalResult::Single(at) => [Some(at.with_timezone(&Utc)), None],
        LocalResult::Ambiguous(a, b) => [Some(a.with_timezone(&Utc)), Some(b.with_timezone(&Utc))],
        LocalResult::None => [
            chrono_tz::GapInfo::new(&local, &zone)
                .and_then(|gap| gap.end)
                .map(|at| at.with_timezone(&Utc)),
            None,
        ],
    };
    points.into_iter().flatten()
}

fn interval_has_instant(zone: Tz, start: NaiveDateTime, end: NaiveDateTime) -> bool {
    start < end && local_instants(zone, start).any(|at| at.with_timezone(&zone).naive_local() < end)
}
pub(super) fn calendar_at(
    model: &Model,
    windows: &[Window],
    local: NaiveDateTime,
) -> (u8, Option<usize>, i64) {
    let mut fallback = (0, None, model.priority);
    for (index, window) in windows.iter().enumerate() {
        let kind = if window.dates.is_some() { 2 } else { 1 };
        if kind > fallback.0 && window.contains(local) {
            fallback = (kind, Some(index), model.schedule[index].priority);
        }
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::{Window, local_instants};
    use crate::{config::TimeWindow, inputs, routing::test_support::weekly};
    use chrono::NaiveDate;
    use chrono_tz::Tz;

    #[test]
    fn nonexistent_calendar_intersections_do_not_overlap_real_instants() {
        let zone: Tz = "America/New_York".parse().unwrap();
        let day = toml::value::Date {
            year: 2026,
            month: 3,
            day: 8,
        };
        let a = TimeWindow {
            dates: Some(vec![day]),
            start: Some("02:10".into()),
            end: Some("02:40".into()),
            ..Default::default()
        };
        let mut b = a.clone();
        b.start = Some("02:20".into());
        b.end = Some("03:30".into());
        assert!(
            !Window::parse(&a)
                .unwrap()
                .overlaps(&Window::parse(&b).unwrap(), zone)
        );
        let mut real = a;
        real.end = Some("03:10".into());
        assert!(
            Window::parse(&real)
                .unwrap()
                .overlaps(&Window::parse(&b).unwrap(), zone)
        );
        let mut bounded = weekly("sun", "02:10", "02:40");
        bounded.effective_from = Some(day);
        bounded.effective_until = Some(toml::value::Date {
            year: 2026,
            month: 3,
            day: 9,
        });
        let window = Window::parse(&bounded).unwrap();
        assert!(!window.overlaps(&window, zone));
        bounded.effective_until = Some(toml::value::Date {
            year: 2026,
            month: 3,
            day: 16,
        });
        let window = Window::parse(&bounded).unwrap();
        assert!(window.overlaps(&window, zone));
    }
    #[test]
    fn rollback_itself_is_a_real_gate_boundary() {
        let zone: Tz = "America/New_York".parse().unwrap();
        let window = Window::parse(&weekly("sun", "01:30", "02:30")).unwrap();
        let boundaries = window.boundaries(zone, inputs::instant("2026-11-01T05:45:00Z").unwrap());
        assert_eq!(
            boundaries[0],
            inputs::instant("2026-11-01T06:00:00Z").unwrap()
        );
        assert_eq!(
            boundaries[1],
            inputs::instant("2026-11-01T06:30:00Z").unwrap()
        );
    }
    #[test]
    fn overnight_belongs_to_start_day_and_clips_at_effective_end() {
        let mut window = weekly("fri", "22:00", "02:00");
        window.effective_until = Some(toml::value::Date {
            year: 2026,
            month: 10,
            day: 24,
        });
        let window = Window::parse(&window).unwrap();
        assert!(
            window.contains(
                NaiveDate::from_ymd_opt(2026, 10, 23)
                    .unwrap()
                    .and_hms_opt(23, 0, 0)
                    .unwrap()
            )
        );
        assert!(
            !window.contains(
                NaiveDate::from_ymd_opt(2026, 10, 24)
                    .unwrap()
                    .and_hms_opt(1, 0, 0)
                    .unwrap()
            )
        );
    }
    #[test]
    fn overlap_includes_spillover_but_not_touching_edges() {
        let a = Window::parse(&weekly("fri", "22:00", "02:00")).unwrap();
        let b = Window::parse(&weekly("sat", "01:00", "03:00")).unwrap();
        assert!(a.overlaps(&b, chrono_tz::UTC));
        let b = Window::parse(&weekly("sat", "02:00", "03:00")).unwrap();
        assert!(!a.overlaps(&b, chrono_tz::UTC));
    }
    #[test]
    fn repeated_local_time_matches_both_occurrences_and_gap_has_no_instant() {
        let zone: Tz = "America/New_York".parse().unwrap();
        let window = Window::parse(&weekly("sun", "01:15", "01:45")).unwrap();
        for text in ["2026-11-01T05:30:00Z", "2026-11-01T06:30:00Z"] {
            assert!(
                window.contains(
                    inputs::instant(text)
                        .unwrap()
                        .with_timezone(&zone)
                        .naive_local()
                )
            );
        }
        assert!(
            !window.contains(
                inputs::instant("2026-11-01T06:00:00Z")
                    .unwrap()
                    .with_timezone(&zone)
                    .naive_local()
            )
        );
        let gap = Window::parse(&weekly("sun", "02:15", "02:45")).unwrap();
        assert!(
            !gap.contains(
                inputs::instant("2026-03-08T07:30:00Z")
                    .unwrap()
                    .with_timezone(&zone)
                    .naive_local()
            )
        );
    }
    #[test]
    fn gap_boundary_advances_to_first_real_instant() {
        let zone: Tz = "America/New_York".parse().unwrap();
        let local = NaiveDate::from_ymd_opt(2026, 3, 8)
            .unwrap()
            .and_hms_opt(2, 30, 0)
            .unwrap();
        assert_eq!(
            local_instants(zone, local).collect::<Vec<_>>(),
            vec![inputs::instant("2026-03-08T07:00:00Z").unwrap()]
        );
    }
}
