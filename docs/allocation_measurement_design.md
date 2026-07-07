# PERF-PR 20 Allocation Measurement Design Report

This is a report-only design note. It does not change production behavior, benchmark code, feature flags, or dependency graphs.

## 1. Executive Summary

Current Criterion benches in Concord provide timing, not allocation counts. That is sufficient for coarse runtime comparisons, but it is not enough to answer whether a hot path is still creating avoidable heap traffic after an optimization lands.

That gap matters now because the recent performance series removed several classes of avoidable work:

- request-path URL sanitization reuse
- auth preparation reuse
- retry metadata cloning reductions
- governor fast paths and bounded storage
- redaction/sensitive-name matching cleanup
- macro/codegen lookup and construction deduplication

Those changes reduced overhead, but further allocation-focused work needs a measurement plan that can show where remaining heap traffic comes from before any new optimization is proposed.

This PR is report-only. It documents measurement options, a staged plan, and the initial target matrix for later allocation-report work.

## 1.1 Implementation Status

A benchmark-only allocation-count prototype now exists under `perf/benches/allocation_counts.rs` and reports a small initial matrix without changing production behavior. The prototype keeps setup-owned fixtures alive until after the allocation snapshot so teardown does not pollute per-operation counts. The first runtime allocation report lives in [`allocation_runtime_hot_paths_report.md`](allocation_runtime_hot_paths_report.md).

## 2. Measurement Goals

The initial allocation-report effort should cover:

- request attempt path
- auth preparation and retry paths
- redaction, debug, and hook paths
- streaming upload and streaming response paths
- pagination collect path
- rate-limit governor paths
- macro/codegen compile-time memory footprint, if practical

The goal is to observe allocation counts and/or peak memory behavior for these paths without changing the runtime semantics being measured.

## 3. Candidate Measurement Approaches

### A. Custom global allocator counter in benchmark-only or test-only contexts

What it can measure:

- allocation and deallocation counts
- allocation sizes, if instrumented
- rough per-path heap churn inside a controlled benchmark or test harness

What it cannot measure:

- allocator internal fragmentation
- retained memory behavior outside the instrumented process
- precise platform-neutral memory residency

Constraints:

- can perturb timings
- must be isolated to benchmark-only or test-only code
- may require careful teardown/reset between runs

CI suitability:

- useful in report mode first
- potentially suitable for later low-noise assertions only if it proves stable

Dependency impact:

- can be dependency-free if implemented with a small custom counting allocator

Fit for Concord now:

- appropriate as a later benchmark-only prototype
- not yet appropriate as a production change

### B. `dhat` or heap-profiling style tooling

What it can measure:

- allocation sites
- heap profiles over time
- retained heap behavior

What it cannot measure:

- exact same timings as the non-instrumented run
- cross-platform consistency without extra setup

Constraints:

- usually adds toolchain or crate dependencies
- often requires a specific allocator or build mode
- may not be easy to run in the normal release-gate environment

CI suitability:

- better as an optional report tool than a gate

Dependency impact:

- may require a new dependency or a separate profiling build path

Fit for Concord now:

- useful if a deeper investigation is needed
- probably too heavy for the first instrumentation pass

### C. OS/toolchain profilers such as heaptrack, Instruments, or similar

What it can measure:

- heap behavior at the process level
- retained allocations
- broad memory-growth trends

What it cannot measure:

- stable cross-platform results
- low-friction CI integration

Constraints:

- tool availability varies widely by platform
- usually manual and environment-specific

CI suitability:

- poor for normal CI
- useful for local diagnosis and report capture

Dependency impact:

- no Rust dependency required, but external tooling is needed

Fit for Concord now:

- appropriate for manual follow-up, not for the first report artifact

### D. Benchmark-local counting allocators

What it can measure:

- allocations inside a single benchmark case
- per-case deltas across hot paths

What it cannot measure:

- allocations outside the benchmark scope
- allocator-specific resident memory behavior

Constraints:

- can slightly perturb timings
- should be confined to `perf/`

CI suitability:

- promising for report-only benchmark output
- later potential for non-failing regression observation

Dependency impact:

- can often remain dependency-free

Fit for Concord now:

- the best first implementation candidate for runtime hot-path allocation reporting

### E. External command-level RSS or peak-memory measurements

What it can measure:

- coarse process-level memory footprint
- build or benchmark command memory growth

What it cannot measure:

- exact per-allocation counts
- attribution to a specific hot path without extra instrumentation

Constraints:

- noisy across platforms and concurrent workloads
- sensitive to caching and background load

CI suitability:

- useful only as a report signal, not as a threshold

Dependency impact:

- no Rust dependency required

Fit for Concord now:

- useful as a secondary signal for macro or benchmark processes

### F. Compile-time memory observations for generated macro fixture crates

What it can measure:

- RSS or peak-memory behavior while compiling generated fixtures
- rough macro/codegen footprint trends

What it cannot measure:

- precise allocation counts inside the compiler process
- direct mapping to a single code path without extra tooling

Constraints:

- command and platform sensitivity
- may need script support similar to the existing macro-scale report

CI suitability:

- report-only is reasonable

Dependency impact:

- can remain dependency-free if implemented through shell tooling

Fit for Concord now:

- worth adding as a separate report if runtime allocation reporting is successful

## 4. Recommended Staged Plan

### Phase A: report-only/manual allocation profiling instructions

- document candidate tooling and the first target paths
- keep the current timing benches unchanged
- use manual profiling or platform tools to validate the measurement approach
- do not introduce pass/fail allocation thresholds

### Phase B: benchmark-only allocation counter prototype in `perf/`

- add a benchmark-local counting allocator or equivalent wrapper
- scope it to selected `perf/benches/*.rs` targets
- report counts as machine-local output only
- do not fail the build on a threshold yet

### Phase C: targeted allocation assertions only if stable and low-noise

- add optional assertions only after the benchmark-local data shows low variability
- keep assertions narrow and opt-in
- prefer reporting over gating until stability is demonstrated

### Phase D: optional CI report mode

- add a report job that records allocations in a controlled environment
- keep it separate from normal pass/fail release gates
- consider thresholds only after multi-platform evidence exists

## 5. Initial Target Matrix

The first allocation-report targets should be:

- `attempt_pipeline/mock_transport_success/minimal_get`
- `attempt_pipeline/retry_once_then_success`
- `auth_runtime/apply/bearer`
- `auth_runtime/cached_preparation/slot_retry_reuses_preparation`
- `redaction_hooks/headers/mixed_case`
- `redaction_hooks/hooks_debug/real_path_vv`
- `streaming_upload/async_read/1MiB/chunk_8KiB`
- `streaming_response/raw_drain/chunks_1024`
- pagination benchmark cases
- `rate_limit_governor/empty_plan/acquire`
- `rate_limit_governor/high_cardinality_keys/joined_futures_32`

These cover the request path, auth, redaction, streaming, pagination, and governor surfaces that already have timing coverage and are now reasonable allocation candidates.

## 6. Safety and Security Constraints

Allocation measurement must preserve the existing security model:

- no live network
- no real credentials
- no body-byte exposure through hooks, debug, or errors
- no raw auth-secret logging
- no weakening redaction simply to expose allocation behavior
- no unsafe allocator tricks unless separately reviewed and justified
- no global allocator changes in production crates

## 7. CI and Release-Gate Policy

- allocation measurement should begin as report-only
- no pass/fail allocation thresholds should be added in the first instrumentation PR
- future thresholds require platform stability evidence
- Criterion timing reports and allocation reports should remain separate unless a later design says otherwise

## 8. Open Questions

- Should Concord use dependency-free allocator counting first, or bring in an optional profiling crate?
- How should async runtime and background allocations be separated from the measured hot path?
- What is the cleanest way to isolate setup allocations from per-iteration allocations?
- How should allocator behavior differences across platforms be handled?
- Should reports focus on transient allocations, retained memory, or both?
- Should macro compile-time memory be handled by a separate report script rather than the runtime benches?

## 9. Proposed Future PRs

Suggested follow-up PRs, not implemented here:

- PERF-PR 21A: benchmark-only allocation counter prototype
- PERF-PR 21B: allocation report for runtime hot paths
- PERF-PR 21C: macro compile-time memory/RSS report
- PERF-PR 21D: documentation update for interpreting allocation reports

## 10. Report-Only Confirmation

This PR does not change production code, benchmark source, feature flags, Cargo manifests, lockfiles, or thresholds.
