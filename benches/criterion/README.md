# Runtime benchmarks (Criterion) and allocation counts

`concord-bench` measures scenarios implemented with Concord and hand-written reqwest. The
synchronous consuming stages promise equal observable results; round trips include explicitly
partial comparisons, as inventoried below. It replaces the former `Instant` harness (`benches/perf`); the keep-alive
loopback server moved to `src/server.rs`. Compile-time workloads stay in `benches/scale`
(hyperfine). A benchmark is not a correctness test — but `cargo test -p concord-bench` checks that
every registered synchronous pair at its measured boundary. Preparation compares method, exact
URL, header names and values (including multiplicity, same-name order and sensitivity), optional
body presence and bytes, HTTP version and request timeout. Cross-name HeaderMap iteration is not
wire order. Decoding compares the actual decoded Rust values on the registered successful
response fixtures. This does not promise identical algorithms, diagnostics or error behavior on
other inputs. No client defaults applied at send time can establish prepared-stage equality.

Historical S3 comparisons **measured unequal prepared work; superseded by the
[M01 corrected comparison](#m01-s3-correction)**. The original observations remain unchanged.

| Path | Contents |
|---|---|
| `src/apis.rs` | Concord declarations used only here (tracker, headers, deep chain, header operations, bodies, native header names) |
| `src/manual.rs` | their hand-written reqwest equivalents (S1–S3 equivalents are `benches/baselines`) |
| `src/lib.rs` | the scenarios, defined once and driven by both runners |
| `src/contract.rs`, `src/tests.rs` | untimed stage observations and equivalence checks driven by those same registrations |
| `benches/runtime.rs` | the Criterion suite (wall time) |
| `src/bin/alloc_count.rs` | deterministic allocation counts (counting global allocator) |
| `summarize.py` | markdown summary of `target/criterion` (mean, 95% CI, change vs a named baseline) |


## Benchmark IDs

`<group>/<scenario>/<implementation>/<stage>`; implementations are `concord`, `manual`
(hand-written reqwest) and, for round trips, `raw_reqwest`. Stages:

| Stage | Measures | How |
|---|---|---|
| `input` | per-call input construction: the endpoint request value (navigation, arguments, `set_*`) or the hand-written arguments | `iter_with_large_drop` (drop not timed); omitted when the hand-written input is a constant or a reference |
| `prepare` | `prepare()` / building the `reqwest::Request` (incl. body encoding) from a fresh input; destruction of consumed input fields is timed | `iter_batched` (`SmallInput`; `LargeInput` for 1 MB bodies); setup and returned-output destruction are not timed |
| `input_prepare` | both, as one call site does (`construct` and the deep chain only) | `iter_with_large_drop` |
| `decode` | `Decoder::decode` vs `error_for_status` + `Response::{json,text,bytes}` on an in-memory `reqwest::Response` (from an `http::Response`), polled once without a runtime; includes destruction of the consumed response/head | `iter_batched`; the `input` stage builds a fresh response; returned decoded-output destruction is not timed |
| `send` | round trip from the handle: input + prepare + execute + decode | async `iter` on a current-thread Tokio runtime |
| `send_prepared` | execute + decode of a request prepared in batch setup | async `iter_batched` (`SmallInput`) |
| `get_bytes` | raw `client.get(url).send()` + `bytes()` (transport floor) | async `iter` |

Groups and scenarios (188 synchronous IDs, 10 round-trip IDs and 32 planner IDs;
`cargo bench -p concord-bench --bench runtime -- --list`):

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

## Comparison inventory

The existing `Case` registrations are the synchronous inventory: `pair` connects counterparts,
`setup` records reusable fixtures and fresh inputs, and `run_stage`/`comparator()` state the
boundary and observable contract. Both measurement runners and the test runner use the same
input and consuming closures. To print every emitted synchronous ID, setup, contract and
comparison group while asserting all 47 comparisons (92 cases, 45 groups), run:

```sh
cargo test --locked -p concord-bench registered_pairs_match -- --nocapture
```

| Registered family / paired IDs | Setup outside the consuming routine | Consuming-stage contract / comparator |
|---|---|---|
| `construct/*/{concord,manual}` | clients reused; fresh request navigation/setters or manual arguments; constants when `trivial_input` | equal-work prepared native state; includes S1, S2, S3 and both tracker forms |
| `headers/{1,4,16}/{concord_text,manual_from_str}` | clients reused; fresh owned strings | equal-work prepared native state |
| `headers/{1,4,16}/{concord_numeric,manual_numeric}` | clients reused; copied integers | equal-work prepared native state |
| `headers/{1,4,16}/{concord_native,manual_prebuilt_clone}` | clients and validated HeaderValues reused; Concord's borrowed constructor arguments are cloned into its input; manual input is a count | equal-work prepared native state; manual clones are inside `prepare` |
| `headers/{1,4,16}/{concord_literal,manual_from_static}` | clients and literal statics reused | equal-work prepared native state |
| `compose/*/{concord,manual}` | fresh owned navigation values for `deep8_navigate`; other cases reuse scopes | equal-work prepared native state; append comparison preserves three ordered `x-b` values |
| `keys/{six_inherited,one_replaced}/{concord_literal,concord_native,manual}`, `keys/riot_match_ids/{concord,manual}` | clients/team scopes reused; fresh endpoints/owned arguments versus borrowed manual arguments | equal-work prepared native state, including sensitive native values |
| `body_encode/{json,text,bytes}/{100B,10KB,1MB}/{concord,manual}` | clients/seeds reused; fresh owned payload clones or shared Bytes handles | equal-work prepared native state, including exact body presence/bytes |
| `decode/{json,text,bytes}/{100B,10KB,1MB}/{concord,manual}` | decoder and byte seeds reused; fresh 200 response, matching content type/length, every call | equal-work decoded Rust value on this fixture; no general metadata/error-path equivalence claim |
| All synchronous `input` IDs | per-call representation construction itself is measured | different features: endpoint values versus manual arguments; no equal-work input claim |
| Synchronous `input_prepare` IDs where registered | clients/scopes reused; input construction is inside the routine | equal-work prepared native state; same comparator as `prepare` |
| `roundtrip/{small,10KB}/{concord,manual}/{send,send_prepared}` | runtime, loopback server, clients and pooled connection reused; `send_prepared` also builds each request outside timing | **partial floor**: manual omits prepared Accept and performs fewer metadata checks; tests compare successful decoded fixture values only |
| `roundtrip/{small,10KB}/raw_reqwest/get_bytes` | same loopback setup | **partial floor**: transport and bytes collection, without JSON decoding; no equal-work ratio |
| `plan_scaling/{8,32,128,512}/{distinct,appends,overrides,removals}/{compose,validate}` | native names and shapes prebuilt | **different features**: runtime plan production versus declaration-name validation; no equal-work comparator |

For `prepare`/`decode`, batch setup never consumes a body to validate it and then times an empty
input. The test runner reconstructs and executes every input twice, observing outputs after each
consuming call. All prepared fixtures have buffered bodies; the comparator refuses unhandled
streaming bodies. Decode fixtures reconstruct the one-shot native response from reusable byte
seeds. Negative controls require detection of altered/missing Accept, repeated value/order/count,
exact URL, method, changed/missing/empty body, sensitivity, version and timeout; cross-name header
insertion order and fresh reconstruction remain positive controls.

Cost placement follows each public call site. Concord navigation, string conversions and
`set_*` calls occur in `input`; the manual S3 builder also constructs its owned arguments there.
Their consuming routines format/query-encode values and assemble the URL, headers and request.
Concord's `&HeaderValue` arguments clone into the endpoint during `input`, while
`manual_prebuilt_clone` clones inside `prepare`; compare that boundary with this setup distinction
in mind. Manual cached deep scopes precompute the URL prefix/header map once; Concord scopes
retain typed values. JSON payloads are cloned in setup on both sides, serialized inside the
consuming routine and destroyed there (even though the manual serializer borrows its local
payload). Text and Bytes are moved into the output body. In-memory decode includes head checks,
body collection/decoding and destruction of the consumed response. Async `iter` also drops each
returned output during timing; async `iter_batched` retains outputs until after timing.

## M01 S3 correction

The manual S3 search now explicitly prepares `Accept: application/json` in
`benches/baselines/src/s3_query.rs`, matching Concord before execution. The registered full-query
pair and a default-query pair with a conflicting native client default both pass prepared-stage
comparison. Native client defaults are applied later and are not a repair for a missing header.

The historical S3 `prepare` and `input_prepare` results/ratios **measured unequal prepared work;
superseded by this corrected contract and its new measurements**. Preserve the original raw
numbers and do not apply an estimated percentage correction. Affected records include the
[initial Criterion summary](../../v4/native-repr/evidence/perf/criterion/summary-a915c8d.md),
[run 2](../../v4/native-repr/evidence/perf/criterion/summary-run2.md),
[run 3](../../v4/native-repr/evidence/perf/criterion/summary-run3.md), and the
[allocation table](../../v4/native-repr/evidence/perf/criterion/allocs-aaa5a3c.txt).
For example, the initial summary retains S3 prepared means of 852.8 ns (Concord) and
662.6 ns (manual), and combined means of 869.9 ns and 662.5 ns. These are historical observations,
not the corrected pair's performance. `input` numbers remain measurements of their respective
input representations. This notice does not invalidate unrelated cases or establish universal
wire inequality.

The corrected pair was measured on 2026-11-04 with Rust 1.99.1, the unchanged locked graph
(reqwest 0.13.5), an i5-13600K / Linux x86-64 host with 20 logical CPUs and `powersave` governor,
under the campaign's exclusive build lease. Only S3's six IDs and the unchanged minimal pair's
five IDs were measured, using Criterion defaults (3 s warm-up, 5 s measurement, 100 samples).
Means and 95% mean confidence intervals, in ns:

| ID below `construct/` | run 1 | run 3 |
|---|---:|---:|
| `s3_search_9_query/concord/prepare` | 849.1 [839.1, 861.7] | 836.8 [834.7, 839.6] |
| `s3_search_9_query/manual/prepare` | 648.2 [641.4, 658.1] | 648.6 [642.2, 656.8] |
| `s3_search_9_query/concord/input_prepare` | 857.0 [847.2, 868.3] | 844.5 [841.1, 849.2] |
| `s3_search_9_query/manual/input_prepare` | 654.4 [650.2, 659.9] | 644.5 [642.4, 647.1] |
| `minimal/concord/prepare` (control) | 163.9 [162.7, 165.6] | 163.3 [162.0, 165.0] |
| `minimal/manual/prepare` (control) | 123.9 [122.7, 125.2] | 119.9 [118.9, 121.1] |

All three timing runs are retained. Run 2 began with elevated CPU pressure (avg10 12.85%), so
run 3 repeated the same selection after pressure subsided. Run 1's CPU pressure avg10 went from
0.78% to 0.00%; run 3's from 0.03% to 0.00%. These snapshots and the exclusive campaign lease do
not certify whole-machine idleness. No timing comparison to the historical unequal pair was run,
and no speed change or corrected historical percentage is inferred.

The separate counting allocator recorded exact per-call input/stage allocation calls and
requested bytes (256 inputs after warm-up): S3 Concord `6 / 62` then `28 / 2417`; S3 manual
`6 / 110` then `24 / 2254`. The unchanged minimal controls recorded `0 / 0` input, then
`9 / 863` (Concord) and `4 / 734` (manual). These are current counts, not an allocation delta
attributed to the correction. Exact commands, source/lock hashes, host snapshots, all estimates
and 100 raw samples per ID per run are in the campaign handoff at
`lanes/m01-bench/measurements/`, with the acceptance receipt in `lanes/m01-bench/REPORT.md`.

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
