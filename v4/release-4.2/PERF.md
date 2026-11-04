# 4.2 — performance: affected measurements and the two Stage 5 decisions

All timing ran under the campaign's exclusive lease (`evidence/heavy.sh --exclusive`), rustc 1.99.1,
the locked graph, on one shared Linux host (i5-13600K, powersave governor, no pinning). Raw
estimates, samples and host snapshots stay in the work directory's lane folders; the reports are in
[reports/](reports/). No performance threshold was added to CI.

## M01 — S3 benchmark equivalence (affected measurements only)

Historical S3 ratios (4.1 README: Concord vs manual prepare 852.8/662.6 ns, combined 869.9/662.5 ns)
compared **unequal prepared work** (only Concord prepared `Accept: application/json`). They are kept
in `benches/criterion/README.md` with a linked notice; no corrected percentage is derived from them.

4.2 repairs the manual side (it prepares the same `Accept`) and connects every promised pair to the
registered stage comparator (`benches/criterion/src/tests.rs`, 47 comparisons over 92 cases, run by
the workspace test gate). Affected IDs re-measured after the code was final (lane m01-bench,
`heavy.sh --exclusive`, rustc 1.99.1, locked graph, Criterion 0.8.2 defaults, i5-13600K, powersave
governor; 3 runs, run 2 under elevated host pressure). Means in ns with 95 % CI from
Criterion's estimates:

| ID (`construct/…`) | run 1 | run 2 | run 3 |
|---|---:|---:|---:|
| `s3_search_9_query/concord/prepare` | 849.1 [839.1, 861.7] | 838.8 [835.2, 843.7] | 836.8 [834.7, 839.6] |
| `s3_search_9_query/manual/prepare` | 648.2 [641.4, 658.1] | 664.0 [652.9, 680.3] | 648.6 [642.2, 656.8] |
| `minimal/concord/prepare` (unchanged control) | 163.9 | 162.6 | 163.3 |
| `minimal/manual/prepare` (unchanged control) | 123.9 | 120.7 | 119.9 |

Allocation counts (separate run, release, 256 inputs): S3 Concord prepare 28 calls / 2417 B,
S3 manual prepare 24 / 2254 B; minimal controls 9 / 863 and 4 / 734. No before/after attribution
of the Accept repair is claimed (no same-toolchain 4.1 timing baseline was re-run). Raw estimates,
samples and host snapshots: lane `m01-bench/measurements/`.

## O02 — response metadata materialization: DEFER_WITH_EVIDENCE

Candidate (lane o02-metadata, `candidate.patch`): the response media scanner skips constructing
`String`s for non-`charset` parameters while still reading and validating every byte. Semantics:
79,328 differential observations (2,479 header-field combinations × Text/Json/Form/IgnoreContentType
× valid/invalid bodies × four feature sets) identical, comparing check result, `CodecError` label,
numeric context, source, Display/Debug, Context and payload; metadata-refusal retention suite passes;
an early-exit mutant (stop after the first charset) is caught by the malformed-tail assertion.

| Median ns/call (B1 / C1 / C2 / B2) | bare head | charset head | 12-param head | 12-param head+decode 16 B |
|---|---|---|---|---|
| | 65.0 / 63.0 / 63.5 / 67.2 | 108.2 / 109.5 / 106.4 / 110.5 | 310.6 / 223.0 / 214.3 / 312.2 | 346.5 / 250.9 / 242.5 / 345.6 |

Allocations per head: bare 1 → 1, charset 3 → 3, twelve parameters 13 → 1. Ordinary headers show no
change beyond noise; the benefit appears only for parameter-heavy synthetic headers whose frequency
in real workloads is unknown. Decision: **DEFER_WITH_EVIDENCE** (lane, coordinator, and the
independent review `rv-stage5`, which re-ran the corpus, added a 7.1M-input direct differential with
0 differences, and reproduced the timings). Next trigger: a real profile in which `media::single`/
`charsets` is at least 1 % of decode CPU; then register the cases in the Criterion registry, keep
the corpus and the direct differential as maintained tests, and review again. The patch is kept as
lane evidence only; nothing is integrated.

## O01 — native header planning: DEFER_WITH_EVIDENCE

Candidate (lane o01-header-plan, `candidate.patch`, private planner in `crates/concord-core/src/__private.rs`):
keeps the existing scan for chains of at most 16 contributions; above, sorts a stack array of indices
by native name and original position and processes equal-name groups. No heap allocation, no emission
change, no new API. Criteria sealed before timing: zero added planner allocations; warm-median
slowdown ≤ 5 % at 1/8 and ≤ 10 % at 16/32; ≥ 20 % full-prepare gain on mixed 64/128 chains.

Semantics: independent sequential oracle, 1,792 valid/invalid cases (sizes 1–512, complete live and
sensitive arrays, flags, exact error Display, validator fields); a liveness mutant is caught; public
controls (`native_headers`, `native_keys`, `native_limits`, `spelling_equivalence`, both oracles,
`validate_names`) plus a new long-chain ordering regression pass on baseline and candidate; the
bench registry's comparator passes with 134 registered cases.

Mixed chains, median µs (baseline / candidate), order B A A B B A A B, restored B4/B5 controls:

| contributions | isolated plan | full prepare | full ratio |
|---:|---|---|---:|
| 1 | 0.014 / 0.014 | 0.107 / 0.110 | 0.97× |
| 8 | 0.272 / 0.263 | 0.432 / 0.432 | 1.00× |
| 16 | 0.830 / 0.829 | 1.110 / 1.111 | 1.00× |
| 32 | 2.566 / 0.848 | 3.566 / 1.530 | 2.33× |
| 64 | 10.19 / 2.03 | 12.49 / 3.35 | 3.73× |
| 128 | 42.92 / 4.62 | 44.05 / 7.29 | 6.04× |
| 512 | 695.7 / 21.7 | 714.3 / 31.7 | 22.6× |

Isolated planning allocates 0 calls on both arms at every size; full-prepare allocation counts are
identical across arms. Costs: about 24·N bytes of stack above N = 16 (peak stack not measured);
+0.63 % fixture binary size. Unchanged control drift B0→B4 +0.35 % (one transient +25.8 % in B1,
kept in the aggregate). D22-terra follow-up completed in the same fixture: baseline → candidate →
baseline and baseline → eager-allocation control → baseline, source-bound (only `crates/concord-core/src/__private.rs`
differs), restored measurements match.

Decision: **DEFER_WITH_EVIDENCE** (lane, coordinator, and the independent review `rv-stage5`, which
found no counterexample in a 30,456-chain byte-identical differential up to N = 2048 plus a
macro-level differential, caught two seeded mutants, saw identical stack-failure points for both
arms under reduced stacks, and reproduced the timings). Ordinary chains (≤ 16) run the identical
algorithm and gain nothing; small-case layout noise reaches ±5–9 %. Next trigger: a real declaration
with native chains of 32 or more where `prepare()` is at least 1 % of request CPU; then adopt with
`o01_header_groups` and a differential as maintained tests and measure release peak stack. Nothing
integrated; patch and fixtures retained as lane evidence.
