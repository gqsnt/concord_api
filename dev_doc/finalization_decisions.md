# v2.1 finalization decisions

> **Historical decision worksheet — not current source of truth.** Selected
> meanings are owned by the active specification and ADRs; release state is
> owned by [`release_gate_status.md`](release_gate_status.md).

The following decisions must be resolved from prototype evidence before release freeze.

| Decision | Initial target | Evidence | Finalization rule |
|---|---|---|---|
| representation trait/driver shape | generic directional pair with sealed core driver | Gate 1 passed mechanically | selected for prototype: exact pair traits plus a core-sealed acquisition driver |
| inherited representation | client/scope default + inline exception + fence | Gates 2–3 | retain only if effective origin is reliably discoverable and diagnostics local |
| optional `with_x` | removed | Human evidence was unavailable; ADR 0012 applies the mandated smaller-surface fallback | v2.1 exposes only exact `set_x`, `omit_x`, and `reset_x` controls |
| query words | `when_present`, `append`, `each` | Gate 3 | publish one spelling only |
| stream erasure | private boxed stream carriers | Gate 8 comparative evidence passed | retained: one construction allocation, zero polling allocations, throughput and size budgets met |
| observer events | attempt started, response head, operation finished, pagination finished, stream finished | Gates 5 and 9 passed mechanically; operator task review unavailable | retain minimum read-only set for prototype; no mutation authority |
| optional static secret | deferred from v2.1 | no owner requirement, implementation, or human/product evidence; ADR 0010 | reject with alternatives; revisit only for a representative multi-auth API |
| contract metadata size | static/effective runtime-accessible descriptions | Gate 10 machine evidence passed through 1,000 endpoints; independent human first-screen review unavailable | retain for the RC; stable freeze remains blocked with the rest of Gate 10 |
| project license | no license grant exists in repository or observed history | `cargo deny` rejects all four public packages; package archives otherwise pass downstream checks | project owner must select and supply the license; do not invent a grant or self-waive the check |
| aggregate body reuse | deferred | pagination/body benchmark | do not add unless explicit capability earns measured value |
| release version | 2.1.0 | all gates | bump only when active repository is one coherent target and migration path exists |

No finalization decision is made by retaining both alternatives as public aliases.
