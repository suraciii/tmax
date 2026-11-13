# Specifications

This directory holds the product contracts for tmax: what the program must do
and the invariants it protects.

- [Product](product/spec.md) — CLI-first access from any calling agent, provider
  instances, authentication, billing, independent configuration and gateway capabilities, and acceptance.
- [Product design](product/design.md) — ownership, native provider entries, configuration
  safety, operation boundaries, and native-protocol forwarding.
- [Supervised service](service/spec.md) — native process recovery, explicit-stop
  suppression, independent login startup, and platform qualification;
  [service design](service/design.md) owns lifecycle coordination and cutover.
- [Configuration](configuration/spec.md) — `~/.tmax` persistence schema, metadata
  overrides, secret references, and activation; [persisted-format example](configuration/example.toml).
- [Model selection](model-routing/spec.md) — models.dev metadata, concrete model
  references, model aliases, ordered dynamic rules, schedules, and continuity.
  - [Routing availability design](model-routing/design.md) — availability holds,
    quota evidence boundaries, guard policy, and retained-session behavior.
  - [Rule inputs](model-routing/inputs/spec.md) — scoped quota/reset facts,
    manual registration, source acquisition, freshness, and publication.
- [Integration support](integrations/spec.md) — shared agent boundaries, provider
  qualification, and criteria for dedicated provider contracts.
  - [Capability projection design](integrations/design.md) — reasoning declarations,
    explicit gateway effort mappings, shared alias choices, and backend-stable routing.
  - [Codex subscription](integrations/codex-subscription/spec.md) — native login sources and ChatGPT gateway access.
  - [OpenCode Go](integrations/opencode-go/spec.md) — workspace keys, session identity, model endpoints, and subscription billing.
  - [Kimi Coding Plan](integrations/kimi-coding-plan/spec.md) — membership keys, native login, regional access, and permitted usage.
  - [DeepSeek API](integrations/deepseek/spec.md) — stateless Responses, account balance, limits, and pricing evidence.
  - [Domestic GLM Coding Plan](integrations/glm-coding-plan/spec.md) — plan-key categories, approved clients, domestic protocol bases, and monitor evidence limits.
  - [StepFun Token Plan](integrations/stepfun-token-plan/spec.md) — dedicated plan paths, `step-5-preview`, truthful native clients, and Credit/quota boundaries.
  - [MiniMax M Plan](integrations/minimax-m-plan/spec.md) — Subscription Key product boundaries, two regional Messages bases, and display-only quota observation limits.
- [CLI](cli/spec.md) — common invocation, help, protected input, and dry runs.
  Command syntax and detailed results live with their owning capability:
  - [Configuration](cli/configuration/spec.md) — saved settings, reload, and invalid-file recovery.
  - [Providers](cli/providers/spec.md) — instances and endpoints; nested authentication and rule-input commands.
  - [Models](cli/models/spec.md) — concrete models and explanations; nested catalog, model-alias, and priority-rule commands.
  - [Agents](cli/agents/spec.md) — targets, model choices, native application, and restoration.
  - [Service](cli/service/spec.md) — outbound network, gateway, Web, and nested request diagnostics.
  - [Results](cli/results/spec.md) — compact JSON and exit codes shared by all commands.
- [Agent session portability](sessions/spec.md) — read-only native session
  discovery and one-way native session import; [design](sessions/design.md)
  owns canonical mapping, fidelity, and native visibility boundaries.
- [Diagnostics](diagnostics/spec.md) — scoped status evidence, historical selection
  reasons, bounded in-memory request history, and persistent gateway error logs.

Shared vocabulary lives in [CONTEXT.md](../CONTEXT.md). The executable and shared
runtime implement management, configuration, native adapters, Web, and gateway
operations. These specifications remain the acceptance authority, not evidence
that every provider, native version, permission, or transport has been verified.

See [AGENTS.md](AGENTS.md) for how specifications are written and owned.
