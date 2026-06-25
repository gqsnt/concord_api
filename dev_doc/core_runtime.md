# Core runtime

`concord_core` executes request plans. It does not know DSL syntax and must remain usable by generated code without depending on macro internals.

## Execution order

The runtime order is:

```text
1. build request plan
2. resolve credentials and attach pending auth slots
3. compute cache identity from the logical request and safe auth partition
4. fresh cache lookup
5. rate-limit acquire
6. materialize a send-only `TransportRequest`
7. transport send
8. drop the materialized transport request
9. classify response/transport failure
10. post-response hook
11. rate-limit observation
12. auth rejection handling
13. retry decision
14. stale cache fallback
15. decode response
16. map/transform endpoint value
17. cache write after endpoint success
```

This order is not user-configurable.

Runtime hooks and rate-limit observation are transport-response metadata observations, not endpoint-success hooks. They may observe HTTP responses that later fail auth handling, retry, stale fallback, or decode/map, but they never receive response body bytes or raw auth material. Cache admission is different: successful values are stored only after endpoint decode and any map/transform succeed.

## Invariants

A fresh cache hit returns before rate-limit acquisition or transport.

Concord does not coalesce ordinary endpoint requests in v1. Two concurrent identical cache-miss endpoint requests both acquire their own rate-limit permit and send their own transport request. Cache may avoid later transport only after a response has been stored.

Concurrent fresh cache hits bypass transport and rate-limit acquisition. Concurrent cache misses are not runtime-coalesced unless a custom cache backend implements its own coordination internally.

Credential acquisition is different from ordinary endpoint execution: `CredentialSlot` may single-flight acquisition/refresh for the same refreshable credential. Concurrent protected requests that all need the same missing credential should produce one credential acquisition and one protected transport send per request after the credential is available. Endpoint-backed/manual credentials remain explicit and are not implicitly acquired by protected requests.

Credential slot generations are monotonic across empty, in-flight, ready, and
failed states. Stale auth rejection, stale acquisition completion, or cancelled
acquisition cleanup must not clear or overwrite newer credential material.
Credential acquisition may coordinate waiters for the same slot, but auth locks
must not be held across credential endpoint or token endpoint I/O.

`BuiltRequest` is the logical request. It contains public route/query/header data, safe auth identities, and typed pending auth slots, but it does not contain raw auth material. Cache keys, debug sinks, hooks, and response metadata operate on this logical request.

Pagination drives one logical request per page and always checks page progress. The runtime records every logical request identity seen during a pagination run and returns a typed pagination error if a later page would reuse any previously seen identity instead of advancing. The old controller loop-key check remains an additional guard, but it is no longer the only defense against repeated pages.

Cache identity is derived before transport materialization. The default cache
key includes the sanitized logical URL plus safe pending-auth metadata:
credential id, usage id, step id, placement, generation when available, and
safe auth identity. Query-auth placement contributes the query key name and
safe identity, never the query-auth value. A protected request whose pending
auth identity is anonymous or otherwise unsafe bypasses cache lookup, stale
fallback, and cache write rather than sharing a public cache entry.

Debug sinks and runtime hooks are body-free. They may observe safe metadata and
redacted headers/URLs, but they must not receive live request or response body
bytes, snippets, previews, or formatted excerpts.

The deprecated dev body capture path is deliberately separate from debug sinks
and hooks. It is opt-in, response-only, local-file-only, and skips protected
auth-bearing requests by default. When enabled, it may capture the received
body before endpoint decode so it remains useful for local diagnosis of bad
provider payloads and decode failures. Release checks treat deprecated use
outside explicit tests as a failure.

Auth preparation does not receive `BuiltRequest` directly. Endpoint auth preparation and auth-internal preparation both receive an auth-only application request that exposes only pending-slot attachment, so custom client contexts cannot insert raw auth into logical headers, query strings, body data, policy data, or request metadata during credential preparation.

`TransportRequest` is materialized only immediately before `Transport::send`. It is the boundary where bearer values, arbitrary auth headers, query-auth values, basic auth headers, and certificate transport metadata are inserted. Concord drops it after send and does not store it in `BuiltResponse` or `DecodedResponse<T>`. Custom transports receive real credentials and must not log them.

Rate-limit keying is strict. A bucket keyed by `[host]` requires the logical request URL to have a host and fails before permit acquisition or transport if it does not. The runtime must not invent fallback key values such as `"<unknown-host>"`; endpoint, method, static, and named key parts remain valid without host data when used alone.

Semantic numeric state uses explicit failure instead of silent saturation. Cache TTL conversions are checked during macro semantic analysis, and request/auth attempt counters return typed errors if they overflow.

Runtime state access should fail explicitly instead of panicking. Request execution maps poisoned auth state, rate-limit window/cooldown state, and other required runtime state into typed auth or runtime-state errors. Cache backends that cannot return `Result` for every operation must still avoid panics and report backend failure through the cache operation result.

Post-response hooks precede rate-limit observation. The `304 NOT_MODIFIED` revalidation path must preserve the same hook then observation ordering before returning the revalidated cached response.

Auth rejection handling happens after response classification but before the normal retry decision. Bounded auth refresh is the first recovery path for configured auth rejection responses. Protected auth rejections do not fall back to stale cache by default, and rejected auth responses are not cached as successful endpoint responses.

Retry decisions happen before stale cache fallback for ordinary failures. Stale fallback is considered only after retry declines or retry budget is exhausted, except for protected auth rejections, which return a typed auth error instead of serving stale cached data.

Successful eligible responses are admitted to cache only after endpoint decode and any map/transform succeed. Auth rejection responses and retryable responses that will be retried are not cached as final successes.

Endpoint response bodies are read into memory only through the bounded body reader. The default runtime limit is 16 MiB, `Content-Length` is checked before reading when present, and chunked or unknown-length bodies are checked cumulatively while reading. Too-large responses fail before decode and before cache write. Cache `max_body` remains a storage eligibility limit and does not control the response read/decode limit.

Pagination follows the same per-page runtime order on each page request: fresh cache lookup, rate limit, transport, classify, hooks, rate-limit observation, auth rejection handling, retry, stale fallback for ordinary failures, decode, map/transform, cache write after endpoint success. Page advancement happens only after the page response has completed endpoint decode/map successfully and the pagination runtime has accepted it. Protected auth rejections retry the same page and do not advance state or serve stale cached data.

Decode and map/transform are the final semantic validation steps before successful cache admission and return. A decode failure does not trigger another transport retry.

Runtime order is covered by characterization tests in `concord_core`.

Endpoint concurrency tests use deterministic gates and explicit arrival counts rather than short sleeps. Timeouts in those tests are deadlock guards, not timing assertions.
