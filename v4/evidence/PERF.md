# Performance and size measurements

Machine: Intel i5-13600K (20 threads), 30 GiB RAM, Linux 7.0.0-42 (Ubuntu), rustc 1.97.1, no
`sccache`/`RUSTC_WRAPPER`. Background load ≈ 1.7–2.1 (1-minute load average) during runs. Source:
branch `concord-v4` at `bfb4870` plus the punycode-label fix (no runtime change). Single machine,
single session: treat differences of a few percent as noise.

The comparison baseline is `benches/baselines`: idiomatic hand-written reqwest wrappers for the same
three API shapes (S1 simple, S2 regional subdomain + nested scopes + shared headers, S3 nine optional
query parameters + JSON POST + DELETE), written during research independently of the Concord code
(`evidence/research/claude_ergonomics.md`). Concord renderings: `examples/shapes`. Both produce the
same requests (wire tests in each crate).

## 1. Runtime

Command: `cargo run --release -p concord-perf` (`benches/perf/src/main.rs`). Each row: median of 9
timed runs after 1 warm-up run; each run times N iterations and divides.

| Case | Concord | Hand-written | Raw reqwest | N per run |
|---|---|---|---|---|
| Build S3 search request, 9 parameters set (no I/O) | 740 ns (min 738, max 742) | 626 ns (623–644) | — | 200,000 |
| Build S2 product request: 2 scopes, subdomain, 3 headers (no I/O) | 779 ns (778–779) | 528 ns (527–529) | — | 200,000 |
| S1 `GET /v1/health` + JSON decode over loopback keep-alive | 9.02 µs (8.97–9.08) | 8.97 µs (8.95–9.14) | 8.99 µs (8.98–9.04), bytes only | 3,000 |

Reading: building a request costs Concord about 110–250 ns more than a hand-written builder — the
generated code clones the parent handles into the endpoint value, formats every value through
`Display` into owned `String`s, validates header bytes and labels, and clones the `reqwest::Client`
`Arc` into `Prepared`. Executing a request is dominated by reqwest/hyper; over loopback the three
variants are within each other's run-to-run spread (~0.1 µs). No claim beyond that is made.
An earlier run (`perf_run1`, same numbers ±2%) used a benchmark server that wrote headers and body
separately and stalled ~40 ms per request on Nagle/delayed ACK; that harness bug was fixed
(`TCP_NODELAY`, single write) before the execution figures above were taken.

## 2. Compile time

### Like-for-like shapes (incremental)

`hyperfine -N --warmup 1 --runs 7`, touching every source file of the crate before each run,
dependencies prebuilt, dev profile:

| Crate | Mean ± σ |
|---|---|
| `concord-example-shapes` (`cargo build -q -p concord-example-shapes`) | 143.0 ms ± 4.1 |
| `concord-baselines` (`cargo build -q -p concord-baselines`) | 120.8 ms ± 3.7 |

### Scaling with declaration size

`benches/scale/gen.py N` generates one `api!` with N endpoints in N/10 scopes (each endpoint: one
path value, two optional and one defaulted query value; each scope: one path value, one optional
header). `hyperfine -N --warmup 1 --runs 5 --prepare 'touch src/lib.rs'`, deps prebuilt, dev profile,
`concord` with `default-features = false`:

| Endpoints | Declaration lines | `cargo check` | `cargo build` | Expanded lines (rustfmt) |
|---|---|---|---|---|
| 50 | 71 | 126.7 ms ± 3.7 | 147.5 ms ± 1.4 | 7,221 |
| 200 | 266 | 314.6 ms ± 3.3 | 348.5 ms ± 7.2 | 28,671 |
| 1,000 | 1,306 | 1.357 s ± 0.022 | 1.526 s ± 0.013 | not measured |

Expansion (`cargo +nightly rustc -- -Zunpretty=expanded`, then `rustfmt`, non-blank lines) is
≈ 143 generated lines per endpoint and grows linearly; incremental check time grows ≈ 1.2 ms per
endpoint. This is the cost Codex's blind review predicted for per-endpoint specialization (D1):
linear, and modest at these sizes; no superlinear behaviour was observed up to 1,000 endpoints.
Release profile (`cargo build --release`, incremental after touching the source, 3 runs):

| Endpoints | Release build | `.rlib` size |
|---|---|---|
| 200 | 3.410 s ± 0.013 | 4.4 MB |
| 1,000 | 17.026 s ± 0.082 | 21.9 MB |

Release builds cost ≈ 17 ms per endpoint for the crate that declares the API, because every
generated (non-generic) method is compiled in that crate whether or not it is called. Marking
generated methods `#[inline]` would defer code generation to call sites; this was **not** tried or
measured (open item in `STATUS.md`).

### Sensitivity propagation (Codex C4-R-011)

A declaration whose N+1 values are chained through templated query keys
(`"k{i}" = "{v_i}-{v_i+1}"`) and end in `"authorization" = v_N`, so every value becomes
withheld one hop at a time. Incremental `cargo check`, `hyperfine -N --warmup 1 --runs 3`,
`target/measure/sens/`:

| N | before (`1b37a1e`, re-composition fixpoint) | after (`1bf83f0`, worklist) |
|---|---|---|
| 400 | 0.320 s ± 0.007 | 0.120 s ± 0.006 |
| 800 | 1.729 s ± 0.016 | 0.175 s ± 0.002 |
| 1,600 | 9.740 s ± 0.061 | 0.350 s ± 0.004 |

### Clean builds

Fresh target directory each time, dev profile, one run each (`target/measure/clean.txt`):

| Build | Wall time |
|---|---|
| `concord --no-default-features --features json` (reqwest without TLS) | 5.15 s |
| `concord` default features (reqwest defaults: rustls + aws-lc-rs, HTTP/2, …) | 9.98 s |
| `concord-baselines` (reqwest `json` + `query`, no TLS) | 4.38 s |
| V3 reference, all default members (evidence/v3_baseline.md) | 24.30 s |

The ≈ 0.8 s difference between Concord without TLS and the baseline is mostly the proc-macro
dependencies (`syn` 3 with `full`). The default-feature cost is reqwest's TLS stack, which an
application using reqwest pays anyway.

## 3. Code size

Effective lines (non-blank, non-comment):

| Component | V4 | V3 (a492d36) |
|---|---|---|
| Runtime | 804 (`crates/concord/src`) | 79,809 (`concord-core`) + 6,495 (`concord-extras`) + 225 (facade) |
| Compiler | 2,654 (`crates/concord-macros/src`) | 14,354 |

Shape sources (effective lines, including DTOs and enum spellings on both sides):

| Shape | Concord (`examples/shapes`) | Hand-written (`benches/baselines`) |
|---|---|---|
| S1 | 18 | 49 |
| S2 | 52 | 130 |
| S3 | 58 | 182 |

## Not measured

Allocation counts, HTTP/2, TLS handshake cost, Windows/macOS, rust-analyzer responsiveness, the
effect of `#[inline]` on generated methods.
