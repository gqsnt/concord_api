# V4.3 candidate — verification

Every result below is credited only to the exact commit it ran on. The final implementation
commit and its records-only successor are listed separately in STATUS.md.

## Local runs (Linux x86_64, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, nightly `cargo 1.101.0-nightly (29c5daa1a 2026-10-07)`)

| Source | Command | Result | Log |
|---|---|---|---|
| `5ed600b` (base) | `scripts/verify.sh --package --bench` | 37/37 PASS (676 workspace tests) | handoff `work/logs/baseline-verify.log` (not in the repository) |
| `637c8d2` (superseded by R4 fixes) | `scripts/verify.sh --package --bench` | 37/37 PASS, exit 0, 1m55s; tests: workspace 747, examples each alone 197, `-p concord` no-default 298 / json-only 459 / form-only 396 / extras-only 393 / reqwest-defaults-only 300, form+json 5, core 13, extras 3/9/4; packaged consumers PASS | delivery ZIP `verification-logs/` |
| `637c8d2` (superseded by R4 fixes) | `scripts/verify.sh --nightly-only` | PASS `nightly-clone-compat`, 16 passed, exit 0 | delivery ZIP `verification-logs/` |
| final implementation commit (see STATUS.md) | `scripts/verify.sh --package --bench` | recorded in the records-only successor | delivery ZIP `verification-logs/` |
| final implementation commit | `scripts/verify.sh --nightly-only` | recorded in the records-only successor | delivery ZIP `verification-logs/` |

## CI (GitHub Actions `ci`: `scripts/verify.sh --package --bench` on Linux and native Windows; nightly conditional-`Clone` lane, non-blocking)

| Source | Run | Linux | Windows (native) | Nightly lane (actual step result) |
|---|---|---|---|---|
| FINAL | PENDING | | | |

## Experiment branch (evidence for G1 only, not for the supported branch)

| Source | Command | Result |
|---|---|---|
| `9e4c0ef` | `scripts/verify.sh --package --bench` | 37/37 PASS, exit 0, 754 workspace tests (`experiments/generated-inputs/evidence/inputs-verify.summary.log`) |
| `9e4c0ef` | `scripts/verify.sh --nightly-only` | PASS `nightly-clone-compat`, 16 passed (`experiments/generated-inputs/evidence/inputs-verify-nightly.log`) |

## Requirement trace

`requirements.json` maps every requirement id to its tests and commands; the tests ran in the
final gate's `test`, `test-examples-isolated` and per-feature steps; the commit each run used is
named in its row.
