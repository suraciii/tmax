# Common CLI results

This child owns shared serialization, outcome categories, and exits. Command
children own their data/detail shapes; [common grammar](../spec.md) owns invocation
and [execution](../spec.md#dry-run-and-execution). Domain specs remain authoritative
for behavior and evidence.

## Result envelope and output

Management emits exactly one compact UTF-8 JSON object plus newline on stdout,
including parse errors, blockers, dry-runs, partial effects, and failures. Never
mix progress, child output, usage, prompts, or a second result into stdout.
Optional redacted stderr is not needed to interpret JSON. Protected input/sign-in
use terminal or explicit inherited descriptors without corrupting stdout.
Explicit [gateway key show](../providers/authentication/spec.md#shared-gateway-key)
delivers only through its qualified separate terminal/pipe channel; stdout still
contains exactly the redacted management result, never the secret line.
`--format text` presents the same outcome/effects/blockers/actions; success goes
to stdout and errors to stderr. Offline help/version remain text, not envelopes.

| Required field | Type and meaning |
| --- | --- |
| `schema_version` | Integer `1`; incompatible implemented-envelope changes require a new version. |
| `operation` | Canonical command-path string, or null when parsing cannot establish it. |
| `code` | Stable machine-readable outcome independent of message wording. |
| `status` | `success`, `dry_run`, `blocked`, `incomplete`, `error`, or `cancelled`; Web self-restart alone may acknowledge `pending` under the [service contract](../service/spec.md#supervision-outcomes). Pending never establishes readiness or completed lifecycle. |
| `data` | Compact command-specific object, or null. Unknown facts are null or explicitly qualified. |

Include these fields only when relevant; do not require empty arrays or unavailable
revision metadata on every ordinary read/success:

| Field | Type and use |
| --- | --- |
| `config` | Config path and needed `file_revision`, `loaded_revision`, or `metadata_revision`. Values are strings or null when needed but unavailable. Known absent file revision is `absent`. |
| `affected` | `{kind, id, scope}[]` resource references. Kind is string; ID/scope are string-or-null, never secret content. |
| `boundaries` | `{name, state, affected, message}[]` for meaningful effect boundaries, always retaining completed/partial/uncertain effects after failure. |
| `blockers` | `{code, message, affected}[]` with actionable redacted reasons. |
| `next_actions` | `{code, operation, arguments, message}[]` for relevant supported actions, not boilerplate on every row/success. |

Resource IDs in command data suffice for simple success; they need not repeat in
an affected array. Config edits return the current file revision when known and
actual saved/loaded outcome. Revision-scoped indices, explicit guards, coherent
explanations, historical facts, and partial outcomes retain their needed revision
context. Routine successes do not need full before/after metadata or diffs.

Lists return stable provider IDs, qualified concrete names, `(protocol, id)` alias
identities, target-local native provider names and `{provider, id, protocol}`
selections, target IDs, or request/recovery handles as appropriate.
Useful readiness/count summaries stay compact. Help defines how to inspect those
identities; do not repeat derivable inspect argv for every item. Explicit inspect,
config show, and capability detail reads return documented metadata, evidence,
differences, and unknowns. Generated orphan secret references and actionable
partial-failure details must not be hidden for compactness.

`next_actions.arguments` is a nonsecret argv array after the canonical operation
path, excluding executable/command words. Never shell-split or interpolate it.
For example, `{"operation":"provider input inspect","arguments":["opencode-go",
"weekly-allowance"]}` or `{"operation":"model schedule remove","arguments":
["deepseek/deepseek-flash","0","--if-revision","CURRENT-REVISION"]}` identifies
an exact operation. External native action has null operation, empty arguments,
and human guidance naming the application/action. Do not invent an executable
tmax command. Any explicit detail-command object uses `{operation, arguments}`.

A structurally ready dry-run uses `dry_run`; blocked dry-run uses `blocked`.
Report full redacted changes, affected resources/sessions, warnings, relevant
current revisions, blockers, deferred credential/remote checks, and applicable
boundaries as `not_executed`. There is no execution token/persisted operation.
Key generation is described without generating/reserving a reference. Full
[configuration impact](../configuration/spec.md#configuration-impact-data) includes
pending file changes, not just the requested leaf edit.

Boundary names are `credential`, `file_save`, `catalog_apply`, `activation`,
`secret_delivery`, `gateway_lifecycle`, `autostart`, `native_write`, `native_reload`,
`observation`, `history`, `recovery`, and `web_entry`. States are `not_requested`, `not_applicable`,
`not_executed`, `completed`, `blocked`, `failed`, `partial`, or
`awaiting_native_reload`. Detailed/dry-run/failure output covers requested
boundaries and every completed/partial/uncertain side effect. Qualify uncertain
facts explicitly in messages/data, not as effect-free cancellation. Simple
success may report completed save/runtime/native outcomes in data rather than
repeat a boundary array. Unrequested autostart drift is `not_requested` plus
relevant diagnostic/action, not a failed unrelated edit; requested reconciliation
failure remains a failed/partial boundary.

Parse errors format without reading config/secrets. A valid explicit text format
still applies when another operand is invalid; invalid/conflicting format options
use default JSON. Omit irrelevant context; use null when a needed fact is unknown.
Name rejected command/option/field but redact arbitrary positional values and
secret-looking input rather than echo argv. Config errors name paths, not rejected
values. Ordinary results, stderr, examples, and logs must not expose secrets,
protected/rejected content, or unredacted credentials. The separate
[gateway error log](../../diagnostics/spec.md#persistent-gateway-error-log)
may contain raw upstream error content, which can echo sensitive material.
Shared gateway-key show is the sole explicit protected key-delivery operation,
never a generic export of upstream secrets. Its ordinary
result contains only reference/readiness and delivery outcome. Partial/uncertain
delivery reports exit 4 with the actual `secret_delivery` boundary, not a false
effect-free cancellation or a replay of secret bytes.

## Outcomes and exit categories

| Exit | Category | Interpretation |
| --- | --- | --- |
| `0` | Completed request or ready dry-run | Requested persistence and mandatory safety effects completed, or effect-free read/dry-run completed. Ordinary desired-state edits may report failed/unavailable background activation as a warning; success never implies loaded or native readiness. Reads may report unhealthy, stopped, stale, or unknown state. |
| `2` | Invalid input | Arguments, schema fields/types/references, TOML syntax, unsupported version, or invalid option combination rejected before effects. Post-effect failure uses 4. |
| `3` | Action required | Explicit key/native input, interaction, qualified scope, complete target selections, no active requests, current-input retry, daemon compatibility/identity, free consistent endpoint, explicit restart, or native reload is needed. Use blocked before effects, incomplete when earlier boundaries completed. |
| `4` | Operation failure | Requested I/O/refresh/probe/explicit activation/lifecycle/autostart/native work or mandatory safety effects failed or have partial/uncertain effects, including native login before local failure. Retain any completed save rather than claiming an effect-free failure. |
| `5` | Cancelled without effects | Cancellation known to precede every local/native effect. Partial/uncertain native or other effects use 4. |

Common stable codes are `ok`, `dry_run_ready`, `dry_run_blocked`,
`invalid_arguments`, `invalid_configuration`, `inputs_changed`,
`login_source_required`, `interactive_required`, `credential_unavailable`,
`storage_version_unsupported`, `storage_record_invalid`,
`unsupported_protected_input`,
`unsupported_protected_output`, `target_incomplete`, `daemon_reused`, `daemon_incompatible`,
`daemon_identity_unavailable`, `endpoint_occupied`, `endpoint_conflict`,
`active_requests`, `restart_required`, `awaiting_native_reload`, `saved_not_active`,
`partial_native_apply`, `recovery_required`, `observation_failed`, `probe_failed`,
`operation_failed`, and `cancelled`. Scoped provider reasons are not extra exit
categories; messages are not identifiers. Missing declarations/invalid references
are schema errors. Valid incomplete targets, unavailable credentials, or
unqualified external combinations remain availability blockers, not invented access.
Unsupported storage versions return `storage_version_unsupported`; malformed or
noncurrent record shapes return `storage_record_invalid`. Such files remain
unchanged; re-enter keys through normal setup. Unsupported recovery contents are
unavailable to current recovery operations.

Ordinary desired-state edits complete successfully after validation, persistence,
and required safety/identity effects. Attempt whole-file background activation
when a compatible service runs. A failed or unavailable background activation
does not turn the completed edit into a failed transaction: return `0`/`ok`, keep
the actual activation reason and failed/blocked boundary in data/warnings, and
report the saved and known loaded revisions separately. Unknown runtime state is
`unavailable`, not `absent`; do not claim that loaded state changed. Guidance must
inspect/reload/restart current saved state, not repeat an already committed create.
The same distinction applies after successful catalog acceptance.

Explicit `config reload`, lifecycle, native authorization, shared-key revocation,
and requested autostart effects retain their own failure exits. Failed explicit
activation uses `saved_not_active` with the previous known loaded revision;
an explicit restart requirement remains action-required, not active success.
No-service edits/reload may exit 0 after validation, persistence when requested,
and required startup reconciliation. Unchanged-intent registration drift alone
does not prevent 0; requested registration failure does, including explicit
same-state repair. Any nonzero post-save outcome is incomplete with the saved
fact/revision and actual failed/partial boundary, never an effect-free failure.

Completed `agent apply` and `agent restore` writes, including reload pending,
return success with exit 0 and code `awaiting_native_reload`; partial writes return 4 with changed/unchanged files
and safe recovery, not effective application. Explicit refresh/probe that cannot
run reports its blocker/failure even if a later read of retained evidence succeeds.
[Input results](../providers/inputs/spec.md#input-data-results) and
[authentication](../providers/authentication/spec.md#provider-authentication-and-local-secrets)
define their publication and native partial-effect outcomes.
