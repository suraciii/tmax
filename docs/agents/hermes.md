# Hermes Agent 0.22.1+683.g848fc0a native configuration

Installed version: **0.22.1+683.g848fc0a**. Source revision:
[`848fc0a`](https://github.com/NousResearch/hermes-agent/tree/848fc0a).
Consulted 2026-10-23. Source inspection used code and safe templates, not live
native configuration or credential data. See the [reference index](README.md).

Applying selected provider models does not require a tmax default. Omit
`agent default set` to preserve the tool's existing defaults and unselected
roles. Credentials are handed off inside this target's own configuration scope.

## Paths and precedence

On Linux/macOS, default home is `~/.hermes`; Windows uses
`%LOCALAPPDATA%\hermes` or `%USERPROFILE%\AppData\Local\hermes`.
`HERMES_DATA_DIR_SUFFIX` adds a suffix. Context-local home override precedes
`HERMES_HOME`, then the platform default. `HERMES_HOME` expands `~` and environment
variables. Active files include `config.yaml`, `.env`, and native `auth.json`.
These are separate model configuration, environment secret sources, and native
authorization state, not one YAML auth schema.

Early `-p`/`--profile` selection sets home before Hermes imports. Without it, the
CLI can follow sticky `active_profile`; named profiles use
`<root>/profiles/<name>`, and `default` uses root. A home already pointing into a
profile is treated as that profile. `HERMES_PROFILE`/`HERMES_PROFILE_NAME` are not
the same early profile-selection interface. Profiles have independent settings,
not live config inheritance. Auth has a deliberate per-provider fallback to root
when profile-local state is absent; see [authentication](#authentication).

Configuration order is defaults → user `config.yaml` deep merge → normalization
and user environment expansion → managed overlay. `HERMES_MANAGED_DIR` selects
an existing managed directory; otherwise POSIX can use `/etc/hermes` when present.
Managed values win per leaf. Runtime provider selection is explicit CLI provider
→ `model.provider` → `HERMES_INFERENCE_PROVIDER` → `auto`: do not assume that the
environment always beats the selected config provider.

Dotenv loading is another order: active-home `.env` overrides inherited shell
values; `.op.env` is conditional; the source project-root `.env` is a development
fallback; enabled external secret sources follow; managed `.env` loads last with
override. Profile-scoped turns avoid copying another profile's dotenv into the
global process. An older profile can receive a one-time missing-dotenv backfill.
Native home/profile selection therefore needs review beyond setting one variable.

Local terminal `.` means invocation cwd. Gateway/messaging/cron use `terminal.cwd`;
remote backends interpret it in their own environment. This is not a promise
that relative key-command paths resolve relative to `config.yaml`. Use absolute
scope and credential paths. Sources:
[`hermes_constants.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/hermes_constants.py#L46-L63),
[`main.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/hermes_cli/main.py#L595-L719),
[`profiles.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/hermes_cli/profiles.py#L2432-L2465),
[`config.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/hermes_cli/config.py#L2349-L2468),
[`env_loader.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/hermes_cli/env_loader.py#L458-L563),
and [official managed scope](https://hermes-agent.nousresearch.com/docs/user-guide/managed-scope).

## Model schema

Hermes does not expose a closed universal provider/model JSON schema.
[`config_providers.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/hermes_cli/config_providers.py#L114-L282)
normalizes a modern `providers` mapping and separate legacy custom-provider
forms. Model selection, endpoint definitions, and capability overrides are
separate surfaces. Unknown normalized provider fields warn and are ignored;
acceptance by a permissive YAML loader is not proof of runtime use.

| Native location | Fields and actual consumption |
| --- | --- |
| `model` | Canonical `default: string` model ID, `provider: string` (`auto` permits native selection), `base_url`, `api_key`, `key_env`, `api_mode`, `streaming: boolean`, positive `context_length`, `ollama_num_ctx`, `default_headers`, `extra_headers`, `session_affinity_header` |
| `providers.<id>` connection | String `name`, optional native `provider`; endpoint first usable `base_url`, `url`, `api`; `api_key`, `key_env` / `api_key_env`, `key_cmd`; `api_mode` / `transport`; `model` / `default_model` |
| Provider model metadata | `models` normalizes a map of ID to metadata, or a list of IDs/objects with `id` or `name`; positive integer `context_length`, boolean `capabilities`, `models_discovered`; per-model context/capabilities override endpoint values |
| Other provider settings | Nonnegative numeric `rate_limit_delay`; `request_timeout_seconds`, `stale_timeout_seconds`; boolean/string `discover_models`; mapping `extra_body`, `extra_headers`; `ssl_ca_cert`, boolean/nonempty-string `ssl_verify`; `catalog_provider`, `session_affinity_header` |
| `model_overrides` | Exact provider/model metadata patches: `context_window`, `supports_tools`, `supports_vision`, `supports_reasoning`, `model_family`. Provider/top-level `_default` fills gaps only for catalog-unknown models. |
| `auxiliary.<task>` | Provider/model/base URL/key/timeout/body settings, with task-specific fields. They select auxiliary routes, not access-control roles. |

Native compatibility aliases exist but tmax does not manufacture them: model
`model`/`name` normalize to `default`; `api_base` normalizes to `base_url`; older
root provider/base/context fields move under `model` only when the corresponding
model field is empty. `extra_headers` wins duplicate entries over
`default_headers`. Legacy `custom_providers` is a list with required `name` and
`base_url`, not the modern map. These are externally accepted grammar facts,
not alternative tmax persistence forms.

Canonical runtime modes are `chat_completions`, `codex_responses`,
`anthropic_messages`, `bedrock_converse`, `codex_app_server`. Native aliases include
`openai`/`openai_chat`/`chat-completions`, `responses`/`openai_responses`,
`anthropic`/`messages`, and `bedrock`. URL detection can constrain or select the
mode; `codex_app_server` needs explicit runtime opt-in. The tmax adapter uses the
canonical three HTTP modes for Chat Completions, Responses, and Messages; a base
URL alone does not qualify a transport. Source:
[`runtime_provider.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/hermes_cli/runtime_provider.py#L128-L175).

Provider IDs include built-ins such as `openrouter`, `nous`, `openai-codex`, and
native Anthropic, plus named custom endpoints. The map is not a finite universal
vendor enum. A `providers` block can tune a built-in without creating a custom
endpoint. `catalog_provider` changes metadata lookup, not inference routing.

Hermes consumes context and boolean capabilities through its own model metadata
paths, not Pi's `contextWindow`/`maxTokens` fields. False capabilities must stay
false, not become unknown. Zero context is not a valid positive context override.
No qualified generic per-model output-limit or pricing writer is asserted by
this adapter. `models.dev` supplies native catalog data; probe, custom metadata,
`model.context_length`, and `model_overrides` resolve at different stages. Source:
[`agent/model_metadata.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/agent/model_metadata.py#L2144-L2235).

For selected concrete offerings, tmax writes accepted effective context to
`providers.<name>.models.<id>.context_length` for route-context resolution and
`model_overrides.<name>.<id>.context_window` for catalog-capability resolution.
It maps known `reasoning` and `tool_call` booleans to the exact model override's
`supports_reasoning` and `supports_tools`. A known input modality list maps to
`supports_vision` by image membership: explicit empty/text-only lists mean false;
absent input modalities leave vision unknown, not false. These canonical
`supports_*` fields belong in `model_overrides`, not unused provider-model keys.
The provider-model boolean reader is a separate route-specific consumer; copying
an arbitrary `capabilities` object there does not patch catalog-derived facts.

Explicit per-model overrides win even when Hermes has conflicting catalog facts;
`_default` only fills catalog gaps and is not used by tmax. Source:
[`agent/models_dev.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/agent/models_dev.py#L575-L823)
and the route-context/capability readers in
[`config_providers.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/hermes_cli/config_providers.py#L510-L585).
Local tmax overrides win over its accepted catalog snapshot, never an unapplied
download. Known zero context blocks application because Hermes cannot represent
it; unknown context remains absent. Unsupported input/output limits and complete
modality lists are not written as invented Hermes fields. No alias aggregation
or current-winner snapshot is generated. Existing native effort, request budgets,
sampling, and unknown-model settings remain native-owned and are preserved.

The primary tmax mapping writes a named provider's `name`, `base_url`, `api_key`,
canonical `transport`, selected `models`, and explicitly selected
`model.provider/default/base_url`. It does not translate Pi metadata into Hermes
YAML or replace native catalog facts with fake zeros. It maps explicit qualified
auxiliary roles to native `auxiliary.<task>.provider/model` only when requested.
Native role names include `vision`, `compression`, `skills_hub`, `approval`,
`review`, `mcp`, `title_generation`, `memory_query_rewrite`, `tts_audio_tags`,
`voice_chat`, `triage_specifier`, `kanban_decomposer`, `profile_describer`,
`goal_judge`, `curator`, `monitor`, `background_review`, `moa_reference`,
`moa_aggregator`; adapter role support is the finite inspected subset, not every
arbitrary key. Old `web_extract`/`session_search` auxiliary blocks are ignored.
See [`config_defaults.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/hermes_cli/config_defaults.py#L726-L828).

Native `agent.reasoning_effort`, `reasoning_overrides`, service tier, and output
request settings are not metadata fields to synchronize. Context capacity does
not impose a new generation cap. The
[shared integration](../../specs/integrations/spec.md#agent-boundaries) and
[routing request-parameter](../../specs/model-routing/spec.md#request-parameters)
contracts remain authoritative; user defaults, model roles, and subagents are not
inferred from a provider definition.

### Finite native example

A synthetic endpoint fragment; it does not select the user's default or roles:

```yaml
providers:
  schema-fixture:
    name: schema-fixture
    base_url: http://127.0.0.1:17891/v1
    transport: chat_completions
    key_env: SCHEMA_FIXTURE_KEY
    models:
      fixture-model: {}
```

The adapter writes `api_key` into current-user-restricted `config.yaml` and
removes competing `key_cmd`/environment references for that provider. This
disclosed plaintext handoff also works with installations that ignore
`key_cmd` (including source revision `4d67ac617`). It does not import native
logins. Restore recovers the original fields, including previous references.

## Authentication

Native `auth.json` is under the active Hermes home. Common root fields are
`version`, `updated_at`, `providers` (provider-specific singleton state),
`credential_pool` (provider ID to rows), `active_provider`, and
`suppressed_sources`. Pool-only stores are also accepted. Writes are atomic;
new files are mode 0600 under a private directory. Do not treat these payloads as
portable auth templates. Source:
[`auth.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/hermes_cli/auth.py#L735-L843).

Pool rows have `id`, `label`, `auth_type` (`oauth` or `api_key`), `priority`,
`source`, `access_token`, optional refresh/expiry fields, status/error fields,
endpoint fields, counters/cooldowns, and opaque `extra`. The enclosing map owns
the provider ID. Pool strategies are `fill_first`, `round_robin`, `random`, and
`least_used`. Root fallback is **per provider**: profile-local auth shadows root
state; missing local auth can borrow root credentials. Refresh of a rotating
borrowed grant writes through to its authoritative source rather than making a
second profile-local refresher. Source:
[`credential_pool.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/agent/credential_pool.py#L40-L130).

For a named custom provider, explicit CLI key wins; otherwise `key_cmd` precedes
static `api_key`/`key_env`. The helper runs in the served profile's shell
environment with a 15-second timeout. It prints one bare token line or JSON
`access_token` with optional `expires_in`, `expiry`, `expiresOn`; multiple bare
lines are rejected. Hermes caches until 60 seconds before known expiry, or
remints after 900 seconds without TTL. It does not automatically remint on 401.
Main and auxiliary routes support this source. Source:
[`command_token_source.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/agent/command_token_source.py#L1-L160).
This differs from Pi/OMP `!` values and Codex executable auth.

Native `hermes auth add` owns the selected provider's key or OAuth flow;
`hermes auth logout` owns native logout. A command named auth does not establish
that a flow is OAuth: OpenRouter PKCE can produce an API-key row. Native pools
own account selection and refresh. Logout does not erase all possible environment
or command sources, revoke an upstream key, or restore tmax model settings.

`auth.adopt_external_logins` defaults true in Hermes. Claude Code/Codex external
stores can otherwise be adopted or refreshed. The tmax native-login mapping
sets it false to retain independent native ownership; the native interactive
external-import offer is separate and must not be used as a workaround. Single-use
rotating grants can invalidate another program when refreshed independently.
Hermes Anthropic PKCE can use `.anthropic_oauth.json`; Nous can mirror shared
root state; Qwen refresh belongs to its own CLI file. OS keychain borrowing is an
external provider-specific source, not Hermes's generic storage backend. Sources:
[`credential_sources.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/agent/credential_sources.py#L1-L40),
[`auth_commands.py`](https://github.com/NousResearch/hermes-agent/blob/848fc0a/hermes_cli/auth_commands.py#L203-L455),
and [official provider documentation](https://hermes-agent.nousresearch.com/docs/integrations/providers).

## Qualification

Retained installed acceptance observed rebuilt tmax apply, actual
`hermes config check` success, and exact-original-byte restore with native key
removal. A separate default-provider loopback run completed a terminal tool loop;
three inference requests used the selected synthetic key. Four native Ollama
metadata probes were unauthenticated and were not counted as inference-key
successes. A separate applied local gateway target emitted
`session_affinity_header: x-session-id`; native main/tool/auxiliary calls shared
the header, resume retained it, and a fresh session changed it. This does not
qualify authenticated alias continuity, native OAuth refresh, a hosted provider,
or subscription entitlement.

On 2026-10-24, isolated post-repair applied YAML was consumed by installed
`load_config_readonly`, `providers_dict_to_custom_providers`,
`get_custom_provider_context_length`, and
`agent.models_dev.get_model_capabilities(..., allow_network=False, config=...)`.
Route context and exact catalog overrides agreed: local false capabilities won
over conflicting native catalog facts, inherited metadata remained effective,
and unknown input left vision unknown. Native output limits remained native
catalog data; aliases received no concrete winner metadata. Zero context blocked
before writes. Reapply/three-way restore preserved native request preferences
and unknown-model settings and removed private key artifacts. These were
offline configuration/metadata readers, not inference, OAuth, entitlement, or
running-agent reload proof.

Apply/restore reported reload pending. Restart the selected Hermes home/profile;
existing sessions may retain their model. Inert discover used the source
provenance API and left observed config/state/dependency facts unchanged. Do not
run `hermes --version` as an allegedly inert schema check: its launcher attempted
an update in earlier qualification. No new bootstrap/update path is needed for
this reference.
