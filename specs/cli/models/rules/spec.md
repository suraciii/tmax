# Model rule commands

This child owns calendar/signal-rule grammar and typed explanations. The
[rule schema](../../../configuration/spec.md#priority-rule-fields) owns fields;
[model selection](../../../model-routing/spec.md#priority-rules) owns ordering,
predicates, fallback, and continuity. Use [common CLI rules](../../spec.md),
[configuration changes](../../configuration/spec.md), and
[shared results](../../results/spec.md).

## Model time rules

| Command | Requested change |
| --- | --- |
| `model schedule add QUALIFIED RULE` | Append one complete rule. |
| `model schedule replace QUALIFIED INDEX --if-revision REVISION RULE` | Replace the complete rule at that file revision, removing omitted optional bounds. |
| `model schedule remove QUALIFIED INDEX --if-revision REVISION` | Remove exactly that local rule at that revision. |
| `model schedule clear QUALIFIED` | Remove the schedule, not base priority. |

`RULE` requires `--priority INTEGER`, one nonempty unique selector form
(`--weekday DAY` repeated or `--date DATE` repeated), and one window form
(`--all-day` or both `--start HH:MM --end HH:MM`). Days are `mon`, `tue`, `wed`,
`thu`, `fri`, `sat`, `sun`. Mixing weekday/date selectors is invalid.
All-day writes true; false is not shorthand. Optional `--effective-from DATE` and
`--effective-until DATE` bound local dates with an exclusive upper bound.

Set the offering's `timezone` before adding a schedule; an empty schedule permits
that setup. All rules use that zone. Times, unequal timed endpoints, overnight
spillover, clipping, overlap rejection, precedence, and DST follow
[Time rules](../../../model-routing/spec.md#time-rules), without extra fields or
inferred windows.

```sh
tmax model update deepseek/deepseek-flash --set timezone Asia/Shanghai
tmax model schedule add deepseek/deepseek-flash --priority 100 --weekday mon --weekday tue --weekday wed --weekday thu --weekday fri --weekday sat --weekday sun --start 00:00 --end 08:00 --effective-from 2026-10-01 --effective-until 2026-11-01
tmax model schedule replace deepseek/deepseek-flash 0 --if-revision CURRENT-REVISION --priority 90 --date 2026-10-23 --all-day
tmax model schedule remove deepseek/deepseek-flash 0 --if-revision CURRENT-REVISION
tmax model schedule clear deepseek/deepseek-flash
tmax model update deepseek/deepseek-flash --clear timezone
```

Each indexed example needs the revision current to its own invocation, not a
stale token reused across writes. Results identify the offering, changed local
position, current file revision, and saved/loaded outcome; inspect returns the
complete local schedule and revision-scoped indices.

## Model priority rules

| Command | Result or requested change |
| --- | --- |
| `model rule list QUALIFIED [--state STATE]` | Compact ordered IDs/positions, definitions, readiness, and current outcomes. |
| `model rule inspect QUALIFIED RULE-ID [--state STATE] [--at INSTANT]` | Complete definition and explanation in the offering's ordered context. |
| `model rule add QUALIFIED RULE-ID SIGNAL-RULE` | Append a new immutable offering-local ID; reject duplicates, including disabled rules. |
| `model rule replace QUALIFIED RULE-ID SIGNAL-RULE` | Replace the entire definition, retaining ID/position and clearing omitted optional fields. |
| `model rule remove QUALIFIED RULE-ID` | Remove only the rule, not inputs, schedule, or base priority. |
| `model rule move QUALIFIED RULE-ID (--before OTHER-RULE-ID \| --last)` | Move before a distinct existing rule or to the end; no indices or priority sort. |
| `model rule enable QUALIFIED RULE-ID` | Set enabled true without changing fields/position. |
| `model rule disable QUALIFIED RULE-ID` | Set enabled false, retaining references/data. |
| `model rule clear QUALIFIED` | Remove all signal rules, not schedule, inputs, or base priority. |

`SIGNAL-RULE` requires `--input INPUT --priority INTEGER --reset-within DURATION`.
Optional `--enabled BOOLEAN` defaults true. Derive kind from the referenced input
in the same captured declaration snapshot used to validate/evaluate the rule;
there is no rule kind operand or persisted rule kind. Input must belong to the
same provider and explicitly cover this upstream model.

For a quota input, `--remaining-at-least-percent DECIMAL` is required; for reset
it is forbidden. Threshold uses the schema's exact decimal-string grammar, not
a float or percent-suffix expression. Duration uses
[elapsed units](../../spec.md#duration-operands) and persists `reset_within_seconds`.
Priority is signed. No other condition, URL, script, or request field is accepted.
Replace requires the full required set, restores enabled true when omitted, and
removes an omitted gate; it is not partial update.

An optional finite time gate reuses schedule selectors/windows/bounds. No gate
operands means no `time_window`; any gate operand requires a complete selector
and window, with optional bounds. They set only `time_window`; priority remains
the outer rule's score. Set the offering's timezone first. Cross-midnight windows,
local dates, and DST use the same Time rules, not a new expression language.

Reads do not refresh or resolve secrets. List uses the common current instant;
inspect may use `--at` under [model explanation](../spec.md#model-explanation)'s
retained-facts-only rule. Mutations accept optional file guard/dry-run and activate
the complete valid candidate. Move rejects missing/self targets; already-last is
an idempotent no-op. Priority edits affect new selections, not established
destinations by themselves. Evaluation follows
[first-match rules](../../../model-routing/spec.md#rule-evaluation-and-explanation):
calendar priority cannot mask an ungated earlier signal. Outside a configured
gate, return false/`outside_time_window` without evaluating input facts.

Given the [manual input declarations](../../providers/inputs/spec.md#provider-input-operations):

```sh
tmax model update opencode-go/deepseek-v4.1-flash --set timezone Etc/UTC
tmax model rule add opencode-go/deepseek-v4.1-flash use-weekly-allowance --input weekly-allowance --priority 120 --reset-within 6h --remaining-at-least-percent 75.0 --date 2026-10-23 --start 08:00 --end 20:00
tmax model rule add opencode-go/deepseek-v4.1-flash prefer-before-reset --input reset-announcement --priority 130 --reset-within 6h
tmax model rule list opencode-go/deepseek-v4.1-flash
tmax model rule inspect opencode-go/deepseek-v4.1-flash use-weekly-allowance --at 2026-10-23T10:00:00Z
tmax model rule replace opencode-go/deepseek-v4.1-flash use-weekly-allowance --input weekly-allowance --priority 110 --reset-within 3h --remaining-at-least-percent 80 --enabled false
tmax model rule enable opencode-go/deepseek-v4.1-flash use-weekly-allowance
tmax model rule disable opencode-go/deepseek-v4.1-flash prefer-before-reset
tmax model rule move opencode-go/deepseek-v4.1-flash prefer-before-reset --before use-weekly-allowance
tmax model rule move opencode-go/deepseek-v4.1-flash prefer-before-reset --last
tmax model rule remove opencode-go/deepseek-v4.1-flash use-weekly-allowance
tmax model rule clear opencode-go/deepseek-v4.1-flash
```

These express caller policy, not current account facts or tariff advice.
Missing/expired input keeps the rule valid but unknown, never zero quota or proof
of reset.

## Priority-evaluation data

`RuleEntry` contains every configured rule field with defaults explicit, plus
zero-based `position`, derived `kind`, and `evaluation`. Derive kind from the
same captured input declaration even when predicates are skipped; never read a
later declaration to label an earlier evaluation. Optional threshold/gate fields
stay absent when unconfigured. Stable ID, not position, addresses rule mutations.

Evaluation contains `outcome` (`matched`, `false`, `unknown`, `disabled`, or
`shadowed`), string-or-null `reason_code`, `shadowed_by` (winning ID-or-null),
`time_window`, `input_basis`, and `predicate_results`.

- `time_window = {state, timezone, utc_offset, next_boundary}`. State is
  `not_configured`, `matched`, `not_matched`, or `not_evaluated`; other fields are
  strings or null. A nonmatching gate uses `outside_time_window` without input
  evaluation. Disabled/shadowed rules claim no predicate evaluation.
- `input_basis` is null when skipped. Otherwise it contains bounded `provider`,
  `input`, `provider_identity`, `authorization_generation`, `declaration_revision`,
  `current_revision`, `kind`, `source`, `scope`, `models`, `availability`,
  `observed_at`, `observed_at_basis`, `valid_until`, `reset_at`, and exactly the
  available percentage or remaining/total/unit representation. Current revision
  is the captured publication revision, with `absent` for known never-published
  data. Unavailable fields are null/absent as qualified by availability.
- Input basis includes no provenance URL, body, credential, upstream account
  identity, or invented percentage. Counter comparison uses original counters
  and configured threshold, not a rounded conversion.
- `predicate_results = {reset_within, remaining_at_least}`. Each is the string
  `true`, `false`, `not_evaluated`, or `not_applicable`; reset uses
  `not_applicable` for quota. The configured seconds horizon, threshold, priority,
  and gate stay visible on the rule. Failed acquisition alone does not discard
  otherwise usable current data.

`PriorityEvaluation` contains:

- UTC `at`, `configuration_revision`, `metadata_revision`, and `input_revisions`.
  The vector contains `{provider, input, provider_identity,
  authorization_generation, declaration_revision, current_revision}`, captured
  coherently with config/metadata. Attempt revisions never replace data revisions.
  Absent/unavailable states are explicit.
- Ordered `priority_rules: RuleEntry[]`, string-or-null `selected_rule_id`, and
  signed-integer `effective_priority`.
- `calendar_fallback = {kind, priority, schedule_index, timezone, utc_offset,
  next_boundary}`. Kind is `date`, `weekly`, or `base`; index is integer-or-null;
  zone/offset/boundary are strings-or-null. This is the independently evaluated
  fallback, not a second competing signal winner.
- `boundaries`: `{kind, at, rule_id, input}[]` for known existing-fact transitions.
  Kind is `calendar`, `time_window`, `observed_at`, `valid_until`, or
  `reset_horizon`; at is UTC; rule/input are string-or-null. Observation-start
  boundaries also apply when historical `--at` precedes a held observation.
  These do not predict upstream resets, future data, or successful acquisition.
  Acquisition due timers are not guaranteed priority-change boundaries.

| Operation | Command-specific `data` |
| --- | --- |
| Rule list | `qualified_model`, `state`, UTC `at`, `selected_rule_id`, `effective_priority`, and `rules` summaries with `id`, `position`, derived `kind`, `input`, `enabled`, `priority`, `outcome`, and `reason_code`. No per-row inspect argv or full input evidence. |
| Rule inspect | `qualified_model`, `state`, `rule: RuleEntry`, and `priority_evaluation: PriorityEvaluation` for the full ordered context. |
| Rule mutations | `qualified_model`, `rule_id` (null for clear), `changed`, requested definition/order effect, needed file revision, and saved/loaded outcome. Include resulting order for move/clear, not mandatory before/after arrays or full evaluations for every mutation. |
| Model explain / concrete inspect | `priority_evaluations` entries with `qualified_model`, boolean `priority_used_for_ranking`, and `priority_evaluation`; the boolean is false for a fixed concrete destination. |

Dry-run returns the full redacted mutation/activation impact and evaluates a
candidate only from held facts; it neither publishes config nor fetches inputs,
extends freshness, or claims future observations. Historical details preserve the
kind captured with that declaration and obey the
[bounded diagnostic evidence](../../../diagnostics/spec.md#minimal-request-record).
Omitted historical rules/candidates stay explicit, not recomputed from current data.
