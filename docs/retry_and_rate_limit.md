# Retry And Rate Limit

Retry and rate-limit policy is declared as named profiles and attached through `default`, scopes, or endpoints.

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

    default {
        retry read
        rate_limit app
    }
}
```

Flat `retry` and `rate_limit` profile declarations remain valid. `policies { ... }` and `default { ... }` are the preferred grouped form for larger clients.

## Retry

Concord's core physical-attempt state machine is the sole general retry authority. It owns request-local policy, recipe reconstruction, general-attempt capacity, delays, retry admission, rate limiting, hooks, and attempt metadata. The managed Reqwest client is permanently configured with `reqwest::retry::never()`, so each native execution represents exactly one observable Concord physical attempt.

`max_attempts` is the total number of general attempts, including the initial send. The independently bounded authentication-recovery resend does not consume this capacity, and general retries do not consume the one-resend authentication-recovery budget. `retry_after` honors `Retry-After` response headers for retryable statuses.

When an endpoint uses inherited retry settings, `RuntimeConfig::max_attempts(...)` independently supplies the absolute cap and defaults to one. A custom `RetryPolicy` only classifies an outcome as `Stop` or `Retry`; it cannot own or report a ceiling. `RuntimeConfig::respect_retry_after(...)` is the inherited-policy opt-in for bounded server-directed timing.

Retry is a bounded transport/status decision layer. Retry decisions happen after transport-response metadata observation and auth rejection handling, and before endpoint decode. Retry does not handle decode failures.

General retry is available only when the authoritative logical body recipe can construct a fresh terminal body. Empty bodies, reusable bytes (including JSON and text), complete request-body factories, factory-backed streams and advanced bodies, all-reusable direct multipart, and complete multipart factories are replayable. Direct byte streams, direct advanced bodies, and direct multipart containing a one-shot stream are not. Body replayability is independent of HTTP-method idempotency; both recipe reconstruction and the configured idempotency policy must allow a general retry. Concord never uses native request cloneability to make this decision.

Every replay reconstructs the logical request plan. Factories run exactly once for each physical attempt that reaches body production, and every attempt receives fresh exact-length, request-limit, producer, and multipart framing state. Direct reusable multipart creates a fresh native `reqwest::multipart::Form`, fresh parts, and a Reqwest-owned boundary and complete `Content-Type` on every attempt.

Retry policy only classifies an outcome as stop or retry. Ordinary retries do not sleep. When `retry_after` is enabled, a valid server-directed `Retry-After` value may delay an otherwise admitted additional attempt, bounded by `max_rate_limit_cooldown(...)`; malformed or unsafe values do not create a delay. Rate-limit handling shares that same admissible wait so one server signal cannot cause two sleeps.

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

A response observer can translate provider headers into rate-limit observations. The callback sees a sanitized header view: sensitive names such as `Set-Cookie`, `WWW-Authenticate`, and token-like headers are redacted before callback access, while `Retry-After` and non-sensitive rate-limit headers remain available.

Rate-limit acquisition happens after secret-free auth collision preflight, credential preparation, and physical-attempt body production, and before hooks, debug, auth materialization, or transport send. It is transport-metadata only. Rate-limit response observation is also metadata only and does not expose request body bytes, response body bytes, raw auth material, or raw sensitive response headers.

Rate-limit response cooldowns are capped as well. The default maximum cooldown duration is finite and configured through runtime settings. The default governor runtime also keeps a fixed maximum number of stored cooldown entries. Remote `Retry-After` values above the configured duration cap fail closed before Concord stores or sleeps on the cooldown, and attempts to store a new distinct cooldown entry after the entry cap is reached fail closed instead of silently growing without bound. Expired cooldown entries are pruned before the entry cap is enforced. Custom rate-limit observers and response policies cannot force a cooldown above these bounds through the default governor runtime.

With `rate-limit-governor` enabled, the default rate limiter enforces declared plans. With `default-features = false`, the default rate limiter fails closed for non-empty declared plans so they do not vanish silently. Empty plans still succeed. `NoopRateLimiter` remains the explicit opt-out when a caller intentionally wants no enforcement.

Rate-limit configuration, acquire, and response-action failures now surface as structured `ApiClientError::RateLimit` values with an inspectable `RateLimitErrorKind`. The execution order and retry behavior are unchanged.
Pure transport errors do not produce response observation.

`rate_limit [...]` lists must not be empty and must not contain duplicate profile names within the same list. Reusing a profile across separate default, scope, endpoint, or profile attachment sites remains valid. Empty inline `rate_limit {}` blocks are rejected because they have no effect. Use `rate_limit off` to clear inherited rate-limit policy.

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

The execution order is fixed. A response selected for resend completes post-response hooks and rate-limit feedback before authentication classification and then general retry classification. Its response and origin lease are released before any general-retry delay. The next retry reserves admission only after that delay, then continues through ordinary reconstruction, rate-limit acquisition, hooks, and one native send.

For each physical attempt:

1. Resolve the public request head and body metadata.
2. Derive secret-free auth placements and validate collisions.
3. Resolve credentials against the planned slots.
4. Produce the physical-attempt body.
5. Acquire rate-limit permits.
6. Run sanitized hooks and debug output.
7. Materialize a native `reqwest::Request` with raw auth and immediately execute it through the managed client.
8. Classify the response or transport failure.
9. Run response and error hooks.
10. Observe rate-limit response metadata.
11. Handle auth rejection and the independently bounded one-resend auth recovery.
12. Apply normal retry policy.
13. Decode the endpoint response and return the decoded response entity output.
14. Return the final value.

`#[cfg(feature = "dangerous-raw-response")] execute_raw_response()` follows the same planning, auth, rate-limit, transport, classification, hook, and retry path, then returns the classified raw response before endpoint decoding. It still obeys the configured response-body limit.

Raw execution still traverses the transport scheduling layer, so rate-limit acquire and response observation semantics remain in effect.
