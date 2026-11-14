# Macro tests

For contributors changing the macro: this page says where a new test belongs and how to check its result.

*[Concord](../../README.md) · [tests](../../tests/README.md) · macro tests*

Add a case to the directory that describes its input and expected result. Keep
compiler output snapshots beside the source that produces them. Run `just test`
from the repository root; all Cargo commands use the offline registry.

- `unit/`: parser, resolution, diagnostics, expression scopes, and generated-code checks. These modules run inside the proc-macro crate so private implementation details stay private.
- `parser/descriptions/`: complete descriptions and deliberate syntax fragments; `parser/malformed/` checks that damaged input never panics.
- `parser/unsupported/`: syntax outside the language, including generic scopes.
- `accepted/`: Rust programs that must compile and run through the facade.
- `diagnostics/structure/`: structural messages; `diagnostics/rules/`: language-rule messages, with message numbers in filenames and `coverage.tsv`.
- `diagnostics/catalogue/`: canonical message examples; `diagnostics/api/`: complete API descriptions with one intentional error and a recorded repair.
- `diagnostics/suggestions/`: compiler snapshots for the macro's suggested repairs.
- `compiler-errors/`: Rust expression, binding, trait, span, and hygiene failures. `limits/` records constant-pattern lookup and repeated concrete trait errors.
- `parser/regressions/`: earlier complete descriptions retained as independent parser, resolution, and validation inputs.
- `snapshots/resolved/`: source descriptions and their resolved facts; `snapshots/generated/`: readable generated Rust.
- `support/`: small data types and context for standalone description fragments; `compiler.rs` runs all accepted and refused compiler cases.

Run `just doc-examples` to check the current pages. The single page-example check lives in `tests/real-apis/tests/pages.rs`. It covers
whole descriptions in the guide, reference overview, root README, and all eight
example pages, compares their exact text, and uses clients compiled by the macro.
Fragments have no standalone-description obligation. Prepare replacement sources
and exact-text fixtures with `python3 -B tools/doc_examples.py` beneath
`target/doc-examples/`, then review and copy them to the displayed paths. Use
`--check` to detect missing, extra, or changed copies.

`make_semantic_fixtures.py` and `make_real_fixtures.py` prepare diagnostic fixtures
under `target/`; expected `.stderr` files require review. Use
`TRYBUILD=overwrite just test` only when intentionally changing a diagnostic,
then inspect every changed source span and message.
