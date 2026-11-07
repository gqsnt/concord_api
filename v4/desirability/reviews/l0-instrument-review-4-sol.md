# Review l0-instrument-review-4-sol (INSTRUMENT re-review, I4 only, on d8327180)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed), 2026-11-07T22:40:14–22:45:20Z.
**Verdict: REJECT** (5 blocking, 1 non-blocking). Confirmed: the mount/PID layout hides the original
bank, sibling worktrees and host transcripts; 40 fail-before/pass-after records and 17 controls
supported; Strava e2 / Sentry e4 gaps closed.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Host git reads agent-writable .git/config (external diff/fsmonitor outside bwrap) | blocking | fix — I4 fix round 2 (diff a copied tree vs immutable baseline under trusted git config inside bwrap; .git/.cargo tampering invalidates) |
| 2 | Bank reachable over the open network once published | blocking | fix — bank sealed outside the repo (hash committed); L0 enters the public branch as a squashed commit so no bank-bearing history is published |
| 3 | Cargo exit 0 credits unexecuted tests (agent runner) | blocking | fix — agent .cargo removed, trusted CARGO_HOME, expected test names must run and pass; no-op runner control |
| 4 | Kraken e4 options compared with the submitted prepare() | blocking | fix — literal assertions; drop-one-option control |
| 5 | Kubernetes e4 literal query order | blocking | fix — parsed pairs; audit all 40 for unprescribed ordering |
| 6 | Semantic scoring depends on reference spelling | non-blocking | fix — "needs review" distinct from "missing" |
