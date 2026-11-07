# PR #5 merge receipt (SDK50 → main) — first record of the desirability loop

Owner instruction 2026-11-07 (in chat): "finish what needs to be finished before the loop → merge
PR #5 → loop". Grant G4 of the mission's AUTHORITY: merge PR #5 with a merge commit when every F5
precondition holds; keep the branch.

| What | Value |
|---|---|
| PR | gqsnt/concord_api#5, `concord-ecosystem-sdk50` → `main`, merge commit, branch kept |
| Main before | `8cae5d4cda10d47fb6036ac72ba54256e7a5536b` (unchanged since the campaign branch started) |
| PR head at start of close | `c1b75b95387baf77cee9d93cf809d7ca7a64fd8f` (Windows `verify` failing: Xero `first_use`) |
| Closing commits | `af48bd1c` Xero journal on Windows; `4629d0e0` records state; `eecb46fa` CLOSING.md; `120727f6` README; `7bd49dc4` CHANGELOG; `5523a2a0` MIGRATION; `d5e796eb` F4 review corrections; `7eb093b4` Xero Windows test (each fast-forward pushed, grant G2) |
| Verified head | `7eb093b41585f378165d2397ca4a98e477c14003`, tree `4b71e00abb2cbaedd058ec35d2fbae3330b974f8` |
| Local gate on the head | `scripts/verify.sh --package --bench` 66/66 PASS, clean tree, rustc 1.99.1, 6m49s (warm slot `work/targets/gate`, first built cold on `120727f6` in 15m26s, 66/66); `5523a2a0` and `d5e796eb` also 66/66 (`d5e796eb` after one rerun: its first run failed `test-multipart-only` on a port-reuse race while other test servers ran; isolated rerun 3/3 PASS) |
| CI on the head (push run 42413881751) | `verify (ubuntu-latest)` success (3,859 tests), `verify (windows-latest)` success on the native Windows runner (3,857 tests: two Unix-only tests fewer), `nightly-clone` success; job logs `head: 7eb093b4… dirty: 0` |
| CI on the synthetic merge (pull_request run 42413885682) | same three jobs successful |
| Independent review (F4) | Codex `gpt-6.1-sol` xhigh read-only (observed from rollout metadata): F-CLOSE on `5523a2a0` ACCEPT_WITH_FIXES (6 non-blocking factual findings, applied in `d5e796eb`); recheck of `d5e796eb` ACCEPT; recheck of `7eb093b4` ACCEPT |
| Merge | `gh pr merge 5 --merge --match-head-commit 7eb093b4…` at 2026-11-07T22:14:14Z, after `gh pr ready 5` and a description rewritten from the records (grants G3, G4); merge commit **`04ac9902025d46c1ef56a1064929fd41c5a36eb7`** |
| Merge verification | parents `8cae5d4c` (old main) and `7eb093b4` (verified head); merge tree `4b71e00a` equals the verified head's tree; `concord-ecosystem-sdk50` kept at `7eb093b4` |
| Local gate on main | `scripts/verify.sh --package --bench` on `04ac9902`: 66/66 PASS, clean tree, 7m03s, **reusing the warm gate slot** (identical tree; cached artifacts, stated) |
| CI on main (push run 42419662300) | `verify (ubuntu-latest)` success (3,859 tests), `verify (windows-latest)` success on the native Windows runner (3,857 tests), `nightly-clone` success; each job log `head: 04ac9902… dirty: 0`. **F6 complete.** |

## In-flight work at close (F1 summary)

From `v4/sdk50/CLOSING.md` § "In-flight work at close": four x50 prototypes (`x50-docs`,
`x50-pages-start`, `x50-adopt`, `x50-transport`) and two b34 prototypes are class C (kept on
their branches; the x50 ones seed hypotheses H04, H09a, H11, H10); `modular-docs` is class A (an
equivalent patch is in main); nothing was class B or D. The owner's six-run docs trial is H04/H03
evidence. Nothing was deleted.

No tag, release or publication. `concord-desirability` starts at the merge commit; this receipt
is its first commit.
