# DeepSeek Harness (DSH)

Two separate version claims apply to this reference:

- **Native integration sources: 0.2.1-rc.2.** Consulted 2026-10-27 from a local checkout of
  `deepseek-harness`, source commit
  [`687f282e8625c6b8d11ff58211a90524be2e4317`](https://github.com/deepseek-ai/deepseek-harness/tree/687f282e8625c6b8d11ff58211a90524be2e4317).
  These citations own the native home target, loader, adapter, credential, and
  model-field sections below.
- **Profile plugin: 0.2.3-alpha.2.** Consulted 2026-11-23 from the running source
  install and a local checkout of `deepseek-harness`, source commit
  [`4b399c39a6d1479cd84ca591b4d7c88e24d38a96`](https://github.com/deepseek-ai/deepseek-harness/tree/4b399c39a6d1479cd84ca591b4d7c88e24d38a96)
  (tag `dsh-v0.2.3-alpha.2`). These citations own the [profile page
  plugin](#profile-page-plugin) surfaces: client slots, locale, the optional
  command service, authenticated web routes, and credential `describe`.

No supported version range is inferred. See the [reference index](README.md), the
owning [DSH home boundary](../../specs/integrations/spec.md#dsh-home-boundary), the
[DSH plugin boundary](../../specs/integrations/spec.md#dsh-plugin-boundary), and
the [DSH plugin contract](../../specs/integrations/dsh-plugin/spec.md).

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

For a relayed entry, use `agent provider add ... --provider tmax` and select a
public route name. The version-1 `--gateway` selector is removed; convert stored
entries with `tmax config migrate`. Name the native entry `tmax` when that name is
free in DSH's shared provider namespace; an existing user adapter route with that
name must be explicitly moved or removed first, not silently overwritten. Keep
the direct entry when adding a relayed one; both can coexist in one target. The
gateway must already have the matching loaded configuration, listener, and shared
key; native apply does not start it. Gateway configuration and actual runtime
continuity qualification are separate. `agent inspect` reports the entry's
`relayed` flag, base URL, credential reference, and applied delivery state.

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
| Home configuration | tmax edits the home's `cordis.patch.yml`, a YAML sequence of entry patches identified by `id`, and configures the `tmax` plugin entry there. It does not install the plugin or author a replacement root document. |
| Profiles | tmax does not enumerate, select, create, or edit profiles or presets, inspect their bundles, or write launcher-owned `cordis.yml`. |
| Precedence | Bundle patches → profile patch → home `cordis.patch.yml` → command-line `--patch` overlays → telemetry switch. A later entry `config` replaces that same entry's complete configuration, not an independent deep merge. |
| Common configuration | tmax writes the providers into the profile settings document DSH's own Models page writes — `profiles/<profile>/cordis.patch.yml`, row `llm-pi-ai`, path `config.providers.<entry>` — so the base bundle's adapter serves them with nothing else installed. Providers the user configured keep their own names and fields. Runtime overlays can still change effective behavior and remain outside tmax's ownership. |
| Preservation | User `llm-pi-ai` configuration, unrelated HOME-level rows, comments, provider fields, and opaque `!!js` expressions remain native-owned. An owned branch that cannot be safely edited blocks rather than evaluating or reserializing arbitrary expressions. |
| Legacy | `settings.yaml` is not the current configuration surface; tmax does not recreate it. |

Sources at the pinned revision:
[`profile-boot.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/apps/cli/src/profile-boot.ts#L67-L88),
[`profile-context.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/boot/app-boot/src/profile-context.ts#L68-L85),
[`config-editor`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/boot/config-editor/src/index.ts#L104-L148),
and the dormant route in the
[`base bundle`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/bundle/base/cordis.patch.yml#L125-L136).

## Independent plugin and shared runtime

DSH's base bundle already composes `llm-pi-ai` (dormant until settings supply
profiles) and the credential store, so tmax needs no plugin to make routes work:
apply writes provider profiles into the profile's settings and the loader mounts
them on the next start. The optional `tmax` bundle only adds the page.

Apply selects the profile: `--profile` if given, otherwise `desktop`, then `web`,
then an error naming the profiles the home has. Two profiles may select the same
entry differently, so each provider's credential reference includes the profile.

Provider route names are global across active adapters, not private to an entry.
tmax keeps the entry's own name, so every reference that already names it
(`agent-default-model`, subagent and tool model choices) keeps resolving. A name
the user configured that tmax does not own blocks application rather than being
overwritten, and tmax never renames, merges, or omits a colliding entry.

tmax writes into the same `providers` mapping the Models page edits, so its
entries appear there beside the user's own. That is the point — DSH serves them
with no further step — and it means a user edit to a provider tmax wrote is
drift: apply reports the conflicting field rather than overwriting it, and
restore puts back the value it recorded. Mounting `llm-pi-ai` a second time
would not be isolation: each instance declares its whole installed catalog, so
disjoint configured route names can still collide with `DUPLICATE_DIRECTORY`.
tmax therefore never adds a mount, an adapter, or a configurable-provider
directory of its own.

Sources at the pinned revision:
[official adapter cookbook](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/docs/cookbook/adding-an-llm-adapter.md#L7-L35),
[`PiAiAdapter` export](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm-pi-ai/src/index.ts#L77-L95),
[adapter constructor](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm-pi-ai/src/adapter.ts#L84-L124),
[catalog-wide directory registration](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm-pi-ai/src/index.ts#L117-L150),
[shared adapter registry](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm/src/index.ts#L393-L458),
[shared provider directory](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/llm/llm/src/index.ts#L488-L538),
and [native loader import](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/vendor/loader/src/config/tree.ts#L110-L128).

## Profile page plugin

A separate bundle entry with id `tmax` may load in a DSH profile. It adds a
first-level page (`sidebar.panellist` plus `main`), four seats on DSH-owned pages
(`sidebar.panellist.trailing`, `settings.models.footer`,
`conversation.input.model.options` and `.pane`, and `tool.call.toolview` keyed
`tmax_status`), a read-only `/tmax` human command when the host exposes the
optional `commands` service, a bounded retained-history area in Diagnostics, one
read tool, one skill, and exact authenticated web routes. The page, seats, and
command read one host model; the page reads the model composed from tmax's
effect-free commands — `agent list` when no target is configured, `agent inspect
TARGET`, `status`, `gateway status`, `provider list`, `provider inspect`,
`config validate`, and `model explain` — and adds only the DSH-side facts tmax
cannot know. There is no `tmax dsh` command, no snapshot contract, and no
fallback source: `diagnostics.readModel` reports `inventory`, and a failed read
is a visible section error.

The page's Apply action is the previewed `tmax agent apply TARGET [--dry-run]`
operation: it writes the target's native DSH files (the profile's `llm-pi-ai`
provider settings and the credential reference) and is the page's only DSH or
native write. It never edits the home patch, per the shared
[DSH plugin boundary](../../specs/integrations/spec.md#dsh-plugin-boundary).
Seats are additive and non-authoritative. The page also performs only previewed,
allowlisted tmax operations, including provider observation, quota resume,
failure reset, management-listener open, and gateway start, stop, and restart; it
does not manage DSH profiles and does not author tmax configuration. Page
behavior is owned by the
[DSH plugin contract](../../specs/integrations/dsh-plugin/spec.md); installation
and use are in the [plugin README](../../dsh-plugin/README.md).

Sources at the running revision:
[route admission default](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/host/webserver/src/index.ts#L781-L790),
[provider registry](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/llm/llm/src/index.ts#L465-L470),
[credential description](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/credentials/credentials/src/index.ts#L226),
[redacted settings](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/settings/settings/src/index.ts#L324-L331),
[effective default model](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/core/agent-default-model/src/index.ts#L68),
[human command registry](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/interaction/commands/src/index.ts#L30),
[sidebar trailing slot](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/client/ui-sidebar/src/client/index.ts#L95),
[models footer slot](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/client/ui-settings-models/src/client/index.ts#L151),
[model selection slots](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/client/ui-model-selection/src/client/slots.ts#L54-L64),
and [tool view slot](https://github.com/deepseek-ai/deepseek-harness/blob/4b399c39a6d1479cd84ca591b4d7c88e24d38a96/packages/client/ui-tool/src/client/apply.ts#L42).

## Protocol and model fields

Each route the plugin projects carries `api`,
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
`compat.supportsDeveloperRole: false` on that native provider in the plugin's own
home patch. A loopback URL cannot convey the upstream's dialect to the native
adapter. The installed OpenCode Go run below required this route setting;
tmax does not rewrite message roles or infer it for every gateway model.
Set it at route level: the selected `models` array is owned and editing that
array after application blocks reapply. Route-level `compat` remains native-owned
and survives reapply; as with other later additions, it can block removal of a
newly inserted plugin during restore.

For direct entries, tmax projects actual upstream declared efforts; for relayed
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
An explicit default selection writes provider/model and, only when the default
declares `effort`, `agent-default-model.reasoningEffort` validated against the
model's `reasoningEfforts` keys; an absent effort preserves existing HOME-level
`agent-default-model` fields. Its home `config` shadows the complete lower-layer
entry rather than merging profile defaults.

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
`tmax` entry is the only supported application format; no earlier shared
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
| Configuration contract | Native home target declaration, selection, explicit apply/restore, the installed `tmax` bundle and its entry ownership, unchanged credential handoff, and the safety boundaries in the owning specification. |
| Source inspection | The pinned 0.2.1-rc.2 checkout implements the cited patch layers, whole-entry configuration replacement, shared runtime and provider directory, reusable protocol adapter, model schema, credential service, and headless disablement. Source inspection is not a native parser or inference run. |
| Profile plugin API fit, 2026-11-23 | The running DSH 0.2.3-alpha.2 (source launch, profile `web`) and the installed tmax 0.1.0 were inspected through `cordis_inspect` and source reads. The plugin's host and client rely on the exact authenticated web-route API (`auth` defaults to `required`), the provider registry, credential `describe`, redacted settings and the configurable-provider directory, client slots and locale registration, the optional `commands` service, and effective default-model selection that this version provides. This establishes API fit; the actual temporary-profile loads are recorded in the pre-port and ported render rows below. |
| Profile plugin inventory, actions, command, and tests, 2026-11-24 | The ported host composes one `/_tmax/state` contract from `agent list` (when no target is configured), `agent inspect TARGET`, `status`, `gateway status`, `provider list`/`provider inspect`, `config validate`, and `model explain`; no host path runs `tmax dsh`, and `diagnostics.readModel` reports `inventory`. Connect is the previewed `tmax agent apply TARGET [--dry-run]` native write. Host, action, command, history, and observation tests cover route ownership and liveness, credential readiness, section `errors`, allowlist validation, exact argv, preview versus execute, gateway guards and deadlines, busy, typed 409 refusals, 502 transport, secret-free results, every `/tmax` subcommand and its bounded output, the 45 s history read and its typed-unavailable split, and the page's explicit load state and unknown-count handling. The full plugin Node suite passed, including its host, actions, command, client, history, observations, quota, adapter, and package tests. |
| Ported inventory real-CLI check, 2026-11-24 | A latest-main binary was built for a read-only smoke check. The real user store is still configuration format 1, so the composed success path could not be exercised against it; the success path is covered by hermetic fake-CLI tests. Real-CLI reads covered target resolution, the typed `unsupported_tmax` gate against the older installed CLI, and the typed `invalid_configuration` answer against the latest binary. That check performed no mutation, apply, refresh, or profile load; the separate ported profile render row records the synthetic-inventory load. |
| Ported profile page render, 2026-11-24 | A temporary DSH 0.2.3-alpha.2 profile loaded the ported root package with a synthetic inventory executable; it did not use a real tmax store. Real authenticated HTTP returned `GET /_tmax/state` `200` with `diagnostics.readModel` `inventory`, target `dsh-main`, one live route, two providers, two routing rows, and gateway `identified`/`loaded`; the page rendered with route and gateway facts and action controls. This exercises the real package-load, host-composition, HTTP, and client-render path for the ported read model. It does not exercise a real format-2 tmax store, a real workspace session, a browser click on an action control, non-dry-run Apply, Refresh, or actions, a successful history-table load, the settings footer, the model menu option and routing pane, the `tmax_status` tool card, or the `/tmax` command. |
| Pre-port profile page render, 2026-11-23 | A temporary DSH 0.2.3-alpha.2 profile loaded the earlier, snapshot-backed plugin and served the page. Real authenticated HTTP returned `GET /_tmax/state` `200`; the page rendered in the light theme, the sidebar **tmax** row showed its trailing marker text `a blocker is reported`, and the gateway card showed **Start gateway** and **Open management**. Real `POST /_tmax/action` previews returned `open_management` 200, `observe_provider` 200 `dry_run_ready`, `resume_quota` 200 `dry_run_ready`, a `gateway_start` dry run typed 409 `runtime_blocked`, and an unknown provider 409 typed; earlier checks covered `401` without the session cookie, `415` for a non-JSON observation POST, and `403` for a cross-origin POST. The page was also rendered in dark theme and with a zh-CN locale. This render exercised the pre-port snapshot-backed read model; it does not qualify the ported inventory-backed read model. Not verified in that run or since: a browser click on any action control, non-dry-run Apply/Refresh/actions, the settings footer card, the model menu option and routing pane, the `tmax_status` tool card, `/tmax` invocation, or a successful history-table load. |
| Retained request history, 2026-11-23 | The history service was exercised against the live CLI and returned a typed `unavailable` / `daemon_identity_unavailable` at HTTP 200 rather than an empty history. Host and client tests cover the typed-unavailable versus transport-502 split, the allowlist, the 50-row response cap and omission count, the page's explicit **Load history** state, the 20-row table, coverage and gaps including the numeric incomplete-capture count, the more-count, and the empty-versus-unavailable rule. A successful history-table load or interaction in a browser was not verified; the live read was the unavailable case only. |
| Independent-plugin loader and protocols, 2026-10-27 | Actual DSH 0.2.1-rc.2 loaded the independent home artifact through its native Loader. A synthetic loopback smoke run passed Chat Completions, Responses, and Messages using an attachment-local PNG image and an `echo` tool declaration. Each route returned the expected text, a tool call, usage of 11 input / 5 output / 16 total tokens, a `tool-calls` finish, and generated replay state. Wire requests carried the truthful `deepseek-harness/0.2.1-rc.2` User-Agent and resolved the native credential reference despite incorrect ambient API keys. This qualifies these local request paths, not hosted access or a replay-state follow-up. |
| Independent-plugin application and recovery, 2026-10-27 | An isolated run used the public Rust apply path, issued native DSH requests through all three protocols, and restored the target with exit 0. Restore removed the newly owned home plugin artifact and patch; profile files remained unchanged. These results do not establish hosted access or native Web settings behavior. |
| Independent-plugin gateway forwarding, 2026-10-27 | Actual DSH requests passed all three protocols through an isolated tmax `Runtime`, `Snapshot`, and gateway router to synthetic loopback upstreams. Concrete qualified model IDs were rewritten to upstream IDs, upstream credentials replaced the gateway credential, and truthful DSH attribution, PNG images, tools, text, usage, finish reasons, and generated replay state were retained. Native home gateway routes and credential references were prepared by the harness; this does not qualify public gateway apply, daemon readiness, hosted access, alias selection, or session continuity. |
| Independent-plugin mixed routes, 2026-10-27 | One actual DSH process dispatched three direct and three concrete gateway routes alongside a native user adapter. All six synthetic image/tool requests passed; duplicate route registration was rejected without replacing existing routes, and profile manifest and patch bytes were unchanged. The gateway setup has the same harness-prepared configuration boundary as the forwarding run above. |
| tmax Web boundary hint, 2026-10-27 | Actual application assets rendered the DSH add form through an isolated read-only local API bridge in a real browser. Selecting DeepSeek Harness displayed the independent `llm-tmax`, home-only artifact/configuration, unchanged user `llm-pi-ai`, and no-profile-management boundary. No Web save or apply was performed, and no managed live home was modified. This verifies the displayed hint, not Web mutation or native Models settings. |
| Installed independent-plugin gateway, 2026-10-27 | The release executable was installed locally and its SHA-256 matched both the release build and the restarted daemon. Public CLI operations applied the home plugin, native gateway credential reference, and explicit default to a distinct `tmax-gateway` route without changing the user's existing `tmax` route. Actual DSH 0.2.1-rc.2 headless runs returned exactly `TMAX_DSH_NATIVE_OK` through the stateless Chat Completions alias `deepseek-v4.1-flash`; retained gateway history recorded completed dispatches to `opencode-go`. The first run received upstream HTTP 422 for a `developer` role. Explicit native route `compat.supportsDeveloperRole: false` corrected that dialect mismatch; reapply changed no files and the next native run succeeded. All nine existing profile manifest/configuration files remained byte-unchanged; native credentials retained mode `0600` and no writer lock remained. This qualifies this installed route and selected upstream, not the second account, other hosted protocols, native Web settings, tool/image requests to the hosted provider, or session continuity. |
| Installed `tmax` Responses cutover, 2026-10-27 | An explicit one-time removal of obsolete user `tmax` routes from the headless and web profile patches freed the shared name; all other profile bytes and manifests were preserved. This was a local configuration change, not automatic profile management by tmax. Public CLI operations renamed the managed entry to `tmax`, selected the stateless Responses alias `deepseek-v4.1-flash`, and applied `api: openai-responses`. A startup check exposed an empty route left by an intermediate build; after explicit removal, the composed managed adapter contained only `tmax`. The final installed release and running daemon matched the release build's SHA-256. Actual DSH headless returned exactly `TMAX_DSH_RESPONSES_OK` with exit 0 and no diagnostics; gateway history recorded Responses, `opencode-go`, HTTP 200, and `completion_evidenced`. Reapply changed no native files; credentials remained mode `0600` with no writer lock. This qualifies the selected hosted Responses request, not both accounts, hosted tools/images, native Web settings, or continuation/replay state. |
| Installed tmax Web protocol choices, 2026-10-27 | In the actual local workspace, model choices included only published protocols and grouped Responses before Chat Completions and Messages for concrete models and same-name aliases. A new alias preferred Responses; editing an existing Chat Completions alias retained Chat, and the DSH default retained its explicit Responses selection. No Web save or native apply was performed in this browser check. This verifies tmax's configuration UI, not DSH's native Models page. |
| Historical home-target runtime, 2026-10-26 | An isolated release CLI configured one existing DSH home through the earlier `llm-pi-ai` integration. Two independently authored headless profiles consumed the same home patch and native credential store. Actual DSH runs returned `DSH native OK` through Chat Completions, Responses, and Messages against synthetic loopback SSE endpoints. The old home entry shadowed lower profile routes. This establishes no hosted-provider acceptance or independent-plugin qualification. |
| Historical home-target recovery and Web, 2026-10-26 | Apply and restore previews left native files unchanged. Restore removed managed credential references and preserved existing home routes/defaults, later native patch rows, and unrelated credential records. Both profile manifests and patches remained byte-unchanged. The actual Web discovery form offered only the DSH home, recognized the registered target, and described the then-current home-only file boundary. This predates the plugin artifact. |
| Historical gateway runtime boundary | Earlier profile-path runs exercised all three protocols through tmax's gateway against loopback fixtures. The historical home-target runs above exercised direct routes, not gateway continuity or hosted providers. These historical runs do not qualify the independent plugin; its concrete gateway-forwarding evidence is recorded separately above. |
| Historical alias reasoning runtime and Web, 2026-10-27 | Before the independent-plugin cutover, a release CLI applied a stateless Chat Completions alias referencing two OpenCode Go provider instances for `deepseek-v4.1-flash`. The actual DSH Web selector displayed Default, Low, High, and Max on the alias. Headless gateway calls succeeded with native default effort and a one-run explicit `high` effort, returning `DSH_ALIAS_OK` and `DSH_ALIAS_HIGH_OK`. Alias membership and native effort defaults remained unchanged. This exercises alias selection and the dispatched hosted route, not both accounts individually, other protocols, session continuity, or the independent plugin. |

The 2026-11-23/24 profile-plugin rows establish API fit for the client slots,
locale, command service, and credential `describe`; the composed inventory read
model and its `inventory` source marker; the action, command, history, and
observation test contracts; a pre-port temporary-profile page render of the
snapshot-backed read model; and a ported temporary-profile page render with a
synthetic inventory. They do not establish browser interaction with the seats or
`/tmax` command, the settings footer, the model option and pane, or the tool
card; a successful history-table load in a browser; non-dry-run Apply, Refresh,
or actions; a real workspace session using the plugin routes; or the ported read
model against a real format-2 tmax store. The ported read model's behavior
beyond the synthetic render is covered by the host and client tests.

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
