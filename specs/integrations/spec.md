# Agent and provider integration support

This document owns shared integration boundaries, support qualification, and the
index of provider-specific contracts under the [product specification](../product/spec.md).
Each child contract owns its service product's exceptional behavior and evidence.
These contracts do not claim that integrations have been implemented or exercised.

A Provider integration is a qualified behavior definition/type, not a managed
upstream access instance or universal service identity. A Provider selects it;
multiple Providers may select the same Provider integration while retaining
independent endpoints, authentication configuration, optional account/subscription
facts, and Models. The `custom` Provider integration can carry unrelated explicitly
configured services; its shared type neither identifies those services nor
qualifies their behavior.

## Support contract

Codex CLI, stock pi, omp, OpenCode, NousResearch Hermes Agent, and DeepSeek Harness
(DSH) are separate integration targets. A provider name and a compatible-looking base URL are not
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

Configuration application treats an unverified version or unavailable version
probe as advisory. It may proceed using the supported schema without claiming
runtime verification. A known incompatible schema (for example OpenCode 1 versus
2), missing executable, invalid selection, or unavailable credential still blocks
application with an actionable reason. Authentication qualification is separate.

## Agent boundaries

| Agent | Documented integration surface | Direct-configuration boundary | Same-protocol gateway boundary |
| --- | --- | --- | --- |
| Codex CLI | Custom model providers, base URLs, model catalogs, API-key sources, native ChatGPT sign-in | Respect user/profile scope, managed restrictions and project trust; do not assume provider/auth settings can be changed in project config | Responses-native route; incremental streaming, tool-call round trips, continuation semantics, and native-login gateway authentication must be qualified |
| stock pi | Agent-directory `models.json`, provider/model overrides, native authentication, model settings | Detect the active agent directory and precedence; changing a built-in base URL can retain its existing credentials | Select the declared native `api`; stock pi configuration is not omp configuration |
| omp | Active-profile `models.yml`/`models.yaml`, provider/model overrides, model-role settings, native authentication | Respect profile/environment path overrides and credential precedence; `auth: oauth` does not itself create credentials or register a sign-in flow | Match the declared native API and transport; an existing omp auth gateway is evidence of an extension point, not a mandatory tmax dependency |
| OpenCode | Provider packages, custom base URLs/models, multiple saved accounts per provider, local configuration and account storage | Detect the versioned schema and server-scoped active account. V1 and V2 provider shapes differ; credential stores and effective policy cannot be inferred from one config file | The provider package selects the protocol. Model picker filtering is not gateway authorization |
| Hermes Agent | Profile-aware configuration, provider/model selection, native login and key sources, per-provider transport | Respect `HERMES_HOME`, native secret storage and managed configuration precedence | Match its selected Chat Completions, Messages, or Codex/Responses transport; a URL override alone does not validate that transport |
| DSH | HOME `cordis.patch.yml`, independent `llm-tmax` provider/model routes, home `tmax-dsh.mjs`, and adjacent home credential references | Require an existing DSH home scope; preserve user `llm-pi-ai`, unrelated HOME-level rows, provider fields, and expressions; do not enumerate or manage profiles or inspect bundles | Register configured routes in DSH's common LLM runtime with native `openai-completions`, `openai-responses`, or `anthropic-messages`; configuration support alone does not establish native session identity or a provider-qualified route |

Agent-owned credential stores must be managed through supported agent operations
or a version-qualified contract. tmax must not assume that changing a database
row or deleting one auth file removes every credential source. Disabling a model
in an agent's picker must not be described as preventing all direct calls.

For a concrete model, native application must preserve known effective
context/output limits, reasoning support, and input modalities where the
version-qualified native schema can express them. Unknown metadata must remain
unknown rather than receive fabricated defaults. This is model-capability
mapping, not a per-request generation budget or permission to change native
preferences. The [request-parameter contract](../model-routing/spec.md#request-parameters)
owns preservation of sampling, reasoning effort, and output-budget settings.
Gateway aliases follow the [alias reasoning capability](#alias-reasoning-capability)
rule below, not a current-winner snapshot.

Effective `reasoning_options` entries with `type = "effort"` and explicit
`values` declare a concrete model's actual upstream reasoning selector
capabilities. Only these entries declare upstream effort names; other entry
types and missing `values` declare none. `reasoning = true` declares support
only and synthesizes no names. An explicit `reasoning = false` stays explicit
and suppresses the effort projection. Missing effort declarations stay unknown.
For a concrete gateway entry, derive caller-facing inputs from these identities
plus explicit protocol-scoped [model maps](../configuration/spec.md#model-effort-maps),
whose targets must be declared upstream values. An explicit key overrides
identity for that name. Direct native entries project the actual upstream
declarations, not gateway maps. Gateway aliases use the intersection below.

| Agent | Projected native fields | Exact-name rule |
| --- | --- | --- |
| stock pi | `thinkingLevelMap` | Every native thinking-level key is written explicitly, holding its exact declared name or null. |
| omp | `thinking` with `mode: "effort"` and `efforts` | `efforts` holds the declared names that exactly match the native effort vocabulary. |
| OpenCode, Chat Completions and Responses | `variants` entries with `id` and `settings.reasoningEffort` | Each declared name becomes one entry; no catalog field is pasted into the custom model. |
| OpenCode, Messages | `variants` entries with `id` and `settings.effort` | Each declared name becomes one entry holding only `id` and that setting; no other variant field or thinking flag is written. |
| DSH | Model `reasoningEfforts` map, or `false` for explicit non-reasoning capability | Exact declared keys from `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max` map to the same wire strings; absent levels are omitted. A map must offer a level beyond `off`. |
| Codex CLI, Hermes Agent | None | These agents have no native reasoning-effort selector to project. |

Native projection maps its derived input names exactly to representable native
names; it never copies upstream map targets into a gateway native selector or
performs model-specific translation there. A partial projection keeps every
unrepresentable input visible as omitted. A projection with no usable native
level is unrepresentable: it blocks that agent's application before native writes
and stays visible in inspection. These fields declare selector capability only.
They set no default effort, level, budget, thinking switch, or request value.
Native capability declarations can affect the tool's model representation;
the [DSH alias limitation](#alias-reasoning-capability) makes that effect explicit.
The native tool keeps ownership of thinking enablement, defaults, and request
preferences under the
[request-parameter contract](../model-routing/spec.md#request-parameters).

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

### Alias reasoning capability

Native application may project common input modalities and reasoning-effort
choices for a gateway alias. Both use every explicitly configured candidate,
including candidates whose provider or model is disabled. Availability, priority,
routing rules, and the current winner must not narrow that set.

#### Common input modalities

| Condition | Required behavior |
| --- | --- |
| Evidence | Use accepted concrete-model catalog metadata plus local overrides. Do not infer support from model names, another account, native defaults, or live availability. |
| Every candidate declares `modalities.input` | Intersect the complete lists, deduplicate, and preserve the first candidate's declaration order. Publish only that common list. An explicit empty list or disjoint lists gives a known empty intersection. |
| Any candidate lacks the declaration | Common support is unknown; omit derived input metadata. Do not substitute text-only, an empty list, or another candidate's declaration. Effort derivation remains independent. |
| Inspection | Expose common support as `declared` with an array (including empty), or `unknown` with null, and every qualified candidate's effective input list or null. Retain accepted metadata provenance. |
| Native representation | Use the same version-qualified input mapping as concrete models. Report projected fields, omitted modalities, unsupported native fields, unknown fallback, or an unrepresentable declaration separately from effort projection. |
| Reapplication | Replace or withdraw obsolete managed input declarations, never retain stale managed image support. Preserve unrelated settings and reject conflicting edits. For an already managed DSH models array, omission removes selected rows' obsolete `input`; on first application, unrelated existing native declarations remain native-owned. Native inheritance is not a claim of text-only support. |
| Empty native representation | If the native schema would reinterpret a known empty set as inheritance, block that target's application before writes rather than invent support. |

This is a declaration for native consumers, not an extra gateway request
allowlist. Existing non-effort request eligibility and retained-session checks
remain unchanged; effort still cannot filter or rerank candidates. No alias
capability override or new configuration setting is introduced.

#### Common effort choices

| Step | Required behavior |
| --- | --- |
| Candidate metadata | Use each concrete candidate's accepted catalog metadata with its local overrides applied. Do not use an unaccepted catalog snapshot or native fallback values. |
| Candidate input names | Collect declared `reasoning_options` values with `type = "effort"`, then add explicit map input keys for this protocol whose targets are positively declared upstream efforts. An explicit key overrides identity for that name. Effective `reasoning = false` or no declared effort names gives an empty set. A reasoning-support boolean alone supplies no names. |
| Fixed candidate effort | If `candidate_reasoning_effort` is configured, restrict that candidate's advertised input set to its fixed upstream name, without applying a model map. Fixed override behavior for other caller inputs is not evidence that they belong in its advertised set. Precedence is fixed > map > identity under the [request-parameter contract](../model-routing/spec.md#request-parameters). |
| Common choices | Intersect all candidate caller-facing input sets after maps and fixed restrictions. Remove duplicates while retaining deterministic declaration order. Never use the union, guess correspondences, or synthesize defaults. |
| Nonempty intersection | Supply `reasoning = true` and one `reasoning_options` entry with `type = "effort"` and the common `values` to the existing native capability mapping. |
| Empty intersection | Supply no alias reasoning metadata. Do not infer reasoning support or emit an alias-wide non-reasoning capability claim. |
| Other metadata | Omit alias limits, output modalities, tools, prices, transport claims, and other capability metadata. Never copy one candidate's or the current winner's metadata. |

Aliases use the same projected, partial, unavailable, blocked, or not-supported
outcomes as concrete models under the native mapping above. Capability projection
must not choose defaults, budgets, request preferences, or candidates, or inject
thinking switches. DSH's native capability encoding has the limitation below.
Only explicit model maps and fixed candidate effort have the narrow gateway
request-resolution behavior linked above; native selectors send input names to
the gateway, where the selected backend resolves them. Effort cannot filter or
rerank routing candidates.

For DSH, an empty or unknown common intersection must write native
`reasoningEfforts: false` to clear any previously advertised selector rather
than retain or inherit stale choices. Inspection still reports `unavailable`
with `reasoning_options_unavailable`. This value marks the alias as non-reasoning
in DSH's native model representation; it does not establish that its concrete
candidates lack reasoning support. It does not change saved effort preferences,
inject a request value, or repair a resulting native preference conflict.

After candidate membership, model maps, fixed candidate effort, or effective
capability metadata changes, inspect and explicitly reapply affected native
entries, then reload the native tool. Accepted catalog changes and local
overrides can change common choices; no metadata or map change automatically
rewrites native files.

The [capability design](design.md) separates declarations, common choices,
native representation, and request intent, including this lossy DSH boundary.

### DSH home boundary

| Surface | Required behavior |
| --- | --- |
| Scope | Existing DSH home directory itself. Discovery uses nonblank `DSH_HOME`, otherwise the OS home directory's `.dsh`. An explicit target path selects the home directly; never infer it from a profile path. Do not enumerate, select, create, or manage profiles or inspect their bundles. |
| Home configuration | Edit the home `cordis.patch.yml` by entry identity and install the embedded plugin as its direct child `tmax-dsh.mjs`. The plugin entry has id `llm-tmax` and its `name` is the artifact's absolute path. Its `config.providers.<native-name>` holds each selected route. Never write profile files, launcher-owned `cordis.yml`, or legacy `settings.yaml`. Preserve unrelated HOME-level rows, comments, provider fields, and opaque expressions; an uneditable owned branch blocks rather than being rewritten. |
| Independent plugin | `llm-tmax` registers only its configured provider routes and reuses DSH's supported protocol adapter, not duplicated wire implementations or a second mount of `llm-pi-ai`. Provider configurations from every adapter register in the common LLM runtime. Provider route names must be globally unique across active adapters; distinct plugin identities do not isolate names or authorize automatic merging. The tmax-owned plugin configuration does not add a second configurable-provider directory or catalog. Preserve both direct upstream and gateway entries. |
| Installation | Apply uses the embedded plugin artifact and DSH's native loader resolution for installed dependencies. It does not run npm installation or change profiles, presets, or bundle composition. |
| Layering | Bundle patches → profile patch → home patch → command-line overlays. A home entry `config` replaces the whole lower-layer configuration for that same entry, not a deep merge. The independent `llm-tmax` entry leaves user `llm-pi-ai` routes and settings untouched; it does not copy or merge their configuration. Home tmax routes are common to profiles using that home. An explicitly managed home default may still shadow lower-layer defaults; runtime overlays remain outside tmax's ownership. |
| Credential handoff | `llm-tmax` uses `apiKeyEnv: TMAX_<stable hash>` in the home patch. Executed apply resolves the referenced tmax key into that home's adjacent current-user-restricted `.credentials.yaml`, `version: 1`, under `refs`; preserve unrelated references and credential records. Accept empty stores and stamp version 1; nonempty stores without version 1 block. Apply and restore executions hold the native sibling `.credentials.yaml.lock` writer lock from initial credential read through commit. An existing lock blocks immediately with `native_credentials_busy`, without changing native files or the external lock; never overwrite or reclaim it. Release owned locks on success and failure. Dry-run/inspect never create locks, resolve secrets, or return values. The native store is shared across profiles, not another tmax authority. |
| Headless credentials | When handing off a key, explicitly enable the native `credentials` service in the HOME patch, including for headless consumption, without inspecting a bundle. Preserve unrelated HOME-level service settings; do not require a secret-bearing environment or launch wrapper. |
| Selection | Model-only application writes no `agent-default-model` entry, default reasoning effort, route `reasoning`, or thinking budget. An explicitly selected default changes only its provider/model reference, preserving existing HOME-level fields, including effort. Lower-layer defaults are not automatically imported into a new home entry `config`. No native model roles or native interactive login are qualified. |
| Output metadata | Leave DSH output capability unprojected: explicit native `maxTokens` also sets a per-request default. Preserve existing HOME-level fields on retained selected models by exact model ID; explicitly projected fields override them, while membership and order remain the explicit selection. Do not import shadowed profile settings. |
| Protocol | Support all three native protocol mappings without translation. A provider route has one protocol and base; incompatible selections in one route block rather than silently changing transport or dropping models. Existing `modelOverrides` on a selected HOME provider route block before writes rather than emitting incompatible `models` plus `modelOverrides`. |
| Runtime qualification | Configuration/source qualification is not inference evidence. Unknown DSH gateway context does not establish a reusable conversation identity. Do not fabricate session headers, static IDs, or another client's User-Agent. Direct OpenCode Go access remains unqualified without its required genuine session mechanism. |
| Recovery | Capture the plugin artifact, patch insertion, managed provider fields, native home credential references, and required credential-service enablement. Restore only owned changes to their captured baselines; preserve later unrelated edits and refuse conflicting owned edits under the common apply/restore contract. If later unowned additions prevent removing a newly inserted plugin, block before native writes rather than leaving an invalid entry or deleting its artifact. Recognize captured pre-write artifact content during interrupted transaction recovery. `llm-tmax` is the sole managed adapter entry; do not maintain or migrate an earlier `llm-pi-ai` application format. User-owned `llm-pi-ai` configuration remains untouched. |

The [versioned DSH reference](../../docs/agents/dsh.md) records source evidence
and its limits without establishing an untested version range or provider access.

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
| StepFun Step Plan Token Plan | [StepFun Token Plan](stepfun-token-plan/spec.md) | Dedicated plan paths, `step-5-preview` scope, monthly Credit, and undocumented plan usage read |
| MiniMax M Plan | [MiniMax M Plan](minimax-m-plan/spec.md) | Regional plan hosts, shared `general` allowance pool, percent-only quota display, and unpublished counter semantics |

Each provider contract must define:

- Its service/access product, exclusions, credential source, owner, refresh or
  replacement behavior, and how upstream identity can remain unknown.
- Concrete direct and gateway scenarios, the finite integration/protocol endpoint map,
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

A known integration identifies one access product and one fixed protocol-to-base
map. Regional products are distinct integration IDs, not another provider field.
Local provider names remain arbitrary labels and never select a destination.
Direct agent configuration and gateway forwarding use the same map.

| Integration ID | Product destination | Owning contract |
| --- | --- | --- |
| `kimi-code-plan-cn` | Kimi Coding Plan, China | [Kimi](kimi-coding-plan/spec.md#endpoints-and-native-protocols) |
| `kimi-code-plan-global` | Kimi Coding Plan, overseas | [Kimi](kimi-coding-plan/spec.md#endpoints-and-native-protocols) |
| `minimax-cn-coding-plan` | MiniMax M/Token Plan, China | [MiniMax](minimax-m-plan/spec.md#endpoints-and-native-protocols) |
| `minimax-coding-plan` | MiniMax M/Token Plan, global | [MiniMax](minimax-m-plan/spec.md#endpoints-and-native-protocols) |

These IDs match the corresponding [models.dev directory](https://models.dev/api.json)
entries, but catalog `api` and `npm` values do not override qualified operation
or authentication contracts. Other supported integration IDs remain unchanged.
Known integrations reject supplied `endpoints`; desired configuration rejects
`region` and the former unqualified Kimi/MiniMax integration IDs. No alias or
implicit default chooses between regional products.

Changing integration is an explicit destination change, not proof of key
portability or safe conversation continuation. Missing protocol entries remain
unavailable. An upstream failure never selects another region or product.

A custom provider must instead declare an explicit base for each protocol it
uses within that one provider instance. Its omitted protocols remain unavailable.
One custom base must not stand in for another protocol, even when an SDK calls
both surfaces OpenAI-compatible. The resolver must not guess protocol paths,
append or remove `/v1`, or probe alternative bases. The integration's declared
operation path is appended only at its documented base/path boundary; this is
not permission to repair the supplied base. The
[configuration endpoint schema](../configuration/spec.md#provider-endpoints)
owns the `endpoints` field for custom integrations only.

Model protocol availability follows the provider's resolved endpoints, not a
per-model approval list. An unverified model/protocol combination must not block
publication, native configuration, selection, or dispatch solely because tmax
lacks upstream qualification evidence. Such evidence remains informational, not
proof of support. The upstream decides whether the exact model accepts a request;
tmax preserves its response without protocol conversion, implicit model changes,
or fallback retries. Agent wire-format requirements, declared operation/transport
boundaries, and original session/authorization bindings still apply.

Protocol declarations advertise availability, not a preferred request protocol.
The client chooses its native protocol explicitly; gateway dispatch follows that
protocol or rejects the request. Configuration guidance and new Web choices
prefer Responses when both the upstream declaration and the client support it.
This preference must not change existing selections, native defaults, or requests.

Every endpoint consumer must use this resolution: direct configuration, gateway
dispatch, model compatibility checks, and provider discovery. Qualified
provider-specific discovery, identity, quota, and balance operations must use
their integration-declared destinations for the selected access product. They
must not derive an undeclared metadata URL from an inference base.
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
integration, protocol endpoint, and authorization. Its integration must qualify how
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
qualified native sign-in. Native model application requires the selections, credentials, and adapter
requirements defined by the [Agent contract](../cli/agents/spec.md#agent-application-and-recovery). Every present model, default,
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
transport set, resolved integration/protocol bases, streaming, a tool-call/result
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
