# Cache, Retry, And Rate Limit

Cache, retry, and rate-limit behavior is declared as named profiles and attached through defaults, scopes, or endpoints.

## Profiles

```rust
client PolicyApi {
    base "https://example.com"

    policies {
        retry read {
            max_attempts 2
            methods [GET]
            on [429, 500, 502, 503, 504]
            retry_after
        }

        cache standard {
            ttl 60s
            revalidate
            on_error serve_stale
        }

        rate_limit app {
            bucket application by [host] {
                100 / 1s
            }
        }
    }

    defaults {
        retry read
        cache standard
        rate_limit app
    }
}
```

Flat `retry`, `cache`, and `rate_limit` profile declarations remain valid. `policies { ... }` and `defaults { ... }` are the preferred grouped form for larger clients.

## Retry

`max_attempts` is the total number of send tries, including the first send. `retry_after` honors `Retry-After` response headers for retryable statuses.

```rust
retry read {
    max_attempts 2
    methods [GET]
    on [429, 500]
    retry_after
}
```

## Cache

A cache profile can set a TTL, request revalidation, and allow stale fallback after retry is exhausted.

```rust
cache standard {
    ttl 60s
    revalidate
    on_error serve_stale
}
```

A fresh cache hit returns before rate-limit acquisition and transport. Stale fallback is considered only after retry declines or the retry budget is exhausted.

## Rate Limit

Rate-limit profiles define buckets and keys.

```rust
rate_limit app {
    bucket application by [host] {
        100 / 1s
    }
}
```

Multiple profiles can be applied to one endpoint.

```rust
GET Search
    path ["search"]
    rate_limit [app, search]
    -> Json<SearchResponse>
```

A response observer can translate provider headers into rate-limit observations.

```rust
#[derive(Default)]
pub struct ProviderRateLimitHeaders;

impl RateLimitObserver for ProviderRateLimitHeaders {
    fn observe(&self, ctx: RateLimitResponseContext<'_>) -> RateLimitObservation {
        ctx.on_429().scope_header("x-rate-limit-type").retry_after()
    }
}
```

```rust
observe rate_limit ProviderRateLimitHeaders
```

## Overrides

Narrower layers can clear inherited policies.

```rust
GET Uncached
    path ["uncached"]
    cache off
    rate_limit off
    -> Text<String>
```

## Runtime Order

The runtime order is fixed:

1. Build and validate the request plan.
2. Resolve required credentials. Missing credentials fail here, before cache lookup or transport.
3. Apply auth to the request.
4. Compute cache and inflight identity after auth injection, so authenticated requests do not collide across credentials.
5. Return a fresh cache hit before inflight coordination, rate-limit acquisition, or transport.
6. Join an existing inflight request when applicable. Followers do not acquire rate-limit permits.
7. Acquire rate-limit permits for the request that will actually be sent.
8. Send the transport request.
9. Classify the response or transport failure.
10. Observe rate-limit response headers after classification.
11. Handle auth rejection and bounded auth refresh before normal retry decisions.
12. Apply normal retry policy. Retryable send failures or retryable statuses are retried before decode.
13. Consider stale cache fallback only after retry declines or the retry budget is exhausted.
14. Cache successful eligible raw responses after classification.
15. Decode the endpoint response. Decode failures do not retry transport.

This order is not user-configurable.
