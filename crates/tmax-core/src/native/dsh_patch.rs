//! Native DSH patch rows addressed by stable entry IDs, never array indexes.
//! Unrelated rows and tagged expressions remain inert and byte-preserved.
use crate::{Error, Result};
use serde_json::{Map, Value, json};
use std::ops::Range;

fn invalid() -> Error {
    Error::new(
        "native_config_invalid",
        "DSH patches require an unambiguous block sequence of entry mappings; inspect the configuration with DSH",
    )
}
fn conflict() -> Error {
    Error::new(
        "native_conflict",
        "The edited DSH branch contains a tagged expression, alias, or comment that cannot be safely replaced",
    )
}
fn value(raw: serde_yaml::Value) -> Result<Value> {
    Ok(match raw {
        serde_yaml::Value::Tagged(tag) => {
            json!({"$dsh_tag":tag.tag.to_string(),"value":value(tag.value)?})
        }
        serde_yaml::Value::Sequence(items) => {
            Value::Array(items.into_iter().map(value).collect::<Result<_>>()?)
        }
        serde_yaml::Value::Mapping(items) => {
            let mut result = Map::new();
            for (key, raw) in items {
                let key = key.as_str().ok_or_else(invalid)?.to_owned();
                result.insert(key, value(raw)?);
            }
            Value::Object(result)
        }
        scalar => serde_json::to_value(scalar).map_err(|_| invalid())?,
    })
}
struct Row {
    id: Option<String>,
    span: Range<usize>,
    mapping: String,
    indent: usize,
    value: serde_yaml::Value,
}
fn rows(text: &str) -> Result<Vec<Row>> {
    if text.trim().is_empty()
        || text
            .lines()
            .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'))
        || text.trim() == "[]"
    {
        return Ok(Vec::new());
    }
    let parsed: serde_yaml::Value = serde_yaml::from_str(text).map_err(|_| invalid())?;
    let list = parsed.as_sequence().ok_or_else(invalid)?;
    if list.is_empty() {
        return Ok(Vec::new());
    }
    let mut offsets = Vec::new();
    let mut offset = 0;
    let mut root_indent = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        if !trimmed.trim().is_empty() && !trimmed.starts_with('#') && root_indent.is_none() {
            root_indent = Some(indent);
        }
        if Some(indent) == root_indent && (trimmed.starts_with("- ") || trimmed.trim_end() == "-") {
            offsets.push((offset, indent));
        }
        offset += line.len();
    }
    if offsets.len() != list.len() {
        return Err(invalid());
    }
    let mut result = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (index, raw) in list.iter().enumerate() {
        let map = raw.as_mapping().ok_or_else(invalid)?;
        let id = map
            .get(serde_yaml::Value::String("id".into()))
            .map(|id| id.as_str().map(str::to_owned).ok_or_else(invalid))
            .transpose()?;
        if let Some(id) = &id
            && !seen.insert(id.clone())
        {
            return Err(invalid());
        }
        let (start, indent) = offsets[index];
        let end = offsets
            .get(index + 1)
            .map_or(text.len(), |(start, _)| *start);
        let mut mapping = String::new();
        for (number, line) in text[start..end].split_inclusive('\n').enumerate() {
            if number == 0 {
                mapping.push_str(&" ".repeat(indent + 2));
                let body = &line[indent + 1..];
                mapping.push_str(body.strip_prefix(' ').unwrap_or(body));
            } else {
                mapping.push_str(line);
            }
        }
        result.push(Row {
            id,
            span: start..end,
            mapping,
            indent,
            value: raw.clone(),
        });
    }
    Ok(result)
}
pub(super) fn parse(text: &str) -> Result<Value> {
    let mut result = Map::new();
    for row in rows(text)? {
        if let Some(id) = row.id {
            let mut fields = value(row.value)?.as_object().ok_or_else(invalid)?.clone();
            fields.remove("id");
            result.insert(id, Value::Object(fields));
        } else {
            // Unaddressed inserts still count as content during restoration.
            result.insert("$unaddressed".into(), json!(true));
        }
    }
    Ok(Value::Object(result))
}
fn tagged(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.contains_key("$dsh_tag") || map.values().any(tagged),
        Value::Array(values) => values.iter().any(tagged),
        _ => false,
    }
}
pub(super) fn set(text: &str, path: &[String], desired: Option<&Value>) -> Result<String> {
    let id = path.first().ok_or_else(invalid)?;
    if id.starts_with('$') {
        return Err(invalid());
    }
    let all = rows(text)?;
    let current = parse(text)?;
    for depth in 1..=path.len() {
        if let Some(branch) = super::document::get(&current, &path[..depth])
            && branch.get("$dsh_tag").is_some()
        {
            return Err(conflict());
        }
    }
    if super::document::get(&current, path).is_some_and(tagged) {
        return Err(conflict());
    }
    if let Some(row) = all.iter().find(|row| row.id.as_ref() == Some(id)) {
        let changed = if path.len() == 1 {
            if row
                .mapping
                .lines()
                .any(|line| super::document::yaml_comment(line).is_some())
            {
                return Err(conflict());
            }
            match desired {
                None => String::new(),
                Some(fields) => render(id, fields, row.indent)?,
            }
        } else {
            let mapping = super::document::yaml_edit(&row.mapping, &path[1..], desired)?;
            let prefix = row.indent + 2;
            if mapping.len() < prefix
                || !mapping.as_bytes()[..prefix]
                    .iter()
                    .all(|byte| *byte == b' ')
            {
                return Err(invalid());
            }
            let mut output = String::new();
            output.push_str(&" ".repeat(row.indent));
            output.push_str("- ");
            output.push_str(&mapping[prefix..]);
            output
        };
        let mut output = text.to_owned();
        output.replace_range(row.span.clone(), &changed);
        if rows(&output)?.is_empty() && !output.lines().any(|line| line.trim() == "[]") {
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str("[]\n");
        }
        parse(&output)?;
        Ok(output)
    } else {
        let Some(desired) = desired else {
            return Ok(text.to_owned());
        };
        let fields = path[1..]
            .iter()
            .rev()
            .fold(desired.clone(), |value, key| json!({key:value}));
        let mut output = if all.is_empty() {
            text.split_inclusive('\n')
                .filter(|line| line.trim() != "[]")
                .collect::<String>()
        } else {
            text.to_owned()
        };
        if !output.is_empty() && !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&render(
            id,
            &fields,
            all.first().map_or(0, |row| row.indent),
        )?);
        parse(&output)?;
        Ok(output)
    }
}
fn render(id: &str, fields: &Value, indent: usize) -> Result<String> {
    let mut map = fields.as_object().ok_or_else(invalid)?.clone();
    map.insert("id".into(), json!(id));
    let mapping = serde_json::to_string(&map).map_err(|_| invalid())?;
    Ok(format!("{}- {mapping}\n", " ".repeat(indent)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_edits_preserve_unrelated_expressions_and_restore_by_identity() {
        let original = "# profile\n- id: llm-pi-ai\n  config:\n    providers:\n      user: {apiKeyEnv: USER_KEY}\n- id: other\n  config:\n    value: !!js process.env.USER_VALUE\n";
        let path = [
            "llm-pi-ai".into(),
            "config".into(),
            "providers".into(),
            "tmax".into(),
        ];
        let applied = set(original, &path, Some(&json!({"api":"openai-responses"}))).unwrap();
        assert_eq!(
            parse(&applied).unwrap()["llm-pi-ai"]["config"]["providers"]["user"],
            json!({"apiKeyEnv":"USER_KEY"})
        );
        assert!(applied.contains("value: !!js process.env.USER_VALUE"));
        let reordered = format!("- id: new\n  config: {{keep: true}}\n{applied}");
        let restored = set(&reordered, &path, None).unwrap();
        assert_eq!(parse(&restored).unwrap()["new"]["config"]["keep"], true);
        assert!(
            parse(&restored).unwrap()["llm-pi-ai"]["config"]["providers"]
                .get("tmax")
                .is_none()
        );
    }
    #[test]
    fn cross_row_aliases_and_unrelated_comment_indentation_survive() {
        let original = "- id: first\n  config: &shared {keep: true}\n# next row note\n- id: llm-pi-ai\n  config:\n    unrelated: *shared\n    providers: {}\n";
        let path = [
            "llm-pi-ai".into(),
            "config".into(),
            "providers".into(),
            "mine".into(),
        ];
        let applied = set(original, &path, Some(&json!({"api":"openai-responses"}))).unwrap();
        assert!(
            applied.starts_with("- id: first\n  config: &shared {keep: true}\n# next row note\n")
        );
        assert!(applied.contains("unrelated: *shared"));
        assert_eq!(
            parse(&applied).unwrap()["llm-pi-ai"]["config"]["unrelated"]["keep"],
            true
        );
    }
    #[test]
    fn last_row_restoration_keeps_comments_and_an_explicit_sequence() {
        for original in ["", "[]\n", "# initial note\n[]\n"] {
            let path = vec!["credentials".into()];
            let applied = set(original, &path, Some(&json!({"disabled":false}))).unwrap();
            let edited = format!("# later note\n{applied}");
            let restored = set(&edited, &path, None).unwrap();
            assert!(restored.contains("# later note"));
            if original.contains("# initial note") {
                assert!(restored.contains("# initial note"));
            }
            assert_eq!(
                serde_yaml::from_str::<serde_yaml::Value>(&restored).unwrap(),
                serde_yaml::Value::Sequence(vec![])
            );
            let reapplied = set(&restored, &path, Some(&json!({"disabled":false}))).unwrap();
            assert_eq!(parse(&reapplied).unwrap()["credentials"]["disabled"], false);
        }
    }
    #[test]
    fn ambiguous_rows_and_edited_expressions_fail_closed() {
        assert!(parse("- id: same\n- id: same\n").is_err());
        assert!(
            set(
                "- id: llm\n  config: !!js process.env.VALUE\n",
                &["llm".into(), "config".into(), "provider".into()],
                Some(&json!("new"))
            )
            .is_err()
        );
    }
}
