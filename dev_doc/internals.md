# Concord internals

This file defines maintainer ownership, compiler stages, and validation paths.

## Workspace

| Path | Owner |
|---|---|
| `crates/concord` | application façade and feature composition |
| `crates/concord-core` | syntax-neutral runtime and extension traits |
| `crates/concord-extras` | first-party extension implementations |
| `crates/concord-macros` | `api!` parser, semantic resolver, and code generator |
| `crates/concord-examples` | compile-checked declarations and runtime examples |
| `crates/concord-test-support` | deterministic loopback and concurrency support |
| `tools/xtask` | repository validation utilities |
| `tools/perf` | internal observational performance probes |
| `fixtures` | isolated Cargo-graph fixtures |

Applications depend only on `concord`.

## Compiler pipeline

The macro pipeline is parse, normalize, validate, resolve the generated
surface, and emit.

### Parse

`syntax` builds a source-shaped AST with spans. It validates local grammar,
literal structure, duplicate local entries, and bounded nesting.

The parser does not inspect concrete Rust type meaning.

### Normalize

`semantic::inheritance` resolves client, scope, and endpoint-leaf operations
into one effective request configuration.

Host, path, query, headers, authentication, timeout, and rate-limit semantics
are normalized once. Source order is retained for ordered entries.

### Validate

Semantic validation resolves value visibility, endpoint identities,
credential dependencies, public-expression ownership, pagination bindings,
rate-limit profile bindings, effective duplicate targets, host constraints,
and static head ceilings.

Cross-attachment-site rules are checked against each final endpoint leaf.

### Surface

`semantic::surface` derives public names, modules, request capabilities,
descriptions, and trait obligations.

Name collision checking uses the exact emitted helper set.

### Emit

`emit` consumes normalized semantic data. It does not reparse source grammar
or infer missing semantics.

Generated code targets the caller-visible `concord` façade through the private
bridge. A bridge-version assertion rejects incompatible macro and runtime
pairs.

## Runtime ownership

Core owns:

- immutable client runtime snapshots;
- public request planning;
- managed request and response bodies;
- credential slots and recovery;
- limiter and retry sequencing;
- native HTTP construction;
- response acquisition and conversion;
- pagination execution;
- hooks, completion, safe errors, and redaction.

Extras implement JSON, CSV, NDJSON, OAuth2 client credentials, and the local
rate limiter behind façade features.

## Testing

Unit tests own local invariants. Integration tests own cross-module runtime
behavior. Trybuild files own generated API absence and compiler diagnostics.
Examples own documentation declarations. Cargo fixtures own dependency-graph
and feature-isolation behavior.

`pwsh -NoLogo -NoProfile -File scripts/check_docs_examples.ps1` requires every
`concord` fence under `docs/` to match whitespace-normalized source in the compiled
`crates/concord-examples/src/docs_spec.rs` fixture.

The commit gate is:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

Snapshot changes are reviewed before `.stderr` files are updated.

### Specification rejection coverage

| Specification rule | Enforcement |
|---|---|
| unknown and ghost syntax | `crates/concord/tests/ui/fail/syntax/ghost_syntax.rs` |
| defaults and public-expression grammar | `default_expression_restrictions.rs`, `spec_rejections.rs`, and `names/` |
| fixed base, static host, static path, and reserved headers | `spec_rejections.rs`, `ipv4_host_prefix.rs`, and `ipv6_host_prefix.rs` |
| credential compatibility and duplicate authentication targets | `authentication/` and `spec_rejections.rs` |
| pagination syntax, bindings, bodies, modes, and terminal capabilities | `pagination/` |
| rate-limit structure, visibility, bindings, keys, windows, and identity | `rate_limits/` and `spec_rejections.rs` |
| dynamic host labels and path atoms | `concord_core::request::head` unit tests and `concord-core/tests/public_api/generated_contract.rs` |

## Security review

Security tests use unique sentinels and inspect `Debug`, `Display`, sources,
plans, descriptions, hooks, completion, limiter metadata, provider failures,
status bodies, and generated Rustdoc.

Positive loopback tests prove private authentication reaches transport while
remaining absent from public observation.

## Performance review

Performance probes are observational. They compare identical machines,
toolchains, feature sets, profiles, and input sizes.

No benchmark threshold defines a semantic contract.

## Documentation ownership

`docs/spec.md` owns DSL grammar and rejection rules.
`docs/runtime.md` owns execution order and observation.
Topic files own material explicitly linked from those two documents.
`docs/design_invariants.md` owns cross-cutting invariants.
`dev_doc/finalization_decisions.md` owns finalization judgments.

Phase 3 enforces DSL examples and rejection diagnostics.

## Frozen state

The private v1 surface is frozen by `FINAL.md` and the annotated `v1.0.0` tag.
New syntax, generated capabilities, publication preparation, website
machinery, and alternate executors are outside the frozen state.
