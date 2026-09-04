# Documentation refactor review for v2.1

## Objective

Replace the v1 documentation set with one coherent v2.1 task-oriented set while code lands in semantic slices.

## Risks

- installing target docs before behavior exists;
- leaving v1 and v2.1 examples side by side;
- repeating normative semantics with divergent wording;
- inherited representation hidden from endpoint-first docs;
- release claims stronger than evidence gates;
- source-anchor breakage during large example migration.

## Refactor sequence

1. Add target docs under a versioned implementation branch or staged directory.
2. Land design invariants and vocabulary with the canonical IR phase.
3. Update subsystem docs only with the corresponding code slice.
4. Keep migration guide recording old forms as quotations.
5. Migrate compile-checked examples and anchors in the same change as syntax/public API.
6. Update generated Rustdoc/descriptions before claiming inherited representation is usable.
7. Remove superseded v1 docs/public aliases after all internal call sites move.
8. Run stale-vocabulary search and docs gates.

## Review checklist

- Does each file have one audience and purpose?
- Is every normative statement owned/tested?
- Does the endpoint-first experience show exact type, representation origin, status, effect, and capability?
- Are private/sensitive examples sanitized?
- Are unsupported features described as unsupported rather than “not yet documented”?
- Are provisional cognitive/performance claims labeled?
- Can a migrator distinguish mechanical rewrite from semantic review?
- Are duplicated explanations links rather than divergent restatements?

## Stop condition

The documentation refactor is complete when an unfamiliar user can follow quick start, add one endpoint, inspect inherited representation, understand one failure, and find exact semantics without opening source or a superseded design report.
