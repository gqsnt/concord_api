# Track P — Criterion runtime benchmark suite

Branch `native-repr-bench` (worktree `wt/bench`, from `a915c8d`). Nothing pushed.

## Model

Claude Opus 5.5 (`claude-opus-5-5`). The reasoning-effort setting can't be seen from inside the session.

## Result

- New crate `benches/criterion` (`concord-bench`) has a Criterion 0.8.2 suite (`benches/runtime.rs`, `harness = false`) with **171 benchmark IDs** in 6 groups.
- A deterministic allocation counter `alloc-count` (`src/bin/alloc_count.rs`, same scenarios and IDs).
- `README.md` and `summarize.py`.
- The `Instant` harness `benches/perf` is removed. Its keep-alive loopback server moved to `src/server.rs`, and it now routes `/json` to a 10 KB body.
- `benches/scale` (hyperfine) is unchanged.
- Ordinary CI only adds `cargo bench --locked -p concord-bench --no-run` (Linux). A new `.github/workflows/bench.yml` runs on `workflow_dispatch` only and labels its output "not a baseline".
- `scripts/verify.sh --msrv` passes.
- No baseline was recorded. Only a short smoke run was done (see below).

## Design

- **Scenarios defined once** (`src/lib.rs`). Each `Case` has an `input` closure (per-call input construction) and a `run` closure that consumes the input (prepare / build / decode). A small `Runner` trait drives the cases:
  - the Criterion adapter (`benches/runtime.rs`) turns each case into separate IDs;
  - the allocation counter counts the same cases.
  
  So the time and allocation tables always describe identical code.
- **Concord side**: `examples/shapes` S1–S3, plus bench-only declarations in `src/apis.rs`: tracker, headers, deep chain, header operations, bodies. Scenario code calls only ordinary generated methods: navigation, positional args, `set_*`, `prepare`, `send`, and `Decoder::decode` via `into_parts`. Migrating the declarations is therefore mechanical and keeps the IDs. One generated type name is spelled out: `TrackerOrgProjectIssues` in `tracker_full`.
- **Hand-written side**: `benches/baselines` for S1–S3, plus `src/manual.rs`. Client-level header values are validated once into `HeaderValue`s, as a careful author would do. `cargo test -p concord-bench` (8 tests) checks that every Concord request equals its hand-written counterpart: method, URL, header multiset and body. The tests also check:
  - in-memory decoding;
  - the loopback payload;
  - that every case runs;
  - that IDs are unique, with exactly 78 cases.
- **Stages and batching**:
  - `input`: `iter_with_large_drop`. Skipped (`trivial_input`) when the hand-written input is `()`, a constant or a reference; Criterion panics on ZST batches.
  - `prepare` / `decode`: `iter_batched`, with `SmallInput`, or `LargeInput` for 1 MB bodies so memory and cache pollution stay bounded.
  - `input_prepare`: in `construct` and the deep chain only.
  - Drops are never timed. `black_box` is applied to inputs.
  - `Throughput::Elements(n)` for headers, `Throughput::Bytes` for bodies, decoding and round trips.
- **Decoding without network**: a `reqwest::Response::from(http::Response<Bytes>)` is polled once with `Waker::noop()`. No runtime is needed, and the poll panics if the future is not ready.
- **Round trips**: a current-thread Tokio runtime runs both client and server, as the old harness did. Runtime, server, client (`no_proxy`) and the pooled connection are set up before measuring. Stages:
  - `send`: everything from the handle;
  - `send_prepared`: a request prepared in setup, then execute + decode;
  - `raw_reqwest/get_bytes`: the transport floor.
- **Headers group**: one function per variant (`headers_concord_text`, `headers_concord_numeric`, `headers_manual_{from_str,numeric,prebuilt,from_static}`). A native-`HeaderValue` Concord variant needs one more function plus `native_{1,4,16}` endpoints.
- **MSRV**: criterion 0.8.2 declares `rust-version = 1.86`, so `concord-bench` sets `rust-version = "1.86"`. The `--msrv` steps select packages with `-p` and never build it. `cargo +1.85` still parses the lockfile, and the msrv steps passed.
- **Criterion features**: `default-features = false`, features `cargo_bench_support` and `async_tokio`. No plotters, rayon or HTML. Cargo.lock gained criterion's dependencies only; no existing versions changed. Everything resolved offline.

## Benchmark IDs (171)

Format: `<group>/<scenario>/<implementation>/<stage>`. Generated with `cargo bench -p concord-bench --bench runtime -- --list`.

```text
construct/minimal/concord/input
construct/minimal/concord/prepare
construct/minimal/concord/input_prepare
construct/minimal/manual/prepare
construct/minimal/manual/input_prepare
construct/s3_search_9_query/concord/input
construct/s3_search_9_query/concord/prepare
construct/s3_search_9_query/concord/input_prepare
construct/s3_search_9_query/manual/input
construct/s3_search_9_query/manual/prepare
construct/s3_search_9_query/manual/input_prepare
construct/s2_product_scoped/concord/input
construct/s2_product_scoped/concord/prepare
construct/s2_product_scoped/concord/input_prepare
construct/s2_product_scoped/manual/prepare
construct/s2_product_scoped/manual/input_prepare
construct/realistic_all_present/concord/input
construct/realistic_all_present/concord/prepare
construct/realistic_all_present/concord/input_prepare
construct/realistic_all_present/manual/input
construct/realistic_all_present/manual/prepare
construct/realistic_all_present/manual/input_prepare
construct/realistic_optional_absent/concord/input
construct/realistic_optional_absent/concord/prepare
construct/realistic_optional_absent/concord/input_prepare
construct/realistic_optional_absent/manual/input
construct/realistic_optional_absent/manual/prepare
construct/realistic_optional_absent/manual/input_prepare
headers/1/concord_text/input
headers/1/concord_text/prepare
headers/1/concord_numeric/input
headers/1/concord_numeric/prepare
headers/1/manual_from_str/input
headers/1/manual_from_str/prepare
headers/1/manual_numeric/input
headers/1/manual_numeric/prepare
headers/1/manual_prebuilt_clone/prepare
headers/1/manual_from_static/prepare
headers/4/concord_text/input
headers/4/concord_text/prepare
headers/4/concord_numeric/input
headers/4/concord_numeric/prepare
headers/4/manual_from_str/input
headers/4/manual_from_str/prepare
headers/4/manual_numeric/input
headers/4/manual_numeric/prepare
headers/4/manual_prebuilt_clone/prepare
headers/4/manual_from_static/prepare
headers/16/concord_text/input
headers/16/concord_text/prepare
headers/16/concord_numeric/input
headers/16/concord_numeric/prepare
headers/16/manual_from_str/input
headers/16/manual_from_str/prepare
headers/16/manual_numeric/input
headers/16/manual_numeric/prepare
headers/16/manual_prebuilt_clone/prepare
headers/16/manual_from_static/prepare
compose/deep8_navigate/concord/input
compose/deep8_navigate/concord/prepare
compose/deep8_navigate/concord/input_prepare
compose/deep8_navigate/manual/input
compose/deep8_navigate/manual/prepare
compose/deep8_navigate/manual/input_prepare
compose/deep8_reused_scope/concord/input
compose/deep8_reused_scope/concord/prepare
compose/deep8_reused_scope/concord/input_prepare
compose/deep8_reused_scope/manual/prepare
compose/deep8_reused_scope/manual/input_prepare
compose/inherit/concord/input
compose/inherit/concord/prepare
compose/override_present/concord/input
compose/override_present/concord/prepare
compose/override_absent/concord/input
compose/override_absent/concord/prepare
compose/append/concord/input
compose/append/concord/prepare
compose/remove/concord/input
compose/remove/concord/prepare
compose/inherit/manual/input
compose/inherit/manual/prepare
compose/override_present/manual/input
compose/override_present/manual/prepare
compose/override_absent/manual/input
compose/override_absent/manual/prepare
compose/append/manual/input
compose/append/manual/prepare
compose/remove/manual/input
compose/remove/manual/prepare
body_encode/json/100B/concord/input
body_encode/json/100B/concord/prepare
body_encode/json/100B/manual/input
body_encode/json/100B/manual/prepare
body_encode/text/100B/concord/input
body_encode/text/100B/concord/prepare
body_encode/text/100B/manual/input
body_encode/text/100B/manual/prepare
body_encode/bytes/100B/concord/input
body_encode/bytes/100B/concord/prepare
body_encode/bytes/100B/manual/input
body_encode/bytes/100B/manual/prepare
body_encode/json/10KB/concord/input
body_encode/json/10KB/concord/prepare
body_encode/json/10KB/manual/input
body_encode/json/10KB/manual/prepare
body_encode/text/10KB/concord/input
body_encode/text/10KB/concord/prepare
body_encode/text/10KB/manual/input
body_encode/text/10KB/manual/prepare
body_encode/bytes/10KB/concord/input
body_encode/bytes/10KB/concord/prepare
body_encode/bytes/10KB/manual/input
body_encode/bytes/10KB/manual/prepare
body_encode/json/1MB/concord/input
body_encode/json/1MB/concord/prepare
body_encode/json/1MB/manual/input
body_encode/json/1MB/manual/prepare
body_encode/text/1MB/concord/input
body_encode/text/1MB/concord/prepare
body_encode/text/1MB/manual/input
body_encode/text/1MB/manual/prepare
body_encode/bytes/1MB/concord/input
body_encode/bytes/1MB/concord/prepare
body_encode/bytes/1MB/manual/input
body_encode/bytes/1MB/manual/prepare
decode/json/100B/concord/input
decode/json/100B/concord/decode
decode/json/100B/manual/input
decode/json/100B/manual/decode
decode/text/100B/concord/input
decode/text/100B/concord/decode
decode/text/100B/manual/input
decode/text/100B/manual/decode
decode/bytes/100B/concord/input
decode/bytes/100B/concord/decode
decode/bytes/100B/manual/input
decode/bytes/100B/manual/decode
decode/json/10KB/concord/input
decode/json/10KB/concord/decode
decode/json/10KB/manual/input
decode/json/10KB/manual/decode
decode/text/10KB/concord/input
decode/text/10KB/concord/decode
decode/text/10KB/manual/input
decode/text/10KB/manual/decode
decode/bytes/10KB/concord/input
decode/bytes/10KB/concord/decode
decode/bytes/10KB/manual/input
decode/bytes/10KB/manual/decode
decode/json/1MB/concord/input
decode/json/1MB/concord/decode
decode/json/1MB/manual/input
decode/json/1MB/manual/decode
decode/text/1MB/concord/input
decode/text/1MB/concord/decode
decode/text/1MB/manual/input
decode/text/1MB/manual/decode
decode/bytes/1MB/concord/input
decode/bytes/1MB/concord/decode
decode/bytes/1MB/manual/input
decode/bytes/1MB/manual/decode
roundtrip/small/concord/send
roundtrip/small/concord/send_prepared
roundtrip/small/manual/send
roundtrip/small/manual/send_prepared
roundtrip/small/raw_reqwest/get_bytes
roundtrip/10KB/concord/send
roundtrip/10KB/concord/send_prepared
roundtrip/10KB/manual/send
roundtrip/10KB/manual/send_prepared
roundtrip/10KB/raw_reqwest/get_bytes
```

## How to run, compare and archive

These steps are in `benches/criterion/README.md`.

```sh
cargo bench -p concord-bench --bench runtime -- --save-baseline v4-pre    # revision A, quiet machine
cargo bench -p concord-bench --bench runtime -- --baseline v4-pre         # revision B: compare
python3 -I benches/criterion/summarize.py --baseline v4-pre --criterion-change
cargo run --release -p concord-bench --bin alloc-count [-- FILTER] [--json allocs.json]
```

- Raw data is written to `target/criterion/<group>/<id_with_underscores>/{new,base,<name>,change}/` (`benchmark.json`, `estimates.json`, `sample.json`, `tukey.json`).
- The README's archive recipe copies `target/criterion`, `summary.md`, `allocs.{md,json}` and `environment.txt` (commit, `rustc -vV`, CPU, load, governor) into `evidence/perf/criterion-<UTC>/` and makes a tarball.
- A full run is about 171 × ~9 s ≈ 25–30 min with Criterion defaults.
- `summarize.py` was tested against a saved `smoke` baseline, both `--baseline` and `--current`.

## Commits

- `9f281fd` bench: replace the Instant harness with a Criterion suite and an allocation counter
- `78118ac` ci: compile benchmarks only; add a manual Criterion workflow

## `scripts/verify.sh --msrv` (at `78118ac`)

```text
head: 78118ac73aeaec8ebf87ba078dcdba07e3776d50  dirty: 0 file(s)
rustc: rustc 1.97.1 (8bab26f4f 2026-07-14)
PASS  fmt
PASS  clippy
PASS  test
PASS  test-no-default
PASS  test-json-only
PASS  test-reqwest-defaults-only
PASS  doc
PASS  doc-no-default
PASS  doc-json-only
PASS  msrv-concord
PASS  msrv-examples
      verify-test-json-only: 365 passed, 0 failed
      verify-test: 451 passed, 0 failed
      verify-test-no-default: 349 passed, 0 failed
      verify-test-reqwest-defaults-only: 351 passed, 0 failed
      verify-msrv-concord: 368 passed, 0 failed
      verify-msrv-examples: 69 passed, 0 failed
```

`cargo bench --locked -p concord-bench --no-run` and the smoke runs built with `--locked` and `--offline`.

## Allocation counts (deterministic; per call, after one warm-up call)

These figures **are** results. They are exact and identical across two runs (`cmp` equal), and machine load does not affect them. The release build is at `78118ac`.

- `input` columns: building the per-call input.
- `stage` columns: `prepare` (build) or `decode`.
- Calls include `alloc`, `alloc_zeroed` and `realloc`. Bytes are bytes requested; frees are not subtracted.

| case | input allocs | input bytes | stage | stage allocs | stage bytes |
|---|---:|---:|---|---:|---:|
| `construct/minimal/concord` | 0 | 0 | prepare | 7 | 207 |
| `construct/minimal/manual` | 0 | 0 | prepare | 2 | 78 |
| `construct/s3_search_9_query/concord` | 6 | 62 | prepare | 29 | 2429 |
| `construct/s3_search_9_query/manual` | 6 | 110 | prepare | 24 | 2254 |
| `construct/s2_product_scoped/concord` | 4 | 87 | prepare | 29 | 1482 |
| `construct/s2_product_scoped/manual` | 0 | 0 | prepare | 12 | 1553 |
| `construct/realistic_all_present/concord` | 11 | 178 | prepare | 45 | 2841 |
| `construct/realistic_all_present/manual` | 12 | 96 | prepare | 14 | 1098 |
| `construct/realistic_optional_absent/concord` | 4 | 106 | prepare | 34 | 1607 |
| `construct/realistic_optional_absent/manual` | 5 | 24 | prepare | 12 | 879 |
| `headers/1/concord_text` | 1 | 16 | prepare | 9 | 865 |
| `headers/1/concord_numeric` | 0 | 0 | prepare | 9 | 853 |
| `headers/1/manual_from_str` | 1 | 16 | prepare | 4 | 697 |
| `headers/1/manual_numeric` | 0 | 0 | prepare | 5 | 725 |
| `headers/1/manual_prebuilt_clone` | 0 | 0 | prepare | 3 | 681 |
| `headers/1/manual_from_static` | 0 | 0 | prepare | 3 | 681 |
| `headers/4/concord_text` | 4 | 64 | prepare | 15 | 961 |
| `headers/4/concord_numeric` | 0 | 0 | prepare | 15 | 913 |
| `headers/4/manual_from_str` | 4 | 64 | prepare | 7 | 745 |
| `headers/4/manual_numeric` | 0 | 0 | prepare | 11 | 857 |
| `headers/4/manual_prebuilt_clone` | 0 | 0 | prepare | 3 | 681 |
| `headers/4/manual_from_static` | 0 | 0 | prepare | 3 | 681 |
| `headers/16/concord_text` | 16 | 256 | prepare | 43 | 5281 |
| `headers/16/concord_numeric` | 0 | 0 | prepare | 43 | 5089 |
| `headers/16/manual_from_str` | 16 | 256 | prepare | 23 | 4873 |
| `headers/16/manual_numeric` | 0 | 0 | prepare | 39 | 5321 |
| `headers/16/manual_prebuilt_clone` | 0 | 0 | prepare | 7 | 4617 |
| `headers/16/manual_from_static` | 0 | 0 | prepare | 7 | 4617 |
| `compose/deep8_navigate/concord` | 16 | 464 | prepare | 50 | 4212 |
| `compose/deep8_navigate/manual` | 8 | 80 | prepare | 16 | 2376 |
| `compose/deep8_reused_scope/concord` | 0 | 0 | prepare | 50 | 4212 |
| `compose/deep8_reused_scope/manual` | 0 | 0 | prepare | 8 | 3173 |
| `compose/inherit/concord` | 0 | 0 | prepare | 22 | 1280 |
| `compose/override_present/concord` | 1 | 10 | prepare | 22 | 1285 |
| `compose/override_absent/concord` | 0 | 0 | prepare | 22 | 1281 |
| `compose/append/concord` | 1 | 10 | prepare | 24 | 1299 |
| `compose/remove/concord` | 0 | 0 | prepare | 20 | 1263 |
| `compose/inherit/manual` | 0 | 0 | prepare | 5 | 1016 |
| `compose/override_present/manual` | 1 | 10 | prepare | 6 | 1026 |
| `compose/override_absent/manual` | 0 | 0 | prepare | 5 | 1016 |
| `compose/append/manual` | 1 | 10 | prepare | 6 | 1026 |
| `compose/remove/manual` | 0 | 0 | prepare | 5 | 1016 |
| `body_encode/json/100B/concord` | 9 | 274 | prepare | 12 | 1269 |
| `body_encode/json/100B/manual` | 9 | 274 | prepare | 7 | 1142 |
| `body_encode/text/100B/concord` | 1 | 100 | prepare | 9 | 870 |
| `body_encode/text/100B/manual` | 1 | 100 | prepare | 4 | 734 |
| `body_encode/bytes/100B/concord` | 0 | 0 | prepare | 10 | 886 |
| `body_encode/bytes/100B/manual` | 0 | 0 | prepare | 4 | 734 |
| `body_encode/json/10KB/concord` | 461 | 15995 | prepare | 18 | 33525 |
| `body_encode/json/10KB/manual` | 461 | 15995 | prepare | 13 | 33398 |
| `body_encode/text/10KB/concord` | 1 | 10240 | prepare | 9 | 870 |
| `body_encode/text/10KB/manual` | 1 | 10240 | prepare | 4 | 734 |
| `body_encode/bytes/10KB/concord` | 0 | 0 | prepare | 10 | 886 |
| `body_encode/bytes/10KB/manual` | 0 | 0 | prepare | 4 | 734 |
| `body_encode/json/1MB/concord` | 43077 | 1539285 | prepare | 25 | 4195061 |
| `body_encode/json/1MB/manual` | 43077 | 1539285 | prepare | 20 | 4194934 |
| `body_encode/text/1MB/concord` | 1 | 1048576 | prepare | 9 | 870 |
| `body_encode/text/1MB/manual` | 1 | 1048576 | prepare | 4 | 734 |
| `body_encode/bytes/1MB/concord` | 0 | 0 | prepare | 10 | 886 |
| `body_encode/bytes/1MB/manual` | 0 | 0 | prepare | 4 | 734 |
| `decode/json/100B/concord` | 9 | 920 | decode | 10 | 738 |
| `decode/json/100B/manual` | 9 | 920 | decode | 10 | 738 |
| `decode/text/100B/concord` | 9 | 929 | decode | 2 | 228 |
| `decode/text/100B/manual` | 9 | 929 | decode | 3 | 253 |
| `decode/bytes/100B/concord` | 9 | 928 | decode | 1 | 128 |
| `decode/bytes/100B/manual` | 9 | 928 | decode | 1 | 128 |
| `decode/json/10KB/concord` | 9 | 920 | decode | 467 | 37555 |
| `decode/json/10KB/manual` | 9 | 920 | decode | 467 | 37555 |
| `decode/text/10KB/concord` | 9 | 929 | decode | 2 | 10368 |
| `decode/text/10KB/manual` | 9 | 929 | decode | 3 | 10393 |
| `decode/bytes/10KB/concord` | 9 | 928 | decode | 1 | 128 |
| `decode/bytes/10KB/manual` | 9 | 928 | decode | 1 | 128 |
| `decode/json/1MB/concord` | 9 | 920 | decode | 43090 | 4426301 |
| `decode/json/1MB/manual` | 9 | 920 | decode | 43090 | 4426301 |
| `decode/text/1MB/concord` | 9 | 929 | decode | 2 | 1048704 |
| `decode/text/1MB/manual` | 9 | 929 | decode | 3 | 1048729 |
| `decode/bytes/1MB/concord` | 9 | 928 | decode | 1 | 128 |
| `decode/bytes/1MB/manual` | 9 | 928 | decode | 1 | 128 |

Per call; `~` marks a mean that is not an exact multiple (non-deterministic).

Observations, for the tracks that change the DSL (not acted on here):
- Concord `prepare` makes roughly 2–4× as many allocations as the hand-written equivalent wherever the hand-written code reuses validated `HeaderValue`s, for example:
  - `compose/inherit`: 22 vs 5;
  - `deep8_reused_scope`: 50 vs 8;
  - `realistic_all_present`: 45 vs 14.
- Each Concord header value costs a `String` (`Display`) plus a `HeaderValue`, and client and scope values are re-formatted on every request. `headers/16/concord_numeric` makes 43 allocations, against 7 for prebuilt clones and 39 for hand-written `HeaderValue::from(u64)`.
- Body encoding adds about 5 allocations / about 130 B over hand-written (content-type and `Parts` buffers), independent of size.
- JSON decoding is identical. Text decoding is 1 allocation cheaper than `Response::text`.

## Smoke run — sanity check only, NOT results

Command:

```text
cargo bench -p concord-bench --bench runtime -- --warm-up-time 1 --measurement-time 2 --sample-size 20 --save-baseline smoke '<subset regex>'
```

- The machine was busy, with other builds running and load average 5.5–16.
- These numbers only show that every stage runs and that magnitudes are plausible.
- Repeat runs moved by up to 2–3×, for example `roundtrip/10KB/*/send_prepared`. Do not compare implementations with these numbers.

Each row shows Criterion's [lower, estimate, upper] of the mean.

```text
construct/minimal/concord/input                            [5.6738 ns  5.8156 ns  5.9272 ns]
construct/minimal/concord/prepare                          [139.15 ns  153.20 ns  177.54 ns]
construct/minimal/concord/input_prepare                    [147.62 ns  151.39 ns  155.93 ns]
construct/minimal/manual/prepare                           [101.77 ns  102.03 ns  102.23 ns]
construct/minimal/manual/input_prepare                     [102.14 ns  102.46 ns  102.82 ns]
construct/realistic_all_present/concord/input              [105.26 ns  105.71 ns  106.27 ns]
construct/realistic_all_present/concord/prepare            [1.5293 µs  1.5391 µs  1.5478 µs]
construct/realistic_all_present/concord/input_prepare      [1.5049 µs  1.5192 µs  1.5367 µs]
construct/realistic_all_present/manual/input               [95.187 ns  95.788 ns  96.161 ns]
construct/realistic_all_present/manual/prepare             [785.62 ns  786.46 ns  787.00 ns]
construct/realistic_all_present/manual/input_prepare       [820.68 ns  821.66 ns  823.17 ns]
construct/realistic_optional_absent/concord/input          [48.068 ns  48.920 ns  49.636 ns]
construct/realistic_optional_absent/concord/prepare        [996.95 ns  998.50 ns  999.76 ns]
construct/realistic_optional_absent/concord/input_prepare  [1.0171 µs  1.0266 µs  1.0338 µs]
construct/realistic_optional_absent/manual/input           [42.113 ns  42.201 ns  42.263 ns]
construct/realistic_optional_absent/manual/prepare         [540.52 ns  545.30 ns  548.88 ns]
construct/realistic_optional_absent/manual/input_prepare   [577.65 ns  579.01 ns  580.26 ns]
headers/4/concord_text/input                               [56.674 ns  57.419 ns  57.973 ns]
headers/4/concord_text/prepare                             [308.64 ns  310.44 ns  311.88 ns]
headers/4/concord_numeric/input                            [7.5419 ns  9.8101 ns  11.229 ns]
headers/4/concord_numeric/prepare                          [352.08 ns  353.53 ns  355.33 ns]
headers/4/manual_from_str/input                            [45.371 ns  45.856 ns  46.389 ns]
headers/4/manual_from_str/prepare                          [287.77 ns  288.96 ns  289.71 ns]
headers/4/manual_numeric/input                             [5.9449 ns  6.5193 ns  6.8109 ns]
headers/4/manual_numeric/prepare                           [580.94 ns  673.36 ns  761.93 ns]
headers/4/manual_prebuilt_clone/prepare                    [248.95 ns  250.64 ns  251.92 ns]
headers/4/manual_from_static/prepare                       [237.24 ns  241.89 ns  247.74 ns]
compose/deep8_navigate/concord/input                       [300.47 ns  301.69 ns  302.75 ns]
compose/deep8_navigate/concord/prepare                     [1.6661 µs  1.7303 µs  1.8228 µs]
compose/deep8_navigate/concord/input_prepare               [1.9321 µs  1.9835 µs  2.0814 µs]
compose/deep8_navigate/manual/input                        [187.05 ns  191.16 ns  195.99 ns]
compose/deep8_navigate/manual/prepare                      [1.7341 µs  1.8157 µs  1.9022 µs]
compose/deep8_navigate/manual/input_prepare                [1.3361 µs  1.5076 µs  1.6510 µs]
compose/deep8_reused_scope/concord/input                   [5.3869 ns  5.5720 ns  5.6935 ns]
compose/deep8_reused_scope/concord/prepare                 [1.4681 µs  1.4697 µs  1.4717 µs]
compose/deep8_reused_scope/concord/input_prepare           [1.4661 µs  1.4716 µs  1.4762 µs]
compose/deep8_reused_scope/manual/prepare                  [538.39 ns  574.14 ns  634.70 ns]
compose/deep8_reused_scope/manual/input_prepare            [476.67 ns  485.46 ns  493.48 ns]
compose/override_present/concord/input                     [13.077 ns  13.942 ns  14.816 ns]
compose/override_present/concord/prepare                   [493.07 ns  496.30 ns  501.89 ns]
compose/override_absent/concord/input                      [6.1002 ns  8.0392 ns  9.5020 ns]
compose/override_absent/concord/prepare                    [492.47 ns  494.99 ns  496.78 ns]
compose/override_present/manual/input                      [10.279 ns  10.320 ns  10.365 ns]
compose/override_present/manual/prepare                    [345.65 ns  346.35 ns  346.85 ns]
compose/override_absent/manual/input                       [5.0648 ns  5.8696 ns  6.2896 ns]
compose/override_absent/manual/prepare                     [341.78 ns  345.24 ns  350.56 ns]
body_encode/json/10KB/concord/input                        [4.5709 µs  4.5933 µs  4.6095 µs]
body_encode/json/10KB/concord/prepare                      [12.302 µs  12.331 µs  12.361 µs]
body_encode/json/10KB/manual/input                         [7.4059 µs  9.5006 µs  11.801 µs]
body_encode/json/10KB/manual/prepare                       [11.956 µs  13.130 µs  14.961 µs]
decode/json/10KB/concord/input                             [323.06 ns  325.26 ns  328.06 ns]
decode/json/10KB/concord/decode                            [36.119 µs  40.344 µs  44.007 µs]
decode/json/10KB/manual/input                              [347.48 ns  370.73 ns  413.89 ns]
decode/json/10KB/manual/decode                             [21.779 µs  25.362 µs  31.627 µs]
roundtrip/small/concord/send                               [8.4525 µs  9.2063 µs  10.477 µs]
roundtrip/small/concord/send_prepared                      [8.3296 µs  8.3499 µs  8.3711 µs]
roundtrip/small/manual/send                                [8.1304 µs  8.1751 µs  8.2317 µs]
roundtrip/small/manual/send_prepared                       [8.2749 µs  8.8104 µs  9.4007 µs]
roundtrip/small/raw_reqwest/get_bytes                      [8.6361 µs  9.2131 µs  10.015 µs]
roundtrip/10KB/concord/send                                [46.025 µs  53.833 µs  63.491 µs]
roundtrip/10KB/concord/send_prepared                       [31.204 µs  31.311 µs  31.397 µs]
roundtrip/10KB/manual/send                                 [41.236 µs  52.611 µs  67.450 µs]
roundtrip/10KB/manual/send_prepared                        [77.640 µs  89.762 µs  113.34 µs]
roundtrip/10KB/raw_reqwest/get_bytes                       [11.669 µs  14.627 µs  19.697 µs]
```

## Limitations

- No baseline has been recorded. It needs a quiet machine, using the README procedure.
- The Concord native-`HeaderValue` variant does not exist yet. The group is ready for it.
- Fairness depends on the hand-written equivalents: prebuilt client headers, `query_pairs_mut`, `RequestBuilder`. Two cases are less direct:
  - The S2 and minimal hand-written cases take borrowed or no arguments, so they have no `input` stage. Compare them on `input_prepare`.
  - S2/S3 use the pre-existing `benches/baselines` as they are.
- Round trips put client and server on one thread over the kernel loopback. They show Concord's overhead relative to transport, not production latency.
- In-memory decoding does not exercise socket streaming.
- Concord takes bodies by value, so dropping the payload falls inside `prepare`. The hand-written JSON case also drops its input inside the routine.
- Only wall time is measured (no CPU counters). Sub-100 ns `input` stages are close to timer resolution.
- `concord-bench` requires Rust ≥ 1.86 and is outside the 1.85 MSRV gates.
- The historical records `v4/refinement/PERF.md` and `v4/evidence/PERF.md` still cite `cargo run -p concord-perf`. They were left unchanged as archived evidence; `docs/MAINTAINERS.md` was updated.
