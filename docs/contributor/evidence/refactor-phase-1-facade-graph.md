# Phase 1 façade and crate-graph validation

Phase 1 was implemented from baseline commit `bdc9717` with Rust and Cargo
1.97.1. Phase 0 and Phase 1 are committed together by explicit implementation
instruction.

## Resulting transitional graph

```text
concord -> concord_core + concord_macros + concord_extras
concord_extras -> concord_core
```

All four sibling edges use `=0.1.0` and `default-features = false`.
`concord_macros` has no normal Concord runtime dependency. Generated code first
resolves the caller-visible `concord` package, including a dependency rename,
and emits through `concord::__private`. The direct `concord_core` resolver
fallback remains only for internal legacy tests and is scheduled for deletion
in Phase 14.

The façade mirrors the transitional core feature set exactly. Its default
features remain `default-tls`, `http2`, and `rate-limit-governor`; JSON remains
opt-in. A workspace check exposed and fixed an accidental-unification issue:
façade feature assertions no longer reexport a core item whose presence changes
when another workspace package enables core JSON.

## Commands and observed results

| Command | Result |
|---|---|
| `cargo check -p concord --all-targets` | PASS |
| `cargo check -p concord --all-targets --all-features` | PASS |
| `cargo test -p concord` | PASS; the one-dependency generated-surface test passed |
| `cargo test -p concord --all-features` | PASS |
| `cargo test -p concord --no-default-features` | PASS |
| `cargo test -p concord_extras --all-features` | PASS; skeleton unit and doc suites contain no tests |
| `cargo test -p concord_macros` | PASS after the reviewed Rust 1.97 Trybuild snapshot update described below; 25 UI groups passed and doctests passed |
| `cargo test -p concord_macros --test dependency_policy` | PASS; 6 tests |
| `cargo check -p concord_examples --all-targets` | PASS as far as the transitional direct-core examples are migrated |
| `cargo metadata --format-version 1 --all-features` | PASS; 187 resolved packages, 7 workspace members |
| `cargo tree -p concord -e normal --prefix none` | PASS; showed façade edges to core, extras, and proc macros, with extras sharing the same core package |
| `cargo generate-lockfile --manifest-path concord_macros/tests/fixtures/Cargo.toml --offline` | PASS; fixture lockfile regenerated |
| `cargo update --manifest-path concord_macros/tests/fixtures/Cargo.toml --offline -p <package> --precise <baseline-version>` for `cc 1.2.67`, `futures-io 0.3.32`, `futures-sink 0.3.32`, `futures-task 0.3.32`, `hyper 1.10.1`, `libc 0.2.186`, `thiserror 2.0.18`, `tokio-macros 2.7.0`, and `tokio-util 0.7.18` | PASS; removed unrelated transitive refreshes from the regenerated fixture lockfile |
| `cargo check --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-renamed-facade --offline --locked` | PASS |
| `cargo expand --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-renamed-facade --lib` | PASS; expansion contained 27 `::runtime::__private` matches, 2 `pub(super) use ::runtime as __core` aliases, and 0 `::concord_core` paths |
| `cargo check --workspace --all-targets` | PASS after fixing the façade/core JSON feature-unification bridge bug |
| `cargo check --workspace --all-targets --all-features` | PASS; retained the pre-existing Windows linker warning and three unused variables in `concord_examples/examples/records.rs` |
| `$env:RUSTDOCFLAGS = "-D warnings"; cargo doc -p concord --all-features --no-deps` | PASS; the host linker warning is outside rustdoc linting |
| `cargo test -p concord_core --test deterministic_redaction` | PASS |
| `cargo test -p concord_core --lib execute::auth_tests::basic_and_custom_header_material_exist_only_at_the_native_boundary -- --exact` | PASS |
| `cargo fmt --all -- --check` | PASS at the final phase gate |
| `git diff --check` | PASS at the final phase gate |

The pinned toolchain still has no applicable `cargo-clippy` binary. The eight
core Trybuild snapshots recorded in the Phase 0 baseline remain a known
pre-existing workspace-test failure and were not silently blessed in this
phase.

## Rust 1.97 macro diagnostic snapshots

The first Phase 1 `cargo test -p concord_macros` run reached macro Trybuild
coverage that the Phase 0 workspace run had not reached because Cargo stopped
at core's earlier failing UI package. Fifteen of 25 macro UI groups mismatched
their stored diagnostics.

The isolated command

```text
cargo test -p concord_macros --test ui generated_request_extension_traits_are_sealed -- --exact --nocapture
```

showed an unchanged privacy error and span with only the Rust 1.97 source-gutter
format changing. The snapshots were then regenerated once with:

```powershell
$env:TRYBUILD = "overwrite"
cargo test -p concord_macros --test ui
```

All 23 changed `.stderr` files were reviewed. Primary error categories and
capability/privacy claims were unchanged. Rust 1.97 added richer source/help
locations and emitted three additional instances of the same request-body
trait-bound error in each of two fixtures. A normal, non-overwrite
`cargo test -p concord_macros` then passed.

## Bounded compatibility work

- Direct macro expansion through `concord_core` remains internal-only and is
  removed in Phase 14.
- `concord_examples` retains its two implementation-crate edges until the
  canonical consumer migration in Phase 5.
- Core still owns concrete extras implementations during Phase 1. Phases 2–4
  move them without changing lifecycle semantics.
- Every package remains `publish = false`; the unrelated crates.io `concord`
  package-name conflict remains a release-planning blocker for Phases 13–14.

No selected design decision required a deviation.
