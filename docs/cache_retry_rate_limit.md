# Cache, Retry, And Rate Limit

Cache, retry, and rate-limit behavior is declared as named profiles and attached through defaults, scopes, or endpoints.

## Profiles

```rust
client PolicyApi {
    base https "example.com"

    default {
        retry read
        cache standard
        rate_limit app
    }

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
```

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
2. Apply auth.
3. Check cache.
4. Apply inflight coordination.
5. Acquire rate-limit permits.
6. Send transport request.
7. Classify response.
8. Observe rate-limit response.
9. Decide auth refresh and retry.
10. Store or fallback through cache.
11. Decode the endpoint response.

This order is not user-configurable.
