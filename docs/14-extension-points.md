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

Skeleton:

```rust
use concord_core::advanced::{BuiltRequest, Transport, TransportError, TransportResponse};
use std::{future::Future, pin::Pin};

#[derive(Clone)]
pub struct RecordingTransport<T> {
    inner: T,
}

impl<T: Transport> Transport for RecordingTransport<T> {
    fn send(
        &self,
        req: BuiltRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        // Record the final v5 plan-built request before forwarding.
        self.inner.send(req)
    }
}
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

Skeleton:

```rust
use concord_core::advanced::{
    AuthError, AuthErrorKind, AuthFuture, CredentialContext, CredentialId, CredentialProvider,
};
use concord_core::prelude::{ApiKey, ClientContext, SecretString};

pub struct EnvApiKeyProvider;

impl<Cx: ClientContext> CredentialProvider<Cx> for EnvApiKeyProvider {
    type Credential = ApiKey;

    fn id(&self) -> CredentialId {
        CredentialId::new("env-api-key")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, Cx>,
    ) -> AuthFuture<'a, Result<ApiKey, AuthError>> {
        Box::pin(async {
            let key = std::env::var("API_KEY")
                .map_err(|err| AuthError::new(AuthErrorKind::AcquireFailed, err.to_string()))?;
            Ok(ApiKey::new(SecretString::new(key)))
        })
    }
}
```

## Manual endpoint credentials

Endpoint-backed credentials use a manual provider internally.

```rust
credential session = endpoint auth_api::LoginForSession
```

Runtime usage:

```rust
api.auth_api()
    .login_for_session(SessionLoginRequest { /* ... */ })
    .acquire_as_session()
    .await?;
```

The lower-level auth-state API remains available for advanced flows:

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

Skeleton:

```rust
use concord_core::advanced::{
    RateLimitContext, RateLimitFuture, RateLimitPermit, RateLimiter,
};
use concord_core::prelude::ApiClientError;

pub struct DistributedLimiter;

impl RateLimiter for DistributedLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        Box::pin(async {
            // Reserve capacity in Redis, a sidecar, or another shared coordinator.
            Ok(RateLimitPermit)
        })
    }
}
```

## Custom cache store

Use a custom cache store for:

- in-memory cache other than the default;
- disk cache;
- distributed cache;
- special invalidation;
- custom HTTP cache behavior.

The DSL should describe cache semantics; the store controls backend behavior.

Skeleton:

```rust
use concord_core::advanced::{
    BuiltRequest, BuiltResponse, CacheFuture, CacheKey, CacheStore, default_cache_key,
};

pub struct SharedCache;

impl CacheStore for SharedCache {
    fn key_for(&self, request: &BuiltRequest) -> Option<CacheKey> {
        Some(default_cache_key(request))
    }

    fn get<'a>(&'a self, key: &'a CacheKey) -> CacheFuture<'a, Option<BuiltResponse>> {
        Box::pin(async move {
            let _ = key;
            None
        })
    }

    fn put<'a>(&'a self, key: CacheKey, response: BuiltResponse) -> CacheFuture<'a, ()> {
        Box::pin(async move {
            let _ = (key, response);
        })
    }
}
```

## What is not stable v5

Do not rely on these as stable extension points unless the current public API explicitly exposes them:

- custom auth placement in the DSL;
- arbitrary external response codecs;
- direct mutation of generated request plans;
- generated-code internals under `internal`.
