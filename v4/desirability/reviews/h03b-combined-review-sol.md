# Review h03b-combined-review-sol (COMBINED review, H03b: candidate + measurement + decision; A3 §5)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T19:49:10Z–19:55:52Z, thread 01a1bcb5-25cc-7732-91a7-920aefb1cf04.
(Candidates and measurement by Claude subagents: cross-provider.)
**Verdict: ACCEPT_WITH_FIXES** (4 non-blocking). **DECISION: KEEP_CURRENT** — +18.596 points misses
the frozen ≥ 20-point threshold; no LOSS or INCONCLUSIVE trigger takes precedence. **Deciding item:**
witness corpus 134/285 → 187/285 for A, B, A′ and B′; +53 passes, +18.6 points, zero regressions;
baseline `a883f7bb` retained. **Family:** INPUT closes (H03 LOSS, then H03b's determinate
KEEP_CURRENT; A1's INCONCLUSIVE exemption does not apply). Stop 3: second consecutive non-accept after
H08's WIN — not triggered.
Confirmed: A′ repairs (unraw, IntoValue docs, `.parse()` wording, K1–K4 documented and tested); the
four repository sites are K1, no other rejection; B′ narrowing and feature gating; storage and wire
generation unchanged; tests not weakened (3963/3966). Corpus regenerated in memory: all 343 snippets,
references and generation records match; all 31 frozen files rehashed; chronology (card before
results, freeze before non-reference compiles); every arm's verdicts, repetitions, regressions and
controls recalculated; I5 diagnostics and cost medians rescored; 36 removals lack intended requests
(+16.9 if counted as failures); missing `Default` impls explain the two absent card examples; S7's
integer failures occur at baseline too. No instrument finding changes the verdict. The gains are real
inference improvements (the "only renames obligations" kill does not fire).

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Record KEEP_CURRENT with the INPUT closure and the Stop-3 count; neither candidate adopted | non-blocking | applied in measurements/H03b.md and DECISIONS |
| 2 | I5 resolutions establish request equivalence under modified setup (Kraken private test policy, GitHub `.labels()` completion); `200 {}` replies produce decoding errors — disclose; keep the four resolutions and B′'s E0283 labels as non-deciding observations | non-blocking | applied |
| 3 | "0 mismatches" does not independently validate Kraken's `API-Sign`; keep the lane's signature observation explicitly unverified | non-blocking | applied (queued as an unverified observation, not a defect) |
| 4 | Disclose that corpus reuse is not independent confirmation and that human attraction, preference and adoption are unmeasured | non-blocking | applied |
