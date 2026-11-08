# Review l0-instrument-review-8-sol (INSTRUMENT re-review, I3 only, on 6638e5b6)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed), 2026-11-07T23:42:05–23:44:48Z.
**Verdict: REJECT** (2 blocking). Confirmed: the three review-6 reproducers addressed (std path
table, whole-type syn parsing, qualified macros + reader sentence); all earlier mechanisms intact;
the stricter reader contract acceptable.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Lifetime arguments discarded (`Error<'static>` credited) | blocking | fix — I3 fix round 6: R2 correctness decided by rustc (exact type identity via `same::<Truth, Answer>` in the SDK module scope, sandboxed); the identity scorer becomes a diagnostic |
| 2 | Alias expansion drops surplus arguments (`DeploymentList<u8>` credited) | blocking | fix — same (rustc checks arity) |

Coordinator note: eight rounds show the hand-written normaliser keeps reimplementing Rust's type
system; round 6 replaces it with the compiler as the oracle.
