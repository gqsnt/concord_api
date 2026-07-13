# Core Runtime

`concord_core` executes request plans. It does not know DSL syntax and must remain usable by generated code without depending on macro internals.

## Execution Order

The runtime order is:

```text
1. resolve the public request head and body metadata
2. construct the secret-free authentication placement plan
3. validate auth collisions against public query and header policy
4. acquire or refresh credentials and bind material to planned slots
5. produce the body for the physical attempt
6. rate-limit acquire
7. run sanitized request debug and pre_send hooks
8. materialize a send-only native `reqwest::Request`
9. transport send
10. on initial transport failure, run transport_error hook
11. on HTTP response, run post_response hook
12. rate-limit observation
13. classify HTTP status
14. handle auth rejection or retry decision where applicable
15. read bounded response body for buffered responses
16. decode endpoint response
17. return decoded response entity output
18. return
```

This order is not user-configurable.

`pre_send` runs after rate-limit acquisition and before raw auth transport materialization. It may abort before transport. `post_response` runs after an HTTP response is received and before response body read and endpoint decode. It is not an endpoint-success hook: it may observe responses that later retry, fail auth handling, fail rate-limit response observation, fail body-size limits, fail HTTP-status classification, or fail decode. `transport_error` observes initial transport-send failures only; it is not called for HTTP status errors or for body-read failures after a response has been received.

Runtime hooks are sanitized metadata observers, not body-capture or policy hooks. They never receive request body bytes, response body bytes, or raw auth material. Hook and debug callback metadata is sanitized before invocation: sensitive request and response headers, sensitive query values in URLs, and other redacted names are not exposed as raw header maps.

Retry is a bounded transport or status decision layer. It runs after transport-response observation and auth rejection handling, and before endpoint decode. Retry does not handle endpoint decode failures. `execute_raw()` follows the same planning, auth, rate-limit, transport, classification, hook, and retry path, then returns the classified raw response before endpoint decoding.

Rate-limit acquisition is also transport-metadata only. Requests that cannot be materialized into a valid URL do not acquire a permit. Rate-limit contexts expose only sanitized request metadata and response metadata.

## Invariants

Concord does not coalesce ordinary endpoint requests in v1. Two concurrent identical endpoint requests both acquire their own rate-limit permit and send their own transport request.

Credential acquisition is different from ordinary endpoint execution: `CredentialSlot` may single-flight acquisition or refresh for the same refreshable credential. Concurrent protected requests that all need the same missing credential should produce one credential acquisition and one protected transport send per request after the credential is available. Endpoint-backed credentials remain explicit and are not implicitly acquired by protected requests.

Credential slot generations are monotonic across empty, in-flight, ready, and failed states. Older auth rejection handling, older acquisition completion, or cancelled acquisition cleanup must not clear or overwrite newer credential material.

Each `RequestPlan` owns one request-local `PreparedBody`. That capability owns media type, standard `SizeHint`, replayability, and one-shot state, and it produces the body for each physical attempt. Empty and immutable byte bodies are reusable, stream and multipart bodies are one-shot, and custom request entities can supply an explicit replay factory. No endpoint descriptor, request-plan view, retry state, or built request keeps an independent replayability or body-metadata authority.

`BuiltRequest` is created only after public-head preflight, credential preparation, and attempt-body production. It contains public route, query, and header data, the unchanged secret-free placement plan, and the body produced for that attempt, but it does not contain raw auth material. Request body bytes are not copied into debug, hook, rate-limit, retry, or error metadata.

Pagination drives one logical request per page and always checks page progress. The runtime records every logical request identity seen during a pagination run and returns a typed pagination error if a later page would reuse any previously seen identity instead of advancing.

Pagination is type-driven at runtime: generated endpoints name a pagination controller type, `PaginateBinding` loads and stores endpoint-backed fields, and core owns the loop around `PaginationRuntime` / `PaginationRuntimeAdapter` over `EndpointPagination` implementations.

Controller loop-key checking is an additional pagination defense, not the only non-progress guard. Even when a controller disables its own loop-key check, the runtime request-identity guard remains active for the logical page request.

Public query parameters and public headers cannot silently collide with reserved auth names. Query-auth keys are rejected before transport if they already exist as public query parameters, and bearer, Basic, and custom header-auth names are rejected before rate-limit acquisition and transport if they already exist as public headers.

Debug sinks and runtime hooks are body-free. They may observe safe metadata and redacted headers or URLs, but they must not receive live request or response body bytes. The transport still receives the raw request material it needs at the send boundary.

Debug output must not include request or response body snippets, previews, or formatted excerpts. Body bytes belong only to transport send or bounded response read paths.

The deprecated dev body capture path is deliberately separate from debug sinks and hooks. It is opt-in, response-only, local-file-only, and skips protected auth-bearing requests by default.

Auth preparation does not receive `BuiltRequest` directly. Endpoint auth preparation and auth-internal preparation receive an auth-only application request bound to one preplanned slot. Custom client contexts can bind compatible material but cannot choose placement, add query sensitivity, or insert raw auth into logical headers, query strings, body data, policy data, or request metadata.

The native `reqwest::Request` is materialized per physical attempt. It is the boundary where credential values are inserted and is consumed immediately by the managed client. Concord does not retain it in response values.

Rate-limit keying is strict. A bucket keyed by `[host]` requires the logical request URL to have a host and fails before permit acquisition or transport if it does not.

There is no fallback key for `[host]`. Hostless logical URLs fail explicitly rather than being grouped under an empty, endpoint, or static key.

Semantic numeric state uses explicit failure instead of silent saturation. Request and auth attempt counters return typed errors if they overflow.

Runtime state access should fail explicitly instead of panicking. Request execution maps poisoned auth state, rate-limit window state, cooldown state, and other required runtime state into typed auth or runtime-state errors.

Auth rejection handling happens after response classification but before the normal retry decision. Bounded auth refresh is the first recovery path for configured auth rejection responses, but only when the effective absolute attempt cap has capacity; the default cap of one permits no refresh resend.

Auth locks are not held across credential endpoint I/O or token endpoint I/O. Slot state transitions mark an in-flight generation before network work, and completion stores material only if the slot still expects that generation.

Retry exhaustion returns the final transport or status error that caused the retry loop to stop, with retry context attached through safe diagnostics. It does not replace the final failure with a generic retry error.

Endpoint response bodies are read into memory only through the common frame-aware bounded body reader. The default runtime limit is 16 MiB; only the body’s contractual size hint may reject before polling, while every delivered data frame is counted cumulatively.

Runtime configuration is client-owned. `RuntimeConfig::default()` starts with no debug output, no-op hooks, no retry policy, the feature-selected default rate limiter, a 60-second rate-limit cooldown cap, pagination loop detection enabled, a 16 MiB endpoint response-body limit, and disabled dev body capture. Client configuration is applied before endpoint policy and pending-request overrides. Pending-request overrides cover request options such as debug level, timeout, and attempt; absolute retry capacity is supplied by endpoint retry configuration and has no separate authentication budget.

Runtime configuration is clone-on-write, but auth state is shared across cloned clients. Changing runtime config on one clone does not retroactively change another clone, while auth-state mutation on one clone can be observed by other clones that share the same auth-state handle. Credential isolation requires a separate client instance or separate auth state, not just `clone()`.

Concurrent-request characterization tests cover the same clone-on-write snapshot model under overlap: request-local config and pagination state stay isolated, auth and observer metadata remain request-scoped, and cancelled work does not poison later requests.

Public runtime failures surface through `ApiClientError`. Tests should match variants or `ErrorCategory` for stable behavior and use string assertions only for safety checks such as proving raw auth, secrets, and body bytes are absent from `Display`, `Debug`, `source()` chains, debug sinks, hooks, rate-limit metadata, and retry metadata.

Pagination follows the same per-page runtime order on each page request. Page advancement happens only after the page response has completed endpoint decode successfully and the pagination runtime has accepted it. Protected auth rejections retry the same page and do not advance state.

Runtime order is covered by characterization tests in `concord_core`.

Endpoint concurrency tests use deterministic gates and explicit arrival counts rather than short sleeps. Timeouts in those tests are deadlock guards, not timing assertions.

Cancellation tests use the same deterministic harness to abort requests after a known phase entry. The supported proofs are phase-local cleanup, no late decode, no late page advancement, and no leaked body or auth material in safe metadata.
