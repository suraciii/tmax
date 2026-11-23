# tmax

[简体中文](README.md) | English

`tmax` is a local, CLI-first tool for managing model-provider access, optional
subscription information, and model choices for coding agents. A supplementary
local Web interface is also available.

- Manage independent named access instances, including multiple instances for one
  service or account; separate instances do not establish separate allowances.
- Manage native model-provider settings for Codex CLI, pi, omp, OpenCode, and
  Hermes Agent.
- Run a local, same-protocol model gateway with concrete models or user-defined
  aliases. Native configuration and gateway forwarding are two delivery forms of
  the same provider inventory.
- Use offline CLI help and JSON output for explicit management operations.

## Usage scenarios

Examples of requests to your agent:

### One model name across accounts

> Map `deepseek-v4.1-flash` from both my OpenCode Go accounts and the official DeepSeek API to the same model name, `deepseek-v4.1-flash`. Prefer DeepSeek through OpenCode Go.

### Switch OpenCode’s default model based on provider pricing

> Schedule OpenCode’s default model to switch with providers’ peak and off-peak pricing: use DeepSeek during off-peak hours and switch back to another model I specify during peak hours.

### Configure StepFun for every agent

> Configure every agent to use StepFun’s `step-5-preview`. My API key is: `***`.

## Build

Build requirements: Rust 1.88 or newer, a C toolchain, and `just`.

```sh
git clone https://github.com/suraciii/tmax.git
cd tmax
just build
./target/release/tmax --help
./target/release/tmax integration list
```

## Use from DeepSeek Harness

The DSH plugin is this repository's npm package: its manifest is the repository
root's `package.json`, which points DSH's exports, bundle patch, and packed files
at the modules in `dsh-plugin/`; the locale files stay beside the manifest at
`locale/`, because DSH reads display text and the detail document from fixed
package paths. It is not published to npm, so DSH installs it straight from
GitHub.

In DSH Web, open **Plugins** > **Add plugin** and paste the repository address:

```text
https://github.com/suraciii/tmax
```

The CLI installs and selects the same bundle in one step:

```sh
dsh plugin --profile web add https://github.com/suraciii/tmax
```

Restart or refresh DSH Web after installation, then open the **tmax** page in the
sidebar to connect and inspect models and quotas. The plugin requires a `tmax`
executable on `PATH` plus a configured DSH target and gateway. Updates follow the
repository's default branch; uninstalling removes the profile dependency:

```sh
dsh plugin --profile web update tmax
dsh plugin --profile web remove tmax
```

Working from a checkout instead, `just dsh-install` links this repository into the
profile, so source edits reach DSH at its next recomposition; `just dsh-remove`
uninstalls it. The plugin used to be the `dsh-plugin/` directory; a profile that
still links it needs one more `just dsh-install`, because that directory no
longer carries a manifest.

## Documentation

- [CLI usage](docs/usage.md) — provider and model setup, native configuration,
  and the local gateway.
- [DSH plugin](dsh-plugin/README.md) — install, page surfaces, actions, and limits.
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
