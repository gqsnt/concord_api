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

Benchmark output is machine-local. Treat it as a comparative signal on one machine and one build, not as universal truth.

Criterion does not provide allocation counts by itself. If allocation measurement is added later, it will need a separate profiler or counting allocator.

Benchmark helpers must stay in-memory and deterministic:

- no live network access
- no real credentials
- no filesystem timing dependencies
