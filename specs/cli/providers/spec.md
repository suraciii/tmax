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
| `provider list` | Compact IDs/names, `source` (`upstream` or `gateway`), enabled/authentication state, billing/quota freshness, safe file/loaded quota suspension projections, and affected-target counts. |
| `provider inspect ID` | File/loaded endpoints, configured/inherited/overridden/effective metadata and provenance, `source`, authentication `via`/known-or-unknown identity, model access, subscription/balance facts, revision-qualified file/loaded quota guard policy, safe file/loaded quota suspension status, input readiness, retained secret references, and file/loaded differences. The derived gateway provider additionally reports its route-name children and a `delivery` block whose `targets` array holds one `{target, entry, applied_delivery}` item per declared relayed entry, with the last field shaped by [Agent result data](../agents/spec.md#agent-result-data). |
| `provider refresh ID --scope SCOPE` | Explicit qualified non-inference checks. `SCOPE` is `auth`, `models`, `quota`, or `all`; return each check/channel separately. Model discovery registers nothing. For the derived gateway provider `tmax`, see [Gateway provider refresh](#gateway-provider-refresh). |

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

`provider refresh` for an upstream provider returns `data.checks`, an array of
`{scope, input_name, outcome, reason_code, attempt}`. Input name/attempt are null
for non-channel checks. Outcome is `completed`, `unavailable`, `failed`, or
`not_executed`; channel attempt uses the [input result](inputs/spec.md#input-data-results).
Dry-run reports intended checks without secrets, writes, or upstream calls.

### Gateway provider refresh

`provider refresh tmax --scope SCOPE` checks the derived gateway provider. Without
a configured gateway that provider ID is not in the inventory, and the request
fails as an unknown resource. `SCOPE` is required. The result is
`data = {provider: "tmax", boundary: "reachability_only", checks: [...]}` with one
check per requested scope in the order `auth`, `models`, `quota` for `all`. Checks
run independently: a failed check must not prevent another requested scope from
running, and every requested check appears in `data.checks`.

Each check reports its own `scope`, `outcome`, and `boundary`. A `failed` check
makes the result `observation_failed` with exit 4. A requested check that could
not run reports `not_executed` and fails that verification the same way —
`observation_failed` with exit 4 — never as success; `gateway_not_ready` and
`no_daemon_history` are examples. A dry run is not a failed verification and
exits 0.

| Scope | Bound | Completed proof |
| --- | --- | --- |
| `auth` | One authenticated `GET /v1/models` with a 5-second deadline. | `proof: shared_key_accepted` |
| `models` | One bounded inference request per enabled route with a 60-second deadline per route. | Per-route `proof: route_reachable_through_gateway` |
| `quota` | Read of the identified daemon's observed usage; never an upstream request. | `proof: observed_gateway_usage`, `upstream_contact: false` |

Auth check data:

| Outcome | Fields |
| --- | --- |
| `completed` | `proof: shared_key_accepted`, `publication_model_count` (nonnegative integer, or null when the response body is unavailable). |
| `failed` | `reason_code`, optional `http_status`, optional `repair`. |

| Auth `reason_code` | Condition | `repair` |
| --- | --- | --- |
| `gateway_not_ready` | The daemon is not identified as compatible or the listener is not ready. | `tmax gateway start` |
| `key_rotated_not_reloaded` | The loaded shared-key reference differs from saved configuration. | `tmax gateway restart` |
| `shared_key_rejected` | The listener answered 401 or 403; `http_status` is present. | `tmax gateway key replace` |
| `gateway_unreachable` | The loopback request could not be sent. | absent |
| `gateway_response_invalid` | The listener answered another non-success status; `http_status` is present. | absent |
| Credential or storage read code | The stored shared key could not be read. | `tmax gateway key replace` |

Models check data:

| Field | Contract |
| --- | --- |
| `outcome` | `completed` when every checked route completed; `failed` when a route or the shared key failed; `not_executed` when no request ran. |
| `routes` | One `{route, protocol, outcome, reason_code?, http_status?}` per enabled route when requests ran; the route-name array for `not_executed`/`gateway_not_ready`; one `{route, protocol, outcome, reason_code}` per enabled route when a shared-key or credential failure stopped the requests. |
| `summary` | `{routes, completed, failed}` over the checked routes; present whenever route items are reported, including a pre-request failure that lists every route, and absent for `not_executed`/`gateway_not_ready`. |
| `boundary` | `reachability_only`. |

Each route is checked with the first of `responses`, `chat-completions`, and
`messages` that the route declares. A completed route reports
`proof: route_reachable_through_gateway`. A failed route reports the gateway's
stable reason code from a bounded error body, or `http_<status>` when none is
available; a transport failure is `unreachable`, and a missing declared protocol is
`protocol_unavailable`. When the gateway is not ready, every route is listed by
name with `not_executed`/`gateway_not_ready` and no request is sent. When the
daemon loaded a different shared-key reference than saved configuration, every
route reports `key_rotated_not_reloaded` with `repair: tmax gateway restart`. When
the shared key cannot be read, every route reports that failure and no request is
sent.

Quota check data:

| Field | Contract |
| --- | --- |
| `outcome` | `completed` when observed usage is available; otherwise `not_executed`. |
| `reason_code` | Null when completed; otherwise the unavailable reason, for example `no_daemon_history` without a daemon. |
| `usage` | The observed gateway usage projection; unknown and partial usage remain separate. |
| `proof` | `observed_gateway_usage`. |
| `upstream_contact` | `false`. |
| `boundary` | `reachability_only`. |

A dry run reports `outcome: not_executed` and `reason_code: dry_run` for every
requested scope, with `planned` data instead of a check result. It sends no HTTP
request, resolves no shared key, and contacts no daemon. `planned` is
`{listen, method, path}` for `auth`, `{routes: [{route, protocol}]}` for `models`,
and `{source: observed_gateway_usage, upstream_contact: false}` for `quota`.

A successful refresh proves reachability from tmax only. It must not prove that an
Agent target loaded or used the configuration, that an untested protocol works, or
that observed usage is a provider-side balance. The `reachability_only` boundary
belongs on the top-level result and on every check object, including a dry-run
check.

Examples: `tmax integration list`, `tmax integration inspect deepseek`,
`tmax provider list`, `tmax provider inspect deepseek`,
`tmax provider refresh deepseek --scope quota`, and
`tmax provider refresh tmax --scope models`. Registry-based help must use
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

The derived gateway provider is not a user-managed instance: `provider add`,
`provider rename`, and `provider remove` refuse its reserved ID, and its lifecycle
belongs to [Gateway provider](../../product/spec.md#gateway-provider).

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
