//! Explicit format migration from desired-configuration version 1 to version 2.
//!
//! Migration is a finite, explicit operation: it merges per-protocol model-alias
//! records into one route record and rewrites legacy relayed Agent entries to
//! name the derived gateway provider. A record that cannot be merged exactly is
//! refused; no document is ever guessed or partially rewritten.

use crate::{
    Error, Request, ResourceRef, Result,
    config::{self, CONFIG_VERSION, GATEWAY_PROVIDER_ID, LEGACY_CONFIG_VERSION},
    storage::Store,
};
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet};
use toml_edit::DocumentMut;

/// Fields every legacy record of one alias group must agree on.
const SHARED_ALIAS_FIELDS: [&str; 5] = [
    "candidates",
    "enabled",
    "continuity",
    "fallback",
    "candidate_reasoning_effort",
];
/// The complete field vocabulary of one legacy model-alias record.
const RECORD_FIELDS: [&str; 7] = [
    "id",
    "protocol",
    "candidates",
    "candidate_reasoning_effort",
    "enabled",
    "continuity",
    "fallback",
];
const MODEL_ALIAS: &str = "model_alias";
const PROVIDER: &str = "provider";
const AGENT_PROVIDER_ENTRY: &str = "agent_provider_entry";
/// The legacy continuity value that is also the format-2 effective default.
const DEFAULT_CONTINUITY: &str = "session";

pub(super) fn run(store: &Store, r: &Request) -> Result<Json> {
    let path = store.root.join("tmax.toml");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::new(
                "unknown_resource",
                "There is no configuration file to migrate",
            ));
        }
        Err(_) => {
            return Err(Error::new(
                "configuration_unavailable",
                "Cannot read the personal configuration",
            ));
        }
    };
    let revision = config::revision(&bytes);
    if r.if_revision
        .as_ref()
        .is_some_and(|expected| expected != &revision)
    {
        return Err(Error::new(
            "inputs_changed",
            "Saved configuration differs from the requested revision",
        ));
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid_toml())?;
    let mut document: DocumentMut = text.parse().map_err(|_| invalid_toml())?;
    let root: toml::Value = toml::from_str(text).map_err(|_| invalid_toml())?;
    match root.get("version").and_then(toml::Value::as_integer) {
        Some(LEGACY_CONFIG_VERSION) => {}
        Some(CONFIG_VERSION) => {
            return Err(Error::new(
                "invalid_arguments",
                "The configuration already uses the current format; migration is not needed",
            )
            .field("version"));
        }
        _ => {
            return Err(Error::new(
                "invalid_configuration",
                "Configuration format version is unsupported; only format version 1 can be migrated",
            )
            .field("version"));
        }
    }
    let mut refusals = Vec::new();
    // A version-1 file may still define a real access instance named tmax. Version
    // 2 reserves that ID for the gateway provider, so migrating would silently
    // drop the definition; refuse and name the manual resolution instead.
    if (root.get("gateway").is_some() || root.get("proxy").is_some())
        && root
            .get("providers")
            .and_then(|providers| providers.get(GATEWAY_PROVIDER_ID))
            .is_some()
    {
        refusals.push(Refusal::new(
            PROVIDER,
            Some(GATEWAY_PROVIDER_ID.into()),
            "providers.tmax",
            "provider ID tmax is reserved for the gateway provider; rename or remove the stored definition before migrating".into(),
        ));
    }
    let aliases = collect_aliases(&root, &mut refusals);
    let rewritten_entries = collect_entries(&root, &aliases.route_names, &mut refusals);
    if !refusals.is_empty() {
        return Err(refused(refusals));
    }
    if root.get("model_aliases").is_some() {
        document["model_aliases"] = model_aliases_item(&aliases.groups);
        // A former array-of-tables header has no key-to-value separator yet.
        if let Some(mut key) = document.as_table_mut().key_mut("model_aliases") {
            key.leaf_decor_mut().set_suffix(" ");
        }
    }
    rewrite_entries(&mut document, &rewritten_entries);
    document["version"] = toml_edit::value(CONFIG_VERSION);
    if let Err(error) = config::parse(&document.to_string()) {
        let mut refusal = Error::new(
            "migration_refused",
            &format!(
                "The migrated configuration is invalid; no file was written: {}",
                error.message
            ),
        );
        refusal.field = error.field;
        return Err(refusal);
    }
    let alias_summary = json!({
        "merged": aliases
            .groups
            .iter()
            .map(|group| json!({"id":group.id,"protocols":group.protocols}))
            .collect::<Vec<_>>(),
        "total_records": aliases.total_records,
        "total_routes": aliases.groups.len(),
    });
    let rewritten = rewritten_entries
        .iter()
        .map(|entry| json!({"target":entry.target,"entry":entry.entry}))
        .collect::<Vec<_>>();
    if r.dry_run {
        return Ok(json!({
            "state":"preview",
            "from_version":LEGACY_CONFIG_VERSION,
            "to_version":CONFIG_VERSION,
            "revision":revision,
            "aliases":alias_summary,
            "entries":rewritten,
            "refusals":[],
        }));
    }
    let recovery = crate::secrets::protect_recovery(store, &bytes, "format_migration")?;
    let saved = config::save(store, &revision, &document)?;
    Ok(json!({
        "state":"migrated",
        "from_version":LEGACY_CONFIG_VERSION,
        "to_version":CONFIG_VERSION,
        "revision":saved,
        "recovery":recovery,
        "aliases":alias_summary,
        "entries":rewritten,
    }))
}

fn invalid_toml() -> Error {
    Error::new(
        "invalid_configuration",
        "Configuration is not valid TOML 1.0",
    )
}

/// One refused migration object. `detail` names the concrete problem for the
/// aggregate error message; `scope` is the machine-readable reason.
struct Refusal {
    kind: &'static str,
    id: Option<String>,
    scope: String,
    detail: String,
}

impl Refusal {
    fn new(
        kind: &'static str,
        id: Option<String>,
        scope: impl Into<String>,
        detail: String,
    ) -> Self {
        Self {
            kind,
            id,
            scope: scope.into(),
            detail,
        }
    }
}

fn refused(refusals: Vec<Refusal>) -> Error {
    let listed = refusals
        .iter()
        .map(|refusal| {
            let id = refusal.id.as_deref().unwrap_or("unknown");
            format!("{} '{}' ({})", refusal.kind, id, refusal.detail)
        })
        .collect::<Vec<_>>()
        .join("; ");
    let mut error = Error::new(
        "migration_refused",
        &format!(
            "Migration refused for {} object(s); no configuration was written: {listed}",
            refusals.len()
        ),
    );
    error.affected = refusals
        .into_iter()
        .map(|refusal| ResourceRef {
            kind: refusal.kind.into(),
            id: refusal.id,
            scope: Some(refusal.scope),
        })
        .collect();
    error
}

#[derive(Default)]
struct AliasState {
    groups: Vec<AliasGroup>,
    /// Every intended route name, including ids whose own records are refused.
    /// A concrete-selection refusal must not blame a route name for an
    /// unrelated alias-record conflict.
    route_names: BTreeSet<String>,
    total_records: usize,
}

struct AliasGroup {
    id: String,
    protocols: Vec<String>,
    candidates: Vec<String>,
    enabled: bool,
    continuity: String,
    fallback: Option<toml::Value>,
    effort: BTreeMap<String, String>,
}

struct LegacyRecord {
    protocol: String,
    shared: SharedAlias,
}

struct SharedAlias {
    candidates: Vec<String>,
    enabled: bool,
    continuity: String,
    fallback: Option<config::FailureConditions>,
    fallback_raw: Option<toml::Value>,
    effort: BTreeMap<String, String>,
}

fn collect_aliases(root: &toml::Value, refusals: &mut Vec<Refusal>) -> AliasState {
    let mut state = AliasState::default();
    let Some(value) = root.get("model_aliases") else {
        return state;
    };
    let Some(records) = value.as_array() else {
        refusals.push(Refusal::new(
            MODEL_ALIAS,
            Some("model_aliases".into()),
            "invalid_records",
            "model_aliases must be an array of records".into(),
        ));
        return state;
    };
    state.total_records = records.len();
    let mut order = Vec::new();
    let mut grouped: BTreeMap<String, Vec<std::result::Result<LegacyRecord, String>>> =
        BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        let Some(table) = record.as_table() else {
            refusals.push(index_refusal(index, "the record must be a table"));
            continue;
        };
        let id = table
            .get("id")
            .and_then(toml::Value::as_str)
            .filter(|id| !id.is_empty());
        let Some(id) = id else {
            refusals.push(index_refusal(index, "missing string 'id'"));
            continue;
        };
        if !grouped.contains_key(id) {
            order.push(id.to_string());
        }
        state.route_names.insert(id.to_string());
        grouped
            .entry(id.to_string())
            .or_default()
            .push(extract_record(table).map_err(|detail| format!("record {index}: {detail}")));
    }
    for id in order {
        let records = grouped
            .remove(&id)
            .expect("grouped ids are keyed by appearance order");
        let mut failures = Vec::new();
        let mut valid = Vec::new();
        for record in records {
            match record {
                Ok(record) => valid.push(record),
                Err(detail) => failures.push(detail),
            }
        }
        if !failures.is_empty() {
            refusals.push(Refusal::new(
                MODEL_ALIAS,
                Some(id),
                "invalid_record",
                failures.join("; "),
            ));
            continue;
        }
        let Some(first) = valid.first() else {
            continue;
        };
        let differing = SHARED_ALIAS_FIELDS
            .iter()
            .filter(|field| {
                valid[1..]
                    .iter()
                    .any(|record| field_differs(field, &first.shared, &record.shared))
            })
            .copied()
            .collect::<Vec<_>>();
        if !differing.is_empty() {
            let fields = differing.join(", ");
            refusals.push(Refusal::new(
                MODEL_ALIAS,
                Some(id),
                format!("conflicting_records: {}", differing.join(",")),
                format!("records disagree on {fields}"),
            ));
            continue;
        }
        let protocols = valid
            .iter()
            .map(|record| record.protocol.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        state.groups.push(AliasGroup {
            id,
            protocols,
            candidates: first.shared.candidates.clone(),
            enabled: first.shared.enabled,
            continuity: first.shared.continuity.clone(),
            fallback: first.shared.fallback_raw.clone(),
            effort: first.shared.effort.clone(),
        });
    }
    state
}

fn index_refusal(index: usize, detail: &str) -> Refusal {
    Refusal::new(
        MODEL_ALIAS,
        Some(format!("model_aliases[{index}]")),
        "invalid_record",
        detail.into(),
    )
}

fn field_differs(field: &str, left: &SharedAlias, right: &SharedAlias) -> bool {
    match field {
        "candidates" => left.candidates != right.candidates,
        "enabled" => left.enabled != right.enabled,
        "continuity" => left.continuity != right.continuity,
        "fallback" => left.fallback != right.fallback,
        "candidate_reasoning_effort" => left.effort != right.effort,
        _ => unreachable!("shared alias fields are a closed vocabulary"),
    }
}

fn extract_record(table: &toml::Table) -> std::result::Result<LegacyRecord, String> {
    for key in table.keys() {
        if !RECORD_FIELDS.contains(&key.as_str()) {
            return Err(format!("unsupported field '{key}'"));
        }
    }
    let protocol = table
        .get("protocol")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| "missing string 'protocol'".to_string())?;
    if !config::PROTOCOLS.contains(&protocol) {
        return Err(format!(
            "protocol '{protocol}' is not chat-completions, responses, or messages"
        ));
    }
    let candidates = table
        .get("candidates")
        .ok_or_else(|| "missing array 'candidates'".to_string())?
        .as_array()
        .ok_or_else(|| "'candidates' must be an array".to_string())?
        .iter()
        .map(|candidate| {
            candidate
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "'candidates' must contain strings".to_string())
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let enabled = match table.get("enabled") {
        None => true,
        Some(value) => value
            .as_bool()
            .ok_or_else(|| "'enabled' must be a boolean".to_string())?,
    };
    let continuity = match table.get("continuity") {
        None => DEFAULT_CONTINUITY.to_string(),
        Some(value) => value
            .as_str()
            .ok_or_else(|| "'continuity' must be a string".to_string())?
            .to_string(),
    };
    let fallback_raw = match table.get("fallback") {
        None => None,
        Some(value) => Some(toml::Value::Table(
            value
                .as_table()
                .ok_or_else(|| "'fallback' must be a table".to_string())?
                .clone(),
        )),
    };
    let fallback = match &fallback_raw {
        None => None,
        Some(value) => Some(
            value
                .clone()
                .try_into::<config::FailureConditions>()
                .map_err(|error| format!("invalid 'fallback': {error}"))?,
        ),
    };
    let effort = match table.get("candidate_reasoning_effort") {
        None => BTreeMap::new(),
        Some(value) => value
            .clone()
            .try_into::<BTreeMap<String, String>>()
            .map_err(|error| format!("invalid 'candidate_reasoning_effort': {error}"))?,
    };
    Ok(LegacyRecord {
        protocol: protocol.to_string(),
        shared: SharedAlias {
            candidates,
            enabled,
            continuity,
            fallback,
            fallback_raw,
            effort,
        },
    })
}

struct RelayedEntry {
    target: String,
    entry: String,
    selector: &'static str,
}

fn collect_entries(
    root: &toml::Value,
    route_names: &BTreeSet<String>,
    refusals: &mut Vec<Refusal>,
) -> Vec<RelayedEntry> {
    let Some(agents) = root.get("agents").and_then(toml::Value::as_table) else {
        return Vec::new();
    };
    let gateway_configured = root.get("gateway").is_some() || root.get("proxy").is_some();
    let mut relayed = Vec::new();
    for (target, target_value) in agents {
        let Some(providers) = target_value
            .get("providers")
            .and_then(toml::Value::as_table)
        else {
            continue;
        };
        for (entry, entry_value) in providers {
            let Some(table) = entry_value.as_table() else {
                continue;
            };
            let gateway = table.get("gateway").and_then(toml::Value::as_bool);
            let proxy = table.get("proxy").and_then(toml::Value::as_bool);
            let relayed_by_gateway = gateway == Some(true);
            let relayed_by_proxy = proxy == Some(true);
            if !relayed_by_gateway && !relayed_by_proxy {
                continue;
            }
            let mut reasons: Vec<(&'static str, String)> = Vec::new();
            if gateway.is_some() && proxy.is_some() {
                reasons.push((
                    "conflicting_source_selectors",
                    "declares both 'gateway' and legacy 'proxy'".into(),
                ));
            }
            if table.contains_key("provider") {
                reasons.push((
                    "conflicting_source_selectors",
                    "already declares a 'provider' selector".into(),
                ));
            }
            if !gateway_configured {
                reasons.push((
                    "gateway_not_configured",
                    "a relayed entry cannot resolve without the gateway".into(),
                ));
            }
            let concrete = table
                .get("models")
                .and_then(toml::Value::as_array)
                .map(|models| {
                    models
                        .iter()
                        .filter_map(|selection| selection.get("id").and_then(toml::Value::as_str))
                        .filter(|id| !route_names.contains(*id))
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if !concrete.is_empty() {
                let selections = concrete
                    .iter()
                    .map(|id| format!("'{id}'"))
                    .collect::<Vec<_>>()
                    .join(", ");
                reasons.push((
                    "concrete-gateway-selection",
                    format!(
                        "selection(s) {selections} are not route names; the gateway provider publishes route names only: define an alias for the concrete model, or drop the selection"
                    ),
                ));
            }
            if !reasons.is_empty() {
                let scope = reasons
                    .iter()
                    .map(|(scope, _)| *scope)
                    .find(|scope| *scope == "concrete-gateway-selection")
                    .unwrap_or(reasons[0].0)
                    .to_string();
                let detail = reasons
                    .iter()
                    .map(|(_, detail)| detail.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                refusals.push(Refusal::new(
                    AGENT_PROVIDER_ENTRY,
                    Some(format!("{target}/{entry}")),
                    scope,
                    detail,
                ));
                continue;
            }
            relayed.push(RelayedEntry {
                target: target.clone(),
                entry: entry.clone(),
                selector: if relayed_by_gateway {
                    "gateway"
                } else {
                    "proxy"
                },
            });
        }
    }
    relayed
}

fn model_aliases_item(groups: &[AliasGroup]) -> toml_edit::Item {
    let mut routes = toml_edit::Array::new();
    for group in groups {
        let mut route = toml_edit::InlineTable::new();
        route.insert("id", toml_edit::Value::from(group.id.clone()));
        let mut protocols = toml_edit::Array::new();
        for protocol in &group.protocols {
            protocols.push(toml_edit::Value::from(protocol.clone()));
        }
        route.insert("protocols", toml_edit::Value::Array(protocols));
        let mut candidates = toml_edit::Array::new();
        for candidate in &group.candidates {
            candidates.push(toml_edit::Value::from(candidate.clone()));
        }
        route.insert("candidates", toml_edit::Value::Array(candidates));
        if !group.effort.is_empty() {
            let mut effort = toml_edit::InlineTable::new();
            for (candidate, value) in &group.effort {
                effort.insert(candidate.clone(), toml_edit::Value::from(value.clone()));
            }
            route.insert(
                "candidate_reasoning_effort",
                toml_edit::Value::InlineTable(effort),
            );
        }
        if !group.enabled {
            route.insert("enabled", toml_edit::Value::from(false));
        }
        if group.continuity != DEFAULT_CONTINUITY {
            route.insert(
                "continuity",
                toml_edit::Value::from(group.continuity.clone()),
            );
        }
        if let Some(fallback) = &group.fallback
            && let Some(value) = edit_value(fallback)
        {
            route.insert("fallback", value);
        }
        routes.push(toml_edit::Value::InlineTable(route));
    }
    toml_edit::Item::Value(toml_edit::Value::Array(routes))
}

/// Convert a legacy TOML value into the inline value form format 2 stores.
fn edit_value(value: &toml::Value) -> Option<toml_edit::Value> {
    let wrapper = toml::Value::Table(toml::map::Map::from_iter([(
        "value".to_string(),
        value.clone(),
    )]));
    let document = toml_edit::ser::to_document(&wrapper).ok()?;
    document
        .get("value")
        .and_then(toml_edit::Item::as_value)
        .cloned()
}

fn rewrite_entries(document: &mut DocumentMut, rewritten: &[RelayedEntry]) {
    if rewritten.is_empty() {
        return;
    }
    let Some(agents) = document
        .get_mut("agents")
        .and_then(toml_edit::Item::as_table_like_mut)
    else {
        return;
    };
    for (target_key, target) in agents.iter_mut() {
        let target_name = target_key.get().to_string();
        let Some(providers) = target
            .get_mut("providers")
            .and_then(toml_edit::Item::as_table_like_mut)
        else {
            continue;
        };
        for (entry_key, entry) in providers.iter_mut() {
            let entry_name = entry_key.get().to_string();
            let Some(plan) = rewritten
                .iter()
                .find(|plan| plan.target == target_name && plan.entry == entry_name)
            else {
                continue;
            };
            let Some(table) = entry.as_table_like_mut() else {
                continue;
            };
            table.remove(plan.selector);
            table.insert(
                "provider",
                toml_edit::value(GATEWAY_PROVIDER_ID.to_string()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    const MAIN_FIXTURE: &str = "# personal settings\nversion = 1\nmodel_aliases = [\n  { id = \"route\", protocol = \"responses\", candidates = [\"up/model\"] },\n  { id = \"route\", protocol = \"chat-completions\", candidates = [\"up/model\"], enabled = true },\n  { id = \"other\", protocol = \"responses\", candidates = [\"up/model\"] },\n]\n\n[providers.up]\nintegration = \"custom\"\n\n[providers.up.endpoints]\nchat-completions = \"http://127.0.0.1:39999\"\nresponses = \"http://127.0.0.1:39999\"\n\n[providers.up.models.model]\n";

    const CONFLICT_FIXTURE: &str = "version = 1\n\n[[model_aliases]]\nid = \"route\"\nprotocol = \"responses\"\ncandidates = [\"up/a\"]\n\n[[model_aliases]]\nid = \"route\"\nprotocol = \"chat-completions\"\ncandidates = [\"up/b\"]\n\n[providers.up]\nintegration = \"custom\"\n\n[providers.up.endpoints]\nchat-completions = \"http://127.0.0.1:39999\"\nresponses = \"http://127.0.0.1:39999\"\n\n[providers.up.models.a]\n\n[providers.up.models.b]\n";

    const GATEWAY_FIXTURE: &str = "# personal settings\nversion = 1\n\n[gateway]\nlisten = \"127.0.0.1:17840\"\nkey_ref = \"secret:gw\"\n\n# upstream provider\n[providers.up]\nintegration = \"custom\"\n\n[providers.up.endpoints]\nresponses = \"http://127.0.0.1:39999\"\n\n[providers.up.models.model]\n\n[[model_aliases]]\nid = \"route\"\nprotocol = \"responses\"\ncandidates = [\"up/model\"]\n\n[network]\nproxy_url = \"http://127.0.0.1:8080\"\n\n[agents.local]\nagent = \"pi\"\nscope = \"/native/local\"\n\n[agents.local.providers.relay]\ngateway = true\n\n[[agents.local.providers.relay.models]]\nid = \"route\"\nprotocol = \"responses\"\n";

    const ORPHAN_GATEWAY_FIXTURE: &str = "# personal settings\nversion = 1\n\n# upstream provider\n[providers.up]\nintegration = \"custom\"\n\n[providers.up.endpoints]\nresponses = \"http://127.0.0.1:39999\"\n\n[providers.up.models.model]\n\n[[model_aliases]]\nid = \"route\"\nprotocol = \"responses\"\ncandidates = [\"up/model\"]\n\n[agents.local]\nagent = \"pi\"\nscope = \"/native/local\"\n\n[agents.local.providers.relay]\ngateway = true\n\n[[agents.local.providers.relay.models]]\nid = \"route\"\nprotocol = \"responses\"\n";

    const SHARED_POLICY_FIXTURE: &str = "# policy settings\nversion = 1\nmodel_aliases = [\n  { id = \"route\", protocol = \"responses\", candidates = [\"up/model\"], continuity = \"stateless\", fallback = { http_statuses = [429] }, candidate_reasoning_effort = { \"up/model\" = \"high\" } },\n  { id = \"route\", protocol = \"chat-completions\", candidates = [\"up/model\"], continuity = \"stateless\", fallback = { http_statuses = [429] }, candidate_reasoning_effort = { \"up/model\" = \"high\" } },\n]\n\n[providers.up]\nintegration = \"custom\"\n\n[providers.up.endpoints]\nchat-completions = \"http://127.0.0.1:39999\"\nresponses = \"http://127.0.0.1:39999\"\n\n[providers.up.models.model.overrides]\nreasoning = true\nreasoning_options = [{ type = \"effort\", values = [\"low\", \"high\"] }]\n";

    const INLINE_ENTRY_FIXTURE: &str = "# inline entry\nversion = 1\n\n[gateway]\nlisten = \"127.0.0.1:17840\"\nkey_ref = \"secret:gw\"\n\n[providers.up]\nintegration = \"custom\"\n\n[providers.up.endpoints]\nresponses = \"http://127.0.0.1:39999\"\n\n[providers.up.models.model]\n\n[[model_aliases]]\nid = \"route\"\nprotocol = \"responses\"\ncandidates = [\"up/model\"]\n\n[agents.inline]\nagent = \"pi\"\nscope = \"/native/inline\"\n\n[agents.inline.providers]\nrelay = { gateway = true, models = [{ id = \"route\", protocol = \"responses\" }] }\n\n# historical proxy spelling\n[agents.legacy]\nagent = \"pi\"\nscope = \"/native/legacy\"\n\n[agents.legacy.providers.old]\nproxy = true\n\n[[agents.legacy.providers.old.models]]\nid = \"route\"\nprotocol = \"responses\"\n";

    const CONCRETE_FIXTURE: &str = "# personal settings\nversion = 1\n\n[gateway]\nlisten = \"127.0.0.1:17840\"\nkey_ref = \"secret:gw\"\n\n# upstream provider\n[providers.up]\nintegration = \"custom\"\n\n[providers.up.endpoints]\nresponses = \"http://127.0.0.1:39999\"\n\n[providers.up.models.model]\n\n[[model_aliases]]\nid = \"route\"\nprotocol = \"responses\"\ncandidates = [\"up/model\"]\n\n[agents.local]\nagent = \"pi\"\nscope = \"/native/local\"\n\n[agents.local.providers.relay]\ngateway = true\n\n[[agents.local.providers.relay.models]]\nid = \"route\"\nprotocol = \"responses\"\n\n[[agents.local.providers.relay.models]]\nid = \"up/model\"\nprotocol = \"responses\"\n";

    fn store(directory: &tempfile::TempDir) -> Store {
        Store::at(directory.path().into())
    }

    fn request(dry_run: bool) -> Request {
        Request {
            command: vec!["config".into(), "migrate".into()],
            operands: Vec::new(),
            arguments: BTreeMap::new(),
            dry_run,
            if_revision: None,
        }
    }

    async fn dispatch(store: &Store, dry_run: bool) -> Result<Json> {
        crate::operations::test_dispatch(store, &request(dry_run)).await
    }

    fn write(store: &Store, text: &str) {
        store.write_raw("tmax.toml", text.as_bytes()).unwrap();
    }

    fn read(store: &Store) -> Vec<u8> {
        std::fs::read(store.root.join("tmax.toml")).unwrap()
    }

    #[tokio::test]
    async fn merges_per_protocol_records_previews_without_writing_and_migrates_once() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);
        write(&store, MAIN_FIXTURE);
        let before = read(&store);
        let revision = config::revision(&before);
        let preview = dispatch(&store, true).await.unwrap();
        assert_eq!(preview["state"], "preview");
        assert_eq!(
            preview["from_version"].as_i64(),
            Some(LEGACY_CONFIG_VERSION)
        );
        assert_eq!(preview["to_version"].as_i64(), Some(CONFIG_VERSION));
        assert_eq!(preview["revision"].as_str(), Some(revision.as_str()));
        assert_eq!(preview["aliases"]["total_records"].as_u64(), Some(3));
        assert_eq!(preview["aliases"]["total_routes"].as_u64(), Some(2));
        let merged = preview["aliases"]["merged"].as_array().unwrap();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0]["id"], "route");
        assert_eq!(
            merged[0]["protocols"],
            json!(["chat-completions", "responses"])
        );
        assert_eq!(merged[0]["protocols"].as_array().unwrap().len(), 2);
        assert_eq!(merged[1]["id"], "other");
        assert_eq!(merged[1]["protocols"], json!(["responses"]));
        assert_eq!(preview["entries"], json!([]));
        assert_eq!(preview["refusals"], json!([]));
        assert_eq!(read(&store), before);
        assert!(!store.root.join("recovery").exists());

        let applied = dispatch(&store, false).await.unwrap();
        assert_eq!(applied["state"], "migrated");
        assert_eq!(
            applied["from_version"].as_i64(),
            Some(LEGACY_CONFIG_VERSION)
        );
        assert_eq!(applied["to_version"].as_i64(), Some(CONFIG_VERSION));
        let saved = read(&store);
        assert_eq!(
            applied["revision"].as_str(),
            Some(config::revision(&saved).as_str())
        );
        let migrated = saved.clone();
        assert_ne!(applied["revision"].as_str(), Some(revision.as_str()));
        let text = String::from_utf8(saved).unwrap();
        assert!(text.contains("# personal settings"));
        assert!(text.contains("version = 2"));
        assert!(!text.contains("protocol ="));
        let loaded = config::load(&store).unwrap();
        assert_eq!(loaded.config.model_aliases.len(), 2);
        assert_eq!(loaded.config.model_aliases[0].id, "route");
        assert_eq!(
            loaded.config.model_aliases[0].protocols,
            ["chat-completions", "responses"]
        );
        assert!(loaded.config.model_aliases[0].enabled);
        assert!(loaded.config.model_aliases[0].continuity.is_none());
        assert_eq!(loaded.config.model_aliases[1].protocols, ["responses"]);

        let recovery = crate::secrets::recovery_list(&store).unwrap();
        assert_eq!(recovery["coverage"], "complete");
        let entries = recovery["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0]["original_revision"].as_str(),
            Some(revision.as_str())
        );
        assert_eq!(entries[0]["config_file"], "tmax.toml");
        assert_eq!(entries[0]["rejection_category"], "format_migration");

        let repeated = dispatch(&store, false).await.unwrap_err();
        assert_eq!(repeated.code, "invalid_arguments");
        assert!(repeated.message.contains("already"));
        assert_eq!(read(&store), migrated);
        assert_eq!(config::load(&store).unwrap().config.model_aliases.len(), 2);
    }

    #[tokio::test]
    async fn conflicting_records_are_refused_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);
        write(&store, CONFLICT_FIXTURE);
        let before = read(&store);
        for dry_run in [true, false] {
            let error = dispatch(&store, dry_run).await.unwrap_err();
            assert_eq!(error.code, "migration_refused");
            assert!(error.message.contains("route"));
            assert!(error.message.contains("candidates"));
            assert!(error.affected.iter().any(|reference| {
                reference.kind == "model_alias" && reference.id.as_deref() == Some("route")
            }));
            assert_eq!(read(&store), before);
        }
    }

    #[tokio::test]
    async fn relayed_entries_are_rewritten_and_reported() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);
        write(&store, GATEWAY_FIXTURE);
        let applied = dispatch(&store, false).await.unwrap();
        assert_eq!(applied["state"], "migrated");
        assert_eq!(
            applied["entries"],
            json!([{"target":"local","entry":"relay"}])
        );
        let text = String::from_utf8(read(&store)).unwrap();
        assert!(text.contains("provider = \"tmax\""));
        assert!(!text.contains("gateway = true"));
        assert!(text.contains("# upstream provider"));
        assert!(text.contains("proxy_url = \"http://127.0.0.1:8080\""));
        let loaded = config::load(&store).unwrap();
        let entry = &loaded.config.agents["local"].providers["relay"];
        assert_eq!(entry.provider, "tmax");

        assert!(loaded.config.gateway.is_some());
        assert_eq!(
            loaded.config.network.unwrap().proxy_url,
            "http://127.0.0.1:8080"
        );
    }

    #[tokio::test]
    async fn inline_relayed_entry_is_rewritten_in_place() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);
        write(&store, INLINE_ENTRY_FIXTURE);
        let applied = dispatch(&store, false).await.unwrap();
        assert_eq!(
            applied["entries"],
            json!([
                {"target":"inline","entry":"relay"},
                {"target":"legacy","entry":"old"}
            ])
        );
        let text = String::from_utf8(read(&store)).unwrap();
        assert!(text.contains("provider = \"tmax\""));
        assert!(!text.contains("gateway = true"));
        assert!(!text.contains("proxy = true"));
        assert!(text.contains("relay = {"));
        let loaded = config::load(&store).unwrap();
        assert_eq!(
            loaded.config.agents["inline"].providers["relay"].provider,
            "tmax"
        );
        assert_eq!(
            loaded.config.agents["legacy"].providers["old"].provider,
            "tmax"
        );
    }

    #[tokio::test]
    async fn relayed_entry_without_a_gateway_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);
        write(&store, ORPHAN_GATEWAY_FIXTURE);
        let before = read(&store);
        for dry_run in [true, false] {
            let error = dispatch(&store, dry_run).await.unwrap_err();
            assert_eq!(error.code, "migration_refused");
            let reference = error
                .affected
                .iter()
                .find(|reference| reference.kind == "agent_provider_entry")
                .unwrap();
            assert_eq!(reference.id.as_deref(), Some("local/relay"));
            assert_eq!(reference.scope.as_deref(), Some("gateway_not_configured"));
            assert_eq!(read(&store), before);
        }
    }

    #[tokio::test]
    async fn concrete_gateway_selection_is_refused_and_dropping_it_migrates() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);
        write(&store, CONCRETE_FIXTURE);
        let before = read(&store);
        let error = dispatch(&store, false).await.unwrap_err();
        assert_eq!(error.code, "migration_refused");
        assert!(error.message.contains("up/model"));
        assert!(error.message.contains("route names only"));
        let reference = error
            .affected
            .iter()
            .find(|reference| reference.kind == "agent_provider_entry")
            .unwrap();
        assert_eq!(reference.id.as_deref(), Some("local/relay"));
        assert_eq!(
            reference.scope.as_deref(),
            Some("concrete-gateway-selection")
        );
        assert_eq!(read(&store), before);

        write(&store, GATEWAY_FIXTURE);
        let applied = dispatch(&store, false).await.unwrap();
        assert_eq!(applied["state"], "migrated");
        assert_eq!(
            applied["entries"],
            json!([{"target":"local","entry":"relay"}])
        );
        let loaded = config::load(&store).unwrap();
        assert_eq!(
            loaded.config.agents["local"].providers["relay"].provider,
            "tmax"
        );
    }

    #[tokio::test]
    async fn shared_continuity_fallback_and_candidate_effort_merge_into_one_route() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);
        write(&store, SHARED_POLICY_FIXTURE);
        let applied = dispatch(&store, false).await.unwrap();
        assert_eq!(applied["state"], "migrated");
        assert_eq!(
            applied["aliases"]["merged"],
            json!([{"id":"route","protocols":["chat-completions","responses"]}])
        );
        let loaded = config::load(&store).unwrap();
        let alias = &loaded.config.model_aliases[0];
        assert_eq!(alias.protocols, ["chat-completions", "responses"]);
        assert_eq!(alias.continuity.as_deref(), Some("stateless"));
        assert_eq!(
            alias.fallback,
            Some(config::FailureConditions {
                http_statuses: vec![429],
                connection_errors: false,
                transport_errors: false,
            })
        );
        assert_eq!(alias.candidate_reasoning_effort["up/model"], "high");
        let text = String::from_utf8(read(&store)).unwrap();
        assert!(text.contains("continuity = \"stateless\""));
        assert!(text.contains("fallback = {"));
        assert!(text.contains("candidate_reasoning_effort = {"));
    }

    #[tokio::test]
    async fn comments_and_unrelated_settings_survive_in_place_editing() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);
        let fixture = "# top comment\nversion = 1\n\n# provider comment\n[providers.up]\nintegration = \"custom\"\n\n[providers.up.endpoints]\nresponses = \"http://127.0.0.1:39999\"\n\n[providers.up.models.model]\n\n[network]\nproxy_url = \"http://127.0.0.1:8080\"\n";
        write(&store, fixture);
        let applied = dispatch(&store, false).await.unwrap();
        assert_eq!(applied["aliases"]["total_records"].as_u64(), Some(0));
        assert_eq!(applied["aliases"]["total_routes"].as_u64(), Some(0));
        assert_eq!(applied["entries"], json!([]));
        let text = String::from_utf8(read(&store)).unwrap();
        assert!(text.contains("# top comment"));
        assert!(text.contains("# provider comment"));
        assert!(text.contains("version = 2"));
        assert!(text.contains("[providers.up]"));
        assert!(text.contains("proxy_url = \"http://127.0.0.1:8080\""));
        config::load(&store).unwrap();
    }

    #[tokio::test]
    async fn absent_configuration_is_an_unknown_resource() {
        let directory = tempfile::tempdir().unwrap();
        let store = store(&directory);
        let error = dispatch(&store, false).await.unwrap_err();
        assert_eq!(error.code, "unknown_resource");
        assert_eq!(error.message, "There is no configuration file to migrate");
    }

    /// Acceptance shape: nine routes that each declare two protocols migrate from
    /// eighteen per-protocol records to nine records with one candidate list each.
    #[tokio::test]
    async fn eighteen_protocol_records_merge_into_nine_routes() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        let mut text = String::from(
            "version=1\n[gateway]\nlisten='127.0.0.1:17840'\nkey_ref='secret:gw'\n[providers.up]\nintegration='custom'\n[providers.up.endpoints]\nchat-completions='http://127.0.0.1:39999'\nresponses='http://127.0.0.1:39999'\n[providers.up.auth]\nmethod='api-key'\nkey_ref='secret:up'\n",
        );
        for index in 0..9 {
            text.push_str(&format!("[providers.up.models.'m{index}']\n"));
        }
        for index in 0..9 {
            for protocol in ["chat-completions", "responses"] {
                text.push_str(&format!(
                    "[[model_aliases]]\nid='route-{index}'\nprotocol='{protocol}'\ncandidates=['up/m{index}']\ncontinuity='stateless'\n"
                ));
            }
        }
        store.write_raw("tmax.toml", text.as_bytes()).unwrap();
        let request = |dry_run| Request {
            command: vec!["config".into(), "migrate".into()],
            operands: Vec::new(),
            arguments: Default::default(),
            dry_run,
            if_revision: None,
        };
        let preview = run(&store, &request(true)).unwrap();
        assert_eq!(preview["aliases"]["total_records"], 18);
        assert_eq!(preview["aliases"]["total_routes"], 9);
        let applied = run(&store, &request(false)).unwrap();
        assert_eq!(applied["aliases"]["total_routes"], 9);
        let loaded = config::load(&store).unwrap();
        assert_eq!(loaded.config.model_aliases.len(), 9);
        assert!(loaded.config.model_aliases.iter().all(|route| {
            route.protocols == ["chat-completions", "responses"]
                && route.candidates.len() == 1
                && route.continuity.as_deref() == Some("stateless")
        }));
    }

    #[test]
    fn stored_reserved_provider_with_a_gateway_is_refused_not_dropped() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::at(directory.path().into());
        std::fs::create_dir_all(&store.root).unwrap();
        let text = "version = 1\n[gateway]\nlisten = \"127.0.0.1:17840\"\nkey_ref = \"secret:gw\"\n[providers.tmax]\nintegration = \"custom\"\n[providers.tmax.endpoints]\nresponses = \"http://127.0.0.1:1\"\n";
        store.write_raw("tmax.toml", text.as_bytes()).unwrap();
        let error = run(&store, &request(false)).unwrap_err();
        assert_eq!(error.code, "migration_refused");
        assert!(error.message.contains("provider 'tmax'"), "{error:?}");
        assert!(
            error
                .affected
                .iter()
                .any(|reference| reference.scope.as_deref() == Some("providers.tmax")),
            "{error:?}"
        );
        assert_eq!(
            std::fs::read_to_string(store.root.join("tmax.toml")).unwrap(),
            text
        );
    }
}
