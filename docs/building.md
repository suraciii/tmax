# Building and verification

This page describes the build requirements, the build commands, the verification
gates, and the evidence each gate retains.

## Requirements

The build requires Rust 1.88 or newer and a C toolchain. The workspace carries
no additional native library dependencies or vendored native code. `Cargo.lock`
pins the dependency set. Build, check, lint, test, and run recipes use `--locked`.

Verification additionally requires:

- `just` — the command runner the gates use.
- Python 3.10 or newer — the verification and hygiene scripts under `scripts/`.
- `git` — the scripts read the index and the working tree to record source
  provenance.

Install the Rust check components with `rustup component add rustfmt clippy`.

The scripts run with `python3 -B`, so no bytecode cache is written.

## Build commands

```bash
just build
cargo build --locked --release --workspace
```

Both commands produce the same output:

- `target/release/tmax` — the command-line binary.

Run the binary with optional arguments:

```bash
just run --help
just run --version
./target/release/tmax
```

With no arguments the binary prints its help and exits with status 0. `--help`
and `--version` print the requested text. Unknown arguments are rejected with a
nonzero exit status.

## Verification gates

```bash
just fmt       # check formatting without modifying source
just check     # type-check every workspace target
just clippy    # lint the workspace, warnings denied
just test      # run the workspace test suite
just test-scripts  # run the verification tooling's own tests
just smoke     # exercise the CLI entrypoint
just ci-pr     # the required pull request gate
just ci        # the full gate: ci-pr plus the release build
```

`just ci-pr` is the required check on every pull request. `just ci` runs the full
gate after merge into `main` and on manual dispatch. A gate stops at the first
recipe that fails.

After the main gate passes, CI builds and exercises the release CLI on Linux,
macOS, and Windows with stable Rust. A separate Linux job uses Rust 1.88.0 to
check the minimum supported version.

## Verification evidence

```bash
just verify "$(mktemp -d)/full"      # retain the full gate
just verify-pr "$(mktemp -d)/pr"     # retain the pull request gate
```

Both commands run `scripts/verify.py`, which executes the selected gate and
writes its result with source provenance into the evidence directory. The
directory records the repository state before and after the run, the command
output, the exit status, and the total elapsed time.

`gate.log` contains combined command output. `report.json` records tool versions,
source hashes, timing, and exit status. `before.diff` and `after.diff` retain the
tracked changes. Repositories without a first commit are supported; their
commit field is null, and staged and untracked files are still measured.

The runner invokes `just ci` or `just ci-pr` directly. Just owns dependency
ordering and gate bodies. A failed gate keeps its nonzero exit status. A
successful gate fails verification if sources changed or the final snapshot
could not be recorded. Ignored build outputs do not count as source changes.

The evidence directory must be outside the repository: neither the repository
root nor any path inside it is accepted, and a pre-existing directory is
rejected. Use a temporary directory outside the repository, for example under
`/tmp` or a scratch workspace.

Gate output is retained, not printed for review alone. A gate run whose evidence
was not retained is not a recorded pass.

## See also

- [Documentation index](README.md)
- [Test architecture](../eng/testing.md)
- [Attribution and provenance](../attribution/NOTICE)
