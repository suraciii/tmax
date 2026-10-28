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

Daemon control uses Unix local sockets or local-only Windows named pipes with
an explicit OS-user ACL and peer identity checks. Windows needs no separate
daemon, service installation, or administrator setup: ordinary CLI and Web
actions launch the detached executable. Browser launch uses the Windows system
browser. Opt-in Windows login startup uses only tmax's owned current-user Run
value; Linux uses `systemctl --user` and tmax's owned unit. Missing session
services or unavailable credentials remain visible blockers; they do not trigger
implicit inference startup or route fallback.

The tmax store keeps API keys and credential-bearing recovery data as plaintext
under `~/.tmax`. Unix uses mode-0600 files and mode-0700 directories; Windows
uses an owner-only ACL. These permissions are not encryption. Ordinary
configuration stores references, not key bytes. Credential contents never
appear in stdout, logs, or ordinary inspection.

Unsupported storage-format records return `storage_version_unsupported` or
`storage_record_invalid` and remain unchanged. Re-enter provider keys through
`provider key set` or replace the shared key with `gateway key replace`. Recovery
contents in unsupported formats are unavailable to current recovery operations.

Replacing the executable does not upgrade a running daemon. This executable
uses management protocol `tmax-control/5`; the Model identity and selected-model
result cutover is incompatible with an older `tmax-control/4` daemon. If the
current CLI reports `daemon_incompatible`, use
the running service's matching older executable to stop it explicitly:

```sh
/path/to/previous/tmax gateway stop
/path/to/previous/tmax web stop
/path/to/current/tmax web open --no-browser
```

Use the previous executable's `proxy stop` instead of `gateway stop` only for a
pre-Gateway binary. Its existing Web stop controls are another supported option.
Normal active-request stop protections still apply; resolve their reported
blockers rather than assuming an executable replacement stopped inference.
Start the upgraded gateway explicitly when wanted, using `tmax gateway start`.
The new CLI does not take over, terminate, automatically restart, or send
unsupported state-changing commands to an incompatible daemon. Do not delete
or edit routing/session JSON to upgrade: supported historical operational records
normalize internally while preserving identity tokens, retained evidence, and
continuation blockers.

Native credential-file handoff is version-qualified, restricted, and explicitly
disclosed; it is not an export command or token-import feature. Provider-key
entry uses hidden terminal input or explicit `--key-stdin`. Shared gateway-key
delivery uses a qualified terminal or output pipe. Never place credential bytes
in argv, environment variables, or ordinary output. Native provider sign-in
retains its native owner's platform-specific input and credential-storage rules.

Windows source review and cross-compilation do not establish actual Windows
startup, console, security-descriptor, or browser runtime behavior. Record those
as unverified unless the corresponding Windows operations were exercised; the
portable CI help/version smoke alone is not daemon lifecycle evidence.

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
