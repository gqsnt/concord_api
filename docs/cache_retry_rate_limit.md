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
            capacity 10_000 entries
            max_body 2 mib
            shared
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
    on transport [Timeout, Connect]
    retry_after
    idempotency header "Idempotency-Key"
}
```

Supported transport retry kinds are `Timeout`, `Connect`, `Tls`, `Dns`, `Io`, `Request`, and `Other`.

## Cache

A cache profile can set a TTL, request revalidation, and allow stale fallback after retry is exhausted.

```rust
cache standard {
    http
    ttl 60s
    revalidate
    on_error serve_stale
    capacity 10_000 entries
    max_body 2 mib
    shared
}
```

A fresh cache hit returns before rate-limit acquisition and transport. Stale fallback is considered only after retry declines or the retry budget is exhausted.

For protected requests, cache identity includes the logical request plus safe
auth identity. Auth material that is inserted later at the transport boundary,
including query-auth parameters, still contributes safe cache identity so two
credentials do not share an entry for the same public URL. Raw bearer tokens,
API keys, auth headers, query-auth values, client secrets, request bodies, and
response bodies are never cache-key material. If Concord cannot construct a
safe auth identity for a protected request, cache lookup, stale fallback, and
cache store are bypassed for that request.

Local cache attachments can use profile names, `only`, `off`, or shorthand patches.

```rust
cache standard
cache only standard
cache off
cache http
cache 5m
cache revalidate
cache stale_on_error
cache {
    max_body 128 kib
}
```

`on_error ignore` disables stale fallback for that cache policy. `on_error serve_stale` enables stale fallback after retry is exhausted. The `cache stale_on_error` attachment is shorthand for serving stale data on error.

Cache sizing fields are public v1 syntax and map to runtime-backed cache configuration.

```rust
cache standard {
    capacity 10_000 entries
    max_body 512 kib
    shared
}
```

- `capacity N entries` limits the maximum number of cache entries.
- `max_body N bytes|kb|kib|mb|mib|gb|gib` limits the cached response body size.
- `shared` enables shared cache mode.

Decimal units are `kb`, `mb`, and `gb`; binary units are `kib`, `mib`, and `gib`. `bytes` uses a multiplier of 1.

Child cache profiles override only the sizing fields they set. Local `cache { ... }` patches change only the provided fields and preserve inherited cache config.

Cache TTL values use checked arithmetic during semantic analysis. Overflowing duration conversions are rejected at compile time instead of saturating to a different value.

Runtime cache state failures are surfaced through cache backend failure handling instead of panicking. A cache backend state failure may produce a cache miss or `NotStored(Backend)` according to the cache operation, but request execution must not panic on a poisoned cache index.

`max_body` is a cache storage limit only. Concord still reads endpoint response bodies under the runtime response-body limit before decode. The default response read limit is 16 MiB and can be changed with `RuntimeConfig::max_response_body_bytes(...)`; `RuntimeConfig::no_response_body_limit()` is the explicit escape hatch. A response that exceeds the read limit fails before decode and before any cache write, and this body-limit failure is not retryable by default.

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

`rate_limit [...]` lists must not be empty and must not contain a duplicate profile name within the same list. Reusing a profile across separate defaults, scopes, endpoints, or behaviors remains valid.

An empty `rate_limit {}` block is rejected because it has no effect. Use `rate_limit off` to clear inherited rate-limit policy, or provide at least one bucket in an inline block.

`[host]` is a strict key part. If a bucket uses `[host]`, the request URL must have a host; otherwise execution fails before rate-limit permit acquisition and before transport. Concord does not invent fallback host values such as `"<unknown-host>"`. Endpoint, method, static string, and named key parts do not require a URL host unless they are combined with `[host]`.

Rate-limit runtime state failures, such as poisoned window or cooldown locks, return typed runtime-state errors. They are reported before transport when the state is required for permit acquisition or cooldown handling.

Internal auth HTTP responses use a separate 1 MiB body limit for token and credential-acquisition calls. That limit is independent from endpoint response reads and from cache `max_body`.

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

1. Build the logical request.
2. Resolve/apply auth into pending slots and sidecar identity.
3. Compute cache identity from the logical request and safe auth partition.
4. Run fresh cache lookup.
5. Acquire rate-limit permits.
6. Materialize `TransportRequest` with raw auth.
7. Send the transport request.
8. Stop exposing the materialized request.
9. Classify the response or transport failure.
10. Run response/error hooks.
11. Observe rate-limit response headers.
12. Handle auth rejection and bounded auth refresh.
13. Apply normal retry policy.
14. Consider stale cache fallback only after retry declines or the retry budget is exhausted.
15. Cache successful eligible raw responses.
16. Decode the endpoint response.

Fresh cache hits bypass rate-limit acquisition and transport. Decode failures do not retry transport and do not use stale fallback. Successful cacheable raw responses are currently cached before endpoint decode. Rate-limit observation is response-based; Concord does not expose a separate transport-error observation API in v1.

`BuiltRequest` and response metadata are safe to inspect: Concord stores auth as typed slots and safe identities until the transport boundary. Custom advanced `ClientContext` auth preparation, including internal auth preparation, must use the `apply_*_credential` helpers; auth hooks do not receive `BuiltRequest` and cannot write raw auth into logical URL or headers. A custom `Transport` receives real credential material in the materialized request and is responsible for not logging it.

Concord does not coalesce ordinary endpoint requests in v1. Concurrent identical cache-miss requests are sent independently; cache may avoid later transport only after a response has been stored.

Poisoned runtime locks are not public panic paths. Request execution reports typed auth or runtime-state errors when auth state, rate-limit state, or other required runtime state is unavailable.

This order is not user-configurable.
