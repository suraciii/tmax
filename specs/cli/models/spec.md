# Model commands

This child owns concrete model reads/edits and selection explanation. The
[model contract](../../model-routing/spec.md) owns identity, compatibility,
selection, and continuity; [Configuration](../../configuration/spec.md) owns
saved fields. Use [common CLI rules](../spec.md),
[configuration changes](../configuration/spec.md), and [shared results](../results/spec.md).
[Catalog](catalog/spec.md), [model aliases](aliases/spec.md), and
[rules](rules/spec.md) own their command details.

## Model inspection

| Command | Result |
| --- | --- |
| `model list [--provider ID] [--protocol P] [--kind KIND]` | Compact concrete/alias identities, declared protocol support, shared publication, and availability. `KIND` is `concrete` or `alias`; omission includes both. |
| `model inspect QUALIFIED [--protocol P]` | Concrete model details across all supported protocols; optional protocol filters the details. Include effective metadata/provenance, local overrides, configured protocol-scoped `effort_map`, revision-scoped local array indices, capabilities, priority/rules, and file/loaded differences. |
| `model inspect ID [--protocol P]` | Inspect a model alias's definition, candidates, shared continuity, and availability. Without protocol, infer only one uniquely configured alias ID. Ambiguous IDs require `--protocol`. |

| Inspection fact | Contract |
| --- | --- |
| Concrete identity | A qualified name identifies a model independently of protocol; inspect requires no protocol operand. Filtering cannot manufacture protocol support or upstream access. |
| Alias identity | `(protocol, id)`; inference, agent model/default/role operations, and explanation still require explicit protocol. |
| List `kind` | `concrete` or `alias`. |
| List `published_protocols` | Derived only from enabled existing provider/model/alias definitions and declared supported protocols, never native Agent selections. Publication is not proof of upstream success. |
| Availability | Disabled/unavailable/unsupported conditions follow the model contract. |
| Shared publication | Every shared-key holder sees the same list; no gateway-model CRUD, publishing/permission table, or automatic catalog enrollment. |
| Compact list | Do not repeat metadata, full evaluations, or inspect argv per row. |
| Detailed concrete read | Expose [priority evaluations](rules/spec.md#priority-evaluation-data) once per model, not per protocol. |
| Native reasoning projection | Concrete inspection includes upstream `native_projection` keyed by agent. OpenCode also reports `protocols` keyed by the model's published protocols; each uses the [projection shape and states](../agents/spec.md#agent-result-data). Separate `gateway_projection` is keyed by protocol and describes mapped caller-facing inputs for gateway entries. Direct native entries retain upstream capabilities. These are desired capability projections, not defaults or evidence of native application. |
| Alias input capability | `capabilities.input` contains `state` (`declared` or `unknown`), `common` (array, including empty, or null), and `candidates` of `{model, input}` (qualified reference and effective array or null). `metadata.effective` carries the derived common input and effort declarations; `metadata.source` and `metadata.metadata_revision` identify the accepted catalog basis when available. `native_projection` reports each Agent's expected input and effort representation. These are derived facts, never editable alias settings or proof of entitlement. |

Examples: `tmax model list --kind concrete`, `tmax model list --kind alias`,
`tmax model inspect deepseek/deepseek-flash`,
`tmax model inspect deepseek/deepseek-flash --protocol chat-completions`, and
`tmax model inspect L0 --protocol chat-completions`.

## Concrete models, metadata lists, and reasoning options

| Command | Requested change |
| --- | --- |
| `model add QUALIFIED [EDIT]` | Register exactly that upstream model under an existing provider, even if absent from catalog metadata. |
| `model update QUALIFIED [EDIT]` | Edit the scalar fields below; at least one edit is required. |
| `model rename QUALIFIED NEW-MODEL-ID` | Change the upstream-ID component within the same provider, update references, and preserve internal model identity. Never move providers. |
| `model remove QUALIFIED` | Remove only that registration after reference/input-coverage checks, not catalog data or other instances. |
| `model override list replace QUALIFIED FIELD [--value STRING]...` | Replace `overrides.modalities.input` or `overrides.modalities.output`. No values writes an explicit empty array. |
| `model reasoning add QUALIFIED --type STRING [--value STRING]... [--empty-values]` | Append one local reasoning-option table; an absent override starts empty, not copied from catalog. |
| `model reasoning replace QUALIFIED INDEX --if-revision REVISION --type STRING [--value STRING]... [--empty-values]` | Replace one complete local table at that file revision. |
| `model reasoning remove QUALIFIED INDEX --if-revision REVISION` | Remove exactly one local table at that revision. |
| `model reasoning clear QUALIFIED` | Write an explicit empty local reasoning-options array. |

A reasoning-option table has required string `type` and optional string-array
`values`, without a synthetic ID, selected value, or extra local metadata shape.
Repeated values preserve argument order; omission removes optional `values` on
replace. `--empty-values` writes `values = []` and cannot combine with `--value`.
Missing indices are errors. Removing the last entry leaves an explicit empty array;
`model update QUALIFIED --clear overrides.reasoning_options` restores inheritance.
These fields describe supported controls, not chosen request parameters.

| Model `FIELD` | Scalar type and clear behavior |
| --- | --- |
| `enabled` | Boolean; clear restores true. |
| `priority` | Signed integer; clear restores zero. |
| `timezone` | IANA-zone string; clear only if no schedule/signal time gate requires it. |
| `overrides.name`, `overrides.description`, `overrides.family`, `overrides.knowledge`, `overrides.release_date`, `overrides.last_updated` | Strings; clear restores inheritance. Date-like catalog metadata stays a string, not a local subscription/time-rule date. |
| `overrides.attachment`, `overrides.reasoning`, `overrides.tool_call`, `overrides.structured_output`, `overrides.temperature`, `overrides.open_weights` | Booleans; clear restores inheritance. |
| `overrides.limit.context`, `overrides.limit.input`, `overrides.limit.output` | Nonnegative integers; clear restores the individual inherited limit. |
| `overrides.cost.input`, `overrides.cost.output`, `overrides.cost.reasoning`, `overrides.cost.cache_read`, `overrides.cost.cache_write`, `overrides.cost.cache_storage` | Nonnegative finite numbers in catalog units; clear restores the inherited fact. |
| `overrides.interleaved` | Boolean representation; setting replaces a local table. |
| `overrides.interleaved.field` | Required nonempty string in the table representation; explicitly clear the parent to replace a local boolean. |

Clear also accepts `overrides`, `overrides.limit`, `overrides.modalities`,
`overrides.modalities.input`, `overrides.modalities.output`, `overrides.cost`,
`overrides.reasoning_options`, and `overrides.interleaved`. Structured fields use
only these forms, not JSON/TOML blobs. Identity fields, inference parameters,
and downloaded catalog fields are not arbitrary local override paths.

Interleaved metadata is exactly one alternative: boolean, or table with required
`field`. A boolean set replaces a local table atomically. To replace a boolean,
use `--clear overrides.interleaved --set overrides.interleaved.field STRING`.
Setting both representations is invalid. Editing an existing table changes only
its leaf; clearing required `field` alone is invalid. Clear the parent to inherit.

```sh
tmax model reasoning add deepseek/deepseek-flash --type effort --empty-values
tmax model update deepseek/deepseek-flash --clear overrides.reasoning_options
```

These examples distinguish explicit empty arrays from inheritance, not verified
upstream reasoning support. Mutation results identify the model, changed fields,
needed file revision, and saved/loaded outcome; full metadata remains in inspect.

## Model effort maps

| Command | Requested change |
| --- | --- |
| `model effort-map set QUALIFIED INPUT UPSTREAM --protocol P` | Upsert exactly one caller-facing input to upstream target correspondence for this model/protocol, preserving other entries. |
| `model effort-map remove QUALIFIED INPUT --protocol P` | Remove exactly one mapping; an absent entry is an error. |
| `model effort-map clear QUALIFIED --protocol P` | Remove all mappings for that protocol; already empty is a successful no-op. |

These are ordinary configuration mutations with the shared dry-run, revision,
validation, and saved/loaded result rules, not a separate preview/apply workflow.
`P` must be `chat-completions` or `responses` and supported by the model;
Messages is rejected. `INPUT` and `UPSTREAM` must be nonempty and contain no
whitespace or control characters. `UPSTREAM` must be an explicitly declared
effort in the accepted effective metadata after overrides; unknown, disabled, or
toggle-only reasoning does not qualify it. Invalid names, protocols, and targets
are usage/validation errors, not passthrough values.

The [configuration schema](../../configuration/spec.md#model-effort-maps) owns
the dictionary and effective-target validation; [Request parameters](../../model-routing/spec.md#request-parameters)
owns gateway resolution and fixed > map > identity precedence. Removal restores
identity, not input disabling. Maps do not supply omitted effort or modify native
defaults, switches, or budgets, and do not apply to direct native entries. Read
configured maps through `model inspect QUALIFIED`; inspect is not a new effort-map
resource. After changing projected inputs, explicitly reapply affected native
entries and reload their tools under the [alias capability rule](../../integrations/spec.md#alias-reasoning-capability).

```sh
tmax model effort-map set custom-gateway/example-model max high --protocol chat-completions
tmax model effort-map remove custom-gateway/example-model max --protocol chat-completions
tmax model effort-map clear custom-gateway/example-model --protocol chat-completions
```

The [configuration example](../../configuration/example.toml) supplies the
synthetic model and declared target for these examples; they are not hosted
provider qualification.

## Model explanation

| Command | Effect-free explanation |
| --- | --- |
| `model explain ID --protocol P [--target TARGET [--agent-provider ENTRY] \| --agent AGENT --path PATH] [--at INSTANT] [--session SESSION] [--state STATE]` | Explain an explicit model ID without managed-target prerequisites, optionally assessing a declared target, its named native entry, or an undeclared native scope. |

| Explanation context | Operand and selection contract |
| --- | --- |
| Public ID without native context | `ID` is a public qualified concrete or alias ID; use gateway requirements. |
| Declared target without entry | `--target TARGET` assesses the public qualified concrete or alias ID against the target's native requirements. No saved model/default/role selection is required. |
| Public/native ID with entry context | `--agent-provider ENTRY` requires `--target TARGET` and an existing entry. Upstream-source `ID` is the exact unqualified upstream model ID under its provider; gateway-source `ID` is the public qualified concrete or alias ID. The entry need not have selected the model. |
| Native entry assessment | Assess native support, supplier restrictions, and credential handoff, not permission or a private model list. Incomplete target selections/default do not block explanation. Never infer context from a shared key or similar entry name. |
| Undeclared native scope | `--agent AGENT --path PATH` assesses the public qualified concrete or alias ID with the same qualified inspection as `agent inspect --agent AGENT --path PATH`; do not register or import the scope. |
| Context selectors | Omit all selectors, choose `--target` with optional `--agent-provider`, or choose both `--agent` and `--path`. Reject mixed or incomplete selectors. |
| Candidate scope | Use only the explicit alias candidates or the requested concrete model, not native imports. A tool name/readable native login is not proof of supplier compatibility or access. |

| Explanation evidence or boundary | Contract |
| --- | --- |
| No `--at` | Use one coherent observation/admission instant with config, accepted metadata, and input revisions captured together. |
| `--at INSTANT` | Use retained facts valid then; do not fetch, predict, or rebuild missing history. |
| Candidate evidence | Return selected/excluded models, reasons/ties, effective priority, ordered signal-rule explanations, calendar fallback, zone/UTC offset, next boundary, and provenance/freshness under the [priority result contract](rules/spec.md#priority-evaluation-data). |
| Candidate effort | Show the alias-local configured choice under [Request parameters](../../model-routing/spec.md#request-parameters), independently of capability declarations. Do not present an omitted override as an upstream default. |
| Support/access | Distinguish documented, verified, incompatible, and unknown support; missing qualification/credentials remain blocked/unknown, not fabricated access. |
| Alias continuity | Shared by every caller of the definition; native selections/defaults/roles do not override it. |
| `--session SESSION` | Inspect an existing gateway handle matching this public ID/protocol, with or without native context. Return its retained destination or blocked continuation, not a new-session winner. Reject upstream-source entries. |
| Conversation identity | Shared key/model alone is not identity. Session-stable forwarding requires supported native conversation identity and scope; never invent a session or reconstruct conversation history. |
| Retained target attribution | Independent evidence when present, not a consequence of the shared key. |
| Effects | No settings/native writes, secret retrieval, login refresh, inference start/quota use, session reservation, execution handle, or hidden apply step. Reject dry-run. |

### Model-explanation data

`data.context` is `{kind, target, agent_provider, agent, path}`:

| `kind` | `target` | `agent_provider` | `agent` | `path` |
| --- | --- | --- | --- | --- |
| `gateway` | null | null | null | null |
| `agent_provider` | Declared `TARGET` | Target-local `ENTRY`, not an upstream instance ID | Target's tool identity | Target's resolved native path |
| `agent_target` | Declared `TARGET` | null | Target's tool identity | Target's resolved native path |
| `native_scope` | null | null | Explicit `AGENT` | Inspected resolved native path |

All context fields other than `kind` are strings or null.

| Other model-explanation data | Contract |
| --- | --- |
| `data.source` | Present only for `kind = agent_provider`; exactly `{provider: string}` or `{gateway: true}`. |
| `data.requested` | Exactly `{id, protocol}`; both are required strings. |
| `data.continuity` | `session` or `stateless` for alias gateway explanations; otherwise null. |
| `data.selection.model_identity` | Stable local identity of the selected provider-owned Model; independent of provider/model display names and explicit ID renames. |
| `data.selection.qualified_model` | Exact selected `provider-id/model-id`; distinct from the requested public alias ID. |
| Selection tie-break reason | `stable_model_identity` when equal effective priorities are resolved by stable local Model identity. |
| `data.priority_evaluations[].candidate_reasoning_effort` | Exact configured string for that evaluated candidate, or null for no configured override. |
| `data.selection.candidate_reasoning_effort` | Exact configured string for the selected destination, or null. A retained-session read uses the current configuration view and preserves its separate original decision. |
| Selection/session evidence | Follow the model contract and retain captured revisions; no authorization or target-permission result. |

Examples: `tmax model explain L0 --protocol chat-completions`,
`tmax model explain L0 --protocol chat-completions --target pi-main --agent-provider local`,
`tmax model explain deepseek-flash --protocol chat-completions --target pi-main --agent-provider upstream`,
`tmax model explain L0 --protocol chat-completions --target pi-main`,
`tmax model explain L0 --protocol chat-completions --agent pi --path /home/user/.pi/agent`,
and `tmax model explain L0 --protocol chat-completions --session SESSION`.
Use a returned current session handle; it is retained-data identity, not approval.
