# DeepSeek Harness (DSH)

Source-qualified version: **0.2.1-rc.2**. Consulted 2026-10-26 from the installed
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
its public concrete or alias ID. The gateway must already have the matching
loaded configuration, listener, and shared key; native apply does not start it.
Gateway configuration and actual runtime continuity qualification are separate.
In the Web workspace choose **DeepSeek Harness** in **Coding tools**, select the
existing DSH home directory (normally `~/.dsh`), and use the same explicit
apply/restore actions. tmax does not enumerate, select, create, or manage profiles.
Skill installation remains shared `.agents` or Hermes-only; `skill install
--agent dsh` is not supported.

## Native scope and precedence

| Surface | Native behavior and tmax boundary |
| --- | --- |
| Home | The target scope is the existing DSH home itself. Discovery uses nonblank `DSH_HOME`, otherwise the OS home directory's `.dsh`; an explicit target path selects that home directly. It is never inferred from a profile path. |
| Home configuration | tmax edits the home's `cordis.patch.yml`, a YAML sequence of entry patches identified by `id`. It does not author a replacement root document. |
| Profiles | tmax does not enumerate, select, create, or edit profiles, inspect their bundles, or write launcher-owned `cordis.yml`. |
| Precedence | Bundle patches → profile patch → home `cordis.patch.yml` → command-line `--patch` overlays → telemetry switch. A later entry `config` replaces that entry's complete configuration, not an independent deep merge. |
| Common configuration | The home configuration is authoritative for every profile using that home. A home `llm-pi-ai.config` shadows the complete lower-layer configuration for that entry; lower profile routes and defaults are not automatically copied or merged into it. Runtime overlays can still change effective behavior and remain outside tmax's ownership. |
| Preservation | Existing unrelated HOME-level `llm-pi-ai` provider fields, unrelated rows, comments, and opaque `!!js` expressions remain native-owned. An owned branch that cannot be safely edited blocks rather than evaluating or reserializing arbitrary expressions. |
| Legacy | `settings.yaml` is not the current configuration surface; tmax does not recreate it. |

Sources at the pinned revision:
[`profile-boot.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/apps/cli/src/profile-boot.ts#L67-L88),
[`profile-context.ts`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/boot/app-boot/src/profile-context.ts#L68-L85),
[`config-editor`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/boot/config-editor/src/index.ts#L104-L148),
and the dormant route in the
[`base bundle`](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/bundle/base/cordis.patch.yml#L125-L136).

## Protocol and model fields

`llm-pi-ai.config.providers.<name>` contains the provider route. Protocol and base
URL belong to the route, not to each model. Selections that need different APIs
or bases cannot share one native route; split them into distinct target-local
provider entries. Application blocks an unrepresentable mixed route instead of
silently dropping selections.
An existing HOME-level route with `modelOverrides` cannot also accept a selected
`models` list under the native schema. Apply blocks before writing; migrate that
route through DSH or choose a separate target-local provider entry.

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
| `maxTokens` | Intentionally not projected by tmax: an explicit native value also selects a per-request output default. Preserve existing HOME-level settings; lower-layer settings shadowed by the home entry are not imported. |
| `input` | Representable declared `text`/`image` inputs. DSH interprets an empty list as inheritance, not as a model accepting no inputs. Explicit unrepresentable declarations must not silently become inheritance. |
| `reasoningEfforts` | Capability map with exact keys `off`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, or `false` for explicit non-reasoning capability. |

DSH's native `maxTokens` is not purely descriptive: an explicit configured value
also becomes the model's per-request output default. Inherited catalog/fallback
capacity does not have that effect. tmax therefore leaves output capability
unprojected rather than changing the user's generation budget as a metadata
side effect. Existing HOME-level output settings remain unchanged; lower-layer
settings for an entry shadowed by the home configuration are not imported.
For retained model IDs, application preserves existing HOME-level model fields
such as `maxTokens`, display names, and thinking budgets, while replacing only
explicitly projected capabilities. Selection still owns model membership and
order; fields are matched by exact model ID rather than array position.

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

Sources:
[`credentials-local` format and path](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/credentials/credentials-local/src/index.ts#L28-L118),
[reference resolution](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/credentials/credentials-local/src/index.ts#L624-L631),
and [`headless` bundle](https://github.com/deepseek-ai/deepseek-harness/blob/687f282e8625c6b8d11ff58211a90524be2e4317/packages/bundle/headless/cordis.patch.yml#L32-L43).

## Evidence and runtime limits

| Evidence class | What this reference establishes |
| --- | --- |
| Configuration contract | Native home target declaration, selection, explicit apply/restore, home patch and credential ownership, and the safety boundaries described above. |
| Source inspection | The pinned 0.2.1-rc.2 checkout implements the cited patch layers, whole-entry configuration replacement, model schema, protocols, credential service, and headless disablement. Source inspection is not a native parser or inference run. |
| Home-target runtime, 2026-10-26 | An isolated release CLI configured one existing DSH home. Two independently authored headless profiles consumed the same home patch and native credential store. Actual DSH runs returned `DSH native OK` through Chat Completions, Responses, and Messages against synthetic loopback SSE endpoints. Existing home routes remained available; the home entry shadowed lower profile routes. This establishes no hosted-provider acceptance. |
| Home-target recovery and Web, 2026-10-26 | Apply and restore previews left native files unchanged. Restore removed managed credential references and preserved existing home routes/defaults, later native patch rows, and unrelated credential records. Both profile manifests and patches remained byte-unchanged. The actual Web discovery form offered only the DSH home, recognized the registered target, and described the home-only file boundary. |
| Gateway runtime boundary | Earlier profile-path runs exercised all three protocols through tmax's gateway against loopback fixtures. The home-target runs above exercised direct routes, not gateway continuity or hosted providers. |
| Alias reasoning runtime and Web, 2026-10-27 | A release CLI applied a stateless Chat Completions alias referencing two OpenCode Go provider instances for `deepseek-v4.1-flash`. The actual DSH Web selector displayed Default, Low, High, and Max on the alias. Headless gateway calls succeeded with native default effort and a one-run explicit `high` effort, returning `DSH_ALIAS_OK` and `DSH_ALIAS_HIGH_OK`. Alias membership and native effort defaults remained unchanged. This exercises alias selection and the dispatched hosted route, not both accounts individually, other protocols, or session continuity. |

In the home-target runs, DSH 0.2.1-rc.2 sent its truthful
`deepseek-harness/...` User-Agent and no qualified session header. Complete independent requests may use
fixed concrete models or explicitly stateless aliases, with no session binding.
Responses requires `store: false` and no upstream-owned continuation state.
Session-required aliases and provider restrictions still require separate
qualification. Do not synthesize a static session ID, substitute another agent's
User-Agent, or infer stock pi's session behavior merely because DSH uses pi-ai.

All three mappings have native source support and home-target local runtime
evidence, not proof that every provider/model/transport accepts them. Existing HOME-level effort preferences
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
