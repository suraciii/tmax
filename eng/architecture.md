# Workspace boundaries and ownership

This page records the production runtime boundary and feature ownership within
the existing crates.

## Runtime boundary

Program execution enters through the `tmax` binary. The binary owns argument
parsing and exits with the status the command surface specifies. It holds no
library code that another consumer depends on yet.

When the workspace gains a library crate, this page records the boundary: the
facade the binary calls, the types that own parameters, and the stages that share
one derivation. Until then, `crates/tmax-cli/src/main.rs` is the whole runtime.

## Feature ownership

Feature ownership stays inside a crate:

- `crates/tmax-cli/src/` owns the command surface, its argument types, and its
  exit statuses.

New capabilities follow the same rule: one owning module per capability, and one
authoritative home per rule. A capability that gains a product contract lives in
`specs/`; see [specs/AGENTS.md](../specs/AGENTS.md).

## See also

- [Documentation index](../docs/README.md).
- [Test architecture](testing.md).
- [Documentation language](documentation-language.md).
