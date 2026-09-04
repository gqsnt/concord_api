# ADR 0001 — Inherited directional representations with inline exceptions

- **Status:** Selected; trait/driver and inheritance mechanical gates supported; human gate pending

## Decision

Allow independent `request R;` and `response R;` defaults at client/scope
owners. Allow inline `via R` exceptions on body/result positions. Add
scope-local `request explicit;` and `response explicit;` fences for
heterogeneous subtrees.

The selected trait relationship is:

```text
RequestRepresentation<Input>
ResponseRepresentation<Output>
```

Response representations select one core-sealed `ResponseDriver`. Core alone
owns native response acquisition and body polling. An endpoint records its
exact input and output independently, so representation markers may be
zero-sized and may implement more than one exact pair.

## Property earned

Normal format-homogeneous APIs can remain domain-shaped, while exceptional
endpoints keep representation visible beside the affected caller type.
Downstream representations can convert core-owned bounded inputs without
receiving native response authority or exposing driver types to callers.

## Mechanical evidence

On 2026-09-04 the pair and sealed-driver shape passed:

- Rust 1.86 checks for core, macros, extras, façade, and no-default-feature builds;
- all workspace test targets and the maintained runtime tests;
- a downstream-style zero-sized marker implementing multiple request and
  response pairs;
- generated endpoint and runtime endpoint construction using exact anchored
  output types.

The prototype exposed and fixed generated-module shadowing of domain output
types. Generated endpoint implementations now refer to invocation-owned type
aliases rather than resolving an authored bare type inside an endpoint module.
Independent request/response defaults, scope overrides, inline exceptions,
explicit fences, missing/wrong pair diagnostics, description provenance, and
semantic diff are covered by the macro/UI/runtime suites.

## Rejected

- wrapper towers or aliases in ordinary caller output;
- one coupled `format Json` shorthand;
- output-type-only inference;
- an extension-defined native acquisition mode;
- associated decoder/driver types in ordinary endpoint return types.

## Risks and pending controls

Inherited representation remains a hidden dependency. Preamble ordering,
provenance in descriptions/docs/diff, explicit fences, and focused pair
diagnostics are implemented; unfamiliar-user evidence is still blocked.
Stream erasure/performance remains Gate 8 work and is not decided by this ADR.

## Revisit

If authors cannot reliably locate the effective representation or pair
diagnostics remain remote/confusing, narrow or remove inheritance rather than
adding aliases.
