# tmax

A DSH plugin that projects tmax's gateway access into a DSH profile. Development
slice (M0): dynamic model projection, `tmax_status`, the `tmax` skill,
`tmax dsh connect` credential-reference handoff, and the **tmax** page in
DSH Plugins.

## Requirements

- DeepSeek Harness `0.2.2-rc.1` or a compatible `0.2.x` release.
- tmax installed on `PATH` (or `tmaxPath` in the row config), with a declared
  DSH target that has a gateway provider entry.
- A gateway credential reference already present in the DSH credentials service.
  The plugin never reads or copies key material.

## Connect the existing DSH home

The plugin uses DSH credential references; it never copies a key. Write the
references from tmax after the gateway is configured:

```sh
tmax dsh connect --home "$DSH_HOME" --dry-run
tmax dsh connect --home "$DSH_HOME"
```

The dry run checks the target, DSH home, permissions, and planned references
without reading `.credentials.yaml` or resolving the key. The real run merges
only the target references into that document, preserves unrelated entries and
comments, and writes it owner-only. It does not create a DSH patch or an
`llm-tmax` artifact.

## Install from GitHub

This plugin is distributed with the tmax repository, not through npm. Clone the
repository and install the package directory into the `web` profile:

```sh
git clone https://github.com/suraciii/tmax.git
cd tmax
just dsh-install
```

`just dsh-install` delegates to `dsh plugin --profile web add "$PWD/dsh-plugin"`.
Restart or refresh DSH Web. To update, pull the repository and re-run
`just dsh-install`; uninstall with `just dsh-remove`. The package is
intentionally `private`; that is a distribution choice, not an install
limitation.

The bundle patch inserts one `tmax` row. Configure it in the profile's
`cordis.patch.yml` if needed:

```yaml
- id: tmax
  config:
    home: /home/you/.dsh
    target: dsh-main
    refreshSeconds: 60
    providerPrefix: tmaxp-
```

`providerPrefix` exists so the plugin can run beside a legacy `llm-tmax` route
during migration; production uses an empty prefix after the legacy route is
removed. `home` defaults to `DSH_HOME`; `target` defaults to the sole DSH target.

## How it reads tmax

The Host half runs, with a timeout and no shell:

```sh
tmax dsh projection --home /home/you/.dsh
```

It accepts only a successful tmax envelope, rebuilds model descriptors from
`routes[].models[].capabilities`, registers one DSH provider route per tmax
gateway route, and refreshes on an interval. It emits `llm/models-updated` after
each change. A missing tmax or projection does not fail plugin activation; the
plugin stays loaded and `tmax_status` reports the blocker.

`PiAiAdapter` consumes *resolved* provider profiles, not raw plugin config.
Because the adapter does not export its resolver, this plugin fills the six
scalar defaults from `@deepseek-ai/dsh-llm-pi-ai` 0.2.2-rc.1
(`streamIdleTimeoutMs` and the request-image bounds) and sets the retry policy.
A missing `streamIdleTimeoutMs` otherwise reaches `idleWatchdog(undefined)` and
fails the first stream with `timeoutMs must be a positive finite number`.

Two capability edges are handled explicitly so one unknown model cannot break
the whole catalog:

- Unknown input modalities are advertised as `['text']`; pi-ai requires a
  concrete modality list and text-only is conservative.
- A model with no positive `contextWindow` is omitted from the adapter catalog
  until tmax publishes one; DSH would otherwise reject the whole provider
  group with invalid context metadata.

## Safety boundaries

- The plugin never accepts, reads, logs, or returns a key or token.
- It does not write tmax configuration or DSH home patches.
- It does not start the tmax gateway; readiness is reported, not acted on.
- It does not materialize model `maxTokens`; output capacity stays absent.
- It does not translate protocols; each tmax route names one protocol.
- DSH retry is disabled for these routes; tmax owns explicit fallback.

## Manual check

```sh
tmax dsh projection --home "$DSH_HOME" | jq '.data.routes[] | {name, protocol, models: [.models[].id]}'
```

Then open the Web model picker. The route appears under `providerPrefix` +
`name` (for example `tmaxp-tmax`); models keep their tmax public IDs.

## Client Plugins page

`client.js` registers the **tmax** bundle page in Plugins. It reads
the live Host model catalog through `ctx.remote.session.modelCatalog()` and
shows:

- connected plus the projected model list and gateway readiness;
- disconnected with a **Connect now** button plus the terminal command as a
  fallback;
- the Host provider failure message when the catalog lookup fails.

The Host half serves two trusted routes, guarded by the composition's
`connection.requestRejection` fence:

| Route | Effect |
| --- | --- |
| `GET /_tmax/status` | Secret-free projection summary, gateway readiness, models, blockers. |
| `POST /_tmax/connect` | Runs `tmax dsh connect` for the configured target, then refreshes. |

The connect route is an explicit user action. It writes only DSH credential
references through tmax; no key is returned, logged, or sent to the browser.

## Tests

```sh
cd dsh-plugin && npm test
```

The Host test stubs the DSH tree and runs the real plugin entry point: it
checks dynamic route registration, resolved adapter defaults, capability
mapping (text fallback, unknown-context filtering, no `maxTokens`), the status
tool, the skill, and the missing-tmax path. The client test loads the real
`client.js` through a fake `window.__ModuleLoader__` and checks the one
plugins bundle-page registration.

## Current limits

- Tool: `tmax_status` only. The Plugins page shows connect, gateway readiness, and the model list.
- Still not included: automatic legacy migration and model badges. Install and
  uninstall stay with DSH; distribution is source-based.
- A gateway whose daemon identity is unknown reports `ready: null`; the plugin
  does not reinterpret that as ready or as a hard stop.
- The linked package is installed in the `web` profile and the live registry
  returns the projected models. Editing this source requires the profile/HMR
  loader to re-import it, or a DSH restart; a browser page refresh is then
  enough for the picker.

## License

The plugin is part of the tmax repository and is distributed under
GPL-3.0-only; see the repository [LICENSE](../LICENSE).
