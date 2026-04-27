# 13. Runtime and Request Lifecycle

The DSL generates typed endpoints.

The runtime executes request plans.

Most users interact with the generated client and pending requests. Extension authors use the advanced runtime traits.

## Plan-based execution

Generated endpoint facade calls produce a request plan internally.

```rust
api.users().get(42).await?;
```

Under the hood:

```text
Endpoint::plan -> RequestPlan -> ApiClient::execute_plan
```

## Runtime order

A request follows this order:

1. build URL from base, host fragments, path segments and query;
2. apply inherited headers, query, timeout, cache, retry and rate-limit policy;
3. encode the request body;
4. prepare auth;
5. ask cache before sending;
6. return immediately on a fresh cache hit;
7. acquire rate-limit permits;
8. coordinate inflight duplicate requests;
9. send through transport;
10. observe rate-limit response data;
11. let auth inspect response and invalidate/retry if needed;
12. update cache after accepted responses;
13. apply retry decision if needed;
14. decode response;
15. apply endpoint mapping.

## Cache interaction

Fresh cache hits skip transport, retry, and rate-limit acquisition.

Stale revalidation goes through transport and rate-limit.

## Auth interaction

Auth is prepared before cache lookup so cache keys can include auth identity.

If a credential is rejected, Concord can invalidate the exact credential generation used and retry within the auth retry limit.

## Rate-limit and retry interaction

A 429 response can inform both rate limit and retry behavior.

The runtime should avoid double sleeping the same `Retry-After` delay.

## Inflight interaction

Inflight coordination prevents duplicate safe requests from being sent concurrently.

Followers reuse the leader response.

## Transport

Transport is an advanced extension point.

Generated clients usually use the default transport or `new_with_transport` in tests.

## Debugging

Use debug level globally:

```rust
let api = users_api::UsersApi::new()
    .with_debug_level(DebugLevel::V);
```

Or per request:

```rust
api.users()
   .get(42)
   .debug_level(DebugLevel::VV)
   .await?;
```

`DebugLevel::V` is concise.

`DebugLevel::VV` includes more request/response details.

## Runtime configuration

Generated clients may expose methods such as:

```rust
.with_debug_level(...)
.with_pagination_caps(...)
.with_rate_limiter(...)
.with_cache_store(...)
```

Advanced runtime configuration belongs to `concord_core::advanced`.

## What normal users should know

Most users need only:

```rust
api.scope().endpoint().await?;
```

and maybe:

```rust
.debug_level(...)
.timeout(...)
.cache_bypass()
.paginate()
```

The rest is for library integrators and tests.
