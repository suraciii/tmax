# Diagnostics and request history

**Authority**: This document owns diagnostic evidence, effective-versus-applied
reporting, and local proxy request history. The [product contract](../product/spec.md)
owns provider lifecycle, Agent provider configuration, the model proxy, security, and billing scope. The
[configuration contract](../configuration/spec.md#save-reload-and-apply) owns
save, activation, and native application. [Model selection](../model-routing/spec.md#selection-and-continuity)
owns eligibility, priority evaluation, and conversation continuity.
[Input values and acquisition](../model-routing/inputs/spec.md#input-values-and-acquisition)
owns current quota/reset heads, validity, replacement, and acquisition.
The [CLI contract](../cli/spec.md) owns command grammar,
[dry-run and execution](../cli/spec.md#dry-run-and-execution),
serialized results, and exit codes. CLI and Web must expose the same diagnostic
facts and operation outcomes; the Web is not a prerequisite for CLI operations.
Shared terms are defined in [CONTEXT.md](../../CONTEXT.md).

This is a target contract, not an implementation claim. The
[product implementation status](../product/spec.md#implementation-status) remains
applicable.

## Independent status facts

Diagnostic detail must keep these facts separate. Summary views must not conflate
them; success in one row must not supply success in another.

| Fact | Required distinction |
| --- | --- |
| Configuration | The sole authority `~/.tmax/tmax.toml`'s validation result and revision versus the loaded configuration and accepted metadata revisions. Saved, invalid, and saved-but-not-active settings must not appear active. A target with empty native providers/selections or no default is valid but incomplete for native application, not an invalid login source or a blocker for independent proxy inference. |
| Native application | Not applied, applied, awaiting native reload, conflicted, blocked, or partially applied/recovery-required, with the affected target and applied revision. Written native settings are not proof of running-agent use. |
| Process and listener | Actual resident identity, authority, executable/management compatibility, and inference/Web listener states under the [resident contract](../product/spec.md#resident-service-identity). Distinguish absent, identified compatible, incompatible, unidentified occupied endpoint, and unavailable evidence. A PID or a running Web interface does not prove inference availability or usable upstream authorization. |
| Startup registration | Saved autostart intent versus this authority's actual OS registration, ownership, and latest reconciliation outcome. Drift is not a failed unrelated edit; explicit repair follows the [configuration contract](../configuration/spec.md#save-reload-and-apply). |
| Effective agent state | The latest qualified evidence of the running target's native provider entries, each source and endpoint/model selection, and native configuration precedence. Applied files, a requested reload, or a reachable listener alone must not verify that effective state. Unsupported runtime inspection must remain unknown. |
| Provider authentication | Configured authentication method and login source `auth.via`, when applicable, and locally known credential availability versus a qualified upstream authentication result. A secret reference, readable local secret file, or configured key must not become a verified upstream login. Unknown upstream account identity must remain unverified. |
| Shared proxy access and models | Current shared-key availability and loaded acceptance state versus listener readiness and the common enabled, protocol-qualified publication list. No Agent registration, native selected subset, or target default is needed for inference. Native subsets do not restrict requests. A shared key does not identify a target or conversation, and a saved reference is not proof of a readable key. |
| Model reachability | The exact provider authorization, offering, native protocol, transport, and tested capability scope of a successful or failed request. Catalog presence, model discovery, authentication success, and quota-query success must not verify inference or untested capabilities. |
| Quota freshness | The latest applicable provider observation, its freshness, and the latest refresh outcome. Unknown, stale, unavailable, and manual information must remain distinct from a fresh authoritative observation. Quota, balance, credential expiry, quota reset, and renewal must remain separate facts. |
| Priority inputs | Each declared provider-local quota/reset channel's source, declaration and current-head revisions, scope/model coverage, normalized value or unavailable reason, applicability to saved versus loaded settings, and latest acquisition outcome. Manual quota, a public reset forecast, provider quota evidence, and an actual completed reset are different facts. Acquisition intent, a running/paused/blocked acquisition state, and head freshness must remain distinct. |
| Outbound network | Saved versus loaded global mode and nonsecret proxy destination, with the policy used by the operation. A configured proxy is not proof of network reachability. Native-agent networking is outside this evidence scope. |

Views must identify the affected targets and the next supported action for an
incomplete fact. A stopped listener must not turn applied native configuration
into an unapplied state. An unavailable credential must block only its affected
scope, not imply that every provider or target is unavailable.
After a partial native sign-in, show the affected shared credential scope and
whether its current authorization is established. A failed local save or an
unchanged session reference must not preserve a current success claim for native
authorization that changed or can no longer be verified.

Management operations use compact JSON by default, with optional `--format text`.
The [CLI contract](../cli/spec.md) owns summary and detail projections. Routine
lists and successful edits need not repeat full evidence, diffs, metadata, or
derivable inspection commands. Explicit detail reads expose the evidence below.
Relevant current revisions, actual saved/loaded/native outcomes, blockers,
current-user-restricted recovery handles, and partial or unknown effects must remain explicit.
Dry runs retain the full impact under the
[execution contract](../cli/spec.md#dry-run-and-execution), without effects,
reservations, secret reads, login, or upstream calls. Execution uses current inputs.

A request that passes through the proxy can provide evidence for that request's
scope. It does not verify every model, protocol, target, or continuation path.
Native upstream-source entries send inference directly to the provider, outside
proxy observation, while proxy-source entries in the same target may send other
traffic through tmax. No proxy history entry is expected for unobserved upstream
traffic. Missing records must not prove direct access, bypass, inactivity, or
success; requests may be outside coverage, unobserved, cleared, or evicted.

## Evidence scope and currency

Explicit diagnostic detail must make the following evidence available for each fact:

- Its source and evidence kind: configured intent, local validation/inspection,
  explicitly supplied manual input, public reset announcement, qualified provider
  observation, explicit inference probe, or observed proxy request.
  Documentation, catalog metadata, manual assertions, scheduled forecasts, and
  runtime verification must remain distinguishable.
- The observation/check time and the latest attempt outcome. A read time must not
  replace the time at which the fact was established. Input observation, receipt,
  and expiry times remain distinct under the input authority.
- Its scope: relevant provider and safe authorization identity, target, offering,
  protocol, transport, capability, and billing window or coverage when applicable.
  Input kind, source, declaration, covered models, and scoped reset/window
  identity must remain attributable. Unestablished scope remains unknown.
- The relevant revisions or instances: desired and loaded configuration,
  accepted metadata, native application/effective inputs, authorization generation,
  input declaration/head revisions, and process/listener instance as applicable.
  A missing required revision must be explicit, not inferred from current state.
- Its result, freshness or superseded state, any named evidence gap, and a safe,
  actionable reason. Unsupported checks must report unavailable, not success.

Compact summaries may omit this full evidence projection. They must retain safe
resource identities, meaningful states, and material blockers or uncertainty.
Omission must not turn unknown, partial, stale, or saved-but-not-active facts into
success. A summary's state must remain consistent with its detailed evidence.

An evidence result must not apply beyond its recorded scope. A change to a
relevant endpoint, authorization, model/protocol capability, accepted metadata,
native input, or configuration dependency must supersede the old result for the
changed scope. A process restart must not reuse an earlier listener instance's
availability as current evidence. An unrelated change need not invalidate evidence
whose dependencies are known to be unchanged.

Superseded or stale evidence may remain visible as dated evidence only where
its owning lifecycle retains it. It must not supply a current successful/green
status for a replacement scope. This does not require a catalog of old input
heads. If dependencies cannot be established, current verification remains
unknown. Input validity and reset semantics come from the input authority;
other observations use their qualified integration. Use the input authority's
[validity and applicability](../model-routing/inputs/spec.md#validity-and-applicability)
projection, not a diagnostic-specific freshness hierarchy or guessed interval.

A failed refresh must show the latest failed attempt separately from an older
observation. It must not silently refresh the older observation's timestamp.
A network error must not become definitive credential revocation, quota
exhaustion, or a missing model. A generic HTTP status must not establish a more
specific provider cause without qualified evidence.

[Input heads](../model-routing/inputs/spec.md#input-records) and
[acquisition attempts](../model-routing/inputs/spec.md#last-acquisition-attempt)
are separate evidence. A failed attempt does not revise the retained head's
observation time or original expiry. A qualified withdrawal or explicit clear
must not be rendered as a failed network request with a still-active forecast.
Present the input authority's actual head state and attempt outcome without
inventing a history of former values. Exact percentage strings or the exact
remaining/total/unit counter group may explain a configured rule; they are not
account-wide billing evidence, and local request counters must not fabricate them.

## Reads and explicit checks

Status, provider/model inspection, history reads, and selection previews must be
side-effect-free. They may inspect the local inputs permitted by the owning
contracts. They must not start or stop listeners, change native settings, execute
command-backed secret resolution, sign in, refresh credentials,
contact a provider for new observations, or perform inference.
refreshing a Web view must not trigger those actions.

Explicit `proxy key show` and its protected Web reveal equivalent are sensitive
retrieval operations, not diagnostic inspection. Their normal results report
only protected-delivery outcomes under the
[key-disclosure contract](../cli/providers/authentication/spec.md#shared-proxy-key).
Status, source inspection, history, and ordinary management results never expose
the key. Key replacement reports known managed dependencies separately from the
unknown, nonenumerable unmanaged-client scope; it cannot claim all clients updated.

An explicitly configured service acquisition interval is a separate opt-in under
[Acquisition](../model-routing/inputs/spec.md#acquisition).
It permits only that authority's bounded non-inference acquisition while the
existing local service runs. It is not a status/preview side effect or permission
for general background checks, sign-in, or inference probes. Input/acquisition
views must report the saved/loaded intent, active or paused/blocked condition,
latest attempt, and known due state without starting the service or acquiring a fact.
Admission and preview consume published heads without waiting for acquisition.

An explicit observation refresh must identify the provider, check kind, and
requested scope before it runs. Supported non-inference authentication,
model-discovery, and quota/balance checks are separate evidence kinds. They may
use the selected provider's authorization only through its qualified integration.
Named input refresh must follow the input authority's declared source, scope,
and manual-versus-fetched restrictions; a provenance URL is not an arbitrary
fetch destination. Neither kind of refresh may silently substitute an inference
call when a non-inference source is absent. An unsupported source must remain
unavailable with the supported next action or provider-dashboard link.

Observation refresh updates dated evidence, not desired configuration, user
priorities, schedules, or native settings. Manual input record/clear is explicit
caller-supplied data management, not a verified provider check. No prior dry run
is required. The declared login source may refresh an existing
session through its qualified flow without changing authorization identity or
source. If the operation needs a new sign-in, replacement authorization,
listener lifecycle change, or native write, it must stop with that prerequisite
and identify the explicit operation that owns the requested effect.
Refresh failure must preserve unrelated observations and must not disable an
otherwise qualified inference path.

A paid inference probe is a separate explicit action. Its command must identify
one qualified offering, its provider authorization, exact native protocol and
transport, and the capability actually tested. Help and optional `--dry-run`
must disclose that this scope can consume quota or incur charges, including when
the amount is unknown. The explicit probe invocation requests that cost-bearing
operation; no separate cost-acceptance flag, approval state, token, or prior dry
run is required. A model-alias selection preview must not silently probe its
candidates. A probe must not change its model, provider, or authorization after a
failure. It must report the tested scope, time, result, and unknown execution or
usage information without claiming account identity, plan, quota, or renewal from a
successful reply.

Probes must remain separate from process/listener health and non-inference
observation refresh. Opening a view, starting a listener, applying configuration,
or an inference failure must not schedule background inference probes. These
operations must not imply paid inference. A probe that passes through the proxy
must be distinguishable from ordinary client traffic in its request record.
A direct probe must not claim proxy observation of traffic tmax did not receive.

## Minimal request record

The proxy must assign an opaque local correlation ID before authentication and
selection so that a rejection can be diagnosed without a chosen destination.
The ID must be scoped to the proxy-process instance. It must not encode or derive
from credentials, request content, or native session identifiers. Possession of
it must not authenticate a client, resume a conversation, or grant any capability.
It must not be used as a substitute for the qualified session identity required
by the continuity contract.

A request record is a minimal diagnostic event, not an OpenTelemetry trace,
conversation transcript, or replay record. Only the following semantic data may
be retained or returned by request-diagnostic views; serialized field names
belong to the CLI contract.
History lists may show compact summaries. Explicit detail reads expose retained
decision evidence and its gaps without rerunning selection or adding unretained data.

| Data | Required meaning |
| --- | --- |
| Correlation and origin | Local correlation ID, proxy-process instance, and ordinary client request versus explicit probe when known. |
| Optional native context | Safe local Agent target and native provider name only when independently established by a qualified mechanism. Both may be unknown, including after successful shared-key authentication. Neither the shared key nor unsupported caller claims establish native context. |
| Requested model and protocol | Validated public model ID, concrete or alias kind, and native protocol when safely established. Unknown, unrecognized, and malformed values must be distinguishable without retaining their raw input. Serialized kind values follow the [CLI history contract](../cli/service/diagnostics/spec.md#paid-inference-probes-and-request-history). |
| Selected destination | Selected qualified offering, local provider reference, and a safe local handle for the selected authorization generation, only after selection actually occurs. The handle must not reveal a secret, secret fingerprint, upstream account identifier, or native session token. |
| Admission and revisions | Admission timestamp, loaded configuration revision, accepted metadata revision, and the coherent vector of relevant input declaration/head revisions or explicit absent/cleared states used at admission. Retain safe scope/authorization-generation handles, not upstream identity strings. A rejection before admission shows receipt time and that admission did not occur. If pre-dispatch revalidation repeats selection, distinguish the snapshot/instant actually used by that decision from the initial one. |
| Decision basis | Actual selection instant; fixed concrete, new dynamic, or retained-destination reason; selected named signal rule or calendar/base fallback; effective priority; and stable tie-break reason when relevant. Include ordered captured rule outcomes and their controlled reasons, including shadowing, without claiming unevaluated predicates. A fixed destination's displayed priority is not the cause of its selection. Inapplicable, omitted, or unavailable details remain explicit. |
| Rule and calendar basis | Only validated typed rule ID/order/enabled state, quota/reset kind derived from the input declaration captured for that decision, replacement priority, horizon, exact decimal-string threshold, and bounded typed `time_window` definition/boundaries when present. Retain an evaluated gate's actual match result, saved zone, and UTC offset; do not invent a gate/input result for a disabled or shadowed rule or an input result after `outside_time_window`. |
| Input scalar basis | Safe provider/input references; opaque `provider_identity`, `authorization_generation`, declaration/head revisions; source/kind and a validated nonsecret scope or opaque scope handle; applicability and its controlled unavailable reason; normalized `observed_at`, `observed_at_basis`, `received_at`, `valid_until`, and `reset_at`; and the quota representation actually consulted: exact decimal-string `remaining_percent` or exact decimal-string `remaining`/`total` with a validated nonsecret `unit` or opaque unit handle, never an invented approximate percentage. Model coverage may be represented by the validated offering and coverage result. Use only the [input-record fields](../model-routing/inputs/spec.md#input-records), not a source response, arbitrary adapter map, or old-input catalog. |
| Result and stage | In progress, rejected, completed, failed, cancelled, interrupted, or outcome unknown as established, with the stage reached and failure stage when known. Distinguish local admission/capacity, shared-key authentication, model validation/eligibility, selection, upstream credential availability/acquisition/refresh, upstream connection/dispatch/response/stream, and downstream delivery or cancellation. Unknown stages must remain unknown. |
| Safe upstream result | Validated upstream status and integration-allowlisted code when available, plus a safe classified cause. Absence of a status/code must not be replaced by an invented provider error. |
| Duration and usage | Observed duration and provider-reported usage in supported typed units when available. Partial and unknown usage, and whether downstream delivery completed, must be explicit. Usage token counts are not authentication tokens. |
| Completeness and execution | Whether the record is in progress, terminal and complete, partial, or missing an outcome; which usage fields are known; and the evidence for upstream execution or completion. Not dispatched, execution evidenced, completion evidenced, and execution unknown must not be conflated. |

The decision basis must be captured from the decision that was actually used.
A later history read must not rerun selection against current configuration,
clock, metadata, input heads, quota, or credentials. A retained session
destination must be identified as retained, not presented as the winner for a
new session. Keep its original decision basis distinct from the later request's
admission revisions. Missing original rule outcomes or scalar facts remain
unknown; a current head, a later observation, or `--at` must not reconstruct them.

Full candidate/rule explanations must fit the
[history bounds](#session-only-bounded-history). Preserve the actual selected
rule or fallback and effective-priority reason before optional competing-candidate
detail. If a bounded record omits explanation details, expose that evidence gap
and the captured subset. Do not round exact percentages/counters, invent a
percentage or missing outcome, retain an unbounded list, or rerun old decisions
to fill the gap.

If a request fails before selection, its destination and upstream authorization
fields remain absent. Shared-key acceptance or rejection never invents an Agent,
native provider, conversation, or upstream identity. Correlation remains possible
using the locally assigned ID and the actual failure stage. Target or native
entry removal does not erase retained history or revoke independent proxy access.

Retain independently established native attribution and its safe names as
captured, even after a target or native entry is renamed or removed. Native-context
history filters match only that retained attribution, require no current target
registration, and never match unknown context. Current settings must not
reconstruct or relabel the original attribution.

### Data minimization and safe errors

Request diagnostics must not retain or render request/response bodies, prompts,
completions, reasoning content, tool arguments or results, credentials,
authentication/session tokens, headers, raw upstream errors, raw URLs, or arbitrary
request fields. There is no request-body capture mode or opt-in archive feature.
The rule/input scalar allowlist permits neither raw source bodies, source
reference URLs, HTML/Markdown, arbitrary source fields, executable expressions,
nor upstream account/native session identifiers. A current input-inspection view
may show qualified provenance under the input authority; it must not copy that
provenance document or URL into a request record. Only allowlisted typed usage
counters may be extracted from usage responses; unknown usage fields must not be
copied as an opaque object.

The allowlist applies to error paths as well as successful requests. Error text
from providers, native tools, secret resolvers, parsers, and transports must not be
copied into diagnostic records or management output. Safe explanations must use
controlled reason codes/templates and validated, nonsecret local references.
An upstream code is safe only if the integration explicitly recognizes its
bounded format and meaning; arbitrary error strings are not safe codes.

Only established, nonsecret public identifiers may be rendered as model,
offering, or target names. Unrecognized or malformed caller values must produce a
field name and safe classification without echoing the value. If a name cannot
be established as safe, use an opaque local reference or unknown instead.
Per-field sizes and numeric ranges must be finite and validated before retention;
truncating a raw error or malformed field is not sufficient redaction.

### Forwarding and uncertainty

Diagnostic classification must not swallow a native stream error or replace an
interrupted response with fabricated success. Forwarding, cancellation, error
meaning, and available usage follow the
[product forwarding contract](../product/spec.md#model-proxy). A successful HTTP
status or connection alone must not count as a completed inference when its
stream fails or terminates without the required completion evidence.

Once dispatch may have reached upstream, a timeout, lost connection, cancellation,
or force-stop must retain unknown upstream execution or completion when the
transport cannot establish it. Local cancellation does not prove that upstream
execution stopped or that no charge occurred. Missing or interrupted usage must
remain unknown or partial, not zero usage or a known zero bill. Diagnostics must
not authorize replay to obtain a clearer outcome.

Local capacity or request-size rejection must remain distinguishable from
upstream throttling and show that inference was not dispatched. Unknown
operations and unbound resource requests must not invent a selected destination.
Connection, outbound-proxy, credential-acquisition, and refresh-deadline failures
must identify the actual known stage without exposing a raw URL or secret.
A local deadline is not evidence of upstream credential revocation or exhausted
quota. No text during valid reasoning is not itself a timeout or failed request
under the [transport-deadline contract](../product/spec.md#transport-deadlines).

## Session-only bounded history

Request history and its derived usage aggregates must exist only in bounded
memory for the lifetime of the proxy process. tmax must not persist them to disk
or reconstruct them from durable logs after restart. A new process must report
that earlier request coverage is unavailable, not infer earlier successes,
failures, or usage. Returning the retained safe metadata through CLI/Web reads
does not create a persistent history or archive capability.

Persisted current input heads and the last acquisition attempts have the
independent [retention lifecycle](../model-routing/inputs/spec.md#retention-and-removal).
They must not become an archive of superseded input facts or a source for
replaying historical ranking. Minimal original decision evidence retained with a
session binding follows the
[continuity contract](../model-routing/spec.md#selection-and-continuity); it must
not become a general durable request-history or input-history catalog.

The implementation must select, measure, and document concrete finite
record-count, resident-memory, and per-record bounds. CLI and Web must expose the
actual limits and retention/eviction policy; no numerical limit is fixed by this
specification. Active records, dry-run assessment, and any execution-local clear
snapshot must also fit bounded memory. Capacity eviction or diagnostic
incompleteness must not change request forwarding, request-lifecycle accounting,
or conversation bindings.

History results identify their proxy-process/reset scope, requested and retained
range or explicit empty state, and relevant coverage gaps once per result, not
per row. Listings state whether they include in-progress or outcome-incomplete
records. Explicit history detail/status reads also expose process start time,
latest clear/reset time and outcome, concrete capacity limits, retained count,
and evidence of eviction, deletion, or incomplete capture. Retained ranges expose
their oldest/newest timestamps when nonempty. A request ID outside retained
coverage returns unavailable with known coverage, not invented success or a
guessed absence reason.

An empty history means no records are retained in that coverage. It is not a
statement that no requests ran. Any eviction counts or coverage markers are
process-local diagnostic metadata, not a hidden all-time request or usage total.

### Clear execution and concurrent requests

`history clear` is an explicit mutation under the
[CLI execution contract](../cli/spec.md#dry-run-and-execution). Its optional
`--dry-run` reports the count and coverage of currently retained locally terminal
records and discloses in-progress records that would remain. It does not freeze
membership, create a token, reserve records, or authorize a later clear. No dry
run is required.

Actual execution takes a bounded operation-local snapshot of the exact terminal
retained record IDs in the current process/history reset identity. Terminal means
the local request has ended, including rejection, failure, cancellation,
interruption, or an unknown outcome; it does not mean successful inference or
known upstream completion. Terminal records must be immutable.

The operation deletes only members of that execution snapshot. Requests received
after the snapshot and requests still in progress when it was taken remain
eligible for normal retention, even if they finish before deletion completes.
It must not apply a moving time cutoff, clear whatever exists at commit time,
cancel a request, or suppress a later request's record. Arrivals and completions
must never broaden snapshot membership.

Restart, another successful clear/reset, or replacement of the history identity
during the operation must reject its stale mutation before deletion. Normal
capacity eviction may make snapshot members already absent; execution may remove
the remaining members but must report removed versus already absent counts and
must never substitute newly retained records. These checks concern changes
during one operation, not changes between a dry run and a later execution.

The clear result identifies the execution's process/reset scope, snapshot count,
removed/already-absent counts, retained concurrent coverage, and the new reset
identity/time. Its aggregates no longer include cleared records. No operation
snapshot is persisted or reused by another call. A dry run promises neither
future retention nor future-request suppression.

Clearing or evicting history, stopping the proxy, and restarting it must not delete
session bindings or their original decision basis, protected native-restore/recovery
information, authentication, current input heads/last acquisition attempts,
provider quota/balance observations, or desired settings. Their lifecycles belong
to their owning contracts. In particular, losing diagnostic history must not
turn an established session into a new selection.

### Aggregates and eligibility

Request counts, durations, and observed usage summaries must cover only retained
records and disclose that coverage and any gaps. They must not survive their
source records as an all-time total. Known usage and records with unknown or
partial usage must be reported separately. Unknown usage must not contribute a
fabricated zero. Incompatible units or scopes must not be summed.

Provider account observations must remain separate from these local summaries,
as required by the [billing contract](../product/spec.md#billing-quota-and-renewal-information).
Local history cannot establish account-wide usage, remaining quota, or an official
bill. Clearing history does not reset an upstream allowance.

Diagnostics record failures; they do not introduce a generic automatic circuit
breaker, transient-failure cooldown, failure counter for exclusion, automatic
retry, or fallback. Only the existing qualified authentication, capability, and
quota evidence in [Selection and continuity](../model-routing/spec.md#selection-and-continuity)
may change eligibility. A request error, diagnostic summary, or absent record must
not create a new exclusion, rewrite user priorities, or move an established
destination. Configured priority rules may evaluate published input heads under
the [rule authority](../model-routing/spec.md#rule-evaluation-and-explanation);
that evaluation is not a diagnostic inference of user intent or eligibility.

## Observable acceptance

1. Saved, loaded, applied, effective native-provider, listener/shared-key,
   authentication, model-test, quota, input-head, and acquisition facts can differ
   visibly. An incomplete Agent does not block an independent ready proxy.
   Relevant revision changes supersede old success within their actual scope
   without falsifying retained evidence or invalidating known independent facts.
2. Inspection and selection previews perform no inference, acquisition, provider
   refresh, secret resolution, or lifecycle mutation. Explicit non-inference
   refresh, manual input management, opted-in service acquisition, and
   exact-scope paid probes remain distinguishable. An opted-in interval must
   not turn opening a view or admitting a request into acquisition, and a probe
   needs no separate cost-acceptance flag or approval flow.
3. Proxy requests and failures before selection correlate by safe local ID.
   The shared key never supplies a target/native-provider or conversation identity,
   even after authentication. Qualified independent native context may be retained;
   unmanaged traffic remains valid with unknown context. Later configuration,
   input-head, rule-order, or identity changes cannot rewrite a recorded selection
   reason or reconstruct absent old facts.
4. Malformed model values and credential-bearing dynamic errors appear only as
   safe classifications. Diagnostics expose no bodies, tokens, headers, or raw
   errors. A failed stream stays failed or incomplete with honest usage and
   execution uncertainty while preserving native error forwarding.
5. Limits and retained coverage are discoverable. Eviction, empty history, native
   upstream traffic, and restart do not imply success, bypass, or complete coverage.
6. An optional history-clear dry run reports current terminal coverage without
   freezing membership or creating a prerequisite. Actual execution snapshots
   current terminal records and preserves concurrent and in-progress requests.
   An intervening clear/restart during that operation rejects its stale mutation.
   Cleared-record contributions disappear; restart drops all request history.
   Both preserve session bindings and original decision evidence, recovery
   information, authentication, current input heads/attempts, provider quota
   observations, and settings.
7. Retained summaries expose unknown/partial usage instead of zero and never
   create official bills, all-time totals, or generic failure-based exclusions.
8. For the Codex reset, weekly GLM quota, and expensive-period
   [scenarios](../model-routing/spec.md#signal-and-calendar-scenarios), detailed
   explanations expose matched, false, unknown, disabled, and shadowed rule states
   with the common instant, configuration/metadata/input revisions, captured
   derived kind, exact scalar basis, optional gate result/zone/offset, and calendar fallback.
   `outside_time_window` must not claim the input was evaluated. Never turn
   manual input or a scheduled reset into verified exhaustion, quota recovery,
   or a reason to move an old session.
9. At exact validity, horizon, threshold, reset, and saved-zone gate boundaries,
   preview and admission explanations agree for identical captured snapshots.
   Historical evidence remains the original decision, not a re-evaluation at
   the read clock. Bounded omissions stay explicit; source bodies/URLs and unsafe
   identifiers remain absent even when inputs are malformed or acquisition fails.
