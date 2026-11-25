# Agent target commands

This child owns target declaration, provider bindings/selections, Agent
inspection/application, and recovery grammar. [Product](../../product/spec.md#agent-provider-configuration)
and [agent integrations](../../integrations/spec.md) own capabilities, Agent
ownership, and application safety. [Configuration](../../configuration/spec.md)
owns saved references. Use [common CLI rules](../spec.md),
[configuration changes](../configuration/spec.md), and [shared results](../results/spec.md).

## Agent declarations, selections, defaults, and roles

| Command | Requested change |
| --- | --- |
| `agent add TARGET --agent AGENT --path PATH` | Declare a target with no provider entries, default, or roles. No gateway/key provisioning or actual-file writes. |
| `agent update TARGET [--agent AGENT] [--path PATH]` | Edit explicit structural fields; at least one is required. |
| `agent rename OLD NEW` | Rename the target/dependent login-source references while retaining ownership/recovery identity. |
| `agent remove TARGET` | Stop managing it after reference checks; leave actual files/credentials/recovery and shared gateway access untouched. |
| `agent model add TARGET NAME ID --protocol P [--force]` | Add one exact model to the named provider entry, not its default or role. The selection is judged where it is chosen: if the Agent's configuration cannot take it, the add is refused with that code and the two ways forward, and nothing is written. `--force` saves it anyway and reports `forced_blocker` naming what application will refuse, so the refusal never arrives later as a surprise. A missing credential is not a selection fact: adding stays possible before the key is configured. |
| `agent model remove TARGET NAME ID --protocol P` | Remove after default/role reference checks. |
| `agent model clear TARGET NAME` | Empty this entry's selections only after its default/role references are cleared. |
| `agent default set TARGET NAME ID --protocol P [--effort EFFORT \| --native-effort EFFORT]` | Choose an existing selection of that provider entry as default, optionally declaring the reasoning effort tmax writes. Omit both options to stop managing a previously configured effort. |
| `agent default clear TARGET` | Remove the default, preserving entries/selections/roles. |
| `agent role set TARGET ROLE NAME ID --protocol P` | Map one qualified model role to an existing selection of that entry. |
| `agent role remove TARGET ROLE` | Remove exactly that model-role mapping. |
| `agent role clear TARGET` | Remove all model-role mappings, not selections/default. |

| Target operand | Contract |
| --- | --- |
| `--agent AGENT` | Select the tool and save `agents.<TARGET>.agent`; supported values follow [Invocation](../spec.md#invocation). |
| `--path PATH` | Save `scope`; resolve relative paths through [Invocation](../spec.md#invocation). Provider input `--scope` is unchanged. |
| `NAME` | Exact target-local provider-entry name, not another tmax upstream provider ID. |
| Default/role reference | `{provider, id, protocol}`, where `provider` is `NAME`. A default may also carry one `effort` (caller-facing; DSH, pi, OMP, OpenCode) or one `native_effort` (verbatim; Codex, Hermes), never both. A role carries no effort. |
| `ROLE` | Adapter-supported model role, not an access role, universal LLM role, or inference setting. Inspection lists supported names; reject unknown names. |
| `--effort EFFORT` | Caller-facing model effort name for DSH, pi, OMP, or OpenCode. It must be nonempty without whitespace, declared by the selected model's effective reasoning options, and representable by the target's `native_projection`; otherwise application blocks before writes. For OpenCode it must not contain `#`, which delimits the composite model reference, and saving rejects it. |
| `--native-effort EFFORT` | Verbatim native reasoning effort for Codex or Hermes. It must be nonempty without whitespace; tmax writes it without validating a selector. Refuse it for Agents that expose a validated selector. |

Every present source/model/default/role/login-source reference must resolve. Empty
provider tables/model lists and an absent default are valid incremental setup,
not a draft/wizard state. An incomplete target may be a qualified login source.
Incomplete configuration, credentials, or qualification block Agent application,
not independent gateway inference. The first selection never becomes default.
Clear default/roles explicitly before removing referenced selections; no cascade.
Continuity is edited on the [alias definition](../models/aliases/spec.md), never
on a target, provider entry, default, or role.

## Agent provider bindings

| Command | Requested change |
| --- | --- |
| `agent provider add TARGET NAME --provider PROVIDER` | Create an entry that references one configured provider and has an empty model list. |
| `agent provider update TARGET NAME --provider PROVIDER` | Replace the referenced provider, validating the complete resulting entry and target references. |
| `agent provider remove TARGET NAME` | Remove the entry after default/role and other reference checks; do not write actual files or revoke gateway access. |

`--provider` is required and names one configured provider ID; `tmax` names the
derived [gateway provider](../../product/spec.md#gateway-provider). There is no
second selector and no fallback. Reject a missing or unknown provider and duplicate
entry names. Provider replacement validates every existing selected ID/protocol
against the new provider and reports affected selections/default/roles. Reject
unresolved IDs; never reinterpret, repair, clear, or add them implicitly. Existing
valid selections may remain; there is no extra empty-list precondition.

For a direct entry, `ID` is the exact upstream model ID under the referenced
provider, without a redundant provider prefix. For a relayed entry, it is a public
route name that declares the selection's protocol, without an adapter's native
display prefix.
Protocol is explicit and establishes no conversion or compatibility. Prefer
Responses when the upstream declares it and the native client supports it, under
the [client-owned protocol rule](../../integrations/spec.md#endpoint-resolution).
Direct and relayed entries may coexist in one target. Namespace conflicts or lack
of qualified multi-provider support block application; never omit an entry
silently.

Saving any declaration, binding, or selection writes expected configuration only:
it does not start inference, apply actual files, import login, or provision a
shared key. Saving changes the target revision; `agent inspect` reports the
target as `unapplied` whenever the current target revision differs from the
applied target revision. It never creates a native pending change, merge state,
or accept/discard step. Removing an entry leaves its actual
files until explicit apply or restore. Agent selections control configuration/model pickers only, never gateway
publication or request access. All holders of the current shared key use the same derived model
list; no target registration or Agent binding is required for standalone clients.
Agent/path edits do not restore the old location or import credentials. Recovery
keeps its original path; login-source generation changes follow
[authorization replacement](../providers/authentication/spec.md#authorization-replacement).

```sh
tmax provider add deepseek --set metadata deepseek
tmax provider key set deepseek --key-stdin
tmax model add deepseek/deepseek-flash
tmax model alias add L0 --protocol chat-completions --candidate deepseek/deepseek-flash
tmax gateway configure --listen 127.0.0.1:17840
tmax gateway start
tmax agent add pi-main --agent pi --path /home/user/.pi/agent
tmax agent provider add pi-main upstream --provider deepseek
tmax agent model add pi-main upstream deepseek-flash --protocol chat-completions
tmax agent provider add pi-main local --provider tmax
tmax agent model add pi-main local L0 --protocol chat-completions
tmax agent default set pi-main upstream deepseek-flash --protocol chat-completions
tmax agent apply pi-main --dry-run
tmax agent apply pi-main
tmax agent inspect pi-main
```

This illustrates grammar, not verified integration. It assumes the catalog entry,
current-user-restricted tmax secret file or explicit protected key input, a
qualified configuration path, and support for the complete mixed configuration.
Missing prerequisites and the Agent reload stay explicit. Inspect is not proof of paid inference.
Help uses supported model roles, not invented ones.

## Agent application and recovery

| Command | Result or requested boundary |
| --- | --- |
| `agent list` | Compact target IDs/tool identities, resolved paths, provider/model counts, and `default_configured`. |
| `agent discover [--agent AGENT]` | Supported installations/paths, versions, profiles, and read-only `native_default`, without registration/import. |
| `agent inspect TARGET` | Actual entries and their configured provider source, relayed delivery facts, precedence, supported roles/requirements, effective auth/model use, `native_projection`, expected/applied/effective state, the target revision, the applied target revision, the `unapplied` flag, blockers, and recovery. |
| `agent inspect --agent AGENT --path PATH` | Inspect an explicit undeclared configuration path without registration/import. |
| `agent apply TARGET` | Apply expected configuration authoritatively to the target's actual files: overwrite or remove managed fields and entries from expected configuration, leave unmanaged fields alone, retain one managed snapshot for restore, and record the target revision it applied. Report affected files, applied state, and the retained snapshot. |
| `agent restore TARGET` | Restore the target's one managed snapshot, captured immediately before its last successful apply; report affected files and record state. The target reports `unapplied` until a later successful apply. |
| `agent restore --recovery RECORD` | Restore exactly that record, including after removal, without recreating a target. |
| `agent recovery list [--target TARGET]` | Recovery handles/paths under current-user-restricted `~/.tmax`, including removed targets, with record state and restore availability, and no credentials. |
| `agent recovery inspect RECORD` | Ownership, target identity, affected files, and remaining supported actions. |

Apply and restore support dry-run; reads do not. Recovery handles are not
target IDs; reused IDs do not select predecessor records. Exact-handle recovery
works from the sole home with absent/invalid config without activating it.
Restoring a record does not recreate a target or delete credentials.

Apply rejects missing required selections or unavailable credentials and known
incompatible schemas. Pi, OMP, OpenCode, Hermes, and DSH do not require a default
selection: applying model entries alone writes no defaults or unselected roles.
A default's model and effort are independent fields: an absent effort preserves
the Agent's current effort, including inside the same actual entry or object, and
a configured effort is written only for the selected default model. The apply
projection validates the configured effort against the selected model's
`native_projection` and blocks on an unrepresentable value instead of dropping,
translating, or downgrading it; that apply returns
`native_default_effort_unrepresentable` with exit 3 before writes. An explicit `native_effort` is passed through
only for Codex and Hermes. OpenCode encodes model and effort in one composite
reference: removing only the effort keeps the current composite value, while
clearing or changing the default rewrites that reference.
For DSH, preservation applies to HOME-level settings, not automatic import of
shadowed profile configuration. Separate settings files are not created or edited without selected
defaults/roles. Codex requires a default because its review model shares the
active provider. Each target receives its own credential handoff in its
configuration scope; DSH uses the target home's adjacent `.credentials.yaml`,
shared by profiles using that home, not another tool's credential file. Hermes
uses its `api_key` field in current-user-restricted `config.yaml`; inspection and
preview disclose the inline plaintext handoff without returning the value. Apply
removes competing key commands/environment references only from the selected
providers, retaining them for explicit restoration. Explicit apply can manage a
selected provider name already present in the actual configuration: preview
discloses the existing entry, managed fields are written authoritatively, and
unmanaged fields remain intact. It does not import actual settings into expected
configuration. A field or file another tmax target owns, and a foreign provider
name that tmax does not own, remain blockers before any write.
DSH uses the profile's own settings document, under the
[DSH home boundary](../../integrations/spec.md#dsh-home-boundary). A provider is a
profile-level setting in DSH, so apply writes `profiles/<profile>/cordis.patch.yml`
row `llm-pi-ai`, path `config.providers.<entry>`, with the endpoint, the
protocol's `api`, the model list, and a credential reference; the base bundle's
adapter loads it, so nothing has to be installed first. Apply selects the profile
by `--profile`, otherwise `desktop`, then `web`, and reports the choice. It never
creates profiles, edits presets or bundle composition, or touches the launcher-owned
`cordis.yml`. Discovery uses nonblank `DSH_HOME`, otherwise the OS home directory's
`.dsh`; an explicit scope selects the home directly. The home `cordis.patch.yml` is
never written.
User `llm-pi-ai` configuration stays independent and untouched. Every adapter's
provider configurations register in DSH's common LLM runtime, so selected provider
names must be globally unique; tmax does not automatically merge colliding names.
Removing a previously managed DSH route removes its managed fields; unmanaged
settings and containers remain as unmanaged. A route left with unmanaged content
does not block the apply.
Direct and relayed entries may coexist. Existing HOME-level fields and
expressions remain preserved under the owning boundary. Model-only application
writes no `agent-default-model` entry or reasoning preferences; an explicit
default writes provider/model and, only when configured, one validated effort,
while an absent effort retains the existing HOME-level setting. Credential handoff writes the referenced value into the credential document the
base bundle's credential service already reads, so it resolves for headless
consumption without further configuration. Recovery covers the profile rows tmax
wrote, the credential references, and any legacy artifact it retires. The `tmax` entry is the
only managed adapter entry; no earlier adapter application format is supported or
automatically migrated. Runtime overlays remain outside tmax's ownership.

Direct entries use their referenced endpoints/auth and need no running tmax
gateway or model aliases. Relayed entries reference provider `tmax` and use its
derived local URL and the current shared key, requiring the matching ready
listener and loaded snapshot. Start explicitly first. Applying both kinds
preserves precedence, supported credential handoff, and disclosure of any
required plaintext in actual files. Never copy
upstream login tokens to create another refresh maintainer. Apply is authoritative
over tmax-managed fields: a later edit to one is overwritten by the next apply,
unmanaged fields stay, and partial write or reload state is reported. Restore is
single-level from the retained managed snapshot and also overwrites later edits to
managed fields. Apply records the target revision it applied; restore retains
that record and reports the target as `unapplied` until a later successful apply
captures the current revision. Recovery
is stored as plaintext under current-user-restricted `~/.tmax`; file permissions
are not encryption. There is no automatic application and no merge mode.

Unsupported-format recovery records are preserved and reported as unavailable;
current operations do not restore their contents.

Shared [gateway key replacement](../providers/authentication/spec.md#shared-gateway-key)
reports known Agent dependencies without implicitly applying them; affected
entries need explicit apply and Agent reload. Removing a target/entry or
restoring the retained snapshot never revokes the shared key. Stop management with
`agent remove`; restore first if wanted, or use retained recovery later.

## Agent result data

| Agent result data | Shape and meaning |
| --- | --- |
| Target/tool identity | `target` is the declared target ID; `agent` is its tool identity, never a target ID. An undeclared configuration scope has null `target`; resolved `path` identifies the inspected scope. |
| `NativeModelRef` | `{provider, id, protocol}` with a target-local provider name; a default adds optional `effort` or `native_effort`. |
| `AgentProviderView` | `name`, `source` (`{provider: string}`), `relayed`, and exact `{id, protocol}` selections. A relayed entry also reports `base_url`, `credential_ref` (DSH target with one selected protocol only), and `applied_delivery`; a direct entry has null values for those fields. |
| Provider detail | Qualified requirements, readiness, actual precedence/evidence, and expected/applied/effective state; no mode, target credential, permission list, or target continuity field. |
| `configuration_revision` | `agent inspect` returns the current saved configuration revision at the top level; null when unavailable. It covers the whole saved file and is not the target revision or an applied revision. |
| `target_revision` | A stable revision of that target's expected configuration alone, comparing equal across unrelated target changes and configuration-file formatting. It is not the saved configuration revision, an actual-file hash, or an applied revision. |
| `applied_target_revision` | The target revision captured by the retained managed record's last successful apply; null when no record is retained or the record captured no revision. A null value never means the target is current. |
| `unapplied` | False only when the retained managed record represents completed actual configuration and its captured target revision equals `target_revision`; true when expected configuration has advanced, when no record is retained, or when the record represents an incomplete write, a recovery-required state, or a restore. Null when the scope is unavailable and the comparison cannot be made; a null value is unknown, never false. The owed Agent reload is separate. It reports a target-level desired-versus-applied comparison, not a native pending change, merge, per-field difference, or count of model items. |
| `applied_delivery` | Relayed entry field comparing the applied delivery facts with the current ones: `state` (`current`, `stale`, `blocked`, `unknown`, `not_applied`, or `not_configured`), `revision`, `applied_revision` when recorded, named `reasons`, and `routes.removed`. Reasons are not merged: `listen_changed`, `key_rotated`, `route_removed`, and `entry_selection_changed` make the state `stale`; `entry_references_missing_route` and `gateway_not_ready` make it `blocked` when no drift is present, and an apply whose writes did not finish is `blocked` with `apply_incomplete`. The recorded revision is target-scoped over the `route/protocol` pairs that target references, so an unrelated added route changes neither the revision nor the state, while a changed selection protocol is drift. An unknown or not-applied state names its `reason_code` (`delivery_not_recorded` or `never_applied`). |
| `agent inspect` | `data.providers` contains entries; `data.default` uses `NativeModelRef` with optional `effort` or `native_effort`, `data.roles` use model-only `NativeModelRef`, and a null default means unmanaged. |
| `readiness` | `agent inspect` reports one row per dimension a target's blockers touch, plus the dimensions that are fine: `ready` is true only when nothing blocks, `blocked` lists `{dimension, code, message, next_action}` in the order a reader meets them (`scope`, `native_installation`, `plugin`, `gateway`, `credential`, `compatibility`, `selection`, then any other), and `dimensions` lists every row's `state`. A dimension carries its first blocker; the `blockers` array beside it carries them all. `next_action` is the action that clears that dimension: a runnable command where one exists, otherwise one sentence naming what the reader must do, and null only where no action exists (`scope`, which the reader selects). |
| `environment` | `agent apply` reports the environment facts it evaluated in both a dry run and a blocked run: `native`, the client's installation evidence, for an Agent tmax generates against a version floor. Environment obstacles are collected together and returned in one run with `blockers` in a fixed order, so a later obstacle is not hidden behind an earlier one; `_code` is the first of them. |
| `native_default` | `agent inspect` reports the tool's currently persisted default selection read-only: `state` (`available` or `unavailable` with `reason_code`) and `fields` carrying `present`, `provider`, `model`, and `effort` when the actual file exposes them. It is inspection evidence, never expected configuration and never imported automatically. |
| `native_projection` | `agent inspect` and `agent apply --dry-run` return one entry per provider entry: `name`, `source` (`{provider: string}`), and `models` of `{id, protocol, capabilities, efforts, projection}`; `capabilities` carries the model's safe capability metadata, and `efforts` lists the effort names representable for that model and target, or is empty. A `projection` has `state`, `reason_code`, `fields`, and `omitted`; a blocked projection adds `message`. `fields` carries only safe capability metadata and never secrets. |
| Lists | Target/tool identities, counts, and readiness summaries instead of complete entries/evaluations; `default_configured` reports whether the target has a configured default. |

Projection semantics follow the shared
[capability mapping](../../integrations/spec.md#agent-boundaries) boundary.
`native_projection` is the expected projection and is reported even when the
gateway is not running. Application instead requires a ready gateway listener.
Direct entries retain actual upstream declarations. Relayed entries use the same
states below after the shared
[alias reasoning capability](../../integrations/spec.md#alias-reasoning-capability)
rule intersects all candidate input sets after maps and fixed restrictions. No
provider entry receives a gateway translation map, default, or thinking switch.

| Projection `state` | `reason_code` and meaning |
| --- | --- |
| `projected` | Null `reason_code`; `fields` carries the projected native capability fields. |
| `partial` | `native_reasoning_options_partial`; `fields` carries the representable exact names and `omitted` lists the declared names the native schema cannot express. |
| `not_supported` | `native_reasoning_options_unavailable`; the agent has no native reasoning-effort selector and `fields` is empty. |
| `unavailable` | `reasoning_disabled` (explicit concrete-model `reasoning = false`), `reasoning_options_unavailable` (no declared effort names or no common alias effort names), or `native_projection_unavailable` (no projection for this model); `fields` is empty. |
| `blocked` | `native_metadata_unrepresentable` with `message` explaining the unrepresentable capability; the error never fails inspection and blocks only that agent's application before writes. |

Each projection also has an independent `input` projection with `state`,
`reason_code`, `fields`, and `omitted`, and optional `message`. Its states are
`projected`, `partial`, `unavailable` (unknown declaration/native inheritance),
`not_supported` (no qualified native input field), or `blocked` (the declaration
cannot be represented without changing its meaning). Top-level effort state
does not conceal or override this input state. A blocked input projection blocks
application before writes, but remains inspectable.

Each projection also has an independent `limits` projection with `state`
(`projected`, `partial`, or `unavailable`), `reason_code` (`limits_unknown`
unless projected), and `fields` (`contextWindow`/`maxTokens`, null when that
limit has no catalog or declared fact). Unknown limits are never invented. An
alias selection carries a limit only when every candidate's effective facts
declare the same value; disagreement or any gap leaves that alias limit unset.

`fact_gaps` accompanies `native_projection` in `agent inspect`, `agent apply`
results, and `agent apply --dry-run`: one entry per model whose reasoning
efforts or context limit have no catalog or declared fact, carrying `entry`,
`model`, `missing` (`reasoning_efforts`, `limit.context`), and
`agent_self_completion` (`none` for static configuration surfaces such as DSH,
`undocumented` elsewhere). It reports that the actual entry stays bare for the
missing fields; it does not block application. `model add` returns the same
`fact_gap` object for the added model when the saved catalog snapshot supplies
neither fact.


A null `native_projection` means planning failed or is unavailable; `blockers`
remain authoritative.

Agent inspection reports `scope_state` separately from the saved target. When a
declared path is absent or unusable, it retains the target and resolved `path`,
sets `scope_state: unavailable`, and provides `scope_reason_code` (`absent`,
`not_directory`, `unreadable`, or `unavailable`); actual apply remains blocked.
`applied_state` is record-derived: `unknown` when the scope is unavailable,
`not_applied` when no tmax-managed record is retained, and otherwise the retained
record's state (for example `applied_reload_pending`, `write_pending`, or
`recovery_required`). It does not compare expected configuration against the
current files. The separate `unapplied` flag compares the target revision with
the revision retained by the last successful apply and treats a retained record
without completed actual configuration as unapplied; it is not a native pending
change, merge, or per-field difference, and no `pending_native_changes` field is
produced. `awaiting_native_reload` is
separate: it means written changes await a tool reload/restart.

Installation evidence consists of `state`, `version`, `executable`, and `source`,
with states `not_installed`, `probe_failed`, `version_unknown`, and `detected`.
When the executable is absent, evidence consists only of `state: not_installed`
and `version: null`. If a declared target's inspection scope is unavailable,
installation is not checked and reports `state: not_checked`, `version: null`,
and reason code `native_scope_unavailable`.

Applying configuration is a declared-target file edit and does not require or
probe the tool executable, except for OpenCode schema detection and the DSH
version floor. A positively detected OpenCode 1.x schema reports
`native_schema_incompatible`, exit 3, and no writes. A positively detected
DSH older than the one qualified version reports `native_version_unqualified` with
the detected and qualified versions, exit 3, and no writes; a dry run
reports the same blocker instead of ready. A DSH home with no `desktop` or `web`
profile reports `unknown_resource` naming the profiles it has, exit 3, and no
writes, in a dry run as well. tmax supports only that qualified DSH
version and newer; `package.json` `engines.dsh` refuses an older DSH at
installation. Missing or unknown versions do not block application. Apply
reports the environment facts it evaluated beside those blockers (`environment`,
and the legacy `installation`/`plugin` keys for the same values) but emits no
version warning of its own; neither `native_version_probe_failed` nor
`native_executable_not_found` is emitted.
Operations that execute the tool's own commands, such as browser login, require
the executable and report `native_not_installed` when it is absent.

Successful apply/restore returns exit 0 and status `success`, including when its
code is `awaiting_native_reload`. The reload reminder remains explicit; a
completed file write is not a failed application.

Successful apply results identify the target and agent, applied state, changed
and unchanged files, recovery, and fact gaps; a write also reports its path and
`written`. Apply dry-run reports unresolved credentials without reading secrets
or starting processes. A schema-blocked result identifies the target, agent,
path, blockers, and effects. Partial writes enumerate changed/unchanged
files and safe recovery; a completed write with a reload reminder is a write
outcome, not a claim about the running tool. Detail keeps expected configuration,
the retained applied record, and effective Agent state distinct. Incomplete
effects stay visible.

Examples: `tmax agent discover --agent pi`, `tmax agent inspect pi-main`,
`tmax agent inspect --agent pi --path /home/user/.pi/agent`,
`tmax agent restore pi-main`, `tmax agent recovery list --target pi-main`,
`tmax agent recovery inspect RECORD`, and `tmax agent restore --recovery RECORD`.
Use returned recovery handles; inspect retained recovery instead of replaying
stale writes.
