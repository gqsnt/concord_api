# V4.3 candidate — verification

Every result below is credited only to the exact commit it ran on. The final implementation
commit and its records-only successor are listed separately in STATUS.md.

## Local runs (Linux x86_64, `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, nightly `cargo 1.101.0-nightly (29c5daa1a 2026-10-07)`)

| Source | Command | Result | Log |
|---|---|---|---|
| `5ed600b` (base) | `scripts/verify.sh --package --bench` | 37/37 PASS (676 workspace tests) | handoff `work/logs/baseline-verify.log` (not in the repository) |
| FINAL | `scripts/verify.sh --package --bench` | PENDING | `verification-logs/` in the delivery ZIP |
| FINAL | `scripts/verify.sh --nightly-only` | PENDING | `verification-logs/` in the delivery ZIP |

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
final gate's `test`, `test-examples-isolated` and per-feature steps on FINAL.
