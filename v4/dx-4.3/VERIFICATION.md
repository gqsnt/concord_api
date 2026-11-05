# V4.3 candidate — verification

Every result below is credited only to the exact commit it ran on. The final implementation
commit and its records-only successor are listed separately in STATUS.md.

## Local runs (Linux x86_64, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, nightly `cargo 1.101.0-nightly (29c5daa1a 2026-10-07)`)

| Source | Command | Result | Log |
|---|---|---|---|
| `5ed600b` (base) | `scripts/verify.sh --package --bench` | 37/37 PASS (676 workspace tests) | handoff `work/logs/baseline-verify.log` (not in the repository) |
| `637c8d2` (superseded by R4 fixes) | `scripts/verify.sh --package --bench` | 37/37 PASS, exit 0, 1m55s; tests: workspace 747, examples each alone 197, `-p concord` no-default 298 / json-only 459 / form-only 396 / extras-only 393 / reqwest-defaults-only 300, form+json 5, core 13, extras 3/9/4; packaged consumers PASS | delivery ZIP `verification-logs/` |
| `637c8d2` (superseded by R4 fixes) | `scripts/verify.sh --nightly-only` | PASS `nightly-clone-compat`, 16 passed, exit 0 | delivery ZIP `verification-logs/` |
| **`a6fb998`** (final implementation) | `scripts/verify.sh --package --bench` | **37/37 PASS, exit 0**, 1m52s; tests: workspace 747, examples each alone 197, `-p concord` no-default 298 / json-only 459 / form-only 396 / extras-only 393 / reqwest-defaults-only 300, form+json 5, core 13, extras 3/9/4; `package-check` and `consumers-packaged` PASS | delivery ZIP `verification-logs/` |
| **`a6fb998`** | `scripts/verify.sh --nightly-only` | **PASS** `nightly-clone-compat`, 16 passed, exit 0 (nightly `rustc 1.101.0-nightly`) | delivery ZIP `verification-logs/` |

## CI (GitHub Actions `ci`: `scripts/verify.sh --package --bench` on Linux and native Windows; nightly conditional-`Clone` lane, non-blocking)

| Source | Run | Linux | Windows (native) | Nightly lane (actual step result) |
|---|---|---|---|---|
| **`a6fb998`** (final implementation), push | [41899630861](https://github.com/gqsnt/concord_api/actions/runs/41899630861) | 37/37 PASS (747 workspace, 197 examples alone) | **37/37 PASS** on `windows-latest` (747, 197) | `PASS nightly-clone-compat`, 16 passed |
| `a6fb998`, pull request #2 | [41899644446](https://github.com/gqsnt/concord_api/actions/runs/41899644446) | see the run (same source, PR merge ref) | | |
| `637c8d2` (superseded by the R4 fixes), push / PR | [41897452645](https://github.com/gqsnt/concord_api/actions/runs/41897452645) / [41897498079](https://github.com/gqsnt/concord_api/actions/runs/41897498079) | success | success | success |
| `926daaa` (early integration) | [41895869741](https://github.com/gqsnt/concord_api/actions/runs/41895869741) | success | success | `PASS nightly-clone-compat`, 16 passed |
| experiment `9e4c0ef` | [41897456223](https://github.com/gqsnt/concord_api/actions/runs/41897456223) | success | success | success |
| experiment `76f23f5` | [41895872627](https://github.com/gqsnt/concord_api/actions/runs/41895872627) | success | success | success |

Step counts for `a6fb998` were read from the job logs (`PASS`/`FAIL` lines of `scripts/verify.sh`),
not only from the job status; the nightly lane is non-blocking, and its log shows the step passed.
The 4.2 release CI is not credited to any of these commits. Runs on the records-only successor
verify the same code and are not needed for this table.

## Experiment branch (evidence for G1 only, not for the supported branch)

| Source | Command | Result |
|---|---|---|
| `9e4c0ef` | `scripts/verify.sh --package --bench` | 37/37 PASS, exit 0, 754 workspace tests (`experiments/generated-inputs/evidence/inputs-verify.summary.log`) |
| `9e4c0ef` | `scripts/verify.sh --nightly-only` | PASS `nightly-clone-compat`, 16 passed (`experiments/generated-inputs/evidence/inputs-verify-nightly.log`) |

## Requirement trace

`requirements.json` maps every requirement id to its tests and commands; the tests ran in the
final gate's `test`, `test-examples-isolated` and per-feature steps; the commit each run used is
named in its row.
