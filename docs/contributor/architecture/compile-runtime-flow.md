# Compile-time to runtime flow

This page traces one real declaration from source tokens to a completed
request. The declaration is compiled in `concord-examples`, included in the
Start path, and executed against loopback by the example tests.

## The declaration

```rust
{{#include ../../../crates/concord-examples/src/getting_started/minimal.rs:first_api}}
```

There is one client, one fixed base, one `GET` endpoint, one static path atom,
no request body, and a successful response whose body is discarded.

```text
source tokens
   -> semantic endpoint minimal_api.health
   -> MinimalApi + minimal_api::Health + HealthRequest
   -> immutable ClientCore + inert Request<Health>
   -> one managed GET /v1/health operation
   -> ()
```

## 1. Façade handoff

The application invokes `concord::api!`, not the proc-macro crate directly.
The façade wrapper adds one compiler input:

- a dependency-renaming-safe path using `$crate`.

For this declaration the path resolves to the application's `concord`
dependency, even when that dependency has been renamed in `Cargo.toml`. The
generated inherent surface is feature-neutral; Cargo-selected implementations
remain ordinary Rust items and trait implementations rather than private
proc-macro capability tokens.

## 2. Parse

The syntax stage turns tokens into a source tree while preserving spans:

```text
SourceApi
  client MinimalApi
    base "http://minimal.example.test/v1/"
  endpoint health
    method GET
    response NoContent
    path ["health"]
```

Parsing answers grammar questions only. It records named clauses without
requiring an arbitrary textual order and preserves order where meaning is
ordered, such as path atoms and rate-limit dimensions. It does not decide
which generated methods exist.

## 3. Resolve and validate

Semantic resolution walks the lexical tree once and produces the model used
by every later compiler stage. For `health`, it determines:

- the source path `MinimalApi::health`;
- the contract ID `minimal_api.health`;
- the `GET` method and fixed base;
- the effective path `/v1/health`;
- an empty request-body specification;
- `NoContent` as the response specification;
- inherited timeout, authentication, and rate-limit selections;
- the exact absence of pagination and credential-storage capabilities.

Static validation happens against the effective endpoint, after inheritance
and shaping are known. This matters for rules whose validity depends on the
final model, such as pagination-controlled rate-limit keys or host shaping on
an IP base.

## 4. Derive the Rust surface

The surface stage derives names and capabilities from that resolved meaning.
For this declaration it owns:

| Source meaning | Generated artifact |
|---|---|
| `client MinimalApi` | `MinimalApi`, `MinimalApiBuilder`, module `minimal_api` |
| endpoint `health` | `minimal_api::Health` |
| direct bound request | `minimal_api::HealthRequest` |
| client navigation | `MinimalApi::health()` |
| explicit endpoint path | `Health::new()` then `client.request(endpoint)` |
| response selection | direct await, `.response()`, and `.response_bytes()` |
| absent declarations | no pagination or credential-storage terminal |

It also constructs the endpoint documentation model consumed by generated
Rustdoc. Emitters do not rediscover names or capabilities from syntax.

## 5. Emit and type-check

Emission produces ordinary Rust containing:

- the public client, builder, module, endpoint value, and named request;
- a sealed generated endpoint trait;
- private implementations that describe head preparation, body metadata, and
  response type to core;
- a const generated-contract handshake and structural-limit equality checks;
- generated Rustdoc derived from the endpoint documentation model.

All runtime paths go through the façade path supplied in step 1. Emitted code
does not name `concord_core` or a concrete extras type. The Rust compiler then
checks user-supplied body, response, provider, controller, and other
specification types through their semantic trait bounds.

At this point the API is fully compiled, but no client or request exists.

## 6. Build the immutable client

`MinimalApi::new()` creates the façade-selected HTTP and runtime
baselines, validates them, parses the fixed base, and builds one shared core
snapshot. With default features, the façade installs a fresh local limiter;
this declaration has no rate-limit rules, so that policy is simply available
for any compatible runtime endpoint.

The built client holds:

- a shared private core containing base URL, transport, limits, hooks, and
  limiter;
- an immutable generated public-value snapshot;
- an immutable credential-slot store.

Cloning `MinimalApi` shares those objects. Building another client creates a
separate snapshot unless the application explicitly supplies shared extension
objects.

Construction can fail with `ClientBuildError`, but it performs no remote I/O.

## 7. Construct the request

`client.health()` creates `HealthRequest`, which wraps the core
`Request<Health>`. The endpoint value contains the declaration-derived
instructions required to prepare its head.

This remains inert:

- no operation ID is allocated;
- no deadline starts;
- no hook, provider, limiter, codec, or body stream is called;
- no native request is built or sent.

Request-local controls consume and return a new wrapper. The explicit path
through `minimal_api::Health::new()` binds the same endpoint model to the same
core request type.

## 8. First poll starts the operation

Awaiting the named request delegates to core's `IntoFuture` implementation.
Core then:

1. allocates an operation ID and fixes the selected deadline;
2. shapes and validates the public head;
3. obtains finite body metadata;
4. delivers `before_send`;
5. prepares the body;
6. performs authentication preflight;
7. acquires rate-limit authority;
8. finalizes authentication, builds the private native request, and attaches
   protected material;
9. executes native HTTP under the same deadline;
10. observes the sanitized response for limiter policy, then invokes the
    response-head hook;
11. handles authentication challenge or terminal status behavior;
12. applies the selected successful response acquisition;
13. fixes the result and attempts one completion event.

For `NoContent`, successful acquisition validates the response contract and
drops the body without polling it. The final user value is `()`.

The [managed executor](managed-executor.md) documents each stage's inputs,
effects, errors, deadline behavior, protected state, and cancellation
contract.

## Planning is a side branch

An explicit `Health` value can be passed to `client.plan(&endpoint)`. Planning
uses the same public-head shaping and validation path, including response
`Accept` budget accounting. It stops before:

- application body preparation;
- hooks and completion;
- credentials and providers;
- limiter acquisition;
- native request construction or transport.

The result is a sanitized `SafeRequestPlan`, not a preview of an authenticated
wire request.

## Failure ownership along the trace

| Point | Primary failure surface | Main proof owner |
|---|---|---|
| Parse/resolve/surface | focused compiler diagnostic | macro unit tests and facade Trybuild stderr |
| Generated handshake | const-evaluation compiler failure | contract-mismatch fixture |
| Builder | `ClientBuildError` with safe kind/input | facade and core construction tests |
| Public-head preparation | `ApiError` category before transport | core request tests and facade planning tests |
| Policy/provider/hook/body | safe semantic `ApiError` | core deterministic executor tests |
| Transport/status/response | reduced transport or bounded response failure | core loopback/deterministic tests |
| Cancellation | future is dropped; completion is not guaranteed | core cancellation tests |

The declaration itself is compiler-owned by
`crates/concord-examples/tests/construction.rs`. Its loopback execution is
owned by `crates/concord-examples/tests/local_runtime.rs`.

## Related

- [Macro pipeline](../macro-pipeline.md)
- [Generated surface and contract](generated-surface.md)
- [Managed executor](managed-executor.md)
- [Generated API reference](../../reference/generated-api.md)
- [Your first API](../../start/first-api.md)
