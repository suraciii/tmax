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

The proxy accepts same-protocol requests and forwards to the configured provider;
it does not translate protocols. Configure a real provider credential before
expecting upstream completion. Pi 1.2.0 completed a same-protocol request through
tmax to a private synthetic loopback provider; this verifies local wiring, not a
hosted account, credential, or entitlement.

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