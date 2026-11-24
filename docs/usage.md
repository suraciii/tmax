# CLI usage: providers, routing, and native agents

This guide uses supported command grammar, but credentials and upstream calls are
user-specific. CLI help is offline (`tmax help provider input`, for example); it
never creates a starter configuration. The default `--format json` prints one
compact JSON line intended for scripts; `--format text` prints the same result
envelope as indented JSON.

Ordinary provider/model/agent/network edits save desired settings without requiring
a running service. `provider inspect`, file-state reads, and configuration
`--dry-run` previews also work without credentials or network. A stopped service
has no loaded snapshot; an unidentified or inaccessible service is **unavailable**,
not proof that nothing is running or that no sessions are affected.

A provider is an independent upstream access instance; multiple providers can use
the same service or account without proving separate allowances. Each concrete
model belongs to one provider and is referenced as `provider-id/model-id`. An
agent target is an independently managed native scope. A model alias is one named
route that declares native protocols and references existing models, with optional
alias-local reasoning effort per candidate; candidates have no independent
identity or lifecycle.
Account identity and subscription information are optional facts, authentication
is provider configuration, and conversation bindings are runtime state. An
integration selects qualified behavior, not a universal service identity.
See the [domain vocabulary](../CONTEXT.md) and
[ownership contract](../specs/product/design.md#product-structure).

After an ordinary edit has saved successfully and completed required safety work,
exit 0 means the desired change is committed. Background activation may still
have a failed/unavailable warning. Check `save`, `file_revision`, `runtime`,
`activation`, and loaded revision fields; do not repeat an already committed
`provider add` or another create. Inspect `tmax status`, then use explicit
`tmax config reload` after resolving its blocker, or `tmax gateway restart` for a
saved listener-address change. Loaded state is not claimed to change until
activation actually succeeds. Catalog acceptance follows the same distinction.

Explicit reload/start/restart, shared-key revocation, native authorization or
application, requested autostart, and mandatory safety work retain nonzero failure
exits. If a save completed first, their incomplete result still identifies that
saved revision and the actual failed/partial boundary. Validation, persistence,
and revision conflicts are never converted into successful saves.

## Simplified configuration syntax

Use `--set metadata VALUE` and `--set integration VALUE` for provider scalar
selections; the corresponding standalone flags and the `region` field are removed.
`provider auth set ID --via TARGET` implies login authentication and takes no
`--method`. Interactive login operations keep their own browser/device choice.

In manually maintained configuration, remove `network.mode`: a network table
contains only its required `proxy_url`, and direct connections omit the table.
Remove `all_day = true` from schedule entries and signal-rule time gates;
omitting both times means the whole selected local day. Timed windows require
both `start` and `end`; equal or half-specified times remain invalid.

Known integrations select fixed destinations, never explicit `endpoints` or a
`region`. Kimi uses `kimi-code-plan-cn` or `kimi-code-plan-global`; MiniMax uses
`minimax-cn-coding-plan` or `minimax-coding-plan`. These IDs match models.dev, while
the qualified protocol and quota paths remain owned by tmax's integration
contracts. In particular, this naming change does not replace MiniMax's existing
`api.minimaxi.com` China destination with a catalog URL.

Before upgrading a manually maintained file, replace an old integration/region
pair using the [cutover table](../specs/configuration/spec.md#historical-regional-integration-cutover),
remove `region`, and clear any known-integration endpoint selectors. Other known
integrations no longer need `region = "default"`. Custom integrations still need
explicit protocol bases. Validate the resulting file before reload. Removed
desired syntax is rejected, not silently interpreted. Local instance IDs are
unchanged and do not select a region.

```sh
tmax provider add work --set integration kimi-code-plan-cn
tmax provider add travel --set integration kimi-code-plan-global
```

`work` and `travel` are arbitrary local names; the integration selects the product
and destinations, without repeating a region field.

Historical routing snapshots are normalized separately without erasing stored
model identities, authorization generations, or observations.

## Configuration format version 2

The authority is `version = 2`. tmax rejects a version-1 file with
"Configuration format version 1 is unsupported; run `tmax config migrate`"; it
does not accept both formats and never migrates the file automatically. A model
alias is one route record per name that declares `protocols` and keeps one
ordered `candidates` list for every declared protocol.

`tmax config migrate --dry-run` reports the plan and every refusal without
writing. Run `tmax config migrate` to convert the file; apply is the default when
`--dry-run` is absent, and there is no `--apply` flag. The command:

1. Merges version-1 records that share an alias `id` into one record only when
   their `candidates`, `enabled`, `continuity`, `fallback`, and
   `candidate_reasoning_effort` are identical.
2. Rewrites every version-1 native provider entry that uses `gateway = true` or
   its historical `proxy = true` spelling to `provider = "tmax"` with the same
   models. A relayed entry selects public route names only; an entry that selects
   a concrete qualified model ID is refused, so define a route for that model or
   drop the selection. A version-2 entry that still contains either field is
   rejected as an unknown field.
3. Validates the complete converted document, stores the exact pre-migration
   bytes in the recovery area, then saves atomically.

Refused records are named and left untouched; the command exits nonzero and
writes nothing. On success the result returns an opaque recovery handle for the
pre-migration bytes. Review or discard it with
`tmax config recovery list`, `tmax config recovery inspect RECORD`, and
`tmax config recovery discard RECORD`. Those records retain the exact bytes for
manual recovery only: there is no restore operation, so restoring a pre-migration
copy means copying the retained file back by hand. A version-2 file is refused as
`invalid_arguments` (exit 2) because the configuration already uses the current
format; migration is not needed and nothing is written.

Migration does not start the gateway and does not rewrite native files. Start the
gateway and re-apply each affected target:

```sh
tmax config migrate --dry-run
tmax config migrate
tmax gateway start
tmax agent apply TARGET
```

## Add one provider and model

Choose a supported integration with `tmax integration list` and
`tmax integration inspect INTEGRATION_ID`. For a custom gateway, supply an exact
protocol base URL instead; this example uses a reserved placeholder URL and must
be replaced with your own endpoint:

```sh
PROVIDER_ID=my-gateway
MODEL_ID=exact-upstream-model-id
BASE_URL=https://gateway.example/v1

tmax provider add "$PROVIDER_ID" --set integration custom \
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

### Explicit gateway effort maps

After inspection establishes the actual supported upstream effort values, use
the [model effort-map commands](../specs/cli/models/spec.md#model-effort-maps) to
set, remove, or clear a protocol's input-name dictionary. The
[configuration example](../specs/configuration/example.toml) shows a synthetic
`max` -> `high` map with an explicit target declaration, not hosted qualification.
Targets must be positively declared in effective accepted metadata; do not guess
support from a model name or a reasoning boolean. Removing an entry restores
identity, not input disabling. Maps supply no omitted effort, native defaults,
thinking switches, or budgets, and Messages maps are rejected.

These maps run only in the gateway. Direct native entries keep actual upstream
capabilities. For aliases, fixed candidate effort takes precedence over a model
map, then unmapped inputs use identity. Alias selectors expose the intersection
of candidate input sets after these settings, not a winner's raw effort names.
Changing only effort never silently selects another backend: the gateway selects
using other constraints, then validates effort on that backend or rejects before
publishing a binding or dispatching. Explicitly reapply affected native entries
and reload the tool after changing advertised inputs.

After manual edits, `tmax config validate` and `tmax config reload --dry-run`
validate map targets against accepted effective declarations. An invalid target
also blocks executed reload before routing, autostart, or activation effects.

Local verification on 2026-10-27 used the release binary with an isolated OS-home
store and synthetic loopback upstreams. With higher-priority A declaring
`low`/`high` and B declaring `high`/`max`, unmapped `max` rejected on A without
upstream dispatch; an explicit A `max` -> `high` map forwarded to A for both Chat
Completions and Responses. The run also exercised omitted effort, fixed override
precedence, protocol isolation, removal/clear, and real Pi file application:
gateway alias inputs included the mapped common names while direct entries
retained upstream-only names. No hosted inference, provider entitlement, or
running native-agent consumption was established by this verification.

### Common alias input capabilities

Gateway aliases expose only input modalities declared by every configured
candidate, including disabled or temporarily unavailable candidates. Inspect
with `tmax model inspect ALIAS --protocol chat-completions`: `capabilities.input`
shows the common list and each candidate's effective declaration. A missing
declaration is unknown, not false or text-only; correct the concrete model's
metadata through existing model operations rather than invent an alias override.

When both candidates declare text and image, native application includes both.
Adding a text-only candidate removes common image support. Explicitly reapply
affected Agent targets and reload the tool after changing membership or effective
metadata; saving tmax settings does not rewrite native files. Reapplication
withdraws obsolete managed input declarations. DSH cannot represent a known empty
common input set and blocks before writing; an unknown declaration leaves native
fallback visible rather than preserving a stale tmax image claim. Capability
metadata and local parsing do not prove that a hosted upstream accepts images.

Local verification on 2026-10-27 used the release binary with an isolated OS-home
store, a local gateway, and a real DeepSeek Harness 0.2.1-rc.2 profile. Two
candidates declaring text and image produced the common list; clearing one
declaration made the intersection unknown and DSH's managed `input` row was
withdrawn while native fallback stayed visible; restoring text and image
reprojected both. A known empty intersection blocked preview and execution before
any native write, leaving the patch and credential files byte-identical. No
hosted inference or provider entitlement was exercised.

### Existing configuration with removed tier fields

The current `~/.tmax/tmax.toml` authority strictly rejects the former per-model
and model-alias `tier` fields; tmax does not automatically convert or rewrite
that authority. A file with `tier` fields predates format version 2. If you
choose to update it manually, remove each per-model `tier` field and replace each
alias's `tier` field with a nonempty `candidates` list of exact, already
registered `provider-id/model-id` names that you choose. Keep the version-1 alias
ID and `protocol` field; `tmax config migrate` merges those records into version 2
afterwards.
Names such as `L0`, `L1`, and `coding` have no built-in meaning or membership.
Keep model priorities, time schedules, and ordered rules unchanged. Distinct
native/vendor service tiers, pricing/context tiers, and subscription-plan metadata
are not removed fields.

Do not edit or delete routing-state or session JSON for this conversion. tmax
normalizes supported historical operational records internally without read-time
writes; corrupt records fail closed. This does not convert the desired authority,
clear retained sessions, or remove permanent continuation blockers.

## DeepSeek model discovery

For an official DeepSeek provider whose key is already configured, explicitly
refresh its model list without running inference:

```sh
tmax provider refresh personal --scope models
tmax provider inspect personal
```

This authenticated operation reads `https://api.deepseek.com/models` and retains
exact upstream IDs as an observation. It does not register models, download or
apply catalog metadata, verify inference, or incur a tmax paid probe. Register
only the desired exact IDs separately with `model add`. A failed refresh retains
the last good list separately from its failed attempt. Auth and quota refreshes
use the separate official balance endpoint; neither discovery nor balance
establishes the account owner's identity.

## OpenCode Go observations

Configure each independently selected account under its own provider ID, supply
its key through explicit protected input (such as `--key-stdin`), and refresh
the desired scope:

```sh
tmax provider add go-a --set integration opencode-go
tmax provider key set go-a
tmax provider refresh go-a --scope quota
tmax provider inspect go-a
```

Repeat with a distinct provider ID and its own key for another account. Inspection
does not contact the provider. Go usage observations contain consumed percentages
and reset announcements only; they do not report exact remaining allowance,
workspace identity, entitlement, or a renewal date. Their source freshness is
unqualified: local retrieval time is not a provider measurement time, and tmax
does not infer staleness from age. These observations do not create priority-rule
inputs or combine accounts' allowance. Renewal remains unknown unless explicitly
known subscription information supplies it; a monthly plan alone is not a date.
See the owning
[Go quota contract](../specs/integrations/opencode-go/spec.md#quota-and-renewal)
for the qualified current response boundary and source evidence.

An unavailable or failed requested refresh returns exit 4 and retains the latest
attempt separately from any prior applicable fact. It does not establish revoked
authorization or exhausted quota. Inference still needs its own qualified model,
protocol, client identity, and conversation session; a successful usage read does
not prove that path.

## Plan quota observations

The plan integrations `kimi-code-plan-cn`, `kimi-code-plan-global`,
`glm-coding-plan`, `stepfun-token-plan`, `minimax-cn-coding-plan`, and
`minimax-coding-plan` each own one bounded read-only quota GET beside their
inference endpoints. Select the exact integration, then refresh the quota scope:

```sh
tmax provider add glm-a --set integration glm-coding-plan
tmax provider refresh glm-a --scope quota
tmax provider inspect glm-a
```

Observations are display-only. They publish raw provider-reported counters,
percents, and reset values with their unit or encoding left unqualified; they
never create priority-rule inputs, and one successful HTTP read does not prove
plan entitlement or account identity. Unknown quota stays unknown, a failed
refresh retains the last dated fact instead of clearing it, and retrieval time
is not a provider measurement time. Each integration keeps its own boundary in
its contract: [Kimi](../specs/integrations/kimi-coding-plan/spec.md#membership-quota-and-renewal),
[GLM](../specs/integrations/glm-coding-plan/spec.md#allowance-and-monitor-evidence),
[StepFun](../specs/integrations/stepfun-token-plan/spec.md#credit-and-quota-evidence),
and [MiniMax](../specs/integrations/minimax-m-plan/spec.md#quota-observation).
Providers on `custom` integration, such as a console-only plan, have no
qualified quota source; record manual subscription information instead of
scraping browser sessions.

Kimi quota reads preserve partial information: a missing or unusable counter,
reset timestamp, or window description does not discard other readable counters
or windows. Missing values remain unknown and are not filled from an older
observation or calculated as zero. A response with no readable allowance counters
still reports a failed refresh and retains the previous dated observation. See
the [Kimi tolerant-read contract](../specs/integrations/kimi-coding-plan/spec.md#tolerant-quota-reads).

CLI readers get the same report. `provider inspect ID` exposes
`data.observations.billing.display` with `rows` and qualification `notes`, and
`provider list` repeats it per quota entry. Each row names its window or bucket,
whether the percentage is consumed or remaining, exact counters when the source
reports them, and reset evidence. An unqualified epoch-millisecond reset stays a
raw value with an optional labeled local conversion; it is not a confirmed reset
or renewal. A failed refresh keeps the previous dated fact beside the failure,
and an integration without a quota operation says so instead of offering an
impossible query.

## Local Overview and evidence

The Web Overview groups the provider and agent evidence already available through
local read operations. It does not refresh providers, resolve secrets, sign in,
start the gateway, or apply native files. An explicitly requested refresh or native
change remains a separate task.

Treat saved configuration, loaded state, applied files, and effective native
runtime as separate facts. A configured authentication method is not verified
authentication; an applied file is not proof that a running agent uses it. Missing,
unavailable, stale under an explicitly declared rule, and freshness-unqualified
observations are different states. Do not infer validity from retrieval age or
turn unknown quota into zero. Keep consumed-only observations provider-local; do
not pool their percentages or allowances. Show a renewal date only when known,
and keep a past date distinct from an unknown date.

### Web language

The redesigned workspace uses Simplified Chinese labels, including 网关 for
Gateway. See the [Web workspace guide](web.md) for its task areas and operation
feedback. Stable IDs, CLI commands, configuration names, and raw results retain
their documented spelling. The CLI remains English.

## Explaining models and applying metadata

Use `tmax model explain ID --protocol PROTOCOL` for an effect-free explanation of
a public qualified concrete or alias ID. Add `--target TARGET` to assess it for a
declared native target, optionally with `--agent-provider ENTRY` for an existing
entry. For an undeclared native scope, use `--agent AGENT --path PATH` instead;
this does not register or import the scope. Do not mix the two context forms.
With a direct entry, use its exact unqualified upstream model ID;
without an entry, concrete IDs must be qualified. No saved native model, default,
or role selection is required. For example:

```sh
tmax model explain L0 --protocol chat-completions --target pi-main
tmax model explain L0 --protocol chat-completions \
  --agent omp --path "$HOME/.omp/agent"
tmax model explain PUBLIC_MODEL --protocol responses --state loaded
```

These examples assume the alias `L0` and target `pi-main` already exist. The alias
uses only its configured candidates; its name supplies no candidate selection.
Explanation data identifies `gateway`, `agent_target`, `agent_provider`, or
`native_scope` context and reports `requested` as exactly `{id, protocol}`.

`--state loaded` explains the daemon loaded view, not pending saved edits. Model
explanation does not retrieve secrets, sign in, run inference, reserve a session,
or apply native files. It assesses only qualified local evidence; a readable tool
or configured login does not verify native authentication, entitlement, or access.

For an existing gateway request, use the opaque `scope_handle` returned in retained
request decision evidence:

```sh
tmax model explain PUBLIC_MODEL --protocol PROTOCOL --session HANDLE
```

The command reports the retained destination or blocker without fresh ranking;
it does not reserve a session, migrate a conversation, or unblock continuation.
The handle must match the public model ID and protocol. Session mode rejects a direct entry.

Catalog adoption is also explicit. `catalog apply --revision REVISION` applies a
validated downloaded snapshot; its preview/result compares accepted and candidate
effective metadata after overrides and reports affected concrete selections,
aliases, and native projections. Review `reapply_required`, blocked
unrepresentable facts, and any gateway-route revalidation even when native files are
unchanged. The result distinguishes accepted-before, candidate, and accepted-after
revisions; a preview does not advance accepted metadata. Activation reports the
actual service outcome. Catalog apply does not implicitly apply native files or
change endpoint, enablement, or destination settings.

## Request fallback and temporary provider priority

Fallback is optional on each stateless alias definition. These examples
assume `gpt-6.1-terra` already references `cpa-us2/gpt-6.1-terra` and
`llm-us/gpt-6.1-terra`, with CPA priority 2 and LLM priority 1:

```sh
tmax model alias update gpt-6.1-terra \
  --set continuity stateless --set fallback.http_statuses 429
tmax provider update cpa-us2 \
  --set failure_policy.errors.http_statuses 429 \
  --set failure_policy.consecutive_failures 3 \
  --set failure_policy.priority 0 \
  --set failure_policy.duration_seconds 18000
tmax provider inspect cpa-us2
tmax model explain gpt-6.1-terra --protocol responses --state loaded
```

The first two CPA 429 responses fall back to another eligible provider. The third
also triggers a provider-wide replacement priority of 0 for five hours; later
new selections prefer LLM. Fallback belongs to the route definition and applies
to every declared protocol. The provider streak is shared across its models, not
copied per alias.
No quota API or asserted remaining balance is needed; 429 is a selected failure
condition, not proof of exhausted allowance. Configuring a policy does not start
the gateway or change native agent files.

During the fixed interval, successes do not end the override and further failures
do not extend it. Expiry restores normal rule evaluation and clears the old
streak. Restart and history clear preserve it.
Policy replacement and explicit reset fence outcomes from older admitted
requests, including after restoring the original policy. An unrelated reload or
same-policy restart preserves the state; pending saved changes do not reset a
still-loaded policy.

To explicitly end it early:

```sh
tmax provider failure-reset cpa-us2 --dry-run
tmax provider failure-reset cpa-us2
```

To remove the policies:

```sh
tmax model alias update gpt-6.1-terra --clear fallback
tmax provider update cpa-us2 --clear failure_policy
```

Connection-establishment failures can be opted into with
`fallback.connection_errors`; ambiguous transport failures use
`fallback.transport_errors`. Provider counting uses the corresponding fields
under `failure_policy.errors`. HTTP 5xx and ambiguous transport fallback can
duplicate execution or billing; configure them only when that trade-off is
acceptable. Unknown clients, retained sessions, upstream-owned resource state,
local rejection, and requests whose response delivery has begun cannot use
fallback. Each provider is attempted at most once per request; exhaustion returns
the last upstream rejection or transport failure, with a bounded attempt chain
in gateway request details. Failed attempts also appear in the separate local
[error log](#gateway-error-log), even if a later fallback succeeds.

## Confirmed quota suspension

By default, a confirmed OpenCode Go quota denial suspends that provider from
gateway routing. A generic HTTP 429 does not establish quota exhaustion. The
successfully saved suspension survives restarts and is distinct from both the
optional quota guard and the provider's enabled setting. A qualified successful
quota check that confirms all windows are available clears it; failed or unknown
checks do not. An announced reset is a recheck hint, not proof of recovery.
While the daemon runs with the configuration loaded, enabled suspended providers
are checked through the quota API at most once per provider per 60 seconds.
Retained destinations remain blocked rather than moving to another provider.
If saving a denial fails, the daemon still excludes the affected provider and
retries the save. The loaded status reports pending persistence and unconfirmed
durability; the file view can differ. A crash can lose this unsaved protection.

Inspect the safe saved and loaded status, or explicitly resume without waiting:

```sh
tmax provider list
tmax provider inspect opencode-go
tmax provider quota-resume opencode-go --dry-run
tmax provider quota-resume opencode-go --if-revision REVISION
```

Use the saved configuration revision from inspection or configuration output for
`REVISION`, or omit `--if-revision`. Preview makes no changes. Resume clears the
automatic suspension without checking upstream availability or changing the
enabled setting; an explicitly disabled provider remains disabled. A new
confirmed denial or qualified exhausted observation can suspend it again. The
Web provider detail also displays the loaded and saved views and offers resume.

## Quota guard

`quota_guard` is an optional provider-scoped policy. When the latest quota
observation for the provider exposes a qualified blocking window state such as
`rate-limited` or `exhausted`, the provider is excluded from new selection until
the earliest of the observation's applicability bound, the guard's maximum
observation age, and the announced reset. Only integrations whose contract
qualifies that state participate; currently that is OpenCode Go's
`rate-limited` window status. Other plan quota reads remain display-only. The
guard never changes model priority, enablement, or a retained session's
destination.

```sh
tmax provider update opencode-go --set quota_guard.max_observation_age_seconds 3600
tmax provider inspect opencode-go
tmax model explain opencode-go/deepseek-v4.1-flash --protocol responses --state loaded
tmax provider update opencode-go --clear quota_guard
```

The guard is absent by default because an exhausted included allowance may still
allow paid continuation. If no reset is announced, the guard creates no hold;
the separate confirmed quota suspension can still apply. A generic 429, a
capacity rejection, or a transport failure is not quota evidence; configure
request fallback or failure policy separately. An observation older than the
configured age does not create a guard hold. Manual quota resume also prevents
an unchanged older observation from immediately reinstating that hold; fresh
qualified evidence may block routing again.

## Gateway error log

Read the local error log directly with ordinary file tools:

```sh
jq -r '.error_message' ~/.tmax/error.log
jq '.errors[].error_details' ~/.tmax/error.log
jq 'select(.http_status == 200 and .outcome == "failed")' ~/.tmax/error.log
tail -F ~/.tmax/error.log
```

The file is created when the gateway first records an error. It remains readable
after the service stops or the saved configuration becomes invalid. There is no
dedicated CLI or Web query for error logs. The log is separate from current-process
`history list`, `history inspect`, `history clear`, and usage aggregates. Clearing
history does not delete the log.

tmax stores the active log at `~/.tmax/error.log`, restricted to the current OS user.
It does not automatically upload the file or intentionally store authentication
headers or secret values. Raw upstream error bodies and events may echo request
snippets or other sensitive content, so review the file before sharing it. The log
does not archive request bodies or successful response content. Local rejection,
transport failure, HTTP 200 stream failure, cancellation, interruption, and unknown
completion can also be recorded. An earlier fallback failure can lack its response
body; the detail identifies that gap.

Each line is a compact terminal request event, with a final `outcome` and an
`errors` array of failed attempt details. A completed request with earlier failures
is a recovered event at warning level, not a final failure. The root `time` is the
terminal logging time; detail times are capture times. Error details contain parsed
JSON when complete and valid, or captured text for plain, malformed, or truncated
content. Root and detail `error_message` fields are readable summaries. The root
contains no record or decision snapshot.

Before an append would exceed 4 MiB, tmax renames the active log to `error.log.1`,
replacing the previous backup, and creates a fresh active log. The two files retain
at most 8 MiB in total; older generations age out. Raw error capture is limited to
64 KiB across at most 16 details per row. Summaries, identifiers, stages, and reason
codes are limited to 512 UTF-8 bytes each. `truncated` marks truncated or omitted
content; `evidence_gap` includes request diagnostic gaps and log truncation.
Rotation, capture or write gaps, and traffic outside the gateway limit coverage.
Logging does not wait for a busy log lock: it reports a gap instead of blocking
the response. A failed append attempts to remove only its newly written suffix.
A failed rollback or crash can leave a partial final line; there is no atomic-row
guarantee. See the
[diagnostics contract](../specs/diagnostics/spec.md#persistent-gateway-error-log)
for the event schema and storage boundary.

## Native login and logout boundaries

Native sign-in and sign-out run through a qualified native command; they are not
proof that tmax has OAuth access or provider entitlement. Before invoking the owner
command, a protected preflight durably advances authorization lineage for both
saved and loaded views. It invalidates dependent inputs (including manual quota)
and prunes their observations, while retaining subscription and other manual
declarations. An `authorization_invalidation` failure is pre-native: the owner
command has not run. A `native_effect_uncertain` or partial native result means do
not assume the old authorization or observations were restored and do not retry
blindly. Inspect the same provider and native scope, then recover through that
scope's qualified owner; never copy native tokens into tmax or another agent.

## Gateway naming upgrade

Use `tmax gateway ...` and the `[gateway]` configuration table. Structured
operation names, result fields, reason codes, Web navigation (`#gateway`), and
the explicit key endpoint (`/api/gateway-key/reveal`) use the same name. The old
model-service command and flag spellings are no longer accepted; update scripts
and API clients.

A managed target reaches the gateway through the derived provider: use
`tmax agent provider add ... --provider tmax` and select public route names. The
version-1 `--gateway` entry selector is removed; see
[Configuration format version 2](#configuration-format-version-2) for conversion.
A version-2 entry that keeps a removed `gateway` or `proxy` field is rejected as
an unknown field.

Existing configuration and shared-key records remain readable. Reads and previews
preserve the saved file; the next explicit configuration save canonicalizes the
historical root `[proxy]` name while retaining key references and values.
Supplying old and new root names together is rejected. See the
[configuration contract](../specs/configuration/spec.md#format-and-authority).
The independent outbound-network command remains `tmax network proxy URL`.

## Gateway-only access

The local gateway is independent of managed Pi/Codex targets. Direct clients do not
need an `agent` declaration or native apply. A concrete qualified model can be
used directly by a client that knows its `provider-id/model-id` ID; a managed Agent
entry that uses the gateway selects a public route name, which the Web calls a
gateway entry.

### The gateway provider

Configuring the gateway also contributes one derived provider named `tmax`. It is
not stored configuration: the gateway owns its listen address, shared key
reference, and route set, and its models are the gateway's public route names.
A managed target reaches the gateway by referencing that provider.

```sh
tmax agent provider add dsh-main tmax --provider tmax
tmax agent model add --protocol responses dsh-main tmax glm-5.4
tmax agent apply dsh-main
```

Provider operations cannot create, rename, or remove provider `tmax`, and model
operations cannot edit its derived model set. `tmax provider inspect tmax` reports
`source: gateway`, the derived endpoints and route names, and a `delivery` block
with the current delivery revision and each declared relayed entry's applied
delivery state and named drift reasons. `tmax gateway remove` is refused while a
declared target entry still references the provider; remove those entries first.
The version-1 `--gateway` entry selector is removed; `tmax config migrate`
rewrites a stored version-1 entry to `provider = "tmax"`.

Consumers such as the DSH plugin read this inventory instead of computing their
own route set: `agent list` for the declared target that matches a home and its
path, `agent inspect TARGET` for each entry's `relayed` flag, `base_url`,
`credential_ref`, and applied delivery state, and `gateway status` for daemon
discovery state, listener readiness, and listen address. Native application
remains `tmax agent apply TARGET`; there is no separate projection or connect
command.

### Gateway entries

A gateway entry is the Web interface's name for one model alias: a
client-selectable name that references one or more existing concrete models. The
Models page offers **通过网关使用** on each concrete model that does not already
have an entry. The default entry name is the model ID part of the reference (for
example `glm-5.4` from `z.ai/glm-5.4`), and the dialog asks the user to confirm it
and the native protocols before saving. If that name is already taken, the form pre-fills a
provider-prefixed suggestion and lets the user edit it; it never renames an
existing entry. The accepted name is the client-facing route name.

Saving an entry only writes the alias definition. It does not configure or start
the gateway and does not modify Agent target files. The entry status therefore
shows the remaining next step: configure the gateway, start it, reload a gateway
that has not loaded the newest configuration, use the entry in a tool, or resolve
a disabled entry or unavailable source. When the gateway is ready, follow the
[gateway provider](#the-gateway-provider) section to make the entry a relayed
Agent selection.

```sh
tmax model alias add glm-5.4 --protocol responses --candidate z.ai/glm-5.4
tmax gateway configure --listen 127.0.0.1:17840
tmax gateway start
```

The corresponding Web action is the same `model alias add` operation; it does
not introduce a second gateway-model resource. The Gateway page lists entries,
their source models, protocols, status, and next action instead of showing only a
publication count. Raw `provider-id/model-id` IDs remain available to direct
gateway clients; an entry is a convenient client name, not an access-control
boundary. For a multi-model entry, use the advanced gateway-entry editor in the
Models page or the CLI to add candidates, continuity, fallback, or per-candidate
effort.

### Connecting an unmanaged client

An agent without a version-qualified [native reference](agents/README.md) is
unmanaged: tmax does not read or write its configuration. Connect it as a plain
provider instead:

- **Base URL** — the configured listener, for example `http://127.0.0.1:17840`.
  The protocol paths are `/v1/chat/completions`, `/v1/responses`, and
  `/v1/messages`; the client's own configuration chooses the protocol, and the
  gateway does not translate it.
- **Authorization** — `Authorization: Bearer <shared key>`. Deliver the key on an
  attached terminal with `tmax gateway key show` and paste it into the client; no
  generated or revealed key belongs in examples, argv, environment, or files.
- **Model** — any ID published for the current key: a concrete
  `PROVIDER_ID/MODEL_ID` or an enabled alias. A route serves only the protocols
  it declares. No Agent registration, native selection, client registry, or
  allowlist entry is involved.

```sh
curl -s http://127.0.0.1:17840/v1/models \
  -H "Authorization: Bearer <paste from tmax gateway key show>"
```

Do not infer an unknown agent's schema from a similar tool or edit its files on a
guess. Manual connection is the supported path because an unqualified format has
no evidence; an agent becomes a maintained target only with a version-qualified
reference and the shared apply/restore contract, as the agents above.

Inference request bodies are limited to 512 MiB (536,870,912 bytes). This local
HTTP-body byte limit is separate from a model's context window or token budget.
An oversized body returns HTTP 413 with `request_too_large` before upstream
dispatch; the gateway never truncates the request. Inspect the running limits
with `tmax gateway status`. Each body is buffered and parsed, so concurrent large
requests can use substantially more memory than their raw body sizes. Management
API limits remain separate and unchanged.

Available model protocols follow the provider's resolved endpoints, not a
per-model approval list. An endpoint declaration is not proof of upstream model
support: tmax forwards the selected native protocol and returns the upstream
result. It does not translate protocols, switch models, or retry another
destination when the upstream rejects a combination.

The client chooses the protocol in its configuration and request path. Prefer
Responses when upstream declarations and the client both support it; the gateway
does not negotiate or switch protocols. Concrete IDs publish the provider's
resolved protocols, while an alias publishes only the protocols it declares.
`agent model add ... --protocol responses` selects Responses for a
native client; it is not a gateway-wide preference or a change to saved Chat
Completions selections. Use separate DSH provider entries for different protocols.

The default public ID can be the concrete model:

```sh
PUBLIC_ID="$PROVIDER_ID/$MODEL_ID"
```

To use a stable alias instead, create it before starting the gateway. A route
declares one or more protocols and shares one ordered candidate list across them:

```sh
PUBLIC_ID=my-model
tmax model alias add "$PUBLIC_ID" --protocol chat-completions --protocol responses \
  --candidate "$PROVIDER_ID/$MODEL_ID"
```

A request uses one declared protocol; a candidate that does not expose it cannot
serve that request, and the gateway does not translate between protocols. To
replace the declared protocol list later, repeat `--protocol` with the complete
new list; omit it to keep the current list:

```sh
tmax model alias update "$PUBLIC_ID" --protocol chat-completions
```

`tmax model list --kind alias` reports one row per route with `protocols` (the
declared list), `published_protocols` (empty while disabled), `enabled`, and
`continuity`; `--protocol P` filters to routes that declare `P`.
`tmax model inspect ROUTE` reports the full definition and treats `--protocol` as
an optional projection filter: without it, the report uses the first declared
protocol and names it in `protocol`, and an undeclared protocol is an
`invalid_arguments` error. An unknown route ID is `unknown_resource`.

To pin an effort for a particular candidate, update that candidate's alias
entry. The options are repeatable for distinct candidates; each effort must be
listed in that concrete model's effective positive `reasoning_options` metadata
(`type = "effort"` with explicit `values`). `reasoning = true` alone is not
enough. A configured value takes precedence over both caller effort and the
model map when that candidate is selected. Without a fixed override, an explicit
model map applies to supplied input, otherwise identity or omission is preserved.
The candidate advertises only its fixed upstream name; common alias choices are
intersected after maps and fixed restrictions. Effort never chooses a different
backend. Chat Completions and Responses support this; Messages does not. The
route must declare Chat Completions or Responses; a route that declares only
Messages cannot configure candidate effort.

```sh
CANDIDATE="$PROVIDER_ID/$MODEL_ID"
tmax model alias update "$PUBLIC_ID" \
  --candidate-reasoning-effort "$CANDIDATE=$SUPPORTED_EFFORT"
tmax model alias update "$PUBLIC_ID" \
  --clear-candidate-reasoning-effort "$CANDIDATE"
```

Set `SUPPORTED_EFFORT` to a nonempty value exposed for that concrete model.
These alias-specific options do not define a global default or arbitrary
request parameters.

Then configure and start the loopback listener explicitly:

```sh
tmax gateway configure --listen 127.0.0.1:17840
tmax gateway start
tmax gateway status
```

Initial `gateway configure` provisions the sole shared local key in a current-user
restricted file under `~/.tmax`; the ordinary configuration stores its reference.
The key file is plaintext, not encrypted. `gateway start` is a separate explicit
lifecycle action. Provider-key setup prompts invisibly on an attached terminal
or accepts `--key-stdin`; the Web form contains a provider-key field. Use explicit
key input for automation, never argv, environment, or
ordinary configuration files. CLI and Web need no separate installation command.
New background processes require a reachable native user service manager.
Provision the shared key to a client only through the qualified
`gateway key show` delivery flow. Ordinary inspection does not reveal it. All
current-key holders share the same published model list; no client registration
is required.

tmax resolves the OS account's home, not an override of the `HOME` environment
variable. On Unix, a home that yields an unrepresentable control-socket pathname
blocks startup with `daemon_control_path_invalid`, the actual path, and the
platform's representation error before launching a daemon. Use an OS account
whose home permits that pathname; tmax does not relocate its store or fall back
to a shorter endpoint. No listener, credentials, or native configuration are
changed by this startup blocker.

New processes run in a native supervised user job: systemd on Linux, launchd on
macOS, or Task Scheduler on Windows. The job runs the foreground daemon for its
whole lifetime. Windows control remains a private same-user named pipe.
`tmax web open` opens the system browser; `--no-browser` starts management without
opening it. Closing the browser or launching terminal does not stop inference.
The manager can recover an unexpected process exit without a connected client.
Recovery restores only still-requested listeners at their acknowledged addresses.
It never replays interrupted inference.

On Linux, systemd applies the process restart limit. The recovery attempt count
in `tmax gateway status` reports automatic restarts, not the initial start.
If the process exits before it can report listener readiness, use
`journalctl --user -u tmax.service -b` to see the foreground diagnostic.

A configured gateway starts at future user-manager/login activation by default;
no declaration is required. `tmax gateway configure --set autostart false` is
the explicit opt-out; `--clear autostart` (or `--set autostart true`) restores
the default. Disabling does not stop current listeners or remove their
supervision. A retained definition with login startup disabled is normal.
No command implicitly enables Linux linger, elevates privileges, or installs a
machine-wide service. Logout survival depends on the actual user-manager session.
An unavailable manager returns `service_manager_unavailable`; it does not fall
back to a detached process. `gateway status` reports `supervision`, `autostart`,
and actual listener readiness as independent facts.
Startup reads the same current-user-restricted
secret files. Unsupported storage-format records return `storage_version_unsupported` or
`storage_record_invalid` and remain unchanged. Re-enter the provider key with
`provider key set` or replace the shared key with `gateway key replace`. Recovery
contents in unsupported formats are unavailable to current recovery operations.

If the current CLI reports `daemon_incompatible`, the daemon is too old for
this executable's management protocol. Use the running daemon's matching
pre-cutover CLI (`tmax gateway stop`, then `tmax web stop`) or its existing Web
controls to perform the explicit stops before starting the current executable;
the new CLI will not take over or terminate an incompatible daemon.

To update tmax, replace the executable and run `tmax gateway restart`. Restart
recycles the daemon process from the on-disk executable: in-flight requests are
cancelled (never drained, replayed, or refused), ownership is released, the
replacement process starts with the current saved settings, and a Web listener
that was running is restored. `tmax gateway stop` likewise cancels in-flight
requests instead of refusing while they are active.

Repeated `gateway start` and compatible `web open` reuse the identified listener,
return `daemon_reused`, and do not reload pending saved settings.

Stopping inference leaves Web available, including after a later process crash.
Stopping the last requested listener disarms recovery for the current activation.
During queued recovery, stop still records suppression before it reports success.
Invalid saved settings, a missing shared key, or a conflicting address block
inference; they do not justify a tight restart loop or a different port.
A saved inference address change requires explicit restart, even after a crash.
Malformed recovery metadata requires explicit lifecycle repair rather than guessed
listener requests; restarting with no identifiable live Web owner does not guess
the missing Web address. Open Web explicitly afterward when needed.

The Web restart action first acknowledges a pending request because its HTTP
process must exit. After acknowledgement, the page reconnects and reports
completion only after a new process instance serves the requested listeners.
Without acknowledgement, the outcome remains unknown; refresh status rather than
resubmitting or treating an unrelated process replacement as this request's success.
CLI restart waits for readiness before returning success.

Recovery is bounded to five starts in its attempt window. Linux and macOS request
a two-second native restart delay with a sixty-second window; Windows uses the
Task Scheduler minimum sixty-second delay and a five-minute attempt window.
Application readiness has a separate fifteen-second deadline after manager start.
Native command deadlines remain independent of inference lifetime.
Inspect `supervision.recovery` for observed attempts and the actual native delay.

Linux local smoke exercised repeated start, client-independent crash recovery,
inference stop followed by Web-only recovery, queued-stop suppression, independent
login activation, CLI restart, and actual browser restart/reconnection.
Windows cross-compilation is not native Task Scheduler qualification.
macOS launchd and Windows login-session recovery still require native-host evidence.

The gateway is a plain model provider for its callers: any request bearing the
shared key is routed and forwarded unchanged, whatever client sends it and
whether or not it carries complete conversation context. It accepts
same-protocol requests and forwards to the configured provider; it does not
translate protocols. A recognized native client only gains that conversation
destination affinity; unrecognized clients are served the same way, and an
upstream that itself requires a native session or client identity still filters
which candidates are eligible. Configure a real provider credential before
expecting upstream completion. Synthetic loopback checks qualify local wiring
only. The exact hosted scenario and its native request-control qualifications are
recorded in the owning [Go evidence section](../specs/integrations/opencode-go/spec.md#sources-and-evidence-status);
they do not establish every client's model access or entitlement.

## Status, failure history, and gateway verification

`tmax status` reads saved configuration and retained daemon facts; it does not
resolve secrets, refresh providers, sign in, or start a service. Use `--brief` for
the answer an operator needs first:

```sh
tmax status --brief
```

The result is the normal envelope; `data` always contains `brief: true`, `usable`,
`blocker`, and `next_action`. Read it in that order:

| Field | Meaning |
| --- | --- |
| `usable` | `false` when any known blocker exists, `true` when none does, `null` only when an unidentified or unavailable daemon is the remaining uncertainty for a relayed entry. |
| `blocker` | `null` when usable; otherwise `{code, message, field, next_action}`. `message` names the concrete condition, and `field` is set when an invalid configuration field caused it. |
| `next_action` | The repair command to run, or `null` when usable. |

Only the first blocker is reported. Common codes and their next actions:

| Blocker code | Meaning | Next action |
| --- | --- | --- |
| `configuration_format` | The saved file uses an unsupported format version. | `tmax config migrate` |
| `configuration_invalid` | The saved file cannot be read or validated. | `tmax config validate` |
| `nothing_configured` | No configuration file exists or it declares no providers. | `tmax provider add --help` |
| `daemon_identity_unavailable` | Relayed entries need daemon evidence that is unidentified, incompatible, or unavailable. | `tmax gateway status` |
| `gateway_not_ready` | The gateway is configured but not ready. | `tmax gateway start` |
| `key_rotated_not_reloaded` | The identified daemon loaded a different shared-key reference than saved configuration. | `tmax gateway restart` |
| `delivery_unknown` | Retained delivery state for a relayed entry could not be read. | `tmax agent inspect TARGET` |
| `delivery_stale` | An applied relayed target is stale. | `tmax agent apply TARGET` |
| `delivery_blocked` | An applied relayed target is blocked. | `tmax agent apply TARGET` |
| `stale_shared_key` | Clients are presenting a shared key this gateway rejects. | `tmax gateway key show` |

A direct-only installation stays usable while the daemon is stopped, and the
daemon-identity and gateway-readiness blockers apply only when a declared relayed
entry needs the gateway. `daemon_identity_unavailable` applies only when a daemon
reports `discovery_state` `unidentified`, `incompatible`, or `unavailable`; an
absent daemon reports `discovery_state: absent`, skips that rung, and reports
`gateway_not_ready` instead. The `delivery_stale` and `delivery_blocked` messages
name the affected target and its drift or block reasons, so replace `TARGET` with
the reported ID. `stale_shared_key` is one aggregated condition;
`tmax status --brief` does not repeat it per rejected request. Full `tmax status`
keeps the detailed fields and adds `data.failures`; a null `data.failures` means
no daemon supplied failure history.

A `delivery_unknown` blocker means the retained delivery state of a relayed entry
could not be read; run `tmax agent inspect TARGET` for the reason code. Recorded
delivery facts that are missing or outdated — for example after a format
migration — are refreshed by an unchanged `tmax agent apply TARGET`, after which
`agent inspect` and `provider inspect tmax` stop reporting `unknown`.

### Reading failure history

`failures` is the daemon's aggregated projection of retained requests under the
[failure aggregation contract](../specs/diagnostics/spec.md#failure-aggregation).
Full `tmax status` and `tmax gateway status` expose it as `data.failures` when a
compatible daemon supplies retained history:

- `groups` — retained failed and rejected requests grouped by reason code and
  stage, with `count`, `first_at`, `last_at`, and the latest `outcome`. The largest
  groups are kept and the rest are counted in `omitted_groups`.
- `repeated` — failed or rejected requests that belong to a group with at least
  two records. `repeated: 0` means no condition occurred twice in the retained
  window; it does not mean requests succeeded.
- `stale_shared_key` — null, or the shared-key rejection condition named once:
  `count`, `first_at`, `last_at`, `reason_code`, `message`, and `repair`.
- `coverage` — the retained-history scope and limits.

This history is bounded in process memory, not a durable audit log. It covers only
what the current gateway process retained; a restart drops it, and a replacement
process reports no earlier failure coverage. Use `tmax history list` when
per-request rows are needed.

### Verifying gateway access

`tmax provider refresh tmax --scope SCOPE` checks the derived gateway provider with
`SCOPE` equal to `auth`, `models`, `quota`, or `all`:

| Scope | What a successful check proves | Bound |
| --- | --- | --- |
| `auth` | The shared key is accepted by the running gateway. | One authenticated `GET /v1/models`, 5 s |
| `models` | One bounded inference request per enabled route reaches the gateway. | One request per route, 60 s per route |
| `quota` | Observed usage from the identified gateway process. | No upstream contact |

```sh
tmax provider refresh tmax --scope auth
tmax provider refresh tmax --scope models
tmax provider refresh tmax --scope all --dry-run
```

Read `data.checks`: each check reports its own `scope`, `outcome`, and
`boundary: reachability_only`. `outcome: completed` with
`proof: shared_key_accepted` means the gateway accepted the current key. A failed
auth check names a reason code and usually a `repair`:
`gateway_not_ready` (`tmax gateway start`), `key_rotated_not_reloaded`
(`tmax gateway restart`), or `shared_key_rejected` (`tmax gateway key replace`).
The `models` check reports one `{route, protocol, outcome, reason_code?}` per
enabled route plus `summary: {routes, completed, failed}` and
`boundary: reachability_only`; when the gateway is not ready it reports
`not_executed` and lists the route names without sending requests, and when the
daemon loaded a different shared-key reference every route reports
`key_rotated_not_reloaded` with repair `tmax gateway restart`. The `quota`
check returns observed usage with `proof: observed_gateway_usage` and
`upstream_contact: false`; without a daemon it reports
`not_executed`/`no_daemon_history`.

A failed check does not stop the other requested scopes; the result reports every
check and reports `observation_failed` with exit 4 when a check failed or could
not run. A `not_executed` check did not run, so do not read it as success.
`--dry-run` reports the planned checks without contacting the gateway, reading the
key, or contacting the daemon.

A successful refresh proves reachability from tmax only. It does not prove that an
Agent target loaded or used the configuration, that an untested protocol works, or
that observed usage is a provider-side balance.

### Shared-key rotation

Replacing the shared key affects every applied relayed target and every unmanaged
client that holds the previous key:

1. Run `tmax gateway key replace`. The result reports `gateway_key` and
   `rotation.affected_targets`, `rotation.next_action`, and
   `rotation.unmanaged_clients`.
2. Make sure the running listener loaded the replacement. A saved replacement
   activates automatically in a running daemon; after a replacement made while the
   gateway is stopped, the next `tmax gateway start` loads it. If `tmax status
   --brief` reports `key_rotated_not_reloaded`, run `tmax gateway restart`.
3. Run `tmax agent apply TARGET` for every affected target. Each one is stale with
   reason `key_rotated` until it is applied again; `rotation.next_action` names the
   first.
4. Update each unmanaged client with the new key. tmax cannot enumerate them;
   `rotation.unmanaged_clients` states this explicitly. Deliver the new value with
   `tmax gateway key show`.

A client that still uses the previous key produces repeated shared-key rejections,
which `tmax status --brief` reports as one `stale_shared_key` blocker until the
client is updated.

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

Rules are model-local, append in explicit order, and compare retained exact
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

Calendar schedules are local-time rules; set the model timezone first. For an
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

## Default model and reasoning effort

A target default may also declare one reasoning effort. For DSH, pi, OMP, and
OpenCode the value is a caller-facing effort name that the selected model
declares and the tool can represent; Codex and Hermes take a verbatim native
value instead:

```sh
tmax agent default set dsh-main tmax deepseek-v4.1-flash \
  --protocol responses --effort max
tmax agent apply dsh-main
```

Only explicitly configured fields are written. A default that omits the effort
preserves whatever effort the tool currently has, including inside the same
native entry or settings object. A configured effort the model or tool cannot
represent blocks application before any native write and lists the available
names. Use `--native-effort` only for Codex or Hermes; it is written without
selector validation.

Removing the effort from the default (run `agent default set` without
`--effort`) stops managing it; the next apply restores that field's captured
baseline and still refuses to overwrite an external edit. OpenCode is the
exception because its model and variant are one native reference: removing only
the effort keeps the current variant, while `agent default clear` or a model
change rewrites the reference and the whole field is managed as one unit.

If the tool's own settings changed a tmax-owned default after apply, the next
apply stops with a conflict instead of overwriting it. Resolve it explicitly:
`tmax agent inspect TARGET` lists the conflicting fields and the tool's current
native default;
`tmax agent recovery accept TARGET` records the tool's current native values as
the new recovery baseline (then set the tmax default to the same model and
effort and apply, which writes nothing); `tmax agent restore TARGET` returns the
captured pre-tmax baseline; and `tmax agent recovery discard TARGET` followed by
`tmax agent apply TARGET` makes tmax's configured default win at the cost of the
original restore baseline. The tool must reload or restart before new sessions
use the updated default; running sessions keep their selection.

## Pi target: apply, reload, restore

Native configuration is explicit and separate from gateway startup. Use an existing
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
inference or make a paid probe. `tmax agent apply` reads referenced provider keys
from tmax's current-user-restricted files and uses the native adapter's declared
credential handoff. Unsupported storage-format key records return `storage_version_unsupported` or
`storage_record_invalid` and remain unchanged; re-enter the key with
`provider key set` before applying. Unsupported recovery contents are unavailable
to current recovery operations. The `awaiting_native_reload` state means the native
tool must reload/restart before the change is effective. When the tool rewrites
its own configuration and apply reports `native_conflict`, resolve it
explicitly: `agent restore` reverts owned fields, or `agent recovery accept
TARGET` re-baselines the retained record to the current native state without
native writes, credential deletion, or losing credential ownership; apply then
proceeds from the accepted state.

For qualified Pi and OMP concrete selections, known effective model context/output
limits, reasoning capability, and supported text/image input metadata are written
using the versioned native schema. False values remain false; unknown metadata is
not manufactured, and an unrepresentable explicit token limit blocks before native
writes. Alias metadata is not copied from whichever candidate currently wins: an
alias row carries a context/output limit only when every candidate's effective
facts declare the same value, so disagreement or a gap leaves that limit unset.
`model add`, `agent inspect`, and `agent apply` report `fact_gap`/`fact_gaps`
when a model has no catalog or declared reasoning-effort or context-limit fact:
the native entry stays bare for those fields, and a static surface such as DSH
completes none of them itself. Declare the facts with `model update` or
`model reasoning`, or accept the degraded entry.
See [versioned agent schemas](agents/README.md) for the distinct native fields,
authentication precedence, scopes, and qualification limits.

Model capability limits are not generation budgets, and the gateway does not
police request parameters against them. Native sampling, thinking effort, output
budgets, tool use, structured output, and modalities stay owned by the native
agent and travel to the upstream unchanged; the upstream clamps or rejects them
with its own error. Use the agent's supported request controls when deliberately
bounding an authorized probe.

```sh
tmax agent restore "$TARGET"
```

Restore preserves later user edits and reports when the native tool must reload.
With Pi 1.2.0, a private synthetic-provider fixture's apply and restore each
returned exit 3 / `awaiting_native_reload`; the installed Pi parser recognized
the selected model. This proves local native configuration parsing, not hosted
credentials, entitlement, or permission.

For a Pi client that should use the gateway, add a relayed entry, select a public
route name, set it as default, then apply the target:

```sh
tmax agent provider add "$TARGET" local --provider tmax
tmax agent model add "$TARGET" local "$PUBLIC_ID" --protocol chat-completions
tmax agent default set "$TARGET" local "$PUBLIC_ID" --protocol chat-completions
tmax agent apply "$TARGET"
```

The gateway itself must already be configured and explicitly started; this native
target path is optional. Pi 1.2.0 completed a same-protocol Chat Completions
round-trip through tmax to a private synthetic loopback provider using a
synthetic key. This is local gateway/native wiring evidence, not hosted access.

`model probe` is a separate potentially paid operation. Choose a transport
listed by `integration inspect` and invoke it explicitly; apply/restore never
run it. Hosted permission is verified outside this guide.

For the complete contract, see [CLI specs](../specs/cli/spec.md),
[provider input specs](../specs/cli/providers/inputs/spec.md),
[model routing](../specs/model-routing/spec.md), and
[Pi/agent target specs](../specs/cli/agents/spec.md).

## Native session listing and import

`session list` reads the session stores of the declared Agent targets without
changing native files, credentials, or configuration. It lists Codex, pi, omp,
OpenCode, Hermes, and DSH sessions, including archived, hidden, and unnamed
native-readable rows. A row contains bounded metadata and a bounded title, not
message bodies or tool output. With no selector it scans every declared target;
use `--target` for one target, or `--agent` plus `--path` for an undeclared,
list-only scope.

```sh
tmax session list
tmax session list --target pi-main --cwd "$PWD"
tmax session list --agent omp --path "$HOME/.omp/agent" --since 2026-11-01T00:00:00Z
```

`--cwd` matches only the primary recorded working directory after lexical
normalization; it does not match additional workspace roots. `--since` and
`--until` are inclusive RFC 3339 bounds on updated time. The default limit is 50;
the maximum is 500. A target that is absent, unresolved, unreadable, or
unsupported is reported in `targets` and `warnings` without suppressing rows from
other targets. Rows are ordered newest first and deduplicated by native store and
session ID.

`session import` creates one new native session in a declared destination target
from an exact source session ID. The source is read-only. tmax selects one visible
branch, omits hidden reasoning, system/developer context, attachments, additional
workspace roots, and other non-portable state, and preserves tool activity as
non-replayable text. It does not copy providers, models, credentials, settings,
defaults, roles, or gateway state.

```sh
tmax session import pi-main SESSION_ID --to omp-main --dry-run
tmax session import pi-main SESSION_ID --to omp-main
tmax session import pi-main SESSION_ID --to codex-main --tail 8
```

The destination must be a declared target with a qualified writer: pi, omp,
OpenCode, or Codex. Hermes and DSH are read-only session sources in this version.
The destination working directory defaults to the source's recorded primary cwd;
supply `--cwd` with an absolute existing directory to override it. A missing,
null, or ambiguous source cwd fails rather than falling back to the destination
target or process cwd. The writer binds the effective cwd, and tmax verifies the
new session through the destination's own reader before reporting success.

`--tail N` keeps the first prompt and the most recent N turns, adds an explicit
omission marker, and reports dropped turn and byte counts. `--title` supplies a
bounded, control-free destination title. `--allow-over-budget` conflicts with
`--tail` and applies only when a known destination budget would otherwise block
the import. `--again` creates a new copy after an earlier completed import;
without it, an existing matching import is a conflict. `--dry-run` reports the
plan and qualification without writing native files or creating a provenance
record. Completed imports are recorded under tmax runtime storage as bounded
provenance (counts, paths, schemas, title, and source revision), never as a
transcript copy. A missing or corrupt record store is reported as
`record_unavailable` and does not block the native import.

Exit `0` means the requested list or verified import completed. Invalid
selectors or source schemas exit `2`; a precondition that prevents native effects
exits `3`; a native write, registration, readback, or post-commit record failure
exits `4`. A partial target scan still exits `0` and reports `coverage: partial`.

## DSH target: profile-scoped native application

DeepSeek Harness uses the same target/provider/model operations as the Pi example,
with `--agent dsh` and the existing DSH home as its scope. Discovery uses nonblank
`DSH_HOME`, otherwise the OS home directory's `.dsh`.

A provider is a profile-level setting in DSH — its Models page writes into
`profiles/<profile>/cordis.patch.yml`. Apply writes there too: one provider per
target entry, named `tmax-<entry>`, with the endpoint, protocol, models, and a
credential reference, and puts the referenced key in the credential document DSH
already reads. Nothing has to be installed first, and the next DSH start serves
the routes. Apply selects the profile by `--profile`, otherwise `desktop`, then
`web`, and reports which one it used.

Apply preserves the user's own providers in that row and every unrelated field
and expression. It does not create or delete profiles, change bundle composition,
run npm, or touch the launcher-owned `cordis.yml`. Provider configurations from
all adapters share DSH's LLM runtime; native provider names must be globally
unique, not automatically merged. Direct and relayed entries may coexist. Provider configurations from all adapters share DSH's LLM runtime;
native provider names must be globally unique, not automatically merged. Direct
and relayed entries may coexist. Runtime overlays remain outside
tmax's ownership. The [DSH home boundary](../specs/integrations/spec.md#dsh-home-boundary)
owns the application and recovery rules.

```sh
DSH_SCOPE="${DSH_HOME:-$HOME/.dsh}"
tmax agent discover --agent dsh
tmax agent add dsh-main --agent dsh --path "$DSH_SCOPE"
tmax agent provider add dsh-main upstream --provider "$PROVIDER_ID"
tmax agent model add dsh-main upstream "$MODEL_ID" --protocol chat-completions
tmax agent apply dsh-main --dry-run
tmax agent apply dsh-main
tmax agent inspect dsh-main
tmax agent restore dsh-main --dry-run
```

Model-only application requires no default and writes no `agent-default-model`
entry or thinking preferences. An explicit default writes provider/model and,
only when the default declares `--effort`, one validated effort; an absent
effort preserves the existing HOME-level setting rather than importing profile
defaults. Native route APIs support all three protocols; incompatible
protocol/base selections cannot share one provider entry. Executed apply hands
the key to the target home's private native `.credentials.yaml` `refs` map and
uses a stable `TMAX_...` reference in the adjacent home patch. It enables the
native credential service in the HOME patch for headless consumption without
bundle inspection, exported secrets, or a tmax launcher. Preview and inspect
never resolve or return key values.
Output capability is intentionally not projected because DSH's explicit
`maxTokens` would also change a per-request default.

Reload DSH instances using that home after apply/restore. Restoration covers the
profile rows tmax wrote and its credential references while preserving unrelated
edits. The profile's `llm-pi-ai` entry is the only application format tmax
writes: it never edits the home `cordis.patch.yml`, so a row an earlier release
put there is yours to remove.
File writes alone are not runtime or provider-access evidence. DSH 0.2.1-rc.2
loaded the independent plugin and completed local synthetic image/tool requests
through all three protocols, with direct and concrete gateway routes coexisting
alongside a native user adapter. An isolated public apply–native request–restore
run also passed with profiles unchanged. Synthetic gateway forwarding used a
real isolated tmax router with harness-prepared native configuration. Separately,
an installed release and matching daemon passed public gateway apply and actual
DSH headless Chat Completions and Responses requests through stateless aliases
to the selected OpenCode Go provider. The current managed native entry is `tmax`,
using `openai-responses`. Freeing that name required explicit removal of obsolete
user routes in two local profile patches; tmax does not automate that edit.
These runs do not qualify both accounts, hosted tools/images, native Web settings,
or replay-state follow-up. DSH has no qualified native login or Go session
mechanism. See
the [DSH guide](agents/dsh.md) for the source revision, native loader and adapter
reuse, credential ownership, exact reasoning-effort mapping, gateway limitations,
and the explicit restore command. Skill installation remains shared `.agents`
or Hermes-only.
