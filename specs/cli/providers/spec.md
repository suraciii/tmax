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
| `integration inspect ID` | Regions, protocol bases/custom-base requirements, finite method/path operations and original-binding requirements, protocols/transports, authentication methods/login sources, supported input source/kind/scope/model coverage and normalized fields, noninteractive acquisition, restrictions, and known/unknown compatibility evidence. |
| `provider list` | Compact IDs/names, enabled/authentication state, billing/quota freshness, and affected-target counts. |
| `provider inspect ID` | File/loaded endpoints, configured/inherited/overridden/effective metadata and provenance, authentication `via`/known-or-unknown identity, model access, subscription/balance facts, input readiness, retained secret references, and file/loaded differences. |
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
| `provider add ID [--metadata CATALOG-ID] [--integration INTEGRATION] [--region REGION] [--endpoint P URL]... [EDIT]` | Create one instance, optionally without authentication, subscription, or models. Select metadata or an explicit supported integration and satisfy its endpoint requirements. |
| `provider update ID [--endpoint P URL]... [--clear-endpoint P]... [EDIT]` | Edit selected scalar fields/protocol entries, preserving other settings. At least one edit is required. |
| `provider rename OLD NEW` | Rename the local ID and all dependent qualified references, preserving identity. |
| `provider remove ID` | Remove contained settings after reference checks, with retained-secret/native disclosures. |
| `provider failure-reset ID` | Explicitly clear this provider's current consecutive failure streak and fixed-priority override; preserve settings, history, bindings, and quota facts. Supports effect-free dry-run. |

Add flags set their corresponding fields; setting the same field through `EDIT`
is an error. Display names are independent of local IDs. `REGION` is an exact ID
from the selected known integration. Its
[endpoint rules](../../integrations/spec.md#endpoint-resolution) decide whether a
singleton default suffices or an explicit region/known-base selector is required.
Custom integrations require explicit protocol bases.

`--endpoint P URL` adds/replaces exactly one `endpoints` entry; repeat for distinct
protocols. `--clear-endpoint P` removes only that entry, not models or providers.
Reject duplicate protocols and a set/clear pair for the same protocol. Use these
operands, not scalar paths or serialized objects. Endpoint, region, and integration
edits form one validated change; omitted entries remain unchanged. The
[endpoint schema](../../configuration/spec.md#provider-endpoints) owns selector
validation and the meaning of an omitted custom base.

| Provider `FIELD` | Scalar type and clear behavior |
| --- | --- |
| `metadata`, `integration` | Strings; clear removes the selection, subject to valid explicit integration/mapping requirements. Metadata change is not a rename. |
| `enabled` | Boolean; clear restores true. |
| `region` | Known-integration region string; clear removes the selector subject to resulting endpoint requirements. |
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

Clear also accepts `endpoints`, `overrides`, `subscription`, and
`subscription.charge`. Clearing endpoints may combine with protocol operands to
rebuild the table atomically. Subscription clear does not cancel upstream;
charge clear makes price unknown. Refresh never rewrites these facts.
Authentication uses dedicated commands, not scalar auth edits.

```sh
tmax provider update opencode-go --set subscription.charge.amount 20.00 --set subscription.charge.currency USD
tmax provider update opencode-go --clear subscription.charge
tmax provider add custom-gateway --integration custom --endpoint chat-completions https://gateway.example/openai/v1 --endpoint messages https://gateway.example/anthropic
tmax provider update custom-gateway --endpoint responses https://gateway.example/responses/v1
tmax provider update custom-gateway --clear-endpoint responses
tmax provider add deepseek-known --metadata deepseek --endpoint messages https://api.deepseek.com/anthropic
tmax provider inspect deepseek-known
```

Prices are user-maintained, not live tariff claims. Custom URLs are illustrative
caller-supplied bases, not verified services; `custom` defines finite operations.
The DeepSeek example uses an exact known selector. Its omitted protocol entries
come from that map, unlike omitted custom entries. Update region with
`--set region REGION` or `--clear region`. Help shows actual registry region choices,
singleton defaults, and multi-protocol setup where supported.


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
- `setup`: `singleton_default`, `region_or_known_base`, or `explicit_bases`.
- `regions`: `{id, endpoints: EndpointMap}[]` from the integration's map; empty for
  custom integrations.
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
(null or `{region: string-or-null, endpoints: EndpointMap}`), `resolution`
(null or the object below), and string-or-null `reason_code`. Absent snapshot,
missing provider, and unreadable data remain distinct, not empty resolutions.

Resolution contains `kind` (`known` or `custom`), string-or-null `region`,
`source` (`singleton_default`, `region`, `known_base`, `region_and_known_base`, or
`explicit_bases`), `endpoints: EndpointMap`, and `unavailable_protocols: P[]`.
A resolved base is config evidence, not connectivity/authentication/transport or
continuation qualification.

`provider inspect` returns `data.endpoints = {file, loaded}`, one view per snapshot.
Routine add/update summarizes the endpoint change and saved/loaded outcome; use
inspect for both complete views. Dry-run and endpoint failures return
`data.endpoint_resolution` for the evaluated candidate when available, with
[config impact](../configuration/spec.md#configuration-impact-data). Candidate
bases are not claimed saved, loaded, reachable, or reserved.
