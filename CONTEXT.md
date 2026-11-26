# Model Access Management

tmax manages a person's access to model providers, subscription information,
and the use of that access by local coding agents.

## Language

**Provider**:
A user-managed upstream access instance that contains one authentication
configuration, its models, and optional subscription information. The one derived
gateway provider is also a Provider, but it has no user-owned metadata: the
gateway owns its endpoints, authentication, and model set. Separate upstream
providers can access the same service or upstream account without implying
separate allowances.
_Avoid_: Vendor, catalog provider, connection resource, account pool

**Catalog provider**:
A selectable external directory entry supplying default provider and model
metadata. Several local providers may share it while retaining independent
metadata overrides, authorization, and billing.
_Avoid_: Local provider ID, credential, model developer

**Provider integration**:
A supported definition of upstream protocols, authentication methods, endpoint
rules, and operations. A known-service definition identifies one access product
and fixed destinations, including its regional choice, not a user account.
_Avoid_: Integration as a standalone name, provider instance, subscription, catalog provider, Agent integration

**Account identity**:
The upstream user or workspace associated with a provider's authorization and
billing when known, not a separately managed resource inside tmax.
_Avoid_: Local provider ID, credential count, global user account

**Authentication configuration**:
A provider's API-key reference or declared login source and session reference,
not a separately managed credential resource. Credential material is distinct
from the configuration that refers to it.
_Avoid_: Account identity, subscription, credential pool

**Login authentication**:
Authentication established through a provider's interactive sign-in flow,
possibly producing an expiring and refreshable session.
_Avoid_: API-key login, reusable bearer key

**API-key authentication**:
Authentication with a provider-issued key, independent of how the user signed
in to the provider's website to create that key.
_Avoid_: Subscription authentication

**Subscription**:
Optional plan, allowance, and renewal information associated with a provider,
not a separately managed resource. Missing subscription information does not
establish a provider's billing state or entitlement.
_Avoid_: API key, prepaid balance

**Quota window**:
A provider-defined allowance measured over a period with a provider-defined
reset condition. Its unit and coverage are part of its meaning.
_Avoid_: Balance, local token count

**Quota observation**:
A source-attributed allowance fact for one provider, quota window, and model
coverage, with an observation time, an access state, and any announced reset.
Its source contract determines when it stops being applicable. It is evidence,
not a permanent balance or a recovery guarantee.
_Avoid_: Local balance, observed request usage, independent allowance per key

**Quota access state**:
The provider-qualified condition of a quota window in one quota observation:
available, rate-limited, exhausted, unavailable, or unknown. Exhausted means the
included allowance is consumed, not that upstream access is necessarily blocked.
It is not inferred from a generic HTTP status or local request count.
_Avoid_: Balance state, inferred quota denial, remaining quota

**Reset announcement**:
A source's statement that an allowance reset is planned for a specified time.
It bounds the applicability of an earlier quota observation; passing it does not
prove that the reset occurred or that an individual account recovered quota.
_Avoid_: Confirmed quota, renewal date

**Model**:
A named child of one provider, registered under its exact upstream model ID with
protocols, capabilities, and local settings. The same upstream model under another
provider is a distinct local model, not evidence of different model weights.
_Avoid_: Model offering, global model entity, protocol-specific duplicate

**Model reference**:
The exact qualified name `provider-id/model-id` identifying an existing model
under its provider, independent of wire protocol. It is a reference value, not
another model, a catalog ID, a gateway alias, or an upstream URL.
_Avoid_: Display label, catalog model ID, gateway model ID

**Model capability declaration**:
A sourced statement of a concrete model's supported inputs, limits, or controls,
not a selected request value or proof of authorized upstream access.
_Avoid_: Generation default, verified entitlement, request override

**Reasoning effort**:
An exact named value for a model's reasoning control, distinct from enabling
thinking or choosing a reasoning budget. Names need not have equivalent meaning
across models or native protocols.
_Avoid_: Universal thinking level, reasoning capability, thinking switch

**Reasoning effort mapping**:
An explicit model-scoped correspondence from a caller-facing effort name to an
upstream effort value for one protocol. It expresses user intent, not an
inferred equivalence of reasoning quality, a thinking toggle, or a default.
_Avoid_: Automatic level normalization, fixed effort, capability override

**Native capability projection**:
A representation of model capability declarations or derived alias common input
capabilities and effort choices in an Agent's supported configuration vocabulary,
not a native request preference.
_Avoid_: Model copy, default selection, request transformation

**Agent default selection**:
A target's explicit choice of default model and, optionally, one reasoning
effort written into that Agent's own actual configuration; distinct from a
capability declaration, a gateway request default, or a session choice.
_Avoid_: Universal default level, forced thinking, request override

**Candidate eligibility**:
Whether a configured model candidate may enter selection at an instant,
determined by configuration, authorization, protocol or client requirements,
applicable availability holds, and quota suspension. It is not effective
priority, request admission, or a preference.
_Avoid_: Routing eligibility, availability score, fallback result

**Availability hold**:
A scoped, time-bounded exclusion from candidate eligibility with a reason and
evidence. It changes neither configured enablement nor effective priority, and
does not move a retained session.
_Avoid_: Suspension, disablement, cooldown, priority override

**Quota suspension**:
A provider-authorization-scoped exclusion from routing after qualified evidence
of blocked quota access, retained until confirmed recovery or explicit manual
resume. It is separate from user-configured enablement and preference.
_Avoid_: Account deletion, permanent disablement, failure priority override

**Quota guard**:
A provider-scoped policy that turns an applicable quota observation whose access
state is rate-limited or exhausted into an availability hold until the observation
stops being applicable (including by the policy's maximum observation age) or its
announced reset passes. It is a user choice about included allowance, not proof
that upstream access is blocked, and it does not apply to another provider or
window.
_Avoid_: Quota enforcement, inferred denial, permanent disable

**Effective priority**:
A model's preference score at a particular instant, after applying
evaluable user-defined priority rules. Higher scores rank eligible candidates first.
_Avoid_: Routing weight, measured price

**Priority rule**:
A user-defined condition and replacement preference score within one model.
It changes preference, not candidate membership, entitlement, or task demand.
_Avoid_: Task planner, quota reservation, inferred optimization

**Request fallback**:
An explicitly permitted attempt through another provider among a model alias's
configured candidates after an upstream failure, before response delivery begins.
It does not migrate a retained session or infer remaining quota.
_Avoid_: Protocol translation, unlimited retry, automatic candidate discovery

**Failure priority override**:
A provider-scoped policy replacing its models' normal effective priorities with
a fixed score for a fixed duration after consecutive configured failures.
It changes preference, not enablement, quota facts, or established destinations.
_Avoid_: Priority cap, permanent priority edit, quota exhaustion proof

**Rule input**:
A named, provider-scoped source of dated quota or reset-announcement facts for
priority rules, with explicit model coverage and validity.
_Avoid_: Desired priority, universal balance, verified entitlement

**Agent**:
A local coding tool such as Codex, pi, omp, OpenCode, Hermes Agent, or DeepSeek
Harness (DSH), distinct from a particular installation or configuration of that tool.
_Avoid_: Family, configuration target, provider account

**Agent integration**:
Version-qualified support for an Agent's configuration, authentication ownership,
and safe restoration of tmax-managed changes. It is a tool-support capability,
not an Agent target, upstream access instance, or independently managed resource.
_Avoid_: Native as a standalone name, agent runtime, provider integration

**Agent target**:
One local agent installation, profile, or supported configuration scope that
can be managed independently. It identifies which Agent and which actual
configuration scope are being configured. DSH uses one home-wide scope, not
separate profile targets.
_Avoid_: Agent type, provider account, calling agent

**Harness**:
The user-facing name for one declared Agent target in the DSH in-context page
and in product guidance for that page's configuration flow. It is the existing
target and its configuration scope, not a tool, a DSH profile, a seat, or a
separately managed resource. The `agent` command family still declares, edits,
applies, and restores it.
_Avoid_: Tool, seat, DSH profile, agent type, separate resource

**Expected configuration**:
The current desired tmax-managed settings for one Agent target: its provider
entries, model selections, default, and roles. It is saved intent, not a
statement about the Agent's files. "Desired configuration" means the same thing.
_Avoid_: Pending changes, applied state, native state

**Target revision**:
A stable revision of one Agent target's expected configuration alone. Unrelated
targets and configuration-file formatting do not change it. It is a local
comparison value for that target, not an actual-configuration hash, the saved
configuration revision, or a native revision.
_Avoid_: Configuration revision, file hash, native revision

**Unapplied configuration**:
The state of an Agent target that has no completed apply of its current expected
configuration. That includes expected configuration that advanced after the last
successful apply, a target that has never been applied, and a retained record
that represents an incomplete write, a recovery-required state, or a restore. It
is a tmax desired-versus-applied comparison, not a native pending change, a
merge, a per-field difference, or a conflict.
_Avoid_: Native pending changes, pending queue, unmerged changes

**Actual configuration**:
An Agent target's own configuration files and credential artifacts that tmax
inspects or writes for that target. It contains both tmax-managed fields and
fields tmax leaves unmanaged. The Agent owns those files; tmax changes them only
through an explicit apply or restore. Not an upstream protocol, wire surface, or
model-catalog fact.
_Avoid_: Native configuration for an Agent's own files, applied configuration, target state

**Tmax-managed configuration**:
The fields, entries, and credential references of one Agent target's actual
configuration that tmax writes, recorded in its retained managed snapshot.
Everything else in those files is unmanaged and belongs to the user and the
Agent; apply and restore leave it in place.
_Avoid_: Owned settings without a retained record, conflict markers, managed state

**Authoritative apply**:
An apply whose only authority for tmax-managed configuration is the current
expected configuration. It reads the target's current actual inputs, overwrites
or removes managed fields and entries to match expected configuration, preserves
unmanaged fields, and does not block on later user edits to managed fields. A
user confirmation is consent to that overwrite; ownership and namespace guards
for another target's files or a foreign provider name still apply.
_Avoid_: Merge, three-way comparison, conflict resolution, pending queue

**Single-level restore**:
Restoration from the one retained managed snapshot for an Agent target, captured
immediately before its last successful apply and replaced by the next one. It returns tmax-managed fields and entries to that
snapshot, leaves unmanaged fields in place, and overwrites later edits to
managed fields. It is not a change history, a merge, or a conflict-resolution
step.
_Avoid_: Three-way merge, re-baselining, conflict-free restore, per-field recovery layers

**Agent session**:
A persisted native conversation owned by one Agent target, including its visible
messages, tool activity, and native metadata. It is not a Session binding, a
login session, or a tmax-managed resource.
_Avoid_: Session binding, conversation route, transcript configuration

**Agent session store**:
The version-qualified native location or database that contains Agent sessions
for one Agent target. It is resolved from native home/profile and session-root
selectors, not inferred from the target ID or a configuration filename, and
remains native-owned.
_Avoid_: Configuration scope, tmax session database

**Session working directory**:
The primary native directory a session records as its working directory. It is
session metadata, not the target's configuration path, the session store path,
or necessarily an existing directory now.
_Avoid_: Target path, session store path, project root

**Additional workspace root**:
A native directory beyond the session working directory that a multi-root Agent
session may access. It is native workspace state, not canonical visible history.
_Avoid_: Secondary cwd, imported project

**Session reference**:
The exact pair of an Agent target identifier or explicitly inspected native scope
and a native session identifier identifying one Agent session. A scope-based
reference is list-only. It is a reference value, not a session ID shared across
Agents or a resumable conversation identity.
_Avoid_: Bare session ID, slash-joined session name, cross-agent session ID, gateway session

**Session import**:
Creation of a new native Agent session in a destination Agent target from one
source Agent session, without modifying the source. It is not synchronization,
backup, migration, or resumption of the source.
_Avoid_: Session sync, session migration, conversation backup

**Import record**:
A bounded tmax-owned provenance record for one completed Session import, used to
detect repeats and show origin. It does not copy the transcript or own the
imported Agent session.
_Avoid_: Transcript copy, managed session, backup

**Calling agent**:
The user's assistant acting through tmax on the user's behalf. It need not be
an Agent target or use an Agent whose actual configuration tmax manages.
_Avoid_: Agent target, built-in tmax assistant

**Agent provider**:
A named provider entry in an Agent target's actual configuration. Each entry
references exactly one configured provider: a direct entry references a
user-managed provider, and a relayed entry references the derived gateway
provider `tmax`. Several entries can coexist
where the agent supports them; they are not separate upstream accounts.
_Avoid_: Provider account, target operating mode

**Login source**:
The application and configuration scope responsible for a provider's sign-in
session and its refresh: tmax where supported, or an explicitly selected Agent
target.
_Avoid_: Account owner, permission role, credential import

**Gateway**:
The local service that forwards requests for concrete models or model aliases using
the selected upstream provider. It can serve clients without managing their
actual configuration.
_Avoid_: Proxy, model proxy, agent operating mode, permission-management service

**Model alias**:
One route name that declares one or more native protocols and one ordered list of
explicit references to existing models, so callers get one shared gateway request
ID per protocol. It does not create a model, establish identical weights, or
define an independently managed account pool.
_Avoid_: Virtual model, provider alias, model entity

**Gateway entry**:
The local Web interface's presentation of one Model alias as a client-selectable
public route name. It is the same persisted alias, not a second resource, a Model,
a proxy, or a permission.
_Avoid_: Proxy model, proxy, gateway model record, virtual model

**Alias candidate configuration**:
A model reference and optional alias-local settings within one model alias,
shared by every protocol the route declares. It has no independent identity or
lifecycle, and does not copy the referenced model.
_Avoid_: Candidate model entity, provider pool, shared model default

**Session binding**:
Runtime state retaining a conversation's selected provider, model, destination,
and authorization continuity. It is not a user-managed configuration resource or
an Agent session.
_Avoid_: Shared account session, model selection configuration, Agent session

**Observed usage**:
Usage measured from requests that passed through tmax. It is not a provider's
account-wide usage report or an authoritative bill.
_Avoid_: Subscription usage, remaining quota

**Usage ledger**:
A bounded local record of gateway-observed usage facts, one record per terminal
gateway request, without request identity, request content, or decision evidence.
It is not request history, an archive, a bill, or a balance.
_Avoid_: Request history, usage database, audit log, telemetry

**Usage record**:
One terminal gateway request's typed usage facts inside the usage ledger. It is
not a diagnostic request record and cannot be inspected as a request.
_Avoid_: Request record, trace, transcript

**Usage report**:
A read-time aggregation over the usage ledger and allowance readings for a stated
period and coverage. It is not a stored total, an all-time history, or a bill.
_Avoid_: Dashboard total, usage statistics, account usage

**Usage coverage**:
The ledger range and known store gaps a usage report covers: evictions, dropped
writes, clears, and unreadable periods. Traffic outside the gateway is unobserved
with an extent that cannot be known. Coverage is not a completeness guarantee.
_Avoid_: Coverage guarantee, full history, complete usage

**Allowance reading history**:
Dated provider quota or balance readings retained for display and insight. It is
not the current quota observation, an eligibility fact, or a remaining balance.
_Avoid_: Quota history as evidence, confirmed balance, current allowance

**Effective price**:
The price and source used to compute one cost estimate from observed usage. It is
not a tariff schedule, a provider charge, or a settled price.
_Avoid_: Provider price, real price, billed rate

**Cost estimate**:
A locally computed money estimate from observed typed usage and a frozen effective
price. It is not a bill, actual spend, a subscription charge, or a remaining
balance.
_Avoid_: Bill, spend, charge, invoice

**Insight**:
A dated, scoped, deterministic statement that derives a claim from a usage report
and proposes at most one explicit user action. It is not a fact, an alert, a
routing rule, or an automatic policy.
_Avoid_: Recommendation engine, anomaly, alert, forecast
