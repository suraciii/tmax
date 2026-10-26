# Workspace boundaries and ownership

This page records the production runtime boundary and feature ownership within
the existing crates.

## Runtime boundary

Program execution enters through the `tmax` binary. `tmax-cli` owns Clap grammar,
offline help, presentation selection, and process exit. `tmax-core` owns the
shared domain, semantic validation, persistence, operations, and resident runtime.

The shared facade is `execute(Request) -> Response`. A request contains a finite
command path, grammar-ordered operands, canonical named arguments, optional
dry-run, and an optional file revision guard. It is not a configuration-document
or shell-command interface. CLI and Web call the same operation owner. The
validated `Config` types own persisted field semantics; the command catalog and
finite editable-field definitions also drive Web controls.

## Feature ownership

- `tmax-cli/src/` owns command grammar and presentation, not business rules.
- `tmax-core/src/config.rs` owns typed desired configuration, validation, content
  revisions, and comment-preserving save constraints.
- `operations/` owns finite resource mutations and effect/result boundaries.
- `provider_integration.rs` owns upstream endpoint/operation declarations and
  explicit outbound transport; `catalog.rs` owns downloaded versus accepted metadata.
- `routing.rs` and `inputs.rs` own exact priority evaluation, scoped facts,
  coherent revisions, and guarded publication.
- `observations.rs` owns bounded retained provider facts and separate latest
  attempts, guarded by provider incarnation, authorization, endpoint/source
  contract, and captured publication revision. Renames preserve the identity;
  removal/recreation and authorization changes cannot revive old results.
- `agent_integration.rs` and `agent_integration/` own Agent-specific configuration
  schemas, credential handoff, supported login owners, conflict detection, and
  protected three-way restoration. The private `agent_integration/installation.rs`
  module owns executable resolution, scoped command construction, and version
  evidence, including safe Hermes provenance probing. Application, authentication,
  and recovery keep their existing policy and persistence ownership.
- `storage.rs` and `secrets.rs` own OS-home resolution, current-user-restricted
  plaintext secret/recovery files, safe inventory, and protected channels.
- `service.rs` and `service/` own resident identity/lifecycle and browser protections.
- `gateway.rs` is the stable public gateway facade and owns the shared runtime
  budget declarations. Private feature owners under `gateway/` separate:
  - `runtime.rs`: loaded snapshots, credential acquisition, bounded admission,
    cancellation, and request lifecycle.
  - `bindings.rs`: destination selection, retained session continuity, explanation,
    and guarded activation. `bindings/store.rs` alone owns the persisted binding
    representation, historical normalization, bounded IO, and initialization.
  - `forwarding.rs`: request authentication, same-protocol HTTP forwarding, streamed
    delivery, and model publication. `errors.rs` owns finite safe error responses.
  - `client_identity.rs`, `native_context.rs`, and `opencode_session.rs`: qualified
    native context, header handoff, and bounded conversation attribution.
  - `capabilities.rs`: finite request/model capability checks.
  - `probe.rs`: explicit fixed-destination diagnostics without resident startup.
  - `response_observer.rs`: bounded JSON/SSE inspection and history evidence
    finalization. Probes reuse inspection while retaining their distinct result
    and usage-state projection.
  Feature tests live with their owners; `test_support.rs` supplies shared fixtures
  only and is compiled only for tests.
- `diagnostics.rs` owns bounded in-memory request evidence and history retention.

Configuration save, service activation, OS startup registration, and native
application remain separate observable failure boundaries. No module introduces
another desired-state authority or protocol-conversion layer.

Retained session changes are staged, persisted, and only then published in RAM.
A failed binding-store write leaves the previous live bindings authoritative;
blocked sessions never bypass that state after a session-to-stateless edit.
Dry-run uses the same blocker rules over the resident or durable retained state,
returning bounded opaque handles rather than native session identifiers. It does
not publish bindings, reserve work, resolve secrets, or initialize storage.

Provider observation time is not an invented expiry. The declared DeepSeek
balance source establishes a dated, exact-as-of observation, not a validity
interval; subsequent routing evaluations report temporal qualification unknown.
Manual subscription metadata does not establish API balance, account ownership,
or entitlement. Failed refreshes retain the prior fact with a distinct latest
attempt. Discovery observations never register concrete models implicitly.

Inference HTTP connections use `TCP_NODELAY` so separate response headers and
streamed body chunks do not wait for Nagle buffering and a peer's delayed ACK.
The listener rejects a connection if it cannot configure that socket option.
The upstream client remains shared by the loaded snapshot, and response bodies
remain pull-driven; this transport setting does not change admission limits,
inspection bounds, cancellation, or forwarding semantics.

Selection without a native binding key does not acquire the session-binding
mutex. Requests with a key retain serialized binding checks and durable writes.
The loaded-snapshot authority still excludes configuration activation during
selection.

## Local verification boundary

Real CLI and browser workflows are exercised under a dedicated passwd-backed OS
account, never through a production `HOME` override. Installed agent parser,
application/restoration, and synthetic loopback session/protocol tests are
separate qualifications from provider-authorized hosted inference. Native
configuration application may correctly require a native reload; it does not
claim that a running native process already adopted the change.

The runtime admission limit is 16 concurrent requests with no waiting queue and
a 4MiB request-body limit. A local-fixture run of 17 held requests observed 16
accepted and one HTTP503, and a 5MiB body was rejected with HTTP413. Explicit
cancel-active interrupted one observed active request; ordinary stop refused it.
The separate 16KiB downstream-chunk and 64KiB response-inspection limits are
implementation bounds. A deterministic history measurement retained 256 records,
accounting for 2,163,456 resident-record bytes and 417,096 serialized bytes, below
the 32KiB-per-record / 8MiB-total accounting budgets. This accounting includes
owned capacities and conservative JSON-entry overhead, not total process RSS.

## See also

- [Documentation index](../docs/README.md).
- [Test architecture](testing.md).
- [Documentation language](documentation-language.md).
