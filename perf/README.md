# Perf Harness

`perf/` is a standalone benchmark package that is excluded from the root workspace on purpose.

Reasons:

- The repository root is a virtual Cargo workspace, so root-level `benches/` would not be discovered reliably.
- Benchmarks are not part of the normal correctness release gate.
- Keeping perf in its own excluded package prevents workspace-wide checks from pulling benchmark-only dependencies into the standard gate.

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
```

Set `CONCORD_PERF_FULL=1` to enable the larger optional fixtures:

```bash
CONCORD_PERF_FULL=1 cargo bench --manifest-path perf/Cargo.toml --bench streaming_upload
CONCORD_PERF_FULL=1 cargo bench --manifest-path perf/Cargo.toml --bench rate_limit_governor
```

Benchmark output is machine-local. Treat it as a comparative signal on one machine and one build, not as universal truth.

Benchmarks report timing only. They are not pass/fail gates for release automation.

The default rate-limit suite measures insertion and acquisition overhead. Active cooldown waiting is intentionally not timed in the default suite because it reflects timer behavior rather than governor lookup overhead. The joined-futures cases are labeled explicitly; the 1,000-future fixture stays behind `CONCORD_PERF_FULL=1`.

Criterion does not provide allocation counts by itself. If allocation measurement is added later, it will need a separate profiler or counting allocator.

Benchmark helpers must stay in-memory and deterministic:

- no live network access
- no real credentials
- no filesystem timing dependencies
