# Runtime benchmarks (Criterion) and allocation counts

`concord-bench` measures the **same scenarios** implemented with Concord and with equivalent
hand-written reqwest. It replaces the former `Instant` harness (`benches/perf`); the keep-alive
loopback server moved to `src/server.rs`. Compile-time workloads stay in `benches/scale`
(hyperfine). A benchmark is not a correctness test — but `cargo test -p concord-bench` checks that
each Concord request equals its hand-written counterpart (method, URL, header multiset, body), so
the comparisons are like for like.

| Path | Contents |
|---|---|
| `src/apis.rs` | Concord declarations used only here (tracker, headers, deep chain, header operations, bodies, native header names) |
| `src/manual.rs` | their hand-written reqwest equivalents (S1–S3 equivalents are `benches/baselines`) |
| `src/lib.rs` | the scenarios, defined once and driven by both runners |
| `benches/runtime.rs` | the Criterion suite (wall time) |
| `src/bin/alloc_count.rs` | deterministic allocation counts (counting global allocator) |
| `summarize.py` | markdown summary of `target/criterion` (mean, 95% CI, change vs a named baseline) |

Requires Rust ≥ 1.86 (criterion 0.8); the crate is outside the 1.85 MSRV gates.

## Benchmark IDs

`<group>/<scenario>/<implementation>/<stage>`; implementations are `concord`, `manual`
(hand-written reqwest) and, for round trips, `raw_reqwest`. Stages:

| Stage | Measures | How |
|---|---|---|
| `input` | per-call input construction: the endpoint request value (navigation, arguments, `set_*`) or the hand-written arguments | `iter_with_large_drop` (drop not timed); omitted when the hand-written input is a constant or a reference |
| `prepare` | `prepare()` / building the `reqwest::Request` (incl. body encoding) from a fresh input | `iter_batched` (`SmallInput`; `LargeInput` for 1 MB bodies), drop not timed |
| `input_prepare` | both, as one call site does (`construct` and the deep chain only) | `iter_with_large_drop` |
| `decode` | `Decoder::decode` vs `error_for_status` + `Response::{json,text,bytes}` on an in-memory `reqwest::Response` (from an `http::Response`), polled once without a runtime | `iter_batched`; the `input` stage is building that response |
| `send` | round trip from the handle: input + prepare + execute + decode | async `iter` on a current-thread Tokio runtime |
| `send_prepared` | execute + decode of a request prepared in batch setup | async `iter_batched` (`SmallInput`) |
| `get_bytes` | raw `client.get(url).send()` + `bytes()` (transport floor) | async `iter` |

Groups and scenarios (188 IDs; 180 until the `keys` group was added, 171 until the native and literal header variants were added; `cargo bench -p concord-bench --bench runtime -- --list`):

- `construct`: `minimal` (S1 `GET health`), `s3_search_9_query`, `s2_product_scoped` (the
  `examples/shapes` / `benches/baselines` pairs measured by the former harness),
  `realistic_all_present` (3 path values, 10 query values incl. a repeated one, 2 scopes,
  6 inherited headers incl. a sensitive one) and `realistic_optional_absent` (same endpoint, only
  the 2 initialized query values).
- `headers`: `{1,4,16}/{variant}` with `Throughput::Elements(n)`; variants `concord_text`
  (owned `String`s), `concord_numeric` (`u64`), `concord_native` (`&HeaderValue` per call,
  cloned), `concord_literal` (literal values, lowered to statics), `manual_from_str`,
  `manual_numeric` (`HeaderValue::from(u64)`), `manual_prebuilt_clone` (clones of `HeaderValue`s
  validated once; the counterpart of `concord_native`) and `manual_from_static` (the counterpart
  of `concord_literal`). A variant is one function in `src/lib.rs` (`headers_*`) plus endpoints in
  `src/apis.rs`.
- `compose`: `deep8_navigate` (8 nested scopes, each a path value and a header, navigated per
  call), `deep8_reused_scope` (from a level-8 handle built once; the hand-written handle caches
  its URL prefix and header map), and from a reused scope handle: `inherit`, `override_present`,
  `override_absent` (`override .. else inherit`), `append`, `remove`.
- `keys`: native header names (`header(AUTHORIZATION)`, custom `HeaderName` constants) against
  literal names and hand-written reqwest, from a reused scope handle: `six_inherited` (six headers,
  one sensitive) and `one_replaced` (the same with `x-a` replaced through a literal spelling of a
  native key) as `{concord_literal,concord_native,manual}`, and `riot_match_ids/{concord,manual}`
  (routed host, native sensitive key, one optional query). A chain with a native key is composed
  when the request is built (`HeaderPlan`), a literal chain at compile time.
- `body_encode`: `{json,text,bytes}/{100B,10KB,1MB}` with `Throughput::Bytes`.
- `decode`: `{json,text,bytes}/{100B,10KB,1MB}` with `Throughput::Bytes`.
- `roundtrip`: `{small,10KB}/{concord,manual,raw_reqwest}/{send,send_prepared,get_bytes}`;
  keep-alive HTTP/1.1 against `127.0.0.1`, client and server on one current-thread runtime;
  runtime, server, clients and the pooled connection are created before measuring. `small` is the
  ~90 B S1 health document, `10KB` the JSON payload of the body groups.

IDs are stable: they are compared across revisions. Rename one only together with the baselines
that use it. Scenario code calls ordinary generated methods only, so migrating the declarations in
`examples/shapes` and `src/apis.rs` to a new DSL surface does not change the IDs.

## Running

```sh
cargo bench -p concord-bench --bench runtime                         # everything (~25–30 min)
cargo bench -p concord-bench --bench runtime -- '^headers/'           # a regex filter on IDs
cargo bench -p concord-bench --bench runtime -- --warm-up-time 1 --measurement-time 2 '^construct/'  # quick look, not a result
cargo run --release -p concord-bench --bin alloc-count                # allocation table (seconds)
cargo run --release -p concord-bench --bin alloc-count -- headers/ --json allocs.json
```

Criterion defaults apply (3 s warm-up, 5 s measurement, 100 samples, 95% confidence, noise
threshold 1%).

## Baselines and comparison

```sh
# On revision A (e.g. before a DSL change):
cargo bench -p concord-bench --bench runtime -- --save-baseline v4-pre
# On revision B, same machine, same conditions:
cargo bench -p concord-bench --bench runtime -- --baseline v4-pre     # compares; does not overwrite v4-pre
# --baseline aborts at the first ID the baseline lacks (e.g. a variant added since); use
# --baseline-lenient v4-pre when IDs were added.
python3 -I benches/criterion/summarize.py --baseline v4-pre --criterion-change > comparison.md
```

- Raw data lands in `target/criterion/<group>/<id with / replaced by _>/`: `new/` (latest run),
  `base/` (previous run), `<name>/` (each `--save-baseline <name>`), `change/` (latest
  comparison). Each holds `benchmark.json` (full ID, throughput), `estimates.json` (mean, median,
  std dev, slope with 95% CIs, in ns), `sample.json` (raw iteration counts and times) and
  `tukey.json`. `--save-baseline` writes the named directory instead of `new/`.
- `summarize.py` reads `<current>/` (default `new`; `--current <name>` reports a saved baseline)
  and prints mean, 95% CI of the mean and throughput; with `--baseline <name>` also the baseline
  mean and the relative change of the means; `--criterion-change` adds Criterion's own change
  estimate and CI (only meaningful if the last run used `--baseline <name>`).
- Never mix machines, toolchains, CPU governors or load conditions between a baseline and a
  comparison. Run a comparison at least twice; treat changes inside both runs' CIs, or under
  ~5% for sub-microsecond IDs, as noise.

## Archiving raw data as evidence

```sh
RUN=evidence/perf/criterion-$(date -u +%Y%m%dT%H%M%SZ)
mkdir -p "$RUN"
cp -r target/criterion "$RUN/"                                   # every sample, all baselines
python3 -I benches/criterion/summarize.py --current v4-pre > "$RUN/summary.md"
cargo run --release -p concord-bench --bin alloc-count -- --json "$RUN/allocs.json" > "$RUN/allocs.md"
{ git rev-parse HEAD; rustc -vV; uname -a; nproc; uptime; \
  cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor 2>/dev/null; } > "$RUN/environment.txt"
tar -C "$(dirname "$RUN")" -czf "$RUN.tar.gz" "$(basename "$RUN")"
```

Record in the report: commit, toolchain, CPU, load, governor, the exact command line, and whether
the machine was otherwise idle.

## Allocation counts

`alloc-count` runs the same cases (same IDs) with a counting global allocator: one warm-up call,
then N inputs are built (`input` columns) and consumed (`stage` columns) with outputs kept alive
until counting stops. Reported per call: allocation calls (`alloc`, `alloc_zeroed`, `realloc`)
and bytes requested (frees not subtracted). The figures are exact and repeatable for a given
build; a `~` marks a mean that is not an exact multiple. Allocation counts do not depend on
machine load, so they are a stable complement to the noisy time figures.

## Noise and limits

- Do not record baselines on shared CI runners or on a machine running other builds: the
  `bench` workflow (manual dispatch) exists to check that the suite runs and to look at gross
  effects only.
- Sub-100 ns IDs (most `input` stages) are dominated by timer and loop overhead; compare them
  within one run only.
- The round trips include kernel loopback and the scheduler; their CIs are wider. Client and
  server share one thread (no cross-thread wake-ups), as in the former harness.
- In-memory decoding polls the future once with a no-op waker; it measures decoding, not body
  streaming from a socket.
- Concord takes bodies by value: dropping the encoded payload is part of `prepare`. The
  hand-written JSON case serializes from a reference and also drops its input inside the routine.
- Time is wall-clock only (`WallTime`); no CPU-counter measurement.
