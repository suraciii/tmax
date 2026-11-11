# DSH plugin projection and connection

Authority: [CLI surface](../spec.md), [Agent targets](../agents/spec.md),
[Product](../../product/spec.md). These commands serve a DSH plugin; they do not manage a DSH profile.

## Invocation

```text
tmax dsh projection [TARGET] [--home PATH]
```

`TARGET` selects one declared DSH target. Without it, `--home PATH` selects the
DSH target whose native scope is exactly that path. Without either, the
command uses `DSH_HOME` when set and unique, then the sole declared DSH target.
Unknown or ambiguous selection is an error; the command never guesses.

The command is a read-only projection. It must not resolve or emit credentials,
write native files, apply a target, start or stop a service, refresh remote
state, or create/select/modify a DSH profile.

## Result

The result has `schema_version = 1` and these fields:

| Field | Contract |
| --- | --- |
| `authority` | Absolute tmax authority path. |
| `tmax_version` | Producing executable version. |
| `configuration_revision` | Current saved configuration revision. |
| `generated_at` | RFC 3339 observation time. |
| `target` | `id`, native `path`, configured `default`, and gateway route `routes` names. |
| `gateway` | `configured`, `ready`, `listen`, `daemon_discovery_state`, and `reason_code`. `ready` is `null` when daemon identity or compatibility is unknown; unknown is not `false`. |
| `routes` | One object per gateway native provider entry: `name`, `protocol`, `base_url`, `credential_ref`, and `models`. |
| `blockers` | Declared blockers with stable `code` and `message`; a blocked projection is still a successful read. |

`credential_ref` is a reference name for the DSH credentials service, never key
material. Each route names one protocol; a target entry that mixes protocols is
reported as a blocker instead of being reinterpreted.

Each model contains `id`, `protocol`, `kind` (`concrete` or `alias`),
`availability`, and capability fields in DSH vocabulary (`contextWindow`,
`input`, `reasoningEfforts`). The projection must not emit `maxTokens` or any
per-request output default. Unknown capability fields remain absent; they are
not replaced with DSH defaults. Alias entries also report `continuity` and
`candidate_count`.

```json
{
  "schema_version": 1,
  "authority": "/home/user/.tmax",
  "tmax_version": "0.1.0",
  "configuration_revision": "…",
  "generated_at": "2026-11-10T09:45:00Z",
  "target": {
    "id": "dsh-main",
    "path": "/home/user/.dsh",
    "default": { "provider": "tmax", "id": "deepseek-v4.1-flash", "protocol": "responses" },
    "routes": ["tmax"]
  },
  "gateway": {
    "configured": true,
    "ready": true,
    "listen": "127.0.0.1:17840",
    "daemon_discovery_state": "compatible",
    "reason_code": null
  },
  "routes": [{
    "name": "tmax",
    "protocol": "responses",
    "base_url": "http://127.0.0.1:17840",
    "credential_ref": "TMAX_…",
    "models": [{
      "id": "deepseek-v4.1-flash",
      "protocol": "responses",
      "kind": "alias",
      "continuity": "stateless",
      "candidate_count": 2,
      "availability": "unverified",
      "capabilities": {
        "contextWindow": 500000,
        "input": ["text", "image"],
        "reasoningEfforts": { "max": "max" }
      }
    }]
  }],
  "blockers": []
}
```

## Outcomes

| Outcome | Exit |
| --- | --- |
| Projection built, including unknown/unavailable gateway or declared blockers | 0 |
| Unknown, non-DSH, or ambiguous target selection | 2 |
| Unreadable or invalid saved configuration | 4 |

`schema_version` is a consumer compatibility gate. A consumer that does not
recognize it must refuse the projection rather than guess field meaning.

## Connection

```text
tmax dsh connect [TARGET] [--home PATH] [--dry-run]
```

Target selection is identical to `projection`. The command requires a
configured gateway and at least one gateway route on the target; a route with
mixed protocols is refused instead of split. It writes only the stable
`credential_ref` names for those routes into the target DSH home's
`.credentials.yaml` `refs` mapping. It does not write a DSH patch, an
`llm-tmax` artifact, or a provider route, start a service, or resolve a secret
for a dry-run.

Execution:

1. Validates that the DSH scope exists, is outside tmax storage, and that an
   existing credentials document is a regular owner-only file using version 1.
2. Acquires the DSH credentials writer lock, re-reads the document, and merges
   only the target reference keys. Unrelated references, comments, and layout
   are preserved.
3. Resolves the configured gateway key at execution time and atomically
   replaces the document with owner-only permissions. The key never appears in
   the result, a log, or a command argument.
4. Re-running with the same key is idempotent and reports `changed: false`.

`--dry-run` checks the target, DSH home, file permissions, and planned
references without reading the credentials document or resolving the gateway
key. Its result reports `changed: null` and `effects.secret_reads: false`,
`effects.native_writes: false`, and `effects.locks_created: false`.

| Outcome | Exit |
| --- | --- |
| Connected, unchanged, or a dry-run plan | 0 |
| Unknown/ambiguous target, missing gateway, or a target with no gateway route | 2 |
| Unsafe/non-owner-only or invalid credentials document, unavailable scope, missing/invalid gateway key, or write failure | 4 |

```json
{
  "schema_version": 1,
  "target": { "id": "dsh-main", "path": "/home/user/.dsh" },
  "credentials": {
    "path": "/home/user/.dsh/.credentials.yaml",
    "state": "present",
    "permissions": "owner_only"
  },
  "references": [{
    "route": "tmax",
    "protocol": "responses",
    "credential_ref": "TMAX_…",
    "action": "written"
  }],
  "gateway": { "ready": true, "listen": "127.0.0.1:17840", "reason_code": null },
  "changed": true,
  "effects": {
    "native_writes": true,
    "secret_reads": true,
    "locks_created": true,
    "processes_started": false
  }
}
```
