# Provider and integration commands

This child owns provider integration discovery, provider settings, refresh, and endpoint
results. [Integrations](../../integrations/spec.md) owns qualification;
[Configuration](../../configuration/spec.md) owns saved fields. Use
[common CLI rules](../spec.md), [configuration changes](../configuration/spec.md),
and [shared results](../results/spec.md).
[Authentication](authentication/spec.md) and [inputs](inputs/spec.md) have their
own command contracts.

## Integration discovery

| Command | Result |
| --- | --- |
| `integration list` | Compact stable integration IDs, protocols, endpoint-setup requirements, and authentication summaries. |
| `integration inspect ID` | Fixed protocol bases/custom-base requirements, finite method/path operations and original-binding requirements, protocols/transports, authentication methods/login sources, supported input source/kind/scope/model coverage and normalized fields, noninteractive acquisition, restrictions, and known/unknown compatibility evidence. |
| `provider list` | Compact IDs/names, enabled/authentication state, billing/quota freshness, safe file/loaded quota suspension projections, and affected-target counts. |
| `provider inspect ID` | File/loaded endpoints, configured/inherited/overridden/effective metadata and provenance, authentication `via`/known-or-unknown identity, model access, subscription/balance facts, revision-qualified file/loaded quota guard policy, safe file/loaded quota suspension status, input readiness, retained secret references, and file/loaded differences. |
| `provider refresh ID --scope SCOPE` | Explicit qualified non-inference checks. `SCOPE` is `auth`, `models`, `quota`, or `all`; return each check/channel separately. Model discovery registers nothing. |

Provider integration discovery reads the built-in qualified registry without catalog,
network, credentials, providers, targets, or valid config. Returned IDs are the
provider-setup operands, not instance/catalog IDs or editable plugin definitions.
Unknown IDs are errors. Discovery neither invents endpoints nor proves a client's
authorization compatible; unsupported/unknown combinations stay explicit.

Refresh supports `--dry-run`. It may update observations and invoke the configured
login source's qualified same-identity refresh under
[credential-refresh coordination](../../product/spec.md#credential-refresh-coordination).
It cannot sign in, switch identity, mutate another source's credentials, import
native settings, apply catalog metadata, run inference, or edit desired settings.
`--scope quota` and the quota part of `all` include declared provider-source quota
channels covered by that integration's quota acquisition. They exclude manual
inputs and Codex reset announcements; use `provider input refresh` for a selected
nonmanual channel. Report unsupported/unavailable checks and channels with no
qualified acquisition. A failed check is not revocation, zero balance, or generic
routing exclusion. Explicit refresh creates no scheduler or inference replay.
Outbound work uses [network policy](../service/spec.md#outbound-network-configuration)
and [transport deadlines](../../product/spec.md#transport-deadlines).

`provider refresh` returns `data.checks`, an array of
`{scope, input_name, outcome, reason_code, attempt}`. Input name/attempt are null
for non-channel checks. Outcome is `completed`, `unavailable`, `failed`, or
`not_executed`; channel attempt uses the [input result](inputs/spec.md#input-data-results).
Dry-run reports intended checks without secrets, writes, or upstream calls.

Examples: `tmax integration list`, `tmax integration inspect deepseek`,
`tmax provider list`, `tmax provider inspect deepseek`, and
`tmax provider refresh deepseek --scope quota`. Registry-based help must use
actual supported IDs instead of promising guessed integrations.

## Provider operations and subscription fields

| Command | Requested change |
| --- | --- |
| `provider add ID [--endpoint P URL]... [EDIT]` | Create one instance, optionally without authentication, subscription, or models. Use `--set metadata` and `--set integration` for scalar selections; only custom integrations accept protocol bases. |
| `provider update ID [--endpoint P URL]... [--clear-endpoint P]... [EDIT]` | Edit selected scalar fields/protocol entries, preserving other settings. At least one edit is required. |
| `provider rename OLD NEW` | Rename the local ID and all dependent qualified references, preserving identity. |
| `provider remove ID` | Remove contained settings after reference checks, with retained-secret/native disclosures. |
| `provider failure-reset ID` | Explicitly clear this provider's current consecutive failure streak and fixed-priority override; preserve settings, history, bindings, and quota facts. Supports effect-free dry-run. |
| `provider quota-resume ID` | Clear automatic quota suspension without changing enablement or settings. `--dry-run` is effect-free and `--if-revision` guards the saved configuration. A new confirmed denial may suspend again. |

Suspension status is separate from the optional quota guard and provider enablement.
Inspection reports `suspended`, `available`, `manually_resumed`, `unsupported`, or
an explicitly unavailable view; safe suspension evidence may include its source,
window, suspension time, reset hint, and next check time. The loaded view describes
current routing; a saved projection need not match it. Resume is not an upstream
availability check and does not enable a disabled provider.
The loaded projection comes from the identified daemon. A disk-only file view
does not establish current routing when publication is pending. Pending status
includes `persistence: pending`, `durability: unconfirmed`, and a safe
`persistence_error`; inspection must not silently substitute disk state.

Provider scalar selections use `EDIT` only. Display names are independent of local
IDs. The integration ID selects one product and fixed destinations under the
[endpoint rules](../../integrations/spec.md#endpoint-resolution).
Custom integrations require explicit protocol bases; known integrations require
no separate endpoint or region selector.

`--endpoint P URL` adds/replaces exactly one `endpoints` entry; repeat for distinct
protocols. `--clear-endpoint P` removes only that entry, not models or providers.
Reject duplicate protocols and a set/clear pair for the same protocol. Use these
operands, not scalar paths or serialized objects. Endpoint and integration edits
form one validated change; omitted entries remain unchanged. The
[endpoint schema](../../configuration/spec.md#provider-endpoints) owns selector
validation and the meaning of an omitted custom base.

| Provider `FIELD` | Scalar type and clear behavior |
| --- | --- |
| `metadata`, `integration` | Strings; clear removes the selection, subject to valid explicit integration/mapping requirements. Metadata change is not a rename. |
| `enabled` | Boolean; clear restores true. |
| `overrides.name`, `overrides.doc` | Strings; clear restores inheritance. |
| `subscription.plan`, `subscription.billing_period` | Strings; clear makes the manual fact unknown. |
| `subscription.renewal_date`, `subscription.expiry_date` | Dates; clear means unknown. |
| `subscription.auto_renew` | Boolean; clear means unknown, not false. |
| `subscription.charge.amount`, `subscription.charge.currency` | Nonnegative decimal-amount and uppercase three-letter currency strings, required together. |
| `failure_policy.enabled` | Boolean; default true for an explicitly complete policy. |
| `failure_policy.errors.http_statuses` | Comma-separated unique integers 400–599. |
| `failure_policy.errors.connection_errors`, `failure_policy.errors.transport_errors` | Booleans selecting failure classes. |
| `failure_policy.consecutive_failures` | Positive unsigned 32-bit threshold. |
| `failure_policy.priority` | Signed fixed replacement score, not a delta or cap. |
| `failure_policy.duration_seconds` | Positive integer duration in seconds. |
| `failure_policy` | Clear-only container; remove the policy. |
| `quota_guard.max_observation_age_seconds` | Positive integer seconds; default `3600` when the policy table is enabled. |
| `quota_guard` | Clear-only container; remove the policy. |

Clear also accepts `endpoints`, `overrides`, `subscription`, `subscription.charge`,
and `quota_guard`. Clearing endpoints may combine with protocol operands to
rebuild the table atomically. Subscription clear does not cancel upstream;
charge clear makes price unknown. Refresh never rewrites these facts.
Authentication uses dedicated commands, not scalar auth edits.

```sh
tmax provider update opencode-go --set subscription.charge.amount 20.00 --set subscription.charge.currency USD
tmax provider update opencode-go --clear subscription.charge
tmax provider update opencode-go --set quota_guard.max_observation_age_seconds 3600
tmax provider update opencode-go --clear quota_guard
tmax provider add custom-gateway --set integration custom --endpoint chat-completions https://gateway.example/openai/v1 --endpoint messages https://gateway.example/anthropic
tmax provider update custom-gateway --endpoint responses https://gateway.example/responses/v1
tmax provider update custom-gateway --clear-endpoint responses
tmax provider add deepseek-known --set metadata deepseek
tmax provider inspect deepseek-known
```

Prices are user-maintained, not live tariff claims. Custom URLs are illustrative
caller-supplied bases, not verified services; `custom` defines finite operations.
The DeepSeek example uses its integration's complete fixed endpoint map, unlike
omitted custom entries. Help lists exact product-qualified integration IDs and
their supported protocol bases; there is no separate region selection.


`provider inspect` includes `failure_state`: current streak, fixed override,
trigger reason/time, and expiry or an explicit unavailable state. The
[routing policy](../../model-routing/spec.md#request-fallback-and-failure-priority-override)
owns semantic state transitions. A manual reset never initiates an upstream probe.

## Endpoint setup and resolution data

`EndpointMap` is an object with only `chat-completions`, `responses`, and
`messages` keys and base-URL string values. Absence never inherits another
protocol's base.

`EndpointSupport` contains:

- `kind`: `known` or `custom`.
- `setup`: `fixed` or `explicit_bases`.
- `endpoints`: one fixed `EndpointMap` for a known integration, empty for custom.
- `operations`: finite `{protocol, method, path_template,
  requires_original_binding}[]`, using declared operation strings and a boolean
  binding flag, not arbitrary request destinations.

`integration inspect` returns `data.endpoint_support: EndpointSupport`.
Integration-list entries return `endpoint_setup` with that setup enum. Catalog
setup detail returns `integration_choices` entries with `integration` and
`endpoint_setup`; help defines the corresponding integration inspection command.
No list item must repeat derivable inspect argv. Catalog `api` metadata does not
replace registry qualification.

`ProviderEndpointView` contains `snapshot_state` (`available`, `absent`, or
`unavailable`), `configuration_revision` (string-or-null), `configured`
(null or `{endpoints: EndpointMap}`), `resolution`
(null or the object below), and string-or-null `reason_code`. Absent snapshot,
missing provider, and unreadable data remain distinct, not empty resolutions.

Resolution contains `kind` (`known` or `custom`),
`source` (`integration` or `explicit_bases`), `endpoints: EndpointMap`, and
`unavailable_protocols: P[]`.
A resolved base is config evidence, not connectivity/authentication/transport or
continuation qualification.

`provider inspect` returns `data.endpoints = {file, loaded}`, one view per snapshot.
Routine add/update summarizes the endpoint change and saved/loaded outcome; use
inspect for both complete views. Dry-run and endpoint failures return
`data.endpoint_resolution` for the evaluated candidate when available, with
[config impact](../configuration/spec.md#configuration-impact-data). Candidate
bases are not claimed saved, loaded, reachable, or reserved.

## Quota display data

Quota facts remain display-only observations. `provider inspect` returns the
observation projection at `data.observations`; its `billing.display` is the
source-neutral report and `billing.dashboard` the provider console link.
`provider list` repeats the same report as `display` on each quota entry under
`observations.observations`, together with `billing_kind` so a caller can
distinguish a supported source from one that never has a quota operation. A null
`display` means the integration has no subscription quota source; it never means
zero allowance.

`display` contains:

- `plan`: null, or provider-reported `{name?, level?, expires_at_ms?}`. Raw
  `expires_at_ms` stays unqualified and is not a renewal date.
- `rows`: one entry per provider-reported window or bucket.
- `notes`: stable qualification codes. Surfaces localize them; they are not
  user-supplied text.

A row contains `label`, `basis`, optional `percent`, optional exact `counters`,
optional verbatim `status`, optional `expiry`, and `reset`.

`label` keys are `rolling`, `weekly`, `monthly`, `weekly_allowance`, `duration`
(`duration`, `unit`), `bucket` (`name`, `window`), `limit` (raw `type`, `unit`,
`number`), or `credit` (`kind`). `basis` is `consumed_percent`,
`remaining_percent`, `counters`, or `credit`. Counter values are the provider's
exact decimal strings; the projection never derives a missing counter, unit, or
window name.

`reset.kind` is `announced`, `provider_timestamp`, `window_end`,
`unqualified_epoch_millis`, or `absent`. `announced` and `provider_timestamp`
carry an ISO `at` and are qualified instants. `window_end` and
`unqualified_epoch_millis` carry provider `raw` epoch milliseconds plus
`encoding`; a surface may show a conditional local conversion only when it is
labeled unconfirmed. Reset, subscription expiry, and renewal remain separate
facts.

Rows never invent a window the source does not report: MiniMax video is
weekly-only, and Kimi's `usage` and `limits` stay separate rows because they may
describe the same allowance. A percent basis keeps its source precision and
supports no arithmetic reconstruction of exact remaining allowance.
