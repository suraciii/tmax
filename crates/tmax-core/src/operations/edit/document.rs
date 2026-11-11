use crate::{Error, Result};
use toml::Value;

pub(super) fn error() -> Error {
    Error::new(
        "invalid_arguments",
        "The requested resource, field, or option combination is invalid",
    )
}
pub(super) fn table() -> Value {
    Value::Table(toml::map::Map::new())
}
pub(super) fn get<'a>(root: &'a Value, path: &[&str]) -> Result<&'a Value> {
    path.iter()
        .try_fold(root, |v, key| v.get(*key).ok_or_else(error))
}
pub(super) fn get_mut<'a>(root: &'a mut Value, path: &[&str]) -> Result<&'a mut Value> {
    path.iter()
        .try_fold(root, |v, key| v.get_mut(*key).ok_or_else(error))
}
pub(super) fn put(root: &mut Value, path: &[&str], value: Value) -> Result<()> {
    let (last, parents) = path.split_last().ok_or_else(error)?;
    let mut current = root;
    for part in parents {
        let t = current.as_table_mut().ok_or_else(error)?;
        current = t.entry((*part).to_string()).or_insert_with(table);
    }
    current
        .as_table_mut()
        .ok_or_else(error)?
        .insert((*last).to_string(), value);
    Ok(())
}
pub(super) fn remove(root: &mut Value, path: &[&str]) -> Result<Option<Value>> {
    let (last, parents) = path.split_last().ok_or_else(error)?;
    let mut current = root;
    for part in parents {
        let Some(next) = current.get_mut(*part) else {
            return Ok(None);
        };
        current = next;
    }
    Ok(current.as_table_mut().ok_or_else(error)?.remove(*last))
}
pub(super) fn string(s: &str) -> Value {
    Value::String(s.into())
}
pub(super) fn strings(values: &[String]) -> Value {
    Value::Array(values.iter().map(|s| string(s)).collect())
}
pub(super) fn integer(s: &str) -> Result<Value> {
    s.parse::<i64>().map(Value::Integer).map_err(|_| error())
}
pub(super) fn date(s: &str) -> Result<Value> {
    let value: toml::value::Datetime = s.parse().map_err(|_| error())?;
    if value.date.is_none() || value.time.is_some() || value.offset.is_some() {
        return Err(error());
    }
    Ok(Value::Datetime(value))
}
pub(super) fn ensure(root: &mut Value, path: &[&str]) -> Result<()> {
    if get(root, path).is_err() {
        put(root, path, table())?;
    }
    Ok(())
}
pub(super) fn new_named(root: &mut Value, parent: &[&str], id: &str, value: Value) -> Result<()> {
    ensure(root, parent)?;
    let t = get_mut(root, parent)?.as_table_mut().ok_or_else(error)?;
    if t.contains_key(id) {
        return Err(Error::new(
            "invalid_arguments",
            "A resource with that exact ID already exists",
        ));
    }
    t.insert(id.into(), value);
    Ok(())
}
pub(super) fn sync_table(
    target: &mut dyn toml_edit::TableLike,
    before: Option<&Value>,
    after: &Value,
) -> Result<()> {
    sync_table_inner(target, before, after, false)
}

fn sync_table_inner(
    target: &mut dyn toml_edit::TableLike,
    before: Option<&Value>,
    after: &Value,
    inline: bool,
) -> Result<()> {
    let after = after.as_table().ok_or_else(error)?;
    let before = before.and_then(Value::as_table);
    if let Some(before) = before {
        for key in before.keys() {
            if !after.contains_key(key) {
                target.remove(key);
            }
        }
    }
    for (key, value) in after {
        let old = before.and_then(|b| b.get(key));
        if old == Some(value) {
            continue;
        }
        if value.is_table() {
            let inline_child = inline
                || target.get(key).is_some_and(|item| {
                    matches!(
                        item,
                        toml_edit::Item::Value(toml_edit::Value::InlineTable(_))
                    )
                });
            if !target.get(key).is_some_and(toml_edit::Item::is_table_like) {
                target.insert(
                    key,
                    if inline {
                        toml_edit::Item::Value(toml_edit::Value::InlineTable(
                            toml_edit::InlineTable::new(),
                        ))
                    } else {
                        toml_edit::Item::Table(toml_edit::Table::new())
                    },
                );
            }
            sync_table_inner(
                target
                    .get_mut(key)
                    .and_then(toml_edit::Item::as_table_like_mut)
                    .ok_or_else(error)?,
                old,
                value,
                inline_child,
            )?;
            if value.as_table().is_some_and(|table| table.is_empty())
                && let Some(item) = target.get_mut(key)
            {
                if inline_child {
                    if let Some(table) = item.as_inline_table_mut() {
                        table.set_dotted(false);
                    } else {
                        *item = toml_edit::Item::Value(toml_edit::Value::InlineTable(
                            toml_edit::InlineTable::new(),
                        ));
                    }
                } else if let Some(table) = item.as_table_mut() {
                    table.set_implicit(false);
                    table.set_dotted(false);
                } else {
                    *item = toml_edit::Item::Table(toml_edit::Table::new());
                }
            }
        } else {
            let wrapper = Value::Table(toml::map::Map::from_iter([("v".into(), value.clone())]));
            let doc = toml_edit::ser::to_document(&wrapper).map_err(|_| error())?;
            let mut replacement = doc.get("v").ok_or_else(error)?.clone();
            if let (Some(old), Some(new)) = (
                target.get(key).and_then(toml_edit::Item::as_value),
                replacement.as_value_mut(),
            ) {
                *new.decor_mut() = old.decor().clone();
            }
            target.insert(key, replacement);
        }
    }
    Ok(())
}

pub(super) fn move_renamed(
    document: &mut toml_edit::DocumentMut,
    renames: &[(String, String)],
) -> Result<()> {
    // Move renamed tables first so their comments and table ordering travel with the resource.
    for (old_name, new_name) in renames {
        if let Ok((provider_id, m)) = crate::config::qualified(old_name) {
            let (_, new_m) = crate::config::qualified(new_name)?;
            if let Some(models) = document
                .get_mut("providers")
                .and_then(|providers| providers.get_mut(provider_id))
                .and_then(|provider| provider.get_mut("models"))
                .and_then(toml_edit::Item::as_table_mut)
                && let Some(item) = models.remove(m)
            {
                models.insert(new_m, item);
            }
        } else if let Some(providers) = document
            .get_mut("providers")
            .and_then(toml_edit::Item::as_table_mut)
            && let Some(item) = providers.remove(old_name)
        {
            providers.insert(new_name, item);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_comments_and_unrelated_settings() {
        let mut doc: toml_edit::DocumentMut =
            "# personal\nversion=1\n[providers.x]\n# label\nintegration='deepseek'\nenabled=true\n"
                .parse()
                .unwrap();
        let before: Value = toml::from_str(&doc.to_string()).unwrap();
        let mut after = before.clone();
        put(
            &mut after,
            &["providers", "x", "enabled"],
            Value::Boolean(false),
        )
        .unwrap();
        sync_table(doc.as_table_mut(), Some(&before), &after).unwrap();
        assert!(doc.to_string().contains("# personal"));
        assert!(doc.to_string().contains("# label"));
        assert!(doc.to_string().contains("integration='deepseek'"));
    }
}
