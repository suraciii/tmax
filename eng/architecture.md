# Workspace boundaries and ownership

This page records crate boundaries, module responsibilities, dependencies, and
state ownership. Behavioral contracts belong in [specs](../specs/README.md);
implementation details and verification results do not belong here.

## Crate and entrypoint boundaries

- `tmax-cli` owns command grammar, offline help, presentation, and process exit.
- `tmax-core` owns domain rules, operations, persistence, and the resident runtime.
- CLI and Web use the same core operation boundary. Neither presentation surface
  owns a separate business-policy implementation.
- Configuration types, the command catalog, and editable-field definitions are
  shared authorities rather than independently maintained UI schemas.

## Feature ownership

Paths below are relative to `crates/tmax-core/src/`.

| Owner | Responsibility |
| --- | --- |
| `config.rs` | Desired configuration and its validation. |
| `operations.rs`, `operations/` | Management operations and configuration-change coordination. |
| `provider_integration.rs` | Provider access capabilities and outbound transport. |
| `catalog.rs` | External directory metadata and locally accepted metadata. |
| `routing.rs`, `routing/` | Model priority, eligibility, and selection. |
| `inputs.rs`, `inputs/` | Rule-input facts and shared provider/declaration lineage. |
| `observations.rs` | Retained provider observations and acquisition attempts. |
| `agent_integration.rs`, `agent_integration/` | Agent configuration, authentication ownership, and restoration. |
| `service.rs`, `service/` | Resident ownership, management surfaces, and platform lifecycle integration. |
| `gateway.rs`, `gateway/` | Inference forwarding and retained session continuity. |
| `storage.rs`, `secrets.rs`, `secrets/` | Local persistence and protected credential storage/channels. |
| `diagnostics.rs` | Request evidence and history. |

## Internal feature boundaries

Feature roots expose shared entrypoints. Their implementation modules remain
private to the feature unless another feature has a concrete consumer.

### Operations

- `arguments.rs` and `results.rs` own operation argument interpretation and result
  presentation.
- `configuration.rs`, `inspection.rs`, and `secret_commands.rs` own configuration
  coordination, management views, and secret-management operations.
- `commands.rs` owns the command catalog. `references.rs` owns typed deletion
  dependencies.
- `edit.rs` owns the configuration-edit transaction. Resource modules under
  `edit/` own provider, model, alias, Agent, and service-setting transformations.
- `edit/document.rs`, `edit/fields.rs`, and `edit/references.rs` own document
  editing, editable-field definitions, and rename-reference transformations.

### Resident service

- `control.rs` owns the private management protocol and client/server coordination.
- `resident.rs` owns the resident aggregate, activation, surfaces, and lifecycle.
- `web.rs` owns the browser-facing management boundary and its operation bridge.
- `startup.rs` owns startup-intent coordination. Platform-specific control,
  startup, and Web modules own their OS integrations.

### Model selection

- `routing/calendar.rs` owns calendar semantics.
- `routing/priority.rs` owns model validation and priority evaluation.
- `routing/selection.rs` owns candidate eligibility, advice, and new selection.
- Retained-session continuity belongs to gateway bindings, not to new selection.

### Rule inputs

- `inputs/model.rs` and `store.rs` own retained representations and their codec.
- `lineage.rs` and `lifecycle.rs` own identity relationships and state transitions.
- `exact.rs` and `value.rs` own input-value semantics; `snapshot.rs` owns the
  coherent read view consumed by selection.
- `commands.rs` and `inspection.rs` own management operations and views.
- `acquisition.rs`, `codex_resets.rs`, and `scheduler.rs` own acquisition, the
  qualified public-source integration, and scheduling.

### Agent integration

- `scope.rs`, `discovery.rs`, and `installation.rs` own native scopes and
  installation evidence. `planning.rs` owns desired native projections.
- `application.rs` and `recovery.rs` own native changes and restoration.
- `authentication.rs`, `browser_login.rs`, and `compatibility.rs` own native
  authentication state, browser sign-in orchestration, and compatibility.
- `adapters.rs` and `adapters/` own Agent-specific mappings; `document.rs` and
  `dsh_patch.rs` own native document editing.

### Gateway

- `runtime.rs` owns inference runtime state and request lifecycle.
- `bindings.rs` and `bindings/store.rs` own retained destinations and persistence.
- `forwarding.rs` and `errors.rs` own forwarding and gateway error responses.
- `request_facts.rs` validates the complete JSON request and extracts policy facts
  without retaining message text, attachment payloads, or a full request tree.
  Selection, continuity, reasoning, and provider checks share those facts.
- `request_body.rs` overlays the selected model and resolved effort on borrowed
  raw fields. Unchanged bodies reuse their input bytes; rewritten bodies allocate
  one final output buffer. Duplicate keys and private-number representations use
  recursive compatibility normalization to preserve the former parser's semantics.
  Forwarding releases the input bytes and policy facts before waiting for upstream
  credentials or response headers; the outbound transport owns the emitted body.
- Client-identity and native-context modules own client attribution;
  `capabilities.rs` owns request/model compatibility checks.
- `probe.rs` owns explicit diagnostics; `response_observer.rs` owns response
  evidence projection.

## Dependencies and state authority

Operations coordinate feature owners rather than duplicate their domain rules.
The service hosts management and inference surfaces. Gateway selection consumes
routing; routing consumes configuration, input snapshots, metadata, and shared
compatibility assessments. Platform-specific code stays behind its owning
feature boundary.

Desired configuration is the user's intent. Input facts, provider observations,
loaded snapshots, session bindings, and request history are distinct operational
state, not alternative desired-state authorities. Native Agent files remain
external managed targets.

Configuration persistence, service activation, native application, and OS startup
registration are separate architectural effect boundaries, not one distributed
transaction.

Tests live with the feature or semantic owner they exercise. Shared test support
contains fixtures rather than production policy.

## See also

- [Documentation index](../docs/README.md).
- [Test architecture](testing.md).
- [Documentation language](documentation-language.md).
