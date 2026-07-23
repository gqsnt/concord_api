# Phase 10 macro architecture evidence

Phase 10 is a behavior-preserving compiler reorganization. The public
declaration language, generated Rust surface, façade bridge calls, and runtime
behavior are unchanged. The final expansion is byte-for-byte identical to the
Phase 9 baseline.

## Compiler ownership

The macro now has one explicit directional pipeline:

```text
source tokens
    → syntax AST
    → normalized contract
    → generated surface
    → artifact emitters
    → concord::__private
    → concord-core lifecycle
```

`lib.rs` owns the only parse → resolve → emit entry.

### Syntax

`syntax/ast.rs` is the only parsed source representation. Domain parsers own
declarations, values and explicit bodies, request shaping, credentials,
pagination, rate limits, keywords, and diagnostics. Clause spans and
duplicate-clause combination behavior were retained exactly.

The syntax layer has colocated tests for:

- client/scope/endpoint structure;
- required, syntactically optional, defaulted, and explicit body values;
- order-independent request clauses;
- secret, built-in source, and opaque custom-provider declarations;
- opaque pagination controller types and ordered initializers;
- finite rate-limit windows and cost;
- both spans of a duplicate clause.

### Semantics and surface

`semantic/model.rs` is the only normalized contract. The root resolver performs
tree orchestration; domain modules own:

- `symbols.rs`: value shapes, namespaces, collisions, and reference resolution;
- `inheritance.rs`: host/path/query/header/auth/timeout/rate operation
  normalization;
- `auth.rs`: credential sources, compatibility, effective placement, and
  dependency cycles;
- `pagination.rs`: binding ownership and normalized controller fields;
- `rate_limits.rs`: lexical composition, finite windows, and stable rule
  identities;
- `surface.rs`: all generated names, module paths, docs identity, setters,
  clone eligibility, and terminal capabilities.

Each domain has normalized-model tests that inspect semantic records rather
than formatted tokens. Production semantic code imports neither `quote` nor a
token stream and performs no emission.

### Emission

Emitters are split by generated artifact:

| Module | Owned artifact |
|---|---|
| `client.rs` | client snapshots and named builders |
| `scopes.rs` | scope modules and facades |
| `endpoints.rs` | explicit endpoint values and bindings |
| `requests.rs` | named request wrappers and inherent terminals |
| `credentials.rs` | typed credential facades and handles |
| `descriptions.rs` | safe semantic endpoint descriptions |
| `docs.rs` | generated Rustdoc |
| `assertions.rs` | private specification aliases and semantic trait bounds |
| `bridge.rs` | generated contract version/assertion |
| `shaping.rs` | normalized public-head construction |
| `helpers.rs` | small private-name and token helpers |

Every listed public artifact has a local signature/capability test. Emitters
consume the normalized model and surface; a production scan finds no
`syn::Result`, `syn::Error`, or new user-validation path under `emit/`.

The caller-visible façade continues to be resolved in `crate_path.rs`.
Generated paths use `concord::__private`, including dependency renames. The
generated contract remains version 22 because its call contract did not
change.

## Public-output equivalence

The same PowerShell `Out-String` capture and counters used for the Phase 9
`concord-macros/tests/generated_surface.rs` baseline produced:

| Metric | Phase 9 | Phase 10 | Change |
|---|---:|---:|---:|
| SHA-256 | `326b52bcec1b27b6cb9dd7f91b63dd0d67b93f552acd7a19a2e6998be6d912ce` | identical | none |
| Expanded lines | 12,566 | 12,566 | 0 |
| Characters | 665,616 | 665,616 | 0 |
| Characters without newlines | 640,484 | 640,484 | 0 |
| Whitespace-delimited fragments | 39,294 | 39,294 | 0 |
| Declaration-shaped lines | 1,536 | 1,536 | 0 |

This exact hash also proves that moving description construction and the
contract assertion into local emitters did not alter generated tokens,
signatures, module topology, docs text, capabilities, or bridge paths.

The renamed-façade expansion remains:

- 30 `::runtime::__private` paths;
- two `pub(super) use ::runtime as __core` aliases;
- zero direct `::concord_core` paths;
- zero concrete extras implementation names.

## Compilation measurement

An isolated clean `cargo check -p concord-macros --lib --quiet` used a fresh
target directory on the same Windows/Rust 1.97.1 machine:

| Metric | Phase 9 baseline | Phase 10 | Change |
|---|---:|---:|---:|
| Clean macro-library check | 7.142 s | 7.111 s | -0.031 s (-0.43%) |

Peak memory was not measured because no suitable recorder was available. The
exact expansion is unchanged, so emitted item/token growth is zero.

## Tests and diagnostics

The compiler library now has 61 unit tests. They include colocated syntax,
semantic, surface, and emitter tests while retaining cross-domain normalized
contract tests.

The complete 27-group macro UI suite and façade UI suite pass without changing
any `.stderr` snapshot. This preserves clause spans, duplicate diagnostics,
public vocabulary, capability absence, privacy failures, and dependency-rename
behavior.

Two supplemental source-policy tests originally enumerated the former flat
`ast.rs`/`model.rs`/`codegen.rs` paths. The first complete macro run therefore
failed when it attempted to open a deliberately removed path. The tests were
updated to enumerate production Rust source recursively and to inspect the
single syntax model, normalized model, and artifact emit directory. Their
compiler/runtime proofs were unchanged, and the complete suite passed on the
rerun. Phase 12 still owns removal of source-scan proofs where compiler
privacy/type-neutrality tests supersede them.

## Contributor documentation

`docs/developer/macro-compiler.md` now documents:

- source tokens → syntax AST → normalized contract → generated surface →
  artifact tokens → façade bridge → core lifecycle;
- the exact validation owner at every boundary;
- how to add a clause without introducing another model;
- how to add a capability through `semantic::surface`;
- emitter, bridge, dependency-rename, diagnostic, and review rules.

The repository architecture and current generated-contract/path references
were updated to the façade/extras graph and contract version 22.

## Commands and observed results

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo test -p concord-macros --quiet` | PASS; 61 library tests, 17 compiler/graph/Rustdoc fixtures, all generated runtime suites, 9 dependency-policy tests, 27 UI groups, and doctests |
| `cargo test -p concord-macros --test ui --quiet` | PASS; all 27 UI groups |
| `cargo test -p concord --test ui --all-features --quiet` | PASS; all façade compile-pass/fail groups |
| `cargo test -p concord --test generated_surface --all-features --quiet` | PASS |
| `cargo test -p concord --all-features --quiet` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS |
| `cargo test -p concord --test endpoint_planning --all-features --quiet` | PASS; five description/planning/lifecycle proofs |
| `cargo test -p concord-core --all-features --test deterministic_redaction --test error_redaction --test hook_redaction --test proxy --test sanitized_metadata --test status_body_contract --quiet` | PASS; 17 seeded redaction/status-body tests |
| `cargo test -p concord-core --test public_rustdoc --all-features --quiet` | PASS; two public-surface tests |
| `cargo test --workspace --doc --quiet` | PASS |
| `RUSTDOCFLAGS='-D warnings' cargo doc -p concord --all-features --no-deps` | PASS; the localized MSVC proc-macro linker message remained |
| `cargo +1.85.0 test -p concord-macros --lib --quiet` | PASS; all 61 compiler tests |
| `cargo +1.85.0 check --workspace --all-targets --all-features` | PASS |
| `cargo tree -p concord-macros -e normal --depth 1` | PASS; only `http`, proc-macro parsing/emission crates, and `url`; no Concord runtime/extras/façade edge |
| renamed-façade offline/locked expansion counters | PASS; 30 private paths, two aliases, zero direct-core paths, zero concrete extras names |
| `cargo expand -p concord-macros --test generated_surface` plus Phase 9 counters/hash | PASS; exact byte-for-byte baseline match |
| isolated clean `cargo check --target-dir target/phase10-final -p concord-macros --lib --quiet` | PASS; 7.111 s |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | UNAVAILABLE; `cargo-clippy.exe` is not applicable to the installed Rust 1.97.1 toolchain |
| `rust-analyzer --version` | PASS; `rust-analyzer 1.97.1 (8bab26f4 2026-07-14)` |
| `git diff --check` | PASS |

The interactive rust-analyzer completion/hover/navigation protocol was not
executed in an editor and remains a manual release gate. The compiler, façade
surface, Rustdoc, dependency-rename, and exact-expansion gates passed; they are
not reported as a substitute for that manual interaction.

## Deviations and remaining blockers

No selected design decision or product invariant required a deviation.

Clippy remains unavailable for the installed toolchain. The manual
rust-analyzer protocol remains unexecuted. The localized MSVC linker message is
an observed toolchain warning, not a Rust source warning. No snapshot was
blindly updated.
