# Exact beta-candidate evidence — `0f72e865`

**Evidence date:** 2026-09-04
**Claim boundary:** Observed command results for the exact code/documentation
candidate `0f72e8653ee0fc08ea7de0e606f01c8d130b53c2`. This is not a stable
release claim. Owner authority, unfamiliar-human evidence, independent human
review, and macOS execution remain unavailable.

## Candidate and repository state

- Working branch: `fix/v2.1-release-truth`
- Candidate SHA: `0f72e8653ee0fc08ea7de0e606f01c8d130b53c2`
- Audited release anchor: `adad5326de2e2569016ba6f1f01cfe76b2ddf0be`
- Main anchor: `f7464c58f4a60b7332547e8f91039a269171a6ff`
- Preserved backup: `50460e7d4358f5d3ea747e5bb3c4429715543a04`
- `git diff --check "$(git merge-base main HEAD)" HEAD`: exit 0.
- `git status --porcelain --untracked-files=all`: empty.
- No tag existed, and no push, tag, publication, force operation, or local
  merge was performed.

The evidence-record commit that contains this file changes maintained evidence
only. The canonical documentation checker, release audit, branch-diff check,
and worktree check must be rerun on that later HEAD.

The first post-evidence release-audit run correctly rejected the Markdown
hard-break whitespace originally present on the evidence-date line. A focused
follow-up removed it. On `1019dda0dcd55227799f19077fbaf37fbbc41c63`,
`just docs` and `release-audit` then exited 0; the failed run is not counted as
a pass.

## Windows execution

Environment: Microsoft Windows NT 10.0.26200.0, x86_64/AMD64;
`rustc 1.97.1 (8bab26f4f 2026-07-14)`, LLVM 22.1.6;
`cargo 1.97.1 (c980f4866 2026-06-30)`; MSRV Cargo 1.86.0;
rust-analyzer 1.97.1; just 1.58.0; cargo-deny 0.20.2; actionlint
1.7.12; cargo-nextest 0.9.143; workspace-local cargo-expand 1.0.126.

All rows below ran at the exact candidate SHA.

| Command | Exit | Exact result boundary / summary |
|---|---:|---|
| `just fmt` | 0 | Workspace formatting check passed. |
| `just check` | 0 | Metadata, default, no-default, all-feature, focused integration-feature, and policy checks passed. |
| `just lint` | 0 | Workspace/all-target/all-feature Clippy passed with warnings denied. |
| `just test` | 0 | Default profile 885/885 and all-feature profile 910/910 passed. |
| `just ui` | 0 | Macro 189/189 plus no-default, all-feature, and core source-only UI harnesses passed. |
| `just msrv` | 0 | Rust 1.86.0 public packages passed all-feature/no-default checks and semantic UI. |
| `just fixtures` | 0 | Fixture graph checks passed. |
| `just docs` | 0 | Canonical docs check, doctests, and warning-denied default/focused/all-feature Rustdoc passed. |
| `just package` | 0 | Four archives generated, unpacked, normalized metadata checked, packaged Rustdoc built, and extracted-only downstream check/run passed. |
| `just deny` | 0 | Dependency advisories, bans, licenses, and sources passed; duplicate-version warnings remained. This does not supply a project license because unpublished workspace packages are private to this policy. |
| `just workflow` | 0 | actionlint passed. |
| `just analyzer` | 0 | analysis-stats and diagnostics completed; 3,007,900 dependency LOC, 114,385 workspace LOC, 53,677 processed items, 312 IDE files, and 3,196 MiB reported peak. Diagnostics were inactive-code weak warnings. |
| `just bench` | 0 | `concord_overhead` compiled with `--no-run`. |
| `just verify` | 0 | The complete maintained aggregate, including package and release audit, passed and ended on exact SHA `0f72e865...`. |
| `cargo test --workspace --all-features --locked` | 0 | All unit, integration, UI-harness unit, and documentation tests passed. |
| `cargo rustdoc -p concord --all-features --locked -- -D warnings` | 0 | Façade Rustdoc passed. |
| `cargo rustdoc -p concord-extras --all-features --locked -- -D warnings` | 0 | Extras Rustdoc passed. |
| `cargo run -p concord-xtask -- migration-audit` | 0 | Reported 19 deliberate opaque-payload review sites; no command failure. |
| `cargo run -p concord-xtask -- docs-check` | 0 | 1 extracted DSL fence, 56 conceptual fences, 29 anchors, 48 links, 107 active-file retired-language checks, and single status owner passed. |
| `cargo run -p concord-xtask -- release-audit` | 0 | Docs truth, exact SHA, branch diff, clean tree, publication safety, lifecycle, artifact, clock, and authority checks passed. |
| `cargo check --workspace --all-targets --all-features --locked` | 0 | Exact Windows CI platform check passed. |
| `cargo nextest run --profile ci -p concord --test runtime --all-features --locked -E "test(runtime::deterministic::)"` | 0 | Exact Windows CI smoke: 4 passed, 30 skipped. |

The package archives were `concord-core` (185 files, 351.2 KiB compressed),
`concord-macros` (39, 121.8 KiB), `concord-extras` (24, 41.9 KiB), and
`concord` (314, 130.6 KiB), all at `2.1.0-beta.1`. Cargo warned that the
manifests have no license/repository/homepage metadata; those warnings are the
truthful Gate 0 blocker, not waived evidence.

### Windows performance and scale

The first `expansion_scale` attempt exited 1 because `cargo-expand` was absent.
`cargo install cargo-expand --locked --root target/tools/cargo-expand` then
installed version 1.0.126 locally with exit 0, and the exact required command
was rerun successfully:

| Endpoints | Expanded lines | Methods | Expand ms | Build ms | Binary bytes | Description bytes |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 1,068 | 39 | 13,168 | 11,589 | 307,200 | 1,649 |
| 10 | 5,865 | 219 | 372 | 1,013 | 361,472 | 13,214 |
| 100 | 54,195 | 2,019 | 711 | 2,495 | 911,872 | 128,864 |
| 500 | 268,995 | 10,019 | 2,078 | 9,882 | 3,356,160 | 642,864 |
| 1,000 | 537,495 | 20,019 | 3,817 | 19,193 | 6,411,264 | 1,285,364 |

`cargo run -p concord-perf --release --bin stream_driver_comparison --locked`
exited 0. Boxed construction/poll allocations were `(1, 0)`; paired boxed to
fastest ratios were `0.989, 0.989, 0.985` for byte workloads and `1.000,
0.980` for item workloads. The lifecycle signature was
`pending,item,pending,error,end`, with early drop once for every strategy. All
precommitted budgets passed.

`cargo bench -p concord-perf --bench concord_overhead` exited 0 and executed
17/17 named categories. Selected exact totals for 100 iterations were direct
execution 8.8788 ms, managed retry 16.031 ms for 100 logical/200 physical
sends, authenticated managed execution 3.0927206 s, rate-limited managed
execution 11.9766 ms, pagination 16.3554 ms, and buffered record collection
12.5414 ms.

## Linux execution

Actual environment: Ubuntu 26.04.1 LTS under WSL2, Linux
6.18.33.2-microsoft-standard-WSL2, x86_64; rustc 1.97.1, LLVM 22.1.6;
Cargo 1.97.1; just 1.58.0; Git 2.53.0. This is native Linux execution of Linux
binaries, not cross-compilation.

| Command | Exit | Exact result boundary / summary |
|---|---:|---|
| `cargo check --workspace --all-targets --all-features --locked` | 0 | Exact Linux CI platform check passed. |
| `cargo nextest run --profile ci -p concord --test runtime --all-features --locked -E "test(runtime::deterministic::)"` | 0 | Exact Linux CI smoke: 4 passed, 30 skipped. |
| `cargo run --locked -p concord-xtask -- docs-check --locked` | 0 | Same canonical counts and truth checks as Windows passed. |
| `cargo rustdoc -p concord --all-features --locked -- -D warnings` | 0 | Linux façade Rustdoc passed. |
| `cargo rustdoc -p concord-extras --all-features --locked -- -D warnings` | 0 | Linux extras Rustdoc passed. |
| `cargo package -p concord-core --list ...` | 0 | 185 package entries; required files present. |
| Four `cargo package -p <package> --no-verify --locked ...` commands | 0 | Linux created the four exact beta archives in dependency order. |
| Four-archive process-artifact scan | 0 | No `dev_doc`, generated kit, raw benchmark, or prompt path was present. |
| `cargo nextest run --profile ci --workspace --exclude concord-macros --lib --tests --examples --all-features --locked` | 0 | After resource remediation, 910/910 tests passed, 0 skipped. |
| `cargo run -p concord-perf --release --bin stream_driver_comparison --locked` | 0 | Linux allocation, lifecycle, drop, and throughput budgets passed. |

The first attempt at the broad 910-test Linux run exited 1 with `ENOSPC` while
linking. `/tmp` was a 7.8 GiB tmpfs at 100%; this task's verified target used
4.2 GiB while unrelated workloads used the remainder. The exact reproducible
target `/tmp/concord-v21-0f72-target` was removed after path verification, and
the run was repeated with a fresh target on F: (155 GiB free), where it passed.
An earlier shell-loop packaging attempt also exited 1 with “package name cannot
be empty”; it was an orchestration quoting error, and the four explicit package
commands immediately passed. Neither failed command was hidden or counted as a
code pass.

The Linux stream probe reported boxed construction/poll allocations `(1, 0)`,
byte ratios `0.995, 0.993, 0.995`, item ratios `0.995, 0.971`, the same
`pending,item,pending,error,end` lifecycle and early-drop result, and a passing
budget decision.

## Unavailable mandatory evidence

- **Gate 0:** no authorized license, package namespace, registry control,
  product-history answer, or official metadata decision. Registry facts are in
  `package_namespace_2026-09-04.md`.
- **Gate 3:** zero qualifying unfamiliar-human participants; correctness,
  timing, confidence, findings, and retests are not measured.
- **Gate 8/10 macOS:** zero macOS runs. Docker, Podman, Multipass, Vagrant,
  QEMU, Lima, Colima, and Tart were not installed; no macOS host was supplied;
  remote push authorization remains an unanswered owner decision.
- **Gate 10 human review:** zero qualifying independent Rustdoc/semantic-diff
  reviewers; penetrability is not established by generated docs or AI review.

Therefore the strongest supported lifecycle is `2.1.0-beta.1` integration
candidate with publication disabled. Stable finalization and local merge are
forbidden by policy.
