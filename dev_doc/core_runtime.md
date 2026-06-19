# Core runtime

`concord_core` executes request plans. It does not know DSL syntax and must remain usable by generated code without depending on macro internals.

## Execution order

The runtime order is:

```text
1. build request plan
2. resolve credentials and attach pending auth slots
3. compute cache/inflight identity from the logical request and safe auth partition
4. fresh cache lookup
5. inflight coordination
6. rate-limit acquire
7. materialize a send-only `TransportRequest`
8. transport send
9. drop the materialized transport request
10. classify response/transport failure
11. post-response hook
12. rate-limit observation
13. auth rejection handling
14. retry decision
15. stale cache fallback
16. cache write
17. decode response
```

This order is not user-configurable.

## Invariants

A fresh cache hit returns before inflight coordination, rate-limit acquisition, or transport.

Inflight followers join the sender result and do not acquire rate-limit permits or send transport.

`BuiltRequest` is the logical request. It contains public route/query/header data, safe auth identities, and typed pending auth slots, but it does not contain raw auth material. Cache keys, inflight keys, debug sinks, hooks, and response metadata operate on this logical request.

Auth preparation does not receive `BuiltRequest` directly. Endpoint auth preparation and auth-internal preparation both receive an auth-only application request that exposes only pending-slot attachment, so custom client contexts cannot insert raw auth into logical headers, query strings, body data, policy data, or request metadata during credential preparation.

`TransportRequest` is materialized only immediately before `Transport::send`. It is the boundary where bearer values, arbitrary auth headers, query-auth values, basic auth headers, and certificate transport metadata are inserted. Concord drops it after send and does not store it in `BuiltResponse` or `DecodedResponse<T>`. Custom transports receive real credentials and must not log them.

Post-response hooks precede rate-limit observation. The `304 NOT_MODIFIED` revalidation path must preserve the same hook then observation ordering before returning the revalidated cached response.

Auth rejection handling happens before normal retry. Bounded auth refresh is the first recovery path for configured auth rejection responses.

Retry decisions happen before stale cache fallback. Stale fallback is considered only after retry declines or retry budget is exhausted.

Successful eligible raw responses are cached after classification. Auth rejection responses and retryable responses that will be retried are not cached as final successes.

Decode happens last. A decode failure does not trigger another transport retry.

Runtime order is covered by characterization tests in `concord_core`.
