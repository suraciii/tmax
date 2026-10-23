# Specification Instructions

## Structure

```text
specs/
├── AGENTS.md              # This file
└── {topic}/
    ├── spec.md            # Required — product behavior
    ├── design.md          # Optional — boundaries, invariants
    └── {sub-topic}/       # Same shape, nested on demand
```

## Rules

- Follow [documentation language rules](../eng/documentation-language.md).
- Write durable target state only.
- Keep one fact in one authoritative document; link instead of duplicating rules.
- `spec.md` states observable product behavior, not implementation or test procedures.
- Define shared domain terms in [CONTEXT.md](../CONTEXT.md). Define concepts
  specific to one capability in its owning spec and link to shared terms.
- Higher layers constrain lower layers. Never the reverse.
- Do not use `docs/` as a competing product or design authority.
- **Authority** (optional) — name the owning capability and link to dependencies.
- **Laws** (optional) — state invariants that must always hold.

## Presentation

Express the target contract visually. Skip preambles and keep prose brief.
Choose the smallest form that makes the rule clear:

| Content | Form |
| --- | --- |
| Branching behavior or decision order | Non-executable, contract-level pseudocode |
| States and transitions | Mermaid state diagram |
| Interactions or data flow | Mermaid sequence or flow diagram |
| Ownership or hierarchy | Shallow text tree or Mermaid |
| Fields, independent conditions, or comparisons | Markdown table |
| Simple requirement or exception | Short sentence |

- Diagrams and pseudocode state the contract; do not repeat them in prose.
- Make conditions, outcomes, scope, and failure effects explicit.
- Describe observable rules, not implementation calls or source-file layouts.
- Keep only relevant actors, states, branches, and boundaries.
- Put short scope statements and constraints next to the visual they qualify.
- Show enough of the target state to preserve ownership and order.
- Keep visuals in the owning Markdown document.

## Context layers

- **Domain language** ([CONTEXT.md](../CONTEXT.md)) → what each term is.
- **Specification documents** (`spec.md` / `design.md`) → what the product does.
- **Code** → the implementation.

## Ownership

A capability directory is owned by the capability it names. Its `spec.md` owns
user-visible behavior; its optional `design.md` owns boundaries and invariants.
Write each rule in its owning document and link from dependent capabilities.

## See also

- [Specification index](README.md).
- [Product contract](product/spec.md).
