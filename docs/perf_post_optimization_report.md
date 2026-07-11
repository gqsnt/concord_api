# PERF-PR 19 Post-Optimization Performance Report

This is a report-only summary of the validated performance work completed across PERF-PR 1 through PERF-PR 18. It records the current machine-local evidence after the runtime, macro/codegen, build-footprint, and security-adjacent optimization series. It does not introduce new optimizations or behavior changes.

## Status Legend

Every PERF-PR entry below is tagged with one of these three states:

- **implemented** — active in the runtime today and reachable by generated clients.
- **partial** — a mechanism exists in the runtime, but nothing in the generated-client path currently activates it.
- **deferred** — not yet implemented; tracked as future work only.

## 1. Executive Summary

Validated work falls into these groups:

- Benchmark and report infrastructure
  - PERF-PR 1 through PERF-PR 7 established the standalone `perf/` benchmark package, runtime benchmark coverage, build-footprint reporting, macro-scale reporting, and release-gate timing reporting. **[implemented]**
- Runtime-path optimizations
  - PERF-PR 8 lowered async-read/file-upload chunk allocation overhead. **[implemented]**
  - PERF-PR 9 bounded governor cooldown cardinality and preserved fail-closed behavior. **[implemented]**
  - PERF-PR 10 reused the sanitized URL through the attempt path. **[implemented]**
  - PERF-PR 11 added governor empty-plan and no-cooldown fast paths. **[implemented]**
  - PERF-PR 12 made retry metadata/header cloning lazier. **[implemented]**
  - PERF-PR 13 removed duplicate auth collision validation. **[implemented]**
  - PERF-PR 17 reduced sensitive-name matching allocations. **[implemented]**
  - PERF-PR 18 added and activated a request-local cached auth preparation mechanism for generated clients on retry-stable credential paths. **[implemented]**
- Macro/codegen optimizations
  - PERF-PR 14 added local lookup indexes for facade/codegen resolution. **[implemented]**
  - PERF-PR 15 avoided duplicate `FacadeIr` construction. **[implemented]**
- Build-footprint and feature work
  - PERF-PR 16A produced the optional reqwest transport design report. **[implemented]**
  - PERF-PR 16B implemented the optional reqwest feature split while keeping default users compatible. **[implemented]**
- Security-adjacent runtime cleanup
  - PERF-PR 10, 12, 13, 17, and 18 all kept sanitized surfaces intact while removing avoidable work. **[implemented]**

### PERF-PR 18 status detail: request-local cached auth preparation

PERF-PR 18 implemented a request-local auth-preparation cache in `concord_core/src/client/execute.rs`. The cache is gated by the typed `AuthPreparationReuse` field on `PreparedAuthCredential`; `AuthProvenance.layer` is diagnostic-only and is no longer used as a control signal.

Generated clients now set `AuthPreparationReuse::RequestLocal` for retry-stable static credential paths: API key, static bearer, and basic credentials. OAuth2 client credentials and endpoint/manual credentials remain `Never` because they can refresh or be externally replaced outside ordinary transport-retry flow.

The cache remains request-local only. It is cleared on auth-rejection retry before the next attempt, so stale rejected material is not reused after invalidation.

All timing and benchmark observations below are machine-local and report-only.

## 2. Current Validation Matrix

The following commands were run from the repository root:

- `cargo fmt`
  - passed
- `cargo nextest run --workspace --all-targets`
  - passed: 938 tests run, 938 passed, 0 skipped
- `cargo doc --workspace --no-deps`
  - passed
- Historical feature, footprint, macro-scale, and gate-timing report commands
  were retired with the old command surface. Current deferred diagnostics are
  `just perf-check`, `just perf-test`, and `just bench-check`.

## 3. Runtime Benchmark Matrix

All benchmark commands were run with `cargo bench --manifest-path perf/Cargo.toml --bench <name>` and passed.

### Harness sanity

- `smoke`
- `smoke_roundtrip`: `[364.61 ns 508.65 ns 820.86 ns]`

### Attempt path

- `attempt_pipeline`
  - `mock_transport_success/minimal_get`: `[1.9731 us 1.9867 us 2.0005 us]`
  - `many_headers/32`: `[7.6474 us 7.6917 us 7.7419 us]`
  - `many_query_params/32`: `[8.7849 us 8.8385 us 8.8918 us]`
  - `bearer_auth`: `[2.7149 us 2.7258 us 2.7363 us]`
  - `query_auth`: `[3.1991 us 3.2345 us 3.2699 us]`
  - `retry_configured_but_success`: `[2.1085 us 2.1201 us 2.1317 us]`
  - `retry_once_then_success`: `[3.4648 us 3.4961 us 3.5283 us]`
  - `debug_level/none`: `[1.9678 us 1.9782 us 1.9883 us]`
  - `debug_level/v`: `[1.9625 us 1.9708 us 1.9787 us]`
  - `debug_level/vv`: `[1.9591 us 1.9677 us 1.9759 us]`
  - `noop_hooks/noop_rate_limiter`: `[1.9537 us 1.9636 us 1.9736 us]`
  - `custom_rate_limiter/counting`: `[2.1867 us 2.2124 us 2.2364 us]`

### Auth runtime

- `auth_runtime`
  - `baseline/no_auth`: `[2.0353 us 2.0553 us 2.0752 us]`
  - `apply/bearer`: `[2.9476 us 2.9800 us 3.0135 us]`
  - `apply/header`: `[3.0168 us 3.0535 us 3.0911 us]`
  - `apply/query`: `[3.4233 us 3.4880 us 3.5633 us]`
  - `apply/multiple_requirements`: `[4.0299 us 4.1328 us 4.2459 us]`
  - `collision/query/error_path`: `[2.2823 us 2.3178 us 2.3584 us]`
  - `repeated_credential/retry_reuses_material`: `[5.2793 us 5.3797 us 5.4884 us]`
  - `cached_preparation/slot_retry_reuses_preparation`: before `[6.1259 us 6.2486 us 6.3945 us]`, after `[6.9359 us 7.1612 us 7.4065 us]` — PR 12 machine-local run; the fixture now uses the typed `AuthPreparationReuse::RequestLocal` opt-in instead of the deleted provenance marker
  - `cached_credential/slot_two_requests`: `[5.6623 us 5.6980 us 5.7378 us]`

### Rate-limit governor

- `rate_limit_governor`
  - `empty_plan/acquire`: `[512.76 ns 517.62 ns 522.98 ns]`
  - `single_bucket_x1_window/acquire`: `[3.7488 us 3.7884 us 3.8327 us]`
  - `multi_bucket_windows/acquire`: `[21.324 us 21.464 us 21.613 us]`
  - `same_key/joined_futures_32`: `[53.538 us 54.068 us 54.660 us]`
  - `high_cardinality_keys/joined_futures_32`: `[117.96 us 118.79 us 119.68 us]`
  - `cooldown/no_action_observation_fast_path`: `[102.17 ns 103.19 ns 104.19 ns]`
  - `cooldown/on_response_store`: `[784.30 ns 788.99 ns 794.11 ns]`
  - `cooldown/cardinality_below_cap_32`: `[26.087 us 26.436 us 26.843 us]`
  - `cooldown/high_cardinality_to_cap_128`: `[116.00 us 117.32 us 118.65 us]`
  - `cooldown/cap_reached_error_path`: `[919.93 ns 930.86 ns 943.13 ns]`

### Redaction and hooks

- `redaction_hooks`
  - `url_query/small`: `[3.3707 us 3.4213 us 3.4788 us]`
  - `url_query/many`: `[18.132 us 18.322 us 18.528 us]`
  - `headers/small`: `[3.8725 us 3.9093 us 3.9475 us]`
  - `headers/many`: `[27.049 us 27.285 us 27.609 us]`
  - `headers/mixed_case`: `[14.657 us 14.752 us 14.850 us]`
  - `debug/disabled`: `[6.8384 us 6.8986 us 6.9687 us]`
  - `debug/v`: `[6.8743 us 6.9357 us 7.0024 us]`
  - `debug/vv`: `[7.7259 us 7.7737 us 7.8266 us]`
  - `hooks/noop_runtime`: `[6.7872 us 6.8336 us 6.8971 us]`
  - `hooks/counting_runtime`: `[6.7797 us 6.8204 us 6.8672 us]`
  - `hooks_debug/real_path_vv`: `[12.986 us 13.111 us 13.252 us]`

### Streaming upload

- `streaming_upload`
  - `async_read/1MiB/chunk_1KiB`: `[58.625 us 58.961 us 59.332 us]`
  - `async_read/1MiB/chunk_8KiB`: `[21.715 us 21.818 us 21.925 us]`
  - `async_read/1MiB/chunk_64KiB`: `[38.693 us 39.029 us 39.398 us]`
  - `async_read/16MiB/chunk_1KiB`: `[1.2575 ms 1.2720 ms 1.2874 ms]`
  - `async_read/16MiB/chunk_8KiB`: `[534.06 us 540.97 us 548.61 us]`
  - `async_read/16MiB/chunk_64KiB`: `[766.42 us 781.14 us 796.80 us]`
  - `byte_stream/1MiB/chunk_1KiB`: `[33.472 us 33.760 us 34.067 us]`
  - `byte_stream/1MiB/chunk_8KiB`: `[4.8125 us 5.0839 us 5.3566 us]`
  - `byte_stream/1MiB/chunk_64KiB`: `[2.5543 us 2.6328 us 2.7163 us]`
  - `byte_stream/16MiB/chunk_1KiB`: `[711.88 us 724.11 us 737.08 us]`
  - `byte_stream/16MiB/chunk_8KiB`: `[163.91 us 169.98 us 176.22 us]`
  - `byte_stream/16MiB/chunk_64KiB`: `[223.64 us 229.88 us 236.14 us]`

### Streaming response

- `streaming_response`
  - `raw_drain/chunks_16`: `[4.2113 us 4.2434 us 4.2782 us]`
  - `raw_drain/chunks_1024`: `[137.38 us 138.48 us 139.69 us]`
  - `ndjson/records_128`: `[7.4032 us 7.4867 us 7.5708 us]`
  - `sse/events_128`: `[32.951 us 33.174 us 33.421 us]`
  - `multipart/parts_32`: `[21.035 us 21.287 us 21.591 us]`

## 4. Build-Footprint and Feature Observations

Current feature-state observations:

- `cargo check -p concord_core --no-default-features` passed.
- `cargo check -p concord_core --no-default-features --features json` passed.
- `cargo check -p concord_core --no-default-features --features transport-reqwest` passed.
- `cargo check -p concord_core --no-default-features --features "transport-reqwest json"` passed.
- `cargo tree -p concord_core --no-default-features -i reqwest` reported that `reqwest` is absent by failing with `package ID specification 'reqwest' did not match any packages`.
- `cargo tree -p concord_core --no-default-features --features transport-reqwest -i reqwest` showed `reqwest v0.13.3` under `concord_core`.

Historical footprint-report observations:

- `workspace metadata summary: perf_present_in_packages: no`
  - this historical observation came from a retired report command.
- `perf package metadata summary: perf_present_in_packages: yes`
  - this historical observation came from a retired report command.
- `no-default concord_core includes reqwest: no`
  - this historical observation came from a retired report command.
- `transport-reqwest concord_core includes reqwest: yes`
  - this historical observation came from a retired report command.
- `serde_json present in concord_macros tree: yes`
  - this line comes from the historical report's plain `cargo tree -p concord_macros` capture. In this repository's current tree output, that capture includes a `[dev-dependencies]` section, so it sees `serde_json`; the normal `cargo tree -p concord_macros --edges normal,features` tree does not show `serde_json`
- the historical report also emitted local build timing lines including `0.07s`, `1.44s`, `3.56s`, `1.76s`, and `9.77s`

These are machine-local observations only.

## 5. Macro Compile-Time Observations

The command used for this historical macro-scale measurement has been retired.
Current deferred diagnostics are documented in `perf/README.md`.

Current local fixture summary:

- `fixture_root: /mnt/f/projects/condord_api_last/target/perf-macro-scale`
- `sizes: 1 10 50 100 250`
- `full_mode: disabled`
- generated fixtures:
  - `size-1`
  - `size-10`
  - `size-50`
  - `size-100`
  - `size-250`

Current local `cargo check` times from the generated fixture runs:

- `size-1`: `real 45.27`
- `size-10`: `real 39.60`
- `size-50`: `real 41.80`
- `size-100`: `real 39.56`
- `size-250`: `real 39.34`

The numbers are report-only and machine-local.

## 6. Release-Gate Timing Observations

The command used for this historical gate-timing measurement has been retired.
Current deferred diagnostics are documented in `perf/README.md`.

Current report header and summary:

- `date_utc: 2026-07-07T04:00:59Z`
- `repository_root: /mnt/f/projects/condord_api_last`
- `nextest: cargo-nextest 0.9.138 (fc97e97bb 2026-06-21)`
- `cargo_deny: cargo-deny 0.19.9`
- `output_file_mode: disabled`
- `strict_mode: disabled`
- `timer: /usr/bin/time -p`
- `report_only: true`
- `timing_thresholds: none`
- `commands_run: 15`
- `failed: 0`
- `skipped: 0`

There are no pass/fail timing thresholds. The report is timing-only and machine-local.

## 7. Security and Architecture Invariants

The validated optimization series preserved the following:

- auth collision checks still occur before rate-limit, hooks, debug, and transport side effects
- retry and rate-limit behavior remain bounded
- no-governor fail-closed behavior remains intact for non-empty plans
- debug, hooks, and public errors/source chains do not expose raw auth secrets or body bytes
- pagination remains collect-only
- the optional reqwest split preserves custom transports without reqwest
- macro parser, normalization, sema, and codegen boundaries remain intact

## 8. Open Risks and Next Candidates

Current risks and caveats:

- benchmark numbers are machine-local and will vary by machine, toolchain, and background load
- Criterion does not provide allocation counts by itself
- the optional reqwest feature matrix will need continued maintenance as the crate surface evolves
- the doc-hidden `DefaultTransport` compatibility shim remains part of the reqwest optionality story
- the request-local auth-preparation cache (PERF-PR 18) is implemented for generated retry-stable static credentials; OAuth2 and endpoint/manual credentials remain uncached by design
- sensitive-name matching remains ASCII-case-insensitive by design

Likely future candidates, not implemented here:

- allocation-specific measurement tooling
- broader macro compile-time profiling
- transport streaming/report refinement
- public docs polish after the feature split
- targeted cleanup of doc-hidden compatibility surfaces

## 9. Report-Only Confirmation

This report does not change production behavior, benchmark thresholds, feature flags, or Cargo manifests.
