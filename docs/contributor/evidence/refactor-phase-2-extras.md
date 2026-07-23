# Phase 2 pure extras extraction validation

Phase 2 was implemented from commit `6b87ac6` with Rust and Cargo 1.97.1.

## Resulting ownership

`Json<T>`, `Csv<T>`, `NdJson<T>`, `OffsetPagination`,
`PagePagination`, `CursorPagination<C>`, and `HasNextCursor<C>` now live only
in `concord_extras`. The implementations use documented public semantic
contracts from `concord_core`; extras has no native Reqwest access or hidden
friend bridge.

Core continues to own `EncodeBody`, `DecodeBody`, `RecordDecoder`, the bounded
record stream, `PaginationController`, the sequential pagination loop, limits,
identity handling, and all-or-error result behavior. Moving the concrete
implementations therefore did not introduce another executor or acquisition
path.

The façade forwards `json`, `csv`, and `ndjson` to extras and preserves
`concord::Json`, `concord::codec::Json`, the small prelude, records-module, and
pagination-module paths. Pagination controllers are featureless. NDJSON does
not enable buffered JSON.

The former core `json` feature is gone. A temporary `oauth2-client` feature
contains only the legacy built-in OAuth provider's `serde_json` dependency and
does not expose `Json<T>`; Phase 4 removes that temporary ownership.

The macro no longer detects JSON spelling, emits a JSON feature assertion, or
depends on extras. Public tests prove that a local type named `Json`, an alias
of the extras codec, and a fully qualified façade codec are all treated as
ordinary response specification types.

## Commands and observed results

| Command | Result |
|---|---|
| `cargo check -p concord_core --no-default-features --all-targets` | PASS |
| `cargo check -p concord_extras --no-default-features --all-targets` | PASS |
| `cargo check -p concord_extras --all-features --all-targets` | PASS |
| `cargo test -p concord_extras --no-default-features` | PASS |
| `cargo test -p concord_extras --all-features` | PASS; 10 unit tests and 1 external public-controller test |
| `cargo test -p concord --no-default-features` | PASS; local `Json` neutrality test passed |
| `cargo test -p concord --no-default-features --features json` | PASS; aliased and fully qualified extras JSON tests passed |
| `cargo test -p concord --all-features` | PASS; 3 integration tests |
| `cargo test -p concord_macros` | PASS; 33 unit, 17 compile, 6 dependency-policy, all integration, 25 UI, and doctest groups passed |
| `cargo tree -p concord_macros --edges normal,build` | PASS; no core, extras, or façade edge |
| `cargo tree -p concord_core --edges normal` | PASS; no CSV, Serde, or JSON ecosystem edge |
| `cargo tree -p concord_core --all-features --edges normal` | PASS; `serde_json` appears only for the transitional OAuth2 implementation |
| `cargo check -p concord --no-default-features` | PASS |
| `cargo check -p concord --no-default-features --features json` | PASS |
| `cargo check -p concord --no-default-features --features records` | PASS |
| `cargo check -p concord --no-default-features --features csv` | PASS |
| `cargo check -p concord --no-default-features --features ndjson` | PASS |
| `cargo check -p concord --no-default-features --features csv,ndjson` | PASS |
| `cargo check --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-json-disabled --features ndjson --offline --locked` | PASS; enabling NDJSON did not make `Json` available |
| `cargo check --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-renamed-facade --all-features --offline --locked` | PASS |
| `cargo test --target-dir target/fixture-contracts --manifest-path concord_macros/tests/fixtures/Cargo.toml -p concord-fixture-extension-certification --all-features --offline --locked` | PASS; local downstream codec/controller implementations and their runtime proofs passed |
| `cargo check -p concord_perf --all-targets --all-features` | PASS after migrating tooling to the façade |
| `cargo +1.85.0 check -p concord_core --no-default-features --all-targets` | PASS |
| `cargo +1.85.0 check -p concord_extras --all-features --all-targets` | PASS |
| `cargo +1.85.0 check -p concord --no-default-features --features json,csv,ndjson` | PASS |
| `$env:RUSTDOCFLAGS = "-D warnings"; cargo doc -p concord_core --all-features --no-deps` | PASS |
| `$env:RUSTDOCFLAGS = "-D warnings"; cargo doc -p concord_extras --all-features --no-deps` | PASS |
| `$env:RUSTDOCFLAGS = "-D warnings"; cargo doc -p concord --all-features --no-deps` | PASS |
| `cargo test -p concord_core --lib --all-features` | PASS; 351 core lifecycle and boundedness tests |
| `cargo test -p concord_core --test deterministic_redaction` | PASS |
| `cargo test -p concord_core --lib execute::auth_tests::basic_and_custom_header_material_exist_only_at_the_native_boundary -- --exact` | PASS |
| `cargo test -p concord_core --all-features --lib auth::oauth2::tests::malformed_unsafe_and_non_success_oauth_responses_are_body_safe -- --exact` | PASS; 1 focused redaction test |
| `cargo test -p concord_examples --all-features --test canonical_examples` | PASS; construction remained inert and deterministic |
| `cargo metadata --format-version 1 --all-features --no-deps` | PASS |
| `cargo check --workspace --all-targets` | PASS; only the pre-existing localized MSVC linker message was emitted |
| `cargo check --workspace --all-targets --all-features` | PASS; only the same linker message was emitted |
| `cargo fmt --all -- --check` | PASS |
| `git diff --check` | PASS |

The generated-contract bridge version was advanced from 15 to 16 in core,
macro emission, compatibility fixtures, and version tests.

## Diagnostics and known tooling limits

Nine macro Trybuild snapshots changed after macro tests switched from removed
core implementations to local public-contract implementations. They were
reviewed individually before a normal `cargo test -p concord_macros` pass:
changes were source/type locations and removal of core `Json` suggestions, not
weakened privacy or capability diagnostics.

`cargo test -p concord_core --test ui` remains unsuccessful on Rust 1.97.1.
The eight baseline diagnostic drifts recorded in Phase 0 remain, and this
standalone feature selection also exposes two existing feature-sensitive
fixtures (`development_namespace` and response/multipart capability
availability). No core snapshot was blessed. Clippy remains unavailable for
the pinned toolchain.

The localized MSVC linker message is emitted by rustc while producing the
proc-macro DLL import library; it is not a Rust source warning.

No selected design decision required a deviation.
