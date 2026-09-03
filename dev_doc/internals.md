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

The macro pipeline is raw parse, owner/name resolution, canonical semantic IR,
validation/surface computation, then projection.

### Parse

`syntax` builds a source-shaped AST with spans. It validates local grammar,
literal structure, duplicate local entries, and bounded nesting.

The parser does not inspect concrete Rust type meaning.

### Resolve canonical IR

`semantic` assigns stable owner, value, endpoint, and attachment identities and
resolves client, scope, and endpoint-leaf operations into one effective request
configuration with source origins.

Subdomain, path, ordered query/header patches, authentication, timeout,
repeatability, pagination, and rate-limit semantics are normalized once. Source
order is retained for ordered entries. Rust types remain opaque.

### Validate

Semantic validation resolves value visibility, endpoint identities,
credential dependencies, public-expression ownership, pagination bindings,
rate-limit profile bindings, effective duplicate targets, host constraints,
and static head ceilings.

Cross-attachment-site rules are checked against each final endpoint leaf.

### Surface

`semantic::surface` derives public names, modules, request capabilities and
trait obligations. Descriptions, Rustdoc, semantic snapshots, diagnostics, and
code emission consume the same resolved model.

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
- decision gates, fixed completion observation, structured errors, trusted
  diagnostics, and redaction.

Extras implement JSON, CSV, NDJSON, OAuth2 client credentials, and the local
rate limiter behind façade features.

## Testing

Unit tests own local invariants. Integration tests own cross-module runtime
behavior. Trybuild files own generated API absence and compiler diagnostics.
Examples own documentation declarations. Cargo fixtures own dependency-graph
and feature-isolation behavior.

`pwsh -NoLogo -NoProfile -File scripts/check_docs_examples.ps1` requires every
`concord` fence under the root README, `FINAL.md`, `docs/`, and `dev_doc/` to
either match whitespace-normalized source in the compiled
`crates/concord-examples/src/docs_spec.rs` fixture or carry a `concord-source`
marker resolving to a compiled anchor. Marked Rust fences are compared against
the same anchors.

Source anchors are restricted to compiled roots:

| Anchor root | Compiled by |
|---|---|
| `crates/concord-examples/src` | the `concord-examples` lib target |
| `crates/concord-examples/tests` | the `concord-examples` integration test targets |

The checker rejects absolute paths, `..` traversal, paths outside those roots,
and non-`.rs` sources. It validates anchor integrity per file: duplicate anchor
names, unopened `ANCHOR_END`, unclosed anchors, overlapping regions, and regions
selecting no source lines all fail. Proper nesting is supported so one fixture
can serve both a whole-contract and a single-endpoint quotation; nested markers
are excluded from the compared source. Local Markdown links are resolved, and
`#fragment` targets are checked against the target document's headings.

Adding an anchor root requires a documented reason and a compiled Cargo target.

### Validation levels

`cargo test --workspace` is not the gate. The `ui` targets set `test = false`,
and MSRV, feature-isolation, documentation, and fixture checks are separate.

| Level | Command | Owns |
|---|---|---|
| fast loop | `just fmt`, `just docs-check` | formatting and documentation contract |
| pre-commit | `just check`, `just test`, `just ui`, `just docs`, `just msrv` | features, runtime tests, compiler contracts, rustdoc, MSRV |
| release equivalent | `just verify` | the pre-commit set plus `fixtures`, `deny`, `workflow`, `analyzer`, `bench` |

`.github/workflows/ci.yml` mirrors these as the `quality`, `test`,
`documentation`, `ui`, `msrv`, `fixtures_policy`, `platform`, and `analyzer`
jobs. CI does not currently invoke `scripts/check_docs_examples.ps1`; that
check is owned by `just docs` and `just docs-check` locally.

Snapshot changes are reviewed before `.stderr` files are updated. A snapshot is
re-blessed only after confirming the new diagnostic proves the property the
fixture exists to prove, not merely that the fixture still fails.

### Specification rejection coverage

| Specification rule | Enforcement |
|---|---|
| unknown and ghost syntax | `crates/concord/tests/ui/fail/syntax/ghost_syntax.rs` |
| defaults and public-expression grammar | `default_expression_restrictions.rs`, `spec_rejections.rs`, and `names/` |
| fixed base, static subdomain, static path, and reserved headers | `spec_rejections.rs`, `ipv4_host_prefix.rs`, and `ipv6_host_prefix.rs` |
| credential compatibility and duplicate authentication targets | `authentication/` and `spec_rejections.rs` |
| pagination syntax, bindings, bodies, modes, and terminal capabilities | `pagination/` |
| rate-limit structure, visibility, bindings, keys, windows, and identity | `rate_limits/` and `spec_rejections.rs` |
| dynamic subdomain labels and path atoms | `concord_core::request::head` unit tests and `concord-core/tests/public_api/generated_contract.rs` |

## Security review

Security tests use unique sentinels and inspect `Debug`, `Display`, sources,
plans, descriptions, gates, completion, limiter metadata, provider failures,
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
`docs/design_invariants.md` owns cross-cutting invariants.
`docs/README.md` owns the canonical user-facing inventory.
Topic guides explain concepts end to end without becoming parallel specifications.
`dev_doc/documentation_contract.md` owns documentation classes, examples, links, and validation.
`dev_doc/finalization_decisions.md` owns historical finalization judgments.

The documentation checker validates DSL fences, source-anchored usage examples, local links, and inventory coverage.

## Active version

Concord 2.0.0 is the sole production language. The generated macro/core
handshake is 32. Selected retired syntax exists only in migration diagnostics;
there is no compatibility semantic mode or alternate executor. `FINAL.md`
records the active constitution and `just verify` is the release-equivalent
gate.
