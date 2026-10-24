# Native agent configuration references

These references describe **external, version-qualified native formats**, not a
universal tmax configuration schema. Stock pi and omp are separate agents. A
field with the same spelling does not establish the same validation, credential
precedence, or runtime behavior.

| Agent | Qualified installed version | Native model settings | Native authentication |
| --- | --- | --- | --- |
| [Stock pi](pi.md) | 1.2.0 | `models.json`, `settings.json` | Provider-keyed `auth.json` |
| [omp](omp.md) | 18.12.6 | `models.yml` / `models.yaml`, `config.yml` / existing `config.yaml` | SQLite `agent.db`, or native auth broker |
| [Codex CLI](codex.md) | 0.162.1 | `config.toml`, adjacent profile configuration | Native file/keyring/automatic credential store |
| [OpenCode V2](opencode.md) | 2.0.28 | `opencode.jsonc` / `opencode.json` | Server-scoped SQLite accounts; legacy file migration is separate |
| [Hermes Agent](hermes.md) | 0.22.1+683.g848fc0a | Profile `config.yaml` | Native `auth.json`, credential pools, `.env`, provider key sources |

The [shared integration contract](../../specs/integrations/spec.md#agent-boundaries)
owns observable behavior. The [routing request-parameter contract](../../specs/model-routing/spec.md#request-parameters)
owns sampling, thinking effort, and generation-budget preservation. The
[agent CLI contract](../../specs/cli/agents/spec.md) owns apply and recovery.
These pages do not redefine those contracts or authorize new provider routes.

## Qualification and evidence

Consulted on 2026-10-23. Each page identifies the exact source revision and the
installed behavior that was actually exercised. No supported version range is
inferred from one installation. Source-qualified means that a parser or consumer
implements a field; it does not mean that a provider accepted a request, a login
succeeded, or a subscription grants entitlement.

The retained five-agent acceptance used private passwd-home isolation and native
scopes outside the repository, synthetic local credentials, and loopback protocol
fixtures. No production native auth was imported. It observed installed parser
or local runtime consumption, actual tmax apply, and actual restore for all five
agents. Apply and restore reported `awaiting_native_reload`; a completed file
write is not proof that another running native process reloaded it. Exact-byte
restore and three-way preservation are different claims. See each page.

Concrete-metadata qualification is recorded separately from the earlier
five-agent acceptance. The OpenCode and Hermes references include bounded
2026-10-24 installed effective-consumer evidence; a pre-repair parser pass alone
does not prove metadata fidelity. Provider-authorized runtime evidence belongs
in the [provider contracts](../../specs/integrations/spec.md#provider-specific-contracts),
not in this field reference. In particular, a temporary Pi request-budget hook
in a GLM check is not native metadata or five-agent provider qualification.

## Safe scope and credential boundaries

Use an explicit absolute native scope. Native environment selectors are not a
replacement for real OS-home isolation: programs can still consult passwd home,
XDG roots, project files, external-login stores, plugins, or a running server.
OpenCode account operations additionally identify the intended server/database.
Hermes version inspection must use inert source provenance; its launcher version
path can bootstrap or update an installation.

The examples are finite synthetic native fragments, not instructions to replace
a user's complete files or change defaults, model roles, model scope, subagents,
sampling, or reasoning preferences. Key references name only environment
variables, commands, or private fixture paths. There are no portable OAuth-file
examples. Keep native credential payloads, command output, and SQLite credential
columns out of diagnostics.

Native login and refresh retain their native owner. A shared-scope login/logout
can affect other native consumers. Removing a model entry, restoring native
configuration, deleting a local key artifact, native logout, upstream revocation,
and cancellation of a subscription are distinct operations. tmax does not import
native credentials or copy OAuth tokens to another refresher.

Apply/restore resolve a referenced tmax secret only when the requested operation
requires the key. Current-user-restricted tmax secret files are plaintext, not
encrypted. An executed native key handoff can create a private, mode-0600
**plaintext native-owned** key artifact, referenced through the agent's own key
mechanism. New tmax recovery records also contain plaintext and are restricted
to the current OS account.

Native recovery operations accept only supported storage formats. Unsupported
recovery records remain unchanged and are unavailable to current recovery
actions. Dry-run and inspect disclose the native credential handoff without
resolving a secret. Later unrelated edits are preserved; conflicting later owned
edits or a changed credential artifact require explicit recovery rather than force overwrite.
