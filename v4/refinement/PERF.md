# Refinement performance record (M5)

Single machine: Intel i5-13600K (20 threads), 30 GiB, Linux 7.0.0-42, rustc 1.97.1, no
`sccache`. **The desktop was in use during all runs** (a game at ~1.2 cores, load average 2.5–4.5);
builds used `CARGO_BUILD_JOBS=6`. Every comparison is paired in one session, the suite order rotated
row by row, each variant in its own target directory; differences under ~5% are treated as noise.
Raw hyperfine JSON (every sample), sizes and load snapshots: `evidence/perf/<run>/`.

## How to reproduce

```text
python3 -I benches/scale/gen.py suite OUT --concord <snapshot>/crates/concord [--syntax alpha]
benches/scale/ab.sh RESULTS OUT_A OUT_B ...      # RUNS, WARM, ROWS, CARGO_BUILD_JOBS
python3 -I benches/scale/ab_summary.py RESULTS suite-<baseline>
cargo run --release -p concord-perf             # runtime, in each snapshot
```

Workloads (`gen.py`): `scale1000` (1,000 endpoints in 100 scopes; each endpoint one path value, two
optional and one initialized query value; each scope a path value and an optional header; one
sensitive client header), `scale_deep` (32 nested scopes, 2 endpoints per level), `scale_shared`
(200 endpoints, 56 inherited query/header contributions each), consumers `use5`/`use100`/`use1000`
(binaries calling that many of the 1,000 endpoints: `prepare` + a boxed `send` future), `fan_app`
(binary over two libraries calling 100 endpoints each, 50 shared). Variants are source snapshots
exported by `review_snapshot.py`: **alpha** `8f7adc2`, **refined** `de6471f`, **noinline** `de6471f`
with `#[inline]` removed from generated `prepare` only (`evidence/perf/noinline.diff`), **final**
`d1844be`. The alpha suite is generated with `--syntax alpha` (only the root constructor and
optional `override` spelling differ; no workload has typed outputs).

## 1. `#[inline]` placement (D17): KEEP blanket `#[inline]`

Run `ab-20261102T015701` (RUNS=3), seconds, mean:

| Row | alpha | refined | noinline (no `#[inline]` on `prepare`) |
|---|---|---|---|
| release, edit API, rebuild `scale1000` | 5.74 | 7.28 | 13.69 |
| release, edit API, rebuild + `use5` | 6.17 | 7.61 | 14.08 |
| release, edit API, rebuild + `use100` | 8.54 | 9.73 | 15.44 |
| release, edit API, rebuild + `use1000` | 53.00 | 51.49 | 50.53 |
| release, edit API, rebuild + `fan_app` | 8.38 | 9.62 | 13.77 |
| release, edit consumer `use100` only | 2.74 | 2.50 | 1.86 |
| release, edit consumer `use1000` only | 46.77 | 44.27 | 36.85 |
| release, edit `fan_a` (fanout) | 2.82 | 2.54 | 1.90 |
| release `scale_shared` / `scale_deep` | 2.73 / 0.78 | 4.36 / 1.23 | 9.52 / 2.81 |

Without `#[inline]`, every `prepare` is code-generated in the declaring crate whether used or not.
Compared with `refined` (the same code with `#[inline]`): release builds of the declaring crate and
release rebuilds after an API edit cost 1.43–2.29× more (`use1000`: 0.98×); consumer-only release
rebuilds get 12–25% cheaper (`use5` −12%, `use1000` −17%, `use100` and the fanout −25%); `check`
and dev builds are within ~1–6%. *Correction (final no-change check):* an earlier version of this
paragraph quoted "1.4–3.6× / 25–33%", partly computed against the alpha column, which does not
isolate the attribute. (Consumer savings come from code no longer generated per consumer — `fan_app` carries 200 out-of-line `prepare` copies for 150
distinct endpoints with `#[inline]`, 150 without). The trade-off favours the blanket attribute for
APIs larger than what any one consumer uses; a consumer that calls nearly every endpoint of a huge
API (`use1000`) is the case it does not help. Revisit trigger: a real downstream dominated by
consumer-only rebuilds of most of a very large API.

## 2. Refined vs alpha: compile-time regression found, attributed, reduced

The first paired run showed `cargo check` of the declaring crate **+41%** (`scale1000`) and
**+117%** (`scale_shared`). Attribution (runs `attr-*`, `attr2-*`; nightly `-Ztime-passes`):

| Variant of `de6471f` | check `scale_shared` | release `scale_shared` |
|---|---|---|
| as is | 1.096 | 4.412 |
| without the per-contribution `const _` guard | 0.849 | 3.786 |
| without guard and without `const { HeaderName::from_static }` | 0.778 | 3.172 |
| … and infallible query formatting | 0.754 | 2.814 |
| `#[derive(Clone)]` instead of the D08 conditional impl | 1.076 | 4.420 |
| alpha | 0.508 | 2.676 |

`-Ztime-passes` (`scale1000`, alpha vs no-const variant): the difference was in
`macro_expand_crate` (0.56 → 0.86 s); type checking and borrow checking were unchanged. Two causes
in the macro: the L1 fix re-ran the native `HeaderName` validator in `const` for every inherited
header of every endpoint, and the generated-doc notes scanned every owner's contributions once per
owner (quadratic). Fixed in `79a0faf`: one compile-time-evaluated `static` per distinct header name
per client (still a compile error when invalid, even if unused), notes computed in one pass.

Final paired run `final-20261102T024522` (alpha vs `d1844be`, RUNS=3), seconds:

| Row | alpha | final | ratio |
|---|---|---|---|
| check `scale1000` | 1.453 | 1.848 | 1.27 |
| check `scale_shared` | 0.513 | 0.731 | 1.42 |
| check `scale_deep` | 0.220 | 0.288 | 1.31 |
| dev build `scale1000` | 1.529 | 2.012 | 1.32 |
| dev, edit API + `use100` | 1.733 | 2.168 | 1.25 |
| release `scale1000` | 5.745 | 6.755 | 1.18 |
| release `scale_shared` / `scale_deep` | 2.814 / 0.844 | 3.571 / 1.089 | 1.27 / 1.29 |
| release, edit API + `use5` / `use100` / `fan_app` | 6.20 / 8.58 / 8.74 | 7.12 / 9.53 / 9.64 | 1.15 / 1.11 / 1.10 |
| release, edit API + `use1000` | 54.82 | 54.79 | 1.00 |
| release, edit consumer `use5` / `use100` / `use1000` / `fan_app` | 0.43 / 2.85 / 48.93 / 2.85 | 0.46 / 2.76 / 47.98 / 2.68 | 1.06 / 0.97 / 0.98 / 0.94 |

Release binaries: `use1000` 6.53 → 6.42 MB, `use100` 2.88 → 2.88 MB; `libscale1000.rlib` 37.1 →
36.4 MB. **Accepted regression:** checking/building the *declaring* crate costs ~0.4 ms more per
endpoint in `check` (+27% on the 1,000-endpoint API, +42% on the shared-contribution shape) and
+18–29% in release; consumers are unaffected. The remaining cost tracks expansion size (5.44 →
7.44 MB for `scale1000`; macro expansion time per MB is unchanged, ~0.1 s/MB): about a third is
generated documentation (doc attributes 0.38 → 1.03 MB — value placements, initializers,
sensitivity, request summaries), the rest is per-endpoint code for setters of required values, the
conditional `Clone` impl, fallible formatting with value-free error labels, and `Debug` as
statements. What it buys is listed in `DECISIONS.md` (D01, D08, D12, D18).

## 3. Runtime (request construction and loopback execution)

`cargo run --release -p concord-perf` (median of 9 runs; 200,000 iterations per run for
construction, 3,000 for execution), two alternating rounds per variant
(`evidence/perf/runtime-20261102T030914/runtime.txt`):

| Case | alpha | final | hand-written |
|---|---|---|---|
| S3 search, 9 query values set, `prepare()` | 730 / 736 ns | 841 / 839 ns | 614–637 ns |
| S2 product, 2 scopes + subdomain + 3 headers, `prepare()` | 790 / 767 ns | 803 / 818 ns | 513–522 ns |
| S1 health over loopback (keep-alive), JSON | 9.14 / 9.12 µs | 8.31 / 9.12 µs | 8.19–9.05 µs |

S3's +105 ns is attributed: a variant of `d1844be` whose value formatting uses `to_string()` again
(panicking on a broken `Display`) measured 740/740 ns vs 841/840 ns in the same session — std's
`to_string` has type-specialized fast paths that `write!` into a fresh `String` does not. Accepted:
~100 ns for nine values (≈1% of a loopback request) for a value-free build error instead of a
panic or a silently truncated segment. Execution cost is unchanged.

## Not measured

Allocation counts per request; HTTP/2 and TLS; Windows/macOS; rust-analyzer responsiveness; the
inline trade-off under LTO or `codegen-units = 1`; consumers of very large APIs in several crates
beyond the two-crate fanout. Old `v4/evidence/PERF.md` figures belong to the alpha and are
historical.
