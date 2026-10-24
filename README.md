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

The workspace includes explicit management operations, a shared local Web
surface, encrypted secret/recovery storage, rule-driven model selection, and a
same-protocol resident proxy. Native adapters are version- and scope-specific;
configuration application is not proof of a running agent's use or upstream
entitlement. Provider-authorized streaming/tool/continuation qualification remains
separate from local parser and transport checks. Start with the
[product specification](specs/product/spec.md),
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

```bash
just run integration list
just run provider add personal --integration deepseek
just run model add personal/deepseek-flash
just run provider key set personal
just run provider inspect personal
```

See the [usage guide](docs/usage.md) for incremental provider/model setup,
protected input, native pi direct/proxy application and restoration, and manual
quota-driven rules. The examples use finite commands, not configuration blobs.

Protected key entry and encrypted-store unlock use separate hidden terminal or
explicit inherited-pipe inputs; secrets never belong in argv or TOML. Saving
settings does not sign in, apply native files, start inference, or send a paid
probe. Use command help for explicit proxy setup, native application, and recovery.
`web open --no-browser` starts supplementary loopback management independently of
inference. The resident uses Unix local sockets or same-user Windows named pipes.
Windows supports hidden-console input, protected Web entry, detached startup,
and opt-in current-user login startup without a service installation. Linux
login startup uses an owned per-user systemd unit. Locked credentials remain an
explicit unlock requirement, never a plaintext fallback or a claim of readiness.

### Native qualification

The implemented native schemas are qualified against Codex CLI **0.162.1**,
pi **1.2.0**, omp **18.12.6**, OpenCode **2.0.28**, and Hermes Agent
**0.22.1+683.g848fc0a**. Other versions are reported separately rather than
silently treated as equivalent. Supported native credential references are used
where available; any native representation that requires a restricted plaintext
handoff is disclosed in the apply result.

Local qualification includes installed configuration parsers, explicit
application/restoration in isolated native scopes, and synthetic loopback
protocol/session checks. Actual pi session resume/new-session and native tool
round trips were exercised through the tmax proxy. These checks do not establish
hosted-provider authentication, subscription entitlement, account identity, or
permission to use a managed Codex gateway. Those require the user's authorized
credentials and applicable native/workspace policy; tmax does not invent
identity headers or copy unrelated credentials to bypass those requirements.

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
  tmax-core/           shared domain, finite operations, native adapters and service
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
  Its [provider contracts](specs/integrations/spec.md#provider-specific-contracts)
  include domestic GLM Coding Plan and StepFun Token Plan requirements; a spec
  does not create a built-in integration or claim verified native/provider support.
- [specs/cli/spec.md](specs/cli/spec.md) — common CLI rules and capability-specific command contracts.
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
