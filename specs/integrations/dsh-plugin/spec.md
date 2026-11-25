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
access. tmax's CLI and Web remain where provider and model definitions are
authored. The plugin may add one existing catalog model to its one declared DSH
target through the fixed, previewed `add_model` target-selection action; it does
not edit providers, models, aliases, priorities, or schedules, does not manage
DSH profiles or presets, and is not a second tmax authority. The browser half
never runs tmax; the host half reads it, and its only target-selection write is
the fixed `add_model` action. Its other mutations are previewed observation,
provider repair, and gateway lifecycle actions; it never edits a provider,
model, alias, priority, or schedule definition.

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
6. The plugin does not author tmax provider, model, alias, priority, or schedule
   definitions and does not manage DSH profiles. It may add one existing model
   selection to its declared DSH target through `add_model`; Apply remains
   tmax's own native-application operation, not a plugin-authored file format.
7. Every action the page offers is allowlisted and previewed. The browser names
   one action and closed operands, never a command, flag, or path. `add_model`
   is one fixed composite target-selection operation with per-step results; it
   is not arbitrary action chaining.
8. The read model source is explicit: `diagnostics.readModel` reports
   `inventory`, the composition of effect-free tmax reads. There is no snapshot
   source and no fallback source; a failed read is a visible error, never an
   empty successful section.
9. A seat is additive: it registers only on a declared DSH slot, never replaces
   DSH-owned content, never changes a DSH selection or setting, and never acts
   without an explicit preview and confirmation.

## Page content

The page is one first-level DSH page with three tabs: **Models**, **Gateway**,
and **Quota**. It answers whether model access is working and how much of each
account's quota is spent, and exposes the one control that adds an existing tmax
model to the declared DSH target. Configuration revisions, schemas, route
detail, full gateway supervision, diagnostics, and retained request history stay
in the CLI.

| Section | Required content |
| --- | --- |
| Header | The page name, when the state was generated, one reachability verdict, and the one control that changes it when the page has it: **Start gateway** when the gateway is not ready, **Apply** when DSH has not been given the declared routes or a credential tmax knows is missing. Otherwise one line names what stands in the way — a pending reload, an invalid or unavailable configuration with its own errors, a route-ownership conflict, or a tmax blocker — followed by every failed section read and every route blocker, bounded and code first, with a **Retry** that reads again and never observes. **Refresh** observes every listed provider that reports a quota source. A failed read keeps the last good table, labels it last known, and shows the failure once. The tab bar follows the header; switching tabs never reads or acts. |
| Models | Every tmax provider and every concrete model and alias under it, with the target's selection ratio `joined/total`; no provider-level "joined" or "not joined" text. Search matches provider name/id, model id, and protocol, and shows results grouped by provider. A provider drills into its model rows: model id, published protocols, default mark, joined mark, and an **Add to DSH** control for an unselected, enabled model with a published protocol. The control is offered; the action refuses a pending or unreadable target state before any write, and the pending refusal's add bar offers **Apply current changes first**, which opens the ordinary Apply preview instead of adding a page-level Apply button. After that Apply succeeds, the page may restore the Models tab, clear the search, reopen the same provider, and reopen the same add preview once. That is a UI resume, not a second action; the Add still waits for an explicit confirmation. The page's target default is marked. |
| Gateway | The configured and ready state, listener address, shared-key loaded state, and tmax's reason code when not ready; the **Start gateway** control when configured and not ready. It shows no supervision, autostart, or other CLI detail. |
| Quota | One card per tmax account that reports a quota window, closest to its limit first; an account without any window takes no card. Every window the account reports is one bar filled with the share it has used, stating its name, what it used of what, and when it resets, whatever basis the account reports in: a percentage window states its share of 100%, a reported quantity states what was used of it; an unqualified reset time is printed as approximate, an absent one is left out. A window whose share cannot be computed is text, never a bar. A card carries a tag only for a state a person must act on or know about — disabled, suspended, failing — and the one action its evidence calls for: resume for a reported suspension, reset for an active failure. |
| Actions | The preview and result of one confirmed provider, gateway, Apply, or **Add to DSH** action: typed code, status, message, data, blockers, next actions, and effects. Preview and confirmation are separate; cancel clears. An add preview states the selected entry and protocol and writes nothing. An observation names only the accounts it could not read. |

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
| Settings models footer | `settings.models.footer` | One tmax card: gateway readiness, active and usable route counts, model count, a reason when tmax models are not usable, and the one control that helps. **Apply** appears only when the page's own next action is Apply (never for an invalid or unavailable configuration, a failed read, a pending reload, or an inert owner) and previews and then confirms the same native-application write as the page; **Reload** appears when the read failed or the page's next action is retry; **Open tmax** opens the plugin page only when that layout page is present. It reads the host state only. |
| Tool card | `tool.call.toolview` keyed `tmax_status` | A compact card rendered from the tool result already returned: target, routes, gateway, provider and model counts, blockers, and a failed/unreadable/pending state. It never performs a new read, observation, or action. |

Seat rules:

- Every seat read is an effect-free GET. A seat must not observe, apply, resolve
  a credential, or mutate tmax except through the explicit Apply preview and
  confirmation in the settings footer.
- Seat data is a one-shot GET taken when the seat mounts, served from a short
  shared client cache for seats mounted together. A seat has no subscription or
  timer: an already-mounted marker or footer can lag a newer page read, an action
  result, or a state change until that seat remounts or the page reloads. The
  only seat actions that take a fresh read are the settings footer's explicit
  Reload and Apply.
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
| `status` | State readability, target and effective default, applied and pending state, route count and per-route liveness and credential readiness, gateway readiness, provider count, and declared blockers. This is also the empty-input default. |
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
serves the same names.

- The state reports the owner (`settings`, `unapplied`, `none`, `other`), the
  scope (`profile_scoped` when the settings carry them), and a `route_owner_conflict`
  when another provider already serves the declared route names.
- The page never registers, replaces, or disposes an LLM route, and it never
  mounts an adapter of its own.
- Every declared route stays in the state read model with its blockers; a route
  with no usable model, credential, or base URL is reported, not hidden.
- No credential value ever reaches the page. The state names a credential by
  reference and reports whether it is configured; the page draws the verdict a
  missing credential implies and never the reference itself.

## Reads

| Route | Method | Contract |
| --- | --- | --- |
| `/_tmax/state` | GET | The host read model for the page: one plugin-owned state contract composed from effect-free tmax reads, plus DSH-only facts. Cache only. |
| `/_tmax/status` | GET | Compatibility summary of the same read model for the first-generation page. Cache only. |
| `/_tmax/usage` | GET | Compatibility provider and usage rows derived from the same read model. Cache only. |
| `/_tmax/history` | GET | One explicit, read-only retained-history read: compact request metadata, coverage, and bounded rows. It never observes, refreshes, starts a service, clears, or writes. |

Read purity:

- The host composes the state from effect-free tmax reads: `agent list`,
  `agent inspect`, `status`, `gateway status`, `provider list`,
  `provider inspect`, `config validate`, `model list`, and `model explain`. It
  must not run `provider refresh`, request a vendor quota endpoint, run
  `agent apply`, or resolve a credential value.
- There is no versioned snapshot source and no compatibility composition: the
  read model is the same composition on every supported tmax version. An older
  tmax whose inspection lacks the relayed-entry or native-capability fields is
  reported as unsupported, not silently projected with guessed fields.
- A GET may read redacted native configuration state and route credential
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
DSH or native write; [Apply](#apply) is the only such write.

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
POST /_tmax/connect
{ "dryRun": true }
```

The request states its intent explicitly with a `dryRun` or `preview` boolean:
`true` previews and `false` executes; a preview statement wins when the two
forms disagree. A `?dry-run=1`/`?preview=1` query previews only. A request that
states neither is refused with `400` `invalid_dry_run` before any tmax call.

- The preview runs `tmax agent apply TARGET --dry-run`. It must not write, read a
  secret, or resolve one, and it discloses the planned native writes, secret
  reads, processes started, locks created, and files it would change.
- Execution runs one `tmax agent apply TARGET` operation and writes the target's
  native DSH files: the `llm-pi-ai` providers in the profile's
  `cordis.patch.yml` and the credential reference. It does not install anything,
  edit tmax configuration, or create or rename DSH profiles.
- `TARGET` is the configured target, or the one declared DSH target whose path
  matches the resolved home. An ambiguous or absent match is a `409` typed
  refusal, not an invented target.
- The result carries the typed result, changed and unchanged native files,
  applied state, recovery, effects, blockers, and supported next actions. It must
  not carry a key value or native file contents.
- The response carries a freshly composed read model. The page must not report a
  route as live because a file was written; the home integration becomes live
  only when DSH reloads the home entry.
- Apply is the only bare native write this contract permits; `add_model` is the
  one fixed target-selection write and ends in the same Apply. An observation
  publishes evidence; [`resume_quota` and `reset_failure`](#actions) are explicit
  tmax mutations, not DSH or native writes. Gateway start, stop, and restart are
  separate previewed actions on tmax's inference service; native restore,
  provider authentication, provider/model definition edits, and profile or
  preset changes are not page operations and remain explicit CLI or tmax Web
  operations.

## Actions

```http
POST /_tmax/action
{ "action": "observe_provider", "provider": "…", "scope": "quota", "dryRun": true }
```

The page's explicit provider, gateway, and management operations run through one
allowlisted action surface. The browser names one action and its closed operand
set; it never names a command, flag, or path. The host owns the exact argv, the
per-action timeout, and the single-flight key.

| Action | Operands | Preview | Execution |
| --- | --- | --- | --- |
| `connect` | none; the configured target | `agent apply TARGET --dry-run` | Writes the target's native DSH files. |
| `add_model` | one provider id, one model id, one protocol | Catalog-derived plan; writes nothing | Fixed target-selection sequence: create the provider entry when no matching entry exists, add the model selection, then `agent apply TARGET`. |
| `observe_provider` | one provider id; `scope` is `quota`, `auth`, `models`, or `all`, default `quota` | Provider refresh dry run | Publishes provider evidence. |
| `resume_quota` | one provider id | `provider quota-resume --dry-run` | Publishes an explicit tmax mutation that clears a quota suspension. |
| `reset_failure` | one provider id | `provider failure-reset --dry-run` | Publishes an explicit tmax mutation that clears a failure override. |
| `open_management` | none | `tmax web status` | Starts or reuses the management listener and reports its ready URL; it does not start inference, reload configuration, or launch a browser. |
| `gateway_start` | none | `gateway start --dry-run` | Starts the tmax inference gateway when no compatible owner is running, or reuses a compatible running owner. |
| `gateway_stop` | none | `gateway stop --dry-run` | Stops the tmax inference gateway, not the Web listener, and reports interrupted work. |
| `gateway_restart` | none | `gateway restart --dry-run` | Recycles the tmax inference gateway through its native manager and reports readiness. |

The page uses this endpoint for provider, gateway, and management actions. Its
Apply form still uses `POST /_tmax/connect`, and its whole-surface Refresh still
uses `POST /_tmax/observe`; those two routes remain supported compatibility forms
for the same apply and observation operations. The allowlist also accepts
`connect` and per-provider observation so one surface can carry them.

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
- `dryRun: true` previews; `dryRun: false` executes. Every action the page offers
  previews before it executes, and the page must not chain one action into
  another.
- Single-flight is shared across the action and compatibility forms: connect,
  whole-surface observation, per-provider observation, and each provider's
  repair respect one lock set, so a second concurrent attempt is refused as
  busy. Distinct providers' repairs stay independent; management preview and
  execution have their own lock; each gateway verb locks itself.
- A gateway lifecycle action requires a readable current state and a configured
  gateway. An unavailable state is `502 state_unavailable`; an unknown gateway
  section is `409 gateway_unavailable`; an unconfigured gateway is
  `409 gateway_not_configured`. A preview must not start, stop, or restart
  anything. Lifecycle calls have a longer deadline than provider repairs because
  they may wait on supervision and readiness.
- Gateway lifecycle is an explicit, previewed operation on tmax's own inference
  service; it does not apply or restore native files, change configuration, or
  manage the DSH host.
- The action surface does not widen ownership of tmax state: `connect` is the
  only bare native write, `add_model` is the one fixed target-selection write,
  observation publishes evidence, and the two repairs are explicit tmax
  mutations.
- `add_model` is offered only for a catalog model that is enabled, has a
  published protocol, and is not already selected for that protocol. The
  provider must be in the current catalog; a relayed provider additionally
  requires a configured gateway. The target must resolve.
- The client never names an entry, a command, or a flag. The host reuses an
  existing target entry whose protocol matches; otherwise it derives a free
  target-local entry name. A DSH provider-namespace conflict is an Apply-time
  refusal, not a client guess.
- The add preview is the catalog-derived plan (entry, protocol, and step names);
  it writes nothing and sends no target-selection command, but a cold or expired
  state cache is composed with the same effect-free reads as any state read.
  The action refuses `pending_native_changes` when the target already has pending
  native changes and `target_uninspectable` when that pending state is unreadable;
  a standalone Apply must run first, so the add cannot silently carry unrelated
  selections, defaults, roles, or native edits. Confirmation runs
  the fixed sequence over that clean target plus the new selection; the first
  refusal stops it and reports the completed prefix and failed step. There is no
  rollback: a partial desired-state write is visible in the fresh state, and the
  page can Apply or repair it. The action shares the `connect` single-flight lock
  because both write the target.
- The page never chains one action into another; `add_model` is one allowlisted
  action whose internal steps are fixed by the host, not a browser-driven
  sequence. The one UI exception is the contextual Apply for a refused Add: its
  success may reopen the same Add preview, never execute the Add, and never form
  a browser-driven provider/model/apply chain.

Action responses:

| Outcome | Status | Body |
| --- | --- | --- |
| Completed action or preview | `200` | `ok: true` with the typed `result` and a freshly read `state`. |
| Typed tmax refusal | `409` | `ok: false` with the refusal code and message, the typed `result`, and a freshly read `state`. For `add_model`, `result.data` also names the completed `steps` and `failedStep`; a prior step may have written desired state. |
| Provider not in the current read model | `409` | Typed refusal only; no `result` and no `state`, because the provider is not addressable. A typed tmax refusal that happens to report an unknown provider is still an attempted operation and keeps its `result` and `state`. |
| Target not resolved | `409` | The typed target or inventory code (for example `target_unresolved`) as a refusal only; no `result` and no `state`, because Apply cannot name a target. |
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
| Route readiness | `active` means DSH currently serves a route with that provider name. It is independent of configured route membership and of credential readiness, and the read model reports each separately; the page folds them into its verdict rather than listing them. |
| tmax providers | One card per provider tmax lists that reports a quota window, closest to its limit first; a provider without a window takes no card. A failed provider read is named where the cards would be, never read as "no account". |
| Quota window | Draw a share only from a computable fraction. No fraction means text only, never a bar. The bar, the printed value, and the accessible label describe the same quantity: the share used. A reported used amount wins over subtracting what is left, and the quota prints beside the used amount — the reported quantity, or 100% for a percentage window. |
| Quota tone and reset | Colour marks what needs attention, never decoration: usage warns from 75% used and errors from 90%. A reset within a day (today or tomorrow) states its clock, any other day states its date, and the year appears only when it is not the current one. An unqualified epoch is read as Unix milliseconds and printed as approximate with its caveat beside it. |
| History | Availability and reason come from the state; the retained list is an explicit read, never a mount fetch. Missing history is missing evidence, not proof of no requests; a typed unavailable answer is not an empty table; a successful read with zero retained records is a reported zero. |
| History coverage | Show the process scope, retained count, range, storage limits, eviction, the numeric incomplete-capture count (positive is a gap, zero is none reported), prior-process and direct-traffic coverage, and material gaps. Unknown coverage stays unknown, and a gap is a warning rather than a normal state. |
| Action availability | Offer an action only when the reported evidence calls for it. No evidence, no action. |
| Action result | A preview and its execution report the typed code, status, message, data, blockers, next actions, and effects. A preview is not execution. Invalid operands, unknown provider, a gateway guard, busy, an unavailable read model, a tmax refusal, and transport failure are distinct outcomes; only an attempted operation carries a typed result and a fresh state. |
| Seat truth | A seat may render only facts from the same host read model as the page: unknown is not zero, an unreadable or failed state is not a ready marker, and a throttled or missing state is stated. A count derived from a failed or unavailable section renders unknown; only a readable empty list is zero. No seat invents readiness, counts, or model availability. |
| Command output | The `/tmax` command reports the same facts as the read model, distinguishes unknown from zero, states history unavailability with its reason, and never renders a failed read as an empty successful result. |
| Blockers and errors | Code first, bounded message. A blocker is a fact about the reported state, not a mutation. |

## State consumer contract

The plugin state contract is plugin-owned and has `schemaVersion: 3`. It is not a
tmax CLI contract: the host composes it from effect-free tmax reads and adds the
DSH-only facts tmax cannot know. `null` means unknown or not reported unless the
row says otherwise; it is never a stand-in for zero or for an empty list.

| Field | Type | Meaning |
| --- | --- | --- |
| `ok` | boolean | Whether the host produced a usable read model. A failed target inventory keeps `ok` true when the provider-list or configuration-validation read answered, and sets it false only when neither answered; the failure is still a `projection` error with a null target. Section-level failures inside a usable model are reported in `errors` and do not hide the model; an invalid configuration is reported inside `config`. |
| `errors` | array of `{section, code, message}` (strings) | One entry per failed section read. Sections are `projection` (the target inventory and route source), `authority` (tmax data directory and CLI version), `providers`, `catalog`, `gateway`, and `config`; Empty when every read answered. Per-provider and per-model failures stay in their own records; this list is for section-level reads only. A section must show its own entry and must not render an empty result in place of a failed read. |
| `schemaVersion` | integer | Plugin state schema, currently `3`. |
| `generatedAt` | string | RFC 3339 time when the host composed this state. |
| `configurationRevision` | string or null | tmax saved-configuration revision. |
| `tmaxVersion` | string or null | Producing CLI version. |
| `authority` | string or null | tmax data directory. |
| `target` | object or null | Target dossier: `id` and `path` (string or null), `default` (object or null), `nativeDefault` (object or null), `effectiveDefault` (object or null), `appliedState` (string or null), `pendingNativeChanges` (object, array, or null), `blockers` (array of `{code, message}`), and `installation` (`{version, state, source}` with string or null members, or null). A failed target inventory yields a `projection` error and a null target; `ok` stays true when the provider-list or configuration-validation read answered and is false only when neither answered. A missing native default stays null and is not an error. |
| `ownership` | object | `owner` (`settings`, `unapplied`, `other`, `none`, or `unknown`), `scope` (`profile_scoped` or null), `reason` (`apply_not_run` or null), and `conflict` (`{code, message}` or null). The page registers nothing, so this reports who serves the declared routes rather than what the page did. |
| `gateway` | object or null | `configured` and three-valued `ready` (boolean or null), `listen` (string or null), `daemonDiscoveryState` (`identified`, `unidentified`, or `unknown`), `reasonCode` (string or null), `supervision` (`state`, `reasonCode` strings or null), `autostart` (`savedIntent` boolean or null; `registration` string or null; `unitMatches` boolean or null), and `sharedKey` (`loadedState` `loaded`, `absent`, or `unknown`; `reasonCode` string or null). |
| `config` | object | `state` (`valid`, `invalid`, or `unavailable`), `revision` (string or null), and `errors` (array of `{code, message}`). An absent saved configuration is a valid empty configuration; a missing revision stays null rather than invented. |
| `routes` | array | Per route: `name` and `provider` (string or null), `protocol` (string or null), `baseUrl` (string), `credentialRef` (string or null), `credentialConfigured` (`true`, `false`, or null), `scope` (`home_wide`, `profile_scoped`, or null), `models` (array; each `id`, `name`, `kind`, `continuity`, `availability` strings or null, `candidateCount` integer or null, and `capabilities` object; the target inspection currently reports no kind, continuity, or candidate count, so those stay null, and availability stays `unverified`), `active` (boolean), and `blockers` (array of `{code, message, models?}`). Route `blockers` describe only the profile-scoped fallback this plugin owns or attempts to register; they are empty when it registers nothing. |
| `providers` | object | One `tmax` array holding every provider tmax lists: `id` and `name` strings, `enabled` (boolean or null), `authentication` (string or null), `source` (`upstream`, `gateway`, or null), `affectedTargets` (integer), `accountIdentity` (string or null), `subscription` (object or null), `integration` (string or null), `billing` (object), `suspension` (object), `failure` (object), and `error` (`{code, message}` or null). A provider's `billing` carries `reasonCode` and `lastAttempt` only when the listed attempt reported them (for example `unsupported_source` with `lastAttempt: unavailable`), so a provider with no supported source is distinct from one that was never observed. |
| `catalog` | object | The model catalog. `state` is `available` only when the provider list and `model list` both answered, otherwise `unavailable`; `entriesState` is `available` or `unknown` (the target dossier); `default` is `{provider, model, protocol}` or null; `providers` is an array of `{id, name, source, integration, enabled, entry, entries, models, totalCount, joinedCount}`. `entries` is null when the target dossier is unavailable, otherwise an array of `{name, relayed, protocols}`; `entry` is the first matching entry name or null. Each model is `{id, kind, enabled, availability, protocols, publishedProtocols, declaredProtocols, joined, joinedProtocols, default}`; `declaredProtocols` is `null` when the source does not report it. `joined` and `joinedCount` are null when the target dossier is unknown, never false. A provider-list failure makes the whole catalog `unavailable` rather than empty; the `providers` section error carries that failure. `totalCount` is null when the model read failed. A model-list failure is a `catalog` section error, not an empty provider list. |
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
  provider, model, and protocol named by `add_model` must be exact catalog
  members; the entry is host-derived. No client string reaches argv.
  Single-flight covers connect and `add_model` under the same target-write lock,
  plus observation, provider repairs, management, and each gateway verb; a
  duplicate is busy.
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

- Provider, model, alias, priority, schedule, or default editing; the one fixed add of an existing catalog model to the declared target is the only target-selection write.
- Credential entry, reveal, cleanup, or interactive sign-in.
- Native restore; provider authentication; DSH start/stop/restart; profile,
  preset, and session management. Native application is the contracted Apply
  action, also used by the fixed `add_model` sequence; it is not configuration
  authority. Gateway start is the one contracted gateway page action; stop and
  restart are CLI operations.
- Automatic observation, automatic Apply, or arbitrary action chaining; `add_model` is the one fixed composite.
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
- Seats require the DSH 0.2.3-alpha.2 client slot declarations named in
  [Seats](#seats). The optional `/tmax` command requires the host `commands`
  service; without it no command is registered and the page, seats, tools, and
  routes are unaffected.
- Another DSH version requires new evidence, not a changed version claim. A DSH
  plugin API change may require an explicit port.
- The state contract composes tmax's existing reads. It must not add a tmax
  command, change CLI output, or become a tmax contract by being consumed.
