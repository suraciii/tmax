# Calling-agent skill

`tmax skill install [--agent hermes] [--dry-run]` installs the bundled,
model-invoked `tmax` entry skill. It guides the calling agent through
`tmax --help` and discovered subcommand help for operations, usage scenarios,
and sensitive-input boundaries; command syntax remains authoritative in CLI help.
The skill ships inside the executable and requires no checkout or network.

| Selection | Destination |
| --- | --- |
| No `--agent` | OS user's `~/.agents/skills/tmax/SKILL.md` |
| `--agent hermes` | Native Hermes home plus `skills/tmax/SKILL.md`; default OS user's `~/.hermes/skills/tmax/SKILL.md` on Unix |
| Other `--agent` values | Argument error; no effects |

Hermes uses its active home's `skills` directory ([native loader](https://github.com/NousResearch/hermes-agent/blob/848fc0a/tools/skills_tool.py)).
The installer supports literal absolute and cwd-relative `HERMES_HOME` paths and
leading `~/` using the OS user's home, independent of `HOME`. Environment-variable
expressions must be expanded by the caller; parent traversal and `~other-user`
forms are rejected rather than installed into a different scope. Set
`HERMES_HOME` explicitly to install into a profile. Without it, native defaults
honor `HERMES_DATA_DIR_SUFFIX` (one directory-name suffix); Windows uses
`LOCALAPPDATA/hermes` or OS user's `~/AppData/Local/hermes`. Sticky profile selection
and managed scope are not inferred. This is native skill scope, not selection
of tmax's configuration authority.

| Observed destination | Execution | Dry-run |
| --- | --- | --- |
| Absent | Create complete skill without replacing a concurrent entry; `state: installed`, `changed: true` | `state: would_install`, `changed: false`; create nothing |
| Identical regular current-user-owned file | `state: already_installed`, `changed: false`; preserve file identity and permissions | Same |
| Different existing content | Block with `inputs_changed`; preserve exact content | Same blocker |
| Symlink, nonregular entry, foreign-owned native directory/file, group/world-writable native directory on Unix, or unavailable storage | Error without replacing that entry | Same blocker |

Results use the [shared envelope](../results/spec.md) and report `skill`, `agent`
(`agents` by default or `hermes`), resolved absolute `path`, `state`, and `changed`.
Identical installation and ready dry-run exit 0; invalid arguments exit 2;
conflicting content exits 3; unsafe/unavailable storage exits 4. A storage failure
after publication may leave the complete installed skill; report failure rather
than claiming durable completion. Unrelated files/directories are preserved.

Installation neither reads nor writes tmax configuration, credentials, services,
or agent settings and does not launch an agent. There is no force/update/uninstall
interface. To resolve a content conflict, move the existing skill aside explicitly
before installing. To preview and install:

```sh
tmax skill install --dry-run
tmax skill install
tmax skill install --agent hermes
```
