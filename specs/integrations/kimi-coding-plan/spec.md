# Kimi Coding Plan

## Authority and scope

This specification owns the Kimi Coding Plan integration: Kimi membership access
used for personal interactive coding through a membership API key or a qualified
native-agent login. It extends the [shared integration contract](../spec.md) and
its [provider-specific index](../spec.md#provider-specific-contracts).
[Product](../../product/spec.md) owns provider identity and authentication
lifecycle. [Configuration](../../configuration/spec.md) owns the existing fields.
[Model selection](../../model-routing/spec.md) owns routing and continuity.
Shared terms are defined in [CONTEXT.md](../../../CONTEXT.md).

Kimi Open Platform pay-as-you-go access is a different service product. Its keys,
endpoints, balance, and billing must not be substituted for Coding Plan access.
Managing the first-party Kimi Code CLI is not an additional tmax agent target.
Its native login is evidence about that client, not a credential-import feature.

## Authentication and upstream identity

Third-party key setup must accept a membership key created in the
[Kimi Code Console](https://www.kimi.com/code/console). Signing into that website
to create a key does not make inference login-authenticated. A command such as
OpenCode's `auth login` that asks for this key remains `api-key` authentication.
The provider's secret reference must select the supplied membership key, not an
ambient Open Platform key with a familiar environment-variable name.

The [official overview](https://www.kimi.com/code/docs/en/index.html) and
[authentication error reference](https://www.kimi.com/code/docs/en/kimi-code/error-reference.html#invalid-authentication)
establish that Coding Plan and Open Platform keys and base URLs are not
interchangeable. A key prefix can help explain a product mismatch. It must not
prove credential validity, account identity, membership status, or model access.

A membership key has no OAuth refresh workflow in this contract. Recovery from a
revoked or expired key requires an explicit supplied-key replacement. tmax must
not create or rotate upstream keys. Native credential precedence must be resolved
before describing which key an agent will use.

The [Kimi Code CLI guide](https://www.kimi.com/code/docs/kimi-code-cli/guides/getting-started.html)
documents its own Kimi Code OAuth device flow. This does not establish a public
tmax-owned login flow or permission to copy its tokens. The
[omp provider documentation](https://github.com/can1357/oh-my-pi/blob/main/docs/providers.md)
separately lists native `kimi-code` OAuth. That is agent-project evidence, not
Kimi endorsement of omp or tmax.

A managed `login` path must name a version-qualified omp target as its login
source. That target performs sign-in and remains the sole refresh owner.
A saved `auth.method = "login"` or native `auth: oauth` setting must not manufacture a
session. Another agent or a tmax gateway must not consume that native session
without its own qualified owner/consumer forwarding contract. When that contract
is missing, setup must report the login path as unavailable and explain the
missing qualification. tmax must not offer a generic tmax-owned Kimi login or
import Kimi CLI tokens to make the path appear available.

A second independently selectable Kimi membership is another provider. Several
keys from one membership do not establish several subscriptions. Credential
replacement follows the
[product authentication rules](../../product/spec.md#providers-and-authentication).
A successful inference probe does not establish account identity or safe
continuation of conversations tied to the old authorization.

The consulted sources do not establish a general public, non-inference identity
or membership-query API for tmax. Setup must permit unknown upstream identity.
A configured but unresolved key cannot authenticate; a resolved key and valid
Coding endpoint establish local readiness, not verified membership access.
Authentication and entitlement remain unverified without a supported check.
tmax must not invent a `whoami` endpoint or issue a completion as a supposedly
free validation step. A network failure leaves remote validity unknown rather
than proving a revoked key or expired membership.

An explicit inference probe must disclose that it can consume quota or incur
charges. Its result must identify the tested provider, integration, protocol,
model, and time. A successful reply proves that request worked, not account identity,
plan, quota, or renewal.

## Endpoints and native protocols

The [official endpoint table](https://www.kimi.com/code/docs/en/index.html#service-endpoint)
defines the following known map under the
[shared endpoint resolver](../spec.md#endpoint-resolution):

| Integration ID | `chat-completions` base | `responses` base | `messages` base |
| --- | --- | --- | --- |
| `kimi-code-plan-cn` | `https://api.kimi.com/coding/v1` | `https://api.kimi.com/coding/v1` | `https://api.kimi.com/coding/` |
| `kimi-code-plan-global` | `https://api.kimi.ai/coding/v1` | `https://api.kimi.ai/coding/v1` | `https://api.kimi.ai/coding/` |

The declared HTTP creation operations append these suffixes at the documented
base/path boundary:

| Native protocol | Method | Suffix | Required request identity |
| --- | --- | --- | --- |
| `chat-completions` | `POST` | `/chat/completions` | Selected membership authorization, exact model, and qualified real client |
| `responses` | `POST` | `/responses` | Selected membership authorization, exact model, and qualified continuation scope |
| `messages` | `POST` | `v1/messages` | Selected membership authorization, exact model, and qualified real client |

For example, the China Messages endpoint is
`https://api.kimi.com/coding/v1/messages`. A target that expects a base URL must
not receive a full request URL that causes it to append the path twice. Direct
configuration and gateway forwarding must resolve the same map entry.

Kimi's regional products use distinct integration IDs under the shared resolver;
there is no separate region setting. The adapter shows the actual Coding host
and native protocol. Changing integration requires explicit configuration and
qualification for that access. Documentation of both hosts does not prove
cross-region key portability. Neither `api.moonshot.ai/v1` nor
`api.moonshot.cn/v1` is a Coding Plan fallback.

These creation paths do not establish a model-discovery API, ID-only response
retrieval, cancellation, deletion, or file-management operation. An undeclared
operation or transport and an unavailable original resource binding must fail
closed under the
[shared operation contract](../spec.md#supported-operations-and-resource-identity).
The native login-source requirements and
[personal interactive use requirements](#personal-interactive-use-and-client-identity)
still apply; an endpoint or resource ID cannot qualify another credential owner
or remove a client restriction.

Protocol support is not interchangeable capability support. Qualification must
cover the selected agent's streaming, reasoning, tool calls, native transports,
and continuation behavior. The [request-parameter contract](../../model-routing/spec.md#request-parameters)
governs native settings. The adapter must not inject reasoning settings, compact
history, or convert protocols to make an incompatible request work.

Model and context entitlement must come from the selected membership and current
provider evidence. A shared model ID does not imply the same context entitlement
for every plan. Agent display labels must not change upstream IDs. For example,
the official Hermes guide distinguishes the wizard label `kimi-k3` from the
upstream ID `k3`. Catalog availability is not membership entitlement.

## Agent consumption and native scope

The following are distinct documented scenarios, not tmax runtime verification:

| Agent | Evidence and direct-configuration scenario | Scope and gateway boundary |
| --- | --- | --- |
| Codex CLI | The [Kimi Codex guide](https://www.kimi.com/code/docs/en/third-party-tools/codex.html) documents a Console membership key, a custom provider at the regional `/coding/v1` base, and `wire_api = "responses"`. | Respect the qualified Codex user/profile and managed scope. Existing ChatGPT sign-in does not authenticate Kimi. The documented direct Responses route does not require a local relay. A Responses gateway requires separate qualification of the real Codex client and continuation behavior. |
| Hermes Agent | The [Kimi Hermes guide](https://www.kimi.com/code/docs/en/third-party-tools/hermes.html) documents membership-key setup and an explicit Chat Completions configuration at `/coding/v1`. | Resolve the active `HERMES_HOME`/profile and effective key source. A native Coding Plan wizard choice is not OAuth. Qualify the selected Chat Completions route; do not infer Messages or Responses support from the brand menu. Telegram, Discord, batch, or service access is not qualified by the personal local interactive setup. |
| OpenCode | The [Kimi OpenCode guide](https://www.kimi.com/code/docs/en/third-party-tools/opencode.html) documents selecting Kimi For Coding and supplying a membership API key through native authentication. | The versioned provider package determines the actual protocol and endpoint. The guide does not qualify every custom endpoint or package. Respect the server-scoped active credential/account selection described in the shared agent contract. A new native login can affect other consumers of that server. |
| stock pi | The [pi provider documentation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/providers.md) lists Kimi For Coding key access through `KIMI_API_KEY`. This is agent-project evidence only. | Resolve the active agent directory and effective native API, key, and base URL. Do not treat the environment name or a base-URL override as proof of the selected credential. Both direct access and a matching native-protocol gateway require version-specific qualification. |
| omp | The [omp provider documentation](https://github.com/can1357/oh-my-pi/blob/main/docs/providers.md) lists native `kimi-code` OAuth. This is distinct from the membership-key scenarios and is agent-project evidence only. | Resolve the active profile and native credential precedence. Qualify the owner, selected authorization, endpoint, and protocol. Native OAuth availability does not grant tmax or another target access to the session. A custom key/base configuration must be qualified separately; it must not inherit an unrelated stored session. |

A managed target must retain the selected provider authorization, not silently
use another membership because native authentication is provider-scoped or
selects from several saved accounts. If the native target cannot select the
required authorization independently, tmax must identify the affected native
scope and block that combination. It must not promise per-project or concurrent
per-session account isolation that the target cannot provide.

Direct documented membership-key paths must remain independent of gateway
qualification. A user must be able to choose a qualified concrete direct model
without creating a model alias or running tmax on the inference path. A local
same-protocol gateway can provide concrete-model or alias access only
for a separately qualified route. It must retain the actual client's identity
and the [session binding](../../model-routing/spec.md#selection-and-continuity).
Changing keys or providers must not transfer provider-owned continuation state.

## Personal interactive use and client identity

The [community guidelines](https://www.kimi.com/code/docs/en/kimi-code/community-guidelines.html)
restrict Coding Plan subscriptions to personal interactive use. They prohibit
account or API-access resale, repackaging access as a service, client-identity
spoofing, and non-interactive batch or data-pipeline use. They explicitly
prohibit sharing an account with other people through a reverse gateway.

The integration must not offer shared or remote membership gateways. It must
not impersonate Kimi CLI or another supported client by substituting a fabricated
User-Agent or masking the real client. A blocked client must not be made to work
by changing identity headers or rotating to another key on the same membership.

The guidelines permit normal use across the user's devices and coding tools.
They also distinguish a forward network proxy from account sharing. These facts
do not establish blanket approval or prohibition of tmax's single-user local
relay. That route must retain an explicit conditional or unknown support result
until its client identity, use, and forwarding contract are qualified. A
successful HTTP request alone is not provider authorization for the relay.

The [official overview](https://www.kimi.com/code/docs/en/index.html#configuring-in-third-party-tools)
explicitly requires the tool's real User-Agent. It does not publish a universal
recognized-client string, an exhaustive allowlist, or a required conversation
header. Native client/version qualification must not be replaced by a fabricated
approved User-Agent. Identity preservation covers main, tool-result,
continuation, and auxiliary calls under the
[shared identity contract](../spec.md#client-and-conversation-identity); it does
not import Go's special header requirement.

## Membership quota and renewal

The [membership documentation](https://www.kimi.com/code/docs/en/kimi-code/membership.html)
states that logged-in devices, official clients, and membership API keys share
allowance. Extra local provider IDs, keys, agents, or routes must not multiply
that allowance. Unknown upstream identity must remain unverified rather than
being used to invent separate billing scopes.

Quota inspection must preserve the actual windows and reset information
reported for the selected membership, not infer them from a plan name or a
fixed plan-generation classification. Renewal, expiry, quota reset, and native
credential expiry remain separate facts. Local traffic is not account-wide
membership usage. Renewal and reset completion remain unknown; a quota read
reports windows, not renewal.

The [Console](https://www.kimi.com/code/console) and
[Subscription quota page](https://www.kimi.com/membership/subscription?tab=quota)
provide user-facing membership and quota information. They do not establish a
subscription-management or renewal API for tmax. Inspection keeps the dashboard
links and optional user-maintained subscription information.

The integration's quota operation is
`GET https://api.kimi.com/coding/v1/usages` for `kimi-code-plan-cn`, or
`GET https://api.kimi.ai/coding/v1/usages` for `kimi-code-plan-global`. It sends
the selected membership key in `Authorization: Bearer`. One bounded read on
2026-10-27 returned HTTP 200 with a weekly usage object and separate window
limits (five-hour and weekly) carrying provider-reported limit/remaining
counters and reset timestamps. This read is a non-inference observation
operation; no inference was issued to obtain it.

The counter unit is unpublished, so counters must not be labeled requests or
tokens. Overlapping legacy and new fields must not be counted as two allowances.
The shared membership allowance is not multiplied per local provider or key.
The read qualifies display observations only, never renewal facts or rule inputs.
tmax must not scrape browser sessions or import CLI tokens to fill quota fields.

### Tolerant quota reads

The weekly `usage` object and each `limits` row are independent sources of
partial information. Missing fields do not imply zero or invalidate other fields.

| Returned information | Observable behavior |
| --- | --- |
| Recognizable `limit`, `used`, or `remaining` decimal-string counter | Preserve the exact reported string independently; any one counter is useful. Do not require the other counters or calculate missing values. |
| Missing, null, negative, or otherwise unrecognized counter | Leave that field unknown without rejecting other counters or windows. Preserve explicit zero. |
| Recognizable `resetTime` | Preserve the reported timestamp independently. An absent or unusable reset does not discard valid counters. A reset alone is not an allowance observation. |
| Missing or unusable `usage`, `limits`, or individual row | Continue reading the other sources; omit rows with no recognizable counters. |
| Partial window description | Preserve recognizable `duration` and `timeUnit` fields independently; leave the rest unknown. Do not infer window scope, merge repeated descriptions, or sum their counters. |
| Unknown or identity/billing-control fields | Ignore them; retain only the qualified allowance fields. |
| At least one recognizable counter | Publish the new partial observation through the existing CLI and Web detail surfaces. Missing fields remain absent, not copied from an older observation. |
| Request failure, unreadable JSON, or no recognizable counters anywhere | Report refresh failure and retain the previous dated observation. Do not report zero allowance or disable inference. |

The existing bounded read and deadline still apply; no complete-response schema
or fixed number of windows is required. Source, retrieval time, and unknown
freshness remain visible for partial observations.

The membership documentation also describes Extra Usage, managed by the user
in the upstream Console or Subscription page, not by tmax. Included quota
exhaustion alone does not prove that inference is unavailable. Routing follows
the [shared selection contract](../../model-routing/spec.md#selection-and-continuity)
using effective provider evidence, without requiring access to billing controls.

## Provider-specific failures and recovery

The [official error reference](https://www.kimi.com/code/docs/en/kimi-code/error-reference.html)
uses the same HTTP status for different causes. Native agents can also re-wrap
errors. Classification must use available upstream text and context, not the
status alone. tmax must preserve an unrecognized result as unknown rather than
inventing an error code or converting it into zero quota.

| Documented symptom | Required user result and recovery |
| --- | --- |
| `The API Key appears to be invalid or may have expired` or `Invalid Authentication` (401) | Report the affected authentication failure. Direct the user to verify the Console key, selected Coding product/region, authentication format, and effective native credential source. An explicit corrected or replacement key can recover access. Do not try Open Platform billing or another regional host. |
| A 401 message denies `k3`, HighSpeed, a larger context window, or the supplied model ID | Report the denied model, context capability, or invalid model ID. Do not label the whole membership key revoked. Offer correction of the explicit model configuration or the provider's plan page. Do not silently substitute a model, shrink context, or retry unchanged. |
| `unable to verify your membership benefits` (402) | Report that membership verification is temporarily unavailable. Retain the key and recorded subscription, and link to the Console. Do not mark the subscription cancelled or replace credentials automatically. |
| A provider-reported usage-limit denial (403) | Show the actual denied window and any provider-sourced reset information, and link to the Console. Do not equate it with key revocation or assume another key provides fresh allowance. |
| `You've reached your concurrent request limit` (403) | Report the documented account risk-control/concurrency restriction, not zero remaining quota. Refer to the community guidelines for recovery. Do not promise a quota reset or bypass the restriction with client spoofing or key changes. |
| `We're receiving too many requests` or `The engine is currently overloaded` (429) | Report a transient request or capacity limit, not depleted membership or revoked authentication. Recovery follows the [shared request lifecycle](../../product/spec.md#local-request-admission). |
| `method not found`, or a model-not-found/permission-denied result (404) | Identify the configured Coding path, protocol, and model for correction. Preserve ambiguity when the response does not distinguish a missing model from denied access. Do not redirect to Open Platform. |
| Missing reasoning history, an exceeded context limit, or another request-format error (400) | Report the native request/capability incompatibility. Leave request and conversation repair to the agent or user. Do not fabricate missing reasoning data, rewrite history, or translate the request. |
| Native `kimi-code` session expiry or refresh failure | Report the login-source target and failed native operation. Recovery uses that source's supported refresh or sign-in flow. Do not fall back to a membership key, another account, or Kimi CLI token extraction. |

## Observable acceptance

- Setup distinguishes a Console membership key, an Open Platform key, and native
  omp login. A native command named `login` does not alter the selected method.
  An unsupported tmax-owned or cross-agent login path has a named unavailable
  result rather than a successful placeholder.
- Provider inspection shows the selected product-qualified integration, resolved
  per-protocol Coding bases, declared operations, login source or key reference,
  and qualification status. Direct and gateway use share the map. An invalid
  integration, undeclared method/path, or unsupported original-resource binding
  cannot trigger key migration, path guessing, regional failover, or platform billing.
- Without a supported non-inference identity check, a provider can retain an
  unknown identity without a paid probe. Local readiness, network failure, and a
  successful explicit inference probe remain distinct observable results.
- Key replacement retains a same-identity provider's subscription entries while
  invalidating old observations. Unknown identity stays unverified. Another
  membership requires another provider, and unsafe old-session continuation is
  blocked.
- Each advertised direct agent path works in its qualified native scope without
  a tmax inference dependency. Conflicting native credential precedence or a
  server-wide account switch is visible; an unisolatable authorization is blocked.
- A gateway route reports its own identity, use, protocol, and continuity
  qualification. Documented direct key access does not imply relay approval.
  The route cannot require spoofing or account sharing to work.
- Usage inspection preserves shared membership allowance and the selected
  membership's actual quota windows. Quota-window facts are observable through
  the qualified regional usage read; renewal and reset completion remain
  unknown with dashboard links and optional user-maintained subscription
  information. Local traffic cannot masquerade as account-wide usage. Included
  quota exhaustion does not by itself make a candidate ineligible.
- The documented failure classes remain distinguishable even when they share
  HTTP 401 or 403. Recovery identifies the actual key, entitlement, window,
  temporary membership check, native login source, or risk-control problem without
  bypassing client restrictions.

## Evidence status and official references

The linked Kimi documentation was checked on **2026-10-23**. It establishes
provider-documented product boundaries, membership-key scenarios, regional
endpoints, usage restrictions, and error meanings. The native Kimi CLI guide
establishes only that client's OAuth flow. The pi and omp links establish
agent-project capabilities, not upstream endorsement.

The provider and agent references are linked with the claims they support above.
The Console and Subscription links are user-facing sources, not established
machine-readable inspection APIs.

One bounded runtime scenario on **2026-10-23** used real pi **1.2.0** through
the tmax `kimi-coding-plan` China Messages route with exact `kimi-for-coding`.
The user selected China explicitly before credentialed requests. Native stream
events, one harmless tool-result round trip, and one same-process continuation
completed. A documented request-budget hook changed only existing `max_tokens`
from 32768 to 512; native model capacity and reasoning support remained intact.
This qualifies neither stock-default generation budgets nor other models,
regions, transports, or native reasoning controls.

HTTP statuses and terminal upstream request history were not collected because
the acceptance harness used an incorrect history filter. Native completion is
not a substitute for that missing evidence. This bounded single-user relay
result does not establish vendor relay authorization, account identity, plan,
quota, renewal, or native OAuth forwarding. No inference was issued during the
quota read. One bounded quota HTTP read was performed on **2026-10-27**
against the selected regional coding base; it was a non-inference GET that
returned the window limits recorded above. It establishes no schema version,
no counter unit, no renewal fact, and no rule-input capability.
Supported agent-version ranges remain unset. Qualification records must follow
the [shared support contract](../spec.md#support-contract) and keep provider,
agent, and runtime evidence separate.
