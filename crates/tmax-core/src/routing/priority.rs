use super::calendar::{Window, calendar_at, invalid};
use crate::{
    Error, Result,
    config::{self, Config, Model, Provider},
    inputs::{self, Snapshot},
};
use chrono::{DateTime, Duration, Offset, Utc};
use chrono_tz::Tz;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

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
    // The parent validates model coverage with its map key; this validates all remaining rule semantics.
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

pub(super) fn evaluate(
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

#[cfg(test)]
mod tests {
    use super::evaluate;
    use crate::{
        config::{self, Config, Model, Provider, TimeWindow},
        inputs::{self, Snapshot},
        routing::{explain, test_support::weekly},
        storage::Store,
    };
    use chrono::{Duration, Utc};
    use std::collections::BTreeMap;

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
    #[test]
    fn fractional_horizon_does_not_round_into_match() {
        let at = inputs::ExactInstant::parse("2026-10-23T11:00:00Z").unwrap();
        assert!(inputs::within_horizon("2026-10-23T12:00:00Z", &at, 3600).unwrap());
        assert!(!inputs::within_horizon("2026-10-23T12:00:00.000000001Z", &at, 3600).unwrap());
        assert!(!inputs::within_horizon("2026-10-23T12:00:00.0000000001Z", &at, 3600).unwrap());
    }
}
