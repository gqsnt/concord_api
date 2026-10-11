# Stream driver comparison

**Status:** Historical v2.1 evidence (ADR 0011); not re-run for the v3 tree.

- **Recorded:** 2026-09-04
- **Host:** Windows x86-64
- **Toolchain:** `rustc 1.97.1 (8bab26f4f 2026-07-14)`
- **Command:** `cargo run -p concord-perf --release --bin stream_driver_comparison --locked`
- **Raw summary:** `target/perf/stream-driver-comparison/latest.txt` (generated, intentionally not committed)
- **Selected implementation:** private boxed erasure, with public `ByteStream` and `ItemStream<T>` unchanged

## Precommitted budgets

These budgets were fixed before the final measurement:

- no allocation per item or chunk solely because of erasure;
- no more than one unavoidable boxed allocation at construction;
- boxed byte throughput within 10% of the fastest practical strategy;
- boxed item throughput within 15%;
- no binary-size increase greater than 15% over the smallest practical strategy without a compensating gain;
- identical lifecycle, bounds, error, cancellation, and completion semantics.

## Method

The maintained probe compares a monomorphized driver, a private enum, and a
non-devirtualizable boxed trait object. Twelve samples rotate execution order.
The table reports independently sorted medians. Budget ratios are medians of
within-sample boxed/fastest pairs, so clock changes between samples do not
manufacture a deficit or gain. The item cases perform actual `serde_json` and
`csv-core` parsing. `std::hint::black_box` retains the consumed results.

Allocation counts use `allocation-counter`. Lifecycle probes cover pending,
item, terminal error, end, and early drop. Three otherwise equivalent release
binaries isolate strategy code size. Their builds used empty, separate target
directories.

## Results

| Workload | Static | Private enum | Boxed | Boxed/fastest paired median |
|---|---:|---:|---:|---:|
| Construction | 3.58 ns | 3.57 ns | 30.47 ns | not budgeted; one box allowed |
| Construction + first item | 0.29 ns | 0.29 ns | 33.40 ns | not budgeted; includes allocation |
| Bytes, 256-byte chunks | 1,282.73 MiB/s | 1,168.93 MiB/s | 1,475.98 MiB/s | 1.000 |
| Bytes, 4 KiB chunks | 1,043.51 MiB/s | 1,095.36 MiB/s | 1,135.52 MiB/s | 1.000 |
| Bytes, 64 KiB chunks | 1,134.47 MiB/s | 1,116.31 MiB/s | 1,116.36 MiB/s | 1.000 |
| NDJSON records | 19.91 M items/s | 19.59 M items/s | 26.18 M items/s | 1.000 |
| CSV records | 23.55 M items/s | 24.17 M items/s | 23.98 M items/s | 0.986 |

| Strategy | Construction allocations | Per-poll allocations | Clean probe build | Release binary |
|---|---:|---:|---:|---:|
| Static | 0 | 0 | 17.35 s | 126,464 bytes |
| Private enum | 0 | 0 | 17.78 s | 126,464 bytes |
| Boxed | 1 | 0 | 17.95 s | 126,464 bytes |

All three lifecycle signatures were
`pending,item,pending,error,end`; all recorded one early drop. Concord's focused
record-stream suite independently passed 36 tests after the comparison code was
added, including chunk-boundary parity, backpressure, bounded staging, terminal
error, observation, batching, and drop cancellation.

## Decision

Retain the current private boxed erasure. It meets every precommitted budget:
one construction allocation, zero erasure allocations while polling, no
measured throughput deficit beyond the limits, identical probe binary size,
and the same lifecycle signature. The roughly 30 ns construction cost is paid
once per stream and buys the smallest private implementation surface; it does
not justify a public decoder type or a more complex private driver.

## Limitations

This is a single Windows/x86-64 host measurement, not a cross-platform claim.
It isolates driver dispatch with in-memory chunks; it does not include sockets,
TLS, scheduling, or application work. Absolute throughput is therefore not an
end-to-end service prediction. The committed probe, allocation assertions,
budget assertions, size probes, and runtime lifecycle tests make the decision
repeatable and fail visibly if the selected strategy later crosses a budget.

## Release-candidate reproduction

The unchanged probe was rerun at `9470e24`. Boxed/fastest paired medians were
0.993, 0.996, and 1.000 for the three byte sizes and 0.995/0.978 for
NDJSON/CSV. Allocation counts remained static `(0,0)`, enum `(0,0)`, boxed
`(1,0)` for construction/polling, and every lifecycle remained
`pending,item,pending,error,end` with one observed early drop. The selected
boxed strategy therefore remained inside every precommitted budget.
