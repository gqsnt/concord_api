# Concord v2 documentation review

## Scope

The public documentation was rewritten as one v2 corpus rather than mechanically
renaming the previous language. `docs/spec.md`, `docs/runtime.md`, and
`docs/design_invariants.md` remain the normative owners. Topic guides explain
those rules without defining parallel semantics. `docs/README.md` inventories
every top-level user page exactly once.

## Evidence model

Prominent complete examples use `concord-source` anchors into compiled
`concord-examples` targets. Standalone DSL fences must occur in the compiled
`docs_spec.rs` fixture. The PowerShell checker validates anchor containment and
integrity, normalized source equality, closed fences, local links/fragments,
and documentation inventory coverage.

Anchors are restricted to compiled Rust files beneath
`crates/concord-examples/src` or `crates/concord-examples/tests`. Absolute paths,
upward traversal, other extensions, duplicate/unbalanced/overlapping markers,
and empty regions fail. Proper nesting is supported and nested marker lines are
excluded from rendered source.

## v2 corrections

- All declarations contain descendants inside their client.
- Teaching signatures show the real domain input/output before `using Spec`.
- Optionality, defaults, and repetition are taught as independent facts.
- Query/header/subdomain/path/auth inheritance is described as explicit patch
  algebra.
- Plans are named credential-free and are described as potentially containing
  visible sensitive public data.
- Retry docs include repeatability, replayability, stage, dispatch knowledge,
  budget, attempt, and aggregate deadline gates.
- Pagination docs state one deadline and the exact prefix/complete-within laws.
- Rate docs require explicit limiter enforcement and canonical typed keys.
- Decision gates and completion observation are documented as different
  authorities.
- Runtime endpoint docs use the v2 builder vocabulary and reject native escape
  semantics.
- Migration documentation is the only user-facing home for retired spellings.

## Gate

Documentation is accepted only when `just docs` and the full `just verify` pass.
The final implementation report records the exact run results; this review does
not pre-assert a green gate.
