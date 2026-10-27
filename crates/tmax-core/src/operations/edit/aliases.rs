use super::document::{error, get, get_mut, put, remove, string, strings, table};
use super::fields::{apply_fields, require_change};
use super::references::rename_alias_refs;
use crate::operations::arguments::{arg, args, operand, required};
use crate::{Error, Request, Result};
use toml::Value;

pub(super) fn mutate(root: &mut Value, r: &Request) -> Result<()> {
    let id = operand(r, 0)?;
    let protocol = required(r, "protocol")?;
    let action = r.command.last().map(String::as_str).unwrap_or("");
    if r.command.len() == 4 {
        let pos = alias_position(root, id, protocol)?;
        let alias = &mut aliases_mut(root)?[pos];
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
        trim_candidate_efforts(alias)?;
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
    Ok(())
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
fn alias_source(a: &mut Value, r: &Request, required_source: bool) -> Result<()> {
    let candidates = args(r, "candidate");
    if required_source && candidates.is_empty() {
        return Err(Error::new(
            "invalid_arguments",
            "Add at least one registered concrete model with --candidate provider-id/model-id",
        )
        .field("candidate"));
    }
    if !candidates.is_empty() {
        put(a, &["candidates"], strings(candidates))?;
    }
    trim_candidate_efforts(a)?;
    let mut seen = std::collections::BTreeSet::new();
    for assignment in args(r, "candidate-reasoning-effort") {
        let (candidate, effort) = assignment.rsplit_once('=').ok_or_else(|| {
            Error::new(
                "invalid_arguments",
                "Use --candidate-reasoning-effort provider/model=effort",
            )
        })?;
        if !seen.insert(candidate) || effort.is_empty() {
            return Err(Error::new(
                "invalid_arguments",
                "Duplicate or empty candidate reasoning effort",
            )
            .field(candidate));
        }
        put(
            a,
            &["candidate_reasoning_effort", candidate],
            string(effort),
        )?;
    }
    for candidate in args(r, "clear-candidate-reasoning-effort") {
        if !seen.insert(candidate)
            || remove(a, &["candidate_reasoning_effort", candidate])?.is_none()
        {
            return Err(Error::new(
                "invalid_arguments",
                "Duplicate, conflicting, or absent candidate reasoning effort",
            )
            .field(candidate));
        }
    }
    Ok(())
}
fn trim_candidate_efforts(alias: &mut Value) -> Result<()> {
    let Some(mut efforts) = remove(alias, &["candidate_reasoning_effort"])? else {
        return Ok(());
    };
    let candidates = get(alias, &["candidates"])?.as_array().ok_or_else(error)?;
    efforts
        .as_table_mut()
        .ok_or_else(error)?
        .retain(|candidate, _| {
            candidates
                .iter()
                .any(|value| value.as_str() == Some(candidate))
        });
    if efforts.as_table().is_some_and(|values| !values.is_empty()) {
        put(alias, &["candidate_reasoning_effort"], efforts)?;
    }
    Ok(())
}
