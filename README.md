# tmax

See the [product vision](specs/product/spec.md#vision) for intended users,
cross-agent access reuse, and user-defined dynamic priorities.

tmax manages a person's model access through custom-named provider instances,
each with authentication and optional subscription information. Multiple accounts
for one service have distinct IDs and use models.dev metadata with local overrides.
The target design is CLI-first: any user's agent can discover capabilities through
help and compose explicit operations. Management output defaults to concise JSON,
changes support `--dry-run`, and tmax-owned configuration and data live under
`~/.tmax`; no authored configuration document is required. A supplementary local
Web interface exposes the same operations. Agent-native provider configuration
and same-protocol model proxying are independent capabilities. One native target
can use upstream providers and tmax together; the personal proxy uses one shared
key and the model list derived from enabled concrete models and model aliases.

**Runtime status:** the executable currently implements only help and version
output. Management, Web, and proxy behavior are specified but not implemented.
Start with the [product specification](specs/product/spec.md),
[design boundaries](specs/product/design.md), and [domain language](CONTEXT.md).

## Build

The build requires Rust stable (1.88 or newer), `just`, Python 3.10 or newer,
and `git`. The workspace has no native dependencies and no system libraries
beyond a C toolchain.

```bash
just build
```

This produces `target/release/tmax`.

## Run

```bash
just run --help
just run --version
```

The CLI prints help with no arguments and rejects unknown arguments.

## Verification

`just ci-pr` is the pull request gate and `just ci` is the full gate. Use
`just verify-pr` or `just verify` to retain their output and source provenance
outside the repository. See [Building and verification](docs/building.md) for
the commands and evidence format.

## Repository layout

```
crates/
  tmax-cli/            the `tmax` binary and its command surface
eng/                   architecture, testing, and documentation-language policies
specs/                 product contracts: spec.md, design.md, per capability
docs/                  user-facing guidance
scripts/               verification and repository hygiene tooling
attribution/           provenance of the adapted engineering conventions
```

## Scope

The [product specification](specs/product/spec.md) owns the target scope and
non-goals. The workspace's engineering conventions were adapted from
SpektraFilm-rs; its film simulation, image data, GPU/native dependencies, and
packaging code are not part of tmax.

[attribution/NOTICE](attribution/NOTICE) records the source project, the pinned
reference checkout, and the license terms that the adapted material inherits.

## Documentation

- [specs/product/spec.md](specs/product/spec.md) — target product behavior and acceptance.
- [specs/configuration/spec.md](specs/configuration/spec.md) — `~/.tmax` persistence and activation; [persisted-format example](specs/configuration/example.toml).
- [specs/model-routing/spec.md](specs/model-routing/spec.md) — models.dev metadata, concrete models, model aliases, dynamic priority rules, schedules, and multi-source selection.
- [specs/model-routing/inputs/spec.md](specs/model-routing/inputs/spec.md) — scoped quota/reset inputs, explicit registration, acquisition, and validity.
- [specs/integrations/spec.md](specs/integrations/spec.md) — shared agent/provider boundaries and the index of dedicated provider contracts.
- [specs/cli/spec.md](specs/cli/spec.md) — common CLI rules and capability-specific command contracts; only help/version are implemented.
- [specs/diagnostics/spec.md](specs/diagnostics/spec.md) — status evidence, request-history coverage, and safe cleanup.
- [specs/AGENTS.md](specs/AGENTS.md) — how specifications are written and owned.
- [docs/README.md](docs/README.md) — the user-facing guide list.
- [eng/](eng/) — architecture, testing, and documentation-language policies.
- [CODING_STANDARDS.md](CODING_STANDARDS.md) — reviewer judgment rules.
- [CONTRIBUTING.md](CONTRIBUTING.md) — the contribution process.

## License

The program is [GPL-3.0-only](LICENSE). Adapted engineering content inherits the
source project's GPL version 3 terms; see
[attribution/NOTICE](attribution/NOTICE).
