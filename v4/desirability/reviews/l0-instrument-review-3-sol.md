# Review l0-instrument-review-3-sol (INSTRUMENT re-review, I3 only, on 78571132)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed), 2026-11-07T22:11:23–22:16:56Z.
**Verdict: REJECT** (3 blocking, 1 non-blocking). Confirmed: compile/home/environment and IPC
exposures addressed by construction; four wrong-owner controls reject on 86 items; sealed/final
hashes unchanged, only hashes tracked.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | R3 early `return` bypasses the typed binding | blocking | fix — I3 fix round 3 (single-expression answers; reviewer's case as a control) |
| 2 | Stream items get primary R3 credit without a checked result | blocking | fix — round 3 (check stream Result with valid fixtures, or exclude from primary) |
| 3 | R2 erases module names (`crate::models::Error`) | blocking | fix — round 3 (real module/definition index; wrong-owner controls from existing modules) |
| 4 | Call-form filter rejects parenthesised/block forms | non-blocking | fix — round 3 |
