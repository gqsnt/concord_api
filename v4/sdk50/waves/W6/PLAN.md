# Wave 6 — SDK46–50 (held-out final first pass)

Library baseline: `c3fd4f1e` (owner instruction R-2). This wave is held out: its contracts were
known from the cohort, but no adapter, fixture or shared change was prepared for them, and their
roles receive **no slice notes** — only the cohort contract in the standard capsule (no
capability-crate hints, no campaign findings). Capsules forbid reads outside the worktree (P-1).
First results are preserved; later fixes are corrected results and are retested.

| SDK | Author | Fresh consumer | Maintainer |
|---|---|---|---|
| SDK46 | Opus | Sol 6.1 | Sol 6.1 |
| SDK47 | Sol 6.1 | Opus | Opus |
| SDK48 | Opus | Sol 6.1 | Sol 6.1 |
| SDK49 | Sol 6.1 | Opus | Opus |
| SDK50 | Opus | Sol 6.1 | Sol 6.1 |

(Rotation R-1 continues from SDK45 Sol.) Live provider calls: NOT_RUN (no paid calls; the
Anthropic Messages slice never calls the API).
