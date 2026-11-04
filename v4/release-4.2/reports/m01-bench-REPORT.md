# Lane m01-bench — M01 benchmark equivalence

Local implementation and validation are complete. **The shared worktree branch could not be committed because its Git metadata is mounted read-only.** A real commit on an isolated lane-local `v42/m01-bench` branch, a verified portable bundle and the full patch are ready for the coordinator.

- Worktree: `/home/canarit/projects/concord-v42/wt-m01`.
- Shared worktree branch: `v42/m01-bench`; its HEAD remains `07dcdd03c61a1822341e92d3820e66117f1aef14`.
- Prepared final commit: **`875f21accf564a85b6fb03cff9becf7d64101e07`**, parent `07dcdd03c61a1822341e92d3820e66117f1aef14`.
- Commit message ends with `Lane: m01-bench`.
- Portable deliverables: [m01-bench.bundle](m01-bench.bundle), [candidate.patch](candidate.patch), [prepared-commit.json](prepared-commit.json).
- Date/platform: 2026-11-04, Linux x86_64. Actual model/effort were not observable; none are inferred.

## Changes and contract

Only these six owned files changed:

| File | Result |
|---|---|
| `benches/baselines/src/s3_query.rs` | S3 search explicitly prepares `Accept: application/json`, inside the consuming routine, before query application/build. No client-default substitution. |
| `benches/criterion/src/lib.rs` | Extends the existing `Case` registrations with comparison groups, setup and stage comparator descriptions. Header representations map to their intended manual counterparts. The existing `Runner` accepts observable outputs; measurement runners never perform observations. |
| `benches/criterion/src/contract.rs` | Untimed prepared-request and decoded-value observations. |
| `benches/criterion/src/tests.rs` | Executes the actual registered closures twice with reconstructed inputs, compares every promised synchronous pair, adds discrimination controls, pins S3 against a conflicting client default, and qualifies round-trip partial floors. Replaces the incomplete hand-maintained pair list. |
| `benches/criterion/benches/runtime.rs` | Corrects the consuming-input destruction comment; annotates round-trip partial floors and planner comparisons of different operations. No measured routine changed here. |
| `benches/criterion/README.md` | Inventory, precise timing/setup/destruction boundaries, linked historical S3 notice preserving numbers, and the actual corrected measurements with limits. |

No dependency, manifest, lock, toolchain, production crate, example, gate, historical report or old release record changed. No requested change to `examples/shapes` is needed.

The inventory covers **230 emitted IDs**: 188 synchronous, 10 round-trip and 32 planner IDs. The same existing registry drives Criterion, allocation counting and the equivalence test. It connects 92 synchronous cases into 45 comparison groups and **47 actual comparisons**; the two three-member native/literal key groups account for the additional comparisons. `checks/inventory.txt` is generated test output, not another registry.

Prepared comparison checks method, exact URL spelling/order, canonical header names, bytes, multiplicity, same-name order and sensitivity, optional body presence/bytes, HTTP version and request timeout. It ignores cross-name HeaderMap iteration. Fixtures share the same native client. All registered prepared bodies are buffered; unsupported streaming observations fail explicitly. One-shot decode responses are reconstructed from reusable byte seeds for every input and compared as actual decoded Rust values on the selected successful response fixtures.

Equal-work promises concern the stated observable stage result. Input representations, internal algorithms, diagnostic work and arbitrary error paths are not claimed identical. Native HeaderValue cloning occurs in Concord input construction and in manual preparation; manual deep reused scopes cache their URL/header map. These placements and consumed-input destruction are explicit in the README. JSON serialization and destruction of its local owned input are timed on both sides; returned synchronous outputs are dropped after timing.

The wider audit identified round-trip manual requests without prepared Accept and fewer metadata checks. They are explicitly classified as partial floors; their successful small/10KB decoded values are checked. Raw bytes transport is a partial floor. Planner compose/validate measure different features. Neither category retains an equal-work ratio promise.

## Commands, working directories and results

All workspace commands below ran in `/home/canarit/projects/concord-v42/wt-m01`. Every compiling Cargo command used a finite outer `timeout` and `/home/canarit/projects/concord-v42/tools/heavy.sh`; all measurements held `--exclusive`. The measurement Python script runs the nested Cargo command while that lease remains held, captures host conditions and archives samples before releasing it. No other target directory was created.

[COMMANDS.md](COMMANDS.md) preserves exact outer argv, cwd and timestamps, including the receipt runner. Each `*.receipt.json` retains actual exit, log SHA256 and source state. The decisive workspace checks are:

```sh
timeout 1200 /home/canarit/projects/concord-v42/tools/heavy.sh cargo +1.99.1 test --locked -p concord-bench
timeout 1200 /home/canarit/projects/concord-v42/tools/heavy.sh cargo +1.99.1 clippy --locked -p concord-bench --all-targets -- -D warnings
timeout 1500 /home/canarit/projects/concord-v42/tools/heavy.sh cargo +1.99.1 bench --locked -p concord-bench --no-run
```

| Check | Expected exit/assertion | Actual exit and decisive output | Evidence |
|---|---|---|---|
| Ordinary package test | 0; all selected tests execute and pass | **0**, `10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` | [test-final.log](checks/test-final.log), [receipt](checks/test-final.receipt.json) |
| Clippy all targets | 0; no warnings under `-D warnings` | **0**, `Finished dev profile` | [clippy-final.log](checks/clippy-final.log), [receipt](checks/clippy-final.receipt.json) |
| Bench compilation | 0; library, allocation runner and runtime harness compile | **0**, `Executable benches/runtime.rs (target/release/deps/runtime-5537403572baed14)` | [bench-no-run-final.log](checks/bench-no-run-final.log), [receipt](checks/bench-no-run-final.receipt.json) |
| Registry inventory | 0; unique IDs and every pair execute | **0**, `EQUIVALENCE: 92 registered cases; 45 groups; 47 comparisons; fresh inputs checked twice` | [initial nocapture test](checks/test-initial.log), [extracted inventory](checks/inventory.txt) |
| Benchmark listing | 0; expected emitted IDs | **0**, listing parsed as **230 unique IDs** | [benchmark-list.log](checks/benchmark-list.log) |
| Format/whitespace | 0 | **0**, no diff from `cargo +1.99.1 fmt --package concord-bench --package concord-baselines -- --check` and `git diff --check` | Executed before preparing the commit; prepared index also passed `git diff --cached --check` |

The initial test used `cargo +1.99.1 test --locked -p concord-bench -- --nocapture`, under the same 1200-second wrapper, to retain the exact expanded inventory and pair witnesses. Final cached checks refreshed the source closure including the new module; they were not used as fresh cold-build performance observations.

## Acceptance ledger

The command/cwd/result records above and the measurement/replay commands below are part of each row's receipt.

| Required item | Command and cwd | Expected | Actual / decisive witness |
|---|---|---|---|
| 1. Repair S3 | `cargo +1.99.1 test --locked -p concord-bench`, worktree cwd, leased as above | 0; registered full-query pair and independent default-query positive agree before send | **0**; `tests::registered_pairs_match ... ok`; nocapture output `PAIR construct/s3_search_9_query: 2 registered outputs equal`; `tests::s3_prepares_json_accept_before_client_defaults ... ok` |
| 2. One inventory | Nocapture package test and `cargo +1.99.1 bench --locked -p concord-bench --bench runtime -- --list`, worktree cwd, leased | 0; every promised pair and emitted ID classified | **0** for both; 92 registered cases / 45 groups / 47 comparisons; 230 distinct listed IDs; setup/boundary/comparator printed for every synchronous ID; partial-floor/different-feature families annotated at registration and in README |
| 3. Every equal-work pair asserted | Ordinary package test, worktree cwd, leased | 0; stage observations agree for every registered pair, including all body sizes, header variants, S2 and S3 | **0**, 10 tests pass, no filtering; two fresh executions per case. Version/timeout/flags included; absence preserved. |
| 4. Negative and positive controls | Same ordinary test | 0 because deliberate differences are rejected by `assert_ne!`, positives accepted | **0**; all six named comparator tests pass, plus corrected S3. Details below. No unrelated compile error was counted as detection. |
| 5. Historical notice and timed costs | `git diff --check` and README relative-link existence assertions, worktree cwd | 0; historical files untouched; raw numbers retained with linked notice | **0**; all relative links exist. README names affected S3 prepare/combined observations, preserves 852.8/662.6 ns and 869.9/662.5 ns examples and links original summaries/allocation table. No corrected percentage. |
| 6. Final affected measurements and controls | Exact exclusive commands below and in COMMANDS.md, worktree cwd | 0; only six S3 and five unchanged minimal IDs; 100 raw samples each; two allocation rows per selection | **0** for all three Criterion runs and both allocation commands; each timing log says `ARCHIVED 11 benchmark IDs with estimates and samples`; 33 sample files / 3300 sample rows verified; every allocation row `exact: true` |
| 7. Required package checks | Three exact leased workspace commands above | All exit 0 | **0**, **0**, **0**; final test, clippy and bench receipts bind the same final source manifest |
| RV03 candidate replay | Named `enforce-shipped-equality` command below; copied consumer cwd | 0; shipped pair contract equal and final original strict equality passes | **0**; `R01-shipped-pair expected_equal=true`, `contract_equal:true`; all 12 recorded cases pass; final equality witness below |
| Common branch commit | Normal `git add ... && git commit ...`, worktree cwd | 0; commit on shared `v42/m01-bench` | **128 before commit**: `Unable to create .../index.lock: Read-only file system`. Portable commit/bundle prepared and verified; coordinator action remains necessary. |

### Comparator controls

All execute inside the ordinary test command; a negative is successful only if the same comparator used by registered-pair tests distinguishes it.

- `comparator_detects_altered_and_missing_accept`: changed Accept value and missing Accept both rejected.
- `comparator_detects_repeated_value_order_and_multiplicity`: one changed repeated value, same-name order swap and removed duplicate each rejected.
- `comparator_detects_exact_url_and_method`: same query multiset in another exact order and changed method rejected.
- `comparator_detects_body_changes_and_absence_versus_empty`: changed bytes, empty body and removed body rejected; separate absent-versus-present-empty control rejected.
- `comparator_detects_native_configuration_and_sensitivity`: timeout, HTTP version and header sensitivity changes each rejected.
- `comparator_accepts_reconstruction_and_cross_name_insertion_order`: independently rebuilt request and cross-name insertion reorder accepted, retaining repeated-value order.
- `s3_prepares_json_accept_before_client_defaults`: both sides explicitly prepare application/json even with a text/plain native client default; sensitive bearer header and absent body pinned.
- `registered_pairs_match`: repaired full S3 pair and every other registered equal-work group pass. No header sorting erases same-name order.

## Candidate RV03 replay

Historical input directory was only read: `/home/canarit/projects/concord-v42/replay-v41/consumer-4/`.

Copied to `/home/canarit/projects/concord-v42/lanes/m01-bench/consumer-4-candidate/`; its three local dependency paths point explicitly into this worktree. It has its own retained lock, SHA256 `55121ab25bce0160a36e92c4c97e8821b086ae70e16447b0b3270b48012e4347`, unchanged from the copied initial lock. The workspace lock is separately unchanged.

The historical program first asserts the old bug before its final strict enforcement. In the candidate copy only, expectations were updated: shipped pair/header count/Accept presence, matching-client-default result, wildcard-default projection result, and idempotent native Accept correction using `insert` rather than appending a duplicate. Its original comparator and final strict header equality enforcement are unchanged. Added one final output witness. The original main source and exact adaptation are retained in [consumer-4-original-main.rs](consumer-4-original-main.rs) and [consumer-4-candidate.patch](consumer-4-candidate.patch); own-fixture hashes are in [consumer-4-source.json](consumer-4-source.json).

Exact effective command, cwd `/home/canarit/projects/concord-v42/lanes/m01-bench/consumer-4-candidate`:

```sh
CARGO_TARGET_DIR=/home/canarit/projects/concord-v42/wt-m01/target \
RV03_OUTPUT=/home/canarit/projects/concord-v42/lanes/m01-bench/consumer-4-candidate/artifacts/enforce-shipped-equality \
timeout 1200 /home/canarit/projects/concord-v42/tools/heavy.sh cargo +1.99.1 run --offline --locked --bin rv03-bench-sol -- enforce-shipped-equality
```

Expected 0 and actual **0**. Decisive output:

```text
CANDIDATE_SHIPPED_EQUALITY: all original comparator fields equal={"body_presence_and_bytes_equal":true,"contract_equal":true,"header_iteration_equal":true,"header_multiset_equal":true,"method_equal":true,"timeout_equal":true,"url_bytes_equal":true,"version_equal":true}
```

[Full log](checks/consumer-4-candidate.log), [receipt](checks/consumer-4-candidate.receipt.json), [structured summary](consumer-4-candidate/artifacts/enforce-shipped-equality/summary.json), and prepared native captures are retained. All 12 recorded cases passed; no live send was made by this consumer. It retains the historical comparator's multiset semantics; the maintained comparator additionally tests same-name order and flags.

The unmodified V4.1 baseline recipe was **not rerun**. Historical failing logs were inspected, including RV03's intended strict assertion; this is not claimed as a new baseline execution. An initial receipt-helper invocation in the copied consumer cwd failed before Cargo because it assumed cwd was a Git checkout; the helper was corrected to identify this worktree explicitly. That setup failure is not a product/control result.

## Measurements

Rust: `1.99.1 (58cf8f9c3 2026-10-13)`, LLVM 23.1.1, `x86_64-unknown-linux-gnu`. Locked reqwest 0.13.5, Criterion 0.8.2. Default package features: Concord `json` and `reqwest-defaults` (therefore extras), plus the benchmark dependencies' reqwest json/query features. No custom RUSTFLAGS, compiler wrapper, build-job setting, alternate target directory or profile override was set. Criterion used the bench profile; allocation counting used release. Allocation instrumentation is not linked into the Criterion harness.

Workspace `Cargo.lock` SHA256: `a535c0e17ac26c33593d2af9f0608eb24c4c1aca0a9807a4e089ee354f0bc7fb`.

Exact timing commands for N = 1, 2, 3, expanded individually with all outer argv in COMMANDS.md:

```sh
timeout 900 /home/canarit/projects/concord-v42/tools/heavy.sh --exclusive \
  python3 /home/canarit/projects/concord-v42/lanes/m01-bench/measurements/measure.py criterion-runN \
  cargo +1.99.1 bench --locked -p concord-bench --bench runtime -- \
  --save-baseline m01-fixed-runN \
  '^construct/(s3_search_9_query|minimal)/(concord|manual)/(input|prepare|input_prepare)$'
```

Only those 11 matching IDs were timed. Criterion defaults: 3-second warm-up, 5-second requested measurement interval, 100 samples, 95% confidence. Named baseline directories preserve `benchmark.json`, `estimates.json`, `sample.json` and `tukey.json` for every ID; all three raw trees and summaries are in [measurements/](measurements/). There are 11 sample files, each with 100 iteration/time pairs, per run. No same-toolchain V4.1 timing baseline or correction-magnitude attribution was run.

The following are **means with 95% mean CIs in ns from estimates.json**, not terminal slope estimates. All input/combined/control rows are retained in each run's full summary.

| ID below `construct/` | run 1 | run 2 (elevated initial pressure) | run 3 |
|---|---:|---:|---:|
| `s3_search_9_query/concord/prepare` | 849.1 [839.1, 861.7] | 838.8 [835.2, 843.7] | 836.8 [834.7, 839.6] |
| `s3_search_9_query/manual/prepare` | 648.2 [641.4, 658.1] | 664.0 [652.9, 680.3] | 648.6 [642.2, 656.8] |
| `s3_search_9_query/concord/input_prepare` | 857.0 [847.2, 868.3] | 857.7 [850.8, 868.6] | 844.5 [841.1, 849.2] |
| `s3_search_9_query/manual/input_prepare` | 654.4 [650.2, 659.9] | 642.2 [640.2, 644.5] | 644.5 [642.4, 647.1] |
| `minimal/concord/prepare` | 163.9 [162.7, 165.6] | 162.6 [161.7, 163.9] | 163.3 [162.0, 165.0] |
| `minimal/manual/prepare` | 123.9 [122.7, 125.2] | 120.7 [119.7, 122.0] | 119.9 [118.9, 121.1] |

All measuring commands actually acquired the exclusive campaign lease. Host: Intel i5-13600K, 20 logical CPUs, Linux, `powersave` governor. Before/after snapshots include compiler/Cargo versions, CPU details, governor/frequency, load, CPU/I/O/memory pressure, affinity, visible process CPU use and selected build settings. Global idleness is not established by the campaign lease or namespace-local process listing.

| Run | Before → after UTC | 1-minute load before → after | CPU pressure avg10 before → after |
|---|---|---:|---:|
| 1 | 04:29:18 → 04:30:40 | 3.94 → 2.86 | 0.78% → 0.00% |
| 2 | 04:31:31 → 04:32:53 | 10.70 → 4.91 | 12.85% → 0.00% |
| 3 | 04:33:56 → 04:35:12 | 3.60 → 2.66 | 0.03% → 0.00% |

Run 2 was retained, and run 3 was added after pressure subsided. These are bounded current observations; no universal latency, speed improvement, statistical before/after effect or corrected historical percentage is claimed. No performance threshold was added to CI. The filtered harness still performs ordinary untimed group/loopback setup; its runtime shutdown stops its localhost tasks.

Exact allocation commands, same worktree cwd:

```sh
timeout 300 /home/canarit/projects/concord-v42/tools/heavy.sh --exclusive \
  python3 /home/canarit/projects/concord-v42/lanes/m01-bench/measurements/measure.py alloc-s3 \
  cargo +1.99.1 run --release --locked -p concord-bench --bin alloc-count -- \
  construct/s3_search_9_query/ --json /home/canarit/projects/concord-v42/lanes/m01-bench/measurements/alloc-s3.json

timeout 300 /home/canarit/projects/concord-v42/tools/heavy.sh --exclusive \
  python3 /home/canarit/projects/concord-v42/lanes/m01-bench/measurements/measure.py alloc-minimal \
  cargo +1.99.1 run --release --locked -p concord-bench --bin alloc-count -- \
  construct/minimal/ --json /home/canarit/projects/concord-v42/lanes/m01-bench/measurements/alloc-minimal.json
```

Both expected 0, actual **0**; only their two selected cases executed in the counter. Each case warms once and counts 256 reconstructed inputs/consuming calls, retaining outputs until counting stops. Requested bytes include realloc sizes; frees are not subtracted. Consumed inputs are destroyed inside the consuming routine. Every JSON output row has `exact: true`.

| Case | input allocation calls / bytes | prepare allocation calls / bytes |
|---|---:|---:|
| S3 Concord | 6 / 62 | 28 / 2417 |
| S3 manual | 6 / 110 | 24 / 2254 |
| minimal Concord (unchanged control) | 0 / 0 | 9 / 863 |
| minimal manual (unchanged control) | 0 / 0 | 4 / 734 |

No allocation delta is attributed to the Accept repair without an executed comparable baseline.

## Source, artifact identity and branch blocker

The final test, clippy and bench receipts all reference the same 267-file transitive local source manifest, SHA256 `0483d4dcee55e953b0095a8560af81d0acd80439e7f05455b3303f4a080f4ded`. It covers all four production crates, shapes, both benchmark crates, workspace manifest/lock and toolchain pin. Every one of those file hashes was checked against prepared commit `875f21accf564a85b6fb03cff9becf7d64101e07`: [binding receipt](prepared-commit.source-binding.json). The three measurement manifests differ from the final manifest only in the README; **all Rust source, manifests, lock and toolchain bytes match**. Measurements occurred after the Rust code was final. README updates afterward recorded methodology/results only.

All builds used this worktree's `target/`; copied consumer execution explicitly targeted the same directory and kept its own lock. Initial checks performed actual compilation, and the final checks reused Cargo's source-tracked artifacts. No unknown existing binary was substituted. Raw samples, locks, fixture source, patch, commands, hashes and important logs are retained; no target directory or dependency cache is part of the portable evidence set. [evidence.sha256.json](evidence.sha256.json) hashes the retained evidence files (excluding this report and the hash manifest itself).

The normal shared-worktree commit attempt exited **128** at `git add`, before any commit:

```text
fatal: Unable to create '/home/canarit/projects/concord_api/.git/worktrees/wt-m01/index.lock': Read-only file system
```

`findmnt` confirms the worktree Git directory is an `ext4` mount with `ro,nosuid,nodev,relatime`: [mount log](checks/git-mount-status.log). This is a filesystem restriction, not a request for further permission or an automatic approval-review rejection. The failed command is preserved in [git-commit-blocker.json](checks/git-commit-blocker.json).

A lane-local isolated bare Git directory was initialized, using the existing base object store read-only. Its own `v42/m01-bench` branch contains the real final commit with exactly the six owned paths and the required message suffix. `git bundle verify /home/canarit/projects/concord-v42/lanes/m01-bench/m01-bench.bundle` exited **0**, reporting:

```text
875f21accf564a85b6fb03cff9becf7d64101e07 refs/heads/v42/m01-bench
The bundle requires this ref:
07dcdd03c61a1822341e92d3820e66117f1aef14
```

No shared repository ref, owner's checkout, other worktree, tag, remote or publication was modified. The six source changes remain in this worktree; its read-only index/ref state is deliberately not misreported as clean or committed.

## Not run, open questions and next owner action

- Unmodified historical V4.1 replay: not run (optional in this brief); historical logs inspected only.
- Whole benchmark suite and unrelated timing/allocation families: not measured, per the bounded brief. Every synchronous registered case nevertheless executed twice in the ordinary test.
- `scripts/verify.sh`: never run; central gate belongs to the coordinator.
- Native Windows, broad workspace gate and independent release review: not run by this lane; no claim of full release acceptance.
- No live external API, credentials, global setting change, other agent/model session, push, merge, tag or publication. Owned loopback requests had finite client timeouts in tests; enclosing commands were bounded, and all started commands exited. No owned process remains.
- No source questions remain. No change in another source owner's files is requested.
- **Coordinator action required:** import [m01-bench.bundle](m01-bench.bundle) (it contains `refs/heads/v42/m01-bench` at the prepared SHA, with the existing starting SHA as prerequisite) and integrate the validated commit through the coordinator's writable Git context. The shared worktree branch update could not be completed from this lane's filesystem. The patch is available as an alternate handoff. Independent review/integration and final campaign records remain coordinator-owned.
