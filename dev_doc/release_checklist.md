# Release Checklist

## Local V1 Gate

Run:

```bash
bash ./scripts/check_v1.sh
```

This runs the required local v1 verification commands without publishing or packaging. The default gate does not require external credentials or network access.

The script runs:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --all-targets
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

It explicitly requires `cargo-nextest`.

## Individual Commands

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --all-targets
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

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

Before a v1 tag, run:

```bash
bash ./scripts/check_v1.sh
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
- The release gate does not require external credentials or network access by default.
- Query auth secrets are redacted from debug URLs.
- Query auth redaction tests pass.
- No auth secret appears in debug output tests.
- `401` and `403` auth rejection behavior matches `AuthStepPolicy` defaults.
- Redaction tests cover debug output, errors, wrappers, and OAuth client secrets.
- Feature-surface drift has been checked through `scripts/check_features.sh`.
- Macro trybuild diagnostics have been refreshed only when wording or spans intentionally changed.
- Runtime characterization tests cover request order, auth collision ordering, rate-limit observation, retry exhaustion, pagination progress, and cancellation boundaries.
- Runtime order tests still pass.
- No outdated DSL syntax remains in docs or examples.
- No broad clippy allows.
- No runtime behavior changed without characterization tests.
- No macro behavior changed without parser, sema, or codegen tests.
- No public API change was made without explicit stability review.
- No crates.io publishing or packaging step is included in this PR.
