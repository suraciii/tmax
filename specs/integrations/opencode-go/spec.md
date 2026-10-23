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
requirements apply to direct and proxy use. HTTP/SSE evidence does not qualify an
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
| Codex CLI | User-scoped custom provider with `wire_api = "responses"`, the Go Responses base from the map above, and an explicit key source such as `OPENCODE_API_KEY`; Codex appends `/responses` | Only Responses-qualified models are eligible. Respect the supported user/profile scope and restrictions on project-level provider configuration. Go recognizes Codex native session identity, but some versions and proxy configurations omit it. |
| Hermes Agent | Native `opencode-go` provider, `hermes model`, and `OPENCODE_GO_API_KEY` or the supported saved key; the guide distinguishes pay-as-you-go `opencode-zen` and `OPENCODE_ZEN_API_KEY` | Qualify the active `HERMES_HOME`/profile and its effective secret and model settings. The official guide requires v0.21.6 or later for automatic session coverage, including main and auxiliary OpenCode requests. This is an upstream requirement, not a tmax-tested version range. |
| stock pi | Native `opencode-go/<model-id>` and `OPENCODE_API_KEY`, or native key entry through `/login`; the default saved-key store is `~/.pi/agent/auth.json` | Resolve the active agent directory and credential precedence. The same environment-variable name also serves its pay-as-you-go provider. Official Go guidance says current builds send session information and older installations need an update; it does not establish a numeric tmax support range. |
| omp | The omp project lists `opencode-go` and `opencode-zen` using `OPENCODE_API_KEY`; model/provider configuration belongs to the active profile's `models.yml`/`models.yaml` | Qualify omp separately from stock pi, including API selection, credential precedence, active profile/auth store, and session/client headers on every Go-bound inference path. The consulted project evidence does not establish that full Go identity contract. |

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
truthfully with its own user agent rather than only a generic SDK or HTTP-library
name. tmax must not impersonate OpenCode, Codex, Hermes, pi, or another validated
client to obtain access.

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
because tmax is absent from that path. Proxy forwarding must preserve the qualified
native session headers or forward `x-opencode-session` from the actual qualified
session identity. Every forwarding path must retain the truthful client identity;
handling the main visible request alone is insufficient. A proxy or client
version that drops session identity must remain unavailable until an upgrade,
supported configuration change, or qualified forwarding path fixes that loss.
A working direct path must not be disabled merely because a proxy path loses the
header.

A Go session header is not account authentication, proof of workspace ownership,
or authorization to select another provider. Proxy session selection must also
satisfy the
[verified client-session boundary](../../model-routing/spec.md#selection-and-continuity).
A missing reliable session identity cannot be repaired by inventing a supported
client header. Fixed destinations and qualified stateless selection do not waive
Go's own stable-conversation requirement. If the route cannot meet both
contracts, it must block the affected use and explain the missing identity.

## Quota and renewal

Go allowance belongs to the subscribed workspace. Model-dependent usage units
and quota windows must remain sourced provider data, not fixed tmax constants
or estimates of remaining requests.

The consulted documentation directs users to [Console](https://opencode.ai/console)
for current usage; it does not establish a stable public Go quota or renewal API.
Without a qualified source, show those observations as unknown, provide the
Console link, and retain optional user-maintained subscription information.
A monthly plan does not establish a specific workspace's next renewal date.

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
| Missing or incomplete session/client identity | Block the affected setup or request path. Identify the agent version, transport, or auxiliary path that lacks coverage. Offer an upgrade or a qualified native/direct/forwarding configuration, not a forged user agent or static session header. |
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
3. Each advertised direct or proxy scenario resolves the same selected provider
   key, native scope, region/protocol map, exact model, and declared operation.
   Discovery uses its separate declared URL; it cannot be guessed beneath the
   inference base. Undeclared methods, paths, and transports remain unavailable.
   A second OpenCode saved key cannot silently switch existing consumers.
   Shared environment names do not substitute one workspace key for another.
4. `deepseek-v4.1-flash` retains its documented Chat Completions scenario.
   Ambiguous Responses claims remain unknown until qualified for that exact model.
   Codex cannot gain access by translating that Chat Completions endpoint.
5. Every advertised direct or proxy path retains truthful client identification
   and stable conversation identity across main, auxiliary, streaming, tool, and
   continuation requests. Missing native session coverage blocks the affected
   path, including fixed or stateless routes that fail Go's requirement.
6. Provider inspection shows sourced Go quota and renewal, or explicit unknown
   information with a Console link. It does not mirror Console billing controls;
   failure to query quota cannot disable otherwise usable inference.
7. Authentication, permission, identity-header, protocol, and scoped upstream
   quota/billing failures have actionable results without silently substituting
   a sibling provider or moving an established conversation.

These requirements supplement the
[shared qualification boundary](../spec.md#qualification-before-advertising-support).
Support labels must distinguish official provider documentation, agent-project
evidence, and actual tmax runtime evidence.

## Sources and evidence status

The following documentation was consulted on **2026-10-23**. No tmax runtime
integration was exercised. The documented client versions and native setup
examples are upstream evidence, not verified tmax version ranges. Live workspace
identity, quota, and renewal remain unknown until supplied by a qualified source
or explicitly labeled user entry.

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

Agent-project evidence, not additional OpenCode endorsement:

- [omp providers](https://github.com/can1357/oh-my-pi/blob/main/docs/providers.md):
  Go provider/key names and credential/profile precedence. Full Go session and
  client-identity behavior remains unqualified by the consulted evidence.
- [omp endpoint constraints](https://github.com/can1357/oh-my-pi/blob/main/docs/provider-endpoint-constraints.md):
  endpoint and transport distinctions; not proof of Go header coverage.
- [stock pi models](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md):
  key ownership and precedence, native API selection, and retained authentication
  when only a built-in provider's base URL changes.
