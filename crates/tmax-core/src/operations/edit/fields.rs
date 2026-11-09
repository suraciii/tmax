use super::document::{date, error, integer, put, remove, string};
use crate::operations::arguments::{args, boolean};
use crate::{Error, Request, Result};
use toml::Value;

pub(super) fn require_change(r: &Request) -> Result<()> {
    if r.arguments.keys().all(|k| k == "non-interactive") {
        return Err(Error::new(
            "invalid_arguments",
            "At least one explicit edit is required",
        ));
    }
    Ok(())
}
pub(super) fn apply_fields(value: &mut Value, r: &Request, owner: &str) -> Result<Vec<String>> {
    let clears = args(r, "clear");
    let sets = args(r, "set");
    let mut seen = std::collections::BTreeSet::new();
    for field in clears
        .iter()
        .chain(sets.as_chunks::<2>().0.iter().map(|p| &p[0]))
    {
        if !seen.insert(field) {
            return Err(Error::new(
                "invalid_arguments",
                "Duplicate or conflicting field operand",
            )
            .field(field));
        }
    }
    for field in clears {
        field_kind(owner, field, true)?;
    }
    for pair in sets.as_chunks::<2>().0 {
        field_kind(owner, &pair[0], false)?;
    }
    for field in clears {
        let path: Vec<_> = field.split('.').collect();
        remove(value, &path)?;
    }
    for pair in sets.as_chunks::<2>().0 {
        let kind = field_kind(owner, &pair[0], false)?;
        let v = match kind {
            "boolean" => Value::Boolean(boolean(&pair[1])?),
            "integer" => integer(&pair[1])?,
            "comma-u16-list" => Value::Array(
                pair[1]
                    .split(',')
                    .map(|part| {
                        part.trim()
                            .parse::<u16>()
                            .map(|n| Value::Integer(i64::from(n)))
                            .map_err(|_| error())
                    })
                    .collect::<Result<Vec<_>>>()?,
            ),
            "nonnegative-integer" => {
                let n = pair[1].parse::<i64>().map_err(|_| error())?;
                if n < 0 {
                    return Err(error());
                }
                Value::Integer(n)
            }
            "positive-integer" => {
                let n = pair[1].parse::<i64>().map_err(|_| error())?;
                if n <= 0 {
                    return Err(error());
                }
                Value::Integer(n)
            }
            "number" => {
                let n = pair[1].parse::<f64>().map_err(|_| error())?;
                if !n.is_finite() || n < 0.0 {
                    return Err(error());
                }
                Value::Float(n)
            }
            "date" => date(&pair[1])?,
            _ => string(&pair[1]),
        };
        let path: Vec<_> = pair[0].split('.').collect();
        if pair[0] == "overrides.interleaved" {
            remove(value, &["overrides", "interleaved"])?;
        }
        put(value, &path, v)?;
    }
    Ok(seen.into_iter().cloned().collect())
}
pub(in crate::operations) fn fields(owner: &str) -> &'static [(&'static str, &'static str)] {
    match owner {
        "provider" => &[
            ("metadata", "string"),
            ("integration", "string"),
            ("enabled", "boolean"),
            ("overrides.name", "string"),
            ("overrides.doc", "string"),
            ("subscription.plan", "string"),
            ("subscription.billing_period", "string"),
            ("subscription.renewal_date", "date"),
            ("subscription.expiry_date", "date"),
            ("subscription.auto_renew", "boolean"),
            ("subscription.charge.amount", "string"),
            ("subscription.charge.currency", "string"),
            ("failure_policy.enabled", "boolean"),
            ("failure_policy.errors.http_statuses", "comma-u16-list"),
            ("failure_policy.errors.connection_errors", "boolean"),
            ("failure_policy.errors.transport_errors", "boolean"),
            ("failure_policy.consecutive_failures", "integer"),
            ("failure_policy.priority", "integer"),
            ("failure_policy.duration_seconds", "integer"),
            (
                "quota_guard.max_observation_age_seconds",
                "positive-integer",
            ),
            ("quota_guard", "container"),
            ("failure_policy", "container"),
            ("failure_policy.errors", "container"),
            ("endpoints", "container"),
            ("overrides", "container"),
            ("subscription", "container"),
            ("subscription.charge", "container"),
        ],
        "model" => &[
            ("enabled", "boolean"),
            ("priority", "integer"),
            ("timezone", "string"),
            ("overrides.name", "string"),
            ("overrides.description", "string"),
            ("overrides.family", "string"),
            ("overrides.knowledge", "string"),
            ("overrides.release_date", "string"),
            ("overrides.last_updated", "string"),
            ("overrides.attachment", "boolean"),
            ("overrides.reasoning", "boolean"),
            ("overrides.tool_call", "boolean"),
            ("overrides.structured_output", "boolean"),
            ("overrides.temperature", "boolean"),
            ("overrides.open_weights", "boolean"),
            ("overrides.limit.context", "nonnegative-integer"),
            ("overrides.limit.input", "nonnegative-integer"),
            ("overrides.limit.output", "nonnegative-integer"),
            ("overrides.cost.input", "number"),
            ("overrides.cost.output", "number"),
            ("overrides.cost.reasoning", "number"),
            ("overrides.cost.cache_read", "number"),
            ("overrides.cost.cache_write", "number"),
            ("overrides.cost.cache_storage", "number"),
            ("overrides.interleaved", "boolean"),
            ("overrides.interleaved.field", "string"),
            ("overrides", "container"),
            ("overrides.limit", "container"),
            ("overrides.modalities", "container"),
            ("overrides.modalities.input", "container"),
            ("overrides.modalities.output", "container"),
            ("overrides.cost", "container"),
            ("overrides.reasoning_options", "container"),
        ],
        "alias" => &[
            ("enabled", "boolean"),
            ("continuity", "string"),
            ("fallback.http_statuses", "comma-u16-list"),
            ("fallback.connection_errors", "boolean"),
            ("fallback.transport_errors", "boolean"),
            ("fallback", "container"),
        ],
        _ => &[],
    }
}
fn field_kind(owner: &str, field: &str, clear: bool) -> Result<&'static str> {
    let kind = fields(owner)
        .iter()
        .find(|(name, _)| *name == field)
        .map(|(_, kind)| *kind)
        .ok_or_else(|| {
            Error::new(
                "invalid_arguments",
                "Unknown editable field for this resource",
            )
            .field(field)
        })?;
    if (kind == "container" && !clear) || (clear && field == "overrides.interleaved.field") {
        return Err(error());
    }
    Ok(kind)
}

#[cfg(test)]
mod tests {
    use super::super::document::table;
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn policy_status_lists_are_typed_and_whole_policies_can_be_cleared() {
        let mut request = Request {
            command: vec!["provider".into(), "update".into()],
            operands: vec!["p".into()],
            arguments: BTreeMap::from([(
                "set".into(),
                vec![
                    "failure_policy.errors.http_statuses".into(),
                    "429, 503".into(),
                    "failure_policy.priority".into(),
                    "-100".into(),
                ],
            )]),
            dry_run: true,
            if_revision: None,
        };
        let mut value = table();
        apply_fields(&mut value, &request, "provider").unwrap();
        assert_eq!(
            value["failure_policy"]["errors"]["http_statuses"],
            Value::Array(vec![Value::Integer(429), Value::Integer(503)])
        );
        assert_eq!(value["failure_policy"]["priority"].as_integer(), Some(-100));
        for invalid in ["", "429,", "429,no", "65536"] {
            request.arguments.get_mut("set").unwrap()[1] = invalid.into();
            assert!(apply_fields(&mut table(), &request, "provider").is_err());
        }
        request.arguments = BTreeMap::from([("clear".into(), vec!["failure_policy".into()])]);
        apply_fields(&mut value, &request, "provider").unwrap();
        assert!(value.get("failure_policy").is_none());
        assert!(field_kind("alias", "fallback", true).is_ok());
        assert!(field_kind("alias", "fallback", false).is_err());
    }
    #[test]
    fn quota_guard_age_is_positive_and_policy_can_be_cleared() {
        let mut request = Request {
            command: vec!["provider".into(), "update".into()],
            operands: vec!["p".into()],
            arguments: BTreeMap::from([(
                "set".into(),
                vec![
                    "quota_guard.max_observation_age_seconds".into(),
                    "3600".into(),
                ],
            )]),
            dry_run: true,
            if_revision: None,
        };
        let mut value = table();
        apply_fields(&mut value, &request, "provider").unwrap();
        assert_eq!(
            value["quota_guard"]["max_observation_age_seconds"].as_integer(),
            Some(3600)
        );
        for invalid in ["0", "-1", "not-an-integer"] {
            request.arguments.get_mut("set").unwrap()[1] = invalid.into();
            assert!(apply_fields(&mut table(), &request, "provider").is_err());
        }
        assert!(field_kind("provider", "quota_guard", false).is_err());
        request.arguments = BTreeMap::from([("clear".into(), vec!["quota_guard".into()])]);
        apply_fields(&mut value, &request, "provider").unwrap();
        assert!(value.get("quota_guard").is_none());
    }
    #[tokio::test]
    async fn quota_guard_edits_persist_and_zero_never_saves() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::storage::Store::at(directory.path().into());
        let original = "version=1\n[providers.p]\nintegration='deepseek'\n";
        store.write_raw("tmax.toml", original.as_bytes()).unwrap();
        let mut request = Request {
            command: vec!["provider".into(), "update".into()],
            operands: vec!["p".into()],
            arguments: BTreeMap::from([(
                "set".into(),
                vec![
                    "quota_guard.max_observation_age_seconds".into(),
                    "3600".into(),
                ],
            )]),
            dry_run: false,
            if_revision: None,
        };
        super::super::execute(&store, &request).await.unwrap();
        let saved = std::fs::read(store.root.join("tmax.toml")).unwrap();
        assert_eq!(
            crate::config::load(&store).unwrap().config.providers["p"]
                .quota_guard
                .as_ref()
                .unwrap()
                .max_observation_age_seconds,
            3600
        );
        request.arguments.get_mut("set").unwrap()[1] = "0".into();
        assert_eq!(
            super::super::execute(&store, &request)
                .await
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        assert_eq!(std::fs::read(store.root.join("tmax.toml")).unwrap(), saved);
        request.arguments = BTreeMap::from([("clear".into(), vec!["quota_guard".into()])]);
        super::super::execute(&store, &request).await.unwrap();
        assert!(
            crate::config::load(&store).unwrap().config.providers["p"]
                .quota_guard
                .is_none()
        );
    }
    #[tokio::test]
    async fn clearing_dotted_quota_guard_age_keeps_guard_enabled() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::storage::Store::at(directory.path().into());
        store
            .write_raw(
                "tmax.toml",
                b"version=1\n[providers.p]\nintegration='deepseek'\nquota_guard.max_observation_age_seconds=120\n",
            )
            .unwrap();
        let request = Request {
            command: vec!["provider".into(), "update".into()],
            operands: vec!["p".into()],
            arguments: BTreeMap::from([(
                "clear".into(),
                vec!["quota_guard.max_observation_age_seconds".into()],
            )]),
            dry_run: false,
            if_revision: None,
        };
        super::super::execute(&store, &request).await.unwrap();
        let loaded = crate::config::load(&store).unwrap();
        assert_eq!(
            loaded.config.providers["p"]
                .quota_guard
                .as_ref()
                .unwrap()
                .max_observation_age_seconds,
            3600
        );
        let saved = std::fs::read_to_string(store.root.join("tmax.toml")).unwrap();
        assert!(saved.contains("quota_guard"));
    }

    #[tokio::test]
    async fn clearing_inline_quota_guard_age_keeps_guard_enabled() {
        for text in [
            "version=1\nproviders = { p = { integration='deepseek', quota_guard = { max_observation_age_seconds=120 } } }\n",
            "version=1\nproviders = { p = { integration='deepseek', quota_guard.max_observation_age_seconds=120 } }\n",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let store = crate::storage::Store::at(directory.path().into());
            store.write_raw("tmax.toml", text.as_bytes()).unwrap();
            let request = Request {
                command: vec!["provider".into(), "update".into()],
                operands: vec!["p".into()],
                arguments: BTreeMap::from([(
                    "clear".into(),
                    vec!["quota_guard.max_observation_age_seconds".into()],
                )]),
                dry_run: false,
                if_revision: None,
            };
            super::super::execute(&store, &request).await.unwrap();
            let loaded = crate::config::load(&store).unwrap();
            assert_eq!(
                loaded.config.providers["p"]
                    .quota_guard
                    .as_ref()
                    .unwrap()
                    .max_observation_age_seconds,
                3600
            );
            let saved = std::fs::read_to_string(store.root.join("tmax.toml")).unwrap();
            assert!(saved.contains("quota_guard"), "{saved}");
        }
    }

    #[test]
    fn no_arbitrary_fields_and_parent_rebuild() {
        let r = Request {
            command: vec!["model".into(), "update".into()],
            operands: vec!["p/m".into()],
            arguments: BTreeMap::from([(
                "set".into(),
                vec![
                    "overrides.temperature".into(),
                    "false".into(),
                    "priority".into(),
                    "-1".into(),
                ],
            )]),
            dry_run: true,
            if_revision: None,
        };
        let mut value = table();
        apply_fields(&mut value, &r, "model").unwrap();
        assert_eq!(value["overrides"]["temperature"].as_bool(), Some(false));
        assert_eq!(value["priority"].as_integer(), Some(-1));
        assert!(field_kind("model", "temperature", false).is_err());
    }
}
