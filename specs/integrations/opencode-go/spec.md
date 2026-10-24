# OpenCode Go integration

## Authority and scope

This specification owns OpenCode Console **Go** and **Go Plus** subscription
access. A configured provider represents access to one subscribed Console
workspace. It does not represent the OpenCode agent or every service sold through
Console.

[Shared integration qualification and agent boundaries](../spec.md) apply.
[Product behavior](../../product/spec.md),
[configuration](../../configuration/spec.md), and
[model selection and continuity](../../model-routing/spec.md) remain authoritative
for their shared rules. Terms follow [CONTEXT.md](../../../CONTEXT.md). This
contract adds no Account resource, credential pool, configuration language, or
plugin requirement.

Console pay-as-you-go access is a different access product, not a fallback for Go.
Subscription and workspace administration remain in the provider's Console.

## Authentication and workspace identity

Setup must accept a user-supplied Console key bound to the workspace that holds
the Go subscription. Inference uses `Authorization: Bearer`; no additional
workspace-selection header is required.

Signing into Console to create a key is not inference login authentication.
OpenCode `/connect`, pi `/login`, or another native command that saves this key
must remain API-key setup in tmax. tmax must not offer Go OAuth or extract Console
browser sessions.

Go and pay-as-you-go use Console keys. tmax must treat their values as opaque.
It must not require a distinct Go key format, infer a subscription from a prefix,
or select the billing product from the key alone. The selected integration,
workspace entitlement, and endpoint determine the access being configured.
A successful model listing does not prove entitlement to call those models.

The Console workspace and its member own the upstream key. tmax owns the supplied
secret reference; an agent owns any native copy saved through its supported key
setup. The effective credential source and all affected native scopes must be
visible. Environment-variable names are not proof that two provider instances
use different keys.

These keys have no OAuth refresh owner. Console permits optional key expiration
and states that an expired key cannot be renewed. Recovery requires the user to
create and supply a replacement key. tmax must not create, refresh, rotate, or
revoke upstream keys automatically. A key that can list models but lacks
**Models > Request** permission must not be labeled inference-ready. Setup must
not require budget-write or billing-management permission for inference.

The upstream billing scope is the subscribed workspace, not a key, agent target,
or local provider ID. Go workspaces are single-member.

Two independent Go subscriptions must use separate providers, such as
`opencode-go` and `opencode-go-2`, with their own workspace keys and subscription
information. Several keys for the same workspace consume the same Go allowance.
Several agents using that workspace do not create additional allowance. tmax must
not claim independent subscriptions from key count or duplicate their quota in a
combined view.

A replacement key for the same known workspace identity preserves the provider.
A known different workspace identity requires a separate provider. The consulted
sources do not establish a general identity lookup that proves a supplied key's
subscription owner. Replacement follows the
[product authentication rules](../../product/spec.md#providers-and-authentication);
unknown workspace identity remains unknown.

## Product endpoints and model protocols

The [official harness contract](https://opencode.ai/v2/docs/console/harnesses/)
separates these bases:

| Access product | Base URL |
| --- | --- |
| Go subscription access | `https://opencode.ai/inference/go` |
| Console pay-as-you-go | `https://opencode.ai/inference` |

The integration must qualify the exact model, native protocol, endpoint, agent
version, and transport together. A Go catalog entry does not support every Go
protocol. The known map under the
[shared endpoint resolver](../spec.md#endpoint-resolution) is:

| Region ID | Native protocol | Exact protocol base | Declared inference operation beneath that base |
| --- | --- | --- | --- |
| `default` | `chat-completions` | `https://opencode.ai/inference/go/openai/v1` | `POST /chat/completions` |
| `default` | `responses` | `https://opencode.ai/inference/go/openai/v1` | `POST /responses` |
| `default` | `messages` | `https://opencode.ai/inference/go/anthropic` | `POST /v1/messages` |

The [Inference API](https://opencode.ai/v2/docs/console/api/inference/) documents
the HTTP creation methods. The
[Anthropic harness guide](https://opencode.ai/v2/docs/console/harnesses/claude-code/)
establishes the Messages base/path boundary. These sources do not add another
tmax agent target or qualify every model for every listed operation.

The model table in the
[Go documentation](https://opencode.ai/v2/docs/console/go/#endpoints) lists
`deepseek-v4.1-flash` at
`https://opencode.ai/inference/go/openai/v1/chat/completions`. This is a
Chat Completions setup scenario. It must not be advertised as a Codex Responses
route merely because the model is available on Go.

The harness overview and Codex guide also refer to Responses support for
“DeepSeek V4.” That family-level statement does not establish the exact
`deepseek-v4.1-flash` Responses endpoint. The general Inference API description
and harness guidance also differ on whether a model can accept several APIs.
Where the exact model/protocol evidence conflicts or remains ambiguous, tmax must
show that combination as unknown and block unsupported application or dispatch.
It must not resolve the conflict by guessing paths, translating protocols, or
broadening one model's qualified API to an entire family. Other independently
qualified model/protocol combinations must remain available.

The separately declared discovery operation is
`GET https://opencode.ai/inference/go/v1/models` for the selected workspace key.
It is not `/models` guessed beneath one of the protocol bases. Its results are
catalog evidence, not proof of credential identity, subscription allowance,
all-protocol support, or successful agent conversations. Model and endpoint
changes follow the
[model-catalog adoption contract](../../model-routing/spec.md#model-catalog).

Each declared inference operation requires the selected workspace authorization,
an exact qualified model, and the truthful client and stable conversation
identity defined [below](#conversation-and-client-identity). The same map and
requirements apply to direct and gateway use. HTTP/SSE evidence does not qualify an
undeclared transport. No response retrieval, cancellation, deletion, file
management, or other ID-only operation is established by these creation paths
or the discovery URL. Unknown operations and missing original bindings follow
the [shared fail-closed contract](../spec.md#supported-operations-and-resource-identity).

## Agent setup and native scope

The following are documented setup scenarios, not verified tmax adapters. The
shared agent boundaries still govern versioned configuration precedence and
credential handoff.

| Agent | Documented Go setup | Required qualification and native scope |
| --- | --- | --- |
| OpenCode | `/connect`, select **OpenCode Go**, supply the Console key, then select `opencode-go/<model-id>` | Qualify the versioned provider schema and the actual model's protocol. In V2, saved accounts and active selection belong to the server; the newest login becomes active. A second saved key does not establish concurrent per-session account selection. |
| Codex CLI | User-scoped custom provider with `wire_api = "responses"`, the Go Responses base from the map above, and an explicit key source such as `OPENCODE_API_KEY`; Codex appends `/responses` | Only Responses-qualified models are eligible. Respect the supported user/profile scope and restrictions on project-level provider configuration. Go recognizes Codex native session identity, but some versions and gateway configurations omit it. |
| Hermes Agent | Native `opencode-go` provider, `hermes model`, and `OPENCODE_GO_API_KEY` or the supported saved key; the guide distinguishes pay-as-you-go `opencode-zen` and `OPENCODE_ZEN_API_KEY` | Qualify the active `HERMES_HOME`/profile and its effective secret and model settings. The official guide requires v0.21.6 or later for automatic session coverage, including main and auxiliary OpenCode requests. This is an upstream requirement, not a tmax-tested version range. |
| stock pi | Native `opencode-go/<model-id>` and `OPENCODE_API_KEY`, or native key entry through `/login`; the default saved-key store is `~/.pi/agent/auth.json` | Resolve the active agent directory and credential precedence. The same environment-variable name also serves its pay-as-you-go provider. Official Go guidance says current builds send session information and older installations need an update; it does not establish a numeric tmax support range. |
| omp | The omp project lists `opencode-go` and `opencode-zen` using `OPENCODE_API_KEY`; model/provider configuration belongs to the active profile's `models.yml`/`models.yaml` | Qualify omp separately from stock pi, including API selection, credential precedence, active profile/auth store, and session/client headers on every Go-bound inference path. The consulted omp **18.12.6** source establishes the session-header mechanism below; it does not establish successful Go inference, entitlement, or OpenCode endorsement. |

The Hermes and pi guides describe native per-model API selection. That evidence
does not make a URL-only custom provider equivalent to their native integration,
or prove that every current model works with a particular installed version.
The omp project establishes an agent capability, not OpenCode endorsement or
inclusion in Go's validated-client list.

Selecting a tmax provider must select its actual workspace key. Where a native
provider has one shared active key or account, two local tmax provider IDs do not
create isolation. The adapter must use supported credential pinning or isolated
native scopes. Otherwise it must mark that multi-provider direct configuration
unavailable and identify the shared scope and affected targets. In particular,
adding the second OpenCode V2 saved account must not silently retarget existing
consumers through its automatic activation behavior.

## Conversation and client identity

Go is documented for coding-agent traffic. The client must identify itself
truthfully with its own recognizable coding-agent user agent rather than only a
generic SDK or HTTP-library name. tmax must preserve that identity, not impersonate
OpenCode, Codex, Hermes, pi, or another validated client to obtain access. A user
agent family identifies a mechanism, not an installed version; version and path
qualification require separate native evidence.

Every Go-bound inference request must carry a stable conversation session ID.
The recommended header is `x-opencode-session`. Go also recognizes some native
client session headers. An adapter may rely on one only when it qualifies the
actual header and its semantics for the selected agent version and request path.
The existence of a configurable headers object is not proof that it can supply
a changing native conversation identity.

Coverage must include streaming and nonstreaming calls, later tool turns,
continuations, and auxiliary model calls made through the Go provider, such as
summarization or other background agent work. Each request must retain the
identity of its actual conversation. The adapter must preserve any qualified
native parent/subsession distinctions rather than merge unrelated work into one
provider-wide session. It must not use a static ID for a key, provider, process,
or all conversations, or infer an ID from prompt text.

Native upstream configuration must establish this behavior in the agent itself
because tmax is absent from that path. Gateway forwarding must preserve the qualified
native session headers or forward `x-opencode-session` from the actual qualified
session identity. Every forwarding path must retain the truthful client identity;
handling the main visible request alone is insufficient. A gateway or client
version that drops session identity must remain unavailable until an upgrade,
supported configuration change, or qualified forwarding path fixes that loss.
A working direct path must not be disabled merely because a gateway path loses
the header.

A tmax gateway forwarding to Go must attribute every request. Go answers a
request that names no session with HTTP 400, so a client that supplies no
session header cannot reach Go at all. When no session header is present, the
gateway must mint one in OpenCode's own id shape and keep that id stable for
the conversation. A conversation's turns must then share one session id, and
distinct conversations must not share one.

A request that names a session but not qualifiably must still be rejected. An
unrecognized client, or two headers that disagree, named a session and got it
wrong. Attribution must not paper over that failure.

A value the client did supply is authoritative. The gateway must not replace
it, whatever its shape, because its exact value is what Go routes by.

The minted id is routing attribution, not a client identity. It must not
impersonate a validated client, must not alter the truthful user agent, and
must not be a static per-key, per-provider, or per-process constant. The id
shape must match the layout OpenCode's own client mints, because Go's free tier
rejects a malformed id. The seed that keeps one id per conversation must come
only from headers that name a conversation, and never from message content. A
per-message id is not a conversation marker and must not seed the map. The
retained map must be bounded. Where a qualified native session header exists,
that value must remain the only forwarded identity.

The source-qualified omp **18.12.6** mechanism is a per-request header projection
in `packages/ai/src/providers/inference-headers.ts`. For providers `opencode-go`
and `opencode-zen`, `applyInferenceHeaders` sets `x-opencode-session` to the
agent's authoritative conversation session ID and overwrites any same-named
header, while `User-Agent` uses a caller-wins rule. The ID is the live session
identity, not a static provider, key, process, or conversation-set value. The
companion `applySessionHeader` path supplies a model's configured
`compat.sessionHeader` name with the same live ID for other hosts.

This qualifies the header mechanism, its live per-conversation value, and its
overwrite precedence. It does not establish that omp covers every auxiliary
Go-bound path, that Go recognizes the header, that a tmax `models.yml` custom
provider reaches the same code path, or successful Go inference. The
`extraHeaders` configuration object is a separate static mechanism: writing a
fixed `x-opencode-session` there is not a qualified per-conversation identity and
must not be used to claim coverage.

The source-qualified stock pi **1.2.0** mechanisms have distinct boundaries:

| Native path | Required identity behavior |
| --- | --- |
| Direct built-in OpenCode provider | The native provider wrapper supplies `x-opencode-session` from the actual session. A generic custom provider is not that wrapper. |
| tmax gateway, Chat Completions | Native session-affinity configuration supplies dynamic `x-session-id` even for the local gateway URL. Forward that exact qualified value as `x-opencode-session`, retaining the native header and unchanged truthful user agent. This does not assert that Go itself recognizes `x-session-id`. |
| Missing session, unknown client, conflicting session headers, or an unqualified native path | Reject before upstream dispatch. Never repair the request with a static/generated ID, replacement client identity, or a different destination. |

Native tool turns, continuations, and resume preserve the same session; a native
new conversation supplies its distinct identity. Forwarding must preserve those
semantics and any native auxiliary-session distinctions. This qualification does
not extend pi's Chat Completions affinity mechanism to other APIs or versions.

A Go session header is not account authentication, proof of workspace ownership,
or authorization to select another provider. Gateway session selection must also
satisfy the
[verified client-session boundary](../../model-routing/spec.md#selection-and-continuity).
A missing reliable session identity cannot be repaired by inventing a supported
client header. Fixed destinations and qualified stateless selection do not waive
Go's own stable-conversation requirement. If the route cannot meet both
contracts, it must block the affected use and explain the missing identity.

The gateway's own attribution is the exception to that rule, and only for a
direct upstream forward to Go. It supplies a per-conversation id in OpenCode's
shape so that a client with no session header is not turned away outright. It
must not be a forged user agent, a static session header, or a substitute for a
qualified native session where one exists.

Request history must retain the specific identity or compatibility blocker for a
fixed offering or candidates sharing one blocker, including
`native_session_required` when Go's qualified session mechanism is missing.
Different candidate failures must not be mislabeled as one shared cause. A
retained destination rejected by client/session checks or configuration activation
must stay retained and blocked as appropriate; identity forwarding cannot rerank,
revive, or transfer that conversation.

## Quota and renewal

Go allowance belongs to the subscribed workspace. Model-dependent usage units
and quota windows must remain sourced provider data, not fixed tmax constants
or estimates of remaining requests.

The finite quota operation is a bearer-authenticated
`GET https://opencode.ai/zen/go/v1/usage` for the selected provider's key, with
truthful tmax client identification and `Accept: application/json`. This standalone
read requires no inference session header. It has no query, pagination, history,
alternate path, or inferred `/inference/.../usage` destination. It follows the
shared finite observation deadline, network policy, dry-run, protected-secret,
and guarded publication contracts.

The current wire boundary was observed on **2026-10-23**, not published as a
versioned provider schema:

| Field | Qualified meaning and boundary |
| --- | --- |
| `usage.rolling`, `usage.weekly`, `usage.monthly` | All three named allowance windows must be present; no per-model allocation or coverage is inferred. |
| Each window's `status` | The qualified current value is `ok`. An unknown or historical status is unsupported representation, not an inferred quota denial. |
| Each window's `percent` | Nonnegative integer, floored percentage consumed. Preserve it without clamping or deriving remaining allowance; zero does not establish exactly zero consumption. |
| Each window's `resetsAt` | Valid ISO-8601 UTC `Z` instant, retained exactly as a reset announcement, not proof of a completed reset or subscription renewal. |

Only these sourced window fields and their consumption/reset interpretation may
be retained from the response. Unsupported, missing, malformed, or oversized
representations fail the observation instead of creating zero or partial quota.
Transport, HTTP, parsing, or credential failures retain the last good dated fact
for that exact provider/authorization/source binding and report the latest
attempt separately. No result may populate a sibling provider or survive a
changed authorization binding as current evidence.

The response supplies no measurement timestamp, validity interval, raw allowance,
exact remaining amount, account/workspace identity, entitlement, or renewal date.
Local retrieval time must remain labeled retrieval time, not provider `asOf`;
freshness stays unqualified, with no synthesized `asOf`, TTL, or expiry. These
consumed-only observations supply no priority-rule inputs or routing veto.
Passing a reset announcement cannot recover quota locally. Unknown quota or
renewal remains explicit, with a [Console](https://opencode.ai/console) link and
optional user-maintained subscription information. A monthly plan does not
establish a workspace's next renewal date.

Console budgets are not Go quota data and are not an integration dependency.
Do not query or mirror Console credits, budgets, Extra Usage settings, or spending
caps. These remain outside the [billing scope](../../product/spec.md#billing-quota-and-renewal-information).
The only relevant Extra Usage fact is that upstream may continue charging after
included allowance is exhausted. Apply the shared
[eligibility evidence rules](../../model-routing/spec.md#selection-and-continuity)
without requiring access to those billing controls or changing the Go endpoint.

## Blocking failures and recovery

The integration must preserve provider-reported distinctions without inventing
Go-specific error codes or undocumented management APIs.

| Condition | Required user-visible result and recovery |
| --- | --- |
| Missing, invalid, revoked, or expired Console key | Block the affected authorization and show the provider's evidence. Console documents HTTP `401` for revoked or expired keys. Expiration requires a new key, not OAuth refresh. Use the workspace-aware replacement rules above; do not borrow another agent's credential. |
| Key lacks inference permission, or effective workspace access changed | Show the denied capability and the known upstream reason. Console documents HTTP `403` for requests outside key permissions. Ask the user to review Console access or supply an authorized key. A successful catalog read must not hide the inference failure. |
| Wrong workspace, absent Go entitlement, or an unqualified key kind | Show that Go access is unavailable or unverified with the known scope. Do not infer a Go-specific status code, buy a subscription, switch to pay-as-you-go, or select another workspace key silently. |
| Missing or incomplete session/client identity | Block the affected setup or request path. Identify the agent version, transport, or auxiliary path that lacks coverage. Offer an upgrade or a qualified native/direct/forwarding configuration, not a forged user agent or static session header. A gateway forward to Go must still mint its own per-conversation attribution id when the client supplied no session header. |
| Model/protocol mismatch or conflicting endpoint evidence | Keep that combination unavailable or unknown. Identify the exact model and requested protocol. Require a qualified compatible model/path; do not translate requests or silently change the model. |
| Provider-reported quota or billing denial | Preserve the reported cause, affected model/window, and any supplied reset information; direct the user to Console. Do not infer a different billing cause from HTTP status alone or query Console budgets to explain it. A depleted included allowance is not by itself proof that all Go access is blocked. |
| Generic rate limit, network failure, or ambiguous upstream rejection | Preserve the scoped failure and any supplied recovery information. Do not mark the subscription depleted, revoked, or expired without supporting evidence. |
| Shared native account switch, lost session binding, or changed workspace authorization during continuation | Block unsafe continuation and show the affected target/session. Restore the qualified original selection where possible or require an explicit new conversation under the chosen provider; do not transfer upstream conversation state to another workspace. |

## Acceptance boundary

1. Setup accepts the supplied Console key without a Go-only format rule or an
   OAuth workflow, and shows workspace identity as verified or unverified as the
   evidence permits.
2. Two independent workspace subscriptions remain separately addressable
   providers. Multiple keys and agents for one workspace do not multiply its
   allowance. A same-workspace key replacement preserves the provider and user
   subscription data while invalidating credential-derived observations.
3. Each advertised direct or gateway scenario resolves the same selected provider
   key, native scope, region/protocol map, exact model, and declared operation.
   Discovery uses its separate declared URL; it cannot be guessed beneath the
   inference base. Undeclared methods, paths, and transports remain unavailable.
   A second OpenCode saved key cannot silently switch existing consumers.
   Shared environment names do not substitute one workspace key for another.
4. `deepseek-v4.1-flash` retains its documented Chat Completions scenario.
   Ambiguous Responses claims remain unknown until qualified for that exact model.
   Codex cannot gain access by translating that Chat Completions endpoint.
5. Every advertised direct or gateway path retains truthful client identification
   and stable conversation identity across main, auxiliary, streaming, tool, and
   continuation requests. Missing native session coverage blocks the affected
   path, including fixed or stateless routes that fail Go's requirement.
   Pi's qualified affinity forwarding preserves the genuine session through
   resume and native new-session changes, rejects conflicting headers and unknown
   clients before dispatch, and retains the specific blocker in request history.
   It must not bypass retained-binding or activation safety. A gateway forward
   to Go attributes a client that supplied no session header with its own
   per-conversation id in OpenCode's shape, keeps one id per conversation, must
   not replace a client-supplied value, and must leave the truthful user agent
   unchanged.
6. Provider inspection reports the declared Go usage read's consumed percentages,
   provider status, and reset announcements, or explicit unknown information with
   a Console link. Malformed/failed refreshes retain the last good binding-scoped
   fact and a separate failed attempt; sibling providers remain isolated. No
   exact remaining quota, freshness, identity, renewal, entitlement, or rule input
   is inferred. Failure to query quota cannot disable otherwise usable inference,
   and no Console billing controls are mirrored.
7. Authentication, permission, identity-header, protocol, and scoped upstream
   quota/billing failures have actionable results without silently substituting
   a sibling provider or moving an established conversation.

These requirements supplement the
[shared qualification boundary](../spec.md#qualification-before-advertising-support).
Support labels must distinguish official provider documentation, agent-project
evidence, and actual tmax runtime evidence.

## Sources and evidence status

The documentation and upstream-source evidence below were consulted on
**2026-10-23**. A sanitized credentialed observation on that date qualified the
exact standalone usage alias with HTTP `200`, truthful `tmax/0.1.0` User-Agent,
and JSON Accept header, showing the three current windows and fields specified
[above](#quota-and-renewal). That direct upstream read is not acceptance evidence
for tmax's quota entrypoint or a versioned provider schema. The historical merged
handler used different field names and is not the current wire contract.
Documented client versions and native setup examples are upstream evidence, not
verified tmax version ranges. Workspace identity and renewal remain unknown
without independently qualified evidence or explicitly labeled user entry.
The installed pi 1.2.0 client source was inspected for the mechanisms above;
source inspection alone does not establish successful Go inference.

The **2026-10-25** gateway attribution work followed two third-party reference
implementations, both consulted as evidence rather than endorsement:

- [magpie `dd1e882`](https://github.com/yetone/magpie/blob/dd1e8822d916f8aeb085db4db93fc3d79e2263ed/internal/provider/opencode_client.go),
  an independent gateway that judges the destination by URL, forwards a
  client-supplied session unchanged, and otherwise mints one in OpenCode's shape
  while keeping one id per conversation through a bounded map.
- [OpenCode `packages/schema/src/identifier.ts`](https://github.com/anomalyco/opencode/blob/fe3f3a41f79ad292cc3c7c629567385a20ec5130/packages/schema/src/identifier.ts),
  the id layout a minted value must match. A malformed id is rejected by Go's
  free tier, so shape correctness is a functional requirement, not cosmetic.

Neither reference was copied. tmax judges the destination by URL as magpie does,
but does not adopt its user-agent replacement: this contract requires a truthful
client identity, and tmax reports its own agent string unchanged.

The OpenCode reference was inspected at `fe3f3a41` (1.18.32), which is not the
pinned 2.0.28, so it qualifies the id layout only and is not authority for the
live contract. Source inspection alone does not establish successful Go
inference.

On **2026-10-23**, actual tmax 0.1.0 entrypoint acceptance on SHA256
`f94f1c25367e2f049025f5c8966208f61ae52a113e09ac81ba36fb2b3e1f00f7`
used two independently supplied keys as separate provider instances. Each
provider's quota refresh returned HTTP `200` and retained its own usage snapshot.
Installed **pi 1.2.0 → tmax → Go `deepseek-v4.1-flash`**, using Chat Completions
and SSE, completed one harmless tool-call/result round trip and one continuation
per provider: three HTTP `200` requests each, with complete downstream delivery
and reported usage. Provider, offering, authorization-generation, and quota
observations remained isolated; workspace ownership was not established and no
combined allowance was inferred.

The concrete offering inherited accepted models.dev metadata without capability
overrides. Native inspection matched its context/output capacities, reasoning
capability, and text/image input. A documented native `before_provider_request`
hook changed only the existing request-generation budget to at most **512**;
the model's output capacity remained **384,000**, not 512. Automatic retries and
compaction were disabled. This qualifies the bounded native requests, not stock
default budgets, full-capacity or media workloads, all Go models/protocols,
auxiliary paths, or a general client-version range. Account identity, renewal,
source freshness, exact remaining quota, and provider-rule inputs remain subject
to their independent evidence boundaries.

Official OpenCode sources:

- [Go](https://opencode.ai/v2/docs/console/go/): workspace restriction, coding-agent
  identity, native session recognition, client version notes, model endpoints,
  allowance accounting, dashboard usage, and Extra usage.
- [Harnesses](https://opencode.ai/v2/docs/console/harnesses/): separate Go and
  pay-as-you-go bases, protocol paths, and native sessions.
- [Codex harness](https://opencode.ai/v2/docs/console/harnesses/codex/):
  Responses-only client setup, user-scope provider configuration, and Go profile.
- [Hermes harness](https://opencode.ai/v2/docs/console/harnesses/hermes/): native
  provider/key selection, per-model APIs, and automatic session requirement.
- [Pi harness](https://opencode.ai/v2/docs/console/harnesses/pi/): native Go model
  selection, shared key variable, saved API keys, and model-catalog updates.
- [Console API and keys](https://opencode.ai/v2/docs/console/api/) and
  [Inference API](https://opencode.ai/v2/docs/console/api/inference/):
  workspace binding, bearer authentication, permissions, expiration, and revocation.
- [Budgets](https://opencode.ai/v2/docs/console/budgets/): why Console spending
  budgets do not establish Go allowance.
- [OpenCode V2 provider accounts](https://opencode.ai/v2/docs/cli/providers/):
  server-owned saved credentials, newest-account activation, and native switching.

Go usage provenance, distinct from a published versioned API contract:

- [Merged usage-route PR #16513](https://github.com/anomalyco/opencode/pull/16513)
  and its [historical handler](https://github.com/anomalyco/opencode/blob/2b8a5969e932c15083e599c82d34ce0268f81b9e/packages/console/app/src/routes/zen/go/v1/usage.ts):
  source-backed route/authentication provenance, but an older response schema.
- [Thin inference-gateway migration PR #54563](https://github.com/anomalyco/opencode/pull/54563):
  the exact `/zen/go/v1/usage` alias is forwarded after migration; it does not
  establish a replacement `/inference/.../usage` path.
- [Production wire report #53701](https://github.com/anomalyco/opencode/issues/53701):
  community sample of the current windows and issue-thread flooring evidence,
  not a provider-guaranteed schema, enum, freshness, or permission contract.


Agent-project evidence, not additional OpenCode endorsement:

- [omp providers](https://github.com/can1357/oh-my-pi/blob/main/docs/providers.md):
  Go provider/key names and credential/profile precedence.
- [omp endpoint constraints](https://github.com/can1357/oh-my-pi/blob/main/docs/provider-endpoint-constraints.md):
  endpoint and transport distinctions.
- [omp source inspected for the session-header mechanism](https://github.com/can1357/oh-my-pi/blob/7414b7c919b59dd14db354a8f6b9164426237dff/packages/ai/src/providers/inference-headers.ts):
  `applyInferenceHeaders` projects the authoritative session ID as
  `x-opencode-session` for `opencode-go`/`opencode-zen` with overwrite
  precedence, and `applySessionHeader` supplies a configured
  `compat.sessionHeader` name with the same live ID. Source inspection
  qualifies the mechanism, not Go recognition, auxiliary-path coverage, or
  successful Go inference.
- [omp Go route resolution](https://github.com/can1357/oh-my-pi/blob/7414b7c919b59dd14db354a8f6b9164426237dff/packages/catalog/src/provider-models/openai-compat.ts):
  `createOpenCodeApiResolution` defaults to `openai-completions` and applies
  per-ID pins from the `api-routes` behavior rules. `deepseek-v4.1-flash` is not
  pinned to Responses, which agrees with this contract's Chat Completions
  scenario for that exact ID. Source inspection does not qualify the Go
  endpoint itself.

Agent-project evidence, not additional OpenCode endorsement:

- [stock pi models](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md):
  key ownership and precedence, native API selection, and retained authentication
  when only a built-in provider's base URL changes.
- [pi source pinned to the installed 1.2.0 qualification](https://github.com/earendil-works/pi/tree/06a466935536f8c9bcacb2d5efc0fc021be1bdff):
  `pi-ai`'s `api/openai-completions` supplies dynamic `x-session-id` in
  OpenRouter affinity mode and uses the truthful native user agent;
  `providers/opencode-headers` independently supplies `x-opencode-session` for
  built-in OpenCode providers. The coding agent's session manager generates
  UUIDv7 for ordinary new conversations and preserves the stored ID on resume.
  These client-source facts qualify the identity mechanism, not Go's recognition
  of an untranslated affinity header or a successful tmax inference request.
