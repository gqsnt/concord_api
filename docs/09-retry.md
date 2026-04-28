# 9. Retry

Retry policy describes:

- which methods can retry;
- which statuses can retry;
- which transport errors can retry;
- how many attempts are allowed;
- whether `Retry-After` is honored.

## Basic retry profile

```rust
client Api {
    base https "example.com"

    default {
        retry read
    }

    retry read {
        attempts 2
        methods [GET, HEAD]
        on [429, 500, 502, 503, 504]
        retry_after
    }
}
```

`attempts 2` means at most two attempts total: the original request and one retry.

## Scope retry

```rust
scope service {
    retry read

    GET Flaky
        as flaky
        path ["flaky"]
        -> Json<Value>
}
```

## Endpoint retry

```rust
GET Flaky
    as flaky
    path ["flaky"]
    -> Json<Value>
    retry read
```

## Turning retry off

```rust
GET NoRetry
    as no_retry
    path ["no-retry"]
    -> Json<Value>
    retry off
```

## Statuses

```rust
on [429, 500, 502, 503, 504]
```

## Transport errors

If supported by your current build, transport errors can be declared in the retry profile.

Use names matching the core transport categories, such as:

```rust
on transport [Timeout, Connect, Tls, Dns, Io, Request, Other]
```

## Retry-After

```rust
retry_after
```

When present, Concord honors server-provided `Retry-After` where possible.

Retry and rate-limit coordinate so the client does not double-sleep for the same server cooldown.

## Unsafe methods and idempotency

Safe methods such as `GET` and `HEAD` are the normal retry target.

For `POST`, `PUT`, `PATCH`, and `DELETE`, use an idempotency strategy when retrying is safe.

Example:

```rust
retry write {
    attempts 2
    methods [POST]
    on [500, 502, 503]
    idempotency header "Idempotency-Key"
}
```

Endpoint:

```rust
POST CreatePost(idempotency_key: String, body: Json<NewPost>)
    as create
    path ["posts"]
    -> Json<Post>
{
    header "Idempotency-Key" = idempotency_key
    retry write
}
```

## Practical guidance

Use small retry budgets.

Always be explicit for unsafe methods.

Use `retry_after` for APIs that document `Retry-After` on `429` or `503`.
