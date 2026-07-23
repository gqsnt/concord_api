# Phase 12 contract-oriented tests and fixtures evidence

Phase 12 was implemented in the worktree based on commit `39cbf9d`.
Validation used Rust/Cargo 1.97.1 on Windows, with the declared Rust 1.85
minimum checked separately.

## Contract ownership

`docs/developer/test-architecture.md` is now the checked-in proof inventory.
It maps every product invariant to one primary owner, classifies every
integration test and fixture present at the Phase 12 starting commit, and
records the exact replacement before an obsolete proof is removed.

The final ownership boundaries are:

- `concord_core` owns generic lifecycle, security, bounds, transport,
  configuration, semantic-extension, and native-boundary proofs;
- `concord` owns generated public APIs, compiler diagnostics, Rustdoc paths,
  renamed-dependency behavior, and downstream extension certification;
- `concord_extras` owns concrete JSON, CSV, NDJSON, pagination, governor,
  observer, and OAuth2 behavior through public core contracts;
- `concord_test_support` owns its deterministic executor and diagnostic
  behavior;
- `concord_examples` continues to test the actual importable example modules;
- standalone fixtures exist only where dependency replacement, dependency
  renaming, feature absence, or feature unification is itself the subject.

Core integration tests are grouped under `lifecycle`, `security`, `bounds`,
`transport`, `configuration`, and `public_types` harnesses. Their assertion
bodies were moved without replacing deterministic event, attempt, first
excess, cancellation, redaction, or native-boundary checks with source scans.
The three former feature-specific core UI harnesses are one feature-aware
`ui` harness.

The macro package now retains only compiler unit tests beside its
syntax/semantic/emission implementation. It has no dev dependency on the
runtime, façade, test-support package, or Trybuild. Generated integration
tests and the shared third-party semantic implementations moved to the
façade, where every product-facing import is through `concord`.

## Compiler and Rustdoc proofs

The façade UI suite is organized by authentication, capabilities,
compatibility, construction, features, names, pagination, privacy,
rate-limits, and syntax, with compile-pass cases grouped by construction and
language. It contains 62 source cases after removing two exact duplicates:

- `cross_client_endpoint` duplicated the retained
  `wrong_client_endpoint` proof;
- the macro-owned dangerous raw-stream rejection duplicated the retained
  façade capability proof.

Active paths use `RuntimeEndpoint`. The one
`prepared_endpoint_alias_deprecated` compatibility case remains paired with
the temporary alias and is scheduled for deletion in Phase 14.

All Rust 1.97.1 snapshot changes were individually reviewed. Fourteen core
snapshots changed only because the current compiler places feature-gate
annotations, help text, and trait notes differently; the same error codes and
privacy targets remain. The records-without-pagination snapshot was
separately corrected to use core's featureless `Text` response rather than
the already-extracted `Json<T>`. No snapshot was copied blindly.

The façade Rustdoc owner checks stable public paths, methods, privacy, and
feature-gated namespaces. Workspace doctests and warning-denied documentation
for core, extras, and façade pass.

Rust 1.85 compiles and runs the runtime, extras, and macro unit suites, but
the exact full façade and core no-default commands stop only in Trybuild
because the checked-in Rust 1.97 diagnostic rendering differs. Before the UI
harnesses run, all ordinary façade/core tests pass; the façade reports 19
rendering mismatches across three grouped harness tests and core reports 14.
The Rust 1.97 snapshots remain the deliberate stable-lane owner, so the MSRV
rendering was not blessed over them.

## Extras as a downstream contract proof

`concord_extras` integration tests compile concrete types against public
`concord_core` traits and exercise their runtime policies directly:

- JSON media matching, encode/decode, and parser-text redaction;
- CSV and NDJSON framing, media types, chunk boundaries, and safe failures;
- offset, page, cursor, and next-cursor controller progression;
- governor key bounds, capacity, pruning, cooldown, permit ordering, and
  cancellation;
- observer Retry-After policy;
- OAuth2 builder validation, Basic/form encoding, isolated acquisition,
  expiry/reacquisition, and seeded redaction.

Its metadata assertion proves that the only normal Concord edge is
`concord_core`; there is no façade or proc-macro dependency and no friend
boundary.

## Consolidated graph fixtures

The two former fixture workspaces and their lockfiles are replaced by:

```text
fixtures/Cargo.toml
fixtures/Cargo.lock
fixtures/features
fixtures/dependency/renamed_facade
fixtures/dependency/contract_mismatch
fixtures/dependency/contract_mismatch/incompatible_facade
```

`incompatible_facade` is support for the replacement-graph handshake proof,
not an independent product owner. The three standalone test packages each
have a graph-dependent reason:

- `features` isolates absence combinations and executes the no-governor
  fail-closed contract;
- `renamed_facade` proves generated bridge resolution through a renamed
  direct dependency;
- `contract_mismatch` proves the version handshake against a package
  replacement exposing contract version 999.

Feature absence bins cover JSON, CSV, NDJSON, multipart, OAuth2, raw response,
custom retry, default TLS, HTTP/2, gzip, Brotli, deflate, and development
tools. The no-governor runtime proof observed:

```text
CONCORD_NO_GOVERNOR_RATE_LIMITS_OK generated=4 runtime=4 fallback_sends=0
```

The façade public-compile harness invokes every fixture with `--offline` and
`--locked`, redirects output beneath `target/fixtures`, and expects the
intentional absence/mismatch cases to fail. Only the root and fixture
`Cargo.lock` files remain outside build output.

Standalone enabled-feature, extension-certification, direct-macro
missing-core, and development-tools packages were deleted after their
compiler, metadata, or normal-workspace replacements passed.

## Feature and invariant validation

The façade check matrix passed for default, no-default, all-features, JSON,
records, CSV, NDJSON, CSV plus NDJSON, multipart, governor, OAuth2, dangerous
raw response, dangerous custom retry, and the supported
TLS/HTTP2/compression combination. Each absence fixture is invoked in an
isolated Cargo command so another bin cannot accidentally activate its
feature.

The complete default and all-feature workspace suites pass. Focused
revalidation additionally passed for:

- the core seeded error/hook/sanitized-metadata security suite;
- all complete-only status-body acquisition and first-excess cases;
- zero-effect planning and generated/runtime executor equivalence;
- generated authentication ordering and provider replacement;
- every extras implementation and public-contract test;
- exact Reqwest compression and multipart feature isolation.

No native request/response or authentication material became public. No
executor, acquisition mode, pagination loop, limiter default owner, or
parallel semantic model was added in this phase.

## Commands and observed results

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo fmt --manifest-path fixtures/Cargo.toml --all -- --check` | PASS |
| `cargo check --workspace --all-targets` | PASS |
| `cargo check --workspace --all-targets --all-features` | PASS |
| 14-command façade feature check matrix | PASS for every combination listed above |
| `cargo test --workspace` | PASS; 49 result groups |
| `cargo test --workspace --all-features` | PASS; 51 result groups |
| `cargo test --workspace --test ui` | PASS; façade 6 and core 2 grouped harness tests |
| `cargo test -p concord --test ui --no-default-features` | PASS; 6 grouped harness tests |
| `cargo test -p concord --test ui --all-features` | PASS; 8 grouped harness tests |
| `cargo test -p concord-core --no-default-features --test ui` | PASS; 3 grouped harness tests |
| `cargo test -p concord --test public_compile --no-default-features` | PASS; 6 graph groups |
| `cargo test -p concord --test dependency_policy` | PASS; 10 assertions |
| `cargo test -p concord --test rustdoc --all-features` | PASS; 2 assertions |
| `cargo test -p concord --all-features --test endpoint_planning` | PASS; 5 tests |
| `cargo test -p concord --all-features --test deterministic_generated_runtime` | PASS; 1 lifecycle-equivalence test |
| `cargo test -p concord --all-features --test generated_authentication` | PASS; 5 tests |
| `cargo test -p concord-core --all-features --test security` | PASS; 3 grouped security tests |
| `cargo test -p concord-core --all-features --test bounds` | PASS; 12 bounded status-body tests |
| `cargo test -p concord-core --all-features --test transport` | PASS; 17 transport/native-boundary tests |
| `cargo test -p concord-extras --all-features` | PASS; 37 unit tests, 7 integration tests, and 1 compile-fail doctest |
| `cargo test -p concord-test-support --all-features` | PASS; 3 unit and 4 deterministic integration tests |
| `cargo test -p concord-macros --lib` | PASS; 61 compiler unit tests |
| `cargo test --workspace --doc` | PASS; 3 compile-fail doctests |
| `RUSTDOCFLAGS='-D warnings' cargo doc -p concord --all-features --no-deps` | PASS; localized MSVC proc-macro linker warning remained |
| `RUSTDOCFLAGS='-D warnings' cargo doc -p concord-core --all-features --no-deps` | PASS |
| `RUSTDOCFLAGS='-D warnings' cargo doc -p concord-extras --all-features --no-deps` | PASS |
| `cargo metadata --format-version 1 --all-features --no-deps` | PASS |
| `cargo metadata --manifest-path fixtures/Cargo.toml --format-version 1 --locked --no-deps` | PASS |
| lockfile enumeration excluding `target` | PASS; only `Cargo.lock` and `fixtures/Cargo.lock` |
| `cargo deny check` | PASS; advisories, bans, licenses, and sources all `ok`; existing duplicate-version warnings remain |
| `cargo +1.85.0 check --workspace --all-targets` | PASS |
| `cargo +1.85.0 test -p concord-extras --no-default-features` | PASS |
| `cargo +1.85.0 test -p concord-macros --lib` | PASS; 61 tests |
| `cargo +1.85.0 test -p concord --no-default-features` | RUNTIME PASS, UI UNAVAILABLE AS A CROSS-VERSION SNAPSHOT; 19 Rust 1.85 rendering mismatches |
| `cargo +1.85.0 test -p concord-core --no-default-features` | RUNTIME PASS, UI UNAVAILABLE AS A CROSS-VERSION SNAPSHOT; 14 Rust 1.85 rendering mismatches |
| `cargo clippy --version` | UNAVAILABLE; `cargo-clippy.exe` is not applicable to installed Rust 1.97.1 |
| `git diff --check` | PASS |

The localized MSVC linker warning appears while creating the proc-macro DLL
import library and does not fail warning-denied Rustdoc. Cargo deny continues
to report the already-recorded `core-foundation` and `windows-sys` duplicate
versions as warnings.

## Deviations and remaining blockers

No selected design decision or product invariant required a deviation.

The exact Rust 1.85 Trybuild rendering cannot share the Rust 1.97 snapshot
files; both compiler lanes still compile and run ordinary tests. Clippy is
unavailable for the installed pinned toolchain. The manual rust-analyzer
interaction remains a release gate.
