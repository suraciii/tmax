# Stock pi 1.2.0 native configuration

Revision: [`06a466935536f8c9bcacb2d5efc0fc021be1bdff`](https://github.com/earendil-works/pi/tree/06a466935536f8c9bcacb2d5efc0fc021be1bdff).
Installed version: **1.2.0**. Consulted 2026-10-23. This is stock pi, not omp.
See the [reference index](README.md) for safety and evidence boundaries.

Applying selected provider models does not require a tmax default. Omit
`agent default set` to preserve the tool's existing defaults and unselected
roles. Credentials are handed off inside this target's own configuration scope.
On Windows, discovery supports the official npm package through its `pi.cmd`
installation marker and package entrypoint, launched directly with Node.js.

## Paths and precedence

The default agent directory is `~/.pi/agent`. `PI_CODING_AGENT_DIR` or the SDK's
`agentDir` selects another directory. Native model definitions are
`<agent-dir>/models.json`; startup defaults and preferences are
`<agent-dir>/settings.json`; saved credentials are `<agent-dir>/auth.json`.
There is no omp profile selector or `models.yml` substitution in this format.

Trusted `<cwd>/.pi/settings.json` overlays user settings. Project extensions can
register providers and models after trust. Project context-file ancestor discovery
is separate from settings discovery. CLI/session selections override startup
selections; resuming a session restores its model/thinking changes without
rewriting startup defaults. Do not infer a project `models.json` or managed/system
provider layer from another agent's behavior. Use absolute agent-directory and
credential-command paths; shell commands run in native process context, not in a
guaranteed directory relative to `models.json`.

Sources: [configuration](https://github.com/earendil-works/pi/blob/06a466935536f8c9bcacb2d5efc0fc021be1bdff/packages/coding-agent/docs/configuration.md),
[settings](https://github.com/earendil-works/pi/blob/06a466935536f8c9bcacb2d5efc0fc021be1bdff/packages/coding-agent/docs/settings.md),
and [model selection](https://github.com/earendil-works/pi/blob/06a466935536f8c9bcacb2d5efc0fc021be1bdff/packages/coding-agent/docs/models.md).

## Model schema

The authoritative file grammar is
[`model-config.ts`](https://github.com/earendil-works/pi/blob/06a466935536f8c9bcacb2d5efc0fc021be1bdff/packages/coding-agent/src/core/model-config.ts).
The installed `dist/core/model-config.js` was also inspected. It strips a BOM and
JSON comments before JSON parsing; this is not YAML. Root `providers` is required
and maps arbitrary exact provider IDs to provider objects. Optional `$schema` is
a string. A provider ID is not an API enum or a tmax provider ID.

| Location | Native fields and types |
| --- | --- |
| Provider | Optional nonempty strings `name`, `baseUrl`, `apiKey`, `api`; `oauth` accepts only `"radius"`; `headers: Record<string,string>`; `authHeader: boolean`; `compat` uses the native compatibility schema; `models: Model[]`; `modelOverrides: Record<model-id,Override>` |
| Model | Required nonempty `id`; optional nonempty `name`, `api`, `baseUrl`; `reasoning: boolean`; `input: ("text" \| "image")[]`; `contextWindow`, `maxTokens`: numbers strictly greater than zero |
| Additional model metadata | `thinkingLevelMap`, `inputLimits`, `cost`, `promptCache`, `headers`, `compat`, `samplingParams`, `samplingParamsByThinkingLevel` |
| Override | Optional model metadata as above, but no replacement `id`, `api`, or `baseUrl`; `cost` can be partial |
| Startup settings | `defaultProvider: string`, `defaultModel: string`; `enabledModels: string[]` controls startup/cycling patterns, not authorization |

`api` is a nonempty string in the file schema, not an enumerated JSON field. It
must resolve to an installed native API implementation. The tmax mapping uses
`openai-completions`, `openai-responses`, or `anthropic-messages` for its respective
native protocol; this is not a claim that every registered API is a tmax route.
A custom model needs an effective API and base URL; a syntactically accepted string
alone does not qualify transport.

[`model-schema.ts`](https://github.com/earendil-works/pi/blob/06a466935536f8c9bcacb2d5efc0fc021be1bdff/packages/ai/src/providers/model-schema.ts)
defines cost as numeric `input`, `output`, `cacheRead`, `cacheWrite` in USD per
million tokens, with optional `tiers[]` adding `inputTokensAbove` plus those four
rates. Zero price is valid and is not unknown price. `promptCache.short/long`
are optional positive seconds. `thinkingLevelMap` keys are `off`, `minimal`, `low`,
`medium`, `high`, `xhigh`, `max`, with string or null values. It is not permission
to fabricate effort support from `reasoning: true`. tmax projects effective
`reasoning_options` effort values into this map as capability declarations: every
key is written explicitly, holding its exact declared name or null. Names outside
the keys are never translated and stay visible as omitted. A declaration with no
level besides `off` is unrepresentable and blocks application before native
writes. The map declares selector capability and sets no default thinking level
or request value.

`inputLimits.maxRequestBytes`, `images.maxPerMessage`, and `images.maxPerRequest`
are positive integers. `images.resize` accepts positive integer `maxWidth`,
`maxHeight`, `maxBytes`, and integer `jpegQuality` from 1 through 100. The native
documentation distinguishes attachment resizing from not-yet-enforced hard
history limits. Sampling maps are arbitrary string-keyed values; per-thinking
maps use the seven native thinking keys. These are native request preferences,
not fields for metadata synchronization to overwrite.

The file schema does not set `additionalProperties: false`. Unknown fields can
survive validation but are not thereby consumed. Unknown `modelOverrides` IDs are
ignored. A `models` row adds or replaces the same provider/model ID; an override
patches an existing built-in or extension row. Native
[`provider-composer.ts`](https://github.com/earendil-works/pi/blob/06a466935536f8c9bcacb2d5efc0fc021be1bdff/packages/coding-agent/src/core/provider-composer.ts)
uses omitted-value defaults for a new unknown custom model: reasoning false,
text input, context 128000, output 16384. These are native fallbacks, not verified
provider capabilities. Explicit `false` and `[]` are distinct from omitted values.
Explicit zero context/output is invalid, not a request to use a fallback.

For a concrete tmax offering, the qualified capability mapping is effective
`limit.context` → `contextWindow`, `limit.output` → `maxTokens`, `reasoning` →
`reasoning`, and supported `modalities.input` → `input` (intersection with native
text/image). Unknown metadata is omitted; an unrepresentable known zero blocks
application rather than being clamped or silently omitted. Alias rows do not
claim their current winner's capabilities. See the
[shared integration boundary](../../specs/integrations/spec.md#agent-boundaries).
A metadata maximum is not an instruction to impose a new per-request generation
cap. Existing sampling, effort, and output preferences remain owned by the
[routing contract](../../specs/model-routing/spec.md#request-parameters).

### Finite native example

This provider fragment uses an environment reference, not a reusable secret.
It does not change settings or roles. The values describe a synthetic local
fixture, not a published model or an upstream default.

```json
{
  "providers": {
    "schema-fixture": {
      "baseUrl": "http://127.0.0.1:17891/v1",
      "api": "openai-completions",
      "apiKey": "${SCHEMA_FIXTURE_KEY}",
      "models": [{
        "id": "fixture-model",
        "reasoning": false,
        "input": ["text"],
        "contextWindow": 32768,
        "maxTokens": 4096
      }]
    }
  }
}
```

## Authentication

Credential resolution is runtime `--api-key` → saved `auth.json` credential →
`models.json` `apiKey` → provider environment or ambient cloud credentials.
Changing a built-in base URL can therefore retain a higher-precedence native
credential. tmax must not assume a model-file key always wins.

The source-backed native `auth.json` root maps provider IDs to a credential:

- API key: `type: "api_key"`, optional `key: string`, optional
  `env: Record<string,string>`.
- OAuth: `type: "oauth"`, `access: string`, `refresh: string`, finite numeric
  `expires`; provider-owned additional fields remain native.

These are field/ownership facts, not a credential-file template. Source:
[`auth-storage.ts`](https://github.com/earendil-works/pi/blob/06a466935536f8c9bcacb2d5efc0fc021be1bdff/packages/coding-agent/src/core/auth-storage.ts).
The native file backend creates directories mode 0700 and new auth files mode
0600. This is plaintext file storage, not an OS keychain or OMP's SQLite pool.

`apiKey` and header strings support literal text, `$NAME`/`${NAME}` interpolation,
and leading `!command`; `$$` and `$!` escape literal characters. **A bare
`MY_KEY` is literal text in Pi 1.2.0**, unlike OMP's environment-name fallback.
Missing interpolated values leave the reference unresolved. Native model-file
commands resolve at request time without model-file caching; other auth-store
resolution paths can cache commands. Do not infer a refresh mechanism from a
shell command. Source:
[`resolve-config-value.ts`](https://github.com/earendil-works/pi/blob/06a466935536f8c9bcacb2d5efc0fc021be1bdff/packages/coding-agent/src/core/resolve-config-value.ts).

Native `/login` performs the registered provider's API-key or OAuth flow;
`/logout` removes its stored credential. The native runtime owns refresh and
provider-specific login behavior. Consumers sharing the agent directory share
that stored authorization. Removing it does not clear a process override or an
environment key. No native-to-tmax credential import or OAuth copying is part of
this mapping. tmax key handoff uses native `!` command syntax referencing a
private native-owned artifact, not a second OAuth store.

## Qualification

Retained installed acceptance observed Pi 1.2.0 parser success, selected model
recognition, actual tmax apply, and actual restore. A synthetic local native →
tmax → fixture run verified the selected key, tool round trip, and dynamic
`x-session-id`: fresh sessions differ, resume matches, and RPC `new_session`
changes identity. Its native Chat Completions compatibility options were
`sendSessionAffinityHeaders: true` and `sessionAffinityFormat: "openrouter"`.
This bounded evidence is not a recommendation to add compatibility flags to
unrelated direct routes.

Three-way restore retained a later unowned comment and `quietStartup` preference,
and removed owned model/key changes. A manually reformatted probe restored the
semantic baseline, not original formatting. The later repaired reapply consumed
the credential through the installed parser path. No actual native OAuth login,
refresh, hosted entitlement, or paid provider request was qualified by these
local checks. Open `/model` to reload model definitions; restart for persisted
startup defaults. Existing sessions can retain their selections.

Independent post-repair qualification used release SHA-256
`7f205cd16aa8c9063f99c9f4ee0883e6ef2cd4e06b1ff05201f4d86a04a19136`.
Actual apply wrote context 1000000, output 65536, reasoning true, and text/image
input from a concrete offering; unsupported audio metadata was filtered. The
installed parser consumed those values. Separate parser checks accepted explicit
reasoning false and empty input. Restore preserved a later unowned field and
original settings bytes and removed the private key artifact. A known zero
output limit failed with `native_metadata_unrepresentable` before unlock or
native writes. This check used private passwd-home and disconnected network
namespaces; it did not infer capabilities from a hosted model or run inference.
