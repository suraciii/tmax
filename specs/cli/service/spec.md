# Network and daemon-service commands

This child owns outbound network, gateway/Web configuration/lifecycle, and their
results. [Product](../../product/spec.md) owns network coverage, daemon identity,
and listener independence. [Configuration](../../configuration/spec.md) owns
saved fields and activation. Use [common CLI rules](../spec.md),
[configuration changes](../configuration/spec.md), and [shared results](../results/spec.md).
[Probes/history](diagnostics/spec.md) have a separate command contract.
[Supervised service](../../service/spec.md) owns native-manager startup, crash
recovery, stop suppression, and existing-installation cutover.

## Outbound network configuration

The [network schema](../../configuration/spec.md#outbound-network-settings) owns
URL constraints/defaults; [outbound policy](../../product/spec.md#outbound-network-policy)
owns traffic coverage and failure behavior.

| Command | Result or requested change |
| --- | --- |
| `network proxy URL` | Save a network table containing exactly that proxy URL, replacing the previous policy. |
| `network direct` | Remove the optional network table, the canonical saved direct representation. |
| `network show [--state STATE]` | Inspect saved/loaded policy, default provenance, effective proxy address/port, and which future work consumes it. No connectivity or secret check. |

Mutations support common dry-run/revision guards. Help lists `http`, `https`, and
`socks5`, their default proxy ports, and SOCKS5
proxy-side destination DNS. Userinfo/proxy-authentication inputs are unsupported.
There is no provider override, bypass list, environment discovery, direct fallback,
or TLS-verification bypass.

Execution validates/saves policy without contacting the proxy, starting services,
or changing native networking. Report saved versus loaded state. Future independent
CLI work uses validated saved policy; future daemon work uses loaded policy;
in-flight work retains its captured policy. Use `config reload` for pending or
saved-but-not-active changes; compatible gateway/Web reuse never implicitly reloads.

Examples: `tmax network proxy http://127.0.0.1:8080`,
`tmax network proxy socks5://127.0.0.1:1080`, `tmax network direct --dry-run`,
`tmax network direct`, `tmax network show`, and `tmax network show --state loaded`.
These addresses do not claim a running proxy or successful connectivity.

## Gateway configuration and field coverage

`gateway configure [--listen ADDRESS] [--clear autostart]` creates/edits the optional
gateway table. At least one operand is required; creation requires listen and
explicitly provisions the sole shared key in a current-user-restricted plaintext
storage-version-2 file, saving its `key_ref` with the settings. Listen is numeric
loopback `host:port` (bracket IPv6), port 1–65535. Preserve unrelated settings and
never start inference or write native files. Existing-table edits retain the key
reference; no implicit repair or key generation occurs on ordinary
configure/start/reload/apply. Missing or unreadable key material blocks
readiness; explicit [key replacement](../providers/authentication/spec.md#shared-gateway-key)
repairs a missing or unreadable retained key.

Report provisioning, save, and activation separately. Dry-run describes key
provisioning without generating/reserving a reference or reading key bytes. Actual
provisioning uses `data.gateway_key` from the shared-key contract; failed save retains
any orphan reference for explicit cleanup. Never return key bytes.
Clear autostart restores disabled intent, not unconditional registration retry.

`gateway remove` requires stopped inference and resolved native `--gateway` entry
references. It removes gateway configuration/access, not the stored secret, native
files, or unknown external copies. It never stops inference implicitly. Saved
listen changes require explicit restart when the running listener cannot activate
them. Do not silently move it or report it current. Both commands support common
dry-run/guards and separate save/activation outcomes. Explicit autostart
enable/disable sets the boolean and always reconciles registration.

Provider/model/model-alias operations own their existing definitions and shared
publication; Agent operations own native bindings/selections; gateway configuration
and key operations own the sole listener/key. No separate publishing table,
client resource, per-Agent key, or gateway-model command family exists. Standalone
compatible local clients need no registered or complete Agent target.

## Autostart

The [save/reload contract](../../configuration/spec.md#save-reload-and-apply) owns
last-acknowledged desired startup intent. Save/reload reconciles registration only
when that accepted intent changes, not on mere revision change, drift, or prior
registration failure. Acknowledge validated persisted intent independently of
registration success, never unsaved values. Required reconciliation works without
a daemon and reports separately from save/activation; failure is incomplete.

Unchanged-intent unrelated edits/reloads neither retry/remove registration nor
fail only for drift. Report `not_requested` plus drift/failed state and the repair
value, without starting inference. Login startup validates current config and
enabled intent; stale registration cannot override disabled intent. Missing or
unavailable credentials block readiness, not an insecure fallback or a false ready state.
Current-activation recovery is independent of login-start intent.

Upstream credential changes on reload/start/restart use
[replacement safety](../providers/authentication/spec.md#authorization-replacement),
without a separate account-confirmation operand or gate. Shared local key changes
follow [their independent lifecycle](../providers/authentication/spec.md#shared-gateway-key).

## Gateway lifecycle and local Web entry

| Command | Result or requested boundary |
| --- | --- |
| `gateway status` | Actual daemon/versions, inference/Web listeners, startup intent/drift/failure, shared-key readiness, derived publication, and safe bound-session conditions; native dependencies are not serving prerequisites. |
| `web status` | Actual daemon/versions and management listener or explicit absence, independent of inference. |
| `gateway start` | Reuse compatible running inference without reload, or validate saved config/shared key and start inference in the sole owner. New processes use native supervision independently of autostart; success requires actual readiness. No managed-target prerequisite, key provisioning, or native application. |
| `gateway stop` | Stop inference, not Web, and suppress its pending recovery even while the process is absent. Cancel in-flight requests as transports allow and report the actual interrupted set; never wait for in-flight work. |
| `gateway restart` | Recycle through the native manager from the on-disk executable; explicitly transition a compatible unsupervised owner after preflight. Cancel in-flight work, release ownership, start inference with current valid settings (including saved address changes), and restore a requested Web listener. Report success only after requested listeners are ready. |
| `gateway autostart enable` | Save true and always reconcile per-user login activation; disclose credential and session limitations without starting inference. |
| `gateway autostart disable` | Save false and always reconcile removal of login activation, not current inference or its supervision. |
| `web open [--listen ADDRESS] [--no-browser]` | Start/reuse management in the sole compatible daemon, using native supervision for a new process; report its ready URL and open the browser unless suppressed. Do not implicitly start inference or reload. |
| `web stop` | Stop management and suppress its pending recovery, not inference/native agents. If neither listener remains requested, disarm process recovery and release ownership. |

Mutations support dry-run; autostart saves accept the common file guard. Reads are
effect-free. Stop and restart serialize admission with the actual active set;
successful stop cannot interrupt newly admitted work. Cancellation closes
admission and reports the actual interrupted set without replay. Dry-run
freezes/reserves nothing. Stop/restart changes neither startup intent nor
retained conversation bindings.

[Daemon identity](../../product/spec.md#daemon-service-identity) owns sole-config
ownership, compatibility, and concurrent-start convergence. Discover/identify the
owner before effects. Compatible repeated start returns `daemon_reused` with
actual process/versions, loaded revision, listener, and Web URL. Do not revalidate
pending settings, restart, or assume caller version equals daemon version.
Executable-version difference alone does not authorize replacement.

Concurrent starts converge; a losing creator reuses the compatible identified
owner. Incompatible owner, unidentified occupied endpoint, and unavailable identity
block effects. Never kill/take over an unknown owner/socket, start a second version,
or choose a different port. Status may successfully report these conditions;
lifecycle fails closed. Inference stop leaves Web usable for config repair. After
explicit stops leave both surfaces stopped, shutdown/release follows Product;
supervision must not undo that stop. An explicit gateway restart replaces the
process while preserving sole ownership. Incompatible recovery needs a compatible CLI
or that daemon's available Web controls before starting the new executable.
Next actions must not claim the incompatible current CLI can perform that stop.

Autostart touches only tmax's per-user login activation and any required owned
definition, never unrelated entries or current listener state. A retained service
definition with login activation disabled is not autostart drift.
A Web address is numeric loopback plus port. Without
one, reuse management or allocate one initial OS-selected loopback port. Initial
allocation is not fallback from conflict/occupation. An explicit mismatch with
running Web is `endpoint_conflict`; an occupied requested endpoint is
`endpoint_occupied`. Neither permits relocation/replacement. Report actual URL
and daemon identity; Web listen is ephemeral, not a persisted profile/field.

Web has no login/pairing/password/token. Host/Origin rules follow
[Product](../../product/spec.md#local-web-and-cli-experience). Opening Web needs
neither inference nor valid desired config. Closing browser/terminal does not
stop background inference; CLI completion never depends on Web availability.

Examples: `tmax gateway configure --listen 127.0.0.1:17840`,
`tmax gateway configure --clear autostart`, `tmax gateway start`,
`tmax gateway key show`, `tmax gateway key replace`,
`tmax gateway stop --dry-run`, `tmax gateway stop`, `tmax gateway restart`,
`tmax gateway autostart enable`, `tmax gateway autostart disable`, `tmax gateway remove`,
`tmax web open --no-browser`, `tmax web open --listen 127.0.0.1:17841`, and
`tmax web stop`. Independent examples are not an automatic sequence.

### Supervision outcomes

Existing lifecycle syntax and configuration fields are unchanged.
Service-definition preparation and manager control belong to `gateway_lifecycle`
or `web_entry`, not `autostart`, unless login activation was requested.
Dry-run must disclose required definition changes, supervision transfer,
interrupted work, and locally observable manager blockers without performing them.
It must not reserve a manager job or change a recovery generation.

| Condition | Result |
| --- | --- |
| Existing compatible unsupervised owner reused | Existing success/reuse result with supervision explicitly inactive and a supported transition action; no hidden recycle |
| Unsupported manager or unavailable required user session before effects | Exit 3 with `service_manager_unavailable` and the specific platform reason; no detached fallback |
| Definition belongs to another authority/application | Exit 3 with `service_registration_conflict`; leave it unchanged |
| Manager permission or I/O operation fails | Exit 4 with `service_control_failed`, completed effects, and safe external permission guidance; no automatic elevation |
| Requested listener does not become ready before its deadline | Exit 4 with `service_readiness_failed`; report actual listener/manager state and request withdrawal or uncertain partial effects |
| Recovery exhausts native attempts or meets a permanent blocker | Status succeeds as a read, exposing the cause and explicit start/restart action rather than readiness |
| Web requests replacement of its own supervised process | `service_restart_requested` with `status: pending` acknowledges the request, not readiness; delivery can be interrupted by process exit |

Existing identity, configuration, credential, and endpoint codes retain their
own meanings when more specific than the manager result.
A successful stop requires confirmed stop suppression, not merely a delivered
signal. No failure may silently report a queued start as cancelled.
Web restart acknowledgement contains `old_process_instance`,
`restart_completion: pending_reconnect`, and a finite
`reconnect_deadline_seconds`. It is not a persisted operation or replay token.
The UI observes the replacement without submitting the mutation again.
CLI restart retains synchronous readiness completion.

## Network-policy data

`NetworkView` contains:

| Key | Type and meaning |
| --- | --- |
| `snapshot_state` | `available`, `absent`, or `unavailable` |
| `configuration_revision` | Opaque string-or-null; known absent config is `absent` |
| `configured` | Null for absent/unreadable optional table; otherwise the validated `proxy_url` field |
| `effective` | Null for unavailable/not-loaded policy; otherwise `{mode, proxy_url, proxy_scheme, proxy_host, proxy_port}` |
| `reason_code` | Controlled string-or-null explaining default/absent/unavailable state |

Direct has null proxy fields. Proxy mode has a validated URL, scheme `http`/`https`/`socks5`,
host, and positive effective port including scheme default. An absent config file
has the direct default; absent loaded state has no daemon policy. Invalid or
unreadable config does not acquire a default effective policy.

`network show` returns `data.state`, `data.snapshot_state`, `data.policy: NetworkView`,
and `data.future_work_scope` (`independent_cli` for file or `daemon` for loaded).
`gateway status` exposes saved/loaded network detail as `data.network = {file, loaded}`
when inspectable. Routine mutation/reload success summarizes policy and actual
saved/loaded outcome rather than repeating both complete views. Full dry-run,
pending-activation impact, or failure returns relevant actual views and
`data.network_candidate` separately from actual state. Dry-run does not change
either. A URL is not proof of reachability.

## Daemon and listener data

Detailed status/lifecycle uses `DaemonView`: `discovery_state` (`absent`,
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

`gateway status`/`web status` return `data.daemon: DaemonView`; root `status` may
summarize the same actual facts with readiness counts and relevant detail actions.
Lifecycle returns actual daemon identity/listeners and
`data.lifecycle = {surface, action, requested_listen, actual_listen}`. Surface is
`inference` or `web`; action is `started`, `reused`, `stopped`, `restarted`, `none`,
or `planned`; listen values are string-or-null. Include `planned_action` for dry-run
and `daemon_before` only when needed to explain partial effects or a change.
A routine success need not repeat before-state or unrelated metadata.

Dry-run uses planned action and leaves actual views unchanged without reserving
owner/address. Reuse exits 0 with `daemon_reused` and discovered actual loaded
revision/listeners, not saved values; activation/restart are not requested.
`daemon_incompatible`, `daemon_identity_unavailable`, `endpoint_occupied`, and
`endpoint_conflict` block changes with exit 3 before effects. Return safe actual
identity/versions and supported recovery, not force/takeover/port fallback.
Partial start after a completed boundary retains the incomplete/failure result.

Detailed `gateway status` additionally returns `data.shared_key = {file_reference,
loaded_reference, loaded_state}`. References are safe strings or null; loaded
state is `absent`, `ready`, or `unavailable`. Status reports retained loaded state
without reading, resolving, or revealing a key. It returns actual daemon evidence,
not fabricated success from a reference.
`data.publication = {snapshot_state, model_count}` summarizes the loaded derived
list; snapshot state uses common available/absent/unavailable values, and count
is a nonnegative integer or null without available data. It does not repeat
every model: use [model list/inspect](../models/spec.md#model-inspection).
Known native dependencies use resource references in `data.dependencies` when
relevant; `data.unmanaged_clients = "unknown"` never claims complete client
enumeration. Shared key availability/serving readiness are independent of
native selection/default readiness and of whether any Agent is registered.

### Supervision data

Detailed gateway/Web status and affected lifecycle results include
`data.supervision` independently of `data.daemon` and `data.autostart`.
Root status and Web must summarize the same facts without equating running,
supervised, ready, or enabled-for-login.

| Field | Meaning |
| --- | --- |
| `manager` | `systemd_user`, `launchd_user`, `windows_task`, `none`, or null when unknown |
| `registration` | `owned`, `absent`, `conflict`, or `unavailable`; installation alone is not active supervision |
| `state` | `supervised`, `unsupervised`, `starting`, `recovering`, `stopped`, `blocked`, `failed`, or `unavailable`; `supervised` requires verified ownership of the actual daemon process |
| `process_owned` | Boolean-or-null; associates the native manager's observed process with the daemon's qualified peer identity |
| `reason_code` | Controlled safe cause or null; use the specific platform/session/blocker reason |
| `requested` | `{inference, web}` with boolean-or-null values for current-activation requests, not observed listener readiness |
| `recovery` | `{attempts, limit, retry_delay_seconds, last_exit_code}`; values are nonnegative numbers or null where unknown, except signed exit code; attempts and limit are integers |
| `session` | `{scope, survives_logout}`; scope is `user_manager`, `user_login`, or null, and survival is boolean-or-null based on evidence, not inferred from installation |

`data.autostart` continues to describe saved login-start intent and its actual
registration separately. Unknown manager/process evidence must not become
`stopped`, `unsupervised`, or disabled login startup.
Recovery timing describes the native policy, not a guarantee of upstream service
availability. Detailed failure results include the existing next-action form.

