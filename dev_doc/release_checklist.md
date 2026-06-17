# Release checklist

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

When trybuild snapshots intentionally change, run:

```bash
TRYBUILD=overwrite cargo test -p concord_macros current_trybuild_fixtures_match_expected_results
```

## Manual audit

- Public DSL docs updated.
- Developer docs updated.
- Examples compile.
- Riot fixture passes.
- Docs reference covers new keywords.
- Behavior/rate-limit merge tests updated.
- Runtime order tests still pass.
- No stale DSL syntax in docs/examples/tests.
- No broad clippy allows.
- No runtime behavior change without characterization tests.
- No macro behavior change without parser/sema/codegen tests.
- No public API change without explicit release notes and compatibility review.
