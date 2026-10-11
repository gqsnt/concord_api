# ADR 0003 — Word-visible patch operations and explicit `each`

- **Status:** Superseded for v3 by [ADR 0015](0015-direct-placement-and-checked-symbolic-patch-programs.md): the word operations and explicit `each` remain, but plain assignment over an inherited key is no longer a replacement, and direct placement plus structural preconditions are added.
- **v2.1 status (historical):** Selected and mechanically supported on Windows/Linux; human comprehension remains unknown for public freeze.

## Decision

Use assignment, `when_present`, `append`, `remove`, family `replace`, and `off`. Use `each` for zero-or-many occurrences.

## Property earned

Inheritance intent and occurrence cardinality are visible at the placement without lexical `Vec` detection or hidden trait-selected cardinality.

## Rejected

- canonical `?=`/`+=`;
- JavaScript-style `...` spread;
- automatic collection repetition;
- collection wrapper at declaration;
- generic query-object serialization.

## Semantic laws

- assignment clears/replaces even on optional absence;
- `when_present` inherits on absence and replaces on presence;
- `append` preserves and adds;
- present empty `each` under `when_present` explicitly clears.

## Revisit

Only if unfamiliar-user tasks demonstrate another single spelling is materially more accurate/faster. Never publish equivalent aliases together.
