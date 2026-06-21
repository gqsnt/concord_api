# Design Invariants

Concord is a Rust API-tree DSL and contract compiler.

This page records the design rules that should stay true while the DSL, macro compiler, and runtime evolve.

## API Shape

Concord represents an HTTP API as a typed tree:

```text
client root
  scope/layer branches
    endpoint leaves
```

The client owns base identity, shared variables, credentials, defaults, named profiles, and runtime configuration.

Scopes refine route, host, auth, and policy context.

Endpoint stanzas describe individual HTTP operations.

The inverse shapes are not the Concord model. Endpoints do not own clients. Layers do not own clients. A client owns the API contract, layers refine it, and endpoints are the leaves.

## Endpoint Purity

Endpoint leaves should primarily describe endpoint contracts:

* HTTP method
* endpoint name
* typed parameters
* path projection
* query projection
* request body codec
* pagination declaration
* response codec
* optional response mapping

Cross-cutting behavior should be inherited from scopes or named profiles whenever possible.

## Response Terminator

`-> Codec<Response>` is the visual endpoint terminator.

Normal Concord style should avoid placing policy clauses after the response line. This keeps endpoint leaves visually closed by their return contract.

## Macro And Core Boundary

The macro parses and resolves DSL syntax into semantic request data.

The core executes syntax-neutral request plans.

Core runtime code must not depend on raw DSL syntax.

Raw parser syntax may represent rejected forms so diagnostics can point at the right token. Resolved macro IR should be context-specific: ordinary policy, route, and pagination values must not carry auth-secret references after sema. Public expression contexts also must not depend on generated implementation locals such as `auth`, `secret`, `cx`, `ep`, `vars`, `self`, or `request`; sema closes those references before codegen. Codegen should render resolved data and return typed errors for impossible construction failures instead of relying on validation-dependent panics.

Runtime diagnostics are metadata-only for bodies. Debug sinks, stderr debug
logs, runtime hooks, and callback-style diagnostics must not receive live
request or response body bytes, even truncated or formatted previews.
The only body-oriented developer aid is the deprecated, explicit, disabled by
default local response-file capture path; it is not connected to debug sinks,
hooks, or logging.

Credential slots use monotonic generations across all states, including empty
states. Auth rejection invalidates only the generation that was applied to the
rejected request, stale credential completions cannot overwrite newer material,
and cancellation of an in-flight credential acquisition must wake waiters
without leaving the slot permanently in flight. Auth locks are not held across
credential endpoint or token endpoint I/O.

## Runtime Pipeline

The runtime pipeline order is fixed.

DSL improvements should compile to existing semantic concepts such as auth requirements, cache settings, retry settings, rate-limit plans, codecs, pagination plans, and request plans.

Changing runtime order requires dedicated tests and a dedicated PR.

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

Advanced behavior must not make simple APIs noisy.

## Readability Rule

The API tree must remain readable without understanding every low-level policy mechanism.

A reader should be able to scan a Concord client in this order:

1. API shape.
2. Endpoint contracts.
3. Named behavior and policy attachment.
4. Low-level mechanism details.
