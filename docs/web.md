# Local Web workspace

Run `tmax web open` to open the local workspace. It uses the same configuration
and operations as the CLI. The browser does not require an account or start the
inference gateway. The built binary includes all scripts, styles, and icons; the
page makes no external font, image, or script requests.

## Common tasks

- **Overview** shows saved provider, model, and target counts, gateway state, and
  manually recorded renewal dates. Unknown quota is not zero quota. A recorded
  renewal date does not establish a billing outcome.
- **Providers** groups independent instances by service. Search by name or ID.
  Add an instance from the local catalog or choose explicit manual setup. The
  integration registry supplies product-qualified choices and fixed protocol bases.
  Configure authentication separately. Provider details expose subscriptions,
  observations, inherited metadata, overrides, and explicit upstream refresh.
  The allowance section leads with the reported plan, each window or bucket with
  its consumed or remaining basis, exact counters, reset evidence, and retrieval
  time; the raw record stays behind a disclosure. It distinguishes a retained
  observation from a missing one and from a freshness guarantee the provider did
  not supply. **查询额度** opens a dedicated panel that fixes the scope to quota;
  **刷新可查询额度** refreshes each supported provider and reports failures per
  provider. A failed query retains the previous dated record, and an integration
  without a quota operation does not offer a query.
  **Configure error policy** sets matched HTTP/connection/transport failures, the
  consecutive-failure threshold, a signed fixed replacement priority, and duration
  in hours. New policies require explicit enablement. Details distinguish loaded
  routing state from saved policy and expose trigger/expiry and an explicit reset.
  Preview does not change the counter or active override.
- **Models** separates concrete models from aliases. Concrete forms edit names,
  enabled state, and base priority. Alias forms select exact concrete candidates
  and a native protocol, with an optional reasoning-effort input for each
  candidate. Leave it blank when creating an alias to omit a fixed override;
  blanking a saved value while editing clears that override. Values must occur
  in the candidate's effective `reasoning_options` metadata with `type = "effort"`
  and explicit `values`; `reasoning = true` alone is not enough. A fixed effort
  overrides caller input and model maps for the selected candidate. Without it,
  explicit model maps apply, otherwise identity or omission is preserved. Manage
  model maps with the [CLI effort-map commands](../specs/cli/models/spec.md#model-effort-maps).
  Chat Completions and Responses support maps and fixed effort; Messages does not.
  Changing only effort retains the otherwise selected backend or rejects before
  binding publication and dispatch; it never chooses another backend. Restore
  name inheritance with the separate clear action; an explicit empty string
  remains different from an absent override.
  Alias forms also expose optional request-error fallback, requiring explicit
  stateless full-context continuity and selected failure conditions. No provider
  is attempted twice in one request; successful response delivery prevents any
  later fallback. Transport and HTTP 5xx retries can duplicate upstream execution
  or billing. See [request fallback and temporary priority](usage.md#request-fallback-and-temporary-provider-priority).
  Concrete **Details** shows effective reasoning declarations, catalog/override
  provenance, and each tool's desired selector mapping, including protocol-specific
  OpenCode fields and DSH `reasoningEfforts`. Missing declarations are not inferred
  from reasoning support.
- **Coding tools** keeps Codex, pi, omp, OpenCode, Hermes, and DeepSeek Harness separate. Adding a
  configuration checks local directories and tool versions for the selected tool.
  Select a found directory or enter a path manually; registered directories link
  to their existing configuration. Discovery failure does not block manual entry.
  Discovery does not import settings or credentials, and installation detection
  does not establish that the tool will use the configuration. Applying writes
  the declared target even when the executable is not on `PATH`; discovery
  reports that absence and never blocks the write. Saving opens the configuration details.
  Add an upstream or gateway provider entry, select its models and native
  protocols, then optionally choose its default (required for Codex). The model
  choices list only published protocols, grouped Responses first for both concrete
  models and aliases. A new alias initially prefers Responses when a concrete
  candidate publishes it; existing selections retain their protocol. The client
  uses the selected protocol;
  the gateway does not translate or choose another one. Saving these declarations
  does not write native files.
  Apply and restore remain separate explicit actions with optional previews.
  Details also show desired per-model reasoning selectors, omitted names, and
  blockers. These are not evidence of native reload or inference; default effort
  remains tool-owned.
  DSH requires an existing home directory (`DSH_HOME`, otherwise the OS home's
  `.dsh`); application installs the independent `tmax-dsh.mjs` plugin, declares
  its `llm-tmax` entry in `cordis.patch.yml`, and hands keys to the adjacent native
  `.credentials.yaml`. It enables the credential service for headless consumption
  without bundle inspection and preserves user `llm-pi-ai` routes. It does not
  enumerate, select, create, or manage profiles. The managed home entry shadows
  lower entries with the same identity. Model-only application writes no
  `agent-default-model` entry; an
  explicit default changes provider/model while preserving existing HOME-level
  effort. Runtime overlays remain outside tmax's ownership. See the
  [DSH reference](agents/dsh.md) for evidence limits; historical profile-path
  checks do not establish current home-wide, multi-profile behavior.
- **Gateway** shows listener readiness, native process supervision, login-start
  registration, shared-key state, and current-process request history separately.
  Configure, start, stop, and change login startup explicitly. A stopped inference
  listener stays stopped during Web-only process recovery. Restart reconnects the
  page and verifies a new ready process before displaying completion; it never
  automatically resubmits a mutation after connection loss. Revealed keys disappear
  on hide, navigation, page exit, or switching away from the tab.
  Persistent gateway errors are kept in the local [log file](usage.md#gateway-error-log),
  not in the history panel. Clearing current-process history does not delete that file.

The redesigned workspace uses Simplified Chinese labels and retains stable IDs,
protocol names, and command names. It uses the same backend operations and local
configuration as the CLI. Gateway is labeled 网关; literal `gateway` command and
configuration names remain visible in technical details. Advanced operations
expose the complete backend operation catalog.
Credentials are separate password fields; browser storage is not used. Submission,
including a preview or failed request, clears these fields. Previews do not send keys.

Configuration forms capture the revision at opening. The backend catalog declares
which commands support this guard; read operations and skill installation omit it. A conflicting edit must be
reopened after refreshing. Do not retry a create operation that already saved
successfully. Partial outcomes and activation warnings remain visible, with the
backend result available in an expandable technical detail view.

## Codex sign-in

In **Providers**, choose **Configure authorization** for a Codex subscription.
The dialog offers an independent authentication scope by default. Choose
**Start sign-in**, then open the official login page. tmax shows progress and
saves the provider reference and coding-tool target after Codex confirms success.
Closing the dialog keeps the attempt running; reopen it to resume observation,
or choose **Cancel sign-in** to stop it. Attempts expire after ten minutes.

Use an existing Codex target only when its shared authorization is intended to
change. A failed attempt can reuse its retained private scope from the selector.
Configuration changes during sign-in prevent saving over newer edits. If login
succeeds but saving or runtime activation fails, inspect the reported outcome
before retrying. An existing unrelated login is never imported automatically.

The official Codex executable is required. Browser authentication checks the
installed authentication interface, without requiring an exact CLI version.
If the required methods are unavailable, update Codex and retry. Native model
configuration applies its own schema checks. The optional device-code method depends on OpenAI account
settings and availability. Codex stores and refreshes credentials; tmax stores
only a reference and account metadata. The independent scope uses Codex's file
credential store with private local permissions, not encryption.

A saved login does not establish model entitlement, quota, workspace identity,
or working gateway forwarding. Configure and verify the intended models and
consumer separately. See the [authentication contract](../specs/integrations/codex-subscription/web-login.md)
for lifecycle and ownership details.

## Browser verification

Use the normal Rust gates from [Building and verification](building.md). The Web
origin test also checks all embedded assets, MIME types, no-store headers, and
the same-origin content security policy.

An additional browser smoke scenario is available in
[`scripts/web_smoke.js`](../scripts/web_smoke.js). With `playwright-cli` available,
open a running tmax URL in a dedicated browser session from a temporary working
directory. Then run the scenario by its absolute filename:

```text
playwright-cli -s=tmax-web-test open http://127.0.0.1:61094 --browser=msedge
playwright-cli -s=tmax-web-test run-code --filename=/absolute/path/to/tmax/scripts/web_smoke.js
playwright-cli -s=tmax-web-test close
```

Use the actual local URL from `tmax web status`; `61094` is only an example.
The scenario creates unique provider, model, and alias fixtures, removes them in
`finally`, and checks preview isolation, scalar values, concurrent editing,
search, navigation, narrow layouts, credential-preview isolation, skill preview,
and advanced reads. It performs no authentication, upstream
inference, or native application. A failed cleanup must be investigated before
rerunning. Browser snapshots belong outside the repository.

The optional [`scripts/web_discovery_smoke.js`](../scripts/web_discovery_smoke.js)
scenario controls discovery responses to verify directory selection, duplicate
targets, missing installations, failures, delayed results, and the handoff to
model configuration. It creates and removes one uniquely named target declaration;
it never applies native files. Run it in the same dedicated browser session.

The optional [`scripts/web_agent_apply_smoke.js`](../scripts/web_agent_apply_smoke.js)
checks OpenCode's optional default, the application form without command/target
fields, preview isolation, and successful application with a non-error reload
reminder. It creates a temporary target declaration and mocks the
application result; it does not write native files or prove runtime inference.
