# Testing API clients

## Goal

Execute the real generated client against deterministic local HTTP support,
assert the public plan and received request, and keep network-dependent service
behavior out of the test suite.

## Complete code

This test imports the same `MinimalApi` and `execute` function used by the
Start guide. It scripts one real 204 response and routes the client through an
explicit loopback proxy:

```rust,ignore
{{#include ../../crates/concord-examples/tests/local_runtime.rs:first_request_loopback}}
```

The declaration itself remains in the example library, so tests do not create
a second synthetic contract that can drift from application examples.

## Why these declarations

`ScriptedServer` owns a finite queue of exact responses. `SafeProxy` directs
the logical `.test` URL to loopback while preserving the normal Concord
transport, request shaping, hooks, authentication boundary, and response
handling. The test never contacts a public host.

Planning the explicit `minimal_api::Health` value checks the stable endpoint
identity through the real public-head preparation path. It is useful for
public method/URL/header assertions, but it is not counted as execution.

`execute(&client).await` crosses the actual managed lifecycle. The test then
asserts one received request, its method, and its encoded target. This catches
drift in declaration shape and runtime binding without parsing generated
source.

The response is not faked at the application return boundary. The 204 travels
through HTTP status handling and the `NoContent` response specification before
becoming `()`.

## Adaptations

Queue status, headers, complete bodies, or explicit chunk sequences to test
buffered decoders, byte streams, records, and status capture. Fragment CSV or
NDJSON across one-byte and arbitrary boundaries rather than assuming a network
chunk equals a record.

Use the TLS test server and CONNECT proxy for trust roots, private
authentication placement, and provider/application separation. Assert the
server's received authenticated data only inside controlled test support;
keep plans and hooks redacted.

For pagination, queue each page and assert the next query state in order. For
rate limiting, inject a deterministic limiter and assert that transport cannot
begin before admission. For cancellation, use explicit gates or paused time
instead of arbitrary sleeps.

Keep construction tests alongside runtime tests. Construction tests import
canonical modules and type-check generated request names, `set_*`, `clear_*`,
`reset_*`, pagination terminals, and feature-gated capabilities without
opening a connection.

Use compile-fail/UI tests for invalid DSL and absent capabilities. Match
focused public diagnostics; do not replace compiler privacy tests with
source-string searches.

## Production notes

Do not make CI depend on a vendor sandbox, DNS, wall-clock delays, or an
operator secret. Live smoke checks, when explicitly owned, are a separate
external gate and must not replace deterministic coverage.

Assert observable contracts, not incidental native implementation details.
Prefer endpoint IDs, sanitized plans, typed errors, received HTTP shape, and
completion outcomes. Avoid snapshots of unstable `Debug` text.

Keep test limits small enough to run quickly while still covering exact
maximum and first-excess behavior where the contract depends on a boundary.
Use maintained workspace fixtures and shared support instead of copying ad hoc
servers into every test module.

## Related

- [Make the first request](../start/first-request.md)
- [Requests, planning, and errors](../guide/requests-planning-errors.md)
- [Hooks and observability](../features/hooks-and-observability.md)
- [Finite behavior](../concepts/finite-behavior.md)
- [Contributor testing](../contributor/testing.md)
