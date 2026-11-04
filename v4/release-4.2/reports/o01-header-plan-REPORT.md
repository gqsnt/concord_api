# O01 native header planning — completed experiment

Decision: **DEFER_WITH_EVIDENCE**. Retain the baseline for this release unless the coordinator's independent review and workload decision justify adopting the attached candidate. One allocation-free optimization candidate was completed and compared; no second optimization was needed. It meets the predeclared *observed warm-median* bounds and substantially improves the measured 32/64/128 chains, not merely 512. This is not independent adoption approval or a universal equivalence/performance claim.

Next decision trigger: independently review `candidate.patch` against the semantic corpus and large-chain stack use, and establish that native chains of at least 32 contributions occur on a relevant hot preparation path (or explicitly accept optimizing this bounded workload without caller profiles). Recheck the small-case bound on a quieter host if that bound must be guaranteed: one unchanged control had a transient 26% excursion and first-use small-case observations are noisy. No further implementation is required to make the candidate reviewable.

Branch: `v42/o01-header-plan`. Starting and final HEAD: `4874405bffa9f70716c4d5cb208852edec0fd05d`. **No new commit**: the brief explicitly says Git is read-only for Codex; no Git mutation/commit was attempted. The production planner is restored byte-for-byte to baseline. Tests and experiment fixtures remain uncommitted. Actual worker model/effort is not observable in this runtime; neither is inferred. No agents, other model sessions, pushes, merges, tags or publication were used.

## Criteria declared before timing

[PLAN.md](PLAN.md) was written before any timing, with zero allowed behavioral differences. Initial plan SHA-256, printed before semantic execution: `a6f0eba3433fa37224ba350c885d5e5c52ddcf2b357604bcfce6dd6d3421151c`. Later fixture clarifications were also written before timing; they did not change acceptance bounds. Final plan SHA-256: `36602222338c131583b27cba7e09d281494784892a9d9703aa2b9085b02cdcec`.

The fixed limits were zero added planner allocations at every size; warm median slowdown ≤5% at 1/8 and ≤10% at 16/32 for isolated planning and full prepare; repeatable ≥20% full-prepare benefit on mixed 64/128 chains. Observed largest positive slowdown across the three patterns and the two boundaries was about 3.6% at 1, 0.2% at 8, 3.0% at 16; every 32 case improved. These are observed sample medians, not confidence bounds. Zero added allocations and the 64/128 benefit were reproduced. Noise prevents turning the small percentages into guaranteed limits.

## Implementation and exact files

The private candidate keeps the scan for N≤16. Above 16 it sorts a stack array of indices by native name plus original index, builds predecessors for the structural check, then processes equal-name groups for liveness/classification. Diagnostics still run in original declaration order, with the media conflict last. There are no heap tables, new dependencies, emission changes or second public library API. The threshold was fixed before timing and is supported by the measured 32/64/128 results. Unlike the old R02 prototype, this adds no heap allocation.

[**candidate.patch**](candidate.patch) contains exactly:

- `crates/concord-core/src/__private.rs` (candidate source, currently restored in the worktree).
- `crates/concord-core/tests/o01_header_groups.rs` (new independent sequential oracle).
- `crates/concord/tests/o01_header_order.rs` (new long-chain public contract regression).

Patch SHA-256: `845d8689a412d486ce59828736174ac312ed98318002f905ae1440d77ca20f90`. A dry run against a lane-local baseline tree passed, exit 0, with all three paths checked: [log](raw/candidate-patch-check.log). The complete candidate source is also [candidate-private.rs](candidate-private.rs); the original is [baseline-private.rs](baseline-private.rs).

[experiment-fixtures.patch](experiment-fixtures.patch) separately preserves the six benchmark files: modified `benches/criterion/src/lib.rs` and `src/tests.rs` (registration and inventory counts only), new `src/o01.rs`, `src/o01_generated.rs`, `src/bin/o01-timing.rs`, `src/bin/o01-alloc.rs`. Those six and the two new regression files are the eight uncommitted worktree paths. Existing registry edits are the minimum needed to register the new cases through its equivalence test: 92→134 cases, 45→66 groups, 47→68 comparisons, six→seven synchronous families. No shared Cargo.toml or Cargo.lock was edited.

[candidate-files.json](candidate-files.json) records exact paths and hashes. The generator and all driving/summary scripts are lane-local. `liveness-mutant.patch` and `eager-allocation-control.patch` are deliberately faulty qualification controls, not adoption candidates. There are no rejected optimization variants hidden in the tree.

## Semantic comparison and acceptance evidence

All command cwd values below are `/home/canarit/projects/concord-v42/wt-o01`. Every compiling command goes through `/home/canarit/projects/concord-v42/tools/heavy.sh`, with a finite 900-second timeout, Rust `+1.99.1`, `--locked`, and this worktree's `target/`. Final commands set `RUST_MIN_STACK=67108864` **only in the command environment**, after the generated fixture overflowed rustc's default thread stack. No machine setting or toolchain was changed.

For compactness, `C` below expands to `RUST_MIN_STACK=67108864 timeout 900 /home/canarit/projects/concord-v42/tools/heavy.sh cargo +1.99.1`; `Q` adds `--exclusive` immediately after `heavy.sh`. Exact argument vectors, cwd, environment, source hashes, expected/actual exits and logs are in [RUN_LEDGER.json](RUN_LEDGER.json) and `raw/<run>.json`.

The public command was:

```text
C test --locked -p concord --test native_headers --test native_keys --test native_limits --test spelling_equivalence --test oracle_composition --test oracle_sensitivity --test validate_names --test o01_header_order
```

| Acceptance item / command | Expected exit and assertion | Actual / decisive witness |
|---|---|---|
| Declare criteria before timing; read PLAN and source | Criteria sealed before timing; no behavior changes | Plan existed before timing; final fixture clarifications precede B0, whose exclusive lease starts 05:01:45 UTC |
| Baseline and candidate: `C test --locked -p concord-core` | 0; maintained exhaustive oracle plus independent long chains and docs pass | Both 0; core: 3 maintained tests, 1 new oracle test, 6 positive and 2 compile-fail doctests pass. [baseline](raw/baseline-sealed-core.log), [candidate](raw/candidate-core.log) |
| Baseline and candidate: public command above | 0; exact maintained public controls and long ordering regression pass | Both 0; 297 tests across eight binaries on candidate, including `long_chain_preserves_suppression_cadence_classification_and_order ... ok`. [baseline controls](raw/baseline-sealed-public.log), [final baseline long test](raw/baseline-fixture-final-order.log), [candidate](raw/candidate-public.log) |
| Baseline and candidate: `C test --locked -p concord-bench --lib -- --nocapture` | 0; reconstructed registered prepared outputs equal independent native reference | Both 0; `EQUIVALENCE: 134 registered cases; 66 groups; 68 comparisons; fresh inputs checked twice`; `10 passed; 0 failed`. [baseline](raw/baseline-fixture-final-bench.log), [candidate](raw/candidate-bench-parity.log) |
| Qualify independent liveness oracle: `C test --locked -p concord-core --test o01_header_groups -- --nocapture` | Clean 0 → viable mutant 101 at intended live-array assertion → restore 0 | Mutant 101: `assertion ... failed: live N=17 seed=0 mode=0`; restored 0, all eight sizes print `224 valid/invalid cases`. [mutant](raw/candidate-liveness-mutant.log), [restore](raw/candidate-oracle-restored.log) |
| Full prepare, isolated plan, first-use/warm, all sizes: `Q run --release --locked -p concord-bench --bin o01-timing` | 0 and full finite sample inventory; unchanged control every process | B0/A0/A1/B1/B2/A2/A3/B3/B4 all 0, each `O01_TIMING_COMPLETE`. Source-switch logs visibly recompile core and consumers. `raw/<phase>-timing.log/json` |
| Separate allocation measurements: `Q run --release --locked -p concord-bench --bin o01-alloc` | 0; calibration 1/257, all isolated plans 0/0, full-prep counts comparable | B0/A0/B4/B5 all 0; `CALIBRATION,1,257`; all 42 rows exactly equal across arms/restorations; `O01_ALLOCATION_COMPLETE`. [table](ALLOCATION_TABLE.md) |
| D22 eager-control semantics: `C test --locked -p concord-core --test header_plan --test o01_header_groups` | 0 despite allocation injection | 0; all four tests pass. [E0 semantics](raw/E0-semantics.log) |
| D22 allocation negative control: allocation command on E0 | 101 at intended counter assertion, not a compile/setup error | 101; `ALLOC,plan,1/distinct,1,1`; `O01 planner allocation contract`; `left: (1, 1)`, `right: (0, 0)`. [log](raw/E0-alloc.log) |
| D22 same-fixture timing/control restoration | E0 timing 0, restored B5 allocation/timing/oracle 0; exact source restored | All expected exits observed; B5 `O01_TIMING_COMPLETE`, `O01_ALLOCATION_COMPLETE`, oracle `4 passed; 0 failed`. [restored oracle](raw/B5-core-restored.log), [closure](SOURCE_CLOSURE.json) |
| Reviewable patch / whitespace | patch dry-run 0; `git diff --check` 0 | Both 0; [patch check](raw/candidate-patch-check.log) |

The independent model is a forward per-name declaration-state/list interpreter over integer identities; it never calls production composition to produce expectations. It checks 1,792 cases: 32 seeds × seven valid/invalid modes × 1/8/16/17/32/64/128/512. It compares **complete** live and sensitive arrays, absent else-inherit live bits, all three special flags, exact prepare error Display strings, and validator endpoint/declaration/earlier/reason. It includes uppercase parsing into separate native HeaderName objects, all eight reserved names, duplicate Set, missing target, media conflict and earlier-error priority. Maintained short exhaustive tests provide neighboring coverage.

The 21 new public pairs independently pin complete prepared method, URL, per-name bytes/order/sensitivity, body absence, version and timeout. Distinct names are permuted, aliases use separate native const/static spellings, and mixed inherited chains exercise remove/reintroduction, override, absent inheritance, present empty-each replacement, empty append and absent append. Final mixed inputs are explicitly sensitive, so their expected flags follow directly from declarations. Partial per-name marks are checked by the independent direct model and maintained public classification controls. The new 18-contribution inherited fixture verifies native cached Authorization classification, repeated preparation, suppressed failing formatting, empty each, owner remove order, sticky classification, initializer cadence, structural failure before header values and body/path/query failure before header composition. No emission/default/cache logic was modified.

Positive comparator reconstruction and negative missing/altered Accept, URL/method, body absence, value order/multiplicity, native settings and sensitivity controls all pass. Mutation accounting: **2 viable killed, 0 surviving, 0 equivalent/uncertain, 0 unviable, 0 timeout** across the two intentionally introduced mutants; one qualifies liveness, one allocation counting, not the entire system.

## Measurements

Protocol and exact setup/drop boundaries: [METHOD.md](METHOD.md). Raw elapsed samples, iterations and process labels: [timings.json](timings.json), `raw/*-timing.log`; full result tables: [warm](TIMING_TABLE.md), [first use](COLD_TABLE.md), [controls](CONTROL_TABLE.md). `summarize.py` reproduces tables. No instrumented allocator is linked into the timing executable. Compilation, input/name setup and printing are not timed as plan/preparation work. Full warm prepare includes output drop; a separate total includes fresh endpoint/scope construction too.

Main source order was **B0 A0 A1 B1 B2 A2 A3 B3**, then restored **B4**. Four adjacent B/A or A/B pairs balance order. Tables take each process's sample median, then the median of four processes per arm; B4/B5 and E0 are excluded from optimization aggregates. There are five warm samples per plan/prepare and three per total in each process. Every case also has a first-use row in a fresh process. These first-use rows are not cache-flushed CPU-cold measurements or isolated OnceLock costs. First-use aliases at 8 were noisier/slower for candidate (median 5.30→7.67 µs total); no small cold-case benefit is claimed.

Mixed chains, microseconds per operation:

| contributions | isolated baseline / candidate (µs) | full prepare baseline / candidate (µs) | full speed ratio |
|---:|---:|---:|---:|
| 1 | 0.014 / 0.014 | 0.107 / 0.110 | 0.97× |
| 8 | 0.272 / 0.263 | 0.432 / 0.432 | 1.00× |
| 16 | 0.830 / 0.829 | 1.110 / 1.111 | 1.00× |
| 32 | 2.566 / 0.848 | 3.566 / 1.530 | 2.33× |
| 64 | 10.189 / 2.034 | 12.491 / 3.345 | 3.73× |
| 128 | 42.917 / 4.618 | 44.050 / 7.292 | 6.04× |
| 512 | 695.707 / 21.677 | 714.255 / 31.655 | 22.56× |

Distinct and alias gains also appear at 32/64/128. At 32 full prepare, distinct was 5.894→2.681 µs and aliases 3.064→1.646 µs. The 512 results are scaling observations, not the basis for threshold selection. No empirical universal complexity exponent or product throughput forecast is inferred.

Unchanged literal prepare medians were B0 191.08 ns, A0 194.83, A1 191.43, B1 240.42, B2 193.12, A2 190.08, A3 196.19, B3 191.19, B4 191.74, B5 188.97. B0→B4 drift is about +0.35%; B0→B5 about −1.10%, while B1's transient was about +25.8%. All phases, including B1, remain in the raw data and main aggregation. Small differences within such noise are inconclusive.

### Allocation table

Isolated planning: **0 calls / 0 requested bytes** for every size/pattern on B0, A0, B4 and B5. Full prepare below has the same values on all four; no allocation tradeoff was hidden by timing. Bytes are cumulative requested allocation/reallocation sizes, not peak/retained memory; output destruction occurs after counting.

| contributions | distinct calls / bytes | aliases calls / bytes | mixed calls / bytes |
|---:|---:|---:|---:|
| 1 | 3 / 676 | 3 / 676 | 3 / 676 |
| 8 | 5 / 1988 | 6 / 1542 | 3 / 676 |
| 16 | 7 / 4612 | 7 / 2694 | 4 / 964 |
| 32 | 9 / 9860 | 8 / 4998 | 6 / 2276 |
| 64 | 11 / 20356 | 9 / 9606 | 9 / 5476 |
| 128 | 13 / 41348 | 10 / 18822 | 12 / 11876 |
| 512 | 17 / 167300 | 12 / 74118 | 18 / 50276 |

The candidate does add stack storage: an N-index array and an `Option<usize>` predecessor array (source-level combined storage roughly 24N bytes on this 64-bit target, 12 KiB at N=512, before optimizer reuse). Actual peak stack was not measured. This tradeoff deserves review beyond the tested bound.

Emission is unchanged, so an emission compile/code-size matrix is not applicable. The measured all-fixtures release timing binary was 9,921,648 bytes baseline and 9,983,872 candidate (+62,224, about 0.63%). This includes many const-generic fixture instantiations and symbols; it is not a shipping application/library footprint estimate. Build/lease wall times are not compiler performance comparisons.

## D22-terra restoration follow-up and source binding

Two complete source-bound restorations ran **in this exact O01 fixture**:

1. B0 baseline → A0/A1 grouped candidate → B1 baseline, with further balanced repeats and B4 allocation/timing restoration.
2. B4 baseline → E0 eager-allocation control → B5 baseline, using the same input/output-barrier timing executable and separately instrumented allocation executable. E0's semantic test passed before its intended allocation assertion failed; E0 timing and B5 restored timing completed.

No different fixture's restored result and no archival restored receipt is substituted. This is a new completed follow-up, not a retroactive pass of D22-terra's blocked historical execution. Chosen scaling observations are the seven-size/three-pattern isolated and full-prepare rows above and their raw samples.

`SOURCE_CLOSURE.json` verifies every recorded Rust source is identical between B0 and B5, and only `crates/concord-core/src/__private.rs` differs for A0 and E0. Core hashes:

- Baseline/restored: `28bd0a9267ab4c2958581a8eac0275d104a75f6d404d47680bff10442052881d`.
- Group candidate: `fead07333b7175b88c701fe85eeb9c95fa37b1977c0c8418f5d8afb9fbde4973`.
- Eager control: `ab1e9de52c7e4948f902b98a5b3239cdbce42a4954985f613cfcc4af96de91e7`.

All timing lock hashes are `d490302056098b035d5769c63e27ce10549e48f0f5f8dbf49d5457a71a8971ce` (the integrated 4.2 graph, not the old review's lock). Baseline B0/B1/B2/B3/B4/B5 timing binary hash is `b30a82bf40673baeb6e9d44bb841c5299184beaa2bfd59e0b13a0d6e4b17d92d`; candidate A0/A1/A2/A3 is `2daf8db3e17d325c9974f14eef71410409877d0e59449b5dad32e77b57e431e9`. Source switches really compile core and consumers; same-arm repeats invoke Cargo on identical qualified source. Hashes plus Cargo logs are local source/binary evidence, not cryptographic compiler attestation.

## Host, failures, limitations and handoff

Linux x86_64, Intel i5-13600K, 20 logical CPUs, `powersave` governor. Observed rustc: **1.99.1 (58cf8f9c3 2026-10-13), LLVM 23.1.1**; [version output](raw/rustc-version.txt). Main/restored timing snapshots had 1-minute load about 2.08–8.56. CPU pressure was not uniformly zero. Complete initial CPU/memory data is in [initial-host.json](raw/initial-host.json); before/after governor/frequency/load/CPU/IO pressure snapshots are in each timing JSON. Exclusive campaign leases do not exclude unrelated machine tasks or fix frequency/scheduling. No global settings or CPU affinity were changed.

Preliminary fixture failures are retained and **not credited as detection or parity**. Initial benchmark compilation reported an unused N=8 parameter and rustc stack overflow (exit 101); the default-stack repeat also overflowed (101). The first long public test omitted Encode::CONTENT_TYPE (101), then passed after correction. `baseline-final-bench-parity` and `baseline-sealed-bench-parity` expected 0 but returned 101 on my incorrect mixed sensitivity expectations. These were fixture/reference errors. Before timing, the final mixed sources were explicitly marked and independently pinned, the final public long test passed, and baseline/candidate complete parity passed. This makes the classification limitation explicit: the final mixed request reference does not independently model every unmarked shared-value macro-classification closure. Maintained public oracles and the independent planner model cover partial classification; no classification repair is claimed.

Unrun/unclaimed: production caller chain distributions; independently scheduled adoption review; native Windows/other architectures; other compiler versions/feature matrices; truly hardware-cold or isolated classification-cache timings; extreme N beyond 512 and peak stack/resident memory; full compile-edit statistics; response/transport timing or live APIs; universal correctness/security assurance. The coordinator-owned `scripts/verify.sh`, workspace-wide gate, and replay consumers were intentionally not run. Existing public tests use their owned loopback fixtures; the new timing/allocation fixtures perform no I/O. All spawned commands completed and were waited; no owned listener or detached process remains. No historical report, evidence, cases.json, v4 record or owner checkout was changed.

Open adoption questions: caller relevance of ≥32 native contributions; acceptable stack/code-size costs; whether a quieter-host confirmation should certify the small-case bound. Required other-owner code changes: **none**. Coordinator action is the explicit independent review/adoption decision; optional benchmark retention is available as its own patch. This lane is complete with a deferred candidate and source-bound measurements; no production change is silently integrated.
