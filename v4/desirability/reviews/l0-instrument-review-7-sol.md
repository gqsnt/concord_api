# Review l0-instrument-review-7-sol (INSTRUMENT re-review, I4 only, on 7f5a83e6)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed), 2026-11-07T23:19:18–23:24:05Z.
**Verdict: REJECT** (3 blocking, 2 non-blocking). Confirmed: bank hash ca093132… (139 files, 40
tasks), public meta matches, 40 fail-before/pass-after, 96 oracle tests, 40 no-op runners and 18
bad controls rejected; git isolation, literal options and query multisets fix their defects;
squashed publication is sufficient if bank-bearing history stays unpublished (no remote ref
contains it).

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Host-side oracle install follows a planted symlink (host file truncation) | blocking | fix — I4 fix round 3 (no symlinks in the copy; O_NOFOLLOW install inside confinement; sentinel control) |
| 2 | Build scripts can rewrite the oracle; library code can forge harness output | blocking | fix in layers — read-only oracle/harness binds, build.rs/build-deps/proc-macro changes invalid, forging-pattern scan, residual in-process risk documented; controls |
| 3 | Kraken e4 signature expectation calls the editable `Credentials::signature` | blocking | fix — independent HMAC computation; wrong-signing-path control; audit all 40 |
| 4 | Classification detection attributed from unrelated compile failures | non-blocking | fix — classification-specific evidence only |
| 5 | Old-pattern match in a comment produces "missed propagation" | non-blocking | fix — review candidate |
