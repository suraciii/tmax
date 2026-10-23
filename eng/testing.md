# Test architecture

**Tests must catch meaningful failures, not serve as formalities; speed must
never weaken assertions.**

**Pull request — `ci-pr`:** Check formatting, type-check the workspace, lint it
with warnings denied, run the workspace and verification-tool tests, and
exercise the CLI entrypoint. This is the pull request verification gate.

**Main — `ci`:** Everything in `ci-pr` plus the release build. This gate runs
after merge into `main` and on manual dispatch. A failure is handled as a
separate integration repair.

**Script tests:** `just test-scripts` discovers and runs the Python tests under
`scripts/`. Each test exercises the tooling it covers against a real temporary
repository, because the tooling reads the Git index and the working tree.

**Smoke:** `just smoke` runs the CLI through its real entrypoint with `--help`
and `--version`. It is the minimum check that the binary starts and reports the
workspace version. It is not a release build; `just build` covers that.

Logic tests must use small deterministic in-memory inputs instead of unnecessary
environment setup. Persistence tests must use real IO in isolated temporary
directories. A test that needs a file must create it under its own temporary
directory and must not write inside the repository.

The [Justfile](../Justfile) defines test selection. The
[README](../README.md#build) documents dependencies and verification commands.
