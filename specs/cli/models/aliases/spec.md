# Model alias commands

This child owns model-alias grammar and results. The
[model contract](../../../model-routing/spec.md) owns candidate eligibility,
identity, selection, and continuity. Use [common CLI rules](../../spec.md),
[configuration changes](../../configuration/spec.md), and
[shared results](../../results/spec.md). [Model inspect](../spec.md#model-inspection)
owns concrete/alias inspection syntax.

## Model aliases and candidate membership

| Command | Requested change |
| --- | --- |
| `model alias add ID --protocol P (--tier TIER \| --candidate QUALIFIED...) [EDIT]` | Create exactly `(protocol, id)` with one source; edit `enabled` and/or `continuity`. |
| `model alias update ID --protocol P [--new-protocol P] [--tier TIER \| --candidate QUALIFIED...] [EDIT]` | Edit scalar fields and/or completely replace the candidate source. At least one change is required. |
| `model alias candidates add ID --protocol P --candidate QUALIFIED...` | Append to an existing explicit list; reject duplicates and tier sources. |
| `model alias candidates remove ID --protocol P --candidate QUALIFIED...` | Remove named explicit members; reject missing members and an empty result. |
| `model alias rename OLD NEW --protocol P` | Rename the public ID and target selections/references with no old-ID redirect. |
| `model alias remove ID --protocol P` | Remove the definition after reference checks. |

| Alias `FIELD` | Scalar type and clear behavior |
| --- | --- |
| `enabled` | Boolean; clear restores true. |
| `continuity` | `session` or `stateless`; clear restores `session`. Applies to every caller of this definition, not native Agent selections. |

`EDIT` accepts only these fields. Concrete mappings remain fixed and accept no
continuity edit. Stateless selection still requires client/upstream qualification;
it cannot bypass supplier conversation requirements or make a shared key a safe
conversation identity.

| Candidate or identity condition | Observable contract |
| --- | --- |
| Explicit `--candidate` operands | Preserve order; accept only registered qualified concrete offerings. Reject alias candidates, recursive aliases, bare providers, duplicates, and an empty list. |
| `model alias update --candidate` | Replace the complete list, switching from a tier when present; never append implicitly. |
| `--tier TIER` | `L0`–`L3`; replace any previous list. An empty tier is valid but unavailable. |
| Update omits both source forms | Preserve the existing source; never guess a source change or add a provider. |
| Publication | Derive the shared list from enabled definitions; no separate proxy-model enrollment or per-client list. |
| `--new-protocol P` | Change identity only after collision and old-reference checks. Remove old `(protocol, id)` references first; never convert protocols or silently change a target's protocol. |

```sh
tmax model alias add L0 --protocol chat-completions --tier L0
tmax model alias add deepseek-v4.1-flash --protocol chat-completions --candidate opencode-go/deepseek-v4.1-flash --candidate opencode-go-2/deepseek-v4.1-flash --candidate deepseek/deepseek-flash
tmax model alias update L0 --protocol chat-completions --set enabled false
tmax model alias update L0 --protocol chat-completions --clear enabled --tier L1
tmax model alias update L0 --protocol chat-completions --candidate deepseek/deepseek-flash
tmax model alias update L0 --protocol chat-completions --set continuity stateless
tmax model alias update L0 --protocol chat-completions --clear continuity
tmax model alias candidates add L0 --protocol chat-completions --candidate opencode-go/deepseek-v4.1-flash
tmax model alias candidates remove L0 --protocol chat-completions --candidate deepseek/deepseek-flash
tmax model alias rename L0 preferred --protocol chat-completions
tmax model alias update preferred --protocol chat-completions --new-protocol responses
tmax model alias remove preferred --protocol responses
```

These are independent examples with required reference conditions, not an
executed workflow.

| Alias result | Contract |
| --- | --- |
| Mutation summary | `(protocol, id)`, changed scalar fields or source/members, and saved/loaded outcome. |
| Inspect continuity | Effective continuity and whether it is explicit or the `session` default. |
| Session impact | Remove/disable and candidate/continuity changes report affected existing sessions under [configuration impact](../../configuration/spec.md#configuration-impact-data); never retarget or revive old bindings. |
| Full detail | Definitions and candidate explanations belong in model inspect/explain, not routine mutation summaries. |
