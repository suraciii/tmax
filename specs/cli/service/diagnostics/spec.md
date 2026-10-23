# Probe and request-history commands

This child owns paid-probe and history grammar/results. The
[diagnostics contract](../../../diagnostics/spec.md) owns evidence, coverage,
retention, outcomes, and bounded historical detail. Use
[common CLI rules](../../spec.md) and [shared results](../../results/spec.md).

## Paid inference probes and request history

| Command | Result or requested boundary |
| --- | --- |
| `model probe QUALIFIED --protocol P --transport TRANSPORT` | One explicit bounded potentially paid inference check against that concrete offering/qualified transport. No separate approval operand. |
| `history list [--since INSTANT] [--until INSTANT] [--provider ID] [--model ID --protocol P] [--outcome OUTCOME] [--target TARGET]` | Compact retained request metadata with process/history scope, requested/retained range, and material coverage gaps once per result. |
| `history inspect REQUEST` | Redacted detail and historical evidence for that request, or explicit no-longer-retained state. |
| `history clear` | Clear locally terminal records at the current serialized boundary, never in-progress or subsequently terminal/arriving records. |

Probe/clear support dry-run. Transport is an exact integration-qualified choice,
not inferred from protocol. Probe help/dry-run disclose bounded request shape,
transport, known reference pricing, unknown charges, and possible paid quota/credit
use. Actual result reports observed usage or honest unknowns. No alias ID,
candidate fan-out, automatic retry, or replay after upstream execution is allowed.
A probe is not a non-inference health check. Direct execution needs no preview.
Timing follows [transport deadlines](../../../product/spec.md#transport-deadlines),
not an invented generation/first-token deadline. Dry-run neither reads credentials
nor contacts upstream.

History is memory-only, without content archives/logs or body capture. Time filters
use admission time, or receipt time for pre-admission rejection, in `[since, until)`;
records identify which. Filters combine with AND. Model is exact public ID and
requires protocol; protocol alone is invalid. Outcome is `in-progress`, `rejected`,
`completed`, `failed`, `cancelled`, `interrupted`, or `unknown`. No filters means
all retained records, not complete historical coverage. `--target` matches only
independently established retained attribution and excludes unknown context; it
does not require the target to remain registered. Never infer target/native
provider from the shared key, selected public model, or unsupported caller claims.
Native provider/model selections are not a proxy access filter.

| Request-history data | Contract |
| --- | --- |
| List row | Stable request handle, useful identity/time/outcome summaries, and relevant failure reasons; no full evidence or derivable inspect argv. |
| Requested model | Retain public model ID, protocol, and recognized kind `concrete` or `alias`; preserve unknown/unrecognized/malformed distinctions under [Diagnostics](../../../diagnostics/spec.md). |
| Native context | Nullable `target` and `agent_provider` require independently established attribution; unknown remains null after shared-key authentication. The selected upstream provider/authorization is distinct from the native provider name. |
| Inspect attribution | Qualified evidence or explicit unknown, never guessed caller identity. Historical names/evidence survive entry/target changes or removal without relabeling, access revocation, or session-binding deletion. |
| Result coverage | Process/reset scope and material limits/gaps. |
| Detailed history/status | Retain process start/clear time, actual limits/counts, eviction/capture evidence, and historical rule/input snapshots as specified by Diagnostics. Never use current config to relabel historical derived rule kind or reconstruct omitted candidates/rules. |
| Upstream-source native traffic | Outside proxy coverage even if the same target also has proxy entries; missing records prove neither inactivity, bypass, nor successful access. |

Clear includes terminal rejected/failed/cancelled/interrupted/unknown records as
qualified by Diagnostics. Dry-run reports current eligible count/coverage and
in-progress work that remains, without retaining a record set. Actual clear
recomputes process/history identity and freezes membership only at its serialized
boundary. Later completions/arrivals remain. In-operation reset/process replacement
rejects before clearing the wrong history. Clear changes only history/derived
aggregates, not settings, bindings, recovery, or quota data. It never starts a
service to invent history. Results report cleared count, retained in-progress
count, process/history scope, and actual coverage changes.

Examples: `tmax model probe deepseek/deepseek-flash --protocol chat-completions
--transport TRANSPORT --dry-run`, the same without dry-run,
`tmax history list --provider deepseek --outcome failed`,
`tmax history list --model L0 --protocol chat-completions --since 2026-10-23T00:00:00Z --until 2026-10-24T00:00:00Z`,
`tmax history inspect REQUEST`, `tmax history clear --dry-run`, and
`tmax history clear`. Help uses an actually qualified transport, not a guessed
protocol-derived choice. Use returned current history handles.
