//! Lossless edits for the finite native settings tmax owns. Native credentials
//! are never decoded from auth stores; configuration bytes stay out of results.
use crate::{Error, Result};
use serde_json::Value;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) enum Format {
    Json,
    Yaml,
    Toml,
    Credential,
}

pub(super) fn parse(text: &str, format: Format) -> Result<Value> {
    if text.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    if format == Format::Yaml
        && text
            .lines()
            .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'))
    {
        return Ok(serde_json::json!({}));
    }
    let value = match format {
        Format::Credential => {
            serde_json::json!({"$credential": crate::config::revision(text.as_bytes())})
        }
        Format::Json => {
            let clean = clean_json(text)?;
            serde_json::from_str(&clean).map_err(|_| invalid())?
        }
        Format::Yaml => serde_yaml::from_str(text).map_err(|_| invalid())?,
        Format::Toml => {
            let value: toml::Value = toml::from_str(text).map_err(|_| invalid())?;
            serde_json::to_value(value).map_err(|_| invalid())?
        }
    };
    if !value.is_object() {
        return Err(invalid());
    }
    Ok(value)
}
fn invalid() -> Error {
    Error::new(
        "native_config_invalid",
        "Native configuration cannot be safely edited; inspect its syntax with the native owner",
    )
}

// Strip comments/trailing commas without changing byte offsets. Strings remain
// opaque, including URL slashes and escaped quotation marks.
fn clean_json(text: &str) -> Result<String> {
    let bytes = text.as_bytes();
    let mut clean = bytes.to_vec();
    // Qualified JSON native readers accept an initial UTF-8 BOM. Keep all edit
    // offsets relative to the original bytes instead of stripping its prefix.
    if bytes.starts_with(b"\xef\xbb\xbf") {
        clean[..3].fill(b' ');
    }
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i = string_end(bytes, i)?;
            continue;
        }
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                clean[i] = b' ';
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            clean[i] = b' ';
            clean[i + 1] = b' ';
            i += 2;
            while i + 1 < bytes.len() && !bytes[i..].starts_with(b"*/") {
                if bytes[i] != b'\n' && bytes[i] != b'\r' {
                    clean[i] = b' ';
                }
                i += 1;
            }
            if i + 1 >= bytes.len() {
                return Err(invalid());
            }
            clean[i] = b' ';
            clean[i + 1] = b' ';
            i += 2;
        } else {
            i += 1;
        }
    }
    i = 0;
    while i < clean.len() {
        if clean[i] == b'"' {
            i = string_end(&clean, i)?;
            continue;
        }
        if clean[i] == b',' {
            let mut j = i + 1;
            while j < clean.len() && clean[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < clean.len() && matches!(clean[j], b'}' | b']') {
                clean[i] = b' ';
            }
        }
        i += 1;
    }
    String::from_utf8(clean).map_err(|_| invalid())
}
fn string_end(bytes: &[u8], start: usize) -> Result<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Ok(i + 1),
            _ => i += 1,
        }
    }
    Err(invalid())
}
#[derive(Debug)]
struct Node {
    span: Range<usize>,
    members: Vec<(String, Range<usize>, Node)>,
}
fn json_node(clean: &str, pos: &mut usize) -> Result<Node> {
    let b = clean.as_bytes();
    while *pos < b.len() && b[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
    let start = *pos;
    let mut members = Vec::new();
    if b.get(*pos) == Some(&b'{') {
        *pos += 1;
        loop {
            while *pos < b.len() && (b[*pos].is_ascii_whitespace() || b[*pos] == b',') {
                *pos += 1;
            }
            if b.get(*pos) == Some(&b'}') {
                *pos += 1;
                break;
            }
            if b.get(*pos) != Some(&b'"') {
                return Err(invalid());
            }
            let ks = *pos;
            *pos = string_end(b, *pos)?;
            let key: String = serde_json::from_str(&clean[ks..*pos]).map_err(|_| invalid())?;
            while *pos < b.len() && b[*pos].is_ascii_whitespace() {
                *pos += 1;
            }
            if b.get(*pos) != Some(&b':') {
                return Err(invalid());
            }
            *pos += 1;
            let child = json_node(clean, pos)?;
            members.push((key, ks..*pos, child));
        }
    } else if b.get(*pos) == Some(&b'[') {
        let mut depth = 1;
        *pos += 1;
        while *pos < b.len() && depth > 0 {
            match b[*pos] {
                b'"' => {
                    *pos = string_end(b, *pos)?;
                    continue;
                }
                b'[' => depth += 1,
                b']' => depth -= 1,
                _ => {}
            }
            *pos += 1;
        }
        if depth != 0 {
            return Err(invalid());
        }
    } else if b.get(*pos) == Some(&b'"') {
        *pos = string_end(b, *pos)?;
    } else {
        while *pos < b.len() && !matches!(b[*pos], b',' | b'}' | b']' | b'\n' | b'\r') {
            *pos += 1;
        }
    }
    if start == *pos {
        return Err(invalid());
    }
    let mut end = *pos;
    while end > start && b[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    Ok(Node {
        span: start..end,
        members,
    })
}
fn nested(path: &[String], value: &Value) -> Value {
    path.iter()
        .rev()
        .fold(value.clone(), |v, key| serde_json::json!({key: v}))
}
fn json_edit(text: &str, path: &[String], value: Option<&Value>) -> Result<String> {
    let source = if text.trim().is_empty() { "{}\n" } else { text };
    let clean = clean_json(source)?;
    let root = json_node(&clean, &mut 0)?;
    let mut node = &root;
    for (depth, key) in path.iter().enumerate() {
        if let Some((_, field_span, child)) = node.members.iter().find(|(k, _, _)| k == key) {
            if depth + 1 != path.len() {
                node = child;
                continue;
            }
            let mut output = source.to_owned();
            if let Some(v) = value {
                output.replace_range(
                    child.span.clone(),
                    &serde_json::to_string(v).map_err(|_| invalid())?,
                );
            } else {
                if source.as_bytes()[field_span.clone()]
                    .iter()
                    .zip(&clean.as_bytes()[field_span.clone()])
                    .any(|(original, clean)| *original == b'/' && *clean == b' ')
                {
                    return Err(Error::new(
                        "native_conflict",
                        "Restoration would discard a comment inside an owned native collection; preserve or move it with the native owner",
                    ));
                }
                let b = clean.as_bytes();
                let mut end = field_span.end;
                while end < node.span.end - 1 && b[end].is_ascii_whitespace() {
                    end += 1;
                }
                if b.get(end) == Some(&b',') {
                    output.remove(end);
                    output.replace_range(field_span.clone(), "");
                } else {
                    let mut previous = field_span.start;
                    while previous > node.span.start && b[previous - 1].is_ascii_whitespace() {
                        previous -= 1;
                    }
                    output.replace_range(field_span.clone(), "");
                    if previous > node.span.start && b[previous - 1] == b',' {
                        output.remove(previous - 1);
                    }
                }
            }
            return Ok(output);
        }
        let Some(v) = value else {
            return Ok(source.to_owned());
        };
        if clean.as_bytes().get(node.span.start) != Some(&b'{') {
            return Err(invalid());
        }
        let at = node.span.end - 1;
        let encoded =
            serde_json::to_string(&nested(&path[depth + 1..], v)).map_err(|_| invalid())?;
        let key = serde_json::to_string(key).map_err(|_| invalid())?;
        let comma = if node.members.is_empty() { "" } else { "," };
        let mut output = source.to_owned();
        // The old trailing comma was blanked in clean, not in source.
        let trailing = source[node
            .members
            .last()
            .map_or(node.span.start + 1, |(_, r, _)| r.end)..at]
            .trim_start()
            .starts_with(',');
        output.insert_str(
            at,
            &format!("{}\n{key}: {encoded}\n", if trailing { "" } else { comma }),
        );
        return Ok(output);
    }
    Err(invalid())
}

// YAML edits are restricted to block mappings, or JSON-compatible flow values.
// Anchors and merge keys on an edited branch fail closed: modifying an alias's
// definition could otherwise change an unrelated native setting.
fn yaml_edit(text: &str, path: &[String], value: Option<&Value>) -> Result<String> {
    if text.trim_start().starts_with('{') {
        return json_edit(text, path, value);
    }
    let lines: Vec<(usize, &str)> = text
        .split_inclusive('\n')
        .scan(0, |offset, line| {
            let start = *offset;
            *offset += line.len();
            Some((start, line))
        })
        .collect();
    let mut lo = 0;
    let mut hi = lines.len();
    let mut indent = 0;
    for (depth, wanted) in path.iter().enumerate() {
        let mut found = None;
        for (i, (_, source_line)) in lines.iter().enumerate().take(hi).skip(lo) {
            let line = source_line.trim_end_matches(['\n', '\r']);
            let actual = line.len() - line.trim_start_matches(' ').len();
            let trimmed = line.trim_start();
            if actual != indent
                || trimmed.starts_with('#')
                || trimmed == "-"
                || trimmed.starts_with("- ")
            {
                continue;
            }
            if let Some(colon) = yaml_colon(&line[actual..]) {
                let key_text = &line[actual..actual + colon];
                let key: String = serde_yaml::from_str(key_text).map_err(|_| invalid())?;
                if key == *wanted {
                    found = Some((i, actual + colon + 1));
                    break;
                }
            }
        }
        if let Some((i, colon)) = found {
            let mut end = i + 1;
            while end < hi {
                let line = lines[end].1;
                let t = line.trim();
                let actual = line.len() - line.trim_start_matches(' ').len();
                let sequence_item = actual == indent && (t == "-" || t.starts_with("- "));
                if !t.is_empty() && !t.starts_with('#') && actual <= indent && !sequence_item {
                    break;
                }
                end += 1;
            }
            let start_byte = lines[i].0;
            let end_byte = lines.get(end).map_or(text.len(), |(o, _)| *o);
            let inline = lines[i].1[colon..].trim();
            if inline.starts_with('&') || inline.starts_with('*') {
                return Err(invalid());
            }
            if depth + 1 == path.len() {
                let mut output = text.to_owned();
                if let Some(v) = value {
                    let line_end = lines[i].1.trim_end_matches(['\n', '\r']);
                    let tail = &line_end[colon..];
                    let comment = yaml_comment(tail).map(|n| &tail[n..]).unwrap_or("");
                    // Refuse to erase comments inside an owned compound value.
                    if end > i + 1
                        && text[lines[i].0 + lines[i].1.len()..end_byte]
                            .lines()
                            .any(|l| yaml_comment(l).is_some())
                    {
                        return Err(Error::new(
                            "native_conflict",
                            "An edited native collection has user comments; preserve or move it with the native owner before changing this collection",
                        ));
                    }
                    let replacement = format!(
                        "{} {}{}{}\n",
                        &line_end[..colon],
                        serde_json::to_string(v).map_err(|_| invalid())?,
                        if comment.is_empty() { "" } else { " " },
                        comment
                    );
                    output.replace_range(start_byte..end_byte, &replacement);
                } else {
                    if text[start_byte..end_byte]
                        .lines()
                        .any(|l| yaml_comment(l).is_some())
                    {
                        return Err(Error::new(
                            "native_conflict",
                            "Restoration would discard a native comment; inspect the owned setting",
                        ));
                    }
                    output.replace_range(start_byte..end_byte, "");
                }
                return Ok(output);
            }
            if inline.starts_with('{') {
                let value_start = lines[i].0 + colon + lines[i].1[colon..].len()
                    - lines[i].1[colon..].trim_start().len();
                let raw = lines[i].1[colon..].trim();
                let changed = json_edit(raw, &path[depth + 1..], value)?.replace('\n', " ");
                let mut output = text.to_owned();
                output.replace_range(value_start..value_start + raw.len(), &changed);
                return Ok(output);
            }
            if !inline.is_empty() && !inline.starts_with('#') {
                return Err(invalid());
            }
            lo = i + 1;
            hi = end;
            indent = (lo..hi)
                .find_map(|j| {
                    let l = lines[j].1;
                    let t = l.trim();
                    if t.is_empty() || t.starts_with('#') {
                        None
                    } else {
                        Some(l.len() - l.trim_start_matches(' ').len())
                    }
                })
                .unwrap_or(indent + 2);
        } else {
            let Some(v) = value else {
                return Ok(text.to_owned());
            };
            let at = lines.get(hi).map_or(text.len(), |(offset, _)| *offset);
            let key = serde_json::to_string(wanted).map_err(|_| invalid())?;
            let mut addition = String::new();
            if at > 0 && !text[..at].ends_with('\n') {
                addition.push('\n');
            }
            addition.push_str(&format!(
                "{}{key}: {}\n",
                " ".repeat(indent),
                serde_json::to_string(&nested(&path[depth + 1..], v)).map_err(|_| invalid())?
            ));
            let mut output = text.to_owned();
            output.insert_str(at, &addition);
            return Ok(output);
        }
    }
    Err(invalid())
}
fn yaml_colon(s: &str) -> Option<usize> {
    let mut quote = None;
    let mut escape = false;
    for (i, c) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if quote == Some('"') && c == '\\' {
            escape = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
        } else if c == '\'' || c == '"' {
            quote = Some(c);
        } else if c == ':' && s[i + 1..].chars().next().is_none_or(char::is_whitespace) {
            return Some(i);
        }
    }
    None
}
fn yaml_comment(s: &str) -> Option<usize> {
    let mut quote = None;
    let mut escape = false;
    for (i, c) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if quote == Some('"') && c == '\\' {
            escape = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
        } else if c == '\'' || c == '"' {
            quote = Some(c);
        } else if c == '#' && (i == 0 || s.as_bytes()[i - 1].is_ascii_whitespace()) {
            return Some(i);
        }
    }
    None
}
fn toml_value(value: &Value) -> Result<toml_edit::Item> {
    match value {
        Value::Object(map) => {
            let mut t = toml_edit::Table::new();
            for (k, v) in map {
                t.insert(k, toml_value(v)?);
            }
            Ok(toml_edit::Item::Table(t))
        }
        _ => {
            let encoded = format!("value = {}", value);
            let parsed = encoded
                .parse::<toml_edit::DocumentMut>()
                .map_err(|_| invalid())?;
            Ok(parsed["value"].clone())
        }
    }
}
fn toml_edit(text: &str, path: &[String], value: Option<&Value>) -> Result<String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| invalid())?;
    let mut table = doc.as_table_mut();
    for key in &path[..path.len() - 1] {
        if !table.contains_key(key) {
            if value.is_none() {
                return Ok(text.to_owned());
            }
            table.insert(key, toml_edit::Item::Table(toml_edit::Table::new()));
        }
        table = table
            .get_mut(key)
            .and_then(toml_edit::Item::as_table_mut)
            .ok_or_else(invalid)?;
    }
    let key = path.last().ok_or_else(invalid)?;
    if let Some(v) = value {
        let mut item = toml_value(v)?;
        if let (Some(new), Some(old)) = (
            item.as_value_mut(),
            table.get(key).and_then(toml_edit::Item::as_value),
        ) {
            *new.decor_mut() = old.decor().clone();
        }
        table.insert(key, item);
    } else {
        if table.get(key).is_some_and(|item| {
            item.to_string()
                .lines()
                .any(|line| yaml_comment(line).is_some())
        }) {
            return Err(Error::new(
                "native_conflict",
                "Restoration would discard a native comment; inspect the owned setting",
            ));
        }
        table.remove(key);
    }
    Ok(doc.to_string())
}
pub(super) fn set(
    text: &str,
    format: Format,
    path: &[String],
    value: Option<&Value>,
) -> Result<String> {
    if path.is_empty() {
        return Err(invalid());
    }
    let output = match format {
        Format::Json => json_edit(text, path, value)?,
        Format::Yaml => yaml_edit(text, path, value)?,
        Format::Toml => toml_edit(text, path, value)?,
        Format::Credential => return Err(invalid()),
    };
    parse(&output, format)?;
    Ok(output)
}
pub(super) fn get<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter().try_fold(value, |v, k| v.get(k))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comment_only_yaml_is_an_empty_native_mapping() {
        let original = "# existing user note\n\n";
        assert_eq!(
            parse(original, Format::Yaml).unwrap(),
            serde_json::json!({})
        );
        let edited = set(
            original,
            Format::Yaml,
            &["modelRoles".into(), "default".into()],
            Some(&Value::from("fixture/m")),
        )
        .unwrap();
        assert!(edited.contains("# existing user note"));
        assert_eq!(
            parse(&edited, Format::Yaml).unwrap()["modelRoles"]["default"],
            "fixture/m"
        );
    }
    #[test]
    fn jsonc_preserves_comments_and_parameters() {
        let original =
            "{ // user\n \"providers\": {\"mine\": {\"body\": {\"temperature\": 0.4}}},\n}\n";
        let edited = set(
            original,
            Format::Json,
            &[
                "providers".into(),
                "local".into(),
                "settings".into(),
                "baseURL".into(),
            ],
            Some(&Value::from("http://127.0.0.1:17840")),
        )
        .unwrap();
        assert!(edited.contains("// user"));
        assert!(edited.contains("\"temperature\": 0.4"));
        assert_eq!(
            parse(&edited, Format::Json).unwrap()["providers"]["local"]["settings"]["baseURL"],
            "http://127.0.0.1:17840"
        );
    }
    #[test]
    fn initial_json_bom_survives_apply_and_restore_with_unrelated_unicode_edits() {
        let original = "\u{feff}{ // user note\n\"defaultModel\":\"old\",\"theme\":\"深色\"}\n";
        let path = ["defaultModel".into()];
        let applied = set(original, Format::Json, &path, Some(&Value::from("new"))).unwrap();
        assert!(applied.starts_with('\u{feff}'));
        assert!(applied.contains("// user note"));
        assert_eq!(
            parse(&applied, Format::Json).unwrap()["defaultModel"],
            "new"
        );
        let later = applied.replace("深色", "浅色");
        let restored = set(&later, Format::Json, &path, Some(&Value::from("old"))).unwrap();
        assert_eq!(restored, original.replace("深色", "浅色"));
        for malformed in [" \u{feff}{}", "{}\u{feff}", "\u{feff}{broken}"] {
            assert!(parse(malformed, Format::Json).is_err());
        }
    }
    #[test]
    fn yaml_preserves_unowned_comments() {
        let original =
            "# user\nagent:\n  temperature: 0.5 # leave\nmodel:\n  provider: old # selected\n";
        let edited = set(
            original,
            Format::Yaml,
            &["model".into(), "provider".into()],
            Some(&Value::from("new")),
        )
        .unwrap();
        assert!(edited.contains("temperature: 0.5 # leave"));
        assert!(edited.contains("# selected"));
        assert_eq!(
            parse(&edited, Format::Yaml).unwrap()["model"]["provider"],
            "new"
        );
    }
    #[test]
    fn yaml_edits_indentless_model_sequences_and_following_siblings() {
        let original = "# native config\nproviders:\n  local:\n    models:\n    - id: old\n      name: Old model\n    auth: oauth\n    unrelated: keep\n  other:\n    models: []\n";
        let edited = set(
            original,
            Format::Yaml,
            &["providers".into(), "local".into(), "auth".into()],
            Some(&Value::from("apiKey")),
        )
        .unwrap();
        let models = serde_json::json!([{"id":"new","api":"openai-completions"}]);
        let edited = set(
            &edited,
            Format::Yaml,
            &["providers".into(), "local".into(), "models".into()],
            Some(&models),
        )
        .unwrap();
        let result = parse(&edited, Format::Yaml).unwrap();
        assert_eq!(result["providers"]["local"]["models"], models);
        assert_eq!(result["providers"]["local"]["auth"], "apiKey");
        assert_eq!(result["providers"]["local"]["unrelated"], "keep");
        assert_eq!(
            result["providers"]["other"]["models"],
            serde_json::json!([])
        );
        assert!(edited.starts_with("# native config\n"));
    }
    #[test]
    fn toml_preserves_request_settings() {
        let edited = set(
            "# user\nmodel_reasoning_effort = 'high'\nmodel = 'old' # keep\n",
            Format::Toml,
            &["model".into()],
            Some(&Value::from("new")),
        )
        .unwrap();
        assert!(edited.contains("model_reasoning_effort = 'high'"));
        assert!(edited.contains("# keep"));
    }
    #[test]
    fn restoring_json_collection_does_not_discard_later_comment() {
        let error = set(
            "{\"providers\":{\"mine\":{/* later user note */}}}",
            Format::Json,
            &["providers".into(), "mine".into()],
            None,
        )
        .unwrap_err();
        assert_eq!(error.code, "native_conflict");
    }
    #[test]
    fn deleting_json_field_preserves_adjacent_comment() {
        let edited = set(
            "{\"model\":\"tmax\", // user note\n\"temperature\":0.4}",
            Format::Json,
            &["model".into()],
            None,
        )
        .unwrap();
        assert!(edited.contains("// user note"));
        assert_eq!(
            parse(&edited, Format::Json).unwrap(),
            serde_json::json!({"temperature":0.4})
        );
    }
    #[test]
    fn secret_comments_never_become_parse_errors() {
        let error = parse("{key: SUPER_SECRET}", Format::Json).unwrap_err();
        assert!(!error.to_string().contains("SUPER_SECRET"));
    }
}
