# omp 18.12.6 native configuration

Version: **18.12.6**. The upstream tag `v18.12.6` resolves to
[`45c35e40c46166dc726ccc52f5d27f40f2449f99`](https://github.com/can1357/oh-my-pi/tree/45c35e40c46166dc726ccc52f5d27f40f2449f99).
Consulted 2026-10-23. This is not stock pi's JSON or auth-file schema. See the
[reference index](README.md) for shared evidence and security boundaries.

Applying selected provider models does not require a tmax default. Omit
`agent default set` to preserve the tool's existing defaults and unselected
roles. Credentials are handed off inside this target's own configuration scope.

## Paths and precedence

Default model configuration is `~/.omp/agent/models.yml`, then `models.yaml`.
When both are absent, native legacy `models.json` migration can create YAML.
Default settings are `~/.omp/agent/config.yml`, or an existing `config.yaml`.
If both exist, `config.yml` is effective and `config.yaml` is shadowed. tmax
inspection exposes that precedence; apply updates the resolved effective file
in place instead of creating a canonical filename that suppresses unrelated
settings. Broker checks use the same resolution. Recovery uses the exact
captured filename rather than switching to another file or migrating settings.
Native credentials normally live in SQLite `~/.omp/agent/agent.db`, not
`models.db` (model cache) and not stock pi's `auth.json`.

`--profile NAME` wins over `OMP_PROFILE`; `PI_PROFILE` is the fallback only when
`OMP_PROFILE` is absent. Named profiles use
`~/.omp/profiles/<name>/agent/`. `--profile default` and `OMP_PROFILE=default`
select the default profile, not a named `profiles/default` directory.
`PI_CONFIG_DIR` changes the native config-root directory name.
`PI_CODING_AGENT_DIR` relocates the **default-profile** agent directory; named
profiles ignore it. A programmatic model-registry path can override file lookup.
tmax default discovery follows these environment selectors, including
`PI_PROFILE` only when `OMP_PROFILE` is absent. An explicitly declared target
path remains its independent native scope; it is not reinterpreted by the
calling process's active profile.
On Linux/macOS, existing corresponding `omp` directories under configured XDG
data/state/cache roots can relocate categorized data including the credential
store without moving settings. Inspect active native paths; an agent-directory
string alone does not prove the SQLite location.

Settings merge, high to low: declared setting environment variables → runtime
overrides → later `--config` overlays → earlier overlays (`PI_CONFIG_FILES`
loads before CLI overlays) → project settings → active-profile global settings →
built-in defaults. Project lookup is `<cwd>/.omp/settings.json` then
`<cwd>/.omp/config.yml`; it does not walk ancestors for a nearest `.omp`.
Objects deep-merge; higher arrays/scalars replace lower values. Native null
usually reads as unset/default; cleared project model roles instead fall back
to global roles. Session/model CLI options are not all settings-registry keys.
No generic system/managed layer from another agent is implied.

Native key commands use process shell context. Prefer absolute command/file and
scope paths; do not assume cwd is the models-file directory. Native adapters
launch an explicit default-profile scope with `PI_CODING_AGENT_DIR` and clear
`OMP_PROFILE`/`PI_PROFILE`; they do not change the user's profile or role policy.

Sources: pinned [settings](https://github.com/can1357/oh-my-pi/blob/45c35e40c46166dc726ccc52f5d27f40f2449f99/docs/settings.md),
[models](https://github.com/can1357/oh-my-pi/blob/45c35e40c46166dc726ccc52f5d27f40f2449f99/docs/models.md),
and [provider paths and precedence](https://github.com/can1357/oh-my-pi/blob/45c35e40c46166dc726ccc52f5d27f40f2449f99/docs/providers.md).

## Model schema

The exact grammar is
[`models-config-schema-bundle.ts`](https://github.com/can1357/oh-my-pi/blob/45c35e40c46166dc726ccc52f5d27f40f2449f99/packages/coding-agent/src/config/models-config-schema-bundle.ts),
with additional validation in
[`models-config.ts`](https://github.com/can1357/oh-my-pi/blob/45c35e40c46166dc726ccc52f5d27f40f2449f99/packages/coding-agent/src/config/models-config.ts).
Root `providers` is an optional record keyed by exact native provider ID. Unknown
root keys are not rejected, but the registry consumes only `providers`.

| Location | Native field grammar |
| --- | --- |
| Provider connection | Optional `baseUrl`, `apiKey`: nonempty strings; `api`: native enum; `headers: Record<string,string>`; `authHeader: boolean`; `auth: "apiKey" \| "none" \| "oauth"`; `transport` only `"pi-native"` |
| Provider composition | Optional `models: Model[]`, `modelOverrides: Record<string,Override>`, `discovery`, `compat`, `remoteCompaction`, `disableStrictTools: boolean` |
| Model identity | Required nonempty `id`; optional nonempty `name`, `baseUrl`; optional native `api` and `kind` (kind must be served by the API) |
| Model capability | Optional `reasoning: boolean`, `input: ("text" \| "image")[]`, `supportsTools: boolean`; numeric `contextWindow`, `maxTokens`; `maxContextWindow`: positive safe integer, at least `contextWindow` when both exist |
| Model pricing/cache | Optional `cost` with required numeric `input`, `output`, `cacheRead`, `cacheWrite`; `promptCache` with optional numeric `short`, `long`; numeric `premiumMultiplier` |
| Other model fields | `thinking`, `headers`, `compat`, `imageInputDecoder: "stb"`, positive integer `imageCountBudget`, native `tokenizer` enum, `omitMaxOutputTokens: boolean`, `preferWebsockets: boolean`, nonempty `contextPromotionTarget`, nonempty `compactionModel`, `remoteCompaction` |
| Override | Sparse model metadata; `cost` rates are individually optional. Override has no new `id` or `baseUrl`. Its API/kind can change an existing row. |
| Settings, not models file | `modelRoles: Record<string,string>`, `enabledModels`, `enabledProviders`, `disabledProviders`, `modelProviderOrder`, `cycleOrder`; `modelRoleStorage: "global" \| "project"` |

Provider API enum: `openai-completions`, `openai-responses`,
`openai-codex-responses`, `azure-openai-responses`, `anthropic-messages`,
`bedrock-converse-stream`, `google-generative-ai`, `google-gemini-cli`,
`google-vertex`, `openrouter-decisions`, `openai-decisions`,
`workers-ai-systemone`, `typesafe`. Model APIs additionally accept native runner
APIs except `web-search`. Runner and judgment APIs are not thereby tmax chat
routes. tmax's same-protocol custom mapping uses the respective
`openai-completions`, `openai-responses`, and `anthropic-messages` values.

A nonempty custom `models` list requires a base URL, an effective provider/model
API, and `apiKey` unless auth is `none` or `oauth`. An override-only provider must
supply a recognized connection, override, discovery, or compaction change.
`auth: oauth` waives a file requirement and forces OAuth-style request shaping;
it does **not** create tokens or register a login. Custom `anthropic-messages`
also defaults to that shaping when `auth` is omitted. Explicit `auth: apiKey`
keeps ordinary key shaping.

`discovery.type` accepts `ollama`, `llama.cpp`, `lm-studio`,
`openai-models-list`, `gateway`, `litellm`, `apple-foundation-models`.
Optional `timeoutMs` is positive finite numeric. `injectV1: boolean` is valid only
for `openai-models-list`. Discovery normally needs provider API, except `gateway`.
Provider transport `pi-native` uses an omp auth-gateway endpoint; it is not a
synonym for any wire API or a mandatory tmax dependency.

Native `thinking.mode` accepts `effort`, `budget`, `google-level`,
`anthropic-adaptive`, `anthropic-budget-effort`. Its effort vocabulary is
`minimal`, `low`, `medium`, `high`, `xhigh`, `max`; `efforts` (or the native legacy
range fields) is required. `defaultLevel`, `effortMap`, `supportsDisplay`, and
`requiresEffort` describe actual native controls. Do not synthesize a ladder
from `reasoning: true`. tmax projects actual upstream efforts for direct entries,
and gateway-facing input names for gateway entries under the
[shared mapping and alias rules](../../specs/integrations/spec.md#alias-reasoning-capability),
as `thinking` with `mode: "effort"` and exact representable `efforts`. Names
outside the vocabulary stay visible as omitted, not renamed by the native
adapter. Gateway model maps resolve after backend selection; tmax does not copy
them into omp's `effortMap` or `compat.reasoningEffortMap` for direct or gateway
entries. With no name inside the vocabulary, the ladder is unrepresentable and
blocks application before native writes. The projection sets no `defaultLevel`,
budget, or request value. Native compatibility keys are a curated per-API schema
plus recognized runtime wire axes. Unknown compatibility keys warn nonfatally;
preservation does not prove the API reads them. No guessed compatibility flags
are needed for the basic provider example.

Bedrock-only provider fields are string `guardrailIdentifier`,
`guardrailVersion`, enum `guardrailTrace` (`enabled`, `disabled`, `enabled_full`),
and `requestMetadata: Record<string,string>`. Remote compaction fields are optional
`enabled`, `v2StreamingEnabled` booleans and native `api`, `endpoint`, `model`,
`v2Endpoint`, `streamingEndpoint` strings. Their existence does not authorize
tmax to add a new operation. The pinned schema is authoritative for those
additional native fields; tmax does not write them to map model capabilities.

### Capability values and request preferences

`validateProviderConfiguration` rejects `contextWindow <= 0` and `maxTokens <= 0`
for **custom model rows**. The same auxiliary positivity check does not cover
`modelOverrides`; do not generalize that loophole into a valid custom-row value.
`maxContextWindow` has its own strict integer check on both forms.

[`custom-models.ts`](https://github.com/can1357/oh-my-pi/blob/45c35e40c46166dc726ccc52f5d27f40f2449f99/packages/coding-agent/src/config/custom-models.ts)
carries explicit metadata, then uses `??` to fall back to a bundled model
reference and only then native false/text/128000/16384 defaults. It preserves
explicit false and empty input. Explicit custom limits overwrite built catalog
limit corrections at finalization; later native context-tier/ceiling policies can
still apply. Zero prices are valid numeric values, not missing prices.

The independently qualified concrete tmax mapping is effective `limit.context`
→ `contextWindow`, `limit.output` → `maxTokens`, boolean `reasoning` → `reasoning`,
and supported `modalities.input` → text/image `input`. Unknown is omitted; known
zero cannot be represented in a custom row and blocks rather than silently
becoming a native default. Gateway aliases follow the shared
[alias reasoning capability](../../specs/integrations/spec.md#alias-reasoning-capability)
rule instead of copying concrete metadata.
This is capability metadata, not a per-request generation cap. Existing native
sampling, thinking effort/budgets, role presets, `omitMaxOutputTokens`, and other
request preferences must remain untouched under the
[routing contract](../../specs/model-routing/spec.md#request-parameters).

### Finite native example

This is a synthetic models-file fragment. It does not modify `config.yml`, model
roles, enabled models, subagent policies, or request preferences.

```yaml
providers:
  schema-fixture:
    baseUrl: http://127.0.0.1:17891/v1
    api: openai-completions
    auth: apiKey
    apiKey: SCHEMA_FIXTURE_KEY
    models:
      - id: fixture-model
        reasoning: false
        input: [text]
        contextWindow: 32768
        maxTokens: 4096
```

Set the named fixture environment variable only inside an isolated check. In
OMP, a bare `apiKey` string first names an exact-case nonempty environment
variable, otherwise it is the literal key. This differs from Pi 1.2.0's `$NAME`
interpolation. `!command` supplies trimmed stdout asynchronously with a 10-second
timeout. Model loading does not execute command-backed keys; actual requests or
explicit online credential probes can. Commands and header sources have native
cache/refresh behavior; they are not generic secret-export hooks.

## Authentication

The pinned native
[`auth/cascade.ts`](https://github.com/can1357/oh-my-pi/blob/45c35e40c46166dc726ccc52f5d27f40f2449f99/packages/ai/src/auth/cascade.ts)
orders sources: runtime key → **pinned `models.yml` config key** → stored OAuth →
login-sourced stored API key → extension config fallback → provider environment
→ other stored API key. Unlike stock Pi, the custom-provider config key outranks
stored OAuth. Account-restricted native sessions have additional fail-closed
selection rules; a config key does not grant permission to use pooled OAuth.

Native `.env` loading takes the first nonempty value: existing process environment,
project `.env`, active agent `.env`, active config-root `.env`, then `~/.env`.
Named profiles change the roots. This credential resolution is separate from
settings overlay precedence and provider enable/disable policy.

The credential facade accepts provider-to-one-or-many credentials. Native
[`auth/types.ts`](https://github.com/can1357/oh-my-pi/blob/45c35e40c46166dc726ccc52f5d27f40f2449f99/packages/ai/src/auth/types.ts)
defines API key `{type: "api_key", key: string, source?: "login"}` and OAuth
`{type: "oauth", ...OAuthCredentials}`. Tokens and provider-specific account
fields remain native payloads. SQLite rows carry provider, row identity, and
disable state; these are not a supported user-editable auth YAML schema.
`auth.accountPolicies` matches provider/account identity and numeric priority,
reserve/reclaim policies. Native account rotation and workspace-qualified pools
mean a provider entry is not proof of one pinned account.

Native `omp login [provider]` and `/login` own sign-in; `/logout` owns removal.
Successful login may refresh the online catalog. Consumers sharing the active
store share its account effects. In broker mode, `auth.broker.url/token` or
`OMP_AUTH_BROKER_URL/TOKEN` selects the native broker. Discovery reads connection
settings from environment/global configuration, not project redirection. The
broker owns durable writes and refresh; snapshots replace refresh tokens with a
sentinel but can contain sensitive access tokens/API keys. Do not dump snapshots.
The gateway is another surface, not a second credential owner. See pinned
[broker/gateway documentation](https://github.com/can1357/oh-my-pi/blob/45c35e40c46166dc726ccc52f5d27f40f2449f99/docs/auth-broker-gateway.md).

No auth-file import, broker migration, or Pi-to-OMP credential mirroring is part
of this mapping. tmax's key handoff uses native `!` syntax and a private native
artifact; native OAuth refresh remains with the native store/broker owner.

## OpenCode Go session identity

`packages/ai/src/providers/inference-headers.ts` projects omp's authoritative
conversation session ID onto Go-bound requests. For providers `opencode-go` and
`opencode-zen`, `applyInferenceHeaders` sets `x-opencode-session` to that live
ID and overwrites any same-named header; `User-Agent` uses a caller-wins rule
instead. The companion `applySessionHeader` supplies a model's configured
`compat.sessionHeader` name with the same live ID for other hosts.

This is a per-conversation value, not a static provider, key, process, or
conversation-set constant. tmax does not write this header: the agent supplies
it on its own request path, and tmax is absent from that path. A fixed
`extraHeaders` entry is a different static mechanism and does not qualify
per-conversation coverage.

The [Go integration contract](../../specs/integrations/opencode-go/spec.md#conversation-and-client-identity)
owns the boundary. Source inspection qualifies the mechanism only; it does not
establish Go's recognition of the header, coverage of every auxiliary Go-bound
path, or successful Go inference.

### When omp sends no session header

The gate above is a provider-ID string match on `opencode-go`/`opencode-zen`,
and the live session ID is projected only after session establishment, so the
first requests of a session carry no header. A provider configured under any
other name — a tmax-side name, for instance — never carries one.

Routing omp through the tmax gateway covers that gap. When the client supplied no
session header, the gateway attributes the request itself with an id in OpenCode's
own shape, held stable per conversation, and forwards the truthful `omp/...`
user agent unchanged. Any session value the client did supply is forwarded
verbatim and must not be replaced. See the contract's
[gateway attribution clause](../../specs/integrations/opencode-go/spec.md#conversation-and-client-identity).

### Gateway model names

An OMP selector combines its native provider entry and the gateway's public
model ID. For example, entry `tmax` and alias `deepseek-v4.1-flash` produce
`tmax/deepseek-v4.1-flash`. The alias can select the concrete model
`opencode-go/deepseek-v4.1-flash` without exposing that provider prefix in OMP.

Use an explicitly stateless alias for the qualified OMP 18.12.6 Chat Completions
path. Its context and continuation limits are owned by the
[routing contract](../../specs/model-routing/spec.md#selection-and-continuity).
Add the exact OMP selector to `enabledModels` when that allow-list is nonempty.

An installed OMP 18.12.6 invocation with retries and model fallback disabled
completed through this short alias to OpenCode Go. OMP returned `OK`, and tmax
retained an alias request with provider `opencode-go` and outcome `completed`.
This qualifies that Chat Completions request, not session-stable alias routing
or other OMP versions and protocols.

## Qualification

Retained installed evidence observed actual tmax apply, installed
`omp models ls --json --no-extensions` parser success and selected model
recognition, then exact-original-byte restore and key-artifact removal.
Apply/restore reported reload pending. Initial comments-only YAML and version
prefix defects were repaired and exercised; their failed attempts were not
counted as passes. This OMP evidence is parser/apply/restore, not native paid
inference, hosted entitlement, or successful OAuth login/refresh. Metadata
qualification after the four-field repair is separately recorded in the external
audit report. Restart the selected profile to consume persisted model changes;
existing sessions may retain model selections. Native live settings reload does
not imply that a healthy active session switches models.

Independent post-repair qualification used release SHA-256
`7f205cd16aa8c9063f99c9f4ee0883e6ef2cd4e06b1ff05201f4d86a04a19136`.
Actual apply and the installed JSON model listing agreed on context 1000000,
output 65536, reasoning true, and text/image input from concrete metadata;
unsupported audio was filtered. Separate native parser checks retained false
reasoning and empty input. Restore preserved a later unowned field and exact
original settings bytes and removed the key artifact. Known zero output was
blocked before unlock or native writes. Safe `agent inspect` reported the
correct runtime-key → pinned-config → stored-OAuth precedence. All checks ran
with private passwd-home isolation and no outbound network.

On 2026-10-24, installed OMP 18.12.6 read `modelRoleStorage: project`
before and after actual tmax application to an existing `config.yaml`, and
again after restore. No shadowing `config.yml` was created. The actual installed
model listing consumed context 123456, output 8192, false reasoning, and
text/image input. Three-way restore preserved a later unowned thinking setting
and the original comment, and removed the private key artifact; apply/restore
still reported reload pending. Separate private-home discovery exercised the
explicit default profile, `PI_PROFILE` fallback, and named-profile isolation
from `PI_CODING_AGENT_DIR`. These were offline parser/effective-setting checks,
not inference, OAuth, entitlement, or running-session model-switch proof.
