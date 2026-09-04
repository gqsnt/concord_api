# Concord v2.1 release-gate status

Recorded 2026-09-04 for the v2.1 implementation prototype. `Passed` means the
repository supplies the gate's required mechanical evidence. It does not turn
the prototype into a release candidate while any other gate remains
provisional or blocked.

| Gate | Status | Evidence and remaining boundary |
|---|---|---|
| 1 — representation trait/driver | Passed | Rust 1.86, workspace/UI tests, downstream multi-pair/custom representation tests, sealed-driver privacy tests, and the complete built-in response matrix pass. Ordinary output does not expose decoder/driver types. |
| 2 — representation inheritance | Passed | Client/scope defaults, directional overrides, inline exceptions, explicit fences, invalid pairs, provenance descriptions, and semantic diff are compile/runtime tested. Human discoverability belongs to Gate 3. |
| 3 — DSL/generated DX study | Blocked by external participant availability | The selected word language and exact control matrix are implemented, but no unfamiliar relevant Rust participants were available. Per the mandated fallback, `with_x` was removed; cognitive comprehension evidence is still absent. The ready study is in `dev_doc/dx_study/`. |
| 4 — query/header/atom properties | Passed | Alias-neutral value tests, one-pass actual/sanitized atom emission, patch algebra, ordered repetition, case-insensitive headers, collision rejection, first-excess bounds, and sensitive noninterference pass. |
| 5 — normative runtime trace | Passed | Deterministic direct/body/retry/auth-recovery/physical-recovery/observer/cancellation suites prove one executor, one deadline, stage order, dispatch knowledge, and non-result-determining observation. |
| 6 — pagination proof suite | Passed | Generated and core controller tests cover offset/page/cursor/header/relative-Link paths, safe/complete capability gates, Prefix/CompleteWithin, cross-origin/cycle/progress limits, aggregate deadline/budgets, per-page preparation, retry, cancellation, and all-or-error output. |
| 7 — rate/server-delay behavior | Passed | Canonical framed keys, stable IDs, conjunctive whole-plan admission, explicit limiter selection, reacquisition boundaries, parsed server delay, future-only feedback, cooldown bounds/eviction, and redaction pass. |
| 8 — stream lifecycle/performance | Passed | `stream_driver_comparison` compares static, private enum, and boxed drivers for construction, allocation, byte/item throughput, lifecycle, compile time, and binary size. Boxed erasure meets every precommitted budget; runtime record-stream tests cover the public lifecycle. See `dev_doc/evidence/stream_driver_comparison.md`. |
| 9 — information-flow security | Passed | Sentinel suites cover public/sensitive/private values, bodies, output, credentials/provider refresh, rate keys, pagination identities, errors/debug/plans/events, native target separation, and runtime binding/security-context constraints. |
| 10 — large contract/docs/hygiene | Blocked by external reviewer availability | Clean baseline/candidate check/build/test, no-change and leaf/parent edits, 1–1,000 endpoint expansion/method/binary/description scale, analyzer, executable docs, package, and downstream-artifact checks have measured evidence. No non-primary relevant reviewer was available for generated Rustdoc and parent-diff review. See `dev_doc/evidence/large_contract_compile_docs.md` and `dev_doc/rustdoc_review/`. |

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

This repository is a coherent **v2.1 release candidate**. Stable finalization
and merge remain prohibited because the same external limitation leaves Gate 3
and the human-review portion of Gate 10 blocked: no independent relevant Rust
participants/reviewer were available.
