# Concord v2.1 release-gate status

Recorded 2026-09-04 for the v2.1 implementation prototype. `Passed` means the
repository supplies the gate's required mechanical evidence. It does not turn
the prototype into a release candidate while any other gate remains
provisional or blocked.

| Gate | Status | Evidence and remaining boundary |
|---|---|---|
| 1 — representation trait/driver | Passed | Rust 1.86, workspace/UI tests, downstream multi-pair/custom representation tests, sealed-driver privacy tests, and the complete built-in response matrix pass. Ordinary output does not expose decoder/driver types. |
| 2 — representation inheritance | Passed | Client/scope defaults, directional overrides, inline exceptions, explicit fences, invalid pairs, provenance descriptions, and semantic diff are compile/runtime tested. Human discoverability belongs to Gate 3. |
| 3 — DSL/generated DX study | Blocked by environment | The selected word language and exact control matrix are implemented, but no unfamiliar relevant Rust participants were available. `with_x` and cognitive freeze remain provisional. |
| 4 — query/header/atom properties | Passed | Alias-neutral value tests, one-pass actual/sanitized atom emission, patch algebra, ordered repetition, case-insensitive headers, collision rejection, first-excess bounds, and sensitive noninterference pass. |
| 5 — normative runtime trace | Passed | Deterministic direct/body/retry/auth-recovery/physical-recovery/observer/cancellation suites prove one executor, one deadline, stage order, dispatch knowledge, and non-result-determining observation. |
| 6 — pagination proof suite | Passed | Generated and core controller tests cover offset/page/cursor/header/relative-Link paths, safe/complete capability gates, Prefix/CompleteWithin, cross-origin/cycle/progress limits, aggregate deadline/budgets, per-page preparation, retry, cancellation, and all-or-error output. |
| 7 — rate/server-delay behavior | Passed | Canonical framed keys, stable IDs, conjunctive whole-plan admission, explicit limiter selection, reacquisition boundaries, parsed server delay, future-only feedback, cooldown bounds/eviction, and redaction pass. |
| 8 — stream lifecycle/performance | Provisional | Lazy lifecycle, post-return errors, bounds, backpressure, cancellation, and stream terminal evidence pass. The maintained 100-operation runtime probe passed, but it does not yet compare monomorphized, boxed, and enum-erased drivers or allocation/code-size across target workloads. |
| 9 — information-flow security | Passed | Sentinel suites cover public/sensitive/private values, bodies, output, credentials/provider refresh, rate keys, pagination identities, errors/debug/plans/events, native target separation, and runtime binding/security-context constraints. |
| 10 — large contract/docs/hygiene | Provisional | Version 2.1.0, one active language, MSRV/features/docs/analyzer/fixtures/migration checks pass. Expansion measurements passed for 1/10/100/500/1,000 endpoints (1,054/5,788/53,488/265,488/530,488 lines); clean/incremental memory, binary-size, generated-Rustdoc human clarity, and parent-change human review remain incomplete. |

## Maintained verification

The individual maintained commands passed in this environment: `just fmt`,
`just check`, `just lint`, `just test`, `just ui`, `just msrv`, `just fixtures`,
`just docs`, `just deny`, `just workflow`, `just analyzer`, and `just bench`.
The aggregate `just verify` completed with exit code 0 on 2026-09-04. Its
non-failing diagnostics were the permitted duplicate-dependency notices for
`base64`, `core-foundation`, `syn`, and `windows-sys`, plus rust-analyzer weak
warnings for inactive/test-only code.

Additional evidence commands:

- `cargo bench -p concord-perf --bench concord_overhead -- --smoke`: 17/17
  named categories executed.
- `cargo bench -p concord-perf --bench concord_overhead`: 17/17 categories,
  100 iterations each; managed retry performed exactly 200 physical sends for
  100 logical operations.
- `cargo run -p concord-perf --release --bin expansion_scale`: passed after a
  workspace-local `cargo-expand` install; 1/10/100/500/1,000 points completed.
- `cargo run -p concord-xtask -- migration-audit`: passed read-only and emitted
  19 intentional review markers for exact opaque `Option`/`Vec` payloads; no
  retired syntax was rewritten or silently guessed. String/comment spellings
  are masked and do not produce false migration findings.

## Release decision

This repository is a coherent **v2.1 implementation prototype**. It is not a
release candidate: Gate 3 is blocked and Gates 8 and 10 remain provisional.
