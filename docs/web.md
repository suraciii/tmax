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
  integration registry supplies protocols, regional choices, and endpoint defaults.
  Configure authentication separately. Provider details expose subscriptions,
  observations, inherited metadata, overrides, and explicit upstream refresh.
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
  Concrete **Details** shows effective reasoning declarations, catalog/override
  provenance, and each tool's desired selector mapping, including protocol-specific
  OpenCode fields and DSH `reasoningEfforts`. Missing declarations are not inferred
  from reasoning support.
- **Coding tools** keeps Codex, pi, omp, OpenCode, Hermes, and DeepSeek Harness separate. Adding a
  configuration checks local directories and tool versions for the selected tool.
  Select a found directory or enter a path manually; registered directories link
  to their existing configuration. Discovery failure does not block manual entry.
  Discovery does not import settings or credentials, and installation detection
  does not establish compatibility. Saving opens the configuration details.
  Add an upstream or gateway provider entry, select its models, then
  optionally choose its default (required for Codex). Saving these declarations does not write native files.
  Apply and restore remain separate explicit actions with optional previews.
  Details also show desired per-model reasoning selectors, omitted names, and
  blockers. These are not evidence of native reload or inference; default effort
  remains tool-owned.
  DSH requires an existing home directory (`DSH_HOME`, otherwise the OS home's
  `.dsh`); application edits its common `cordis.patch.yml` and hands keys to the
  adjacent native `.credentials.yaml`, enabling the credential service in the
  HOME patch for headless consumption without bundle inspection. It does not
  enumerate, select, create, or manage profiles. Whole home entry configurations
  shadow lower profile configurations rather than deep-merging their routes or
  defaults. Model-only application writes no `agent-default-model` entry; an
  explicit default changes provider/model while preserving existing HOME-level
  effort. Runtime overlays remain outside tmax's ownership. See the
  [DSH reference](agents/dsh.md) for evidence limits; historical profile-path
  checks do not establish current home-wide, multi-profile behavior.
- **Gateway** shows the listener, shared-key state, and current-process request
  history. Configure, start, stop, and change login startup explicitly. Revealed
  keys disappear on hide, navigation, page exit, or switching away from the tab.

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
configuration has separate compatibility checks. The optional device-code method depends on OpenAI account
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
fields, preview isolation, and successful application with non-error reload and
version reminders. It creates a temporary target declaration and mocks the
application result; it does not write native files or prove runtime inference.
