# Agent session portability

Shared terms are defined in [CONTEXT.md](../../CONTEXT.md). Within this
capability, *session* means an **Agent session**, not a Session binding, a login
session, or a gateway request.

Authority: [Agent integration](../integrations/spec.md) owns target scope and
version qualification; each session adapter owns native store and schema
detection. [Model selection](../model-routing/spec.md) owns model references and
capability metadata. This capability owns read-only session discovery and one-way
native session import. It does not own Agent configuration, provider
authentication, gateway routing, or retained conversation destinations.

## Scope

tmax must let one person:

1. List the native sessions that exist across declared Agent targets.
2. Create a new native session in one declared Agent target from a selected
   session in another target.

The source session is always read-only. The destination session is created once,
as a new native session, and then belongs to the destination Agent. tmax keeps
only a bounded import record for provenance and repeat detection.

The local Web interface may expose the same list and import operations. It uses
the same validation and result semantics, remains optional for every CLI task,
and must not return transcript bodies in list or import-preview responses.

The capability excludes:

- continuous synchronization, live session migration, or background copying;
- a central transcript store, backup bundle, or cross-machine transfer;
- importing providers, models, settings, structured credential state, or login
  sessions;
- copying project files, project instructions, project settings, or additional
  workspace roots;
- starting an interactive Agent session, resuming a session, or controlling a
  running Agent process;
- rewriting, deleting, or merging the source session;
- deleting, restoring, or merging an imported destination session; the destination
  Agent owns its retention and deletion;
- hidden reasoning, native credential stores, approvals, sandbox state, usage data,
  or provider-owned opaque tokens;
- generated summaries or any other inferred conversation content;
- search, transcript viewing, and attachment migration in the initial contract.

## Version and schema policy

This contract is defined over adapter capabilities, not one native schema. The
reader or writer adapter owns store discovery, version fingerprinting, and field
mapping. No core list or import rule depends on a fixed filename, table name,
field name, path encoding, or native CLI flag.

A reader declares the schema signatures it can safely read. An Agent type or
target ID does not imply a schema version. It may list bounded metadata from an
unqualified newer or older store when the shape is recognizable; otherwise the
target or session is `unsupported` or `unsupported_version` and other targets
remain readable. Safe metadata reads may leave optional fields null; they never
invent a value or infer it from a name. A writer is enabled only for an exact
qualified schema and native registration path; an unknown or changed schema fails
closed before native writes. No generic SQL, JSONL, or path-encoder fallback is a
writer.

Each adapter must:

- probe bounded metadata and schema identity, using a package or binary version as
  a hint rather than sole proof;
- tolerate unknown fields, leave native stores unchanged on read, and not infer
  semantics from names or from another Agent's shape;
- use a read-only snapshot or native read API; listing does not migrate, repair,
  checkpoint, or vacuum a native store;
- select one authoritative representation when a store exposes index, file, or
  cache views, and deduplicate by native ID;
- treat legacy or migrated layouts as separate read-only schemas rather than
  assuming they are current;
- keep version detection, mapping, and compatibility fixtures inside the adapter,
  with one qualified entry per supported schema.

## Laws

1. Listing and previewing write no native store and no tmax configuration.
2. Import never modifies the source and never overwrites an existing destination
   session.
3. An imported session is native-owned. tmax stores no transcript copy; the source
   and destination native stores hold the only session data.
4. Import changes no provider, model, default, role, credential, or gateway
   configuration. It copies recorded visible conversation content and binds the
   new native session to a working directory; it copies no project files or
   project settings.
5. An unknown Agent session schema, store version, or writer qualification fails
   closed before a native write.
6. Native visibility is required. A destination session that the destination Agent
   cannot discover, list, and read back through its own session path for the
   destination working directory or an all-session native reader is not a
   completed import.
7. Import is atomic where the native format permits it, and otherwise reports
   the exact partial state and recovery action.
8. Repeating the same import does not create a second session when repeat
   detection is available, unless the caller explicitly requests another copy.
9. Session content never leaves the local machine. List results and Import
   records contain bounded metadata, not transcript bodies; a title derived from
   the first user message is a bounded label, not a transcript view.
10. tmax does not generate a summary, rename a role, infer a model mapping, or
    invent tool semantics to make a source fit a destination.

## Session store and working directory

A target's session store is the exact native location bound to that target. The
resolver follows the target's native home/profile selectors and native session-root
selectors; it is not inferred from the target ID, a configuration filename, or the
process working directory. Where native settings relocate sessions, the resolved
root is used. If a target cannot be bound to exactly one store, list reports that
target as `unresolved` and import fails with `session_store_unresolved` rather
than guessing a default. For OpenCode, the store is the database or server
binding, not the configuration directory; the native data-home database is not
implied by a configuration scope.

Session-root selectors are resolved by the adapter under native rules, not tmax's
process cwd. Multiple selectors follow native precedence; tmax does not invent a
cross-agent precedence. A relative selector that cannot be resolved
deterministically, or a target with more than one plausible store, leaves the
store unresolved. When a store exposes index, file, or cache views, the adapter
selects one authoritative representation and deduplicates by native ID; it never
merges conflicting metadata or emits the same session twice.

A session's working directory is the primary directory recorded in native session
metadata, verbatim. It may be null, may no longer exist, and is not the target
path or the store path. If a schema distinguishes a current primary cwd from
historical per-turn directories, the adapter selects the current primary cwd
according to its qualification and does not merge those paths; if it cannot select
one, cwd is unknown and import requires `--cwd`. The adapter must not choose
arbitrarily. Additional workspace roots recorded by a multi-root format are native
workspace state; they are not a separate session row and are not canonical
visible history.

Listing scans the resolved store, not only a native picker's current
cwd/project. Native Codex, pi, omp, OpenCode, Hermes, and DSH surfaces may filter,
hide, or group sessions by cwd/project; such a filtered view is not proof that a
session is absent. Listing is an inventory: it includes native-readable archived,
hidden, and unnamed sessions and reports their flags; it does not apply a native
picker's current/unnamed/archived filters. List returns the recorded cwd and any
derivable additional roots, and native discovery verification uses a scope that
contains the destination cwd or an all-session native reader.

`--cwd` matches the primary recorded working directory exactly after lexical
normalization under the host path rules. It does not resolve symlinks, walk
descendants, or match additional workspace roots. A session with no recorded cwd,
or a cwd the adapter cannot select unambiguously, is excluded from a cwd-filtered
result and reported as `cwd_unknown` in the target summary.

The primary working directory is selected independently of transcript content.
Import never falls back to the destination's default or process cwd. The default
is the source's recorded primary cwd; `--cwd` explicitly overrides it. The
selected directory must be an absolute existing directory and representable by
the destination writer. A missing or null source cwd fails; a missing path fails.
The destination writer must bind the effective cwd to its native session
metadata, index, and project grouping where those exist. The result reports the
effective cwd; it may differ from the requested spelling after native
canonicalization.

Additional workspace roots are not imported. An import from a multi-root source
proceeds with primary-cwd-only retention and reports
`workspace_roots_not_imported` with the omitted count. This is an explicit
retention loss, not silent success.

## Security and local exposure

- Session discovery, listing, and import are local operations. No session
  content, metadata, or Import record is sent to a catalog, provider, telemetry
  service, or remote host.
- A session reader reads the native session store, not the native credential
  store. It does not resolve provider secrets to inspect or copy a session.
- Listing returns bounded metadata and, at most, a bounded title label. It does
  not return message bodies, tool arguments, tool output, or file content.
- Import writes the selected visible text and tool digest into the destination
  Agent's native store. tmax does not guarantee detection or redaction of secrets
  that the source recorded inside message text or tool output.
- Visible source text is untrusted content. tmax does not sanitize instructions,
  tool output, or prompt-injection content; the destination Agent's own system
  context remains authoritative.
- Import records contain counts and provenance metadata only, never transcript
  bodies.
- The destination working directory may cause the destination Agent to load
  native project instructions, settings, trust rules, and tool permissions. tmax
  neither copies nor vouches for that project context.
- Errors may name a target, native ID, store path, or reason code, but must not
  copy message bodies or tool output into logs or ordinary results.
- The optional local Web interface follows the existing local-process trust and
  loopback boundaries. It must not persist transcript content in browser storage
  or return it from list and import-preview responses.

## Discovery and listing

### Invocation

```text
tmax session list [--target TARGET]
                  [--agent AGENT --path PATH]
                  [--cwd PATH] [--since INSTANT] [--until INSTANT]
                  [--limit N] [--format json|text]
```

- With no selector, listing scans every declared Agent target.
- `--target` restricts the scan to one declared target. It is single-valued and
  conflicts with `--agent`/`--path`.
- `--agent` and `--path` together inspect one explicit native scope without
  registering a target or importing configuration. `--path` requires `--agent`
  and an explicit scope is list-only.
- `--cwd` is an absolute path. For listing it matches the primary recorded
  working directory exactly after lexical normalization under the host path
  rules; it is not a recursive path filter, does not resolve symlinks, and does
  not match additional workspace roots. A session whose recorded cwd is null or
  ambiguous is excluded from a cwd-filtered result and reported as `cwd_unknown`
  in the target summary.
- `--since` and `--until` are inclusive, both by the session's updated time. A
  session whose updated time is unknown is excluded from a time-filtered result
  and reported as `timestamp_unknown` in the target summary. To page older
  results, set `--until` to the oldest returned `updated_at`; rows sharing that
  timestamp may repeat and are deduplicated by the caller using target and native
  ID.
- `--limit` is a positive integer; the default is 50 and the maximum is 500. The
  result reports `truncated: true` when more sessions matched, so the caller can
  narrow with filters or use the maximum.
- `--format` follows the shared CLI result contract. JSON is the default.

Listing requires no daemon, gateway, provider credential, or model selection.
A listing row is produced from native metadata and bounded reads; the reader must
not parse an entire transcript solely to produce a row.

Session list and import never prompt for a missing target, session, title, model,
or conflict decision. Missing required input is an error.

### Result

Each row identifies one native session. For a declared target, the Session
reference is the pair of `target` and `native_id`. For an explicit `--agent --path`
scope, the pair is `scope` and `native_id` and is list-only; it cannot be imported
because import requires declared source and destination targets.

| Field | Meaning |
| --- | --- |
| `target` | Primary declared target, or null for an explicit scope |
| `targets` | All declared targets associated with the same store and native session; empty for an explicit scope |
| `scope` | Resolved explicit native scope path, otherwise null |
| `agent` | `codex`, `pi`, `omp`, `opencode`, `hermes`, or `dsh` |
| `native_id` | Exact native session identifier |
| `title` | Native title, or a bounded first-user-prompt fallback |
| `cwd` | Primary recorded working directory, verbatim; null when absent or ambiguous |
| `workspace_roots` | Additional native workspace roots beyond `cwd` when derivable; empty when none, null when unknown |
| `created_at`, `updated_at` | Native timestamps, when present |
| `model` | Recorded provider/model identity, when present |
| `archived` | Native archived state when recorded; otherwise null |
| `hidden` | Native hidden state when recorded; otherwise null |
| `message_count`, `tool_call_count` | Counts when derivable without an unbounded parse; otherwise null |
| `state` | `readable`, `incomplete`, `unsupported_version`, or `unreadable` |
| `imported_from` | `{target, native_id}` from an Import record, otherwise null |
| `store` | Opaque native store identity, never a transcript body |

`imported_from` is provenance from the Import record. It does not prove that the
destination session still contains the imported history or that the source still
exists.

The result also reports one entry per scanned target or explicit scope with its
overall state and reason code, including `cwd_unknown` when a cwd-filtered scan
excluded rows whose primary cwd was absent, null, or ambiguous. Target states are
`scanned`, `absent`, `partial`, `unsupported`, `unresolved`, or `unreadable`;
`absent` means the resolved store does not exist, not that it is unreadable. One
unreadable or unresolved target must not suppress readable rows from another. A
completed scan exits 0 even when some targets are unreadable, unresolved, or
unsupported; an invalid selector or unknown declared target exits 2. The command
does not present partial target coverage as a complete inventory. With no declared
targets and no explicit scope, the result is an empty inventory, not an error.

Rows are ordered by `updated_at` newest first, then by target and native ID for a
deterministic tie-break. Rows with unknown updated time sort after known rows. If
a native title is absent, the reader may use a bounded prefix of the first user
message. It must not generate a title.

Rows are unique by `store` and `native_id`. When several declared targets resolve
to the same native store, the row lists them in `targets` in lexical target-ID
order and uses the first as `target`. A `--target` filter limits which stores are
scanned; it does not hide other declared targets that resolve to a scanned store.
The result exposes the store identity so a caller is not required to treat target
IDs as stores. The same native session is never returned twice merely because two
targets resolve to it.

Listing does not return message bodies, tool arguments, tool output, or file
content. A later read-only inspection operation may expose a bounded preview
under a separate contract.

## Session import

### Invocation

```text
tmax session import SOURCE_TARGET SESSION_ID --to DESTINATION_TARGET
                    [--title TITLE] [--cwd PATH]
                    [--tail TURNS] [--allow-over-budget]
                    [--again] [--dry-run] [--format json|text]
```

- `SOURCE_TARGET` and `DESTINATION_TARGET` must be declared Agent targets.
- `SESSION_ID` is the exact native ID returned by listing. The Session reference
  is the pair of source target and native ID, not a joined string.
- `--title` overrides the imported title. It must be nonempty, contain no control
  characters, and fit the destination's native title limit; an invalid override is
  rejected. When absent, the source title or its bounded first-prompt fallback is
  used. A title collision is resolved with a bounded numeric suffix at native write
  time; if no unique title can be produced, import fails.
- `--cwd` is an absolute existing directory and overrides the destination working
  directory. Without it, import uses the source's primary recorded cwd. A null or
  missing source cwd fails; tmax never falls back to the destination target's
  default cwd or the process cwd. The destination writer may canonicalize the
  path; the result reports the effective cwd.
- `--tail TURNS` is a positive integer and limits the import to the most recent
  turns, even when the full visible branch would fit. It requires retention of the
  first retained user prompt and any preservable source summary.
- `--allow-over-budget` imports the full visible branch despite a known budget
  overflow. It conflicts with `--tail`.
- `--again` permits a new destination copy after an earlier completed import.
- `--dry-run` produces the complete plan without reading native credential stores,
  writing native files, or creating an Import record. It reports the destination
  target and store, but does not allocate or reserve a native session ID.

### Preconditions

Import must fail before any native write when:

- either target is undeclared, the source session does not exist, or the source
  state is not `readable`;
- the source snapshot is incomplete or contains a torn or unparseable record that
  affects the selected visible branch;
- the source exceeds the reader's documented bounded size or record limit and the
  requested selection cannot be read safely;
- the source and destination resolve to the same native store;
- either target cannot be bound to exactly one native session store;
- the source or destination Agent version has no qualified reader or writer;
- the destination store is active or locked and the qualified writer cannot
  write safely;
- the destination reader cannot determine a required effective model;
- the source records no primary working directory, or the adapter cannot select
  one unambiguously, and `--cwd` is absent;
- the selected working directory is not an absolute existing directory, or the
  destination writer cannot represent or bind it;
- the source visible branch is ambiguous;
- `--tail` and `--allow-over-budget` are both present;
- a completed import already links this source to this destination and `--again`
  is absent;
- the selected history exceeds a known budget and neither `--tail` nor
  `--allow-over-budget` is present.

A precondition failure has no native effect. A dry run reports the same
conditions plus the proposed destination identity, counts, omissions, title, and
budget outcome.

### Canonical visible history

The source reader captures a stable snapshot and selects one **visible branch**:
the root-to-leaf path ending at the most recent identifiable leaf. If the format
has several equally recent leaves or cannot identify a leaf, import fails rather
than choosing an arbitrary branch. A **turn** begins at a user message and
includes the following assistant and tool activity until the next user message.

The reader extracts only visible conversation content and bounded metadata. It
does not read native credential stores. Message text and tool output may contain
secrets that the source recorded; tmax does not claim to detect or redact
arbitrary secrets embedded in visible text. The copy remains local and uses the
destination Agent's native file or database permissions; tmax must not broaden
access beyond the destination's normal storage.

The canonical history contains:

| Source content | Import behavior |
| --- | --- |
| User and assistant visible text | Preserve in order |
| Source-native compaction or summary records | Preserve only when the destination has an equivalent native representation; otherwise omit with a reason |
| Tool activity | Preserve as non-replayable text: tool name, recorded arguments, bounded outcome, error/exit state, and changed paths |
| Source title, timestamps, model identity | Preserve as metadata where the destination format supports the field; model identity is provenance, not an automatic model selection |
| Primary working directory | Bind the destination session to the effective cwd; it is session metadata, not visible history |
| Additional workspace roots | Omit; report the count as `workspace_roots_not_imported` |
| Hidden reasoning or thinking traces | Omit |
| System, developer, environment, and harness-injected context | Omit when the source identifies it distinctly; otherwise preserve as recorded and report `provenance_unknown` |
| Native credential fields, authorization headers, and provider tokens | Omit structured credential state. Secrets embedded in visible text or tool output are copied as recorded and are not guaranteed to be detected or redacted |
| Approvals, sandbox/permission state, usage, cost, token counts | Omit; they describe the source runtime or observation, not portable conversation |
| File snapshots, worktree state, attachments, images, audio | Omit in the initial contract with explicit counts |
| Parent, child, subagent, and forked-session relations | Do not copy; the selected session is imported as a standalone session |
| Structured tool calls and results | Do not recreate as executable or replayable native tool records |

The writer must not turn a source tool call into a destination tool call with a
different vocabulary, and must not replay a historical tool result. Tool activity
is context, not an executable plan. A source archived or hidden state is metadata;
the imported destination session is created in the destination's normal active
state.

### Context budget

When the destination writer can determine the effective model and its context
limit, the imported history must fit a conservative destination budget that
leaves room for the destination Agent's own system context, tool definitions,
new turns, and output. If it does not fit:

- without `--tail` or `--allow-over-budget`, import fails with
  `context_budget_exceeded`;
- with `--tail`, the writer keeps the first retained user prompt, any preservable
  source summary, and the most recent turns that fit; it inserts explicit omission
  markers and reports the dropped turn and byte counts;
- if the pinned content alone does not fit, import fails;
- `--allow-over-budget` imports the full visible branch and reports the known
  overflow.

When the effective model or limit is unknown, import proceeds with a reported
`context_unknown` warning. The writer must not guess a limit or silently truncate.

tmax never asks a model to summarize the source, and never substitutes a
generated handoff brief for the recorded conversation.

### Native write and visibility

The qualified destination writer:

1. obtains a new native session ID from the native importer, or allocates one for
   a direct writer;
2. maps the canonical history into the destination's version-qualified format;
3. creates the native session through an official import operation when one is
   available, otherwise through the qualified file or database writer;
4. registers the session in the destination Agent's own discovery/index path;
5. reads the new session back through the destination reader;
6. confirms that the destination Agent's discovery sees the session and that the
   canonical visible text and tool digest, title, working directory, and model
   metadata match the plan, allowing destination-assigned IDs and timestamps.

A destination session that is not discoverable by the destination Agent is a
failed import. Verification uses native discovery that contains the effective
cwd or an all-session native reader; absence from a cwd/project-filtered picker
is not proof of failure. The operation reports the destination's native ID
whenever one is known.

Import does not start an interactive Agent session, send inference, or resume the
destination Agent. It may return a structured native resume hint, but the hint is
informational and is not proof of successful resume.

### Result

A completed import reports at least:

| Field | Meaning |
| --- | --- |
| `source` | `{target, native_id, revision, model, state, cwd, workspace_roots}` captured for the plan |
| `destination` | `{target, native_id, store, title, cwd}` for the created session |
| `counts` | Imported turns, messages, tool-activity entries, and compaction records |
| `omitted` | Counts and reason codes for excluded content, such as other branches, reasoning, attachments, additional workspace roots, or unsupported summaries |
| `context` | `{state, model, limit, reserve, estimated, selected_turns}`; unknown facts remain null |
| `visibility` | `verified` or the exact failed registration/readback state |
| `record` | `created`, `unavailable`, or `failed`, with the bounded Import record identity when available |
| `resume` | Optional structured native resume hint `{program, arguments}` or null; tmax does not execute it |
| `recovery` | Present when native work is partial or verification is incomplete |

A dry run reports the same plan with source and selected counts, the destination
target and store, title resolution, working-directory resolution, omissions, and
context state. It reports no allocated destination native ID.

Shared result categories apply:

| Exit | Session outcome |
| --- | --- |
| `0` | Listing completed, or import completed and verified |
| `2` | Invalid selector, conflicting options, invalid operand value, unknown target/session, or unsupported source/destination schema version |
| `3` | Blocked before native effects: source unreadable/incomplete/oversized, conflict, unqualified writer, same store, unresolved session store, missing/null/unrepresentable working directory, unknown required model, destination lock, or context budget |
| `4` | Native write, registration, verification, or a record write after native commit failed or is partial/uncertain |
| `5` | Cancellation before any native effect |

`record_unavailable` is a warning on an otherwise completed import and remains
exit `0`. A `record_failed` after a verified native commit is a partial outcome and
does not invalidate the native session. `workspace_roots_not_imported` is a
warning with the omitted count on an otherwise completed import; the dry run
reports the same omission.

A listing that completed with unreadable targets remains exit `0` and reports
the per-target states; it must not claim complete coverage.

### Conflicts and repeat detection

tmax records one bounded **Import record** per completed import under its own
runtime state directory, not in the Agent configuration. Each completed import
creates a record with:

```text
source store + source native ID + source revision
  → destination store + destination native ID
```

Target identifiers and paths are recorded as metadata, not as the identity key.
This prevents a second target that resolves to the same native store from
bypassing repeat detection. A different resolved store identity does not inherit
conflicts from another store; old records establish conflicts only for the identity
they recorded. The record contains counts, adapter versions, source and destination
schema signatures, a bounded title label, source and destination primary working
directories, the omitted additional-root count, timestamps, and a source revision
identity. It does not store the transcript; a title label derived from the first
user message remains a bounded label, not a transcript body.

Import record storage has its own version. A missing store is empty. An
unsupported or unreadable store, or a malformed record within a supported store
version, does not block the native import; the result reports `record_unavailable`
and discloses that repeat detection is unavailable for this operation. This is
safe because a duplicate import creates another native session and never
overwrites one.

Import records are bounded by a documented count and byte limit. Pruning the
oldest records may remove an `imported_from` annotation or repeat detection for an
old import; it never changes a native session. The result reports the record
state when pruning affects the current operation.

- If any completed record for the source/destination store pair still has an
  existing destination session, import fails with `already_imported` and returns
  the existing destination reference or references.
- `--again` creates another destination session with a new native ID and a new
  record. No overwrite or in-place update is available.
- If every recorded destination session for the pair no longer exists, a new
  import is allowed.
- If the Import record cannot be written after a verified native commit, the
  result reports the destination ID and `record_failed`; the native session is
  not deleted merely to make provenance consistent.

### Failure and recovery

| Failure point | Required effect |
| --- | --- |
| Before native write | No destination session, no Import record, source unchanged |
| During a file-based native write | Remove the incomplete tmax-created artifact when it is not yet discoverable; otherwise report it |
| During a database transaction | Roll back when the native format supports rollback; otherwise report the exact committed state |
| After native commit, before verification | For a tmax-created file that is not yet discoverable, remove it and report failure; for a database or official-import commit, report the destination ID, `committed_unverified`, and the required inspection or cleanup action |
| After verification, but the Import record cannot be written | Report `record_failed` with the verified destination reference; an unavailable record store is `record_unavailable` instead |
| Source changed during read | Import the captured snapshot and report its revision; never reread and merge a newer source into the same plan |

The source is never rolled back, rewritten, or deleted as part of recovery.

## Capability qualification

The initial target contract requires:

| Agent | List | Import destination | Native visibility path |
| --- | --- | --- | --- |
| Codex | Required | Codex CLI targets | New rollout plus the version-qualified native registration/index path; desktop-app registration is not implied |
| pi | Required | Required | New session file under the resolved session root and effective cwd bucket; native reader sees it |
| omp | Required | Required | New session file under the resolved session root and effective cwd bucket; native reader sees it |
| OpenCode | Required | Required through the official import operation when the installed CLI supports it | Native database/index after the official import; legacy session schemas are separate |
| Hermes Agent | Required | Not qualified initially | Read-only native session database; direct writes require a later qualified writer |
| DSH | Required | Not qualified initially | Read-only native session history; writes require a later qualified writer |

An Agent version outside its qualified reader or writer range remains listable
when the reader can safely identify metadata; import fails closed. Observed
native formats are evidence for a specific version, not a version range or a
license to write an arbitrary schema.

## Acceptance boundary

Acceptance requires sanitized native fixtures and actual installed Agent entry
points; it does not require paid inference.

1. List sessions for each supported Agent without changing native files, including
   a corrupt or unsupported session, an absent or unresolved store, an archived or
   hidden session, a null-cwd session, an ambiguous-cwd session, a session whose
   cwd no longer exists, a multi-root session, a qualified compressed Codex
   rollout, and rows whose exact counts remain null rather than triggering an
   unbounded transcript parse. List an explicit undeclared scope and confirm it
   remains list-only. Confirm an unsupported session remains explicit rather than
   disappearing; `truncated` is reported when more unique rows match the limit;
   `--cwd` matches only the primary recorded directory; and a null or ambiguous
   cwd is reported as `cwd_unknown`.
2. Import a selected visible branch between pi and omp in both directions, then
   reopen the destination through a native all-session reader and a discovery
   scope containing its effective cwd, verify the new session is listed, and
   verify its native resume entry resolves without sending paid inference. Absence
   from a cwd/project-filtered picker is not a verification failure.
3. Import from Codex, pi, omp, OpenCode, Hermes, and DSH as sources into the
   qualified Codex CLI, pi, omp, and OpenCode destinations. Exercise both direct
   file writers and the official OpenCode import path. Include archived, hidden,
   and multi-root source sessions; confirm the destination session is created
   active and a multi-root source reports `workspace_roots_not_imported`.
4. Confirm tmax modifies no source session record or content; incidental read-only
   database bookkeeping does not count as a session change. No source session is
   deleted or rewritten.
5. Confirm hidden reasoning, system context, native credential state, approvals,
   usage data, attachments, and parent/child relations are excluded or explicitly
   reported. Confirm secrets embedded in visible text or tool output remain a
   documented copy risk, not an unverified redaction claim.
6. Exercise title collision, null, missing, or ambiguous recorded cwd, an explicit
   existing absolute `--cwd`, a relative or invalid `--cwd`, an unrepresentable
   destination cwd, ambiguous branch, oversized source, over-budget history,
   unknown model, same-store source/destination, duplicate import through a second
   target resolving to the same store, unresolved session store, unavailable
   Import record store, destination lock, registration failure, and verification
   failure. Confirm the effective cwd is reported and destination project files
   or settings are not copied.
7. Confirm list results return no message bodies or tool output beyond a bounded
   title label, Import records contain no transcript body beyond a bounded title
   label, and no session content is sent to a network service.
8. Confirm a failed import leaves no discoverable partial session, and an
   uncertain post-commit failure reports the exact destination state and recovery.
9. Exercise two recognized schema signatures for one Agent and at least one
   unrecognized signature. Confirm known fields list without unbounded parsing,
   unknown fields are tolerated, an unqualified writer fails before native
   writes, one target's unsupported version does not suppress another, and
   overlapping index/file/cache views do not duplicate rows.
