# Mental model

Think of a Concord contract as five linked projections of one semantic tree.

```text
declaration tree
  -> generated Rust navigation and request controls
  -> unbound intent tree
  -> static descriptions and semantic snapshots
  -> bounded managed execution and evidence
```

## Tree and owners

Each `api!` invocation owns one client. Scopes and endpoints live inside it and
nested scopes live inside their lexical parent. A value is owned by the client,
one exact named scope, or an endpoint. Public expressions make that ownership
visible with `client.name`, `regional.name`, or `endpoint.name`.

The same containment appears in generated modules, direct methods,
`Client::unbound()`, and `ContractDescription`. A scope is an immutable selected
domain context, not a cosmetic namespace.

## Values are independent facts

A value has an opaque payload type, required/optional presence, optional
initializer, observation class, and owner. These facts do not leak into each
other. In particular:

- `x: Option<T>` is a required domain payload;
- `x?: T` adds `Option<T>` owned by Concord;
- `Vec<T>` is not repeated until a placement says `each`;
- `#[redact]` changes observation, not the real request value.

Defaults are evaluated once when their narrowest client/scope/endpoint snapshot
is created and are retained for reset and every later attempt/page.

## Attachments are explicit algebras

Subdomain and path support inherit, ordinary composition, replace, and off.
Query is an ordered multimap patch with assignment, append, remove, replace,
and off. Headers use case-insensitive assignment/removal without generic append.
Authentication placements add, replace, or turn off. Rate policies only add.

The macro resolves these operations with source origins before emission. A
request does not dynamically rediscover inheritance.

## Public and private

Public route/head shaping produces both the real public wire representation and
a redaction-aware observed representation. A plan borrows the inert request and
stops there. During execution, credentials are resolved privately, rate
admission occurs, and only then are credentials placed on a native attempt.

Secrets and credentials never become “redacted public values”; they are absent
from public structures. A redacted ordinary value remains real for wire shaping,
canonical rate keys, and internal pagination identity, while observations see a
fixed marker.

## Terminals and effects

A request is inert until a terminal future is polled. Await returns the declared
output. `.response()` adds accepted metadata. Pagination, accepted bytes, and
credential storage exist only for endpoints that declare and prove them.

One terminal creates one operation deadline. Pagination passes it through every
page, retry, recovery, limiter wait, acquisition, conversion, and controller
step.

## Runtime authority

Retry classification advises; core repeatability, dispatch stage, replayability,
budget, attempt, and deadline gates decide. Request/response gates may reject at
named phases. Completion observers receive an already-fixed bounded outcome and
cannot change it.

Runtime endpoints plug into this same model. They may choose public shape,
specifications, auth, repeatability, and rate policies through bounded builders,
but never receive native request mutation authority.

See the [architecture decision](v2_architecture.md) for projection laws and the
[design invariants](design_invariants.md) for maintainer obligations.
