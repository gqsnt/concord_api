# Core runtime

`concord_core` executes request plans. It does not know DSL syntax and must remain usable by generated code without depending on macro internals.

## Execution order

The runtime order is:

```text
1. build request plan
2. resolve/apply auth
3. compute cache/inflight identity after auth
4. fresh cache lookup
5. inflight coordination
6. rate-limit acquire
7. transport send
8. classify response/transport failure
9. post-response hook
10. rate-limit observation
11. auth rejection handling
12. retry decision
13. stale cache fallback
14. cache write
15. decode response
```

This order is not user-configurable.

## Invariants

A fresh cache hit returns before inflight coordination, rate-limit acquisition, or transport.

Inflight followers join the sender result and do not acquire rate-limit permits or send transport.

Post-response hooks precede rate-limit observation. The `304 NOT_MODIFIED` revalidation path must preserve the same hook then observation ordering before returning the revalidated cached response.

Auth rejection handling happens before normal retry. Bounded auth refresh is the first recovery path for configured auth rejection responses.

Retry decisions happen before stale cache fallback. Stale fallback is considered only after retry declines or retry budget is exhausted.

Successful eligible raw responses are cached after classification. Auth rejection responses and retryable responses that will be retried are not cached as final successes.

Decode happens last. A decode failure does not trigger another transport retry.

Runtime order is covered by characterization tests in `concord_core`.
