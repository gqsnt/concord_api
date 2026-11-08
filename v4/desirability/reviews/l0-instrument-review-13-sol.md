# Review l0-instrument-review-13-sol (INSTRUMENT re-review: I3 fix 8, I4 fix 5, I1 reachability, on bc997a04)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T04:28:32Z–04:33:48Z, thread 01a1b96a-47d9-70e3-bb8c-5be0fcb71def.
**Verdict: ACCEPT_WITH_FIXES** (0 blocking, 4 non-blocking). Confirmed: I3 15 alias controls and
the 55 SDK shape checks pass, saved self-test 154/154 + 32/32; I4 36 scanner controls, 11 framing
controls, 7 omission stages and 1,112 saved logs reproduced, 40 validation records no semantic
difference, oracle-verdict AST and other verdict functions unchanged vs eb078a78, bank hash
e7d963b9 (141 files, 40 tasks), listed lexical limits do not feed `oracle_verdict`; I1 all five
self-test checks pass, all 55 public-view format-61 JSON files inspected (glob/module re-exports,
impl edges), no unreached generated endpoint fns, no missed impls on reached owners, the 13
zero-public-endpoint SDKs reproduced. Not executed: cargo, rustc, builds, runners, bwrap.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | I3: raw byte strings defeat collision detection (shared stripper) | non-blocking | known limit (README, `4e880e20`): no such shape in the 55 SDKs; a wrong alias that breaks compilation leaves R2 unjudged via positive controls |
| 2 | I3: macro-generated globs bypass the recursive check | non-blocking | known limit (same) |
| 3 | I3: allowlisted spellings / derives trusted | non-blocking | known limit (same) |
| 4 | I4: captured fake target section manufactures stage evidence before ambiguity is handled | non-blocking | fixed by coordinator `eb46bf2b` (ambiguity checked before evidence); control added, discriminates vs the round-5 rule; to be checked in the baseline review |

Decision: I1 reachability, I3 fix 8 and I4 fix 5 **accepted**. Per the stop rule recorded at
review 11, further diagnostic-attribution and alias-precondition edge cases are known limits
unless they reach a score or verdict. Instruments ready to bring to concord-desirability.
