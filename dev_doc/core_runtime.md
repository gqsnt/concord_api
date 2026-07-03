# Core Runtime

`concord_core` executes request plans. It does not know DSL syntax and must remain usable by generated code without depending on macro internals.

## Execution Order

The runtime order is:

```text
1. build request plan
2. resolve credentials and attach pending auth slots
3. validate auth collisions against public query and header policy
4. rate-limit acquire
5. materialize a send-only `TransportRequest`
6. transport send
7. drop the materialized transport request
8. classify response or transport failure
9. post-response hook
10. rate-limit observation
11. auth rejection handling
12. retry decision
13. decode response
14. map or transform endpoint value
15. return
```

This order is not user-configurable.

Runtime hooks and rate-limit observation are transport-response metadata observations, not endpoint-success hooks. They may observe HTTP responses that later fail auth handling, retry, or decode and map, but they never receive response body bytes or raw auth material.

Retry is a bounded transport or status decision layer. It runs after transport-response observation and auth rejection handling, and before endpoint decode and mapping. Retry does not handle endpoint decode failures or map failures. `execute_raw()` follows the same planning, auth, rate-limit, transport, classification, hook, and retry path, then returns the classified raw response before endpoint decoding.

Rate-limit acquisition is also transport-metadata only. Requests that cannot be materialized into a valid URL do not acquire a permit. Rate-limit contexts expose only sanitized request metadata and response metadata.

## Invariants

Concord does not coalesce ordinary endpoint requests in v1. Two concurrent identical endpoint requests both acquire their own rate-limit permit and send their own transport request.

Credential acquisition is different from ordinary endpoint execution: `CredentialSlot` may single-flight acquisition or refresh for the same refreshable credential. Concurrent protected requests that all need the same missing credential should produce one credential acquisition and one protected transport send per request after the credential is available. Endpoint-backed credentials remain explicit and are not implicitly acquired by protected requests.

Credential slot generations are monotonic across empty, in-flight, ready, and failed states. Older auth rejection handling, older acquisition completion, or cancelled acquisition cleanup must not clear or overwrite newer credential material.

`BuiltRequest` is the logical request. It contains public route, query, and header data plus typed pending auth slots, but it does not contain raw auth material. Request body bytes stay on the transport side of the boundary; they are not copied into debug, hook, rate-limit, retry, or error metadata.

Pagination drives one logical request per page and always checks page progress. The runtime records every logical request identity seen during a pagination run and returns a typed pagination error if a later page would reuse any previously seen identity instead of advancing.

Pagination is type-driven at runtime: generated endpoints name a pagination controller type, `PaginateBinding` loads and stores endpoint-backed fields, and core owns the loop around `EndpointPagination` implementations.

Controller loop-key checking is an additional pagination defense, not the only non-progress guard. Even when a controller disables its own loop-key check, the runtime request-identity guard remains active for the logical page request.

Public query parameters and public headers cannot silently collide with reserved auth names. Query-auth keys are rejected before transport if they already exist as public query parameters, and bearer, Basic, and custom header-auth names are rejected before rate-limit acquisition and transport if they already exist as public headers.

Debug sinks and runtime hooks are body-free. They may observe safe metadata and redacted headers or URLs, but they must not receive live request or response body bytes.

Debug output must not include request or response body snippets, previews, or formatted excerpts. Body bytes belong only to transport send or bounded response read paths.

The deprecated dev body capture path is deliberately separate from debug sinks and hooks. It is opt-in, response-only, local-file-only, and skips protected auth-bearing requests by default.

Auth preparation does not receive `BuiltRequest` directly. Endpoint auth preparation and auth-internal preparation both receive an auth-only application request that exposes only pending-slot attachment, so custom client contexts cannot insert raw auth into logical headers, query strings, body data, policy data, or request metadata during credential preparation.

`TransportRequest` is materialized only immediately before `Transport::send`. It is the boundary where bearer values, arbitrary auth headers, query-auth values, Basic auth headers, and certificate transport metadata are inserted. Concord drops it after send and does not store it in `BuiltResponse` or `DecodedResponse<T>`.

Rate-limit keying is strict. A bucket keyed by `[host]` requires the logical request URL to have a host and fails before permit acquisition or transport if it does not.

There is no fallback key for `[host]`. Hostless logical URLs fail explicitly rather than being grouped under an empty, endpoint, or static key.

Semantic numeric state uses explicit failure instead of silent saturation. Request and auth attempt counters return typed errors if they overflow.

Runtime state access should fail explicitly instead of panicking. Request execution maps poisoned auth state, rate-limit window state, cooldown state, and other required runtime state into typed auth or runtime-state errors.

Auth rejection handling happens after response classification but before the normal retry decision. Bounded auth refresh is the first recovery path for configured auth rejection responses.

Auth locks are not held across credential endpoint I/O or token endpoint I/O. Slot state transitions mark an in-flight generation before network work, and completion stores material only if the slot still expects that generation.

Retry exhaustion returns the final transport or status error that caused the retry loop to stop, with retry context attached through safe diagnostics. It does not replace the final failure with a generic retry error.

Endpoint response bodies are read into memory only through the bounded body reader. The default runtime limit is 16 MiB, `Content-Length` is checked before reading when present, and chunked or unknown-length bodies are checked cumulatively while reading.

Runtime configuration is client-owned. `RuntimeConfig::default()` starts with no debug output, no-op hooks, no retry policy, the feature-selected default rate limiter, `max_auth_retries = 8`, pagination loop detection enabled, a 16 MiB endpoint response-body limit, and disabled dev body capture. Client configuration is applied before endpoint policy and pending-request overrides. Pending-request overrides cover request options such as debug level, timeout, and attempt; v1 has no per-request override for body limit, hooks, rate limiter, retry policy, or auth retry budget.

Concurrent-request characterization tests cover the same clone-on-write snapshot model under overlap: request-local config and pagination state stay isolated, auth and observer metadata remain request-scoped, and cancelled work does not poison later requests.

Public runtime failures surface through `ApiClientError`. Tests should match variants or `ErrorCategory` for stable behavior and use string assertions only for safety checks such as proving raw auth, secrets, and body bytes are absent from `Display`, `Debug`, `source()` chains, debug sinks, hooks, rate-limit metadata, and retry metadata.

Pagination follows the same per-page runtime order on each page request. Page advancement happens only after the page response has completed endpoint decode and mapping successfully and the pagination runtime has accepted it. Protected auth rejections retry the same page and do not advance state.

Runtime order is covered by characterization tests in `concord_core`.

Endpoint concurrency tests use deterministic gates and explicit arrival counts rather than short sleeps. Timeouts in those tests are deadlock guards, not timing assertions.

Cancellation tests use the same deterministic harness to abort requests after a known phase entry. The supported proofs are phase-local cleanup, no late decode or map, no late page advancement, and no leaked body or auth material in safe metadata.
