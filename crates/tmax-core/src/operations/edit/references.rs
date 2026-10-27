use super::document::{error, put, string};
use crate::Result;
use std::collections::BTreeMap;
use toml::Value;

pub(super) fn rename_refs(
    root: &mut Value,
    old: &str,
    new: &str,
    provider_rename: bool,
) -> Result<()> {
    if let Some(list) = root.get_mut("model_aliases").and_then(Value::as_array_mut) {
        for a in list {
            if let Some(c) = a.get_mut("candidates").and_then(Value::as_array_mut) {
                for name in c {
                    if let Some(s) = name.as_str()
                        && (s == old || provider_rename && s.starts_with(&format!("{old}/")))
                    {
                        let replacement = if s == old {
                            new.to_string()
                        } else {
                            format!("{new}/{}", &s[old.len() + 1..])
                        };
                        *name = Value::String(replacement);
                    }
                }
            }
            if let Some(efforts) = a
                .get_mut("candidate_reasoning_effort")
                .and_then(Value::as_table_mut)
            {
                let renamed: Vec<_> = efforts
                    .keys()
                    .filter_map(|name| {
                        if name == old {
                            Some((name.clone(), new.to_string()))
                        } else if provider_rename && name.starts_with(&format!("{old}/")) {
                            Some((name.clone(), format!("{new}/{}", &name[old.len() + 1..])))
                        } else {
                            None
                        }
                    })
                    .collect();
                for (old, new) in renamed {
                    let effort = efforts.remove(&old).expect("existing effort");
                    efforts.insert(new, effort);
                }
            }
        }
    }
    if let Some(agents) = root.get_mut("agents").and_then(Value::as_table_mut) {
        for target in agents.iter_mut().map(|(_, value)| value) {
            let mut changed = BTreeMap::new();
            if let Some(entries) = target.get_mut("providers").and_then(Value::as_table_mut) {
                for (name, e) in entries {
                    let direct = e
                        .get("provider")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    if provider_rename && direct.as_deref() == Some(old) {
                        put(e, &["provider"], string(new))?;
                    }
                    if let Some(models) = e.get_mut("models").and_then(Value::as_array_mut) {
                        for m in models {
                            let Some(id) = m.get("id").and_then(Value::as_str).map(str::to_string)
                            else {
                                continue;
                            };
                            let direct_model =
                                direct.as_ref().is_some_and(|p| format!("{p}/{id}") == old);
                            let replacement = if direct_model {
                                Some(new.split_once('/').ok_or_else(error)?.1.to_string())
                            } else if direct.is_none()
                                && (id == old
                                    || provider_rename && id.starts_with(&format!("{old}/")))
                            {
                                Some(if id == old {
                                    new.into()
                                } else {
                                    format!("{new}/{}", &id[old.len() + 1..])
                                })
                            } else {
                                None
                            };
                            if let Some(new_id) = replacement {
                                changed.insert((name.clone(), id), new_id.clone());
                                put(m, &["id"], string(&new_id))?;
                            }
                        }
                    }
                }
            }
            if let Some(default) = target.get_mut("default") {
                rename_native_ref(default, &changed)?;
            }
            if let Some(roles) = target.get_mut("roles").and_then(Value::as_table_mut) {
                for role in roles.iter_mut().map(|(_, value)| value) {
                    rename_native_ref(role, &changed)?;
                }
            }
        }
    }
    Ok(())
}
fn rename_native_ref(r: &mut Value, changes: &BTreeMap<(String, String), String>) -> Result<()> {
    if let (Some(p), Some(id)) = (
        r.get("provider").and_then(Value::as_str),
        r.get("id").and_then(Value::as_str),
    ) && let Some(new) = changes.get(&(p.into(), id.into()))
    {
        put(r, &["id"], string(new))?;
    }
    Ok(())
}
pub(super) fn rename_alias_refs(
    root: &mut Value,
    old: &str,
    new: &str,
    protocol: &str,
) -> Result<()> {
    if let Some(targets) = root.get_mut("agents").and_then(Value::as_table_mut) {
        for t in targets.iter_mut().map(|(_, value)| value) {
            let mut names = Vec::new();
            if let Some(entries) = t.get_mut("providers").and_then(Value::as_table_mut) {
                for (name, e) in entries {
                    if e.get("gateway").and_then(Value::as_bool) == Some(true)
                        && let Some(models) = e.get_mut("models").and_then(Value::as_array_mut)
                    {
                        for m in models {
                            if m.get("id").and_then(Value::as_str) == Some(old)
                                && m.get("protocol").and_then(Value::as_str) == Some(protocol)
                            {
                                put(m, &["id"], string(new))?;
                                names.push(name.clone());
                            }
                        }
                    }
                }
            }
            let update = |r: &mut Value| -> Result<()> {
                if r.get("id").and_then(Value::as_str) == Some(old)
                    && r.get("protocol").and_then(Value::as_str) == Some(protocol)
                    && r.get("provider")
                        .and_then(Value::as_str)
                        .is_some_and(|p| names.iter().any(|n| n == p))
                {
                    put(r, &["id"], string(new))?;
                }
                Ok(())
            };
            if let Some(d) = t.get_mut("default") {
                update(d)?;
            }
            if let Some(roles) = t.get_mut("roles").and_then(Value::as_table_mut) {
                for r in roles.iter_mut().map(|(_, value)| value) {
                    update(r)?;
                }
            }
        }
    }
    Ok(())
}
