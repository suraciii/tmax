# Catalog commands

This child owns catalog grammar and results. The
[catalog contract](../../../model-routing/spec.md#model-catalog) owns snapshot
validation and accepted metadata. Use [common CLI rules](../../spec.md),
[configuration changes](../../configuration/spec.md), and
[shared results](../../results/spec.md).

## Catalog

`SNAPSHOT` is `accepted` or `latest`, default `accepted`. Latest is the last
successfully validated download, not a live fetch. Reads identify the snapshot,
source, retrieval time, staleness, and material accepted/downloaded differences.

| Command | Result or requested boundary |
| --- | --- |
| `catalog providers [--snapshot SNAPSHOT]` | Compact selectable catalog-provider metadata and qualified integration/authentication/endpoint-setup choices. |
| `catalog models CATALOG-ID [--snapshot SNAPSHOT]` | Compact model IDs/metadata under that entry; register nothing. |
| `catalog inspect CATALOG-ID [--model MODEL-ID] [--snapshot SNAPSHOT]` | Detailed metadata/provenance, qualified fixed-integration/explicit-base setup choices, and the first-unused local provider-ID suggestion. |
| `catalog refresh` | Download and validate metadata without upstream credentials. Preserve accepted defaults; do not apply or activate the download. |
| `catalog apply --revision REVISION` | Apply exactly that validated downloaded snapshot and activate resulting valid settings when the service runs. Report affected providers/models/targets and actual saved/loaded outcome. |

Refresh/apply support dry-run. Apply accepts the common optional file revision
guard. Its required catalog revision selects retained data, not an operation
reservation or file revision. Apply never implicitly refreshes. Dry-run describes
intended download/application without fetching, changing accepted metadata, or
activating settings.

Provider creation still requires an explicit local ID. A suggestion from inspect
is not reserved; collision at execution is an error, not permission to select a
different ID. Catalog inspection uses
[qualified endpoint setup](../../providers/spec.md#endpoint-setup-and-resolution-data),
not catalog `api` fields as endpoint/compatibility authority.

List results provide identities and useful summaries without complete metadata or
inspect argv for every row. Inspect exposes full supported details. Apply returns
the selected catalog revision, affected summary, and actual activation state;
dry-run, consequential pending changes, and failures retain
[full config impact](../../configuration/spec.md#configuration-impact-data).
No result claims downloaded metadata is already accepted or loaded.
Apply and its dry-run revalidate configured model effort-map targets and fixed
candidate effort against the proposed effective declarations after overrides.
An incompatible adoption is rejected without dropping user settings. Impact
includes changed gateway-facing input sets and affected alias intersections;
native reapplication remains explicit under the
[shared capability rule](../../../integrations/spec.md#alias-reasoning-capability).

Examples: `tmax catalog providers`, `tmax catalog models deepseek`,
`tmax catalog inspect deepseek --model deepseek-flash`, `tmax catalog refresh`, and
`tmax catalog apply --revision REVISION`. Use a returned downloaded revision.
