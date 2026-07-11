# PR 7 Performance Measurement Baselines

Date: 2026-07-08

These are machine-local, report-only measurements. They do not add correctness gates,
pass/fail thresholds, live network calls, or real credentials.

## PR 9: Macro Scale Matrix

Tooling: historical report command, now retired. Current deferred diagnostics are
`just perf-check`, `just perf-test`, and `just bench-check`.

Fixtures are generated under `target/perf-macro-scale` and built with artifacts under
`target/perf-macro-scale-target`, so they are outside the workspace and are not discovered
by normal workspace builds. `cargo-expand` was available through the Windows cargo wrapper
and was used for expanded-size measurements. Each matrix point used its own clean target
directory for the timed check.

| endpoints | scope policy ops | expanded bytes | expanded lines | cold cargo check real |
| ---: | ---: | ---: | ---: | ---: |
| 5 | 2 | 102,674 | 2,357 | 0.52s |
| 5 | 10 | 130,674 | 2,877 | 0.54s |
| 20 | 2 | 279,978 | 6,209 | 0.60s |
| 20 | 10 | 391,978 | 8,289 | 0.66s |
| 50 | 2 | 638,569 | 14,005 | 0.83s |
| 50 | 10 | 918,569 | 19,205 | 0.99s |

Decision rule: proceed with PR 9 if expanded size or build time at 50x10 is materially
super-linear vs the 5x2 baseline scaled by endpoint count, for example more than 25%
above the linear projection. Otherwise defer PR 9.

Decision: defer. Expanded size was measurable, and the decision rests on expanded bytes:
50x10 is below the 5x2 endpoint-scaled projection (918,569 bytes actual vs 1,026,740
projected). Cold per-point build time is also below the endpoint-scaled projection
(0.99s actual vs 5.20s projected). The corrected build-time column is monotonic with
fixture scale and no longer shows the warm-cache artifact from the earlier run.

## PR 10: `syn` `extra-traits`

Tooling: temporary `concord_macros/Cargo.toml` edit, reverted before finishing.

The experiment removed `extra-traits` from the `syn` feature list on a throwaway basis.
`concord_macros` compiled as-is without compiler errors, so no `Debug`, `Eq`, `Hash`, or
`PartialEq` users were identified by the compiler.

| configuration | command | real time |
| --- | --- | ---: |
| with `extra-traits` | `cargo check -p concord_macros` | 11.64s |
| without `extra-traits` | `cargo check -p concord_macros` | 11.66s |
| with `extra-traits` | small generated fixture check | 0.30s |
| without `extra-traits` | small generated fixture check | 0.33s |

Decision rule: proceed if the feature can be removed with localized changes and
clean-build saves measurable time. Otherwise document-and-accept.

Decision: document-and-accept. Removal appears source-compatible, but this local run did
not show a measurable clean-build win.

## PR 11: Gate Timing

Tooling: historical report command, now retired.

The historical timing helper mirrored the former release command and recorded per-step wall time with no
thresholds.

| step | real time |
| --- | ---: |
| architecture boundary | 4.29s |
| feature matrix | 4.10s |
| format check | 0.62s |
| clippy workspace all targets | 0.37s |
| macro integration tests | 1.09s |
| macro generated tests | 0.86s |
| macro trybuild current | 39.59s |
| macro trybuild sema | 5.36s |
| macro trybuild codegen | 2.88s |
| core tests | 3.48s |
| core tests all features | 4.99s |
| examples tests | 3.01s |
| examples tests all features | 0.60s |
| workspace tests | 51.08s |
| workspace tests all features | 51.07s |
| workspace all-target tests | 51.00s |
| rustdoc warnings denied | 3.89s |

Total timed wall time: 228.29s.

Redundant per-crate subset steps counted for the PR 11 decision: macro integration,
macro generated, macro trybuild current/sema/codegen, core tests, core tests all
features, examples tests, and examples tests all features. These total 61.86s, or 27.1%
of the timed gate.

Decision rule: proceed if the redundant per-crate steps are approximately 20% or more of
total gate wall time. Otherwise defer.

Decision: proceed. The redundant per-crate share was 27.1% on this machine-local run.

## PR 8: URL Rebuild Isolation

Tooling: `cargo bench --manifest-path perf/Cargo.toml --bench attempt_pipeline -- --noplot`

The scratch hoist built the fully populated base `Url` once at the `drive_attempts`
entry point, passed `&Url` into the per-attempt request builder, and cloned it for each
attempt. The scratch edit compiled, was benchmarked, and was reverted afterward.

| benchmark | baseline mean | hoist mean | change |
| --- | ---: | ---: | ---: |
| `mock_transport_success/minimal_get` | 1.4029 us | 1.4150 us | +0.9% |
| `bearer_auth` | 1.8443 us | 1.9025 us | +3.2% |
| `query_auth` | 2.1288 us | 2.1827 us | +2.5% |
| `many_query_params/32` | 6.3294 us | 6.4307 us | +1.6% |
| `retry_once_then_success` | 2.5525 us | 2.2682 us | -11.1% |

Decision rule: proceed if the hoist shows a measurable improvement beyond noise on
retry/pagination scenarios and does not regress single-attempt scenarios. Otherwise defer.

Decision: defer. The retry scenario improved, but `many_query_params/32` regressed and
the single-attempt `minimal_get` path did not improve. The decision rule therefore is not
satisfied.

## Historical PR 13: CSV Footprint

Tooling: historical report command, now retired.

The historical no-default dependency tree contained both `csv` and `csv-core` for CSV
record support. The historical decision considered feature-gating that support to
reduce the dependency tree. PR-01 instead removed CSV record support and both
dependencies completely.

| measurement | value |
| --- | ---: |
| no-default unique dependency lines | 68 |
| historical no-default includes `csv` | yes |
| historical no-default includes `csv-core` | yes |
| current no-default includes `csv` | no |
| current no-default includes `csv-core` | no |
| `cargo check -p concord_core --no-default-features` | 5.09s |
| `cargo check -p concord_core --no-default-features --features json` | 6.80s |
| `cargo check -p concord_core --all-features` | 6.07s |

Decision rule: proceed if gating meaningfully reduces the no-default compile time or
dependency count. Otherwise document-and-accept.

Historical decision: consider feature-gating CSV record support. PR-01 removed the
support and dependencies completely, so the current no-default tree contains neither
dependency and no future feature-gating action remains. A true compile-time
before/after remains historical PR-13 implementation validation.

## Raw Outputs

Raw report outputs were captured under `target/perf-reports/` and are intentionally not
part of the committed diff.
