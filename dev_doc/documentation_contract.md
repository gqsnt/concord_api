# Documentation contract

This file defines documentation audiences, ownership, validation, and change
rules.

## Document classes

| Class | Purpose | Normative? |
|---|---|---|
| tutorial | leads a reader through a working sequence | no |
| guide | recommends how to model, use, or extend Concord | no |
| explanation | reconstructs concepts and design causality | no |
| reference | defines exact public grammar or runtime contract | yes where named below |
| maintainer record | explains implementation, proof, or historical judgment | internal |

## Normative owners

- `docs/spec.md` owns DSL grammar, merge rules, structural validation, and diagnostics.
- `docs/runtime.md` owns execution order, attempts, deadlines, response modes,
  pagination runtime, cancellation, observation, and runtime configuration.
- `docs/design_invariants.md` owns cross-cutting implementation and extension
  invariants.
- Topic guides explain concepts end to end but must link to the normative owner
  instead of creating incompatible rules.
- `dev_doc/finalization_decisions.md` owns historical finalization judgments.

## Canonical index

`docs/README.md` is the canonical inventory for user-facing Markdown under
`docs/`. Every other top-level `docs/*.md` page must be linked exactly once from
that index.

The root `README.md` is a product entry point and links to the documentation
index rather than reproducing its full inventory.

## Examples

Every fenced Concord DSL block under the root README or `docs/` must either:

1. match normalized source in `crates/concord-examples/src/docs_spec.rs`; or
2. carry a `concord-source` marker referencing a compiled source anchor.

Prominent generated-usage Rust blocks carry the same marker and must match one
compiled source anchor. Unmarked Rust fragments are explanatory and must not be
presented as complete copy-paste programs.

Marker format:

```text
<!-- concord-source: crates/concord-examples/src/path.rs#anchor_name -->
```

Anchors use:

```text
// ANCHOR: anchor_name
...
// ANCHOR_END: anchor_name
```

## Link and inventory validation

`scripts/check_docs_examples.ps1` validates:

- closed Markdown fences;
- DSL fence language and compiled source;
- marked Rust/DSL source anchors;
- local relative Markdown links and their `#fragment` targets;
- canonical docs inventory coverage;
- absence of references to removed user-documentation files.

### Source-anchor guarantees

An anchor reference is untrusted metadata and must resolve to compiled source.
The checker rejects absolute paths, `..` traversal, non-`.rs` files, and any
path outside these roots:

- `crates/concord-examples/src`
- `crates/concord-examples/tests`

Both roots are compiled by `concord-examples` Cargo targets, so an anchored
example cannot drift from code that builds. Adding a root requires a documented
reason and a compiled target.

Anchor integrity is validated per source file. Duplicate anchor names, an
`ANCHOR_END` with no opening, an unclosed anchor, overlapping regions, and a
region selecting no source lines are all failures. Proper nesting is supported
so a single fixture can serve both a whole-contract and a single-endpoint
quotation; nested markers never appear in rendered documentation.

Errors name the documentation file, the anchor, the source path, and the exact
reason, using repository-relative paths and never source contents.

## Change rules

A public syntax, generated API, runtime behavior, or invariant change updates
its normative owner first. Guides and examples then update to explain the
change.

A guide-only clarification must not create a new rule absent from the normative
reference. Historical records are amended with a supersession note rather than
rewritten to pretend the earlier decision never existed.

Documentation examples are code. Their source anchors, tests, and links are
reviewed with the same care as public API code.
