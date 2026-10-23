# Contributing to tmax

Thank you for contributing. This guide covers the contribution process. Build
requirements, verification, and the full command list live in
[Building and verification](docs/building.md).

## Agent skills

Use [`/to-spec`](.agents/skills/to-spec/SKILL.md) to write a spec following
[specs/AGENTS.md](specs/AGENTS.md).

## Before you open a pull request

- Search open issues and open pull requests for duplicates. Link the closest one,
  or write "none found".

## Commits

- Follow [Conventional Commits](https://www.conventionalcommits.org/) with a
  scope, for example `fix(cli): reject unknown arguments with a usage message`.
- Add a Developer Certificate of Origin sign-off with `git commit -s`.
- Use squash merges into `main`, with the pull request title as the commit
  subject.

## Gates and evidence

- `just ci-pr` is the required pull request gate.
- `just ci` runs the full local gate: everything in `ci-pr` plus the release
  build.
- `just verify-pr <dir>` and `just verify <dir>` retain gate output with source
  provenance. The evidence directory must be outside the repository.

Automated checks are necessary but not sufficient. Exercise the changed behavior
through the real entrypoint and report the exact scenario and result in the pull
request. The required evidence depends on the change:

| Change | Required evidence |
| --- | --- |
| CLI behavior | A run through the real binary with the affected arguments |
| Verification tooling | Isolated regression tests and a real gate run with evidence |
| Documentation | Resolve relative links and execute documented commands |

## Standards

- [Coding standards](CODING_STANDARDS.md) — reviewer judgment rules.
- [Test architecture](eng/testing.md) — test requirements and organization.
- [Workspace layout](eng/architecture.md) — crate boundaries.
- [specs/](specs/AGENTS.md) — product contracts; [docs/](docs/README.md) — user guides.

## Language

Issues, pull requests, commits, and documentation are written in English. See
[documentation language](eng/documentation-language.md).

## License

The program is [GPL-3.0-only](LICENSE). Adapted engineering conventions retain
their source project's GPL version 3 terms. Third-party skill provenance and
license notices are retained in [attribution](attribution/NOTICE).
No contributor license agreement is required.
