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
| `agent model add TARGET NAME ID --protocol P` | Add one exact model to the named native provider entry, not its default or role. |
| `agent model remove TARGET NAME ID --protocol P` | Remove after default/role reference checks. |
| `agent model clear TARGET NAME` | Empty this entry's selections only after its default/role references are cleared. |
| `agent default set TARGET NAME ID --protocol P` | Choose an existing selection of that native provider entry as default. |
| `agent default clear TARGET` | Remove the default, preserving entries/selections/roles. |
| `agent role set TARGET ROLE NAME ID --protocol P` | Map one qualified native role to an existing selection of that entry. |
| `agent role remove TARGET ROLE` | Remove exactly that native mapping. |
| `agent role clear TARGET` | Remove all native mappings, not selections/default. |

| Native operand | Contract |
| --- | --- |
| `--agent AGENT` | Select the tool and save `agents.<TARGET>.agent`; supported values follow [Invocation](../spec.md#invocation). |
| `--path PATH` | Save `scope`; resolve relative paths through [Invocation](../spec.md#invocation). Provider input `--scope` is unchanged. |
| `NAME` | Exact target-local native provider name, not another tmax upstream provider ID. |
| Default/role reference | `{provider, id, protocol}`, where `provider` is `NAME`. |
| `ROLE` | Adapter-supported native model role, not an access role, universal LLM role, or inference setting. Inspection lists supported names; reject unknown names. |

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
| `agent provider add TARGET NAME (--provider PROVIDER \| --gateway)` | Create an entry with the explicit source and an empty model list. |
| `agent provider update TARGET NAME (--provider PROVIDER \| --gateway)` | Replace the source, validating the complete resulting entry and target references. |
| `agent provider remove TARGET NAME` | Remove the entry after default/role and other reference checks; do not write native files or revoke gateway access. |

Exactly one source is required: `--provider` references one existing tmax upstream
access instance; `--gateway` uses the sole tmax model gateway. Neither is a fallback.
Reject missing/mixed selectors and duplicate entry names. Source replacement
validates every existing selected ID/protocol against the new source and reports
affected selections/default/roles. Reject unresolved IDs; never reinterpret,
repair, clear, or add them implicitly. Existing valid selections may remain;
there is no extra empty-list precondition.

For an upstream source, `ID` is the exact upstream model ID under that provider,
without a redundant provider prefix. For a gateway source, it is the public concrete
qualified name or alias ID, without an adapter's native display prefix.
Protocol is explicit and establishes no conversion or compatibility. Prefer
Responses when the upstream declares it and the native client supports it, under
the [client-owned protocol rule](../../integrations/spec.md#endpoint-resolution).
Upstream and gateway entries may coexist in one target. Native namespace conflicts
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
tmax provider add deepseek --metadata deepseek
tmax provider key set deepseek --key-stdin
tmax model add deepseek/deepseek-flash
tmax model alias add L0 --protocol chat-completions --candidate deepseek/deepseek-flash
tmax gateway configure --listen 127.0.0.1:17840
tmax gateway start
tmax agent add pi-main --agent pi --path /home/user/.pi/agent
tmax agent provider add pi-main upstream --provider deepseek
tmax agent model add pi-main upstream deepseek-flash --protocol chat-completions
tmax agent provider add pi-main local --gateway
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
| `agent list` | Compact target IDs/tool identities, resolved paths, provider/model counts, `default_configured`, and desired/applied/effective condition. |
| `agent discover [--agent AGENT]` | Supported installations/paths, versions, profiles, and compatibility without registration/import. |
| `agent inspect TARGET` | Native entries/sources, precedence, supported roles/requirements, effective auth/model use, native capability projection, desired/applied/effective differences, blockers, and recovery. |
| `agent inspect --agent AGENT --path PATH` | Inspect an explicit undeclared native path without registration/import. |
| `agent apply TARGET` | Apply current complete desired configuration; identify owned files, credential handoff, native reload, affected paths, and recovery. |
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
DSH uses the independent `llm-tmax` plugin under the
[DSH home boundary](../../integrations/spec.md#dsh-home-boundary). Apply installs
the embedded `tmax-dsh.mjs` as a direct child of the existing home and manages
its entry in `cordis.patch.yml`; native loader resolution uses installed DSH
dependencies without npm installation. It never edits profiles, presets, bundles,
or launcher-owned `cordis.yml`. Discovery uses nonblank `DSH_HOME`, otherwise
the OS home directory's `.dsh`; an explicit scope selects the home directly.
User `llm-pi-ai` configuration stays independent and untouched. Every adapter's
provider configurations register in DSH's common LLM runtime, so selected native
names must be globally unique; tmax does not automatically merge colliding names.
Removing a previously managed DSH route restores its captured baseline and removes
introduced empty containers, not an invalid empty provider registration. Unowned
settings that keep a newly introduced route in use block before native writes.
Direct upstream and gateway entries may coexist. Existing HOME-level fields and
expressions remain preserved under the owning boundary. Model-only application
writes no `agent-default-model` entry or reasoning preferences; an explicit
default changes provider/model only, retaining existing HOME-level effort
settings. Credential handoff enables the native credential service in the HOME
patch, including for headless consumption. Recovery covers the artifact, plugin
insertion, and owned provider and credential fields. `llm-tmax` is the only
managed adapter entry; no earlier adapter application format is supported or
automatically migrated. Runtime overlays remain outside tmax's ownership.

Upstream entries use their referenced endpoints/auth and need no running tmax
gateway or model aliases. Gateway entries use its local URL and the current shared
key, requiring the matching ready listener and loaded snapshot. Start explicitly
first. Applying both kinds preserves native precedence, supported credential
handoff, and disclosure of any required plaintext in native files. Never copy
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
| `NativeModelRef` | `{provider, id, protocol}` with a target-local provider name. |
| `AgentProviderView` | `name`, `source` (`{provider: string}` or `{gateway: true}`), and exact `{id, protocol}` selections. |
| Provider detail | Qualified requirements, readiness, native precedence/evidence, and desired/applied/effective differences; no mode, target credential, permission list, or target continuity field. |
| `agent inspect` | `data.providers` contains entries; `data.default` and `data.roles` use `NativeModelRef`, with null default when absent. |
| `native_projection` | `agent inspect` and `agent apply --dry-run` return one entry per native provider entry: `name`, `source` (`{provider: string}` or `{gateway: true}`), and `models` of `{id, protocol, projection}`. A `projection` has `state`, `reason_code`, `fields`, and `omitted`; a blocked projection adds `message`. `fields` carries only safe capability metadata and never secrets. |
| Lists | Target/tool identities, counts, and readiness summaries instead of complete entries/evaluations; `default_configured` reports whether the target has a configured default. |

Projection semantics follow the shared
[capability mapping](../../integrations/spec.md#agent-boundaries) boundary.
Gateway concrete entries use caller-facing names after explicit model maps;
direct entries retain actual upstream declarations. Gateway aliases use the same
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
limit has no catalog or declared fact). Unknown limits are never invented.

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
`applied_state` describes current desired-versus-written state, while
`last_application_state` reports the historical last application. When comparison
is available, `pending_native_changes` identifies native field `set`/`release`
changes or a credential-reference replacement; otherwise it is null. These are
unwritten desired deltas. `awaiting_native_reload` is separate: it means written
native changes await a tool reload/restart, not that those deltas remain pending.
Installation version evidence distinguishes `not_installed`, `probe_failed`, `version_unknown`,
and `detected`. Missing tools (`native_not_installed`) and known incompatible
schemas (`native_schema_incompatible`) block application with exit 3 and no
native writes. Other version uncertainty produces `native_version_unverified`
in `data.warnings` without blocking application.

Successful apply/restore returns exit 0 and status `success`, including when its
code is `awaiting_native_reload`. The reload reminder and unverified runtime
state remain explicit; a completed file write is not a failed application.

Routine mutation results identify `data.target`, `data.provider` when entry-scoped,
requested changes, and actual saved/loaded or applied/reload outcome. Native
partial writes enumerate changed/unchanged files and safe recovery; write-complete
but reload-pending is not fully effective application. Detail keeps desired,
written, and effective state distinct. Incomplete/unknown effects stay visible.

Examples: `tmax agent discover --agent pi`, `tmax agent inspect pi-main`,
`tmax agent inspect --agent pi --path /home/user/.pi/agent`,
`tmax agent restore pi-main`, `tmax agent recovery list --target pi-main`,
`tmax agent recovery inspect RECORD`, `tmax agent restore --recovery RECORD`, and
`tmax agent recovery discard RECORD`. Use returned recovery handles; inspect
conflicts instead of forcing old writes.
