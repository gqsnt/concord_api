# Concord local performance probes

`concord_perf` is an unpublished, harness-free benchmark package for relative local observation.
It defines no product threshold and is not part of the Concord production dependency graph.

The package reuses the workspace-pinned Tokio runtime and `concord_test_support` loopback server so
body collection, credential preparation, limiter acquisition, and pagination can be exercised
without internet latency or an external service. Those dependencies are benchmark-only support;
they add no dependency to `concord_core` or `concord_macros`.

The credential ready-path probe observes a pre-populated generated credential handle without
performing native execution; it does not weaken Concord's HTTPS-only authentication boundary.

`--smoke` forces one bounded iteration. Otherwise `CONCORD_BENCH_ITERATIONS` selects between 1 and
1,000 iterations, with a default of 100. Timings are observational only.
