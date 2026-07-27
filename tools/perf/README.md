# Concord local performance probes

`concord-perf` is a harness-free benchmark package for relative local
observation. It defines no product threshold and is not part of the Concord
production dependency graph.

The package reuses the workspace-pinned Tokio runtime and imports
`concord_test_support` directly for its loopback server, so body collection,
credential preparation, limiter acquisition, pagination, generic
specification selection, and bounded record framing can be exercised without
internet latency or an external service. Ordinary product imports come only
from the `concord` façade; the test-support dependency is benchmark-only and
does not enter a product package.

The safe-planning category uses the public `client.plan(&endpoint)` contract and
the real sanitized head builder. Authentication and rate-limiting measurements
execute generated requests through the ordinary managed lifecycle and bounded
loopback transport. The managed-retry category uses the public façade and real
managed executor for one bodyless 503-to-success retry; it disables private
protocol recovery and uses `without_budget()` only to remove rolling-budget
availability from the measurement. No benchmark-only runtime API or generated
conversion is part of a product crate.

`--smoke` forces one bounded iteration. The executable records each category
only after its named operation completes and asserts that all sixteen distinct
categories ran. Otherwise `CONCORD_BENCH_ITERATIONS` selects between 1 and
1,000 iterations, with a default of 100. Timings are observational only.

Run the lifecycle probe with:

```text
cargo bench --locked -p concord-perf --bench concord_overhead
cargo bench --locked -p concord-perf --bench concord_overhead -- --smoke
cargo bench --locked -p concord-perf --bench concord_overhead -- --category managed-retry --smoke
cargo bench --locked -p concord-perf --bench concord_overhead -- --list
```

The isolated retry command performs an untimed correctness preflight, then
reports logical operations, one managed retry per operation, expected physical
application sends, total elapsed time, and per-operation time. Sequential
503/success script pairs and aggregate request/classifier assertions make a
misaligned or incomplete lifecycle fail the command. There is no performance
threshold.

Interpret timings only as comparisons from the same machine, toolchain, build
profile, and iteration count. Do not turn a single observation into a product
threshold or commit generated result snapshots.

`cargo run --locked -p concord-perf --bin expansion_scale` generates isolated
1, 10, 100, and 1,000-endpoint contracts under `target/perf`, expands each
with the installed `cargo-expand`, and prints comparable line, character,
whitespace-fragment, declaration-shaped-item, and elapsed-time counters. Pass
explicit endpoint counts to run a subset. Generated sources and build outputs
remain disposable target artifacts.
