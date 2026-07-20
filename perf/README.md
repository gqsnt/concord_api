# Concord local performance probes

`concord_perf` is an unpublished, harness-free benchmark package for relative local observation.
It defines no product threshold and is not part of the Concord production dependency graph.

The package reuses the workspace-pinned Tokio runtime and `concord_test_support` loopback server so
body collection, credential preparation, limiter acquisition, pagination, generic specification
selection, and bounded record framing can be exercised
without internet latency or an external service. Those dependencies are benchmark-only support;
they add no dependency to `concord_core` or `concord_macros`.

The public-head probe calls Concord's actual production head preparation and validation path, then
stops before hooks, body work, authentication, limiting, native HTTP, or response handling. The
credential ready-path probe uses an HTTPS logical endpoint with a pre-populated static credential
and calls the actual production preflight and post-limiter finalization operations; it stops before
native materialization and performs no provider acquisition, refresh, or transport. Both probes use
the explicitly unstable, feature-gated `concord_core::__development` boundary and receive only a
body-free aggregate observation.

`--smoke` forces one bounded iteration. The executable records each category only after its named
operation completes and asserts that all eleven distinct categories ran. Otherwise
`CONCORD_BENCH_ITERATIONS` selects between 1 and 1,000 iterations, with a default of 100. Timings
are observational only.
