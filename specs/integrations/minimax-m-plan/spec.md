# MiniMax M Plan

## Authority and scope

This contract owns the MiniMax M Plan integration, registered as integration ID
`minimax-m-plan` in this directory `specs/integrations/minimax-m-plan/`. M Plan is
also marketed as Token Plan. It is a subscription sold at platform.minimax.io
whose Subscription Key authorizes inference and one display-only quota read.
It extends the [shared integration contract](../spec.md) and its
[provider-specific index](../spec.md#provider-specific-contracts).
[Product](../../product/spec.md) owns provider identity and authentication
lifecycle. [Model routing](../../model-routing/spec.md) owns selection.
Shared terms are defined in [CONTEXT.md](../../../CONTEXT.md).

| Excluded product or capability | Required boundary |
| --- | --- |
| Pay-as-you-go MiniMax API keys (`sk-api-` prefix) | A different billing product. The official FAQ states the two key classes cannot be used interchangeably. This contract must not present PAYG access as M Plan allowance. |
| Account balance and credit packs | This integration must not query them. Their state remains unknown. |
| Video, speech, and image generation management | Not declared operations of this contract. |
| MiniMax Code client (agent.minimax.io) | A first-party agent, not a tmax agent target. Its sign-in is not a tmax login source and its session must not be imported. |

Each inference request must select one exact model. The Anthropic-compatible
interface documentation names `MiniMax-M3.1-Flash` and `MiniMax-M3`. Catalog
availability and a tier name must not establish entitlement for the selected
Subscription Key.

## Authentication and upstream identity

| Condition | Required behavior |
| --- | --- |
| Credential source | Use the user-supplied Subscription Key from [Plan Details](https://platform.minimax.io/console/plan). Authentication is `api-key` only. |
| Key class | The documented prefix is `sk-cp-`. A prefix can help explain a product mismatch; it must not prove credential validity, account identity, membership status, or model access. |
| Wrong key class | Error `2057` `not token plan token` is the wrong-key-class signal. The user must replace the pay-as-you-go key with a Subscription Key. tmax must not call a balance endpoint to work around it. |
| Refresh and replacement | A Subscription Key has no OAuth refresh owner. A revoked, expired, or replaced key requires explicit key replacement. tmax must not create or rotate upstream keys. |
| Login flows | This contract declares no OAuth, no login flow, and no tmax-owned sign-in. Website sign-in that produces a key is not inference login authentication. |
| Account identity | Account, membership, and renewal identity may remain unknown. A quota read or a successful inference request must not be read as proof of them. |
| Region binding | A key issued by one platform is not valid on the other region's host. Official error `2049` `invalid API Key` is the cross-host evidence. |
| Membership sharing | All tools and keys of one membership share one usage pool. Several keys or several local provider instances must not be treated as independent allowances. |

## Region, endpoints, and native protocols

The region map below is source-qualified under the
[shared endpoint resolver](../spec.md#endpoint-resolution):

| Region ID | `messages` base |
| --- | --- |
| `global` | `https://api.minimax.io/anthropic` |
| `china` | `https://api.minimaxi.com/anthropic` |

The declared HTTP creation operation appends its suffix at the documented
base/path boundary:

| Native protocol | Method | Suffix | Transport |
| --- | --- | --- | --- |
| `messages` | `POST` | `/v1/messages` | HTTP with SSE |

The base already ends in `/anthropic`, and the Anthropic SDK appends
`/v1/messages`. The resulting request URLs are
`https://api.minimax.io/anthropic/v1/messages` for `global` and
`https://api.minimaxi.com/anthropic/v1/messages` for `china`. A target that
expects a base URL must not receive a full request URL that causes the path to
be appended twice.

Region selection must be explicit. The two region hosts are not interchangeable.
A missing, ambiguous, or conflicting region selector must be rejected before
upstream access. The resolver must not infer a region from credentials, catalog
names, locale, or network location. An upstream failure must not cause regional
failover.

This contract declares `messages` as its only inference protocol. The provider
also publishes OpenAI-compatible and other surfaces; a documented provider path
must not widen the declared set. No model-discovery, identity, balance, history,
file, video-generation, speech, or image operation is declared by this contract.
The quota read in the next section is the only declared non-inference operation.

Request parameters remain native under
[the shared parameter contract](../../model-routing/spec.md#request-parameters).
This contract must not inject reasoning settings, compact history, or convert
protocols to make an incompatible request work.

## Agent consumption and native scope

This contract claims no agent-specific native qualification. Provider-documented
client guides for Claude Code, Codex, Cursor, OpenCode, Pi, Hermes Agent, and
OpenClaw are evidence of documented direct setup, not verified tmax adapters,
agent-version ranges, or gateway approval. Direct custom transport requires an
explicit base for each selected protocol under
[`custom` integration](../spec.md#custom-integration), which supplies no M Plan
account, quota, or rule-input acquisition.

The shared
[client and conversation identity contract](../spec.md#client-and-conversation-identity)
applies. The consulted sources establish no mandatory User-Agent literal and no
special session header. tmax must not fabricate a recognized-client string or
impersonate a supported client. This is an evidence limit, not proof that no
runtime restriction exists. Main, tool-result, continuation, and auxiliary calls
must retain the selected region, base, authorization, and truthful native
identity. MiniMax Code sign-in remains excluded; its session must not be
imported to make a route appear available.

## Quota observation

The quota read is identified as source id `minimax_plan_remains_api` under
source contract `minimax_token_plan_remains_2026_10_28`.

| Item | Contract |
| --- | --- |
| Operation | `GET {selected region host}/v1/token_plan/remains` |
| `global` URL | `https://api.minimax.io/v1/token_plan/remains` |
| `china` URL | `https://api.minimaxi.com/v1/token_plan/remains` |
| Authorization | `Authorization: Bearer <Subscription Key>` |
| Success gate | `base_resp.status_code === 0` only |

### Evidence

| Source | Established meaning | Unqualified boundary |
| --- | --- | --- |
| [Official M Plan FAQ](https://platform.minimax.io/docs/m-plan/faq.md), Method 3 | Documents this exact `GET` with `Authorization: Bearer` as an officially recommended usage check. The FAQ's curl example uses the documentation host `www.minimax.io`; this contract uses the selected region API host from the map above. | The FAQ publishes no response schema. The [documentation index](https://platform.minimax.io/docs/llms.txt) was checked and lists no quota-response schema page. |
| [First-party MiniMax CLI](https://github.com/MiniMax-AI/cli/blob/main/src/client/endpoints.ts) | `quotaEndpoint(baseUrl)` returns `${baseUrl}/v1/token_plan/remains`, and `chatEndpoint(baseUrl)` returns `${baseUrl}/anthropic/v1/messages`. Commit `c3a58e9` of 2026-06-08 moved the quota path off the legacy `/v1/api/openplatform/coding_plan/remains`, which the commit records as returning `cookie is missing, log in again` for a line-matched key. | Client implementation evidence, not a published API permission or a schema guarantee. The CLI's internal region table is that client's configuration and does not amend this contract's region map. The legacy path must not be used. |
| Bounded runtime read, 2026-10-27 | One `GET` against the international host with the user's Subscription Key returned HTTP 200 and `status_code: 0`. The `general` bucket reported interval remaining percent 100 with status 1 and zero totals, and weekly remaining percent 100 with status 3 and zero totals. The `video` bucket reported status 3 with zero totals in both windows. | One key, one host, one read. The snapshot is consistent with both a counts-not-provisioned reading and an untouched-plan reading. It must not be read as unlimited, as no-plan, or as exhausted. |

### Response semantics

The envelope is `{model_remains: [...], base_resp: {status_code, status_msg}}`.
HTTP 200 carries business errors, so success is `base_resp.status_code === 0`
only. Per-bucket fields:

| Field | Established meaning | Retention |
| --- | --- | --- |
| `model_name` | A bucket category such as `general` (the shared text and modality pool) or `video`. It is never a catalog model ID. | Retained as a label. |
| `current_interval_remaining_percent`, `current_weekly_remaining_percent` | The authoritative remaining percents per window. | Retained. |
| `current_interval_status`, `current_weekly_status` | An enum of 1/2/3 whose meaning is not settled. | Retained verbatim, never interpreted. |
| `start_time`, `end_time`, `weekly_start_time`, `weekly_end_time`, `remains_time` | Epoch-millisecond window bounds and a countdown value. | Retained verbatim as source values. |
| `current_interval_total_count`, `current_interval_usage_count`, `current_weekly_total_count`, `current_weekly_usage_count` | Count fields with an unpublished unit. MiniMax's own CLI documents the ambiguity: the `*_usage_count` fields mean remaining on the current path and consumed on the legacy path. | Not retained as quota facts. |
| `weekly_boost_permille` | A display multiplier documented only by the first-party CLI. | Not retained. |

Non-video models are subject to a 5-hour window and a 7-day window. Video
models are subject only to the weekly window. Text, image, audio, and video
share one usage pool per membership. Unused usage does not carry over. The
official tiers are Go, Explore, and Build; a tier name must not be converted
into a numeric allowance.

### Required unknowns

| Unknown | Required handling |
| --- | --- |
| Counter unit for the `*_count` fields | Remains unknown. No unit may be invented or inferred from token labels. |
| Status enum meaning for 1/2/3 | Remains unknown. The consulted sources do not settle one reading. |
| `weekly_boost_permille` semantics | Remains unknown outside the first-party CLI's display use. |
| Per-model split of the shared pool | Remains unknown. A bucket label is not a per-model allowance. |
| Renewal, expiry, and credit-pack state | Remains unknown and is not queried. |
| Freshness, TTL, and provider measurement time | Remains unknown. Local retrieval time is not provider `asOf`. |

### Display-only posture

- Quota observation is display-only. This integration advertises no `provider`
  source [rule input capability](../spec.md#rule-input-capabilities), and
  declares no `kind`, `scope`, or model coverage for it.
- Only the semantic success gate and the per-window remaining percents are
  retained.
- Ambiguous count fields are not retained.
- Status enums must not be interpreted as unlimited, not-in-plan, or
  entitlement.
- Zero totals together with status 3 must never render as unlimited and must
  never render as zero remaining.
- Failure or drift must degrade to a dated or unknown result. It must never
  become zero quota and must never be treated as authentication revocation.
- Local retrieval time is not provider `asOf`.
- This integration must not call a pay-as-you-go balance endpoint such as
  `account/query_balance`.
- This integration must not use browser sessions or cookies.

## Failures and recovery

| Condition | Observable result and recovery |
| --- | --- |
| Wrong key class (`2057` `not token plan token`) | Report the wrong-key-class signal and the affected provider. Direct the user to replace the pay-as-you-go key with the Subscription Key from Plan Details. Do not call a balance endpoint, do not treat the plan as revoked, and do not substitute another product. |
| Invalid or cross-region key (`2049` `invalid API Key`, `1004` `not authorized`) | Report the affected key and selected region. Require explicit correction of the region selector or the key. Do not attempt regional failover, key rotation, or path probing. |
| HTTP 200 with a business error (`base_resp.status_code` non-zero) | Report `status_code` and `status_msg` verbatim. The only success gate is `status_code === 0`; a 200 response must not be parsed as quota. Do not invent an error code. |
| Quota acquisition unavailable or failed | Keep quota dated or unknown. Never render zero quota and never treat the failure as authentication revocation. Otherwise qualified inference remains unaffected. |
| Schema drift, missing fields, or an unrecognized status value | Fail closed. Retain the last good observation as dated. Do not guess a field meaning or normalize a new enum value. |
| Provider-reported usage limit or rate limit during inference (`2056`, `2062`, `1002`) | Report the actual upstream scope and recovery guidance. An inference-time limit is a separate fact from the quota observation and must not be merged into it. |

## Observable acceptance

- Setup exposes the M Plan product, the two region IDs, the exact `messages`
  base, and the Subscription Key source. A missing or conflicting region
  selector is rejected before upstream access.
- Setup distinguishes a Subscription Key from a pay-as-you-go key without
  treating a key prefix as proof of validity, identity, or entitlement.
- The quota read targets the selected region host path with Bearer
  authorization, and reports `base_resp.status_code` as the only success gate.
- Quota display shows the bucket label, the per-window remaining percents, the
  verbatim status values, and the observation date. Unknown fields stay visibly
  unknown.
- Zero totals with status 3 never renders as unlimited and never renders as
  zero remaining.
- No `provider` source rule input capability is advertised, and no quota value
  changes a priority, a selection, or an admission decision.
- A failed or drifted quota read leaves a dated or unknown result while
  inference continues. No balance endpoint is called and no browser session or
  cookie is used.
- Renewal, expiry, credit packs, and account balance remain unknown and
  unqueried.

## Evidence status and official references

Official MiniMax documentation was consulted on **2026-10-28**. It establishes
the product boundary, the Subscription Key and key-class separation, the two
regional Anthropic bases, the Messages operation, the usage windows, and the
recommended quota endpoint. The consulted pages are
[M Plan FAQ](https://platform.minimax.io/docs/m-plan/faq.md),
[Usage](https://platform.minimax.io/docs/m-plan/usage-rules.md),
[M Plan overview](https://platform.minimax.io/docs/m-plan/intro.md),
[Subscribe to M Plan](https://platform.minimax.io/docs/m-plan/quickstart.md),
[Other tools](https://platform.minimax.io/docs/m-plan/other-tools.md),
[Anthropic SDK](https://platform.minimax.io/docs/api-reference/text-anthropic-api.md),
[Create Message](https://platform.minimax.io/docs/api-reference/text-chat-anthropic.md),
[Error codes](https://platform.minimax.io/docs/api-reference/errorcode.md),
[MiniMax CLI](https://platform.minimax.io/docs/m-plan/minimax-cli.md), and
[Pi](https://platform.minimax.io/docs/m-plan/pi.md).

The [documentation index](https://platform.minimax.io/docs/llms.txt) was checked
for a published quota-response schema and lists none. The response field
semantics above therefore come from the first-party CLI source
[`src/client/endpoints.ts`](https://github.com/MiniMax-AI/cli/blob/main/src/client/endpoints.ts),
its quota response types, its `usage_count` resolution utility, and commit
`c3a58e9` of 2026-06-08. That is client implementation evidence, not a published
API contract, and it grants no stability guarantee.

One bounded runtime read on **2026-10-27** used the international host and the
user's Subscription Key. No inference request was issued. No agent-version
range is set. This read qualifies neither the `china` host, another key, nor any
agent, transport, or gateway route. Qualification records must follow
[the shared support contract](../spec.md#support-contract) and keep provider,
agent, and runtime evidence separate.
