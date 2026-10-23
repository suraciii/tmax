use super::{arg, args, boolean, duration, operand, required};
use crate::{
    Error, Request, Result,
    config::{self, Config},
    storage::Store,
};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use toml::Value;

pub fn is_edit(path: &str) -> bool {
    matches!(
        path,
        "provider add"
            | "provider update"
            | "provider rename"
            | "provider remove"
            | "provider key set"
            | "provider auth set"
            | "provider auth clear"
            | "provider login"
            | "provider logout"
            | "provider input add"
            | "provider input replace"
            | "provider input remove"
            | "model add"
            | "model update"
            | "model rename"
            | "model remove"
            | "model override list replace"
            | "model reasoning add"
            | "model reasoning replace"
            | "model reasoning remove"
            | "model reasoning clear"
            | "model schedule add"
            | "model schedule replace"
            | "model schedule remove"
            | "model schedule clear"
            | "model rule add"
            | "model rule replace"
            | "model rule remove"
            | "model rule move"
            | "model rule enable"
            | "model rule disable"
            | "model rule clear"
            | "model alias add"
            | "model alias update"
            | "model alias candidates add"
            | "model alias candidates remove"
            | "model alias rename"
            | "model alias remove"
            | "agent add"
            | "agent update"
            | "agent rename"
            | "agent remove"
            | "agent provider add"
            | "agent provider update"
            | "agent provider remove"
            | "agent model add"
            | "agent model remove"
            | "agent model clear"
            | "agent default set"
            | "agent default clear"
            | "agent role set"
            | "agent role remove"
            | "agent role clear"
            | "network proxy"
            | "network direct"
            | "proxy configure"
            | "proxy key replace"
            | "proxy remove"
            | "proxy autostart enable"
            | "proxy autostart disable"
    )
}
fn error() -> Error {
    Error::new(
        "invalid_arguments",
        "The requested resource, field, or option combination is invalid",
    )
}
fn table() -> Value {
    Value::Table(toml::map::Map::new())
}
fn get<'a>(root: &'a Value, path: &[&str]) -> Result<&'a Value> {
    path.iter()
        .try_fold(root, |v, key| v.get(*key).ok_or_else(error))
}
fn get_mut<'a>(root: &'a mut Value, path: &[&str]) -> Result<&'a mut Value> {
    path.iter()
        .try_fold(root, |v, key| v.get_mut(*key).ok_or_else(error))
}
fn put(root: &mut Value, path: &[&str], value: Value) -> Result<()> {
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
fn remove(root: &mut Value, path: &[&str]) -> Result<Option<Value>> {
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
fn string(s: &str) -> Value {
    Value::String(s.into())
}
fn strings(values: &[String]) -> Value {
    Value::Array(values.iter().map(|s| string(s)).collect())
}
fn integer(s: &str) -> Result<Value> {
    s.parse::<i64>().map(Value::Integer).map_err(|_| error())
}
fn date(s: &str) -> Result<Value> {
    let value: toml::value::Datetime = s.parse().map_err(|_| error())?;
    if value.date.is_none() || value.time.is_some() || value.offset.is_some() {
        return Err(error());
    }
    Ok(Value::Datetime(value))
}
fn index(r: &Request) -> Result<usize> {
    operand(r, 1)?.parse().map_err(|_| error())
}
fn ensure(root: &mut Value, path: &[&str]) -> Result<()> {
    if get(root, path).is_err() {
        put(root, path, table())?;
    }
    Ok(())
}
fn new_named(root: &mut Value, parent: &[&str], id: &str, value: Value) -> Result<()> {
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
fn require_change(r: &Request) -> Result<()> {
    if r.arguments
        .keys()
        .all(|k| matches!(k.as_str(), "non-interactive" | "unlock-stdin" | "unlock-fd"))
    {
        return Err(Error::new(
            "invalid_arguments",
            "At least one explicit edit is required",
        ));
    }
    Ok(())
}
fn apply_fields(value: &mut Value, r: &Request, owner: &str) -> Result<Vec<String>> {
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
pub(super) fn fields(owner: &str) -> &'static [(&'static str, &'static str)] {
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
            ("tier", "string"),
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
    if arg(r, "all-day").is_some() {
        put(&mut v, &["all_day"], Value::Boolean(true))?;
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
fn selection(id: &str, p: &str) -> Value {
    Value::Table(toml::map::Map::from_iter([
        ("id".into(), string(id)),
        ("protocol".into(), string(p)),
    ]))
}
fn reference(name: &str, id: &str, p: &str) -> Value {
    let mut v = selection(id, p);
    v.as_table_mut()
        .unwrap()
        .insert("provider".into(), string(name));
    v
}
fn aliases_mut(root: &mut Value) -> Result<&mut Vec<Value>> {
    if root.get("model_aliases").is_none() {
        put(root, &["model_aliases"], Value::Array(Vec::new()))?;
    }
    get_mut(root, &["model_aliases"])?
        .as_array_mut()
        .ok_or_else(error)
}
fn alias_position(root: &Value, id: &str, protocol: &str) -> Result<usize> {
    root.get("model_aliases")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter().position(|a| {
                a.get("id").and_then(Value::as_str) == Some(id)
                    && a.get("protocol").and_then(Value::as_str) == Some(protocol)
            })
        })
        .ok_or_else(error)
}
fn rename_refs(root: &mut Value, old: &str, new: &str, provider_rename: bool) -> Result<()> {
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
fn mutate(root: &mut Value, r: &Request, store: &Store) -> Result<(Json, Vec<(String, String)>)> {
    let path = r.command.join(" ");
    let mut renames = Vec::new();
    let mut info = json!({"requested":path});
    match path.as_str() {
        "provider add" => {
            let id = operand(r, 0)?;
            let mut p = table();
            for key in ["metadata", "integration", "region"] {
                if let Some(v) = arg(r, key) {
                    if args(r, "set")
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .any(|p| p[0] == key)
                    {
                        return Err(error());
                    }
                    put(&mut p, &[key], string(v))?;
                }
            }
            apply_fields(&mut p, r, "provider")?;
            endpoints(&mut p, r)?;
            new_named(root, &["providers"], id, p)?;
            info["provider"] = json!(id);
        }
        "provider update" => {
            require_change(r)?;
            let p = get_mut(root, &["providers", operand(r, 0)?])?;
            apply_fields(p, r, "provider")?;
            endpoints(p, r)?;
        }
        "provider rename" => {
            let old = operand(r, 0)?;
            let new = operand(r, 1)?;
            let p = remove(root, &["providers", old])?.ok_or_else(error)?;
            new_named(root, &["providers"], new, p)?;
            rename_refs(root, old, new, true)?;
            renames.push((old.into(), new.into()));
        }
        "provider remove" => {
            let id = operand(r, 0)?;
            info["retained_authentication"] = json!(get(root, &["providers", id])?.get("auth"));
            if remove(root, &["providers", id])?.is_none() {
                return Err(error());
            }
            info["native_files"] = json!("unchanged; direct consumers can continue outside tmax");
        }
        "provider input add" | "provider input replace" => {
            let p = operand(r, 0)?;
            let name = operand(r, 1)?;
            let mut d = table();
            for key in ["kind", "source", "scope"] {
                put(&mut d, &[key], string(required(r, key)?))?;
            }
            if args(r, "model").is_empty() {
                return Err(error());
            }
            put(&mut d, &["models"], strings(args(r, "model")))?;
            put(
                &mut d,
                &["max_age_seconds"],
                Value::Integer(duration(required(r, "max-age")?)?),
            )?;
            if let Some(interval) = arg(r, "refresh-every") {
                put(
                    &mut d,
                    &["refresh_interval_seconds"],
                    Value::Integer(duration(interval)?),
                )?;
            }
            get(root, &["providers", p])?;
            if path.ends_with("add") {
                new_named(root, &["providers", p, "inputs"], name, d)?;
            } else {
                get(root, &["providers", p, "inputs", name])?;
                put(root, &["providers", p, "inputs", name], d)?;
            }
        }
        "provider input remove" => {
            if remove(
                root,
                &["providers", operand(r, 0)?, "inputs", operand(r, 1)?],
            )?
            .is_none()
            {
                return Err(error());
            }
        }
        "provider key set" => {
            let id = operand(r, 0)?;
            get(root, &["providers", id])?;
            let reference = arg(r, "key-ref").unwrap_or("__credential_provision_pending__");
            if arg(r, "key-ref").is_some() && arg(r, "key-stdin").is_some() {
                return Err(error());
            }
            put(
                root,
                &["providers", id, "auth"],
                Value::Table(toml::map::Map::from_iter([
                    ("method".into(), string("api-key")),
                    ("key_ref".into(), string(reference)),
                ])),
            )?;
        }
        "provider auth set" => {
            if required(r, "method")? != "login" {
                return Err(error());
            }
            let id = operand(r, 0)?;
            get(root, &["providers", id])?;
            let mut auth = table();
            put(&mut auth, &["method"], string("login"))?;
            put(&mut auth, &["via"], string(required(r, "via")?))?;
            if let Some(s) = arg(r, "session-ref") {
                put(&mut auth, &["session_ref"], string(s))?;
            }
            put(root, &["providers", id, "auth"], auth)?;
        }
        "provider auth clear" | "provider logout" => {
            get(root, &["providers", operand(r, 0)?])?;
            remove(root, &["providers", operand(r, 0)?, "auth"])?;
        }
        "provider login" => {
            let id = operand(r, 0)?;
            let p = get(root, &["providers", id])?;
            let via = arg(r, "via")
                .or_else(|| {
                    p.get("auth")
                        .and_then(|a| a.get("via"))
                        .and_then(Value::as_str)
                })
                .ok_or_else(|| {
                    Error::new(
                        "login_source_required",
                        "Select an explicit supported native login source",
                    )
                })?
                .to_string();
            let auth = Value::Table(toml::map::Map::from_iter([
                ("method".into(), string("login")),
                ("via".into(), string(&via)),
            ]));
            put(root, &["providers", id, "auth"], auth)?;
            info["via"] = json!(via);
        }
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
        p if p.starts_with("model alias ") => {
            let id = operand(r, 0)?;
            let protocol = required(r, "protocol")?;
            let action = r.command.last().map(String::as_str).unwrap_or("");
            if r.command.len() == 4 {
                let pos = alias_position(root, id, protocol)?;
                let alias = &mut aliases_mut(root)?[pos];
                if alias.get("tier").is_some() {
                    return Err(error());
                }
                let candidates = alias
                    .get_mut("candidates")
                    .and_then(Value::as_array_mut)
                    .ok_or_else(error)?;
                if args(r, "candidate").is_empty() {
                    return Err(error());
                }
                for name in args(r, "candidate") {
                    let pos = candidates.iter().position(|v| v.as_str() == Some(name));
                    if action == "add" {
                        if pos.is_some() {
                            return Err(error());
                        }
                        candidates.push(string(name));
                    } else {
                        candidates.remove(pos.ok_or_else(error)?);
                    }
                }
            } else if action == "add" {
                let mut alias = table();
                put(&mut alias, &["id"], string(id))?;
                put(&mut alias, &["protocol"], string(protocol))?;
                alias_source(&mut alias, r, true)?;
                apply_fields(&mut alias, r, "alias")?;
                aliases_mut(root)?.push(alias);
            } else {
                let pos = alias_position(root, id, protocol)?;
                match action {
                    "remove" => {
                        aliases_mut(root)?.remove(pos);
                    }
                    "update" => {
                        require_change(r)?;
                        let a = &mut aliases_mut(root)?[pos];
                        if let Some(new) = arg(r, "new-protocol") {
                            put(a, &["protocol"], string(new))?;
                        }
                        alias_source(a, r, false)?;
                        apply_fields(a, r, "alias")?;
                    }
                    "rename" => {
                        let new = operand(r, 1)?;
                        put(&mut aliases_mut(root)?[pos], &["id"], string(new))?;
                        rename_alias_refs(root, id, new, protocol)?;
                    }
                    _ => return Err(error()),
                }
            }
        }
        "agent add" => {
            let id = operand(r, 0)?;
            let mut target = table();
            put(&mut target, &["agent"], string(required(r, "agent")?))?;
            put(&mut target, &["scope"], string(required(r, "path")?))?;
            put(&mut target, &["providers"], table())?;
            new_named(root, &["agents"], id, target)?;
        }
        "agent update" => {
            require_change(r)?;
            let t = get_mut(root, &["agents", operand(r, 0)?])?;
            for (flag, key) in [("agent", "agent"), ("path", "scope")] {
                if let Some(v) = arg(r, flag) {
                    put(t, &[key], string(v))?;
                }
            }
        }
        "agent remove" => {
            if remove(root, &["agents", operand(r, 0)?])?.is_none() {
                return Err(error());
            }
            info["native_files"] =
                json!("unchanged; restore information and native credentials retained");
        }
        "agent rename" => {
            let old = operand(r, 0)?;
            let new = operand(r, 1)?;
            let target = remove(root, &["agents", old])?.ok_or_else(error)?;
            new_named(root, &["agents"], new, target)?;
            if let Some(providers) = root.get_mut("providers").and_then(Value::as_table_mut) {
                for p in providers.iter_mut().map(|(_, value)| value) {
                    if p.get("auth")
                        .and_then(|a| a.get("via"))
                        .and_then(Value::as_str)
                        == Some(old)
                    {
                        put(p, &["auth", "via"], string(new))?;
                    }
                }
            }
            info["target_rename"] = json!([old, new]);
        }
        "agent provider add" | "agent provider update" | "agent provider remove" => {
            let target = operand(r, 0)?;
            let name = operand(r, 1)?;
            get(root, &["agents", target])?;
            if path.ends_with("remove") {
                if remove(root, &["agents", target, "providers", name])?.is_none() {
                    return Err(error());
                }
            } else {
                if arg(r, "provider").is_some() == arg(r, "proxy").is_some() {
                    return Err(error());
                }
                let mut entry = if path.ends_with("add") {
                    table()
                } else {
                    get(root, &["agents", target, "providers", name])?.clone()
                };
                remove(&mut entry, &["provider"])?;
                remove(&mut entry, &["proxy"])?;
                if let Some(provider) = arg(r, "provider") {
                    put(&mut entry, &["provider"], string(provider))?;
                } else {
                    put(&mut entry, &["proxy"], Value::Boolean(true))?;
                }
                if path.ends_with("add") {
                    put(&mut entry, &["models"], Value::Array(Vec::new()))?;
                    new_named(root, &["agents", target, "providers"], name, entry)?;
                } else {
                    put(root, &["agents", target, "providers", name], entry)?;
                }
            }
        }
        "agent model add" | "agent model remove" | "agent model clear" => {
            let target = operand(r, 0)?;
            let name = operand(r, 1)?;
            let e = get_mut(root, &["agents", target, "providers", name])?;
            if e.get("models").is_none() {
                put(e, &["models"], Value::Array(Vec::new()))?;
            }
            let list = e
                .get_mut("models")
                .and_then(Value::as_array_mut)
                .ok_or_else(error)?;
            if path.ends_with("clear") {
                list.clear();
            } else {
                let id = operand(r, 2)?;
                let protocol = required(r, "protocol")?;
                let pos = list.iter().position(|m| {
                    m.get("id").and_then(Value::as_str) == Some(id)
                        && m.get("protocol").and_then(Value::as_str) == Some(protocol)
                });
                if path.ends_with("add") {
                    if pos.is_some() {
                        return Err(error());
                    }
                    list.push(selection(id, protocol));
                } else {
                    list.remove(pos.ok_or_else(error)?);
                }
            }
        }
        "agent default set" => {
            put(
                root,
                &["agents", operand(r, 0)?, "default"],
                reference(operand(r, 1)?, operand(r, 2)?, required(r, "protocol")?),
            )?;
        }
        "agent default clear" => {
            get(root, &["agents", operand(r, 0)?])?;
            remove(root, &["agents", operand(r, 0)?, "default"])?;
        }
        "agent role set" => {
            let target = operand(r, 0)?;
            let role = operand(r, 1)?;
            let agent = get(root, &["agents", target, "agent"])?
                .as_str()
                .ok_or_else(error)?;
            if !crate::native::supported_roles(agent).contains(&role) {
                return Err(Error::new(
                    "invalid_arguments",
                    "This native Agent does not declare that model role",
                ));
            }
            put(
                root,
                &["agents", target, "roles", role],
                reference(operand(r, 2)?, operand(r, 3)?, required(r, "protocol")?),
            )?;
        }
        "agent role remove" => {
            if remove(root, &["agents", operand(r, 0)?, "roles", operand(r, 1)?])?.is_none() {
                return Err(error());
            }
        }
        "agent role clear" => {
            get(root, &["agents", operand(r, 0)?])?;
            remove(root, &["agents", operand(r, 0)?, "roles"])?;
        }
        "network direct" => {
            remove(root, &["network"])?;
        }
        "network proxy" => {
            put(
                root,
                &["network"],
                Value::Table(toml::map::Map::from_iter([
                    ("mode".into(), string("proxy")),
                    ("proxy_url".into(), string(operand(r, 0)?)),
                ])),
            )?;
        }
        "proxy configure" => {
            require_change(r)?;
            let exists = root.get("proxy").is_some();
            if !exists {
                put(
                    root,
                    &["proxy"],
                    Value::Table(toml::map::Map::from_iter([
                        ("listen".into(), string(required(r, "listen")?)),
                        ("key_ref".into(), string("__credential_provision_pending__")),
                    ])),
                )?;
            } else if let Some(listen) = arg(r, "listen") {
                put(root, &["proxy", "listen"], string(listen))?;
            }
            for clear in args(r, "clear") {
                if clear != "autostart" {
                    return Err(error());
                }
                remove(root, &["proxy", "autostart"])?;
            }
            info["provision_proxy_key"] = json!(!exists);
        }
        "proxy key replace" => {
            get(root, &["proxy"])?;
            put(
                root,
                &["proxy", "key_ref"],
                string("__credential_provision_pending__"),
            )?;
        }
        "proxy remove" => {
            if remove(root, &["proxy"])?.is_none() {
                return Err(error());
            }
            info["stored_secrets"] = json!("retained");
        }
        "proxy autostart enable" | "proxy autostart disable" => {
            get(root, &["proxy"])?;
            put(
                root,
                &["proxy", "autostart"],
                Value::Boolean(path.ends_with("enable")),
            )?;
        }
        _ => return Err(error()),
    }
    let _ = store;
    Ok((info, renames))
}
fn endpoints(p: &mut Value, r: &Request) -> Result<()> {
    let mut protocols = std::collections::BTreeSet::new();
    for protocol in args(r, "clear-endpoint") {
        if !protocols.insert(protocol) {
            return Err(error());
        }
        remove(p, &["endpoints", protocol])?;
    }
    for pair in args(r, "endpoint").as_chunks::<2>().0 {
        if !protocols.insert(&pair[0]) {
            return Err(error());
        }
        put(p, &["endpoints", &pair[0]], string(&pair[1]))?;
    }
    Ok(())
}
fn alias_source(a: &mut Value, r: &Request, required_source: bool) -> Result<()> {
    let tier = arg(r, "tier");
    let candidates = args(r, "candidate");
    if tier.is_some() && !candidates.is_empty() {
        return Err(error());
    }
    if required_source && tier.is_none() && candidates.is_empty() {
        return Err(error());
    }
    if let Some(tier) = tier {
        remove(a, &["candidates"])?;
        put(a, &["tier"], string(tier))?;
    } else if !candidates.is_empty() {
        remove(a, &["tier"])?;
        put(a, &["candidates"], strings(candidates))?;
    }
    Ok(())
}
fn rename_alias_refs(root: &mut Value, old: &str, new: &str, protocol: &str) -> Result<()> {
    if let Some(targets) = root.get_mut("agents").and_then(Value::as_table_mut) {
        for t in targets.iter_mut().map(|(_, value)| value) {
            let mut names = Vec::new();
            if let Some(entries) = t.get_mut("providers").and_then(Value::as_table_mut) {
                for (name, e) in entries {
                    if e.get("proxy").and_then(Value::as_bool) == Some(true)
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
fn sync_table(
    target: &mut dyn toml_edit::TableLike,
    before: Option<&Value>,
    after: &Value,
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
            if !target.get(key).is_some_and(toml_edit::Item::is_table_like) {
                target.insert(key, toml_edit::Item::Table(toml_edit::Table::new()));
            }
            sync_table(
                target
                    .get_mut(key)
                    .and_then(toml_edit::Item::as_table_like_mut)
                    .ok_or_else(error)?,
                old,
                value,
            )?;
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

pub async fn execute(store: &Store, r: &Request) -> Result<Json> {
    let path = store.root.join("tmax.toml");
    let bytes = match std::fs::read(&path) {
        Ok(v) => Some(v),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Err(Error::new(
                "configuration_unavailable",
                "Cannot read personal configuration",
            ));
        }
    };
    let revision = bytes
        .as_ref()
        .map(|b| config::revision(b))
        .unwrap_or_else(|| "absent".into());
    if r.if_revision.as_ref().is_some_and(|rev| rev != &revision) {
        return Err(Error::new(
            "inputs_changed",
            "Saved configuration differs from the requested revision",
        ));
    }
    let text = bytes
        .as_ref()
        .map(|b| {
            std::str::from_utf8(b)
                .map_err(|_| Error::new("invalid_configuration", "Configuration must be UTF-8"))
        })
        .transpose()?
        .unwrap_or("version = 1\n");
    let mut document: text_document::Document = text.parse().map_err(|_| {
        Error::new(
            "invalid_configuration",
            "Configuration cannot be parsed; inspect recovery options",
        )
    })?;
    let mut root: Value = toml::from_str(text).map_err(|_| {
        Error::new(
            "invalid_configuration",
            "Configuration cannot be parsed; inspect recovery options",
        )
    })?;
    let before = root.clone();
    let old = config::parse(text).ok();
    if let Some(old) = &old {
        super::references::guard(old, r)?;
    }
    let (mut data, renames) = mutate(&mut root, r, store)?;
    let path = r.command.join(" ");
    let candidate_text = toml::to_string(&root).map_err(|_| error())?;
    let mut candidate = config::parse(&candidate_text)?;
    if path == "proxy remove" {
        let state = crate::service::status(store).await?;
        if state
            .pointer("/resident/inference/state")
            .and_then(Json::as_str)
            == Some("running")
        {
            return Err(Error::new(
                "active_requests",
                "Stop inference explicitly before removing proxy settings",
            ));
        }
    }
    if r.dry_run {
        let mut impact = super::impact(store, &candidate, &revision, old.as_ref()).await?;
        impact["requested"] = data;
        if path == "provider login" || path == "provider logout" {
            let source = if path == "provider login" {
                candidate.providers[operand(r, 0)?].auth.as_ref()
            } else {
                old.as_ref()
                    .and_then(|c| c.providers.get(operand(r, 0).ok()?))
                    .and_then(|p| p.auth.as_ref())
            };
            if let Some(config::Auth::Login { via, .. }) = source {
                impact["credential"] = crate::native::login(
                    store,
                    old.as_ref().unwrap_or(&candidate),
                    operand(r, 0)?,
                    via,
                    true,
                )
                .await?;
            }
        }
        return Ok(impact);
    }
    config::guard(store, &revision)?;
    let needs_key = (path == "proxy configure" && data["provision_proxy_key"] == true)
        || path == "proxy key replace"
        || (path == "provider key set" && arg(r, "key-ref").is_none());
    let unlock = if needs_key {
        crate::secrets::unlock(r)?
    } else {
        None
    };
    let mut created = None;
    if needs_key && unlock.is_none() {
        return Err(Error::new(
            "credential_unavailable",
            "Unlock protected storage through a separate protected input before supplying or provisioning a key",
        ));
    }
    if needs_key {
        let value: zeroize::Zeroizing<Vec<u8>> = if path == "provider key set" {
            zeroize::Zeroizing::new(crate::secrets::read_protected_input(r, "key")?)
        } else {
            use rand::RngCore;
            let mut b = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut b);
            use base64::Engine;
            zeroize::Zeroizing::new(
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(b)
                    .into_bytes(),
            )
        };
        let owner = if path == "provider key set" {
            provider_owner(store, old.as_ref().unwrap_or(&candidate), operand(r, 0)?)?
        } else {
            "proxy".into()
        };
        let reference =
            crate::secrets::put(store, &owner, &value, unlock.as_deref().map(Vec::as_slice))?;
        if path == "provider key set" {
            put(
                &mut root,
                &["providers", operand(r, 0)?, "auth", "key_ref"],
                string(&reference),
            )?;
        } else {
            let previous = old
                .as_ref()
                .and_then(|c| c.proxy.as_ref())
                .map(|p| p.key_ref.clone());
            put(&mut root, &["proxy", "key_ref"], string(&reference))?;
            data["proxy_key"] = json!({"action":if path=="proxy key replace"{"replaced"}else{"provisioned"},"reference":reference,"previous_reference":previous});
            data["unmanaged_clients"] = json!("unknown");
        }
        created = Some(reference);
    }
    if path == "provider key set" && arg(r, "key-ref").is_some() {
        let inventory = crate::secrets::inventory(store)?;
        let reference = required(r, "key-ref")?;
        let id = operand(r, 0)?;
        let identity =
            crate::inputs::provider_identity(store, old.as_ref().unwrap_or(&candidate), id)?
                .ok_or_else(error)?;
        let valid = inventory["entries"].as_array().is_some_and(|a| {
            a.iter().any(|e| {
                e["reference"] == reference
                    && e["owner"]["kind"] == "provider"
                    && e["owner"]["identity"] == identity
                    && e["metadata_state"] == "known"
            })
        });
        if !valid {
            return Err(Error::new(
                "credential_unavailable",
                "Select an existing protected reference owned by this provider incarnation",
            ));
        }
    }
    if path == "provider login" || path == "provider logout" {
        if arg(r, "non-interactive").is_some() {
            return Err(Error::new(
                "interactive_required",
                "Native sign-in requires an explicit interactive owner operation",
            ));
        }
        let original = old.as_ref().unwrap_or(&candidate);
        let provider = operand(r, 0)?;
        let auth = if path == "provider login" {
            candidate.providers[provider].auth.as_ref()
        } else {
            original.providers[provider].auth.as_ref()
        };
        let Some(config::Auth::Login { via, .. }) = auth else {
            return Err(error());
        };
        let effect = if path == "provider login" {
            crate::native::login(store, original, provider, via, false).await?
        } else {
            crate::native::logout(store, original, provider, via, false).await?
        };
        if let Some(code) = effect
            .get("_code")
            .and_then(Json::as_str)
            .filter(|code| *code != "awaiting_native_reload")
        {
            data["_code"] = json!(code);
            data["_status"] = json!("blocked");
            data["credential"] = effect;
            data["save"] = json!("not_requested");
            return Ok(data);
        }
        if effect.get("_code").and_then(Json::as_str) == Some("awaiting_native_reload") {
            data["_code"] = json!("awaiting_native_reload");
        }
        data["credential"] = effect.clone();
        if path == "provider login" {
            let Some(session) = effect.get("session_ref").and_then(Json::as_str) else {
                data["_code"] = json!("recovery_required");
                data["save"] = json!("not_requested");
                return Ok(data);
            };
            put(
                &mut root,
                &["providers", provider, "auth", "session_ref"],
                string(session),
            )?;
        }
        if let Err(error) = crate::inputs::authorization_changed(store, provider) {
            data["_code"] = json!("recovery_required");
            data["save"] = json!("not_requested");
            data["authorization_invalidation"] = json!({"state":"failed","code":error.code});
            return Ok(data);
        }
    }
    candidate = config::parse(&toml::to_string(&root).map_err(|_| error())?)?;
    // Move renamed tables first so their comments and table ordering travel with the resource.
    for (old_name, new_name) in &renames {
        if let Ok((provider_id, m)) = config::qualified(old_name) {
            let (_, new_m) = config::qualified(new_name)?;
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
    sync_table(document.as_table_mut(), Some(&before), &root)?;
    let saved = match config::save(store, &revision, &document) {
        Ok(revision) => revision,
        Err(error) => {
            if created.is_none() && data.get("credential").is_none() {
                return Err(error);
            }
            data["_code"] = json!("saved_not_active");
            data["save"] = json!("failed");
            data["retained_secret_reference"] = json!(created);
            data["native_application"] = json!("not_requested");
            return Ok(data);
        }
    };
    data["file_revision"] = json!(saved);
    data["save"] = json!("saved");
    let reconciliation = (|| -> Result<()> {
        crate::routing::reconcile(
            store,
            old.as_ref().unwrap_or(&Config::empty()),
            &candidate,
            &renames,
        )?;
        for (old, new) in &renames {
            if !old.contains('/') {
                crate::secrets::rename_owner(store, "provider", old, new)?;
            }
        }
        if let Some(pair) = data["target_rename"].as_array()
            && let (Some(old), Some(new)) = (
                pair.first().and_then(Json::as_str),
                pair.get(1).and_then(Json::as_str),
            )
        {
            crate::native::rename_target(store, old, new)?;
        }
        if path == "agent remove" {
            crate::native::remove_target(store, operand(r, 0)?)?;
        }
        if path == "provider remove" {
            crate::secrets::detach_owner(store, "provider", operand(r, 0)?)?;
        }
        Ok(())
    })();
    if let Err(e) = reconciliation {
        data["_code"] = json!("saved_not_active");
        data["runtime"] = json!("not_loaded");
        data["reconciliation"] = json!({"state":"failed","code":e.code,"message":e.message});
        return Ok(data);
    }
    let activation = crate::service::activate(store, &candidate, &saved).await;
    let autostart = if path.starts_with("proxy autostart ") {
        crate::service::autostart(store, path.ends_with("enable"), false).await
    } else {
        crate::service::reconcile_autostart(store, &candidate, false).await
    };
    data["file_revision"] = json!(saved);
    data["save"] = json!(if root == before { "unchanged" } else { "saved" });
    data["native_application"] = json!("not_requested");
    match activation {
        Ok(v) => {
            let not_running = v["activation"] == "not_running";
            let failed = v["_code"].as_str().map(str::to_string);
            data["activation"] = v;
            data["runtime"] = json!(if not_running {
                "not_running"
            } else if failed.is_some() {
                "not_loaded"
            } else {
                "loaded"
            });
            if let Some(code) = failed {
                data["_code"] = json!(code);
            }
        }
        Err(e) => {
            data["_code"] = json!(if e.code == "restart_required" {
                "restart_required"
            } else {
                "saved_not_active"
            });
            data["runtime"] = json!("not_loaded");
            data["activation_error"] = json!({"code":e.code,"message":e.message});
        }
    }
    match autostart {
        Ok(v) => data["autostart"] = v,
        Err(e) => {
            data["_code"] = json!("saved_not_active");
            data["autostart"] = json!({"state":"failed","code":e.code,"message":e.message});
        }
    }
    Ok(data)
}
mod text_document {
    pub type Document = toml_edit::DocumentMut;
}

#[cfg(test)]
mod tests {
    use super::*;
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

#[cfg(test)]
mod regressions {
    use super::*;
    fn request(command: &[&str], operands: &[&str], sets: &[&str]) -> Request {
        Request {
            command: command.iter().map(|s| s.to_string()).collect(),
            operands: operands.iter().map(|s| s.to_string()).collect(),
            arguments: BTreeMap::from([(
                "set".into(),
                sets.iter().map(|s| s.to_string()).collect(),
            )]),
            dry_run: false,
            if_revision: None,
        }
    }
    #[tokio::test]
    async fn actual_update_preserves_inline_siblings_comments_and_reports_no_service() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::at(temp.path().to_path_buf());
        std::fs::create_dir_all(&store.root).unwrap();
        std::fs::write(store.root.join("tmax.toml"),"# owner comment\nversion=1\n[providers.p]\nintegration='deepseek'\noverrides = { name = 'Personal', doc = 'old' } # preserve inline\n").unwrap();
        let result = execute(
            &store,
            &request(&["provider", "update"], &["p"], &["overrides.doc", "new"]),
        )
        .await
        .unwrap();
        let text = std::fs::read_to_string(store.root.join("tmax.toml")).unwrap();
        let loaded = config::load(&store).unwrap();
        assert_eq!(
            loaded.config.providers["p"].overrides.name.as_deref(),
            Some("Personal")
        );
        assert_eq!(
            loaded.config.providers["p"].overrides.doc.as_deref(),
            Some("new")
        );
        assert!(text.contains("# owner comment"));
        assert!(text.contains("# preserve inline"));
        assert_eq!(result["runtime"], "not_running");
        assert!(!store.root.join("autostart.json").exists());
    }
    #[tokio::test]
    async fn stale_guard_and_dry_run_never_initialize_missing_home() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::at(temp.path().join("missing"));
        let mut r = Request {
            command: vec!["provider".into(), "add".into()],
            operands: vec!["p".into()],
            arguments: BTreeMap::from([("integration".into(), vec!["deepseek".into()])]),
            dry_run: true,
            if_revision: Some("absent".into()),
        };
        execute(&store, &r).await.unwrap();
        assert!(!store.root.exists());
        r.if_revision = Some("stale".into());
        assert_eq!(
            execute(&store, &r).await.unwrap_err().code,
            "inputs_changed"
        );
        assert!(!store.root.exists());
    }
    #[test]
    fn reference_safe_model_rename_updates_native_defaults_and_input_coverage() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::at(temp.path().into());
        let mut root:Value=toml::from_str("version=1\n[providers.p]\nintegration='deepseek'\n[providers.p.models.old]\n[providers.p.inputs.q]\nkind='quota'\nsource='manual'\nscope='weekly'\nmodels=['old']\nmax_age_seconds=60\n[agents.a]\nagent='pi'\nscope='/native'\ndefault={provider='up',id='old',protocol='chat-completions'}\n[agents.a.providers.up]\nprovider='p'\nmodels=[{id='old',protocol='chat-completions'}]\n").unwrap();
        let r = Request {
            command: vec!["model".into(), "rename".into()],
            operands: vec!["p/old".into(), "new/with/slash".into()],
            arguments: BTreeMap::new(),
            dry_run: true,
            if_revision: None,
        };
        let (_, renames) = mutate(&mut root, &r, &store).unwrap();
        assert_eq!(renames, vec![("p/old".into(), "p/new/with/slash".into())]);
        let c = config::parse(&toml::to_string(&root).unwrap()).unwrap();
        assert_eq!(c.providers["p"].inputs["q"].models, ["new/with/slash"]);
        assert_eq!(c.agents["a"].default.as_ref().unwrap().id, "new/with/slash");
    }
}

fn provider_owner(store: &Store, config: &Config, id: &str) -> Result<String> {
    let identity=crate::inputs::provider_identity(store,config,id)?.ok_or_else(||Error::new("credential_unavailable","Provider incarnation is unavailable; explicitly reload the valid authority before credential provisioning"))?;
    Ok(format!("provider:{identity}:{id}"))
}
