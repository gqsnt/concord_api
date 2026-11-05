# Concord 4.4 extensions: verification

Results are credited only to the exact commit they ran on. Local runs: Linux x86_64,
`rustc 1.99.1 (58cf8f9c3 2026-10-13)`, the maintained `scripts/verify.sh`, in a clean detached
worktree that was not edited while the gate ran. CI: GitHub Actions `ci` (`scripts/verify.sh
--package --bench` on `ubuntu-latest` and native `windows-latest`; the non-blocking
`nightly-clone` lane, whose step result and test summary are read from the job log, not inferred
from the job status).

The maintained inventory grew from 37 to 43 steps for the new packages and features:
`test-extras-stream`, `test-extras-csv`, `test-extras-jsonl`, `test-pagination`, `doc-streaming`,
`doc-pagination`; `clippy-features` covers `stream`, `csv` and `jsonl` alone; `graph` guards
`concord-pagination` and the new `concord-extras` edges; `package-check` packages
`concord-pagination` and accepts nested `src/` modules; every new `examples/*` package is run alone
by `test-examples-isolated`.

## Local

| Commit | Command | Result |
|---|---|---|
| `e0d03d8` (pre-rewrite candidate) | `scripts/verify.sh --package --bench` | 42/43 PASS; FAIL `clippy` (`manual_range_contains` in the test server's lifecycle test, fixed in the next commit); workspace 1,267 tests, examples each alone 450 |
| `0dd6550` (first pushed candidate, reviewed) | `scripts/verify.sh --package --bench` / `--nightly-only` | 41/43 PASS; FAIL `clippy` and `clippy-features` (three stream-stress lints; `ReadError::recoverable` unused with `csv` alone), fixed in `297b95e`; workspace 1,279, examples 450 / PASS `nightly-clone-compat`, 16 passed |
| `d2681065` (all review fixes integrated) | `scripts/verify.sh --package --bench` / `--nightly-only` | **43/43 PASS**; workspace 1,322 tests, examples each alone 459; seven downstream consumers in path and packaged modes / **PASS** `nightly-clone-compat`, 16 passed. Gate worktree `wt-gate-e0d03d8` moved to this commit (`git status` empty before and after); its build cache was reused because the disk had 42 GB free and a cold gate had used about 45 GB (logs `local-d268106-gate.txt` and `local-d268106-nightly.txt` are in the delivery ZIP under `evidence/`) |

## CI

| Commit | Run | Linux | Windows (native) | Nightly lane (step result) |
|---|---|---|---|---|
| `0dd6550` | [41957773882](https://github.com/gqsnt/concord_api/actions/runs/41957773882) | FAIL `clippy`, `clippy-features` (as locally) | FAIL `clippy`, `clippy-features`, and `test` / `test-examples-isolated` (`concord-example-kubernetes-app` fixture manifest: CRLF checkout changed the hashed `.json-values` fixtures) | PASS `nightly-clone-compat`, 16 passed |
| `297b95e` | [41958244949](https://github.com/gqsnt/concord_api/actions/runs/41958244949) | **43/43 PASS** (workspace 1,279, examples alone 450) | FAIL `test`, `test-examples-isolated` (the same Kubernetes fixture hashes; job 127018187023). Fixed by `-text` attributes on the hashed fixture directories (`97ddcc3`); a simulated CRLF checkout went from 64 CR bytes in those fixtures to 0 | **PASS** `nightly-clone-compat`, 16 passed |
| `3a87b95` | [41962206220](https://github.com/gqsnt/concord_api/actions/runs/41962206220) | **43/43 PASS** (workspace 1,293, examples alone 452) | **43/43 PASS** (workspace 1,293, examples alone 452): the CRLF fix holds on native Windows | **PASS** `nightly-clone-compat`, 16 passed |

The final implementation commit, its local gate and its CI are recorded below once the review
fixes are integrated.

## Final

Pending.
