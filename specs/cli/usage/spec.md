# Usage analytics commands

This child owns usage report, insight, and clear grammar and results. The
[usage analytics contract](../../usage-analytics/spec.md) owns behavior, evidence,
retention, and insight rules. Use [common CLI rules](../spec.md) and
[shared results](../results/spec.md).

## Commands

| Command | Result or requested boundary |
| --- | --- |
| `usage summary [--period PERIOD \| --since INSTANT --until INSTANT]` | One usage report: period, coverage, totals, per-provider cost estimates, allowance readings, and per-outcome counts. |
| `usage breakdown --by DIMENSION [--period PERIOD \| --since INSTANT --until INSTANT]` | The same report scope, with one aggregate row per value of `DIMENSION`. |
| `usage insights [--period PERIOD \| --since INSTANT --until INSTANT]` | Fired deterministic insights and refusals over the same scope. |
| `usage clear [--dry-run]` | Explicit removal of retained usage records and allowance readings; dry-run reports the same counts without deleting. |

`PERIOD` is `today`, `7d`, `30d`, `90d`, or `all`; the default is `today`. A
day-based period starts at the local midnight that begins its trailing calendar-day
window and ends at the read instant, using the operating system's time zone. `all`
covers the retained ledger and ends at the read instant. An explicit range uses
`[since, until)` instants and requires both bounds. `--period` excludes
`--since`/`--until`; one-sided ranges are invalid; the range must be increasing.

A usage record joins a period by admission time, or by receipt time when admission
did not occur. An allowance reading joins by its observation time. The result
states which basis applied.

`DIMENSION` is `provider`, `model`, `requested`, `target`, `protocol`, `outcome`,
`origin`, or `day`. `--by` is required for `usage breakdown` and takes exactly one value.
A `day` row uses a local calendar date as its `key`.

## Read behavior

Reads are effect-free. They must not start or contact the service, observe a
provider, refresh a credential, prune or rewrite the store, or change routing. CLI
reads work while the service is stopped.

An absent store, a corrupt or unreadable store, an unsupported store version, and
a failed read are distinct outcomes. Each is reported with a typed reason and its
coverage; none is an empty report or a zero total. An empty report states that no
records are retained in the period, not that no request ran.

Every report states that coverage is gateway-observed only: direct and native
upstream traffic is unobserved, explicit probes are separate, and diagnostic
request history is excluded. It discloses that token buckets are protocol-scoped
and exclusive, that cache buckets are never added to input, and that cost
estimates are frozen API-equivalent estimates rather than billed amounts. Usage
counters distinguish known zero, unknown, partial, and ambiguous. Per-provider
cost entries state their currency, price sources, billing basis, and priced,
partially priced, and unpriced record counts.

`usage breakdown` bounds its rows; `data.costs` and `data.allowances` use separate
documented bounds. Each bound reports its omitted count. No operation exposes an
individual usage record, a request handle, or a request timeline.

A read that finds the store unavailable completes with exit 0 and reports the
unavailable coverage. It must not return a fabricated empty result.

## Result data

`data.period` is `{kind, since, until, timezone}`. `kind` is `today`, `7d`, `30d`,
`90d`, `all`, or `range`. `since` and `until` are UTC instants. Day-based windows
and `range` always have known bounds from the read time; `all` bounds are null only
when no record exists; unavailable coverage yields null bounds with the coverage
reason. `timezone` names the operating system zone used for day boundaries.

`data.scope` states `source: "gateway_observed"`, `direct_traffic: "unobserved"`,
`probes: "separate"`, and `diagnostic_history: "excluded"`.

`data.coverage` appears once per result and carries `state` (`available`, `absent`,
or `unavailable`) and a `reason_code` when not available. It carries the ledger
`oldest` and `newest` instants, `retained_records`, `evicted`, `dropped`,
`cleared_at`, `reportable`, bounded `gaps`, and `retention`, plus a separate
`readings` object with the same shape for allowance readings. Instants are null
when empty; counts are numeric when coverage is available, absent, or the range is
empty, and null when unavailable. Eviction and drop counters cover the current
coverage epoch since the last successful clear; `cleared_at` names the epoch start.

`reportable` is `{since, until}`: the intersection of the requested period with the
retained record span, or null when no retained record falls inside the requested
period. `data.period` keeps the requested period unchanged. A requested bound that
extends outside the covered epoch is reported as a gap, never silently dropped.

`gaps` is bounded to 16 entries, ordered by severity then newest first within one
kind. A gap is one of:

| Gap kind | Shape |
| --- | --- |
| Cleared epoch | `{"kind":"cleared","at":INSTANT}` |
| Eviction | `{"kind":"evicted","count":N}` |
| Dropped write | `{"kind":"dropped","count":N}` |
| Restart loss | `{"kind":"restart","loss":"unknown"}` |
| Outside coverage | `{"kind":"out_of_coverage","from":INSTANT|null,"to":INSTANT|null}` |

`retention` is `{usage_days: 90, readings_days: 45, enforcement: "on_write",
usage_overdue, readings_overdue}`. Each boolean reports delayed enforcement for its
own horizon: `usage_overdue` is true when the oldest retained usage record is older
than 90 days, and `readings_overdue` is true when the oldest retained reading is
older than 45 days. Either can only be true before the next write-prune. When the
store is absent both are false; when coverage is unavailable both are null. A
requested range that starts before `oldest` reports an `out_of_coverage` gap from
the requested start to `oldest`; a range that ends after the read instant reports
the uncovered tail.

The `readings` object carries `state`, `reason_code`, its own `oldest`, `newest`,
`retained_records`, `evicted`, `dropped`, `cleared_at`, `reportable`, and `gaps`.
Its gaps contain reading evictions, reading drops, and reading out-of-coverage
ranges; it does not repeat the ledger's restart or cleared gap because readings
share the ledger epoch.

`data.disclosures` appears once per report and insight result:

| Field | Contract |
| --- | --- |
| `token_comparability` | Token buckets are protocol-scoped and exclusive; raw bucket totals must not be compared across protocols or providers. |
| `cached_input` | Cache-read and cache-write are separate priced buckets and are never added to input; an input cost estimate can overstate when a provider counts cached prompt tokens inside its input counter and no cache counter is recognized. |
| `cost_basis` | Estimates are frozen reference prices at write time; subscription-backed records are labeled and their estimate is API-equivalent, not a billed amount. |
| `probes` | Probe tokens are included in token totals but excluded from request, outcome, and duration counts. |
| `excluded` | `["provider_reported_total_tokens","foreign_counter_names"]`. |

`data.totals` carries:

| Field | Contract |
| --- | --- |
| `requests` | Terminal gateway requests in scope, excluding probes. |
| `outcomes` | Counts for `completed`, `failed`, `rejected`, `cancelled`, `interrupted`, and `unknown`. |
| `probes` | Probe request count, reported separately. |
| `usage` | Exclusive `input_tokens`, `output_tokens`, `cache_read_tokens`, and `cache_write_tokens` exact integer sums over known, nonconflicting values. Each is null when no known value exists; a known zero sums as zero. Sums are never rounded or saturated at the capture cap. |
| `usage_states` | Record counts for `reported`, `partial`, `unknown`, and `ambiguous`. |
| `duration` | `{p50_ms, p90_ms, samples, sample_minimum, state}`. Percentiles are null with `state` set to `available`, `low_samples`, or `no_records`. |
| `membership_basis` | The timestamp basis used for period membership: `admission`, `receipt`, or `mixed`. |

When coverage is `unavailable`, `data.totals` is null; `data.costs`, `data.rows`,
`data.allowances`, `data.insights`, and `data.refusals` are empty arrays; and every
omitted count is zero. No aggregate is fabricated.

`data.costs` is a bounded array with one object per provider instance:
`{provider, currency, estimate, estimate_reason, priced, partially_priced,
unpriced, sources, billing_kinds, api_equivalent, unpriceable_axes}`. `estimate` is
the exact decimal string of the integer micro-USD sum (no rounding, no floating
point), or null with `estimate_reason` (`unpriced`, `no_priced_axes`, or
`mixed_billing_scope`). `currency` is null when no estimate exists. `sources` is the
sorted union of the effective-price sources used; each record's stored price source
is a comma-separated source set. `billing_kinds` is the sorted distinct subset of
`subscription`, `api`, and `unknown` among the provider's retained records.

A provider entry never combines with another provider's cost, and it never combines
billing scopes: when more than one `billing_kind` is present, the entry reports no
estimate with `estimate_reason: "mixed_billing_scope"`, keeps its priced, partially
priced, and unpriced counts, and sets `api_equivalent` to false. When exactly one
kind is present, the estimate covers only that scope and `api_equivalent` is true
only for `subscription`; `api` and `unknown` are false. `unpriceable_axes` names
axes that cannot be priced. `data.omitted_costs` counts provider entries beyond the
bound. Records whose usage is unknown (`none` cost state) are excluded from the
priced, partially priced, and unpriced counts.

`data.rows` appears for `usage breakdown` only. Each row carries the requested
dimension value under `key`; `key` is null for an unknown value, and the product
must not fill it from current configuration. A row carries the aggregate counts,
usage, usage states, and duration for that value. Rows for `provider` and `model`
carry the provider-scoped `cost` object; rows for the other dimensions set `cost`
to null with `cost_reason: "cross_provider"`; `cost_reason` is null when `cost` is
present. `data.omitted_rows` counts rows beyond the bound.

`data.allowances` repeats dated allowance readings in the period as separate
facts: `{provider, binding, window, basis, percent, counters, unit, observed_at,
source, access_state, reset}`. `binding` is an opaque local handle for the observed
authorization incarnation and generation; it contains no secret or upstream
identity. Readings contain no computed remaining allowance and no subtraction from
observed usage. `data.omitted_allowances` counts readings beyond the bound; the
most recent readings are retained.

`data.insights` appears for `usage insights` and contains only fired insights:
`{rule, scope, claim, evidence, assumption, action, generated_at}`. `scope` names
the period and the evaluated provider, model, target, binding, and window
references the claim uses. `evidence` names the values behind the claim, the
sample counts, and the coverage. `assumption` is null or a conditional statement
the claim depends on. `action` is either `{operation, arguments}` naming an
existing supported operation or `{operation: null, arguments: [], message}` naming
external guidance.

`data.refusals` appears for `usage insights` and contains `{rule, scope, reason}`
for each evaluated scope that could not fire, including `below_threshold`. The
refusal reason is the first failing condition in the rule's documented condition
order. A `quota_pace` refusal reason of `coverage_gap` means a known coverage loss may
fall inside the evaluated reading interval. A rule with no applicable scope is
omitted from both arrays. Refusals are
bounded per result; the bound is documented, and retained refusals keep rule order
then scope order, with `data.omitted_refusals` counting the rest. An empty insight
list with refusals is a completed read, not a statement that usage is healthy.

## Bounds

| Bound | Value | Meaning |
| --- | --- | --- |
| `data.costs` | 64 | Provider cost entries; `data.omitted_costs` counts the rest. |
| `data.rows` | 200 | Breakdown group rows; `data.omitted_rows` counts the rest. |
| `data.allowances` | 200 | Newest allowance readings; `data.omitted_allowances` counts the rest. |
| `data.refusals` | 200 | Refusals in rule order then scope order; `data.omitted_refusals` counts the rest. |
| Duration samples | 20 | Minimum samples for a percentile. |
| `coverage.gaps` | 16 | Most severe coverage losses. |
| Retention | 90 days usage, 45 days readings | Age-based eviction eligibility, enforced on write or explicit clear. |
| Ledger rows | 200,000 | Usage record row cap. |
| Reading rows | 100,000 | Allowance reading row cap. |
| Page count | 32,768 | Maximum SQLite page count of the ledger file. |

Each bound reports its omitted count where the result keeps a list. A read never
prunes or rewrites the store.

## Clear behavior

`usage clear --dry-run` reports what would be removed without deleting data.
Execution removes the retained usage records and allowance readings in one bounded
operation; records committed after the operation begins remain.

`data.clear` is `{dry_run, executed_at, removed: {usage_records,
allowance_readings}, retained: {usage_records, allowance_readings}}`. `executed_at`
is null for a dry run. The root `data.coverage` is the resulting coverage. A
successful clear starts a new coverage epoch: `evicted` and `dropped` reset, and
`cleared_at` names the epoch start.

Clearing usage does not delete diagnostic request history, the error log, quota
observations or current heads, routing input heads, session bindings, recovery
state, settings, or credentials. It never resets an upstream allowance.

An absent store yields a successful clear result with zero removed and absent
coverage. An unreadable or unsupported store is blocked before deletion with exit
3 and a typed reason naming the local artifact; the operation must not guess or
partially delete.

## Examples

```text
tmax usage summary
tmax usage summary --period 7d
tmax usage summary --since 2026-11-01T00:00:00Z --until 2026-11-08T00:00:00Z
tmax usage breakdown --by provider --period 30d
tmax usage breakdown --by requested --period 7d
tmax usage insights --period 7d
tmax usage clear --dry-run
tmax usage clear
```

Exit categories follow [shared results](../results/spec.md): 0 for a completed
read or clear, 2 for invalid arguments, 3 for a blocked clear, and 4 for a failed
clear. A read that reports unavailable coverage still exits 0.
