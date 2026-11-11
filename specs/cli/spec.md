# CLI command surface

This specification owns command grammar, offline help, output, and exit categories.
[Product](../product/spec.md) owns behavior; [Configuration](../configuration/spec.md)
owns saved fields and save/reload/apply boundaries; [Model selection](../model-routing/spec.md)
owns routing; [Diagnostics](../diagnostics/spec.md) owns evidence and history.
The command surface is a target contract; see [Implementation status](#implementation-status).

| Capability | Command and result contract |
| --- | --- |
| Configuration and recovery | [Configuration](configuration/spec.md) |
| Integration discovery and provider settings | [Providers](providers/spec.md) |
| Authentication, shared gateway key, and retained secrets | [Authentication](providers/authentication/spec.md) |
| Provider inputs and current data | [Inputs](providers/inputs/spec.md) |
| Concrete models, metadata, and explanation | [Models](models/spec.md) |
| Catalog snapshots | [Catalog](models/catalog/spec.md) |
| Model aliases and candidates | [Model aliases](models/aliases/spec.md) |
| Calendar and signal rules | [Rules](models/rules/spec.md) |
| Agent targets, native provider bindings, application, and recovery | [Agents](agents/spec.md) |
| DSH plugin projection | [DSH projection](dsh/spec.md) |
| Calling-agent entry skill | [Skill installation](skill/spec.md) |
| Network, gateway, and Web | [Service](service/spec.md) |
| Probes and request history | [Diagnostics commands](service/diagnostics/spec.md) |
| Shared JSON and exits | [Results](results/spec.md) |

## Invocation

With no arguments, `--help`, or `-h`, `tmax` prints help to stdout and exits 0.
`--version` and `-V` print the program name and workspace version and exit 0.
Help states the program's purpose. Help/version stay text regardless of format
options and read no configuration, secrets, observations, or network resources.

Management uses `tmax [GLOBAL-OPTIONS] COMMAND [COMMAND-OPTIONS]`. Long global
options may follow a subcommand. Unknown commands, arguments, fields, and flags
are errors. Do not guess commands, ignore options, accept duplicate single-valued
options, or treat a missing value as a default choice. A single-valued global may
appear at most once across the full command path, including parent and leaf levels.

| Global option | Contract |
| --- | --- |
| `--format json` or `--format text` | `--format json` prints one compact JSON line and is the management default. `--format text` prints the same envelope as indented JSON. One valid occurrence is supported before or after subcommands; an invalid or repeated value is rejected in default JSON without echoing argv. |
| `--non-interactive` | Never prompt or launch interactive sign-in. Explicit key input and qualified native authorization/key input remain permitted; missing required interaction is a blocker. |

New API keys use explicit hidden terminal input or `--key-stdin`. See
[configuration recovery](configuration/spec.md#invalid-file-recovery) for invalid-file handling.

All invocations use the OS user's home, including on Windows. All tmax-owned
persistent artifacts remain under `~/.tmax`; the sole config is
`~/.tmax/tmax.toml`. There is no home/config/profile selection from cwd, caller,
environment, or CLI flags. Native files/stores stay native; OS startup registration
is an external integration. Relative native `--path` operands resolve against
`~/.tmax`, not cwd. Results show resolved config and native paths.

Absent config is distinct from invalid config. Reads and dry-runs create neither.
The first successful desired-state edit creates `version = 1` only after checking
for concurrent creation/change. Never replace invalid content with empty or
partially accepted settings; [config reset](configuration/spec.md#invalid-file-recovery)
is the explicit recovery path.

Uppercase words are operands; brackets mark optional syntax; `...` permits repetition.

| Operand | Contract |
| --- | --- |
| `AGENT` | `codex`, `pi`, `omp`, `opencode`, `hermes`, or `dsh` for native-agent management; skill installation accepts only `hermes`, with omission selecting shared `.agents` skills. This is not a restriction on calling agents. |
| `TARGET` | Exact named native installation/profile/configuration scope; context selectors use `--target`. |
| `P` | `chat-completions`, `responses`, or `messages`. |
| `STATE` | `file` or `loaded`; default `file`. |
| `INSTANT` | RFC 3339 with an explicit offset. |
| `DATE` | `YYYY-MM-DD`. |
| `QUALIFIED` | [Qualified model name](../model-routing/spec.md#qualified-model-names). |

IDs are exact and case-sensitive.

## Offline help and capability discovery

`tmax help [COMMAND...]` and `tmax COMMAND... --help` describe the path without
reading settings, secrets, native files, service state, or remote state. Both
forms expose the same inherited `--format` and `--non-interactive` options at a
supported path. Root help groups and lists the capabilities above and their purposes.

Leaf help supplies complete grammar, defaults, required combinations, enums,
effects/non-effects, summary and detail output, exit outcomes, and a valid example.
Editable-field help enumerates accepted paths, scalar types, constraints,
clear/default/inheritance behavior, and dedicated structured operations without
loading config or downloading a schema. Qualified native roles/transports point
to inspection instead of guessed support. Integration examples use actual local
registry IDs and supported choices, not invented IDs or endpoint defaults.

Any calling agent may compose ordinary operations. There is no built-in LLM,
planner, workflow, approval layer, proposal document, recursive schema exchange,
or Web prerequisite. No command infers add/remove/default/start/apply intent.
For example, `tmax help`, `tmax provider add --help`,
`tmax model schedule add --help`, `tmax help model alias add`,
`tmax help agent provider add`, and `tmax help gateway key show` work offline.

## Inspection and observation

Ordinary inspection uses retained facts; it does not resolve secrets or refresh
remote state. Explicit `gateway key show` is the separate sensitive retrieval
operation with protected delivery, never an ordinary status/inspect response.
Integration discovery and native inspection may read qualified local sources
without importing settings or credentials. State-selecting reads report
`data.snapshot_state` as `available`, `absent`, or `unavailable`; a missing loaded
snapshot is not an empty collection. Loaded reads retain the actual loaded view
even when the saved file is invalid, without activating or repairing it.
Local desired-state inspection and configuration previews require no daemon
service, credentials, or network. If daemon discovery fails, return declared
local data with loaded/runtime `unavailable` and its reason. Do not turn unknown
loaded state or affected-session coverage into known absence or an empty complete
set. Explicit runtime, refresh, revocation, and native effects retain their own
blockers and failures under [shared outcomes](results/spec.md#outcomes-and-exit-categories).
Explicit refresh is separate from inspection, sign-in, inference, native import,
and desired-state edits. Its owning capability defines the permitted effects.

## Scalar field operands and defaults

`EDIT` means repeatable `--set FIELD VALUE` and `--clear FIELD`. Each owning
command lists its exhaustive finite paths. `FIELD` is an exact relative schema
path, never an expression, index, wildcard, or new schema language. Help infers
types; callers supply no type tag. Brace notation only abbreviates finite paths
in these specifications.

- Strings are one literal shell operand, without TOML/JSON quoting or environment
  interpolation. Booleans are exactly `true` or `false`. Integers/numbers obey
  schema ranges and finiteness. Signed priority fields accept negative decimal
  operands (for example `--set priority -10` and `--priority -10`); other options
  retain strict flag/operand parsing. Dates, enums, URLs, and zones obey their schemas.
- Clear removes an optional field/object and restores its documented default,
  inheritance, or unknown fact. It does not write null, zero, false, or an empty
  string. Required fields cannot be cleared into invalid state.
- False, zero, and permitted empty strings are explicit overrides. Dedicated list
  replacement can write an empty list; clearing the override restores inheritance.
- Apply clears, then sets, as one change regardless of argv order. Reject duplicate
  field operands and conflicting assignments. Clearing a parent then setting its
  children permits an explicit atomic rebuild or union-representation change.
- Validate required field groups together. Preserve unmentioned fields and local
  siblings; inherited siblings use the schema's recursive merge. Do not copy
  effective inherited arrays into overrides. Empty optional containers may be
  removed; explicit empty metadata arrays stay distinct from absent overrides.
- Structured/list edits use dedicated operations, never serialized scalar blobs.
  Whole-object/list replacement occurs only when the command explicitly says so.

Array indices are zero-based local saved positions shown with resource identity
and file revision, not stable IDs or reservations. Optional `--if-revision REVISION`
requires exact current file-content revision; `absent` requires an absent file.
A mismatch returns `inputs_changed` before effects, including protected input or
sign-in. Dry-run enforces it too. Omitting it does not disable in-operation checks.
Indexed `model reasoning replace|remove` and `model schedule replace|remove`
require the guard. A successful intervening write requires a new current revision;
named rule/input IDs do not require a preflight read.

### Duration operands

`DURATION` is a positive integer followed by exactly one lower-case unit: `s`,
`m`, `h`, or `d`. These elapsed units are exactly 1, 60, 3600, and 86400 seconds,
not calendar-day or DST arithmetic. Accept `30s`, `15m`, `6h`, or `1d`, not zero,
bare numbers, fractions, signs, compound values, expressions, or upper-case units.
Use checked exact conversion within the destination field's existing integer
range. Persist the existing `*_seconds` fields. This grammar applies to
`--reset-within`, `--max-age`, and `--refresh-every`, not dates/calendar windows.

## Dry-run and execution

`--dry-run` is optional on the same mutation, refresh, lifecycle, paid probe, or
explicit gateway-key retrieval with the same semantic operands. Ordinary effect-free
reads/explanations reject it.
It reports full redacted changes, affected scopes/sessions, boundaries, warnings,
known blockers, and deferred checks. It must not write files/state/log/cache or
recovery records, reserve references/record sets, generate keys, read protected
input, resolve saved secrets, refresh/sign in, launch native programs/browsers,
change services/OS registration, call upstream, or run inference. Safe existing
local config/ownership/runtime reads are permitted.

Direct execution needs no preview, approval, or stored plan. It checks current
inputs and performs only the requested operation. Protected input and explicitly
selected sign-in may require qualified interaction; `--non-interactive` controls
that interaction, not permission to mutate. Dry-run is no reservation, execution
token, or proof of credentials/upstream success. Every later invocation recomputes.

Execution captures relevant file/metadata revisions, native inputs/ownership,
credential source/generation and safe identity, input declaration/current-data
revisions, service/admission/history state, recovery identity, and startup intent.
Recheck required inputs under serialization immediately before each effect.
Compare content/identity, not only timestamps. Missing is not empty. Reject a
relevant concurrent change before overwrite; never silently rebase, expand,
force, or automatically retry. Unrequested registration drift does not invalidate
an unrelated edit.

Before-effects conflicts return `blocked`/`inputs_changed` with a supported next
action. Later failures disclose completed, partial, and uncertain effects; there
is no cross-boundary atomicity promise for login, secrets, save, activation, or
native writes. Native sign-in can change a source's account before local commit; preserve
[authentication failure reporting](providers/authentication/spec.md#provider-authentication-and-local-secrets).
Management operations have no automatic retry/replay engine or human-policy
escalation. Inference attempts follow explicit
[gateway fallback policy](../model-routing/spec.md#request-fallback-and-failure-priority-override).

## Implementation status

The command surface is implemented by `tmax-cli`, with shared semantic validation
and operations in `tmax-core`. Help/version remain offline text; management
commands use the shared result envelope and explicit effect boundaries. Native
and provider qualification remain independently scoped; this document does not
claim that every external integration has passed end-to-end verification.
