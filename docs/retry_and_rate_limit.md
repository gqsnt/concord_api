# Retry And Rate Limit

Retry and rate-limit behavior is declared as named profiles and attached through defaults, scopes, or endpoints.

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

        rate_limit app {
            bucket application by [host] {
                100 / 1s
            }
        }
    }

    defaults {
        retry read
        rate_limit app
    }
}
```

Flat `retry` and `rate_limit` profile declarations remain valid. `policies { ... }` and `defaults { ... }` are the preferred grouped form for larger clients.

## Retry

`max_attempts` is the total number of send tries, including the first send. `retry_after` honors `Retry-After` response headers for retryable statuses.

Retry is a bounded transport/status decision layer. Retry decisions happen after transport-response metadata observation and auth rejection handling, and before endpoint decode. Retry does not handle decode failures.

Invalid or overflowing retry delays return a typed configuration error rather than panicking or sleeping.

```rust
retry read {
    max_attempts 2
    methods [GET]
    on [429, 500]
    on transport [Timeout, Connect]
    retry_after
    idempotency header "Idempotency-Key"
}
```

Supported transport retry kinds are `Timeout`, `Connect`, `Tls`, `Dns`, `Io`, `Request`, and `Other`.

## Rate Limit

Rate-limit profiles define buckets and keys.

```rust
rate_limit app {
    bucket application by [host, endpoint, method, "static"] {
        cost 1
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

Rate-limit acquisition happens after request planning, auth preparation, and auth collision validation, and before transport send. It is transport-metadata only. Rate-limit response observation is also metadata only and does not expose request body bytes, response body bytes, or raw auth material.

Rate-limit configuration, acquire, and response-action failures now surface as structured `ApiClientError::RateLimit` values with an inspectable `RateLimitErrorKind`. The execution order and retry behavior are unchanged.
Pure transport errors do not produce response observation.

`rate_limit [...]` lists must not be empty and must not contain duplicate profile names within the same list. Reusing a profile across separate defaults, scopes, endpoints, or behaviors remains valid. Empty inline `rate_limit {}` blocks are rejected because they have no effect. Use `rate_limit off` to clear inherited rate-limit policy.

`[host]` is strict. If a bucket uses `[host]`, the request URL must have a host. Hostless `[host]` keying fails before permit acquisition and before transport. Concord does not invent fallback host key values such as `"<unknown-host>"`. Endpoint, method, static string, and named key parts remain valid without host data when used alone.

Rate-limit runtime state failures, such as poisoned window or cooldown locks, return typed runtime-state errors. Rate-limit contexts do not expose request body bytes, response body bytes, or raw auth material.

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

Named rate-limit keys are bound where their source variables are visible.

```rust
rate_limit tenant_bucket {
    bucket method by [tenant_key] {
        5 / 1s
    }
}

scope tenants(tenant_id: String) {
    path ["tenants", tenant_id]
    rate_limit key tenant_key = tenant_id
}
```

Narrower layers can add profiles, replace with `only`, or clear with `off`.

```rust
rate_limit [app, tenant_bucket]
rate_limit only tenant_bucket
rate_limit off
```

## Overrides

Narrower layers can clear inherited policies.

```rust
GET Unmetered
    path ["unmetered"]
    retry off
    rate_limit off
    -> Text<String>
```

## Runtime Order

The execution order is fixed:

1. Build the logical request.
2. Resolve required credentials and prepare auth material.
3. Validate auth collisions against public query and header policy.
4. Acquire rate-limit permits.
5. Materialize `TransportRequest` with raw auth.
6. Send the transport request.
7. Classify the response or transport failure.
8. Run response and error hooks.
9. Observe rate-limit response metadata.
10. Handle auth rejection and bounded auth refresh.
11. Apply normal retry policy.
12. Decode the endpoint response and return the decoded response entity output.
13. Return the final value.

`execute_raw()` follows the same planning, auth, rate-limit, transport, classification, hook, and retry path, then returns the classified raw response before endpoint decoding.

Raw execution still traverses the transport scheduling layer, so rate-limit acquire and response observation behavior remain in effect for `execute_raw()`.
