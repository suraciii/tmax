# Configuration commands

This child owns configuration/status grammar and results. The
[configuration contract](../../configuration/spec.md#save-reload-and-apply)
owns persistence, activation, and native-application boundaries. All commands use
[common CLI rules](../spec.md) and [shared results](../results/spec.md).

## Explicit configuration changes

| Command | Result or requested boundary |
| --- | --- |
| `status` | Compact config path/revisions, resident/listener identity and versions, provider/target readiness counts, recovery conditions, and startup drift/failure. Return blockers and supported detail actions where needed. |
| `config show [--state STATE]` | Redacted structured config and its revision. A missing loaded snapshot is explicit. |
| `config validate` | Validate current saved config; separate syntax/type/reference errors from external availability. Missing config stays absent. |
| `config reload [--if-revision REVISION] [--dry-run]` | Validate the entire saved file and activate it if the service runs, without rewriting it, applying native settings, signing in, or starting listeners. |

Desired-state operations accept `--dry-run` and optional `--if-revision REVISION`.
This includes credential-reference changes, catalog apply, shared proxy-key
replacement, and autostart intent. No preceding preview/authored document is required.

Edits validate the complete candidate, preserve unrelated comments/fields/order,
and activate the whole valid candidate when the service runs. Pending manual
edits are part of that activation, not a per-field overlay. There is no file
watcher. Missing credentials/qualification remain availability conditions.
No-service edits report saved but not loaded. A failed save cannot activate
unsaved state. Secret creation, sign-in, save, catalog apply, activation, autostart,
and native work remain separate failure boundaries.

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
| Native Agent model subset | A configuration choice, not a proxy-request restriction. |

Examples: `tmax status`, `tmax config show --state loaded`,
`tmax config validate`, `tmax config reload --dry-run`, and `tmax config reload`.

## Invalid-file recovery

`config reset [--if-revision REVISION] [--dry-run]` replaces an existing invalid
or corrupt config, not a valid file or an absent file. Ordinary setup initializes
an absent file; ordinary edits/removals change a valid one. No authored TOML is
required.

Before replacement, preserve the exact rejected bytes encrypted and protected
under `~/.tmax/recovery`; malformed content may contain secrets. Return an opaque
recovery handle, never the contents. Unsafe preservation, unknown current file
identity, or concurrent change blocks overwrite. Save `version = 1` with empty
desired collections as one valid change. If preservation succeeds but save fails,
return the retained handle and completed recovery/failed save boundaries.

With a service running, activate the empty desired configuration atomically,
removing configured proxy access and shared publication. Retain old bindings
blocked, not reassigned; admitted requests follow normal continuity. Leave a
running listener bound but unavailable rather than stopping/moving it.
`proxy stop` is separate. Native files/upstream consumers, stored secrets,
credential ownership, and recovery records remain untouched. Do not sign out,
apply/restore native settings, start services, or delete retained credentials.
Omitted autostart means disabled intent and uses normal changed-intent
reconciliation. Report recovery/save/activation/autostart separately.
Later setup uses ordinary commands, including explicit proxy provisioning.

Dry-run reports rejected-file identity, protected preservation/reset, and full
proxy-key/publication/session impact without creating an artifact. There is no
snapshot catalog, invalid-byte replay, restore service, partial salvage, or automatic reset.

| Command | Retained invalid-file artifact operation |
| --- | --- |
| `config recovery list` | Safe handles, capture times, and protection state; no rejected content. |
| `config recovery inspect RECORD` | Config-file identity, original revision, byte count, capture time, rejection category, ownership/protection, and discard support. Never decrypt rejected content. |
| `config recovery discard RECORD [--dry-run]` | Delete only that exact owned artifact after current ownership/concurrency checks. Do not change settings, credentials, native recovery, or bindings. |

These operations work with absent/invalid config and use safe retained metadata,
not secret resolution. They concern only reset artifacts; `agent recovery` owns
native-change recovery. Retain artifacts until explicit discard, without expiry
or restoration. Every completed capture remains reachable even if later reset
fails. Discard reports its recovery boundary, not another resource's deletion.

Examples: `tmax config reset --dry-run`, `tmax config reset`,
`tmax config recovery list`, `tmax config recovery inspect RECORD`, and
`tmax config recovery discard RECORD --dry-run`. Use a returned record handle;
a later discard recomputes safety instead of relying on the preview.

## Configuration-impact data

Routine success identifies the requested edit and affected resources, returns
`config.file_revision` when saved state is known, and reports `data.save`
(`saved`, `unchanged`, or `not_requested`) and `data.runtime` (`loaded`,
`not_running`, `unchanged`, or `not_loaded`). State explicitly when native settings
were not applied. Do not report saved state as loaded or native application.
No-service validation/reload may succeed without a loaded snapshot.

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

Dry-run leaves `file_revision_after` unchanged. Candidate revision is null when
provisioning is needed to determine it; never invent generated secret references.

`data.changes` entries are `{boundary, resource, path, kind, before, after}`.
Resource uses the [shared reference](../results/spec.md#result-envelope-and-output).
Path is schema-defined; kind is `add`, `replace`, `remove`, or `move`; values are
redacted supported field values or null for absence/unavailability.
`data.warnings` uses `{code, message, affected}`.

`file_save` changes describe only the requested saved edit. `activation` changes
describe the entire loaded-to-candidate difference, including pending manual
changes and dependent upstream credentials, shared proxy-key/publication,
endpoint/network, rule/input, listener, native-target, session, and startup effects.
Reload has no save changes. Catalog apply
includes its complete effective activation impact. Affected resources, blockers,
warnings, and next actions cover that full impact. No loaded baseline is not an
empty diff. Actual results retain the evaluated baselines when explaining a
failure and report saved-but-not-active state with the previous loaded revision.
