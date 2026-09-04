# ADR 0003 — Word-visible patch operations and explicit `each`

- **Status:** Selected; human gate pending

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

