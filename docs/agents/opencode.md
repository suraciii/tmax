# OpenCode V2 2.0.28 native configuration

Schema reference version: **2.0.28**. Source revision:
[`026b106bc5d41b679ffc413264229c340871e4d3`](https://github.com/anomalyco/opencode/tree/026b106bc5d41b679ffc413264229c340871e4d3).
Consulted 2026-10-23. This reference uses the pinned **V2** source. V1 examples
with singular `provider`, `npm`, or `options` must not be assumed equivalent.
See the [reference index](README.md).

## Paths and precedence

Native global config uses the platform config root's `opencode` directory,
normally `~/.config/opencode` on Linux; `OPENCODE_CONFIG_DIR` selects an explicit
config directory. Files are `opencode.json` and `opencode.jsonc`. Project discovery
walks from the current location through ancestors, loading direct files and
`.opencode` directories. XDG config/data/state/cache roots can differ from the
config selector. Native account state is server/database scoped, not scoped only
by one JSONC filename.

`OPENCODE_DB` selects the SQLite path; relative values resolve under the native
data root. An isolated CLI execution needs an explicit database, all applicable
XDG roots, an empty project cwd, and a selected private server/standalone command.
A global config selector alone does not isolate a running managed server.
System config directories are Linux `/etc/opencode`, macOS
`/Library/Application Support/opencode`, Windows `%ProgramData%\opencode`.

Pinned ordinary-field load order is **low to high**: managed preferences → system
→ well-known remote config → global → explicit file → direct project files
root-to-cwd → `.opencode` directories root-to-cwd → inline content. Source comments
explicitly give system/managed **policy evaluation** reverse authority. Ordinary
model/default overlays and restrictive policy evaluation are not the same order.
The current website describes a different ordinary-field system priority; this
page follows the exact qualified source, not an unpinned website merge rule.
The core accepts explicit file/content options, but `OPENCODE_CONFIG` and
`OPENCODE_CONFIG_CONTENT` CLI environment wiring was not established in this
audit; those selectors remain unqualified for this revision.

`{env:NAME}` substitution uses the native process environment; a missing value
becomes empty. `{file:path}` resolves relative to the source configuration file's
directory, supports absolute paths and `~/`, and trims/escapes its content.
Missing file references normally fail; do not invent an empty-value fallback.
Sources: [`config.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/core/src/config.ts#L218-L270),
[`config/discovery.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/core/src/config/discovery.ts#L12-L133),
[`config/variable.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/core/src/config/variable.ts#L26-L69),
and [`database-path.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/cli/src/database-path.ts#L4-L13).

## Model schema

The exact V2 schema is
[`schema/src/config/provider.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/schema/src/config/provider.ts#L11-L105).
Root `providers` maps native provider IDs to entries; root `model` selects a
`provider/model` reference. A `#variant` suffix selects a native variant.
The map's model key is the local selection ID; `modelID` is the wire model ID.

| Native location | Exact relevant fields/types |
| --- | --- |
| `providers.<id>` | Optional `canonical: Provider.ID`, `name: string`, ordered `env: string[]`, `package: string`, `compatibility`, `settings`, `headers: Record<string,string>`, `body: Record<string,JSON>`, `models: Record<string,Model>` |
| Provider `settings` | Known `timeout`, `headerTimeout`, `chunkTimeout`: finite number or literal false; `compaction`, `transport`, `encoding` use native schemas; additional JSON-valued package settings are allowed |
| `models.<id>` | Optional string `modelID`, `family`, `name`, `package`; `compatibility`; `settings`, `headers`, `body`; partial `capabilities`; `variants[]`; `cost` object or array; `disabled: boolean`; `limit` |
| Model `capabilities` | Optional `tools: boolean`, `input: string[]`, `output: string[]`; **no `reasoning` boolean in this config object** |
| Model `limit` | Optional integer `context`, `input`, `output`; the schema's integer type is not Pi's positive-number validation |
| Model `cost` | Required finite USD-per-million-token `input`, `output`; optional `tier: {type: "context", size: integer}` and `cache: {read?, write?}` rates |
| `variants[]` | Required string `id` plus optional model settings/headers/body overlays, not a universal effort enum |
| `agents.<role>` | Optional model, request/system/description, mode/hidden/color/steps/disabled/permissions fields under the native agent schema |

`settings.baseURL` and `settings.apiKey` are package settings, not universal
provider fields. The adapter selects `@ai-sdk/openai-compatible` for Chat
Completions, `@ai-sdk/openai` for Responses, or `@ai-sdk/anthropic` for Messages.
Provider/model `package` selects the consumer protocol. Arbitrary package IDs do
not establish a qualified tmax route. The adapter emits per-model `modelID`,
package, and base URL as needed; it does not infer one shared protocol from a
provider name. Explicit `settings.apiKey` pins the custom package key source.

Native model defaults are tools true, input text/image, output text, context
200000, output 32000, absent input limit, and empty costs/variants. These are
native new-model defaults, not actual endpoint capabilities or zero-price claims.
Explicit false and empty arrays are not absence. Source:
[`schema/src/model.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/schema/src/model.ts#L19-L143).
For selected concrete models, tmax projects the accepted effective metadata:
`limit.context/input/output`, `tool_call` to `capabilities.tools`, and explicit
input/output modality arrays to `capabilities.input/output`. Local overrides win
over the accepted catalog; downloaded-but-unaccepted metadata is not used. False,
empty arrays, and zero integer limits remain explicit. Integers beyond JavaScript's
exact safe range block application rather than being rounded. Unknown fields stay
absent in the generated patch, leaving native defaults/overlays to the native
consumer; this does not establish actual endpoint capabilities.

The custom-model schema cannot express a reasoning-support boolean. tmax never
emits that unsupported field, prices, or request-body defaults. Effective
`reasoning_options` effort values project as `variants` entries instead: each
entry's `id` and one settings key carry an exact declared name as a capability
declaration. Chat Completions and Responses use `settings.reasoningEffort`;
Messages uses `settings.effort` alone. The entry writes no other variant field
and synthesizes no thinking flag, budget, or default variant/effort; the native
tool owns thinking enablement and request defaults. A model alias receives no
candidate aggregation or current-winner metadata. Projected `variants` are owned
model fields like the other capability fields; native request settings, bodies,
and unrelated model entries are preserved. Metadata output capacity is not a
generation budget. Native variant-mapping sources:
[`variant.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/core/src/variant.ts#L78-L105)
(Chat Completions/Responses) and
[`variant.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/core/src/variant.ts#L311-L331)
(Messages).

Native catalog input is a different grammar: `release_date`, `reasoning`,
`reasoning_options`, `tool_call`, `modalities`, prices/tiers, and limits from
models.dev. Catalog reasoning options generate package-specific variants.
Do not paste catalog fields into a V2 custom model definition. The pinned
normalizer diagnoses unsupported `release_date`, `attachment`, `reasoning`,
`temperature`, and `experimental` model keys. V1 whitelist/blacklist keys are also
not confirmed V2 model grammar. Excess decoded fields are ignored after native
normalization; malformed input can be rejected rather than partially activated.
Sources: [`models-dev.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/core/src/models-dev.ts#L20-L249)
and [`config/normalize.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/core/src/config/normalize.ts#L53-L55).

A provider's `models` map overlays entries, not an access-control model list.
Native picker behavior, historical whitelist/disable observations, and direct
`run --model` behavior do not establish gateway authorization. Historical local
observations of `whitelist`/`disabled_providers` differ from the pinned V2
normalizer; their exact legacy/runtime path was not re-qualified here. No alias,
default, or compatibility field is invented to hide that source discrepancy.

Only explicitly requested default/role mappings are written. Native agent
selection uses a configured valid default, then `build`, then a visible
non-subagent. Existing native request/variant/body parameters stay intact under
the [routing contract](../../specs/model-routing/spec.md#request-parameters).
A capability maximum is not a new sampling, effort, or generation-budget setting.
The [shared integration contract](../../specs/integrations/spec.md#agent-boundaries)
remains authoritative for metadata fidelity and unsupported mappings.

### Finite native example

A synthetic V2 endpoint fragment; it does not select a default or agent role:

```jsonc
{
  "providers": {
    "schema-fixture": {
      "name": "schema-fixture",
      "package": "@ai-sdk/openai-compatible",
      "env": [],
      "settings": {
        "baseURL": "http://127.0.0.1:17891/v1",
        "apiKey": "{env:SCHEMA_FIXTURE_KEY}"
      },
      "models": {
        "fixture-model": { "modelID": "fixture-model" }
      }
    }
  }
}
```

The tmax protected handoff uses the native `{file:tmax-<identity>.key}`
mechanism instead. The path is relative to the configuration file so native
variable substitution does not reinterpret Windows backslashes. The referenced
key artifact is native-owned plaintext with
private permissions; it is not a tmax OAuth export.

## Authentication

V2 integration credentials are SQLite `credential` rows, separate from the
service/control `account` table. Native credential fields are:

- Entry: `id`, `integrationID`, `label`, `active`, `value`.
- OAuth value: `type: "oauth"`, `methodID`, string `refresh` and `access`,
  nonnegative integer `expires`, optional metadata map.
- API-key value: `type: "key"`, string `key`, optional metadata and form
  `configuration`.
- External value: `type: "external"`, `methodID`, optional metadata. Secrets and
  renewal remain outside that row's store.

These are source type/ownership facts, not a database-editing API. Source:
[`schema/src/credential.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/schema/src/credential.ts#L30-L78).
Provider packages and integration methods own native auth/refresh behavior.
Saved credentials are considered active/newest-first before server-process
environment connections. New login activates by default; native switch selects
an exact credential ID; removing the active row selects the newest remaining
credential. OAuth refresh, where implemented, occurs within five minutes of
expiry. Sources:
[`core/src/credential.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/core/src/credential.ts#L108-L205)
and [`integration.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/core/src/integration.ts#L396-L410).

Native login/list/switch/logout commands target the selected server or private
standalone client. Login, active-account switching, and logout affect shared
server consumers, not just a config file. Logout of one saved credential is not
proof that environment/external credentials are gone or an upstream token was
revoked. tmax records safe ownership/account handles without importing payloads;
its logout must target the recorded ID rather than another newest account.

Legacy `<global.data>/auth.json` is a separate possible credential-bearing file,
normally under `~/.local/share/opencode` on Linux. The pinned migration imports
recognized API/OAuth/well-known credentials to SQLite, skips integrations already
present, and **does not delete or write back the legacy file**. This audit did
not establish every possible remaining runtime reader. Do not promise complete
sign-out by deleting one backend or assert an ongoing dual-backend cascade from
file presence alone. Source:
[`import_legacy_credentials.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/core/src/database/migration/20260805200742_import_legacy_credentials.ts#L30-L109).
No native credential import, token copying, or direct SQLite credential edits are
part of tmax's model apply/restore.

## Applying models and session routing

A default model is optional. Without one, tmax adds the selected provider models
and leaves OpenCode's current default alone. An unverified version produces a
warning, not an application gate; OpenCode 1 remains incompatible with this V2
configuration format.

OpenCode V2 supplies `x-opencode-session` and matching `X-Session-Id` on model
requests, with `opencode/<channel>/<version>/<name>` as its User-Agent. Children
share the parent affinity and forks share their source affinity. Sources at
2.0.19: [model-request.ts](https://github.com/anomalyco/opencode/blob/v2.0.19/packages/core/src/session/model-request.ts),
[affinity.ts](https://github.com/anomalyco/opencode/blob/v2.0.19/packages/core/src/session/affinity.ts),
and [app.ts](https://github.com/anomalyco/opencode/blob/v2.0.19/packages/core/src/app.ts).
tmax forwards the actual header to Go. Session aliases using Chat Completions
bind on first use with complete history, then retain that destination across
resume and gateway restart. See the [continuity contract](../../specs/model-routing/spec.md#selection-and-continuity)
for missing bindings and upstream-owned resource limits.

## Qualification

Windows acceptance on 2026-10-24 used installed OpenCode **2.0.19**, an isolated
configuration/database, and a local synthetic Chat Completions server. Application
without a default preserved the existing native selection. After explicitly
setting a fixture default, `run --standalone --format json` (without a model
override) and a subsequent `--session` run both reached the local gateway and
returned streamed responses. The actual native affinity and User-Agent reached
the upstream, and restore recovered the original configuration bytes. This does
not establish hosted-provider authorization or fix Windows protected key input;
the fixture used a synthetic stored credential reference.


Retained installed acceptance observed actual apply, then
`opencode run --standalone --format json` with **no model override**, using the
configured default. It completed locally; all three captured Chat Completions
requests used the selected model and synthetic key. Restore recovered exact
original config bytes and removed the key artifact. Apply/restore reported
reload pending. This establishes installed local route/config consumption, not
successful OAuth, hosted entitlement, metadata parity, or every provider package.

On 2026-10-24, isolated post-repair application was consumed by installed
`opencode api --standalone GET /api/model`. The effective model reader preserved
accepted inherited limits, local overrides, explicit zero limits, false tools,
and empty input/output arrays; unaccepted latest metadata was not used. An
oversized, inexact JavaScript integer blocked before native writes. Concrete
gateway selections retained metadata while aliases received no winner snapshot.
Reapply and three-way restore preserved later native request budgets, effort,
sampling, and unknown-model settings and removed the private key artifacts.
This was offline metadata/configuration consumption, not inference, OAuth,
entitlement, or proof that a running agent reloaded.

Earlier `debug config` hit an occupied managed-service port;
`debug config --standalone` was unsupported. Those are retained failed attempts,
not parser passes and not commands to repeat. Use the actually qualified
standalone `run` path for an isolated runtime fixture. Native `opencode reload`
addresses the intended server/location; config watchers and credential updates
are separate. tmax does not restart a shared server. Sources:
[official V2 config](https://opencode.ai/v2/docs/config/),
[provider accounts](https://opencode.ai/v2/docs/cli/providers/), and
[`reload.ts`](https://github.com/anomalyco/opencode/blob/026b106bc5d41b679ffc413264229c340871e4d3/packages/cli/src/commands/handlers/reload.ts#L10-L25).
