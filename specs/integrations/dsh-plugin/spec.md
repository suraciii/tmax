# DSH plugin page

## Authority and scope

This specification owns the tmax profile plugin inside DeepSeek Harness (DSH):
its first-level page, its DSH-native seats, its `/tmax` read command, the host
read model, explicit operations, web-route admission, and evidence rules. It
extends the shared [DSH home boundary](../spec.md#dsh-home-boundary) and
[DSH plugin boundary](../spec.md#dsh-plugin-boundary), consumes tmax's effect-free
reads — [agent inspection](../../cli/agents/spec.md#agent-application-and-recovery),
[provider inspection](../../cli/providers/spec.md#provider-operations-and-subscription-fields),
[gateway and Web status](../../cli/service/spec.md#daemon-and-listener-data),
[configuration validation](../../cli/configuration/spec.md#explicit-configuration-changes),
[model explanation](../../cli/models/spec.md#model-explanation), and
[retained request history](../../diagnostics/spec.md#session-only-bounded-history)
— and follows the [product contract](../../product/spec.md) for access, evidence,
and secret protection. Shared terms are defined in [CONTEXT.md](../../../CONTEXT.md).

There is no `tmax dsh` command. The plugin composes its read model from the
commands above and maps it into one plugin-owned state contract. That contract
is not a second tmax CLI contract, and the plugin does not add a command.

The plugin is an in-context lens and safe control surface for tmax-managed
access. tmax's CLI and Web remain where providers, models, and targets are
authored. The page presents every declared tmax target as a **Harness**: one
target from `tmax agent list`, not the DSH profile, a seat, or a separately
managed resource. The plugin may add or remove model selections in a declared
Harness through the fixed `add_model` and `remove_model` actions, and may apply
one declared Harness through `apply_target`. It does not declare,
rename, or remove a Harness, edit providers, models, aliases, priorities, or
schedules, manage DSH profiles or presets, and is not a second tmax authority.
The browser half never runs tmax; the host half reads it, and its only
target-selection writes are `add_model` and `remove_model`; its only actual-file
write is the per-Harness `apply_target`. Its other operations are explicit
observation, previewed provider repair, and previewed gateway lifecycle actions;
it never edits a provider, model, alias, priority, or schedule definition. A DSH Harness's apply
writes under the [DSH home boundary](../spec.md#dsh-home-boundary); applying
another Agent's Harness follows that Agent's own integration and application
contract.

The page, seats, the `/tmax` command, and the `tmax_status` tool read one host
read model. They must not hold separate business logic for the same facts. Seats
and the command are DSH-facing projections of that model; they are not a second
authority.

## Laws

1. A read from the page, a seat, the `tmax_status` tool, or a `/tmax`
   subcommand other than `history` is effect-free: it must not observe a
   provider, resolve a credential value, apply a target, or write a file.
2. Observations are explicit and timestamped. Reusing a reading never changes its
   observation time.
3. Unknown, absent, and zero are distinct values. None may render as another.
4. One route owner serves a route name at a time. The plugin never registers a
   partial set, replaces another registration, or serves a route it reports as
   another owner's.
5. No credential value crosses to the browser, logs, argv, environment, or an
   ordinary response. A reference name is not a secret.
6. The plugin does not author tmax provider, model, alias, priority, schedule,
   or Harness definitions and does not manage DSH profiles. `add_model` and
   `remove_model` change one declared Harness's desired configuration only and
   never write actual configuration; `apply_target` remains tmax's own
   per-Harness Agent-application operation, not a plugin-authored file format.
7. Every action the page offers is allowlisted and names closed operands, never
   a command, flag, or path. An actual-file write previews before it executes:
   `apply_target` previews and applies exactly one Harness. `add_model` and
   `remove_model` are fixed desired-configuration saves with per-step results;
   they execute directly and write no actual configuration. There is no
   page-wide apply and no arbitrary action chaining.
8. The read model source is explicit: `diagnostics.readModel` reports
   `inventory`, the composition of effect-free tmax reads. There is no snapshot
   source and no fallback source; a failed read is a visible error, never an
   empty successful section.
9. A seat is additive: it registers only on a declared DSH slot, never replaces
   DSH-owned content, never changes a DSH selection or setting, and never
   performs an action.

## Page content

The page is one first-level DSH page with four tabs: **Models**, **Harness**,
**Gateway**, and **Quota**. It answers whether model access is working, what each
declared Harness selects, and how much of each account's quota is spent.
Configuration revisions, schemas, route detail, full gateway supervision,
diagnostics, and retained request history stay in the CLI.

| Section | Required content |
| --- | --- |
| Header | The page name, when the state was generated, and **Refresh**, which observes every listed provider that reports a quota source. The header offers no page-wide reachability verdict and no Apply; failed section reads and route blockers are named in compact lines below it with a **Retry** that reads again and never observes. A failed read keeps the last good table, labels it last known, and shows the failure once. Start gateway is a Gateway-tab control, not a header action. The tab bar follows the header; switching tabs never reads or acts. |
| Models | Every tmax provider and every concrete model and alias under it, grouped by provider, each showing its published protocols; no provider-level selection text. Search matches provider name/id, model id, and protocol. Each model row carries one inline Harness control listing the declared Harnesses that select it and an aggregate applied/unapplied/unknown status dot. Opening the control shows a searchable picker over declared Harnesses with one membership checkbox per Harness, each with its target-level status; each checkbox change saves only desired configuration through the fixed `add_model` or `remove_model` action and writes no actual configuration. **完成** only closes the picker. The page may summarize how many Harnesses are unapplied and link to the Harness tab; it must not claim a count of changed model items. |
| Harness | One row per declared Harness: target name and Agent, target ID and model count, selected models as chips, default mark, applied state, `待应用` (unapplied), `已应用` (applied), or an unknown marker, blockers, an **添加模型** button to the Models tab, and an **应用** control that previews and then applies that Harness alone. A row whose retained record awaits the Agent reload says so beside the applied state. No row applies another Harness, and the page has no page-wide Apply. |
| Gateway | The configured and ready state, listener address, shared-key loaded state, and tmax's reason code when not ready; the **Start gateway** control when configured and not ready. It shows no supervision, autostart, or other CLI detail. |
| Quota | One card per tmax account that reports a quota window, closest to its limit first; an account without any window takes no card. Every window the account reports is one bar filled with the share it has used, stating its name, what it used of what, and when it resets, whatever basis the account reports in: a percentage window states its share of 100%, a reported quantity states what was used of it; an unqualified reset time is printed as approximate, an absent one is left out. A window whose share cannot be computed is text, never a bar. A card carries a tag only for a state a person must act on or know about — disabled, suspended, failing — and the one action its evidence calls for: resume for a reported suspension, reset for an active failure. |
| Actions | The preview or result of one confirmed provider, gateway, save, or apply action: typed code, status, message, data, blockers, next actions, and effects. An apply preview and its confirmation are separate, and cancel clears the preview. A save runs directly and reports its typed result, including the target Harness, entry, model, protocol, and step names. An apply preview states the Harness and the files it would write. An observation names only the accounts it could not read. |

## Seats

Seats are narrow additions to DSH-owned pages. Each registers on a declared slot
through the DSH slot service and appears only when the declaring DSH package is
part of the running composition; a missing slot needs no page change. The client
half declares the DSH client packages it injects:
`@deepseek-ai/dsh-client-locale`, `@deepseek-ai/dsh-client-ui-sidebar`,
`@deepseek-ai/dsh-client-ui-layout`, `@deepseek-ai/dsh-client-ui-conversation`,
`@deepseek-ai/dsh-client-ui-settings-models`, and
`@deepseek-ai/dsh-client-ui-tool`. A missing package simply leaves that seat
unregistered; the plugin page and the other seats keep working.

| Seat | Slot | Contract |
| --- | --- | --- |
| Sidebar marker | `sidebar.panellist.trailing` | Non-interactive severity marker beside the plugin's sidebar row: one dot plus screen-reader text. Error names an invalid configuration, a suspension, an unready gateway, an ownership conflict, or a blocker; warn names an unavailable configuration, a failed section read, or a route or gateway that cannot serve yet; ok names an active, usable route with a ready gateway; unknown means no readable state. It never acts, never opens the page, and never claims readiness from unknown values. |
| Settings models footer | `settings.models.footer` | One tmax card: gateway readiness, active and usable route counts, model count, the DSH Harness IDs, how many DSH Harnesses are unapplied, a reason when tmax models are not usable, and the one control that helps. **Reload** appears when the read failed or the page's next action is retry; **Open tmax** opens the plugin page only when that layout page is present. The card offers no Apply: application is per Harness on the page's Harness tab, and no page-wide apply exists. It reads the host state only. |
| Tool card | `tool.call.toolview` keyed `tmax_status` | A compact card rendered from the tool result already returned: target, routes, gateway, provider and model counts, blockers, and a failed or unreadable state. It never performs a new read, observation, or action. |

Seat rules:

- Every seat read is an effect-free GET. A seat must not observe, apply a
  Harness, resolve a credential, or mutate tmax.
- Seat data is a one-shot GET taken when the seat mounts, served from a short
  shared client cache for seats mounted together. A seat has no subscription or
  timer: an already-mounted marker or footer can lag a newer page read, an action
  result, or a state change until that seat remounts or the page reloads. The
  only seat action that takes a fresh read is the settings footer's explicit
  Reload.
- A seat must not change a DSH-owned selection, setting, or route; it only adds
  content on its declared slot.
- When the state is unavailable or a section read failed (including `ok: false`),
  a seat says so or stays silent; it must not render an empty successful card, a
  ready marker, or a zero count. A count derived from a failed or unavailable
  section is unknown, not zero; only a readable empty list is a reported zero.
- Seat text is bounded and localized with the page's English and Chinese locale
  data; the page's truth rules for unknown, absent, and zero apply unchanged.

## Command

```text
/tmax [status|providers|routing|gateway|diagnostics|history]
```

The host registers `/tmax` when the optional DSH `commands` service is present.
Registration is not required for the page, seats, tool, or routes; without the
service the command simply does not exist. `/tmax` is a human read command, not
a model-facing tool: its output never enters a model turn.

| Subcommand | Required content |
| --- | --- |
| `status` | State readability, the configured DSH target and effective default, its applied state, route count and per-route liveness and credential readiness, gateway readiness, provider count, and declared blockers. This is also the empty-input default. |
| `providers` | Every tmax provider with authentication, integration, quota state and kind, freshness, observation time, suspension, failure, and any provider error. Unknown quota stays unknown and is never zero. |
| `routing` | Every projected model with selection, candidate count, selected count, exclusions, and any per-model error. |
| `gateway` | Configured and ready state, listener, daemon discovery and reason, supervision, autostart facts, and shared-key state. |
| `diagnostics` | Read-model source, generation time, revision, tmax version, history availability and reason, last inventory read time, last error, and every section error. |
| `history` | One read-only `tmax history list` call: coverage, requested range, matched and omitted counts, and a bounded record listing. It shares the retained-history contract with the HTTP `history` route; an unavailable daemon or history is stated with its reason, never as an empty history and never as zero requests. |

Rules:

- The command is read-only. It must not observe a provider, refresh, apply,
  start or stop a service, or write any file; `history` uses the read-only
  `history list` read and must not start a daemon.
- Non-history subcommands read the host state model; the command may force a
  fresh state read but must not re-stamp an observation time.
- The subcommand set is closed and single-word. Empty input is `status`; any
  other input returns an error naming the usage and performs no read.
- Output is bounded: at most 20 routes, 40 providers, 40 routing rows, 20
  section errors, 10 history records, and 10 blockers; each cell is capped at
  120 characters. A truncated list states how
  many rows were omitted. Unreadable values render as unknown or not reported,
  never as zero. When a section read failed or its data is absent, the command
  prints an `unavailable (code: message)` line for that section instead of zero
  counts; a failed read is never rendered as an empty successful section.
- A command failure is returned as error text; it must not be reported as a
  successful empty result.
- Retained-history records are request metadata only. Bodies, headers, URLs,
  credential values, and model output never cross; the command's `history`
  subcommand and the HTTP `history` route read the same compact evidence.

## Request history

```http
GET /_tmax/history
```

The host runs one read-only `tmax history list` with a 45 s deadline (it may
wait on daemon identity and the control RPC, including startup margin, but
starts no service) and maps the result onto a compact contract. A GET must not
observe, refresh, apply, start a service, clear history, or write anything.

| Outcome | Status | Body |
| --- | --- | --- |
| Read completed | `200` | `ok: true`, `generatedAt`, `coverage`, `records`, and `truncated`/`omitted` when rows were dropped. |
| Typed unavailability | `200` | `ok: false`, a typed `code` and bounded `message`, optional `blockers` and `coverage`. A no-daemon-history or identity answer is availability evidence, never an empty list or a zero-request claim. |
| Transport or internal failure | `502` | `history_unavailable`; no fabricated rows. |

Rows: one compact object per retained request: request handle, time and time
basis, outcome, requested model and model kind, protocol, qualified model,
provider, target, agent-configured provider, and reason code. `qualifiedModel`
and `durationMs` appear only when the source record carries them; the current
`tmax history list` projection does not emit them (they belong to the
record/inspect surfaces), so a list row's selected model is unknown. The allowlist is the whole contract: a body, header, URL, credential, or
model output is never mapped. At most 50 rows are returned; `truncated` and
`omitted` combine tmax's own omission count with the cap and are absent only
when nothing was dropped.

Coverage: process instance and start, last clear, retained count, oldest and
newest, evicted count, incomplete-capture count, prior-process coverage,
direct-traffic coverage, storage limits (records and record bytes), retention,
and material gaps. `incompleteCapture` is a numeric count of incomplete
captures: a positive count is a material gap, and zero is none reported; the
host also accepts a boolean compatibility value (true as a gap). Unknown coverage
stays unknown; it is not empty coverage.

The page shows no history: retained history is a CLI read (`tmax history list`
and `inspect`, and the `/tmax history` subcommand), and the page neither fetches
nor renders it.

## Route ownership

The page owns no routes: `tmax agent apply` writes the providers into a profile's
settings and the client's own adapter serves them. This contract owns what the
page reports about that, and the blocker it raises when something else already
serves the same names. These facts describe the configured DSH target's declared
routes; a different Harness's apply status is separate, and one Harness may be
unapplied while these routes stay current.

- The state reports the owner (`settings`, `unapplied`, `none`, `other`), the
  scope (`profile_scoped` when the settings carry them), and a `route_owner_conflict`
  when another provider already serves the declared route names.
- The page never registers, replaces, or disposes an LLM route, and it never
  mounts an adapter of its own.
- Every declared route stays in the state read model with its blockers; a route
  with no usable model, credential, or base URL is reported, not hidden.
- No credential value ever reaches the page. The state names a credential by
  reference and reports whether it is configured; the page reports the route
  blocker a missing credential implies and never the reference itself.

## Reads

| Route | Method | Contract |
| --- | --- | --- |
| `/_tmax/state` | GET | The host read model for the page: one plugin-owned state contract composed from effect-free tmax reads, plus DSH-only facts. Cache only. |
| `/_tmax/status` | GET | Compatibility summary of the same read model for the first-generation page. Cache only. |
| `/_tmax/usage` | GET | Compatibility provider and usage rows derived from the same read model. Cache only. |
| `/_tmax/history` | GET | One explicit, read-only retained-history read: compact request metadata, coverage, and bounded rows. It never observes, refreshes, starts a service, clears, or writes. |

Read purity:

- The host composes the state from effect-free tmax reads: `agent list`, one
  `agent inspect` per declared Harness and for the configured DSH target,
  `status`, `gateway status`, `provider list`, `provider inspect`,
  `config validate`, `model list`, and `model explain`. It
  must not run `provider refresh`, request a vendor quota endpoint, run
  `agent apply`, or resolve a credential value.
- The declared-target section is independent of the configured DSH target: every
  declared Harness is read even when the DSH target itself does not resolve.
- There is no versioned snapshot source and no compatibility composition: the
  read model is the same composition on every supported tmax version. An older
  tmax whose inspection lacks the relayed-entry or native-capability fields is
  reported as unsupported, not silently projected with guessed fields.
- A GET may read redacted actual configuration state and route credential
  readiness through the DSH credentials service `describe` operation. `true` and
  `false` are the only known states; an unavailable store, a failed call, or an
  ordinary unknown is `null`, not missing.
- The page reads once when it mounts. It must not add a polling timer, a
  visibility listener, or any other automatic observation. The host inventory
  refresh timer only re-reads tmax; it is not an observation and does not change
  any reading's observation time.
- A failed read returns a typed code and a bounded message. The page keeps the
  last good table and names the failure. It must not clear evidence or replace it
  with zero.
- A partially failed read model is still a completed read. `ok` stays true when
  the host produced a usable model, and every failed section read is named in the
  state `errors` list (see [State consumer contract](#state-consumer-contract)).
- The host may cache the read model. A cache must not change a reading's
  observation time or make the state generation time look like one.

State generation time is the host composition time. The last inventory read time
is when that composition completed; it is not an observation time for any
provider fact.

## Observation

```http
POST /_tmax/observe
{ "dryRun": false }
```

Observation publishes evidence. It is the only explicit way to publish provider
evidence. tmax treats an explicit provider refresh as an effectful operation: it
shares the mutation preview gate, it may publish quota readings, and a
refreshed reading can feed tmax's automatic quota suspension. It is not a
DSH or actual-file write; [Apply](#apply) is the only such write.

The page exposes the whole-surface form as **Refresh**; it offers no
per-provider observation, so a reader never has to pick which account to read.
A refresh attempts only the providers whose integration reports a quota source:
the list's `billing_kind` other than `unknown`, the same set the tmax Web UI
refreshes. A provider with no source is not attempted and produces no item, so
it can never read as a failed observation. A completed refresh names the accounts
whose observation failed or that a person can fix.

`dryRun` is a boolean; an absent value means `false`. An invalid value is refused
without an observation. One observation previews or publishes one quota-scope
`tmax provider refresh` per provider with a quota source; the plugin holds no
vendor endpoint, credential resolution, or quota reading of its own.

Each result item identifies its source (`tmax`), the provider id, and one status:

| Status | Meaning |
| --- | --- |
| `refreshed` | The observation completed and published a reading. |
| `blocked` | The observation cannot run without a configuration, credential, or source change. No reading was replaced. |
| `failed` | The observation was attempted and failed, or exceeded the observation deadline. The previous reading and its observation time are retained. |
| `unchanged` | The request was a dry run. Nothing was executed. |

Rules:

- Only this surface and the per-provider observe action observe a provider. A
  page open, a visibility change, a timer, a read, or a model-facing tool call
  must not observe.
- An observation is one explicit user action. The page must not chain it with an
  Apply or any other operation.
- A dry run must make no vendor request, execute no tmax refresh, and resolve no
  credential. Its items are `unchanged` or, when a known blocker exists,
  `blocked`.
- A failure must not be presented as success. The response is `200` only when no
  item failed and `207` when at least one item failed. `ok` is false when any
  item failed.
- A concurrent observation is refused with a busy result instead of being
  queued.
- A completed observation returns a freshly composed read model so the page can
  show the new and retained readings.

`POST /_tmax/usage/refresh` performs the same observation for the
first-generation page; its `dryRun`/`preview` form performs no real observation.

## Apply

```http
POST /_tmax/action
{ "action": "apply_target", "target": "…", "dryRun": true }
```

The page applies one declared Harness through `apply_target` with a `dryRun`
boolean: `true` previews and `false` or absent executes. The page always sends
the preview first and executes only after the user confirms it.

- `TARGET` must be one declared Harness in the current state. A target that is not
  declared is a `409` typed refusal, not an invented Harness and not a silent
  fallback to the configured DSH target.
- The preview runs `tmax agent apply TARGET --dry-run`. It must not write, read a
  secret, or resolve one, and it discloses the planned file writes, secret
  reads, processes started, locks created, and files it would change.
- Execution runs one `tmax agent apply TARGET` operation and writes that
  Harness's actual configuration through tmax's owning integration. For the DSH
  Harness these are the `llm-pi-ai` providers in the selected profile's
  `cordis.patch.yml` and the adjacent credential reference; another Agent's
  Harness writes only what that Agent's own application contract owns. It does
  not install anything, edit tmax configuration, or create or rename DSH
  profiles.
- Apply remains authoritative for that Harness: it overwrites tmax-managed
  actual configuration from the current desired configuration, preserves
  unmanaged fields, retains that target's single-level restore snapshot, and
  records the applied target revision.
- Applying one Harness never applies or writes another Harness. The page has no
  page-wide Apply.
- The result carries the typed result, changed and unchanged actual files,
  applied state, recovery, effects, blockers, and supported next actions. It must
  not carry a key value or actual file contents.
- The response carries a freshly composed read model. The page must not report a
  route as live because a file was written; the managed profile route becomes
  live only when DSH reloads those settings.

Apply is the only bare actual-file write this contract permits. `add_model` and
`remove_model` write desired configuration only and never apply; an observation
publishes evidence; [`resume_quota` and `reset_failure`](#actions) are explicit
tmax mutations, not DSH or actual-file writes. Gateway start, stop, and restart are
separate previewed actions on tmax's inference service; Agent restore,
provider authentication, Harness declaration or removal, provider/model
definition edits, and profile or preset changes are not page operations and
remain explicit CLI or tmax Web operations.

The legacy `POST /_tmax/connect` form still previews and executes `agent apply`
for the configured DSH target. It requires an explicit `dryRun`/`preview`
boolean, or a `?dry-run=1`/`?preview=1` query that previews only, and refuses a
request that states neither with `400` `invalid_dry_run` before any tmax call.
It is retained for existing callers; the page's Harness rows use `apply_target`.

## Actions

```http
POST /_tmax/action
{ "action": "observe_provider", "provider": "…", "scope": "quota", "dryRun": true }
```

The page's explicit target-selection, provider, gateway, and management
operations run through one allowlisted action surface. The browser names one
action and its closed operand set; it never names a command, flag, or path. The
host owns the exact argv, the per-action timeout, and the single-flight key.

| Action | Operands | Preview | Execution |
| --- | --- | --- | --- |
| `add_model` | one declared Harness target, one provider id, one model id, one protocol | Dry-run plan derived from the catalog and Harness naming the target entry, protocol, and step names; writes nothing | Saves desired configuration only: create the target entry when the Harness has no matching entry, then add the model selection. Never runs `agent apply`. |
| `remove_model` | one declared Harness target, one provider id, one model id, one protocol | Dry-run plan derived from the Harness naming the target entry and all protocol steps; writes nothing | Saves desired configuration only: removes every protocol selection for that provider/model from the Harness entry that holds it, as one host-fixed sequence. Refuses when more than one entry holds the model. Never runs `agent apply`. |
| `apply_target` | one declared Harness target | `agent apply TARGET --dry-run` | Writes that Harness's actual configuration. |
| `connect` | none; the configured DSH target | `agent apply TARGET --dry-run` | Legacy compatibility form: writes the configured DSH target's actual files. |
| `observe_provider` | one provider id; `scope` is `quota`, `auth`, `models`, or `all`, default `quota` | Provider refresh dry run | Publishes provider evidence. |
| `resume_quota` | one provider id | `provider quota-resume --dry-run` | Publishes an explicit tmax mutation that clears a quota suspension. |
| `reset_failure` | one provider id | `provider failure-reset --dry-run` | Publishes an explicit tmax mutation that clears a failure override. |
| `open_management` | none | `tmax web status` | Starts or reuses the management listener and reports its ready URL; it does not start inference, reload configuration, or launch a browser. |
| `gateway_start` | none | `gateway start --dry-run` | Starts the tmax inference gateway when no compatible owner is running, or reuses a compatible running owner. |
| `gateway_stop` | none | `gateway stop --dry-run` | Stops the tmax inference gateway, not the Web listener, and reports interrupted work. |
| `gateway_restart` | none | `gateway restart --dry-run` | Recycles the tmax inference gateway through its native manager and reports readiness. |

The page uses this endpoint for target-selection, provider, gateway, and
management actions. Its whole-surface Refresh still uses
`POST /_tmax/observe`; the legacy `POST /_tmax/connect` form remains supported
for the configured DSH target. The allowlist also accepts per-provider
observation so one surface can carry it.

Rules:

- The page offers `resume_quota` only when the provider reports a suspension and
  `reset_failure` only when the provider reports an active failure. It offers
  `gateway_start` whenever the configured gateway is not ready, including when
  readiness is unknown and when tmax's own blocker asks for the start; that
  preview reports the current blocker instead of performing anything. It offers
  no `observe_provider`, `open_management`, `gateway_stop`, or `gateway_restart`:
  Refresh reads every account with a quota source at once, and the rest belong
  to the CLI. The action route still accepts them for other callers. No
  evidence, no action.
- A provider operand must be exactly one provider id from the current read model.
  Anything else is refused before argv. `scope` is a closed set.
- `dryRun: true` previews; `dryRun: false` executes. The page previews
  `apply_target` before it executes and runs an `add_model` or `remove_model`
  save directly; the host still answers a save dry run with its plan. The page
  must not chain one action into another.
- Single-flight is shared across the action and compatibility forms. The legacy
  connect path, `add_model`, and `remove_model` share one desired-configuration
  write lock, so a target write does not interleave with another. `apply_target`
  locks one Harness: a duplicate apply of that Harness is refused as busy while
  different Harnesses stay independent. Whole-surface observation, per-provider
  observation, and each provider's repair respect one lock set; management
  preview and execution have their own lock; each gateway verb locks itself.
- A gateway lifecycle action requires a readable current state and a configured
  gateway. An unavailable state is `502 state_unavailable`; an unknown gateway
  section is `409 gateway_unavailable`; an unconfigured gateway is
  `409 gateway_not_configured`. A preview must not start, stop, or restart
  anything. Lifecycle calls have a longer deadline than provider repairs because
  they may wait on supervision and readiness.
- Gateway lifecycle is an explicit, previewed operation on tmax's own inference
  service; it does not apply or restore actual files, change configuration, or
  manage the DSH host.
- The action surface does not widen ownership of tmax state: `apply_target` is
  the only bare actual-file write, `add_model` and `remove_model` are
  desired-configuration writes, observation publishes evidence, and the two
  repairs are explicit tmax mutations.
- `add_model` is offered only for a declared Harness and a catalog model that is
  enabled, has a published protocol, and is not already selected for that
  protocol in that Harness. The provider must be in the current catalog; a
  relayed provider additionally requires a configured gateway. `remove_model`
  requires a declared Harness and a selection that exists in it. A selection
  write needs the Harness's own inspection to have answered; a declared Harness
  whose entries cannot be read is refused instead of guessed.
- The client never names an entry, a command, or a flag. The host reuses a
  Harness entry that references the named provider and already serves the
  protocol, then one with no model yet; otherwise it derives a free target-local
  entry name. A removal that would match more than one entry refuses as
  ambiguous instead of choosing one. A DSH provider-namespace conflict is an
  apply-time refusal, not a client guess.
- A save writes desired configuration only and never applies; the Harness's
  `unapplied` flag then follows its target revision comparison. The host derives
  the target, entry, protocol, and step names from the current effect-free reads.
  The first refusal stops it and reports the completed prefix and failed step.
  There is no rollback: a partial desired-configuration write is visible in the
  fresh state.
- The page never chains one action into another; each is one allowlisted action
  whose internal steps are fixed by the host, not a browser-driven sequence.
  `remove_model` may remove several protocol selections for one model as one
  host-fixed step sequence, stopping at the first refusal. There is no page-wide
  apply, contextual apply, UI resume, or pending management.

Action responses:

| Outcome | Status | Body |
| --- | --- | --- |
| Completed action or preview | `200` | `ok: true` with the typed `result` and a freshly read `state`. |
| Typed tmax refusal | `409` | `ok: false` with the refusal code and message, the typed `result`, and a freshly read `state`. For `add_model` and `remove_model`, `result.data` also names the completed `steps` and `failedStep`; a prior step may have written desired state. |
| Provider not in the current read model | `409` | Typed refusal only; no `result` and no `state`, because the provider is not addressable. A typed tmax refusal that happens to report an unknown provider is still an attempted operation and keeps its `result` and `state`. |
| Target not declared | `409` | The typed target or inventory code as a refusal only; no `result` and no `state`, because a selection or apply cannot name a Harness. The codes are `unknown_target` for a target outside the current `harnesses`, `harnesses_unavailable` when the list section failed, and `target_uninspectable` when a selection write needs a Harness whose own inspection failed. A target absent from the current `harnesses` is this refusal, not a new Harness. |
| Gateway not configured or its state unavailable | `409` | `gateway_not_configured` or `gateway_unavailable` as a typed refusal only; no `result` and no `state`, because the lifecycle cannot be attempted. |
| Busy | `409` | `code: busy`; no `result` and no `state`, because the action did not run. |
| Invalid request | `400` | Typed code and message; no `result` and no `state`. |
| Current read model unavailable | `502` | `code: state_unavailable`; no `result` and no `state`, because the provider cannot be addressed. |
| Transport, timeout, or missing JSON envelope | `502` | Typed `result` and `state` when a state read is available; `state` may be `null` if even the fallback read fails. |
| Unexpected host failure after the attempt began | `502` | `code: action_failed` with a typed `result`; `state` when one is available. A route-level unexpected failure returns the typed result but no state. |

A freshly read state is the invalidated read after the attempt, not the cached
pre-action model. When the forced read fails, a provider action may fall back to
the state it used to address the provider, and a response may carry `state: null`
only when no read is available at all. The host must not claim success for a
refusal, and no outcome carries secret material.

## Evidence

| Evidence | Required rule |
| --- | --- |
| Unknown, absent, zero | A missing reading is unknown. An unsupported source is reported as no supported source, with its last attempt when the host reports one; it is not the same as no observation. A zero reading is reported as zero. None of these may render as another, and none means unlimited. |
| Observation time | The state carries the time from the reading's own data; the page shows only when the state was generated, and the response time, the request time, and the browser clock never substitute for a reading's time. |
| Freshness | Use tmax's reported freshness and notes. The page must not invent a staleness threshold or relabel a reading stale by its own clock. |
| Catalog availability | Availability is tmax evidence about its own routing, not a DSH availability claim and not a request result. |
| Subscription and renewal | Show the plan and renewal only when tmax reports them. An absent subscription stays absent; it is not a zero allowance. |
| Configuration revision | Not shown on the page; a CLI read. A revision change is not proof that an agent applied or reloaded it. |
| State schema | The plugin state `schemaVersion` is gated, not displayed: an unsupported value is a visible failure, not a guess. |
| Read-model source | Not shown on the page; a CLI read. A failed source read is a visible error, never a silently empty section. |
| Route readiness | `active` means DSH currently serves a route with that provider name. It is independent of configured route membership and of credential readiness, and the read model reports each separately; the page does not present a single target verdict. |
| tmax providers | One card per provider tmax lists that reports a quota window, closest to its limit first; a provider without a window takes no card. A failed provider read is named where the cards would be, never read as "no account". |
| Quota window | Draw a share only from a computable fraction. No fraction means text only, never a bar. The bar, the printed value, and the accessible label describe the same quantity: the share used. A reported used amount wins over subtracting what is left, and the quota prints beside the used amount — the reported quantity, or 100% for a percentage window. |
| Quota tone and reset | Colour marks what needs attention, never decoration: usage warns from 75% used and errors from 90%. A reset within a day (today or tomorrow) states its clock, any other day states its date, and the year appears only when it is not the current one. An unqualified epoch is read as Unix milliseconds and printed as approximate with its caveat beside it. |
| History | Availability and reason come from the state; the retained list is an explicit read, never a mount fetch. Missing history is missing evidence, not proof of no requests; a typed unavailable answer is not an empty table; a successful read with zero retained records is a reported zero. |
| History coverage | Show the process scope, retained count, range, storage limits, eviction, the numeric incomplete-capture count (positive is a gap, zero is none reported), prior-process and direct-traffic coverage, and material gaps. Unknown coverage stays unknown, and a gap is a warning rather than a normal state. |
| Action availability | Offer an action only when the reported evidence calls for it. No evidence, no action. |
| Action result | A preview and its execution report the typed code, status, message, data, blockers, next actions, and effects. An apply preview is not execution; a save execution is not an actual-file write. Invalid operands, unknown provider, a gateway guard, busy, an unavailable read model, a tmax refusal, and transport failure are distinct outcomes; only an attempted operation carries a typed result and a fresh state. |
| Desired configuration | A successful add or remove result changes tmax desired configuration only; it writes no actual configuration and is not evidence that a Harness changed its files. A later apply result is the only actual-file evidence for that Harness. |
| Harness status | A Harness row's `待应用` and `已应用` come from that row's `unapplied` and `appliedState`; `待重载` appears only when `appliedState` reports the retained record awaiting the Agent reload. `待应用` means the Harness has no completed apply of its current target revision, including a retained record that is incomplete or restored. An unknown `unapplied` renders as unknown, never as applied or unapplied. `待应用` is never a native pending change, a merged state, or a count of changed model items; the state carries no per-model applied set, so the page must not render one. |
| Seat truth | A seat may render only facts from the same host read model as the page: unknown is not zero, an unreadable or failed state is not a ready marker, and a throttled or missing state is stated. A count derived from a failed or unavailable section renders unknown; only a readable empty list is zero. No seat invents readiness, counts, or model availability. |
| Command output | The `/tmax` command reports the same facts as the read model, distinguishes unknown from zero, states history unavailability with its reason, and never renders a failed read as an empty successful result. |
| Blockers and errors | Code first, bounded message. A blocker is a fact about the reported state, not a mutation. |

## State consumer contract

The plugin state contract is plugin-owned and has `schemaVersion: 5`. It is not a
tmax CLI contract: the host composes it from effect-free tmax reads and adds the
DSH-only facts tmax cannot know. `null` means unknown or not reported unless the
row says otherwise; it is never a stand-in for zero or for an empty list.

| Field | Type | Meaning |
| --- | --- | --- |
| `ok` | boolean | Whether the host produced a usable read model. A failed target inventory keeps `ok` true when the provider-list or configuration-validation read answered, and sets it false only when neither answered; the failure is still a `projection` error with a null target. Section-level failures inside a usable model are reported in `errors` and do not hide the model; an invalid configuration is reported inside `config`. |
| `errors` | array of `{section, code, message}` (strings) | One entry per failed section read. Sections are `projection` (the target inventory and route source), `harnesses` (the declared-target list and its per-target reads), `authority` (tmax data directory and CLI version), `providers`, `catalog`, `gateway`, and `config`; Empty when every read answered. Per-provider, per-Harness, and per-model failures stay in their own records; this list is for section-level reads only. A section must show its own entry and must not render an empty result in place of a failed read. |
| `schemaVersion` | integer | Plugin state schema, currently `5`. |
| `generatedAt` | string | RFC 3339 time when the host composed this state. |
| `configurationRevision` | string or null | tmax saved-configuration revision. |
| `tmaxVersion` | string or null | Producing CLI version. |
| `authority` | string or null | tmax data directory. |
| `target` | object or null | The configured DSH target dossier, kept for the DSH-native seats and route ownership: `id` and `path` (string or null), `default` (object or null), `nativeDefault` (object or null), `effectiveDefault` (object or null), `appliedState` (string or null), `blockers` (array of `{code, message}`), and `installation` (`{version, state, source}` with string or null members, or null). A failed target inventory yields a `projection` error and a null target; `ok` stays true when the provider-list or configuration-validation read answered and is false only when neither answered. A missing native default stays null and is not an error. |
| `harnesses` | array | Every declared target from `tmax agent list`, one row per target: `id`, `agent` and `path` (string or null), `providerCount` and `modelCount` (integer or null), `defaultConfigured` (boolean or null), `appliedState` (string or null), `unapplied` (boolean or null), `blockers` (array of `{code, message}`), `default` (the explicit default selection object or null), and `providers` (array of `{name, sourceProvider, relayed, models: [{id, protocol}]}` in inspection order). The list read fixes the row set and its compact counts; `agent inspect TARGET` supplies `appliedState`, `unapplied`, `default`, `providers`, and blockers. A target whose inspection failed stays a row with a typed blocker and null inspected facts; an empty readable target list is a reported empty array. `unapplied` is null when the comparison is unknown, including an unavailable target scope; a null value is never rendered as current or as applied. A failed list read is a `harnesses` section error beside whatever rows were reported. |
| `ownership` | object | `owner` (`settings`, `unapplied`, `other`, `none`, or `unknown`), `scope` (`profile_scoped` or null), `reason` (`apply_not_run` or null), and `conflict` (`{code, message}` or null). The page registers nothing, so this reports who serves the declared routes rather than what the page did. |
| `gateway` | object or null | `configured` and three-valued `ready` (boolean or null), `listen` (string or null), `daemonDiscoveryState` (`identified`, `unidentified`, or `unknown`), `reasonCode` (string or null), `supervision` (`state`, `reasonCode` strings or null), `autostart` (`savedIntent` boolean or null; `registration` string or null; `unitMatches` boolean or null), and `sharedKey` (`loadedState` `loaded`, `absent`, or `unknown`; `reasonCode` string or null). |
| `config` | object | `state` (`valid`, `invalid`, or `unavailable`), `revision` (string or null), and `errors` (array of `{code, message}`). An absent saved configuration is a valid empty configuration; a missing revision stays null rather than invented. |
| `routes` | array | Per route: `name` and `provider` (string or null), `protocol` (string or null), `baseUrl` (string), `credentialRef` (string or null), `credentialConfigured` (`true`, `false`, or null), `scope` (`home_wide`, `profile_scoped`, or null), `models` (array; each `id`, `name`, `kind`, `continuity`, `availability` strings or null, `candidateCount` integer or null, and `capabilities` object; the target inspection currently reports no kind, continuity, or candidate count, so those stay null, and availability stays `unverified`), `active` (boolean), and `blockers` (array of `{code, message, models?}`). Route `blockers` name reasons the declared route cannot serve (an unsupported protocol, model, or missing usable model); they are empty when the route is usable. |
| `providers` | object | One `tmax` array holding every provider tmax lists: `id` and `name` strings, `enabled` (boolean or null), `authentication` (string or null), `source` (`upstream`, `gateway`, or null), `affectedTargets` (integer), `accountIdentity` (string or null), `subscription` (object or null), `integration` (string or null), `billing` (object), `suspension` (object), `failure` (object), and `error` (`{code, message}` or null). A provider's `billing` carries `reasonCode` and `lastAttempt` only when the listed attempt reported them (for example `unsupported_source` with `lastAttempt: unavailable`), so a provider with no supported source is distinct from one that was never observed. |
| `catalog` | object | The model catalog. `state` is `available` only when the provider list and `model list` both answered, otherwise `unavailable`; `entriesState` is `available` or `unknown` (the target dossier); `default` is `{provider, model, protocol}` or null; `providers` is an array of `{id, name, source, integration, enabled, entry, entries, models, totalCount, joinedCount}`. `entries` is null when the target dossier is unavailable, otherwise an array of `{name, relayed, protocols}`; `entry` is the first matching entry name or null. Each model is `{id, kind, enabled, availability, protocols, publishedProtocols, declaredProtocols, joined, joinedProtocols, default}`; `declaredProtocols` is `null` when the source does not report it. `joined` and `joinedCount` are null when the target dossier is unknown, never false. A provider-list failure makes the whole catalog `unavailable` rather than empty; the `providers` section error carries that failure. `totalCount` is null when the model read failed. A model-list failure is a `catalog` section error, not an empty provider list. The `entry`, `entries`, `joined`, and `joinedCount` fields describe the configured DSH target only; the per-Harness controls derive membership from `harnesses`, not from these fields. |
| `routing` | array | One row per projected model: `model`, `protocol`, `kind`, `continuity`, `availability` (string or null), `candidateCount` (integer or null), `selection` (`{qualifiedModel, provider, model, priority}` or null), `candidates` (array of `{qualifiedModel, priority, selected}`), `excluded` (array of `{qualifiedModel, reasonCode}`), and a typed `error` (`{code, message}` or null). |
| `diagnostics` | object | `readModel` (`inventory`), `history` (`state` `available` or `unavailable`; `reasonCode` string or null), `lastProjectionAt` (string or null; the last inventory read time), and `lastError` (string or null). |

Consumers:

- A consumer must gate `schemaVersion` and refuse an unsupported value rather
  than infer field meaning. It must not require absent fields.
- The `tmax_status` tool returns the same facts compactly, including the read
  model source. It must not observe. The seats and `/tmax` command consume the
  same host read model and must not add separate business rules for its facts.
- The page must not duplicate host mapping or synthesize a missing section.
- The host must not invent a section the failed reads did not produce; a failed
  section keeps its `errors` entry and its consumers render it as unavailable.

## Web route security

- Routes are exact paths. The plugin declares no prefix, catch-all, or file
  route.
- Every route requires authenticated DSH admission. The plugin must not declare
  an anonymous route.
- A POST request is admitted only with a same-origin statement when it carries
  one, a JSON media type, and a body of at most 16 KiB. One operation of each
  kind runs at a time; a duplicate is refused as busy. A wrong method, a failed
  origin check, a wrong media type, and an oversized body have distinct typed
  refusals.
- The action route accepts one allowlisted action with closed operands only. A
  Harness named by `add_model`, `remove_model`, or `apply_target` must be in the
  current state; a provider, model, and protocol named by `add_model` must be
  exact catalog members, while `remove_model` names one exact selection held by
  that Harness, and the host removes that provider/model's protocol selections
  from the Harness. The entry and argv are host-derived. No client string reaches
  argv. Single-flight covers each Harness's apply,
  selection writes, the legacy connect path, observation, provider repairs,
  management, and each gateway verb; a duplicate is busy.
- The plugin must not add permissive CORS headers or wildcard origin admission.
- No secret appears in a response, log, argv, environment, or error text. Action
  and Apply results are filtered to non-secret fields, and host error text is
  bounded and code-prefixed before it reaches the browser.
- The browser half stores no tmax data in local or session storage.
- Outbound vendor destinations use an exact host and path allowlist. A configured
  base URL that does not match is reported as having no supported source; it must
  not receive the provider's key or a guessed request.
- The page offers no key entry, key reveal, or sign-in control.

## Non-goals

- Provider, model, alias, priority, schedule, or default editing, and declaring,
  renaming, or removing a Harness. `add_model` and `remove_model` are the only
  target-selection writes; `apply_target` is the only actual-file write.
- Credential entry, reveal, cleanup, or interactive sign-in.
- Agent restore; provider authentication; DSH start/stop/restart; profile,
  preset, and session management. Agent application is the contracted
  per-Harness Apply action; it is not configuration authority. Gateway start is
  the one contracted gateway page action; stop and restart are CLI operations.
- A page-wide or multi-Harness apply.
- Automatic observation, automatic apply, or arbitrary action chaining;
  `add_model`, `remove_model`, and `apply_target` are fixed operations.
- Request bodies, headers, URLs, model output, an error-log viewer, and history
  clear or export controls. The page shows no history; `/tmax history` lists
  retained metadata only.
- Session import, export, or synchronization.
- Embedding the tmax Web interface, or opening it from the page; `tmax web open`
  is the CLI's, and the page duplicates nothing.

## Compatibility

- Verified baseline: DSH `0.2.3-alpha.2` and tmax `0.1.0`. The plugin declares
  `engines.dsh` `>=0.2.3-alpha.2 <0.3.0`. The
  [DSH reference](../../../docs/agents/dsh.md#evidence-and-runtime-limits)
  records the evidence and its limits. Only a tested version is qualified; a
  declared engine range is not evidence. The home configuration tmax writes has a
  separate, older source qualification in that reference.
- The read model is composed from the effect-free commands named in
  [Reads](#reads); no snapshot command is required. An older tmax whose
  inspection lacks the relayed-entry or native-capability fields is reported as
  unsupported, with the next action being to update tmax.
- The Harness flow requires a tmax whose `agent inspect` reports the target
  revision, the applied target revision, and `unapplied`. An absent or unknown
  value renders as unknown, never as applied.
- Seats require the DSH 0.2.3-alpha.2 client slot declarations named in
  [Seats](#seats). The optional `/tmax` command requires the host `commands`
  service; without it no command is registered and the page, seats, tools, and
  routes are unaffected.
- Another DSH version requires new evidence, not a changed version claim. A DSH
  plugin API change may require an explicit port.
- The state contract composes tmax's existing reads. It must not add a tmax
  command, change CLI output, or become a tmax contract by being consumed.
