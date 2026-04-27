# 14. Extension Points

Normal users should mostly use `concord_core::prelude::*`.

Extension authors use `concord_core::advanced::*`.

## Extension map

| Need | Extension point |
| --- | --- |
| custom transport | `Transport` |
| custom credential source | `CredentialProvider` |
| custom cache backend | `CacheStore` |
| custom rate limiter | `RateLimiter` |
| custom rate-limit response parsing | `RateLimitObserver` |
| custom retry behavior | `RetryPolicy` if exposed by current build |
| runtime hooks/debugging | runtime hooks / debug sinks |

## Custom transport

Use a custom transport for:

- tests;
- non-reqwest HTTP stack;
- recording;
- offline simulation.

Generated clients usually provide:

```rust
let api = client::Client::new_with_transport(...);
```

## Custom credential provider

Credential providers are used when built-in sources are not enough.

Built-in credential materials include:

```rust
AccessToken
ApiKey
BasicCredential
ClientCertificate
```

A provider acquires and refreshes credential material.

Credential slots handle:

- caching current material;
- refresh;
- concurrent waiters;
- manual set/clear;
- generation tracking.

## Manual endpoint credentials

Endpoint-backed credentials use a manual provider internally.

```rust
credential session = endpoint auth_api::LoginForSession
```

Runtime usage:

```rust
api.auth_state()
   .session()
   .acquire(api.auth_api().login_for_session(...))
   .await?;
```

## Custom rate-limit observer

Use this when the upstream API reports rate-limit state in headers.

```rust
#[derive(Default)]
pub struct RiotRateLimitHeaders;

impl RateLimitObserver for RiotRateLimitHeaders {
    fn observe(&self, ctx: RateLimitResponseContext<'_>) -> RateLimitObservation {
        ctx.on_429()
            .scope_header("x-rate-limit-type")
            .retry_after()
    }
}
```

DSL:

```rust
observe rate_limit RiotRateLimitHeaders
```

## Custom rate limiter

Use a custom limiter when permits must be coordinated outside the process.

Examples:

- distributed quota;
- shared Redis buckets;
- observability-only limiter;
- test recording limiter.

## Custom cache store

Use a custom cache store for:

- in-memory cache other than the default;
- disk cache;
- distributed cache;
- special invalidation;
- custom HTTP cache behavior.

The DSL should describe cache semantics; the store controls backend behavior.

## What is not stable v4

Do not rely on these as stable extension points unless the current public API explicitly exposes them:

- custom auth placement in the DSL;
- arbitrary external response codecs;
- direct mutation of generated request plans;
- generated-code internals under `internal`.
