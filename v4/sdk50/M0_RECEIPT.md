# M0 — SDK20 integration receipt

The handoff authorized merging PR #4 (SDK20) after verification. Done 2026-11-07 00:06 CET.

| What | Value |
|---|---|
| PR | gqsnt/concord_api#4, `concord-dx-sdk20` → `main`, ordinary merge commit, branch kept |
| Handoff pin | `90b69011db92c8b900c23dd0c35176ad100d1204`, tree `6a654331…`; equal to the remote head at start (2026-11-06 23:04 CET) |
| Main before | `a95fb2fb7548a719b0058da2032f59ecf4d069ea` (an ancestor of the pin) |
| Added before merging | `a57f020a` (an earlier session's unpushed SDK20 records: final-candidate section, delivery helpers) and `5c3ef5f6` (registry fix, below) — pushed as a fast-forward |
| Blocker found and fixed | registry token cache: `sent + Duration::from_secs(expires_in)` panicked for `expires_in = 18446744073709551615` (`overflow when adding duration to instant`, `client.rs:258`), reproduced by `examples/distribution-registry-sdk/tests/protocol.rs::an_unrepresentable_token_lifetime_is_a_token_response_error` before the fix; now `Instant::checked_add` → `AuthProblem::TokenResponse`, token neither cached nor sent |
| Verified head | `5c3ef5f6ecff6ada6d16b7c047e2ffeb78ffab4f`, tree `da1de36da27ea77a599bacc6af0cff523a84bff9` |
| Local gate on the head | `scripts/verify.sh --package --bench` 49/49 PASS, clean tree, 10m00s (cold), rustc 1.99.1; `--nightly-only` PASS (`nightly-clone-compat` 16 passed; local nightly 1.101.0-nightly e9bbadf68 2026-10-12) |
| CI on the head (push run 42303147393) | `verify (ubuntu-latest)` 49 PASS, `verify (windows-latest)` 49 PASS (native Windows runner), `nightly-clone` with `nightly-clone-compat` 16 passed (rustc 1.101.0-nightly 249c1e4e3 2026-11-05); `head: 5c3ef5f6… dirty: 0` in each job log |
| CI on the synthetic merge (pull_request run 42303151752) | same three jobs, same step counts, on `7e7a34ac` |
| Merge | `gh pr merge --merge --match-head-commit 5c3ef5f6…`; merge commit **`8cae5d4cda10d47fb6036ac72ba54256e7a5536b`**, parents `a95fb2fb`, `5c3ef5f6`; tree `da1de36d` = the verified head's tree |
| Local gate on main `8cae5d4c` | `scripts/verify.sh --package --bench` 49/49 PASS, clean tree, 3m39s, **reusing the M0 target** (identical tree; cached artifacts, stated); `--nightly-only` PASS (16 passed) |
| CI on main (push run 42309603283) | `verify (ubuntu-latest)` 49 PASS, `verify (windows-latest)` 49 PASS (native Windows runner), `nightly-clone` with `nightly-clone-compat` 16 passed (rustc 1.101.0-nightly 249c1e4e3 2026-11-05); each job log `head: 8cae5d4c… dirty: 0`. **M0 complete.** |
| Logs (outside the repository) | `work/logs/m0-gate-5c3ef5f6-*`, `work/logs/main-8cae5d4c-*` |

No tag, release or publication. The campaign branch `concord-ecosystem-sdk50` starts at `8cae5d4c`.
A records-only main update was not needed: this receipt lives on the campaign branch.
