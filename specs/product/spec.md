# tmax product specification

Shared terms are defined in [CONTEXT.md](../../CONTEXT.md). This document defines
the target product, not the behavior already available in the executable.

## Vision

tmax lets individual developers control and reuse their model access across
coding agents, and use their existing services and subscriptions more fully
through user-defined dynamic priorities.

The primary users have multiple coding agents and multiple model services or
subscriptions. They must be able to reuse supported access without reconstructing
its setup for each tool. Reuse remains subject to actual agent capabilities and
provider restrictions; a subscription is not universal API access.

Users decide the rules, either directly or through their own calling agents.
tmax must expose rule-management capabilities through its CLI and document their
meaning so any calling agent can understand and use them. tmax executes configured
rules automatically as their inputs change and must explain the resulting
priorities and their basis. It does not infer the user's preferences or author a
consumption strategy.

Rules must support priorities driven by remaining quota, reset announcements,
and time-dependent prices. Representative user intentions include:

- Prefer models using a Codex subscription when an applicable reset is announced.
- Prefer models using a GLM subscription when much of its weekly allowance
  remains and the window is close to resetting.
- Lower GLM or DeepSeek model priority during a period the user identifies as
  more expensive.

These are possible user rules, not default policies, current tariff claims, or
permission to expand the configured candidate set or move an established session.
The user decides how to use the resulting access. tmax must not manufacture work
to consume an allowance, decide which tasks are worth doing, or promise optimal
spending or complete quota consumption.

tmax must read supported information sources and accept information explicitly
registered by the user or their calling agent, with its source distinguished.
Codex reset announcements can come from services such as
[Codex Resets](https://codex-resets.com/) or explicit registration. An announcement
is not proof that a reset happened or that an individual account's quota recovered;
passing the announced time does not establish either fact.

If a required input is missing, stale, or otherwise unreliable, the dependent
rule must not affect priority. Other evaluable rules and the base priority remain
available, and the reason must be visible. Unknown data is not zero quota and
does not by itself disable a provider.

The product promise is complete rule-management capabilities, accurate execution,
and explainable results, not an autonomous assistant or an inferred optimizer.

## Purpose and scope

tmax lets one person manage model access through locally named provider instances,
including multiple accounts for one service, authentication, billing information,
and access for local coding agents. It is CLI-first, with a supplementary local
Web interface. Users can manage native model-provider settings, run a local
model proxy, or combine both, with each operation reporting its actual effect.
Any calling agent able to run local commands may use tmax; it does not need a
tmax-specific runtime integration. The calling agent interprets the user's goal
and chooses operations. tmax supplies explicit capabilities, checks its domain
invariants, executes requested changes, and reports evidence. It must not embed
an assistant, infer an operation sequence, or require a wizard or approval system.

Native configuration management must support Codex, stock pi, omp, OpenCode,
and Hermes Agent as distinct Agents. This list does not restrict which
assistant may call tmax. The initial provider scope includes
[Codex subscription access](../integrations/codex-subscription/spec.md),
[OpenCode Go](../integrations/opencode-go/spec.md),
[Kimi Coding Plan](../integrations/kimi-coding-plan/spec.md), and the
[official DeepSeek API](../integrations/deepseek/spec.md).

The user must be able to answer four questions: which access do I have, which
agent uses it, what will a change affect, and what quota or renewal information
is actually known?

Model management must support concrete models mapped directly to agent settings,
user-assigned tiers and rule-driven dynamic priorities, and model aliases backed
by one or more sources. Alias IDs may express tiers such as L0-L3 or uses such as
deepseek-v4.1-flash. [Model selection](../model-routing/spec.md) owns their behavior,
including time-based schedules.

Agent provider configuration and model proxying are independent capabilities.
An agent target may contain both upstream provider entries and a tmax proxy entry
where its native capabilities permit. Neither capability requires use of the other.
The proxy must perform same-protocol forwarding only. It must not translate
Responses, Chat Completions, or Anthropic Messages into one another.

Team administration, remote proxy sharing, subscription purchase/cancellation,
and payment execution are outside this product scope. Selection must stay within
user-configured candidates; inferred global cost optimization and hidden replay
after upstream execution are excluded. The product must not bypass provider
client restrictions or represent a subscription as unrestricted API credit.
There are no local client accounts, roles, permission groups, or per-agent model
allowlists. This is one person's tool, not a multi-user gateway. Native agent
model roles are configuration settings, not access-control roles.

tmax must not provide an uninstall feature or a dedicated uninstall workflow.
Existing stop, autostart, native-restoration, and secret-cleanup operations retain
their independent meanings; none implicitly performs the others or removes the
program. Users may use their own calling agent to uninstall it.

## Configuration authority

tmax owns one personal configuration at `~/.tmax/tmax.toml`, independent of the
calling agent and working directory. All tmax-owned persisted data, caches,
logs, protected secrets, session bindings, and recovery records belong under
`~/.tmax`; request history remains in memory under its retention contract.
Native-agent configuration and native-owned credentials remain in their native
locations. Platform startup registration is an external integration, not a
second tmax data directory.

CLI operations must cover every supported setting without requiring the user
or calling agent to write TOML, JSON, YAML, or another configuration document.
Web operations edit the same desired state. TOML is the persisted representation,
not an input task the caller must complete. Optional manual edits remain possible;
writes must detect concurrent changes and preserve unrelated settings and comments.
The [configuration contract](../configuration/spec.md) owns location, field syntax,
validation, and the separate persistence, reload, and native-application boundaries.

Ordinary configuration contains secret references, not secret values or login
tokens. Usage observations, health results, retained conversation destinations,
and applied-state records are runtime state, not another source of desired
configuration. Neither runtime state nor a models.dev refresh may rewrite user intent.

Native agent configuration is a managed target, not a second authority. tmax must
not provide agent-to-tmax import of providers, models, settings, or credentials.
Inspection is read-only and limited to configuration scope, compatibility, safe
change previews, and conflict detection; native edits must not flow back into tmax.

Executed CLI/Web configuration changes save desired settings and activate the
entire current valid file when the service is running. This includes earlier
manual edits that were pending. Manual edits alone do not activate anything;
explicit reload or a subsequent executed management edit is the activation point.
The operation and its optional dry run must disclose the full change from loaded
state, not only fields named by that command. Invalid activation retains the
prior active snapshot and reports the file as not active. Cold startup must reject
invalid current configuration. Native agent application remains separate;
saving or reloading tmax configuration must not silently rewrite agent files.

An invalid saved configuration must have an explicit CLI recovery path that does
not require authoring a replacement document. Reset is a last-resort operation
only for an existing invalid file: preserve its exact bytes as protected recovery
data before replacing desired settings with empty configuration. Report lost
desired selections and resulting access revocation. Do not change native files,
sign out, delete stored secrets, discard prior recovery information, or move old
sessions to another destination. Recovery artifacts remain inspectable without
exposing secrets and are retained until explicitly discarded; resetting does
not authorize replay of rejected content.

### Backup and restoration boundary

tmax must not provide built-in backup bundles, configuration history snapshots,
or cross-machine synchronization. Invalid-file recovery and native-change
restoration are not backups of a complete working installation.

Guidance for external system backups must identify `~/.tmax` as the tmax-owned
state boundary. A consistent file copy requires stopping the resident service
and all other tmax writers; copying only `tmax.toml` is not a complete backup.
Secret-bearing backup copies require the same protection as their source.
Encrypted secret files alone do not guarantee recovery: their unlock material
or platform protection must remain available. Native-agent files, native-owned
credentials, and OS startup registration are outside that directory.

Restoring external files does not verify credentials, restore upstream account
state, or establish that native settings still match. Startup must validate the
restored authority normally and retain unknown or unavailable evidence where
dependencies cannot be established. Machine-bound protection and native paths
may prevent reuse on another machine. tmax must not promise portable recovery,
replay requests, or overwrite current native settings as a side effect of startup.

## Providers and authentication

A user must be able to add, name, inspect, disable, reauthenticate, and remove a
provider. Each provider represents one independently managed upstream access
instance, with one authorization configuration, service integration, region/endpoints,
native protocols, and model settings. A second account or subscription for the
same service is another provider, not a nested account or a second credential resource.

Each provider must have a unique user-editable local ID and a separate display
name. The local ID qualifies model references and is independent of the optional
models.dev catalog-provider ID and integration type. Several providers may share
one catalog entry without sharing credentials, subscription data, or model priorities.
Catalog behavior is defined in [Model catalog](../model-routing/spec.md#model-catalog).

Provider creation must begin with selecting provider metadata from models.dev,
or choosing explicit custom setup when no suitable entry exists. Selected metadata
supplies defaults for the provider and its models; the user must be able to override
supported fields at provider or individual-model scope without re-entering the
complete definition. The metadata selection, local ID, and authentication are
separate settings. Setup must show inherited values, local overrides, and the
effective result, with an action to remove an override and restore inheritance.

Custom setup must expose locally supported integration choices without requiring
a catalog entry, catalog download, credentials, or a running proxy. Each choice
must identify its stable integration ID, native protocols, base-URL convention,
required setup fields, and supported authentication methods. These declarations
do not verify an endpoint or grant a subscription entitlement.

Endpoint selection follows the shared
[endpoint-resolution contract](../integrations/spec.md#endpoint-resolution).
Known services use one integration-owned region/protocol mapping for direct
configuration and proxy forwarding. Custom services use explicit per-protocol
bases within the same provider. A catalog URL is not universal protocol support;
missing or ambiguous endpoint selection must remain unavailable.

Setup must accept a custom ID or suggest the catalog-provider ID when available.
If that suggestion is already used, it must suggest the first unused numeric
suffix starting at 2: deepseek, deepseek-2, deepseek-3. Disabled instances still
occupy their IDs. An explicitly entered duplicate must be rejected, not silently
renamed. Creation must enforce uniqueness even for concurrent submissions.
Custom providers without a catalog entry require an explicit local ID.

Adding or removing instances must never renumber others. Changing a display name
must not change model references. An explicit local-ID rename must update
dependent configuration references together, without compatibility aliases or
retargeting established sessions. A collision must leave the existing references
unchanged. Alias here means the chosen local ID, not a separately managed object.

Authentication is configured inside the provider. Login sessions and API keys
must not require separate account, connection, or credential resources. Each
provider has one active authentication method. Login authentication declares a
login source; missing authentication is valid setup but cannot serve inference.
Upstream user/workspace identity is optional metadata, not a setup prerequisite.
Authentication method and billing are independent. Automatic creation or rotation
of upstream API-key secrets is excluded; the shared local proxy key follows
the [model-proxy lifecycle](#model-proxy).

Replacing a key for the same upstream identity preserves the provider. Adding
independently selectable access creates another provider. Different authorizations
may share one upstream billing identity; the number of local providers must not
be used to infer subscription count or multiply an allowance.
A credential update for a named provider needs no separate account declaration.
If upstream identity cannot be established, it remains unknown.
Credential changes invalidate prior authentication and quota observations.
They must not enable continuation of an old conversation without evidence that
the original authorization and destination remain safe to use.
A known different identity requires a separate provider.

Login authentication and API-key authentication must be separate workflows:

- Login must use a supported provider or native-agent sign-in flow. tmax must
  show which application owns the resulting session and refreshes it, when that
  flow changes active native authorization, and which shared consumers it affects.
  Cancellation or failure before any native change must preserve existing
  providers and selections. If native authorization changes before identity
  validation or local commit completes, or an interruption leaves that effect
  unknown, report the partial or uncertain outcome and affected scopes.
  Retaining old configuration is not proof that old authorization survived.
  Block affected tmax-managed use until the selected authorization is reestablished
  through a qualified recovery flow. Direct native consumers may already be
  affected and must not be described as protected by that block.
  A known different identity must not be committed to the existing provider.
  Promise unchanged native authorization only for a qualified isolated or staged
  flow; otherwise disclose the switch and recovery boundary before sign-in.
  If the native effects or a safe recovery boundary cannot be qualified, block
  that sign-in path rather than invent token copying or an automatic rollback.
- API-key setup must accept a supplied key or a supported secret reference.
  Signing in to a provider website to create a key does not make this provider
  login-authenticated. Replacing a key for the same upstream identity must preserve
  the local provider identity and show affected agent configurations and proxy routes.

A credential must have one refresh owner. tmax must not copy one refresh token
into multiple independently refreshing agent stores. Native-agent sessions may
remain native-agent-owned. A proxy route may use such a session only through an
explicitly supported forwarding contract. Technical token readability is not
proof of credential portability or permitted use.
The [provider-specific contracts](../integrations/spec.md#provider-specific-contracts)
own authentication methods, supported login sources, and consumption conditions for each
service product. A native command named `login` may only store an API key; its command
name does not determine tmax's authentication method.

Provider inspection must follow the [diagnostics contract](../diagnostics/spec.md):
configuration, authentication, model reachability, and quota freshness are
separate facts. Checks must disclose their scope and evidence; any inference
probe requires an explicit action with its cost risk shown before execution.

Disabling a provider must stop its new tmax-managed use without disabling other
instances of the same service. It must identify direct agent configurations that
must be changed; it is not upstream credential revocation. Removal must identify
affected agent configurations and proxy routes before applying changes. Stopping
agent configuration management, deleting a local credential, signing out, revoking
an upstream key, and cancelling a subscription are distinct operations. tmax must
not silently perform one as a side effect of another.

Removal must be rejected while the resulting configuration contains references
to the removed provider, offering, model alias, or login-source target. Show
those references and let the user explicitly remove or replace them before
retrying; do not cascade-delete dependents or select replacements automatically.
Removing a provider leaves native agent files and stored credentials untouched.
The result must identify retained secret references and affected native targets,
including direct access that can continue outside tmax. Local-secret deletion is
a separate explicit action and must reject references still used by desired or
loaded configuration. Deleting a stored key does not revoke copies held by agents
or the upstream service.

### Login source selection

The [CLI authentication contract](../cli/providers/authentication/spec.md)
owns command operands. Non-executable source-selection contract:

```text
on requested provider sign-in:
    if an explicit login source was supplied:
        source = supplied source
    else if the provider has configured login authentication:
        source = saved login source
    else:
        block before effects; report missing source and supported choices
    if source or its native scope is unsupported:
        block this source; do not choose an alternative
    use source's supported sign-in under the authentication failure boundary
```

No source is inferred from a provider ID, metadata, installed Agents, or an
ambient login. tmax is a source only when explicitly selected or already saved
and supported by the integration. A source change must not reuse a session
reference from the previous source. Native credential stores and refresh
responsibility remain with the selected source; this is not credential import.

### Credential refresh coordination

For a login session whose refresh owner is tmax, all consumers must share at most
one refresh in flight for the same authorization generation. This includes
concurrent inference and observation operations and separate CLI/service callers.
A refresh has its own finite deadline under
[Transport deadlines](#transport-deadlines); it must not inherit an unlimited
inference lifetime or hold other callers indefinitely.

Publication must compare the provider identity, authorization generation, and
credential revision captured before refreshing. Re-login, logout, removal,
authorization replacement, or another committed credential change invalidates
the old publication. A late result must not restore revoked access or overwrite
new credentials. Waiters receive the shared result only while their captured
authorization remains applicable; a failed refresh is not successful access.
Cancelling one waiter must release that waiter without cancelling work still
needed by others. Refresh coordination must not introduce periodic refresh work,
retry loops, or replay of an inference request already dispatched.

Native-agent-owned refresh remains with that native owner. tmax must use its
qualified operations rather than copy refresh tokens or introduce a competing
refresh owner.

## Billing, quota, and renewal information

A subscription-backed provider must contain its subscription information, with
at most one subscription per instance in this product scope. Two OpenCode Go
subscriptions are two providers, such as opencode-go and opencode-go-2, with
separate authorization, quota windows, and renewal dates. The product must not
require a separate subscription resource or linking workflow. An API-billed
provider must show applicable usage or balance without inventing a subscription.

Subscription information may be entered before authentication is configured.
Replacing a key for the same upstream identity, signing out, or clearing
authentication must preserve the provider and its subscription information.
Missing authentication may make observations stale; it must not imply that a
subscription ended. Removing a provider removes its local settings and subscription
information without affecting other instances or cancelling the upstream subscription.

Upstream user/workspace identity must scope billing observations when known.
Unknown identity must remain unverified. Authenticating as a different upstream
identity must create a separate provider rather than silently retaining another
identity's plan or quota. Multiple agents or routes using one provider must not
duplicate its allowance; separate local IDs do not prove separate billing scopes.

The provider detail view must include plan name, billing period, renewal or expiry
date, recurring charge and currency, and auto-renewal state when known. These
fields may come from a supported provider source or explicit user entry.
Cross-provider overviews must retain instance IDs and known upstream billing
scope; grouping by service is a view, not a parent resource. A prepaid API balance
must remain a balance. Credential expiry, quota reset, and subscription renewal
must be separate dates.

Every quota or balance observation must identify its provider instance, upstream
billing scope when known, source, retrieval time, coverage, unit, and applicable
window or reset condition. A value must distinguish fresh, stale, unavailable,
and manually entered information. Unavailable is not zero, and unknown auto-renewal
is not disabled auto-renewal. Different billing scopes, windows, currencies, model
pools, or units must not be added into a misleading total.

Provider-reported upstream account usage and proxy-observed usage must appear
separately. Local observations must not be presented as an official bill or used
to invent remaining subscription quota. The [diagnostics contract](../diagnostics/spec.md)
owns direct/proxy observation coverage, incomplete usage, and retained-history limits.

A provider dashboard is not a machine-readable usage API. When no supported
query exists, tmax must provide an explicit unknown state, a provider dashboard
link, and optional user-maintained subscription information. The product must
not scrape browser sessions or collect website passwords to fill these gaps.
Quota refresh failure must not disable otherwise usable inference access or
invalidate another provider instance's observations.

Quota observations affect routing eligibility only through the qualified evidence
rules in [Selection and continuity](../model-routing/spec.md#selection-and-continuity).
The [vision](#vision) permits user-defined priority rules over quota, reset, and
price information. Such observations must not invent a rule or rewrite the user's
configured priorities or schedules; evaluating a configured rule changes its
effective result, not the user's intent.

Supplier billing controls remain in the supplier's console. tmax must not manage,
query, or mirror subscription add-on wallets, Extra Usage switches, member budgets,
or spending-cap settings, nor create unknown-state fields for those excluded
features. This does not remove subscription information, quota observations,
renewal information, or an API-billed provider's supported balance query.

Paid continuation is only an upstream billing fact, not a tmax feature: exhausting
included allowance need not stop requests or further charges. A brief provider
note and console link are sufficient; tmax does not promise a spending ceiling.
Routing follows admission evidence, not a model of the supplier's billing controls.

## Rule-driven priorities

Users must be able to declare scoped rule inputs and create, replace, order,
enable, disable, inspect, and remove finite priority rules through the CLI.
Rules belong to concrete model offerings, not another account pool or a model
alias's private override hierarchy. [Model selection](../model-routing/spec.md)
owns predicates, ordering, calendar fallback, and continuity.

Quota and reset-announcement rules must use declared, dated inputs rather than
guess facts from a model name or a subscription label. User-entered information
must remain distinguishable from fetched evidence. The
[input contract](../model-routing/inputs/spec.md) owns source selection, explicit
recording and refresh, freshness, replacement, withdrawal, and supported sources.
Input definitions are desired configuration; their observed values are runtime
facts and must not be copied into the configuration document.

Automatic acquisition is an explicit per-input choice while the local service
is running. It must not create another daemon, start the service, perform
interactive authentication, or make inference wait for a network refresh.
Without a service, explicit refresh and recording remain available, and
automatic acquisition is visibly paused. Status and selection previews remain
effect-free. Disabling acquisition must not extend an existing fact's validity.

Changing a rule or publishing an input changes future priority evaluations,
not native files, candidate membership, or a retained conversation destination.
Direct configuration may inspect the current ranking, but changing its selected
concrete model remains an explicit operation. The same captured inputs used for
selection must explain the outcome, including rules skipped for unavailable
facts; later observations must not rewrite that historical explanation.


## Agent configuration

The user must be able to discover or explicitly locate an agent target and
inspect its Agent, version, profile, effective configuration locations,
and configuration scope. Detection must be read-only. It must not create tmax
providers from native settings or collect existing agent credentials.

For each target, the user can configure named native provider entries, their
model choices, a default model, and any native model roles. An entry refers to
one upstream provider or to the tmax proxy. Upstream entries select that provider's
concrete models; proxy entries select public concrete model IDs or model aliases.
Both kinds may coexist in the same target. There is no target-wide operating mode.
Unsupported native multi-provider configurations must be explained, not silently
collapsed to one provider. Defaults and native roles identify both the entry and
its model. tmax must not invent universal role or reasoning-level names.
These choices control native configuration and model pickers, not proxy permissions.
Alias continuity belongs to the shared alias definition under
[Model selection](../model-routing/spec.md#selection-and-continuity).
A target may be registered before its model set or default is complete, including
when it is needed as a native login source. Missing setup is an explicit unavailable
state, not a new draft resource. Any declared references must resolve. Native
application remains blocked until its required selections, credentials, and
compatibility are established; incomplete targets do not block standalone proxy use.
Model addition, removal, and default selection are distinct requested changes;
tmax must not infer them from a generic request to switch or optimize access.

Direct model selections must resolve to a specific qualified model and therefore
its provider's authorization. Adding another instance must not switch direct
selections or established proxy sessions. Dry runs of alias candidate
changes must show effects on future selections and any existing sessions that
will be blocked under the [continuity rules](../model-routing/spec.md#selection-and-continuity).
A native account switch must show affected targets and credential scope.

When an agent shares an active account across a profile, process, or server,
tmax must expose that scope and detect conflicting account selections. It must
not promise per-project or per-session isolation that the native agent cannot
provide. It must block incompatible selections and explain whether supported
separate native scopes or proxy routes can provide the requested separation.

The agent configuration view must report whether the requested changes are not
applied, applied, awaiting agent reload, conflicted, or blocked. It must distinguish
saved settings from the running agent's effective configuration. External edits
and higher-precedence settings must be surfaced, not silently overwritten or
claimed as active.

Application state and runtime verification must be presented separately under
the [diagnostics contract](../diagnostics/spec.md). Applied files are not proof
of running-agent use; missing proxy traffic is not proof of bypass.

Support is scoped to an agent version, target, provider entry, authentication
method, protocol, and model capability. Explain unavailable combinations before
changing files. Requests must use the configured concrete offering or an eligible
candidate of the selected model alias; failure must not select an unconfigured
model, provider, credential, or billing product.

## Agent provider configuration

An upstream entry configures inference directly to its selected provider.
No running tmax server is required on that inference path. Supported on-demand
secret resolution may require the installed tmax CLI; disclose that dependency.
A proxy entry uses the tmax listener address and shared access key. Applying it
requires an explicitly started, ready listener; application must not start it.
Adapters supply the native model list through supported discovery or static
configuration. Saving settings and choosing a model never start the proxy.

Every native application must support a redacted dry run showing target files,
credential handoff, affected model selections, and reload or restart requirements.
The actual operation must preserve unrelated settings. Where native secure storage
or references are supported, tmax must use them instead of embedding a secret
into ordinary configuration. Dry-run output and execution results must disclose
any plaintext credential handoff to native storage.

An apply operation must inspect current native inputs and detect changes between
that inspection and its writes. On a conflict it must stop without overwriting
the new content and report what must be inspected. A previous dry run is neither
required nor a stored authorization; execution recomputes from current state.
A failed multi-file operation must report which changes were applied and either
restore its own changes safely or identify a recovery-required state. It must
never report partial application as success.

The user must be able to stop managing an agent's configuration or undo tmax's changes. Restoration must
remove or restore only tmax-owned changes and preserve later user edits. Where
ownership is ambiguous, restoration must stop with a conflict. Backups containing
credentials must receive the same protection as those credentials.

Stopping management removes the target from desired configuration without
rewriting its native files or signing it out. A target referenced as a login
owner cannot be removed until that dependency is explicitly resolved. Removing
a target or one of its provider entries does not revoke the shared proxy key,
change the proxy's models, or stop the service. Existing native settings may
continue to work. Offer restoration before removal, not as an implicit side
effect. Retain protected restore information until restoration completes or the
user explicitly discards it; this is recovery data, not desired configuration.

## Model proxy

The proxy serves compatible local clients independently of native agent
configuration management. No registered agent target or client record is required.
It exposes one shared model list derived from enabled provider offerings and
alias definitions under their declared protocols, not another publication or
permission list. An agent's native model picker can show a subset without
restricting requests to the proxy. Runtime requests do not create configuration.

The first proxy start must be explicit. Once started, it runs in the background
independently of the launching terminal or Web session. Opening management,
saving settings, or applying native configuration does not start inference.
Closing the browser does not stop it. User-login autostart is optional and
disabled until enabled; setup explains the credential unlock requirement.
A locked store produces an actionable blocked state, not an insecure fallback
or false readiness. CLI and Web provide stop and autostart controls. Stopping
reports known affected targets and does not switch them to upstream access;
independently configured clients cannot be enumerated reliably.

Normal stop must refuse while requests are in flight and report the affected
targets and active request count. An explicit force-stop cancels those requests
as far as their transports allow and reports interruption; it must not replay
them. Admission and stopping must be coordinated so a successful normal stop
cannot interrupt a newly admitted request. There is no background drain queue
or automatic force-stop timeout. Stopping does not change the autostart setting.

Agent application state and proxy availability are independent. A stopped or
blocked listener leaves native proxy configuration applied but unavailable;
it must not be reported as an unapplied configuration or silently replaced with
direct access. Restart follows the configuration contract's startup validation
and checks required local credentials before reporting availability. An unavailable
upstream credential blocks its routes; an unavailable shared key blocks proxy admission.
Conversation bindings follow the
[continuity contract](../model-routing/spec.md#selection-and-continuity).

Concrete proxy mappings must be derived from enabled, authorized provider/model
configuration, using qualified `provider-id/model-id` public names. They do not
require another route resource. Concrete requests pin that offering; unknown or
unavailable models must not cause registration, fuzzy lookup, or implicit fallback.

A model alias maps a user-defined request model ID and native protocol to
concrete candidates under [Model selection](../model-routing/spec.md#model-aliases-and-candidate-sources).
It is not a new model, account pool, route resource, or protocol converter.
The [multi-source example](../model-routing/spec.md#multi-source-model-alias-example)
shows different upstream IDs behind one request ID. Concrete proxy access still
needs no alias definition.

Matching protocol names are necessary but not sufficient. Before applying an
agent configuration, the product must check the known transport, streaming, tool-call,
reasoning, context, and conversation-state requirements of the selected agent
against the selected upstream. A known unsupported requirement must block the
configuration change or request with an actionable explanation. The proxy must not strip
unsupported fields to make a request appear successful, fabricate tool results,
or rebuild conversation history on behalf of an incompatible client.

Support must be declared for the actual operations, not just a protocol family.
The shared [operation and resource-identity contract](../integrations/spec.md#supported-operations-and-resource-identity)
governs finite method/path support and requests identified only by an upstream
resource ID. Unknown operations and requests without a reliable original-target
binding must be rejected; a current default or alias selection is not
proof of resource ownership.

Forwarding must preserve request semantics, provider-required session identity,
stream event order, tool-call identifiers and results, upstream error meaning,
and available usage information. Cancellation must end upstream consumption as
far as the upstream transport supports. Alias replacement, routing headers,
and authorized credential handling must not become protocol translation.

Inference request parameters remain owned by the agent's native settings and
request. Provider/model metadata overrides describe capabilities, not sampling
defaults or reasoning choices. tmax must not introduce provider-, model-alias-,
or agent-level request-parameter overrides; the detailed boundary is in
[Request parameters](../model-routing/spec.md#request-parameters).

Each in-flight request must keep its selected concrete or alias ID, provider,
model, and authorization. Configuration changes must expose their impact before
activation. Provider-issued conversation, response, file, or cache identifiers must
not cross authorization or endpoint boundaries. An agent that cannot safely resume
must require a new conversation or remain blocked; the proxy must not emulate
another provider's state store. Ambiguous failures must not trigger hidden
resubmission that duplicates billing or side effects.

One shared local key authenticates inference requests, including on loopback.
Every holder uses the same enabled concrete models and model aliases. There are no
client registrations, target allowlists, or permission grants. Model enablement,
configured candidates, native compatibility, and upstream credential requirements
still determine whether a request can run. The key neither identifies an agent
or conversation nor authenticates management or bypasses browser protections.

Native-login forwarding keeps local-proxy and upstream authentication distinct.
Agent-held tokens must not be persisted or forwarded to an arbitrary host.
Where an upstream requires native session identity, the integration must establish
that identity and use its matching provider credentials. It must not forward one
native session under another account. Missing identity support makes that path
unavailable; it does not justify a local client-permission system.

Initial explicit proxy configuration provisions one unpredictable key in protected
storage and saves its reference. Dry runs describe provisioning without generating
a secret or reserving a reference. Native application reuses this key through the
normal credential-handoff rules. Save, start, reload, model changes, and agent
registration do not rotate it. A missing key is reported, not silently replaced.

Explicit key retrieval supports independently configured clients. It delivers the
key only to the requested protected output or explicit local reveal surface under
the [shared-key command contract](../cli/providers/authentication/spec.md#shared-proxy-key);
ordinary inspection and result JSON never contain it. This is not a generic
upstream-credential export facility.

Explicit replacement updates the shared key reference. Activation rejects the old
key on subsequent requests; in-flight requests retain their captured credentials.
Report known native configurations needing reapplication and explain that manually
configured clients must update too. There is no overlapping-key grace period.
Replacing this local key does not change conversation identities or retained
upstream destinations; later requests must supply the current key.
Removing an agent never revokes this key. Removing proxy configuration stops its
use after dependencies are resolved; stored-secret cleanup remains explicit.
Failures distinguish provisioning, save, activation, and native application.
A generated but unreferenced key remains discoverable for explicit cleanup.

### Resident service identity

One `~/.tmax` authority may have only one resident service owner. CLI, Web, and
autostart must identify the actual service, its authority, process identity,
version, management compatibility, and current listener state before controlling
or reusing it. A PID, port, or stale ownership record alone is insufficient proof.
Concurrent starts must converge on one owner, not create independent writers or
conversation stores.

A repeated start of an already-running compatible inference listener must return
the existing process and actual state without restarting, reloading pending
configuration, or reapplying native files. Starting inference on a compatible
management-only owner is an explicit listener start, not a second resident.
The management interface must remain usable when inference is stopped.
When explicit stop operations leave both inference and Web stopped, the resident
must finish or cancel its owned background work safely and release ownership.
Automatic input acquisition then follows its existing no-service state. An
explicit listener restart remains one operation and must not release ownership
between its stop and start boundaries.

An incompatible service must be reported with the client and service versions
and an explicit stop/start recovery action; the new client must not issue
unsupported state-changing commands. Replacing the executable does not replace
the running service. Its normal stop protections still apply. An unidentified
process occupying the endpoint is a conflict, not a tmax instance to take over.
There is no automatic restart, takeover, termination, or alternate-port search.
Recovery uses a CLI compatible with the running service, or its still-available
Web controls, to perform the existing explicit stops before starting the new
executable. If no compatible control surface is available, report that missing
prerequisite instead of claiming that the incompatible client's stop can work.

On Unix, the control socket pathname under the OS user's home must be
representable by the platform's pathname Unix socket API. An unsupported path
must produce an actionable `resident_control_path_invalid` blocker before
resident launch or owner locks, child creation, or service initialization.
Dry-run reports this known blocker without effects. A path-representation failure
is not endpoint occupation or an unidentified child-start failure. Do not
relocate the store, shorten the path, or choose an alternate control endpoint.

### Local request admission

The proxy must bound active requests, accepted request size, and request/response
buffers. When local capacity is unavailable, reject a new request immediately
with a distinguishable local-capacity result before upstream dispatch. There is
no request waiting queue or local retry/failover. A caller decides whether to
make a new request; rejection must not change model priorities or eligibility.

Streaming must propagate backpressure instead of accumulating unbounded output
for a slow client. Enforce finite buffering even when an operation needs to
inspect a body; reject an over-limit request rather than truncate its semantics.
Client disconnect must release local resources and cancel upstream consumption
as far as the qualified transport permits. It must not imply that billing or
upstream execution stopped. Implementations must measure and document concrete
limits; this contract does not prescribe unmeasured tuning values.

### Transport deadlines

Connection establishment must have a finite deadline covering DNS, TCP, an
explicit outbound-proxy handshake, and TLS as applicable. Noninteractive
credential acquisition and tmax-owned refresh must have finite deadlines of
their own, including a peer that accepts a connection but never finishes its
response. Expiry must release the waiting operation and report the actual stage.
These deadlines do not prescribe a time limit for the user's interactive sign-in.

After inference dispatch, tmax must not impose a default total-generation,
first-token, or text-idle timeout. A reasoning interval without text is not
failure evidence. A connection or credential deadline must not remain attached
as an accidental lifetime limit on an established inference stream. Explicit
caller cancellation and qualified client/upstream deadlines remain observable;
none permits automatic replay or switching the request's destination.

### Outbound network policy

All tmax-owned outbound network operations must use one explicit global policy:
direct access by default, or the configured HTTP/SOCKS proxy. This includes
inference forwarding, credential refresh, catalog retrieval, provider
observations, and public announcement acquisition. CLI and resident execution
must use the same saved/loaded policy rules from the
[configuration contract](../configuration/spec.md).

tmax must not implicitly inherit `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`,
`NO_PROXY`, their lowercase forms, or system proxy autodiscovery. A configured
proxy failure must fail the affected operation rather than retry directly.
There are no per-provider proxy overrides or implicit destination bypass rules.
Proxy routing does not weaken TLS verification or authorize sending provider
credentials to another origin.

Changes affect newly started outbound work, not the route of an already-running
request. Optional dry runs and inspection show the effective nonsecret policy
and saved/loaded differences without contacting the network. The policy does
not rewrite native-agent networking or the networking of a delegated
native sign-in process; those boundaries must be disclosed, not claimed as
tmax-controlled traffic.

## Local Web and CLI experience

The CLI is the primary product entrypoint. `tmax --help` must explain what tmax
can do and list its capability groups. Command help must explain purpose, inputs,
effects, important limits, and how to inspect the result. Help must be usable
without configuration, authentication, or network access. The caller chooses
commands and their sequence; no separate skill, assistant integration, or Web
session is required to complete a supported task.

Management commands return concise JSON by default, including failures and
partial effects. A human-readable format may be requested. Help and version
remain readable text. Ordinary results identify the requested change, affected
objects, and whether it took effect. Detailed reads expose configuration
differences and diagnostic evidence. Failures must report completed changes
and needed recovery; short output must not hide partial or uncertain effects.

Commands and help must describe user actions, not internal storage mechanisms.
Callers must not repeat facts already determined by a referenced object.
Use simple time units for CLI durations and explicit provider-entry choices.
The [CLI contract](../cli/spec.md) owns their exact syntax and result fields.

The supplementary Web interface must have these task-oriented areas:

| Area | Primary information and actions |
| --- | --- |
| Overview | Provider-instance problems grouped by service, agent configuration state, proxy health, stale quota information, upcoming known renewal dates |
| Providers | Metadata selection, local ID, inherited defaults, local overrides and effective values, authentication, model access, subscription or API balance, quota and freshness |
| Models | Concrete qualified offerings with metadata overrides, capabilities, tiers and schedules; model aliases with public names, candidate sources, and selection previews |
| Agents | Five supported Agents, native targets and provider entries, model choices, effective configuration, preview/apply/restore |
| Proxy | Listener lifecycle, shared key, derived concrete-model and alias availability, continuity, health and request metadata |

Provider creation and native provider/model selections are explicit operations.
CLI operations persist their own changes; they do not require a caller-authored
complete document. Sign-in, native application, and proxy startup remain separate
operations; saving declarations must not perform them.
Adding another provider instance must not silently replace an existing agent's
selected provider or default model.
Editing a model alias must not silently rewrite unrelated direct agent configurations.

Every supported management capability must be reachable through the CLI, including
metadata overrides, subscription fields, model schedules, alias candidate sets,
target selections, and recovery. The [CLI contract](../cli/spec.md) owns command
grammar, dry-run behavior, JSON output, and exit semantics. Web actions use the
same operations and result meanings, not a separate business model or parsed
human-readable terminal output. Provider sign-in may use its required interactive
flow. Management HTTP paths remain implementation details.

CLI and Web must expose the same next action for incomplete operations:

| Outcome | Next action |
| --- | --- |
| Provider saved without authentication | Sign in through the supported owner or supply a key; model and subscription editing remain available |
| Saved without a running service | Start explicitly when proxy use is wanted; direct native application remains independent |
| Saved but not active | Fix the reported cause and explicitly reload; a listener-address change requires restart |
| Invalid saved configuration | Inspect the validation error; correct supported fields through explicit operations where possible, or explicitly reset invalid configuration with protected preservation of the rejected bytes |
| Concurrent or conflicting native edits | Inspect current inputs and issue the intended operation again; do not overwrite external changes or treat an earlier dry run as binding |
| Partial native application | Show changed and unchanged targets/files; restore owned changes or issue a new operation based on current state |
| Native login changed authorization before failure or rejected identity | Show affected native/shared consumers and block affected managed use; reestablish the intended identity through the qualified owner, then inspect current state |
| Startup registration failed or drifted | Show saved intent separately from registration; explicitly enable or disable autostart to reconcile this authority's registration |
| Applied but awaiting native reload | Perform the named agent reload/restart; do not reapply blindly |

Results identify the requested operation, affected objects, completed effects,
blocking reason, and supported next action without exposing secrets.
Mutations support `--dry-run` on the same command. It describes current effects,
known blockers, and uncertainty without changing files, reserving resources,
reading secrets, signing in, or sending upstream requests. A normal invocation
executes directly after checking current inputs and constraints; no prior dry run,
confirmation token, approval receipt, or persisted plan is required.
An operation that only changes desired settings may succeed without applying
native files. An incomplete requested activation or application must not return
success. Recovery uses explicit authentication, autostart, reload, apply, and
restore operations, not a workflow engine or automatically scheduled retries.

### First usable agent

An empty installation must support the following complete task through ordinary
CLI operations, with equivalent Web capabilities. This is a documented path, not
an enforced sequence, wizard, persisted plan, or background setup job. The calling
agent chooses operations from help and current state.

1. Discover capabilities through help and inspect current state under `~/.tmax`.
   Missing configuration is empty setup, not a request to write a starter file.
   The first configuration mutation creates valid persistence automatically.
   An existing invalid file is reported with its recovery options, not silently
   replaced by an empty configuration.
2. Add a provider from catalog metadata or explicit custom integration settings.
   Local integration discovery must work without catalog access. Discover or
   explicitly locate the native target and register its Agent and scope.
   A target needed as a login source may exist before models or a default are set.
   These declarations must not sign in or write native files.
3. Supply a key through protected input or invoke sign-in through the selected source.
   Show local credential readiness separately from upstream verification.
   Cancelled or failed actions retain saved definitions for subsequent operations;
   native effects follow the authentication failure boundary. Unknown quota or
   renewal must not block otherwise qualified access.
4. Register concrete offerings and add the target's native provider entries and
   selected models, default, and native roles through CLI operations. A tier may
   be resolved for an inspected target before choosing its concrete default.
   Upstream entries need no proxy or model alias. Proxy entries can use a single
   concrete offering; tiers, schedules, and model aliases are optional.
5. For proxy use, explicitly configure, start, and validate the listener. Apply the target's
   desired configuration as a separate operation, optionally preceded by its
   dry run. Report written changes, outstanding native reload, and runtime
   evidence separately. The user or calling agent may perform the named native
   reload; tmax does not do it silently.
6. Inspect the result. An inference probe is a separate explicit operation with
   its cost risk disclosed, never an automatic setup step. Configuration success
   and model discovery do not prove a working tool-call conversation.

Unsupported combinations must identify their missing prerequisite before native
application. Incomplete providers or targets remain inspectable and editable
through CLI operations. Neither surface requires authoring a configuration
document, importing native credentials, or inventing a workflow record to resume.

Proxy-only use omits native target registration and application. Configure upstream
access and models, configure and start the proxy, and explicitly retrieve its
shared key for the user's client. No client record or permission assignment is needed.

### Local access and secret protection

Management and proxy listeners must remain loopback-only. The Web interface is
a local visual equivalent of CLI operations: it has no login, pairing flow,
management password, or authenticated browser session. Local processes with
access to the listener are trusted; this is not isolation from other OS users
or hostile local software. Remote access and reverse-proxy publication are unsupported.

No-auth management must still reject untrusted Host and Origin values, including
DNS-rebinding and cross-site requests. Management reads must be side-effect-free;
mutations must require same-origin request checks and a non-simple request format,
not a link or cross-origin HTML form. Do not enable permissive CORS. These are
browser-request protections, not a user authentication workflow. The browser must
not persist secrets in local storage or return them through ordinary read responses.
Explicit shared-proxy-key reveal is a separate same-origin user action, not an
inspection response; it must not cache or log the key.

tmax-owned persistent secrets and credential-bearing recovery files must be
protected in encrypted storage under `~/.tmax`, with user-restricted access and
an explicit unlock or supported OS-protection mechanism. Unlock material must
not be persisted beside encrypted data as an unprotected reusable secret.
Locked or unavailable storage must be visible, never a reason to fall back to
plaintext files. Native-owned credential stores remain subject to the explicit
credential-handoff rule and are not imported into tmax.

The [diagnostics contract](../diagnostics/spec.md) owns operational evidence,
request-history coverage, and cleanup. Diagnostics must not expose provider
secrets through status reads or error details. Provider URLs must not be taken
from inference request input, and credential-bearing redirects to another
origin must not be followed.

## Acceptance boundary

Product acceptance requires all of the following, with real supported agent
versions and provider-authorized credentials where a provider request is needed:

1. Keep two OpenCode Go subscriptions as separate provider instances with custom
   or automatically suggested unique IDs, independent authentication, subscription
   details, quota observations, and renewal dates. Changing one must not alter the
   other. Authentication and entitlement follow the
   [OpenCode Go contract](../integrations/opencode-go/spec.md).
2. Record subscription details within a provider before authentication is set up.
   Preserve them across same-identity key replacement and sign-out. Show API-billed
   access without a subscription. Multiple targets, routes, or authorizations for
   one upstream identity must not duplicate its quota or inherit another identity's observations.
3. Configure and restore each of Codex, pi, omp, OpenCode, and Hermes without
   overwriting unrelated or concurrently changed configuration.
4. Use an upstream native entry while tmax is stopped, and a proxy entry while
   the browser is closed. Configure both entries in one supported native target.
   Serve an independently configured client without registering any agent target.
5. Complete streaming output and a tool-call round trip for each advertised
   agent/protocol route. Reject a protocol mismatch before upstream inference.
6. Expose an expired login, revoked key, unavailable upstream, stale usage source,
   and exhausted quota as distinct conditions when the upstream evidence permits.
7. Reject invalid shared proxy keys, unavailable models, and untrusted cross-site
   management requests without leaking credentials or conversation content.
   Valid shared-key holders see the same model list; native picker subsets do not
   impose request permissions. Open the Web interface without login or pairing.
8. Explain unknown quota and renewal information, and keep proxy observations
   separate from authoritative provider balances.
9. Preserve a running request's provider and authorization during a route change,
   and prevent unsafe reuse of upstream conversation state across identity changes.
10. Resolve deepseek and deepseek-2 independently against the same catalog entry.
    Exercise custom IDs, automatic suffix collisions, rename collisions, and
    reference updates without renumbering instances or retargeting sessions.
    Reject conflicting direct selections within a native shared scope; dry runs
    and actual results identify all targets affected by a requested shared-scope switch.
11. Exercise the [model-selection acceptance boundary](../model-routing/spec.md#acceptance-boundary):
    catalog refresh, instance-qualified IDs, tier isolation, schedules, and conversation continuity.
12. Edit desired configuration through CLI, Web, and the text file without creating
    competing authorities. Detect concurrent edits; keep credentials and runtime
    observations out of ordinary configuration. Inspect native agent configuration
    without importing providers, models, settings, or credentials, or overwriting later edits.
13. Keep an explicitly started proxy running after its launching terminal and Web
    session close. Exercise opt-in login startup and a locked credential store;
    do not claim readiness or change to direct access when unlock is required.
    Refuse normal stop with active requests; explicit force-stop reports interruption.
    Preserve retained conversation destinations across a normal stop/start.
    An unrelated direct-configuration edit must not retry failed startup
    registration or fail solely because registration drift remains. Explicit
    autostart controls reconcile tmax's registration without changing unrelated entries.
14. Create two providers from one models.dev metadata entry without duplicating
    its complete definition. Override provider and individual-model fields in one
    instance, restore inheritance by removing overrides, and leave the other
    instance's settings, credentials, subscription, and routing preferences unchanged.
15. Map a concrete offering directly into each supported agent configuration without
    requiring a model alias. Expose concrete proxy access from existing settings
    without storing redundant same-name routes or importing agent state.
16. Configure a scenario model alias with explicit candidates from two OpenCode
    Go instances and official DeepSeek. Keep the agent-visible choice stable while
    new selections use eligible candidates and their exact upstream IDs. Model
    availability must precede native selection; no selection callback may be assumed.
17. Exercise the [configuration contract](../configuration/spec.md): persisted
    field validation, reference checks, explicit reload, saved/loaded/applied state
    separation, invalid-file startup, and concurrent-write rejection. Configure
    every supported setting through CLI operations without authoring documents.
    Preserve native request parameters instead of injecting metadata capabilities.
18. Remove providers and targets only after references are explicitly resolved.
    Distinguish retained native files and secrets from removed desired settings;
    stopping management must not silently sign out or revoke upstream access.
19. Provision and explicitly replace the one shared proxy key. Ordinary reads
    must not expose it; explicit protected delivery supports unmanaged clients.
    Reject the old key after activation, report known configurations needing
    reapplication, and distinguish save, activation, and native-application failures.
    Removing a target must not revoke the key or block independently configured clients.
20. Follow the [provider-specific contracts](../integrations/spec.md#provider-specific-contracts)
    for native login sources, key/product separation, unverified account identity,
    and scoped entitlement failures. Do not treat one agent's login or protocol
    support as universal cross-agent subscription access.
    Distinguish cancellation before native changes from failed or rejected login
    after a shared authorization changed; expose affected consumers and qualified
    recovery without claiming old authorization survived a failed local commit.
21. Complete the first usable-agent path without a model alias, an inferred
    tier, or an automatic paid probe. Resume after cancelled authentication,
    a saved-but-not-active revision, or an agent awaiting reload without
    duplicating providers, overwriting native changes, or repeating completed writes.
    Discover custom integration choices without catalog access, and preview a
    tier for an unsaved native target without a placeholder model selection.
    Register a native login source before model/default completion without native
    writes, implicit login, or proxy startup.
22. Follow the [diagnostics acceptance boundary](../diagnostics/spec.md#observable-acceptance):
    distinguish configuration from runtime evidence, explain historical decisions
    from their original revisions, and clear request history without losing
    conversation bindings or recovery information.
23. Follow the [CLI contract](../cli/spec.md) for offline capability help, optional
    same-command dry runs, direct execution without confirmation tokens, concise
    default JSON, and truthful partial/unknown outcomes. Web is never a prerequisite
    for CLI tasks and must report the same effects and recovery actions.
24. Use the same `~/.tmax` configuration and data from different calling agents and
    working directories. Caller identity does not create a new target or change
    which personal resources are managed. Keep native targets in their own scopes.
25. Configure and inspect quota and reset-announcement rules through explicit
    CLI operations. Exercise the [rule-input contract](../model-routing/inputs/spec.md#acceptance)
    and model-selection acceptance with supplied facts, not invented upstream
    availability. A high remaining allowance near reset and a pending announced
    reset can change new selections according to user rule order. A configured
    expensive-period schedule remains the fallback when no signal rule matches.
26. Withdraw or expire an input, change its source or authorization scope, and
    reject late conflicting publication. Only dependent priority rules lose
    their effect; no forecast restores quota or establishes unavailable access.
    Preserve fixed selections, retained sessions, and original decision evidence.
27. Keep explicit input acquisition usable without a service. Opt-in automatic
    acquisition runs only with the service and available noninteractive access;
    reads, previews, and request admission never trigger acquisition.
28. Apply a management edit after a pending manual edit and disclose the entire
    loaded-to-candidate change. Detect stale positional edits using a revision
    precondition without a confirmation token or persisted execution plan.
29. Discover retained secret references after declaration removal, reset, or
    interrupted provisioning without revealing their values. Unknown reference
    safety remains a cleanup blocker rather than permission to delete.
30. Reject local overload before upstream dispatch, bound buffering for slow
    clients, and distinguish local capacity from upstream throttling. Connection
    and credential deadlines must not terminate valid long-running inference.
31. Use one region/protocol endpoint resolution in both modes; require explicit
    custom protocol bases and reject unsupported operations or unbound resource
    IDs without probing providers or creating a resource directory.
32. Apply explicit global outbound policy consistently in CLI and service work;
    never inherit ambient proxy settings or fall back directly after proxy failure.
33. Converge concurrent starts on one resident owner, reuse compatible active
    listeners without reloading, and reject incompatible or unidentified owners
    without automatic takeover, restart, or port changes.
34. Coordinate tmax-owned refresh across consumers with a finite deadline and
    guarded publication; cancellation and authorization replacement must not
    allow an old result to overwrite the current authorization or replay inference.

An agent's native mode may remain the only supported path for a particular
subscription. Such a limitation must be named, not described as complete
cross-agent support. Shared qualification belongs to
[Agent and provider integration](../integrations/spec.md); each provider contract
owns its provider-specific acceptance boundary and external evidence.

## Implementation status

The workspace includes shared management, configuration, routing/input,
protected-storage, native-adapter, Web, and proxy implementations. Runtime
qualification is scoped to actual versions, native scopes, credentials,
operations, and transports. Local tests or parser checks do not establish
provider entitlement or a verified hosted-provider tool-call conversation.
This specification remains a design and acceptance contract, not a release claim.
