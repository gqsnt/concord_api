# Review l0-instrument-review-2-sol (INSTRUMENT re-review, on 343fec74)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed, rollout turn_context),
2026-11-07T21:31:08–21:37:46Z. **Verdict: REJECT** (5 blocking, 3 non-blocking). Confirmed fixed
from review 1: S3 SigV4 recomputation, ClickHouse username, I1 classifier fixes, PokeAPI material
filter, direct wrong-owner rejection, runtime sandbox basics, USGS fix, lane-4 canaries sampled.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | R3: compile outside sandbox (`include!`/`option_env!`), host Unix sockets reachable | blocking | fix — I3 fix lane 2 (sandboxed compile with allowlisted env, `--tmpfs /run`, discriminating controls) |
| 2 | R3: named discard / `.ok()` / `send_raw` credited | blocking | fix — I3 fix lane 2 (answer = the send-result expression bound to the declared type; controls) |
| 3 | R2: inner `concord_core` segment collapses invented owners | blocking | fix — I3 fix lane 2 (complete-prefix matching; nested wrong-owner controls) |
| 4 | I4: oracles reachable via git objects / sibling worktrees; exposure not invalidating | blocking | fix — I4 fix lane (isolated export + bubblewrap hiding ~/projects; exposed sessions invalid) |
| 5 | I4: Strava e2 / Sentry e4 oracles accept incomplete changes | blocking | fix — I4 fix lane (strengthen, audit all 40, partial-omission controls) |
| 6 | I4 semantic-site scoring follows reference layout | non-blocking | I4 fix lane if cheap |
| 7 | I1 reachability (owner of a public method may be unreachable) | non-blocking | coordinator, when rustdoc JSON is produced (enforce no-private-items input; owner reachability via `paths`) |
| 8 | I5 Config exemption; compiling caller cases deleted | non-blocking | **fixed** `d0c2de8d` (UI 93/102; caller 25/30) |
