# Perf Harness

`perf/` is a standalone benchmark package excluded from the root workspace so
benchmark-only dependencies do not affect ordinary workspace checks.

Current targets measure:

- managed-client construction for `ProtocolRecovery`, `Disabled`, and `Status`;
- visible native execution and allocation counts;
- one bounded authentication recovery and credential caching;
- pagination, streaming upload/response, rate-limit governor/cooldown, and
  redaction hooks.

Run the maintained checks with:

```bash
just perf-check
just perf-test
just bench-check
```

Or run individual targets:

```bash
cargo bench --manifest-path perf/Cargo.toml --bench retry_modes
cargo bench --manifest-path perf/Cargo.toml --bench auth_runtime
cargo bench --manifest-path perf/Cargo.toml --bench allocation_counts
cargo bench --manifest-path perf/Cargo.toml --bench streaming_upload
cargo bench --manifest-path perf/Cargo.toml --bench streaming_response
cargo bench --manifest-path perf/Cargo.toml --bench rate_limit_governor
cargo bench --manifest-path perf/Cargo.toml --bench pagination
cargo bench --manifest-path perf/Cargo.toml --bench redaction_hooks
cargo bench --manifest-path perf/Cargo.toml --bench smoke
```

Benchmarks use deterministic in-process native-executor fixtures: no live
services, real credentials, or network latency measurement. Results measure
local preparation, polling, buffering, and orchestration costs only.
