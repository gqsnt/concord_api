# Architecture overview

Concord turns a remote HTTP contract into ordinary typed Rust, then runs every
generated and runtime-defined request through one bounded lifecycle. The
compiler and runtime are deliberately separate: compile time decides which
operations are valid and which Rust methods exist; runtime owns effects,
deadlines, protected material, transport, and finite response acquisition.

## System at a glance

```text
remote HTTP contract
        |
        | concord::api! declaration
        v
parse -> resolve and validate -> derive surface -> emit typed Rust
        |                                  concord-macros
        v
client builder -> immutable client -> inert typed request
        |                                  generated surface
        | first poll / await
        v
public head -> hook/body/auth preflight -> limiter -> private HTTP
        -> response policy -> bounded acquisition
        |                                  concord-core
        v
typed output or safe ApiError
```

`concord` is the single application-facing façade around that flow.
`concord-extras` supplies first-party implementations of semantic extension
traits, while `concord-core` remains the owner of the lifecycle itself. Native
HTTP types stay behind core's private boundary.

The [compile-time to runtime trace](architecture/compile-runtime-flow.md)
follows the real `MinimalApi::health` declaration through every layer.

## The architectural boundaries

| Boundary | Decides | Must not decide |
|---|---|---|
| Declaration compiler | grammar, inheritance, generated names, static capabilities, cross-declaration validity | runtime policy objects, credentials, native transport state |
| Generated Rust | typed construction, immutable snapshots, endpoint identity, request capabilities, public-head instructions | a second executor, direct native HTTP, unbounded escape paths |
| Façade | public namespaces, feature forwarding, macro handoff, approved default composition | core semantics duplicated in wrappers |
| Semantic runtime | one deadline, effect ordering, bounds, error reduction, extension calls, response acquisition | application-specific codecs or policy implementations |
| Extras | reusable codecs, providers, controllers, limiters, and observers | privileged access to core internals |
| Native HTTP boundary | connection and protocol mechanics under resolved safe configuration | public request identity, extension APIs, user-visible native errors |

This separation makes two kinds of evolution independent. A declaration can
gain a validation rule without changing the runtime bridge. A runtime
implementation can change behind an existing semantic contract without
changing generated Rust. When emitted code and runtime plumbing must agree on
a new private operation, the
[generated contract version](generated-contract.md) changes atomically.

## Why generated requests are inert

Calling an endpoint constructs an owned snapshot. It does not allocate an
operation ID, start a deadline, call a hook or provider, acquire limiter
authority, encode a body, or open a connection. Effects begin only when the
request future is polled.

That boundary provides:

- predictable composition before execution;
- request-local timeout and debug selection without shared mutation;
- safe planning through the real public-head path;
- cancellation through ordinary future ownership;
- one place to apply policy to generated and runtime-defined endpoints.

It also keeps the generated layer small. Generated endpoint types describe how
to prepare a semantic request; they do not implement transport.

## Why there is one managed executor

Authentication, rate limiting, hooks, bodies, pagination pages, status
capture, and response decoding interact. Allowing each feature to send HTTP
independently would make ordering and cancellation depend on which features
were selected. Core instead sequences them under one operation deadline.

The central ordering has security consequences. Public request shaping and
validation finish before private authentication material is attached. Each
visible attempt runs its hook, body reconstruction, current authentication
preflight, and limiter admission before that attachment. Response limiter
observation finishes before the response-head hook, authentication challenge,
declared status acceptance, and body acquisition. All terminal paths reduce
to a safe result and then attempt one completion event.

See [Managed executor](architecture/managed-executor.md) for the stage
contract and the rationale for each ordering edge.

## Why native HTTP stays private

The native client and request contain details that semantic extensions should
not depend on:

- credential-bearing URLs and headers;
- transport error chains and configuration internals;
- protocol-specific bodies and retry state;
- objects that could bypass response limits or lifecycle hooks.

Concord exposes bounded semantic traits instead. A codec receives bytes and a
finite context. A provider receives a managed HTTP capability. A limiter sees
a safe URL, a bounded plan, and sanitized response headers. Hooks receive
sanitized lifecycle events. First-party extras use those same public
contracts; they have no friend-only transport path.

## Finite and safe by construction

Bounds are applied where data first becomes owned or work first becomes
repeatable. Compile time rejects structurally excessive declarations. Client
construction validates fixed configuration. Request preparation caps public
head construction. The executor controls body bytes, empty-chunk progress,
provider HTTP, status capture, pagination, rate-limit keys, and streamed
responses.

Error conversion happens at the same boundaries. Extension-authored or native
messages are reduced to stable categories or finite safe kinds. Sanitized URLs
and headers are prepared before observation. Exact public limits remain
canonical in [Limits](../reference/limits.md); the architecture pages explain
which layer owns them.

## Change questions

Before changing a boundary, determine:

1. Is this source-language meaning, generated public surface, private bridge
   plumbing, runtime semantics, or a concrete extension implementation?
2. Which layer has enough information to enforce the rule once?
3. Could the change expose protected/native state or create a second effect
   path?
4. What happens at the first excess, timeout, extension failure, or
   cancellation point?
5. Which compiler, runtime, fixture, and public-surface proof owns the result?

Moving behavior to a more convenient crate is not neutral. A macro dependency
on core would couple compiler loading to runtime implementation. A core
dependency on extras would make an optional implementation authoritative. A
native type in an extension trait would make transport replacement and safe
error reduction impossible.

## Verification map

The architecture is tested as behavior, not inferred from file layout:

- `cargo run --locked -p concord-xtask -- policy --locked` checks product
  dependency direction and generated path policy.
- `cargo nextest run -p concord-macros --lib --locked` owns parsing, semantic,
  surface, emission, and bridge unit proofs.
- `cargo nextest run -p concord --test ui --all-features --locked` owns generated
  public capability, privacy, and diagnostic proofs.
- `cargo nextest run -p concord-core --lib --tests --all-features --locked` owns executor ordering,
  cancellation, bounds, and redaction.
- `cargo run --locked -p concord-xtask -- fixtures --locked` owns Cargo-graph,
  renamed-façade, feature-absence, and contract-mismatch proofs.

## Related

- [Crate boundaries](architecture/crate-boundaries.md)
- [Compile-time to runtime flow](architecture/compile-runtime-flow.md)
- [Generated surface and contract](architecture/generated-surface.md)
- [Managed executor](architecture/managed-executor.md)
- [Feature composition and evolution](architecture/feature-composition.md)
- [Macro pipeline](macro-pipeline.md)
