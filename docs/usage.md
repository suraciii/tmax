# CLI usage: providers, routing, and Pi

This guide uses supported command grammar, but credentials and upstream calls are
user-specific. CLI help is offline (`tmax help provider input`, for example); it
never creates a starter configuration. Management output is JSON by default;
`--format text` can follow a command when a human-readable view is preferred.

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

## DeepSeek model discovery

For an official DeepSeek provider whose key is already configured, explicitly
refresh its model list without running inference:

```sh
tmax provider refresh personal --scope models
tmax provider inspect personal
```

This authenticated operation reads `https://api.deepseek.com/models` and retains
exact upstream IDs as an observation. It does not register offerings, download or
apply catalog metadata, verify inference, or incur a tmax paid probe. Register
only the desired exact IDs separately with `model add`. A failed refresh retains
the last good list separately from its failed attempt. Auth and quota refreshes
use the separate official balance endpoint; neither discovery nor balance
establishes the account owner's identity.

## OpenCode Go observations

Configure each independently selected account under its own provider ID, supply
its key through protected input, and explicitly refresh the desired scope:

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
start the proxy, or apply native files. An explicitly requested refresh or native
change remains a separate task.

Treat saved configuration, loaded state, applied files, and effective native
runtime as separate facts. A configured authentication method is not verified
authentication; an applied file is not proof that a running agent uses it. Missing,
unavailable, stale under an explicitly declared rule, and freshness-unqualified
observations are different states. Do not infer validity from retrieval age or
turn unknown quota into zero. Keep consumed-only observations provider-local; do
not pool their percentages or allowances. Show a renewal date only when known,
and keep a past date distinct from an unknown date.

## Explaining models and applying metadata

Use `tmax model explain ID --protocol PROTOCOL` for an effect-free explanation of
a public model. To assess a native entry, add both
`--target TARGET --agent-provider ENTRY`. Resolve a tier in a declared target with
`tmax model explain --tier L0 --protocol PROTOCOL --target TARGET`, or inspect an
undeclared native path with `--agent AGENT --path PATH`. For example:

```sh
tmax model explain --tier L0 --protocol chat-completions --target pi-main
tmax model explain --tier L0 --protocol chat-completions \
  --agent omp --path "$HOME/.omp/agent"
tmax model explain PUBLIC_MODEL --protocol responses --state loaded
```

`--state loaded` explains the resident loaded view, not pending saved edits. Model
explanation does not retrieve secrets, sign in, run inference, reserve a session,
or apply native files. It assesses only qualified local evidence; a readable tool
or configured login does not verify native authentication, entitlement, or access.

For an existing proxy request, use the opaque `scope_handle` returned in retained
request decision evidence:

```sh
tmax model explain PUBLIC_MODEL --protocol PROTOCOL --session HANDLE
```

The command reports the retained destination or blocker without fresh ranking;
it does not reserve a session, migrate a conversation, or unblock continuation.
Session mode rejects an upstream-source entry and tier context.

Catalog adoption is also explicit. `catalog apply --revision REVISION` applies a
validated downloaded snapshot; its preview/result compares accepted and candidate
effective metadata after overrides and reports affected concrete selections,
explicit/tier aliases, and native projections. Review `reapply_required`, blocked
unrepresentable facts, and any proxy-route revalidation even when native files are
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

## Proxy-only access

The local proxy is independent of managed Pi/Codex targets. Direct clients do not
need an `agent` declaration or native apply. A concrete qualified model can be
used directly; aliases are optional.

The default public ID can be the concrete offering:

```sh
PUBLIC_ID="$PROVIDER_ID/$MODEL_ID"
```

To use a stable alias instead, create it before starting the proxy:

```sh
PUBLIC_ID=my-model
tmax model alias add "$PUBLIC_ID" --protocol chat-completions \
  --candidate "$PROVIDER_ID/$MODEL_ID"
```

Then configure and start the loopback listener explicitly:

```sh
tmax proxy configure --listen 127.0.0.1:17840
tmax proxy start
tmax proxy status
```

Initial `proxy configure` provisions the sole shared local key in protected
storage; `proxy start` is a separate explicit lifecycle action. On platforms that
require unlock, use the documented inherited readable descriptor form, e.g.
`--unlock-fd FD`; `FD` is a descriptor number, never a secret or path. Provision
the shared key to a client only through the protected `proxy key show` delivery
flow. Ordinary inspection does not reveal it. All current-key holders share the
same published model list; no client registration is required.

tmax resolves the OS account's home, not an override of the `HOME` environment
variable. On Unix, a home that yields an unrepresentable control-socket pathname
blocks startup with `resident_control_path_invalid`, the actual path, and the
platform's representation error before launching a resident. Use an OS account
whose home permits that pathname; tmax does not relocate its store or fall back
to a shorter endpoint. No listener, credentials, or native configuration are
changed by this startup blocker.

The proxy accepts same-protocol requests and forwards to the configured provider;
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

Rules are offering-local, append in explicit order, and compare retained exact
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

Calendar schedules are local-time rules; set the offering timezone first. For an
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

Native configuration is explicit and separate from proxy startup. Use an existing
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
inference or make a paid probe. `tmax agent apply` uses supported platform
protection or permitted hidden terminal unlock. Only when explicit unlock is
required and the launcher has supplied a readable inherited pipe at FD 3, use
`tmax --unlock-fd 3 agent apply "$TARGET"` instead; never name an FD that is not
open or pass unlock bytes in argv, environment, or files. `awaiting_native_reload`
means the native tool must reload/restart before the change is effective.

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

For a Pi client that should use the proxy, add an explicit proxy entry, select
the public model ID, set it as default, then apply the target:

```sh
tmax agent provider add "$TARGET" local --proxy
tmax agent model add "$TARGET" local "$PUBLIC_ID" --protocol chat-completions
tmax agent default set "$TARGET" local "$PUBLIC_ID" --protocol chat-completions
tmax agent apply "$TARGET"
```

The proxy itself must already be configured and explicitly started; this native
target path is optional. Pi 1.2.0 completed a same-protocol Chat Completions
round-trip through tmax to a private synthetic loopback provider using a
synthetic key. This is local proxy/native wiring evidence, not hosted access.

`model probe` is a separate potentially paid operation. Choose a transport
listed by `integration inspect` and invoke it explicitly; apply/restore never
run it. Hosted permission is verified outside this guide.

For the complete contract, see [CLI specs](../specs/cli/spec.md),
[provider input specs](../specs/cli/providers/inputs/spec.md),
[model routing](../specs/model-routing/spec.md), and
[Pi/agent target specs](../specs/cli/agents/spec.md).