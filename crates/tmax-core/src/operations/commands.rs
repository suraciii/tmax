use crate::{Error, Request, Result};
use serde_json::{Value, json};

struct Command {
    path: &'static str,
    min: usize,
    max: usize,
    options: &'static str,
    mutation: bool,
}
impl Command {
    fn supports_revision(&self) -> bool {
        self.mutation
            && !matches!(
                self.path,
                "skill install" | "provider login-cancel" | "usage clear"
            )
    }
}

/// Report periods accepted by the `usage` command group.
const USAGE_PERIODS: [&str; 5] = ["today", "7d", "30d", "90d", "all"];
/// Breakdown dimensions accepted by `usage breakdown --by`.
const USAGE_DIMENSIONS: [&str; 8] = [
    "provider",
    "model",
    "requested",
    "target",
    "protocol",
    "outcome",
    "origin",
    "day",
];

const COMMANDS: &[Command] = &[
    Command {
        path: "provider login-start",
        min: 1,
        max: 1,
        options: "via new-codex method",
        mutation: true,
    },
    Command {
        path: "provider login-status",
        min: 1,
        max: 1,
        options: "",
        mutation: false,
    },
    Command {
        path: "provider login-cancel",
        min: 1,
        max: 1,
        options: "attempt",
        mutation: true,
    },
    Command {
        path: "skill install",
        min: 0,
        max: 0,
        options: "agent",
        mutation: true,
    },
    Command {
        path: "status",
        min: 0,
        max: 0,
        options: "brief",
        mutation: false,
    },
    Command {
        path: "integration list",
        min: 0,
        max: 0,
        options: "",
        mutation: false,
    },
    Command {
        path: "integration inspect",
        min: 1,
        max: 1,
        options: "",
        mutation: false,
    },
    Command {
        path: "config show",
        min: 0,
        max: 0,
        options: "state",
        mutation: false,
    },
    Command {
        path: "config validate",
        min: 0,
        max: 0,
        options: "",
        mutation: false,
    },
    Command {
        path: "config migrate",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "config reload",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "config reset",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "config recovery list",
        min: 0,
        max: 0,
        options: "",
        mutation: false,
    },
    Command {
        path: "config recovery inspect",
        min: 1,
        max: 1,
        options: "",
        mutation: false,
    },
    Command {
        path: "config recovery discard",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "provider list",
        min: 0,
        max: 0,
        options: "",
        mutation: false,
    },
    Command {
        path: "provider inspect",
        min: 1,
        max: 1,
        options: "",
        mutation: false,
    },
    Command {
        path: "provider refresh",
        min: 1,
        max: 1,
        options: "scope",
        mutation: true,
    },
    Command {
        path: "provider failure-reset",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "provider quota-resume",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "provider add",
        min: 1,
        max: 1,
        options: "endpoint set clear",
        mutation: true,
    },
    Command {
        path: "provider update",
        min: 1,
        max: 1,
        options: "endpoint clear-endpoint set clear",
        mutation: true,
    },
    Command {
        path: "provider rename",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "provider remove",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "provider key set",
        min: 1,
        max: 1,
        options: "key-stdin key-ref",
        mutation: true,
    },
    Command {
        path: "provider auth set",
        min: 1,
        max: 1,
        options: "via session-ref",
        mutation: true,
    },
    Command {
        path: "provider auth clear",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "provider login",
        min: 1,
        max: 1,
        options: "via",
        mutation: true,
    },
    Command {
        path: "provider logout",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "secret list",
        min: 0,
        max: 0,
        options: "",
        mutation: false,
    },
    Command {
        path: "secret cleanup",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "provider input list",
        min: 1,
        max: 1,
        options: "state",
        mutation: false,
    },
    Command {
        path: "provider input inspect",
        min: 2,
        max: 2,
        options: "state",
        mutation: false,
    },
    Command {
        path: "provider input add",
        min: 2,
        max: 2,
        options: "kind source scope model max-age refresh-every",
        mutation: true,
    },
    Command {
        path: "provider input replace",
        min: 2,
        max: 2,
        options: "kind source scope model max-age refresh-every",
        mutation: true,
    },
    Command {
        path: "provider input remove",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "provider input refresh",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "provider input record",
        min: 2,
        max: 2,
        options: "observed-at valid-until reset-at remaining-percent remaining total unit reference if-input-revision",
        mutation: true,
    },
    Command {
        path: "provider input clear",
        min: 2,
        max: 2,
        options: "if-input-revision",
        mutation: true,
    },
    Command {
        path: "model list",
        min: 0,
        max: 0,
        options: "provider protocol kind",
        mutation: false,
    },
    Command {
        path: "model inspect",
        min: 1,
        max: 1,
        options: "protocol",
        mutation: false,
    },
    Command {
        path: "model add",
        min: 1,
        max: 1,
        options: "set clear",
        mutation: true,
    },
    Command {
        path: "model update",
        min: 1,
        max: 1,
        options: "set clear",
        mutation: true,
    },
    Command {
        path: "model rename",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "model remove",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "model override list replace",
        min: 2,
        max: 2,
        options: "value",
        mutation: true,
    },
    Command {
        path: "model reasoning add",
        min: 1,
        max: 1,
        options: "type value empty-values",
        mutation: true,
    },
    Command {
        path: "model reasoning replace",
        min: 2,
        max: 2,
        options: "type value empty-values",
        mutation: true,
    },
    Command {
        path: "model reasoning remove",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "model reasoning clear",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "model effort-map set",
        min: 3,
        max: 3,
        options: "protocol",
        mutation: true,
    },
    Command {
        path: "model effort-map remove",
        min: 2,
        max: 2,
        options: "protocol",
        mutation: true,
    },
    Command {
        path: "model effort-map clear",
        min: 1,
        max: 1,
        options: "protocol",
        mutation: true,
    },
    Command {
        path: "model schedule add",
        min: 1,
        max: 1,
        options: "priority weekday date start end effective-from effective-until",
        mutation: true,
    },
    Command {
        path: "model schedule replace",
        min: 2,
        max: 2,
        options: "priority weekday date start end effective-from effective-until",
        mutation: true,
    },
    Command {
        path: "model schedule remove",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "model schedule clear",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "model rule list",
        min: 1,
        max: 1,
        options: "state",
        mutation: false,
    },
    Command {
        path: "model rule inspect",
        min: 2,
        max: 2,
        options: "state at",
        mutation: false,
    },
    Command {
        path: "model rule add",
        min: 2,
        max: 2,
        options: "input priority reset-within enabled remaining-at-least-percent weekday date start end effective-from effective-until",
        mutation: true,
    },
    Command {
        path: "model rule replace",
        min: 2,
        max: 2,
        options: "input priority reset-within enabled remaining-at-least-percent weekday date start end effective-from effective-until",
        mutation: true,
    },
    Command {
        path: "model rule remove",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "model rule move",
        min: 2,
        max: 2,
        options: "before last",
        mutation: true,
    },
    Command {
        path: "model rule enable",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "model rule disable",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "model rule clear",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "model alias add",
        min: 1,
        max: 1,
        options: "protocol candidate candidate-reasoning-effort clear-candidate-reasoning-effort set clear",
        mutation: true,
    },
    Command {
        path: "model alias update",
        min: 1,
        max: 1,
        options: "protocol candidate candidate-reasoning-effort clear-candidate-reasoning-effort set clear",
        mutation: true,
    },
    Command {
        path: "model alias candidates add",
        min: 1,
        max: 1,
        options: "candidate",
        mutation: true,
    },
    Command {
        path: "model alias candidates remove",
        min: 1,
        max: 1,
        options: "candidate",
        mutation: true,
    },
    Command {
        path: "model alias rename",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "model alias remove",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "model explain",
        min: 1,
        max: 1,
        options: "protocol target agent-provider agent path at session state",
        mutation: false,
    },
    Command {
        path: "catalog providers",
        min: 0,
        max: 0,
        options: "snapshot",
        mutation: false,
    },
    Command {
        path: "catalog models",
        min: 1,
        max: 1,
        options: "snapshot",
        mutation: false,
    },
    Command {
        path: "catalog inspect",
        min: 1,
        max: 1,
        options: "model snapshot",
        mutation: false,
    },
    Command {
        path: "catalog refresh",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "catalog apply",
        min: 0,
        max: 0,
        options: "revision",
        mutation: true,
    },
    Command {
        path: "agent list",
        min: 0,
        max: 0,
        options: "",
        mutation: false,
    },
    Command {
        path: "agent discover",
        min: 0,
        max: 0,
        options: "agent",
        mutation: false,
    },
    Command {
        path: "agent inspect",
        min: 0,
        max: 1,
        options: "agent path profile",
        mutation: false,
    },
    Command {
        path: "agent add",
        min: 1,
        max: 1,
        options: "agent path",
        mutation: true,
    },
    Command {
        path: "agent update",
        min: 1,
        max: 1,
        options: "agent path",
        mutation: true,
    },
    Command {
        path: "agent rename",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "agent remove",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "agent provider add",
        min: 2,
        max: 2,
        options: "provider",
        mutation: true,
    },
    Command {
        path: "agent provider update",
        min: 2,
        max: 2,
        options: "provider",
        mutation: true,
    },
    Command {
        path: "agent provider remove",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "agent model add",
        min: 3,
        max: 3,
        options: "protocol force",
        mutation: true,
    },
    Command {
        path: "agent model remove",
        min: 3,
        max: 3,
        options: "protocol",
        mutation: true,
    },
    Command {
        path: "agent model clear",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "agent default set",
        min: 3,
        max: 3,
        options: "protocol effort native-effort",
        mutation: true,
    },
    Command {
        path: "agent default clear",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "agent role set",
        min: 4,
        max: 4,
        options: "protocol",
        mutation: true,
    },
    Command {
        path: "agent role remove",
        min: 2,
        max: 2,
        options: "",
        mutation: true,
    },
    Command {
        path: "agent role clear",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "agent apply",
        min: 1,
        max: 1,
        options: "profile",
        mutation: true,
    },
    Command {
        path: "agent restore",
        min: 0,
        max: 1,
        options: "recovery",
        mutation: true,
    },
    Command {
        path: "agent recovery list",
        min: 0,
        max: 0,
        options: "target",
        mutation: false,
    },
    Command {
        path: "agent recovery inspect",
        min: 1,
        max: 1,
        options: "",
        mutation: false,
    },
    Command {
        path: "network proxy",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
    },
    Command {
        path: "network direct",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "network show",
        min: 0,
        max: 0,
        options: "state",
        mutation: false,
    },
    Command {
        path: "gateway configure",
        min: 0,
        max: 0,
        options: "listen set clear",
        mutation: true,
    },
    Command {
        path: "gateway remove",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "gateway key replace",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "gateway key show",
        min: 0,
        max: 0,
        options: "output-fd",
        mutation: true,
    },
    Command {
        path: "gateway status",
        min: 0,
        max: 0,
        options: "",
        mutation: false,
    },
    Command {
        path: "gateway start",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "gateway stop",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "gateway restart",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "web status",
        min: 0,
        max: 0,
        options: "",
        mutation: false,
    },
    Command {
        path: "web open",
        min: 0,
        max: 0,
        options: "listen no-browser",
        mutation: true,
    },
    Command {
        path: "web stop",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "model probe",
        min: 1,
        max: 1,
        options: "protocol transport",
        mutation: true,
    },
    Command {
        path: "session list",
        min: 0,
        max: 0,
        options: "target agent path cwd since until limit",
        mutation: false,
    },
    Command {
        path: "session import",
        min: 2,
        max: 2,
        options: "to title cwd tail allow-over-budget again",
        mutation: true,
    },
    Command {
        path: "history list",
        min: 0,
        max: 0,
        options: "since until provider model protocol outcome target",
        mutation: false,
    },
    Command {
        path: "history inspect",
        min: 1,
        max: 1,
        options: "",
        mutation: false,
    },
    Command {
        path: "history clear",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "usage summary",
        min: 0,
        max: 0,
        options: "period since until",
        mutation: false,
    },
    Command {
        path: "usage breakdown",
        min: 0,
        max: 0,
        options: "period since until by",
        mutation: false,
    },
    Command {
        path: "usage insights",
        min: 0,
        max: 0,
        options: "period since until",
        mutation: false,
    },
    Command {
        path: "usage clear",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
];
pub fn catalog() -> Value {
    json!(COMMANDS.iter().map(|c| {
        let owner = if c.path.starts_with("provider ") {
            "provider"
        } else if c.path.starts_with("model alias ") {
            "alias"
        } else if c.path.starts_with("model ") {
            "model"
        } else {
            ""
        };
        let field_defs = super::edit::fields::fields(owner);
        let metadata: serde_json::Map<String, Value> = c.options.split_whitespace().map(|name| {
            let fields = if name == "set" || name == "clear" {
                if c.path == "gateway configure" {
                    vec!["autostart"]
                } else {
                    field_defs.iter().filter(|(field, kind)| {
                        if name == "set" { *kind != "container" } else { *field != "overrides.interleaved.field" }
                    }).map(|(field, _)| *field).collect()
                }
            } else {
                vec![]
            };
            let field_types: serde_json::Map<String, Value> = fields.iter().map(|field| {
                let kind = if c.path == "gateway configure" {
                    "boolean"
                } else {
                    field_defs.iter().find(|(name, _)| name == field).expect("declared field").1
                };
                ((*field).into(), json!(kind))
            }).collect();
            let choices = match name {
                "protocol" if c.path.starts_with("model effort-map ") => vec!["chat-completions", "responses"],
                "protocol" => vec!["chat-completions", "responses", "messages"],
                "agent" if c.path == "skill install" => vec!["hermes"],
                "agent" => vec!["codex", "pi", "omp", "opencode", "hermes", "dsh"],
                "state" => vec!["file", "loaded"],
                "snapshot" => vec!["accepted", "latest"],
                "method" if c.path == "provider login-start" => vec!["browser", "device"],
                "source" => vec!["manual", "provider", "codex-resets"],
                "kind" if c.path.starts_with("provider input") => vec!["quota", "reset"],
                "kind" => vec!["concrete", "alias"],
                "enabled" => vec!["true", "false"],
                "transport" => vec!["http"],
                "weekday" => vec!["mon", "tue", "wed", "thu", "fri", "sat", "sun"],
                "period" => USAGE_PERIODS.to_vec(),
                "by" => USAGE_DIMENSIONS.to_vec(),
                _ => vec![],
            };
            (name.into(), json!({
                "arity": if matches!(name, "set" | "endpoint") { 2 } else { 1 },
                "choices": choices,
                "fields": fields,
                "field_types": field_types,
                "repeatable": matches!(name, "set" | "clear" | "endpoint" | "clear-endpoint" | "value" | "weekday" | "date" | "candidate" | "candidate-reasoning-effort" | "clear-candidate-reasoning-effort" | "model")
                    || (name == "protocol" && matches!(c.path, "model alias add" | "model alias update")),
                "switch": matches!(name, "new-codex" | "last" | "empty-values" | "no-browser" | "key-stdin" | "brief"),
            }))
        }).collect();
        json!({
            "command": c.path.split_whitespace().collect::<Vec<_>>(),
            "operation": c.path,
            "operands": {"min": c.min, "max": c.max, "labels": operand_labels(c.path)},
            "arguments": c.options.split_whitespace().collect::<Vec<_>>(),
            "argument_metadata": metadata,
            "mutation": c.mutation,
            "dry_run": c.mutation,
            "revision_guard": c.supports_revision(),
        })
    }).collect::<Vec<_>>())
}
fn operand_labels(path: &str) -> &'static [&'static str] {
    match path {
        "integration inspect" => &["Integration ID"],
        "provider rename" => &["Existing provider ID", "New provider ID"],
        p if p.starts_with("provider input ") => {
            if p.ends_with("list") {
                &["Provider ID"]
            } else {
                &["Provider ID", "Input name"]
            }
        }
        p if p.starts_with("provider ") => &["Provider ID"],
        "model rename" => &["Qualified model", "New exact upstream model ID"],
        "model effort-map set" => &["Qualified model", "Input effort", "Upstream effort"],
        "model effort-map remove" => &["Qualified model", "Input effort"],
        p if p.starts_with("model rule ") => {
            if p.ends_with("list") || p.ends_with("clear") {
                &["Qualified model"]
            } else {
                &["Qualified model", "Rule ID"]
            }
        }
        "model reasoning replace"
        | "model reasoning remove"
        | "model schedule replace"
        | "model schedule remove" => &["Qualified model", "Local index"],
        "model override list replace" => &["Qualified model", "Metadata list field"],
        "model alias rename" => &["Existing alias ID", "New alias ID"],
        p if p.starts_with("model alias ") => &["Alias ID"],
        "model explain" => &["Public model ID"],
        p if p.starts_with("model ") => &["Qualified model"],
        "agent rename" => &["Existing target ID", "New target ID"],
        "agent role set" => &[
            "Target ID",
            "Native role",
            "Native provider entry",
            "Model ID",
        ],
        "agent role remove" => &["Target ID", "Native role"],
        "agent model add" | "agent model remove" | "agent default set" => {
            &["Target ID", "Native provider entry", "Model ID"]
        }
        p if p.starts_with("agent provider ") || p == "agent model clear" => {
            &["Target ID", "Native provider entry"]
        }
        p if p.starts_with("agent recovery ") || p.starts_with("config recovery ") => {
            &["Recovery handle"]
        }
        p if p.starts_with("agent ") => &["Target ID"],
        p if p.starts_with("catalog ") => &["Catalog provider ID"],
        "network proxy" => &["Outbound proxy URL"],
        "history inspect" => &["Request handle"],
        "session import" => &["Source target", "Native session ID"],
        "secret cleanup" => &["Secret reference"],
        _ => &[],
    }
}
pub fn validate(r: &Request) -> Result<()> {
    let path = r.command.join(" ");
    let c = COMMANDS
        .iter()
        .find(|c| c.path == path)
        .ok_or_else(|| Error::new("invalid_arguments", "Unknown management command path"))?;
    if r.operands.len() < c.min || r.operands.len() > c.max {
        return Err(Error::new(
            "invalid_arguments",
            "Unexpected number of positional operands",
        ));
    }
    if r.dry_run && !c.mutation {
        return Err(Error::new(
            "invalid_arguments",
            "Effect-free reads do not accept dry-run",
        ));
    }
    if r.if_revision.is_some() && !c.supports_revision() {
        return Err(Error::new(
            "invalid_arguments",
            "This operation does not save configuration",
        ));
    }
    for (key, values) in &r.arguments {
        let private_key_fd = path == "provider key set" && key == "key-fd";
        if !private_key_fd
            && !c
                .options
                .split_whitespace()
                .chain(["non-interactive"])
                .any(|k| k == key)
        {
            return Err(Error::new(
                "invalid_arguments",
                "Unknown option for this finite management command",
            )
            .field(key));
        }
        let repeated = matches!(
            key.as_str(),
            "set"
                | "clear"
                | "endpoint"
                | "clear-endpoint"
                | "value"
                | "weekday"
                | "date"
                | "candidate"
                | "candidate-reasoning-effort"
                | "clear-candidate-reasoning-effort"
        ) || (key == "model" && path.starts_with("provider input "))
            || (key == "protocol"
                && matches!(path.as_str(), "model alias add" | "model alias update"));
        if values.is_empty()
            || (!repeated && values.len() != 1)
            || ((key == "set" || key == "endpoint") && values.len() % 2 != 0)
        {
            return Err(
                Error::new("invalid_arguments", "Option occurrence or arity is invalid").field(key),
            );
        }
        if matches!(
            key.as_str(),
            "non-interactive"
                | "new-codex"
                | "key-stdin"
                | "last"
                | "empty-values"
                | "no-browser"
                | "brief"
        ) && values.as_slice() != ["true"]
        {
            return Err(Error::new(
                "invalid_arguments",
                "Boolean switches must be represented as true",
            )
            .field(key));
        }
    }
    if path.starts_with("model effort-map ")
        && !super::arg(r, "protocol")
            .is_some_and(|protocol| matches!(protocol, "chat-completions" | "responses"))
    {
        return Err(Error::new(
            "invalid_arguments",
            "Effort maps require --protocol chat-completions or responses; Messages is unsupported",
        )
        .field("protocol"));
    }
    if path == "provider key set" {
        let key_stdin = r.arguments.contains_key("key-stdin");
        let key_fd = r
            .arguments
            .get("key-fd")
            .and_then(|v| v.first())
            .and_then(|s| crate::secrets::parse_descriptor(s));
        if (key_stdin && r.arguments.contains_key("key-fd"))
            || (r.arguments.contains_key("key-ref") && (key_stdin || key_fd.is_some()))
        {
            return Err(Error::new(
                "invalid_arguments",
                "Key input choices are mutually exclusive, and key-reference selection accepts no key input",
            ));
        }
    }
    for key in ["key-fd", "output-fd"] {
        if let Some(values) = r.arguments.get(key)
            && crate::secrets::parse_descriptor(&values[0])
                .is_none_or(|fd| key == "output-fd" && fd < 3)
        {
            return Err(Error::new(
                "invalid_arguments",
                "Protected channel descriptor is outside its documented range",
            )
            .field(key));
        }
    }
    for key in ["state", "protocol", "agent", "snapshot", "period", "by"] {
        if let Some(values) = r.arguments.get(key) {
            if values.is_empty() {
                return Err(Error::new(
                    "invalid_arguments",
                    "Option occurrence or arity is invalid",
                )
                .field(key));
            }
            let valid = match key {
                "state" => ["file", "loaded"].contains(&values[0].as_str()),
                // `--protocol` is repeatable: every declared value must be known.
                "protocol" => values
                    .iter()
                    .all(|value| crate::config::PROTOCOLS.contains(&value.as_str())),
                "agent" => {
                    if path == "skill install" {
                        values[0] == "hermes"
                    } else {
                        ["codex", "pi", "omp", "opencode", "hermes", "dsh"]
                            .contains(&values[0].as_str())
                    }
                }
                "snapshot" => ["accepted", "latest"].contains(&values[0].as_str()),
                "period" => USAGE_PERIODS.contains(&values[0].as_str()),
                "by" => USAGE_DIMENSIONS.contains(&values[0].as_str()),
                _ => true,
            };
            if !valid {
                return Err(
                    Error::new("invalid_arguments", "Unsupported enumerated option value")
                        .field(key),
                );
            }
        }
    }
    if path.starts_with("usage ") {
        validate_usage_scope(r, &path)?;
    }
    if (path == "model schedule replace"
        || path == "model schedule remove"
        || path == "model reasoning replace"
        || path == "model reasoning remove")
        && r.if_revision.is_none()
    {
        return Err(Error::new(
            "invalid_arguments",
            "Indexed replacement or removal requires if-revision",
        ));
    }
    Ok(())
}

/// Validate the `usage` group's period and range contract.
///
/// The Web path bypasses clap, so these rules live beside the descriptor that
/// declares the options. `--period` selects a trailing calendar window or `all`;
/// `--since`/`--until` select one explicit `[since, until)` range. The two forms
/// are mutually exclusive, a range needs both bounds, and the range must be
/// increasing. `usage breakdown` requires exactly one known `--by` dimension.
fn validate_usage_scope(r: &Request, path: &str) -> Result<()> {
    let period = r.arguments.get("period");
    let since = r.arguments.get("since").and_then(|v| v.first());
    let until = r.arguments.get("until").and_then(|v| v.first());
    if period.is_some() && (since.is_some() || until.is_some()) {
        return Err(Error::new(
            "invalid_arguments",
            "A period and an explicit since/until range are mutually exclusive; keep --period or keep --since with --until",
        )
        .field("period"));
    }
    if since.is_some() != until.is_some() {
        return Err(Error::new(
            "invalid_arguments",
            "An explicit range requires both --since and --until",
        )
        .field(if since.is_some() { "until" } else { "since" }));
    }
    if let (Some(since), Some(until)) = (since, until) {
        let since = usage_instant(since, "since")?;
        let until = usage_instant(until, "until")?;
        if since >= until {
            return Err(Error::new(
                "invalid_arguments",
                "The --since bound must precede the --until bound",
            )
            .field("since"));
        }
    }
    if path == "usage breakdown" {
        let dimension = r.arguments.get("by").and_then(|v| v.first());
        let Some(dimension) = dimension else {
            return Err(Error::new(
                "invalid_arguments",
                "usage breakdown requires --by with one dimension: provider, model, requested, target, protocol, outcome, origin, or day",
            )
            .field("by"));
        };
        if !USAGE_DIMENSIONS.contains(&dimension.as_str()) {
            return Err(Error::new(
                "invalid_arguments",
                "Unsupported breakdown dimension; use provider, model, requested, target, protocol, outcome, origin, or day",
            )
            .field("by"));
        }
    }
    Ok(())
}

fn usage_instant(value: &str, field: &str) -> Result<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|at| at.with_timezone(&chrono::Utc))
        .map_err(|_| {
            Error::new(
                "invalid_instant",
                "Expected an RFC 3339 instant with an explicit offset, such as 2026-11-01T00:00:00Z",
            )
            .field(field)
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn usage_commands_are_finite_and_validate_their_scope() {
        let entries = catalog();
        let find = |operation: &str| {
            entries
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["operation"] == operation)
                .unwrap()
                .clone()
        };
        for (operation, options) in [
            ("usage summary", vec!["period", "since", "until"]),
            ("usage breakdown", vec!["period", "since", "until", "by"]),
            ("usage insights", vec!["period", "since", "until"]),
            ("usage clear", vec![]),
        ] {
            let entry = find(operation);
            assert_eq!(entry["operands"]["min"], 0);
            assert_eq!(entry["operands"]["max"], 0);
            assert_eq!(entry["arguments"], json!(options));
            assert_eq!(entry["mutation"], operation == "usage clear");
            assert_eq!(entry["dry_run"], operation == "usage clear");
            assert_eq!(entry["revision_guard"], false);
        }
        assert_eq!(
            find("usage summary")["argument_metadata"]["period"]["choices"],
            json!(["today", "7d", "30d", "90d", "all"])
        );
        assert_eq!(
            find("usage breakdown")["argument_metadata"]["by"]["choices"],
            json!([
                "provider",
                "model",
                "requested",
                "target",
                "protocol",
                "outcome",
                "origin",
                "day"
            ])
        );
        let usage_request =
            |command: &[&str], arguments: &[(&str, &[&str])], dry_run: bool| Request {
                command: command.iter().map(|value| (*value).into()).collect(),
                operands: vec![],
                arguments: arguments
                    .iter()
                    .map(|(key, values)| {
                        (
                            (*key).into(),
                            values.iter().map(|value| (*value).into()).collect(),
                        )
                    })
                    .collect(),
                dry_run,
                if_revision: None,
            };
        let since = "2026-11-01T00:00:00Z";
        let until = "2026-11-08T00:00:00Z";
        for operation in [&["usage", "summary"][..], &["usage", "insights"][..]] {
            for arguments in [
                vec![],
                vec![("period", &["today"][..])],
                vec![("period", &["7d"][..])],
                vec![("period", &["30d"][..])],
                vec![("period", &["90d"][..])],
                vec![("period", &["all"][..])],
                vec![("since", &[since][..]), ("until", &[until][..])],
            ] {
                let request = usage_request(operation, &arguments, false);
                validate(&request).unwrap();
                assert!(validate(&usage_request(operation, &arguments, true)).is_err());
            }
        }
        validate(&usage_request(
            &["usage", "breakdown"],
            &[("by", &["day"][..]), ("period", &["30d"][..])],
            false,
        ))
        .unwrap();
        validate(&usage_request(&["usage", "clear"], &[], false)).unwrap();
        validate(&usage_request(&["usage", "clear"], &[], true)).unwrap();

        for (command, arguments) in [
            (
                &["usage", "summary"][..],
                vec![("period", &["7d"][..]), ("since", &[since][..])],
            ),
            (
                &["usage", "summary"][..],
                vec![("period", &["all"][..]), ("until", &[until][..])],
            ),
            (&["usage", "summary"][..], vec![("since", &[since][..])]),
            (&["usage", "summary"][..], vec![("until", &[until][..])]),
            (
                &["usage", "summary"][..],
                vec![("since", &[until][..]), ("until", &[since][..])],
            ),
            (
                &["usage", "summary"][..],
                vec![("since", &[since][..]), ("until", &[since][..])],
            ),
            (&["usage", "summary"][..], vec![("period", &["week"][..])]),
            (&["usage", "summary"][..], vec![("by", &["provider"][..])]),
            (&["usage", "breakdown"][..], vec![("period", &["7d"][..])]),
            (&["usage", "breakdown"][..], vec![("by", &["unknown"][..])]),
            (
                &["usage", "breakdown"][..],
                vec![("by", &["day", "provider"][..])],
            ),
            (&["usage", "clear"][..], vec![("period", &["7d"][..])]),
            (&["usage", "clear"][..], vec![("by", &["day"][..])]),
        ] {
            let request = usage_request(command, &arguments, false);
            assert_eq!(
                validate(&request).unwrap_err().code,
                "invalid_arguments",
                "{command:?} {arguments:?}"
            );
        }
        let request = usage_request(
            &["usage", "summary"],
            &[("since", &["not-an-instant"][..]), ("until", &[until][..])],
            false,
        );
        assert_eq!(validate(&request).unwrap_err().code, "invalid_instant");
        let mut guarded = usage_request(&["usage", "clear"], &[], false);
        guarded.if_revision = Some("absent".into());
        assert!(validate(&guarded).is_err());
    }

    #[test]
    fn effort_map_catalog_and_protocol_contract_are_finite() {
        let entries = catalog();
        for (action, operands) in [
            ("set", vec!["p/m", "xhigh", "high"]),
            ("remove", vec!["p/m", "xhigh"]),
            ("clear", vec!["p/m"]),
        ] {
            let operation = format!("model effort-map {action}");
            let entry = entries
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["operation"] == operation)
                .unwrap();
            assert_eq!(entry["operands"]["min"], operands.len());
            assert_eq!(entry["operands"]["max"], operands.len());
            assert_eq!(
                entry["argument_metadata"]["protocol"]["choices"],
                json!(["chat-completions", "responses"])
            );
            assert_eq!(entry["mutation"], true);
            assert_eq!(entry["dry_run"], true);
            assert_eq!(entry["revision_guard"], true);
            let mut request = Request {
                command: operation.split_whitespace().map(str::to_string).collect(),
                operands: operands.into_iter().map(str::to_string).collect(),
                arguments: std::collections::BTreeMap::new(),
                dry_run: true,
                if_revision: Some("revision".into()),
            };
            assert!(validate(&request).is_err());
            request
                .arguments
                .insert("protocol".into(), vec!["messages".into()]);
            assert!(validate(&request).is_err());
            request
                .arguments
                .insert("protocol".into(), vec!["responses".into()]);
            validate(&request).unwrap();
            request.arguments.insert(
                "set".into(),
                vec!["effort_map.responses.high".into(), "low".into()],
            );
            assert!(validate(&request).is_err());
        }
    }

    #[test]
    fn rejects_arbitrary_web_fields() {
        let request = Request {
            command: vec!["provider".into(), "add".into()],
            operands: vec!["x".into()],
            arguments: std::collections::BTreeMap::from([(
                "document".into(),
                vec!["version=2".into()],
            )]),
            dry_run: false,
            if_revision: None,
        };
        assert!(validate(&request).is_err());
    }

    #[test]
    fn native_agent_management_accepts_dsh() {
        let request = Request {
            command: vec!["agent".into(), "discover".into()],
            operands: vec![],
            arguments: std::collections::BTreeMap::from([("agent".into(), vec!["dsh".into()])]),
            dry_run: false,
            if_revision: None,
        };
        assert!(validate(&request).is_ok());
        let entries = catalog();
        let discover = entries
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["operation"] == "agent discover")
            .unwrap();
        assert!(
            discover["argument_metadata"]["agent"]["choices"]
                .as_array()
                .unwrap()
                .contains(&json!("dsh"))
        );
    }

    #[test]
    fn config_migrate_is_registered_and_alias_protocol_arity_matches_metadata() {
        let entries = catalog();
        let find = |operation: &str| {
            entries
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["operation"] == operation)
                .unwrap()
        };
        let migrate = find("config migrate");
        assert_eq!(migrate["mutation"], true);
        assert_eq!(migrate["dry_run"], true);
        assert_eq!(migrate["revision_guard"], true);
        for operation in ["model alias add", "model alias update"] {
            assert_eq!(
                find(operation)["argument_metadata"]["protocol"]["repeatable"],
                true
            );
        }
        assert!(
            find("model alias remove")["argument_metadata"]
                .get("protocol")
                .is_none()
        );
        let mut migrate = Request {
            command: vec!["config".into(), "migrate".into()],
            operands: vec![],
            arguments: std::collections::BTreeMap::new(),
            dry_run: true,
            if_revision: Some("absent".into()),
        };
        validate(&migrate).unwrap();
        migrate
            .arguments
            .insert("protocol".into(), vec!["responses".into()]);
        assert!(validate(&migrate).is_err());
        let mut alias = Request {
            command: vec!["model".into(), "alias".into(), "add".into()],
            operands: vec!["L0".into()],
            arguments: std::collections::BTreeMap::from([
                (
                    "protocol".into(),
                    vec!["responses".into(), "chat-completions".into()],
                ),
                ("candidate".into(), vec!["p/m".into()]),
            ]),
            dry_run: false,
            if_revision: None,
        };
        validate(&alias).unwrap();
        alias.command = vec!["model".into(), "inspect".into()];
        alias.operands = vec!["p/m".into()];
        alias.arguments.remove("candidate");
        assert!(validate(&alias).is_err());
    }

    #[test]
    fn skill_install_rejects_unsupported_agent_and_config_revision() {
        let entries = catalog();
        for (operation, supports_revision) in [
            ("skill install", false),
            ("status", false),
            ("provider add", true),
        ] {
            let entry = entries
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["operation"] == operation)
                .unwrap();
            assert_eq!(entry["revision_guard"], supports_revision);
        }
        let mut request = Request {
            command: vec!["skill".into(), "install".into()],
            operands: vec![],
            arguments: std::collections::BTreeMap::new(),
            dry_run: true,
            if_revision: None,
        };
        assert!(validate(&request).is_ok());
        for agent in ["codex", "pi", "omp", "opencode", "dsh", "agents", "unknown"] {
            request.arguments.insert("agent".into(), vec![agent.into()]);
            assert_eq!(validate(&request).unwrap_err().code, "invalid_arguments");
        }
        request
            .arguments
            .insert("agent".into(), vec!["hermes".into()]);
        assert!(validate(&request).is_ok());
        request.if_revision = Some("absent".into());
        assert!(validate(&request).is_err());
    }
}
