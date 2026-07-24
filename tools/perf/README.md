# Concord local performance probes

`concord-perf` is a harness-free benchmark package for relative local observation. It defines no product threshold and is not part of the Concord production dependency graph.

The package reuses the workspace-pinned Tokio runtime and imports
`concord_test_support` directly for its loopback server, so body collection,
credential preparation, limiter acquisition, pagination, generic
specification selection, and bounded record framing can be exercised without
internet latency or an external service. Ordinary product imports come only
from the `concord` façade; the test-support dependency is benchmark-only and
does not enter a product package.

The safe-planning category uses the public `client.plan(&endpoint)` contract and
the real sanitized head builder. The lower-level public-head probe calls the
same production preparation and validation path, then stops before hooks, body
work, authentication, limiting, native HTTP, or response handling. The
ready-credential probe uses an HTTPS logical endpoint with a pre-populated
static credential and calls the production preflight and post-limiter
finalization operations; it stops before native materialization and performs no
provider acquisition, refresh, or transport. The two lower-level probes use
the doc-hidden, explicitly unstable `concord::__benchmark` boundary and
receive only a body-free aggregate observation. Named generated wrappers
surrender their ordinary internal request carrier through a token-sealed
conversion that only these helpers can invoke. The bridge performs no
alternate transport and requires no public feature.

`--smoke` forces one bounded iteration. The executable records each category
only after its named operation completes and asserts that all sixteen distinct
categories ran. Otherwise `CONCORD_BENCH_ITERATIONS` selects between 1 and
1,000 iterations, with a default of 100. Timings are observational only.

`cargo run -p concord-perf --bin expansion_scale` generates isolated 1, 10,
100, and 1,000-endpoint contracts under `target/perf`, expands each with the
installed `cargo-expand`, and prints comparable line, character,
whitespace-fragment, declaration-shaped-item, and elapsed-time counters. Pass
explicit endpoint counts to run a subset. Generated sources and build outputs
remain disposable target artifacts.

The current checked-in observation is
[`results/phase-14-windows-msvc.md`](results/phase-14-windows-msvc.md).
