set dotenv-load := false

# List available project commands.
default:
    @just --list

# Build release binaries.
build:
    cargo build --locked --release --workspace

# Run the CLI with optional arguments.
run *args:
    cargo run --locked -p tmax-cli -- {{args}}

# Check formatting without modifying source.
fmt:
    cargo fmt --all -- --check

# Type-check every workspace target.
check:
    cargo check --locked --workspace --all-targets

# Treat workspace lint warnings as errors.
clippy:
    cargo clippy --locked --workspace --all-targets -- -D warnings

# Run the workspace test suite.
test:
    cargo test --locked --workspace

# Reject Python caches in the Git index.
check-python-cache:
    python3 -B scripts/check_python_cache.py

# Compile Python sources in memory, including new untracked scripts.
check-python:
    python3 -B -c 'from pathlib import Path; [compile(p.read_bytes(), str(p), "exec") for p in Path("scripts").rglob("*.py")]'

# Exercise verification tooling in isolated temporary repositories.
test-scripts:
    python3 -B -m unittest discover -s scripts -p 'test_*.py'

# Exercise the actual CLI entrypoint.
smoke:
    cargo run --locked -p tmax-cli -- --help
    cargo run --locked -p tmax-cli -- --version

# Required pull request gate.
ci-pr: check-python-cache check-python fmt check clippy test test-scripts smoke

# Full local and main-branch gate.
ci: ci-pr build

# Retain full-gate output and source provenance outside the repository.
verify evidence_dir:
    python3 -B scripts/verify.py {{quote(evidence_dir)}}

# Retain PR-gate output and source provenance outside the repository.
verify-pr evidence_dir:
    python3 -B scripts/verify.py --gate ci-pr {{quote(evidence_dir)}}

# Install or update the DSH plugin from this checkout into a DSH profile.
dsh-install profile="web":
    dsh plugin --profile {{profile}} add "$PWD/dsh-plugin"

# Remove the DSH plugin from a DSH profile.
dsh-remove profile="web":
    dsh plugin --profile {{profile}} remove tmax
