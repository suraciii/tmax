# DeepSeek Harness (DSH)

Source-qualified version: **0.2.1-rc.2**. Consulted 2026-10-27 from the installed
checkout `/home/szf/repos/deepseek-harness`, source commit
[`687f282e8625c6b8d11ff58211a90524be2e4317`](https://github.com/deepseek-ai/deepseek-harness/tree/687f282e8625c6b8d11ff58211a90524be2e4317).
No supported version range is inferred (`version_range: null`). See the
[reference index](README.md) and the owning
[DSH home boundary](../../specs/integrations/spec.md#dsh-home-boundary).

## Configure through tmax

DSH is a native tmax target, not a launch wrapper. Use the same CLI or Web
provider/model selection, apply, inspection, and restoration operations as other
agents. Start with an existing DSH home outside `~/.tmax` and a tmax provider
whose endpoint, key, and exact model are configured. These operands are examples;
`PROVIDER_ID` and `MODEL_ID` refer to that existing configuration:

```sh
DSH_SCOPE="${DSH_HOME:-$HOME/.dsh}"
TARGET=dsh-main

tmax agent discover --agent dsh
tmax agent add "$TARGET" --agent dsh --path "$DSH_SCOPE"
tmax agent provider add "$TARGET" upstream --provider "$PROVIDER_ID"
tmax agent model add "$TARGET" upstream "$MODEL_ID" \
  --protocol chat-completions
tmax agent apply "$TARGET" --dry-run
tmax agent apply "$TARGET"
tmax agent inspect "$TARGET"
```

This is model-only application: it writes no `agent-default-model` entry or
reasoning preference. If wanted, explicitly choose a selected model with
`agent default set`; no native model-role names or interactive login are
qualified for DSH.
After application, reload/restart DSH instances using that home. Successful file
application is not proof of native consumption or hosted inference. To restore
managed changes while preserving unrelated later edits:

```sh
tmax agent restore "$TARGET" --dry-run
tmax agent restore "$TARGET"
```

For a gateway provider entry, use `agent provider add ... --gateway` and select
its public concrete or alias ID. Name the native entry `tmax` when that name is
free in DSH's shared provider namespace; an existing user adapter route with that
name must be explicitly moved or removed first, not silently overwritten. Keep
the direct upstream entry when adding a gateway entry; both can coexist in one
target. The gateway must already have the matching loaded configuration,
listener, and shared key; native apply does not start it. Gateway configuration
and actual runtime continuity qualification are separate.

Choose the client's protocol explicitly. Prefer `--protocol responses` when the
upstream declares Responses and the selected public ID publishes it; the native
route then uses `api: openai-responses`. Each DSH provider entry has one API and
base URL, so different protocols require distinct native names. This does not
authorize gateway translation or change an existing selection. See the shared
[endpoint contract](../../specs/integrations/spec.md#endpoint-resolution).
In the Web workspace choose **DeepSeek Harness** in **Coding tools**, select the
existing DSH home directory (normally `~/.dsh`), and use the same explicit
apply/restore actions. tmax does not enumerate, select, create, or manage profiles.
Skill installation remains shared `.agents` or Hermes-only; `skill install
--agent dsh` is not supported.

## Native scope and precedence

| Surface | Native behavior and tmax boundary |
| --- | --- |
| Home | The target scope is the existing DSH home itself. Discovery uses nonblank `DSH_HOME`, otherwise the OS home directory's `.dsh`; an explicit target path selects that home directly. It is never inferred from a profile path. |
| Home configuration | tmax edits the home's `cordis.patch.yml`, a YAML sequence of entry patches identified by `id`, and installs the embedded plugin as the direct-child `tmax-dsh.mjs`. It does not author a replacement root document. |
| Profiles | tmax does not enumerate, select, create, or edit profiles or presets, inspect their bundles, or write launcher-owned `cordis.yml`. |
| Precedence | Bundle patches → profile patch → home `cordis.patch.yml` → command-line `--patch` overlays → telemetry switch. A later entry `config` replaces that same entry's complete configuration, not an independent deep merge. |
| Common configuration | The independent home entry has id `llm-tmax` and `name` equal to the absolute path of `tmax-dsh.mjs`. Its routes are common to profiles using that home. It does not replace, copy, or merge user `llm-pi-ai` configuration. Runtime overlays can still change effective behavior and remain outside tmax's ownership. |
| Preservation | User `llm-pi-ai` configuration, unrelated HOME-level rows, comments, provider fields, and opaque `!!js` expressions remain native-owned. An owned branch that cannot be safely edited blocks rather than evaluating or reserializing arbitrary expressions. |
| Legacy | `settings.yaml` is not the current configuration surface; tmax does not recreate it. |

Sources at the pinned revision:
[`profile-boot.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/apps/cli/src/profile-boot.ts#L67-L88),
[`profile-context.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/boot/app-boot/src/profile-context.ts#L68-L85),
[`config-editor`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/boot/config-editor/src/index.ts#L104-L148),
and the dormant route in the
[`base bundle`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/bundle/base/cordis.patch.yml#L125-L136).

## Independent plugin and shared runtime

Apply installs tmax's embedded plugin artifact; it does not run `npm install`,
change a profile, or manage bundle composition. DSH's native loader loads the
absolute artifact path. During `apply`, the plugin imports installed DSH
dependencies through the native Loader tree rather than static package imports
resolved from the home directory. The plugin exports `name = 'llm-tmax'` and
`inject = ['llm']` and reuses the public root `PiAiAdapter` export for protocol
handling rather than maintaining another wire implementation.

The plugin registers only the provider routes explicitly configured under
`llm-tmax.config.providers`. Provider configurations from all adapters register
in the same LLM runtime. Provider route names are global across active adapters,
not private to a plugin entry. Choose native names distinct from existing routes;
tmax does not silently rename, merge, or omit colliding entries.

Mounting `llm-pi-ai` a second time is not isolation: each instance declares its
whole installed catalog, so disjoint configured route names can still collide
with `DUPLICATE_DIRECTORY`. The independent plugin registers no configurable-
provider directory: its configuration remains tmax-owned. Live provider and model
lists make these routes selectable alongside native user routes, but tmax routes
are not editable through native Models settings. Change their desired state in
tmax and explicitly apply it. The existing adapter and its configuration remain
intact.

Sources at the pinned revision:
[official adapter cookbook](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/docs/cookbook/adding-an-llm-adapter.md#L7-L35),
[`PiAiAdapter` export](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm-pi-ai/src/index.ts#L77-L95),
[adapter constructor](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm-pi-ai/src/adapter.ts#L84-L124),
[catalog-wide directory registration](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm-pi-ai/src/index.ts#L117-L150),
[shared adapter registry](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm/src/index.ts#L393-L458),
[shared provider directory](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm/src/index.ts#L488-L538),
and [native loader import](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/vendor/loader/src/config/tree.ts#L110-L128).

## Protocol and model fields

`llm-tmax.config.providers.<name>` contains the provider route, with `api`,
`baseURL`, `apiKeyEnv`, and the selected `models`. Protocol and base URL belong
to the route, not to each model. Selections that need different APIs or bases
cannot share one native route; split them into distinct provider entries with
globally unique native names. Application blocks an unrepresentable mixed route
instead of silently dropping selections.
An existing HOME-level route in the managed entry with `modelOverrides` cannot
also accept a selected `models` list under the reused adapter schema. Apply
blocks before writing; migrate that route explicitly or choose a separate
native provider name. Unrelated routes in user `llm-pi-ai` are not changed.

| tmax protocol | Native route `api` |
| --- | --- |
| `chat-completions` | `openai-completions` |
| `responses` | `openai-responses` |
| `messages` | `anthropic-messages` |

| Native field | Meaning and limits |
| --- | --- |
| `apiKeyEnv` | Native credential reference name, not the credential value. |
| `baseURL` | Resolved base for the selected protocol; a compatible-looking URL alone is not provider qualification. |
| `models[].id`, `name` | Exact selected model ID and known display name. No fabricated model or name is needed. |
| `contextWindow` | Known positive, exactly representable integer context capacity. Unknown metadata remains absent. |
| `maxTokens` | Intentionally not projected by tmax: an explicit native value also selects a per-request output default. Preserve retained HOME-level model settings in the managed entry; do not import settings from user `llm-pi-ai` or profiles. |
| `input` | Representable declared `text`/`image` inputs. DSH interprets an empty list as inheritance, not as a model accepting no inputs. Explicit unrepresentable declarations must not silently become inheritance. |
| `reasoningEfforts` | Capability map with exact keys `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, or `false` for explicit non-reasoning capability. |

DSH's native `maxTokens` is not purely descriptive: an explicit configured value
also becomes the model's per-request output default. Inherited catalog/fallback
capacity does not have that effect. tmax therefore leaves output capability
unprojected rather than changing the user's generation budget as a metadata
side effect. For retained model IDs in the managed entry, application preserves
existing HOME-level model fields such as `maxTokens`, display names, and thinking
budgets while replacing only explicitly projected capabilities. Selection still
owns model membership and order; fields are matched by exact model ID rather
than array position. User `llm-pi-ai` and profile model settings remain independent
and are not imported.

Route-level protocol template arguments remain inherited when a model declares
no arguments. The native schema's empty dictionaries do not erase route-level
`compat.chatTemplateKwargs` or `chatTemplateArgs`; nonempty model dictionaries
override their matching route field. These are native request settings, not
version-compatibility fallbacks.

For a Chat Completions gateway route whose upstream does not accept the
`developer` role, explicitly set
`llm-tmax.config.providers.<name>.compat.supportsDeveloperRole: false` in the
home patch. A loopback URL cannot convey the upstream's dialect to the native
adapter. The installed OpenCode Go run below required this route setting;
tmax does not rewrite message roles or infer it for every gateway model.
Set it at route level: the selected `models` array is owned and editing that
array after application blocks reapply. Route-level `compat` remains native-owned
and survives reapply; as with other later additions, it can block removal of a
newly inserted plugin during restore.

For direct entries, tmax projects actual upstream declared efforts; for gateway
entries it projects caller-facing input names under the
[shared mapping and alias rules](../../specs/integrations/spec.md#alias-reasoning-capability).
The native adapter uses exact names and identical wire strings, with maps resolved
only inside the gateway after backend selection. Absent keys stay absent;
unrepresentable names remain visible as omitted.
A map with no level beyond `off` cannot represent a reasoning model and blocks
application. `reasoning = true` without declared effort values invents no levels;
explicit `reasoning = false` maps to native `reasoningEfforts: false`. The map is
not a chosen effort. tmax does not set route `reasoning`, `thinkingBudgets`, or
`agent-default-model.reasoningEffort` as a side effect of model configuration.
An explicit default selection changes provider/model only, preserving existing
HOME-level `agent-default-model` fields, including reasoning effort. Its home
`config` shadows the complete lower-layer entry rather than merging profile defaults.

Gateway aliases also project [common input modalities](../../specs/integrations/spec.md#common-input-modalities).
All configured candidates must declare image input before the alias advertises
it. Reapplication withdraws obsolete managed `input` declarations when evidence
becomes unknown, while preserving unrelated row preferences. Native fallback may
still apply; DSH defaults custom models with no input declaration to text. A
known empty common input list blocks application because DSH would reinterpret
it as inheritance.


Sources:
[`provider.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm-pi-ai/src/provider.ts#L47-L62),
[`config.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm-pi-ai/src/config.ts#L287-L349),
[`catalog.ts` model fields](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm-pi-ai/src/catalog.ts#L602-L623),
[reasoning vocabulary](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm-pi-ai/src/catalog.ts#L75-L86),
[reasoning resolver](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm-pi-ai/src/catalog.ts#L719-L767),
and [`agent-default-model`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/core/agent-default-model/src/index.ts#L24-L55).

## Credentials and recovery

Executed apply hands the referenced tmax upstream or gateway key to DSH's
**plaintext native-owned** `$DSH_HOME/.credentials.yaml`, under
`version: 1` and `refs.TMAX_<stable hash>`. The adjacent home patch contains only
the matching `apiKeyEnv` reference. The store is shared by profiles using that
home; unrelated references and `records` must remain intact. Native credential
and tmax recovery files are current-user-restricted, not encrypted. Inspection
and dry-run disclose the handoff without resolving, printing, or returning the secret.

Apply and restore executions hold DSH's sibling `.credentials.yaml.lock` writer
lock from the initial credential read through commit. The owner-only lock records
the writer PID. An existing lock blocks immediately with `native_credentials_busy`,
leaving both native files and that lock unchanged; retry after its owner releases
it. tmax never overwrites or reclaims an existing lock, and releases its own lock
on success or failure. Inspection and dry-run do not create locks. An empty,
comment-only, or `{}` credential store is initialized to version 1 on apply;
nonempty stores without version 1 must first be migrated through DSH.

The native credential service resolves stored references before inherited process
environment, project dotenv, and home dotenv fallback. The shipped headless
bundle disables `credentials`. When handing off a key, tmax explicitly enables
that service in the HOME patch without inspecting or selecting a bundle. It does
not require exporting a secret or launching DSH through tmax. Credential-service
changes participate in restoration, preserving unrelated HOME-level service settings.
No login token is copied and no new OAuth refresher is introduced. Removing a
target does not revoke a provider key or sign DSH out.

Apply and restore track the direct-child plugin artifact, its HOME patch
insertion, managed provider fields, credential references, and credential-service
enablement under the [owning recovery contract](../../specs/integrations/spec.md#dsh-home-boundary).
Restoration removes a newly owned insertion or artifact only when its captured
baseline and subsequent edits permit that operation. It preserves unrelated
later edits and reports conflicts instead of overwriting them. The independent
`llm-tmax` entry is the only supported application format; no earlier shared
`llm-pi-ai` adapter layout is maintained or automatically migrated. User-owned
`llm-pi-ai` entries and providers remain untouched.
Reapplying after a native provider entry is removed or renamed also removes the
old, introduced route when it has no remaining settings. Unowned additions that
would leave that route active or invalid block before native writes; move or
remove those additions explicitly rather than deleting them implicitly.

If later unowned additions keep a newly inserted plugin entry nonempty after
owned fields are reverted, restore stops with `native_conflict` before any native
write. It never leaves a nameless insertion or deletes an artifact still needed
by that entry. Interrupted plugin writes accept the captured pre-write bytes;
unrecognized later edits still block restoration.

The inserted plugin row is regenerated when managed fields change. For rows
outside tmax's JSON-compatible generated form, any `!`, `&`, `*`, or `#` marker
blocks apply and restore with `native_conflict`, including markers inside quoted
literals. This conservative rule avoids flattening YAML tags, anchors, aliases,
or comments; generated JSON string literals remain editable. Unrelated patch
rows remain byte-preserved.

Sources:
[`credentials-local` format and path](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/credentials/credentials-local/src/index.ts#L28-L118),
[reference resolution](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/credentials/credentials-local/src/index.ts#L624-L631),
and [`headless` bundle](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/bundle/headless/cordis.patch.yml#L32-L43).

## Evidence and runtime limits

| Evidence class | What this reference establishes |
| --- | --- |
| Configuration contract | Native home target declaration, selection, explicit apply/restore, independent `llm-tmax` artifact and patch ownership, unchanged credential handoff, and the safety boundaries in the owning specification. |
| Source inspection | The pinned 0.2.1-rc.2 checkout implements the cited patch layers, whole-entry configuration replacement, shared runtime and provider directory, reusable protocol adapter, model schema, credential service, and headless disablement. Source inspection is not a native parser or inference run. |
| Independent-plugin loader and protocols, 2026-10-27 | Actual DSH 0.2.1-rc.2 loaded the independent home artifact through its native Loader. A synthetic loopback smoke run passed Chat Completions, Responses, and Messages using an attachment-local PNG image and an `echo` tool declaration. Each route returned the expected text, a tool call, usage of 11 input / 5 output / 16 total tokens, a `tool-calls` finish, and generated replay state. Wire requests carried the truthful `deepseek-harness/0.2.1-rc.2` User-Agent and resolved the native credential reference despite incorrect ambient API keys. This qualifies these local request paths, not hosted access or a replay-state follow-up. |
| Independent-plugin application and recovery, 2026-10-27 | An isolated run used the public Rust apply path, issued native DSH requests through all three protocols, and restored the target with exit 0. Restore removed the newly owned home plugin artifact and patch; profile files remained unchanged. These results do not establish hosted access or native Web settings behavior. |
| Independent-plugin gateway forwarding, 2026-10-27 | Actual DSH requests passed all three protocols through an isolated tmax `Runtime`, `Snapshot`, and gateway router to synthetic loopback upstreams. Concrete qualified model IDs were rewritten to upstream IDs, upstream credentials replaced the gateway credential, and truthful DSH attribution, PNG images, tools, text, usage, finish reasons, and generated replay state were retained. Native home gateway routes and credential references were prepared by the harness; this does not qualify public gateway apply, resident readiness, hosted access, alias selection, or session continuity. |
| Independent-plugin mixed routes, 2026-10-27 | One actual DSH process dispatched three direct and three concrete gateway routes alongside a native user adapter. All six synthetic image/tool requests passed; duplicate route registration was rejected without replacing existing routes, and profile manifest and patch bytes were unchanged. The gateway setup has the same harness-prepared configuration boundary as the forwarding run above. |
| tmax Web boundary hint, 2026-10-27 | Actual application assets rendered the DSH add form through an isolated read-only local API bridge in a real browser. Selecting DeepSeek Harness displayed the independent `llm-tmax`, home-only artifact/configuration, unchanged user `llm-pi-ai`, and no-profile-management boundary. No Web save or apply was performed, and no managed live home was modified. This verifies the displayed hint, not Web mutation or native Models settings. |
| Installed independent-plugin gateway, 2026-10-27 | The release executable was installed locally and its SHA-256 matched both the release build and the restarted resident. Public CLI operations applied the home plugin, native gateway credential reference, and explicit default to a distinct `tmax-gateway` route without changing the user's existing `tmax` route. Actual DSH 0.2.1-rc.2 headless runs returned exactly `TMAX_DSH_NATIVE_OK` through the stateless Chat Completions alias `deepseek-v4.1-flash`; retained gateway history recorded completed dispatches to `opencode-go`. The first run received upstream HTTP 422 for a `developer` role. Explicit native route `compat.supportsDeveloperRole: false` corrected that dialect mismatch; reapply changed no files and the next native run succeeded. All nine existing profile manifest/configuration files remained byte-unchanged; native credentials retained mode `0600` and no writer lock remained. This qualifies this installed route and selected upstream, not the second account, other hosted protocols, native Web settings, tool/image requests to the hosted provider, or session continuity. |
| Installed `tmax` Responses cutover, 2026-10-27 | An explicit one-time removal of obsolete user `tmax` routes from the headless and web profile patches freed the shared name; all other profile bytes and manifests were preserved. This was a local configuration change, not automatic profile management by tmax. Public CLI operations renamed the managed entry to `tmax`, selected the stateless Responses alias `deepseek-v4.1-flash`, and applied `api: openai-responses`. A startup check exposed an empty route left by an intermediate build; after explicit removal, the composed managed adapter contained only `tmax`. The final installed release and running resident matched the release build's SHA-256. Actual DSH headless returned exactly `TMAX_DSH_RESPONSES_OK` with exit 0 and no diagnostics; gateway history recorded Responses, `opencode-go`, HTTP 200, and `completion_evidenced`. Reapply changed no native files; credentials remained mode `0600` with no writer lock. This qualifies the selected hosted Responses request, not both accounts, hosted tools/images, native Web settings, or continuation/replay state. |
| Installed tmax Web protocol choices, 2026-10-27 | In the actual local workspace, model choices included only published protocols and grouped Responses before Chat Completions and Messages for concrete models and same-name aliases. A new alias preferred Responses; editing an existing Chat Completions alias retained Chat, and the DSH default retained its explicit Responses selection. No Web save or native apply was performed in this browser check. This verifies tmax's configuration UI, not DSH's native Models page. |
| Historical home-target runtime, 2026-10-26 | An isolated release CLI configured one existing DSH home through the earlier `llm-pi-ai` integration. Two independently authored headless profiles consumed the same home patch and native credential store. Actual DSH runs returned `DSH native OK` through Chat Completions, Responses, and Messages against synthetic loopback SSE endpoints. The old home entry shadowed lower profile routes. This establishes no hosted-provider acceptance or independent-plugin qualification. |
| Historical home-target recovery and Web, 2026-10-26 | Apply and restore previews left native files unchanged. Restore removed managed credential references and preserved existing home routes/defaults, later native patch rows, and unrelated credential records. Both profile manifests and patches remained byte-unchanged. The actual Web discovery form offered only the DSH home, recognized the registered target, and described the then-current home-only file boundary. This predates the plugin artifact. |
| Historical gateway runtime boundary | Earlier profile-path runs exercised all three protocols through tmax's gateway against loopback fixtures. The historical home-target runs above exercised direct routes, not gateway continuity or hosted providers. These historical runs do not qualify the independent plugin; its concrete gateway-forwarding evidence is recorded separately above. |
| Historical alias reasoning runtime and Web, 2026-10-27 | Before the independent-plugin cutover, a release CLI applied a stateless Chat Completions alias referencing two OpenCode Go provider instances for `deepseek-v4.1-flash`. The actual DSH Web selector displayed Default, Low, High, and Max on the alias. Headless gateway calls succeeded with native default effort and a one-run explicit `high` effort, returning `DSH_ALIAS_OK` and `DSH_ALIAS_HIGH_OK`. Alias membership and native effort defaults remained unchanged. This exercises alias selection and the dispatched hosted route, not both accounts individually, other protocols, session continuity, or the independent plugin. |

In the historical home-target runs, DSH 0.2.1-rc.2 sent its truthful
`deepseek-harness/...` User-Agent and no qualified session header. Complete independent requests may use
fixed concrete models or explicitly stateless aliases, with no session binding.
Responses requires `store: false` and no upstream-owned continuation state.
Session-required aliases and provider restrictions still require separate
qualification. Do not synthesize a static session ID, substitute another agent's
User-Agent, or infer stock pi's session behavior merely because DSH uses pi-ai.

All three mappings have native source support and independent-plugin local
runtime evidence. This does not establish that every provider/model/transport
accepts them. Existing HOME-level effort preferences
remain unchanged: an unsupported native effort fails until the selected model's
actual capability or the caller's explicit preference is corrected. tmax does
not repair that mismatch by selecting a default effort.

A gateway alias can expose native `reasoningEfforts` under the shared
[alias reasoning capability](../../specs/integrations/spec.md#alias-reasoning-capability)
rule. It does not pin the alias to one account or borrow the current winner's
metadata. An inherited explicit effort outside the projected choices can still
be rejected by DSH; tmax does not silently clear or replace that preference.
When the common choices are empty or unknown, native `reasoningEfforts: false`
clears the old selector under the shared rule and marks the alias as non-reasoning
in DSH. Inspection remains `unavailable` with `reasoning_options_unavailable`.
This does not declare the candidates non-reasoning or change saved effort
preferences; an incompatible inherited preference can still fail.
Inspect and reapply affected entries as specified by the shared rule, then reload
DSH. Neither native application nor this projection changes alias routing.

In particular, direct **OpenCode Go is not qualified**: no genuine DSH Go session
mechanism is established by this reference. The
[Go contract](../../specs/integrations/opencode-go/spec.md) owns provider session
requirements. Successful native file application, credential resolution, or a
local loopback request cannot establish that entitlement or session coverage.
