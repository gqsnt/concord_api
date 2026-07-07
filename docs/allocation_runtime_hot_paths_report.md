# PERF-PR 21B Allocation Report for Runtime Hot Paths

This is a report-only summary of the current runtime allocation-count prototype. It is built on the PERF-PR 21A benchmark-only counting allocator under `perf/benches/allocation_counts.rs`. It does not change production behavior, benchmark semantics, feature flags, Cargo manifests, or thresholds.

## 1. Executive Summary

This is the first runtime allocation-count report for Concord hot paths using the benchmark-only prototype from PERF-PR 21A.

The counts below are process-local and machine-local. They are report-only observations from one local run. No allocation thresholds are introduced here.

## 2. Measurement Method

The allocation counter exists only in the standalone `perf` benchmark binary for `allocation_counts`.

That means:

- production crates are not instrumented
- setup-owned fixture teardown is excluded by keeping setup state alive until after the allocation snapshot
- consumed inputs may still be intentionally allocated inside the measured block
- async runtime and library overhead around the measured operation may still be included
- the counts are not universal and can vary by platform, toolchain, allocator, and background load

The current prototype reports four counters:

- allocation calls
- deallocation calls
- allocated bytes
- deallocated bytes

## 3. Allocation Target Matrix

Current local output from:

```text
cargo bench --manifest-path perf/Cargo.toml --bench allocation_counts
```

### attempt_pipeline/mock_transport_success/minimal_get

- alloc calls: `23`
- dealloc calls: `21`
- bytes allocated: `10902`
- bytes deallocated: `9522`
- caveat: `setup_teardown_excluded async_runtime_may_be_included`

### auth_runtime/apply/bearer

- alloc calls: `41`
- dealloc calls: `34`
- bytes allocated: `14595`
- bytes deallocated: `12888`
- caveat: `setup_teardown_excluded async_runtime_may_be_included`

### redaction_hooks/headers/mixed_case

- alloc calls: `31`
- dealloc calls: `27`
- bytes allocated: `12612`
- bytes deallocated: `10994`
- caveat: `setup_teardown_excluded async_runtime_may_be_included`

### streaming_upload/async_read/1MiB/chunk_8KiB

- alloc calls: `155`
- dealloc calls: `154`
- bytes allocated: `2115945`
- bytes deallocated: `2115921`
- caveat: `setup_teardown_excluded async_runtime_may_be_included`

## 4. Interpretation Notes

- Deallocations can be lower than allocations if some measured objects stay live until after the snapshot.
- Deallocations can also include teardown for objects owned by the measured operation when the operation intentionally consumes an owned input inside the measured block.
- The current prototype counts a successful `realloc` as one allocation event and one deallocation event.
- These counts should be compared only within similar local runs unless later stability data is collected.

## 5. Comparison with Timing Reports

Criterion timing reports and allocation-count reports answer different questions.

- Criterion timing reports measure elapsed runtime.
- Allocation-count reports measure heap traffic.

They are complementary, not interchangeable. This report does not compare allocation counts to timing results as though they were the same metric.

## 6. Security and Behavior Confirmation

This report does not change:

- production behavior
- auth, retry, rate-limit, or redaction behavior
- public APIs
- feature flags
- live network behavior
- credential handling
- hook/debug/error surfaces

The existing security guarantees remain intact: hooks, debug output, and public errors/source chains do not expose raw auth secrets or body bytes.

## 7. Open Issues and Next Candidates

Open questions remain around:

- isolating async runtime overhead from the measured path
- separating setup allocations from per-operation allocations even more precisely
- allocator and platform variability
- the lack of thresholds at this stage

Likely next candidate PRs:

- extend allocation reporting to pagination, governor, and retry cases
- add a macro compile-time memory/RSS report
- add a local repeated-run script to gauge allocation-count variability

## 8. Report-Only Confirmation

This report is informational only. It introduces no production code changes, no benchmark thresholds, and no new dependency requirements.
