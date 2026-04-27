# 10. Rate Limiting

Rate-limit policy describes which buckets a request belongs to.

The runtime limiter uses the plan to:

1. acquire permits before transport;
2. store cooldowns after limited responses.

## Basic profile

```rust
client RiotClient {
    base https "riotgames.com"

    observe rate_limit RiotRateLimitHeaders

    default {
        rate_limit app
    }

    rate_limit app {
        bucket application by [host] {
            500 / 10s
            30000 / 10m
        }
    }

    rate_limit match_v5_method {
        bucket method by [host, endpoint] {
            2000 / 10s
        }
    }
}
```

## Applying profiles

Inherited default:

```rust
default {
    rate_limit app
}
```

Endpoint-specific addition:

```rust
GET GetMatch(match_id: String)
    as get_match
    path [match_id]
    -> Json<MatchDto>
    rate_limit match_v5_method
```

Multiple profiles:

```rust
rate_limit [account_standard_method, riot_high_volume_method]
```

Replacing inherited plans:

```rust
rate_limit only match_v5_method
```

Disabling rate limits:

```rust
rate_limit off
```

## Bucket syntax

```rust
bucket method by [host, endpoint] {
    2000 / 10s
}
```

Bucket fields:

| Part | Meaning |
| --- | --- |
| `method` | bucket kind |
| `[host, endpoint]` | bucket key |
| `2000 / 10s` | max requests per window |

Multiple windows:

```rust
bucket application by [host] {
    500 / 10s
    30000 / 10m
}
```

## Bucket keys

Common key parts:

```text
host
endpoint
method
"literal"
named key
```

Use `host`, not old `route.host`.

## Cost

If supported by the parser, use `cost` when one request consumes more than one permit.

```rust
bucket method by [host, endpoint] {
    cost 3
    30 / 10s
}
```

`cost` defaults to `1`.

## Observing rate-limit responses

v4 uses `RateLimitObserver`.

Example:

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

Declare it:

```rust
observe rate_limit RiotRateLimitHeaders
```

The observer reports what the server said. The runtime maps that information to the active request buckets.

## Runtime limiter extension

Custom limiters live under `concord_core::advanced`.

A limiter receives the generated `RateLimitPlan` and decides when a request may proceed.

Use this for:

- testing recorded plans;
- process-local token buckets;
- distributed rate limiting;
- custom cooldown storage.

## Runtime interactions

A fresh cache hit skips rate-limit acquisition.

A stale cache revalidation does acquire permits.

Retry attempts pass through the limiter again.

Inflight followers should not consume their own permit when they reuse a leader request.

## Practical guidance

Model upstream docs directly.

If an API documents an application quota and a method quota, define both buckets.

Use response observers only for server-provided cooldown semantics.
