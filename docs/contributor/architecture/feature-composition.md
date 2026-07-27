# Feature composition and evolution

Cargo features select available implementations and transport capabilities.
They must not select competing semantic lifecycles. Whether an endpoint uses
JSON, records, OAuth2, multipart, or compression, its request still enters the
same core executor.

The exact public feature table belongs to [Feature flags](../../reference/features.md).
This page explains which layer composes each feature and how to evolve that
composition safely.

## Composition layers

| Layer | Feature responsibility | Example |
|---|---|---|
| `concord-core` | compile native/runtime capability and expose semantic contracts | TLS, HTTP/2, multipart transport, decompression, records |
| `concord-extras` | compile an optional first-party trait implementation | JSON, CSV, NDJSON, OAuth2 client credentials, local limiter |
| `concord` façade | forward features, reexport enabled items, install approved defaults | default TLS/HTTP2/rate-limit bundle |
| application Cargo graph | unify all requests for the final package | downstream default/no-default selection |

A declaration that names `Json<T>` or a custom `MyCodec` is the same syntax:
Rust trait checking decides whether the type is available and implements the
contract. The generated inherent surface is feature-neutral, so the façade
passes only its dependency-renaming-safe path to the declaration compiler.

## Default bundle

The façade default set is:

```text
default-tls + http2 + rate-limit
```

This is an application convenience bundle, not core's definition of a valid
runtime. Core and extras have empty default sets. The façade disables sibling
defaults and forwards each selected capability explicitly.

With `rate-limit`, each generated client builder starts with a fresh
façade-composed `LocalRateLimiter`. An application-selected limiter has higher
precedence and is never replaced by generated default installation.

TLS and HTTP/2 are native transport capabilities. Their absence removes the
corresponding configuration/connection support; it does not replace the
managed executor.

## No-default behavior

`default-features = false` is a supported graph, not a smaller undocumented
product:

- generated clients without declared rate rules can build with core's
  fallback limiter;
- a generated client with rate rules fails construction with
  `LimiterUnavailable` unless the application deliberately selects a
  functional limiter or `NoopRateLimiter`;
- a runtime endpoint with a nonempty rate plan and only the fallback limiter
  fails with a rate-limit configuration error before transport;
- first-party codecs/providers/policies are absent, while their public
  semantic traits remain available when owned by core;
- unavailable transport and feature-gated builder methods are absent at
  compile time;
- the normal `.response_bytes()` terminal remains available without a feature
  and is restricted by its sealed response-mode bound.

`fixtures/features/tests/no_local_limiter.rs` proves both fail-closed paths and
the deliberate custom/no-op alternatives in an isolated Cargo graph.

## Feature unification

Cargo features are additive for one package version. If any dependency enables
`concord/json`, the final `concord` package has `json`, even when another edge
used `default-features = false`.

Design consequences:

- enabling an implementation may add reexports, but must not silently change
  unrelated endpoint semantics;
- absence claims require an isolated fixture, not a test inside an
  all-feature workspace build.

Inspect unexpected composition with:

```bash
cargo tree -e features
cargo metadata --format-version 1 --all-features --locked
```

## Extras stay ordinary

First-party implementations receive no private channel:

- `Json<T>` implements public codec contracts;
- `Csv` and `NdJson` implement public record-decoder contracts;
- pagination policies implement `PaginationController`;
- OAuth2 client credentials implement the credential-provider contract and use
  managed provider HTTP;
- `LocalRateLimiter` and `RetryAfterObserver` implement public rate-limit
  contracts.

This makes an extras feature replaceable by a downstream implementation. A
feature that requires native transport access or a friend-only trait is a sign
that the semantic core contract is incomplete.

## Adding or evolving a capability

First classify the change.

### A new concrete implementation

1. Confirm an existing core trait expresses the bounded semantics.
2. Implement it in extras or downstream code.
3. Add an extras feature with only necessary optional dependencies.
4. Forward and reexport it through the façade.
5. Add all-feature success and isolated absence coverage.
6. Update the canonical feature table owner.

No macro branch or generated-contract bump should be needed.

### A new semantic runtime capability

1. Define the public model and lifecycle owner in core.
2. Specify bounds, safe errors, redaction, cancellation, and ordering.
3. Integrate it into the existing executor instead of adding another send
   path.
4. Expose its configuration/trait through focused façade namespaces.
5. Add a concrete extras implementation only if one is broadly useful.

If generated endpoints need to describe the capability, continue with the
compiler path below.

### A new declaration or generated capability

1. Add syntax only after identifying the resolved semantic fact.
2. Resolve and validate it before surface derivation.
3. Put exact names and availability in the surface model.
4. Emit through the façade-relative path.
5. Keep emitted inherent Rust feature-neutral; express type-specific
   availability through ordinary items and semantic trait bounds.
6. Decide whether the existing hidden bridge suffices. Bump the generated
   contract only when private compiler/runtime agreement changes.
7. Add macro unit, facade UI, runtime, Rustdoc, and feature-fixture proofs in
   proportion to the change.

## Evolution non-goals

Do not use a feature to:

- create a second request executor or native client escape;
- let the macro recognize first-party type names;
- make core depend on extras;
- turn a finite mode into an unbounded alternative;
- change existing behavior based on feature unification when simple item
  availability is sufficient;
- add a default merely to make one test graph pass.

## Verification matrix

| Claim | Primary check |
|---|---|
| enabled composition works | workspace all-feature check/test/Rustdoc |
| minimal graph works | workspace no-default check and Rustdoc |
| disabled item is absent | `concord-xtask fixtures` |
| default limiter precedence | core runtime-config and facade integration tests |
| no fallback send with nonempty plans | isolated no-local-limiter fixture |
| macro remains type-neutral | facade public API tests |
| dependency direction remains valid | `concord-xtask policy` |

Run the full feature-sensitive set:

```bash
cargo check --workspace --all-targets --no-default-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo run --locked -p concord-xtask -- fixtures --locked
cargo run --locked -p concord-xtask -- policy --locked
cargo rustdoc -p concord --no-default-features --locked -- -D warnings
cargo rustdoc -p concord --all-features --locked -- -D warnings
```

## Related

- [Crate boundaries](crate-boundaries.md)
- [Generated surface and contract](generated-surface.md)
- [Generated contract versioning](../generated-contract.md)
- [Feature flags](../../reference/features.md)
- [Unsupported forms](../../reference/unsupported.md)
