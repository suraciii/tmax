# Network and resident-service commands

This child owns outbound network, proxy/Web configuration/lifecycle, and their
results. [Product](../../product/spec.md) owns network coverage, resident identity,
and listener independence. [Configuration](../../configuration/spec.md) owns
saved fields and activation. Use [common CLI rules](../spec.md),
[configuration changes](../configuration/spec.md), and [shared results](../results/spec.md).
[Probes/history](diagnostics/spec.md) have a separate command contract.

## Outbound network configuration

The [network schema](../../configuration/spec.md#outbound-network-settings) owns
URL constraints/defaults; [outbound policy](../../product/spec.md#outbound-network-policy)
owns traffic coverage and failure behavior.

| Command | Result or requested change |
| --- | --- |
| `network proxy URL` | Save proxy mode and exactly that URL, replacing the previous policy. |
| `network direct` | Remove the optional network table, the canonical saved direct representation. |
| `network show [--state STATE]` | Inspect saved/loaded policy, default provenance, effective proxy address/port, and which future work consumes it. No connectivity or secret check. |

Mutations support common dry-run/revision guards. A valid manually authored
`mode = "direct"` remains accepted; it is not a separate CLI operation or state.
Help lists `http`, `https`, and `socks5`, their default proxy ports, and SOCKS5
proxy-side destination DNS. Userinfo/proxy-authentication inputs are unsupported.
There is no provider override, bypass list, environment discovery, direct fallback,
or TLS-verification bypass.

Execution validates/saves policy without contacting the proxy, starting services,
or changing native networking. Report saved versus loaded state. Future independent
CLI work uses validated saved policy; future resident work uses loaded policy;
in-flight work retains its captured policy. Use `config reload` for pending or
saved-but-not-active changes; compatible proxy/Web reuse never implicitly reloads.

Examples: `tmax network proxy http://127.0.0.1:8080`,
`tmax network proxy socks5://127.0.0.1:1080`, `tmax network direct --dry-run`,
`tmax network direct`, `tmax network show`, and `tmax network show --state loaded`.
These addresses do not claim a running proxy or successful connectivity.

## Proxy configuration and field coverage

`proxy configure [--listen ADDRESS] [--clear autostart]` creates/edits the optional
proxy table. At least one operand is required; creation requires listen and
explicitly provisions the sole shared key in a current-user-restricted plaintext
storage-version-2 file, saving its `key_ref` with the settings. Listen is numeric
loopback `host:port` (bracket IPv6), port 1–65535. Preserve unrelated settings and
never start inference or write native files. Existing-table edits retain the key
reference; no implicit repair or key generation occurs on ordinary
configure/start/reload/apply. Missing or unreadable key material blocks
readiness; explicit [key replacement](../providers/authentication/spec.md#shared-proxy-key)
repairs a missing or unreadable retained key.

Report provisioning, save, and activation separately. Dry-run describes key
provisioning without generating/reserving a reference or reading key bytes. Actual
provisioning uses `data.proxy_key` from the shared-key contract; failed save retains
any orphan reference for explicit cleanup. Never return key bytes.
Clear autostart restores disabled intent, not unconditional registration retry.

`proxy remove` requires stopped inference and resolved native `--proxy` entry
references. It removes proxy configuration/access, not the stored secret, native
files, or unknown external copies. It never stops inference implicitly. Saved
listen changes require explicit restart when the running listener cannot activate
them. Do not silently move it or report it current. Both commands support common
dry-run/guards and separate save/activation outcomes. Explicit autostart
enable/disable sets the boolean and always reconciles registration.

Provider/model/model-alias operations own their existing definitions and shared
publication; Agent operations own native bindings/selections; proxy configuration
and key operations own the sole listener/key. No separate publishing table,
client resource, per-Agent key, or proxy-model command family exists. Standalone
compatible local clients need no registered or complete Agent target.

## Autostart

The [save/reload contract](../../configuration/spec.md#save-reload-and-apply) owns
last-acknowledged desired startup intent. Save/reload reconciles registration only
when that accepted intent changes, not on mere revision change, drift, or prior
registration failure. Acknowledge validated persisted intent independently of
registration success, never unsaved values. Required reconciliation works without
a resident and reports separately from save/activation; failure is incomplete.

Unchanged-intent unrelated edits/reloads neither retry/remove registration nor
fail only for drift. Report `not_requested` plus drift/failed state and the repair
value, without starting inference. Startup validates current config and enabled
intent; stale OS registration cannot override disabled intent. Missing or
unavailable credentials block readiness, not an insecure fallback or a false ready state.

Upstream credential changes on reload/start/restart use
[replacement safety](../providers/authentication/spec.md#authorization-replacement),
without a separate account-confirmation operand or gate. Shared local key changes
follow [their independent lifecycle](../providers/authentication/spec.md#shared-proxy-key).

## Proxy lifecycle and local Web entry

| Command | Result or requested boundary |
| --- | --- |
| `proxy status` | Actual resident/versions, inference/Web listeners, startup intent/drift/failure, shared-key readiness, derived publication, and safe bound-session conditions; native dependencies are not serving prerequisites. |
| `web status` | Actual resident/versions and management listener or explicit absence, independent of inference. |
| `proxy start` | Reuse compatible running inference without reload, or validate saved config/shared key and start inference in the sole acquired resident. No managed-target prerequisite, key provisioning, or native application. |
| `proxy stop [--cancel-active]` | Stop inference, not Web. Refuse active work unless interruption is explicitly requested; report actual affected requests and only independently established targets. |
| `proxy restart [--cancel-active]` | Explicit stop/start using current valid settings, including saved address change, without stopping Web/replacing the resident. Same active-request rule. |
| `proxy autostart enable` | Save true and always reconcile per-user login-start registration; disclose missing or unavailable credential state. |
| `proxy autostart disable` | Save false and always reconcile registration removal, not stop inference. |
| `web open [--listen ADDRESS] [--no-browser]` | Start/reuse management in the sole compatible resident, report actual URL, and open browser unless suppressed. Do not implicitly start inference or reload. |
| `web stop` | Stop management only in the identified compatible resident, not inference/native agents. |

Mutations support dry-run; autostart saves accept the common file guard. Reads are
effect-free. `--cancel-active` requests interruption scope, not human approval.
Stop/restart serialize admission with the actual active set; normal successful stop
cannot interrupt newly admitted work. Cancellation closes admission and reports
the actual interrupted set without replay. Dry-run freezes/reserves nothing.
Stop/restart changes neither startup intent nor retained conversation bindings.

[Resident identity](../../product/spec.md#resident-service-identity) owns sole-config
ownership, compatibility, and concurrent-start convergence. Discover/identify the
owner before effects. Compatible repeated start returns `resident_reused` with
actual process/versions, loaded revision, listener, and Web URL. Do not revalidate
pending settings, restart, or assume caller version equals resident version.
Executable-version difference alone does not authorize replacement.

Concurrent starts converge; a losing creator reuses the compatible identified
owner. Incompatible owner, unidentified occupied endpoint, and unavailable identity
block effects. Never kill/take over an unknown owner/socket, start a second version,
or choose a different port. Status may successfully report these conditions;
lifecycle fails closed. Inference stop leaves Web usable for config repair. After
explicit stops leave both surfaces stopped, shutdown/release follows Product;
listener restart retains ownership. Incompatible recovery needs a compatible CLI
or that resident's available Web controls before starting the new executable.
Next actions must not claim the incompatible current CLI can perform that stop.

Autostart touches only tmax's per-user registration, never unrelated entries or
immediate inference startup. A Web address is numeric loopback plus port. Without
one, reuse management or allocate one initial OS-selected loopback port. Initial
allocation is not fallback from conflict/occupation. An explicit mismatch with
running Web is `endpoint_conflict`; an occupied requested endpoint is
`endpoint_occupied`. Neither permits relocation/replacement. Report actual URL
and resident identity; Web listen is ephemeral, not a persisted profile/field.

Web has no login/pairing/password/token. Host/Origin rules follow
[Product](../../product/spec.md#local-web-and-cli-experience). Opening Web needs
neither inference nor valid desired config. Closing browser/terminal does not
stop background inference; CLI completion never depends on Web availability.

Examples: `tmax proxy configure --listen 127.0.0.1:17840`,
`tmax proxy configure --clear autostart`, `tmax proxy start`,
`tmax proxy key show`, `tmax proxy key replace`,
`tmax proxy stop --dry-run`, `tmax proxy stop --cancel-active`, `tmax proxy restart`,
`tmax proxy autostart enable`, `tmax proxy autostart disable`, `tmax proxy remove`,
`tmax web open --no-browser`, `tmax web open --listen 127.0.0.1:17841`, and
`tmax web stop`. Independent examples are not an automatic sequence.

## Network-policy data

`NetworkView` contains:

| Key | Type and meaning |
| --- | --- |
| `snapshot_state` | `available`, `absent`, or `unavailable` |
| `configuration_revision` | Opaque string-or-null; known absent config is `absent` |
| `configured` | Null for absent/unreadable optional table; otherwise validated optional `mode` and `proxy_url` fields |
| `effective` | Null for unavailable/not-loaded policy; otherwise `{mode, proxy_url, proxy_scheme, proxy_host, proxy_port}` |
| `reason_code` | Controlled string-or-null explaining default/absent/unavailable state |

Direct has null proxy fields. Proxy has validated URL, scheme `http`/`https`/`socks5`,
host, and positive effective port including scheme default. An absent config file
has the direct default; absent loaded state has no resident policy. Invalid or
unreadable config does not acquire a default effective policy.

`network show` returns `data.state`, `data.snapshot_state`, `data.policy: NetworkView`,
and `data.future_work_scope` (`independent_cli` for file or `resident` for loaded).
`proxy status` exposes saved/loaded network detail as `data.network = {file, loaded}`
when inspectable. Routine mutation/reload success summarizes policy and actual
saved/loaded outcome rather than repeating both complete views. Full dry-run,
pending-activation impact, or failure returns relevant actual views and
`data.network_candidate` separately from actual state. Dry-run does not change
either. A URL is not proof of reachability.

## Resident and listener data

Detailed status/lifecycle uses `ResidentView`: `discovery_state` (`absent`,
`compatible`, `incompatible`, `unidentified`, or `unavailable`), string-or-null
`reason_code`, `identity`, `inference`, and `web`. When version comparison matters,
include `caller_version` and `version_relation` (`same`, `different`, or `unknown`).
Executable equality and management compatibility are separate. Unverified actual
facts remain null, never copied from caller/requested settings.

Identity is null or `{config_path, process_instance, pid, executable_version,
management_protocol_version, loaded_revision}`. PID is positive integer; loaded
revision is string-or-null; other fields are strings. Process instance distinguishes
PID reuse, not a credential or alternate config. Listener views are null or
`{state, listen, url}`; state is `running`, `stopped`, or `unavailable`; address/URL
are string-or-null. Web URL is actual management entry, not inferred from inference.

`proxy status`/`web status` return `data.resident: ResidentView`; root `status` may
summarize the same actual facts with readiness counts and relevant detail actions.
Lifecycle returns actual resident identity/listeners and
`data.lifecycle = {surface, action, requested_listen, actual_listen}`. Surface is
`inference` or `web`; action is `started`, `reused`, `stopped`, `restarted`, `none`,
or `planned`; listen values are string-or-null. Include `planned_action` for dry-run
and `resident_before` only when needed to explain partial effects or a change.
A routine success need not repeat before-state or unrelated metadata.

Dry-run uses planned action and leaves actual views unchanged without reserving
owner/address. Reuse exits 0 with `resident_reused` and discovered actual loaded
revision/listeners, not saved values; activation/restart are not requested.
`resident_incompatible`, `resident_identity_unavailable`, `endpoint_occupied`, and
`endpoint_conflict` block changes with exit 3 before effects. Return safe actual
identity/versions and supported recovery, not force/takeover/port fallback.
Partial start after a completed boundary retains the incomplete/failure result.

Detailed `proxy status` additionally returns `data.shared_key = {file_reference,
loaded_reference, loaded_state}`. References are safe strings or null; loaded
state is `absent`, `ready`, or `unavailable`. Status reports retained loaded state
without reading, resolving, or revealing a key. It returns actual resident evidence,
not fabricated success from a reference.
`data.publication = {snapshot_state, model_count}` summarizes the loaded derived
list; snapshot state uses common available/absent/unavailable values, and count
is a nonnegative integer or null without available data. It does not repeat
every model: use [model list/inspect](../models/spec.md#model-inspection).
Known native dependencies use resource references in `data.dependencies` when
relevant; `data.unmanaged_clients = "unknown"` never claims complete client
enumeration. Shared key availability/serving readiness are independent of
native selection/default readiness and of whether any Agent is registered.
