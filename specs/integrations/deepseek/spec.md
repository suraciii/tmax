# Official DeepSeek API

## Authority and scope

This specification owns integration with the official, API-key-authenticated
DeepSeek Platform API. It is an API-billed access product, not a subscription.
DeepSeek website or chat-app login is not inference authentication for this product.
Consumer chat sessions, subscription-login workflows, third-party DeepSeek hosting,
and automatic key creation or rotation are outside this integration.

The [shared integration contract](../spec.md) owns qualification states and agent
boundaries. The [product contract](../../product/spec.md) owns provider lifecycle,
security, billing observations, native configuration, and gateway forwarding. The
[configuration contract](../../configuration/spec.md) owns configuration fields.
[Model selection](../../model-routing/spec.md) owns protocol eligibility, schedules,
and conversation continuity. Shared terms come from [CONTEXT.md](../../../CONTEXT.md).
This specification adds no account resource, credential pool, or configuration schema.

## Authentication and account identity

Setup must offer `api-key` with a key created on the
[DeepSeek Platform](https://platform.deepseek.com/api_keys). It must not offer a
subscription `login` alternative for this service. Website sign-in used to create
the key must not be described as a refreshable inference session.

Credential storage, replacement, and revocation follow the
[shared authentication contract](../../product/spec.md#providers-and-authentication).
Setup identifies the effective key source and native scope; API keys have no
login-session refresh owner.

The [balance response](https://api-docs.deepseek.com/api/get-user-balance) has no
account-owner identifier. A successful query must not establish the named person
or account that owns a key. Local provider IDs, key fingerprints, and matching
balance amounts must not be used to infer account identity.

Key replacement follows
[the product's identity-replacement rule](../../product/spec.md#providers-and-authentication).
An unknown identity remains unknown; a balance query cannot establish it.
Independently selectable keys remain separate providers, not a key pool.

DeepSeek's [account limits](https://api-docs.deepseek.com/quick_start/rate_limit)
apply across keys for one account. More keys, local providers, agents, or protocols
must not be represented as extra balance, independent quota, or multiplied
concurrency. Where shared billing identity is unknown, inspection must state that
the instances may share upstream limits rather than sum their apparent capacity.

Caller-supplied `user_id` is a business-side isolation value, not provider-verified
account identity or an independent allowance. This includes the Anthropic
`metadata.user_id` form. tmax must not use it to identify the key owner, infer a
separate billing scope, or evade account-level limits. Request parameter handling
remains governed by [Model selection](../../model-routing/spec.md#request-parameters).

## Native protocols and access paths

DeepSeek documents three native protocol surfaces. Qualification must identify
the exact model, client version, native configuration scope, transport, and required
features for each surface. A positive result on one surface must not qualify the
others.

The known map and finite HTTP inference operations are:

| Region ID | Native protocol | Official upstream base | Declared operation beneath that base |
| --- | --- | --- | --- |
| `default` | `chat-completions` | `https://api.deepseek.com` | `POST /chat/completions`, with complete required message history |
| `default` | `responses` | `https://api.deepseek.com` | `POST /responses`, with complete required input history and no server-state dependency |
| `default` | `messages` | `https://api.deepseek.com/anthropic` | `POST /v1/messages`, with the supported message, thinking, and tool-result forms |

The [shared endpoint resolver](../spec.md#endpoint-resolution) owns selection of
this published endpoint set. Native configuration uses the selected map entry
for upstream access. The gateway may forward the same declared operation after
its transport and continuation behavior are qualified. Neither path depends on
conversion between Chat Completions, Responses, and Messages. A
catalog `api` URL or generic OpenAI base must not replace the Messages map entry
or establish Responses continuation support.

DeepSeek provides an official
[Codex integration guide](https://api-docs.deepseek.com/quick_start/agent_integrations/codex)
using Responses and a native model catalog. This is a documented direct-use
scenario. tmax must not label all Codex-to-DeepSeek use incompatible merely
because DeepSeek does not store Responses conversations. The actual Codex version,
model metadata, tool forms, and history behavior still require qualification.
The guide's shared Codex configuration scope must be included in the affected-target
view; it must not be presented as isolated per project or per running session.
Secret handling follows the
[product authentication contract](../../product/spec.md#providers-and-authentication),
not the guide's setup script or plaintext-key example.

Stock pi, omp, OpenCode, Hermes Agent, and DSH must select one of these native surfaces
under the [shared agent and qualification boundaries](../spec.md#agent-boundaries).

The [Anthropic compatibility table](https://api-docs.deepseek.com/guides/anthropic_api)
is not a promise of full Anthropic feature parity. Required content blocks,
reasoning behavior, tools, and ignored request controls must be checked against
that table. tmax must not infer Claude entitlement from a name that upstream accepts
and maps to a DeepSeek model. The configured model must retain its explicit
DeepSeek model identity and billing provenance.

Every declared inference operation requires the selected provider key and exact
qualified model under the shared native-scope rules. The separate non-inference
[model discovery](#model-discovery) and [balance](#balance-usage-and-renewal)
operations use their declared destinations, not URLs derived from the selected
inference base. This contract does not
declare beta completion, file management, or ID-only response retrieval,
deletion, or cancellation forwarding. A returned ID or upstream documentation
for another API does not qualify those operations under
[the shared operation contract](../spec.md#supported-operations-and-resource-identity).

The [first-call guide](https://api-docs.deepseek.com/) documents bearer API-key
authentication for OpenAI-compatible calls. The
[Anthropic header table](https://api-docs.deepseek.com/guides/anthropic_api#http-header)
supports `x-api-key` for Messages. The consulted official API sources establish
no recognized-client User-Agent requirement or special conversation header.
Those requirements must not be invented from another access product.
Truthful native identity and genuine session semantics still follow the
[shared client-identity contract](../spec.md#client-and-conversation-identity)
for direct/gateway main, tool-result, continuation, and auxiliary requests.

The current first-call guide names `deepseek-flash` and `deepseek-v4-pro`.
It notes that historical Flash IDs can resolve to a newer served model. A
configured ID must remain exact; accepted aliases and catalog metadata do not
prove an unchanged model, protocol capability, or account entitlement.

## Conversation and native scope

The [Responses contract](https://api-docs.deepseek.com/guides/responses_api)
does not support `previous_response_id`, `conversation`, `store`, or `background`.
Unsupported parameters can be silently ignored. The documented response object's
`store` is false; a returned response ID is not evidence of retrievable history.

A client that sends complete required input history can qualify for direct or
same-protocol gateway use. This must include the prior tool calls and tool outputs
needed for the next turn. A client that relies on server-stored history or
background execution must be blocked for those semantics, even if its first
request succeeds. tmax must not reconstruct missing history, strip unsupported
fields to claim success, invent a conversation service, or translate protocols
to make that client appear compatible.

Qualification must cover streamed output, tool-call round trips, reasoning data
required on later turns, and the actual client's continuation behavior. DeepSeek's
Responses SSE stream uses semantic events and terminal response events rather
than a Chat Completions `[DONE]` sentinel. Its documented request keep-alives must
not be mistaken for generated content or completion. Transport handling follows
the [shared deadline contract](../../product/spec.md#transport-deadlines).
Unknown required transport or tool behavior must remain unavailable for that
configuration until qualified.

Statelessness does not relax
[selection and continuity](../../model-routing/spec.md#selection-and-continuity).
Multiple agents may use one provider only where their effective key and native
scope resolve to the selected authorization. A native credential source that
retains another provider's key must block application until its precedence is
resolved. A native scope that cannot isolate independently selected providers
must be reported as shared or blocked, not advertised as per-session isolation.

## Model discovery

A `models` refresh uses the selected provider's bearer API key for
`GET https://api.deepseek.com/models`. This is a separate non-inference
destination. Auth and quota refreshes continue to use the official balance
operation; discovery does not replace either balance observation.

Discovery retains dated, exact model IDs from the response's `data` array.
Responses are bounded to 4 MiB, 10,000 records, and nonempty IDs of at most
512 UTF-8 bytes without control characters. Malformed or excessive responses
fail rather than publish a partial list. Failed refreshes retain the last good
observation separately from the latest attempt under the shared observation
lifecycle.

Discovery does not register models, mutate the catalog or desired
configuration, establish account identity or balance, or qualify inference,
native protocols, agents, or conversation behavior. Authorization replacement
invalidates observations bound to the previous authorization.

## Balance, usage, and renewal

The supported balance source is authenticated
`GET https://api.deepseek.com/user/balance`. Inspection must retain the returned
`balance_infos` entries by `currency`, including `total_balance`, `granted_balance`,
and `topped_up_balance`. `total_balance` includes the other two components;
inspection must not add all three into a larger total. `granted_balance` is the
unexpired granted balance. The response does not provide its expiry calendar.
Different currencies must remain separate without an invented conversion rate.

`is_available` means that the user's balance is sufficient for API calls at the
time of the observation. It does not verify a selected model, agent protocol,
transport, current concurrency availability, account owner, or successful
conversation. Balance success must appear separately from model verification.

This balance is not a recurring allowance and does not establish a billing
period, quota reset, subscription renewal, account-wide usage history, or remaining
request count. The
[shared billing contract](../../product/spec.md#billing-quota-and-renewal-information)
governs observation identity, freshness, and failed refreshes. The
[Platform dashboard](https://platform.deepseek.com/) is the source for billing
information the API does not provide.

The documented balance response supplies no provider measurement timestamp,
validity interval, or freshness guarantee. Local retrieval time must not become
provider `asOf` or an invented TTL. Permission to read this balance must not be
treated as permission for every public DeepSeek API operation.

An authoritative `is_available: false` observation is balance-insufficiency
evidence only for its observed authorization and scope. Automatic exclusion must
meet [the shared evidence requirements](../../model-routing/spec.md#selection-and-continuity).
An unqualified freshness or identity mapping must not become permanent exclusion.
After a user tops up, a fresh successful balance query can establish current
balance availability without requiring another provider. It still cannot mark a
model conversation verified.

## Pricing calendar

[DeepSeek pricing](https://api-docs.deepseek.com/quick_start/pricing) documents
peak and off-peak token prices with weekday windows, weekends, and Chinese
public-holiday exceptions. The tariff, time zone, calendar, model-specific prices,
and effective dates are vendor data, not product constants. tmax must not hard-code
the consulted prices or treat a reference catalog cost as the full tariff.

A user may express a deliberate pricing-based preference through the existing
[time rules](../../model-routing/spec.md#time-rules), including a saved time zone
and explicit date overrides. Pricing refreshes must not create holiday rules,
change priorities or alias candidates, rewrite schedules, or retarget a conversation.
Unknown or stale tariff data must remain labeled as such; it must not prevent
otherwise qualified inference or trigger an automatic cheapest-model decision.

## Failures and recovery

Key failures and replacement follow the
[shared authentication contract](../../product/spec.md#providers-and-authentication);
recovery uses a Platform API key, not a subscription login.

Balance insufficiency links to the Platform for user-controlled recovery.
DeepSeek documents account-level concurrency rejection with HTTP 429; it must not
be labeled depleted balance or assigned an invented quota reset. Another key for
the same account does not increase capacity.

A required unsupported Responses state field, Anthropic feature, or client
transport blocks the affected route under the
[shared qualification contract](../spec.md#qualification-before-advertising-support).
An upstream success that silently ignores required semantics is not qualification
success.

## Acceptance

- Setup offers Platform API-key access without a subscription-login choice or a
  fabricated subscription record. Inspection identifies the effective credential
  source and native scope without claiming a refreshable session.
- Model discovery retains exact IDs without registering models or changing
  catalog metadata. Malformed/excessive responses fail without replacing the
  last good observation. Auth/quota remain separate balance observations;
  discovery does not claim model inference, conversation, or account verification.
- A successful balance query displays separate currency entries and granted,
  topped-up, and total balances without double counting. It shows the observation
  source and time, keeps owner identity unverified, and does not verify a model.
- Key replacement with unknown account identity needs no extra declaration.
  It invalidates prior credential observations and does not silently retain
  conversation eligibility. Known different account access remains separate.
- Two keys for one account do not increase displayed balance, quota, or concurrency.
  A supplied `user_id` does not establish account ownership or independent capacity.
- The official Codex Responses scenario appears as documented and conditional on
  actual version and feature qualification, not categorically incompatible or
  tmax-verified. Its shared native scope is visible before application.
- Direct and gateway selections expose the same resolved region/protocol map,
  exact upstream base, and finite declared method/path operations. Chat
  Completions, Responses, and Anthropic Messages qualify independently; no
  cross-protocol translation or undeclared resource-operation passthrough is offered.
- Full-history Responses continuation can qualify, including tool round trips.
  A client requiring `previous_response_id`, `conversation`, `store`, or
  `background` cannot pass on a successful first turn or ignored fields.
- A balance refresh failure leaves stale or unavailable observations without
  disabling otherwise usable inference. Balance recovery does not claim model
  verification. Concurrency rejection is not misreported as exhausted credit.
- Pricing evidence preserves weekday and holiday distinctions without embedding
  numeric tariffs or automatically changing routing, schedules, or retained sessions.

## Official evidence and status

Official documentation was consulted on 2026-10-23. These sources establish
upstream documentation evidence. On 2026-10-24, provider-authorized non-inference
model discovery was exercised through the real tmax CLI in an isolated,
passwd-backed OS account: the official models refresh completed, retained exact
IDs, and left registered models and catalog metadata unchanged. This bounded
check used no paid inference and establishes no native-agent version range,
protocol transport, tool, or conversation qualification. Qualification labels
must follow [the shared support contract](../spec.md#support-contract).

- [First API call](https://api-docs.deepseek.com/) and
  [Platform key management](https://platform.deepseek.com/api_keys): official
  API-key access, native bases, and Chat Completions setup.
- [Codex integration](https://api-docs.deepseek.com/quick_start/agent_integrations/codex):
  official Responses setup, native model metadata, and shared configuration scope.
- [Responses compatibility](https://api-docs.deepseek.com/guides/responses_api):
  supported request and stream forms, tool limitations, and unsupported server state.
- [Anthropic compatibility](https://api-docs.deepseek.com/guides/anthropic_api):
  Anthropic base, key authentication, model mapping, and feature limitations.
- [Balance API](https://api-docs.deepseek.com/api/get-user-balance): availability
  and currency-denominated balance fields without an account-owner identifier.
- [Rate limits and isolation](https://api-docs.deepseek.com/quick_start/rate_limit):
  account-wide concurrency, key sharing of limits, caller-side `user_id`, and keep-alives.
- [Models and pricing](https://api-docs.deepseek.com/quick_start/pricing):
  token pricing calendar and vendor-adjustable tariffs.
