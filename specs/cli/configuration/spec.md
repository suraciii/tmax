# Configuration commands

This child owns configuration/status grammar and results. The
[configuration contract](../../configuration/spec.md#save-reload-and-apply)
owns persistence, activation, and native-application boundaries. All commands use
[common CLI rules](../spec.md) and [shared results](../results/spec.md).

## Explicit configuration changes

| Command | Result or requested boundary |
| --- | --- |
| `status` | Compact config path/revisions, daemon/listener identity and versions, provider/target readiness counts, recovery conditions, startup drift/failure, and aggregated repeated failures. Return blockers and supported detail actions where needed. |
| `status --brief` | Answer whether the installation is usable, its first blocker, and the next action; no other detail. |
| `config show [--state STATE]` | Redacted structured config and its revision. A missing loaded snapshot is explicit. |
| `config validate` | Validate current saved config; separate syntax/type/reference errors from external availability. Missing config stays absent. |
| `config reload [--if-revision REVISION] [--dry-run]` | Validate the entire saved file and activate it if the service runs, without rewriting it, applying native settings, signing in, or starting listeners. |
| `config migrate [--dry-run] [--if-revision REVISION]` | Convert an existing version-1 file to the current format under [Format migration](#format-migration); `--dry-run` reports the same plan and refusals without writing, and `--if-revision REVISION` guards the saved file. |

Desired-state operations accept `--dry-run` and optional `--if-revision REVISION`.
This includes credential-reference changes, catalog apply, shared gateway-key
replacement, and autostart intent. No preceding preview/authored document is required.

Edits validate the complete candidate, preserve unrelated comments/fields/order,
and activate the whole valid candidate when the service runs. Pending manual
edits are part of that activation, not a per-field overlay. There is no file
watcher. Missing credentials/qualification remain availability conditions.
No-service edits report saved but not loaded. Unavailable service evidence does
not block local desired-state inspection or preview: disclose unavailable loaded
state and unknown activation/session impact instead of pretending the service or
affected sessions are absent. A failed save cannot activate unsaved state. Secret
creation, sign-in, save, catalog apply, activation, autostart, and native work
remain separate boundaries under the [shared outcome contract](../results/spec.md#outcomes-and-exit-categories).

Removal rejects unresolved references and reports them without cascade, guessed
replacement, native-file deletion, secret cleanup, sign-out, or subscription
cancellation. A provider's contained model/subscription/input/rule settings may
be removed only after external references resolve. Model removal checks input
coverage; input removal checks rule references. Disclose retained secrets,
affected native targets, and direct consumers that may continue outside tmax.
Renames reject collisions, migrate references in one saved change, preserve
stable identity, and leave no aliases. Remove/add is not an inferred rename.

Upstream credential replacement and existing-session safety follow
[authentication replacement](../providers/authentication/spec.md#authorization-replacement).

| Activation change | Existing-session/request result |
| --- | --- |
| Candidate removal, alias disablement, or alias-definition continuity change | Report affected bound sessions and in-flight work; block affected old sessions without reassignment or revival after re-addition. |
| Candidate addition or priority edit | Affect new sessions; admitted requests retain their captured snapshots. |
| Native Agent model subset | A configuration choice, not a gateway-request restriction. |

Examples: `tmax status`, `tmax status --brief`, `tmax config show --state loaded`,
`tmax config validate`, `tmax config reload --dry-run`, `tmax config reload`,
and `tmax config migrate --dry-run`.

## Status and brief

`status` is an effect-free read of saved configuration and retained daemon facts.
It must not resolve secrets, refresh providers, sign in, apply native settings, or
start a service. It exits 0 when the read completes, including an unusable or
unknown state.

`status --brief` answers three questions in one read: whether the installation is
usable, which blocker comes first, and what to do next.

| Field | Contract |
| --- | --- |
| `brief` | `true`. |
| `usable` | `false` for every reported blocker; `true` only when no blocker is known; `null` only when a relayed entry depends on an unidentified or unavailable daemon and no other blocker is known. |
| `blocker` | Null when usable; otherwise `{code, message, field, next_action}`. `field` is the affected configuration field when the blocker comes from invalid configuration, otherwise null. |
| `next_action` | The blocker's next action, or null when usable. |

The first applicable blocker decides the answer; later blockers are not reported.
The blocker ladder is ordered:

| Priority | `code` | Condition | Next action |
| --- | --- | --- | --- |
| 1 | `configuration_format` | The saved file uses an unsupported format version (`field: version`). | `tmax config migrate` |
| 2 | `configuration_invalid` | The saved file cannot be read or validated. | `tmax config validate` |
| 3 | `nothing_configured` | The configuration file is absent or declares no providers. | `tmax provider add --help` |
| 4 | `daemon_identity_unavailable` | A relayed entry needs daemon evidence that is unidentified, incompatible, or unavailable. | `tmax gateway status` |
| 5 | `gateway_not_ready` | A gateway is configured for relayed entries but the listener is not ready. | `tmax gateway start` |
| 6 | `key_rotated_not_reloaded` | The identified daemon loaded a shared-key reference that differs from saved configuration. | `tmax gateway restart` |
| 7 | `delivery_unknown` | Retained delivery state for a relayed entry cannot be read. | `tmax agent inspect TARGET` |
| 8 | `delivery_stale` | An applied relayed target is stale. | `tmax agent apply TARGET` |
| 9 | `delivery_blocked` | An applied relayed target is blocked and no target is stale. | `tmax agent apply TARGET` |
| 10 | `stale_shared_key` | Retained gateway history holds shared-key rejections and no earlier blocker applies. | `tmax gateway key show` |

A configuration that declares a relayed entry without a configured gateway cannot be
valid: the derived provider `tmax` exists only while the gateway is configured, so
validation rejects it before the ladder decides (priority 2). There is no separate
"gateway is missing" rung.

`delivery_unknown` names the unreadable retained-delivery error code and the
first declared relayed target. `delivery_stale` and `delivery_blocked` name the
first affected target and its named drift or block reasons. `stale_shared_key`
names the aggregated condition once with its request count. A direct-only
installation remains usable while the daemon is stopped.
`daemon_identity_unavailable` applies only when a daemon reports a
`discovery_state` of `unidentified`, `incompatible`, or `unavailable`; an absent
daemon reports `discovery_state: absent`, skips the identity rung, and reports
`gateway_not_ready` instead. The daemon-identity and gateway-readiness blockers
apply only when a declared relayed entry needs the gateway. `usable` is null only
for an unidentified or unavailable daemon that a relayed entry depends on; an
incompatible daemon is a known blocker with `usable: false`.

Full `status` keeps its existing fields and adds `data.failures`, the
[failure aggregation](../../diagnostics/spec.md#failure-aggregation) projection. When
no daemon evidence exists, `data.failures` is null. `status --brief` must not
collapse the independent
[availability, delivery, and use](../../product/spec.md#availability-delivery-and-use)
facts: its answer names the first blocker, while full `status`, `gateway status`,
`agent inspect`, and `provider inspect tmax` keep the facts separate. A written
file must never become proof of use.

## Invalid-file recovery

`config reset [--if-revision REVISION] [--dry-run]` replaces an existing invalid
or corrupt config, not a valid file or an absent file. Ordinary setup initializes
an absent file; ordinary edits/removals change a valid one. No authored TOML is
required. A version-1 file must be refused and must name
[Format migration](#format-migration) as the next action; reset is not a
migration.

Before replacement, save the exact rejected bytes in a current-user-restricted
plaintext storage-version-2 file under `~/.tmax/recovery`; malformed content may
contain secrets. Desired configuration remains `version = 2`. Return an opaque
recovery handle, never the contents. Unsafe preservation, unknown current file
identity, or concurrent change blocks overwrite. If preservation succeeds but save
fails, return the retained handle and completed recovery/failed save boundaries.

With a service running, activate the empty desired configuration atomically,
removing configured gateway access and shared publication. Retain old bindings
blocked, not reassigned; admitted requests follow normal continuity. Leave a
running listener bound but unavailable rather than stopping/moving it.
`gateway stop` is separate. Native files/upstream consumers, stored secrets,
credential ownership, and recovery records remain untouched. Do not sign out,
apply/restore native settings, start services, or delete retained credentials.
Omitted autostart means enabled intent and uses normal changed-intent
reconciliation. Report recovery/save/activation/autostart separately.
Later setup uses ordinary commands, including explicit gateway provisioning.

Dry-run reports rejected-file identity, restricted preservation/reset, and full
gateway-key/publication/session impact without creating an artifact. There is no
snapshot catalog, invalid-byte replay, restore service, partial salvage, or automatic reset.

| Command | Retained invalid-file artifact operation |
| --- | --- |
| `config recovery list` | Safe handles, capture times, and file-access state; no rejected content. |
| `config recovery inspect RECORD` | Config-file identity, original revision, byte count, capture time, rejection category, ownership/file-access state, and discard support. Never returns rejected content. |
| `config recovery discard RECORD [--dry-run]` | Delete only that exact owned artifact after current ownership/concurrency checks. Do not change settings, credentials, native recovery, or bindings. |

These operations work with absent/invalid config and use safe retained metadata,
not secret resolution. They concern supported current-format reset artifacts only;
`agent recovery` owns native-change recovery. Retain supported artifacts until
explicit discard, without expiry or restoration. Every completed capture remains
reachable even if later reset fails. Discard reports its recovery boundary, not
another resource's deletion. Unsupported-format records are rejected and left
unchanged; current recovery operations cannot inspect, restore, or discard them.

Examples: `tmax config reset --dry-run`, `tmax config reset`,
`tmax config recovery list`, `tmax config recovery inspect RECORD`, and
`tmax config recovery discard RECORD --dry-run`. Use a returned record handle;
a later discard recomputes safety instead of relying on the preview.

## Format migration

`config migrate [--dry-run] [--if-revision REVISION]` converts an existing
version-1 file to version 2. A version-2 file is refused as
`invalid_arguments` (exit 2, `field: version`): the configuration already uses
the current format, so migration is not needed and nothing is written. A dry run
writes nothing and reports the same plan and refusals.

A current-format document must reject the removed native-entry `gateway` and
`proxy` source fields as unknown fields. Only version-1 documents carry that
syntax, and only this command reads it.

1. Merge one record per route: version-1 records with the same alias `id` merge
   into one record with `protocols` only when their `candidates`, `enabled`,
   `continuity`, `fallback`, and `candidate_reasoning_effort` are identical.
2. Rewrite every version-1 native provider entry that uses `gateway = true` or
   its historical `proxy = true` spelling to `provider = "tmax"` with the same
   models.
3. Validate the complete converted document before writing.

The command must first store the exact pre-migration bytes in the existing
recovery area under [Invalid-file recovery](#invalid-file-recovery), then save
atomically. The retained bytes serve manual recovery only: `config recovery
list|inspect|discard` reports and deletes them, and there is no restore or
replay operation. On success the result reports the new revision, the merged
routes, and the rewritten entries.

There is no automatic migration and no dual-syntax acceptance. Migration must not
start the gateway or rewrite native files; after it succeeds, the user must start
the gateway and re-apply each affected target.

| Refusal | Result |
| --- | --- |
| Version-1 records with one `id` disagree on `candidates`, `enabled`, `continuity`, `fallback`, or `candidate_reasoning_effort` | Name the records and leave them untouched. |
| A version-1 `gateway = true` or `proxy = true` entry selects a concrete qualified model ID | Name the entry; a relayed entry selects public route names only, so define a route for that model or drop the selection. |
| A version-1 `gateway = true` or `proxy = true` entry has no gateway configuration table | Name the entry. |
| Any refusal | Exit nonzero and write nothing. |

Example: `tmax config migrate --dry-run`, then `tmax config migrate`.

## Configuration-impact data

Routine success identifies the requested edit and affected resources, returns
`config.file_revision` when saved state is known, and reports `data.save`
(`saved`, `unchanged`, or `not_requested`) and `data.runtime` (`loaded`,
`not_running`, `unchanged`, `not_loaded`, or `unavailable`). State explicitly when
native settings were not applied. Do not report saved state as loaded or native
application. Background synchronization failure is a warning after a completed
ordinary desired save, not a failed create. `data.loaded_revision_before` is the
separately observed pre-activation revision, or null when unavailable.
`data.loaded_revision_after` is populated only by an activation response that
confirms its revision; rejected/unavailable activation leaves it null. Never copy
the before observation into the after field: a concurrent writer may have loaded
another revision meanwhile. No-service validation/reload may succeed without a
loaded snapshot.

Dry-run returns the full redacted intended save and activation impact. Actual
results must also disclose consequential pending-file changes, session blocking,
listener/startup requirements, retained secrets, and any partial/unknown effect.
Routine success need not repeat a full diff or empty boundary arrays. Use
`config show`, the relevant resource inspection, and `status` for current detail;
these reads are not a replay of a past operation.

Where detailed impact is required, `data.config_impact` contains:

- `file_revision_before`, `file_revision_after`, `candidate_revision`, and
  `loaded_revision_before`: opaque strings or null when unavailable. Known file
  absence is `absent`. A computed candidate revision is not a reservation.
- `activation_diff_state`: `complete`, `no_loaded_snapshot`, or `unavailable`.
- `includes_pending_file_changes`: boolean, or null without a known loaded baseline.

Unavailable loaded state leaves `activation_diff_state = unavailable` and
`includes_pending_file_changes = null`. Keep the independently known `file_save`
changes; do not fabricate loaded-to-candidate additions as a complete activation
diff. Unavailable session impact uses null, not an empty complete affected set.

Dry-run leaves `file_revision_after` unchanged. Candidate revision is null when
provisioning is needed to determine it; never invent generated secret references.

`data.changes` entries are `{boundary, resource, path, kind, before, after}`.
Resource uses the [shared reference](../results/spec.md#result-envelope-and-output).
Path is schema-defined; kind is `add`, `replace`, `remove`, or `move`; values are
redacted supported field values or null for absence/unavailability.
`data.warnings` uses `{code, message, affected}`.

`file_save` changes describe only the requested saved edit. `activation` changes
describe the entire loaded-to-candidate difference, including pending manual
changes and dependent upstream credentials, shared gateway-key/publication,
endpoint/network, rule/input, listener, native-target, session, and startup effects.
Reload has no save changes. Catalog apply
includes its complete effective activation impact. Affected resources, blockers,
warnings, and next actions cover that full impact. No loaded baseline is not an
empty diff. Actual results retain the evaluated baselines when explaining a
failure and report saved-but-not-active state with the previous loaded revision.
