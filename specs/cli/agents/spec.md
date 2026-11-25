# Agent target commands

This child owns target declaration, native provider bindings/selections, native
inspection/application, and recovery grammar. [Product](../../product/spec.md#agent-provider-configuration)
and [agent integrations](../../integrations/spec.md) own capabilities, native
ownership, and application safety. [Configuration](../../configuration/spec.md)
owns saved references. Use [common CLI rules](../spec.md),
[configuration changes](../configuration/spec.md), and [shared results](../results/spec.md).

## Agent declarations, selections, defaults, and roles

| Command | Requested change |
| --- | --- |
| `agent add TARGET --agent AGENT --path PATH` | Declare a native target with no provider entries, default, or roles. No gateway/key provisioning or native writes. |
| `agent update TARGET [--agent AGENT] [--path PATH]` | Edit explicit structural fields; at least one is required. |
| `agent rename OLD NEW` | Rename the target/dependent login-source references while retaining ownership/recovery identity. |
| `agent remove TARGET` | Stop managing it after reference checks; leave native files/credentials/recovery and shared gateway access untouched. |
| `agent model add TARGET NAME ID --protocol P [--force]` | Add one exact model to the named native provider entry, not its default or role. The selection is judged where it is chosen: if this native handoff cannot take it, the add is refused with that code and the two ways forward, and nothing is written. `--force` saves it anyway and reports `forced_blocker` naming what application will refuse, so the refusal never arrives later as a surprise. A missing credential is not a selection fact: adding stays possible before the key is configured. |
| `agent model remove TARGET NAME ID --protocol P` | Remove after default/role reference checks. |
| `agent model clear TARGET NAME` | Empty this entry's selections only after its default/role references are cleared. |
| `agent default set TARGET NAME ID --protocol P [--effort EFFORT \| --native-effort EFFORT]` | Choose an existing selection of that native provider entry as default, optionally declaring the reasoning effort tmax writes. Omit both options to stop managing a previously configured effort. |
| `agent default clear TARGET` | Remove the default, preserving entries/selections/roles. |
| `agent role set TARGET ROLE NAME ID --protocol P` | Map one qualified native role to an existing selection of that entry. |
| `agent role remove TARGET ROLE` | Remove exactly that native mapping. |
| `agent role clear TARGET` | Remove all native mappings, not selections/default. |

| Native operand | Contract |
| --- | --- |
| `--agent AGENT` | Select the tool and save `agents.<TARGET>.agent`; supported values follow [Invocation](../spec.md#invocation). |
| `--path PATH` | Save `scope`; resolve relative paths through [Invocation](../spec.md#invocation). Provider input `--scope` is unchanged. |
| `NAME` | Exact target-local native provider name, not another tmax upstream provider ID. |
| Default/role reference | `{provider, id, protocol}`, where `provider` is `NAME`. A default may also carry one `effort` (caller-facing; DSH, pi, OMP, OpenCode) or one `native_effort` (verbatim; Codex, Hermes), never both. A role carries no effort. |
| `ROLE` | Adapter-supported native model role, not an access role, universal LLM role, or inference setting. Inspection lists supported names; reject unknown names. |
| `--effort EFFORT` | Caller-facing model effort name for DSH, pi, OMP, or OpenCode. It must be nonempty without whitespace, declared by the selected model's effective reasoning options, and representable by the target's native projection; otherwise application blocks before native writes. For OpenCode it must not contain `#`, which delimits the composite model reference, and saving rejects it. |
| `--native-effort EFFORT` | Verbatim native reasoning effort for Codex or Hermes. It must be nonempty without whitespace; tmax writes it without validating a selector. Refuse it for Agents that expose a validated selector. |

Every present source/model/default/role/login-source reference must resolve. Empty
provider tables/model lists and an absent default are valid incremental setup,
not a draft/wizard state. An incomplete target may be a qualified login source.
Incomplete configuration, credentials, or qualification block native application,
not independent gateway inference. The first selection never becomes default.
Clear default/roles explicitly before removing referenced selections; no cascade.
Continuity is edited on the [alias definition](../models/aliases/spec.md), never
on a target, native provider entry, default, or role.

## Native provider bindings

| Command | Requested change |
| --- | --- |
| `agent provider add TARGET NAME --provider PROVIDER` | Create an entry that references one configured provider and has an empty model list. |
| `agent provider update TARGET NAME --provider PROVIDER` | Replace the referenced provider, validating the complete resulting entry and target references. |
| `agent provider remove TARGET NAME` | Remove the entry after default/role and other reference checks; do not write native files or revoke gateway access. |

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
Direct and relayed entries may coexist in one target. Native namespace conflicts
or lack of qualified multi-provider support block application; never omit an
entry silently.

Saving any declaration, binding, or selection writes desired state only: it does
not start inference, apply native files, import login, or provision a shared key.
Removing an entry leaves its native files until explicit apply/restore. Native
selections control configuration/model pickers only, never gateway publication or
request access. All holders of the current shared key use the same derived model
list; no target registration or native binding is required for standalone clients.
Agent/path edits do not restore the old location or import credentials. Recovery
keeps its actual original path; login-source generation changes follow
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
current-user-restricted tmax secret file or explicit protected key input, qualified
native path, and support for the complete mixed configuration. Missing
prerequisites/native reload stay explicit. Inspect is not proof of paid inference.
Help uses supported native roles, not invented ones.

## Agent application and recovery

| Command | Result or requested boundary |
| --- | --- |
| `agent list` | Compact target IDs/tool identities, resolved paths, provider/model counts, and `default_configured`. |
| `agent discover [--agent AGENT]` | Supported installations/paths, versions, profiles, and read-only `native_default`, without registration/import. |
| `agent inspect TARGET` | Native entries and their configured provider source, relayed delivery facts, precedence, supported roles/requirements, effective auth/model use, native capability projection, desired/applied differences, blockers, and recovery. |
| `agent inspect --agent AGENT --path PATH` | Inspect an explicit undeclared native path without registration/import. |
| `agent apply TARGET` | Apply current complete desired configuration; report affected files, applied state, and recovery. |
| `agent restore TARGET` | Restore owned changes using the declared target's retained recovery. |
| `agent restore --recovery RECORD` | Restore exactly that record, including after removal, without recreating a target. |
| `agent recovery list [--target TARGET]` | Recovery handles/paths under current-user-restricted `~/.tmax`, including removed targets, with restoration/conflict state and no credentials. |
| `agent recovery inspect RECORD` | Ownership, target identity, affected files, and remaining safe actions. |
| `agent recovery discard RECORD` | Delete that recovery information without native writes or credential deletion. |
| `agent recovery accept TARGET\|--recovery RECORD` | Re-baseline one retained record's owned fields to the current native values: no native write, no credential deletion, credential ownership retained; later applies proceed from the accepted state and restore returns to it. |

Apply/restore/discard/accept support dry-run; reads do not. Recovery handles are not
target IDs; reused IDs do not select predecessor records. Exact-handle recovery
works from the sole home with absent/invalid config without activating it.
Accepting after a tool rewrote its own configuration is the explicit
non-destructive alternative to restore and discard.

Apply rejects missing required selections or unavailable credentials and known
incompatible schemas. Pi, OMP, OpenCode, Hermes, and DSH do not require a default
selection: applying model entries alone writes no defaults or unselected roles.
A default's model and effort are independent fields: an absent effort preserves
the tool's current effort, including inside the same native entry or object, and
a configured effort is written only for the selected default model. The apply
projection validates the configured effort against the selected model's native
projection and blocks on an unrepresentable value instead of dropping,
translating, or downgrading it; that apply returns
`native_default_effort_unrepresentable` with exit 3 before native writes. An explicit `native_effort` is passed through
only for Codex and Hermes. OpenCode encodes model and effort in one composite
reference: removing only the effort keeps the current composite value, while
clearing or changing the default rewrites that reference.
For DSH, preservation applies to HOME-level settings, not automatic import of
shadowed profile configuration. Separate settings files are not created or edited without selected
defaults/roles. Codex requires a default because its review model shares the
active provider. Each target receives its own credential handoff in its native
scope; DSH uses the target home's adjacent `.credentials.yaml`, shared by native
profiles using that home, not another tool's credential file. Hermes uses its native
`api_key` field in current-user-restricted `config.yaml`; inspection and preview
disclose the inline plaintext handoff without returning the value. Apply removes
competing key commands/environment references only from the selected providers,
retaining them for explicit restoration. Explicit apply can manage a selected
provider name already present in native configuration: preview discloses the
existing entry, changed fields are captured for restore, and unrelated fields
remain intact. It does not import native settings into desired configuration.
Conflicting ownership by another tmax target and later edits to owned fields
remain blockers.
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
provider configurations register in DSH's common LLM runtime, so selected native
names must be globally unique; tmax does not automatically merge colliding names.
Removing a previously managed DSH route restores its captured baseline and removes
introduced empty containers, not an invalid empty provider registration. Unowned
settings that keep a newly introduced route in use block before native writes.
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
preserves native precedence, supported credential handoff, and disclosure of any
required plaintext in native files. Never copy
upstream login tokens to create another refresh maintainer. Native writes preserve later user
edits, check ownership/conflicts, and report partial write/reload state. Recovery
is stored as plaintext under current-user-restricted `~/.tmax`; file permissions
are not encryption. No force overwrite or automatic application exists.

Unsupported-format recovery records are preserved and reported as unavailable;
current operations do not restore or discard their contents.

Shared [gateway key replacement](../providers/authentication/spec.md#shared-gateway-key)
reports known native dependencies without implicitly applying them; affected
entries need explicit apply and native reload. Removing a target/entry or
restoring native settings never revokes the shared key. Stop management with
`agent remove`; restore first if wanted, or use retained recovery later.

## Agent result data

| Agent result data | Shape and meaning |
| --- | --- |
| Target/tool identity | `target` is the declared target ID; `agent` is its tool identity, never a target ID. An undeclared native scope has null `target`; resolved `path` identifies the inspected scope. |
| `NativeModelRef` | `{provider, id, protocol}` with a target-local provider name; a default adds optional `effort` or `native_effort`. |
| `AgentProviderView` | `name`, `source` (`{provider: string}`), `relayed`, and exact `{id, protocol}` selections. A relayed entry also reports `base_url`, `credential_ref` (DSH target with one selected protocol only), and `applied_delivery`; a direct entry has null values for those fields. |
| Provider detail | Qualified requirements, readiness, native precedence/evidence, and desired/applied differences; no mode, target credential, permission list, or target continuity field. |
| `configuration_revision` | `agent inspect` returns the current saved configuration revision at the top level; null when unavailable. It is not an applied revision. |
| `applied_delivery` | Relayed entry field comparing the applied delivery facts with the current ones: `state` (`current`, `stale`, `blocked`, `unknown`, `not_applied`, or `not_configured`), `revision`, `applied_revision` when recorded, named `reasons`, and `routes.removed`. Reasons are not merged: `listen_changed`, `key_rotated`, `route_removed`, and `entry_selection_changed` make the state `stale`; `entry_references_missing_route`, `native_edit_conflict`, and `gateway_not_ready` make it `blocked` when no drift is present, and an apply whose native writes did not finish is `blocked` with `apply_incomplete`. The recorded revision is target-scoped over the `route/protocol` pairs that target references, so an unrelated added route changes neither the revision nor the state, while a changed selection protocol is drift. An unknown or not-applied state names its `reason_code` (`delivery_not_recorded` or `never_applied`). |
| `agent inspect` | `data.providers` contains entries; `data.default` uses `NativeModelRef` with optional `effort` or `native_effort`, `data.roles` use model-only `NativeModelRef`, and a null default means unmanaged. |
| `readiness` | `agent inspect` reports one row per dimension a target's blockers touch, plus the dimensions that are fine: `ready` is true only when nothing blocks, `blocked` lists `{dimension, code, message, next_action}` in the order a reader meets them (`scope`, `native_installation`, `plugin`, `gateway`, `credential`, `compatibility`, `selection`, then any other), and `dimensions` lists every row's `state`. A dimension carries its first blocker; the `blockers` array beside it carries them all. `next_action` is the action that clears that dimension: a runnable command where one exists, otherwise one sentence naming what the reader must do, and null only where no action exists (`scope`, which the reader selects). |
| `environment` | `agent apply` reports the environment facts it evaluated in both a dry run and a blocked run: `native`, the client's installation evidence, for an agent tmax generates against a version floor. Environment obstacles are collected together and returned in one run with `blockers` in a fixed order, so a later obstacle is not hidden behind an earlier one; `_code` is the first of them. |
| `native_default` | `agent inspect` reports the tool's currently persisted default selection read-only: `state` (`available` or `unavailable` with `reason_code`) and `fields` carrying `present`, `provider`, `model`, and `effort` when the native file exposes them. It is inspection evidence, never desired configuration and never imported automatically. |
| `native_projection` | `agent inspect` and `agent apply --dry-run` return one entry per native provider entry: `name`, `source` (`{provider: string}`), and `models` of `{id, protocol, capabilities, efforts, projection}`; `capabilities` carries the model's safe capability metadata, and `efforts` lists the effort names representable for that model and target, or is empty. A `projection` has `state`, `reason_code`, `fields`, and `omitted`; a blocked projection adds `message`. `fields` carries only safe capability metadata and never secrets. |
| Lists | Target/tool identities, counts, and readiness summaries instead of complete entries/evaluations; `default_configured` reports whether the target has a configured default. |

Projection semantics follow the shared
[capability mapping](../../integrations/spec.md#agent-boundaries) boundary.
`native_projection` is the desired projection and is reported even when the
gateway is not running. Application instead requires a ready gateway listener.
Direct entries retain actual upstream declarations. Relayed entries use the same
states below after the shared
[alias reasoning capability](../../integrations/spec.md#alias-reasoning-capability)
rule intersects all candidate input sets after maps and fixed restrictions. No
native entry receives a gateway translation map, default, or thinking switch.

| Projection `state` | `reason_code` and meaning |
| --- | --- |
| `projected` | Null `reason_code`; `fields` carries the projected native capability fields. |
| `partial` | `native_reasoning_options_partial`; `fields` carries the representable exact names and `omitted` lists the declared names the native schema cannot express. |
| `not_supported` | `native_reasoning_options_unavailable`; the agent has no native reasoning-effort selector and `fields` is empty. |
| `unavailable` | `reasoning_disabled` (explicit concrete-model `reasoning = false`), `reasoning_options_unavailable` (no declared effort names or no common alias effort names), or `native_projection_unavailable` (no projection for this model); `fields` is empty. |
| `blocked` | `native_metadata_unrepresentable` with `message` explaining the unrepresentable capability; the error never fails inspection and blocks only that agent's application before native writes. |

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
`undocumented` elsewhere). It reports that the native entry stays bare for the
missing fields; it does not block application. `model add` returns the same
`fact_gap` object for the added model when the saved catalog snapshot supplies
neither fact.


A null `native_projection` means planning failed or is unavailable; `blockers`
remain authoritative.

Native inspection reports `scope_state` separately from the saved target. When a
declared path is absent or unusable, it retains the target and resolved `path`,
sets `scope_state: unavailable`, and provides `scope_reason_code` (`absent`,
`not_directory`, `unreadable`, or `unavailable`); actual apply remains blocked.
`applied_state` describes current desired-versus-written state. When comparison
is available, `pending_native_changes` identifies native field `set`/`release`
changes or a credential-reference replacement; otherwise it is null. These are
unwritten desired deltas; a `release` appears only while the owned field is still
present in native. An owned field that is already absent is recovery bookkeeping,
not a pending native write. `awaiting_native_reload` is separate: it means written
native changes await a tool reload/restart, not that those deltas remain pending.

Installation evidence consists of `state`, `version`, `executable`, and `source`,
with states `not_installed`, `probe_failed`, `version_unknown`, and `detected`.
When the executable is absent, evidence consists only of `state: not_installed`
and `version: null`. If a declared target's inspection scope is unavailable,
installation is not checked and reports `state: not_checked`, `version: null`,
and reason code `native_scope_unavailable`.

Applying configuration is a declared-target file edit and does not require or
probe the tool executable, except for OpenCode schema detection and the DSH
version floor. A positively detected OpenCode 1.x schema reports
`native_schema_incompatible`, exit 3, and no native writes. A positively detected
DSH older than the one qualified version reports `native_version_unqualified` with
the detected and qualified versions, exit 3, and no native writes; a dry run
reports the same blocker instead of ready. A DSH home with no `desktop` or `web`
profile reports `unknown_resource` naming the profiles it has, exit 3, and no
native writes, in a dry run as well. tmax supports only that qualified DSH
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
path, blockers, and effects. Native partial writes enumerate changed/unchanged
files and safe recovery; a completed write with a reload reminder is a write
outcome, not a claim about the running tool. Detail keeps desired and written
state distinct. Incomplete effects stay visible.

Examples: `tmax agent discover --agent pi`, `tmax agent inspect pi-main`,
`tmax agent inspect --agent pi --path /home/user/.pi/agent`,
`tmax agent restore pi-main`, `tmax agent recovery list --target pi-main`,
`tmax agent recovery inspect RECORD`, `tmax agent restore --recovery RECORD`, and
`tmax agent recovery discard RECORD`. Use returned recovery handles; inspect
conflicts instead of forcing old writes.
