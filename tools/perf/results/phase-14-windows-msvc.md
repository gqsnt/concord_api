# Phase 14 Windows MSVC performance and expansion observations

These are observational release measurements, not regression thresholds.
They were collected on:

- Windows 11 10.0.26200, `x86_64-pc-windows-msvc`;
- Intel Core i5-13600K, 20 logical processors;
- 31.8 GiB visible memory;
- rustc/cargo 1.97.1 (`8bab26f4f`, 2026-07-14);
- cargo-expand 1.0.123.

Peak process memory was not captured. No result is generalized to another
platform or machine.

## Compile and Rustdoc

| Operation | Command | Observed wall time | Result |
|---|---|---:|---|
| Isolated clean all-feature façade check | `cargo check -p concord --all-targets --all-features --locked --target-dir target/phase14-final-clean-check-23` | 13.297 s | PASS |
| Incremental rerun of the same check | same command and target directory | 0.212 s (Cargo reported 0.14 s) | PASS |
| Fresh all-feature façade Rustdoc | `RUSTDOCFLAGS="-D warnings" cargo rustdoc -p concord --all-features --locked --target-dir target/phase14-final-rustdoc-23` | 12.986 s | PASS |

The target directories are disposable build artifacts and are not committed.

## Generated expansion scale

Command:

```text
cargo run -p concord-perf --bin expansion_scale
```

The harness generates the same body-free endpoint shape for every size,
changes only the endpoint count, reuses one isolated target directory, and
invokes cargo-expand. Lines, characters, whitespace fragments, and
declaration-shaped lines are reproducible text counters. Timings are warm
after the initial tool/build invocation.

| Endpoints | Lines | Characters | Fragments | Declaration-shaped | Expand time |
|---:|---:|---:|---:|---:|---:|
| 1 | 560 | 25,963 | 1,783 | 74 | 426 ms |
| 10 | 3,242 | 164,203 | 10,819 | 425 | 463 ms |
| 100 | 30,422 | 1,553,263 | 101,539 | 3,935 | 701 ms |
| 1,000 | 302,222 | 15,454,663 | 1,008,739 | 39,035 | 3,617 ms |

The first cold one-endpoint harness verification took 16,182 ms because it
included initial compilation. It is not mixed into the warm scale table.

Phase 0's retained aggregate multi-contract snapshot was 7,254 lines,
409,352 characters, 25,302 fragments, and 581 declaration-shaped lines.
Phase 6's different aggregate fixture was 10,105 lines, 540,178 characters,
34,314 fragments, and 1,366 declaration-shaped lines. Those historical
fixtures are not the controlled scale fixture, so their absolute totals are
retained for context rather than presented as like-for-like percentage
changes.

## One-hundred-iteration runtime observation

PowerShell command:

```powershell
$env:CONCORD_BENCH_ITERATIONS='100'
cargo bench -p concord-perf --bench concord_overhead --locked
```

All 16 named categories executed:

| Category | Total for 100 iterations |
|---|---:|
| Core client construction | 817.8 µs |
| Generated endpoint construction | 28.6 µs |
| Request binding | 9.2 µs |
| Safe planning | 143.7 µs |
| Public-head preparation | 126.4 µs |
| Codec encode/decode | 7.5 µs |
| Body-limit polling | 19.6852 ms |
| Direct execution | 13.0386 ms |
| Explicit generated endpoint execution | 12.5988 ms |
| Runtime endpoint execution | 14.9175 ms |
| Ready credential preparation/finalization | 185.5 µs |
| Ready limiter acquisition | 12.6898 ms |
| Pagination loop (two pages per iteration) | 26.9209 ms |
| Stream wrapping | 4.5 µs |
| Generic specification selection | 70.7 µs |
| Record framing | 9.4 µs |

Execution, limiter, body, and pagination categories use deterministic local
or loopback support. They make no external service requests.

## Final smoke

Command:

```text
just perf-smoke
```

The release benchmark compiled and all 16 categories executed once:

| Category | One iteration |
|---|---:|
| Core client construction | 181.2 µs |
| Generated endpoint construction | 3.0 µs |
| Request binding | 1.1 µs |
| Safe planning | 19.2 µs |
| Public-head preparation | 18.6 µs |
| Codec encode/decode | 2.5 µs |
| Body-limit polling | 5.2693 ms |
| Direct execution | 2.0590 ms |
| Explicit generated endpoint execution | 2.4368 ms |
| Runtime endpoint execution | 2.6529 ms |
| Ready credential preparation/finalization | 24.8 µs |
| Ready limiter acquisition | 2.5122 ms |
| Pagination loop | 2.6050 ms |
| Stream wrapping | 0.6 µs |
| Generic specification selection | 4.7 µs |
| Record framing | 0.6 µs |

The comparable Phase 0 one-iteration observations were 229.8 µs for core
construction, 48.1 µs for public-head preparation, 2.8166 ms for body-limit
polling, 36.5 µs for credential readiness, 2.5978 ms for limiter readiness,
2.3646 ms for pagination, 0.7 µs for stream wrapping, 13.3 µs for generic
specification selection, and 0.8 µs for record framing. Single-iteration
loopback timing is noisy and the benchmark gained direct/explicit/runtime
execution categories, so these values establish observations rather than a
claim of universal improvement or regression.
