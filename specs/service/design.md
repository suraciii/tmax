# Native service ownership design

The [service contract](spec.md) owns observable behavior.
This design keeps native process supervision separate from listener admission and
[login-start intent](../configuration/spec.md#save-reload-and-apply).

## Decisions

| Pressure | Rejected alternative | Selected boundary |
| --- | --- | --- |
| Recover when no client is connected | Client wrappers or a tmax watchdog process | Native user service manager owns process lifetime |
| Autostart is disabled but the user explicitly starts inference | Use supervision only when login autostart is enabled | Service definition, current execution, and future-login activation are separate |
| Web and inference share one owner | A separate inference service and Web daemon | One supervised foreground owner; listener requests remain independent |
| A user stops inference before the process crashes | Recover whichever listener the startup command names | Recover the latest serialized listener request for the current activation |
| An old daemon is already serving | Kill it or let two launch paths race for its lock | Reuse it honestly; explicit restart transfers ownership |
| Native manager is unavailable | Fall back silently to a detached child | Preserve a truthful unsupported/unavailable result |
| A permanent startup error cannot improve through retries | Unbounded restart loop | Retained blocker and explicit retry through existing lifecycle commands |

## Ownership

```text
tmax lifecycle operation
  validates authority, compatibility, and platform ownership
  serializes requested listener changes
  native manager
    owns exactly one foreground daemon process
      owns the control endpoint
      owns independent inference and Web listeners
      owns in-process background acquisition
```

The service entrypoint must remain alive for the daemon's entire lifetime.
It must not detach a child and exit successfully while the manager supervises
only the launcher.
The CLI must observe application readiness independently of manager start success.
The native manager must remain the process owner across explicit restart and
unexpected-exit recovery.
The foreground entrypoint must not gate explicit startup on `gateway.autostart`;
only a new login activation applies that gate.

## Recovery coordination

A small operational record under `~/.tmax` retains an authority identity, storage
version, activation identity, monotonic lifecycle generation, requested listeners,
and acknowledged addresses.
It is not a configuration snapshot or a list of requests to replay.
Platform activation identity must distinguish manager/session reactivation from
a daemon process restart; wall-clock age or PID alone is insufficient.
A new activation uses saved autostart intent instead of reviving old listener
requests.
Per-process identifiers such as systemd's `INVOCATION_ID` cannot represent that
activation because they change during process recovery.
Windows qualification must distinguish a new user logon from a restarted task;
the machine-wide Task Scheduler process identity is insufficient.
If a platform cannot establish the activation boundary, startup must report that
limitation rather than reuse stale listener requests.

The existing sole-owner lock remains the final admission barrier for daemon
ownership.
Lifecycle updates also require serialization while no daemon owns that lock.
A stopped-listener generation must invalidate older queued manager invocations.
The foreground owner must check the current generation before binding a listener.
An explicit stop must close any older admission before it reports completion.
A restart must not hold a lock needed by its own replacement while waiting for
that replacement to become ready.

A failed new start must withdraw its new request without erasing a concurrent
newer request or another running listener.
Stop must remain actionable when the manager is between process attempts.
Recovery outcome records retain safe reasons, not raw environment or credential
values.

## Platform realization

| Platform | Mechanism and constraints |
| --- | --- |
| Linux | systemd user service runs the foreground owner. Its activation links govern login autostart separately from explicit starts. Native restart policy retries classified transient exits; intentional all-stopped exit and permanent startup blockers do not loop. No implicit linger or system-level unit. |
| macOS | A user launchd job runs the foreground owner. Current-domain loading is separate from future-login registration. Planned final stop disarms the job rather than killing a KeepAlive process and expecting it to stay dead. Exit classification prevents permanent configuration retry loops. |
| Windows | A current-user scheduled task runs the foreground owner for its complete lifetime. On-demand start and logon trigger are separate. Explicit task stop and bounded restart-on-failure govern process recovery. A startup-folder shortcut does not qualify as supervision. |

Windows qualification must exercise failure after an action has started, not
only failure to launch it, for both on-demand and logon-triggered execution.
The presence of a `RestartOnFailure` setting alone does not prove that behavior.
If the available scheduler cannot meet it, report the backend unavailable rather
than claiming recovery or adding a tmax watchdog.

All backends must preserve authority and executable identity when repairing their
owned definition.
The registered definition must be discoverable by the actual native manager,
including a non-default platform configuration directory.
Windows cutover must retire only the proven-owned legacy Run entry after the
replacement task is prepared, leaving unrelated values unchanged.
Definition changes that require process replacement belong to explicit restart,
not status, repeated start, or unrelated configuration edits.
The implementation must expose native retry limits and session constraints rather
than pretend that all platforms provide identical logout or boot behavior.
No additional generic supervisor framework or public backend configuration is
required.

## Implementation boundaries

| Existing responsibility | Required change |
| --- | --- |
| Daemon acquisition and control | Keep compatible-owner discovery and sole-owner locking; replace new detached owner creation with native-manager startup |
| Foreground owner | Restore validated activation requests; publish actual readiness; distinguish intentional, blocked, and retryable exits |
| Gateway and Web lifecycle | Commit listener requests and stop suppression; route replacement through the same manager |
| Platform startup adapters | Separate owned service definition/current execution from login activation; retire competing owned legacy launch paths at cutover |
| Status and operation results | Report manager ownership, recovery state, session limits, and partial effects separately from listener readiness and autostart |

Implement and qualify the platform boundary before advertising supervised startup
on that platform.
Reuse existing shared result boundaries and public lifecycle commands.
Keep application logs in the existing tmax home; platform service events are
additional diagnostic evidence, not an application-state store.
