//! Bounded, content-free diagnostics. These are process-local observations, never billing facts.
use crate::{Error, Request, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Mutex,
};
use uuid::Uuid;

pub const RECORD_LIMIT: usize = 256;
pub const RECORD_BYTES: usize = 32 * 1024;
pub const HISTORY_BYTES: usize = RECORD_LIMIT * RECORD_BYTES;

#[derive(Clone, Serialize, Deserialize)]
pub struct Record {
    pub request: String,
    pub process_instance: String,
    pub origin: String,
    pub received_at: DateTime<Utc>,
    pub admitted_at: Option<DateTime<Utc>>,
    pub target: Option<String>,
    pub agent_provider: Option<String>,
    pub model: Option<String>,
    pub model_kind: String,
    pub protocol: Option<String>,
    pub provider: Option<String>,
    pub offering: Option<String>,
    pub authorization_generation: Option<String>,
    pub configuration_revision: Option<String>,
    pub decision: Value,
    pub outcome: String,
    pub stage: String,
    pub reason_code: Option<String>,
    pub upstream_status: Option<u16>,
    pub duration_ms: Option<u64>,
    pub usage: BTreeMap<String, u64>,
    pub usage_state: String,
    pub execution: String,
    pub downstream_complete: bool,
    pub evidence_gap: bool,
}
impl Record {
    pub fn terminal(&self) -> bool {
        self.outcome != "in-progress"
    }
}
struct State {
    records: VecDeque<Record>,
    reset: String,
    cleared_at: Option<DateTime<Utc>>,
    evicted: u64,
    omitted: u64,
}
pub struct History {
    pub process: String,
    pub started_at: DateTime<Utc>,
    state: Mutex<State>,
}
impl History {
    pub fn new(process: String) -> Self {
        Self {
            process,
            started_at: Utc::now(),
            state: Mutex::new(State {
                records: VecDeque::with_capacity(RECORD_LIMIT),
                reset: Uuid::new_v4().to_string(),
                cleared_at: None,
                evicted: 0,
                omitted: 0,
            }),
        }
    }
    pub fn begin(&self) -> Record {
        let record = Record {
            request: format!("{}:{}", self.process, Uuid::new_v4()),
            process_instance: self.process.clone(),
            origin: "client".into(),
            received_at: Utc::now(),
            admitted_at: None,
            target: None,
            agent_provider: None,
            model: None,
            model_kind: "unknown".into(),
            protocol: None,
            provider: None,
            offering: None,
            authorization_generation: None,
            configuration_revision: None,
            decision: Value::Null,
            outcome: "in-progress".into(),
            stage: "receipt".into(),
            reason_code: None,
            upstream_status: None,
            duration_ms: None,
            usage: BTreeMap::new(),
            usage_state: "unknown".into(),
            execution: "not_dispatched".into(),
            downstream_complete: false,
            evidence_gap: false,
        };
        let mut state = self.state.lock().expect("history mutex");
        if state.records.len() == RECORD_LIMIT {
            if let Some(index) = state.records.iter().position(Record::terminal) {
                state.records.remove(index);
                state.evicted += 1;
            } else {
                state.omitted += 1;
                return record;
            }
        }
        state.records.push_back(record.clone());
        record
    }
    pub fn update(&self, mut record: Record) {
        record.decision = safe_evidence(&record.decision);
        record.evidence_gap |=
            record.decision.get("evidence_gap").and_then(Value::as_bool) == Some(true);
        for field in [
            &mut record.target,
            &mut record.agent_provider,
            &mut record.model,
            &mut record.protocol,
            &mut record.provider,
            &mut record.offering,
            &mut record.authorization_generation,
            &mut record.configuration_revision,
            &mut record.reason_code,
        ] {
            if field.as_ref().is_some_and(|s| s.len() > 512) {
                *field = None;
                record.evidence_gap = true;
            } else if let Some(value) = field
                && value.capacity() > 512
            {
                value.shrink_to_fit();
            }
        }
        for field in [
            &mut record.request,
            &mut record.process_instance,
            &mut record.origin,
            &mut record.model_kind,
            &mut record.outcome,
            &mut record.stage,
            &mut record.usage_state,
            &mut record.execution,
        ] {
            if field.len() > 512 {
                *field = "unavailable".into();
                record.evidence_gap = true;
            } else if field.capacity() > 512 {
                field.shrink_to_fit();
            }
        }
        let usage_count = record.usage.len();
        record
            .usage
            .retain(|key, _| key.len() <= 64 && key.capacity() <= 64);
        if record.usage.len() != usage_count {
            record.evidence_gap = true;
            record.usage_state = "partial".into();
        }
        if encoded_size(&record) > RECORD_BYTES || record_memory(&record) > RECORD_BYTES {
            record.decision = decision_summary(&record.decision);
            record.evidence_gap = true;
        }
        if encoded_size(&record) > RECORD_BYTES || record_memory(&record) > RECORD_BYTES {
            record.decision = json!({"evidence_gap":true,"reason_code":"record_capacity"});
            record.usage.clear();
            record.usage_state = "unknown".into();
        }
        let mut state = self.state.lock().expect("history mutex");
        if let Some(current) = state
            .records
            .iter_mut()
            .find(|r| r.request == record.request)
            && !current.terminal()
        {
            *current = record;
        }
    }
    fn coverage(&self, state: &State) -> Value {
        json!({"process_instance":self.process,"reset":state.reset,"process_started_at":self.started_at,"last_clear_at":state.cleared_at,"limits":{"records":RECORD_LIMIT,"record_bytes":RECORD_BYTES,"serialized_record_budget_bytes":HISTORY_BYTES,"resident_record_accounting_budget_bytes":HISTORY_BYTES,"accounting":"struct bytes plus string/vector capacities and conservative 256 bytes per JSON object entry"},"retention":"oldest terminal record evicted first; no disk history","retained_count":state.records.len(),"oldest":state.records.front().map(|r|r.received_at),"newest":state.records.back().map(|r|r.received_at),"evicted":state.evicted,"incomplete_capture":state.omitted,"prior_process_coverage":"unavailable","direct_traffic_coverage":"unobserved"})
    }
    pub fn view(&self, request: &Request) -> Result<Value> {
        let state = self.state.lock().expect("history mutex");
        let coverage = self.coverage(&state);
        match request.command.get(1).map(String::as_str) {
            Some("inspect") => {
                let id = request
                    .operands
                    .first()
                    .ok_or_else(|| Error::new("missing_operand", "Request handle required"))?;
                Ok(
                    json!({"coverage":coverage,"record":state.records.iter().find(|r|&r.request==id),"snapshot_state":if state.records.iter().any(|r|&r.request==id){"available"}else{"unavailable"}}),
                )
            }
            Some("list") => {
                let arg = |key: &str| request.arguments.get(key).and_then(|v| v.first());
                if arg("protocol").is_some() != arg("model").is_some() {
                    return Err(Error::new(
                        "invalid_argument",
                        "Model and protocol filters must be supplied together",
                    ));
                }
                if arg("outcome").is_some_and(|value| {
                    !matches!(
                        value.as_str(),
                        "in-progress"
                            | "rejected"
                            | "completed"
                            | "failed"
                            | "cancelled"
                            | "interrupted"
                            | "unknown"
                    )
                }) {
                    return Err(Error::new(
                        "invalid_argument",
                        "Unsupported history outcome filter",
                    )
                    .field("outcome"));
                }
                let parse = |key: &str| -> Result<Option<crate::inputs::ExactInstant>> {
                    arg(key)
                        .map(|s| {
                            crate::inputs::ExactInstant::parse(s).map_err(|_| {
                                Error::new("invalid_argument", "Expected an RFC3339 instant")
                                    .field(key)
                            })
                        })
                        .transpose()
                };
                let since = parse("since")?;
                let until = parse("until")?;
                if since
                    .as_ref()
                    .zip(until.as_ref())
                    .is_some_and(|(a, b)| a >= b)
                {
                    return Err(Error::new(
                        "invalid_argument",
                        "History range must be increasing",
                    ));
                }
                let limit = arg("limit")
                    .map(|value| {
                        value
                            .parse::<usize>()
                            .ok()
                            .filter(|value| *value > 0 && *value <= RECORD_LIMIT)
                            .ok_or_else(|| {
                                Error::new(
                                    "invalid_argument",
                                    "History limit must be within the retained record capacity",
                                )
                            })
                    })
                    .transpose()?
                    .unwrap_or(RECORD_LIMIT);
                let mut rows = Vec::new();
                let mut matched = 0;
                for record in &state.records {
                    let t = crate::inputs::ExactInstant::from_chrono(
                        record.admitted_at.unwrap_or(record.received_at),
                    );
                    if since.as_ref().is_none_or(|s| &t >= s)
                        && until.as_ref().is_none_or(|u| &t < u)
                        && arg("provider").is_none_or(|v| record.provider.as_ref() == Some(v))
                        && arg("model").is_none_or(|v| record.model.as_ref() == Some(v))
                        && arg("protocol").is_none_or(|v| record.protocol.as_ref() == Some(v))
                        && arg("outcome").is_none_or(|v| &record.outcome == v)
                        && arg("target").is_none_or(|v| record.target.as_ref() == Some(v))
                        && arg("agent-provider")
                            .is_none_or(|v| record.agent_provider.as_ref() == Some(v))
                    {
                        matched += 1;
                        if rows.len() < limit {
                            rows.push(json!({"request":record.request,"time":record.admitted_at.unwrap_or(record.received_at),"time_basis":if record.admitted_at.is_some(){"admission"}else{"receipt"},"model":record.model,"model_kind":record.model_kind,"protocol":record.protocol,"provider":record.provider,"target":record.target,"agent_provider":record.agent_provider,"outcome":record.outcome,"reason_code":record.reason_code}));
                        }
                    }
                }
                Ok(
                    json!({"coverage":coverage,"requested_range":{"since":since.as_ref().map(crate::inputs::ExactInstant::canonical),"until":until.as_ref().map(crate::inputs::ExactInstant::canonical)},"includes_in_progress":true,"matched_retained_count":matched,"omitted_by_limit":matched-rows.len(),"records":rows}),
                )
            }
            _ => Err(Error::new(
                "unsupported_operation",
                "Unknown history operation",
            )),
        }
    }
    pub fn clear(&self, dry_run: bool) -> Value {
        // One mutex is the execution-local snapshot/commit boundary. Arrivals and completions
        // cannot broaden its terminal membership, and no unbounded ID copy is allocated.
        let mut state = self.state.lock().expect("history mutex");
        let snapshot = state.records.iter().filter(|r| r.terminal()).count();
        let in_progress = state.records.len() - snapshot;
        let before = state.reset.clone();
        if !dry_run {
            state.records.retain(|r| !r.terminal());
            state.reset = Uuid::new_v4().to_string();
            state.cleared_at = Some(Utc::now());
        }
        json!({"dry_run":dry_run,"process_instance":self.process,"snapshot_reset":before,"snapshot_count":snapshot,"removed":if dry_run{0}else{snapshot},"already_absent":0,"retained_in_progress":in_progress,"coverage":self.coverage(&state)})
    }
    pub fn usage(&self) -> Value {
        let state = self.state.lock().expect("history mutex");
        let mut groups: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
        let mut unknown = 0;
        let mut partial = 0;
        for record in &state.records {
            if record.usage_state == "unknown" {
                unknown += 1;
            } else if record.usage_state == "partial" {
                partial += 1;
            }
            if let Some(provider) = &record.provider {
                let counters = groups.entry(provider.clone()).or_default();
                for (unit, count) in &record.usage {
                    let sum = counters.entry(unit.clone()).or_default();
                    *sum = sum.saturating_add(*count);
                }
            }
        }
        json!({"coverage":self.coverage(&state),"source":"observed_gateway_requests","official_bill":false,"remaining_quota":null,"unknown_usage_records":unknown,"partial_usage_records":partial,"providers":groups})
    }
}
fn encoded_size(value: &impl Serialize) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    if serde_json::to_writer(&mut count, value).is_err() {
        usize::MAX
    } else {
        count.0
    }
}
fn value_memory(value: &Value) -> usize {
    match value {
        Value::String(s) => s.capacity(),
        Value::Array(v) => {
            v.capacity() * std::mem::size_of::<Value>() + v.iter().map(value_memory).sum::<usize>()
        }
        Value::Object(v) => v
            .iter()
            .map(|(k, v)| 256 + k.capacity() + value_memory(v))
            .sum(),
        _ => 0,
    }
}
fn record_memory(record: &Record) -> usize {
    std::mem::size_of::<Record>()
        + [
            Some(&record.request),
            Some(&record.process_instance),
            Some(&record.origin),
            record.target.as_ref(),
            record.agent_provider.as_ref(),
            record.model.as_ref(),
            Some(&record.model_kind),
            record.protocol.as_ref(),
            record.provider.as_ref(),
            record.offering.as_ref(),
            record.authorization_generation.as_ref(),
            record.configuration_revision.as_ref(),
            Some(&record.outcome),
            Some(&record.stage),
            record.reason_code.as_ref(),
            Some(&record.usage_state),
            Some(&record.execution),
        ]
        .into_iter()
        .flatten()
        .map(String::capacity)
        .sum::<usize>()
        + record
            .usage
            .keys()
            .map(|key| 256 + key.capacity())
            .sum::<usize>()
        + value_memory(&record.decision)
}

/// Keep only the routing authority's typed scalar evidence, never source documents.
pub fn safe_evidence(value: &Value) -> Value {
    fn allowed(key: &str) -> bool {
        matches!(
            key,
            "at" | "instant"
                | "configuration_revision"
                | "metadata_revision"
                | "provider"
                | "model"
                | "offering"
                | "offering_identity"
                | "provider_identity"
                | "authorization_generation"
                | "input"
                | "declaration_revision"
                | "head_revision"
                | "head_state"
                | "priority"
                | "effective_priority"
                | "base_priority"
                | "fallback"
                | "calendar"
                | "rules"
                | "priority_rules"
                | "reason"
                | "reason_code"
                | "selection_reason"
                | "tie_break"
                | "id"
                | "order"
                | "enabled"
                | "kind"
                | "source"
                | "scope"
                | "scope_handle"
                | "unit_handle"
                | "outcome"
                | "matched"
                | "shadowed_by"
                | "reset_within_seconds"
                | "remaining_at_least_percent"
                | "time_window"
                | "weekdays"
                | "dates"
                | "all_day"
                | "start"
                | "end"
                | "effective_from"
                | "effective_until"
                | "timezone"
                | "utc_offset"
                | "gate"
                | "input_basis"
                | "applicability"
                | "observed_at"
                | "observed_at_basis"
                | "received_at"
                | "valid_until"
                | "reset_at"
                | "remaining_percent"
                | "remaining"
                | "total"
                | "unit"
                | "coverage"
                | "selected"
                | "candidates"
                | "excluded"
                | "evaluations"
                | "continuity"
                | "decision"
                | "evidence_gap"
                | "public_id"
                | "protocol"
                | "selection"
                | "qualified_model"
                | "priority_evaluations"
                | "excluded_candidates"
                | "priority_used_for_ranking"
                | "priority_evaluation"
                | "selected_evaluation"
                | "original_decision"
                | "input_revisions"
                | "selected_rule_id"
                | "winning_rule"
                | "calendar_fallback"
                | "calendar_superseded"
                | "boundaries"
                | "schedule_index"
                | "next_boundary"
                | "rule_id"
                | "position"
                | "evaluation"
                | "predicate_results"
                | "reset_within"
                | "remaining_at_least"
                | "state"
                | "availability"
                | "current_revision"
                | "models"
        )
    }
    fn copy(value: &Value, depth: usize, budget: &mut usize, gap: &mut bool) -> Value {
        if depth > 16 || *budget < 16 {
            *gap = true;
            return Value::Null;
        }
        *budget -= 16;
        match value {
            Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
            Value::String(s) => {
                if s.len() > *budget || s.contains("://") || s.chars().any(char::is_control) {
                    *gap = true;
                    Value::Null
                } else {
                    *budget -= s.len();
                    value.clone()
                }
            }
            Value::Array(values) => {
                if values.len() > 64 {
                    *gap = true;
                }
                Value::Array(
                    values
                        .iter()
                        .take(64)
                        .map(|v| copy(v, depth + 1, budget, gap))
                        .collect(),
                )
            }
            Value::Object(values) => {
                let mut result = serde_json::Map::new();
                // Capture the actual winning score and fallback before optional competitors.
                let first = [
                    "selection_reason",
                    "configuration_revision",
                    "metadata_revision",
                    "at",
                    "reason_code",
                    "effective_priority",
                    "priority",
                    "selection",
                    "selected",
                    "selected_rule_id",
                    "winning_rule",
                    "calendar_fallback",
                    "calendar_superseded",
                    "selected_evaluation",
                    "fallback",
                    "calendar",
                    "matched",
                    "original_decision",
                    "decision",
                    "rules",
                    "priority_rules",
                ];
                if let (Some(winner), Some(rules)) = (
                    values.get("selected_rule_id").and_then(Value::as_str),
                    values.get("priority_rules").and_then(Value::as_array),
                ) && let Some(rule) = rules
                    .iter()
                    .find(|r| r.get("id").and_then(Value::as_str) == Some(winner))
                {
                    result.insert("winning_rule".into(), copy(rule, depth + 1, budget, gap));
                }
                for key in first {
                    if let Some(value) = values.get(key) {
                        result.insert(key.into(), copy(value, depth + 1, budget, gap));
                    }
                }
                for (key, value) in values {
                    if first.contains(&key.as_str()) {
                        continue;
                    }
                    if allowed(key) {
                        result.insert(key.clone(), copy(value, depth + 1, budget, gap));
                    } else {
                        *gap = true;
                    }
                }
                Value::Object(result)
            }
        }
    }
    let mut gap = false;
    let mut result = copy(value, 0, &mut (24 * 1024), &mut gap);
    if gap {
        if let Value::Object(object) = &mut result {
            object.insert("evidence_gap".into(), json!(true));
        } else {
            result = json!({"decision":result,"evidence_gap":true});
        }
    }
    result
}
fn decision_summary(value: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for key in [
        "selection_reason",
        "configuration_revision",
        "metadata_revision",
        "at",
        "reason_code",
        "effective_priority",
        "priority",
        "selection",
        "selected",
        "selected_rule_id",
        "winning_rule",
        "calendar_fallback",
        "calendar_superseded",
        "fallback",
        "calendar",
        "matched",
    ] {
        if let Some(value) = value.get(key)
            && encoded_size(value) <= 2048
        {
            result.insert(key.into(), value.clone());
        }
    }
    if let Some(evaluation) = value.pointer("/selected_evaluation/priority_evaluation") {
        result.insert("selected_evaluation".into(), decision_summary(evaluation));
    }
    result.insert("evidence_gap".into(), json!(true));
    Value::Object(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clear_preserves_active_and_terminal_is_immutable() {
        let h = History::new("p".into());
        let mut a = h.begin();
        let b = h.begin();
        a.outcome = "failed".into();
        h.update(a.clone());
        a.outcome = "completed".into();
        h.update(a);
        let result = h.clear(false);
        assert_eq!(result["removed"], 1);
        assert_eq!(result["retained_in_progress"], 1);
        let state = h.state.lock().unwrap();
        assert_eq!(state.records[0].request, b.request);
    }
    #[test]
    fn evidence_excludes_content_and_urls() {
        let result = safe_evidence(
            &json!({"body":"secret","reason_code":"expired","input_basis":{"remaining":"1","source_url":"https://secret.test","prompt":"secret"}}),
        );
        assert_eq!(result["reason_code"], "expired");
        assert_eq!(result["input_basis"]["remaining"], "1");
        assert!(result.get("body").is_none());
        assert_eq!(result["evidence_gap"], true);
    }
    #[test]
    fn retention_never_evicts_active() {
        let h = History::new("p".into());
        let active = h.begin();
        for _ in 0..RECORD_LIMIT + 10 {
            let mut r = h.begin();
            r.outcome = "rejected".into();
            h.update(r);
        }
        let state = h.state.lock().unwrap();
        assert!(state.records.iter().any(|r| r.request == active.request));
        assert_eq!(state.records.len(), RECORD_LIMIT);
        assert!(state.evicted > 0);
    }
    #[test]
    fn oversized_identifiers_cannot_escape_record_budget() {
        let h = History::new("p".into());
        let mut r = h.begin();
        r.model = Some("x".repeat(RECORD_BYTES));
        r.outcome = "rejected".into();
        h.update(r);
        let state = h.state.lock().unwrap();
        assert!(serde_json::to_vec(&state.records[0]).unwrap().len() <= RECORD_BYTES);
        assert!(state.records[0].evidence_gap);
    }
    #[test]
    fn measured_history_limits() {
        let h = History::new("measurement".into());
        for index in 0..RECORD_LIMIT {
            let mut record = h.begin();
            record.model = Some("m".repeat(512));
            record.outcome = "completed".into();
            record.decision = json!({"selection_reason":"highest_effective_priority","selection":{"provider":"p","model":"m","priority":index},"selected_evaluation":{"priority_evaluation":{"effective_priority":index,"selected_rule_id":"winner","priority_rules":[{"id":"winner","priority":index,"evaluation":{"outcome":"matched","input_basis":{"remaining":"1","total":"3","remaining_at_least_percent":"33.33"}}}]}}});
            h.update(record);
        }
        let state = h.state.lock().unwrap();
        let resident = state.records.iter().map(record_memory).sum::<usize>();
        let serialized = state.records.iter().map(encoded_size).sum::<usize>();
        assert!(resident <= HISTORY_BYTES);
        assert!(serialized <= HISTORY_BYTES);
        println!(
            "history_measurement records={} accounted_resident_bytes={} serialized_record_bytes={} per_record_bound={} total_record_bound={}",
            state.records.len(),
            resident,
            serialized,
            RECORD_BYTES,
            HISTORY_BYTES
        );
    }
}
