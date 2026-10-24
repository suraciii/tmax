# Model Access Management

tmax manages a person's access to model providers, subscription information,
and the use of that access by local coding agents.

## Language

**Provider**:
A user-configured upstream access instance with a custom model namespace, one
authorization configuration, and optional subscription information. Multiple
accounts for the same service are represented by separate providers.
_Avoid_: Vendor, catalog provider, account pool

**Catalog provider**:
A selectable external directory entry supplying default provider and model
metadata. Several local providers may share it while retaining independent
metadata overrides, authorization, and billing.
_Avoid_: Local provider ID, credential, model developer

**Account identity**:
The upstream user or workspace associated with a provider's authorization and
billing when known, not a separately managed resource inside tmax.
_Avoid_: Local provider ID, credential count, global user account

**Login authentication**:
Authentication established through a provider's interactive sign-in flow,
possibly producing an expiring and refreshable session.
_Avoid_: API-key login, reusable bearer key

**API-key authentication**:
Authentication with a provider-issued key, independent of how the user signed
in to the provider's website to create that key.
_Avoid_: Subscription authentication

**Subscription**:
Optional plan, allowance, and renewal information within a provider, not a
separately managed resource. An API-billed provider has no subscription.
_Avoid_: API key, prepaid balance

**Quota window**:
A provider-defined allowance measured over a period with a provider-defined
reset condition. Its unit and coverage are part of its meaning.
_Avoid_: Balance, local token count

**Reset announcement**:
A source's statement that an allowance reset is planned for a specified time.
It is not evidence that the reset occurred or an individual account recovered quota.
_Avoid_: Confirmed quota, renewal date

**Model offering**:
A concrete model exposed under one provider instance's upstream model ID, with
supported native protocols and capabilities. The same upstream model through
different providers is a distinct offering.
_Avoid_: Model alias, universal model ID, protocol-specific duplicate

**Qualified model name**:
The exact `provider-id/model-id` reference to a local provider instance and its
model offering, independent of wire protocol. It is not a catalog ID, proxy alias, or upstream URL.
_Avoid_: Display label, catalog model ID, proxy model ID

**Effective priority**:
A model offering's preference score at a particular instant, after applying
evaluable user-defined priority rules. Higher scores rank eligible candidates first.
_Avoid_: Routing weight, measured price

**Priority rule**:
A user-defined condition and replacement preference score for one model offering.
It changes preference, not candidate membership, entitlement, or task demand.
_Avoid_: Task planner, quota reservation, inferred optimization

**Rule input**:
A named, provider-scoped source of dated quota or reset-announcement facts for
priority rules, with explicit model coverage and validity.
_Avoid_: Desired priority, universal balance, verified entitlement

**Agent**:
A local coding tool such as Codex, pi, omp, OpenCode, or Hermes Agent, distinct
from a particular installation or configuration of that tool.
_Avoid_: Family, configuration target, provider account

**Agent target**:
One local agent installation, profile, or supported configuration scope that
can be managed independently. It identifies which Agent and which native scope
are being configured.
_Avoid_: Agent type, provider account, calling agent

**Calling agent**:
The user's assistant acting through tmax on the user's behalf. It need not be
an agent target or use an Agent whose native configuration tmax manages.
_Avoid_: Agent target, built-in tmax assistant

**Agent provider**:
A named provider entry in an agent target's native configuration. It connects to
one configured upstream provider or to the tmax proxy. Several entries can coexist
where the agent supports them; they are not separate upstream accounts.
_Avoid_: Provider account, target operating mode

**Login source**:
The application and native scope responsible for a provider's sign-in session
and its refresh: tmax where supported, or an explicitly selected Agent target.
_Avoid_: Account owner, permission role, credential import

**Model proxy**:
The local service that forwards requests for concrete models or model aliases using
the selected upstream provider. It can serve clients without managing their
native configuration.
_Avoid_: Agent operating mode, permission-management service

**Model alias**:
A user-defined request model ID within a native protocol, mapped explicitly to
one or more concrete offerings. Names such as L0 and L1 have no built-in meaning;
an alias names access through the proxy, not a new model or identical weights.
_Avoid_: Virtual model, provider alias, separately managed account pool

**Observed usage**:
Usage measured from requests that passed through tmax. It is not a provider's
account-wide usage report or an authoritative bill.
_Avoid_: Subscription usage, remaining quota
