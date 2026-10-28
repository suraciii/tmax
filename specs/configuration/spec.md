# Configuration and activation

This specification owns tmax's private persisted configuration schema and the
save, reload, and agent-application boundaries. [Product](../product/spec.md)
owns product behavior; [Model selection](../model-routing/spec.md) owns metadata
inheritance, model identity, priorities, and continuity. [Input values and
acquisition](../model-routing/inputs/spec.md#input-values-and-acquisition) owns
runtime input records, freshness, and source acquisition. The schema is a target
contract, not a claim of implemented runtime support. [example.toml](example.toml)
illustrates private persistence; it is never required setup input.

## Format and authority

The canonical model-service table is `[gateway]`; an Agent provider selects it
with `gateway = true`. Readers accept the historical `proxy` spelling only at
these two schema locations. Supplying both names at one location is invalid.
Reads and dry runs never rewrite the authority. An explicit successful save uses
the canonical names, retains the shared-key reference and unrelated settings,
and reports `configuration_migration = "gateway_names"` for a resource edit that
converts historical names. User-chosen IDs and `network.proxy_url` are not renamed.
Existing shared-key records with the historical sole-service owner remain valid;
inspection presents the canonical owner without rewriting key bytes or references.


The sole authority is `~/.tmax/tmax.toml`, one UTF-8 TOML 1.0 document. `~`
means the operating-system user's home directory, including on Windows, not the
working directory or a calling agent's directory. CLI and Web operate on this
same authority and display its resolved path. There is no configurable tmax home,
alternate configuration selection, per-agent tmax authority, tmax profile tree, or
merge of project, user, and system configurations. There is no include mechanism,
template language, environment interpolation, or executable configuration.
Relative native-scope paths resolve against `~/.tmax`, never the working directory;
they do not relocate that agent's native configuration.

All tmax-owned persistent configuration, operational data, caches, logs, secrets,
and recovery information stay under `~/.tmax`. Native-agent files and supported
native credential stores remain at their native locations; OS startup
registration is the external integration exception. New tmax secret and recovery
payload records use storage version 2 and are plaintext files restricted to the
current OS account; file permissions are not encryption. This storage change does
not change the desired configuration schema version. The [product secret-protection
contract](../product/spec.md#local-access-and-secret-protection) owns that access
boundary and credential handoff. Request history remains in memory, not in the
configuration or any persistent history store.

The root requires `version = 1`; this revision does not introduce a new schema
version. Other root fields are `providers`, `model_aliases`, `agents`, `gateway`,
and `network`; omitted collections are empty. Unknown fields, duplicate TOML keys,
unsupported versions, wrong types, and non-finite numbers must be rejected with
a field path and actionable error. A rejected file must not be partially applied.
Errors, dry runs, and other results must redact secret values, including values
accidentally placed in unsupported fields.

In the desired authority, concrete model and model-alias `tier` fields are
unsupported and must be rejected as unknown fields with their paths. Loading,
saving, or reloading that authority must not infer replacement candidates or
perform an automatic conversion. The [usage guide](../../docs/usage.md) describes
explicit user-chosen conversion.

Models containing dots or slashes use quoted table keys. IDs are exact and
case-sensitive. Provider-ID and qualified-model parsing follow
[Qualified model names](../model-routing/spec.md#qualified-model-names).
Agent target IDs and model-alias IDs must be nonempty strings without whitespace
or control characters. An Agent's native display prefix is not part of an alias request ID.

### Explicit management input

Every supported user-editable persisted setting must be manageable through
explicit CLI resource arguments, with Web as an optional equivalent surface.
System-managed fields such as `version` and the provisioned `gateway.key_ref`
are produced by their owning operations, not arbitrary caller assignments.
Callers never need to author or submit TOML, JSON, or YAML documents. The
persisted schema is not a management request language; there is no full-document
proposal or input-file save operation. Finite schema-defined field/value
arguments are ordinary CLI input, not a new schema or arbitrary document
language. The [CLI contract](../cli/spec.md) owns the resource grammar and
argument coverage.

Operations add, update, remove, or rename named resources and explicitly set,
clear, add, or remove their supported settings and entries. Omitted arguments
leave existing settings unchanged except in an explicitly complete replacement
operation, where omission removes the replaced entry's optional fields. Omission
never deletes an unrelated resource. Reference-changing renames are atomic
configuration changes. Callers choose operation order; tmax must not guess which
provider, target, model, default, or candidate to add, remove, or replace.

CLI and Web preserve unrelated settings, comments, and ordering. They must not
rewrite unrecognized configuration into an accepted subset. An execution reads
current inputs and checks content revisions, not just file times, before
overwriting; concurrent changes during that execution produce a conflict.
There is no required earlier preview, confirmation token, persisted plan, or
reservation. Separate calls, including a call following a dry run, recompute
from current state under [Dry run and execution](../cli/spec.md#dry-run-and-execution).

Every operation that may save the authority accepts optional `--if-revision
REVISION`. It requires the file's current content revision to equal that opaque
token before any requested effects; the literal `absent` requires that no file
exists. A mismatch is an effect-free `inputs_changed` conflict, not a request to
rebase or retry. This caller-supplied guard supplements mandatory in-operation
revision checks. Replacing or removing an existing index-addressed schedule or
reasoning-option entry requires this guard: an index alone cannot identify the
caller's intended entry across invocations. Named IDs need no mandatory preflight.
The [CLI contract](../cli/spec.md#scalar-field-operands-and-defaults) owns syntax.

A missing authoritative file is a distinct execution input. An explicit
configuration-changing operation may create the versioned file; it must fail
before overwriting if another operation creates it meanwhile. An existing
invalid file must never be silently replaced with an empty one. Named operations
can correct recognized supported fields when the complete resulting document
validates; they must not require the caller to edit TOML. Unreadable, unparseable,
or unrecognized input produces a clear error and the explicit recovery choice
below, not an arbitrary-file salvage or document-authoring requirement.

Optional direct file inspection/editing and explicit reload remain an advanced
escape hatch, never the required setup or repair path for supported fields.
Manual edits activate through explicit reload or a subsequent executed management
edit that activates the whole valid file, not through a hidden watcher.
Secrets, native sessions, catalog snapshots, stable internal model identities,
usage observations, input values, and active/applied-state records are not copied into this
file. The accepted metadata snapshot is retained under `~/.tmax`; an inaccessible
snapshot must not be replaced by newly downloaded defaults without explicit
catalog application. Declaration validation must not launch sign-in, start a gateway,
or apply native configuration.

Historical operational snapshots are not desired authority. Explicit normalization
of supported legacy identity fields and blocker codes occurs in memory without
read-time writes; desired configuration does not accept legacy aliases.
Explicit candidates remain unchanged; former tier-backed membership comes only
from the same historical snapshot, including disabled models. Historical empty
membership stays unavailable internally, not valid desired configuration.
Corrupt records, duplicate fields, or conflicting legacy/canonical fields fail
closed, including historical evidence nested inside a canonical session binding.
Normalization must preserve provider, model, authorization, input/head, session,
and resource identities, continuity tokens, retained evidence, and permanent blockers.
Canonical snapshots may persist only through existing guarded mutations. Neither
normalization nor an unknown format authorizes clearing state or resetting a
binding. Users must not need to edit or delete operational routing/session JSON.

Historical provider child-identity maps normalize to `models`, selected/bound
Model identities to `model_identity`, and captured selected names to
`qualified_model`. Model removal/disablement blockers normalize to `model_removed`
and `model_disabled`; identity tie-break reasons normalize to
`stable_model_identity`. Only known routing/evidence envelopes are normalized;
arbitrary metadata and exact upstream model-ID maps are not rewritten. New
operational writes and results use canonical names only.

### Invalid-authority recovery

`config reset` is an explicit last-resort recovery operation for an existing
invalid `~/.tmax/tmax.toml`, not a general bulk-removal operation for valid
configuration. A missing authority uses ordinary resource initialization.
The caller supplies no replacement document. Reset replaces the entire desired
configuration with `version = 1` and empty desired collections; omission is
destructive only because this operation explicitly requests that complete reset.

Before replacement, preserve the exact original bytes in a current-user-restricted
plaintext recovery file under `~/.tmax/recovery` and return an opaque recovery
handle, not the original contents. Malformed input may itself contain accidentally
entered secrets and must receive the same file-access restriction. Failure to read
and preserve the complete original safely blocks replacement. Detect intervening
content changes before overwrite; do not reset newer content captured by a
different revision. If capture succeeds but saving fails, report the retained
recovery handle and the unsaved boundary without claiming a successful reset.
`--dry-run` reports the destructive scope and planned restricted preservation
without writing, reserving a handle, reading secrets, or contacting upstreams.

Keep captured recovery artifacts until explicit discard. CLI list and inspect
operations expose only safe artifact metadata, file-access/capture state, and
available actions, never arbitrary original bytes or secret values. Discard
removes only the named artifact, not configuration, credentials, or native files.
Recovery inventory and actions include only supported storage-format files.
Unsupported-format files are rejected and remain unchanged; they are unavailable
to current recovery actions. Corrupt or unknown metadata blocks deletion. The
[CLI contract](../cli/spec.md) owns this minimal recovery command grammar. Neither
reset nor inspection automatically restores invalid content; this is recovery
retention, not a configuration backup or snapshot-management system.

With a running service, activate the empty valid snapshot under the normal
save/activation boundaries. This removes managed model publication and shared
gateway access for subsequent requests; in-flight requests retain their admission
snapshots under the continuity contract. Retain existing conversation bindings
but block their reuse rather than choosing another destination. Do not stop or
hot-move an existing listener: it remains bound but unavailable until explicit
listener operations. The absent autostart flag is disabled intent and follows
the normal changed-intent reconciliation rules below.

Reset does not sign out, revoke upstream credentials, apply or restore native
files, delete stored secrets, erase retained authorization comparison bases,
or discard current-user-restricted plaintext recovery records. Native consumers
accessing upstreams directly can continue outside tmax and must be identified as
unchanged.
Recreate desired settings through ordinary explicit resource operations.
If persistence succeeds but
activation fails, report saved-but-not-active and retry activation through
explicit reload, not by resetting the now-valid file again.

## Provider instances

`[providers.<id>]` is a local access instance. Its table key is its configurable
provider ID.

Provider and AgentTarget are the principal managed objects under the
[product ownership contract](../product/spec.md#purpose-and-scope). The `providers`
and `agents` keys keep their existing syntax. Models are provider-owned children;
account facts, authentication configuration, and optional subscription information
do not introduce account, credential, or subscription resources. A Provider
integration selects qualified behavior, not a universal upstream-service identity.

Supported provider fields:

| Field | Type and meaning |
| --- | --- |
| `metadata` | Optional exact models.dev provider key; omission means no catalog metadata inheritance |
| `integration` | Optional supported tmax Provider integration ID; required when no qualified mapping exists for `metadata` |
| `enabled` | Boolean, default true; false blocks new managed use |
| `region` | Optional exact region ID from the selected known integration; invalid for a custom integration |
| `endpoints` | Optional table from protocol name to exact absolute base-URL string; selection and completeness rules are below |
| `overrides` | Provider metadata differences: optional string fields `name` and `doc`; never endpoint authority |
| `auth` | Optional authentication table described below; omission means authentication not configured |
| `subscription` | Optional user-maintained subscription information |
| `models` | Table of explicitly registered concrete models keyed by exact upstream model ID |
| `inputs` | Optional table of named provider-local input declarations; omission means empty |
| `failure_policy` | Optional provider-wide consecutive-error fixed-priority policy; omission disables it. See [Failure policy fields](#failure-policy-fields). |

Provider integration selection uses tmax's qualified registry, not execution of the
catalog's `npm` value or inference from a provider ID. The local integration
choices are discoverable independently of models.dev through the
[CLI and equivalent Web operations](../cli/spec.md#inspection-and-observation).
Their IDs and setup requirements come from the supported registry, not a guessed
catalog entry or a credential probe.

Only declared `models` entries are enabled candidates; the catalog does not
enroll the entire service. Explicit registration can add an upstream model absent
from the catalog, with local metadata as needed. Missing metadata stays unknown.
External uncertainty such as absent credentials or unverified compatibility is a
visible availability condition, not permission to fabricate metadata or erase
configuration. The integration must block unsupported use before dispatch.

A provider may be added before authentication or model registration. Omitted
`models` means an empty table. This is a normal provider declaration, not a
separate draft resource; missing access prerequisites are visible and block
affected use, not incremental setup.

### Provider endpoints

`region` and `endpoints` are provider settings, not metadata overrides.
`endpoints` accepts only the protocol keys `chat-completions`, `responses`, and
`messages`; each present value is one complete base URL. A base must be absolute
and contain no embedded credentials, query, or fragment. Authenticated traffic
requires HTTPS except for explicitly local endpoints permitted by the
[forwarding contract](../product/design.md#gateway-forwarding-contract).
Preserve an explicitly supplied base; do not add or remove `/v1`, infer another
protocol's path, or interpret a base as permission for arbitrary API operations.

For a known integration, these fields select its declared region-by-protocol map,
not arbitrary endpoint overrides. The
[endpoint resolution rules](../integrations/spec.md#endpoint-resolution) own
selector requirements, exact-match validation, singleton defaults, and
multi-region resolution. The configured `endpoints` table need not repeat the
resolved map; omitting an entry does not itself disable a mapped protocol.

For a custom integration, reject `region` and require at least one explicit
supported protocol entry in `endpoints`. All entries belong to this one provider
instance. An omitted protocol is unavailable, not inherited from catalog `api`,
another entry, or another provider. Removing the last endpoint without changing
to a valid known-integration selection is invalid.

The integration contract owns
[endpoint resolution](../integrations/spec.md#endpoint-resolution), shared native
upstream and gateway consumption, supported operations, and original-destination binding.
These fields select bases; they do not add protocol support or regional failover.
Endpoint edits follow the normal whole-snapshot save and activation boundaries.
They affect future work without repointing in-flight requests or proving that an
existing conversation can continue at a changed destination.

### Authentication

Authentication is nested in the provider; there is no independent Account or
Credential configuration collection.

| `auth.method` | Fields | Meaning |
| --- | --- | --- |
| `api-key` | Required `key_ref` | Nonempty opaque reference to a key in a current-user-restricted tmax secret file |
| `login` | Required `via`; optional `session_ref` | Sign-in session managed through the selected login source's supported flow |

| Login field or boundary | Contract |
| --- | --- |
| `auth.via` | Exact string `tmax` or an existing `agents` table key. The target ID `tmax` is reserved for the tmax login source. |
| `auth.session_ref` | Optional opaque handle scoped to `via`, never token bytes or a token-file path. Omission means sign-in is still required. |
| Native source | The declared target performs supported native sign-in, storage, and refresh; selecting it neither imports credentials nor adopts a discovered session. |
| Declarative save | Saving login authentication does not sign in or turn an API key into a session. Replacing `via` must not retain a session reference from the old source. |
| Consumer scope | Selecting a source does not grant other targets access to its native session. |

Fields belonging to the other authentication method are invalid. Qualified
sources and consumer scopes follow the
[provider-specific contracts](../integrations/spec.md#provider-specific-contracts).
The product's [login-source selection](../product/spec.md#login-source-selection)
owns runtime source lookup when `provider login` omits `--via`; the
[CLI source-selection contract](../cli/providers/authentication/spec.md#login-source-selection)
owns command operands and results.

Keys and login tokens must never appear as ordinary TOML values. A `key_ref`
resolves only during an operation that requires the secret; a dry run does not
retrieve it. Catalog `env` names do not cause automatic secret selection or
environment access. Missing secret files block the affected use. Unsupported
storage-format records are rejected and left unchanged; re-enter the provider key
through ordinary key input.

A provider may name an already declared native target as its login source while
that target has no model selections or default. Authentication setup does not
require prior native model application or completed target availability.
The qualified native sign-in flow still requires a valid, unambiguous source
scope and its supported identity and consumer boundaries.

Replacing authorization retains the named local provider even when the new
upstream account identity cannot be established. Unknown identity remains unknown,
not verified continuity with the prior account. A known different account requires
a separate provider. Retain the last established authorization comparison basis
across sign-out and removal of `auth`; removing and adding authentication must not
bypass that protection.

These rules apply to explicit operations, manual edits, reload, startup, and
changes to a native source target's Agent or scope that change authorization scope.
Invalidate credential-derived observations when authorization changes or its
continuity is uncertain. Block existing-session continuation when safe
authorization continuity cannot be established; unrelated valid routes may remain
available. The [provider lifecycle](../product/spec.md#providers-and-authentication)
and [continuity rules](../model-routing/spec.md#selection-and-continuity) own these
boundaries and supported login sources.

### Retained secret discovery

The secret inventory maintains read-only ownership and cleanup metadata
validity, including secrets left after provider, target, or gateway removal and
after a provisioning crash before reference save or result delivery. Atomically
commit each storage version 2 secret with safe inventory metadata: its opaque
reference, creation instant, and owning provider, target, or sole gateway's stable
local identity and last known display ID. Inventory metadata must be readable
without resolving or exposing secret values; it contains no secret value, token,
fingerprint, credential-derived identifier, raw source body, or reusable key
material. An interrupted creation must expose either neither item or both secret
and discoverable metadata. Unsupported-format files remain unchanged; their
inventory metadata is unavailable with a generic reason, and current cleanup
actions cannot remove them. The shared gateway key is gateway-owned, not agent-owned.

The [CLI `secret list` operation](../cli/providers/authentication/spec.md#provider-authentication-and-local-secrets)
combines this inventory with safe desired, loaded, and known native-reference
information. It does not require the owner to remain configured. Missing,
damaged, or unreadable metadata is scoped unavailable/partial inventory, never an
empty inventory or proof that cleanup is safe. Report enumerated opaque references
even when their owner metadata is unknown. An invalid/unreadable authority makes
desired-reference safety unknown; absent configuration alone cannot establish
loaded/native safety. Cleanup remains the single explicit `secret cleanup REF`
action and fails closed whenever ownership or any relevant reference condition
cannot be established. Listing never deletes secrets or changes desired state.

### Subscription information

`subscription` accepts optional `plan` and `billing_period` strings,
`renewal_date` and `expiry_date` TOML local dates, `auto_renew` boolean, and a
`charge` table with `amount` and `currency`. Amount is a nonnegative decimal
string; currency is a three-letter uppercase code. A charge requires both fields.
Omitted facts remain unknown. Billing-period text is descriptive, not a duration
expression for predicting renewal. Dates do not grant or revoke entitlement.

These values are explicit user entries. Provider-reported facts and dated quota
observations remain source-labeled runtime records; refreshing them must not
rewrite these entries. An absent subscription table means no subscription is
recorded, not a zero-priced subscription or a proven billing state.

### Provider input declarations

`[providers.<id>.inputs.<name>]` declares one input channel under that provider,
not a global account or desired-credential resource. Names are exact,
case-sensitive, nonempty strings without `/`, whitespace, or control characters.
They are immutable channel keys: renaming requires reference-safe removal and a
new declaration, not an alias or implicit identity transfer.

| Field | Type and constraint |
| --- | --- |
| `kind` | Required string `quota` or `reset` |
| `source` | Required string `manual`, `provider`, or `codex-resets`; a registry choice, never a URL, script, environment expression, or plugin |
| `scope` | Required nonempty string without control characters, identifying the quota window or reset product scope; caller-declared for manual sources, integration-advertised for fetched sources |
| `models` | Required nonempty unique array of exact registered upstream model-ID strings in this same provider; no qualified names, wildcards, catalog expansion, or implicit coverage |
| `max_age_seconds` | Required positive TOML integer |
| `refresh_interval_seconds` | Optional positive TOML integer, permitted only for nonmanual sources; absence means no automatic acquisition |

Register the covered concrete models before declaring an input. References must
resolve even for disabled models, inputs that are unavailable, and disabled
priority rules. Removing a covered model rejects unresolved input references;
changing coverage or kind must leave every dependent rule valid.
Provider removal may remove its contained inputs and rules together, but must
first resolve all external provider/model references. An explicit provider-ID
rename preserves channel identity; an explicit model-ID rename updates `models`
coverage and dependent qualified references atomically without guessing that a
different upstream ID has the same source coverage.

Manual sources support either kind. `codex-resets` requires `kind = "reset"` and
`scope = "codex"`; it is applicable only to a provider selecting the qualified
[Codex subscription integration](../integrations/codex-subscription/spec.md).
The `provider` source supports only kinds, scopes, model coverage, and normalized
fields advertised by that provider's selected integration. Actual capability
qualification is discoverable through the
[integration registry](../integrations/spec.md#rule-input-capabilities) and CLI inspection;
neither catalog metadata nor an apparent endpoint establishes it. Unknown enum
values, malformed fields, and missing local references are configuration errors.
A structurally valid declaration with no qualified matching source capability is
retained as unavailable and cannot execute a fetch, rather than being erased or
treated as verified. Provenance references are not configurable fetch addresses.

Declaration fields are desired configuration. Current values, per-channel head
and declaration revisions, provider authorization generation, and acquisition
attempts are operational state under `~/.tmax`, never TOML values. Semantic
definition or authorization changes invalidate affected facts under the
[input lifecycle](../model-routing/inputs/spec.md#input-values-and-acquisition);
a scheduling-only interval change preserves existing validity. Unrelated
configuration changes and identity-preserving local provider renames do not
discard independent facts. Missing or expired runtime data is an availability
condition, not a dangling-reference error.

`refresh_interval_seconds` opts a qualified nonmanual channel into acquisition
only while the existing local service runs. Setting it never starts a service,
reads a secret, or creates another daemon. Without a running service,
automatic acquisition is explicitly paused; explicit refresh and manual record
operations remain available without one. Scheduling, safe credentials, publication,
and late-result rejection belong to the input lifecycle, not configuration loading.

### Failure policy fields

`[providers.<id>.failure_policy]` controls a fixed replacement score, not a cap
or an edit to model base priorities. [Routing](../model-routing/spec.md#request-fallback-and-failure-priority-override)
owns counting, lifetime, and attempt behavior.

| Field | Type and constraint |
| --- | --- |
| `enabled` | Optional boolean, default true when a complete policy is explicitly configured |
| `errors` | Required failure-conditions table below |
| `consecutive_failures` | Required positive integer representable as unsigned 32-bit |
| `priority` | Required signed 64-bit replacement priority |
| `duration_seconds` | Required positive integer representable as an expiry duration |

Failure conditions are shared by `failure_policy.errors` and alias `fallback`:

| Field | Type and constraint |
| --- | --- |
| `http_statuses` | Optional unique array of integer HTTP error statuses 400–599; default empty |
| `connection_errors` | Optional boolean, default false; connection-establishment failures |
| `transport_errors` | Optional boolean, default false; ambiguous transport failures |

At least one condition must be selected. Unknown fields, duplicate statuses,
incomplete policies, zero counts/durations, and unrepresentable expiry are invalid
even for disabled policies. Runtime streaks and expiry are not configuration.


## Concrete model settings

`[providers.<id>.models."<upstream-id>"]` declares one child of its provider,
referenced elsewhere by the exact `provider-id/model-id` name. It supports:

| Field | Type and meaning |
| --- | --- |
| `enabled` | Boolean, default true for an explicitly declared model |
| `priority` | Signed TOML integer, default zero |
| `overrides` | Model metadata differences, not inference request parameters |
| `effort_map` | Optional protocol dictionary: `chat-completions` or `responses` -> caller-facing input effort -> upstream effort string; omission or empty dictionaries means identity behavior. See [Model effort maps](#model-effort-maps). |
| `timezone` | IANA zone, required when `schedule` is nonempty or any signal rule has `time_window` |
| `schedule` | Array of time-rule tables |
| `priority_rules` | Optional ordered array of named signal-rule tables; omission means empty |

Supported model override fields are strings `name`, `description`, `family`,
`knowledge`, `release_date`, and `last_updated`; booleans `attachment`, `reasoning`,
`tool_call`, `structured_output`, `temperature`, and `open_weights`; and these
structured fields:

- `limit`: nonnegative integer `context`, `input`, and `output` fields.
- `modalities`: `input` and `output` arrays of modality-name strings.
- `cost`: nonnegative finite numeric `input`, `output`, `reasoning`, `cache_read`,
  `cache_write`, and `cache_storage` fields, retaining the catalog's field units.
- `reasoning_options`: array of tables with `type` string and optional `values`
  string array, describing supported controls rather than choosing one.
- `interleaved`: boolean or a table containing a nonempty `field` string.

Unsupported metadata fields in a downloaded catalog may be retained as source
data, but are not silently accepted as supported configuration overrides. Unknown
fields in local overrides must be rejected. Identity fields such as `id` and
`canonical_model_id` are not overrides. Array replacement, recursive object
merging, explicit false/zero values, and removal-to-inherit follow
[Model catalog](../model-routing/spec.md#model-catalog). Known integration limits
still constrain what can be advertised or executed.

### Model effort maps

`[providers.<id>.models."<upstream-id>".effort_map.<protocol>]` is an explicit
single-step dictionary from caller-facing effort names to actual upstream effort
values. Only `chat-completions` and `responses` are supported; the model must
support the chosen protocol. Messages mappings are rejected. Both input names
and target values must be nonempty strings without whitespace or control
characters. Multiple inputs may name the same target; targets are not recursively
resolved through the dictionary.

Every target must appear in the model's effective accepted `reasoning_options`
entry with `type = "effort"` and explicit `values`, after local overrides.
Effective `reasoning = false`, unknown metadata, toggle-only controls, and
`reasoning = true` without declared values do not qualify a target. Save, dry-run,
`config validate`, reload/start, override edits, and catalog adoption validate the
corresponding effective declarations; an incompatible change is rejected, not
silently dropped. Reload validates targets before routing reconciliation,
autostart reconciliation, or daemon activation, including when no daemon runs.

Maps apply only inside the gateway, to concrete requests and alias candidates.
Direct native entries retain upstream capabilities and are not given a translated
map. Removing a mapping restores identity for that input; it does not disable the
input. An omitted request effort remains omitted unless an alias candidate has a
fixed effort. Precedence is fixed candidate effort, then model map, then identity,
under [Request parameters](../model-routing/spec.md#request-parameters). No map
chooses a backend, default, thinking switch, budget, or inferred equivalent level.
Manage the dictionaries with the [model effort-map commands](../cli/models/spec.md#model-effort-maps).

### Time-rule fields

Each `[[providers.<id>.models."<upstream-id>".schedule]]` requires an integer
`priority` and exactly one selector:

- `weekdays`: nonempty unique array from `mon`, `tue`, `wed`, `thu`, `fri`, `sat`, `sun`.
- `dates`: nonempty unique array of TOML local dates.

It also requires either `all_day = true`, or both `start` and `end` strings in
24-hour `HH:MM` format, with unequal values. `all_day = false` is not a shorthand
for a timed rule. Optional `effective_from` and `effective_until` are local dates;
the latter is exclusive and must be later than the former when both are present.
All rules use the model's saved `timezone`. Multiple rules express multiple
windows. Precedence, overnight spillover, clipping, overlap rejection, and DST
behavior are defined once in [Time rules](../model-routing/spec.md#time-rules).

### Priority-rule fields

Each `[[providers.<id>.models."<upstream-id>".priority_rules]]` has the following
finite fields, independently of `schedule`:

| Field | Type and constraint |
| --- | --- |
| `id` | Required immutable model-local unique nonempty string without `/`, whitespace, or control characters; disabled entries still occupy their IDs |
| `input` | Required exact name from this provider's `inputs` map |
| `priority` | Required signed TOML integer replacing the effective score |
| `enabled` | Optional boolean, default true |
| `reset_within_seconds` | Required positive TOML integer |
| `remaining_at_least_percent` | Required when the referenced input is `quota`, forbidden when it is `reset`; nonnegative decimal string in inclusive 0–100, without exponent, sign, NaN, or infinity |
| `time_window` | Optional finite calendar gate table reusing the selector/window/bounds fields below; no nested `priority` |

The rule derives its kind from the referenced input declaration; it has no
persisted `kind` field. The input's `models` must explicitly include this
model's exact upstream ID. Validate the derived kind's required and forbidden
fields and coverage even when the rule is disabled. A missing declaration or an
incompatible rule definition is invalid configuration; an unavailable current
value is not. Removing a referenced input rejects and reports the model and
rule IDs. Complete replacement retains the rule's ID and position; add appends,
and explicit move changes order. Remove and add expresses new intent, not a rename.

`time_window`, when present, contains exactly the selector, window, and optional
bounds of [Time-rule fields](#time-rule-fields): `weekdays` or `dates`,
`all_day = true` or `start`/`end`, and optional `effective_from`/`effective_until`.
It contains no `priority`; the surrounding signal rule supplies the replacement.
The model's `timezone` is required. An omitted table is ungated, not an
inferred daily window. Window interpretation uses the existing
[Time rules](../model-routing/spec.md#time-rules), not a second calendar language;
ordered signal gates may overlap because first-match order resolves them.

TOML integers follow TOML 1.0's signed 64-bit range; positive integer fields are
1–9223372036854775807. Percentage strings use one or more decimal digits with
an optional `.` followed by one or more decimal digits; compare their exact
decimal values without binary floating-point rounding or a fixed precision cap.
CLI [duration operands](../cli/spec.md#scalar-field-operands-and-defaults) convert
exactly to the persisted `reset_within_seconds`, `max_age_seconds`, and
`refresh_interval_seconds` integers; their friendly units do not change this schema.
No expression, weight, script, condition tree, or arbitrary source field is accepted.

The [priority-rule contract](../model-routing/spec.md#priority-rules) alone owns
predicates and precedence. The finite optional time gate is ANDed with the input
predicate; no matching gate means that rule cannot win. The first matching enabled
signal rule replaces priority before the unchanged date/weekly/base fallback; a
later larger score does not override it. A separate schedule cannot suppress an
ungated active signal rule: use `time_window` when the boost should apply only in
a known preferred calendar window. Rule/input changes affect new selections, not
the fixed destination of an existing session.

## Model aliases

`[[model_aliases]]` is named configuration defining one public request ID within
a native protocol and referencing already registered provider-owned Models.
Each candidate is a model reference plus alias-local settings, not an independent
Model or candidate resource with its own identity or lifecycle. The existing
`candidates` array and `candidate_reasoning_effort` map encode that configuration;
this ownership distinction adds no new syntax.

| Field | Type and constraint |
| --- | --- |
| `id` | Required exact public model-alias ID |
| `protocol` | Required `chat-completions`, `responses`, or `messages` |
| `candidates` | Required nonempty unique array of registered qualified concrete names; bare provider IDs and alias references are invalid |
| `candidate_reasoning_effort` | Optional map from exact members of `candidates` to nonempty effort strings without whitespace/control characters; omission or an empty map means no overrides. Values require the positive effective capability declaration and supported native protocol in [Request parameters](../model-routing/spec.md#request-parameters). |
| `enabled` | Optional boolean, default true |
| `continuity` | Optional `session` or `stateless`, default `session`; shared by every caller. Stateless requests still require qualified independent request context. |
| `fallback` | Optional failure-conditions table; omission disables request fallback. Requires explicit `continuity = "stateless"`. |

Duplicate `(protocol, id)` pairs and collisions with registered concrete public
IDs in the same protocol scope are invalid, including disabled entries. Missing,
empty, duplicate, or unregistered candidates are invalid configuration. Disabled
or currently unavailable registered candidates remain references, not automatic
replacements. Selection, priority ownership, continuity evidence, and activation
blockers follow [Model aliases](../model-routing/spec.md#model-aliases).
Removing or replacing candidate membership removes overrides for departed
candidates. Renaming a provider or concrete model carries its map keys to the new
qualified name; renaming an alias preserves its candidate settings. Setting or
clearing one candidate's effort preserves other candidates' entries. Explicit
duplicate/conflicting edits and clearing an absent entry are errors. Unknown map
members or unsupported values/protocols are invalid choices, not passthrough.


Alias definitions contain no upstream credentials, copied provider metadata,
per-alias priorities, alias-wide parameter defaults, or arbitrary request
parameters. Only explicit candidate reasoning effort has the narrow behavior in
[Request parameters](../model-routing/spec.md#request-parameters). A new provider
instance is not implicitly added to an explicit candidate list.

## Agent targets

`[agents.<id>]` is the independently managed AgentTarget native scope and its
configuration, not an inference client registration or gateway permission boundary:

| Field | Type and meaning |
| --- | --- |
| `agent` | Required string `codex`, `pi`, `omp`, `opencode`, `hermes`, or `dsh`; the coding tool identity, not this target's ID |
| `scope` | Required path to the native configuration directory/scope recognized by the selected Agent adapter; CLI `--path` writes this field |
| `providers` | Table of agent-local native provider entries, default empty |
| `default` | Optional `{ provider, id, protocol }` reference to a selected model in the named native provider entry |
| `roles` | Optional table mapping supported native model-role names to `{ provider, id, protocol }` references to selected models in the named native entries |

`provider` in a default or role reference is the native entry name, not a tmax
upstream provider ID. Every present reference must match a selected `(id,
protocol)` pair in that entry. These references contain no continuity override.
Protocol selection is explicit; it does not establish compatibility or conversion.
A scope must resolve unambiguously before application; tmax must not write into
an arbitrary directory on the assumption that an agent lives there.

For `dsh`, the scope is the existing DSH home directory itself. Discovery uses
nonblank `DSH_HOME`, otherwise the OS home directory's `.dsh`; an explicit path
selects that home directly, not a profile or an inferred parent. tmax edits the
home `cordis.patch.yml` and adjacent `.credentials.yaml` without enumerating,
selecting, creating, or managing profiles or inspecting bundles. The common
home entry `config` replaces the whole lower-layer entry configuration; profile
routes and defaults are not automatically copied or deep-merged. Existing
HOME-level fields and expressions remain preserved under the
[DSH home boundary](../integrations/spec.md#dsh-home-boundary).

A target add operation saves an empty `providers` table and omits `default`.
Later native-provider, model-selection, and default operations complete the same
target, not a draft, wizard, or separate setup resource. A target with no selected
models or no default is valid but incomplete for native model application.
It may still serve as a qualified native login source, and it does not determine independent
gateway readiness. Credentials, scope resolution, native precedence, and compatibility
must be ready for the entries being applied. Unsupported multi-provider
configuration or native name conflicts block application; never drop an entry,
merge unrelated sources, or silently choose one.

`roles` only maps supported native model roles. It cannot define access roles,
sampling values, reasoning settings, or agent-specific precedence overrides.
Native configuration remains the owner of native request preferences; gateway
model maps and fixed candidate effort do not rewrite those settings. Metadata
`temperature` and `reasoning_options` describe capability, not values to send. See
[Request parameters](../model-routing/spec.md#request-parameters).

### Native provider entries

`[agents.<target>.providers.<name>]` names one provider in that target's native
configuration. Names are exact, nonempty, agent-local names subject to the native
adapter's namespace constraints, not additional tmax upstream access instances.

| Field | Type and meaning |
| --- | --- |
| `provider` | Exact configured tmax upstream provider ID; mutually exclusive with `gateway` |
| `gateway` | Literal `true`, selecting the sole configured model gateway; `false` is invalid |
| `models` | Array of `{ id, protocol }` selections, default `[]`; repeated pairs are invalid |

Exactly one source selector is required. It has no fallback. For a `provider`
source, `id` is the exact registered upstream model ID under that provider,
without its tmax provider prefix. For a `gateway` source, `id` is a public concrete
qualified ID or a configured alias ID, without a native display prefix.
Each selection requires an explicit protocol from the model-alias protocol
set above; it contains no continuity setting. Disabled or unavailable models
remain resolvable references but do not establish usable native configuration.

Upstream and gateway entries may coexist in one target. Native upstream entries
use their selected provider's resolved endpoints and qualified authentication.
Gateway entries use the local gateway URL and current shared key; application
requires the matching ready listener and loaded model definitions. The configured
native subset controls model pickers and native settings only. It never limits
gateway discovery or inference, and no default or selected model is required on
an Agent for an independently configured compatible client to use the gateway.

Source replacement validates every existing selected ID and protocol against
the new source and reports affected defaults, roles, and native settings.
Reject unresolved resulting references; do not reinterpret, rewrite, or clear
them implicitly. An already valid selection need not be emptied first.
Removing an entry requires clearing its default/role references first.
Removal of an entry or target leaves native files unchanged until explicit apply
or restore. Native name/selection changes and target/entry removal never revoke
the shared gateway key, stop the independent gateway, or reset its conversation
bindings. Target removal must resolve provider login-source references; separate
upstream login-source scope or authorization changes retain their continuity checks.
Saving any of these settings neither writes native configuration nor starts
the gateway. Native credential handoff, protected plaintext disclosure, partial
writes, and restoration follow
[Agent provider configuration](../product/spec.md#agent-provider-configuration).

## Model gateway

The optional `[gateway]` table configures the sole model gateway independently of
Agents. It is required by any native provider entry selecting `gateway = true`,
but is also valid with no Agent targets:

| Field | Type and meaning |
| --- | --- |
| `listen` | Required numeric loopback IP and port in `host:port` form, with bracketed IPv6; port 1–65535 |
| `key_ref` | Required provisioned opaque reference to the current protected local gateway access key, not an upstream credential |
| `autostart` | Optional boolean, default false |

Non-loopback addresses are invalid. The public model list is derived directly
from enabled provider-owned models and enabled alias definitions, within their
declared supported protocols. Missing availability or compatibility evidence
remains visible under the model-support and eligibility rules. There is no
`gateway.models` table, separate publication resource, client registry, or private
allowlist. Every holder of the current shared key uses the same published models;
Agent native selections do not restrict this access. Catalog discovery never
enrolls models automatically.

Initial explicit `gateway configure --listen ADDRESS` generates the shared key
once in a current-user-restricted plaintext storage-version-2 file under `~/.tmax`
and saves its reference. Validate the declaration and current inputs first. Report
secret creation, file save, and service activation as separate boundaries; failed
save after creation leaves a discoverable unreferenced secret for explicit cleanup.
A dry run neither generates a key nor chooses or reserves a reference. Normal
configure updates, reload, start, restart, and native application never provision
A missing or unsupported-format key blocks readiness.

`gateway key replace` creates and saves the replacement reference and activates
the new shared key. Subsequent requests must reject the old key; in-flight
requests retain their admission snapshot. Replacement never resets, releases,
or re-scopes retained conversation bindings, their blocked state, or destinations.
Report known managed native dependencies that must update, and explicitly state
that unmanaged clients are not enumerable and must update too. Replacing or
removing the reference does not delete its
stored secret. `gateway remove` requires resolved configuration references and a
stopped listener, then removes gateway settings and managed access, not retained
secrets or native files. Startup registration follows the existing
changed-intent reconciliation rules.

Saving gateway settings does not start a listener. Explicit sensitive key
retrieval is separate from ordinary inspection under the
[secret-protection contract](../product/spec.md#local-access-and-secret-protection).
Listener startup, key handoff, and opt-in login startup follow the product's
[model-gateway contract](../product/spec.md#model-gateway). The management Web
interface has no authentication; the shared key remains required for inference.

## Outbound network settings

The optional `[network]` table is the sole persisted outbound policy.
Omitting it is the canonical direct representation; an explicit valid
`mode = "direct"` table is also accepted in a manually edited file.

| Field | Type and constraint |
| --- | --- |
| `mode` | Optional string `direct` or `proxy`, default `direct` |
| `proxy_url` | Absolute HTTP, HTTPS, or SOCKS5 proxy URL; required for `proxy` and invalid for `direct` |

`proxy_url` accepts the schemes `http`, `https`, and `socks5`. It must contain a
host and a valid port when supplied. An omitted port means 80 for HTTP, 443 for
HTTPS, or 1080 for SOCKS5. Its path is empty or `/`; userinfo, query, and fragment
are invalid. SOCKS5 resolves the destination hostname at the proxy. There are no
proxy-authentication credential settings, per-provider network settings, bypass
lists, environment selectors, or TLS-verification bypass settings.
The [product outbound network policy](../product/spec.md#outbound-network-policy)
owns traffic coverage, explicit routing, and failure behavior.

`network proxy URL` saves `mode = "proxy"` with `proxy_url`; `network direct`
removes the optional table. The [CLI network contract](../cli/service/spec.md#outbound-network-configuration)
owns operands and read syntax. These operations use ordinary revision guards,
validation, whole-snapshot activation, and dry-run rules. They do not start
inference, apply native configuration, or test the proxy. Inspect saved and loaded
policy independently; a saved policy is not evidence of an active or reachable transport.

Independent CLI outbound work captures the policy from its validated current
saved snapshot; an absent authority uses the documented direct default. Daemon
outbound work captures the loaded snapshot's policy. Invalid or unreadable saved
policy blocks work that needs it rather than silently selecting direct access.
Successful activation changes the policy for future daemon work. A save
without a daemon reports saved but not loaded. A failed activation retains the
previous loaded policy and uses explicit reload for retry. Already started work
retains its captured policy; settings never repoint an in-flight connection.
Native agents retain ownership of their own networking.

## Validation and identity

Validation must distinguish syntax/type/reference errors from incomplete setup
and external availability: a missing local reference is invalid; absent target
selections/default, missing or unsupported-format keys, stale catalog, or unqualified
agent combination is reported with its actual scope and cannot be presented as
verified access. Dry runs do not perform inference or upstream verification.

Saved references must resolve against explicit provider/model registrations and
alias definitions. Renaming through management operations updates dependent
references as one configuration change and preserves stable internal identity.
A raw file edit that removes one ID and adds another is removal/addition unless
an explicit rename operation supplied identity continuity; names or credentials
must not be used to guess a rename. Lost session identity blocks continuation
instead of silently assigning the newly named destination.

Deletion follows the [product lifecycle](../product/spec.md#providers-and-authentication):
unresolved references block removal, without automatic cascading or retargeting.
This includes provider `auth.via` references to Agent targets, input model
coverage, and priority-rule input references. Removing an input value through
`clear` does not remove its declaration or satisfy a configuration-reference check.

## Save, reload, and apply

There are three observable states, not three independent configuration sources:

| State | Meaning |
| --- | --- |
| File configuration | Desired settings in `~/.tmax/tmax.toml` |
| Loaded configuration | Validated snapshot currently used by the service, including accepted metadata |
| Applied agent configuration | tmax-owned native changes actually written, possibly awaiting agent reload |

Every CLI/Web edit validates and saves its explicit change against the current
file, preserving unrelated contents. With a running service it then activates the
WHOLE resulting valid snapshot, including previously pending manual edits, for
new selections. There is no hidden pending overlay or per-field active source.
Dry runs expose the complete loaded-to-candidate impact, including authorization,
endpoints/network policy, rules/inputs, targets, bound-session blockers, listener
changes, and startup intent already pending in the file. Executed results report
actual save, loaded, and native-application outcomes, consequential changes beyond
the named edit, and any blocked, partial, or unknown effects. Routine success need
not repeat the full impact or diff; the [CLI impact contract](../cli/configuration/spec.md#configuration-impact-data)
owns result detail. In-flight requests and established destinations follow the
continuity contract. Saving target settings does not
apply native changes. Without a running service, report saved but not loaded;
do not start the gateway implicitly. An incomplete target remains explicitly
unavailable for native application without invalidating independent gateway use.

Manual file edits remain pending until explicit reload OR a subsequent executed
management edit validates and activates the complete current file. There is no
automatic file watcher. Reload changes no file contents; it activates the same
whole-file candidate and exposes the same complete impact without rewriting
native files or performing paid checks. Invalid edits leave the loaded snapshot
unchanged and visibly report the file as not active. Cold start validates the
current file and fails on invalid configuration rather than falling back to a
hidden earlier file revision. A retained catalog snapshot is not a fallback for
invalid configuration. Reload is not a path selector or document-input operation.

Autostart registration is reconciled only when a save or explicit reload changes
the desired `gateway.autostart` intent, or when the caller explicitly invokes
autostart enable/disable. Missing or false is disabled intent. Keep the last
acknowledged intent for tmax's sole authority as operational state under
`~/.tmax`, independently of registration success and loaded service state, so
manual edits can be recognized without a running service. With no acknowledged
intent, true requests registration; disabled intent does not request cleanup.
Execution acknowledges a validated, persisted intent even if the requested
registration fails; a failed save must not acknowledge unsaved intent.

Report requested registration separately from file save and service activation.
A requested registration failure makes that operation incomplete. Subsequent
save/reload with unchanged intent must not retry registration or fail solely
because registration drift remains. Show that drift and the explicit
enable/disable action that retries reconciliation, including when the saved
boolean already has the requested value. A dry run may report required
reconciliation from current intent and registration state but neither performs
it nor reserves that state for a later call.

The OS registration belongs to tmax's sole authority. Remove or replace only
tmax's owned registration; never overwrite an unrelated startup entry.
Reconciliation requires no daemon service and never starts the gateway. A startup
invocation must validate the current file and its enabled autostart setting before
starting inference, so an obsolete registration cannot override a saved disable.

File persistence and service activation are separate boundaries:

| Operation and runtime outcome | Reported result |
| --- | --- |
| Ordinary desired-state edit; validation, persistence and required safety/identity effects complete | Successful saved change, independently of background runtime readiness. Attempt whole-file activation in a compatible running service, including a management-only service with no loaded snapshot. |
| Background activation fails or requires restart | Keep the saved fact/revision, actual failure/warning and separately observed loaded-before revision. An unconfirmed loaded-after revision remains unknown; a concurrent writer may have activated another snapshot. Do not claim activation success or ask the caller to repeat a committed create. |
| Daemon discovery/loaded state cannot be established | Report runtime/activation unavailable and its reason, not stopped, absent, active, or an empty complete session-impact set. |
| Explicit reload/lifecycle/revocation/native/startup work or required safety/identity effect fails | Nonzero blocked/incomplete result with every completed save/credential/native effect retained. Never mask a requested failure as successful activation. |

Listener-address changes require explicit restart; report known managed
dependencies and unknown unmanaged-client scope rather than hot-moving a listener.
No activation or restart may replace an established conversation destination.
Do not silently rewrite a saved file or apply only its valid subset. Explicit
reload recomputes from current saved configuration, never replays an earlier
management edit. The [CLI outcomes](../cli/results/spec.md#outcomes-and-exit-categories)
own exit categories; the [CLI impact contract](../cli/configuration/spec.md#configuration-impact-data)
owns saved/loaded/unavailable projections.

Native agent application is a separate explicit operation against current
desired settings, accepted metadata, native inputs, and credential scope.
It requires selected models and a provider-qualified default, usable credentials,
and qualified compatibility for every applied native entry. It writes only
owned changes, preserves unrelated native settings and precedence, reports partial
failure and current-user-restricted plaintext recovery state under `~/.tmax`, and
distinguishes applied from awaiting native reload. Gateway-source entries additionally
require the matching service snapshot, ready listener, and current shared key
availability.
Upstream-source entries use their provider's endpoint/authentication without a
tmax inference dependency. File edits and catalog application never rewrite
agents in the background.

An optional `--dry-run` previews the same operation using current nonsecret local
inputs. It makes no changes, creates no persistent plan or reservation, does not
read secrets or sign in, and makes no upstream calls. Where execution depends on
protected or upstream information, report that unresolved prerequisite rather
than pretending it was checked. A dry run is neither approval nor a prerequisite
for execution and does not bind a later call or extend a native login session.

Actual execution computes its change from inputs current to that invocation.
Check the authoritative file content revision and, where relevant, accepted
metadata, native inputs, credential references or safe identity revisions, and
registration state before effects that depend on them. Changes during the
operation must be rejected before overwriting newer configuration or native
content; there is no force-through token. If an earlier side effect already
completed, report its exact boundary and applicable recovery state rather than
claiming rollback or success. A subsequent call recomputes from current state.
The [CLI dry-run and execution contract](../cli/spec.md#dry-run-and-execution)
owns command/result behavior; Web operations enforce the same current-input and
concurrency rules, not a separate approval or persisted-plan system.

## Example scope

[example.toml](example.toml) illustrates persistence for two OpenCode Go instances
and official DeepSeek with explicit known-region and known-base selections,
one custom provider with two explicit protocol bases, a global outbound proxy
policy, metadata overrides, optional subscription labels, six concrete models,
arbitrarily named `L0`–`L3` aliases with explicit candidates, a three-source scenario alias with shared continuity,
scheduled priority, provider-local manual quota/reset declarations and ordered
signal rules, and one omp target containing both upstream and gateway native
provider entries. It also shows a provider using its published singleton default
without authentication/models. The gateway/provider/alias settings are valid
without the Agent target; target incompleteness is defined above rather than
required by gateway setup. Manual observed values stay outside TOML and use
[explicit CLI record operations](../cli/providers/inputs/spec.md#provider-input-operations).
Its scopes, candidate lists, and priorities are illustrative caller choices, not current
quota, reset, price, or capability claims. Native scope paths and secret references
require actual local provisioning and qualification.

Users and calling agents do not copy, complete, or submit this file to set up
tmax; explicit resource operations create and update the sole authority.
Parsing the example establishes syntax only. Source model registration,
reference resolution, schedule validation, credential availability, native
compatibility, configuration application, and real inference are distinct checks.
