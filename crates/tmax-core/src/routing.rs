use crate::{
    Error, Result,
    config::{self, Config, Model, Provider, TimeWindow},
    inputs::{self, Snapshot},
    storage::Store,
};
use chrono::{
    DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, Offset,
    TimeZone, Utc,
};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Selection {
    pub provider: String,
    pub model: String,
    pub offering_identity: String,
    pub priority: i64,
    pub evidence: Value,
}

pub fn reconcile(
    store: &Store,
    old: &Config,
    new: &Config,
    renames: &[(String, String)],
) -> Result<()> {
    inputs::reconcile(store, old, new, renames)
}

pub fn destination_identity(
    store: &Store,
    config: &Config,
    qualified: &str,
) -> Result<(String, String, String)> {
    let (provider, model) = config::qualified(qualified)?;
    config::model(config, qualified)?;
    Snapshot::capture(store, config)
        .destination_identity(provider, model)
        .ok_or_else(|| {
            Error::new(
                "identity_unavailable",
                "The destination's persistent identity or authorization lineage is unavailable",
            )
        })
}

pub fn destination_identity_loaded(
    store: &Store,
    config: &Config,
    qualified: &str,
) -> Result<(String, String, String)> {
    let (provider, model) = config::qualified(qualified)?;
    config::model(config, qualified)?;
    Snapshot::capture_loaded(store,config,None).destination_identity(provider,model)
        .ok_or_else(||Error::new("identity_unavailable","The loaded destination's persistent identity or authorization lineage is unavailable"))
}

fn invalid(message: &str) -> Error {
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
struct Window {
    weekdays: Option<BTreeSet<u32>>,
    dates: Option<BTreeSet<NaiveDate>>,
    start: NaiveTime,
    end: NaiveTime,
    all_day: bool,
    from: Option<NaiveDate>,
    until: Option<NaiveDate>,
}
impl Window {
    fn parse(window: &TimeWindow) -> Result<Self> {
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
    fn contains(&self, local: NaiveDateTime) -> bool {
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
    fn overlaps(&self, other: &Self, zone: Tz) -> bool {
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
    fn boundaries(&self, zone: Tz, at: DateTime<Utc>) -> Vec<DateTime<Utc>> {
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
fn calendar_at(
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

pub fn validate_model(model: &Model, provider: &Provider) -> Result<()> {
    let needs_zone =
        !model.schedule.is_empty() || model.priority_rules.iter().any(|r| r.time_window.is_some());
    if needs_zone && model.timezone.is_none() {
        return Err(invalid(
            "Schedules and rule time gates require an explicit IANA time zone",
        ));
    }
    let zone = model
        .timezone
        .as_ref()
        .map(|zone| {
            zone.parse::<Tz>()
                .map_err(|_| invalid("Unknown IANA time zone"))
        })
        .transpose()?;
    let windows: Result<Vec<_>> = model
        .schedule
        .iter()
        .map(|s| Window::parse(&s.window))
        .collect();
    let windows = windows?;
    for (i, window) in windows.iter().enumerate() {
        for other in &windows[i + 1..] {
            if window.overlaps(
                other,
                zone.ok_or_else(|| invalid("Calendar zone is missing"))?,
            ) {
                return Err(invalid(
                    "Calendar schedules overlap at the same precedence, including overnight spillover",
                ));
            }
        }
    }
    let mut ids = BTreeSet::new();
    // The parent validates offering coverage with its map key; this validates all remaining rule semantics.
    for rule in &model.priority_rules {
        if !config::valid_id(&rule.id, false) || !ids.insert(&rule.id) {
            return Err(Error::new(
                "invalid_rule",
                "Rule IDs must be nonempty, unique, and free of slash/whitespace/control characters",
            ));
        }
        let declaration = provider.inputs.get(&rule.input).ok_or_else(|| {
            Error::new(
                "unresolved_reference",
                "Rule input declaration does not resolve",
            )
        })?;
        if rule.reset_within_seconds <= 0 {
            return Err(Error::new("invalid_rule", "Reset horizon must be positive"));
        }
        match (
            declaration.kind.as_str(),
            rule.remaining_at_least_percent.as_deref(),
        ) {
            ("quota", Some(percent)) => {
                inputs::Decimal::percent(percent)?;
            }
            ("reset", None) => {}
            _ => {
                return Err(Error::new(
                    "invalid_rule",
                    "Quota rules require a threshold; reset rules forbid one",
                ));
            }
        }
        if let Some(window) = &rule.time_window {
            Window::parse(window)?;
        }
    }
    Ok(())
}

fn evaluate(
    config: &Config,
    snapshot: &Snapshot,
    provider: &str,
    model_name: &str,
    at: DateTime<Utc>,
    exact_at: &inputs::ExactInstant,
) -> Result<Value> {
    let model = &config.providers[provider].models[model_name];
    let p = &config.providers[provider];
    let zone = model
        .timezone
        .as_ref()
        .map(|zone| {
            zone.parse::<Tz>()
                .map_err(|_| invalid("Unknown IANA time zone"))
        })
        .transpose()?;
    let offset = zone.map(|zone| at.with_timezone(&zone).offset().fix().to_string());
    let windows: Result<Vec<_>> = model
        .schedule
        .iter()
        .map(|schedule| Window::parse(&schedule.window))
        .collect();
    let windows = windows?;
    let (kind, index, calendar_priority) = zone
        .map(|zone| calendar_at(model, &windows, at.with_timezone(&zone).naive_local()))
        .unwrap_or((0, None, model.priority));
    let mut possible = BTreeSet::new();
    for window in &windows {
        possible.extend(
            window.boundaries(zone.ok_or_else(|| invalid("Calendar zone is missing"))?, at),
        );
    }
    let mut boundaries = Vec::new();
    let mut next_calendar = None;
    if let Some(zone) = zone {
        for boundary in possible {
            let before = boundary
                .checked_sub_signed(Duration::nanoseconds(1))
                .unwrap_or(boundary);
            if calendar_at(model, &windows, before.with_timezone(&zone).naive_local())
                != calendar_at(model, &windows, boundary.with_timezone(&zone).naive_local())
            {
                if next_calendar.is_none() {
                    next_calendar = Some(boundary);
                }
                boundaries.push(json!({"kind":"calendar","at":inputs::timestamp(boundary),"rule_id":null,"input":null}));
            }
        }
    }
    let fallback = json!({"kind":match kind{2=>"date",1=>"weekly",_=>"base"},"priority":calendar_priority,"schedule_index":index,"timezone":model.timezone,"utc_offset":offset,"next_boundary":next_calendar.map(inputs::timestamp)});
    let mut priority = calendar_priority;
    let mut selected: Option<String> = None;
    let mut rules = Vec::new();
    let mut revisions = BTreeMap::new();
    for (position, rule) in model.priority_rules.iter().enumerate() {
        let declaration = p
            .inputs
            .get(&rule.input)
            .ok_or_else(|| Error::new("unresolved_reference", "Rule input does not resolve"))?;
        let mut entry = serde_json::to_value(rule)
            .map_err(|_| Error::new("invalid_rule", "Cannot serialize rule"))?;
        if rule.remaining_at_least_percent.is_none() {
            entry
                .as_object_mut()
                .unwrap()
                .remove("remaining_at_least_percent");
        }
        if rule.time_window.is_none() {
            entry.as_object_mut().unwrap().remove("time_window");
        }
        entry["position"] = json!(position);
        entry["kind"] = json!(declaration.kind);
        let mut evaluation = json!({"outcome":"unknown","reason_code":null,"shadowed_by":null,
            "time_window":{"state":if rule.time_window.is_some(){"not_evaluated"}else{"not_configured"},"timezone":null,"utc_offset":null,"next_boundary":null},
            "input_basis":null,"predicate_results":{"reset_within":"not_evaluated","remaining_at_least":if declaration.kind=="reset"{"not_applicable"}else{"not_evaluated"}}});
        if !rule.enabled {
            evaluation["outcome"] = json!("disabled");
        } else if let Some(winner) = &selected {
            evaluation["outcome"] = json!("shadowed");
            evaluation["shadowed_by"] = json!(winner);
        } else {
            let mut gate = true;
            if let Some(window) = &rule.time_window {
                let window = Window::parse(window)?;
                let zone = zone.ok_or_else(|| invalid("Rule time gate requires a saved zone"))?;
                gate = window.contains(at.with_timezone(&zone).naive_local());
                let gate_boundaries = window.boundaries(zone, at);
                evaluation["time_window"] = json!({"state":if gate{"matched"}else{"not_matched"},"timezone":model.timezone,"utc_offset":offset,"next_boundary":gate_boundaries.first().copied().map(inputs::timestamp)});
                for boundary in gate_boundaries {
                    boundaries.push(json!({"kind":"time_window","at":inputs::timestamp(boundary),"rule_id":rule.id,"input":rule.input}));
                }
            }
            if !gate {
                evaluation["outcome"] = json!("false");
                evaluation["reason_code"] = json!("outside_time_window");
            } else {
                let basis = snapshot.input(provider, &rule.input, exact_at);
                if basis["availability"]["state"] != "available" {
                    evaluation["outcome"] = json!("unknown");
                    evaluation["reason_code"] = basis["availability"]["reason_code"].clone();
                } else {
                    let reset = basis["reset_at"].as_str().ok_or_else(|| {
                        Error::new("invalid_record", "Applicable input lacks reset timestamp")
                    })?;
                    let reset_matches =
                        inputs::within_horizon(reset, exact_at, rule.reset_within_seconds)?;
                    evaluation["predicate_results"]["reset_within"] =
                        json!(if reset_matches { "true" } else { "false" });
                    if !reset_matches {
                        evaluation["outcome"] = json!("false");
                        evaluation["reason_code"] = json!("reset_outside_window");
                    } else if let Some(threshold) = &rule.remaining_at_least_percent {
                        let remaining = inputs::quota_matches(&basis, threshold)?;
                        evaluation["predicate_results"]["remaining_at_least"] =
                            json!(if remaining { "true" } else { "false" });
                        if remaining {
                            evaluation["outcome"] = json!("matched");
                        } else {
                            evaluation["outcome"] = json!("false");
                            evaluation["reason_code"] = json!("remaining_below_threshold");
                        }
                    } else {
                        evaluation["outcome"] = json!("matched");
                    }
                }
                for (field, kind) in [
                    ("observed_at", "observed_at"),
                    ("valid_until", "valid_until"),
                ] {
                    if let Some(boundary) = basis[field]
                        .as_str()
                        .and_then(|s| inputs::ExactInstant::parse(s).ok())
                        .filter(|boundary| boundary > exact_at)
                    {
                        boundaries.push(json!({"kind":kind,"at":boundary.canonical(),"rule_id":rule.id,"input":rule.input}));
                    }
                }
                if let Some(reset) = basis["reset_at"]
                    .as_str()
                    .and_then(|s| inputs::ExactInstant::parse(s).ok())
                    && let Some(boundary) = reset
                        .add_seconds(-rule.reset_within_seconds)
                        .filter(|boundary| boundary > exact_at)
                {
                    boundaries.push(json!({"kind":"reset_horizon","at":boundary.canonical(),"rule_id":rule.id,"input":rule.input}));
                }
                evaluation["input_basis"] = basis;
            }
            if evaluation["outcome"] == "matched" {
                priority = rule.priority;
                selected = Some(rule.id.clone());
            }
        }
        // Snapshot references include skipped rules without inventing predicate results.
        revisions.insert(
            rule.input.clone(),
            snapshot.input_revision(provider, &rule.input),
        );
        entry["evaluation"] = evaluation;
        rules.push(entry);
    }
    boundaries.sort_by_cached_key(|boundary| {
        (
            boundary["at"]
                .as_str()
                .and_then(|at| inputs::ExactInstant::parse(at).ok()),
            match boundary["kind"].as_str() {
                Some("calendar") => 0,
                Some("observed_at") => 1,
                Some("reset_horizon") => 2,
                Some("time_window") => 3,
                _ => 4,
            },
        )
    });
    boundaries.dedup();
    Ok(
        json!({"at":exact_at.canonical(),"configuration_revision":snapshot.configuration_revision(),"metadata_revision":snapshot.metadata_revision(),
        "input_revisions":revisions.into_values().collect::<Vec<_>>(),"priority_rules":rules,"selected_rule_id":selected,
        "effective_priority":priority,"calendar_fallback":fallback,"calendar_superseded":selected.is_some(),"boundaries":boundaries}),
    )
}

fn candidates(config: &Config, id: &str, protocol: &str) -> Result<(bool, Vec<(String, String)>)> {
    if !config::PROTOCOLS.contains(&protocol) {
        return Err(Error::new(
            "unsupported_protocol",
            "Unknown native protocol",
        ));
    }
    if let Some(alias) = config
        .model_aliases
        .iter()
        .find(|a| a.id == id && a.protocol == protocol)
    {
        if !alias.enabled {
            return Err(Error::new("disabled_model", "Model alias is disabled"));
        }
        let mut result = Vec::with_capacity(alias.candidates.len());
        for name in &alias.candidates {
            let (p, m) = config::qualified(name)?;
            config::model(config, name)?;
            result.push((p.into(), m.into()));
        }
        return Ok((true, result));
    }
    let (provider, model) = config::qualified(id)?;
    config::model(config, id)?;
    Ok((false, vec![(provider.into(), model.into())]))
}
fn eligibility(
    provider: &Provider,
    model: &Model,
    name: &str,
    protocol: &str,
    inventory: Option<&Value>,
    provider_identity: Option<&str>,
) -> Option<&'static str> {
    if !provider.enabled {
        return Some("provider_disabled");
    }
    if !model.enabled {
        return Some("model_disabled");
    }
    if !crate::integrations::protocols(provider, name)
        .ok()
        .is_some_and(|protocols| protocols.iter().any(|p| p == protocol))
    {
        return Some("unsupported_protocol");
    }
    if !crate::integrations::resolve(provider)
        .ok()
        .is_some_and(|endpoints| endpoints.contains_key(protocol))
    {
        return Some("endpoint_unavailable");
    }
    match &provider.auth {
        None => Some("credential_unavailable"),
        Some(config::Auth::ApiKey { key_ref }) if key_ref.is_empty() => {
            Some("credential_unavailable")
        }
        Some(config::Auth::ApiKey { key_ref })
            if inventory.is_some_and(|inventory| {
                let entry = inventory["entries"].as_array().and_then(|entries| {
                    entries.iter().find(|entry| entry["reference"] == *key_ref)
                });
                entry.is_some_and(|entry| {
                    entry["metadata_state"] != "known"
                        || provider_identity.is_some_and(|identity| {
                            entry["owner"]["identity"].as_str() != Some(identity)
                        })
                }) || entry.is_none() && inventory["coverage"] == "complete"
            }) =>
        {
            Some("credential_unavailable")
        }
        Some(config::Auth::Login { session_ref, .. }) if session_ref.is_none() => {
            Some("credential_unavailable")
        }
        _ => None,
    }
}
/// The unchanged native request and its qualified client requirements.
#[derive(Clone, Copy)]
pub struct RequestRequirements<'a> {
    pub body: &'a Value,
    pub native: &'a crate::gateway::NativeRequirements,
}

pub(crate) struct Advice<'a> {
    pub native: Option<crate::native::compatibility::Context<'a>>,
}

enum SelectionView<'a> {
    Saved {
        body: Option<&'a Value>,
        context: Option<&'a Advice<'a>>,
    },
    Loaded {
        metadata: Option<&'a crate::catalog::Snapshot>,
        request: Option<RequestRequirements<'a>>,
        context: Option<&'a Advice<'a>>,
    },
}
fn decision(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: DateTime<Utc>,
    exact_at: &inputs::ExactInstant,
    view: SelectionView<'_>,
) -> Result<(Value, Option<Selection>)> {
    let (snapshot, body, native, context) = match view {
        SelectionView::Saved { body, context } => {
            (Snapshot::capture(store, config), body, None, context)
        }
        SelectionView::Loaded {
            metadata,
            request,
            context,
        } => (
            Snapshot::capture_loaded(store, config, metadata),
            request.map(|request| request.body),
            request.map(|request| request.native),
            context,
        ),
    };
    let (alias, candidates) = candidates(config, id, protocol)?;
    let inventory = crate::secrets::inventory(store).ok();
    let authorization = context
        .and_then(|context| context.native.as_ref())
        .filter(|context| {
            !context.gateway
                && config
                    .providers
                    .values()
                    .any(|p| matches!(p.auth, Some(config::Auth::Login { .. })))
        })
        .map(|_| crate::native::compatibility::AuthorizationSnapshot::capture(store));
    let mut evaluations = Vec::new();
    let mut ranked = Vec::new();
    let mut excluded = Vec::new();
    for (provider, name) in candidates {
        let p = &config.providers[&provider];
        let m = &p.models[&name];
        let evaluation = evaluate(config, &snapshot, &provider, &name, at, exact_at)?;
        let identity = snapshot.destination_identity(&provider, &name);
        let native_assessment =
            context
                .and_then(|context| context.native.as_ref())
                .map(|context| -> Result<()> {
                    context.assess(config, &provider, &name, protocol, id)?;
                    if !context.gateway
                        && matches!(p.auth, Some(config::Auth::Login { .. }))
                        && let Some(authorization) = &authorization
                    {
                        authorization
                            .as_ref()
                            .map_err(Clone::clone)?
                            .assess(store, config, context, &provider)?;
                    }
                    Ok(())
                });
        let reason = native_assessment
            .as_ref()
            .and_then(|result| result.as_ref().err())
            .map(|error| error.code.as_str())
            .or_else(|| {
                eligibility(
                    p,
                    m,
                    &name,
                    protocol,
                    inventory.as_ref(),
                    identity.as_ref().map(|ids| ids.0.as_str()),
                )
            })
            .or_else(|| {
                body.and_then(|body| {
                    crate::gateway::capability_mismatch(p, &name, m, snapshot.metadata(), body)
                })
            })
            .or_else(|| native.and_then(|native| crate::gateway::native_mismatch(p, native)));
        let observed_unavailability = if reason.is_none()
            && let Some((incarnation, authorization, _)) = &identity
        {
            crate::observations::routing_availability_for_binding(
                store,
                config,
                &provider,
                exact_at,
                (incarnation, authorization),
            )
            .ok()
            .flatten()
        } else {
            None
        };
        let reason = reason.or(observed_unavailability.as_deref());
        let priority = evaluation["effective_priority"]
            .as_i64()
            .unwrap_or(m.priority);
        let qualified = format!("{provider}/{name}");
        if let Some(reason) = reason {
            excluded.push(json!({"qualified_model":qualified,"reason_code":reason,"native_requirement":native_assessment.as_ref().and_then(|result| result.as_ref().err()).map(|error| json!({"code":error.code,"message":error.message}))}));
        } else if let Some((provider_identity, authorization_generation, identity)) = identity {
            ranked.push((
                priority,
                identity,
                provider.clone(),
                name.clone(),
                provider_identity,
                authorization_generation,
            ));
        } else {
            excluded
                .push(json!({"qualified_model":qualified,"reason_code":"identity_unavailable"}));
        }
        evaluations.push(json!({"qualified_model":qualified,"priority_used_for_ranking":alias,"priority_evaluation":evaluation}));
    }
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let selected=ranked.first().map(|(priority,identity,p,m,provider_identity,authorization_generation)|json!({"provider":p,"model":m,"qualified_model":format!("{p}/{m}"),"provider_identity":provider_identity,"authorization_generation":authorization_generation,"offering_identity":identity,"priority":priority}));
    let reason = if !alias {
        "fixed_concrete"
    } else if ranked
        .get(1)
        .is_some_and(|next| Some(next.0) == ranked.first().map(|r| r.0))
    {
        "stable_offering_identity"
    } else {
        "highest_effective_priority"
    };
    let evidence = json!({"public_id":id,"protocol":protocol,"at":exact_at.canonical(),"configuration_revision":snapshot.configuration_revision(),"metadata_revision":snapshot.metadata_revision(),
        "priority_evaluations":evaluations,"excluded_candidates":excluded,"selection":selected,"selection_reason":reason});
    let selection = ranked
        .into_iter()
        .next()
        .map(|(priority, identity, provider, model, _, _)| Selection {
            provider,
            model,
            offering_identity: identity,
            priority,
            evidence: evidence.clone(),
        });
    Ok((evidence, selection))
}
pub fn explain(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: Option<&str>,
) -> Result<Value> {
    explain_native(store, config, id, protocol, at, None)
}
pub(crate) fn explain_native(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: Option<&str>,
    context: Option<&Advice<'_>>,
) -> Result<Value> {
    let now = Utc::now();
    let text = at
        .map(str::to_owned)
        .unwrap_or_else(|| inputs::timestamp(now));
    let exact_at = inputs::ExactInstant::parse(&text)?;
    let at = inputs::instant(&text)?;
    decision(
        store,
        config,
        id,
        protocol,
        at,
        &exact_at,
        SelectionView::Saved {
            body: None,
            context,
        },
    )
    .map(|(evidence, _)| evidence)
}
pub fn select(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: DateTime<Utc>,
) -> Result<Selection> {
    let (_, selection) = decision(
        store,
        config,
        id,
        protocol,
        at,
        &inputs::ExactInstant::from_chrono(at),
        SelectionView::Saved {
            body: None,
            context: None,
        },
    )?;
    selection.ok_or_else(|| {
        Error::new(
            "model_unavailable",
            "No configured candidate has applicable identity, authorization, and protocol access",
        )
    })
}
fn request_unavailable(evidence: &Value) -> Error {
    // A fixed offering, or candidates with the same blocker, has a precise
    // rejection reason. Do not erase it before the gateway records history.
    // Different blockers cannot truthfully be reduced to one candidate's cause.
    let common_reason = evidence["excluded_candidates"]
        .as_array()
        .and_then(|excluded| {
            excluded
                .first()
                .and_then(|candidate| candidate["reason_code"].as_str())
                .filter(|reason| {
                    excluded
                        .iter()
                        .all(|candidate| candidate["reason_code"].as_str() == Some(*reason))
                })
        });
    Error::new(
        common_reason.unwrap_or("model_unavailable"),
        "No configured candidate satisfies the request's native capability, authorization, and protocol requirements",
    )
}

pub fn select_request(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: DateTime<Utc>,
    body: &Value,
) -> Result<Selection> {
    let (evidence, selection) = decision(
        store,
        config,
        id,
        protocol,
        at,
        &inputs::ExactInstant::from_chrono(at),
        SelectionView::Saved {
            body: Some(body),
            context: None,
        },
    )?;
    selection.ok_or_else(|| request_unavailable(&evidence))
}

pub fn select_request_loaded(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: DateTime<Utc>,
    request: RequestRequirements<'_>,
    metadata: Option<&crate::catalog::Snapshot>,
) -> Result<Selection> {
    let (evidence, selection) = decision(
        store,
        config,
        id,
        protocol,
        at,
        &inputs::ExactInstant::from_chrono(at),
        SelectionView::Loaded {
            metadata,
            request: Some(request),
            context: None,
        },
    )?;
    selection.ok_or_else(|| request_unavailable(&evidence))
}

pub(crate) fn explain_loaded(
    store: &Store,
    config: &Config,
    id: &str,
    protocol: &str,
    at: Option<&str>,
    metadata: Option<&crate::catalog::Snapshot>,
    context: Option<&Advice<'_>>,
) -> Result<Value> {
    let now = Utc::now();
    let text = at
        .map(str::to_owned)
        .unwrap_or_else(|| inputs::timestamp(now));
    let exact_at = inputs::ExactInstant::parse(&text)?;
    let at = inputs::instant(&text)?;
    decision(
        store,
        config,
        id,
        protocol,
        at,
        &exact_at,
        SelectionView::Loaded {
            metadata,
            request: None,
            context,
        },
    )
    .map(|(evidence, _)| evidence)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loaded_alias_advice_keeps_native_candidates_metadata_and_input_lineage_together() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().join("state"));
        let mut loaded = Config::empty();
        loaded.providers.insert(
            "old".into(),
            Provider {
                integration: Some("custom".into()),
                metadata: Some("catalog".into()),
                endpoints: [(
                    "chat-completions".into(),
                    "https://example.invalid/v1".into(),
                )]
                .into(),
                auth: Some(config::Auth::ApiKey {
                    key_ref: "not-resolved".into(),
                }),
                models: [(
                    "m".into(),
                    Model {
                        priority: 7,
                        ..Default::default()
                    },
                )]
                .into(),
                ..Default::default()
            },
        );
        loaded.model_aliases.push(config::ModelAlias {
            id: "L0".into(),
            protocol: "chat-completions".into(),
            candidates: vec!["old/m".into()],
            enabled: true,
            continuity: None,
        });
        store
            .write_raw("tmax.toml", toml::to_string(&loaded).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(&store, &Config::empty(), &loaded, &[]).unwrap();
        let loaded_revision = config::raw_revision(&store).unwrap();
        inputs::activate(&store, &loaded, &loaded_revision).unwrap();
        let old_metadata = crate::catalog::Snapshot {
            revision: "loaded-metadata".into(),
            providers: [(
                "catalog".into(),
                json!({"models":{"m":{"reasoning":false}}}),
            )]
            .into(),
            ..Default::default()
        };
        let new_metadata = crate::catalog::Snapshot {
            revision: "saved-metadata".into(),
            ..old_metadata.clone()
        };
        store
            .write_json("catalog/accepted.json", &new_metadata)
            .unwrap();
        let mut saved = loaded.clone();
        let provider = saved.providers.remove("old").unwrap();
        saved.providers.insert("new".into(), provider);
        saved.model_aliases[0].candidates = vec!["new/m".into()];
        saved
            .providers
            .get_mut("new")
            .unwrap()
            .models
            .get_mut("m")
            .unwrap()
            .priority = 99;
        store
            .write_raw("tmax.toml", toml::to_string(&saved).unwrap().as_bytes())
            .unwrap();
        inputs::reconcile(&store, &loaded, &saved, &[]).unwrap();
        let context = Advice {
            native: Some(crate::native::compatibility::Context {
                agent: "codex",
                target: Some("old-target"),
                provider: None,
                gateway: false,
            }),
        };
        let evidence = explain_loaded(
            &store,
            &loaded,
            "L0",
            "chat-completions",
            None,
            Some(&old_metadata),
            Some(&context),
        )
        .unwrap();
        assert_eq!(evidence["metadata_revision"], "loaded-metadata");
        assert_eq!(evidence["configuration_revision"], loaded_revision);
        assert_eq!(
            evidence["excluded_candidates"][0]["qualified_model"],
            "old/m"
        );
        assert_eq!(
            evidence["excluded_candidates"][0]["reason_code"],
            "incompatible"
        );
        assert_eq!(
            evidence["priority_evaluations"][0]["priority_evaluation"]["effective_priority"],
            7
        );
        let saved_context = Advice {
            native: Some(crate::native::compatibility::Context {
                agent: "pi",
                target: Some("new-target"),
                provider: None,
                gateway: false,
            }),
        };
        let evidence = explain_native(
            &store,
            &saved,
            "L0",
            "chat-completions",
            None,
            Some(&saved_context),
        )
        .unwrap();
        assert_eq!(evidence["metadata_revision"], "saved-metadata");
        assert_eq!(
            evidence["excluded_candidates"][0]["qualified_model"],
            "new/m"
        );
        assert_eq!(
            evidence["priority_evaluations"][0]["priority_evaluation"]["effective_priority"],
            99
        );
    }

    #[test]
    fn request_rejection_preserves_shared_blockers_without_mislabeling_mixed_failures() {
        let fixed = json!({"excluded_candidates":[{"qualified_model":"p/m","reason_code":"native_session_required"}]});
        assert_eq!(request_unavailable(&fixed).code, "native_session_required");
        let shared = json!({"excluded_candidates":[
            {"qualified_model":"p/m","reason_code":"native_session_required"},
            {"qualified_model":"q/m","reason_code":"native_session_required"}
        ]});
        assert_eq!(request_unavailable(&shared).code, "native_session_required");
        let mixed = json!({"excluded_candidates":[
            {"qualified_model":"p/m","reason_code":"native_session_required"},
            {"qualified_model":"q/m","reason_code":"provider_disabled"}
        ]});
        assert_eq!(request_unavailable(&mixed).code, "model_unavailable");
        assert_eq!(
            request_unavailable(&json!({"excluded_candidates":[]})).code,
            "model_unavailable"
        );
    }

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
    fn date_override_suppresses_irrelevant_weekly_boundary_without_writes() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_path_buf());
        let mut config = Config::empty();
        let model = Model {
            enabled: true,
            priority: 5,
            timezone: Some("Etc/UTC".into()),
            schedule: vec![
                config::Schedule {
                    priority: 20,
                    window: weekly("fri", "08:00", "12:00"),
                },
                config::Schedule {
                    priority: 30,
                    window: TimeWindow {
                        dates: Some(vec![toml::value::Date {
                            year: 2026,
                            month: 10,
                            day: 23,
                        }]),
                        all_day: Some(true),
                        ..Default::default()
                    },
                },
            ],
            ..Default::default()
        };
        let mut provider = Provider {
            enabled: true,
            integration: Some("custom".into()),
            ..Default::default()
        };
        provider.models.insert("m".into(), model);
        config.providers.insert("p".into(), provider);
        let at = inputs::instant("2026-10-23T09:00:00Z").unwrap();
        let snapshot = Snapshot::capture(&store, &config);
        let evaluation = evaluate(
            &config,
            &snapshot,
            "p",
            "m",
            at,
            &inputs::ExactInstant::from_chrono(at),
        )
        .unwrap();
        assert_eq!(evaluation["calendar_fallback"]["kind"], "date");
        assert_eq!(evaluation["effective_priority"], 30);
        assert_eq!(
            evaluation["calendar_fallback"]["next_boundary"],
            "2026-10-24T00:00:00Z"
        );
        assert!(!store.root.exists());
    }
    #[tokio::test]
    async fn real_manual_head_drives_ordered_rules_and_clear_cannot_reconstruct_history() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().to_path_buf());
        let mut config = Config::empty();
        let mut provider = Provider {
            enabled: true,
            integration: Some("custom".into()),
            ..Default::default()
        };
        provider.endpoints.insert(
            "chat-completions".into(),
            "http://127.0.0.1:19090/v1".into(),
        );
        provider.inputs.insert(
            "weekly".into(),
            config::InputDeclaration {
                kind: "quota".into(),
                source: "manual".into(),
                scope: "weekly".into(),
                models: vec!["m".into()],
                max_age_seconds: 3600,
                refresh_interval_seconds: None,
            },
        );
        let rule =
            |id: &str, threshold: &str, priority, enabled, time_window| config::PriorityRule {
                id: id.into(),
                input: "weekly".into(),
                priority,
                enabled,
                reset_within_seconds: 3600,
                remaining_at_least_percent: Some(threshold.into()),
                time_window,
            };
        let closed = TimeWindow {
            dates: Some(vec![toml::value::Date {
                year: 2000,
                month: 1,
                day: 1,
            }]),
            all_day: Some(true),
            ..Default::default()
        };
        provider.models.insert(
            "m".into(),
            Model {
                enabled: true,
                priority: 80,
                timezone: Some("Etc/UTC".into()),
                priority_rules: vec![
                    rule("closed", "0", 900, true, Some(closed)),
                    rule("too-high", "33.34", 1000, true, None),
                    rule("first-match", "33.33", 7, true, None),
                    rule("later-higher", "0", 9999, true, None),
                    rule("disabled", "0", 99999, false, None),
                ],
                ..Default::default()
            },
        );
        config.providers.insert("p".into(), provider);
        store
            .write_raw("tmax.toml", toml::to_string(&config).unwrap().as_bytes())
            .unwrap();
        let at = Utc::now();
        let mut arguments = BTreeMap::new();
        for (name, value) in [
            ("observed-at", inputs::timestamp(at - Duration::seconds(10))),
            (
                "valid-until",
                inputs::timestamp(at + Duration::seconds(500)),
            ),
            ("reset-at", inputs::timestamp(at + Duration::seconds(600))),
            ("remaining", "1".into()),
            ("total", "3".into()),
            ("unit", "requests".into()),
            ("reference", "https://example.test/manual".into()),
        ] {
            arguments.insert(name.into(), vec![value]);
        }
        let record = crate::Request {
            command: vec!["provider".into(), "input".into(), "record".into()],
            operands: vec!["p".into(), "weekly".into()],
            arguments,
            dry_run: false,
            if_revision: None,
        };
        let recorded = inputs::handle(&store, &config, &record).await.unwrap();
        assert_eq!(recorded["input_change"]["outcome"], "published");
        let explanation = explain(
            &store,
            &config,
            "p/m",
            "chat-completions",
            Some(&inputs::timestamp(at)),
        )
        .unwrap();
        let evaluation = &explanation["priority_evaluations"][0]["priority_evaluation"];
        assert_eq!(evaluation["effective_priority"], 7);
        assert_eq!(evaluation["selected_rule_id"], "first-match");
        let rules = &evaluation["priority_rules"];
        assert_eq!(rules[0]["evaluation"]["reason_code"], "outside_time_window");
        assert!(rules[0]["evaluation"]["input_basis"].is_null());
        assert_eq!(
            rules[1]["evaluation"]["reason_code"],
            "remaining_below_threshold"
        );
        assert_eq!(rules[2]["evaluation"]["input_basis"]["remaining"], "1");
        assert_eq!(rules[2]["evaluation"]["input_basis"]["total"], "3");
        assert!(
            rules[2]["evaluation"]["input_basis"]
                .get("reference")
                .is_none()
        );
        assert!(
            rules[2]["evaluation"]["input_basis"]
                .get("remaining_percent")
                .is_none()
        );
        assert_eq!(rules[3]["evaluation"]["outcome"], "shadowed");
        assert!(rules[3]["evaluation"]["input_basis"].is_null());
        assert_eq!(rules[4]["evaluation"]["outcome"], "disabled");
        let clear = crate::Request {
            command: vec!["provider".into(), "input".into(), "clear".into()],
            operands: vec!["p".into(), "weekly".into()],
            arguments: BTreeMap::new(),
            dry_run: false,
            if_revision: None,
        };
        inputs::handle(&store, &config, &clear).await.unwrap();
        let after = explain(
            &store,
            &config,
            "p/m",
            "chat-completions",
            Some(&inputs::timestamp(at)),
        )
        .unwrap();
        let evaluation = &after["priority_evaluations"][0]["priority_evaluation"];
        assert_eq!(evaluation["effective_priority"], 80);
        assert_eq!(
            evaluation["priority_rules"][2]["evaluation"]["outcome"],
            "unknown"
        );
        assert_eq!(
            evaluation["priority_rules"][2]["evaluation"]["reason_code"],
            "cleared"
        );
    }
    fn weekly(day: &str, start: &str, end: &str) -> TimeWindow {
        TimeWindow {
            weekdays: Some(vec![day.into()]),
            start: Some(start.into()),
            end: Some(end.into()),
            ..Default::default()
        }
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
    fn fractional_horizon_does_not_round_into_match() {
        let at = inputs::ExactInstant::parse("2026-10-23T11:00:00Z").unwrap();
        assert!(inputs::within_horizon("2026-10-23T12:00:00Z", &at, 3600).unwrap());
        assert!(!inputs::within_horizon("2026-10-23T12:00:00.000000001Z", &at, 3600).unwrap());
        assert!(!inputs::within_horizon("2026-10-23T12:00:00.0000000001Z", &at, 3600).unwrap());
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
