# Authentication and local-secret commands

This child owns authentication/protected-input grammar and secret inventory
results. [Product authentication](../../../product/spec.md#providers-and-authentication)
owns identity, native effects, and refresh coordination;
[Configuration](../../../configuration/spec.md#retained-secret-discovery) owns
protected storage and discoverability. Use [common CLI rules](../../spec.md),
[configuration changes](../../configuration/spec.md), and
[shared results](../../results/spec.md).

## Provider authentication and local secrets

| Authentication boundary | Contract |
| --- | --- |
| Provider and source | Operate on a declared provider; login uses a qualified source. Methods and native scopes follow [provider contracts](../../../integrations/spec.md#provider-specific-contracts). |
| Desired state | Preserve subscription/unrelated settings; use ordinary save/activation/autostart boundaries when references change. Never apply native models or settings implicitly. |
| Native source | The selected native source performs supported sign-in, storage, and refresh. Do not import credentials or grant extra local permissions. |

| Command | Requested boundary |
| --- | --- |
| `provider key set ID [--key-stdin \| --key-ref REF]` | Select supported API-key authentication. Read/store a new protected key, or save an existing qualified provider-owned reference without resolving its secret. Replace the auth table and remove old-method fields. |
| `provider auth set ID --method login --via SOURCE [--session-ref REF]` | Declaratively replace the full auth table with explicit `via` and optional source-scoped session reference; no sign-in or credential import. |
| `provider login ID [--via SOURCE]` | Select the source through [login source selection](#login-source-selection), execute qualified interactive sign-in, then attempt reference save/activation. |
| `provider logout ID` | Sign out through the configured supported `via` and report affected native/shared scopes. Preserve provider/subscription declarations and identity comparison basis. |
| `provider auth clear ID` | Remove active auth configuration, not stored keys, native sessions, or upstream access. |
| `secret list` | Discover safe reference/ownership/cleanup metadata, including orphans, without unlock/decryption or a valid config prerequisite. |
| `secret cleanup REF` | Delete that exact retained tmax-owned local secret only after current safety checks. |

All mutations support dry-run. Reference-changing operations accept the common
optional file revision guard. Effect-free `secret list` rejects dry-run.

| Authentication operand or state | Contract |
| --- | --- |
| `SOURCE` | `tmax` or a declared `TARGET`, subject to supplier qualification; distinct from provider-input source enums. `tmax` is reserved and cannot be a target ID. |
| Saved login authentication | [Schema](../../../configuration/spec.md#authentication) requires `auth.via`; an absent whole auth table remains valid incomplete provider setup. |
| `provider auth set` | Require explicit `--via`; never reuse a configured source. Omitted `--session-ref` removes the old reference, including on source change. It creates a configuration reference, not a sign-in. |
| Key/session references | Opaque nonsecret handles scoped to their credential store/source, not token-file paths, key bytes, or native credential imports. Selection cannot bypass ownership, identity, or supplier restrictions. |
| Full auth replacement | Remove unused old-method fields; retain old stored secrets/sessions. |

Without either key option, use hidden terminal key input. `--key-stdin` explicitly
supplies one nonempty UTF-8 line, excluding its terminator, without echo/logging.
`--key-ref` instead saves the existing reference without consuming a key or
unlocking the store merely to edit config. The two options are mutually exclusive.
Noninteractive key entry requires `--key-stdin`; never silently consume a pipe.

Stdin is descriptor 0. Key stdin cannot combine with `--unlock-stdin`. Use platform
protection, permitted hidden terminal unlock, or a qualified separate inherited
protected pipe through `--unlock-fd FD`. The same descriptor cannot serve both
inputs. Unsupported pipe input returns `unsupported_protected_input`, not a
clear-file/argv fallback. Never persist clear supplied input, create a clear-secret
file, or copy reusable unlock material outside `~/.tmax`. Dry-run reads no stream,
prompts, unlocks, or resolves references; it cannot verify key/upstream success.

### Login source selection

[Product login-source selection](../../../product/spec.md#login-source-selection)
is the sole source-resolution flow for `provider login`.

| Command condition or outcome | CLI result |
| --- | --- |
| Product flow cannot select a source | `blocked` / `login_source_required` / exit 3, `data.via = null`, qualified `data.login_sources`, and no effects. |
| Selected source is invalid/unsupported | Report the invalid reference or qualification blocker before effects under the product flow. |
| `--non-interactive` requires interactive sign-in | `blocked` / `interactive_required` / exit 3 before sign-in launch. |
| `--dry-run` | Return selected source, account selection, native/shared consumers, recovery, and deferred credential/remote checks without sign-in, protected input, or any local/native effect. |
| Explicit `--via` changes method/source | Apply [authorization replacement](#authorization-replacement); never retain a previous source's session reference. |
| Native sign-in, save, activation, or startup reconciliation fails | Report actual credential/native effects separately from save/activation outcomes. Preserve [native-effect safety](../../../product/spec.md#providers-and-authentication), even if the auth-reference save never happened. |
| Changed/uncertain native access before failure/cancellation | Exit 4 with affected scopes and recovery; only cancellation known to precede every effect exits 5. |

| Authentication result field | Meaning |
| --- | --- |
| `data.via` | Selected/configured login source for login, login-auth replacement, logout, and provider inspection; null when unselected or inapplicable. |
| `data.login_sources` | Qualified source strings on `login_source_required`; choices are not an inferred selection, permission grant, or credential adoption. |

Examples: `tmax provider key set deepseek --key-stdin`,
`tmax provider key set deepseek --key-ref REF`, `tmax provider auth clear deepseek`,
`tmax secret list`, and `tmax secret cleanup REF --dry-run`. Use a returned qualified
reference. `tmax provider key set deepseek --key-stdin --unlock-fd 3 --non-interactive`
requires actual platform-qualified inherited pipe 3; help describes supported
alternatives without secret argv or helper files.

## Authorization replacement

Naming the existing provider is sufficient to request credential/reference/source
replacement when upstream identity cannot be established. Keep that identity
unknown; the command does not verify sameness or make old conversations safe.
Known different upstream identity requires another provider. Retain the last
comparison basis across auth clear/sign-out so remove/re-add cannot bypass that
rule. Credential changes invalidate prior credential-derived observations and
block unsafe existing-session continuation under the product contract.

Saved manual changes use the same rules on reload/start/restart without a special
confirmation operand or approval gate. Invalid/known-different changes remain
blocked; unknown identity is not silently relabeled verified. Separate invocations
compare current authorization rather than an earlier preview. Same-identity
refresh obeys [credential-refresh coordination](../../../product/spec.md#credential-refresh-coordination);
replacement creates no second refresh maintainer or background sign-in path.

## Shared proxy key

The sole [model proxy](../../../product/spec.md#model-proxy) owns one protected
local access key, distinct from upstream credentials. Explicit initial
`proxy configure --listen ADDRESS` provisions it once; ordinary start/reload,
configuration edits, and native apply never generate a missing key implicitly.
No Agent target, client registration, permission role, or private allowlist is
required. Every current-key holder uses the same derived published model list.

| Command | Requested boundary |
| --- | --- |
| `proxy key replace` | Generate/store a replacement, save its `key_ref`, and activate it when inference is running; never update native/unmanaged clients implicitly. |
| `proxy key show [--output-fd FD]` | Explicit sensitive retrieval of the configured saved shared key through protected delivery, never ordinary inspection or upstream-secret export. |

Replace supports dry-run and the optional file revision guard. Validate current
configuration/ownership before provisioning. Report creation, save, and activation
separately, including any orphan reference for `secret cleanup` after failed save.
Activation rejects the old key on subsequent requests; admitted requests retain
their captured snapshot. Report known managed native-provider dependencies and
required explicit apply/reload. Unmanaged clients are unknown/not enumerable and
must be updated independently. Replacement preserves retained conversation
bindings, destinations, and blocked state; the key is never a session namespace.
Target/native-entry rename/removal or native model-picker changes cannot revoke
proxy access or break those bindings. Separate upstream login-source/credential
dependencies still obey supplier safety.

Show is the sole CLI plaintext-retrieval exception for this local access key.
Without `--output-fd`, write one UTF-8 secret line only to an attached controlling
terminal through its separate terminal handle. With `--output-fd`, use only a
platform-qualified inherited writable pipe descriptor at least 3. Never use
stdout/stderr, a file path, regular file, environment value, log, or raw error.
The descriptor must differ from any unlock-input descriptor, and its pipe must
not alias stdout/stderr. A number at least 3 alone does not qualify the channel.
Qualify output before unlocking/resolving the key. Noninteractive use requires
the pipe; absent/unsupported delivery blocks with no fallback or secret retrieval.
Unlock follows the common protected-input rules with no cached unlock session.
Show reports whether the saved reference matches the actual loaded key; delivery
does not prove readiness, activate saved changes, start inference, or register a
client. It accepts dry-run, which never reads/resolves the key, consumes protected
input, or writes any secret bytes. Ordinary status/inspect never reveal it.

Provision/replace return `data.proxy_key = {action, reference, previous_reference}`.
Action is `provisioned`, `replaced`, or `planned`; references are nonsecret strings
or null (new reference is null for dry-run, never reserved). Action identifies
the credential operation, not proof of saved/active state; report those outcomes
separately. Preserve the actual created reference even after later failure.
Replace also returns `data.dependencies` as known shared resource references and
`data.unmanaged_clients = "unknown"`.
Show returns `data.reference`, `data.loaded_match` (boolean or null when no loaded
comparison is available), and `data.delivery = {channel, descriptor, delivered}`.
Channel is `terminal`/`pipe` or null if unqualified; descriptor is the pipe integer
or null. Delivered is true only after complete delivery, false before delivery or
for dry-run, null for partial/uncertain delivery. The result/stderr never contains
the key; partial effects retain the `secret_delivery` boundary. Missing/locked
key is `credential_unavailable`; missing required interaction is
`interactive_required`; unsupported output is `unsupported_protected_output`.
No generated/revealed key is placed in examples, result JSON, or next-action argv.

Examples: `tmax proxy key replace --dry-run`, `tmax proxy key replace`,
`tmax proxy key show`, and
`tmax proxy key show --output-fd 3 --unlock-fd 4 --non-interactive`.
The last form requires actual qualified separate inherited pipes; no file/argv
fallback is implied.

## Secret inventory data

`secret list` works with absent/invalid config and discovers secrets created before
failed save, interrupted provisioning, or a lost result. Safe inventory metadata
is atomically co-persisted with each secret. No unlock, content decryption, token
export, fingerprint, or desired credential registry is added.

It returns `data.coverage` (`complete`, `partial`, or `unavailable`),
`unavailable_scopes` (resource references), and `entries`. Only complete enumerated
empty inventory may claim an unqualified empty list. Each entry contains:

- `reference`, UTC-or-null `created_at`, `metadata_state` (`known`, `missing`, or
  `unavailable`), and `owner` (null or `{kind, identity, id}`). Owner kind is
  `provider`, `agent`, or `proxy`; identity is a safe stable local identity, not
  upstream identity. Proxy identifies the sole proxy, never an Agent/client;
  its `id` is null. Other IDs are strings or null when no current label is known.
- `references = {desired, loaded, native}`. Each is `{state, resources, reason_code}`
  with state `referenced`, `unreferenced`, or `unknown`, a reference array, and
  string-or-null reason. Partial known references do not establish full safety.
- `cleanup = {state, blockers}`. State is `eligible`, `blocked`, or `unknown`;
  blockers use the shared scoped shape. Listing never reserves eligibility.

Invalid config makes desired-reference safety unknown, not empty. Absent config
does not prove loaded/native references absent. Missing/damaged/unreadable inventory
metadata gives scoped partial/unavailable coverage, not proof of safe deletion.

Cleanup rechecks saved, loaded, and known native references, ownership, and current
concurrency. Unknown safety blocks deletion, including with removed targets or
invalid config. It cannot delete native-owned sessions through tmax storage or
claim upstream revocation or deletion of copies already handed to native or
unmanaged clients. Unknown external copies are not an invented client registry.
Retained native recovery remains reachable independently of current config.

`secret cleanup` returns `reference`, `deleted` (boolean-or-null; false for dry-run),
and `cleanup` with the same safety shape. Uncertain deletion is null, not false,
and retains the actual credential boundary state. Creation/deletion must preserve
secret/inventory discoverability across interruption. There is no automatic
cleanup or additional credential-management resource.
