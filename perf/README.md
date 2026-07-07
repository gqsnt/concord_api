# Perf Harness

`perf/` is a standalone benchmark package that is excluded from the root workspace on purpose.

Reasons:

- The repository root is a virtual Cargo workspace, so root-level `benches/` would not be discovered reliably.
- Benchmarks are not part of the normal correctness release gate.
- Keeping perf in its own excluded package prevents workspace-wide checks from pulling benchmark-only dependencies into the standard gate.

The consolidated post-optimization summary lives in [`../docs/perf_post_optimization_report.md`](../docs/perf_post_optimization_report.md).
The allocation-measurement design note lives in [`../docs/allocation_measurement_design.md`](../docs/allocation_measurement_design.md).

Run the allocation-count prototype with:

```bash
cargo bench --manifest-path perf/Cargo.toml --bench allocation_counts
```

This target prints allocation counts for a very small set of hot-path scenarios. The counts are report-only, local to the process, exclude fixture teardown by keeping setup-owned state alive until after the snapshot, and may still include async runtime and library overhead around the measured operation. For consumed-input cases, the measured block owns the consumed input so the report reflects the operation rather than setup teardown.

Run the smoke benchmark with:

```bash
cargo bench --manifest-path perf/Cargo.toml --bench smoke
```

Run future benches with the same pattern:

```bash
cargo bench --manifest-path perf/Cargo.toml --bench <bench_name>
```

Current benchmark entry points:

```bash
cargo bench --manifest-path perf/Cargo.toml --bench smoke
cargo bench --manifest-path perf/Cargo.toml --bench streaming_upload
cargo bench --manifest-path perf/Cargo.toml --bench rate_limit_governor
cargo bench --manifest-path perf/Cargo.toml --bench attempt_pipeline
cargo bench --manifest-path perf/Cargo.toml --bench auth_runtime
cargo bench --manifest-path perf/Cargo.toml --bench pagination
cargo bench --manifest-path perf/Cargo.toml --bench redaction_hooks
cargo bench --manifest-path perf/Cargo.toml --bench streaming_response
cargo bench --manifest-path perf/Cargo.toml --bench allocation_counts
```

Set `CONCORD_PERF_FULL=1` to enable the larger optional fixtures:

```bash
CONCORD_PERF_FULL=1 cargo bench --manifest-path perf/Cargo.toml --bench streaming_upload
CONCORD_PERF_FULL=1 cargo bench --manifest-path perf/Cargo.toml --bench rate_limit_governor
CONCORD_PERF_FULL=1 cargo bench --manifest-path perf/Cargo.toml --bench pagination
CONCORD_PERF_FULL=1 cargo bench --manifest-path perf/Cargo.toml --bench streaming_response
```

Benchmark output is machine-local. Treat it as a comparative signal on one machine and one build, not as universal truth.

Benchmarks report timing only. They are not pass/fail gates for release automation.

The default rate-limit suite measures insertion and acquisition overhead. Active cooldown waiting is intentionally not timed in the default suite because it reflects timer behavior rather than governor lookup overhead. The joined-futures cases are labeled explicitly; the 1,000-future fixture stays behind `CONCORD_PERF_FULL=1`.

The pagination full suite adds 1,000-page offset and cursor collect fixtures. The streaming response full suite adds larger raw-drain, NDJSON, SSE, and multipart fixtures.

Criterion does not provide allocation counts by itself. If allocation measurement is added later, it will need a separate profiler or counting allocator.

Benchmark helpers must stay in-memory and deterministic:

- no live network access
- no real credentials
- no filesystem timing dependencies

## Footprint Report

Run the local dependency and build-footprint report with:

```bash
./scripts/perf_footprint.sh
```

To write the same report to a file as well:

```bash
CONCORD_PERF_OUT=target/perf-footprint.txt ./scripts/perf_footprint.sh
```

Set `CONCORD_PERF_CLEAN=1` for an opt-in clean run before the report. The report is machine-local, report-only, and not part of the release gates.

## Macro Scale Report

Generate temporary macro-scale fixtures under `target/perf-macro-scale/` and measure `cargo check` time with:

```bash
./scripts/perf_macro_scale.sh
```

Optional variants:

```bash
CONCORD_PERF_OUT=target/perf-macro-scale.txt ./scripts/perf_macro_scale.sh
CONCORD_PERF_FULL=1 ./scripts/perf_macro_scale.sh
CONCORD_PERF_CLEAN=1 ./scripts/perf_macro_scale.sh
```

The generated fixtures are local and report-only. By default they remain under `target/perf-macro-scale/` for inspection. `CONCORD_PERF_CLEAN=1` removes old generated fixtures before regenerating the current run. The report is not a release-gate threshold.

## Release-Gate Timing Report

Time the existing local release/check commands with:

```bash
./scripts/perf_gate_timing.sh
```

To write the same report to a file as well:

```bash
CONCORD_PERF_OUT=target/perf-gate-timing.txt ./scripts/perf_gate_timing.sh
```

Set `CONCORD_PERF_STRICT=1` to treat missing optional external tools such as `cargo-nextest` or `cargo-deny` as failures instead of report skips.

This report is machine-local, report-only, and not a release-gate threshold. It does not run benchmarks and should not produce committed timing output.
