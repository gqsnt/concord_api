# Phase 14 compatibility-removal and release evidence

Phase 14 was implemented in the worktree based on commit `c21ed3c`.
Validation used Rust/Cargo 1.97.1 on `x86_64-pc-windows-msvc`, with Rust
1.85.0 checked separately. The observed machine was Windows 11
10.0.26200, a 20-logical-processor Intel Core i5-13600K, and 31.8 GiB of
memory. Installed supporting tools were Just 1.46.0, cargo-expand 1.0.123,
and rust-analyzer 1.97.1.

## Final product contract

Applications depend only on `concord`. The façade owns public paths, feature
defaults and forwarding, macro reexport, and feature-dependent composition.
It delegates the hidden generated bridge to `concord-core` and composes
downstream implementations from `concord-extras`.

The final Concord dependency direction is:

```text
concord ─────────────► concord-core
   ├────────────────► concord-macros
   └────────────────► concord-extras ─────► concord-core

concord-examples ────► concord
```

There is no normal Concord-package dependency from `concord-macros`.
`concord-extras` has only one Concord-package dependency, `concord-core`.
Core has no edge to macros, extras, or the façade.

The declaration compiler emits only through the caller-visible `concord`
package, including Cargo dependency renames, and through
`concord::__private`. The direct-core fallback was deleted. A graph fixture
proves that `concord-core` plus `concord-macros` is no longer a supported
application setup. The final compiler/runtime generated-contract handshake
is version 23.

Every endpoint declaration produces one predictable direct method, one public
endpoint value, one named request wrapper with inherent valid capabilities,
and one safe description in the scope-oriented module tree. Direct calls and
explicit endpoint binding share the same generated endpoint model.
Runtime-defined endpoints use `RuntimeEndpoint`; all three forms use the same
core executor and lifecycle.

## Compatibility removals

The final source and tests contain no compatibility implementation for:

- direct generated `concord-core` paths or the two-crate user setup;
- `PreparedEndpoint` or `PreparedEndpointBuilder`;
- `DecodedResponse<T>` (the final name is `ApiResponse<T>`);
- configured positional constructors;
- generated request extension traits or generated preludes;
- separate generated `facades` and `endpoints` trees;
- PascalCase endpoint/scope source identifiers or fixed-order clauses;
- request-body inference from a parameter named `body`;
- macro-special OAuth2 or concrete codec/controller/limiter recognition;
- custom-retry or observer policy syntax in the declaration language;
- legacy `var`, rate-limit-template, endpoint-alias, and related parser
  migration branches.

A parameter named `body` without `#[body]` is now an ordinary parameter. The
final-language compile-pass case proves that behavior. Removed-syntax tests
whose only purpose was a migration diagnostic were deleted; final language,
capability absence, privacy, and feature-absence tests remain.

## Lifecycle, security, and boundedness

The complete `just verify` gate passed. Its dedicated security recipe reran
core security, bounds, and lifecycle suites plus façade planning,
authentication, generated lifecycle, and runtime-endpoint lifecycle tests
with all features.

Those owners retain deterministic proofs for inert construction, immutable
client snapshots, public-head-before-private-auth ordering,
hook-before-materialization ordering, credential-preflight-before-limiter and
limiter-before-final-auth ordering, one additional challenge attempt,
status-before-decode, first-excess withholding, finite bodies and streams,
bounded status capture, sequential cycle-detecting all-or-error pagination,
fixed-result exactly-once completion, cancellation without detached
correctness work, and the bounded feature-gated raw-response terminal.

Seeded redaction tests cover protected URL/query/header values, authorization
material, provider values and response sentinels, multipart values, rate
identities, hooks, descriptions, plans, errors, completion events, and Debug
or Display surfaces. Safe planning continues to use the real public-head path
while proving zero hooks, credentials, limiter work, body/factory work,
stream polling, transport, and completion. Native Reqwest requests,
responses, multipart forms, and authentication material remain private to
core. Extras compile only through semantic contracts and introduce no
alternate executor.

## Commands and observed results

The authoritative final correctness command was:

| Command | Result |
|---|---|
| `just verify` | PASS in 447.9 s; formatting, default/no-default/all-feature checks, stable warning-denied Clippy, default/all-feature workspace tests, representative MSRV checks/tests, stable façade/core UI suites, fixtures, examples, doctests/Rustdoc, security/bounds/lifecycle/planning/authentication proofs, feature matrix, and cargo-deny all completed |

Focused commands executed before or after that full gate:

| Command | Result |
|---|---|
| `cargo test -p concord-macros --lib` | PASS; 61 tests |
| `cargo check -p concord --all-targets --all-features` | PASS |
| `cargo test -p concord --test ui --no-default-features final_language_and_named_surface_compile_without_generated_import_conventions --locked` | PASS; all construction and final-language compile-pass cases |
| `cargo test -p concord --test public_compile direct_core_and_macro_dependencies_do_not_form_a_supported_user_setup --no-default-features --locked` | PASS; the graph fixture fails with the focused façade-dependency diagnostic |
| `cargo run -p concord-xtask -- docs` | PASS |
| `cargo test -p concord --test rustdoc --all-features --locked` | PASS; both rendered public-surface tests |
| `cargo test -p concord --test public_compile --no-default-features --locked` | PASS; all seven graph/feature groups |
| `cargo run -p concord-xtask -- fixtures` | PASS on the isolated rerun; fixture metadata and all seven public compile groups pass |
| `cargo metadata --format-version 1 --all-features --no-deps` | PASS; eight workspace packages at 0.1.0 |
| `cargo tree -p concord-macros -e normal --depth 1` | PASS; only proc-macro/compiler dependencies, no Concord runtime package |
| `cargo tree -p concord-extras -e normal --depth 1` | PASS; its only Concord dependency is `concord-core` |
| `cargo fmt --all -- --check` | PASS in the final gate |
| `git diff --check` | PASS in the final audit |
| `rust-analyzer --version` | PASS; `rust-analyzer 1.97.1 (8bab26f4 2026-07-14)` |

One post-documentation fixture invocation was mistakenly run concurrently
with the same `public_compile` target. The Windows linker failed with
`LNK1104` because the other process held the output executable. The fixture
command was rerun alone immediately and passed as shown above; this transient
orchestration failure is not omitted or presented as a product test failure.

`just verify` uses the installed `stable` alias for Clippy because the exact
1.97.1 channel reports that its `cargo-clippy.exe` component is not
applicable. The stable alias reports the same 1.97.1 compiler commit and its
complete `-D warnings` run passed. The localized MSVC proc-macro linker
message remains a compiler/linker warning in ordinary output; the project
does not globally hide unrelated warnings. Cargo-deny passed with the
previously reviewed duplicate-version warnings for `core-foundation` and
`windows-sys`.

## MSRV and editor results

The representative MSRV commands inside `just verify` pass:

```text
cargo +1.85.0 check --workspace --all-targets --no-default-features
cargo +1.85.0 test -p concord-core --lib --no-default-features
cargo +1.85.0 test -p concord-extras --all-features
```

The two exact MSRV Trybuild integration targets were also executed:

| Command | Result |
|---|---|
| `cargo +1.85.0 test -p concord --test ui --no-default-features --locked` | FAIL only in 19 compiler-rendering snapshot differences: 5 of 6 pagination cases, 10 of 13 capability/privacy cases, and 4 of 9 authentication/rate-limit cases. Every final-language compile-pass case and the malformed syntax/name group pass. |
| `cargo +1.85.0 test -p concord-core --test ui --no-default-features --locked` | FAIL only in 14 compiler-rendering snapshot differences: 3 dangerous-feature absence cases, 10 of 14 source-only cases, and 1 multipart case. |

The snapshots remain owned by the pinned stable 1.97.1 compiler and were not
blindly replaced. The semantic compiler failures on MSRV are the intended
ones; only diagnostic layout differs.

The rust-analyzer binary/version check passed, but completion ranking, hover,
navigation, rename-dependency completion, and diagnostic presentation require
an interactive editor session and were not executed. Linux and macOS CI lanes
and GitHub Actions were not executable from this Windows worktree and are not
claimed as passing.

## Packaging and publication posture

All five package inventory commands passed:

```text
cargo package --list -p concord-test-support --allow-dirty
cargo package --list -p concord-core --allow-dirty
cargo package --list -p concord-extras --allow-dirty
cargo package --list -p concord-macros --allow-dirty
cargo package --list -p concord --allow-dirty
```

Full package commands produced:

| Command | Result |
|---|---|
| `cargo package -p concord-test-support --allow-dirty` | PASS; 13 files, 92.9 KiB uncompressed and 24.0 KiB compressed; verification compiled |
| `cargo package -p concord-macros --allow-dirty --locked` | PASS; 36 files, 375.4 KiB uncompressed and 66.1 KiB compressed; verification compiled |
| `cargo package -p concord-core --allow-dirty --locked` | FAIL during registry preparation because exact `concord-test-support = 0.1.0` is unpublished |
| `cargo package -p concord-extras --allow-dirty --locked` | FAIL during registry preparation because exact `concord-core = 0.1.0` is unpublished |
| `cargo package -p concord --allow-dirty --locked` | FAIL during registry preparation because exact `concord-core = 0.1.0` is unpublished |

These failures establish the real publication order:
`concord-test-support` → `concord-core` → `concord-extras` and
`concord-macros` → `concord`. Test support remains an internal, undocumented
user entry point even if registry publication is required for core's
feature-gated development support.

Every package warns that license/license-file, documentation/homepage, and
repository metadata are absent. The public `concord` registry name is
occupied by an unrelated package in the reviewed registry state. All
workspace packages therefore remain `publish = false`; enabling or publishing
them requires approved legal/repository metadata, package-name ownership, and
an explicit user instruction. No publish operation was attempted.

## Performance and expansion

The reproducible scale harness and raw observed tables are recorded in
[`tools/perf/results/phase-14-windows-msvc.md`](../../../tools/perf/results/phase-14-windows-msvc.md).
It covers 1/10/100/1,000-endpoint expansion, isolated clean and incremental
checks, fresh Rustdoc, all sixteen runtime categories over 100 iterations,
and the final one-iteration smoke.

## Release artifacts, limitations, and deviations

- [Migration guide](../../../MIGRATION.md)
- [Changelog/release notes](../../../CHANGELOG.md)
- [Release/package table](../release.md)
- [Unsupported capabilities](../../reference/unsupported.md)

Runtime-defined pagination remains the selected bounded deferral: it is absent
rather than backed by a partial dynamic contract or alternate executor.
Arbitrary unbounded streaming, native transport escape, and other unsupported
forms remain documented in the reference.

Unresolved release limitations are the registry/legal/repository blockers,
interactive rust-analyzer acceptance, non-Windows CI execution, and
compiler-version-specific MSRV Trybuild rendering described above. None
changes the product language or weakens a lifecycle/security invariant.

No selected design decision required a deviation record.
