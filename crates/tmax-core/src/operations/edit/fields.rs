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
            "nonnegative-integer" => {
                let n = pair[1].parse::<i64>().map_err(|_| error())?;
                if n < 0 {
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
            ("region", "string"),
            ("overrides.name", "string"),
            ("overrides.doc", "string"),
            ("subscription.plan", "string"),
            ("subscription.billing_period", "string"),
            ("subscription.renewal_date", "date"),
            ("subscription.expiry_date", "date"),
            ("subscription.auto_renew", "boolean"),
            ("subscription.charge.amount", "string"),
            ("subscription.charge.currency", "string"),
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
        "alias" => &[("enabled", "boolean"), ("continuity", "string")],
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
