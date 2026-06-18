# Release checklist

## Local v1 gate

Run:

```bash
./scripts/check_v1.sh
```

This runs the required local v1 verification commands without publishing or packaging.

The script runs:

```bash
cargo fmt --check
cargo test -p concord_core
cargo test -p concord_macros
cargo test -p concord_examples
cargo test --workspace
cargo doc --workspace --no-deps
cargo clippy --workspace --all-targets -- -D warnings
```

## Individual commands

Run:

```bash
cargo fmt
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
TRYBUILD=overwrite cargo test -p concord_macros current_trybuild_fixtures_match_expected_results
```

After refreshing snapshots, run the local v1 gate again.

## Final stale syntax audit

Before a v1 tag, run:

```bash
./scripts/check_v1.sh
```

and verify that public docs/examples do not describe rejected or removed DSL forms as valid syntax.

## Manual v1 audit

- Public DSL docs are complete and current.
- Developer docs are complete and current.
- `concord_examples/src/docs_dsl.rs` compiles.
- `concord_examples/src/docs_advanced_dsl.rs` compiles.
- Riot fixture passes.
- Public DSL reference covers every public keyword.
- Unsupported/reserved syntax is explicitly documented.
- Cache sizing syntax is documented and compile-checked.
- Same-site duplicate behavior rejection is documented and tested.
- Runtime order tests still pass.
- No stale DSL syntax in docs/examples/tests.
- No broad clippy allows.
- No runtime behavior changed without characterization tests.
- No macro behavior changed without parser/sema/codegen tests.
- No public API change was made without explicit compatibility review.
- No crates.io publishing or packaging step is included in this PR.
