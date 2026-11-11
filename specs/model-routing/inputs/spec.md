# Input values and acquisition

This specification owns scoped runtime values used by priority rules, their
provenance, acquisition, freshness, publication, and retention. It defines target
behavior independently of source availability or end-to-end verification. The
[configuration schema](../../configuration/spec.md#provider-input-declarations)
owns declaration fields and validation; the
[CLI input contract](../../cli/providers/inputs/spec.md#provider-input-operations) owns
command grammar, revision operands, and result envelopes. The
[routing contract](../spec.md) owns rule predicates, ordering, priority replacement,
and conversation continuity. Shared terms follow [CONTEXT.md](../../../CONTEXT.md).

An **input** is a named channel declared within one provider, not a global account
record or a model catalog entry. Its declaration fixes its kind, source, scope,
explicit model coverage, and freshness limit. A quota input reports an allowance
within one scope and reset window; a reset input reports a future reset forecast.
Neither kind by itself establishes account-wide exhaustion, candidate
eligibility, actual reset completion, or subscription renewal. Rules consume
normalized retained values, never remote pages, native credential files, or
natural-language text.

| Source | Meaning and availability boundary |
| --- | --- |
| `manual` | The user or calling agent explicitly records a quota value or reset forecast. Scope, coverage, counters, and provenance remain caller assertions, not provider verification. Recording requires no credentials or running service. |
| `provider` | A qualified integration supplies the declared kind, scope, coverage, and normalized fields through a supported non-inference operation. A familiar provider name, protocol, native display, or readable token does not establish that capability. |
| `codex-resets` | The bounded public announcement reader defined in [Codex reset announcements](#codex-reset-announcements). It supplies reset forecasts only, with scope `codex`, for providers selecting the qualified Codex subscription integration. It is not a quota-query capability. |

The source is exclusive. Recording is allowed only for `manual`; fetching is
allowed only for supported nonmanual sources. There is no manual override,
implicit precedence, merging, or fallback within a channel. To use both sources,
declare distinct channels and reference them through ordered rules; to change a
channel's source, replace its declaration and invalidate its current value.

Unknown source names and malformed or intrinsically inconsistent declarations
are schema errors. A well-typed declaration whose selected integration lacks the
required qualified source, kind, or scope remains visible but unavailable; it
cannot dispatch a speculative fetch. Integration IDs and capabilities come from
the [qualified registry](../../integrations/spec.md#rule-input-capabilities), not
from a guessed provider ID, URL, script, or environment variable. This contract
does not add a GLM integration or infer quota support for any existing product.

## Input records

The canonical runtime types are `InputHead`, its non-null `InputValue`,
`AcquisitionAttempt`, and the computed `InputAvailability`. CLI result envelopes
reference these types rather than define alternate normalized records.

### Current head and normalized value

Runtime records persist under `~/.tmax`, separately from TOML intent. Each channel
has one current **head**, plus its separate last acquisition-attempt result.
Before any publication, `head` is null. An unavailable head is a retained
revision-bearing tombstone, not a zero-valued fact. Availability at an evaluation
instant is computed rather than stored as an enduring freshness status.

The following binding fields are required on both a non-null head and a
non-null `last_attempt`:

| Field | Type and meaning |
| --- | --- |
| `provider` | Exact local provider ID in the selected configuration view. It is a display/reference label, not upstream identity. |
| `input` | Exact provider-local immutable input name. |
| `provider_identity` | Opaque, nonsecret stable local provider-incarnation token, not an upstream user/workspace ID, credential fingerprint, or configuration field. |
| `authorization_generation` | Opaque, nonsecret token for the selected authorization lineage, including a generation for not-yet-configured authorization. |
| `declaration_revision` | Opaque token binding the declaration incarnation and its semantic definition to the applicable integration/source contract. |
| `kind` | `quota` or `reset` from the captured input declaration, not inferred from an acquired payload. |
| `scope` | Exact declared scope. A matching string across providers does not prove shared or independent allowance. |
| `models` | Explicit declared set of exact upstream model IDs within this provider; no catalog-wide or implicit expansion. Array order does not change coverage. |
| `source` | The one declared source. |

A head additionally requires:

| Field | Type and meaning |
| --- | --- |
| `revision` | Opaque head revision, distinct from configuration-file, declaration, and acquisition-attempt revisions. |
| `observation_floor` | Nullable UTC instant used only to reject older publications. With a value it equals `value.observed_at`; withdrawal retains or advances the qualified negative observation time; explicit clear or a new declaration/authorization generation sets it to null. It is not a usable input fact. |
| `value` | Normalized object below, or null for an unavailable head. |
| `unavailable_reason` | Null when `value` is non-null; otherwise exactly `cleared`, `withdrawn`, `declaration_mismatch`, or `authorization_mismatch`. |

A non-null `value` has these fields:

| Field | Type and meaning |
| --- | --- |
| `observed_at` | When the asserted source condition was observed, not the announced reset time. |
| `observed_at_basis` | `manual` for explicit caller recording; `source` for a qualified source's observation timestamp; `retrieval` for a genuinely new successful source read when no authoritative observation timestamp exists. |
| `received_at` | When tmax received this observation. Retrying or displaying it does not change this instant. |
| `valid_until` | Exclusive effective expiry after applying all freshness caps. |
| `reset_at` | Future reset boundary or announcement instant. For quota, the window identity is this scoped input plus `reset_at`; no guessed window start is required. |
| `remaining_percent` | Quota only: exact decimal string in the inclusive range 0 through 100, mutually exclusive with the counter group. |
| `remaining`, `total`, `unit` | Quota only: complete alternative counter group. `remaining` and `total` are exact nonnegative decimal strings, `total > 0`, and `remaining <= total`. The nonempty `unit` identifies their common documented unit within the same scope and window; for manual input it is caller-declared. |
| `reference` | Optional safe absolute HTTPS provenance URL, without embedded credentials or secret-bearing components. It is never a fetch destination or executable instruction. |

Quota values require exactly one of `remaining_percent` or the complete
`remaining`/`total`/`unit` group. Reset values forbid every quota field. No
arbitrary adapter fields, source bodies, announcement text, tokens, secrets, or
native credential contents may be published in either record. Unknown fields or
an invalid normalized candidate must be rejected rather than partially accepted.
Source references may appear in explicit input inspection, not in retained
request-decision evidence.

### Exact quota quantities

The schema's decimal strings, including rule thresholds, are base-ten values
without signs, exponents, NaN, or infinity; they are never binary floating-point
numbers. There is no speculative fixed precision limit. Normalization preserves
numeric value, removes redundant leading integer zeros and trailing fractional
zeros, and emits zero as `"0"`. A source-supplied percentage is exact as reported;
that does not claim an exact physical measurement of the provider's allowance.

An adapter may publish documented comparable remaining and total quantities only
when both apply to the same scope, window, and unit, with a positive denominator.
It must not infer a total from a plan name, displayed bar, historical maximum,
local traffic, another provider instance, or an absent field. Unknown or
incomparable quantities make the attempted observation unavailable, not zero.

A percentage predicate compares `remaining_percent` directly with the threshold,
or compares `100 * remaining >= threshold_percent * total` with exact decimal or
rational arithmetic. It must not round or serialize an approximate percentage.
An adapter may convert counters into `remaining_percent` only when the result is
an exact finite decimal; otherwise it retains the counter group. Thus one unit
out of three remains usable and distinguishes thresholds `"33.33"` and
`"33.34"` without manufacturing a percentage string. Manual recording exposes
the same mutually exclusive representations through the CLI.

### Validity and applicability

All instants require an explicit-offset RFC3339 representation and normalize to
UTC, omitting redundant fractional zeros. Equality compares the represented
instant, not timestamp spelling: different offsets or redundant fractional
precision for the same instant are identical. Decimal lexical variants likewise
compare after exact numeric normalization. Neither variation creates a new
observation, conflict, or freshness extension. Let `P` be publication time. A
newly accepted value must satisfy:

- `observed_at <= received_at <= P`; callers cannot future-date an observation.
- `reset_at > P` and `valid_until > P`.
- `valid_until > observed_at`.
- `valid_until <= observed_at + max_age_seconds`.
- `valid_until <= reset_at` and any earlier source-supplied validity boundary.

Manual recording requires the caller's observation, expiry, reset, and applicable
quota fields. A supplied expiry outside these caps is rejected rather than
silently corrected or extended. For fetched data, normalization uses the earliest
of the source's expiry when present, the declared maximum age, and `reset_at`.
If that effective interval has already ended, no newly valid value can publish.
A qualified passed reset announcement is handled as described
[below](#codex-reset-announcements), not as recovered quota.

A successful new source observation may replace an earlier one. If the source
supplies an authoritative observation timestamp, a later receipt does not
substitute for it. Otherwise a genuinely new successful read uses its receipt
instant with `observed_at_basis = retrieval`; this may renew the local maximum-age
bound even for the same pending announcement reference. It cannot extend an
existing source-supplied bound or the announced reset boundary. Merely replaying
retained bytes, using an unqualified local cached response, retrying a publication,
restarting, or failing a refresh is not a new observation. HTTP revalidation may
supply a new observation only under a qualified source contract establishing
that the source's current representation was actually revalidated; stale local
bytes cannot be relabeled as freshly observed.

Detailed input inspection and rule evaluation expose `availability.state` as `available` or
`unavailable`, with nullable `availability.reason_code`. For one selected
configuration/authorization view and evaluation instant `T`, choose the first
applicable condition in this exact order:

| Condition | `reason_code` |
| --- | --- |
| The required runtime state cannot be read. | `storage_unavailable` |
| The retained normalized record is malformed or fails its storage schema. | `invalid_record` |
| No head has been published for this declaration. | `missing` |
| Provider incarnation, declaration revision, kind, source, scope, or declared coverage does not match this view. | `declaration_mismatch` |
| The selected authorization generation does not match. | `authorization_mismatch` |
| The matching head is a tombstone. | Its `unavailable_reason` |
| `T < observed_at`. | `not_yet_observed` |
| `T >= valid_until`. | `expired` |
| None of the above. | Null; state is `available`. |

The head's explicit coverage must include the rule's concrete upstream model;
configuration validation rejects unresolved or mismatched references before rule
evaluation. Runtime mismatch handling does not excuse invalid configuration.

This order makes mismatch win over expiry, and expiry win at the exact reset
boundary because validity is capped by `reset_at`. A failed or unavailable
acquisition does not itself invalidate a matching still-fresh head; its separate
attempt explains the source failure. With no head, the primary reason remains
`missing`, with attempt failure/capability detail separately visible. None of
these states is exhausted quota or a provider-wide routing exclusion.

An explicit `--at` inspection consumes only the current retained head captured for that view. It
may use that head at a past or future supplied `T` only when the head's own
observation and validity interval contains `T`. It neither reconstructs a former
head/configuration nor predicts future observations, account recovery, or reset
completion. A superseded or cleared value is not recoverable through `--at`.

### Last acquisition attempt

`last_attempt` is null until an acquisition attempt completes. It is not changed
by manual recording or clearing, inspection, a rejected precondition, a dry run,
or an acquisition rejected because another acquisition is in flight. Its binding
fields are defined [above](#current-head-and-normalized-value). It additionally
requires its own opaque `revision`, `started_at`, `finished_at`, `trigger`,
`outcome`, `reason_code`, `head_revision_before`, and `head_revision_after`.
Timestamps normalize to UTC and `started_at <= finished_at`. The trigger is
`explicit`, `automatic`, or `provider_refresh`. The before/after head fields are
opaque head revisions or the literal `absent` when there is no head.

| `outcome` | Allowed `reason_code` and effect |
| --- | --- |
| `published` | Null. A new valid observation atomically replaces the current head. |
| `unchanged` | `identical_observation`. Identical observation retry leaves head revision, timestamps, and expiry unchanged. |
| `withdrawn` | `explicit_absence`, `explicit_withdrawal`, or `reset_time_passed`. A qualified source result atomically replaces the head with a `withdrawn` tombstone. It never establishes recovered quota. |
| `failed` | `transport_error`, `source_format_error`, `invalid_observation`, `older_observation`, `conflicting_observation`, or `storage_unavailable`. No replacement value; any prior value keeps only its original validity. |
| `unavailable` | `unsupported_source`, `credential_unavailable`, or `noninteractive_unavailable`. The prerequisite/source cannot provide this operation; no replacement value. |
| `superseded` | `input_changed`, `declaration_changed`, `authorization_changed`, or `declaration_removed`. A captured publication guard no longer matches; the returned result cannot replace the current head. |

An explicit absence or withdrawal requires a qualified source representation that
asserts the relevant current value no longer exists. Empty response bodies,
missing fields, changed layouts, inaccessible pages, permission failures, and
natural-language guesses are failures or unavailable capability, not withdrawals.
A qualified negative observation obeys the same ordering checks as a positive
one. Its tombstone retains/advances `observation_floor` so a subsequent older
observation cannot resurrect the withdrawn value. It retains no positive payload.

The head and its attempt are committed atomically when publication succeeds.
An attempt-only update does not change the head revision or routing fact
snapshot. A superseded result may attach as the last attempt only to the same
still-current declaration and authorization binding; otherwise it is returned to
its caller without attaching to another generation. If storage itself fails,
report the failed persistence boundary; a non-persisted attempt is not presented
as durable evidence. There is no append-only attempt or event history.

### Publication and concurrency

All CLI and service writers share per-channel serialization and atomic head
publication. A running service is the coordinated owner, not a parallel cache
that an independent CLI writer may bypass. If that owner cannot be coordinated,
reject the mutation rather than create split-brain runtime state. Without the
service, explicit operations use the same persistent record and guards directly.
They neither start the service nor require gateway inference to be running.

An acquisition captures the provider incarnation, authorization generation,
declaration revision, and current head revision before reading its source. It
performs no network operation while holding an admission/publication lock. On
completion it rechecks every captured binding and the head revision before any
publication. A changed head rejects the old result even if the fetched timestamp
looks newer. In particular, clear, a concurrent record, declaration/source
replacement, authorization change, removal, or remove/re-add cannot be undone by
a late result. A fresh acquisition started after clear may publish normally;
clear does not disable the declaration or its future schedule.

The recheck uses the authority captured for the operation: saved declaration for
explicit operations, loaded declaration for automatic work. A change confined
to the other pending view is not a guard failure. Clear and head-revision
conditions refer to that corresponding head; actual authorization-generation
changes invalidate both views.

`record` atomically replaces only a matching manual head. `clear` publishes a new
unavailable revision with `unavailable_reason = cleared`, null value, and null
`observation_floor`; it does not delete the declaration or its rule references.
CLI conditional input revisions apply at the final atomic write, including the
`absent` precondition. A rejection has no runtime effects. Record/clear do not
rewrite TOML, priorities, authentication, native configuration, or service startup
settings.

For the same binding, an observation older than the non-null observation floor
is rejected. At the same observation instant, a different normalized positive or
negative assertion is a conflict, not last-writer-wins. An identical observation
is idempotent, excluding receipt/publication metadata from equality: retries keep
the original receipt time, validity, and head revision. An identical retry may
report unchanged even if that retained head has since expired; it is not a newly
valid observation. Explicit clear deliberately starts a new head without the old
observation floor, but all freshness requirements still apply. New authoritative
reset dates or window observations replace the one current value; they are not
merged into an event list.

Declaration revisions change for kind, source, scope, model coverage, maximum
age, or a material change to the selected integration/endpoint source contract.
A removed and re-added name has a fresh declaration incarnation even if its text
is identical. Source/definition changes publish a new unavailable generation in
the changed view; they do not reinterpret an old manual value as fetched or vice
versa, or replace a different still-loaded declaration before activation.
`refresh_interval_seconds` is acquisition policy only: changing or disabling it
does not invalidate or refresh an existing value. Independent rule, schedule,
priority, display-name, or unrelated provider edits do not invalidate the input.

Provider identity is independent of its local name. A qualified local-ID rename
preserves provider identity, declaration/head revisions, and timestamps; records
render their provider label through the selected configuration view. Replacing
keys, reauthenticating, changing the login source or selected native authorization,
or uncertainty about whether an authorization changed advances the authorization
generation and invalidates dependent values, including manual values. A qualified
same-authorization credential refresh does not advance it. An unverified upstream
identity remains unverified; a generation token is not account proof and cannot
be derived by exposing or fingerprinting credentials.

### Retention and removal

Retain at most the current head and last attempt per declaration incarnation
referenced by saved configuration or a still-loaded configuration snapshot.
Replaced/removed incarnations may remain only while the old loaded snapshot still
references them, for example after a saved removal whose activation failed.
After a successful saved input/provider removal or configuration reset, discard
the unreferenced channel runtime data safely; after the old loaded reference is
released, discard any temporarily retained old incarnation. Do not create an
orphan fact catalog. In-flight work retains only its bounded publication guard
and cannot recreate a removed incarnation.

Clear within an existing declaration retains its tombstone and revision for the
publication guard; remove/re-add cannot reuse it. Expired values may remain as the
single inspectable current head but are never made fresh by restart. Restart
restores a value only to its applicable binding and original validity. Missing,
corrupt, or unavailable state is shown honestly, not replaced by a new receipt
instant or a guessed source value.

Already-admitted immutable snapshots and bounded
[request-decision evidence](../../diagnostics/spec.md#minimal-request-record)
retain their captured normalized basis and declaration-derived kind under their
own lifecycle; this is not an input history or permission to retain source bodies/references.
Record count is bounded by current saved/loaded declarations, not refresh count,
number of announcements, or service lifetime. Inspection cannot enumerate
removed events, superseded heads, or past attempts.

## Acquisition

### Explicit operations and loaded applicability

The [CLI input operations](../../cli/providers/inputs/spec.md#provider-input-operations) provide
explicit refresh for one selected supported nonmanual channel, manual recording,
clear, and read-only inspection. They work without a daemon service and do not
mutate authentication, desired configuration, native application, or startup.
Dry runs inspect only permitted local inputs: no credentials, source read,
publication, or scheduling effects. Inspection, priority preview, `--at`, and
request selection never trigger acquisition or saved-secret resolution.

Explicit operations validate the current saved declaration and capture its
binding. A fetched or manually recorded head can affect the running service only
when its declaration and authorization binding also match the loaded snapshot.
A saved declaration that differs from loaded configuration may receive a valid
saved-runtime record without changing loaded routing; inspection/result evidence
must distinguish saved applicability, loaded applicability, and pending/not-
applicable publication. Acquisition does not activate pending configuration.
Matching records become applicable on a later successful activation only if
still valid; activation never extends their freshness.

`provider refresh` with quota/all scope includes the declared `provider`-source
quota channels that the integration supports, alongside that operation's existing
qualified observations. It does not fetch reset websites, refresh manual values,
register models, or change inputs into another source. An explicit input refresh
is the operation for a selected nonmanual reset channel. Report each channel's
outcome separately rather than synthesizing one account-wide balance or failure.

Fetches use only the selected provider's supported, already-qualified credential
path. An explicit operation may request an existing same-authorization refresh
through its declared login source where supported and may consume explicitly
supplied protected input under the [CLI protection contract](../../cli/spec.md). It cannot sign in,
interactively acquire a different account, borrow ambient browser credentials,
import another agent's tokens, or resolve saved secrets implicitly. Public announcement
reads use no authentication at all. A missing source capability does not block
otherwise qualified inference.

tmax-owned source reads use the global
[outbound network policy](../../product/spec.md#outbound-network-policy), including
public announcements. Same-authorization refresh follows the
[credential coordination contract](../../product/spec.md#credential-refresh-coordination);
input acquisition must not introduce competing refresh or an independent
refresh attempt for the same authorization generation.

### Opt-in service scheduling

A nonmanual declaration's explicit refresh interval enables automatic acquisition
only while the existing local service is running and that configuration is
loaded. It does not install another daemon, enable autostart, start the service,
or make an inspection/network request an acquisition trigger. Without the
service, automatic acquisition is visibly paused while explicit refresh remains
available. No interval means no automatic acquisition; there is no inferred
numerical default.

The first acquisition is due on service startup or when the loaded configuration
enables the interval. Subsequent due instants follow that interval using monotonic
elapsed time. At most one acquisition is in flight per input across service and
explicit callers; an explicit concurrent attempt reports
`acquisition_in_progress` without effects. There is no catch-up burst, queue of
missed ticks, retry between interval ticks, inference probe, or inference replay.
If a tick occurs during an acquisition, skip that tick rather than overlap work.
An explicit refresh does not create a new automatic schedule.

Automatic acquisition captures the current loaded declaration and policy, not
the saved file's pending intent. At publication it rechecks that loaded binding.
A differing or invalid saved file does not pause or replace coherent loaded
behavior; a failed activation leaves that prior behavior intact. Explicit
operations instead capture and recheck the validated saved definition. When the
views differ, their heads remain scoped to their declaration incarnations and
revisions, and only a matching head applies to either view. Actual authorization
generation changes remain live invalidation boundaries for both views.

Automatic work uses already available credentials only. An unavailable local
secret file or missing credentials produces the corresponding unavailable
attempt; no prompt, login, or implicit secret resolution occurs.

Disabling/changing the interval stops/reschedules new automatic attempts; a
captured in-flight attempt may finish only if its declaration, authorization, and
head guards still match. Existing values keep their original validity. Enabling
or changing an interval is not itself a successful source observation.
Inspection summaries show acquisition intent and active, paused, or unavailable
state. Explicit [input detail](../../cli/providers/inputs/spec.md#input-data-results)
distinguishes configured/off, due/in-flight, paused-without-service, unsupported,
and other unavailable reasons, plus loaded policy versus pending saved intent.
No inspection acquires a fact; due times and attempts do not make expired inputs available.

Updates publish one current head atomically. Request admission captures
configuration, metadata, head revisions, and one evaluation instant coherently;
it never waits for refresh or performs source I/O. It sees the prior complete
head or the next complete head, not a partially normalized record. Input/rule
updates affect new-session selection under the
[routing continuity contract](../spec.md#selection-and-continuity); they do not
reassign or revoke a fixed session by themselves.

## Codex reset announcements

### Supported deterministic representation

The public [Codex Resets page](https://codex-resets.com/) was consulted through
static source on 2026-10-23. Its HTML advertises an English Markdown alternate at
[`https://codex-resets.com/index.md`](https://codex-resets.com/index.md), which
returns `text/markdown`; the root also serves Markdown through content
negotiation. This is third-party announcement evidence, not an OpenAI account
query, an authenticated source, a JSON API, or verified tmax adapter support.

The supported reader fetches only that fixed public English Markdown resource
over HTTPS without cookies, credentials, browser automation, or executable
scripts. It does not follow an input's reference URL as a source. There is no
guessed API path, HTML/JavaScript scraping fallback, locale/prose interpretation,
or LLM extraction. Transport failures, refused redirects, and non-success HTTP
responses produce `failed` with `transport_error`. A successful response with an
unsupported content type or representation, including an authentication page or
client-execution requirement instead of the qualified Markdown, produces
`failed` with `source_format_error`. Neither result authorizes ambient browser
inspection or a credential fallback.

Parse the Markdown structure deterministically, accepting a positive forecast
only when all of the following hold:

1. The document identifies itself with the top-level `Codex Resets` heading and
   contains exactly one second-level `Reset scheduled` section.
2. Within that section, before the next second-level heading, there is exactly
   one non-quoted scheduling paragraph containing one code-span absolute UTC
   RFC3339 timestamp and the explicit `Upcoming reset` marker in the site's
   middle-dot-separated timing representation. Relative countdown text and the
   reset-type label have no routing meaning.
3. That section contains exactly one `View announcement` link with an absolute
   HTTPS URL of the observed form
   `https://x.com/thsottiaux/status/` followed by a nonempty decimal status ID,
   without userinfo, query, or fragment. It is retained as `reference`, not read
   or treated as executable instructions.
4. The timestamp is unambiguous and still strictly future at publication.
   Multiple candidate paragraphs, timestamps, pending sections, or announcement
   links are a format failure; the reader does not choose earliest/latest.

The consulted representation had the following supported scheduling shape:

```markdown
## Reset scheduled

**in 9 hours** · `2026-10-23T17:00:00.000Z` · Upcoming reset · Regular reset

A scheduled reset stays pending until it is confirmed. A passed time does not mean it already happened.

> Landing tomorrow before 10am PST together with a small ship.

[View announcement](https://x.com/thsottiaux/status/2113492502355562875)
```

This dated shape qualifies the bounded parser contract, not a hard-coded current
forecast. Only the absolute code-span timestamp sets `reset_at`. Quoted prose,
`in 9 hours`, local dates, PST references, reset counts, averages, reaction counts,
latest-reset cards, and announcement history must not supply or repair it.
The reader must not extrapolate a next reset from history or infer a date when
this representation stops being supported.

This source supplies no authoritative observation or expiry timestamp for the
scheduled section. A genuinely new successful source read therefore uses
`observed_at = received_at` with basis `retrieval`, and effective expiry is capped
by the configured maximum age and `reset_at`. Its announcement URL identifies
the pending announcement, not a quota window or an account. A later successful
read of a changed future timestamp/reference replaces the one current forecast
under the normal ordering/publication guards.

### Pending, unavailable, and withdrawn announcements

The page explicitly states that a scheduled reset remains pending until
confirmed and that a passed time does not establish completion. This warning is
a semantic limit, not account evidence. Even a confirmed historical site entry
does not establish that a specific configured account was eligible, recovered
quota, consumed a banked reset, or can successfully infer.

The qualified format above has no established explicit no-pending or withdrawal
representation. Missing/renamed scheduled sections, unexpected content types,
ambiguous markup, or withdrawal prose therefore produce `source_format_error`,
not invented `explicit_absence` or `explicit_withdrawal`. Retain any previous
matching forecast only through its original expiry. Supporting another negative
representation requires source evidence and deterministic qualification; it
cannot be inferred from the shape of one successful page.

If an otherwise qualified pending announcement's absolute timestamp is no longer
future at publication, report `withdrawn` with `reset_time_passed` and publish an
unavailable tombstone under the normal guards. Merely reaching an existing
head's `reset_at` makes it expired even without a successful subsequent read;
there is no deadline extension, reset-completed event, or automatic recovery
fact. A separate availability policy may let its own observation bound expire;
that is not evidence of recovery. Historical or latest-reset sections cannot be
promoted to a new future forecast, and an old error/absence cannot resurrect a
withdrawn head.

The user or calling agent can always declare a separate manual reset input for
explicit registration without credentials or a running service. They may also
replace an existing channel with source `manual`, which invalidates the fetched
head first. That is a caller assertion, not verified Codex quota. A failed public
reader must not block this manual path or invent a provider quota-query API.

## Acceptance

The specified behavior requires the following observable outcomes; these are not
claims of exercised runtime integration:

1. Inputs are provider-scoped, typed, explicitly model-covered, and available
   through declaration/inspection operations without acquiring credentials.
   Schema/reference errors differ from a well-typed unsupported source capability.
2. Manual recording and explicit supported refresh persist usable values with or
   without the service. Source separation is exact: neither operation silently
   overrides, merges with, or switches the other source. Read-only and dry-run
   operations cause no acquisition, secret resolution, publication, or service start.
3. Quota values accept exactly a documented decimal percentage or complete
   comparable counters. Counter ratios such as one out of three compare exactly;
   missing denominators, incompatible units/windows, absent data, and local
   traffic never become invented percentages or zero quota.
4. Publication rejects future observations, nonfuture reset times, expired new
   values, and invalid manual expiry bounds. A retry, failure, local cached body,
   or restart does not extend an observation's validity. A genuinely new successful
   source observation is distinguished by its timestamp basis and hard caps.
5. At one selected evaluation instant, the ordered applicability table gives one
   deterministic result for missing, malformed, mismatched, cleared, withdrawn,
   future-observed, and expired heads. At reset time the value is expired, not
   proof of reset completion. `--at` cannot reconstruct a removed/superseded head.
6. Clear, record, source/scope/coverage changes, authorization changes, removal,
   and same-name re-creation defeat late acquisitions. Same-instant conflicting
   observations reject; identical retries are idempotent; withdrawal retains only
   its bounded ordering watermark. Unrelated edits and local provider rename do
   not invalidate independent values.
7. Automatic acquisition is explicit interval opt-in within the existing service,
   pauses without it, and continues under its coherent loaded policy despite
   pending saved edits or failed activation. At most one acquisition is in flight;
   it never prompts, overlaps, catches up, or retries between intervals. Admission
   never waits for acquisition.
8. Failed/unavailable attempts preserve an existing head only through its original
   expiry; qualified withdrawal clears it. Saved versus loaded applicability and
   failed persistence are reported without claiming unperformed activation.
9. The public Codex reader accepts only the qualified Markdown scheduling shape,
   absolute future timestamp, and announcement provenance. Missing/unsupported
   negative representations are honest failures, not guessed withdrawal; passed
   timestamps, history, counters, prose, or site confirmation never prove an
   individual account's restored quota. Manual registration remains available.
10. Storage is bounded to current heads/attempts for saved or still-loaded
    declarations. Removal/reset discards unreferenced runtime data; clear retains
    its tombstone, and admitted evidence retains only its bounded normalized
    snapshot. No secrets, raw source content, orphan fact catalog, or event history
    are introduced.
