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
generated names and meanings. Host-base compatibility is checked against each
endpoint's final normalized host shape: DNS-domain bases may receive labels,
while IPv4 and IPv6 bases require the effective list to be empty.
Pagination-bound references are likewise checked at the final endpoint against
every effective rate rule and the dependencies of the final host expression.

`crates/concord-macros/src/semantic/surface.rs` derives the public capability
model: client construction, scope/endpoint names, request wrappers,
descriptions, valid inherent terminals, and each endpoint's structured
`EndpointDocModel`. Façade-selected capabilities are resolved into that model
before emission. It is also the single owner of `set_*` value-control names;
collision validation, endpoint/request models, emitters, and generated
Rustdoc consume those exact names. Emitters must not rediscover semantic facts
or new user errors.

`crates/concord-macros/src/emit` produces public artifacts, documentation,
hidden bridge calls, and feature assertions. Applications invoke only
`concord::api!`; that façade passes its own dependency-renaming-safe path to
the hidden implementation entry. Output never emits direct `::concord_core`
paths and never recognizes JSON, CSV, NDJSON, OAuth2, local limiter, or
pagination-controller type names.

A source-language acceptance or diagnostic change does not by itself change
the hidden generated bridge. The DNS/IP host-shaping validation required no
bridge change. Generated-contract version 27 adds core-owned bounded
formatting for emitted rate-limit key dimensions. Bump the version only when
generated code and the façade/core bridge must agree on a different private
contract. The public `set_*` rename and final semantic pagination validation
do not change that private bridge, so the version remains 27.

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
