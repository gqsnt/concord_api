# Direction lane brief (P3.5) — Claude subagent

A direction is a coherent shape of Concord after several ideas land together. You build one combined
spike on the 3–5 SDKs that best show it, and describe the direction. Same rules as
`/home/canarit/projects/Concord_Next/work/design/SPIKE_BRIEF.md` (charter C1–C9, worktree only,
`--locked`, build only what you touch, never weaken a test, no negation statements, simple controls
must not grow, commit nothing, ≈2 hours ceiling). Return your report in your final message (you
cannot write report files).

Start from the spike branches the coordinator names (merge them into your worktree; resolve
conflicts minimally) or from the target files when an idea has no spike. Prefer the smallest
combined mechanism: if two ideas need the same carrier, use one.

Report, in this order:
1. **Shape** — the declaration as an author writes it for one witness SDK (before → after), and the
   call site (before → after).
2. **What disappears** — per SDK, with `measure.py` numbers (glue, model+behavior, DSL lines,
   negations) and the hand count of removed source; name the files that shrink.
3. **The simple case** — Nager, PokeAPI, Frankfurter: byte-identical or what changes.
4. **Costs** — concepts added (M3), breaking changes (complete list), compile-error quality (M11:
   5 seeded mistakes), docs words if they change (M7), what you could not make work.
5. **What it rules out** — designs or future ideas this direction makes harder.
6. **Tests** — commands and counts; every changed expected value with its reason.
7. **Assessment** — 3 lines: does the combination remove more than the parts, and does the
   declaration read better?
