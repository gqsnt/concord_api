# Release checklist

## Local v1 gate

Run:

```bash
./scripts/check_v1.sh
```

This runs the required local v1 verification commands without publishing or packaging.
The default gate does not require external credentials or network access.

The script runs:

```bash
cargo fmt --check
cargo test -p concord_core redaction
cargo test -p concord_core auth
cargo test -p concord_core cache
cargo test -p concord_core pagination
cargo test -p concord_core
cargo test -p concord_macros
cargo test -p concord_examples
cargo test --workspace
cargo doc --workspace --no-deps
cargo clippy --workspace --all-targets -- -D warnings
```

The default gate does not require external credentials, network access,
publishing, packaging, tagging, or `TRYBUILD=overwrite`.

## Individual commands

Run:

```bash
cargo fmt --check
cargo test -p concord_core redaction
cargo test -p concord_core auth
cargo test -p concord_core cache
cargo test -p concord_core pagination
cargo test -p concord_core
cargo test -p concord_macros
cargo test -p concord_examples
cargo test --workspace
cargo doc --workspace --no-deps
cargo clippy --workspace --all-targets -- -D warnings
```

## Trybuild snapshot refresh

Only when macro diagnostics intentionally change:

```bash
TRYBUILD=overwrite cargo test -p concord_macros --test trybuild_current current_trybuild_fixtures_match_expected_results
```

After refreshing snapshots, run the local v1 gate again. This command is not
part of the default release gate.

## Final stale syntax audit

Before a v1 tag, run:

```bash
./scripts/check_v1.sh
```

and verify that public docs/examples do not describe rejected or removed DSL forms as valid syntax.

## Manual v1 audit

- Public DSL docs are complete and current.
- Developer docs are complete and current.
- Review docs for stale runtime order, stale syntax, and removed implementation concepts.
- Review examples for dangerous live calls; live smoke code must remain environment-gated.
- Live-smoke examples skip by default without `CONCORD_RUN_RIOT_TEST`,
  `CONCORD_RUN_DDRAGON_TEST`, and any required API key environment variables.
- Review generated-code changes for validation-dependent panics when changing codegen.
- `concord_examples/src/docs_dsl.rs` compiles.
- `concord_examples/src/docs_advanced_dsl.rs` compiles.
- Riot fixture passes.
- Public DSL reference covers every public keyword.
- Unsupported/reserved syntax is explicitly documented.
- Cache sizing syntax is documented and compile-checked.
- Same-site duplicate behavior rejection is documented and tested.
- Live smoke examples are environment-gated.
- The release gate does not require external credentials or network access by default.
- Query auth secrets are redacted from debug URLs.
- Query auth redaction tests pass.
- No auth secret appears in debug output tests.
- `401`/`403` auth rejection behavior matches `AuthStepPolicy` defaults.
- Redaction tests cover debug output, errors, wrappers, and OAuth client secrets.
- Runtime order tests still pass.
- No stale DSL syntax in docs/examples.
- No broad clippy allows.
- No runtime behavior changed without characterization tests.
- No macro behavior changed without parser/sema/codegen tests.
- No public API change was made without explicit compatibility review.
- No crates.io publishing or packaging step is included in this PR.
