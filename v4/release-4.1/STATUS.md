# Release 4.1.0 — closure status

A bounded correction and release-preparation pass on `concord-v4-modular`, from `31984ae`
(last implementation commit `fcf343f`). Architecture, syntax, codec contracts and the Rust 1.99.1
minimum/pin are unchanged; no library source file (`crates/*/src`) changed. Work directory
(raw CI logs, gate logs, reviewer snapshot): `/home/canarit/projects/concord_v4_modular_work/v41`.

## Revisions

| Commit | Kind | Change |
|---|---|---|
| `dc9a9a2` | code (gate tooling) | metadata to Python as a file; `scripts/argv-guard.sh` enforces the Windows command-line limit on every platform; CI runs `--package --bench` on Linux and Windows; `check-ci.py` refuses platform-selected gates |
| `44cb8ff` | code (tests, docs) | `NoSuggestion<F>` forwards all three decoding steps; test `no_suggestion_keeps_the_metadata_checks` |
| `0a2a5fa` | code (release) | the four crates at 4.1.0 with `=4.1.0` requirements; package contents and credential check |
| `b376d78` | code (tests) | `checks/consumers/facade-gzip`; feature isolation refuses reqwest decompression in Concord |
| `15b3200` | docs | `CHANGELOG.md`, README status, migration intro |
| `791e991` | code (gate tooling) | `tar` reads `.crate` archives from standard input; `check-ci.py` refuses `tar -f NAME` |
| `f4a8324` | code (gate tooling) | strip the CR Windows Python leaves on the target directory |
| `1517fcd` | code (gate tooling, tests) | review corrections: exact Windows length model (`scripts/argv-length.py`, gate step `argv-length`); every new network test bounded |

No measured revision: no execution path that the benchmarks measure changed, so the performance
campaign (`v4/modular-codecs/PERF.md`) was not repeated.

## Windows

Every native Windows failure, with the exact error and its class, is in `WINDOWS.md` (next to this
file); the raw job logs of each run are in the work directory, under v41/ci/ (with SHA256SUMS). All three were
portable-tooling defects in the gate scripts; none was a library defect or an environmental failure.

| Run (head) | Windows job | Failed internal gates | Root cause |
|---|---|---|---|
| [41635670821](https://github.com/gqsnt/concord_api/actions/runs/41635670821) (`31984ae`) | 125916104134 | `consumers` (`Argument list too long`) | 51,763 bytes of `cargo metadata` as one argument (limit 32,767) |
| [41644184361](https://github.com/gqsnt/concord_api/actions/runs/41644184361) (`15b3200`) | 125945046735 | `package-check`, `consumers-packaged` (`Cannot connect to D: resolve failed`) | GNU tar reads `D:\...` as `host:path`; first Windows run of `--package` |
| [41646913735](https://github.com/gqsnt/concord_api/actions/runs/41646913735) (`791e991`) | 125954551029 | `package-check` (`...target<CR>/package/...: No such file`) | `read` keeps the CR of Windows Python's CR LF |
| [41650716219](https://github.com/gqsnt/concord_api/actions/runs/41650716219) (`f4a8324`) | 125967789323 | none: 34/34 PASS, 651 tests | — |
| [41705581594](https://github.com/gqsnt/concord_api/actions/runs/41705581594) (`1517fcd`) | 126159877312 | none: 35/35 PASS, 651 tests | — |

Regressions: the guard reproduces the first failure on Linux (57,668 characters); `check-ci.py
--workflow` fails on the old `tar` lines and on platform-selected gates; the tar and CR failures
were reproduced on Linux with GNU tar 1.35 and bash.

## Verification of the release candidate `1517fcd`

| Where | Identity | Command | Result |
|---|---|---|---|
| Local, Linux x86_64 | rustc 1.99.1 (58cf8f9c3) | `scripts/verify.sh --package --bench --nightly` | 36/36 PASS (nightly lane included), 651 workspace tests |
| CI, `ubuntu-latest` | run 41705581594, job 126159876992 | `scripts/verify.sh --package --bench` | 35/35 PASS, 651 |
| CI, `windows-latest` (x86_64-pc-windows-msvc) | run 41705581594, job 126159877312 | `scripts/verify.sh --package --bench` | 35/35 PASS, 651 |
| CI, nightly lane (non-blocking) | run 41705581594, job 126159877259 | `scripts/verify.sh --nightly-only` | PASS, 16 |

The nightly lane stays non-blocking (`continue-on-error`), as before. The earlier candidate
`f4a8324` passed the same inventory (local 35/35; CI run 41650716219, Linux and Windows 34/34).

## Review

| Review | Model, effort (observed) | Target | Outcome |
|---|---|---|---|
| Coordinator | Claude Opus 5.5 (`claude-opus-5-5`); its effort setting is the app's and is not observable from inside the session | — | implementation, triage |
| Independent review | `gpt-6.1-sol`, `max` (session `turn_context`) | `31984ae..f4a8324`, full gate run in its own snapshot (34/34 PASS), CI evidence | 2 should-fix: guard length model; unbounded network reads. Fixed in `1517fcd` |
| Narrow re-review | `gpt-6.1-sol`, `max` | `1517fcd` | no finding; both resolved, nothing new |

Reports, frozen: `reports/release41-sol-REPORT.md`, `reports/release41-rr-sol-REPORT.md`
(`reports/SHA256SUMS`). Sol supports `max` (the Codex model list offers low to max and ultra); the
review runner's effort choices were extended to `max` for this run. No Sonnet, Terra or DeepSeek
helper was used. No review was running when the candidate was accepted.

## Not run

- macOS: out of scope for this closure.
- Visual inspection of rendered rustdoc: NOT_RUN (the browser pane cannot open local files). No
  `crates/*/src` file changed in this closure, so rustdoc content differs from the reviewed 4.0
  state only in the version.
- Runtime benchmarks: not repeated (no measured execution path changed); benchmark compilation
  passes in the gate.
- Decompression other than gzip (brotli, zstd, deflate) at run time; Concord enables none and the
  feature-isolation gate refuses them in Concord's own graph.
- Publication on crates.io: not done (`publish = false`; not authorized).

## Release

| Item | Value |
|---|---|
| Merge into `main` | `afcec40` (`--no-ff` merge of `concord-v4-modular` at `54371e4`; `main` was `a492d36`, an ancestor; no conflicts; the merged tree equals `54371e4`'s) |
| Local gate on `afcec40` | `scripts/verify.sh --package --bench --nightly`: 36/36 PASS, 651 tests |
| CI on `main` at `afcec40` | [run 41709804342](https://github.com/gqsnt/concord_api/actions/runs/41709804342): Linux job 126174584782 35/35, Windows job 126174585235 35/35, nightly job 126174585184 PASS |
| CI on `concord-v4-modular` at `54371e4` | [run 41709795989](https://github.com/gqsnt/concord_api/actions/runs/41709795989): success on Linux, Windows and nightly |
| Tag | `v4.1.0`, annotated (tag object `1fc0c56`), pointing to `afcec40`, created after the `main` run passed |
| Not done | publication on crates.io; no branch deleted or rewritten (every push was a fast-forward) |

This section was added after the tag, in a records-only commit on `main`.
