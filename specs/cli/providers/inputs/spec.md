# Provider input commands

This child owns input grammar and result projections. The
[declaration schema](../../../configuration/spec.md#provider-input-declarations)
owns saved types/references. The [input lifecycle](../../../model-routing/inputs/spec.md)
owns normalization, acquisition, publication, freshness, and runtime use. Use
[common CLI rules](../../spec.md), [configuration changes](../../configuration/spec.md),
and [shared results](../../results/spec.md). Inputs remain provider-scoped, not a
global account registry or an authored input document.

## Provider input operations

`PROVIDER` is a declared provider ID; `INPUT` is its exact immutable input name.

| Command | Result or requested change |
| --- | --- |
| `provider input list PROVIDER [--state STATE]` | Name-ordered compact source, freshness, acquisition, and current-revision summaries. |
| `provider input inspect PROVIDER INPUT [--state STATE]` | Complete declaration, current data/unavailable reason, last attempt, saved/current-runtime use, dependencies, and acquisition blockers. |
| `provider input add PROVIDER INPUT INPUT-DEFINITION` | Declare a new channel after covered models are registered; reject an existing name. |
| `provider input replace PROVIDER INPUT INPUT-DEFINITION` | Replace the entire existing declaration under its immutable name, removing omitted optional fields. |
| `provider input remove PROVIDER INPUT` | Remove the declaration after rule-reference checks, without cascade or source/credential effects. |
| `provider input refresh PROVIDER INPUT` | Acquire once from the saved declaration's qualified nonmanual source, normalize, and conditionally publish; do not edit/activate config or start services. |
| `provider input record PROVIDER INPUT --observed-at INSTANT --valid-until INSTANT --reset-at INSTANT [--remaining-percent DECIMAL \| --remaining DECIMAL --total DECIMAL --unit UNIT] [--reference URL] [--if-input-revision REVISION]` | Atomically replace manual current data, or report identical-record success without extending freshness. |
| `provider input clear PROVIDER INPUT [--if-input-revision REVISION]` | Publish a new unavailable revision, preserving declaration/rules and preventing late-fetch resurrection. |

`INPUT-DEFINITION` requires `--kind KIND --source SOURCE --scope SCOPE
--model MODEL-ID... --max-age DURATION`; optional `--refresh-every DURATION`
sets automatic acquisition. `KIND` is `quota` or `reset`; `SOURCE` is `manual`,
`provider`, or `codex-resets`. Repeat `--model` at least once with unique exact
upstream IDs registered under this provider, never qualified names or serialized
lists. Scope is one nonempty operand; provider input `--scope` is not a native path.
Durations use [common elapsed-unit grammar](../../spec.md#duration-operands) and
persist `max_age_seconds` and `refresh_interval_seconds`. Manual sources reject
`--refresh-every`. Replacement requires all required flags again; omitting the
optional interval removes it. An unchanged replacement neither refreshes data nor
extends freshness.

Unknown sources and intrinsically invalid schema combinations are invalid input.
A structurally valid but unqualified integration/source/kind/scope/model combination
remains visibly unavailable and cannot fetch. Add/replace/remove use ordinary
file guards and complete-candidate activation. Removing an input requires its
named rules to be removed first.

Refresh/record/clear are operational-data mutations with `--dry-run`, not config
edits. A config revision is not a data revision. They require a valid saved
declaration and recheck stable provider identity, authorization generation,
declaration revision, and current-data revision at publication. Without a resident
they persist under `~/.tmax` for later matching use. A saved/loaded declaration
difference must report current runtime use or pending/non-use without activation.

Record is manual-only; refresh is nonmanual-only. Quota requires exactly one
representation: `--remaining-percent DECIMAL` in inclusive 0–100, or the complete
`--remaining DECIMAL --total DECIMAL --unit UNIT` group with
`0 <= remaining <= total` and `total > 0`. Do not mix representations. Reset rejects
all quota operands. Counters describe one common documented unit/window, not
mixed balances or request-history estimates. Unit is nonempty without control
characters. Decimal uses the lifecycle's exact nonnegative decimal-string grammar:
no sign, exponent, NaN, infinity, or fixed precision cap. Never round counters
into a percentage or invent a denominator.

Instants require explicit-offset RFC 3339 and normalize to UTC. Receipt time,
manual observation basis, provider/input identity, kind, scope, models, source,
and revisions come from operation/declaration, not record flags. Validate lifecycle
freshness/expiry without extending or correcting conflicting facts. Optional
reference is safe absolute HTTPS provenance without credentials; it is not a fetch
destination, authentication source, or provider verification. Omission removes
prior manual provenance. Manual record needs no credentials.

Optional `--if-input-revision` checks the current channel revision at serialized
publication. `absent` means never published, not expired or cleared; mismatch
rejects without publication. Omitting it retains mandatory internal rechecks.
Lifecycle rules reject older/conflicting observations and late fetches; record
does not merge fetched data. Identical record retains revision and original
receipt/freshness. Clear works for any source, does not disable its interval, and
does not prevent a later separately begun refresh from publishing.

Refresh distinguishes qualified absence/withdrawal from transport, parse, or
credential failure. Failure records an attempt without refreshing prior data;
unknown never means zero quota. Qualified same-identity credential refresh may run
under existing source coordination, without interactive login or implicit secret resolution.
Automatic acquisition is opt-in through the interval and runs only with the
existing service; inspect reports paused without starting it. Dry-run never
fetches, consumes protected input, resolves secrets, or creates/reserves data,
attempts, or revisions.

These independent examples assume an existing provider/model. Record examples
assume publication at `2026-10-23T10:00:00Z`, not timeless valid data:

```sh
tmax provider input add opencode-go weekly-allowance --kind quota --source manual --scope example-weekly-allowance --model deepseek-v4.1-flash --max-age 1h
tmax provider input add opencode-go reset-announcement --kind reset --source manual --scope example-reset-announcement --model deepseek-v4.1-flash --max-age 1d
tmax provider input list opencode-go
tmax provider input inspect opencode-go weekly-allowance
tmax provider input record opencode-go weekly-allowance --observed-at 2026-10-23T09:55:00Z --valid-until 2026-10-23T10:55:00Z --reset-at 2026-10-23T12:00:00Z --remaining-percent 75.0
tmax provider input record opencode-go weekly-allowance --observed-at 2026-10-23T10:00:00Z --valid-until 2026-10-23T11:00:00Z --reset-at 2026-10-23T12:00:00Z --remaining 1 --total 3 --unit requests
tmax provider input record opencode-go reset-announcement --observed-at 2026-10-23T09:55:00Z --valid-until 2026-10-23T12:00:00Z --reset-at 2026-10-23T12:00:00Z
tmax provider input replace opencode-go weekly-allowance --kind quota --source manual --scope example-weekly-allowance --model deepseek-v4.1-flash --max-age 30m
tmax provider input clear opencode-go weekly-allowance --if-input-revision INPUT-REVISION
```

For a qualified nonmanual declaration, `tmax provider input refresh PROVIDER INPUT
--dry-run` and direct execution address only that channel. Setup uses
`--source provider --scope ADVERTISED-SCOPE` or the qualified Codex source, actual
registered models, and an optional explicit `--refresh-every 15m`. Help substitutes
actual capabilities, not guessed integrations, units, reset dates, or URLs.

## Input data results

Normalized [input records](../../../model-routing/inputs/spec.md#input-records),
[last attempts](../../../model-routing/inputs/spec.md#last-acquisition-attempt), and
[use conditions](../../../model-routing/inputs/spec.md#validity-and-applicability)
are the type/enum authority, not extensible adapter maps. Percentages, quota
counters, and thresholds are JSON decimal strings, never floats. Quota data exposes
exactly the supplied percentage or complete counters/unit representation.
Revisions are opaque, not timestamps or array positions. `absent` means known
never-published data; null means unavailable/inapplicable, not expired/cleared.

`InputSummary` contains `input`, `kind`, `source`, `current_revision`,
`availability` (normalized `InputAvailability`), `acquisition_state`, and
`last_attempt_outcome` (null or the retained outcome). The provider, selected
state, and common UTC evaluation instant occur once per result. List output does
not repeat complete declarations, evidence, saved/runtime projections, or inspect
argv per input. Stable IDs plus help identify the detail command.

`InputDetails` adds `provider`, `scope`, `models`, `declaration_revision`, complete
`declaration`, `current_data` (`InputHead` or null), `last_attempt`
(`AcquisitionAttempt` or null), `last_attempt_state` (`absent`, `known`, or
`unavailable`), and `dependencies` (`{qualified_model, rule_id}[]`). A null record
with revision `absent` is never-published; a null unavailable revision is unreadable.
Use these plain result names; the lifecycle's internal current-record terminology
does not rename its stored types or fields.

Details also include:

- `saved_use = {state, reason_code}`. State is `applicable`, `not_applicable`,
  `no_saved_declaration`, or `unavailable`.
- `current_runtime_use = {state, reason_code}`. State is `applicable`, `pending`,
  `no_loaded_snapshot`, or `unavailable`.
- `acquisition = {state, policy_configuration_revision,
  active_refresh_interval_seconds, next_due_at, reason_code}`. State is `manual`,
  `on_demand`, `scheduled`, `in_progress`, `paused`, `not_loaded`, or `unavailable`.
  Active interval is positive integer-or-null; revision/due/reason are string-or-null.

Both use projections compare the selected binding with their respective config
view, regardless of `--state`; availability separately reports data usability.
Different saved binding is `not_applicable`; a valid saved view without it is
`no_saved_declaration`; invalid/unreadable saved state is `unavailable`. A loaded
inspection still exposes saved divergence.

Acquisition names the matching active loaded policy and interval, including an
old interval retained after an unactivated scheduling edit. Pending/invalid saved
edits or failed activation do not pause that loaded policy. `not_loaded` means no
matching loaded automatic policy for the selected saved declaration, not that
another retained loaded incarnation stopped. Use `--state loaded` to inspect it.
`paused` means automatic acquisition lacks a running service. Due time promises
neither source freshness nor priority recovery.

Read-only list/inspect results include `runtime = {snapshot_state,
configuration_revision, reason_code}`. Snapshot state is `available`, `absent`, or
`unavailable`; unavailable resident discovery is not a stopped service or a known
missing loaded snapshot. File-state inspection retains its local declaration and
saved data while qualifying `current_runtime_use` and acquisition as unavailable.
Loaded-state reads without a retrievable snapshot report absence or unavailability
explicitly rather than an empty list of declared inputs. Reads never require
credentials, refresh a source, or start the service.

| Operation | Command-specific `data` |
| --- | --- |
| Input list | `provider`, `state`, `snapshot_state`, UTC `at`, `inputs: InputSummary[]`; absent loaded state is explicit, not an empty declared-provider list. |
| Input inspect | `provider`, `state`, `snapshot_state`, UTC `at`, `input: InputDetails`. |
| Add/replace/remove | `provider`, `input_name`, change outcome, resulting declaration revision when present, and saved/loaded outcome; no mandatory full detail record. |
| Record/clear/refresh | `provider`, `input_name`, `input_change`; refresh also returns exact terminal `attempt` or explicit unavailable reason. Include current runtime use when saved/loaded bindings differ. |

`input_change` contains `outcome` (`published`, `unchanged`, `cleared`, `withdrawn`,
`not_published`, or `planned`), `current_revision_before`, `current_revision_after`,
`published` (boolean-or-null), and `attempt_recorded` (boolean-or-null). Never invent
revisions for unchanged/rejected data. Record/clear have no acquisition attempt.
Refresh returns its terminal attempt even if changed bindings prevent attachment
to the current declaration. Fetch completion, attempt persistence, publication,
and current runtime use are separate facts; partial/unknown results preserve all
material boundaries. Dry-run reports current/proposed data and full config impact
where relevant, planned outcome, false publication/attempt flags, and no reserved
revisions, timestamps, fetch, or retained attempt.

Reads may exit 0 with stale/unavailable data, paused acquisition, or a failed last
attempt. Successful record/clear/withdrawal and identical-record no-op exit 0.
A known qualification/credential/in-flight prerequisite blocks refresh with exit 3;
transport/parse/persistence failure exits 4, disclosing attempt/earlier credential
effects. Qualified withdrawal is not failure or proof of quota recovery. Binding
conflict uses `inputs_changed` or its scoped reason without overwriting newer data.
