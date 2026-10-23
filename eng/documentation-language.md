# Documentation language

These rules govern durable documentation in `specs/`, `docs/`, and `eng/`.

## Language

- Write in English.
- Use short sentences and paragraphs, active voice, American spelling, and stable terms.
- Use ASD-STE100 writing rules as a target; do not claim formal compliance.
- Keep domain terms, identifiers, field names, commands, serialized values, and
  code symbols in their exact spelling.
- Use `must`, `may`, and `must not` for requirements, options, and prohibitions.
- State one rule per sentence.
- Write only statements that an implementation can be checked against. Delete
  generic motivation, repetition, and implementation tours.
- Give each fact one authoritative home. Define a term once and link to it.

## Structure

- Use numbered steps for a linear procedure.
- Product specifications state user-visible behavior. Design specifications state
  boundaries, invariants, and the trade-offs they protect.
- Describe the target state. Record current implementation gaps in the owning
  document instead of weakening the target contract.
- Explain product and domain concepts before implementation details. Do not turn
  classes, methods, call chains, or storage steps into prose.

## Examples and diagrams

- Specification writing and presentation are defined in
  [specs/AGENTS.md](../specs/AGENTS.md).
- Mark pseudocode as non-executable. Executable commands and examples must run or
  parse as written.
- Keep diagrams as renderable text in Markdown. Do not use raw HTML.
