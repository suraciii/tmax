# ChatGPT and Codex subscription access

## Authority and scope

This specification owns ChatGPT subscription access to Codex models. It covers
native-agent sign-in, account-selection boundaries, and the documented ChatGPT
model-gateway path. It supplements the
[shared integration contract](../spec.md#provider-specific-contracts).
[Product behavior](../../product/spec.md),
[configuration](../../configuration/spec.md), and
[model selection and continuity](../../model-routing/spec.md#selection-and-continuity)
remain authoritative for their shared rules. Terms follow
[CONTEXT.md](../../../CONTEXT.md).

This access product is not the OpenAI Platform API. A Platform API key uses
separate API billing and does not consume included ChatGPT subscription access.
Setup must not offer an API-key alternative under this integration or change the
billing product after a subscription limit or authentication failure.

Workspace automation tokens, service accounts, and a tmax-owned ChatGPT OAuth
client are outside this integration.

## Authentication and evidence boundaries

| Login requirement | Contract |
| --- | --- |
| Source | A qualified native Agent target; identify it and its effective credential scope before sign-in |
| Sign-in and refresh | Both remain with that source; [source selection](../../product/spec.md#login-source-selection) does not create a tmax-owned ChatGPT login path |
| Session reference | An opaque reference to the source's authorization, never permission to import a token file |

The evidence for native sign-in has different authority:

| Native path | Established evidence | Qualification boundary |
| --- | --- | --- |
| Codex CLI ChatGPT sign-in | OpenAI documents `codex login`, browser sign-in, cached credentials, automatic refresh, and shared CLI/IDE login state. | Qualify the deployed version, credential-storage mode, effective configuration, workspace restrictions, and all consumers of that scope. |
| omp `openai-codex` sign-in | omp documents its own OAuth login and refreshable credential storage. It also documents automatic ranking and rotation of saved logins. | This is agent-project evidence, not OpenAI endorsement of every plan, client, or relay use. The selected tmax authorization requires a supported pin or isolated native scope. |
| Hermes `openai-codex` sign-in | Hermes documents its own ChatGPT device-code login, optional browser flow, native credential storage, and refresh recovery. | This is agent-project evidence. Hermes does not establish all eligible ChatGPT plans or how its use consumes their allowance. Borrowed Codex logins require the restrictions below. |

A documented omp or Hermes login must not be described as Codex-only access, but
it also must not be advertised as provider-verified tmax support. The other agent
types require their own qualified native sign-in and transport evidence. A
custom base URL or a readable Codex credential cache is not sufficient evidence.

### Session ownership and identity

One native authorization must have one refresh owner. tmax must not copy a
refresh token into another agent store or take refresh ownership because the
token is readable. Codex CLI and its IDE extension can share cached login state;
setup must expose that shared scope rather than promise independent sign-out or
account selection.

Each direct selection must resolve to the configured provider's native
authorization. A supported native account pin or an isolated native scope must
prevent automatic rotation to another login. If the adapter cannot establish that
boundary, the selection must remain blocked. A provider ID, model namespace, or
project directory does not by itself isolate native authentication.

omp's documented automatic rotation must not become tmax account selection.
Hermes must not recover a failed selected login by adopting a different ambient
Codex login. Hermes documents `auth.adopt_external_logins: false` for disabling
automatic adoption, but that setting alone does not prove that the selected
native authorization is pinned. Its native setup can still offer explicit
credential import. tmax setup must use an independent Hermes sign-in rather than
offer that import or rely on a borrowed Codex refresh token.

Targets may share a native-owned session for one provider only when the adapter
establishes the same credential scope, selected authorization, and single refresh
owner through a supported contract. Different provider instances must not alias,
rotate through, or pool one mutable native session. Independently selectable
logins require separate providers and independent supported native scopes, even
when they identify the same upstream user or workspace. Those authorizations do
not establish separate allowances.

Account metadata may remain unverified when the native login source does not expose a
supported identity source. That does not authorize guessing identity from a file
name, email label, or request header. A qualified native scope must still bind
the direct selection to its configured authorization. Native-session forwarding
requires the stronger identity contract described below.

Reauthentication must use the same declared native login source. A known different user
or workspace requires a separate provider. A same-identity reauthentication must
invalidate credential-derived observations and qualify continuation under the
shared continuity contract. Replacing a local gateway key changes local client
authentication, not the upstream login or billing identity. Replacing a Platform
API key is outside this integration.

## Direct configuration

The known integration has one published endpoint set under the
[shared endpoint resolver](../spec.md#endpoint-resolution):

| Region ID | Native protocol | Upstream base |
| --- | --- | --- |
| `default` | `responses` | `https://chatgpt.com/backend-api/codex` |

Codex's documented direct ChatGPT scenario uses this Codex/Responses backend.
It is not a general Chat Completions or Anthropic Messages API. Model
availability and capabilities must follow the selected authorization and
qualified client version. Platform model metadata must not be treated as
subscription entitlement.

A qualified direct target must issue inference requests without a running tmax
server. It must retain the native login source's supported sign-in and refresh path.
For omp and Hermes, qualification must cover their actual Codex/Responses
transport and credential precedence. Selecting `openai-codex` or changing its
base URL does not prove that another credential source, automatic rotation, or
borrowed-login recovery cannot override the selection.

Codex supports managed login-method and workspace restrictions. Setup must
respect the effective restrictions and native storage requirements. It must not
assume that a project-level configuration can override user, system, or managed
policy. When native account selection is shared, every affected target must see
that scope and any conflicting selections must remain blocked.

## Conditional ChatGPT gateway access

OpenAI documents
[ChatGPT sign-in through a model gateway](https://learn.chatgpt.com/docs/enterprise/sign-in-with-chatgpt-through-a-gateway)
using workspace-managed model-provider requirements. This is a conditional proxy
scenario. It does not imply that any personal subscription can redirect traffic
through local user configuration. Qualification must establish the supported Codex build, effective
workspace policy, available management permission, and supported credential
delivery. tmax must not bypass managed policy or add workspace administration to
its local product scope. Where an authorized administrator must supply policy,
setup must identify that prerequisite.

The gateway's qualified upstream destination must be the same resolved
Codex/Responses base used by direct configuration. The local gateway base is not
an upstream endpoint override. The declared HTTP operation surface is:

| Method and path beneath the gateway and upstream bases | Purpose and required binding |
| --- | --- |
| `GET /models` | Codex model discovery for the admitted provider authorization; retain the Codex `models` array contract |
| `POST /responses` | Model-bearing Responses inference for the admitted native authorization and qualified client/session scope |

The subscription route must not forward to the Platform `/v1` API. A generic
OpenAI model-list response alone is insufficient. Streaming, tool turns,
continuation, and any selected incremental or WebSocket transport require the
qualification defined by the
[shared integration contract](../spec.md#qualification-before-advertising-support).
A WebSocket or other transport needs its own explicit operation/transport
declaration; the HTTP creation path does not qualify it.

This contract establishes no ID-only response retrieval, deletion, cancellation,
or file-management forwarding. Such operations remain unavailable without their
own finite declaration and reliable original binding under
[the resource-identity contract](../spec.md#supported-operations-and-resource-identity).
The two model-provider paths do not establish forwarding support for other Codex
connections or product surfaces.

The two credential paths must remain separate:

- The client sends its native ChatGPT token in `Authorization`. The gateway must
  forward it and any `ChatGPT-Account-ID` unchanged to the qualified Codex backend.
- The client sends the shared tmax proxy key in a separate supported
  header. OpenAI's documented static credential path uses `env_http_headers` and
  `X-Gateway-Key`. The gateway must validate its own credential and remove that
  header before forwarding upstream.

The local credential must not replace ChatGPT `Authorization`, grant upstream
entitlement, or serve as proof of an upstream account. The proxy must not retain
the forwarded native token. Local credential lifecycle follows the
[product proxy contract](../../product/spec.md#model-proxy).

OpenAI also documents native gateway OAuth. That option requires the gateway's
own OAuth authorization server. tmax does not require or add that server; this
integration qualifies the separate static local credential path where the client
and policy support it. Codex does not refresh the static header during a running
launch. Local key replacement must therefore disclose the required native
reapplication and relaunch, separately from automatic native ChatGPT refresh.

`ChatGPT-Account-ID` is a forwarded workspace hint, not verified caller identity.
The adapter must establish the forwarded authorization through a supported
identity and native-scope contract before admitting a provider route. It must
not invent a public identity or token-introspection API. Eligible concrete or
alias destinations must be restricted to the provider authorization established
by that contract. A model alias must not select a different login by rewriting
the account header or substituting another agent's session.

Without qualified forwarding identity, policy permission, or credential
delivery, proxy access must be unavailable with the missing prerequisite shown.
A supported native direct option must remain available. Proxy unavailability
must not silently switch an applied target or change its authentication method.
The gateway does not extend subscription access to arbitrary third-party callers.

## Allowance and renewal

OpenAI documents current allowance visibility in the
[Codex usage dashboard](https://chatgpt.com/codex/settings/usage) and through
`/status` during an active Codex CLI session. Native display is not a general
public query API. The consulted documentation does not establish a general public
API for ChatGPT subscription quota or renewal information.
Enterprise analytics or compliance access must not be advertised as a universal
personal subscription query.

When no supported source is qualified, CLI and Web must show quota and renewal as
unknown or unavailable, with an explanation and the relevant dashboard or billing
link. User-maintained subscription information and qualified observations follow
the [billing contract](../../product/spec.md#billing-quota-and-renewal-information).
Unknown query capability must not block an otherwise qualified inference path.

Local and cloud Codex use can share allowance, and other eligible product use
can consume the same billing scope. Multiple native sessions, devices, or local
provider IDs must not multiply that allowance. tmax proxy traffic is only a
partial observation and cannot establish remaining account-wide quota. Plan
prices, model rosters, reset windows, and eligibility must not be hard-coded as
universal subscription facts.

Eligible paid continuation means exhausted included allowance does not by itself
establish blocked inference; apply the
[shared selection evidence rules](../../model-routing/spec.md#selection-and-continuity).
A quota reset, credential expiry, and subscription renewal remain separate facts.

### Priority-rule inputs

The [runtime input contract](../../model-routing/inputs/spec.md#input-values-and-acquisition)
owns typed manual quota/reset recording and the bounded
[`codex-resets` announcement reader](../../model-routing/inputs/spec.md#codex-reset-announcements).
Selecting this integration does not qualify a general `provider`-source quota
query. A manually recorded allowance remains visibly caller-supplied and cannot
replace unknown provider-verified quota or renewal information.

The public announcement source is third-party evidence of a pending future reset,
not an account endpoint. Its supported deterministic representation and failure,
expiry, and withdrawal boundaries belong to the input contract. Neither a
scheduled time, a historical site confirmation, nor the site's counters establish
that this provider's authorization was eligible or its allowance recovered.
Explicit manual registration remains available without credentials or a running
service; it does not unlock inference or change the selected native login.

## Blocked states and recovery

Failures must identify the affected provider, native scope, and failing layer.
The integration must not invent provider error codes or infer a cause that the
available result does not establish.

| Condition | Required result and recovery |
| --- | --- |
| Native sign-in is missing, expired, revoked, or cannot refresh. | Show the native login source's reported authentication state. Require sign-in or reauthentication through that source when needed. Do not adopt an ambient login or create an API-key fallback. A network failure alone must not be labeled revocation. |
| Native rotation, borrowed-login recovery, or a shared scope can select another authorization. | Block the affected selection. Identify the conflicting native scope and require a supported pin, isolated scope, or independent native sign-in. |
| Native sign-in changed the shared CLI/IDE authorization before identity rejection or local commit failure. | Apply the [product authentication failure boundary](../../product/spec.md#providers-and-authentication). Report the changed native scope and affected consumers; recover the intended identity through the qualified owner rather than treating the old session reference as a rollback. |
| Native credentials conflict with enforced login-method, workspace, or storage requirements. | Show the managed restriction and affected target. Require a permitted sign-in or authorized policy correction; a local override must not be offered as a bypass. |
| The gateway's local credential is missing, invalid, or replaced. | Identify a local proxy authentication failure separately from ChatGPT authentication. Require the correct local credential and native reapplication or relaunch as appropriate. Do not sign the user out of ChatGPT as the repair. |
| The upstream rejects ChatGPT authentication, workspace permission, entitlement, or usage. | Preserve the upstream distinction when available. Direct the user to native reauthentication, the workspace administrator, or the provider's usage controls according to the actual evidence. An unspecified rate limit is not proof of depleted subscription allowance. |
| The native-session identity contract is unknown, or the selected provider does not match it. | Block forwarding. Explain the missing identity qualification or mismatch without trusting or rewriting `ChatGPT-Account-ID`. Retain the supported direct option. |
| Managed gateway setup, model discovery, or required Responses semantics cannot be qualified. | Keep that proxy combination unavailable. Identify the missing permission, client capability, or protocol requirement instead of substituting a generic OpenAI route. |
| A quota or renewal query is unavailable or fails. | Keep the information unknown or stale with its source and dashboard action. Do not treat the query failure as failed inference authentication. |
| A public reset-announcement read fails, becomes unsupported, or passes its announced boundary. | Apply the [input lifecycle and source contract](../../model-routing/inputs/spec.md#codex-reset-announcements). Explain the unavailable/expired forecast independently of quota and authentication; never infer recovered account allowance or block explicit manual registration. |

## Observable acceptance

1. Setup offers only qualified native ChatGPT login for this integration, not a
   Platform API-key alternative.
2. Provider details show the native login source, effective credential and
   account-selection scope, and documentation-versus-runtime evidence. Codex,
   omp, and Hermes evidence remains attributed to its publisher.
3. A qualified direct selection uses exactly its configured authorization with
   tmax absent from the inference path. Automatic rotation and borrowed-login
   recovery cannot move it to another login; unsupported isolation is visibly
   blocked before application.
4. Two independently selectable ChatGPT authorizations remain separate providers
   with independent supported native scopes. Adding or refreshing one does not
   switch the other or duplicate shared allowance. Shared CLI/IDE login effects
   remain visible.
   A rejected reauthentication or failed local commit after a shared native
   change reports that effect and its recovery boundary; it does not claim the
   CLI/IDE retained the prior authorization.
5. Proxy setup shows the managed-policy and permission prerequisite. A personal
   account without a qualified setup sees proxy access unavailable, not a promise
   that a base-URL edit enables it.
6. A qualified gateway serves its declared `GET /models` and `POST /responses`
   against the same resolved Codex backend as direct configuration. It rejects
   undeclared method/path and transport combinations. It preserves native
   ChatGPT headers, validates and strips only its local credential, and retains
   the selected provider through tool and follow-up turns. An unverified account
   header or resource ID cannot unlock another provider route.
7. Native ChatGPT refresh and local gateway-key replacement have distinct visible
   outcomes. Local key replacement discloses required native relaunch; it does
   not change upstream identity or repair an upstream entitlement failure.
8. CLI and Web explain unknown quota and renewal with usage or billing links.
   Missing public query capability leaves otherwise qualified inference usable.
9. Authentication, managed-policy, local gateway, upstream entitlement, protocol,
   and quota-information failures remain distinguishable with the recovery action
   defined above. No failure silently changes the login, provider, or billing
   product.
10. Manual quota/reset inputs remain caller assertions. A public Codex reset
    forecast is scoped to its declared models and validity under the input
    contract; unsupported representations and failed reads do not become quota
    queries, recovered allowance, or fabricated dates. Manual registration stays
    available without a resident service or credentials.

## Sources and evidence status

Documentation was consulted on 2026-10-23. These sources establish documented or
conditional scenarios, not verified tmax behavior. No tmax runtime integration
has been exercised. Supported version ranges remain unset until the shared
qualification requirements are satisfied.

OpenAI sources:

- [Authentication](https://developers.openai.com/codex/auth): native ChatGPT login,
  Platform key billing, login caching, automatic refresh, storage, enforced
  login/workspace restrictions, and the separate automation-token path.
- [Sign in with ChatGPT through a gateway](https://learn.chatgpt.com/docs/enterprise/sign-in-with-chatgpt-through-a-gateway):
  managed requirements, Codex backend paths, two independent credentials, header
  preservation, static-header lifetime, and the separate gateway OAuth option.
- [Gateway compatibility](https://learn.chatgpt.com/docs/enterprise/gateway-compatibility):
  Responses conversation requirements for the API/provider credential path. It
  does not replace the subscription gateway's authentication contract.
- [Managed configuration](https://learn.chatgpt.com/docs/enterprise/managed-configuration):
  policy permissions, configuration scope, and enforced requirements.
- [Codex pricing and usage](https://learn.chatgpt.com/docs/pricing): dashboard and
  native usage visibility, shared allowance, and eligible paid credit continuation.
- [Managing billing for ChatGPT and the API platform](https://help.openai.com/en/articles/9039756-managing-billing-for-chatgpt-and-the-api-platform):
  separate billing products.

Agent-project sources:

- [omp providers](https://github.com/can1357/oh-my-pi/blob/main/docs/providers.md):
  native OAuth login, credential precedence, refresh ownership, and automatic
  account rotation. These statements are omp capability evidence, not provider
  endorsement.
- [Hermes providers](https://hermes-agent.nousresearch.com/docs/integrations/providers):
  its own ChatGPT login, refresh, optional Codex credential import, and explicitly
  undocumented plan-eligibility and quota-consumption details.
- [Hermes borrowed CLI logins](https://hermes-agent.nousresearch.com/docs/user-guide/security#borrowed-cli-logins):
  external-login adoption, refresh-token conflicts, independent native login, and
  the limits of disabling automatic adoption.

Third-party announcement evidence:

- [Codex Resets](https://codex-resets.com/) and its advertised
  [English Markdown representation](https://codex-resets.com/index.md): pending
  reset announcements, absolute timestamps, announcement links, and the explicit
  warning that passing a scheduled time does not establish completion. The
  [input source contract](../../model-routing/inputs/spec.md#codex-reset-announcements)
  owns the qualified representation; this source establishes no account quota or
  tmax runtime verification.
