# Model alias commands

This child owns model-alias grammar and results. The
[model contract](../../../model-routing/spec.md) owns Model identity, candidate
eligibility, selection, and continuity. Use [common CLI rules](../../spec.md),
[configuration changes](../../configuration/spec.md), and
[shared results](../../results/spec.md). [Model inspect](../spec.md#model-inspection)
owns concrete/alias inspection syntax.

## Model aliases and candidate membership

These commands edit one ModelAlias record per route name. The record declares its
protocols and one ordered candidate list that applies to every declared protocol.
A candidate is an existing Model reference with alias-local settings, not an
independently managed Model or candidate entity; membership commands do not create
or remove the referenced Model. The route name alone identifies the alias; no
protocol is part of its identity. [Configuration](../../../configuration/spec.md#model-aliases)
owns the persisted fields; [Model selection](../../../model-routing/spec.md#model-aliases)
owns candidate and protocol eligibility.

| Command | Requested change |
| --- | --- |
| `model alias add ID --protocol P [--protocol P ...] --candidate QUALIFIED... [EDIT]` | Create one route with the declared protocol list and a required nonempty candidate list; edit `enabled` and/or `continuity`. |
| `model alias update ID [--protocol P ...] [--candidate QUALIFIED...] [EDIT]` | Edit scalar fields, replace the complete declared protocol list with repeated `--protocol`, and/or completely replace the candidate list. At least one change is required. Omit `--protocol` to keep the current list. |
| `model alias candidates add ID --candidate QUALIFIED...` | Append candidates to the existing list; reject duplicates. |
| `model alias candidates remove ID --candidate QUALIFIED...` | Remove named members; reject missing members and an empty result. |
| `model alias rename OLD NEW` | Rename the public ID and target selections/references for every declared protocol, including relayed entries that reference provider `tmax`; no old-ID redirect. |
| `model alias remove ID` | Remove the definition after reference checks. |

| Alias `FIELD` | Scalar type and clear behavior |
| --- | --- |
| `enabled` | Boolean; clear restores true. |
| `continuity` | `session` or `stateless`; clear restores `session`. Applies to every caller of this definition and every declared protocol, not native Agent selections. |
| `fallback.http_statuses` | Comma-separated unique HTTP error status integers, 400–599. |
| `fallback.connection_errors`, `fallback.transport_errors` | Booleans selecting connection-establishment or ambiguous transport failures. |
| `fallback` | Clear-only container; removes fallback without changing candidates or continuity. |

Scalar `EDIT` accepts the fields above; use separate candidate reasoning-effort
options below for per-candidate overrides. A configured fallback requires
`continuity = "stateless"` and at least one selected error condition. Clear its
parent to disable it. Concrete mappings remain fixed; stateless context still
requires client/upstream qualification. Transport and 5xx fallback disclose
possible duplicate execution/billing; response delivery is never spliced.

| Candidate or identity condition | Observable contract |
| --- | --- |
| Candidate protocol coverage | Every candidate must publish every declared protocol. A command that would leave a declared protocol uncovered is rejected as `invalid_configuration` under `model_aliases.<id>.candidates`; split the route or drop the protocol. A candidate may publish protocols the route does not declare. |
| `--protocol` on `model alias add` | Required at least once; declares the complete protocol list from `chat-completions`, `responses`, and `messages`. Repeated values are deduplicated and stored in canonical order. |
| `--protocol` on `model alias update` | Replaces the complete declared protocol list; omitting it preserves the current list. A resulting empty list is rejected. |
| Explicit `--candidate` operands | Preserve order; accept only registered qualified concrete models. Reject alias candidates, recursive aliases, bare providers, duplicates, and an empty list. |
| `model alias update --candidate` | Replace the complete list with the repeated operands; never append implicitly. |
| Update omits `--candidate` | Preserve the existing list; never guess a membership change or add a provider. |
| Alias ID | Arbitrary user-defined route name, unique among route names; `L0`, `L1`, and `coding` have no built-in candidate membership or ranking. |
| Publication | Derive the shared list from enabled definitions and their declared protocols; no separate gateway-model enrollment or per-client list. |
| Candidates add/remove | Identity is the route ID; these commands accept no `--protocol`. |

```sh
tmax model alias add L0 --protocol chat-completions --candidate deepseek/deepseek-flash
tmax model alias add deepseek-v4.1-flash --protocol chat-completions --protocol responses --candidate opencode-go/deepseek-v4.1-flash --candidate opencode-go-2/deepseek-v4.1-flash --candidate deepseek/deepseek-flash
tmax model alias update L0 --set enabled false
tmax model alias update L0 --clear enabled
tmax model alias update L0 --candidate deepseek/deepseek-flash
tmax model alias update L0 --protocol chat-completions --protocol responses
tmax model alias update L0 --set continuity stateless
tmax model alias update L0 --clear continuity
tmax model alias candidates add L0 --candidate opencode-go/deepseek-v4.1-flash
tmax model alias candidates remove L0 --candidate deepseek/deepseek-flash
tmax model alias rename L0 preferred
tmax model alias remove preferred
```

These are independent examples with required reference conditions, not an
executed workflow.

| Alias result | Contract |
| --- | --- |
| Mutation summary | `ID`, changed declared protocols, scalar fields, or candidates, and saved/loaded outcome. |
| Inspect continuity | Effective continuity and whether it is explicit or the `session` default. |
| Session impact | Remove/disable and candidate/continuity changes report affected existing sessions under [configuration impact](../../configuration/spec.md#configuration-impact-data); never retarget or revive old bindings. |
| Full detail | Definitions and candidate explanations belong in model inspect/explain, not routine mutation summaries. |

### Per-candidate reasoning effort

Both `model alias add` and `model alias update` accept repeatable
`--candidate-reasoning-effort QUALIFIED=EFFORT` and
`--clear-candidate-reasoning-effort QUALIFIED` options. Each candidate may be
set or cleared at most once per mutation; duplicates and setting and clearing
the same candidate are rejected. A set requires `QUALIFIED` to be a current
alias candidate and `EFFORT` to be a nonempty supported value. The value must
appear in that concrete candidate's effective positive `reasoning_options`
metadata (`type = "effort"` with explicit `values`); `reasoning = true` alone
is not enough. A route may configure candidate effort only when at least one
declared protocol is Chat Completions or Responses; a route that declares only
Messages must not configure it. Invalid effort values or a declared protocol set
that cannot carry them exit 2 and identify `candidate_reasoning_effort`.

A configured value is fixed for that exact candidate and takes precedence over
both the caller's effort and any model effort map when it is selected. Without
a fixed value, the gateway applies an explicit model map to supplied input,
otherwise preserving identity or omission. Fixed > map > identity is the
[request-parameter contract](../../../model-routing/spec.md#request-parameters),
not an effort-based backend selector. Chat Completions and Responses accept
configured effort; Messages rejects it. Updating an entry preserves unrelated
map entries. Removing or replacing a candidate drops that candidate's fixed
entry; explicit clear removes the named entry. These options add no global
default, universal effort vocabulary, or other request parameters. The shared
[alias capability rule](../../../integrations/spec.md#alias-reasoning-capability)
restricts this candidate's advertised inputs to its fixed upstream name.

```sh
tmax model alias add coding --protocol chat-completions \
  --candidate provider-a/model-a --candidate provider-b/model-b \
  --candidate-reasoning-effort provider-a/model-a=EFFORT_A \
  --candidate-reasoning-effort provider-b/model-b=EFFORT_B
tmax model alias update coding \
  --clear-candidate-reasoning-effort provider-a/model-a
```

Replace the generic candidate names with registered models and each effort
placeholder with a value supported by that candidate's effective metadata.
