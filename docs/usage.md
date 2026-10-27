# CLI usage: providers, routing, and native agents

This guide uses supported command grammar, but credentials and upstream calls are
user-specific. CLI help is offline (`tmax help provider input`, for example); it
never creates a starter configuration. Management output is JSON by default;
`--format text` can follow a command when a human-readable view is preferred.

Ordinary provider/model/agent/network edits save desired settings without requiring
a running service. `provider inspect`, file-state reads, and configuration
`--dry-run` previews also work without credentials or network. A stopped service
has no loaded snapshot; an unidentified or inaccessible service is **unavailable**,
not proof that nothing is running or that no sessions are affected.

A provider is an independent upstream access instance; multiple providers can use
the same service or account without proving separate allowances. Each concrete
model belongs to one provider and is referenced as `provider-id/model-id`. An
agent target is an independently managed native scope. A model alias is named
configuration referencing existing models, with optional alias-local reasoning
effort per candidate; candidates have no independent identity or lifecycle.
Account identity and subscription information are optional facts, authentication
is provider configuration, and conversation bindings are runtime state. An
integration selects qualified behavior, not a universal service identity.
See the [domain vocabulary](../CONTEXT.md) and
[ownership contract](../specs/product/design.md#product-structure).

After an ordinary edit has saved successfully and completed required safety work,
exit 0 means the desired change is committed. Background activation may still
have a failed/unavailable warning. Check `save`, `file_revision`, `runtime`,
`activation`, and loaded revision fields; do not repeat an already committed
`provider add` or another create. Inspect `tmax status`, then use explicit
`tmax config reload` after resolving its blocker, or `tmax gateway restart` for a
saved listener-address change. Loaded state is not claimed to change until
activation actually succeeds. Catalog acceptance follows the same distinction.

Explicit reload/start/restart, shared-key revocation, native authorization or
application, requested autostart, and mandatory safety work retain nonzero failure
exits. If a save completed first, their incomplete result still identifies that
saved revision and the actual failed/partial boundary. Validation, persistence,
and revision conflicts are never converted into successful saves.

## Add one provider and model

Choose a supported integration with `tmax integration list` and
`tmax integration inspect INTEGRATION_ID`. For a custom gateway, supply an exact
protocol base URL instead; this example uses a reserved placeholder URL and must
be replaced with your own endpoint:

```sh
PROVIDER_ID=my-gateway
MODEL_ID=exact-upstream-model-id
BASE_URL=https://gateway.example/v1

tmax provider add "$PROVIDER_ID" --integration custom \
  --endpoint chat-completions "$BASE_URL"
tmax provider key set "$PROVIDER_ID"
tmax model add "$PROVIDER_ID/$MODEL_ID"
tmax provider inspect "$PROVIDER_ID"
tmax model inspect "$PROVIDER_ID/$MODEL_ID"
```

`provider key set` uses hidden terminal input by default. In a non-interactive
caller, `--key-stdin` explicitly selects one line from stdin; never put key bytes
in argv, environment variables, ordinary files, or command history.
This custom URL is a placeholder. No hosted key/account is supplied; configuration
and inspection do not establish endpoint reachability or hosted permission.

`model add` registers the exact upstream model ID under the local provider ID; it
does not sign in, select a native default, start inference, or change another
provider. Scalar overrides are literal values. False, zero, and an allowed empty
string remain explicit values; `--clear FIELD` restores inheritance/defaults.
For example, supported fields include `--set enabled false`,
`--set priority 0`, and `--set overrides.name ""`.

### Explicit gateway effort maps

After inspection establishes the actual supported upstream effort values, use
the [model effort-map commands](../specs/cli/models/spec.md#model-effort-maps) to
set, remove, or clear a protocol's input-name dictionary. The
[configuration example](../specs/configuration/example.toml) shows a synthetic
`max` -> `high` map with an explicit target declaration, not hosted qualification.
Targets must be positively declared in effective accepted metadata; do not guess
support from a model name or a reasoning boolean. Removing an entry restores
identity, not input disabling. Maps supply no omitted effort, native defaults,
thinking switches, or budgets, and Messages maps are rejected.

These maps run only in the gateway. Direct native entries keep actual upstream
capabilities. For aliases, fixed candidate effort takes precedence over a model
map, then unmapped inputs use identity. Alias selectors expose the intersection
of candidate input sets after these settings, not a winner's raw effort names.
Changing only effort never silently selects another backend: the gateway selects
using other constraints, then validates effort on that backend or rejects before
publishing a binding or dispatching. Explicitly reapply affected native entries
and reload the tool after changing advertised inputs.

Local verification on 2026-10-27 used the release binary with an isolated OS-home
store and synthetic loopback upstreams. With higher-priority A declaring
`low`/`high` and B declaring `high`/`max`, unmapped `max` rejected on A without
upstream dispatch; an explicit A `max` -> `high` map forwarded to A for both Chat
Completions and Responses. The run also exercised omitted effort, fixed override
precedence, protocol isolation, removal/clear, and real Pi file application:
gateway alias inputs included the mapped common names while direct entries
retained upstream-only names. No hosted inference, provider entitlement, or
running native-agent consumption was established by this verification.

### Existing configuration with removed tier fields

The current `~/.tmax/tmax.toml` authority strictly rejects the former per-model
and model-alias `tier` fields; tmax does not automatically convert or rewrite
that authority. If you choose to update it manually, remove each per-model `tier` field and replace each
alias's `tier` field with a nonempty `candidates` list of exact, already registered
`provider-id/model-id` names that you choose. Keep the alias ID and protocol.
Names such as `L0`, `L1`, and `coding` have no built-in meaning or membership.
Keep model priorities, time schedules, and ordered rules unchanged. Distinct
native/vendor service tiers, pricing/context tiers, and subscription-plan metadata
are not removed fields.

Do not edit or delete routing-state or session JSON for this conversion. tmax
normalizes supported historical operational records internally without read-time
writes; corrupt records fail closed. This does not convert the desired authority,
clear retained sessions, or remove permanent continuation blockers.

## DeepSeek model discovery

For an official DeepSeek provider whose key is already configured, explicitly
refresh its model list without running inference:

```sh
tmax provider refresh personal --scope models
tmax provider inspect personal
```

This authenticated operation reads `https://api.deepseek.com/models` and retains
exact upstream IDs as an observation. It does not register models, download or
apply catalog metadata, verify inference, or incur a tmax paid probe. Register
only the desired exact IDs separately with `model add`. A failed refresh retains
the last good list separately from its failed attempt. Auth and quota refreshes
use the separate official balance endpoint; neither discovery nor balance
establishes the account owner's identity.

## OpenCode Go observations

Configure each independently selected account under its own provider ID, supply
its key through explicit protected input (such as `--key-stdin`), and refresh
the desired scope:

```sh
tmax provider add go-a --integration opencode-go
tmax provider key set go-a
tmax provider refresh go-a --scope quota
tmax provider inspect go-a
```

Repeat with a distinct provider ID and its own key for another account. Inspection
does not contact the provider. Go usage observations contain consumed percentages
and reset announcements only; they do not report exact remaining allowance,
workspace identity, entitlement, or a renewal date. Their source freshness is
unqualified: local retrieval time is not a provider measurement time, and tmax
does not infer staleness from age. These observations do not create priority-rule
inputs or combine accounts' allowance. Renewal remains unknown unless explicitly
known subscription information supplies it; a monthly plan alone is not a date.
See the owning
[Go quota contract](../specs/integrations/opencode-go/spec.md#quota-and-renewal)
for the qualified current response boundary and source evidence.

An unavailable or failed requested refresh returns exit 4 and retains the latest
attempt separately from any prior applicable fact. It does not establish revoked
authorization or exhausted quota. Inference still needs its own qualified model,
protocol, client identity, and conversation session; a successful usage read does
not prove that path.

## Local Overview and evidence

The Web Overview groups the provider and agent evidence already available through
local read operations. It does not refresh providers, resolve secrets, sign in,
start the gateway, or apply native files. An explicitly requested refresh or native
change remains a separate task.

Treat saved configuration, loaded state, applied files, and effective native
runtime as separate facts. A configured authentication method is not verified
authentication; an applied file is not proof that a running agent uses it. Missing,
unavailable, stale under an explicitly declared rule, and freshness-unqualified
observations are different states. Do not infer validity from retrieval age or
turn unknown quota into zero. Keep consumed-only observations provider-local; do
not pool their percentages or allowances. Show a renewal date only when known,
and keep a past date distinct from an unknown date.

### Web language

The redesigned workspace uses Simplified Chinese labels, including 网关 for
Gateway. See the [Web workspace guide](web.md) for its task areas and operation
feedback. Stable IDs, CLI commands, configuration names, and raw results retain
their documented spelling. The CLI remains English.

## Explaining models and applying metadata

Use `tmax model explain ID --protocol PROTOCOL` for an effect-free explanation of
a public qualified concrete or alias ID. Add `--target TARGET` to assess it for a
declared native target, optionally with `--agent-provider ENTRY` for an existing
entry. For an undeclared native scope, use `--agent AGENT --path PATH` instead;
this does not register or import the scope. Do not mix the two context forms.
With an upstream-source entry, use its exact unqualified upstream model ID;
without an entry, concrete IDs must be qualified. No saved native model, default,
or role selection is required. For example:

```sh
tmax model explain L0 --protocol chat-completions --target pi-main
tmax model explain L0 --protocol chat-completions \
  --agent omp --path "$HOME/.omp/agent"
tmax model explain PUBLIC_MODEL --protocol responses --state loaded
```

These examples assume the alias `L0` and target `pi-main` already exist. The alias
uses only its configured candidates; its name supplies no candidate selection.
Explanation data identifies `gateway`, `agent_target`, `agent_provider`, or
`native_scope` context and reports `requested` as exactly `{id, protocol}`.

`--state loaded` explains the resident loaded view, not pending saved edits. Model
explanation does not retrieve secrets, sign in, run inference, reserve a session,
or apply native files. It assesses only qualified local evidence; a readable tool
or configured login does not verify native authentication, entitlement, or access.

For an existing gateway request, use the opaque `scope_handle` returned in retained
request decision evidence:

```sh
tmax model explain PUBLIC_MODEL --protocol PROTOCOL --session HANDLE
```

The command reports the retained destination or blocker without fresh ranking;
it does not reserve a session, migrate a conversation, or unblock continuation.
The handle must match the public model ID and protocol. Session mode rejects an
upstream-source entry.

Catalog adoption is also explicit. `catalog apply --revision REVISION` applies a
validated downloaded snapshot; its preview/result compares accepted and candidate
effective metadata after overrides and reports affected concrete selections,
aliases, and native projections. Review `reapply_required`, blocked
unrepresentable facts, and any gateway-route revalidation even when native files are
unchanged. The result distinguishes accepted-before, candidate, and accepted-after
revisions; a preview does not advance accepted metadata. Activation reports the
actual service outcome. Catalog apply does not implicitly apply native files or
change endpoint, enablement, or destination settings.

## Native login and logout boundaries

Native sign-in and sign-out run through a qualified native command; they are not
proof that tmax has OAuth access or provider entitlement. Before invoking the owner
command, a protected preflight durably advances authorization lineage for both
saved and loaded views. It invalidates dependent inputs (including manual quota)
and prunes their observations, while retaining subscription and other manual
declarations. An `authorization_invalidation` failure is pre-native: the owner
command has not run. A `native_effect_uncertain` or partial native result means do
not assume the old authorization or observations were restored and do not retry
blindly. Inspect the same provider and native scope, then recover through that
scope's qualified owner; never copy native tokens into tmax or another agent.

## Gateway naming upgrade

Use `tmax gateway ...`, `tmax agent provider add ... --gateway`, and the
`[gateway]` configuration table. Structured operation names, result fields,
reason codes, Web navigation (`#gateway`), and the explicit key endpoint
(`/api/gateway-key/reveal`) use the same name. The old model-service command
and flag spellings are no longer accepted; update scripts and API clients.

Existing configuration and shared-key records remain readable. Reads and previews
preserve the saved file; the next explicit configuration save canonicalizes the
two historical field names while retaining key references and values. Supplying
old and new names together is rejected. See the
[configuration contract](../specs/configuration/spec.md#format-and-authority).
The independent outbound-network command remains `tmax network proxy URL`.

## Gateway-only access

The local gateway is independent of managed Pi/Codex targets. Direct clients do not
need an `agent` declaration or native apply. A concrete qualified model can be
used directly; aliases are optional.

Available model protocols follow the provider's resolved endpoints, not a
per-model approval list. An endpoint declaration is not proof of upstream model
support: tmax forwards the selected native protocol and returns the upstream
result. It does not translate protocols, switch models, or retry another
destination when the upstream rejects a combination.

The default public ID can be the concrete model:

```sh
PUBLIC_ID="$PROVIDER_ID/$MODEL_ID"
```

To use a stable alias instead, create it before starting the gateway:

```sh
PUBLIC_ID=my-model
tmax model alias add "$PUBLIC_ID" --protocol chat-completions \
  --candidate "$PROVIDER_ID/$MODEL_ID"
```

To pin an effort for a particular candidate, update that candidate's alias
entry. The options are repeatable for distinct candidates; each effort must be
listed in that concrete model's effective positive `reasoning_options` metadata
(`type = "effort"` with explicit `values`). `reasoning = true` alone is not
enough. A configured value takes precedence over both caller effort and the
model map when that candidate is selected. Without a fixed override, an explicit
model map applies to supplied input, otherwise identity or omission is preserved.
The candidate advertises only its fixed upstream name; common alias choices are
intersected after maps and fixed restrictions. Effort never chooses a different
backend. Chat Completions and Responses support this; Messages does not.

```sh
CANDIDATE="$PROVIDER_ID/$MODEL_ID"
tmax model alias update "$PUBLIC_ID" --protocol chat-completions \
  --candidate-reasoning-effort "$CANDIDATE=$SUPPORTED_EFFORT"
tmax model alias update "$PUBLIC_ID" --protocol chat-completions \
  --clear-candidate-reasoning-effort "$CANDIDATE"
```

Set `SUPPORTED_EFFORT` to a nonempty value exposed for that concrete model.
These alias-specific options do not define a global default or arbitrary
request parameters.

Then configure and start the loopback listener explicitly:

```sh
tmax gateway configure --listen 127.0.0.1:17840
tmax gateway start
tmax gateway status
```

Initial `gateway configure` provisions the sole shared local key in a current-user
restricted file under `~/.tmax`; the ordinary configuration stores its reference.
The key file is plaintext, not encrypted. `gateway start` is a separate explicit
lifecycle action. Provider-key setup prompts invisibly on an attached terminal
or accepts `--key-stdin`; the Web form contains a provider-key field. Use explicit
key input for automation, never argv, environment, or
ordinary configuration files. Neither the CLI nor Web requires a daemon
installation. Provision the shared key to a client only through the qualified
`gateway key show` delivery flow. Ordinary inspection does not reveal it. All
current-key holders share the same published model list; no client registration
is required.

tmax resolves the OS account's home, not an override of the `HOME` environment
variable. On Unix, a home that yields an unrepresentable control-socket pathname
blocks startup with `resident_control_path_invalid`, the actual path, and the
platform's representation error before launching a resident. Use an OS account
whose home permits that pathname; tmax does not relocate its store or fall back
to a shorter endpoint. No listener, credentials, or native configuration are
changed by this startup blocker.

On Windows, the same commands launch a detached native resident with private
same-user named-pipe control. `tmax web open` opens the system browser;
`--no-browser` starts management without opening it. Closing the browser or
launching terminal does not stop inference. `tmax gateway autostart enable` opts
into current-user login startup, and `disable` removes only tmax's owned startup
entry without stopping the gateway. Startup reads the same current-user-restricted
secret files. Unsupported storage-format records return `storage_version_unsupported` or
`storage_record_invalid` and remain unchanged. Re-enter the provider key with
`provider key set` or replace the shared key with `gateway key replace`. Recovery
contents in unsupported formats are unavailable to current recovery operations.

If the current CLI reports `resident_incompatible`, the resident is too old for
this executable's management protocol. Use the running resident's matching
pre-cutover CLI (`tmax gateway stop`, then `tmax web stop`) or its existing Web
controls to perform the explicit stops before starting the current executable;
the new CLI will not take over or terminate an incompatible resident.

The gateway accepts same-protocol requests and forwards to the configured provider;
it does not translate protocols. Configure a real provider credential before
expecting upstream completion. Synthetic loopback checks qualify local wiring
only. The exact hosted scenario and its native request-control qualifications are
recorded in the owning [Go evidence section](../specs/integrations/opencode-go/spec.md#sources-and-evidence-status);
they do not establish every client's model access or entitlement.

## Manual quota inputs and ordered rules

Register a manual input after its covered model exists. Scope is caller-declared;
it is not a native path. The concrete timestamp values below are a synthetic
fixture, not a current observation; use a fresh real observation, future validity
end, and later reset instant when recording.

```sh
INPUT=weekly-quota

tmax provider input add "$PROVIDER_ID" "$INPUT" \
  --kind quota --source manual --scope weekly \
  --model "$MODEL_ID" --max-age 30d

tmax provider input record "$PROVIDER_ID" "$INPUT" \
  --observed-at 2026-10-23T13:00:00Z \
  --valid-until 2026-10-24T13:00:00Z \
  --reset-at 2026-10-25T00:00:00Z \
  --remaining 1 --total 3 --unit requests
```

Quota can instead be recorded as `--remaining-percent DECIMAL`; do not combine
both representations. Counters are exact and share one documented unit/window.
Manual input requires no credentials, but it remains user-provided evidence—not
verified provider usage. `provider input inspect` distinguishes current data,
freshness, unavailable/cleared state, dependencies, and last attempt.

Rules are model-local, append in explicit order, and compare retained exact
facts; priority values do not sort rule order. One third is at least 33.33%, but
below 33.34%:

```sh
tmax model rule add "$PROVIDER_ID/$MODEL_ID" threshold-33-34 \
  --input "$INPUT" --priority 20 --reset-within 14d \
  --remaining-at-least-percent 33.34
tmax model rule add "$PROVIDER_ID/$MODEL_ID" threshold-33-33 \
  --input "$INPUT" --priority 10 --reset-within 14d \
  --remaining-at-least-percent 33.33
tmax model rule move "$PROVIDER_ID/$MODEL_ID" threshold-33-33 \
  --before threshold-33-34
tmax model rule list "$PROVIDER_ID/$MODEL_ID"
tmax model rule inspect "$PROVIDER_ID/$MODEL_ID" threshold-33-33 \
  --at 2026-10-23T13:14:10Z
```

For exact input counters `1 / 3 requests`, the 33.34% rule was false and the
33.33% rule matched. At an inspection instant after `valid-until`, both became
`unknown` with `expired`, not zero quota. `provider input clear` publishes an
unavailable revision; replacing the declaration with a different source
invalidates data from the old declaration. No input refresh happens implicitly.

Use `--dry-run` on a mutation to preview its requested change without saving,
reading a secret, or publishing data. For example:

```sh
tmax provider input record "$PROVIDER_ID" "$INPUT" \
  --observed-at 2026-10-23T13:00:00Z \
  --valid-until 2026-10-24T13:00:00Z \
  --reset-at 2026-10-25T00:00:00Z \
  --remaining-percent 50 --dry-run
tmax model rule add "$PROVIDER_ID/$MODEL_ID" preview-rule \
  --input "$INPUT" --priority 10 --reset-within 1d \
  --remaining-at-least-percent 50 --dry-run
```

Dry-run output uses `dry_run_ready`/`dry_run`, `published: false` for an input
record, and `save: not_executed` for config edits. It is a preview, not a
reservation or proof of upstream authorization.

## Calendar edits and revision guards

Calendar schedules are local-time rules; set the model timezone first. For an
indexed replace/remove, read the current revision from `model inspect`, use the
shown zero-based index, and re-read after every successful intervening write:

```sh
tmax model update "$PROVIDER_ID/$MODEL_ID" --set timezone America/New_York
tmax model schedule add "$PROVIDER_ID/$MODEL_ID" \
  --priority 7 --weekday mon --start 09:00 --end 17:00
# Read the revision below, then replace CURRENT_FILE_REVISION with that exact value.
tmax model inspect "$PROVIDER_ID/$MODEL_ID"
REVISION=CURRENT_FILE_REVISION
tmax model schedule remove "$PROVIDER_ID/$MODEL_ID" 0 \
  --if-revision "$REVISION"
```

`--if-revision` must match the current saved file revision exactly. A stale guard
returns exit 3 / `inputs_changed` before changing the indexed entry; an intervening
successful write means the old revision is no longer valid. Input-data guards are
separate: use `--if-input-revision` with `provider input record|clear`.

## Pi target: apply, reload, restore

Native configuration is explicit and separate from gateway startup. Use an existing
Pi scope outside `~/.tmax`; a path inside tmax-owned storage is rejected. This is
the direct-provider command sequence; scope and IDs are examples:

```sh
PI_SCOPE="$HOME/.pi/agent" # use the existing scope you intend tmax to manage
TARGET=pi-main

tmax agent add "$TARGET" --agent pi --path "$PI_SCOPE"
tmax agent provider add "$TARGET" upstream --provider "$PROVIDER_ID"
tmax agent model add "$TARGET" upstream "$MODEL_ID" \
  --protocol chat-completions
tmax agent default set "$TARGET" upstream "$MODEL_ID" \
  --protocol chat-completions
tmax agent apply "$TARGET"
```

Application writes only the requested native scope; it does not start tmax
inference or make a paid probe. `tmax agent apply` reads referenced provider keys
from tmax's current-user-restricted files and uses the native adapter's declared
credential handoff. Unsupported storage-format key records return `storage_version_unsupported` or
`storage_record_invalid` and remain unchanged; re-enter the key with
`provider key set` before applying. Unsupported recovery contents are unavailable
to current recovery operations. The `awaiting_native_reload` state means the native
tool must reload/restart before the change is effective.

For qualified Pi and OMP concrete selections, known effective model context/output
limits, reasoning capability, and supported text/image input metadata are written
using the versioned native schema. False values remain false; unknown metadata is
not manufactured, and an unrepresentable explicit token limit blocks before native
writes. Alias metadata is not copied from whichever candidate currently wins.
See [versioned agent schemas](agents/README.md) for the distinct native fields,
authentication precedence, scopes, and qualification limits.

Model capability limits are not generation budgets. Native sampling, thinking
effort, and requested output settings remain owned by the native agent; applying
metadata does not cap a request to make a capability rejection pass. Use the
agent's supported request controls when deliberately bounding an authorized probe.

```sh
tmax agent restore "$TARGET"
```

Restore preserves later user edits and reports when the native tool must reload.
With Pi 1.2.0, a private synthetic-provider fixture's apply and restore each
returned exit 3 / `awaiting_native_reload`; the installed Pi parser recognized
the selected model. This proves local native configuration parsing, not hosted
credentials, entitlement, or permission.

For a Pi client that should use the gateway, add an explicit gateway entry, select
the public model ID, set it as default, then apply the target:

```sh
tmax agent provider add "$TARGET" local --gateway
tmax agent model add "$TARGET" local "$PUBLIC_ID" --protocol chat-completions
tmax agent default set "$TARGET" local "$PUBLIC_ID" --protocol chat-completions
tmax agent apply "$TARGET"
```

The gateway itself must already be configured and explicitly started; this native
target path is optional. Pi 1.2.0 completed a same-protocol Chat Completions
round-trip through tmax to a private synthetic loopback provider using a
synthetic key. This is local gateway/native wiring evidence, not hosted access.

`model probe` is a separate potentially paid operation. Choose a transport
listed by `integration inspect` and invoke it explicitly; apply/restore never
run it. Hosted permission is verified outside this guide.

For the complete contract, see [CLI specs](../specs/cli/spec.md),
[provider input specs](../specs/cli/providers/inputs/spec.md),
[model routing](../specs/model-routing/spec.md), and
[Pi/agent target specs](../specs/cli/agents/spec.md).

## DSH target: home-wide native application

DeepSeek Harness uses the same target/provider/model operations as the Pi example,
with `--agent dsh` and the existing DSH home as its scope. Discovery uses nonblank
`DSH_HOME`, otherwise the OS home directory's `.dsh`. tmax edits the home's
`cordis.patch.yml`, not any profile or launcher-owned `cordis.yml`. It preserves
unrelated HOME-level provider fields and expressions without enumerating,
selecting, creating, or managing profiles. Home entry `config` replaces the
complete lower-layer configuration; profile routes and defaults are not merged
or automatically copied into the home configuration. Runtime overlays remain
outside tmax's ownership.

```sh
DSH_SCOPE="${DSH_HOME:-$HOME/.dsh}"
tmax agent discover --agent dsh
tmax agent add dsh-main --agent dsh --path "$DSH_SCOPE"
tmax agent provider add dsh-main upstream --provider "$PROVIDER_ID"
tmax agent model add dsh-main upstream "$MODEL_ID" --protocol chat-completions
tmax agent apply dsh-main --dry-run
tmax agent apply dsh-main
tmax agent inspect dsh-main
tmax agent restore dsh-main --dry-run
```

Model-only application requires no default and writes no `agent-default-model`
entry or thinking preferences. An explicit default changes provider/model only,
preserving existing HOME-level effort settings rather than importing profile
defaults. Native route APIs support all three protocols; incompatible
protocol/base selections cannot share one provider entry. Executed apply hands
the key to the target home's private native `.credentials.yaml` `refs` map and
uses a stable `TMAX_...` reference in the adjacent home patch. It enables the
native credential service in the HOME patch for headless consumption without
bundle inspection, exported secrets, or a tmax launcher. Preview and inspect
never resolve or return key values.
Output capability is intentionally not projected because DSH's explicit
`maxTokens` would also change a per-request default.

Reload DSH instances using that home after apply/restore. File writes are not
runtime or provider-access evidence; historical profile-path qualification does
not establish home-wide, multi-profile behavior. DSH has no qualified native
login or Go session mechanism. See the [DSH guide](agents/dsh.md) for the source
revision, credential ownership, exact reasoning-effort mapping, gateway
limitations, and the explicit restore command. Skill installation remains
shared `.agents` or Hermes-only.
