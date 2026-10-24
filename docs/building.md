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

## Runtime prerequisites and protection

The personal authority is resolved from the operating-system user account, not
`HOME`, the working directory, or a CLI configuration selector. All tmax-owned
state is under that account's `~/.tmax`; native configuration and native-owned
credential artifacts remain in their explicitly selected native scopes.

Resident control uses Unix local sockets or local-only Windows named pipes with
an explicit OS-user ACL and peer identity checks. Windows needs no separate
daemon, service installation, or administrator setup: ordinary CLI and Web
actions launch the detached executable. Browser launch uses the Windows system
browser. Opt-in Windows login startup uses only tmax's owned current-user Run
value; Linux uses `systemctl --user` and tmax's owned unit. Missing session
services or locked credentials remain visible blockers, not implicit inference
startup or plaintext-unlock fallbacks.

Persistent secrets and credential-bearing recovery use Argon2id (64 MiB, three
passes, one lane) with per-entry salt, and ChaCha20Poly1305 authenticated
encryption with per-entry nonce. Unlock material is supplied through qualified
hidden terminal or inherited-pipe input and is not saved beside ciphertext.
Safe inventory and recovery metadata remain inspectable while locked. Native
credential-file handoff is version-qualified, restricted, and explicitly
disclosed; it is not an export command or token-import feature.

Resident unlock and protected Web key/unlock handoff use owned anonymous pipes,
available in the standard library since Rust 1.87 and within the Rust 1.88
minimum. Unix launch deliberately enables only its FD 3 unlock handoff in the
child; Windows uses the child's bounded standard-input pipe. Owners close pipe
ends without named plaintext files or secret argv/environment values. Unix
parent pipe ends are close-on-exec: Linux allocates them atomically, while Rust
1.88 on macOS sets close-on-exec after creation; no stronger process-wide
fork-race guarantee is claimed for that fallback. Windows protected console
input is hidden and uses separate console handles, keeping management JSON
separate from prompts and explicit key delivery.

Windows source review and cross-compilation do not establish actual Windows
startup, console, security-descriptor, or browser runtime behavior. Record those
as unverified unless the corresponding Windows operations were exercised; the
portable CI help/version smoke alone is not resident lifecycle evidence.

Installed native agent versions, effective configuration precedence, and provider
permission remain independent prerequisites. A successful parser/apply/restore
check is not a hosted-provider streaming, tool-call, identity, or entitlement
verification. Run only explicitly requested paid probes with authorized access.

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
