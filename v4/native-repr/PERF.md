# Performance record — native headers and formats

Single machine: Intel i5-13600K (20 threads), Linux 7.0.0-42, CPU governor `powersave`, rustc
1.97.1, no `sccache`. The desktop was in use (load average 1.2–2.6 from it alone); measurements
ran only while no build or agent was active. Differences under ~5% for sub-microsecond IDs are
treated as noise. Raw data: `evidence/perf/` (Criterion estimates and samples of every run in
`evidence/perf/criterion/raw-a915c8d-cand-aaa5a3c-run3.tar.xz`, hyperfine JSON in `compile/`, allocation
tables, the attribution runs). A benchmark is not a correctness test: the suites' own tests
(`cargo test -p concord-bench`) only check that each Concord request equals its hand-written
counterpart.

| Identity | Meaning |
|---|---|
| `a915c8d` | baseline (code identical to `b0fe26e`); measured from branch `native-repr-bench` (`78118ac`: the Criterion suite added, `crates/` unchanged) |
| `aaa5a3c` | candidate: native headers, formats, the placement-note fix, the native/literal header benchmark variants |
| `6e60ba2` | compile-suite candidate snapshot (`crates/` identical to `aaa5a3c` apart from the packaging `include` list) |

## How to reproduce

```text
cargo bench -p concord-bench --bench runtime -- --save-baseline <name>          # ~25 min
cargo bench -p concord-bench --bench runtime -- --baseline-lenient <name>       # compare
python3 -I benches/criterion/summarize.py --current <run> --baseline <name>
cargo run --release -p concord-bench --bin alloc-count -- --json allocs.json
python3 -I benches/scale/gen.py suite OUT --concord <snapshot>/crates/concord --syntax via|formats
RUNS=3 benches/scale/ab.sh RESULTS OUT_A OUT_B && python3 -I benches/scale/ab_summary.py RESULTS
```

## 1. Runtime (Criterion, request construction from reused handles)

Baseline: one full run (171 IDs, `--save-baseline a915c8d`). Candidate: two full runs (180 IDs;
run 2 `--save-baseline cand-aaa5a3c`, run 3 `--baseline-lenient a915c8d`). Means; Criterion's 95%
intervals of the mean were within ±1% for every row below except where noted
(`evidence/perf/criterion/summary-run{2,3}.md`).

| ID (`…/prepare`) | a915c8d | candidate run 2 | run 3 | hand-written |
|---|---|---|---|---|
| `construct/minimal/concord` | 138.5 ns | 140.0 ns | 145.3 ns | 102.3 ns |
| `construct/realistic_all_present/concord` | 1.550 µs | 1.401 µs (−9.6%) | 1.414 µs (−8.8%) | 782 ns |
| `construct/realistic_optional_absent/concord` | 1.002 µs | 853 ns (−14.8%) | 901 ns (−10.1%) | 541 ns |
| `construct/s2_product_scoped/concord` | 786 ns | 717 ns (−8.8%) | 731 ns (−7.0%) | 528 ns |
| `compose/inherit/concord` | 481 ns | 412 ns (−14.4%) | 407 ns (−15.5%) | — |
| `compose/deep8_reused_scope/concord` | 1.492 µs | 1.345 µs (−9.9%) | 1.347 µs (−9.7%) | — |
| `headers/16/concord_text` | 1.076 µs | 1.121 µs (wide interval) | 879 ns (−18.3%) | `manual_from_str` 1.063 µs |
| `headers/16/concord_numeric` | 1.209 µs | 965 ns (−20.2%) | 960 ns (−20.6%) | `manual_numeric` 1.071 µs |
| `headers/16/concord_native` (new) | — | 713 ns | 702 ns | `manual_prebuilt_clone` 877 ns |
| `headers/16/concord_literal` (new) | — | 680 ns | 679 ns | `manual_from_static` 845 ns |
| `headers/4/concord_native` / `concord_literal` | — | 236 / 220 ns | 237 / 224 ns | 243 / 231 ns |
| `headers/1/concord_native` / `concord_literal` | — | 146 / 145 ns | 151 / 146 ns | 111 / 108 ns |
| `body_encode/bytes/100B/concord` | 184 ns | 209 ns (+13.3%) | 206 ns (+11.8%) | — |
| `body_encode/json/100B/concord` | 481 ns | 484 ns | 499 ns (+3.7%) | — |
| `body_encode/json/1MB/concord` | 1.127 ms | 1.154 ms | 1.039 ms | — |
| `decode/json/10KB/concord` (`decode`) | 21.27 µs | 21.23 µs | 21.34 µs | — |
| `roundtrip/small/concord/send` | 8.25 µs | 8.22 µs | 8.51 µs | `manual` 8.07 µs |

Native and literal header values reuse their payload: per value no formatting, validation or
payload allocation, and with 16 values Concord is below the hand-written `RequestBuilder::header`
loop (the generated code appends to a `HeaderMap` with pre-validated names; the builder converts
each name again). Formatted text and integers got faster because they no longer go through
`Display` and a second copy.

**Measured cost (not hidden).** Small `bytes`/JSON bodies: a paired re-run in one session
(`evidence/perf/attribution/`, alternating baseline and candidate twice) gave `bytes/100B`
182 → 200 ns and `json/100B` 455 → 479 ns; `json/10KB` 12.24 → 11.44 µs (−6.5%), `text/100B`
200 → 196 ns. Attribution experiments on the candidate: calling the encoder directly instead of
through the per-endpoint static's function pointer — no change (199 vs 200 ns); restoring the old
per-request parse of the content type — 188–192 ns, so about 10 ns comes from cloning the
compile-time `HeaderValue` static rather than parsing a fresh value. The remaining ~8 ns is
unattributed: `perf` is unavailable (`perf_event_paranoid = 4`, a system setting this campaign
does not change). What it buys: a format's content type is validated at compile time instead of
at every request, and one allocation less per body. Its size: 10–24 ns per small body against an
8 µs loopback round trip.

## 2. Allocations per `prepare` (exact, counting allocator; `evidence/perf/criterion/allocs-*`)

| Scenario | a915c8d | candidate | hand-written |
|---|---|---|---|
| `headers/16` text / numeric | 43 / 43 | 27 / 43 | 23 / 39 |
| `headers/16` native / literal | — | 11 / 11 | `prebuilt_clone` 7 / `from_static` 7 |
| `headers/1` native / literal | — | 7 / 7 | 3 / 3 |
| `compose/inherit` | 22 | 16 | 5 |
| `compose/deep8_reused_scope` | 50 | 42 | 8 |
| `construct/realistic_all_present` | 45 | 36 | 14 |
| `body_encode/*` | 9–25 | one fewer each | — |

Payload reuse versus map cost (Track H, `evidence/track_h/alloc/prototype.txt`, 4 header values,
reused client): native `from_static` or shared values and literal values allocate nothing for their
payloads; the `HeaderMap` for 4 names costs 2 allocations in every variant; a `String` value costs 1
copy, an integer 2 (`http`'s own conversion), a formatted template 2 (was 3). Concord's remaining
fixed overhead versus hand-written code is the URL, the `Parts` vectors and the request assembly,
unchanged by this campaign.

## 3. Compile time (hyperfine, paired, RUNS=3, `CARGO_BUILD_JOBS=6`)

Run `ab-20261102T063238` (`evidence/perf/compile/`): suite `a915c8d` generated with
`--syntax via` against the baseline snapshot, suite `cand` with `--syntax formats` against the
`6e60ba2` snapshot; identical generator (version 5) and `Cargo.lock`
(`2876f5ef…`); rows rotate the suite order. Seconds, mean ± σ:

| Row | a915c8d | candidate | ratio |
|---|---|---|---|
| check `scale1000` | 1.739 ± 0.019 | 1.753 ± 0.022 | 1.01 |
| check `scale_shared` (56 inherited contributions, 16 literal headers) | 0.725 ± 0.032 | 0.715 ± 0.006 | 0.99 |
| check `scale_deep` | 0.265 ± 0.006 | 0.271 ± 0.007 | 1.02 |
| check `scale_typed` (200 typed endpoints, 160 inheriting a default, 40 in place) | 0.235 ± 0.004 | 0.245 ± 0.007 | 1.04 |
| check `scale_fanout` (one value, 4,000 distinct placements) | 0.215 ± 0.001 | 0.182 ± 0.004 | 0.85 |
| dev `scale1000` / edit API + `use100` | 1.912 / 2.043 | 1.922 / 2.055 | 1.01 / 1.01 |
| release `scale1000` / `scale_shared` / `scale_typed` | 6.547 / 3.330 / 0.906 | 6.550 / 3.395 / 0.923 | 1.00 / 1.02 / 1.02 |
| release, edit API + `use5` / `use100` / `use1000` / `fan_app` | 6.90 / 9.10 / 50.74 / 9.07 | 6.93 / 9.05 / 50.89 / 9.06 | 1.00 / 0.99 / 1.00 / 1.00 |
| release, edit consumer `use5` / `use100` / `use1000` / `fan_app` | 0.39 / 2.55 / 44.21 / 2.61 | 0.41 / 2.54 / 44.21 / 2.56 | 1.03 / 1.00 / 1.00 / 0.98 |

Release binaries and rlibs: within 0.5% (`sizes.tsv`). The declaring-crate and downstream costs
are unchanged within noise; typed bodies with defaults cost +2–4% (longer generated rustdoc: format,
origin, media type). The placement-note fix (`01774a9`): −15% on the 4,000-placement fanout; a
preliminary, busier run at 16,000 placements (`evidence/perf/compile/hf-16000.json`, 5 runs) gave 0.995 → 0.516 s
(the old scan's quadratic term), at 4,000 0.201 → 0.172 s. The fix does not explain any other
change (no other row moved).

`#[inline]` (D17) is unchanged; no new paired measurement was needed.

## Not measured

HTTP/2 and TLS (including the HPACK effect of the sensitivity flag, reasoned from h2's source by
Track H); Windows/macOS; rust-analyzer; the inline trade-off under LTO; allocation counts in
round trips; profiles (no `perf`).
