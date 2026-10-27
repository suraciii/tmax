# Codex CLI 0.162.1 native configuration

Installed version: **0.162.1**. Source revision:
[`092d3acd6bec3e3a14bdc7e7a2810ab628ab759d`](https://github.com/openai/codex/tree/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d).
Consulted 2026-10-23. This is TOML configuration plus Codex's selected native
credential backend, not a Pi model file or a portable OAuth JSON template.
See the [reference index](README.md).

## Paths and precedence

`CODEX_HOME` defaults to `~/.codex`. Base configuration is
`<CODEX_HOME>/config.toml`. V2 `--profile NAME` layers adjacent
`<CODEX_HOME>/<NAME>.config.toml` over that base; it is not an independently
selected auth store. The selected name must not also be active through legacy
`profile`/`profiles` declarations. CLI and IDE consumers sharing this home can
share authorization and configuration effects.

The pinned loader's ordinary precedence, low to high, is package defaults →
admin preferences → system → cloud enterprise → base user config → selected
profile file → trusted project layers → runtime/CLI/UI overrides. Managed
requirements constrain the effective result independently. System config is
`/etc/codex/config.toml` on Unix or `%ProgramData%\OpenAI\Codex\config.toml` on
Windows. Managed required provider wins before runtime provider selection;
otherwise runtime override → `model_provider` → native `openai` applies.

Project `.codex/config.toml` layers are discovered through ancestors and gated
by trust. Provider-routing keys, including `model_provider`, `model_providers`,
OpenAI/ChatGPT base URLs, and other sensitive destinations/commands, are stripped
from project layers. A trusted repository is not authorized to select credential
destinations. The loader comment mentions cwd `config.toml`, but its inspected
discovery reads `.codex/config.toml`; this audit does not qualify the comment as
another project filename. Prefer absolute scope and credential paths. Native
command auth can declare an absolute `cwd`; do not assume a relative command
reads beside the tmax target or native config.

Sources: [`loader/mod.rs`](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/config/src/loader/mod.rs#L89-L138),
[project discovery](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/config/src/loader/mod.rs#L1653-L1752),
and [`profile_toml.rs`](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/config/src/profile_toml.rs#L17-L74).

## Model schema

[`config_toml.rs`](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/config/src/config_toml.rs#L166-L200)
defines optional string `model`, `model_provider`, and `review_model`.
`model_providers` maps exact custom provider IDs to native provider information.
Built-in providers have special override restrictions; a custom ID is not
permission to replace all built-in behavior. The adapter emits a custom entry
and explicitly requested `model`/`model_provider` selections.

| Native provider fields | Types and meaning |
| --- | --- |
| `name`, `base_url`, `model_catalog_url` | Display string (name defaults empty); optional explicit inference base and native catalog URL |
| `wire_api` | Only `"responses"`, also the native default. `"chat"` is explicitly rejected as removed. |
| `env_key`, `env_key_instructions` | Optional environment-variable name and explanatory text |
| `experimental_bearer_token` | Optional literal bearer; native source discourages it in favor of environment auth |
| `auth` | Native command-backed bearer configuration, not an auth-file path |
| `gateway_oauth`, `aws` | Separate native gateway-OAuth and AWS signing configuration; not generic token-copy fields |
| `query_params`, `http_headers`, `env_http_headers` | String maps; environment header values name variables, and absent/empty variables omit that header |
| `request_max_retries`, `stream_max_retries`, `stream_idle_timeout_ms`, `websocket_connect_timeout_ms` | Optional unsigned integer controls; native retry caps/defaults are consumer-owned |
| `requires_openai_auth`, `supports_websockets`, `supports_standalone_web_search` | Booleans, false by default for a new custom provider |
| `capabilities` | Native provider API-capability object, not per-model context/output/reasoning metadata |

Source:
[`model-provider-info/src/lib.rs`](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/model-provider-info/src/lib.rs#L106-L204).
Runtime-only `include_internal_metadata` is skipped in serialization and cannot
be enabled as ordinary provider config. The source's schema-generation
`deny_unknown_fields` annotation is not evidence that every serde runtime loader
rejects every unknown field. An unknown/ignored field is not a consumed feature.
Explicit false and unsigned zero must not be interpreted as absent by tmax;
native consumers own validity and any special semantics. `auth.timeout_ms` is
specifically nonzero, unlike an arbitrary unsigned setting.

Command auth has executable `command`, `args: string[]`, nonzero unsigned
`timeout_ms`, unsigned `refresh_interval_ms`, and an omitted-or-absolute `cwd`.
Defaults are 5000 ms timeout, 300000 ms refresh interval, and current-directory
resolution for omitted cwd. A zero refresh interval disables proactive refresh
but permits rerunning on the 401 retry path; zero timeout is invalid. Bare command
names resolve through PATH and command paths resolve against cwd. It invokes the
executable directly, not a shell pipeline. The tmax Unix handoff uses `/bin/cat`
with the private native key artifact as an argument. Source:
[command auth declarations](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/protocol/src/config_types.rs#L562-L633).

### Catalog metadata and model roles

`model_catalog_json` is an absolute path to a native `ModelsResponse`; an empty
model list is rejected. A model record is **not** `{id, contextWindow, maxTokens}`.
Native catalog identity is `slug`; important fields include `display_name`,
`description`, `default_reasoning_level`, `supported_reasoning_levels`,
`visibility`, `supported_in_api`, signed `priority`, `context_window`,
`max_context_window`, `auto_compact_token_limit`,
`effective_context_window_percent`, `input_modalities`, tool/instruction fields,
service tiers, reasoning-summary and verbosity support. The exact complete
record is owned by
[`protocol/src/openai_models.rs`](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/protocol/src/openai_models.rs#L405-L520).
It carries signed optional context counts and booleans rather than a generic
cross-agent cost/output-limit schema. This adapter does not invent a native
pricing field or a synthetic catalog.

The native reasoning-effort vocabulary includes none, minimal, low, medium,
high, xhigh, max, ultra, persistent, and custom string values. The model's actual
supported presets determine usable controls; the enum alone is not model support.
The model manager sorts/filters the active catalog for the auth mode and keeps
an explicitly configured model. It does not have one universal default model.
A metadata maximum is not a generation preference; existing
`model_reasoning_effort`, summary, verbosity, and request-budget settings remain
native/user-owned under the
[routing contract](../../specs/model-routing/spec.md#request-parameters).

Gateway aliases follow the shared
[alias reasoning capability](../../specs/integrations/spec.md#alias-reasoning-capability)
rule. The tmax Codex adapter has no native effort-selector projection; the native
catalog fields above do not authorize a synthetic catalog or request defaults.

`review_model` selects `/review`'s model slug under the active provider; it cannot
independently choose another provider. The adapter rejects a cross-provider
review reference. `approvals_reviewer` is a different approval-request routing
setting, not the tmax `review` model role. V2 profile fields do not include
`review_model`, `model_providers`, or `cli_auth_credentials_store`; these cannot
be assumed profile-local simply because `model` is profile-selectable.

### Finite native example

A synthetic custom-provider table, without changing a default or review model:

```toml
[model_providers.schema_fixture]
name = "Schema fixture"
base_url = "http://127.0.0.1:17891/v1"
wire_api = "responses"
env_key = "SCHEMA_FIXTURE_KEY"
requires_openai_auth = false
```

It is a native grammar example, not an upstream entitlement claim. The explicit
tmax key handoff uses `auth.command/args` instead; never put reusable keys or
OAuth tokens in documentation or ordinary TOML results.

## Authentication

`cli_auth_credentials_store` accepts lowercase `file`, `keyring`, `auto`, or
`ephemeral`; native packaged/default mode is **file**. This setting and the
feature-selected keyring implementation are different choices:

| Native store | Scope/security |
| --- | --- |
| File | `<CODEX_HOME>/auth.json`; Unix writes use mode 0600 |
| Direct keyring | OS service `Codex Auth`, account derived from canonical home path; successful save removes the auth file |
| Secrets keyring backend | Secret `CODEX_AUTH`; encrypted `<CODEX_HOME>/secrets/codex_auth.age`, encryption key in OS keyring; successful save removes the auth file |
| Auto | Load keyring first, then file if absent/failed; save keyring first, with file fallback on failure |
| Ephemeral | Process memory, no persistent fallback |

The normal configuration feature `SecretAuthStorage` selects Secrets versus
Direct keyring implementation; do not infer the backend from platform alone.
Sources: [`config/src/types.rs`](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/config/src/types.rs#L135-L175),
[`auth_keyring.rs`](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/core/src/config/auth_keyring.rs#L9-L83),
and [`login/src/auth/storage.rs`](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/login/src/auth/storage.rs#L48-L70).

Native auth payload fields include `auth_mode`, serialized uppercase
`OPENAI_API_KEY`, `tokens`, `last_refresh`, `agent_identity`,
`personal_access_token`, `bedrock_api_key`, `bedrock_access_keys`. Token and
identity structures contain secret material; they are native-owned and not a
user-authored portable auth template. A single home need not have an auth file
when its real selected store is keyring or ephemeral.

Runtime auth, when that caller enables it and policy permits, checks nonempty
`CODEX_API_KEY`, external in-memory ChatGPT tokens, `CODEX_ACCESS_TOKEN`, then the
selected persistent store. `codex login status` disables the `CODEX_API_KEY`
override. Custom provider `env_key`/command/literal mechanisms are separate from
this OpenAI auth-manager cascade. `requires_openai_auth: false` prevents the
adapter from accidentally requiring a native ChatGPT login for a custom key
route; it does not validate the endpoint or credential.

Native browser/device login, status, refresh, and logout own authorization.
Browser ChatGPT login clears existing stored auth before the callback flow, with
best-effort revocation. Do not promise cancellation leaves shared native auth
unchanged. Forced login method/workspace settings constrain sign-in. Managed
ChatGPT auth proactively refreshes near expiry; external auth providers retain
their refresh operation. Logout best-effort revokes stored tokens, deletes the
selected persistent/ephemeral auth, and clears cache; it does not remove every
independent environment or custom command source. Source:
[`login/src/auth/manager.rs`](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/login/src/auth/manager.rs#L1488-L1572)
and [`cli/src/login.rs`](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/cli/src/login.rs#L123-L304).

The tmax mapping respects the selected native store and native login owner.
Neither model apply nor restoration imports native auth, copies refresh tokens,
logs out, revokes upstream access, or promises to restore a switched account.
Encrypted configuration/key recovery is not a complete native-auth backup.

## Qualification

Retained installed acceptance exercised actual apply, strict native config and
loopback Responses execution, and restore. The repaired native run completed;
one captured `/v1/responses` request used the selected synthetic key. An initial
fixture model-refresh GET returned 404 and remains a fixture limitation, not a
successful discovery claim. Restore recovered exact original bytes and removed
the native key artifact. Both operations reported reload pending.

An earlier fabricated `model_catalog_json` failed native validation because the
model lacked instructions (`base_instructions` or
`model_messages.instructions_template`). The repair removed the synthetic catalog
rather than inventing instructions. A bundled-model parser alone did not prove
the selected custom model. Actual strict native execution supplied that bounded
consumption evidence. No paid upstream request, hosted OAuth/refresh, workspace
identity, subscription gateway, or entitlement was qualified here. Restart Codex
and other consumers of the selected home; existing sessions can retain models.
See [official auth](https://developers.openai.com/codex/auth) and the
[shared native boundary](../../specs/integrations/spec.md#agent-boundaries).
