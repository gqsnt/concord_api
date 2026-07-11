# Release Checklist

## Local V1 Gate

Run the canonical maintained-workspace verification command:

```bash
just release
```

This runs the required local verification commands without publishing or
packaging. The gate does not require credentials or live service calls. Cargo
dependency resolution and cargo-deny advisory data may require network access
unless the necessary registry and advisory data are already cached.

The release recipe runs:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --all-targets --all-features --no-tests fail --no-fail-fast --retries 0
cargo test --workspace --doc --all-features
cargo doc --workspace --no-deps --all-features
cargo deny check
```

The root `justfile` checks each required tool directly. Deferred performance
diagnostics are available separately and are not part of release.

## Supply Chain Gate

Run the supply-chain policy gate as a separate diagnostic command:

```bash
just supply-chain
```

This recipe requires `cargo-deny`; install it with:

```bash
cargo install cargo-deny --locked
```

The check covers advisories, yanked crates, licenses, sources and registries, and configured banned or duplicate crate policy. It may require a cached advisory database or network access to refresh advisory data, but it does not use live credentials.

## Individual Commands

For focused diagnosis, use the corresponding recipes in the root `justfile`:

```bash
just fmt-check
just check
just clippy
just test
just doctest
just docs
just supply-chain
```

The canonical release gate runs one executable-test axis:

```text
cargo nextest run --workspace --all-targets --all-features \
  --no-tests fail --no-fail-fast --retries 0
```

Focused default-feature, per-crate, UI, no-default, and feature-specific
commands are diagnostics and are not dependencies of `just release`.

## Trybuild Snapshot Refresh

Trybuild fixtures are split by public UI-contract category under `concord_macros/tests/trybuild/`.

The current trybuild test functions are:

- `trybuild_facade_contract_fixtures`
- `trybuild_endpoint_io_contract_fixtures`
- `trybuild_pagination_contract_fixtures`
- `trybuild_auth_contract_fixtures`
- `trybuild_retry_contract_fixtures`
- `trybuild_route_contract_fixtures`
- `trybuild_parser_diagnostics`
- `trybuild_auth_diagnostics`
- `trybuild_policy_diagnostics`
- `trybuild_pagination_diagnostics`
- `trybuild_route_diagnostics`
- `trybuild_codegen_contract_diagnostics`
- `trybuild_rust_type_errors`

Run the full trybuild suite with:

```bash
cargo nextest run -p concord_macros --test trybuild_current
cargo nextest run -p concord_macros --test trybuild_sema
cargo nextest run -p concord_macros --test trybuild_codegen
```

Refresh stderr output only when macro diagnostics intentionally change:

```bash
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current -- --test-threads=1
```

Category-specific refresh examples:

```bash
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current trybuild_facade_contract_fixtures -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_sema trybuild_parser_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_sema trybuild_auth_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_sema trybuild_policy_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_sema trybuild_pagination_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_sema trybuild_route_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_codegen trybuild_codegen_contract_diagnostics -- --test-threads=1
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_codegen trybuild_rust_type_errors -- --test-threads=1
```

Only use `TRYBUILD=overwrite` when diagnostics intentionally change. Inspect the git diff of `.stderr` files before accepting updates. Path-only changes from fixture moves are acceptable; changed wording and spans must be reviewed.

The repo ships `.config/nextest.toml`. It gives `concord_macros`'s `trybuild_current` binary a longer slow-timeout and places it in the `trybuild` nextest group. The other trybuild binaries use the ordinary nextest scheduling.

The current-pass example above is representative; the other current-pass wrapper names (`trybuild_endpoint_io_contract_fixtures`, `trybuild_pagination_contract_fixtures`, `trybuild_auth_contract_fixtures`, `trybuild_retry_contract_fixtures`, and `trybuild_route_contract_fixtures`) use the same `--test trybuild_current` binary.

Trybuild remains part of the full gate through `cargo nextest run --workspace --all-targets`. The checked-in nextest config only special-cases `concord_macros`'s `trybuild_current` binary for a longer timeout and the `trybuild` group; the other trybuild binaries run under the standard nextest scheduling.

## Final Syntax Audit

Before a release tag, run:

```bash
just release
```

and verify that public docs and examples do not describe rejected DSL forms as valid syntax.

## Manual V1 Audit

- Public DSL docs are complete and current.
- Developer docs are complete and current.
- Review docs for outdated runtime order, outdated syntax, and removed implementation concepts.
- Review examples for dangerous live calls; live smoke code must remain environment-gated.
- Live-smoke examples skip by default without `CONCORD_RUN_RIOT_TEST`, `CONCORD_RUN_DDRAGON_TEST`, and any required API key environment variables.
- Review generated-code changes for validation-dependent panics when changing codegen.
- `concord_examples/src/docs_dsl.rs` compiles.
- `concord_examples/src/docs_advanced_dsl.rs` compiles.
- Riot fixture passes.
- Public DSL reference covers every public keyword.
- Unsupported and reserved syntax is explicitly documented.
- Same-site duplicate behavior rejection is documented and tested.
- The release gate does not require credentials or live service calls; dependency resolution and cargo-deny advisory data may require network access unless cached.
- Query auth secrets are redacted from debug URLs.
- Query auth redaction tests pass.
- No auth secret appears in debug output tests.
- `401` and `403` auth rejection behavior matches `AuthStepPolicy` defaults.
- Redaction tests cover debug output, errors, wrappers, and OAuth client secrets.
- Feature-surface validation uses the maintained workspace check.
- Macro trybuild diagnostics have been refreshed only when wording or spans intentionally changed.
- Runtime characterization tests cover request order, auth collision ordering, rate-limit observation, retry exhaustion, pagination progress, and cancellation boundaries.
- Runtime order tests still pass.
- No outdated DSL syntax remains in docs or examples.
- No broad clippy allows.
- No runtime behavior changed without characterization tests.
- No macro behavior changed without parser, sema, or codegen tests.
- No public API change was made without explicit stability review.
- No crates.io publishing or packaging step is included in this PR.
