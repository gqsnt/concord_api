# Review l0-instrument-review-9-sol (INSTRUMENT re-review, I4 only, on f81caeac)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-07T23:57:26Z–2026-11-08T00:01:12Z, thread 01a1b872-161a-7262-bdc1-d612968ef202.
**Verdict: ACCEPT_WITH_FIXES** (0 blocking, 2 non-blocking). Verified review-7 findings 1–3 fixed
(link-free copy, exclusive no-follow oracle install, read-only measurement binds, post-run oracle
hashing, build-time/harness invalidation, forging scan, independent expectations in the changed
oracles), finding 5 fixed; recomputed the sealed bank hash af768919… (141 files, 40 tasks); judged
the extra invalidation rules reasonable and the documented in-process residual acceptable for
cooperative agents with diff review and fresh measurement targets. Not executed by the reviewer:
cargo, agents, bwrap, attack controls (assessed from source and saved records).

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Comments can earn semantic-site credit (`run.py:1245`, classification presence `:1478`): a comment-only `// TODO: admit /0/private/BalanceEx` credits the signing classification | non-blocking | fix — I4 fix round 4: match behavioural facts and classification presence on comment/doc-stripped code; comment-only control |
| 2 | Generic detection evidence (`run.py:1376`): Kraken's evidence term `SigningInput` matches an unrelated Balance failure, attributed to "existing tests" | non-blocking | fix — I4 fix round 4: evidence must name the new operation/type within the same failure block, else "cannot be assessed"; collision control |

Decision: I4 **accepted** for the baseline once round 4's controls pass (the findings affect only
the diagnostic stage attribution, never the oracle verdict); round 4 is re-checked in the review
of the squashed instruments.
