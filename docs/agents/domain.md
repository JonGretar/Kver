# Domain docs

Kver uses a single-context layout: `GLOSSARY.md` at the repository root and
ADRs under `docs/adr/`.

Before exploring a relevant area, read the glossary and ADRs linked from the
active issue or relevant architecture section. Do not read the whole ADR
archive by default. If these docs do not exist yet, proceed without creating
empty files.

Add a glossary entry when a domain term is settled. Before creating an ADR,
apply the [ADR criteria](../adr/README.md); brief rationale normally belongs
in the existing architecture doc.

Use glossary terms consistently in issues, design discussions, hypotheses,
and tests. Flag a proposed change that conflicts with an existing ADR rather
than silently overriding it.
