use std::ffi::OsString;

use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command, builder::ValueParser};
use tmax_core::Request;

fn protocol_value(id: &'static str, help: &'static str) -> Arg {
    value(id, "P", help).value_parser(["chat-completions", "responses", "messages"])
}

fn agent_value(id: &'static str, help: &'static str) -> Arg {
    value(id, "AGENT", help).value_parser(["codex", "pi", "omp", "opencode", "hermes"])
}

fn weekdays(id: &'static str, help: &'static str) -> Arg {
    repeated(id, "DAY", help).value_parser(["mon", "tue", "wed", "thu", "fri", "sat", "sun"])
}

const MAX_POSITIONALS: usize = 8;
const POSITIONAL_IDS: [&str; MAX_POSITIONALS] = [
    "__operand_1",
    "__operand_2",
    "__operand_3",
    "__operand_4",
    "__operand_5",
    "__operand_6",
    "__operand_7",
    "__operand_8",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputFormat {
    Json,
    Text,
}

impl OutputFormat {
    pub fn scan_global_options(
        argv: &[OsString],
        command: &Command,
    ) -> (Self, Option<&'static str>) {
        let mut format_seen = false;
        let mut format_repeated = false;
        let mut non_interactive_seen = false;
        let mut duplicate_global = None;
        let mut selected: Option<&str> = None;
        let mut index = 1;
        while let Some(token) = argv.get(index).and_then(|arg| arg.to_str()) {
            if token == "--" {
                break;
            }
            if token == "--format" {
                if format_seen {
                    format_repeated = true;
                    duplicate_global = Some("--format");
                }
                format_seen = true;
                if let Some(value) = argv.get(index + 1).and_then(|arg| arg.to_str())
                    && (!value.starts_with('-') || value == "-")
                {
                    if !format_repeated && selected.is_none() {
                        selected = Some(value);
                    }
                    index += 2;
                    continue;
                }
                index += 1;
                continue;
            }
            if let Some(value) = token.strip_prefix("--format=") {
                if format_seen {
                    format_repeated = true;
                    duplicate_global = Some("--format");
                } else {
                    selected = Some(value);
                }
                format_seen = true;
                index += 1;
                continue;
            }
            if token == "--non-interactive" {
                if non_interactive_seen && duplicate_global.is_none() {
                    duplicate_global = Some("--non-interactive");
                }
                non_interactive_seen = true;
                index += 1;
                continue;
            }
            let Some(option) = token.strip_prefix("--") else {
                index += 1;
                continue;
            };
            let (name, attached_value) = option
                .split_once('=')
                .map_or((option, false), |(name, _)| (name, true));
            let value_count = find_long_argument(command, name)
                .map(|argument| {
                    argument
                        .get_num_args()
                        .map(|range| range.min_values())
                        .unwrap_or_else(|| match argument.get_action() {
                            ArgAction::Set | ArgAction::Append => 1,
                            _ => 0,
                        })
                })
                .unwrap_or(usize::from(attached_value));
            let mut consumed = usize::from(attached_value);
            index += 1;
            while consumed < value_count {
                let Some(value) = argv.get(index).and_then(|arg| arg.to_str()) else {
                    break;
                };
                if value.starts_with('-') && value != "-" && !is_negative_number(value) {
                    break;
                }
                consumed += 1;
                index += 1;
            }
        }
        let format = if format_repeated {
            Self::Json
        } else if selected == Some("text") {
            Self::Text
        } else {
            Self::Json
        };
        (format, duplicate_global)
    }
}

fn find_long_argument<'a>(command: &'a Command, name: &str) -> Option<&'a Arg> {
    command
        .get_arguments()
        .find(|argument| argument.get_long() == Some(name))
        .or_else(|| {
            command
                .get_subcommands()
                .find_map(|child| find_long_argument(child, name))
        })
}

fn is_negative_number(value: &str) -> bool {
    value
        .strip_prefix('-')
        .is_some_and(|digits| digits.as_bytes().first().is_some_and(u8::is_ascii_digit))
}

pub fn command() -> Command {
    Command::new("tmax")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Manage your model-provider access, coding-agent settings, and local model proxy.")
        .long_about("tmax manages provider access, subscriptions, model choices, native coding-agent configuration, and a local model proxy. It is CLI-first: compose explicit operations from offline help; no assistant, wizard, or configuration document is required. The supplementary local Web interface uses the same operations. Ordinary desired-state saves succeed after persistence and required safety work; background activation failures remain explicit warnings, not failed saves. Explicit reload, lifecycle, revocation, native and requested startup failures remain nonzero.")
        .subcommand_required(true)
        .disable_help_subcommand(true)
        .arg(global_value("format", "FORMAT", "Output format: json (default) or text.").value_parser(["json", "text"]))
        .arg(global_flag("non-interactive", "Never prompt or launch interactive sign-in. Explicit protected key input remains permitted."))
        .subcommand(help_command())
        .subcommand(status_command())
        .subcommand(config_commands())
        .subcommand(integration_commands())
        .subcommand(provider_commands())
        .subcommand(secret_commands())
        .subcommand(catalog_commands())
        .subcommand(model_commands())
        .subcommand(agent_commands())
        .subcommand(skill_commands())
        .subcommand(network_commands())
        .subcommand(proxy_commands())
        .subcommand(web_commands())
        .subcommand(history_commands())
}

fn help_command() -> Command {
    Command::new("help")
        .about("Show offline help for the root or one supported command path.")
        .long_about("Prints offline help without reading configuration, secrets, native files, service state, or remote state. Supply the exact command path from root help.")
        .arg(Arg::new("__help_path")
            .value_name("COMMAND")
            .action(ArgAction::Append)
            .num_args(0..)
            .value_parser(ValueParser::string()))
}

pub fn help_target(command: &Command, path: &[String]) -> Option<Command> {
    let mut current = command.clone();
    current.build();
    for name in path {
        let child = current
            .get_subcommands()
            .find(|child| child.get_name() == name)?
            .clone();
        current = child;
    }
    let bin_name = if path.is_empty() {
        "tmax".to_owned()
    } else {
        format!("tmax {}", path.join(" "))
    };
    Some(current.bin_name(bin_name))
}

pub fn help_path(matches: &ArgMatches) -> Vec<String> {
    matches
        .get_many::<String>("__help_path")
        .map(|values| values.cloned().collect())
        .unwrap_or_default()
}

fn family(name: &'static str, about: &'static str, command: Command) -> Command {
    debug_assert_eq!(command.get_name(), name);
    command.about(about).subcommand_required(true)
}

fn leaf(name: &'static str, about: &'static str, detail: &'static str) -> Command {
    Command::new(name)
        .about(about)
        .long_about(about)
        .after_long_help(detail)
}
fn leaf_with_fields(
    name: &'static str,
    about: &'static str,
    detail: &'static str,
    fields: &'static str,
) -> Command {
    Command::new(name)
        .about(about)
        .long_about(about)
        .after_long_help(format!("{detail}\n\n{fields}"))
}

fn global_value(id: &'static str, value: &'static str, help: &'static str) -> Arg {
    Arg::new(id)
        .long(id)
        .value_name(value)
        .help(help)
        .action(ArgAction::Append)
        .value_parser(ValueParser::string())
        .global(true)
}

fn global_flag(id: &'static str, help: &'static str) -> Arg {
    Arg::new(id)
        .long(id)
        .help(help)
        .action(ArgAction::Count)
        .global(true)
}

fn value(id: &'static str, value_name: &'static str, help: &'static str) -> Arg {
    Arg::new(id)
        .long(id)
        .value_name(value_name)
        .help(help)
        .action(ArgAction::Set)
        .value_parser(ValueParser::string())
}
fn signed_value(id: &'static str, value_name: &'static str, help: &'static str) -> Arg {
    value(id, value_name, help).allow_negative_numbers(true)
}

fn repeated(id: &'static str, value_name: &'static str, help: &'static str) -> Arg {
    Arg::new(id)
        .long(id)
        .value_name(value_name)
        .help(help)
        .action(ArgAction::Append)
        .value_parser(ValueParser::string())
}

fn pair(id: &'static str, first: &'static str, second: &'static str, help: &'static str) -> Arg {
    Arg::new(id)
        .long(id)
        .value_names([first, second])
        .help(help)
        .action(ArgAction::Append)
        .num_args(2)
        .value_parser(ValueParser::string())
}
fn signed_pair(
    id: &'static str,
    first: &'static str,
    second: &'static str,
    help: &'static str,
) -> Arg {
    pair(id, first, second, help).allow_negative_numbers(true)
}

fn flag(id: &'static str, help: &'static str) -> Arg {
    Arg::new(id).long(id).help(help).action(ArgAction::SetTrue)
}

fn positional(index: usize, label: &'static str, help: &'static str) -> Arg {
    Arg::new(POSITIONAL_IDS[index - 1])
        .index(index)
        .value_name(label)
        .help(help)
        .action(ArgAction::Set)
        .value_parser(ValueParser::string())
}

fn required_pos(index: usize, label: &'static str, help: &'static str) -> Arg {
    positional(index, label, help).required(true)
}

fn mutation(command: Command, dry_run: bool, file_guard: bool) -> Command {
    let mut command = command;
    if dry_run {
        command = command.arg(flag(
            "dry-run",
            "Describe the full redacted operation without performing effects.",
        ));
    }
    if file_guard {
        command = command.arg(value(
            "if-revision",
            "REVISION",
            "Require this exact current configuration-file revision (or absent).",
        ));
    }
    command
}

fn edit_args(command: Command, fields: &'static str) -> Command {
    command
        .arg(signed_pair(
            "set",
            "FIELD",
            "VALUE",
            "Set an exact supported scalar field; repeat for distinct fields.",
        ))
        .arg(
            repeated(
                "clear",
                "FIELD",
                "Clear an exact supported optional field; repeat for distinct fields.",
            )
            .long_help(fields),
        )
}

fn state_arg(command: Command) -> Command {
    command.arg(
        value("state", "STATE", "Select file (default) or loaded state.")
            .value_parser(["file", "loaded"]),
    )
}

fn status_command() -> Command {
    leaf(
        "status",
        "Summarize configuration, resident service, readiness, recovery, and startup state.",
        "Effect-free retained-state read. It does not resolve secrets, refresh remote state, sign in, or start a service. Use config show/validate, provider inspect, agent inspect, proxy status, or web status for detail.\n\nExit: 0 when the status request completed, including unhealthy or unknown state.\nExample: tmax status",
    )
}

fn config_commands() -> Command {
    let reload = mutation(
        leaf(
            "reload",
            "Validate and activate the complete current saved file.",
            "Does not rewrite the file, apply native settings, sign in, or start listeners. A running service activates the whole valid file, including pending manual edits. Invalid activation retains the previous active snapshot.\n\nOptions: --if-revision REVISION guards the saved file; --dry-run reports full redacted activation impact without activation.\nExit: 0 when validation/reload completes; 3 when current action is required; 4 for failed activation.\nExample: tmax config reload --dry-run",
        ),
        true,
        true,
    );
    let reset = mutation(
        leaf(
            "reset",
            "Preserve and replace an existing invalid configuration with empty valid settings.",
            "Only an existing invalid file can be reset. Save the exact rejected bytes in current-user-restricted ~/.tmax/recovery before replacement; output returns an opaque record handle, never content. The artifact is plaintext, not encrypted. Does not delete secrets, sign out, touch native files, discard earlier recovery records, or replay rejected content.\n\nOptions: --if-revision REVISION guards the current file; --dry-run reports the full impact without creating a recovery artifact.\nExit: 0 on completed recovery/reset; 3 when safe preservation or current identity is unavailable; 4 for partial recovery/save/activation effects.\nExample: tmax config reset --dry-run",
        ),
        true,
        true,
    );
    let recovery = family("recovery", "Inspect or discard retained invalid-file recovery records.", Command::new("recovery")
        .subcommand(leaf("list", "List safe metadata for retained invalid-file recovery records.", "Works with absent or invalid configuration. Never displays rejected bytes.\n\nExit: 0 when the inventory read completes.\nExample: tmax config recovery list"))
        .subcommand(leaf("inspect", "Inspect safe metadata for one recovery record.", "Shows file identity, original revision, byte count, capture time, rejection category, ownership/file permissions, and discard support. Never displays rejected content.\n\nExit: 0 when metadata is available; unavailable evidence remains explicit.\nExample: tmax config recovery inspect RECORD")
            .arg(required_pos(1, "RECORD", "Returned recovery handle.")))
        .subcommand(mutation(leaf("discard", "Discard one exact owned invalid-file recovery record.", "Deletes only the selected recovery artifact after current ownership/concurrency checks. Does not change settings, credentials, native recovery, or bindings. Records have no expiry and no restore/replay operation.\n\nExit: 0 when discarded; 3 when current safety checks block; 4 for I/O or uncertain deletion.\nExample: tmax config recovery discard RECORD --dry-run"), true, false)
            .arg(required_pos(1, "RECORD", "Returned recovery handle."))));
    family("config", "Inspect, validate, reload, and recover desired configuration.", Command::new("config")
        .subcommand(leaf("show", "Show redacted saved or loaded configuration.", "Use --state file (default) or loaded. A missing loaded snapshot is explicit, not an empty collection. Reads do not repair, activate, or create configuration.\n\nExit: 0 for available, absent, or explicitly unavailable state.\nExample: tmax config show --state loaded")
            .arg(value("state", "STATE", "file (default) or loaded.").value_parser(["file", "loaded"])))
        .subcommand(leaf("validate", "Validate the current saved configuration without changing it.", "Distinguishes syntax, type, and reference errors from external availability. Missing configuration remains absent; no starter file is created.\n\nExit: 0 when validation/read completes; 2 for invalid saved configuration.\nExample: tmax config validate"))
        .subcommand(reload)
        .subcommand(reset)
        .subcommand(recovery))
}

fn integration_commands() -> Command {
    Command::new("integration")
        .about("Discover supported provider integrations and their qualified capabilities.")
        .subcommand(leaf("list", "List locally supported provider integration IDs and setup summaries.", "Reads the built-in qualified registry only. Does not require a catalog, network, credentials, providers, targets, or valid configuration. The result includes protocol, endpoint setup, and authentication summaries.\n\nExit: 0 when registry discovery completes.\nExample: tmax integration list"))
        .subcommand(leaf("inspect", "Inspect one supported provider integration and its exact capabilities.", "Reports regions/protocol bases, finite operations, protocols/transports, auth methods/login sources, input source/kind/scope/model coverage, noninteractive acquisition, restrictions, and known/unknown qualification. It does not probe an endpoint.\n\nExit: 0 when the registry entry is found; 2 for an unknown integration ID.\nExample: tmax integration inspect deepseek")
            .arg(required_pos(1, "ID", "Stable integration ID from integration list.")))
        .subcommand_required(true)
}

fn skill_commands() -> Command {
    Command::new("skill")
        .about("Install the bundled tmax entry skill for your calling agent.")
        .subcommand(mutation(
            leaf("install", "Install the bundled tmax skill without replacing user content.",
                concat!(
                    "Installs to the OS user's ~/.agents/skills/tmax/SKILL.md by default. --agent hermes selects HERMES_HOME/skills/tmax/SKILL.md. Without HERMES_HOME, Hermes defaults to ~/.hermes on Unix or LOCALAPPDATA/hermes (~/AppData/Local/hermes fallback) on Windows, with HERMES_DATA_DIR_SUFFIX appended to the directory name. ",
                    "HERMES_HOME accepts a literal absolute or cwd-relative path and a leading ~/; expand environment-variable expressions before invoking tmax. Parent traversal and ~other-user forms are rejected. No Hermes profile is inferred.\n\n",
                    "Identical content succeeds unchanged. Different existing content is preserved and blocks installation; move it aside explicitly before trying again. Symlinks and unsafe paths are rejected. --dry-run reports the resolved destination and whether installation is needed without writing. No configuration, credentials, service, agent process, or network access is required. The skill directs agents to offline CLI help rather than duplicating commands.\n\n",
                    "Existing-path owner and directory-write checks apply on Unix. Windows new entries receive owner-only ACLs; pre-existing paths use OS access controls and reparse checks, without owner attestation.\n\n",
                    "Output: skill, agent (agents or hermes), path, state (installed, already_installed, or would_install), and changed.\nExit: 0 when installed, identical, or dry-run ready; 2 for invalid arguments; 3 for conflicting content; 4 for unsafe or unavailable storage.\nExample: tmax skill install --agent hermes"))
                .arg(value("agent", "AGENT", "Install into Hermes skills instead of ~/.agents/skills.").value_parser(["hermes"])),
            true,
            false,
        ))
        .subcommand_required(true)
}

fn provider_commands() -> Command {
    let add_fields = concat!(
        "Supported --set/--clear scalar FIELD paths:\n",
        "`metadata`, `integration`, `region`: strings; clearing removes the selection, subject to valid integration/endpoint mapping. Metadata changes are not renames.\n",
        "`enabled`: boolean; clear restores true. `overrides.name`, `overrides.doc`: strings; clear restores inheritance.\n",
        "`subscription.plan`, `subscription.billing_period`: strings; `subscription.renewal_date`, `subscription.expiry_date`: YYYY-MM-DD dates; clear makes each manual fact unknown.\n",
        "`subscription.auto_renew`: boolean; clear means unknown, not false.\n",
        "`subscription.charge.amount`: nonnegative decimal string; `subscription.charge.currency`: uppercase three-letter currency string. Set or clear the pair together.\n",
        "Clear also accepts `endpoints`, `overrides`, `subscription`, and `subscription.charge`; endpoint tables use dedicated `--endpoint P URL`/`--clear-endpoint P` options, never scalar blobs.",
    );
    let add = mutation(
        edit_args(
            leaf_with_fields(
                "add",
                "Create one independently managed provider instance.",
                "Select catalog metadata or an explicit supported integration. Provider creation may remain incomplete and unauthenticated. --metadata, --integration, --region and repeated --endpoint are explicit selectors; do not set the same field through --set.\n\nExit: 0 when saved; 2 for invalid schema/duplicate ID; 3 for action-required qualification; 4 for persistence or required safety/startup failure.\nExample: tmax provider add custom-gateway --integration custom --endpoint chat-completions https://gateway.example/openai/v1",
                add_fields,
            ),
            add_fields,
        )
        .arg(value("metadata", "CATALOG-ID", "Select catalog provider metadata."))
        .arg(value("integration", "INTEGRATION", "Select an exact supported integration ID."))
        .arg(value("region", "REGION", "Select a region advertised by the integration."))
        .arg(pair("endpoint", "P", "URL", "Set one protocol endpoint; repeat for distinct protocols."))
        .arg(repeated("clear-endpoint", "P", "Remove one protocol endpoint; repeat for distinct protocols.").value_parser(["chat-completions", "responses", "messages"]))
        .arg(required_pos(1, "ID", "Unique local provider ID.")),
        true,
        true,
    );
    let update = mutation(
        edit_args(
            leaf_with_fields(
                "update",
                "Edit selected provider settings without replacing unrelated fields.",
                "At least one change is required. --endpoint P URL replaces exactly one protocol base; --clear-endpoint P removes only that base. Region, endpoint, integration, and scalar edits are validated together.\n\nExit: 0 when saved; 2 for invalid schema; 4 for persistence or required safety/startup failure.",
                add_fields,
            ),
            add_fields,
        )
        .arg(value("region", "REGION", "Set an exact supported region."))
        .arg(pair("endpoint", "P", "URL", "Set one protocol endpoint; repeat for distinct protocols."))
        .arg(repeated("clear-endpoint", "P", "Remove one protocol endpoint.").value_parser(["chat-completions", "responses", "messages"]))
        .arg(required_pos(1, "ID", "Local provider ID.")),
        true,
        true,
    );
    let auth = Command::new("auth")
        .about("Set or clear provider login authentication declaratively.")
        .subcommand(mutation(leaf("set", "Replace provider authentication with explicit login-source configuration.", "Requires --method login and explicit --via SOURCE. This is declarative configuration only: it does not sign in or import native credentials. Omitted --session-ref removes the previous reference.\n\nExit: 0 when saved; 2 for invalid source/method; 3 for qualified action-required blockers; 4 for persistence or required safety/startup failure.\nExample: tmax provider auth set deepseek --method login --via pi-main")
            .arg(value("method", "METHOD", "Currently login; must be explicit.").value_parser(["login"]).required(true))
            .arg(value("via", "SOURCE", "Qualified login source: tmax or a declared target.").required(true))
            .arg(value("session-ref", "REF", "Optional opaque source-scoped session reference."))
            .arg(required_pos(1, "ID", "Local provider ID.")), true, true))
        .subcommand(mutation(leaf("clear", "Remove active provider authentication configuration.", "Does not delete stored keys, sign out native sessions, revoke upstream access, or remove subscription declarations.\n\nExit: 0 when saved; 3 for reference blockers; 4 for persistence or required safety/startup failure.\nExample: tmax provider auth clear deepseek")
            .arg(required_pos(1, "ID", "Local provider ID.")), true, true));
    let key = Command::new("key")
        .about("Configure provider API-key authentication through key input or a qualified reference.")
        .subcommand(mutation(leaf("set", "Set or replace provider API-key authentication.", "Supply either --key-stdin, --key-ref REF, or hidden terminal input. --key-stdin reads one nonempty UTF-8 line without echo. Noninteractive entry requires explicit --key-stdin; never consume a pipe implicitly. Do not supply keys in argv, environment variables, or ordinary configuration files; never print or log them.\n\nDry-run reads no stream, prompts, or references. A key replacement removes prior auth fields but retains old stored secrets.\nExit: 0 when credential and requested save complete; 3 when required key input or interaction is unavailable; 4 for credential, persistence, or required safety/startup failure.\nExample: tmax provider key set deepseek --key-stdin --non-interactive")
            .arg(flag("key-stdin", "Read one key line explicitly from stdin.").conflicts_with("key-ref"))
            .arg(value("key-ref", "REF", "Select a qualified provider-owned secret reference without resolving it.").conflicts_with("key-stdin"))
            .arg(required_pos(1, "ID", "Local provider ID.")), true, true));
    let login = mutation(leaf("login", "Sign in through a qualified provider login owner and then save its reference.", "The selected source owns the interactive sign-in, storage, and refresh. Without --via, source selection follows the product login-source flow; ambiguity blocks before effects. --non-interactive never launches sign-in. Native effects that precede failed local save/identity validation are reported as partial/uncertain; do not assume old authorization survived.\n\nDry-run selects/discloses source and effects without sign-in, input, or local/native changes.\nExit: 0 on successful completion; 3 before effects when interaction/source is required; 4 after changed or uncertain effects; 5 only for cancellation known before any effect.\nExample: tmax provider login deepseek --via pi-main")
        .arg(value("via", "SOURCE", "Explicit supported source: tmax or a declared target."))
        .arg(required_pos(1, "ID", "Local provider ID.")), true, true);
    let logout = mutation(leaf("logout", "Sign out through the provider's configured login owner.", "Preserves provider/subscription declarations and identity-comparison basis. Reports affected native/shared consumers. Does not remove stored keys or provider settings.\n\nExit: 0 when the requested sign-out completes; 3 for required interaction/source; 4 for partial or uncertain native effects.\nExample: tmax provider logout deepseek")
        .arg(required_pos(1, "ID", "Local provider ID.")), true, true);
    let refresh = mutation(leaf("refresh", "Run explicitly requested qualified auth, model, and/or quota checks.", "SCOPE is auth, models, quota, or all. Model discovery registers nothing. Refresh is not sign-in, inference, catalog apply, or desired-state edit. Manual inputs and Codex reset announcements use provider input refresh instead. Reports each check/channel separately; failures do not mean zero quota, revocation, or generic routing exclusion.\n\nExit: 0 when all requested checks complete or dry-run succeeds; 3 for known qualification/credential blockers before checks; 4 when a requested check fails or is unavailable.\nExample: tmax provider refresh deepseek --scope quota")
        .arg(value("scope", "SCOPE", "auth, models, quota, or all.").value_parser(["auth", "models", "quota", "all"]).required(true))
        .arg(required_pos(1, "ID", "Local provider ID.")), true, false);
    let input = provider_input_commands();
    family("provider", "Create, inspect, authenticate, refresh, and remove provider instances.", Command::new("provider")
        .subcommand(leaf("list", "List provider IDs, names, enabled/authentication state, quota freshness, and affected-target counts.", "Compact retained-state read. It does not resolve secrets, refresh remotely, or verify upstream authorization.\n\nExit: 0 when the read completes.\nExample: tmax provider list"))
        .subcommand(leaf("inspect", "Inspect a provider's configuration, inherited/effective metadata, access, inputs, and references.", "File and loaded projections, endpoint resolution, auth source/known-or-unknown identity, models, subscription/balance facts, input readiness, retained secret references, and file/loaded differences are distinct. No secret resolution or remote refresh.\n\nExit: 0 when inspection completes; unknown IDs are invalid input.\nExample: tmax provider inspect deepseek")
            .arg(required_pos(1, "ID", "Local provider ID.")))
        .subcommand(refresh)
        .subcommand(add)
        .subcommand(update)
        .subcommand(mutation(leaf("rename", "Rename a provider ID and migrate all dependent qualified references in one change.", "Reject collisions and preserve provider identity. No compatibility alias, automatic session retarget, or remove/add inference.\n\nExit: 0 when saved; 2 for collision/reference errors; 4 for persistence or required safety/startup failure.\nExample: tmax provider rename deepseek deepseek-main")
            .arg(required_pos(1, "OLD", "Current local provider ID."))
            .arg(required_pos(2, "NEW", "New unique local provider ID.")), true, true))
        .subcommand(mutation(leaf("remove", "Remove one provider after explicit reference resolution.", "Rejects unresolved provider/model/alias/login-source references and reports them; no cascade, replacement choice, native-file deletion, secret cleanup, sign-out, or subscription cancellation. Reports retained secrets, affected targets, and direct consumers that may continue outside tmax.\n\nExit: 0 when saved; 2 for unresolved references; 4 for persistence or required safety/startup failure.\nExample: tmax provider remove deepseek")
            .arg(required_pos(1, "ID", "Local provider ID.")), true, true))
        .subcommand(key)
        .subcommand(auth)
        .subcommand(login)
        .subcommand(logout)
        .subcommand(input)
        .subcommand_required(true))
}

fn provider_input_commands() -> Command {
    let definition_help = "Required declaration options: --kind quota|reset --source manual|provider|codex-resets --scope SCOPE --model MODEL-ID (repeat; exact upstream IDs registered under this provider) --max-age DURATION. Optional --refresh-every DURATION is for nonmanual sources only. DURATION is positive integer plus one lowercase unit s/m/h/d. Replacement requires all required options again.";
    let add = mutation(leaf_with_fields("add", "Declare one provider-scoped quota or reset input channel.", "Covered models must already be registered. Input names are immutable and unique within a provider. Structurally valid but unqualified combinations remain visibly unavailable; no source is guessed.", definition_help)
        .arg(value("kind", "KIND", "quota or reset.").value_parser(["quota", "reset"]).required(true))
        .arg(value("source", "SOURCE", "manual, provider, or codex-resets.").value_parser(["manual", "provider", "codex-resets"]).required(true))
        .arg(value("scope", "SCOPE", "One nonempty provider-input scope; not a native path.").required(true))
        .arg(repeated("model", "MODEL-ID", "Exact registered upstream model ID; repeat for covered models.").required(true))
        .arg(value("max-age", "DURATION", "Maximum data age, e.g. 30m.").required(true))
        .arg(value("refresh-every", "DURATION", "Optional automatic refresh interval; nonmanual sources only."))
        .arg(required_pos(1, "PROVIDER", "Declared provider ID."))
        .arg(required_pos(2, "INPUT", "New exact immutable input name.")), true, true);
    let replace = mutation(leaf_with_fields("replace", "Replace a complete provider-input declaration under its immutable name.", "All required definition options must be supplied again. Omitted optional refresh interval is removed. An unchanged replacement does not refresh data or extend freshness.", definition_help)
        .arg(value("kind", "KIND", "quota or reset.").value_parser(["quota", "reset"]).required(true))
        .arg(value("source", "SOURCE", "manual, provider, or codex-resets.").value_parser(["manual", "provider", "codex-resets"]).required(true))
        .arg(value("scope", "SCOPE", "One nonempty provider-input scope.").required(true))
        .arg(repeated("model", "MODEL-ID", "Exact registered upstream model ID; repeat for covered models.").required(true))
        .arg(value("max-age", "DURATION", "Maximum data age.").required(true))
        .arg(value("refresh-every", "DURATION", "Optional nonmanual automatic refresh interval."))
        .arg(required_pos(1, "PROVIDER", "Declared provider ID."))
        .arg(required_pos(2, "INPUT", "Exact immutable input name.")), true, true);
    let record = mutation(leaf_with_fields("record", "Publish one manual quota observation or reset announcement.", "INSTANT values require RFC 3339 explicit offset. Quota requires exactly one representation: --remaining-percent DECIMAL in 0–100, or --remaining DECIMAL --total DECIMAL --unit UNIT with 0 <= remaining <= total and total > 0. Reset rejects quota fields. Decimals are exact nonnegative decimal strings. Optional --reference is HTTPS provenance only. --if-input-revision guards current input data; identical records do not extend freshness.", "Exit: 0 when published/unchanged; 2 for invalid data; 3 for revision conflict; 4 for persistence failure. Example: tmax provider input record opencode-go weekly-allowance --observed-at 2026-10-23T09:55:00Z --valid-until 2026-10-23T10:55:00Z --reset-at 2026-10-23T12:00:00Z --remaining-percent 75.0")
        .arg(value("observed-at", "INSTANT", "Observation instant with explicit RFC 3339 offset.").required(true))
        .arg(value("valid-until", "INSTANT", "Validity end with explicit RFC 3339 offset.").required(true))
        .arg(value("reset-at", "INSTANT", "Provider-announced reset instant with explicit offset.").required(true))
        .arg(value("remaining-percent", "DECIMAL", "Quota percentage, inclusive 0–100."))
        .arg(value("remaining", "DECIMAL", "Exact remaining quota counter."))
        .arg(value("total", "DECIMAL", "Exact total quota counter."))
        .arg(value("unit", "UNIT", "Shared documented counter unit/window."))
        .arg(value("reference", "URL", "Optional safe absolute HTTPS provenance."))
        .arg(value("if-input-revision", "REVISION", "Require this current input-data revision or absent."))
        .arg(required_pos(1, "PROVIDER", "Declared provider ID."))
        .arg(required_pos(2, "INPUT", "Exact input name.")), true, false);
    let clear = mutation(leaf("clear", "Publish an unavailable revision for one input channel.", "Preserves the declaration and rules. Prevents a late fetch from resurrecting older data. A separately started later refresh may publish. --if-input-revision checks the current data revision.\n\nExit: 0 when cleared; 3 for revision conflict; 4 for persistence failure.\nExample: tmax provider input clear opencode-go weekly-allowance --if-input-revision INPUT-REVISION")
        .arg(value("if-input-revision", "REVISION", "Require this current input-data revision or absent."))
        .arg(required_pos(1, "PROVIDER", "Declared provider ID."))
        .arg(required_pos(2, "INPUT", "Exact input name.")), true, false);
    family("input", "Manage provider-scoped quota and reset-announcement data.", Command::new("input")
        .subcommand(state_arg(leaf("list", "List a provider's input channels and freshness/acquisition summaries.", "Name-ordered compact summaries; selected state is file (default) or loaded. A missing loaded snapshot is not an empty list. Read does not refresh.\n\nExit: 0 even for stale/unavailable data.\nExample: tmax provider input list opencode-go")
            .arg(required_pos(1, "PROVIDER", "Declared provider ID."))))
        .subcommand(state_arg(leaf("inspect", "Inspect one input declaration, current data, attempt, dependencies, and use.", "Current data, last attempt, saved/runtime use, acquisition blockers, and revisions are distinct. No refresh, credential resolution, or inference.\n\nExit: 0 even for stale/unavailable data.\nExample: tmax provider input inspect opencode-go weekly-allowance")
            .arg(required_pos(1, "PROVIDER", "Declared provider ID."))
            .arg(required_pos(2, "INPUT", "Exact input name."))))
        .subcommand(add)
        .subcommand(replace)
        .subcommand(mutation(leaf("remove", "Remove one input declaration after rule-reference checks.", "Does not delete rules, fetch data, or change source credentials; remove dependent rules first.\n\nExit: 0 when saved; 2 for unresolved references; 4 for persistence or required safety/startup failure.\nExample: tmax provider input remove opencode-go weekly-allowance")
            .arg(required_pos(1, "PROVIDER", "Declared provider ID."))
            .arg(required_pos(2, "INPUT", "Exact input name.")), true, true))
        .subcommand(mutation(leaf("refresh", "Acquire and conditionally publish one declared nonmanual input.", "Uses the saved declaration. Does not edit/activate configuration or start services. Refresh is nonmanual-only; record is manual-only. Dry-run makes no network call, consumes no input, and creates no attempt/revision. Automatic acquisition is opt-in and only runs within an already-running service.\n\nExit: 0 for successful publication/withdrawal; 3 for qualification/credential/in-flight blockers; 4 for transport, parse, or persistence failure.\nExample: tmax provider input refresh opencode-go reset-announcement --dry-run")
            .arg(required_pos(1, "PROVIDER", "Declared provider ID."))
            .arg(required_pos(2, "INPUT", "Exact input name.")), true, false))
        .subcommand(record)
        .subcommand(clear)
        .subcommand_required(true))
}

fn secret_commands() -> Command {
    Command::new("secret")
        .about("Inspect retained secret references and explicitly clean up an eligible tmax-owned secret.")
        .subcommand(leaf("list", "List safe local secret ownership, reference, and cleanup metadata.", "Works with absent/invalid config and does not read or expose secret contents. Incomplete inventory coverage is not an empty inventory. Unknown reference safety blocks cleanup.\n\nExit: 0 when safe inventory discovery completes, including partial/unavailable coverage.\nExample: tmax secret list"))
        .subcommand(mutation(leaf("cleanup", "Delete one exact retained tmax-owned local secret after safety checks.", "Rechecks ownership and desired/loaded/known-native references. Unknown safety blocks deletion. Does not delete native-owned sessions or revoke upstream access/copies.\n\nExit: 0 when deleted; 3 when blocked; 4 for failed or uncertain deletion.\nExample: tmax secret cleanup REF --dry-run")
            .arg(required_pos(1, "REF", "Exact qualified secret reference.")), true, false))
        .subcommand_required(true)
}

fn catalog_commands() -> Command {
    let snapshot = |command: Command| {
        command.arg(
            value(
                "snapshot",
                "SNAPSHOT",
                "accepted (default) or latest validated download.",
            )
            .value_parser(["accepted", "latest"]),
        )
    };
    Command::new("catalog")
        .about("Inspect local catalog snapshots, explicitly refresh metadata, and apply one validated download.")
        .subcommand(snapshot(leaf("providers", "List selectable providers from a retained catalog snapshot.", "Identifies snapshot/source/retrieval time/staleness and compact qualified integration/authentication/endpoint setup choices. Latest is a retained validated download, not a live fetch.\n\nExit: 0 for available/absent snapshot results.\nExample: tmax catalog providers")))
        .subcommand(snapshot(leaf("models", "List model metadata for one catalog provider without registering it.", "Catalog model IDs are not local provider or qualified model IDs. This operation does not modify configuration.\n\nExit: 0 when read completes; 2 for unknown catalog ID.\nExample: tmax catalog models deepseek")
            .arg(required_pos(1, "CATALOG-ID", "Exact catalog-provider ID."))))
        .subcommand(snapshot(leaf("inspect", "Inspect catalog metadata and qualified setup choices for one provider/model.", "Shows provenance, qualified endpoint/region setup choices, and the first-unused local ID suggestion. Suggestions are not reserved; provider creation must name its local ID. Catalog API fields do not qualify endpoint behavior.\n\nExit: 0 when details are available; 2 for unknown catalog ID/model.\nExample: tmax catalog inspect deepseek --model deepseek-flash")
            .arg(value("model", "MODEL-ID", "Optional exact catalog model ID."))
            .arg(required_pos(1, "CATALOG-ID", "Exact catalog-provider ID."))))
        .subcommand(mutation(leaf("refresh", "Download and validate catalog metadata without credentials.", "Preserves accepted metadata; does not apply or activate a downloaded snapshot. Dry-run describes the operation without fetching or writing.\n\nExit: 0 for a successful validated download; 3 for action-required network policy; 4 for download/validation failure.\nExample: tmax catalog refresh --dry-run"), true, false))
        .subcommand(mutation(leaf("apply", "Apply exactly one retained validated catalog revision.", "--revision selects downloaded catalog data, not a file revision or operation reservation. Never refreshes implicitly. Reports affected providers/models/targets and saved/loaded activation impact.\n\nExit: 0 when applied; 3 for required current action; 4 for persistence or required safety/startup failure.\nExample: tmax catalog apply --revision CATALOG-REVISION --dry-run")
            .arg(value("revision", "REVISION", "Exact retained downloaded catalog revision.")), true, true))
        .subcommand_required(true)
}

fn model_commands() -> Command {
    let model_fields = concat!(
        "Supported scalar model FIELD paths and values:\n",
        "`enabled`: boolean, clear restores true; `priority`: signed integer, clear restores zero; `timezone`: IANA-zone string, clear only when no schedule/rule time gate requires it.\n",
        "`overrides.name`, `.description`, `.family`, `.knowledge`, `.release_date`, `.last_updated`: strings; date-like metadata remains a string. Clear restores inheritance.\n",
        "`overrides.attachment`, `.reasoning`, `.tool_call`, `.structured_output`, `.temperature`, `.open_weights`: booleans; clear restores inheritance.\n",
        "`overrides.limit.context`, `.input`, `.output`: nonnegative integers. `overrides.cost.input`, `.output`, `.reasoning`, `.cache_read`, `.cache_write`, `.cache_storage`: nonnegative finite numbers in catalog units. Clear restores each inherited fact.\n",
        "`overrides.interleaved`: boolean representation; `.field`: nonempty string in table representation. Switch boolean to table atomically with `--clear overrides.interleaved --set overrides.interleaved.field VALUE`; clear the parent to inherit.\n",
        "Clear also accepts `overrides`, `overrides.limit`, `overrides.modalities`, `overrides.modalities.input`, `overrides.modalities.output`, `overrides.cost`, `overrides.reasoning_options`, and `overrides.interleaved`. Structured/list metadata uses dedicated operations, never serialized blobs.",
    );
    let reasoning = Command::new("reasoning")
        .about("Manage local reasoning-option metadata arrays.")
        .subcommand(mutation(leaf("add", "Append one local reasoning-option table.", "Requires --type STRING. Repeat --value STRING in order, or use --empty-values for an explicit empty values array. An absent override starts empty, not from catalog metadata.\n\nExit: 0 when saved; 2 for invalid metadata; 4 for persistence or required safety/startup failure.\nExample: tmax model reasoning add deepseek/deepseek-flash --type effort --empty-values")
            .arg(value("type", "STRING", "Required reasoning option type."))
            .arg(repeated("value", "STRING", "Optional value; repeat in order."))
            .arg(flag("empty-values", "Write an explicit empty values array; conflicts with --value."))
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name.")), true, true))
        .subcommand(mutation(leaf("replace", "Replace one complete local reasoning-option table at its guarded file revision.", "Requires --if-revision REVISION and --type STRING. Omitted --value removes optional values; --empty-values writes values=[]. Missing index is an error.\n\nExit: 0 when saved; 2 for invalid metadata/index; 3 for revision conflict; 4 for persistence or required safety/startup failure.\nExample: tmax model reasoning replace deepseek/deepseek-flash 0 --if-revision REVISION --type effort")
            .arg(value("if-revision", "REVISION", "Required exact current file revision.").required(true))
            .arg(value("type", "STRING", "Required reasoning option type."))
            .arg(repeated("value", "STRING", "Optional values in order."))
            .arg(flag("empty-values", "Write an explicit empty values array; conflicts with --value."))
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name."))
            .arg(required_pos(2, "INDEX", "Zero-based local array position.")), true, false))
        .subcommand(mutation(leaf("remove", "Remove exactly one reasoning-option table at its guarded file revision.", "Removing the final entry leaves an explicit empty local array; clear the parent override to restore inheritance.\n\nExit: 0 when saved; 2 for invalid index; 3 for revision conflict; 4 for persistence or required safety/startup failure.\nExample: tmax model reasoning remove deepseek/deepseek-flash 0 --if-revision REVISION")
            .arg(value("if-revision", "REVISION", "Required exact current file revision.").required(true))
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name."))
            .arg(required_pos(2, "INDEX", "Zero-based local array position.")), true, false))
        .subcommand(mutation(leaf("clear", "Write an explicit empty local reasoning-options array.", "This differs from clearing overrides.reasoning_options, which restores catalog inheritance.\n\nExit: 0 when saved; 4 for persistence or required safety/startup failure.\nExample: tmax model reasoning clear deepseek/deepseek-flash")
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name.")), true, true));
    let schedule = Command::new("schedule")
        .about("Manage calendar priority schedules on concrete offerings.")
        .subcommand(schedule_rule("add", false))
        .subcommand(schedule_rule("replace", true))
        .subcommand(mutation(leaf("remove", "Remove one schedule entry at its required file revision.", "INDEX is a zero-based local position; guard is required and must match the current file. A later write requires a fresh revision.\n\nExit: 0 when saved; 2 for invalid index; 3 for revision conflict; 4 for persistence or required safety/startup failure.\nExample: tmax model schedule remove deepseek/deepseek-flash 0 --if-revision REVISION")
            .arg(value("if-revision", "REVISION", "Required exact current file revision.").required(true))
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name."))
            .arg(required_pos(2, "INDEX", "Zero-based schedule position.")), true, false))
        .subcommand(mutation(leaf("clear", "Remove the offering's schedule, not its base priority.", "An empty schedule permits clearing the timezone when no other time gate needs it.\n\nExit: 0 when saved; 4 for persistence or required safety/startup failure.\nExample: tmax model schedule clear deepseek/deepseek-flash")
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name.")), true, true));
    let rule = model_rule_commands();
    let alias = model_alias_commands();
    let explain = leaf("explain", "Explain no-inference model selection or one retained session destination.", "Requires a model ID and explicit --protocol. Use a public qualified concrete or alias ID, except with an upstream-source native entry: its ID is the exact upstream model ID under that entry's provider, including any slashes. Optional --at uses retained facts valid then; it never fetches or predicts. Assess native compatibility with --target TARGET, optionally --agent-provider NAME for an existing entry, or --agent AGENT --path PATH for an undeclared scope. Every context assesses only the requested offering or explicit alias candidates. --session inspects an existing proxy session and cannot use an upstream-source entry.\n\nReturns candidates/exclusions, rules, priorities, selection provenance, compatibility and unknown evidence. Does not write settings/native files, resolve secrets, refresh login, start inference, reserve a session, or probe. Rejects --dry-run.\n\nExit: 0 when explanation completes, including blocked/unknown availability; 2 for invalid operand combinations.\nExample: tmax model explain L0 --protocol chat-completions")
        .arg(protocol_value("protocol", "chat-completions, responses, or messages.").required(true))
        .arg(value("target", "TARGET", "Declared target for native assessment.").conflicts_with_all(["agent", "path"]))
        .arg(value("agent-provider", "NAME", "Existing target-local provider entry.").requires("target"))
        .arg(agent_value("agent", "Agent for undeclared native scope.").requires("path"))
        .arg(value("path", "PATH", "Native scope path for undeclared context.").requires("agent"))
        .arg(value("at", "INSTANT", "RFC 3339 explicit-offset evaluation instant."))
        .arg(value("session", "SESSION", "Existing retained proxy session handle."))
        .arg(value("state", "STATE", "file (default) or loaded.").value_parser(["file", "loaded"]))
        .arg(required_pos(1, "ID", "Public concrete/alias ID, or exact upstream ID with an upstream-source native entry."));
    let probe = mutation(leaf("probe", "Run one explicitly requested bounded potentially paid inference check.", "Requires a concrete QUALIFIED offering, protocol, and an integration-qualified transport. A probe may use paid quota/credit; inspect dry-run/capability help for known price and unknown charges. No alias fan-out, retries, or replay. This is not a non-inference health check.\n\nDry-run contacts no upstream and reads no credentials.\nExit: 0 when the requested probe completes; 3 when qualification/credential is required; 4 for request/transport/upstream failure.\nExample: tmax model probe deepseek/deepseek-flash --protocol chat-completions --transport http --dry-run")
        .arg(protocol_value("protocol", "chat-completions, responses, or messages.").required(true))
        .arg(value("transport", "TRANSPORT", "Exact integration-qualified transport; see integration inspect.").required(true))
        .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name, not an alias.")), true, false);
    let override_commands = Command::new("override")
        .about("Manage structured model metadata overrides.")
        .subcommand(Command::new("list")
            .about("Replace one input or output modality list.")
            .subcommand(mutation(leaf("replace", "Replace one local input/output modality list.", "FIELD is exactly overrides.modalities.input or overrides.modalities.output. Repeat --value STRING in order. Omitting --value writes an explicit empty list; model update --clear restores inheritance.\n\nExit: 0 when saved; 2 for unsupported field; 4 for persistence or required safety/startup failure.\nExample: tmax model override list replace deepseek/deepseek-flash overrides.modalities.input --value image")
                .arg(repeated("value", "STRING", "List member; repeat in order. Omit for an explicit empty list."))
                .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name."))
                .arg(required_pos(2, "FIELD", "overrides.modalities.input or overrides.modalities.output.")), true, true)))
        ;
    family("model", "Inspect and manage concrete offerings, aliases, priorities, schedules, and explicit probes.", Command::new("model")
        .subcommand(leaf("list", "List concrete models and aliases with protocol/publication/availability summaries.", "Optional filters are --provider ID, --protocol P, and --kind concrete|alias. Native selections do not publish proxy models. Publication is not proof of upstream success.\n\nExit: 0 when read completes.\nExample: tmax model list --kind concrete")
            .arg(value("provider", "ID", "Filter by local provider ID."))
            .arg(protocol_value("protocol", "Filter by native protocol."))
            .arg(value("kind", "KIND", "concrete or alias.").value_parser(["concrete", "alias"])))
        .subcommand(leaf("inspect", "Inspect one qualified concrete offering or unambiguous alias ID.", "Concrete qualified names identify an offering independent of protocol; --protocol filters details only. Alias identity is protocol plus ID; ambiguous bare IDs require --protocol. Includes effective metadata/provenance, local overrides, revision-scoped local indices, capabilities, priorities/rules, and file/loaded differences.\n\nExit: 0 when read completes; 2 for unknown/ambiguous identity.\nExample: tmax model inspect deepseek/deepseek-flash")
            .arg(protocol_value("protocol", "Optional protocol filter; required for ambiguous alias IDs."))
            .arg(required_pos(1, "QUALIFIED-OR-ID", "Qualified concrete name or alias ID.")))
        .subcommand(edit_args(
            mutation(
                leaf_with_fields("add", "Register one exact upstream model offering under an existing provider.", "Registration is explicit and does not require catalog metadata to list the model.", model_fields)
                    .arg(required_pos(1, "QUALIFIED", "provider-id/model-id.")),
                true,
                true,
            ),
            model_fields,
        ))
        .subcommand(edit_args(
            mutation(
                leaf_with_fields("update", "Edit supported scalar metadata and priority fields for one offering.", "At least one edit is required. Clearing an optional override restores documented inheritance/default. Structured lists and reasoning options use dedicated operations.", model_fields)
                    .arg(required_pos(1, "QUALIFIED", "provider-id/model-id.")),
                true,
                true,
            ),
            model_fields,
        ))
        .subcommand(mutation(leaf("rename", "Rename an upstream model-ID component and migrate dependent references.", "The provider component cannot change. Preserve internal offering identity; reject collisions and leave no aliases.\n\nExit: 0 when saved; 2 for collision/reference errors; 4 for persistence or required safety/startup failure.\nExample: tmax model rename deepseek/deepseek-flash deepseek-v4.1-flash")
            .arg(required_pos(1, "QUALIFIED", "Current provider-id/model-id."))
            .arg(required_pos(2, "NEW-MODEL-ID", "New upstream model-ID component.")), true, true))
        .subcommand(mutation(leaf("remove", "Remove one model registration after reference and input-coverage checks.", "Does not delete catalog metadata or another provider's offering. Resolve references explicitly; do not cascade or choose a replacement.\n\nExit: 0 when saved; 2 for unresolved references; 4 for persistence or required safety/startup failure.\nExample: tmax model remove deepseek/deepseek-flash")
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name.")), true, true))
        .subcommand(override_commands)
        .subcommand(reasoning)
        .subcommand(schedule)
        .subcommand(rule)
        .subcommand(alias)
        .subcommand(explain)
        .subcommand(probe)
        .subcommand_required(true))
}

fn schedule_rule(name: &'static str, indexed: bool) -> Command {
    let about = if indexed {
        "Replace one complete calendar schedule entry at its required file revision."
    } else {
        "Append one complete calendar schedule entry."
    };
    let detail = "Requires --priority INTEGER, one unique selector form (--weekday DAY repeated or --date DATE repeated), and one complete window (--all-day or both --start HH:MM --end HH:MM). DAYS: mon,tue,wed,thu,fri,sat,sun. Weekdays and dates cannot mix. Optional --effective-from DATE and --effective-until DATE use local dates with an exclusive upper bound. Set the model timezone first. Times and DST follow the model time-rule contract.\n\nExit: 0 when saved; 2 for invalid rule; 3 for revision conflict; 4 for persistence or required safety/startup failure.\nExample: tmax model schedule add deepseek/deepseek-flash --priority 100 --weekday mon --start 00:00 --end 08:00";
    let mut command = mutation(leaf(name, about, detail)
        .arg(signed_value("priority", "INTEGER", "Signed replacement priority.").required(true))
        .arg(weekdays("weekday", "Weekday selector; repeat unique values.").conflicts_with("date"))
        .arg(repeated("date", "DATE", "YYYY-MM-DD local date selector; repeat unique values.").conflicts_with("weekday"))
        .arg(flag("all-day", "Use the whole selected local day; alternatively provide both --start and --end.").conflicts_with_all(["start", "end"]))
        .arg(value("start", "HH:MM", "Local window start.").requires("end").conflicts_with("all-day"))
        .arg(value("end", "HH:MM", "Local window end.").requires("start").conflicts_with("all-day"))
        .arg(value("effective-from", "DATE", "Inclusive local-date lower bound."))
        .arg(value("effective-until", "DATE", "Exclusive local-date upper bound."))
        .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name."))
        .group(ArgGroup::new("selector").args(["weekday", "date"]).required(true).multiple(true))
        .group(ArgGroup::new("window").args(["all-day", "start", "end"]).required(true).multiple(true)), true, false);
    if indexed {
        command = command
            .arg(
                value(
                    "if-revision",
                    "REVISION",
                    "Require the exact current file revision.",
                )
                .required(indexed),
            )
            .arg(required_pos(2, "INDEX", "Zero-based schedule position."));
    } else {
        command = command.arg(value(
            "if-revision",
            "REVISION",
            "Optional exact current file revision.",
        ));
    }
    command
}

fn model_rule_commands() -> Command {
    let make = |name: &'static str, replace: bool| {
        let about = if replace {
            "Replace one complete signal rule, retaining its ID and position."
        } else {
            "Append a new immutable offering-local signal-rule ID."
        };
        let detail = "Requires --input INPUT --priority INTEGER --reset-within DURATION. Quota inputs also require --remaining-at-least-percent DECIMAL; reset inputs forbid it. Optional --enabled BOOLEAN defaults true. The input must belong to this provider and cover the upstream model; kind is derived from the same declaration snapshot. Optional time gates require a complete date/weekday selector and complete --all-day or --start/--end window, and use the model timezone. DURATION is a positive integer and one lowercase s/m/h/d unit.\n\nReplace requires the complete required definition; omitted optional gate clears it and omitted enabled restores true. Missing/stale input keeps a valid rule unknown, never zero quota.\n\nExit: 0 when saved; 2 for invalid reference/definition; 3 for required current action; 4 for persistence or required safety/startup failure.\nExample: tmax model rule add opencode-go/deepseek-v4.1-flash use-weekly --input weekly-allowance --priority 120 --reset-within 6h --remaining-at-least-percent 75.0";
        mutation(
            leaf(name, about, detail)
                .arg(
                    value(
                        "input",
                        "INPUT",
                        "Exact input name within the same provider.",
                    )
                    .required(true),
                )
                .arg(
                    signed_value("priority", "INTEGER", "Signed replacement priority.")
                        .required(true),
                )
                .arg(
                    value(
                        "reset-within",
                        "DURATION",
                        "Positive elapsed horizon, e.g. 6h.",
                    )
                    .required(true),
                )
                .arg(value(
                    "remaining-at-least-percent",
                    "DECIMAL",
                    "Required for quota, forbidden for reset.",
                ))
                .arg(value("enabled", "BOOLEAN", "true or false; defaults true."))
                .arg(
                    weekdays("weekday", "Optional time-gate weekday selector.")
                        .conflicts_with("date")
                        .requires("window"),
                )
                .arg(
                    repeated("date", "DATE", "Optional time-gate date selector.")
                        .conflicts_with("weekday")
                        .requires("window"),
                )
                .arg(
                    flag("all-day", "Use the whole selected local day.")
                        .conflicts_with_all(["start", "end"])
                        .requires("selector"),
                )
                .arg(
                    value("start", "HH:MM", "Optional time-gate window start.")
                        .requires("end")
                        .requires("selector")
                        .conflicts_with("all-day"),
                )
                .arg(
                    value("end", "HH:MM", "Optional time-gate window end.")
                        .requires("start")
                        .requires("selector")
                        .conflicts_with("all-day"),
                )
                .arg(value(
                    "effective-from",
                    "DATE",
                    "Optional inclusive local-date bound.",
                ))
                .arg(value(
                    "effective-until",
                    "DATE",
                    "Optional exclusive local-date bound.",
                ))
                .arg(required_pos(
                    1,
                    "QUALIFIED",
                    "Qualified concrete model name.",
                ))
                .arg(required_pos(
                    2,
                    "RULE-ID",
                    "Offering-local immutable rule ID.",
                ))
                .group(
                    ArgGroup::new("selector")
                        .args(["weekday", "date"])
                        .multiple(true),
                )
                .group(
                    ArgGroup::new("window")
                        .args(["all-day", "start", "end"])
                        .multiple(true),
                ),
            true,
            true,
        )
    };
    Command::new("rule")
        .about("Manage offering-local input-driven priority rules.")
        .subcommand(state_arg(leaf("list", "List ordered signal rules and their current outcomes.", "Read-only retained-fact evaluation at one coherent current instant. Does not refresh data or resolve secrets.\n\nExit: 0 even when inputs are unknown/unavailable.\nExample: tmax model rule list opencode-go/deepseek-v4.1-flash")
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name."))))
        .subcommand(state_arg(leaf("inspect", "Inspect one rule in the offering's ordered evaluation context.", "Use optional --at RFC3339 explicit-offset instant with retained facts only. No refresh or credential resolution.\n\nExit: 0 even for false/unknown/shadowed rule outcomes.\nExample: tmax model rule inspect opencode-go/deepseek-v4.1-flash use-weekly --at 2026-10-23T10:00:00Z")
            .arg(value("at", "INSTANT", "RFC 3339 explicit-offset evaluation instant."))
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name."))
            .arg(required_pos(2, "RULE-ID", "Exact immutable rule ID."))))
        .subcommand(make("add", false))
        .subcommand(make("replace", true))
        .subcommand(mutation(leaf("remove", "Remove one signal rule without changing its input, schedule, or base priority.", "Stable rule ID, not array position, addresses this operation.\n\nExit: 0 when saved; 2 for missing ID; 4 for persistence or required safety/startup failure.\nExample: tmax model rule remove opencode-go/deepseek-v4.1-flash use-weekly")
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name."))
            .arg(required_pos(2, "RULE-ID", "Exact immutable rule ID.")), true, true))
        .subcommand(mutation(leaf("move", "Move a signal rule before another or to the end; no priority sort.", "Supply exactly one of --before OTHER-RULE-ID or --last. Moving an already-last rule to last is an idempotent no-op.\n\nExit: 0 when moved/no-op; 2 for missing/self target; 4 for persistence or required safety/startup failure.\nExample: tmax model rule move opencode-go/deepseek-v4.1-flash use-weekly --last")
            .arg(value("before", "RULE-ID", "Move before this distinct existing rule."))
            .arg(flag("last", "Move to the end; mutually exclusive with --before."))
            .group(ArgGroup::new("destination").args(["before", "last"]).required(true))
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name."))
            .arg(required_pos(2, "RULE-ID", "Rule to move.")), true, true))
        .subcommand(mutation(leaf("enable", "Enable one rule without changing fields or order.", "Enabling does not create input data or change fixed selections/session destinations.\n\nExit: 0 when saved; 2 for missing ID; 4 for persistence or required safety/startup failure.\nExample: tmax model rule enable opencode-go/deepseek-v4.1-flash use-weekly")
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name."))
            .arg(required_pos(2, "RULE-ID", "Exact immutable rule ID.")), true, true))
        .subcommand(mutation(leaf("disable", "Disable one rule while retaining fields/references/data.", "Disabling does not remove input declarations or current data.\n\nExit: 0 when saved; 2 for missing ID; 4 for persistence or required safety/startup failure.\nExample: tmax model rule disable opencode-go/deepseek-v4.1-flash use-weekly")
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name."))
            .arg(required_pos(2, "RULE-ID", "Exact immutable rule ID.")), true, true))
        .subcommand(mutation(leaf("clear", "Remove all signal rules without changing schedule, inputs, or base priority.", "\n\nExit: 0 when saved; 4 for persistence or required safety/startup failure.\nExample: tmax model rule clear opencode-go/deepseek-v4.1-flash")
            .arg(required_pos(1, "QUALIFIED", "Qualified concrete model name.")), true, true))
        .subcommand_required(true)
}

fn model_alias_commands() -> Command {
    let alias_edit = "Editable fields: enabled (boolean; clear restores true), continuity (session|stateless; clear restores session). One or more explicit candidates are required, in user order, and accept registered qualified concrete offerings only. Candidate lists do not accept aliases, recursive aliases, bare providers, or duplicates. Protocol and ID jointly identify an alias; names such as L0 and L1 have no built-in meaning.";
    let add = mutation(
        leaf_with_fields(
            "add",
            "Create one protocol-qualified model alias backed by explicit candidates.",
            "Requires one-or-more --candidate QUALIFIED. Candidates form the complete ordered list.\n\nExit: 0 when saved; 2 for invalid ID/candidates; 4 for persistence or required safety/startup failure.\nExample: tmax model alias add L0 --protocol chat-completions --candidate deepseek/deepseek-flash",
            alias_edit,
        )
        .arg(protocol_value("protocol", "chat-completions, responses, or messages.").required(true))
        .arg(repeated("candidate", "QUALIFIED", "Registered qualified concrete offering; repeat in order.").required(true))
        .arg(pair("set", "FIELD", "VALUE", "Set enabled or continuity.").long_help(alias_edit))
        .arg(repeated("clear", "FIELD", "Clear enabled or continuity.").long_help(alias_edit))
        .arg(required_pos(1, "ID", "Public alias ID.")),
        true,
        true,
    );
    let update = mutation(
        leaf_with_fields(
            "update",
            "Edit alias fields and optionally replace its complete candidate list.",
            "At least one change is required. Repeated --candidate replaces the full list, never appends. Omit --candidate to preserve the current list. --new-protocol changes identity only after collision/reference checks.\n\nExit: 0 when saved; 2 for invalid identity/candidates; 4 for persistence or required safety/startup failure.\nExample: tmax model alias update L0 --protocol chat-completions --set enabled false",
            alias_edit,
        )
        .arg(protocol_value("protocol", "Current protocol identity.").required(true))
        .arg(protocol_value("new-protocol", "Optional new protocol identity."))
        .arg(repeated("candidate", "QUALIFIED", "Replace entire ordered candidate list."))
        .arg(pair("set", "FIELD", "VALUE", "Set enabled or continuity.").long_help(alias_edit))
        .arg(repeated("clear", "FIELD", "Clear enabled or continuity.").long_help(alias_edit))
        .arg(required_pos(1, "ID", "Public alias ID.")),
        true,
        true,
    );
    let candidates = Command::new("candidates")
        .about("Append or remove explicit candidates from an alias.")
        .subcommand(mutation(leaf("add", "Append new unique candidates to an alias list.", "Rejects duplicate members and non-concrete/unregistered candidates. Candidate order is preserved.\n\nExit: 0 when saved; 2 for invalid member; 4 for persistence or required safety/startup failure.\nExample: tmax model alias candidates add L0 --protocol chat-completions --candidate deepseek/deepseek-flash")
            .arg(protocol_value("protocol", "Alias protocol identity.").required(true))
            .arg(repeated("candidate", "QUALIFIED", "Registered qualified concrete candidate; repeat.").required(true))
            .arg(required_pos(1, "ID", "Public alias ID.")), true, true))
        .subcommand(mutation(leaf("remove", "Remove named members from an alias list.", "Rejects missing members and an empty resulting list. Does not select replacements.\n\nExit: 0 when saved; 2 for invalid member/result; 4 for persistence or required safety/startup failure.\nExample: tmax model alias candidates remove L0 --protocol chat-completions --candidate deepseek/deepseek-flash")
            .arg(protocol_value("protocol", "Alias protocol identity.").required(true))
            .arg(repeated("candidate", "QUALIFIED", "Exact existing candidate to remove; repeat.").required(true))
            .arg(required_pos(1, "ID", "Public alias ID.")), true, true));
    Command::new("alias")
        .about("Manage public model aliases and their explicit candidates/continuity.")
        .subcommand(add)
        .subcommand(update)
        .subcommand(candidates)
        .subcommand(mutation(leaf("rename", "Rename an alias and update target references without an old-ID redirect.", "Requires --protocol. Rejects collision and unresolved old references; does not convert protocols or retarget sessions.\n\nExit: 0 when saved; 2 for collision/reference errors; 4 for persistence or required safety/startup failure.\nExample: tmax model alias rename L0 preferred --protocol chat-completions")
            .arg(protocol_value("protocol", "Exact alias protocol identity.").required(true))
            .arg(required_pos(1, "OLD", "Current public alias ID."))
            .arg(required_pos(2, "NEW", "New public alias ID.")), true, true))
        .subcommand(mutation(leaf("remove", "Remove an alias definition after reference checks.", "Removal/disablement reports affected retained sessions; no cascade, retarget, or old-ID redirect.\n\nExit: 0 when saved; 2 for unresolved references; 4 for persistence or required safety/startup failure.\nExample: tmax model alias remove L0 --protocol chat-completions")
            .arg(protocol_value("protocol", "Exact alias protocol identity.").required(true))
            .arg(required_pos(1, "ID", "Public alias ID.")), true, true))
        .subcommand_required(true)
}
fn agent_commands() -> Command {
    let agent_update = mutation(leaf("update", "Change explicit structural fields of one declared native target.", "At least one of --agent AGENT or --path PATH is required. Does not restore old files, import credentials, or write native configuration. Relative path operands resolve under ~/.tmax.\n\nExit: 0 when saved; 2 for invalid target/path; 4 for persistence or required safety/startup failure.\nExample: tmax agent update pi-main --path /home/user/.pi/agent")
        .arg(agent_value("agent", "codex, pi, omp, opencode, or hermes."))
        .arg(value("path", "PATH", "Native configuration scope; relative paths resolve under ~/.tmax."))
        .arg(required_pos(1, "TARGET", "Exact declared target ID."))
        .group(ArgGroup::new("scope_fields").args(["agent", "path"]).required(true).multiple(true)), true, true);
    let model = Command::new("model")
        .about("Manage selections within one target-local provider entry.")
        .subcommand(mutation(leaf("add", "Add one exact model selection to a native provider entry.", "Does not edit the default or role mappings; first selection never becomes default. ID semantics depend on whether the entry points to an upstream provider or the local proxy.\n\nExit: 0 when saved; 2 for invalid source/reference; 4 for persistence or required safety/startup failure.\nExample: tmax agent model add pi-main upstream deepseek-flash --protocol chat-completions")
            .arg(protocol_value("protocol", "chat-completions, responses, or messages.").required(true))
            .arg(required_pos(1, "TARGET", "Declared target ID."))
            .arg(required_pos(2, "NAME", "Target-local native provider-entry name."))
            .arg(required_pos(3, "ID", "Exact upstream model ID or public proxy ID.")), true, true))
        .subcommand(mutation(leaf("remove", "Remove one selected model after default/role reference checks.", "Does not edit native files until explicit agent apply.\n\nExit: 0 when saved; 2 for unresolved default/role references; 4 for persistence or required safety/startup failure.\nExample: tmax agent model remove pi-main upstream deepseek-flash --protocol chat-completions")
            .arg(protocol_value("protocol", "Exact native model protocol.").required(true))
            .arg(required_pos(1, "TARGET", "Declared target ID."))
            .arg(required_pos(2, "NAME", "Target-local native provider-entry name."))
            .arg(required_pos(3, "ID", "Exact selected model ID.")), true, true))
        .subcommand(mutation(leaf("clear", "Clear selections for one provider entry after default/role references are removed.", "Writes an empty selection list only in desired configuration; explicit apply is separate.\n\nExit: 0 when saved; 2 for unresolved references; 4 for persistence or required safety/startup failure.\nExample: tmax agent model clear pi-main upstream")
            .arg(required_pos(1, "TARGET", "Declared target ID."))
            .arg(required_pos(2, "NAME", "Target-local native provider-entry name.")), true, true));
    let default = Command::new("default")
        .about("Set or clear the native default for a target.")
        .subcommand(mutation(leaf("set", "Set a target default to a model already selected in the named entry.", "Requires explicit --protocol. A default is configuration, not a proxy permission or inference request.\n\nExit: 0 when saved; 2 if selection/reference is invalid; 4 for persistence or required safety/startup failure.\nExample: tmax agent default set pi-main upstream deepseek-flash --protocol chat-completions")
            .arg(protocol_value("protocol", "Exact native protocol.").required(true))
            .arg(required_pos(1, "TARGET", "Declared target ID."))
            .arg(required_pos(2, "NAME", "Target-local entry name."))
            .arg(required_pos(3, "ID", "Model ID already selected in that entry.")), true, true))
        .subcommand(mutation(leaf("clear", "Remove a target's default without changing entries/selections/roles.", "Native application is separate.\n\nExit: 0 when saved; 4 for persistence or required safety/startup failure.\nExample: tmax agent default clear pi-main")
            .arg(required_pos(1, "TARGET", "Declared target ID.")), true, true));
    let role = Command::new("role")
        .about("Map or remove native model roles supported by the selected Agent adapter.")
        .subcommand(mutation(leaf("set", "Map one qualified native role to an existing selected model.", "ROLE names come from agent inspection; unknown names are invalid. A role is a native configuration setting, not an access-control role or request parameter.\n\nExit: 0 when saved; 2 for unsupported role/reference; 4 for persistence or required safety/startup failure.\nExample: tmax agent role set omp-main smol upstream deepseek-flash --protocol chat-completions")
            .arg(protocol_value("protocol", "Exact native protocol.").required(true))
            .arg(required_pos(1, "TARGET", "Declared target ID."))
            .arg(required_pos(2, "ROLE", "Adapter-supported native model role."))
            .arg(required_pos(3, "NAME", "Target-local provider-entry name."))
            .arg(required_pos(4, "ID", "Model selected by that entry.")), true, true))
        .subcommand(mutation(leaf("remove", "Remove one native role mapping.", "Does not change selections/default or apply native files.\n\nExit: 0 when saved; 2 for unknown role; 4 for persistence or required safety/startup failure.\nExample: tmax agent role remove omp-main smol")
            .arg(required_pos(1, "TARGET", "Declared target ID."))
            .arg(required_pos(2, "ROLE", "Exact adapter-supported role.")), true, true))
        .subcommand(mutation(leaf("clear", "Remove all native role mappings, not model selections/default.", "\n\nExit: 0 when saved; 4 for persistence or required safety/startup failure.\nExample: tmax agent role clear omp-main")
            .arg(required_pos(1, "TARGET", "Declared target ID.")), true, true));
    let provider = Command::new("provider")
        .about("Manage target-local provider entries with explicit upstream or proxy source.")
        .subcommand(mutation(leaf("add", "Add an empty-model native provider entry with exactly one source.", "Choose exactly one of --provider PROVIDER or --proxy. An upstream provider is a tmax provider ID; proxy uses the sole local proxy. Does not provision a key, start services, or write native files.\n\nExit: 0 when saved; 2 for invalid/duplicate entry; 4 for persistence or required safety/startup failure.\nExample: tmax agent provider add pi-main local --proxy")
            .arg(value("provider", "PROVIDER", "Existing tmax upstream provider ID."))
            .arg(flag("proxy", "Use the sole tmax model proxy."))
            .arg(required_pos(1, "TARGET", "Declared target ID."))
            .arg(required_pos(2, "NAME", "Unique target-local provider-entry name."))
            .group(ArgGroup::new("source").args(["provider", "proxy"]).required(true)), true, true))
        .subcommand(mutation(leaf("update", "Replace a target-local provider entry's source.", "Choose exactly one of --provider PROVIDER or --proxy. Validates existing selections/default/roles against the new source; never reinterprets, repairs, clears, or adds them. Does not write native files.\n\nExit: 0 when saved; 2 for invalid source/reference; 4 for persistence or required safety/startup failure.\nExample: tmax agent provider update pi-main local --provider deepseek")
            .arg(value("provider", "PROVIDER", "Existing tmax upstream provider ID."))
            .arg(flag("proxy", "Use the sole tmax model proxy."))
            .arg(required_pos(1, "TARGET", "Declared target ID."))
            .arg(required_pos(2, "NAME", "Target-local provider-entry name."))
            .group(ArgGroup::new("source").args(["provider", "proxy"]).required(true)), true, true))
        .subcommand(mutation(leaf("remove", "Remove a provider entry after default/role/reference checks.", "Does not write native files or revoke shared proxy access. Explicit apply/restore is separate.\n\nExit: 0 when saved; 2 for unresolved references; 4 for persistence or required safety/startup failure.\nExample: tmax agent provider remove pi-main local")
            .arg(required_pos(1, "TARGET", "Declared target ID."))
            .arg(required_pos(2, "NAME", "Target-local provider-entry name.")), true, true));
    let recovery = Command::new("recovery")
        .about("Inspect, restore, or explicitly discard protected native-change recovery records.")
        .subcommand(leaf("list", "List retained recovery handles and paths, optionally scoped to one target.", "Includes records for removed targets. Does not expose credentials.\n\nExit: 0 when inventory read completes.\nExample: tmax agent recovery list --target pi-main")
            .arg(value("target", "TARGET", "Optional declared target filter.")))
        .subcommand(leaf("inspect", "Inspect ownership, target identity, affected files, and safe recovery actions.", "A recovery handle is not a target ID; reused target IDs do not select predecessor records.\n\nExit: 0 when safe metadata is available.\nExample: tmax agent recovery inspect RECORD")
            .arg(required_pos(1, "RECORD", "Returned native-recovery handle.")))
        .subcommand(mutation(leaf("discard", "Discard one native recovery record without native writes or credential deletion.", "Protected restore information is retained until restore completes or it is explicitly discarded.\n\nExit: 0 when discarded; 3 for current safety blocker; 4 for deletion failure.\nExample: tmax agent recovery discard RECORD --dry-run")
            .arg(required_pos(1, "RECORD", "Returned native-recovery handle.")), true, false));
    let inspect = leaf("inspect", "Inspect a declared target or an explicit undeclared native scope without importing it.", "Declared inspection reports native entries/sources, precedence, supported roles/requirements, effective auth/model use, desired/applied/effective differences, blockers, and recovery. Undeclared scope requires --agent and --path together and never registers/imports configuration. Inspection is not proof of paid inference.\n\nExit: 0 when inspect completes, including unknown evidence; 2 for invalid scope syntax.\nExample: tmax agent inspect pi-main")
        .arg(agent_value("agent", "Agent for an explicit undeclared native scope.").requires("path").conflicts_with("__operand_1"))
        .arg(value("path", "PATH", "Native scope path for undeclared inspection.").requires("agent").conflicts_with("__operand_1"))
        .arg(positional(1, "TARGET", "Declared target ID; alternatively supply --agent and --path.").conflicts_with_all(["agent", "path"]))
        .group(ArgGroup::new("scope").args(["__operand_1", "agent"]).required(true).multiple(true));
    family("agent", "Declare targets, edit native provider/model settings, apply changes, and restore owned state.", Command::new("agent")
        .subcommand(leaf("list", "List declared targets and desired/applied/effective readiness summaries.", "Compact state read; does not inspect/install/import external configuration.\n\nExit: 0 when read completes.\nExample: tmax agent list"))
        .subcommand(leaf("discover", "Discover supported Agent installations, scopes, versions, and profiles.", "Read-only local discovery; does not register targets or import settings/credentials.\n\nExit: 0 when discovery completes; unsupported/unavailable installations remain explicit.\nExample: tmax agent discover --agent pi")
            .arg(agent_value("agent", "Optional filter: codex, pi, omp, opencode, or hermes.")))
        .subcommand(inspect)
        .subcommand(mutation(leaf("add", "Declare a native target without provider entries, defaults, or roles.", "Requires --agent and --path. Does not provision a proxy key, start a service, or write native files. Relative paths resolve against ~/.tmax.\n\nExit: 0 when saved; 2 for invalid/duplicate target; 4 for persistence or required safety/startup failure.\nExample: tmax agent add pi-main --agent pi --path /home/user/.pi/agent")
            .arg(agent_value("agent", "codex, pi, omp, opencode, or hermes.").required(true))
            .arg(value("path", "PATH", "Native configuration scope.").required(true))
            .arg(required_pos(1, "TARGET", "New unique target ID.")), true, true))
        .subcommand(agent_update)
        .subcommand(mutation(leaf("rename", "Rename a target and dependent login-source references.", "Retains ownership/recovery identity; no alias is left. Existing native paths/credentials are not moved.\n\nExit: 0 when saved; 2 for collision/reference errors; 4 for persistence or required safety/startup failure.\nExample: tmax agent rename pi-main pi-work")
            .arg(required_pos(1, "OLD", "Current target ID."))
            .arg(required_pos(2, "NEW", "New unique target ID.")), true, true))
        .subcommand(mutation(leaf("remove", "Stop managing a target after reference checks.", "Leaves native files, credentials, protected recovery, and shared proxy access untouched. Restore first if desired. No implicit sign-out or native write.\n\nExit: 0 when saved; 2 for unresolved references; 4 for persistence or required safety/startup failure.\nExample: tmax agent remove pi-main")
            .arg(required_pos(1, "TARGET", "Declared target ID.")), true, true))
        .subcommand(provider)
        .subcommand(model)
        .subcommand(default)
        .subcommand(role)
        .subcommand(mutation(leaf("apply", "Apply the target's complete current desired configuration to native files.", "Checks current native inputs/ownership and credential handoff. Proxy entries require the ready matching listener/shared key; upstream entries do not require tmax proxy. Reports changed/unchanged files and recovery. Written files are not proof of runtime reload. No force overwrite or automatic apply.\n\nDry-run performs no native writes, sign-in, key resolution, or launch.\nExit: 0 when applied; 3 when complete write awaits native reload; 4 for partial/uncertain writes; 5 only for cancellation before effects.\nExample: tmax agent apply pi-main --dry-run")
            .arg(required_pos(1, "TARGET", "Declared target ID.")), true, false))
        .subcommand(mutation(leaf("restore", "Restore owned native changes while preserving later user edits.", "Supply TARGET or --recovery RECORD, not both. Exact recovery handles remain usable after target removal. Ownership conflicts block rather than force overwrite. Does not recreate a target or delete credentials.\n\nDry-run writes nothing.\nExit: 0 when restored; 3 for conflict/action required; 4 for partial or failed native restoration.\nExample: tmax agent restore pi-main --dry-run")
            .arg(value("recovery", "RECORD", "Exact retained recovery handle; alternative to TARGET."))
            .arg(positional(1, "TARGET", "Declared target ID; alternative to --recovery."))
            .group(ArgGroup::new("target_or_recovery").args(["__operand_1", "recovery"]).required(true)), true, false))
        .subcommand(recovery)
        .subcommand_required(true))
}

fn network_commands() -> Command {
    Command::new("network")
        .about("Configure and inspect the one explicit tmax outbound network policy.")
        .subcommand(mutation(leaf("proxy", "Route new tmax-owned outbound work through one explicit proxy URL.", "URL schemes: http, https, socks5; scheme default ports apply. SOCKS5 destination DNS is proxy-side. Userinfo/proxy authentication, provider overrides, bypass lists, environment/system discovery, direct fallback, and TLS bypass are unsupported. Does not contact the proxy or change native networking.\n\nExit: 0 when saved; 2 for invalid URL; 4 for persistence or required safety/startup failure.\nExample: tmax network proxy http://127.0.0.1:8080")
            .arg(required_pos(1, "URL", "Explicit outbound proxy URL.")), true, true))
        .subcommand(mutation(leaf("direct", "Select the canonical direct outbound mode by removing the optional policy table.", "Does not contact remote services or alter native-agent network settings. No ambient proxy variables are inherited.\n\nExit: 0 when saved; 4 for persistence or required safety/startup failure.\nExample: tmax network direct --dry-run"), true, true))
        .subcommand(state_arg(leaf("show", "Show effective and saved/loaded outbound network policy.", "Read-only; no connectivity or secret check. Future independent CLI work uses saved policy; resident work uses loaded policy; in-flight work retains its captured route.\n\nExit: 0 for available/absent/unavailable policy.\nExample: tmax network show --state loaded")))
        .subcommand_required(true)
}

fn proxy_commands() -> Command {
    let autostart = Command::new("autostart")
        .about("Explicitly enable or disable this authority's per-user login-start registration.")
        .subcommand(mutation(leaf("enable", "Save enabled startup intent and reconcile tmax's owned per-user registration.", "Always reconciles even if the saved value is already true. Does not start inference. Reports unavailable credential requirements. Unrelated startup entries are untouched.\n\nExit: 0 when save/reconciliation completes; 3 when action is required; 4 when requested registration fails.\nExample: tmax proxy autostart enable") , true, true))
        .subcommand(mutation(leaf("disable", "Save disabled startup intent and reconcile removal of tmax's registration.", "Always reconciles even if the saved value is already false. Does not stop inference or alter unrelated entries.\n\nExit: 0 when save/reconciliation completes; 4 when requested registration removal fails.\nExample: tmax proxy autostart disable"), true, true));
    let key = Command::new("key")
        .about("Explicitly replace or securely reveal the sole shared local proxy key.")
        .subcommand(mutation(leaf("replace", "Generate/store a new shared inference key and save its reference.", "Validates current config/ownership before provisioning. Saves and activates separately; a provisioned key remains discoverable for explicit cleanup after save failure. Does not update native or unmanaged clients. Replacing revokes old-key admission after activation; admitted requests retain their captured credential.\n\nDry-run does not generate/reserve a key or read protected input.\nExit: 0 when requested boundaries complete; 3 when action is required; 4 for credential/save/activation partial failure.\nExample: tmax proxy key replace --dry-run"), true, true))
        .subcommand(mutation(leaf("show", "Deliver the configured shared proxy key through one explicitly qualified protected output.", "Without --output-fd, writes one UTF-8 line only to the attached controlling terminal. --output-fd FD requires a qualified inherited writable pipe >=3, not aliased to stdout/stderr. Noninteractive use requires the pipe; never fall back to stdout, stderr, argv, environment, or a file. The result never contains key bytes. Delivery does not prove readiness, activate saved changes, start inference, or register clients.\n\nDry-run reads/resolves no key and writes no secret bytes.\nExit: 0 on complete delivery; 3 when interaction is required; 4 on partial/uncertain delivery.\nExample: tmax proxy key show --output-fd 3 --non-interactive")
            .arg(value("output-fd", "FD", "Qualified inherited writable pipe at least 3: Unix descriptor or Windows native handle; never stdout/stderr.")), true, false));
    Command::new("proxy")
        .about("Inspect and explicitly control the background inference listener.")
        .subcommand(leaf("status", "Report actual resident/listener identity, key readiness, publication, and safe-session conditions.", "Read actual resident state and versions; saved/loaded network policy remains distinct. Does not read or reveal the shared key. Native target readiness is not an inference prerequisite.\n\nExit: 0 even when stopped, unavailable, or incompatible state is reported.\nExample: tmax proxy status"))
        .subcommand(mutation(
            leaf("configure", "Create/edit the optional proxy settings; creation provisions the shared key explicitly.", "Initial creation requires --listen numeric loopback host:port and provisions one key. Existing edits retain the key. --clear autostart restores disabled intent but does not force reconciliation. No implicit start, native write, key repair, or key generation on an existing table. Report provision/save/activation separately.\n\nExit: 0 when save completes; 2 for invalid address/config; 3 for required action; 4 for provisioning, persistence, or required safety/startup failure.\nExample: tmax proxy configure --listen 127.0.0.1:17840")
                .arg(value("listen", "ADDRESS", "Numeric loopback host:port; IPv6 must be bracketed."))
                .arg(value("clear", "FIELD", "Only autostart; restores disabled intent.").value_parser(["autostart"])),
            true,
            true,
        ))
        .subcommand(mutation(leaf("remove", "Remove proxy configuration only after inference is stopped and proxy references resolve.", "Does not stop inference implicitly, delete stored key material, edit native files, or remove unknown external copies. Explicit restart is needed to activate a saved address change.\n\nExit: 0 when removed; 3 if listener/references require action; 4 for persistence or required safety/startup failure.\nExample: tmax proxy remove --dry-run"), true, true))
        .subcommand(mutation(leaf("start", "Explicitly start inference in the sole identified compatible resident.", "Reuses a compatible running listener without reload or reapply. A management-only resident may start inference explicitly. Validates saved config and existing shared key; never generates a key or applies native settings. No target prerequisite. On Unix, an unrepresentable OS-home control socket path blocks before resident launch with resident_control_path_invalid; the store is not relocated.\n\nDry-run does not start, reserve address, or resolve secrets.\nExit: 0 when started/reused; 3 for incompatible owner, unsupported control path, unavailable key, or required action; 4 for startup failure.\nExample: tmax proxy start"), true, false))
        .subcommand(mutation(leaf("stop", "Stop inference only; normal stop refuses while requests are active.", "Use --cancel-active to close admission, interrupt the actual active set as transports allow, and report effects. Does not stop Web, alter autostart, switch native clients to upstream, or discard retained sessions.\n\nDry-run reserves/freezes no active set.\nExit: 0 when stopped; 3 when active requests require explicit cancellation; 4 for failed/partial lifecycle.\nExample: tmax proxy stop --cancel-active")
            .arg(flag("cancel-active", "Explicitly interrupt currently active requests; no replay.")), true, false))
        .subcommand(mutation(leaf("restart", "Explicitly stop and start inference using current valid saved settings.", "Includes saved listen-address changes. Does not stop Web or replace the resident. Same active-request rule as stop; --cancel-active requests interruption, not approval.\n\nExit: 0 when restarted; 3 when action is required; 4 for partial/failed stop/start.\nExample: tmax proxy restart" )
            .arg(flag("cancel-active", "Explicitly interrupt current requests before restart.")), true, false))
        .subcommand(autostart)
        .subcommand(key)
        .subcommand_required(true)
}

fn web_commands() -> Command {
    Command::new("web")
        .about("Inspect, open, or stop the supplementary loopback management interface.")
        .subcommand(leaf("status", "Report actual resident identity and management listener state.", "Independent of inference and valid desired configuration. No login, pairing, management password, or authenticated browser session. Status does not start management.\n\nExit: 0 even when stopped or unavailable state is reported.\nExample: tmax web status"))
        .subcommand(mutation(leaf("open", "Start/reuse the Web management listener and optionally open its actual URL.", "Requires one compatible sole resident owner; neither inference nor valid desired configuration is required. Without --listen, reuses management or allocates one initial OS-selected loopback port. Explicit mismatches/conflicts do not relocate or replace a listener. On Unix, an unrepresentable OS-home control socket path blocks before resident launch with resident_control_path_invalid; the store is not relocated. Closing browser/terminal does not stop background inference. --no-browser suppresses opening the browser.\n\nDry-run does not start service, reserve a port, or launch a browser.\nExit: 0 when opened/reused; 3 for owner/endpoint conflict, unsupported control path, or required action; 4 for failed/partial listener/browser work.\nExample: tmax web open --listen 127.0.0.1:17841 --no-browser")
            .arg(value("listen", "ADDRESS", "Optional numeric loopback host:port (IPv6 bracketed)."))
            .arg(flag("no-browser", "Do not launch the system browser.")), true, false))
        .subcommand(mutation(leaf("stop", "Stop the management Web listener only in its identified compatible resident.", "Does not stop inference or alter native configuration. If both inference and Web are explicitly stopped, the resident releases ownership when its owned work is safely complete.\n\nDry-run does not stop listeners.\nExit: 0 when stopped; 3 for incompatible/unidentified owner; 4 for failed lifecycle.\nExample: tmax web stop"), true, false))
        .subcommand_required(true)
}

fn history_commands() -> Command {
    let outcomes = [
        "in-progress",
        "rejected",
        "completed",
        "failed",
        "cancelled",
        "interrupted",
        "unknown",
    ];
    let list = leaf("list", "List retained redacted request metadata with explicit history coverage.", "Filters combine with AND. Time interval is [since, until), using admission time or receipt time for pre-admission rejection. --model requires --protocol; protocol alone is invalid. --target matches only independently established retained attribution. No filters means retained records, not complete history. No request body/content archive is kept.\n\nExit: 0 for a completed history read; absent older records are not inferred.\nExample: tmax history list --provider deepseek --outcome failed")
        .arg(value("since", "INSTANT", "Inclusive RFC 3339 explicit-offset time."))
        .arg(value("until", "INSTANT", "Exclusive RFC 3339 explicit-offset time."))
        .arg(value("provider", "ID", "Exact selected upstream provider ID."))
        .arg(value("model", "ID", "Exact public model ID; requires --protocol."))
        .arg(protocol_value("protocol", "Exact native protocol; requires --model."))
        .arg(value("outcome", "OUTCOME", "Filter by retained outcome.").value_parser(outcomes))
        .arg(value("target", "TARGET", "Independently established target attribution only."));
    Command::new("history")
        .about("Inspect or clear bounded in-memory model-proxy request history.")
        .subcommand(list)
        .subcommand(leaf("inspect", "Inspect one retained request's safe detail and historical evidence.", "If evicted/no longer retained, reports that state explicitly. Historical attribution and evidence survive target/model changes without relabeling. No body/content archive or persistent history.\n\nExit: 0 when the retained/no-longer-retained result is reported.\nExample: tmax history inspect REQUEST")
            .arg(required_pos(1, "REQUEST", "Current retained request handle.")))
        .subcommand(mutation(leaf("clear", "Clear terminal request records at one serialized history boundary.", "Never clears in-progress or subsequently arriving/completing records. Does not change settings, sessions, bindings, recovery, or quota data. It never starts a service. Dry-run reports eligible count/coverage without reserving a record set.\n\nExit: 0 when clear completes; 3 when history identity changed; 4 for failed clear.\nExample: tmax history clear --dry-run"), true, false))
        .subcommand_required(true)
}

pub fn operation_hint(command: &Command, argv: &[OsString]) -> Option<String> {
    let mut index = 1;
    let mut current = command;
    let mut path = Vec::new();
    while let Some(token) = argv.get(index).and_then(|value| value.to_str()) {
        if token == "--format" {
            index += 2;
            continue;
        }
        if token.starts_with("--format=") {
            index += 1;
            continue;
        }
        if token == "--non-interactive" {
            index += 1;
            continue;
        }
        if token.starts_with('-') {
            return (!path.is_empty()).then(|| path.join(" "));
        }
        let Some(child) = current
            .get_subcommands()
            .find(|child| child.get_name() == token)
        else {
            return (!path.is_empty()).then(|| path.join(" "));
        };
        path.push(child.get_name().to_owned());
        current = child;
        index += 1;
        if current.get_subcommands().next().is_none() {
            return Some(path.join(" "));
        }
    }
    (!path.is_empty()).then(|| path.join(" "))
}

pub fn request(root_command: &Command, matches: &ArgMatches) -> Request {
    let mut path = Vec::new();
    let leaf = find_leaf(matches, &mut path);
    let operation_command = command_at_path(root_command, &path).unwrap_or(root_command);
    let command = path;
    let mut operands = Vec::new();
    for id in POSITIONAL_IDS {
        if let Ok(Some(values)) = leaf.try_get_raw(id) {
            for value in values {
                if let Some(value) = value.to_str() {
                    operands.push(value.to_owned());
                }
            }
        }
    }

    let mut arguments = std::collections::BTreeMap::new();
    collect_arguments(operation_command, leaf, &mut arguments);
    // Clap may retain global matches only at the root; inspect root metadata as
    // well, without maintaining a parallel list of option names or types.
    collect_arguments(root_command, matches, &mut arguments);

    let dry_run = leaf
        .try_get_one::<bool>("dry-run")
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false);
    let if_revision = leaf
        .try_get_raw("if-revision")
        .ok()
        .flatten()
        .and_then(|mut values| values.next())
        .and_then(|value| value.to_str().map(str::to_owned));
    Request {
        command,
        operands,
        arguments,
        dry_run,
        if_revision,
    }
}

fn collect_arguments(
    command: &Command,
    matches: &ArgMatches,
    arguments: &mut std::collections::BTreeMap<String, Vec<String>>,
) {
    for id in matches.ids() {
        let key = id.as_str();
        if key == "format"
            || key == "dry-run"
            || key == "if-revision"
            || POSITIONAL_IDS.contains(&key)
        {
            continue;
        }
        let Some(argument) = command
            .get_arguments()
            .find(|argument| argument.get_id().as_str() == key)
        else {
            continue;
        };
        match argument.get_action() {
            ArgAction::SetTrue => {
                if matches
                    .try_get_one::<bool>(key)
                    .ok()
                    .flatten()
                    .copied()
                    .unwrap_or(false)
                {
                    arguments.insert(key.to_owned(), vec!["true".to_owned()]);
                }
                continue;
            }
            ArgAction::Count => {
                if matches.get_count(key) > 0 {
                    arguments.insert(key.to_owned(), vec!["true".to_owned()]);
                }
                continue;
            }
            _ => {}
        }
        if let Ok(Some(values)) = matches.try_get_raw(key) {
            let values: Vec<String> = values
                .filter_map(|value| value.to_str().map(str::to_owned))
                .collect();
            if !values.is_empty() {
                arguments.insert(key.to_owned(), values);
            }
        }
    }
}

fn command_at_path<'a>(command: &'a Command, path: &[String]) -> Option<&'a Command> {
    let mut current = command;
    for part in path {
        current = current
            .get_subcommands()
            .find(|child| child.get_name() == part)?;
    }
    Some(current)
}

fn find_leaf<'a>(matches: &'a ArgMatches, path: &mut Vec<String>) -> &'a ArgMatches {
    if let Some((name, nested)) = matches.subcommand() {
        path.push(name.to_owned());
        find_leaf(nested, path)
    } else {
        matches
    }
}
#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{OutputFormat, command};

    fn os_args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn signed_priority_operands_parse_without_accepting_missing_values_or_flags() {
        for args in [
            vec![
                "tmax",
                "model",
                "update",
                "provider/model",
                "--set",
                "priority",
                "-10",
                "--dry-run",
            ],
            vec![
                "tmax",
                "model",
                "schedule",
                "add",
                "provider/model",
                "--priority",
                "-10",
                "--weekday",
                "mon",
                "--all-day",
            ],
            vec![
                "tmax",
                "model",
                "rule",
                "add",
                "provider/model",
                "rule-id",
                "--input",
                "reset",
                "--priority",
                "-10",
                "--reset-within",
                "1d",
            ],
        ] {
            assert!(command().try_get_matches_from(args).is_ok());
        }

        for args in [
            vec![
                "tmax",
                "model",
                "update",
                "provider/model",
                "--set",
                "priority",
                "--dry-run",
            ],
            vec![
                "tmax",
                "model",
                "update",
                "provider/model",
                "--set",
                "priority",
                "--unknown",
                "-10",
            ],
            vec![
                "tmax",
                "model",
                "schedule",
                "add",
                "provider/model",
                "--priority",
                "--weekday",
                "mon",
                "--all-day",
            ],
        ] {
            assert!(command().try_get_matches_from(args).is_err());
        }
    }

    #[test]
    fn global_option_scanning_obeys_values_terminators_and_duplicates() {
        let root = command();
        let (format, duplicate) = OutputFormat::scan_global_options(
            &os_args(&[
                "tmax",
                "--format",
                "text",
                "model",
                "update",
                "provider/model",
                "--set",
                "priority",
                "-10",
            ]),
            &root,
        );
        assert_eq!(format, OutputFormat::Text);
        assert_eq!(duplicate, None);

        let (format, duplicate) = OutputFormat::scan_global_options(
            &os_args(&[
                "tmax",
                "model",
                "update",
                "provider/model",
                "--set",
                "priority",
                "--",
                "--format",
                "text",
            ]),
            &root,
        );
        assert_eq!(format, OutputFormat::Json);
        assert_eq!(duplicate, None);

        let (format, duplicate) = OutputFormat::scan_global_options(
            &os_args(&[
                "tmax", "--format", "json", "provider", "--format", "json", "list", "--format",
                "json",
            ]),
            &root,
        );
        assert_eq!(format, OutputFormat::Json);
        assert_eq!(duplicate, Some("--format"));

        let (format, duplicate) = OutputFormat::scan_global_options(
            &os_args(&[
                "tmax",
                "--non-interactive",
                "provider",
                "--non-interactive",
                "list",
                "--non-interactive",
            ]),
            &root,
        );
        assert_eq!(format, OutputFormat::Json);
        assert_eq!(duplicate, Some("--non-interactive"));
    }
}
