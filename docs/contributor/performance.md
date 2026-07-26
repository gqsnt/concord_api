# Performance and scale

Concord's performance tooling provides repeatable local observations, not
release thresholds. It measures public façade operations and deterministic
loopback execution so a maintainer can compare one change with another without
adding benchmark-only product APIs.

## What the probes own

`concord-perf` is a non-published workspace tool. Its product imports come
through `concord`; deterministic HTTP comes from the benchmark-only
`concord-test-support` dependency. It is not in the production dependency
graph.

There are two complementary probes:

| Probe | Measures | Does not establish |
|---|---|---|
| `concord_overhead` | runtime construction, planning, codec, body, execution, authentication, limiting, pagination, streaming, and record operations | latency SLO, throughput capacity, production network behavior |
| `expansion_scale` | generated expansion size and local expansion elapsed time for growing endpoint counts | compiler-wide regression threshold, downstream incremental-build time |

The implementations and their command-specific notes remain in
`tools/perf/README.md`, `tools/perf/benches/concord_overhead.rs`, and
`tools/perf/src/bin/expansion_scale.rs`.

## Runtime lifecycle probe

Compile the maintained benchmark owner without running it:

```text
just bench
```

Run a bounded smoke observation:

```text
cargo bench --locked -p concord-perf --bench concord_overhead -- --smoke
```

Run its default observation:

```text
cargo bench --locked -p concord-perf --bench concord_overhead
```

The harness is a normal executable rather than the unstable built-in benchmark
harness. `--smoke` forces one iteration. Otherwise
`CONCORD_BENCH_ITERATIONS` selects from 1 through 1,000, with 100 as the
default.

The executable audits that all fifteen distinct categories ran:

1. core client construction;
2. generated endpoint construction;
3. request binding;
4. safe public planning;
5. codec encoding and decoding;
6. body-limit polling;
7. direct execution overhead;
8. explicit generated endpoint execution overhead;
9. runtime endpoint execution overhead;
10. authenticated managed execution;
11. rate-limited managed execution;
12. pagination loop overhead;
13. stream wrapping;
14. generic specification selection;
15. record framing.

Each timer stops only after its named operation completes. Planning uses
`client.plan(...)` and the real credential-sanitized public-head path.
Managed-execution categories use generated requests, the ordinary lifecycle,
and bounded local transport. There is no hidden fast-path executor.

## Expansion-scale probe

Install `cargo-expand` for the toolchain being measured, then run:

```text
cargo run --locked -p concord-perf --bin expansion_scale
```

The default endpoint counts are 1, 10, 100, and 1,000. Pass explicit counts in
the supported 1 through 1,000 range to select another set:

```text
cargo run --locked -p concord-perf --bin expansion_scale -- 10 100
```

For each count the tool writes an isolated disposable source under
`target/perf`, invokes the installed `cargo-expand`, and prints a Markdown
table containing:

- expanded lines;
- characters;
- whitespace-delimited fragments;
- declaration-shaped Rust items;
- local expansion elapsed milliseconds.

The source, expanded output, and build state under `target/` are review
artifacts. Do not commit them.

## Comparing observations

Change one variable at a time. Record at least:

- exact branch and commit;
- clean or intentionally dirty worktree;
- operating system and architecture;
- Rust toolchain and Cargo version;
- debug/release profile selected by the command;
- feature set;
- benchmark iteration count or expansion endpoint counts;
- machine power/thermal state when it could affect the result.

Compare the same machine, toolchain, profile, feature set, and counts. Warm-up,
background load, allocator behavior, loopback scheduling, and compiler caches
can move one observation. Repeat suspected regressions and inspect the
distribution rather than treating one elapsed duration as a product fact.

Do not commit timing snapshots or add a pass/fail threshold without a separately
approved performance policy, stable measurement environment, variance study,
and named owner.

## Reading runtime results

The categories intentionally separate construction from execution. If a
change affects one stage:

- compare client construction for runtime configuration and shared-state
  changes;
- compare endpoint construction and binding for generated wrapper changes;
- compare safe planning for public-head shaping and validation;
- compare direct, explicit, and runtime endpoint execution together to detect
  divergent paths;
- compare authentication, limiting, and pagination against direct execution
  to localize policy overhead;
- compare body-limit polling and record framing for finite-progress changes.

Absolute loopback time is not an estimate of internet request latency. The
probe is useful because it removes that external noise and exercises the
semantic work Concord owns.

## Reading expansion results

Endpoint count should be evaluated against all printed size measures, not only
wall time. A faster cached expansion can still produce substantially more Rust
for the compiler to parse and analyze. Inspect generated structure when
declaration-shaped item count or characters change unexpectedly.

`cargo-expand` itself is an external development tool and is not pinned by the
workspace lockfile. Record its version. A missing installation makes the
expansion probe unavailable; it is not a product failure and must not be
reported as a pass.

## Performance change hazards

Review runtime work for:

- allocation or cloning added per attempt or page;
- formatting of public values before a bound is known;
- locks held across extension callbacks or external I/O;
- state maps whose cardinality is not tied to a finite policy;
- repeated body conversion or authentication materialization;
- scans whose cost grows with retained pages, keys, headers, or records;
- a generated/runtime code-path split hidden by testing only one surface.

Review compile-time work for:

- repeated parsing or resolution of the same declaration;
- expansion proportional to inherited scopes times endpoints;
- duplicated generated docs or helper implementations;
- deterministic names replaced by larger per-use structures;
- feature branches that emit unused code.

Performance does not override security, redaction, finite behavior, or
correctness. Optimize behind the same semantic boundary and keep the owning
tests passing.

## Verification

Before interpreting timings:

1. run the relevant correctness owner;
2. compile the benchmark with `just bench`;
3. run `--smoke` and confirm the 15/15 category audit;
4. run the chosen repeated observation;
5. rerun the baseline under the same conditions;
6. record any missing expansion tool or environmental limitation.

Benchmark compilation is part of `just verify`; timing execution and
`expansion_scale` are deliberate manual observations. CI does not claim a
performance threshold.

## Related

- [Managed executor](architecture/managed-executor.md)
- [Compile-time to runtime flow](architecture/compile-runtime-flow.md)
- [Testing strategy](testing.md)
- [Release process](release.md)
