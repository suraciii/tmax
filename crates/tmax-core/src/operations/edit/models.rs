use super::document::{
    date, error, get, get_mut, integer, new_named, put, remove, string, strings, table,
};
use super::fields::{apply_fields, require_change};
use super::references::rename_refs;
use crate::operations::arguments::{arg, args, boolean, duration, operand, required};
use crate::{Error, Request, Result, config};
use toml::Value;

pub(super) fn mutate(
    root: &mut Value,
    r: &Request,
    path: &str,
    renames: &mut Vec<(String, String)>,
) -> Result<()> {
    match path {
        "model add" => {
            let (p, m) = config::qualified(operand(r, 0)?)?;
            get(root, &["providers", p])?;
            let mut model = table();
            apply_fields(&mut model, r, "model")?;
            new_named(root, &["providers", p, "models"], m, model)?;
        }
        "model update" => {
            require_change(r)?;
            let (p, m) = config::qualified(operand(r, 0)?)?;
            apply_fields(get_mut(root, &["providers", p, "models", m])?, r, "model")?;
        }
        "model remove" => {
            let (p, m) = config::qualified(operand(r, 0)?)?;
            if remove(root, &["providers", p, "models", m])?.is_none() {
                return Err(error());
            }
        }
        "model rename" => {
            let old = operand(r, 0)?;
            let (p, m) = config::qualified(old)?;
            let new_m = operand(r, 1)?;
            let model = remove(root, &["providers", p, "models", m])?.ok_or_else(error)?;
            new_named(root, &["providers", p, "models"], new_m, model)?;
            if let Some(inputs) = get_mut(root, &["providers", p])?
                .get_mut("inputs")
                .and_then(Value::as_table_mut)
            {
                for input in inputs.iter_mut().map(|(_, value)| value) {
                    if let Some(models) = input.get_mut("models").and_then(Value::as_array_mut) {
                        for model in models {
                            if model.as_str() == Some(m) {
                                *model = string(new_m);
                            }
                        }
                    }
                }
            }
            let new = format!("{p}/{new_m}");
            rename_refs(root, old, &new, false)?;
            renames.push((old.into(), new));
        }
        "model override list replace" => {
            let (p, m) = config::qualified(operand(r, 0)?)?;
            let field = operand(r, 1)?;
            if !matches!(
                field,
                "overrides.modalities.input" | "overrides.modalities.output"
            ) {
                return Err(error());
            }
            let model = get_mut(root, &["providers", p, "models", m])?;
            put(
                model,
                &field.split('.').collect::<Vec<_>>(),
                strings(args(r, "value")),
            )?;
        }
        "model effort-map set" | "model effort-map remove" | "model effort-map clear" => {
            let (p, m) = config::qualified(operand(r, 0)?)?;
            let protocol = required(r, "protocol")?;
            if !matches!(protocol, "chat-completions" | "responses") {
                return Err(Error::new(
                    "invalid_arguments",
                    "Effort maps support only Chat Completions and Responses, not Messages",
                )
                .field("protocol"));
            }
            let model = get_mut(root, &["providers", p, "models", m])?;
            if path.ends_with("set") {
                put(
                    model,
                    &["effort_map", protocol, operand(r, 1)?],
                    string(operand(r, 2)?),
                )?;
            } else if path.ends_with("remove") {
                let input = operand(r, 1)?;
                if remove(model, &["effort_map", protocol, input])?.is_none() {
                    return Err(Error::new(
                        "invalid_arguments",
                        "No effort mapping exists for this protocol and input",
                    )
                    .field(format!(
                        "providers.{p}.models.{m}.effort_map.{protocol}.{input}"
                    )));
                }
            } else {
                remove(model, &["effort_map", protocol])?;
            }
            if get(model, &["effort_map", protocol])
                .is_ok_and(|mappings| mappings.as_table().is_some_and(|v| v.is_empty()))
            {
                remove(model, &["effort_map", protocol])?;
            }
            if model
                .get("effort_map")
                .is_some_and(|mappings| mappings.as_table().is_some_and(|v| v.is_empty()))
            {
                remove(model, &["effort_map"])?;
            }
        }
        "model reasoning add"
        | "model reasoning replace"
        | "model reasoning remove"
        | "model reasoning clear" => {
            let (p, m) = config::qualified(operand(r, 0)?)?;
            let model = get_mut(root, &["providers", p, "models", m])?;
            if get(model, &["overrides", "reasoning_options"]).is_err() {
                put(
                    model,
                    &["overrides", "reasoning_options"],
                    Value::Array(Vec::new()),
                )?;
            }
            let list = get_mut(model, &["overrides", "reasoning_options"])?
                .as_array_mut()
                .ok_or_else(error)?;
            if path.ends_with("clear") {
                list.clear();
            } else if path.ends_with("remove") {
                let index = index(r)?;
                if index >= list.len() {
                    return Err(error());
                }
                list.remove(index);
            } else {
                let mut entry = table();
                put(&mut entry, &["type"], string(required(r, "type")?))?;
                if arg(r, "empty-values").is_some() && !args(r, "value").is_empty() {
                    return Err(error());
                }
                if arg(r, "empty-values").is_some() || !args(r, "value").is_empty() {
                    put(&mut entry, &["values"], strings(args(r, "value")))?;
                }
                if path.ends_with("add") {
                    list.push(entry);
                } else {
                    let index = index(r)?;
                    if index >= list.len() {
                        return Err(error());
                    }
                    list[index] = entry;
                }
            }
        }
        "model schedule add"
        | "model schedule replace"
        | "model schedule remove"
        | "model schedule clear" => {
            let (p, m) = config::qualified(operand(r, 0)?)?;
            let model = get_mut(root, &["providers", p, "models", m])?;
            if model.get("schedule").is_none() {
                put(model, &["schedule"], Value::Array(Vec::new()))?;
            }
            let list = get_mut(model, &["schedule"])?
                .as_array_mut()
                .ok_or_else(error)?;
            if path.ends_with("clear") {
                list.clear();
            } else if path.ends_with("remove") {
                let index = index(r)?;
                if index >= list.len() {
                    return Err(error());
                }
                list.remove(index);
            } else {
                let mut rule = window(r)?;
                put(&mut rule, &["priority"], integer(required(r, "priority")?)?)?;
                if path.ends_with("add") {
                    list.push(rule);
                } else {
                    let index = index(r)?;
                    if index >= list.len() {
                        return Err(error());
                    }
                    list[index] = rule;
                }
            }
        }
        p if p.starts_with("model rule ") => {
            let (provider, model) = config::qualified(operand(r, 0)?)?;
            let m = get_mut(root, &["providers", provider, "models", model])?;
            if m.get("priority_rules").is_none() {
                put(m, &["priority_rules"], Value::Array(Vec::new()))?;
            }
            let list = get_mut(m, &["priority_rules"])?
                .as_array_mut()
                .ok_or_else(error)?;
            let action = r.command.last().map(String::as_str).unwrap_or("");
            if action == "clear" {
                list.clear();
            } else {
                let id = operand(r, 1)?;
                let pos = list
                    .iter()
                    .position(|r| r.get("id").and_then(Value::as_str) == Some(id));
                match action {
                    "add" | "replace" => {
                        if (action == "add") == pos.is_some() {
                            return Err(error());
                        }
                        let mut rule = table();
                        put(&mut rule, &["id"], string(id))?;
                        put(&mut rule, &["input"], string(required(r, "input")?))?;
                        put(&mut rule, &["priority"], integer(required(r, "priority")?)?)?;
                        put(
                            &mut rule,
                            &["reset_within_seconds"],
                            Value::Integer(duration(required(r, "reset-within")?)?),
                        )?;
                        if let Some(enabled) = arg(r, "enabled") {
                            put(&mut rule, &["enabled"], Value::Boolean(boolean(enabled)?))?;
                        }
                        if let Some(threshold) = arg(r, "remaining-at-least-percent") {
                            put(
                                &mut rule,
                                &["remaining_at_least_percent"],
                                string(threshold),
                            )?;
                        }
                        let gate = window(r)?;
                        if gate.as_table().is_some_and(|t| !t.is_empty()) {
                            put(&mut rule, &["time_window"], gate)?;
                        }
                        if let Some(pos) = pos {
                            list[pos] = rule;
                        } else {
                            list.push(rule);
                        }
                    }
                    "remove" => {
                        list.remove(pos.ok_or_else(error)?);
                    }
                    "enable" | "disable" => put(
                        &mut list[pos.ok_or_else(error)?],
                        &["enabled"],
                        Value::Boolean(action == "enable"),
                    )?,
                    "move" => {
                        if arg(r, "before").is_some() == arg(r, "last").is_some() {
                            return Err(error());
                        }
                        let pos = pos.ok_or_else(error)?;
                        let rule = list.remove(pos);
                        if let Some(before) = arg(r, "before") {
                            if before == id {
                                return Err(error());
                            }
                            let target = list
                                .iter()
                                .position(|r| r.get("id").and_then(Value::as_str) == Some(before))
                                .ok_or_else(error)?;
                            list.insert(target, rule);
                        } else {
                            list.push(rule);
                        }
                    }
                    _ => return Err(error()),
                }
            }
        }
        _ => return Err(error()),
    }
    Ok(())
}
fn index(r: &Request) -> Result<usize> {
    operand(r, 1)?.parse().map_err(|_| error())
}
fn window(r: &Request) -> Result<Value> {
    let mut v = table();
    for (flag, key) in [("weekday", "weekdays"), ("date", "dates")] {
        let a = args(r, flag);
        if !a.is_empty() {
            let values = if flag == "date" {
                a.iter().map(|s| date(s)).collect::<Result<Vec<_>>>()?
            } else {
                a.iter().map(|s| string(s)).collect()
            };
            put(&mut v, &[key], Value::Array(values))?;
        }
    }
    for (flag, key) in [("start", "start"), ("end", "end")] {
        if let Some(s) = arg(r, flag) {
            put(&mut v, &[key], string(s))?;
        }
    }
    for (flag, key) in [
        ("effective-from", "effective_from"),
        ("effective-until", "effective_until"),
    ] {
        if let Some(s) = arg(r, flag) {
            put(&mut v, &[key], date(s)?)?;
        }
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn reference_safe_model_rename_updates_native_defaults_and_input_coverage() {
        let mut root:Value=toml::from_str("version=1\n[providers.p]\nintegration='deepseek'\n[providers.p.models.old]\n[providers.p.inputs.q]\nkind='quota'\nsource='manual'\nscope='weekly'\nmodels=['old']\nmax_age_seconds=60\n[agents.a]\nagent='pi'\nscope='/native'\ndefault={provider='up',id='old',protocol='chat-completions'}\n[agents.a.providers.up]\nprovider='p'\nmodels=[{id='old',protocol='chat-completions'}]\n").unwrap();
        let r = Request {
            command: vec!["model".into(), "rename".into()],
            operands: vec!["p/old".into(), "new/with/slash".into()],
            arguments: BTreeMap::new(),
            dry_run: true,
            if_revision: None,
        };
        let mut renames = Vec::new();
        mutate(&mut root, &r, "model rename", &mut renames).unwrap();
        assert_eq!(renames, vec![("p/old".into(), "p/new/with/slash".into())]);
        let c = config::parse(&toml::to_string(&root).unwrap()).unwrap();
        assert_eq!(c.providers["p"].inputs["q"].models, ["new/with/slash"]);
        assert_eq!(c.agents["a"].default.as_ref().unwrap().id, "new/with/slash");
    }
}
