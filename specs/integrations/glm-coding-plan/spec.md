# Domestic GLM Coding Plan

## Authority and scope

This contract owns domestic GLM Coding Plan access issued through
`bigmodel.cn` for `open.bigmodel.cn`. It does not cover international Z.ai,
standard API billing, or arbitrary HTTP applications. The
[shared integration contract](../spec.md) owns support qualification,
[client identity](../spec.md#client-and-conversation-identity), and finite
operations. [Product authentication](../../product/spec.md#providers-and-authentication)
and [selection and continuity](../../model-routing/spec.md#selection-and-continuity)
remain authoritative.

This directory is not a runtime integration ID. No dedicated GLM integration is
registered. Explicit [`custom`](../spec.md#custom-integration) transport can carry
an independently qualified request, but supplies no GLM account discovery,
quota refresh, or provider-source rule input.

## Credentials and entitlement

The [API-key classification notice](https://docs.bigmodel.cn/cn/coding-plan/notice/api-key-management)
distinguishes these products as of 2026-10-16:

| Credential category | Required boundary |
| --- | --- |
| Personal Coding Plan key | Select the personal plan key and its supported plan route. |
| Team Coding Plan key | Select the member's team plan key and applicable seat/overage policy. Personal quota-plugin support does not qualify team observations. |
| Standard API key | This is API-billed access, even if its owner has a subscription or uses a compatible plan host/path. It must not be presented as Coding Plan allowance. |
| Historical compatibility key | Billing depends on the documented legacy classification, endpoint, and account history. A URL or key's appearance must not establish its billing product. |

Setup must identify the selected access product and key source without reading
billing identity from the key format. Website sign-in to obtain a key is not
inference login authentication. API-key access has no OAuth refresh owner.
Revocation recovery requires explicit key replacement through the same selected
product. tmax must not import native tokens, create upstream keys, or substitute
a standard API key or international host.

Account, organization, project, and team-seat identity may remain unknown.
Successful inference or a quota read does not prove them. Independently selected
accounts remain separate providers under the shared authentication contract.
Multiple keys or providers must not be treated as independent plan allowances.

## Domestic endpoints and operations

The [supported-tools endpoint table](https://docs.bigmodel.cn/cn/coding-plan/tool/others)
publishes one domestic endpoint set. These are source-qualified product
requirements, not a new registry region selector. A future known integration
must use one finite domestic map; current custom setup requires each explicit
protocol base under the [endpoint resolver](../spec.md#endpoint-resolution).
Direct and proxy routes must select the same product/protocol destination.

| Protocol | Published domestic base | Finite operation and authentication evidence |
| --- | --- | --- |
| `chat-completions` | `https://open.bigmodel.cn/api/coding/paas/v4` | `POST /chat/completions`, HTTP/SSE; selected plan key in `Authorization: Bearer`. The plan base and [Chat Completions reference](https://docs.bigmodel.cn/api-reference/模型-api/对话补全) jointly establish this path. |
| `messages` | `https://open.bigmodel.cn/api/anthropic` | `POST /v1/messages`, HTTP/SSE; selected plan key through the native qualified auth convention. [Claude-compatible examples](https://docs.bigmodel.cn/cn/guide/develop/claude/introduction) use `x-api-key`; the [Coding Plan Claude guide](https://docs.bigmodel.cn/cn/coding-plan/tool/claude) uses `ANTHROPIC_AUTH_TOKEN`. These conventions are not monitor authorization. |
| `responses` | `https://open.bigmodel.cn/api/v1` | `POST /responses`, HTTP/SSE; selected plan key in `Authorization: Bearer`. [Codex plan setup](https://docs.bigmodel.cn/cn/coding-plan/tool/codex) establishes this product/base; the [Responses creation reference](https://docs.bigmodel.cn/api-reference/response/创建-response.md) establishes the exact operation and auth. Client/transport qualification remains separate. |

Each inference request requires an explicitly configured model. This contract
adds no model-discovery, identity, history, file, ID-only resource, WebSocket,
or background operation. In particular, the monitor origin must not be derived
by modifying an inference URL. The separate quota evidence below does not grant
permission for all monitor or public API endpoints.

The [Responses compatibility guide](https://docs.bigmodel.cn/cn/guide/develop/responses/introduction.md)
documents default `store=false`, stateful continuation through `store=true` and
`previous_response_id`, and terminal SSE events without a `[DONE]` sentinel.
A returned ID alone must not enable cross-provider continuation or resource
access. Any qualified server-state continuation must retain the original
provider/authorization binding under the shared continuity contract. Public
retrieval/deletion documentation does not add those operations to this contract.

The [current model guide](https://docs.bigmodel.cn/cn/coding-plan/latest-model)
identifies `glm-5.3` and `glm-5.3-flash`. The
[overview](https://docs.bigmodel.cn/cn/coding-plan/overview) says older model names
can be rerouted to these models. A configured historical alias is not evidence
of an unchanged model. The Claude-specific `[1m]` context suffix must not become
a new API model offering. Models, context, reasoning, tools, and input/output
capabilities require qualification for the selected client and protocol.

## Approved tools and native identity

| Condition | Required behavior |
| --- | --- |
| Provider-supported client | Use a tool/environment on the [official list](https://docs.bigmodel.cn/cn/coding-plan/tool/others). It includes Pi Coding Agent, OpenCode, and Codex; Hermes is listed as a general agent with different scheduling priority. A generic HTTP SDK is not by itself plan permission. |
| Pi native setup | The [Pi guide](https://docs.bigmodel.cn/cn/coding-plan/tool/pi) documents `zai-coding-cn` and a personal/team API key. A native command named `/login` does not make this OAuth. Resolve the effective agent directory and credential precedence. |
| Exact User-Agent or session header | The consulted GLM plan sources establish no mandatory literal User-Agent or special conversation header. Do not import Go's header requirement or invent a recognized-client string. Preserve truthful native identity under the shared identity contract. |
| Direct versus proxy | Qualify each actual client version, native scope, protocol, and transport separately. Official tool eligibility does not certify every custom provider or local relay. |
| Main, tool, continuation, and auxiliary calls | Keep the selected plan destination, authorization, real client identity, and genuine native conversation/parent-subsession semantics. A static provider-wide session cannot establish routing continuity. |

A local relay must not conceal an unsupported client, bypass the supported-tools
restriction, change billing product, or claim provider endorsement from one
successful request. Missing required capabilities or unqualified native scope
must leave the affected route unavailable with the required client upgrade,
configuration correction, or missing qualification identified.

## Allowance and monitor evidence

| Source | Established meaning | Unqualified boundary |
| --- | --- | --- |
| [Personal plan overview](https://docs.bigmodel.cn/cn/coding-plan/overview) | Allowance is measured in points, with five-hour dynamic replenishment after consumption and a seven-day cycle from the plan order. Renewal and quota reset are distinct. | Published tier totals do not establish this provider's current allowance or decode monitor fields. Team allowance and overage follow the [team product](https://docs.bigmodel.cn/cn/coding-plan/team), not personal assumptions. |
| [Official usage plugin documentation](https://docs.bigmodel.cn/cn/coding-plan/extension/usage-query-plugin) | The plugin queries quota and usage; documented support is personal plans only. | It publishes no versioned quota schema, endpoint-level read scope, or team compatibility guarantee. |
| [Vendor plugin source](https://github.com/zai-org/zai-coding-plugins/blob/main/plugins/glm-plan-usage/skills/usage-query-skill/scripts/query-usage.mjs) | Domestic unwindowed `GET https://open.bigmodel.cn/api/monitor/usage/quota/limit`; supplied key verbatim in `Authorization`, without adding `Bearer `. The source also sends `Accept-Language: en-US,en` and `Content-Type: application/json`. | This is vendor-client implementation evidence, not a general published API permission or stable tmax observation capability. It authorizes no inferred history/billing read. |
| Raw monitor fields | Preserve source labels such as `type`, `unit`, `number`, `percentage`, `usage`, `currentValue`, `remaining`, and `nextResetTime` only with their qualification limits. | Numeric unit enums, counter units, percentage precision, and reset encoding are not established by the consulted sources. Do not identify unit `6` as weekly, convert a numeric reset into a contractual instant, or infer points from token labels. |
| Monitor timing | Local retrieval time can date an observation. | No provider measurement timestamp, validity interval, TTL, or freshness guarantee is qualified. Retrieval time is not provider `asOf`. |

The plugin labels every `TOKENS_LIMIT` row as five-hour token usage while ignoring
`unit` and `number`. That label must not collapse distinct returned rows into one
window or supply missing API semantics. Plan-point documentation must remain
separate from monitor-field interpretation. Unknown units, identity, coverage,
reset semantics, and freshness cannot supply normalized quota rule inputs or an
automatic routing veto. [Rule input qualification](../spec.md#rule-input-capabilities)
remains required.

The plan overview documents off-peak point discounts and a weekday UTC+8 peak
window. These are sourced subscription-consumption rules, not standard monetary
API prices or account-specific quota observations. User time-based priorities
follow the [time-rule contract](../../model-routing/spec.md#time-rules).

## Failures and recovery

| Condition | Observable result and recovery |
| --- | --- |
| Missing/revoked key, wrong key category, or incompatible native credential precedence | Identify the affected product, key source, and native scope. Require explicit correction or replacement; do not infer identity or switch billing products. |
| Wrong domestic protocol base, unknown operation, or unqualified native transport/continuation | Reject the affected route and show the exact required base/path or missing qualification. Do not probe alternate paths or international endpoints. |
| Unsupported tool, lost native identity, or denied model/capability | Report the actual restriction and selected model/client. Require a supported tool/configuration or provider resolution; do not forge identity, shrink requirements, or substitute a model. |
| Exhausted personal plan allowance | Preserve the provider's actual window/reset evidence and link to [personal usage](https://www.bigmodel.cn/coding-plan/personal/usage). No automatic balance/resource-package fallback is permitted. Team overage settings do not grant tmax permission to change billing. |
| Unknown or failed quota source | Show unavailable or dated source-qualified information, not zero quota. Keep otherwise qualified inference independent of observation failure. Do not scrape native/browser sessions or decode undocumented units. |

## Observable acceptance

- Setup distinguishes domestic plan access, standard billing, and legacy-key
  uncertainty without key-shape inference or a fabricated login flow.
- Direct and proxy inspection expose the exact protocol base, model, effective
  key source, client version/scope, and separate qualification result.
- A qualified conversation retains destination and truthful native identity
  through streaming, a tool-result round trip, continuation, and applicable
  auxiliary requests. Unsupported combinations fail closed without substitution.
- Personal plan points, team policy, raw monitor fields, and local token usage
  remain distinct. Unknown monitor units/resets/freshness remain visible.
- A successful custom inference route cannot unlock GLM discovery, quota refresh,
  provider-source inputs, other models/protocols, or international endpoints.

## Evidence and implementation boundary

Official documentation and vendor plugin source were consulted on 2026-10-23.
Official tool/product documentation, native-client implementation evidence, and
actual runtime evidence have different authority under the shared support contract.

One bounded runtime qualification used pi **1.2.0**, its documented
`before_provider_request` generation-budget hook, and unchanged tmax custom
Chat Completions transport for domestic **`glm-5.3-flash`**. Streaming, one tool
round trip, and one continuation succeeded. The hook bounded the existing request
budget; it did not change headers or native identity. This does not qualify stock
native defaults, a built-in GLM integration, all models/protocols, or quota refresh.
One authorized quota GET established only that supplied key's read succeeded;
its account-specific values are not this contract and supply no schema guarantee.
