# Macro pipeline

The declaration compiler is organized by semantic responsibility:

```text
syntax  ->  semantic resolution/validation  ->  surface model  ->  emit
```

`crates/concord-macros/src/syntax` parses tokens and attaches source spans. It
does not decide generated capabilities. Named clauses are collected without
requiring textual order, while ordered lists preserve their source order.

`crates/concord-macros/src/semantic` resolves inheritance, optionality,
defaults, identities, shaping, authentication, pagination, rate limits, and
cross-declaration constraints. The resolved model is the only source for
generated names and meanings.

`crates/concord-macros/src/semantic/surface.rs` derives the public capability
model: client construction, scope/endpoint names, request wrappers,
descriptions, and valid inherent terminals. Emitters consume this model and
must not discover new user errors.

`crates/concord-macros/src/emit` produces public artifacts, documentation,
hidden bridge calls, and feature assertions. Applications invoke only
`concord::api!`; that façade passes its own dependency-renaming-safe path to
the hidden implementation entry. Output never emits direct `::concord_core`
paths and never recognizes JSON, CSV, NDJSON, OAuth2, governor, or
pagination-controller type names.

## Diagnostic rules

- Syntax errors point to the token that violates grammar.
- Duplicate clauses report both original and duplicate spans.
- Semantic diagnostics use remote-contract/public API vocabulary.
- Rust type/trait/feature failures remain compiler diagnostics at the
  declared type.
- Emit discovers no new user error and performs no semantic fallback.
- Diagnostics never include runtime values or protected material.

Run façade compile-pass/fail tests after every grammar or surface change. Read
new stderr instead of accepting it blindly, and test privacy with compiler
errors rather than source scans.

Macro file organization follows stable domains; do not split files merely for
size. Use the façade UI suites, warning-denied Rustdoc, and
`rust-analyzer analysis-stats . --run-all-ide-things` to verify emitted
structure and editor behavior.
