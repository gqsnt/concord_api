# Runtime-defined endpoints

Use `RuntimeEndpoint` when the application cannot know the public method,
path, query, headers, body specification, or response specification at compile
time. It preserves Concord's managed lifecycle and finite boundaries without
turning a dynamic operation into an untyped native request.

## Build and bind the endpoint

Start with a bounded stable `EndpointId` and an application HTTP method. Add
public shape and typed body/response contracts through the consuming builder:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/runtime_endpoints/generated_and_runtime.rs:runtime_endpoint}}
```

`body_codec::<ExampleCodec>(...)` selects both the request input and buffered
media encoder. `response::<ExampleCodec>()` selects the typed response value.
The same builder can use a `RequestBodySpec`, `ResponseBodySpec`, streaming
response, no-content response, semantic authentication placements, and
semantic rate-limit rules.

`build()` returns a typed `RuntimeEndpoint<R>`. Binding it with
`client.request(endpoint)` returns the ordinary `Request<_>` for `R`; awaiting
that request enters the same executor as a generated endpoint.

## What remains automatic

Runtime-defined requests use the selected client's:

- fixed base URL and immutable transport configuration;
- public-head and aggregate structural limits;
- private authentication boundary;
- rate limiter and response observer;
- before-send, response-head, and completion hooks;
- status handling, body preparation, response acquisition, and safe errors.

Planning also uses the ordinary safe shape:

```rust
let plan = client.plan(&endpoint)?;
```

It reports bounded public method, credential-sanitized URL, public headers,
endpoint identity, timeout selection, and capabilities without executing
hooks, credentials, limiter work, bodies, transport, or completion.

## Incremental builder failure

Path segments, query values, headers, authentication plans, rate rules, and
formatted values are checked while builder methods run. The first structural
failure is latched. Later mutations skip formatting and iterator work, and
`build()` returns the safe configuration category.

For repeated query input, Concord consumes only through the first item it
cannot retain. It does not drain the remainder merely to discover that the
endpoint is already invalid; this matters for expensive or even infinite
iterators.

Dynamic values still use bounded formatting. Errors omit the rejected value
and do not expose partial native state.

## Choose timeout behavior

Runtime endpoint timeout selection has three states:

- omission or `.inherit_timeout()` uses the client's configured runtime
  endpoint timeout;
- `.timeout(duration)` selects an explicit operation duration;
- `.disable_timeout()` removes the operation timeout.

Generated endpoints do not inherit that runtime-endpoint default. They retain
the effective timeout from their declaration. A request-local timeout control
can still replace, disable, or restore either selected result.

Disabling the operation timeout trusts provider, limiter, and result-
determining hook futures not to remain pending. Streaming response polling
after a stream opens remains caller-controlled either way.

## Static versus runtime tradeoff

Prefer the DSL when operation shape is part of the stable service contract.
Generated endpoints provide domain-named methods, compile-time value
ownership, generated Rustdoc, capability-specific request wrappers, and
declarative pagination.

Choose `RuntimeEndpoint` for plugin data, operator configuration, or another
genuinely dynamic public operation. Runtime endpoints intentionally have no
partial runtime pagination API. If pagination semantics are stable, declare a
generated endpoint; if they are dynamic, write an explicitly bounded
application loop.

Runtime endpoint values do not expose native transport. Custom behavior still
must fit a public body, response, credential, limiter, or hook contract.

## Related

- [Requests, planning, and errors](../guide/requests-planning-errors.md)
- [Request bodies and buffered responses](bodies-and-buffered-responses.md)
- [Authentication](authentication.md)
- [Rate limiting](rate-limiting.md)
- [Generated API runtime endpoint reference](../reference/generated-api.md#runtime-endpoints)
