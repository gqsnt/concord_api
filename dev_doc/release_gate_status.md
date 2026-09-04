# Concord v2.1 release-gate status

Recorded 2026-09-04 for the v2.1 implementation prototype. `Passed` means the
repository supplies the gate's required mechanical evidence. It does not turn
the prototype into a release candidate while any other gate remains
provisional or blocked.

The command-by-command record is in `dev_doc/release_verification_2_1.md`.

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
| 10 — large contract/docs/hygiene | Blocked by external reviewer and license-owner decisions | Clean baseline/candidate check/build/test, no-change and leaf/parent edits, 1–1,000 endpoint expansion/method/binary/description scale, analyzer, executable docs, package, and downstream-artifact checks have measured evidence. No non-primary relevant reviewer was available for generated Rustdoc and parent-diff review. In addition, the repository contains no project license grant, so `cargo deny` correctly rejects the four public packages. See `dev_doc/evidence/large_contract_compile_docs.md`, `dev_doc/evidence/package_verification.md`, and `dev_doc/rustdoc_review/`. |

## Maintained verification

The maintained commands passed in this environment: `just fmt`, `just check`,
`just lint`, `just test`, `just ui`, `just msrv`, `just fixtures`, `just docs`,
`just workflow`, `just analyzer`, and `just bench`. `just deny` failed: the
four public packages have neither an SPDX `license` expression nor a
`license-file`. `cargo deny` reported advisories, bans, and sources as OK and
licenses as FAILED. No allowlist exception was added for Concord itself.

At implementation commit `9470e24`, `just verify` reproduced that exact result
both in the development worktree and the clean detached candidate worktree. It
passed every recipe through `docs` and then stopped at `deny`; the later
workflow/analyzer/bench recipes passed when invoked individually. The
non-failing diagnostics were duplicate-dependency notices for `base64`,
`core-foundation`, `syn`, and `windows-sys`, plus rust-analyzer weak warnings
for inactive/test-only code.

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

This repository is a technically exercised **v2.1 release candidate**, not a
stable-ready release. Stable finalization and merge remain prohibited because
Gate 3 lacks unfamiliar-human participants, Gate 10 lacks an independent
Rustdoc/diff reviewer, and the package license requires a decision by the
project owner. The implementation stays at `2.1.0-rc.1`; no license, waiver,
stable version, or merge is inferred from technical evidence.
