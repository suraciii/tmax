# tmax

Use tmax-managed model access inside DeepSeek Harness. The plugin projects the
DSH target tmax declares into DSH: a first-level page, three seats on DSH-owned
pages, a read-only `/tmax` command, one read tool, and one skill. It reads
tmax's effect-free inventory — `agent list`, `agent inspect`, `status`,
`gateway status`, `provider list`/`provider inspect`, `config validate`,
`model list`, and `model explain` — and never copies keys, starts the gateway,
or becomes a second provider/model definition authority. Its one write adds an
existing catalog model to the declared DSH target through a fixed, previewed
target-selection action.

Qualified against DSH **0.2.3-alpha.2** and tmax **0.1.0**: the plugin's host and
client API fit, the composed inventory read model, the action and command tests,
and a temporary-profile page render with a synthetic inventory were exercised at
that revision. The tabbed catalog page was also rendered from this worktree in
the live `web` profile against the real tmax 0.1.0 store: all 11 providers, the
19 concrete models and 9 aliases, joined ratios, search, provider drill-in, the
gateway tab, and the quota tab rendered. An Add and an Apply preview were
exercised; neither wrote. A live `add_model` execution was not run: the real
target reports pending native changes, and the action refused with
`pending_native_changes` by design. The earlier temporary-profile render did not
use a real format-2 tmax store. The package declares `engines.dsh`
`>=0.2.3-alpha.2 <0.3.0`; no other DSH version has been tested. The contract is owned by the
[DSH plugin specification](../specs/integrations/dsh-plugin/spec.md); the
[DSH reference](../docs/agents/dsh.md#evidence-and-runtime-limits) records what
was and was not exercised, including the browser surfaces that remain
unverified. That reference also carries the older source qualification of the
home configuration tmax writes (DSH 0.2.1-rc.2), which is separate from this
bundle's evidence.

## Install or update

The plugin is this repository's package: the manifest is the repository root's
`package.json`, which points DSH's exports, bundle patch, and packed files at the
modules in `dsh-plugin/`. The locale files sit beside the manifest at `locale/`,
because DSH reads a bundle's display text and this document from those fixed
package paths. It is not published to npm, so DSH installs it straight from
GitHub.

In DSH Web, open **Plugins** > **Add plugin** and paste the repository address:

```text
https://github.com/suraciii/tmax
```

The CLI installs and selects the same bundle in one step:

```sh
dsh plugin --profile web add https://github.com/suraciii/tmax
```

Restart or refresh DSH Web, then open **tmax** in the sidebar panel list, beside
Plugins and Schedules. The plugin needs a `tmax` executable on `PATH` and a
configured DSH target; a configured gateway is needed to serve models, not to
load or read the page. Updates follow the repository's default branch; uninstall
removes the profile dependency:

```sh
dsh plugin --profile web update tmax
dsh plugin --profile web remove tmax
```

Working from a checkout instead, `just dsh-install` links this repository into
the profile, so source edits reach DSH at its next recomposition; `just dsh-remove`
uninstalls it. The plugin used to be the `dsh-plugin/` directory; a profile that
still links it needs one more `just dsh-install`, because that directory no
longer carries a manifest.

## Configuration

Configuration is optional. The plugin resolves the DSH home from `home`, then
`DSH_HOME`, then `~/.dsh`, and selects the tmax target from `target`, or by that
home when `target` is empty.

| Key | Meaning |
| --- | --- |
| `home` | DSH home to inspect. Defaults to `DSH_HOME`, then `~/.dsh`. |
| `target` | Declared tmax target ID. When empty, the one DSH target whose path matches the home is used. |
| `refreshSeconds` | Inventory refresh interval in seconds; an integer of at least 10, default `60`. It only re-reads tmax; it never observes a provider. |
| `tmaxPath` | `tmax` executable to run. Defaults to `tmax` on `PATH`. |

```yaml
- id: tmax
  config:
    home: /home/you/.dsh
    target: dsh-main
```

## Inventory source

The host half composes one read model from effect-free tmax reads, each with a
bounded deadline:

| Command | Facts it owns |
|---|---|
| `tmax agent list` | Which declared target matches the DSH home, when the plugin has no explicit `target`. |
| `tmax agent inspect <TARGET>` | The target's relayed provider entries, their selected models and native projection, applied and pending state, configuration revision, and tmax's own blockers. |
| `tmax status` | The tmax data directory (reported as the state's authority) and the caller's tmax version. |
| `tmax gateway status` | Daemon discovery state, gateway readiness, listen address, supervision, autostart, and shared-key load state. |
| `tmax provider list` | The tmax provider rows: authentication, affected targets, integration, quota state, and suspension/failure summaries. |
| `tmax provider inspect <ID>` | One provider's billing observations, freshness, quota suspension, and failure detail. |
| `tmax config validate` | Whether the saved configuration is valid, its revision, and its errors. |
| `tmax model list` | Every configured concrete model and gateway alias with its published protocols, availability, and kind. |
| `tmax model explain <MODEL> --protocol <P>` | The selected route candidate, priorities, exclusions, and availability for one projected model. |

A failing inspect is reported as an error, with tmax's own code and message,
rather than as an empty page. A configured gateway whose daemon is absent or
stopped is reported as `ready: false` with `gateway_not_ready`, not as unknown;
readiness stays unreported only when the `gateway status` read itself fails. The
routes the target already declares stay visible either way. There is no
`tmax dsh` command and no snapshot contract: the plugin composes these reads
directly, and `diagnostics.readModel` reports the source as `inventory`. An
older tmax whose inspect predates the relayed-entry or native capability fields
is reported as unsupported, with the next action being to update tmax.

## Route ownership

`tmax agent apply` writes the providers into a profile's `cordis.patch.yml`
(row `llm-pi-ai`, path `config.providers.<entry>`), and DSH's own adapter
serves them. The page therefore registers no route of its own: it reads the
target inventory, lists what apply wrote, and reports each route's blockers.
The state's `ownership.owner` is `settings` when those providers are present,
`unapplied` when apply has not written them, and `other` when something else is
already serving the same names.

The bundle declares `index.js`, `client.js`, and this document from those fixed
package paths. It is not published to npm, so DSH installs it straight from
GitHub.

In DSH Web, open **Plugins** > **Add plugin** and paste the repository address:

```text
https://github.com/suraciii/tmax
```

The CLI installs and selects the same bundle in one step:

```sh
dsh plugin --profile web add https://github.com/suraciii/tmax
```

Restart or refresh DSH Web, then open **tmax** in the sidebar panel list, beside
Plugins and Schedules. The plugin needs a `tmax` executable on `PATH` and a
configured DSH target; a configured gateway is needed to serve models, not to
load or read the page. Updates follow the repository's default branch; uninstall
removes the profile dependency:

```sh
dsh plugin --profile web update tmax
dsh plugin --profile web remove tmax
```

Working from a checkout instead, `just dsh-install` links this repository into
the profile, so source edits reach DSH at its next recomposition; `just dsh-remove`
uninstalls it. The plugin used to be the `dsh-plugin/` directory; a profile that
still links it needs one more `just dsh-install`, because that directory no
longer carries a manifest.

## Apply

On the **tmax** page, select **Apply**. This first requests a preview
(`POST /_tmax/connect` with `{"dryRun": true}`, `{"preview": true}`, or
`?dry-run=1`/`?preview=1`), which runs `tmax agent apply TARGET --dry-run` and
writes nothing; the preview discloses the planned native writes, secret reads,
started processes, created locks, and the files it would change. **Apply** then
sends `{"dryRun": false}` (or `{"preview": false}`) and runs
`tmax agent apply TARGET`, writing the target's native DSH files: the providers
in the profile's `cordis.patch.yml` and the credential reference in the
adjacent `.credentials.yaml`. A request that states neither a
preview nor an execution boolean is refused before any tmax call, and a preview
query never executes. The result reports changed and unchanged files, applied
state, recovery, blockers, next actions, and effects. The key never enters the
browser, a log, or a response, and native file contents never cross.

The terminal equivalents are:

```sh
tmax agent apply "$TARGET" --dry-run
tmax agent apply "$TARGET"
```

Apply is the only bare native write this page offers. The **Add to DSH** action
is the one fixed target-selection write: it creates the target entry when needed,
adds one model selection, and applies the target. Observation publishes provider
evidence, which can feed tmax's automatic quota suspension, but writes no DSH or
native state. Quota resume and failure reset are separate previewed tmax
mutations. Gateway start, stop, and restart are separate previewed tmax
operations; native restore, provider authentication, provider/model definition
edits, and profile or preset changes stay explicit CLI or tmax Web operations.
The page never chains one action into another; `add_model` is one allowlisted
action with a fixed host sequence. A successful contextual Apply may reopen the
same Add preview once, but never executes the Add; that is a UI resume, not a
second action.

## Actions

The page uses `POST /_tmax/action` for provider, gateway, and management
actions. The browser names one action and its closed operands only; the host
owns the exact tmax argv, timeout, and single-flight key. The page's Apply still
uses `POST /_tmax/connect`, and its whole-surface Refresh still uses
`POST /_tmax/observe`; those two routes remain supported compatibility forms for
the same apply and observation operations.

| Action | Where it appears | Preview | What confirmation does |
| --- | --- | --- | --- |
| **Observe quota** (`observe_provider`) | Not offered by the page; Refresh reads every listed provider with a quota source at once | Provider refresh dry run | Publishes provider evidence; scope is `quota` by default. |
| **Resume quota** (`resume_quota`) | Only a provider that reports a suspension | `provider quota-resume --dry-run` | Explicit tmax mutation that clears the suspension. |
| **Reset failure** (`reset_failure`) | Only a provider with an active failure | `provider failure-reset --dry-run` | Explicit tmax mutation that clears the failure override. |
| **Open management** (`open_management`) | Not offered by the page; the CLI opens the management UI | `tmax web status` listener readiness | Starts or reuses the tmax management listener and reports its ready URL. It does not start inference, reload configuration, or launch a browser. |
| **Start gateway** (`gateway_start`) | Configured gateway that is not ready | `gateway start --dry-run` | Starts the tmax inference gateway, or reuses a compatible running owner. |
| **Stop gateway** (`gateway_stop`) | Not offered by the page; the CLI stops a gateway | `gateway stop --dry-run` | Stops the tmax inference gateway (not the Web listener) and reports interrupted work. |
| **Restart gateway** (`gateway_restart`) | Not offered by the page; the CLI restarts a gateway | `gateway restart --dry-run` | Recycles the tmax inference gateway through its native manager and reports readiness. |
| **Apply** (`connect`) | The Apply next action | `tmax agent apply TARGET --dry-run` | Writes the target's native DSH files. The page uses its own Apply panel. |
| **Add to DSH** (`add_model`) | A model row in the Models tab that is enabled, has a published protocol, and is not selected. The preview refuses a target with pending or unreadable native changes. | Catalog-derived plan: entry, protocol, and step names; writes nothing | Fixed sequence: `agent provider add` when no matching entry exists, `agent model add`, then `agent apply TARGET`. It refuses `pending_native_changes` or `target_uninspectable`; apply the target first. The first refusal stops it and reports the completed prefix and failed step; no rollback. |

Every action is previewed first and then explicitly confirmed or cancelled. A
preview writes nothing and is reported separately from execution. The verdict
line offers **Start gateway** whenever a configured gateway is not ready,
readiness unknown included; that preview reports the current blocker instead of
doing anything. A readiness that is unknown because the gateway read itself
failed offers a retry instead, never Start. Stopping or restarting a gateway is
a CLI operation. A gateway action needs a readable current state and a
configured gateway. Results show the typed
code, status, message, data, blockers, next actions, and effects from tmax.
Provider IDs are taken exactly from the current read model, and no client string
reaches tmax argv.

The outcomes are distinct:

- `200` — the action or its preview completed; the body carries the typed
  `result` and a freshly read state.
- `409` — either tmax refused with a typed result and fresh state, or the request
  named a provider outside the current read model, hit a busy action, or asked
  for gateway lifecycle while the gateway is unconfigured or its state is
  unavailable; those guards carry a typed refusal but no result or state.
- `400` — the request itself was invalid; no result or state.
- `502` — the current read model is unavailable (`state_unavailable`, no result or
  state), or the attempt failed after it began: tmax did not answer or the host
  itself failed (typed result and state when a read is available).

A fresh state is a new read after the attempt, not the cached pre-action model.
Action paths are covered by host/action tests; the temporary-profile HTTP checks
are recorded in the [DSH reference](../docs/agents/dsh.md#evidence-and-runtime-limits).
The pre-port page was rendered in both light and dark themes and in English and
Chinese locales, and the ported page was rendered in a temporary profile with a
synthetic inventory executable. The live page's tabs, search, provider drill-in,
Add preview, and Apply preview were exercised in the browser. Non-dry-run
actions (including `add_model` execution and Apply confirmation), Refresh, the
dark theme, the phone layout, and seat interactions beyond the sidebar marker and
gateway card have not been browser-verified.

## The page

The plugin claims a first-level page: `sidebar.panellist` adds the **tmax** row
beside Plugins and Schedules, and `main` renders the page itself. The page has
three tabs — **Models**, **Gateway**, and **Quota** — and answers whether model
access is working, what tmax is configured with, and how much of each account's
quota is spent.

- **Header** — the page name, when the state was generated, one reachability
  verdict, and the one control that changes it when the page has it: **Start
  gateway** when the gateway is not ready, **Apply** when DSH has not been given
  the declared routes or a credential tmax knows is missing. Otherwise one line
  names what stands in the way — a pending reload, an invalid configuration with
  its own errors, a route-ownership conflict, a tmax blocker — followed by every
  failed section read and route blocker, code first, with a **Retry** that reads
  again and never observes. **Refresh** observes every listed provider that
  reports a quota source; a provider with none is not queried and never reads as
  a failure. A failed read keeps the last good table, labels it last known, and
  shows the failure once. The tab bar never reads or acts.
- **Models** — every tmax provider and every concrete model and alias under it,
  with the target's `joined/total` ratio. No provider-level "joined/not joined"
  label. Search matches provider name/id, model id, and protocol, and groups
  results by provider. A provider drills into its model rows: model id, published
  protocols, default mark, joined mark, and **Add to DSH** for an unselected,
  enabled model with a published protocol. The add preview is one line — the
  entry and protocol — and confirmation runs the fixed selection-and-apply
  sequence. If the target has pending native changes, the add bar refuses and
  offers **Apply current changes first**; that opens the ordinary Apply preview.
  After it succeeds, the page restores the Models view and reopens the same add
  preview. Pending alone adds no page-level Apply button.
- **Gateway** — configured and ready state, listener address, shared-key loaded
  state, and tmax's reason when not ready; **Start gateway** when configured and
  not ready. Full supervision and autostart detail stay in the CLI.
- **Quota** — one card per tmax account that reports a quota window, closest to
  its limit first. Every window the account reports is one bar filled with what
  it has used, its name, what it used of what, and when it resets, whatever
  basis the account reports in: a percentage window states its share of 100%, a
  reported quantity states what was used of it, and an unknown reset time is
  left out. A window whose share cannot be computed is text, never a bar. A card
  carries a tag only for a state a person must act on or know about (disabled,
  suspended, failing) and the one action its evidence calls for: **Resume
  quota** for a reported suspension, **Reset failure** for an active failure.
  Colour marks what needs attention — amber from 75% used, red from 90% — and a
  reset states its clock within a day, its date after that, and its year only
  when that is not the current one; an unqualified epoch reads as Unix
  milliseconds and prints as approximate with its caveat.

The page reports no revision, schema number, read-model source, full gateway
supervision, route protocol or base URL, or retained request history: each is a
CLI read. On a phone the account cards stack to one column, the tabs span the
row, and every bar spans its card; on a touch screen controls keep a comfortable
target.

## Seats

The plugin also adds four small surfaces to DSH's own pages. Each registers on a
declared slot only when the DSH composition provides it; missing packages simply
mean that seat is absent, and the plugin page keeps working. The bundle injects
the DSH client packages it needs: `@deepseek-ai/dsh-client-locale`,
`@deepseek-ai/dsh-client-ui-sidebar`, `@deepseek-ai/dsh-client-ui-layout`,
`@deepseek-ai/dsh-client-ui-conversation`, `@deepseek-ai/dsh-client-ui-settings-models`, and
`@deepseek-ai/dsh-client-ui-tool`.

| Seat | Where it appears | What it does |
| --- | --- | --- |
| Sidebar marker | Trailing slot of the **tmax** row in the sidebar panel list | A non-interactive dot with screen-reader text: an invalid configuration, `a blocker is reported`, `gateway not ready`, `quota suspended`, or `route conflict` as errors; an unavailable configuration, a failed section read, or no usable route as warnings; `unknown` when no state is readable. It never opens the page or runs an action. |
| Models footer card | **Settings > Models**, below the model list | Gateway readiness, active and usable route counts, model count, and the reason tmax models are or are not usable. **Apply** appears only when the page's own next action is Apply, then previews and confirms the same native DSH write as the page; **Reload** appears when the read failed or the next action is retry; **Open tmax** opens the plugin page when that page exists. |
| Tool card | The `tmax_status` tool call | A compact card of the result the tool already returned: target, routes, gateway, provider and model counts, and blockers, with pending, unreadable, and failed states. It never performs a new read. |

Seat reads are GET-only and never observe a provider, apply, or mutate tmax.
The only write from a seat is the settings card's explicit Apply preview and
confirmation. Seats never change a DSH selection, setting, or route; they only
add content on the slot they were given.

Seat data is a one-shot GET taken when the seat mounts (seats mounted together
share a short client cache). There is no subscription or timer, so an
already-mounted marker or footer can lag a newer page read, an action result, or
a state change until it remounts or the page reloads. The footer's
**Reload** and **Apply** are the only seat actions that read again.

A failed, unavailable, or non-ok read renders the affected seat counts as
unknown, never as zero: unknown routes, models, or providers are not "none
configured". Only a readable empty list counts as zero.

## /tmax command

When the host exposes DSH's optional `commands` service, the plugin registers
`/tmax`, a human read command. It never sends a model turn and never writes,
observes, refreshes, applies, or starts a service.

```text
/tmax [status|providers|routing|gateway|diagnostics|history]
```

Empty input shows `status`. The subcommands print the same facts as the page's
read model in bounded text:

- `status` — target and effective default, applied and pending state, routes and
  credential readiness, gateway readiness, provider counts, and blockers.
- `providers` — every tmax provider's authentication, integration, quota state,
  freshness, observation time, suspension, and failure. Unknown quota stays
  unknown, never zero.
- `routing` — every projected model's selection, candidates, exclusions, and any
  per-model error.
- `gateway` — readiness, listener, daemon discovery, supervision, autostart, and
  shared-key state.
- `diagnostics` — read-model source (`inventory`), state generation time,
  revision, tmax version, history availability, last inventory read time, last
  error, and every section error.
- `history` — a read-only `tmax history list`: coverage, range, matched and
  omitted counts, and a bounded record list. An unavailable daemon or history is
  reported with its reason, never as an empty history.

Output is bounded: at most 20 routes, 40 providers, 40 routing rows, 20 section
errors, and 10 history records; at most
10 blockers; each cell is capped at 120 characters. A truncated list states how
many rows were omitted. An unknown or multi-word subcommand returns an error
with the usage and performs no read. A failed or absent section prints
`unavailable (code: message)` for that section instead of zero counts. The
HTTP `GET /_tmax/history` route and this subcommand share one retained-history
contract; neither returns bodies, headers, or secrets, and the page shows no
history table.

## HTTP routes

The routes are exact paths on the DSH web server and use DSH's authenticated
route admission (`auth` defaults to `required`). The plugin adds same-origin,
JSON media-type, 16 KiB body-size, and single-flight checks to its POST actions.

| Route | Effect |
| --- | --- |
| `GET /_tmax/state` | The host read model used by the page: `ok`, `schemaVersion`, `generatedAt`, `configurationRevision`, `tmaxVersion`, `authority`, `target`, `ownership` including its mode, `gateway`, `config`, `routes`, `providers` (tmax), `catalog`, `routing`, `diagnostics` including `readModel` (`inventory`), and a top-level `errors` list naming every failed section read. Reads cached state only; `502` only when the host cannot produce any state. |
| `GET /_tmax/status` | First-generation page compatibility summary of the same read model. Reads cached state only. |
| `GET /_tmax/usage` | First-generation provider and usage rows from the same read model. Reads cached state only. |
| `GET /_tmax/history` | Explicit retained-history read. One read-only `tmax history list` (45 s) mapped to `{ ok, generatedAt, coverage, records, truncated?, omitted? }` with at most 50 compact rows; a typed unavailable daemon/history answer is `200` with `ok:false` and its reason (never an empty history), while a missing transport envelope is `502` `history_unavailable`. Bodies, headers, URLs, and secrets never cross. |
| `POST /_tmax/observe` | Explicit observation. Body `{ "dryRun": false }`. One quota-scope `tmax provider refresh` per provider that reports a quota source. Returns per-provider outcomes and fresh state: `200` when no item failed, `207` when one did, `409` when an observation is already running, and `502` when the host cannot run the observation at all. |
| `POST /_tmax/connect` | Explicit Apply. Body `{ "dryRun": true }` or `{ "preview": true }` previews through `tmax agent apply TARGET --dry-run`; `{ "dryRun": false }` or `{ "preview": false }` executes; a `?dry-run=1`/`?preview=1` query previews only. A request that states neither preview nor execution is `400` `invalid_dry_run` before any tmax call. Returns the typed result and fresh state (`200`). A typed refusal from tmax is `409` with the refusal and fresh state; an unresolved target is `409` with its typed code and writes nothing; a busy Apply is `409` `busy` with no state; a transport failure, timeout, or output that is not a JSON envelope is `502` `connect_unavailable`. |
| `POST /_tmax/action` | The allowlisted action surface used by the page for provider and management actions. Body `{ "action": "...", "provider"?: "...", "scope"?: "...", "dryRun": true }` for `connect`, `observe_provider`, `resume_quota`, `reset_failure`, `open_management`, `gateway_start`, `gateway_stop`, and `gateway_restart`; `add_model` additionally names `model` and `protocol`, and the host derives the entry. `200`: typed `result` plus fresh state. `409`: typed refusal plus fresh state, or an unknown-provider, gateway guard, or busy refusal with neither. `400`: invalid request with neither. `502`: `state_unavailable` with neither, or a transport/no-envelope failure with a typed `result` and state when a read is available. |
| `POST /_tmax/usage/refresh` | First-generation explicit tmax observation. Same rules as `/_tmax/observe`; a `dryRun`/`preview` body or `?dry-run=1`/`?preview=1` performs no real observation. |

`POST /_tmax/connect` and `POST /_tmax/observe` remain supported compatibility
forms: the page's Apply and Refresh still use them, and provider, gateway, or
management actions use `POST /_tmax/action`. The plugin also registers the
`tmax_status` tool and the `tmax` skill for agents. The tool reads the same
cached model and never observes.

No route returns key material. Responses, logs, argv, and error text are
secret-free, and action and apply payloads are filtered to non-secret fields.
The page keeps no tmax data in browser storage.

## Limitations

- Routes are registered only when the tmax target declares them; mixed-protocol
  entries are refused.
- A route with no usable credential reference or base URL is not registered. It
  stays visible with a typed blocker instead of being silently hidden.
- Models with unknown context capacity are hidden from DSH with a route blocker
  rather than given a guessed value; unknown input modalities are advertised as
  text-only.
- The page has no configuration editor: provider, model, alias, priority, and
  schedule edits stay in the tmax CLI or Web. Apply is its only bare DSH or
  native write; the fixed target-selection action adds one existing model and
  ends in Apply. Its other operations are observation, the previewed provider
  repairs, and the previewed gateway lifecycle actions.
- Request bodies, headers, URLs, model output, error-log viewing, and history
  clear or export controls are not shipped. The page shows no history;
  `/tmax history` lists retained metadata only, and a typed unavailable answer
  stays unavailable.
- The `/_tmax/state` payload is the plugin's own contract composed from tmax's
  effect-free reads; it is not a tmax CLI contract.
- Native restore, provider authentication, and profile/preset/session features
  are not page operations. Gateway start is the one gateway action the page
  offers, always previewed; stop, restart, and opening the management listener
  are CLI operations.
- The sidebar marker was seen in a real temporary DSH page render of the
  pre-port host read model, in both light and dark themes and in
  English and Chinese locales. The ported page was also rendered in a temporary
  DSH 0.2.3-alpha.2 profile with a synthetic inventory executable: `GET
  /_tmax/state` returned 200 with `diagnostics.readModel: "inventory"`, and the
  page rendered with route and gateway facts and action controls. No real
  format-2 tmax store was available. The settings footer, model option and pane,
  tool card, `/tmax` command invocation, a successful history-table load, and
  non-dry-run actions have not been browser-verified.
- Install/uninstall stays with DSH; tmax does not manage DSH profiles.

## License

Part of the tmax repository, distributed under GPL-3.0-only; see
[LICENSE](../LICENSE).
