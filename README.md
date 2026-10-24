# tmax

[English](README.md) | [简体中文](README.zh-CN.md)

`tmax` is a local, CLI-first tool for managing model-provider access, optional
subscription information, and model choices for coding agents. A supplementary
local Web interface is also available.

- Keep multiple accounts for one service in separate named provider instances.
- Manage native model-provider settings for Codex CLI, pi, omp, OpenCode, and
  Hermes Agent.
- Run a local, same-protocol model gateway with concrete models or user-defined
  aliases. Native configuration and gateway forwarding are independent capabilities.
- Use offline CLI help and JSON output for explicit management operations.

## Build

Build requirements: Rust 1.88 or newer, a C toolchain, and `just`.

```sh
git clone https://github.com/suraciii/tmax.git
cd tmax
just build
./target/release/tmax --help
./target/release/tmax integration list
```

## Documentation

- [CLI usage](docs/usage.md) — provider and model setup, native configuration,
  and the local gateway.
- [Documentation index](docs/README.md) — user guides.
- [Product specification](specs/product/spec.md) — product scope and boundaries.
- [Building and verification](docs/building.md) — dependencies and project checks.

## Credential storage

Provider keys and credential-bearing recovery files are plaintext under
`~/.tmax`, restricted to the current OS account. File permissions are not
encryption. Enter keys through hidden terminal input or `--key-stdin`; never put
credentials in command arguments or environment variables.

## License

The program is [GPL-3.0-only](LICENSE). Adapted engineering content and its
source project's license terms are recorded in [attribution/NOTICE](attribution/NOTICE).
