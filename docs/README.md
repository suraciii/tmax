# Documentation

These pages explain how to build, run, and verify tmax. They describe the project
as it is used; they do not define product contracts.

Contracts live in [../specs/](../specs/AGENTS.md). Engineering practices live in
[../eng/](../eng/architecture.md).

## Guides

- [CLI usage](usage.md) — provider/model setup, manual input and rule previews, proxy-only use, and the explicit Pi apply/restore path.
- [Building and verification](building.md) — dependencies, build commands, the
  verification gates, and the evidence format.

## Evidence

`just verify <dir>` and `just verify-pr <dir>` retain gate output with source
provenance. The evidence directory must be outside the repository; see
[Building and verification](building.md#verification-evidence).
