# Concord v2 architecture decision

- **Status:** accepted
- **Audience:** contributors and API reviewers

## Product identity

Concord is a contract language that projects one semantic API model into a
domain-shaped Rust client, bounded managed execution, and inspectable
operational evidence. The macro and core serve that experience; they are not
separate user-facing products.

## Projection laws

One declaration tree produces isomorphic source ownership, direct method
navigation, unbound intent navigation, generated module/type ownership, and
contract descriptions. Direct construction and unbound-then-bind construction
with the same values produce the same endpoint identity, public head, private
attachments, policy, capabilities, and executor events.

One resolved model feeds code, Rustdoc, descriptions, diagnostics, and semantic
diff snapshots. Rust type tokens remain opaque throughout resolution.

Planning and execution share public shaping. Planning stops before body
materialization, credentials, rate admission, gates, operation identity, or
transport. Execution continues from the same resolved operations through one
managed executor.

## Deliberate v2 choices

- Exactly one client lives in each `api!` invocation, with all descendants
  lexically inside it.
- Owners are explicit: `client.x`, `scope_name.x`, and `endpoint.x`.
- `?` adds one Concord-owned outer `Option`; it never inspects the written type.
- Defaults are retained snapshot values, so reset does not require `Clone` or
  reevaluation.
- Query is an ordered multimap patch; headers are a case-insensitive
  single-value patch; subdomain and path have append/replace/off algebras.
- Body input and response output are written before `using Spec`.
- Repeatability is semantic permission independent of mechanical body replay.
- Pagination owns only declared endpoint slots and shares one aggregate
  deadline.
- Rate keys use canonical typed encoding and rate-limited clients require an
  explicitly selected limiter.
- Plans are credential-free and redaction-aware, not universally “safe.”
- Decision gates and completion observers have different authority.
- Accepted bytes is an explicit endpoint capability.

## Non-goals

Concord is not a native `reqwest` mutation API, middleware framework, OpenAPI
replacement, Rust expression evaluator, dependency-injection container, or DSL
for application retry/transport/proxy policy. It does not expose unrestricted
callbacks or invent traits for static DSL tokens.

## Migration posture

This is one v2 language and one executor. Production-facing v1 spellings and
aliases are removed. The parser recognizes selected old forms only to emit
targeted migration errors. The optional `xtask migration-audit` reports opaque
`Option`/`Vec` sites requiring human review; it cannot infer their former
intent.

See [migration guidance](migration_v1_to_v2.md), the normative
[specification](spec.md), and the [design invariants](design_invariants.md).
