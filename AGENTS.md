# Agent Instructions

## Project

tmax manages locally named model-provider instances, each with authentication and
optional subscription information, for local coding agents. Its surface is a CLI and local Web interface.

- Before changing domain terms or product behavior, read [CONTEXT.md](CONTEXT.md)
  and the governing [product specification](specs/product/spec.md).
- [specs/](specs/AGENTS.md) — product contracts: behavior, boundaries, and invariants.
- [docs/](docs/README.md) — user-facing guidance, build, and verification.
- [eng/](eng/) — architecture, testing, and documentation-language policies.
- [CODING_STANDARDS.md](CODING_STANDARDS.md) — reviewer judgment rules.
- [CONTRIBUTING.md](CONTRIBUTING.md) — the contribution process and its gates.
- [attribution/NOTICE](attribution/NOTICE) — provenance of the adapted engineering conventions.

## Engineering

- Follow KISS and YAGNI. Reuse existing boundaries and dependencies.
- Keep each rule in one authoritative implementation.
- Preserve observable behavior and public contracts.
- Require measured evidence for numerical changes.
- Put durable non-contract conclusions in the relevant issue or pull request.
  Never commit transient artifacts, even if ignored.
- Write tests that catch meaningful failures. See [eng/testing.md](eng/testing.md).

## Verification

- Use the commands in [Justfile](Justfile) and the toolchain documented in
  [README.md](README.md#build).
- Before handing off code, run `cargo fmt --check`, the relevant checks, and the
  relevant tests.
- Exercise changed behavior through the real entrypoint.
- Report failures, environment limits, and evidence scope.

## Language

Write durable documentation in English. See
[eng/documentation-language.md](eng/documentation-language.md).
