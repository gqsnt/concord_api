# Concord v2 finalization decisions

This record supersedes the former major-version freeze decisions. The normative
contract is the v2 implementation prompt as projected into `docs/spec.md`,
`docs/runtime.md`, and `docs/design_invariants.md`.

## Language and migration

- Keep one language: v2. Retired spellings are parsed only where a focused
  migration diagnostic is possible; no compatibility semantic mode remains.
- Keep Rust type tokens opaque. The migration auditor reports `Option` and `Vec`
  sites for human review rather than guessing presence or cardinality intent.
- Treat the written body input and arrow output as the caller-visible types and
  generate endpoint-local associated-type equality assertions.
- Preserve the fixed base path while allowing generated path append/replace/off.
  Reject empty, dot, dot-dot, slash, backslash, and pre-encoded segments before
  one encoding pass.
- Use exact typed setters. Do not preserve broad conversion-based convenience
  methods or silently suppress methods after name collision.

## Semantic projections

- Assign stable internal owner/value/endpoint identities before validation or
  emission. Emitters consume resolved references rather than looking up tokens.
- Use one resolved model for generated code, Rustdoc, descriptions, diagnostics,
  terminal surface, semantic snapshots, and diff input.
- Keep both local and effective attachment views with source origins in public
  reflection. Deterministic snapshots use tool-private serialization without a
  public wire-format promise.
- Generate the unbound tree from the same lexical nodes as direct navigation and
  prove direct/bound equivalence in runtime tests.

## Runtime truth

- Retain the existing executor/body/auth/limiter/acquisition architecture where
  it already enforces the v2 property; adapt it through the canonical bridge
  rather than creating another path.
- Allocate one absolute deadline per caller terminal and pass it through every
  pagination page, retry, recovery, wait, callback, and conversion.
- Keep semantic repeatability independent from mechanical body replay. Expose
  dispatch knowledge rather than remote-commit claims.
- Separate result-determining request/response gates from non-authoritative
  completion observation. Core completion stays synchronous and does not spawn
  detached unbounded export work.
- Make accepted bytes an explicit generated capability. Raw core requests do
  not acquire the terminal merely because a response mode satisfies a bound;
  runtime endpoints select an explicit byte response contract.
- Require explicit limiter selection for any effective rate policy. Canonical
  key bytes are framed by `RateLimitKeyValue` and absent from ordinary evidence.

## Documentation and evidence

- `docs/README.md` is the exact top-level inventory. The documentation checker
  validates links, fragments, compiled anchors, standalone DSL snippets, and
  fence structure.
- `docs/migration_v1_to_v2.md` is the only user guide expected to spell retired
  public names. UI migration fixtures and parser diagnostics are the other
  intentional locations.
- Compiled examples use domain input/output types rather than spec-associated
  projections where the concrete domain type is known.
- Trybuild snapshots are updated only after confirming that failure proves the
  intended semantic rule. Runtime tests prove ordering, inertness, redaction,
  first excess, deadline aggregation, and direct/unbound/runtime parity.

## Toolchain and release

- Workspace package version is 2.0.0, Rust edition is 2024, MSRV remains 1.86,
  and `unsafe_code` remains forbidden.
- Generated bridge version is 32 in macro and core together.
- `just verify` is the final gate and includes formatting, checks, lint, tests,
  UI, MSRV, fixtures, docs, dependency policy, workflow checks, analyzer, and
  compile/benchmark probes.
