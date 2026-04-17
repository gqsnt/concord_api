# 9. Rate Limiting

Rate-limit policy describes the buckets a request belongs to. The runtime rate limiter uses that plan to acquire permits before transport and to store cooldowns after limited responses.

```rust
rate_limit {
    profile app {
        bucket application by [route.host] {
            cost 1
            limit 500 every 10 seconds
            limit 30000 every 10 minutes
        }
    }

    profile method_read {
        bucket method by [route.host, endpoint] {
            cost 1
            limit 30 every 10 seconds
            limit 500 every 10 minutes
        }
    }

    default app
}
```

## Profiles and default plans

Rate-limit profiles are declared in the client `rate_limit` block.

```rust
client RateLimitDslApi {
    scheme: https,
    host: "example.com",

    rate_limit {
        profile app {
            bucket application by [route.host] {
                limit 500 every 10 seconds
            }
        }

        profile method_read {
            bucket method by [route.host, endpoint] {
                limit 30 every 10 seconds
                limit 500 every 10 minutes
            }
        }

        default app
    }
}
```

An endpoint can add a profile to the inherited default.

```rust
GET Ping {
    rate_limit method_read
    -> Json<()>;
}
```

In the tested behavior, `Ping` receives both the inherited `app` bucket and the endpoint `method_read` bucket.

When multiple layers or profiles accidentally emit exact duplicate buckets, the runtime canonicalizes the final request plan and keeps one copy.

## Turning rate limits off

Use `rate_limit off` to clear inherited rate-limit profiles.

```rust
GET NoLimit {
    rate_limit off
    -> Json<()>;
}
```

The resulting request has an empty rate-limit plan.

`rate_limit off` clears proactive bucket acquisition only. A response policy can still produce a reactive cooldown target (for example endpoint fallback) after a limited response.

## Multiple profiles

Apply several profiles with a list.

```rust
GET GetAccountByRiotId {
    rate_limit [account_standard_method, riot_high_volume_method]
    -> Json<AccountDto>;
}
```

Use this when one upstream endpoint counts against multiple documented quotas.

## Replace inherited profiles with `only`

Use `rate_limit only ...` when a scope or endpoint should replace inherited profiles instead of adding to them.

```rust
GET Special {
    rate_limit only method_read
    -> Json<()>;
}
```

The `only` form is also available for inline rate-limit blocks.

## Inline plans

For one-off behavior, define a plan directly.

```rust
GET Limited {
    rate_limit {
        bucket method by [route.host, endpoint] {
            limit 30 every 10 seconds
        }
    }
    -> Json<()>;
}
```

Profiles are preferable when a plan appears more than once.

## Bucket keys

A bucket is identified by a kind and a key.

```rust
bucket method by [route.host, endpoint] {
    cost 1
    limit 30 every 10 seconds
}
```

Supported key parts include:

- `route.host` for the final request host.
- `endpoint` for the generated endpoint name.
- `method` for the HTTP method.
- A string literal for a static component.
- A named key declared with `rate_limit key`.

## Bucket cost

Use `cost` when one request should consume more than one permit.

```rust
bucket method by [route.host, endpoint] {
    cost 3
    limit 30 every 10 seconds
}
```

`cost` defaults to `1`.

## Scope key binding

Use `rate_limit key` to name a route or scope parameter for bucket keys.

```rust
scope platform {
    params { platform: String }
    host[platform, "api"]
    rate_limit key region = platform

    GET ByRegion {
        rate_limit regional_method
        -> Json<()>;
    }
}
```

Then the profile can use `[region, endpoint]`.

```rust
profile regional_method {
    bucket method by [region, endpoint] {
        limit 1600 every 1 minute
    }
}
```

At runtime, the bucket key contains the actual `platform` value, such as `euw1`.

The same key binding syntax is available inside endpoint blocks:

```rust
GET ByRegion {
    rate_limit key region = platform
    rate_limit regional_method
    -> Json<()>;
}
```

## Profile inheritance

Rate-limit profiles can extend base profiles.

```rust
rate_limit {
    profile base {
        bucket application by [route.host] {
            limit 500 every 10 seconds
        }
    }

    profile heavy extends base {
        bucket method by [route.host, endpoint] {
            limit 50 every 10 seconds
        }
    }
}
```

Use inheritance to share application-wide buckets while adding endpoint-specific buckets.

## Custom response policy

A response policy interprets headers from limited responses.

```rust
rate_limit {
    response custom RiotRateLimitHeaders

    profile app {
        bucket application by [route.host] {
            limit 500 every 10 seconds
        }
    }
}
```

The custom type implements `RateLimitResponsePolicy`.

```rust
#[derive(Default)]
pub struct RiotRateLimitHeaders;

impl RateLimitResponsePolicy for RiotRateLimitHeaders {
    fn observe(&self, ctx: &RateLimitResponseContext<'_>) -> RateLimitObservation {
        if ctx.status != http::StatusCode::TOO_MANY_REQUESTS {
            return RateLimitObservation::continue_();
        }

        let mut observation = RateLimitObservation::limited()
            .with_target(RateLimitTarget::current_plan_or_endpoint());

        if let Some(delay) = parse_retry_after(ctx.headers) {
            observation = observation.with_delay(delay);
        }

        observation
    }
}
```

The response policy can choose which bucket receives a cooldown. If the named bucket is missing from the current request plan, the runtime falls back to an endpoint-level cooldown so the next request still observes the delay.

`parse_retry_after` supports both delta-seconds and HTTP-date header forms.

## Runtime limiter

Install a custom limiter with `with_rate_limiter`.

```rust
let api = RateLimitDslApi::new()
    .with_rate_limiter(Arc::new(GovernorRateLimiter::default()));
```

Tests use recording limiters to assert the generated `RateLimitPlan` without waiting for real time.

## Rate limits, retry, cache, and inflight

The runtime order is important:

- A fresh cache hit returns before rate-limit acquisition.
- Stale cache revalidation does acquire rate-limit permits.
- Retry attempts pass through the limiter again.
- Inflight followers do not acquire their own permits; only the leader sends the shared request.

This prevents duplicate concurrent safe requests from consuming multiple permits for the same transport send.

## Practical guidance

Model upstream documentation directly. If the API has an application quota and a method quota, define both buckets.

Use `route.host` when different regions or tenants have independent limits.

Use custom response policy for APIs that report limit scope in non-standard headers.
