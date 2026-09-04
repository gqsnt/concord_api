# Large-contract, compile, and documentation evidence

- **Recorded:** 2026-09-04
- **Candidate measurements:** commits `f94c5de`, `62a4b35` (the latter includes two test-only clean-worktree repairs)
- **Baseline:** v2.0 commit `f7464c5`
- **Host:** Windows x86-64
- **Toolchain:** Cargo/Rust/rust-analyzer 1.97.1 (`8bab26f4f`, 2026-07-14)
- **Raw results:** `target/evidence/raw/` and `target/perf/` (generated and intentionally uncommitted)

## Process-resource method

`scripts/measure_command.ps1` launches a hidden child process, samples its full
process tree every 250 ms, and records elapsed time, exit code, aggregate
working set, and aggregate private bytes. Each clean command below had a unique
previously absent `CARGO_TARGET_DIR`. Worktree
`F:\projects\concord_api2_baseline_measure` was detached at `f7464c5`; worktree
`F:\projects\concord_api2_candidate_measure` was detached at the stated
candidate. The worktrees were clean before and after temporary edit probes.

The aggregate process-tree memory figure can double-count shared pages and can
miss a sub-250-ms peak. It is useful for same-host comparison, not a minimum-RAM
promise.

## Workspace build measurements

| Command | Revision | Exit | Wall time | Peak working set | Peak private bytes |
|---|---|---:|---:|---:|---:|
| clean `cargo check --workspace --all-targets --locked` | v2.0 `f7464c5` | 0 | 20.459 s | 1.486 GB | 1.900 GB |
| clean `cargo check --workspace --all-targets --locked` | v2.1 `f94c5de` | 0 | 25.017 s | 1.644 GB | 2.056 GB |
| no-change repeat of the candidate check | v2.1 `f94c5de` | 0 | 0.491 s | 79.5 MB | 49.1 MB |
| clean `cargo build --workspace --all-targets --all-features --locked` | v2.0 `f7464c5` | 0 | 48.939 s | 6.937 GB | 9.941 GB |
| clean `cargo build --workspace --all-targets --all-features --locked` | v2.1 `f94c5de` | 0 | 50.434 s | 7.214 GB | 10.270 GB |
| clean `cargo test --workspace --all-features --locked` | v2.1 `62a4b35` | 0 | 68.421 s | 7.122 GB | 9.619 GB |

The candidate cold check is 22.3% slower and uses 10.6% more aggregate working
set than v2.0. The all-target/all-feature build is 3.1% slower and uses 4.0%
more aggregate working set. The large cold-check percentage is dominated by
additional checked targets and evidence tooling; the all-feature build is the
better like-for-like release-cost comparison. No-change checking remains below
half a second on this host.

The first clean v2.1 test run correctly failed: it exposed stale exact-control
wording in a macro unit assertion. A subsequent run exposed a CRLF-sensitive
semantic-diff fixture comparison in the fresh worktree. Commits `383bf93` and
`62a4b35` fixed those defects. The table reports the new, separately clean,
passing run; the two failures remain in the generated raw log and are not
reclassified as passes.

## Edit costs

The active Riot contract has 46 endpoints. With dependencies warm and the
candidate worktree clean:

| `cargo check -p concord-examples --all-features --locked` case | Exit | Wall time | Peak working set |
|---|---:|---:|---:|
| no source change | 0 | 0.492 s | 24.1 MB |
| one endpoint path literal changed | 0 | 2.642 s | 510.6 MB |
| client response representation changed to an equivalent named alias, affecting inherited leaves | 0 | 2.685 s | 496.9 MB |

Both temporary edits were reversed with exact patches; `git status --short`
was empty afterward. The representation alias preserved validity while forcing
the generated provenance/type projection for every inheriting leaf to change.

## Generated-contract scale

Command: `cargo run -p concord-perf --release --bin expansion_scale --locked`.
`cargo-expand` 1.0.126 was installed under `target/tools`; fixture dependencies
were warm, while each changed generated target was rebuilt. Binary sizes are
unoptimized fixture executables because they provide a repeatable scale trend;
the separate workspace all-feature build above covers release code generation.

| Endpoints | Expanded lines | Characters | Fragments | Declarations | Methods | Expand | Build | Binary | Static description |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 1,068 | 45,211 | 2,488 | 101 | 39 | 378 ms | 732 ms | 307,200 B | 1,647 B |
| 10 | 5,865 | 256,153 | 12,361 | 497 | 219 | 388 ms | 994 ms | 361,472 B | 13,194 B |
| 100 | 54,195 | 2,373,403 | 111,451 | 4,457 | 2,019 | 725 ms | 2.569 s | 911,872 B | 128,664 B |
| 500 | 268,995 | 11,793,403 | 551,851 | 22,057 | 10,019 | 2.118 s | 15.026 s | 3,356,160 B | 641,864 B |
| 1,000 | 537,495 | 23,568,403 | 1,102,351 | 44,057 | 20,019 | 4.671 s | 19.678 s | 6,411,264 B | 1,283,364 B |

Expansion, method count, binary size, and static descriptions are approximately
linear. The 1,000-endpoint point is large but bounded and completed without a
diagnostic or superlinear cliff. Expansion line count is retained as one
dimension, not treated as the cost conclusion.

## rust-analyzer

`rust-analyzer analysis-stats . --run-all-ide-things` exited 0 in 101.263 s
with 3.461 GB peak aggregate working set. Its own phase report recorded a
32.56 s database load, 5.04 s item-tree collection, 19.66 s inference, 8.74 s
IDE exercise, 112,556 workspace lines, 25,877 declarations, and 18,638 bodies.
The command exercises semantic highlighting, diagnostics, and annotations.

`rust-analyzer diagnostics .` exited 0 in 33.506 s with 2.147 GB peak working
set. Output contained expected `inactive-code` weak warnings for test- and
feature-gated items; there were no error diagnostics. The installed batch CLI
does not expose a stable completion/hover latency command, and no LSP benchmark
harness is maintained here. Interactive completion/hover latency is therefore
explicitly unmeasured rather than inferred from `analysis-stats`.

## Documentation and review boundary

The mechanical documentation matrix (`scripts/check_docs_examples.ps1`, all
workspace doctests, and feature-specific Rustdoc with `-D warnings`) passes and
is described in `dev_doc/documentation_contract.md`. A ready-to-run independent
review packet is in `dev_doc/rustdoc_review/`.

No relevant reviewer other than the primary implementer was available in this
execution environment. Consequently the generated-Rustdoc and parent semantic
diff human-review requirement is not satisfied. This report supports the
machine portion of Gate 10 only and does not convert that missing evidence into
a waiver.
