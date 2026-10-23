use crate::{Error, Request, Result};
use serde_json::{Value, json};

struct Command {
    path: &'static str,
    min: usize,
    max: usize,
    options: &'static str,
    mutation: bool,
}
const COMMANDS: &[Command] = &[
    Command {
        path: "status",
        min: 0,
        max: 0,
        options: "",
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
        path: "provider add",
        min: 1,
        max: 1,
        options: "metadata integration region endpoint set clear",
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
        options: "method via session-ref",
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
        path: "model schedule add",
        min: 1,
        max: 1,
        options: "priority weekday date all-day start end effective-from effective-until",
        mutation: true,
    },
    Command {
        path: "model schedule replace",
        min: 2,
        max: 2,
        options: "priority weekday date all-day start end effective-from effective-until",
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
        options: "input priority reset-within enabled remaining-at-least-percent weekday date all-day start end effective-from effective-until",
        mutation: true,
    },
    Command {
        path: "model rule replace",
        min: 2,
        max: 2,
        options: "input priority reset-within enabled remaining-at-least-percent weekday date all-day start end effective-from effective-until",
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
        options: "protocol tier candidate set clear",
        mutation: true,
    },
    Command {
        path: "model alias update",
        min: 1,
        max: 1,
        options: "protocol new-protocol tier candidate set clear",
        mutation: true,
    },
    Command {
        path: "model alias candidates add",
        min: 1,
        max: 1,
        options: "protocol candidate",
        mutation: true,
    },
    Command {
        path: "model alias candidates remove",
        min: 1,
        max: 1,
        options: "protocol candidate",
        mutation: true,
    },
    Command {
        path: "model alias rename",
        min: 2,
        max: 2,
        options: "protocol",
        mutation: true,
    },
    Command {
        path: "model alias remove",
        min: 1,
        max: 1,
        options: "protocol",
        mutation: true,
    },
    Command {
        path: "model explain",
        min: 0,
        max: 1,
        options: "protocol target agent-provider tier agent path at session state",
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
        options: "agent path",
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
        options: "provider proxy",
        mutation: true,
    },
    Command {
        path: "agent provider update",
        min: 2,
        max: 2,
        options: "provider proxy",
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
        options: "protocol",
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
        options: "protocol",
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
        options: "",
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
        path: "agent recovery discard",
        min: 1,
        max: 1,
        options: "",
        mutation: true,
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
        path: "proxy configure",
        min: 0,
        max: 0,
        options: "listen clear",
        mutation: true,
    },
    Command {
        path: "proxy remove",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "proxy key replace",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "proxy key show",
        min: 0,
        max: 0,
        options: "output-fd",
        mutation: true,
    },
    Command {
        path: "proxy status",
        min: 0,
        max: 0,
        options: "",
        mutation: false,
    },
    Command {
        path: "proxy start",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "proxy stop",
        min: 0,
        max: 0,
        options: "cancel-active",
        mutation: true,
    },
    Command {
        path: "proxy restart",
        min: 0,
        max: 0,
        options: "cancel-active",
        mutation: true,
    },
    Command {
        path: "proxy autostart enable",
        min: 0,
        max: 0,
        options: "",
        mutation: true,
    },
    Command {
        path: "proxy autostart disable",
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
];
pub fn catalog() -> Value {
    json!(COMMANDS.iter().map(|c|{let owner=if c.path.starts_with("provider "){"provider"}else if c.path.starts_with("model alias "){"alias"}else if c.path.starts_with("model "){"model"}else{""};let field_defs=super::edit::fields(owner);let metadata:serde_json::Map<String,Value>=c.options.split_whitespace().map(|name|{let fields=if name=="set"||name=="clear"{if c.path=="proxy configure"{vec!["autostart"]}else{field_defs.iter().filter(|(field,kind)|if name=="set"{*kind!="container"}else{*field!="overrides.interleaved.field"}).map(|(field,_)|*field).collect()}}else{vec![]};let choices=match name{"protocol"|"new-protocol"=>vec!["chat-completions","responses","messages"],"tier"=>vec!["L0","L1","L2","L3"],"agent"=>vec!["codex","pi","omp","opencode","hermes"],"state"=>vec!["file","loaded"],"snapshot"=>vec!["accepted","latest"],"method"=>vec!["login"],"source"=>vec!["manual","provider","codex-resets"],"kind" if c.path.starts_with("provider input")=>vec!["quota","reset"],"kind"=>vec!["concrete","alias"],"enabled"=>vec!["true","false"],"transport"=>vec!["http"],"weekday"=>vec!["mon","tue","wed","thu","fri","sat","sun"],_=>vec![]};(name.into(),json!({"arity":if matches!(name,"set"|"endpoint"){2}else{1},"choices":choices,"fields":fields,"repeatable":matches!(name,"set"|"clear"|"endpoint"|"clear-endpoint"|"value"|"weekday"|"date"|"candidate"|"model"),"switch":matches!(name,"all-day"|"last"|"proxy"|"empty-values"|"cancel-active"|"no-browser"|"key-stdin")}))}).collect();json!({"command":c.path.split_whitespace().collect::<Vec<_>>(),"operation":c.path,"operands":{"min":c.min,"max":c.max,"labels":operand_labels(c.path)},"arguments":c.options.split_whitespace().collect::<Vec<_>>(),"argument_metadata":metadata,"mutation":c.mutation,"dry_run":c.mutation})}).collect::<Vec<_>>())
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
        "model rename" => &["Qualified offering", "New exact upstream model ID"],
        p if p.starts_with("model rule ") => {
            if p.ends_with("list") || p.ends_with("clear") {
                &["Qualified offering"]
            } else {
                &["Qualified offering", "Rule ID"]
            }
        }
        "model reasoning replace"
        | "model reasoning remove"
        | "model schedule replace"
        | "model schedule remove" => &["Qualified offering", "Local index"],
        "model override list replace" => &["Qualified offering", "Metadata list field"],
        "model alias rename" => &["Existing alias ID", "New alias ID"],
        p if p.starts_with("model alias ") => &["Alias ID"],
        "model explain" => &["Public model ID (omit for tier advice)"],
        p if p.starts_with("model ") => &["Qualified offering"],
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
    if r.if_revision.is_some() && !c.mutation {
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
                .chain(["non-interactive", "unlock-stdin", "unlock-fd"])
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
        ) || (key == "model" && path.starts_with("provider input "));
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
                | "unlock-stdin"
                | "key-stdin"
                | "all-day"
                | "last"
                | "proxy"
                | "empty-values"
                | "cancel-active"
                | "no-browser"
        ) && values.as_slice() != ["true"]
        {
            return Err(Error::new(
                "invalid_arguments",
                "Boolean switches must be represented as true",
            )
            .field(key));
        }
    }
    if r.arguments.contains_key("unlock-stdin") && r.arguments.contains_key("unlock-fd") {
        return Err(Error::new(
            "invalid_arguments",
            "Unlock input choices are mutually exclusive",
        ));
    }
    if path == "provider key set" {
        let key_stdin = r.arguments.contains_key("key-stdin");
        let key_fd = r
            .arguments
            .get("key-fd")
            .and_then(|v| v.first())
            .and_then(|s| s.parse::<i32>().ok());
        let unlock_fd = if r.arguments.contains_key("unlock-stdin") {
            Some(0)
        } else {
            r.arguments
                .get("unlock-fd")
                .and_then(|v| v.first())
                .and_then(|s| s.parse::<i32>().ok())
        };
        if (key_stdin && r.arguments.contains_key("key-fd"))
            || (r.arguments.contains_key("key-ref") && (key_stdin || key_fd.is_some()))
            || ((if key_stdin { Some(0) } else { key_fd }).is_some_and(|fd| Some(fd) == unlock_fd))
        {
            return Err(Error::new(
                "invalid_arguments",
                "Key and unlock input must use distinct protected streams, and key-reference selection accepts no key input",
            ));
        }
    }
    for key in ["unlock-fd", "key-fd", "output-fd"] {
        if let Some(values) = r.arguments.get(key)
            && values[0]
                .parse::<i32>()
                .ok()
                .is_none_or(|fd| fd < 0 || (key == "output-fd" && fd < 3))
        {
            return Err(Error::new(
                "invalid_arguments",
                "Protected channel descriptor is outside its documented range",
            )
            .field(key));
        }
    }
    for key in [
        "state",
        "protocol",
        "new-protocol",
        "tier",
        "agent",
        "snapshot",
    ] {
        if let Some(values) = r.arguments.get(key) {
            let valid = match key {
                "state" => ["file", "loaded"].contains(&values[0].as_str()),
                "protocol" | "new-protocol" => {
                    crate::config::PROTOCOLS.contains(&values[0].as_str())
                }
                "tier" => ["L0", "L1", "L2", "L3"].contains(&values[0].as_str()),
                "agent" => {
                    ["codex", "pi", "omp", "opencode", "hermes"].contains(&values[0].as_str())
                }
                "snapshot" => ["accepted", "latest"].contains(&values[0].as_str()),
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_arbitrary_web_fields() {
        let request = Request {
            command: vec!["provider".into(), "add".into()],
            operands: vec!["x".into()],
            arguments: std::collections::BTreeMap::from([(
                "document".into(),
                vec!["version=1".into()],
            )]),
            dry_run: false,
            if_revision: None,
        };
        assert!(validate(&request).is_err());
    }
}
