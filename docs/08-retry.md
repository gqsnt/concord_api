# 8. Retry

Retry policies describe which failures can be retried, which methods are eligible, how many attempts are allowed, and whether `Retry-After` is honored.

```rust
retry {
    profile read {
        attempts 2
        methods [GET, HEAD]
        on status[429, 500, 502, 503, 504]
        retry_after honor
        backoff none
    }
    default read
}
```

A default retry profile applies to endpoints that inherit it.

## Profiles

Profiles are declared in the client `retry` block.

```rust
client ApiDslRetryProfile {
    scheme: https,
    host: "example.com",

    retry {
        profile read {
            attempts 2
            methods [GET, HEAD]
            on status[503]
            backoff none
        }
        default read
    }
}
```

An endpoint that inherits `read` retries a matching `503` once, producing attempts `0` and `1`.

## Turning retry off

Use `retry off` to clear inherited retry policy.

```rust
GET NoRetry
-> Json<()>
{
    retry off
}
```

This endpoint returns the first matching status error instead of retrying.

## Scope retry

Retry can be applied to a scope.

```rust
scope service {
    path["api"]
    retry read

    GET Flaky
    -> Json<()>
    {
    }

    GET NoRetry
    -> Json<()>
    {
        retry off
    }
}
```

`Flaky` inherits the scope policy. `NoRetry` clears it.

## Profile inheritance

A profile can extend another profile.

```rust
retry {
    profile base {
        attempts 2
        methods [GET]
        backoff none
    }

    profile read extends base {
        on status[503]
    }
}
```

The `read` profile inherits attempts, methods, and backoff from `base`, then adds the status rule.

## Inline retry policy

An endpoint or scope can define an inline retry patch.

```rust
GET Limited
-> Json<()>
{
    retry {
        attempts 2
        methods [GET]
        on status[429]
        retry_after honor
        backoff none
    }
}
```

Inline policies are useful for one-off endpoints. Profiles are better when several endpoints share behavior.

## Status and transport triggers

Use `on status[...]` for HTTP status codes.

```rust
on status[429, 500, 502, 503, 504]
```

Use `on transport[...]` for transport failures.

```rust
on transport[Timeout]
```

Transport error names accepted by the DSL include the core transport categories such as `Timeout`, `Connect`, `Tls`, `Dns`, `Io`, `Request`, and `Other`.

## Retry-After

`retry_after honor` tells the retry policy to use the server-provided `Retry-After` delay when one exists.

```rust
retry {
    attempts 2
    methods [GET]
    on status[429]
    retry_after honor
    backoff none
}
```

Rate limiting and retry coordinate. If the rate limiter stores a cooldown from a 429 response, retry does not sleep the same server delay a second time. The next retry still passes through the limiter and observes the cooldown there.

## Backoff

The DSL currently supports:

```rust
backoff none
```

This is useful for deterministic tests and APIs where retry timing is controlled by `Retry-After` or the rate limiter.

For advanced retry behavior, implement a custom `RetryPolicy` and install it on the lower-level `concord_core::ApiClient` in integrations that use the core client directly. The generated wrapper exposes DSL retry policy rather than forwarding that low-level setter.

## Unsafe methods and idempotency

Safe or idempotent methods such as `GET` and `HEAD` can be retried when policy allows them.

For unsafe methods such as `POST`, declare an idempotency requirement.

```rust
retry {
    profile write {
        attempts 2
        methods [POST]
        on status[503]
        idempotency header("Idempotency-Key")
        backoff none
    }
}

POST Create
-> Json<()>
{
    retry write
    headers {
        "Idempotency-Key" as idempotency_key: String
    }
}
```

The generated endpoint constructor requires the idempotency key.

```rust
api.request(endpoints::Create::new("create-1".to_string()))
    .execute()
    .await?;
```

A `POST` that applies the retry profile but does not send the declared idempotency header is not retried.

## Runtime attempt metadata

Each transport attempt has request metadata. Tests assert `attempt = 0` for the first send and `attempt = 1` for the retry.

You can set an initial attempt offset on a pending request:

```rust
api.request(endpoints::Ping::new())
    .attempt(10)
    .execute()
    .await?;
```

This is mostly useful for integrations that need external attempt accounting.

## Retry and cache

A fresh cache hit returns before retry and transport. It does not consume retry attempts.

A stale cache revalidation uses normal transport and retry. If revalidation transport fails, retry is attempted before the cache store's `after_error` fallback can return a stale response.

## Practical guidance

Prefer small retry budgets. Retrying too aggressively can amplify upstream incidents.

Always require idempotency for unsafe methods.

Use `retry_after honor` for 429 and 503 responses when the upstream API documents it.
