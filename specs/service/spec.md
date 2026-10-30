# Supervised local service

**Authority**: This capability owns process supervision, recovery, and lifecycle
ownership for the sole local daemon.
[Product](../product/spec.md#daemon-service-identity) owns identity and listener
independence.
[Configuration](../configuration/spec.md#save-reload-and-apply) owns saved settings
and login-start intent.
[Service commands](../cli/service/spec.md) own command syntax and result fields.

## Scope

The operating system must supervise newly started daemon processes independently
of any Agent, browser, terminal, or calling CLI process.
Explicit process startup must not require enabling login autostart.
The existing gateway and Web commands remain the lifecycle interface.
There is no separate public service-install command or supervision setting.
This capability must not start, stop, restart, or wrap native Agents.

| Independent fact | Meaning |
| --- | --- |
| Requested listeners | The inference and Web surfaces explicitly requested for the current service activation; operational state, not another desired-configuration document |
| Supervision | Whether a qualified native manager owns process recovery for this authority |
| Login autostart | Whether a future user-manager activation may request inference without another explicit start |
| Readiness | Whether the identified process actually serves the requested listener under its applicable configuration and credential checks |

A live PID, installed definition, enabled login trigger, or successful manager
start request must not establish readiness.
Readiness must verify authority, process identity, actual listener address, and
local serving prerequisites without a paid inference probe.
It does not establish upstream reachability or provider entitlement.

## Lifecycle decisions

The following pseudocode is a non-executable contract.

```text
on explicit start or Web open:
  identify the sole owner and native-manager ownership
  if identity, compatibility, or registration ownership conflicts:
    block without replacing a process or registration
  if the requested listener already runs in a compatible owner:
    return actual state without reload or process replacement
  if a compatible owner exists:
    request only the selected listener in that owner
  otherwise:
    require an available qualified native manager
    prepare this authority's service definition without enabling login startup
    record the requested listener and start through that manager
  wait for identified listener readiness before reporting success

on explicit listener stop:
  commit the selected listener's stopped intent against recovery
  close its admission and cancel its active work
  preserve the other listener's requested state
  if neither listener is requested:
    disarm recovery and stop the owned manager job
    release the daemon's ownership

on unexpected process exit:
  recover only the current activation's still-requested listeners
  validate current saved configuration before starting inference
  retain the last acknowledged listener addresses
  never replay an interrupted request

on a new user-manager activation:
  discard prior activation's listener requests and stop suppression
  validate current saved login-start intent
  request inference only if autostart is enabled
  do not restore a prior Web listener
```

An explicit stop must suppress recovery even while the process is absent or the
manager has queued a replacement.
Concurrent starts, stops, and recovery must serialize against the same authority.
A completed stop must not be undone by an older queued start or recovery attempt.
A failure to establish that guarantee must return a failed or incomplete result.
Stopping does not disable login autostart for a later user-manager activation.
Stopping the last listener must not leave an idle daemon or tmax-owned watchdog.
Repeated login events within the same activation must not clear stop suppression.
Activation identity must remain stable across process recovery and change at the
qualified native-manager or user-logon boundary for that platform.

An explicit gateway restart must use the same native manager for process
replacement.
It requests inference and restores a previously requested Web listener.
It must cancel in-flight work under the existing stop contract, not drain it.
It must use the on-disk executable and current validated saved configuration.
It is the explicit operation that may adopt a saved inference address change.
It must not reassign retained conversation destinations.
An explicit start or restart may reset this authority's exhausted native retry
limit after identity and prerequisite checks; reads and unrelated edits must not.
Manager and registration preflight must precede interruption of a working owner.
When Web requests replacement of its own serving process, acknowledgement is not
completion. The page must reconnect and verify a different process instance and
the requested listeners before presenting success. Connection loss must not
cause automatic resubmission of the restart. Without acknowledgement, the request
outcome remains unknown even if another client replaces the process.
A compatible unsupervised Web owner requires the CLI for its first transfer to
native supervision.

## Recovery and readiness

| Condition | Required outcome |
| --- | --- |
| Unexpected process exit with requested listeners | Native manager attempts delayed recovery without a connected client |
| Explicit stop or no requested listeners | No recovery until another explicit start or a later enabled login-start event |
| Invalid saved configuration, missing shared key, or changed inference address | Inference remains blocked with its cause; do not fall back to an earlier configuration, generate a key, or move the listener |
| One provider's credentials are unavailable | Only its routes are unavailable; this does not require process restart |
| Requested Web with invalid inference configuration | Restore Web independently for repair; do not report inference ready |
| Requested address is occupied | Report the conflict and retain the address; do not kill the occupant or allocate a replacement port |
| Missing, malformed, or unsupported recovery state | Do not guess which listeners to reopen; expose recovery unavailable and require explicit lifecycle action |
| Repeated transient process failures | Apply a finite, disclosed restart rate and retry limit; report exhausted recovery instead of an unbounded tight loop |
| Known non-retryable startup blocker | Stop automatic attempts for that blocked scope; retain a safe cause and explicit recovery action |
| Process alive but upstream request fails or remains silent | Do not restart the daemon or replay the request merely for that condition |

Recovery state must contain no credentials, request bodies, loaded configuration
copy, or inference replay queue.
It must be current-user restricted and scoped to the authority and activation.
Its schema and activation identity must be validated before use.
Successful lifecycle operations must durably establish their requested state
before reporting completion.
A failed start must not leave a newly requested listener eligible for a later
surprise start; retain other previously requested listeners.
If cancellation or rollback cannot be confirmed, report partial effects and
whether the manager may still start a process.
A CLI readiness deadline must be finite and must not be an inference deadline.
Implementation qualification must measure and document readiness and recovery
bounds rather than copy unmeasured timing values from another product.

Recovery is a cold start, not a replay of a configuration-management operation.
It may load a newer valid saved revision, which must be reported as such.
It must not implicitly adopt a changed inference listen address.
A Web port initially allocated by the OS becomes the acknowledged address for
recovery within that activation.
Automatic recovery must not launch a browser.
Loss of in-memory request history on process exit must remain an evidence gap;
the separate [error log](../diagnostics/spec.md#persistent-gateway-error-log) does
not reconstruct that history. Retained conversation bindings follow their independent
persistence contract.

## Native-manager and permission boundary

| Environment | Required boundary |
| --- | --- |
| Linux with reachable systemd user manager | Use a user-scoped supervised job; login activation is independently enabled or disabled |
| macOS with usable user launchd domain | Use a user-scoped supervised job; registration for future login is distinct from running the current activation |
| Windows with qualified current-user Task Scheduler recovery | Use a user-owned task whose lifetime covers the actual daemon and whose failure policy recovers both on-demand and login-triggered instances; login trigger is independent of on-demand execution |
| Unsupported manager, unavailable user session, or denied permissions | Report the specific blocker; do not silently launch an unsupervised detached process or escalate privileges |

The guarantee is bounded by the user's available native-manager session.
Closing a terminal or browser must not stop the supervised process.
Survival across logout or startup before login must not be claimed without
platform evidence.
tmax must not enable Linux linger, install a machine-wide service, request
administrator elevation, or change account-wide session policy implicitly.
Status must disclose the observed session limitation and any required external
administrator action.
Native task/service registration alone is insufficient platform qualification.
Unexpected-exit recovery and explicit-stop suppression must work for both
explicitly started and login-started instances before that backend is supported.

Service definitions must pin this authority and the intended executable.
They must not embed credentials or depend on a caller's working directory.
Loaded native job identity must match its owned executable and arguments, not
only the pathname of a currently owned definition.
Unowned native overrides, including systemd drop-ins, block lifecycle effects;
tmax must not delete them or report their policy as its own.
Native-manager records are external platform integration, not a second tmax
configuration authority.
tmax-owned runtime records and application logs remain under `~/.tmax`.
Platform-generated service events may remain in the platform's own event store.
Logs and status must distinguish a failed manager operation from a started
process that never became ready.

## Autostart and existing installations

Autostart reconciliation must only change future-login activation and any
required owned definition; it must not start or stop current listeners.
Disabling autostart must not remove the active process's supervision.
An owned service definition may remain installed while login autostart is disabled.
Unrelated saves, reloads, and reads must not repair service drift.
An explicit lifecycle operation may repair an owned definition needed for that
operation without changing login-start intent.
Definition repair must not replace an unrelated entry or restart a live process
as an unreported side effect.

Supervised lifecycle coordination uses management protocol `tmax-control/6`.
Earlier clients must not bypass its serialized listener-request boundary.
Stop an incompatible daemon through its matching executable or existing Web
controls before starting the upgraded executable.

| Existing state | Cutover behavior |
| --- | --- |
| Compatible daemon not owned by the native manager | Reuse without claiming supervision; report explicit gateway restart as the transition for gateway use |
| Compatible management-only unsupervised daemon | Preserve Web-only behavior; the caller may stop and reopen Web to transition without starting inference |
| Explicit restart of a compatible unsupervised daemon | Verify manager availability and prepare ownership before stopping; release the old owner before starting one supervised replacement |
| Older owned login registration | Reconcile it through an explicit lifecycle operation or required autostart reconciliation; leave no competing launch path |
| Incompatible or unidentifiable owner | Preserve the product's compatible-control recovery requirement; OS-manager access is not permission to bypass it |
| Replacement fails after old process stops | Report downtime, completed effects, and remaining manager state; do not launch a detached fallback |

## Acceptance boundary

| Scenario | Observable invariant |
| --- | --- |
| Start with autostart disabled, then close CLI and browser | Identified ready listener remains supervised; saved autostart stays false |
| Crash the owned process without any client online, while its native-manager session remains available | A new identified process restores only requested listeners at acknowledged addresses |
| Stop inference while Web runs, then crash the process | Web returns; inference stays stopped |
| Stop while recovery is pending | No stale recovery attempt reopens the stopped listener |
| Stop the final listener | Daemon releases ownership and remains absent during the same activation |
| Disable autostart while inference runs, then crash | Current inference can recover; a later login must not start it |
| Stop with autostart enabled, then activate a new user-manager session | Current stop is respected; the new activation follows saved enabled intent |
| Invalid saved file or missing shared key during recovery | No false readiness, secret generation, old-config fallback, or repeated non-retryable startup loop; requested Web remains independently usable |
| Saved listen address changed before a crash | Recovery does not silently move inference; explicit restart uses the saved address |
| Concurrent manual start, login event, and recovery | At most one daemon owner and one writer for retained runtime state |
| Repeated login notification after an explicit stop in the same activation | Stopped listeners remain stopped; only a genuinely new activation can apply login intent again |
| Native manager unavailable or an entry belongs to another application | Actionable failure without privilege escalation, overwrite, or unsupervised fallback |
| Crash after upstream dispatch | No inference replay; history loss and upstream outcome uncertainty remain explicit |
| Restart after executable replacement | New executable serves with the existing ownership and session-binding safeguards |
| Platform restart limit reached or readiness deadline expires | No success claim; report blocked/failed/partial state and the supported explicit retry |

## Implementation qualification

Native manager access, compilation, and definition generation do not establish
process-recovery qualification.
Each platform must exercise on-demand and login-start recovery, queued-stop
suppression, authority identity, and session limits before claiming that scope.
Evidence for one platform must not establish another platform's behavior.
