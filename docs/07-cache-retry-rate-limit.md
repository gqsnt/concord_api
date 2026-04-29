# 7. Cache, Retry, And Rate Limit

Policies are inherited from client to scope to endpoint.

Use one `default` block per client:

```rust
client Api {
    base https "example.com"

    default {
        cache short
        retry read
        rate_limit app
    }

    cache short {
        ttl 60 seconds
    }

    retry read {
        max_attempts 2
        methods [GET]
        on [429, 500]
        retry_after
    }

    rate_limit app {
        bucket application by [host] {
            500 / 10s
        }
    }
}
```

Endpoint-specific policy is explicit:

```rust
GET Expensive
    as expensive
    path ["expensive"]
    -> Json<Value>
    rate_limit only app
```

`max_attempts` counts the first send. `max_attempts 2` means one initial send
and at most one retry.

Rate-limit response observers are declared with:

```rust
observe rate_limit MyObserver
```

Advanced cache stores, rate limiters, and retry policies import from
`concord_core::advanced::*`.
