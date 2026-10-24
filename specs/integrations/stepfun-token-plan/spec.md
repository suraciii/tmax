# StepFun Step Plan Token Plan

## Authority and scope

This contract owns domestic StepFun **Step Plan Token Plan** access from
`platform.stepfun.com`, limited to the explicit model **`step-5-preview`**.
Other Step models, routers, audio products, international access, the historical
request-count Coding Plan, and standard pay-as-you-go API billing are outside
this contract. Public model availability does not add configured candidates.

The [shared integration contract](../spec.md) owns qualification, finite
operations, and [client identity](../spec.md#client-and-conversation-identity).
[Product authentication](../../product/spec.md#providers-and-authentication),
[endpoint resolution](../spec.md#endpoint-resolution), and
[selection and continuity](../../model-routing/spec.md#selection-and-continuity)
remain authoritative. This directory creates no runtime integration ID.
A dedicated StepFun integration is not registered. Explicit
[`custom`](../spec.md#custom-integration) transport supplies no StepFun account,
subscription, quota, or rule-input acquisition capability.

## Authentication and product selection

| Condition | Required behavior |
| --- | --- |
| Credential source | Use the user-supplied API key from the [StepFun platform key page](https://platform.stepfun.com/interface-key). Website sign-in to obtain it is not inference login authentication. |
| Entitlement | An API key authenticates access; an effective Token Plan subscription, available allowance, and selected-model permission remain separate prerequisites under the [official Pi guide](https://platform.stepfun.com/docs/zh/guides/developer/integrations/pi.md). Key syntax or a catalog entry must not prove them. |
| Refresh and replacement | API keys have no OAuth refresh owner. A revoked/missing key requires explicit replacement. tmax must not import Step Code login tokens or create upstream keys. |
| Account identity | Account/project identity may remain unknown without a qualified source. A local provider ID, key fingerprint, or successful model request is not verified ownership. Independently selectable authorizations remain separate providers. |
| Selected billing product | Use the dedicated Step Plan route. A common API-key console does not make PAYG and Token Plan interchangeable or establish a plan-only key format. |

The [overview](https://platform.stepfun.com/docs/zh/step-plan/overview.md)
distinguishes monthly Token Plan Credit from the discontinued-new-sales
request-count Coding Plan. It also separates plan-path consumption from standard
API billing. Neither authentication failure, exhaustion, nor a transport error
authorizes fallback to the standard `/v1` API, another account, or another model.

## Domestic endpoints and finite operations

The [reasoning integration guide](https://platform.stepfun.com/docs/zh/step-plan/integrations/reasoning-api.md)
establishes one domestic endpoint set. These source-qualified requirements are
not an implemented known-integration region selector. Current custom setup must
supply the exact base for each selected protocol. Direct and gateway routes must
use the same product/protocol destination.

| Protocol | Exact base | Operation beneath base | Authentication and transport |
| --- | --- | --- | --- |
| `chat-completions` | `https://api.stepfun.com/step_plan/v1` | `POST /chat/completions` | Selected API key in `Authorization: Bearer`; HTTP with SSE for streaming. |
| `messages` | `https://api.stepfun.com/step_plan` | `POST /v1/messages` | Official Anthropic SDK `api_key` setup uses native key authentication; the [Messages reference](https://platform.stepfun.com/docs/zh/api-reference/chat/messages-create.md) also shows Bearer examples. Qualify the actual native convention; HTTP with SSE for streaming. |

Messages must use the base without an extra `/v1`; the native SDK appends
`/v1/messages`. Chat Completions must retain its `/step_plan/v1` base. The
[OpenCode guide](https://platform.stepfun.com/docs/zh/guides/developer/integrations/open-code.md)
warns that its built-in default `/v1` route is PAYG and needs a plan-specific
override. A provider name must not hide that destination difference.

The [finite query-selector contract](../spec.md#supported-operations-and-resource-identity)
applies to Messages creation: no query or the sole exact `beta=true` selector,
preserved unchanged, not arbitrary query passthrough. Pi **1.2.0** uses
`client.beta.messages.create`; Anthropic TypeScript SDK **0.129.0**
[source](https://github.com/anthropics/anthropic-sdk-typescript/blob/sdk-v0.129.0/src/resources/beta/messages/messages.ts)
uses `POST /v1/messages?beta=true`. That selector does not qualify every upstream
beta feature. The actual Step route and native beta headers/features require
separate provider runtime qualification.

Each request must select `step-5-preview`. No Responses, model-discovery,
identity, quota, balance, history, ID-only resource, file, batch, background,
WebSocket, or audio operation is declared by this contract. A public API path,
response ID, or permission for one operation must not widen this set.

The [model guide](https://platform.stepfun.com/docs/zh/guides/models/step-5-preview.md)
documents a 1,000,000-token context, up to 64k output tokens, SSE, and function
tools. Actual native capabilities and plan permission still require
qualification. A model output limit is not a per-request generation budget.
Messages requires positive `max_tokens`; request parameters remain native under
[the shared parameter contract](../../model-routing/spec.md#request-parameters).

The Messages reference documents reasoning effort through
`output_config.effort` with `low`, `medium`, or `high`. A reasoning-capable model
does not prove that a native client's `thinking` form, disabled mode, or budget
mapping has those semantics. Qualify the actual native controls without silently
rewriting them or changing model capability metadata to force compatibility.

## Client identity and native scope

| Source or condition | Required boundary |
| --- | --- |
| [Official Pi setup](https://platform.stepfun.com/docs/zh/guides/developer/integrations/pi.md) | Documents pi **0.87.1**, a custom Messages provider, active agent-directory configuration, and effective native key precedence. That version evidence does not certify pi 1.2.0 or every gateway. |
| [OpenCode setup](https://platform.stepfun.com/docs/zh/guides/developer/integrations/open-code.md) and [Hermes setup](https://platform.stepfun.com/docs/zh/guides/developer/integrations/hermes-agent.md) | Establish provider-documented client scenarios and plan routing, not verified tmax native adapters. Resolve the actual version, active profile/server scope, and authorization. |
| User-Agent and native session headers | The consulted official plan/API/client sources specify no required recognized User-Agent literal or special conversation header. This is an evidence limit, not proof that runtime restrictions cannot exist. Preserve the real qualified native identity; do not impersonate a supported tool. |
| Main, tool-result, continuation, and auxiliary requests | Retain the selected plan authorization and endpoint, truthful native client identity, and genuine native conversation/parent-subsession semantics. Do not substitute a static provider-wide ID. |
| Direct versus local gateway | Qualify separately. A working native configuration does not certify a relay, and concrete full-history tool turns do not establish dynamic-alias session identity. |

Native field paths and authentication precedence follow the
[versioned Agent references](../../../docs/agents/README.md#qualification-and-evidence).
Those external schemas do not establish Step plan entitlement.

The [Step Plan agreement](https://platform.stepfun.com/docs/zh/agreement/step-plan.md),
effective 2026-05-25, limits use to the subscribing account and supported or
explicitly documented integrations. It prohibits sharing/resale and bypassing
technical limits, including misuse of proxies or token rotation. A local
single-user route must not erase those restrictions or advertise provider
approval solely because a request succeeds. Unqualified identity, scope, model,
or transport combinations must remain unavailable with the missing prerequisite
shown; changing a User-Agent is not recovery.

## Credit and quota evidence

| Source | Established meaning | Required unknowns and limits |
| --- | --- | --- |
| [Token Plan pricing](https://platform.stepfun.com/docs/zh/step-plan/pricing.md) | Credit is allocated monthly, including quarterly/yearly prepayment. Unused period Credit expires without rollover. Additional Credit has separate validity and earliest-expiry consumption order. | Credit is not raw model tokens, remaining request count, PAYG balance, or a measured account snapshot. Public tier totals do not establish this provider's plan. |
| [Pricing and adjustment announcement](https://platform.stepfun.com/docs/zh/announcements/step-plan-usage-adjustment.md) | Step 5 Preview has time-dependent Credit consumption; qualifying historical subscriptions can retain earlier terms. | The account's applicable terms, exact reset instant, renewal date, and completed replenishment are not inferred from a public schedule. |
| [Official Pi guide](https://platform.stepfun.com/docs/zh/guides/developer/integrations/pi.md) | Directs the user to platform usage details to reconcile model and billing source. | A website dashboard is not an API-key-authenticated query contract or permission to use browser cookies. |
| API-key current plan-quota read | No exact read-only Token Plan URL/method/auth/permission/ownership contract is qualified in the consulted official sources. | This is not a claim that no endpoint exists. Do not guess quota suffixes, call a generic balance/admin API, scrape website sessions, or use an unofficial collector to fill the gap. |
| Units, reset, and freshness | Published Credit rules and locally observed inference tokens are separate evidence. | Account-wide usage, remaining Credit, source measurement time, precision, freshness/TTL, per-key coverage, and reset/exhaustion state remain unknown without a qualified observation source. |

Quota inspection must show unavailable or source-qualified unknown data with the
[platform dashboard](https://platform.stepfun.com) link. Unknown quota must not
become zero, exhaustion, a fabricated reset, or a routing veto. A local retrieval
time is not provider `asOf`. Successful inference does not populate plan or
account observations. No provider-source quota rule input is qualified under
[the shared input boundary](../spec.md#rule-input-capabilities).

## Failures and recovery

| Condition | Observable result and recovery |
| --- | --- |
| Missing, revoked, or incorrectly selected key | Identify the effective key source/native scope. Require explicit same-product correction or replacement; no login-token import or ambient credential fallback. |
| Incorrect plan path, duplicate `/v1`, or undeclared method/query | Reject before dispatch where locally detectable. Show the exact selected protocol/base and allowed operation. Unknown, duplicate, or conflicting query selectors remain rejected. |
| Missing subscription, denied `step-5-preview`, unsupported client/feature, or provider restriction | Preserve the actual upstream distinction when available. Require plan/model permission or a supported client/configuration; do not infer a revoked key, substitute a model, or impersonate another client. |
| Provider-reported exhaustion, concurrency limit, or transient capacity/network failure | Report the actual scope and available evidence. Do not invent a reset, try another key, replay silently, or change to PAYG. User-controlled recovery remains in the platform. |
| Quota acquisition unavailable or failed | Keep quota unknown or dated under its qualified source. Keep otherwise qualified inference independent from that information gap. |
| Native scope cannot isolate the selected authorization | Show the shared scope and affected targets. Block the incompatible selection rather than promise per-project account isolation. |

## Observable acceptance

- Setup exposes the Token Plan product, exact domestic plan destination,
  `step-5-preview`, key source, and separate entitlement/identity unknowns.
- Direct and gateway paths report their actual native version, protocol, scope,
  authorization ownership, and independent qualification status.
- Messages creation retains the sole qualified selector when present and rejects
  undeclared selectors without dispatch. The accepted selector grants no new
  operation or beta-feature entitlement.
- Qualified streaming, tool-result, continuation, and auxiliary paths preserve
  selected plan access and truthful native identity without model/PAYG fallback.
- Missing current plan-quota API qualification remains visibly unavailable with
  a dashboard action. Inference usage cannot masquerade as remaining Credit or
  enable an unsupported quota rule input.

## Evidence and implementation boundary

The linked official documentation was consulted on 2026-10-23. Official plan
rules, provider-documented native examples, versioned SDK source, and tmax
runtime evidence remain distinct under the shared support contract.

Runtime qualification must name the exact model, client version, native
reasoning controls, operation selector, transport, and direct/gateway path. A
local pre-dispatch rejection is not a provider or key rejection. A bounded
generation hook does not qualify stock native defaults or change model capacity.
Neither native-schema readiness nor a single inference pass establishes a
built-in StepFun integration, account ownership, or plan-quota support.

On 2026-10-23, one bounded runtime qualification used pi **1.2.0**, Anthropic
SDK **0.129.0**, tmax custom Messages forwarding with `beta=true`, and exact
`step-5-preview`. Streaming, one tool-result round trip, and one continuation
succeeded. Effective catalog metadata reached the native model without
capability overrides. A documented request-budget hook changed only existing
`max_tokens` to 512; stock-default generation budgets remain unqualified.
Native `thinking.type=disabled` was forwarded unchanged and accepted, but this
does not prove that upstream reasoning was disabled or that other effort
controls work. No other model/protocol, full capability suite, plan-quota read,
or account-wide billing fact is qualified by that result.
