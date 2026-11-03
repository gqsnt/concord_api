# Performance

All measurements: Linux 7.0.0-42 (x86_64, Intel i5-13600K, 20 logical CPUs), rustc 1.99.1
(`58cf8f9c3`), one machine, each timed phase started only after the machine was idle (no cargo,
rustc or review agent running, 1-minute load below 2.5, checked three times a minute apart). Raw
output, with the script that produced it (perf_modular.sh: phases 1–2 in one run, phases 3–5
resumed with `FROM=3` after `fcf343f`): the work directory's evidence/perf/run-final-2cbac44/
(Criterion data, hyperfine JSON, logs, environment*.txt). The scripts it drives are in this
repository: `benches/criterion`, `benches/scale/ab.sh`, `benches/scale/gen.py`.

| State | Commit | What it is |
|---|---|---|
| S2 | `686cd24` | `dfdeed8`'s code on Rust 1.99.1 with only the required compatibility changes (the same-compiler baseline) |
| SX | `dc7f134` | the four-crate split, behaviour unchanged (format words, no metadata) |
| S3 | `2cbac44` (phases 1–2), `fcf343f` (phases 3–5) | the final candidate. The two commits' library code differs only by lint attributes on `concord-extras` items that the measured `json`/default builds compile identically (`fcf343f` gates `check_charset` and allows a dead field without `json`/`form`) |
| R5 before / after | `3a836e7` / `a4b465a` | native-key occurrence owners: a scan of every owner before, an index after |

The state-1 baseline (`dfdeed8` on its own pinned 1.97.1) is a gate receipt only (STATUS); it is
not compared here, so compiler changes are not attributed to this increment (PLAN: compare S2 with
S3).

## 1. Allocations (exact counts, `concord-bench --bin alloc-count`)

92 rows (case × stage). 84 are identical in S2 and S3. The 8 that differ:

| Case | Stage | S2 allocs / bytes | S3 allocs / bytes | Why |
|---|---|---:|---:|---|
| `construct/minimal/concord` | prepare | 7 / 207 | 9 / 863 | the typed JSON endpoint now sends `Accept: application/json` (the first header of reqwest's `HeaderMap` allocates its table) |
| `construct/minimal/manual` | prepare | 2 / 78 | 4 / 734 | the hand-written baseline sends the same `Accept` (bench parity); Concord's overhead over it is unchanged: 5 allocations, 129 bytes, in both states |
| `decode/json/{100B,10KB,1MB}/concord` | decode | n / b | n+1 / b+16 | `Json`'s new `Content-Type` check: one 16-byte allocation, consistent with `mime` copying `application/json` |
| `decode/text/{100B,10KB,1MB}/concord` | decode | 2 / b | 5 / b+111 | `Text`'s new charset check on `text/plain; charset=utf-8`: three allocations, consistent with `mime`'s copy of `text/plain` (10 B), the charset list (96 B) and `utf-8` (5 B) |

Every count is exact (`exact = true` in every row). Before this increment the formats checked no
response head; the added allocations are the price of metadata validation, per response, fixed in
size (they do not grow with the body).

## 2. Runtime (Criterion; S2 and S3 with the same short settings: 1 s warm-up, 3 s measurement)

Full table: summary-s3-vs-s2.md in that directory (232 rows). Short runs on a desktop machine: differences within
about ±10% appear in both directions for code this increment did not touch (the hand-written
`manual` baselines move by up to −15%/+21% between the two runs), so only larger, explained
differences are reported.

| Benchmark | S2 | S3 | Change | Reading |
|---|---:|---:|---:|---|
| `construct/minimal/concord/prepare` | 138.9 ns | 159.9 ns | +21 ns | the `Accept` header; `manual` moves the same way (99.1 → 119.9 ns): the overhead over hand-written reqwest stays ~40 ns |
| `decode/bytes/*/concord/decode` | ~139–141 ns | ~150–154 ns | +11–13 ns | the decoder's head step (a function call returning the body step; no allocation for `Binary`'s `()` context) |
| `decode/json/100B/concord/decode` | 562.9 ns | 630.3 ns | +67 ns | `Json`'s `Content-Type` check; at 10 KB and 1 MB the check is lost in decoding (both `concord` and `manual` measured 6–8% faster in S3) |
| `decode/text/100B/concord/decode` | 160.3 ns | 265.8 ns | +105 ns | `Text`'s charset check with three allocations; at 1 MB it is not measurable |
| `headers`, `keys`, `compose`, `construct/*` (other), `body_encode`, `roundtrip` | — | — | within noise | request building, header composition and the loopback round trip are unchanged |

The metadata checks cost a fixed tens to about a hundred nanoseconds per response; a loopback round
trip costs 8–35 µs (`roundtrip/*`), a real network far more.

### Header-plan scaling (H14, S3 only)

`plan_scaling`: one native chain of N contributions, composed (`prepare()`'s header step) and
validated (`validate_header_names()`'s structural pass).

| N | appends compose / validate | distinct compose / validate | overrides compose / validate | removals compose / validate |
|---:|---:|---:|---:|---:|
| 8 | 288 ns / 33 ns | 334 ns / 75 ns | 234 ns / 36 ns | 239 ns / 34 ns |
| 32 | 3.34 µs / 134 ns | 3.06 µs / 737 ns | 2.13 µs / 142 ns | 2.07 µs / 133 ns |
| 128 | 48.2 µs / 492 ns | 49.2 µs / 12.0 µs | 31.6 µs / 556 ns | 32.0 µs / 491 ns |
| 512 | 685 µs / 2.07 µs | 925 µs / 235 µs | 421 µs / 2.23 µs | 423 µs / 2.01 µs |

Validation is linear except for N distinct names (each new name is compared with the earlier
ones: quadratic, 235 µs at 512). Composition grows quadratically with N in every shape; the
group is new in this increment, so there is no earlier number to compare with. Real chains have a
handful of contributions (the Riot client: under 10), where both stay below a microsecond; a chain
of hundreds of native contributions is outside the intended use and would cost under a millisecond
per request.

## 3. Compile time

hyperfine, 3 runs each after a warm build, `cargo +1.99.1 … --offline --locked -q`, touching the
row's source file before each run (`benches/scale/ab.sh`; generated suites of 13 crates: S2 in the
words syntax, SX and S3 in the type-path syntax with `features = ["extras"]`, otherwise
`default-features = false`). Seconds, mean ± standard deviation:

| Row | S2 | SX (split) | S3 (final) | SX/S2 | S3/S2 |
|---|---:|---:|---:|---:|---:|
| `check -p scale1000` (1,000 endpoints) | 1.754 ± 0.020 | 1.768 ± 0.007 | 1.804 ± 0.017 | 1.01 | 1.03 |
| `build -p scale1000` (debug) | 1.923 ± 0.040 | 2.070 ± 0.154 | 2.295 ± 0.068 | 1.08 | **1.19** |
| `build --release -p scale1000` | 6.473 ± 0.012 | 6.503 ± 0.025 | 6.615 ± 0.016 | 1.00 | 1.02 |
| `check -p scale_deep` | 0.263 ± 0.005 | 0.267 ± 0.007 | 0.264 ± 0.002 | 1.02 | 1.00 |
| `build --release -p scale_deep` | 0.988 ± 0.019 | 0.988 ± 0.006 | 0.991 ± 0.015 | 1.00 | 1.00 |
| `check -p scale_shared` | 0.719 ± 0.007 | 0.741 ± 0.004 | 0.733 ± 0.013 | 1.03 | 1.02 |
| `build --release -p scale_shared` | 3.272 ± 0.006 | 3.276 ± 0.026 | 3.307 ± 0.017 | 1.00 | 1.01 |
| `check -p scale_typed` (bodies and outputs) | 0.251 ± 0.012 | 0.253 ± 0.004 | 0.268 ± 0.011 | 1.01 | 1.07 |
| `build --release -p scale_typed` | 0.915 ± 0.013 | 0.916 ± 0.006 | 0.983 ± 0.005 | 1.00 | **1.07** |
| `build --release -p use100`, API edited | 8.690 ± 0.028 | 8.730 ± 0.061 | 8.814 ± 0.019 | 1.00 | 1.01 |
| `build --release -p use100`, consumer edited | 2.238 ± 0.018 | 2.277 ± 0.041 | 2.253 ± 0.008 | 1.02 | 1.01 |

Release artifacts (`compile/sizes.tsv`): the `use100` binary is 2,886,496 bytes (S2), 2,886,712
(SX), 2,894,056 (S3, +0.26%), with 100 out-of-line `prepare` functions in each;
`libscale_typed.rlib` grows 4.1% in S3 (4,725,348 → 4,920,980 bytes).

Reading: the crate split alone (SX) costs 0–3%, except the debug build of the 1,000-endpoint crate
(+8%, with a wide spread: ±0.15 s). The final candidate adds the metadata-aware decoders and the
`Accept` statics: +7% for the typed suite (every endpoint has a body or an output) and +19% for the
debug build of 1,000 endpoints (+0.37 s); `check` and release builds of the large APIs stay within
3%.

## 4. Rebuilding only the Concord crates

hyperfine, 3 runs: `cargo clean` of the Concord packages only (`concord`, plus `concord-core`,
`concord-extras`, `concord-macros` where they exist), then `cargo build --release -p scale1000`
(the Concord crates and the 1,000-endpoint crate that depends on them; third-party dependencies
stay built):

| S2 (one crate) | SX (four crates) | S3 (final) |
|---:|---:|---:|
| 7.245 ± 0.075 s | 7.140 ± 0.018 s | 7.325 ± 0.042 s |

The split does not slow a rebuild (−1.4%, within two standard deviations); the final candidate is
+1.1% over S2.

## 5. R5: native-key occurrence owners

Generated native-key APIs (`--keys native`, words syntax) of 100, 1,000 and 4,000 endpoints, built
against `3a836e7` (before: a scan of every owner per key occurrence) and `a4b465a` (after: the
owner recorded when the contribution is recorded). `-Ztime-passes` on the pinned compiler with
`RUSTC_BOOTSTRAP=1` (measurement only); the first of three runs is several times slower in total
(0.78 s against 0.23 s at 100 endpoints, before and after alike) and is left out. Seconds:

| Endpoints | macro expansion before → after | `check` before → after (hyperfine) | release build before → after |
|---:|---:|---:|---:|
| 100 | 0.10 → 0.10 | 0.234 ± 0.003 → 0.232 ± 0.002 | 0.829 ± 0.009 → 0.823 ± 0.015 |
| 1,000 | 1.00 → 0.98 | 2.005 ± 0.024 → 1.965 ± 0.033 | 7.746 ± 0.016 → 7.739 ± 0.032 |
| 4,000 | 4.17 → 4.03 (−3%) | 8.352 ± 0.040 → 8.243 ± 0.024 (−1.3%) | 31.53 ± 0.09 → 31.49 ± 0.04 |

The index replaces a scan of every owner per key occurrence (MC07): on these APIs it saves 3% of
expansion at 4,000 endpoints and nothing measurable below, and it is not a regression at any
size.

## Not measured

- The compile-time suite with native keys for S3 (`s3native`): NOT_RUN, to keep the run within the
  free disk (the R5 rows measure native keys before and after the indexing).
- Windows and macOS: NOT_RUN (no push).
- Fine-grained timing on CI runners: not attempted (shared runners are not a reliable baseline).
