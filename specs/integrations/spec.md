# Agent and provider integration support

This document owns shared integration boundaries, support qualification, and the
index of provider-specific contracts under the [product specification](../product/spec.md).
Each child contract owns its service product's exceptional behavior and evidence.
These contracts do not claim that integrations have been implemented or exercised.

## Support contract

Codex CLI, stock pi, omp, OpenCode, and NousResearch Hermes Agent are separate
integration targets. A provider name and a compatible-looking base URL are not
sufficient to claim support. Each supported agent configuration must declare the agent
version, configuration scope, account-selection scope, credential ownership,
native provider source, protocol, transport, and required model capabilities.

Compatibility must distinguish:

- Documented: an upstream source describes the capability.
- Verified: the actual supported version and route passed an end-to-end check.
- Conditional: a named restriction or capability requirement must be satisfied.
- Unknown: available evidence is insufficient; this is not a claim of prohibition.
- Incompatible: an identified requirement cannot be satisfied by this agent configuration.

Shared agent documentation was consulted on 2026-10-22. Provider evidence, its
consultation date, and any bounded runtime qualification belong to each child
contract. Documentation, local native-schema checks, and provider-authorized
runtime evidence must remain distinct. None establishes an untested version range.

## Agent boundaries

| Agent | Documented integration surface | Direct-configuration boundary | Same-protocol gateway boundary |
| --- | --- | --- | --- |
| Codex CLI | Custom model providers, base URLs, model catalogs, API-key sources, native ChatGPT sign-in | Respect user/profile scope, managed restrictions and project trust; do not assume provider/auth settings can be changed in project config | Responses-native route; incremental streaming, tool-call round trips, continuation semantics, and native-login gateway authentication must be qualified |
| stock pi | Agent-directory `models.json`, provider/model overrides, native authentication, model settings | Detect the active agent directory and precedence; changing a built-in base URL can retain its existing credentials | Select the declared native `api`; stock pi configuration is not omp configuration |
| omp | Active-profile `models.yml`/`models.yaml`, provider/model overrides, model-role settings, native authentication | Respect profile/environment path overrides and credential precedence; `auth: oauth` does not itself create credentials or register a sign-in flow | Match the declared native API and transport; an existing omp auth gateway is evidence of an extension point, not a mandatory tmax dependency |
| OpenCode | Provider packages, custom base URLs/models, multiple saved accounts per provider, local configuration and account storage | Detect the versioned schema and server-scoped active account. V1 and V2 provider shapes differ; credential stores and effective policy cannot be inferred from one config file | The provider package selects the protocol. Model picker filtering is not gateway authorization |
| Hermes Agent | Profile-aware configuration, provider/model selection, native login and key sources, per-provider transport | Respect `HERMES_HOME`, native secret storage and managed configuration precedence | Match its selected Chat Completions, Messages, or Codex/Responses transport; a URL override alone does not validate that transport |

Agent-owned credential stores must be managed through supported agent operations
or a version-qualified contract. tmax must not assume that changing a database
row or deleting one auth file removes every credential source. Disabling a model
in an agent's picker must not be described as preventing all direct calls.

For a concrete offering, native application must preserve known effective
context/output limits, reasoning support, and input modalities where the
version-qualified native schema can express them. Unknown metadata must remain
unknown rather than receive fabricated defaults. This is model-capability
mapping, not a per-request generation budget or permission to change native
preferences. The [request-parameter contract](../model-routing/spec.md#request-parameters)
owns preservation of sampling, reasoning effort, and output-budget settings.
This rule supplies no alias capability aggregation or current-winner snapshot.

The [versioned native field references](../../docs/agents/README.md#qualification-and-evidence)
record external model/authentication schemas and evidence for each Agent.
They do not replace these contracts or establish provider entitlement.

Integration setup and recovery must be reachable through explicit CLI arguments,
including the native target and authentication owner; callers must not author
configuration documents or use the Web first. tmax's sole private authority is
`~/.tmax/tmax.toml`, independent of the working directory or calling agent. All
tmax-owned persistent data, caches, logs, recovery data, and secret files remain
under `~/.tmax` in the OS user's home, including on Windows. API-key and
credential-bearing recovery payloads are plaintext files restricted to the current
OS account; ordinary configuration stores secret references, not key bytes.
File permissions are not encryption. Never place credentials in argv, environment
variables, TOML, stdout, logs, or ordinary inspection. Agent-owned configuration
and supported native credential stores retain their native locations and ownership;
those native scopes are not alternative tmax homes or configuration authorities.
Upstream login and refresh remain with their declared owner.

tmax-owned credential work follows
[credential refresh coordination](../product/spec.md#credential-refresh-coordination)
without changing native refresh ownership. tmax-owned upstream traffic follows
the [outbound network policy](../product/spec.md#outbound-network-policy) and
[transport deadlines](../product/spec.md#transport-deadlines). Inference admission,
buffering, and backpressure follow
[local request admission](../product/spec.md#local-request-admission).

Operations follow the
[CLI dry-run and execution contract](../cli/spec.md#dry-run-and-execution):
optional dry runs inspect permitted local inputs without effects, secret reads,
login, upstream calls, or reservations. Execution checks current inputs and
rejects conflicting changes during the operation before overwrite. No preview,
confirmation token, or claim of human approval is a prerequisite. Explicit
business inputs specify requested scope and identity claims; they do not verify
identity, qualification, or safe conversation continuation.

Sources:

- [Codex authentication](https://developers.openai.com/codex/auth),
  [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference),
  [gateway compatibility](https://learn.chatgpt.com/docs/enterprise/gateway-compatibility),
  and [ChatGPT through a gateway](https://learn.chatgpt.com/docs/enterprise/sign-in-with-chatgpt-through-a-gateway).
- [stock pi repository](https://github.com/earendil-works/pi) and its
  [model configuration documentation](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/models.md).
- [omp model configuration](https://github.com/can1357/oh-my-pi/blob/main/docs/models.md),
  [providers](https://github.com/can1357/oh-my-pi/blob/main/docs/providers.md), and
  [auth broker/gateway](https://github.com/can1357/oh-my-pi/blob/main/docs/auth-broker-gateway.md).
- [OpenCode configuration](https://opencode.ai/docs/config/),
  [V2 configuration](https://opencode.ai/v2/docs/config/),
  [V2 providers](https://opencode.ai/v2/docs/providers/), and
  [V2 provider accounts](https://opencode.ai/v2/docs/cli/providers/).
- [Hermes providers](https://hermes-agent.nousresearch.com/docs/integrations/providers),
  [configuration](https://hermes-agent.nousresearch.com/docs/user-guide/configuration),
  and [managed scope](https://hermes-agent.nousresearch.com/docs/user-guide/managed-scope).

## Provider-specific contracts

Ordinary OpenAI- or Anthropic-compatible API-key access reuses the shared
configuration, secret-handling, agent, and same-protocol forwarding contracts.
A different hostname or catalog entry alone does not require another spec.
Compatibility must still be qualified; a familiar protocol name is not proof
that the entire upstream API is supported.

A service product must have its own nested `spec.md` before support is advertised
when it adds any distinct authentication or refresh ownership, subscription
entitlement, native-client restriction, session identity, protocol semantics, or
account/quota behavior. API-key authentication does not exempt an integration
with those differences. Specs are per access product, not per user's provider
instance; two local instances of the same product use the same contract.

| Service product | Authoritative contract | Owned special behavior |
| --- | --- | --- |
| ChatGPT / Codex subscription | [Codex subscription](codex-subscription/spec.md) | Native login sources, workspace identity, subscription gateway, and allowance evidence |
| OpenCode Go / Go Plus | [OpenCode Go](opencode-go/spec.md) | Console workspace access, model endpoints, client/session identity, and Go billing |
| Kimi Coding Plan | [Kimi Coding Plan](kimi-coding-plan/spec.md) | Membership versus platform access, regional endpoints, native login, usage restrictions, and shared allowance |
| Official DeepSeek API | [DeepSeek API](deepseek/spec.md) | Stateless Responses, balance and account-limit semantics, and pricing-calendar evidence |
| Domestic GLM Coding Plan | [GLM Coding Plan](glm-coding-plan/spec.md) | Domestic plan keys, approved tools, protocol-specific authorization, and unqualified monitor-field semantics |
| StepFun Step Plan Token Plan | [StepFun Token Plan](stepfun-token-plan/spec.md) | Dedicated plan paths, `step-5-preview` scope, monthly Credit, and unavailable plan-quota API qualification |

Each provider contract must define:

- Its service/access product, exclusions, credential source, owner, refresh or
  replacement behavior, and how upstream identity can remain unknown.
- Concrete direct and gateway scenarios, the finite region/protocol endpoint map,
  native scope, and conditional or unavailable combinations.
- Supported HTTP method/path operations and transports, model-selection inputs,
  required client/session identity, and original-resource binding requirements
  under [the operation contract](#supported-operations-and-resource-identity).
- Session and multi-instance boundaries, including interactions with native
  account selection and the shared conversation-continuity rules.
- Sources for the product's existing quota, renewal, subscription-information,
  and API-balance scope. Provider-console controls are not additional tmax features.
- Provider-specific failures, affected scope, and actionable recovery without
  fabricated APIs, silent identity changes, or billing-product substitution.
- Observable acceptance requirements and dated official or agent-project
  evidence, separately from actual runtime verification.

Shared product, configuration, and routing rules remain authoritative. Child
contracts link to them instead of copying them or introducing parallel schemas.
The directory name is a documentation topic, not a new local provider ID,
runtime integration ID, resource type, plugin interface, or credential pool.
Additional providers with only catalog metadata need no empty contract template.
Provider-specific detail belongs here only when it changes tmax setup, request
handling, routing, observations in the product scope, or recovery. Link to the
vendor for console operations and plan administration instead of reproducing its
manual. An excluded capability needs no setup choice, dashboard field, or
acceptance item merely to demonstrate that it is unsupported.

Setup must offer authentication methods for the selected service product and
qualified agent, not an unconditional login/API-key toggle. The same brand can
sell unrelated access products; selecting an endpoint must not silently exchange
subscription entitlement for platform billing. A documented client setup is
evidence for qualification, not permission to advertise every model, transport,
or alias continuation path as verified. Model lists, plan prices, quota
windows, and reset policies remain sourced provider data, not product constants.

### Client and conversation identity

| Condition | Shared requirement |
| --- | --- |
| A service requires a recognized native client or session header. | Its child contract owns the exact requirement and source-qualified versions. Protocol compatibility alone must not admit the route. |
| Direct native configuration | The actual agent must satisfy the service's identity requirements without tmax on the inference path. |
| Same-protocol forwarding | Retain the truthful qualified native client identity and genuine session semantics across main requests, tool results, continuations, and auxiliary requests. Preserve native parent/subsession distinctions where present. |
| Identity is missing, conflicting, or unqualified for a required path. | Block that path with the missing prerequisite and supported recovery. Do not forge a User-Agent, manufacture a supported-client name, or replace a native conversation with a static provider-wide ID. |
| No special User-Agent value or session header is established by the consulted provider evidence. | State that limit in the child contract. Do not copy another product's requirements or claim that undisclosed runtime restrictions cannot exist. |

Native affinity for tmax routing remains governed by
[selection and continuity](../model-routing/spec.md#selection-and-continuity).
An upstream session header is not provider authentication, verified account
identity, or permission to select another authorization.

## Endpoint resolution

A known integration owns one finite map from its exact region IDs and native
protocols to upstream base URLs. Its provider contract owns the source evidence
and known limitations for that map. Direct agent configuration and gateway
forwarding must use the same resolved map entry. A region ID of `default` names
one published endpoint set; it is not a claim about service geography.

A known provider with regional alternatives must select a region explicitly or
supply an exact known per-protocol base that identifies one region
unambiguously. A singleton published `default` endpoint set may be selected
without a region or base selector. Multiple supplied bases and an explicit
region must select the same map entry. A missing selector for regional
alternatives, or an ambiguous, unknown, or conflicting selector, must be rejected
before upstream access. The resolver must not infer region from credentials,
catalog names, locale, network location, or a trial request.

For a known integration, supplied bases are selectors into the map, not arbitrary
overrides. Selecting a region resolves its declared protocol bases; it does not
qualify every model, operation, transport, or credential for those protocols.
Missing map entries remain unavailable. An upstream failure must not cause
regional failover, a different service product, or a protocol change.

A custom provider must instead declare an explicit base for each protocol it
uses within that one provider instance. Its omitted protocols remain unavailable.
One custom base must not stand in for another protocol, even when an SDK calls
both surfaces OpenAI-compatible. The resolver must not guess protocol paths,
append or remove `/v1`, or probe alternative bases. The integration's declared
operation path is appended only at its documented base/path boundary; this is
not permission to repair the supplied base. The
[configuration endpoint schema](../configuration/spec.md#provider-endpoints)
owns persisted `region` and `endpoints` fields.

Every endpoint consumer must use this resolution: direct configuration, gateway
dispatch, model compatibility checks, and provider discovery. Qualified
provider-specific discovery, identity, quota, and balance operations must use
their integration-declared destinations for the selected access product and
region. They must not derive an undeclared metadata URL from an inference base.
Catalog `api` metadata is endpoint guidance under the
[catalog contract](../model-routing/spec.md#model-catalog), not a universal base
or another endpoint-precedence layer.

Endpoint changes remain explicit configuration changes. The operation and its
optional dry run must report affected credential destinations, routes, native
agent settings, and retained sessions under the shared activation and
[continuity rules](../model-routing/spec.md#selection-and-continuity).

## Supported operations and resource identity

An integration must declare a finite set of supported operations. Each entry
must identify its HTTP method, exact path or constrained path template, native
protocol and transport, resolved destination, and purpose. A path template must
name its permitted variable positions; a prefix or catch-all is not an operation
declaration. The entry must also identify the required model selector or
original binding, provider authorization, and any qualified native
client/session headers. Discovery and management reads are separate operations,
not proof of inference or continuation support.

Query selectors are part of an operation's finite declaration, not arbitrary
passthrough. HTTP Messages creation at `POST /v1/messages` permits no query or
the sole exact query `beta=true`. A qualified route must preserve that selector
unchanged. It must reject unknown, duplicate, or conflicting query selectors
before dispatch. This SDK creation selector does not authorize other beta
features, batch/counting/resource operations, or every compatible upstream.
The version-qualified source is Anthropic TypeScript SDK **0.129.0**
[Messages creation](https://github.com/anthropics/anthropic-sdk-typescript/blob/sdk-v0.129.0/src/resources/beta/messages/messages.ts).

The gateway must reject an undeclared method/path or transport before forwarding.
A supported protocol does not authorize generic passthrough, arbitrary upstream
paths, or every operation in that protocol's API. Opaque native request fields
remain subject to the
[request-parameter contract](../model-routing/spec.md#request-parameters);
preserving them does not widen the operation set.

An ID-only operation, such as retrieval, deletion, or cancellation of an upstream
response or file, requires a reliable binding to the original provider instance,
region, protocol endpoint, and authorization. Its integration must qualify how
the native/session scope or another supported original binding establishes that
identity and any authorization-generation continuity. The resource ID, its
spelling, a caller-supplied account hint, current configuration defaults, or the
current alias winner alone cannot establish the binding.

If the original binding is missing, ambiguous, lost, or incompatible with the
available authorization, the gateway must reject the operation without upstream
access. It must not search providers, ask another endpoint which owns the ID,
rerank candidates, or try another credential. tmax must not build a cross-provider
resource directory or rewrite or encode upstream IDs to create one. Existing
session bindings remain governed by
[selection and continuity](../model-routing/spec.md#selection-and-continuity);
they do not imply support for undeclared resource operations.

Provider contracts must distinguish documented operation evidence from actual
runtime qualification. A documented creation path or returned resource ID does
not establish retrieval, cancellation, deletion, file management, or background
execution support. Unknown operation or identity requirements remain named
limitations, not generic gateway capabilities.

### Custom integration

`custom` is the registry ID for explicit same-protocol configuration of a service
that does not use a known integration's endpoint map. It permits
`chat-completions`, `responses`, and `messages` bases within one provider under
the [endpoint schema](../configuration/spec.md#provider-endpoints). It declares
API-key setup, not a generic login or refresh flow.

Its finite creation surface is:

| Native protocol | Operation beneath its explicit base | Transport |
| --- | --- | --- |
| `chat-completions` | `POST /chat/completions` | HTTP, with SSE when streaming is qualified |
| `responses` | `POST /responses` | HTTP, with SSE when streaming is qualified |
| `messages` | `POST /v1/messages`, with no query or sole `beta=true` | HTTP, with SSE when streaming is qualified |

Each operation requires its exact configured model and provider authorization,
plus any client/session requirements established by its qualified adapter.
Endpoint declaration expresses configuration intent, not evidence that the
custom service supports the operation or the client's required capabilities.
This generic declaration supplies no discovery, identity, quota, balance,
ID-only resource, background, or WebSocket operation. Such capabilities must
come from a separately qualified integration declaration, not a guessed path or
an arbitrary extension of `custom`.

Custom setup is a transport boundary, not a separate subscription product. A
known product's client, entitlement, and billing restrictions still apply when
its requests use `custom`; generic configuration must not erase them. A child
contract without a dedicated registry integration declares requirements, not a
new integration ID or implemented metadata/observation capability. A supplied
key's permission for one operation never authorizes every public API on that host.

## Rule input capabilities

An integration's qualified registry entry must distinguish supported runtime
input acquisition from model discovery, inference, authentication checks, quota
display, and catalog metadata. The
[input lifecycle contract](../model-routing/inputs/spec.md#input-values-and-acquisition)
owns normalized values, source separation, freshness, publication, and scheduling;
the [configuration schema](../configuration/spec.md#provider-input-declarations)
owns their declarations. These are target contracts, not additional verified
query capabilities.

Advertising `provider`-source input support requires evidence for each supported
kind and scope, model coverage, normalized percentage or comparable counter
fields and units, reset boundary, observation/validity semantics, credential
owner, and explicit versus noninteractive acquisition. A native dashboard,
display command, successful inference response, compatible protocol, or readable
token alone does not qualify a query or a parser. Missing capability remains
named unavailable; a well-typed input declaration does not authorize invented
endpoints, arbitrary URL execution, natural-language extraction, or credential
import.

The [Codex subscription contract](codex-subscription/spec.md#priority-rule-inputs)
distinguishes the bounded third-party reset-announcement source from unsupported
general account-quota queries. Manual inputs remain caller assertions under the
input contract, not advertised provider evidence. The registry's integration IDs
remain the authority for selecting these capabilities; documentation directory
names and local provider IDs do not create new IDs or provider products.

## Model-directory evidence

[models.dev/api.json](https://models.dev/api.json) provides catalog entries keyed
by service, with provider-local model IDs, display and optional canonical identity,
capability metadata, limits, and reference costs where present. This is metadata
evidence, not proof of account entitlement, permitted gateway use, or a working tmax
adapter. Local provider IDs remain independent of those external keys.

The [catalog contract](../model-routing/spec.md#model-catalog) owns field mapping,
source precedence, local overrides, and refresh behavior. Upstream documentation
and qualified integration evidence still govern authentication, protocol semantics,
subscription quotas, and time-dependent pricing. An SDK package name in the catalog
does not establish which native protocols a tmax instance can safely serve.

## Time-dependent pricing

Routing schedules must follow the specific provider product, not a vendor name.
The consulted sources establish different kinds of time-dependent charging:

- The [DeepSeek contract](deepseek/spec.md#pricing-calendar) owns its official
  API pricing-calendar evidence and the boundary between tariff data and routing.
- The [GLM Coding Plan contract](glm-coding-plan/spec.md#allowance-and-monitor-evidence)
  owns the distinction between subscription points, time-dependent point
  consumption, and standard API billing.
- The [StepFun Token Plan contract](stepfun-token-plan/spec.md#credit-and-quota-evidence)
  owns monthly Credit and the boundary of account-specific pricing evidence.

These sources explain why the [selection contract](../model-routing/spec.md)
supports time zones, weekday windows, and date overrides. Exact prices, effective
dates, and account eligibility remain sourced data, not automatic routing policy
or evidence of an implemented provider integration.

## Important limits

### Native account selection can be shared

OpenCode V2 documents multiple saved accounts for one provider, account switching
through `/connect` or `auth switch`, and automatic activation of the newest login.
Accounts and credentials belong to the server. tmax must treat a native account
switch as a change to that shared server scope, not assume project-local isolation.
The adapter must qualify account-selection scope for each supported version;
documented account storage does not prove concurrent per-session account selection.

Adding a native account can therefore change existing consumers before any model
configuration is written. The adapter must expose that effect; changing the
shared scope must be an explicitly requested operation effect, not an incidental
consequence of adding an isolated provider. Use supported native account
operations, not direct credential database edits. If the requested isolation is
unavailable, report that limitation instead of changing other consumers.

A native login-source target may be declared with empty model selections and no
default. Missing model application does not invalidate the source or prevent its
qualified native sign-in. Until required model selections, default, credentials,
and qualification are complete, the target remains explicitly unavailable for
native model application and gateway inference. Every present model, default,
and role reference must still resolve under the configuration contract.

### Provider-specific compatibility

The [provider contracts](#provider-specific-contracts) own exceptional native-login,
client-identity, protocol-state, and entitlement requirements. An agent must
satisfy the chosen access product's contract as well as its native protocol.
A successful model list, authentication check, or first response is not a
successful agent conversation. Tool, reasoning, streaming, and continuation
behavior must be qualified without erasing unknown or unsupported requirements.

## Qualification before advertising support

Each special provider must satisfy its own acceptance requirements in addition
to the shared qualification below. Missing evidence must remain a named support
limitation; it must not be replaced by a generic API-key success claim.

Each adapter must be exercised against supported agent versions for discovery,
effective-config precedence, apply/restore, pre-existing login handling, key
replacement, account-switch scope, adding an account without an unrequested
shared-scope switch, and conflicting user edits. Native sign-in qualification
must establish when active authorization changes, whether isolation or staging
preserves the old authorization, and how cancellation, rejected identity, and
local-save failure are recovered before and after that point. Exercise shared
native consumers and
report partial outcomes under the
[product authentication boundary](../product/spec.md#providers-and-authentication).
An upstream native provider entry must issue a real provider request without
the tmax server on the inference path.

Each advertised gateway route must be exercised for its declared method/path and
transport set, resolved region/protocol bases, streaming, a tool-call/result
round trip, cancellation, auth expiry or revocation, upstream errors, correct
account selection, configured-model boundaries, and supported continuation.
Qualification must cover rejection of undeclared operations and missing or conflicting endpoint
selectors. An advertised ID-only operation must establish its original binding;
an unknown binding must fail closed without resource lookup or ID rewriting.
Session qualification must cover missing bindings and the ability to distinguish
a trusted new session from resumed state; a qualified native reset or explicit
blocked limitation must replace an unverified promise that opening a new chat
repairs lost bindings. The record must distinguish provider evidence, agent
evidence, and actual tmax runtime evidence. Missing credentials or provider
permission is a named verification gap, not a simulated pass.
