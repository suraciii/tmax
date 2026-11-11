# Model Access Management

tmax manages a person's access to model providers, subscription information,
and the use of that access by local coding agents.

## Language

**Provider**:
A user-managed upstream access instance containing one authentication configuration,
its models, and optional subscription information. Separate providers can access
the same service or upstream account without implying separate allowances.
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
can be managed independently. It identifies which Agent and which native scope
are being configured. DSH uses one home-wide scope, not separate profile targets.
_Avoid_: Agent type, provider account, calling agent

**Calling agent**:
The user's assistant acting through tmax on the user's behalf. It need not be
an agent target or use an Agent whose native configuration tmax manages.
_Avoid_: Agent target, built-in tmax assistant

**Agent provider**:
A named provider entry in an agent target's native configuration. It connects to
one configured upstream provider or to the tmax gateway. Several entries can coexist
where the agent supports them; they are not separate upstream accounts.
_Avoid_: Provider account, target operating mode

**Login source**:
The application and native scope responsible for a provider's sign-in session
and its refresh: tmax where supported, or an explicitly selected Agent target.
_Avoid_: Account owner, permission role, credential import

**Gateway**:
The local service that forwards requests for concrete models or model aliases using
the selected upstream provider. It can serve clients without managing their
native configuration.
_Avoid_: Proxy, model proxy, agent operating mode, permission-management service

**Model alias**:
A named configuration within one native protocol that gives explicit references
to existing models a shared gateway request ID. It does not create a model,
establish identical weights, or define an independently managed account pool.
_Avoid_: Virtual model, provider alias, model entity

**Alias candidate configuration**:
A model reference and optional alias-local settings within one model alias.
It has no independent identity or lifecycle, and does not copy the referenced model.
_Avoid_: Candidate model entity, provider pool, shared model default

**Session binding**:
Runtime state retaining a conversation's selected provider, model, destination,
and authorization continuity. It is not a user-managed configuration resource.
_Avoid_: Shared account session, model selection configuration

**Observed usage**:
Usage measured from requests that passed through tmax. It is not a provider's
account-wide usage report or an authoritative bill.
_Avoid_: Subscription usage, remaining quota
