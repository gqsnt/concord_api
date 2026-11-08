# Review l0-instrument-review-12-sol (INSTRUMENT re-review, I3 only, on 8dc50be7)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T00:35:26Z–00:39:24Z, thread 01a1b894-df0d-7292-91e6-a98f4961d4f3.
**Verdict: ACCEPT_WITH_FIXES** (0 blocking, 2 non-blocking). Confirmed: both review-10 findings
fixed for the current SDKs (crate-root aliases give absolute paths; the invariant marker rejects
higher-ranked subtyping and keeps identical types, renamed binders, reordered bounds); no new
refusal of a valid natural exact-type answer; alias insertion keeps crate attributes and adds no
`std` to a `no_std` crate; saved full self-test 154/154, 29/29, 172 positive controls; earlier
properties intact. Not executed: builds, rustc, runners, bwrap (assessed from source and saved
artifacts; collision cases via read-only Python probes).

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Alias collision detection incomplete (`score.py:298`): glob-imported `alloc`, macro-generated or `#[cfg(test)]` items missed; nested/renamed/cfg-disabled `extern crate alloc` taken as an existing root alias. None occurs in the 55 SDKs | non-blocking | fix — I3 fix round 8, fail closed: any SDK outside the supported shape (see #2) is recorded as "R2 not scorable: alias precondition" instead of scored; controls on synthetic roots |
| 2 | Injection assumes the default lib target (`score.py:322`): `[lib] path`/`name` would inject into an unused file or the wrong alias. Current SDKs use defaults | non-blocking | fix — same round: read the actual lib target path/name from `cargo metadata` (already used?) or refuse non-default targets as unsupported |

Decision: I3 **ACCEPTED** for the baseline. Round 8 adds fail-closed guards (the loop may change
SDK roots, so a silent precondition failure must not become a score); it is checked in the
combined review with I4 fix round 5 and I1 reachability.
