# The managed lifecycle

Await a request when you want Concord to perform the operation. Every
generated direct call, explicit generated endpoint, runtime-defined endpoint,
pagination page, and supported terminal enters the same managed lifecycle.
That shared path is why authentication, rate limits, hooks, deadlines,
redaction, and finite body behavior remain consistent.

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/getting_started/minimal.rs:first_request}}
```

`client.health()` constructs an inert request. Execution begins only when the
returned future is polled by `.await`.

## What happens during execution

Users can reason about the lifecycle in these observable stages:

1. **Prepare the public request.** Concord shapes the final host, path, query,
   public headers, timeout selection, and endpoint identity, then validates
   their finite structure.
2. **Prepare body metadata.** If the endpoint has a request body, Concord
   inspects bounded structural metadata before materializing its byte stream.
3. **Run before-send policy.** The before-send hook sees only safe public
   operation data. A hook failure stops the request before transport.
4. **Prepare the body and authority.** Concord prepares or reconstructs the
   managed body, resolves current provider credentials without placing them,
   and acquires every applicable rate-limit rule.
5. **Perform HTTP.** The transport runs under the selected operation deadline.
   Only after limiter admission does Concord finalize private authentication
   and materialize the native request. Managed request streams suppress and
   bound non-progressing empty chunks.
6. **Observe the response head.** The selected limiter sees a safe final
   response head before the user response hook. Either may reject the
   operation before response acquisition.
7. **Resolve or repeat the result.** Concord may perform at most one allowed
   authentication recovery across the whole operation. An explicitly
   configured retry policy may admit another visible attempt; otherwise
   Concord applies declared status acceptance and acquires the buffered,
   streaming, or record response mode.
8. **Complete once.** After success or failure is fixed, completion receives a
   safe outcome event exactly once before the result is delivered.

Credentials are never added during planning or exposed to hooks and safe
descriptions. Limiter authority is acquired before private authentication is
placed. Once a final response arrives, limiter observation happens before the
user response-head hook, and both happen before status or body handling. This
ordering explains why a limiter or hook error can take precedence over a
received HTTP status.

Every admitted managed policy retry reruns the visible attempt stages under
the original operation deadline: before-send, body reconstruction, current
authentication preflight, limiter admission, private materialization,
transport, and response observation. The enclosing request still has one
operation identity, one final result, and one completion event. Private
known-safe protocol recovery remains inside a single visible attempt.

## Deadlines and later polling

The operation timeout covers the managed work needed to open and classify the
response, including authentication and response-head policy. Buffered
responses are fully acquired inside that operation. An accepted streaming
terminal returns an open bounded stream; later user polling has its own
consumer-progress rules and is no longer the original request operation.

Completion is intentionally outside the operation timeout because it observes
an already fixed outcome. It is awaited rather than detached, so application
completion code that remains pending can delay delivery. Treat custom
completion work as trusted application code and give it its own bounded
behavior when necessary.

Cancellation does not turn completion into background work. Concord does not
silently detach correctness work merely because the caller stops waiting.
Feature pages describe the precise cancellation and progress behavior for
streaming, records, pagination, and provider HTTP.

## What this means in application code

Construct and plan freely when no effects are wanted. At execution time,
expect a single ordered policy path and match the resulting typed error
category. Keep hook and provider implementations finite, and avoid holding
application locks across request awaits unless that coordination is
intentional.

Client clones share one immutable runtime snapshot, so concurrent calls use
the same configured transport, hooks, providers, and limiter state. Clients
created by separate builds remain isolated unless your application explicitly
shares a component.

This page is the user-level lifecycle. The current contributor
[architecture overview](../contributor/architecture.md) explains internal
ownership; exact bounds and redaction guarantees remain in their canonical
reference pages.

## Related

- [Requests, planning, and errors](../guide/requests-planning-errors.md)
- [Finite behavior](finite-behavior.md)
- [Security and redaction](security-and-redaction.md)
- [Error reference](../reference/errors.md)
- [Contributor architecture](../contributor/architecture.md)
