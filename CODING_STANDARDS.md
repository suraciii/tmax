# Coding review standards

This file contains reviewer-only judgment rules. Read it when a change touches a
data structure, serialized document, default, enum wire value, or module
boundary.

## Data structures and wire models

A review is complete only when it names the authoritative owner, the affected
wire behavior, the consumers that depend on it, and the evidence that
compatibility is preserved or intentionally changed.

- Identify the domain owner before reviewing an extraction or redesign. Keep one
  typed model authoritative. An adapter may translate to a wire model, but it
  must not create a second competing parameter vocabulary.
- Follow the owning capability specification and design document before
  evaluating fields, defaults, enum names, optionality, or nested boundaries.
  Product behavior belongs in `spec.md`; ownership and invariants belong in
  `design.md`.
- Treat Serde attributes as wire behavior. Review `deny_unknown_fields`,
  `default`, `skip`, renames, custom deserializers, omitted fields, JSON `null`,
  and version migrations together with their consumers.
- Trace every structural change across runtime parameters, CLI sources, presets,
  state, recipes, reports, and generated artifacts when those surfaces consume
  the type. A compile pass does not prove wire compatibility.
- Prefer a narrow domain module over a line-count split. A child module must own
  a coherent data boundary and must not duplicate validation, defaults, or
  serialization rules already owned elsewhere.
- Preserve unknown-field rejection, omitted-field behavior, nullability, numeric
  precision, enum spelling, and schema-version policy unless the governing
  specification changes in the same patch.
- Use existing round-trip, rejection, migration, and real-consumer tests. Add a
  permanent test only for a consumer-visible boundary or an uncertain
  compatibility edge; do not test field copies or serializer wiring in isolation.

## Navigation pointers

Use these pointers to reach the authoritative source for the affected model:

- CLI command surface: `crates/tmax-cli/src/` and `specs/cli/`.

## Verification evidence

Run the relevant commands from [Justfile](Justfile). Use `just ci-pr` as the
first local gate. Gate cost and long local gate rules live in
[eng/testing.md](eng/testing.md).
