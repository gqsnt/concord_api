# Review l0-instrument-review-11-sol (INSTRUMENT re-review, I4 fix round 4, on 66edaeff)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T00:21:29Z–00:25:07Z, thread 01a1b888-192d-7552-97f3-c0a9ea548060.
**Verdict: ACCEPT_WITH_FIXES** (0 blocking, 2 non-blocking). Confirmed: both review-9 findings
fixed for their reproducers (recomputed from saved logs); `oracle_verdict` AST identical to
`9fb2423d`, measurement-verdict logic unchanged; 101-tree site audit reproduced in memory (only
the documented Strava/e5 baseline difference); 40 validation records 0 differences; seven
omission stages recomputed; subjects have 0 baseline matches and reach detecting failures;
Kraken/Sentry test-name evidence acceptable; bank hash e7d963b9… (141 files, 40 tasks) matches,
previous-bank diff = seven metadata changes only. Not executed: cargo, agents, runners, bwrap.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Comment blanking not token-aware (`run.py:1158`): strings containing `/*`…`*/` or a raw string with `" //` erase real code; `'"'` char literal before a `//` comment and nested block comments keep comment text (credit) | non-blocking | fix — I4 fix round 5: token-aware comment blanker (strings with escapes, byte/raw/C strings, chars vs lifetimes, nested comments, doc attributes); reviewer's probes as controls |
| 2 | Captured test output can alter failure-block boundaries (`run.py:1383`): a captured `failures:` truncates evidence; a captured `---- x stdout ----` creates a false block | non-blocking | fix — same round: validate block headers against the failed-test inventory; ambiguous framing → cannot be assessed; delimiter-collision controls |

Decision: I4 **ACCEPTED** for the baseline (verdict logic reviewed and unchanged; the findings
affect only diagnostic stage attribution). Round 5 fixes these two; any further edge cases of the
diagnostic attribution found later are recorded as known limits in the README rather than
starting new rounds (smallest coherent change), unless they reach the oracle verdict.
