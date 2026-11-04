# Lane `tooling`: final report

> Saved by the coordinator from the lane's returned text (the harness refuses report files from
> subagents; the lane did not route around it). Lane: Claude Opus subagent (Agent tool, requested
> `opus`/`xhigh`; the session names `claude-opus-5-5`, effort not observable). Logs: `logs/`
> (final rerun `logs/final/`, `logs/final-steps.log`); lane-local drivers: `scratch/`.

- Branch `v42/tooling`, final commit **`44193b6ff196be73557b3503cfb05a1f488fffb6`** (4 commits on
  `07dcdd0`: `8ad1377` M03, `129b06c` M04, `baa4204` M02 + fresh extraction, `44193b6` gate wiring).
- Host: Linux x86_64, rustc 1.99.1 (58cf8f9c3), Python 3.14.4; compiling commands via `heavy.sh`,
  `--locked`; `Cargo.lock` unchanged. Full `verify.sh` not run (central gate).

## verify.sh changes

Added (default section, Linux and Windows): `tool-tests` (`python -I -B -m unittest discover -s
scripts/tests -p "test_*.py" -v`), `output-dirs` (`scripts/tests/output-dirs.sh`),
`test-examples-isolated` (one `cargo test --locked -p <pkg>` per example from `cargo metadata`;
fails on an unlisted example directory, a failing package or 0 tests). Removed
`test-multipart-alone` (covered). Packaged steps write to `target/packaged/{package-check,consumers}`.
Expected `--package --bench` run: 37 PASS lines (was 35); `verify-test-examples-isolated: 122
passed` (compat 11, multipart 11, native 27, quickstart 45, renamed 3, riot 22, shapes 3) replaces
`verify-test-multipart-alone: 11 passed`.

## M02

- `examples/renamed/Cargo.toml` declares `features = ["extras"]` with `default-features = false`.
  Before: `cargo test --locked -p concord-example-renamed` exit 101, E0432 (`Binary`, `Text`).
  After: exit 0, 3 passed; every example alone exits 0.
- Step negatives: renamed without `extras` → `FAIL concord-example-renamed alone (exit 101, 0 tests)`;
  an unlisted `examples/zz-unlisted` → `examples that are not workspace members`.
- No-extras consumer: `ok facade-no-extras (… no concord extras, json, form or concord-extras
  feature)`, positive control `ok facade-full (… "extras" present)`; negative (fixture given
  `extras`) → `FAIL facade-no-extras`.
- Fresh extraction (`scripts/fresh-output.sh`): outputs recreated only from their owner's mark,
  unmarked non-empty directories refused and untouched; `same_sources` detects stale extracted
  trees. Controls against the 4.1 scripts: the old `package-check` deleted an unmarked directory's
  file; the old `consumers.sh` built from a planted stale file. `output-dirs.sh`: 14 controls, 0
  failures. `package-check` also extracts into `extracted/ünïcødé-路径/nested-…` and checks that
  Cargo compiled `concord` from there (Linux run only; ~181 characters on a `D:\a\…` checkout).

## M03

`scripts/check-ci.py --workflow` parses `ci.yml` with a documented YAML subset and checks jobs,
platforms (`runs-on`/matrix), conditions and executable `run` steps; reusable workflows are reported
and never counted. Real workflow: `effective full gate on: Linux, Windows`, plus `static inspection
only: the CI job logs, not this check, show that the gate ran`. `scripts/tests/test_check_ci.py`:
16 positive, 47 negative, 5 parser tests (block/folded scalars, quoting, continuation, `success()`/
`always()`, `defaults.run`, CRLF; comment-only, `echo`, missing gate, `if: false`/`${{ false }}` on
step/job, `needs` on a disabled job, missing platform, `|| true`, `; true`, `continue-on-error`,
`set +e`, `exit 0`, here-docs, `bash -n`, non-bash Windows shell, `BASH_ENV`, reusable job, cargo
in a step, `runner.os`, missing nightly/triggers/filters, anchors, tabs, flow mappings, duplicate
keys, merge keys, `---`). On mutants of the real `ci.yml` the 4.1 checker exits 0 and the new guard
exits 1 for 8 (D24 comment-only, step `if` ×2, job `if`, `|| true`, `; true`, step
`continue-on-error`, Windows removed). Parser cross-checked against PyYAML 6.0.3 lane-locally
(65 accepted texts identical; 8 rejected are the excluded constructs). Policy: unfiltered
`push`/`pull_request` triggers and explicit `shell: bash` on the Windows gate step are required.

## M04

`tests-gen/m4_mutations.py` retired: no maintained caller (only historical `v4/` records reference
it); 4 of 9 mutations target deleted files. `main()` exits 2 with "retired … applies no mutation,
runs no test and reports no verdict"; `scripts/tests/test_m4_retired.py` checks the refusal, the
notice and no maintained caller (negatives: old helper restored → fails; planted caller → fails).

## Found along the way

`consumers.sh`: a failing consumer's missing `fetch.log` made `cat` fail under `set -e` before the
FAIL line and the remaining consumers (exit still non-zero, diagnosis lost). Fixed in `baa4204`.

## Final rerun on 44193b6

PASS: toolchain, ci-inventory, tool-tests (71 tests), output-dirs, graph, consumers,
test-examples-isolated, package-check, consumers-packaged; `dirty: 0`; `check-records.py` 396/0.

## Requested of other owners (coordinator action)

MAINTAINERS.md (multipart line; `ci-inventory` description; `tool-tests`, `output-dirs`, packaged
outputs); CHANGELOG maintenance text; optional new linked note for M03 (4.1's check was a text
search; 4.1 CI logs remain execution evidence). No historical record edit needed for M04.
