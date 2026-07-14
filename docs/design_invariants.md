# Design Invariants

Concord is a Rust API-tree DSL and contract compiler.

This page records the design rules that should stay true while the DSL, macro compiler, and runtime evolve.

## API Shape

Concord represents an HTTP API as a typed tree:

```text
client root
  scope or layer branches
    endpoint leaves
```

The client owns base identity, client-wide variables, credentials, the default attachment block, named profiles, and runtime configuration.

Scopes refine route, host, auth, and policy context.

Endpoint stanzas describe individual HTTP operations.

The inverse shapes are not the Concord model. Endpoints do not own clients. Layers do not own clients. A client owns the API contract, layers refine it, and endpoints are the leaves.

## Endpoint Purity

Endpoint leaves should primarily describe endpoint contracts:

- HTTP method
- endpoint name
- typed parameters
- path projection
- query projection
- request body codec
- pagination declaration
- response codec
- response entity output

Cross-cutting policy should be inherited from scopes or named profiles whenever possible.

## Response Terminator

`-> Codec<Response>` is the visual endpoint terminator.

Normal Concord style should avoid placing policy clauses after the response line. This keeps endpoint leaves visually closed by their return contract.

## Macro And Core Boundary

The macro parses and resolves DSL syntax into semantic request data.

The core executes syntax-neutral request plans.

Core runtime code must not depend on raw DSL syntax.

Raw parser syntax may represent rejected forms so diagnostics can point at the right token. Resolved macro IR should be context-specific: ordinary policy, route, and pagination values must not carry auth-secret references after sema. Public expression contexts also must not depend on generated implementation locals such as `auth`, `secret`, `cx`, `ep`, `vars`, `self`, or `request`; sema closes those references before codegen. Codegen should render resolved data and return typed errors for impossible construction failures instead of relying on validation-dependent panics.

Runtime diagnostics are metadata-only for bodies. Debug sinks, stderr debug logs, runtime hooks, and callback-style diagnostics must not receive live request or response body bytes, even truncated or formatted previews.

The request-side authority is a logical body recipe: empty, reusable bytes,
one-shot byte stream, one-shot HTTP body, terminal-body factory, or multipart
recipe. Rebuildability is a property of that recipe, not HTTP method
idempotency or Reqwest request cloneability. Reusable JSON and text are
encoded once. An explicit exact stream length is structurally guarded against
both early EOF and excess bytes; the request limit is separately applied at
the final native request materialization boundary.

Reqwest is the sole general retry authority. Retry mode is selected once while
constructing the managed client: default protocol recovery, disabled, or a
constrained fixed-origin status policy. Concord owns only one bounded explicit
authentication recovery. Body-recipe rebuildability controls that recovery;
Reqwest body cloneability independently controls hidden resends. Materialized
multipart and streams are not Reqwest-cloneable.

Credential-provider HTTP uses a separate managed Reqwest client and one
Concord submission per provider operation. Its native retry mode is limited to
protocol recovery or disabled; application status retry cannot be inherited.
Provider execution does not consume protected endpoint hooks, rate limiting,
pagination state, or application retry eligibility.

Request execution maps logical recipes directly to `reqwest::Body` or
`reqwest::multipart::Form`. There is no public universal-body bridge and no
common request or response abstraction.

The managed client returns `reqwest::Response` directly to core. Status/header
policy inspection happens on that native value. Buffered processing collects
native chunks through one bounded collector, while streaming processing retains
the native response and reads it lazily. Only after terminal buffering does core
construct the stable public Concord response façade.

The logical URL captured before authentication materialization is the sole URL
authority for ordinary buffered, streaming, hook, rate-limit, pagination,
debug, and error metadata. Native response URLs never replace it.

The `dangerous-dev-tools` feature is separate from debug sinks, hooks, stderr
debug output, public errors, and rate-limit metadata. It exposes only narrow
deterministic test infrastructure and is disabled by default. Its native
executor branch exists at the final managed Reqwest boundary, after pre-send
hooks and authentication materialization. Application and provider managed
clients retain separate executor handles and queues. Synthetic success uses a
real `reqwest::Response`, so response hooks, rate-limit feedback, status/auth
classification, bounded response streaming, and decode remain the production
pipeline.

See [Security Model](security_model.md) for the consumer-facing boundary between safe, advanced, and dangerous surfaces.

Protected auth material is applied only when the runtime materializes the native `reqwest::Request`. Logical request state, debug surfaces, hooks, rate-limit contexts, and error contexts remain free of raw auth material.

Pagination is a typed runtime state machine. Page loops must either make deterministic progress, stop explicitly, or return a typed pagination error. Repeated logical page identities are treated as non-progress and fail instead of silently looping.

Pagination caps remain enforced, but they are not the only loop protection.

Credential slots use monotonic generations across all states, including empty states. Auth rejection invalidates only the generation that was applied to the rejected request, older credential completions cannot overwrite newer material, and cancellation of an in-flight credential acquisition must wake waiters without leaving the slot permanently in flight.

Auth locks are not held across credential endpoint or token endpoint I/O.

## Runtime Pipeline

The runtime pipeline order is fixed.

DSL improvements should compile to existing semantic concepts such as auth requirements, rate-limit profiles, codecs, pagination controller types, descriptors, and request plans. Retry remains a client-construction concern outside the endpoint DSL.

Changing runtime order requires dedicated tests and a dedicated PR.

No development feature persists response bodies. Raw response access is a
separate explicit dangerous surface.

Runtime configuration uses clone-on-write, but auth state is shared across cloned clients. Changing runtime configuration on one clone does not retroactively change another clone, while auth-state mutation on one clone can be observed by other clones that share the same auth-state handle. Credential isolation requires a separate client instance or separate auth state, not just `clone()`.

## Simple Path Preservation

Minimal clients must remain short:

```rust
api! {
    client ExampleApi {
        base "https://api.example.com"
    }

    GET Ping
    path ["ping"]
    -> Text<String>
}
```

Advanced APIs must not make simple APIs noisy.

## Readability Rule

The API tree must remain readable without understanding every low-level policy mechanism.

A reader should be able to scan a Concord client in this order:

1. API shape.
2. Endpoint contracts.
3. Named profile and policy attachment.
4. Low-level mechanism details.
