# 10. Caching

Concord cache policy is declared in the DSL and executed by a runtime `CacheStore`.

A fresh cache hit returns before inflight coordination, rate-limit acquisition, retry, and transport. A stale cache entry can revalidate through the normal send path.

## Client cache profiles

Define profiles in a client-level `cache` block.

```rust
client CachedApi {
    scheme: https,
    host: "example.com",

    cache {
        profile short {
            ttl 60 seconds
            capacity 1024 entries
        }

        profile http_profile {
            http
            ttl 60 seconds
            capacity 64 mib
            max_body 2 mib
            revalidate true
            shared false
            on_error ignore
        }

        default short
    }
}
```

A `default` profile applies to endpoints that inherit cache policy.

## Endpoint cache

An endpoint can apply a named profile.

```rust
GET Cached {
    path["cached"]
    cache short
    -> Json<String>;
}
```

If the client has a default cache profile, the endpoint can omit `cache short` and inherit the default.

## Inline cache policy

An endpoint or scope can define cache policy inline.

```rust
GET Cached {
    path["cached"]
    cache {
        ttl 60 seconds
        max_body 2 mib
    }
    -> Json<String>;
}
```

Inline cache also works without a client-level cache block. In that case the generated client still configures the default backend when the `cache-moka` feature is enabled.

## Cache patching and `only`

Inline cache policy patches inherited cache config by default.

```rust
cache {
    profile tiny {
        ttl 60 seconds
        max_body 1 bytes
    }
    default tiny
}

GET Cached {
    path["cached"]
    cache {
        max_body 2 mib
    }
    -> Json<String>;
}
```

The endpoint keeps the inherited TTL and overrides `max_body`.

Use `cache only ...` to replace inherited config instead of patching it.

```rust
GET Cached {
    cache only short
    -> Json<String>;
}

GET InlineOnly {
    cache only {
        ttl 30 seconds
        max_body 1 mib
    }
    -> Json<String>;
}
```

## Turning cache off

Use `cache off` to clear inherited cache.

```rust
GET Uncached {
    path["uncached"]
    cache off
    -> Json<String>;
}
```

In tests, two calls to an uncached endpoint produce two transport requests even when the response is otherwise cacheable.

## Cache profile fields

The DSL supports these fields:

```rust
profile http_profile {
    http
    ttl 60 seconds
    capacity 64 mib
    max_body 2 mib
    revalidate true
    shared false
    on_error ignore
}
```

`http` selects HTTP semantics mode.

`ttl N seconds` or `ttl N minutes` supplies a default TTL when a response does not provide explicit HTTP freshness headers.

`capacity N entries` bounds by entry count.

`capacity N bytes|kb|kib|mb|mib|gb|gib` bounds by weighted size.

`max_body N bytes|kb|kib|mb|mib|gb|gib` skips storing responses larger than the limit.

`revalidate true|false` controls conditional revalidation for stale entries.

`shared true|false` controls shared-cache behavior for HTTP cache semantics.

`on_error ignore|serve_stale` controls fallback behavior after a failed revalidation attempt.

## Per-request cache mode

Use request-level cache controls when a call must bypass cache or force refresh.

```rust
api.request(endpoints::Cached::new())
    .cache_bypass()
    .execute()
    .await?;

api.request(endpoints::Cached::new())
    .cache_refresh()
    .execute()
    .await?;
```

`cache_bypass()` skips cache lookup and skips cache store/update.

`cache_refresh()` skips lookup, performs transport, then stores/updates cache on success.

## Required feature for the default backend

The generated client configures `MokaCacheStore` when any DSL cache policy requires the default backend.

This code is behind the consuming crate feature named `cache-moka`. That feature must enable `concord_core/cache-moka`.

Example from `concord_examples`:

```toml
[features]
default = ["cache-moka"]
cache-moka = ["concord_core/cache-moka"]
```

If a DSL cache policy needs the default backend and the feature is missing, code generation emits a compile error:

```text
cache default backend requires a `cache-moka` crate feature that enables `concord_core/cache-moka`
```

## Fresh hits

A cacheable GET stores the response after a successful transport response.

```rust
GET Cached {
    path["cached"]
    -> Json<String>;
}
```

If the first response includes `Cache-Control: max-age=60`, then a second call returns the cached body and does not call transport.

The tested behavior also confirms that a fresh hit skips rate-limit acquisition and retry.

## No-store

`Cache-Control: no-store` is not stored.

If the first response is `no-store` and the second response is cacheable, two calls produce two transport requests and return two different values.

## Vary

`Vary` creates separate variants based on request headers.

```rust
GET Localized {
    params { lang: String }
    path["localized"]
    headers { "accept-language" = lang }
    -> Json<String>;
}
```

If responses include:

```text
Vary: accept-language
Cache-Control: max-age=60
```

then `en-US` and `fr-FR` are cached separately. A later `en-US` call reuses the `en-US` cached response.

## Auth identity isolation

Auth runs before cache lookup. Auth usages can record auth identity, and the default cache key includes that identity.

```rust
client CacheAuthApi {
    scheme: https,
    host: "example.com",

    secret { api_key: String }

    auth {
        credential api_key: ApiKey(secret.api_key)
    }

    use_auth HeaderAuth("Authorization", api_key)

    cache {
        profile short { ttl 60 seconds }
        default short
    }
}
```

If the secret changes from `one` to `two`, the cache entries are isolated. Switching back to `one` reuses the original entry for `one`.

## Revalidation with ETag

Stale entries can revalidate with conditional headers.

If the first response contains:

```text
Cache-Control: max-age=0
ETag: "etag-1"
```

then the next request sends:

```text
If-None-Match: "etag-1"
```

If the server returns `304 Not Modified`, Concord returns the cached body and refreshes the stored headers and HTTP cache policy.

## Unsafe invalidation

Successful unsafe methods invalidate matching cached GET entries for the same URI.

```rust
GET Read {
    path["resource"]
    -> Json<String>;
}

POST Write {
    path["resource"]
    -> Json<String>;
}
```

A typical flow:

1. `Read` stores `before`.
2. `Read` returns `before` from cache.
3. `Write` succeeds and invalidates the cached read.
4. `Read` sends transport again and stores `after`.

## Cache store trait

The simple path is to implement `key_for`, `get`, and `put`.

```rust
impl CacheStore for MyCache {
    fn key_for(&self, request: &BuiltRequest) -> Option<CacheKey> {
        Some(default_cache_key(request))
    }

    fn get<'a>(&'a self, key: &'a CacheKey)
        -> Pin<Box<dyn Future<Output = Option<BuiltResponse>> + Send + 'a>>
    {
        Box::pin(async move { self.lookup(key).await })
    }

    fn put<'a>(&'a self, key: CacheKey, response: BuiltResponse)
        -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
    {
        Box::pin(async move { self.store(key, response).await })
    }
}
```

For full HTTP semantics, override lifecycle methods:

- `before_request(request) -> CacheBefore`
- `after_response(request, response, revalidation) -> CacheAfter`
- `after_error(request, error, revalidation) -> Option<BuiltResponse>`

`before_request` can return a fresh hit, a revalidation request with patched headers, a miss, or bypass.

`after_response` can store, update, skip, or invalidate.

`after_error` can optionally serve stale data after a failed revalidation.

## Default cache key

`default_cache_key` includes method, sanitized URL, and auth identities.

Sensitive query values such as `api_key`, `token`, `secret`, and `password` are redacted with a hash instead of stored in plaintext in the key.

## Practical guidance

Cache only endpoints that are safe to cache. GET endpoints are the normal target.

Use short TTLs unless the upstream API documents cache behavior clearly.

Set `max_body` to prevent large responses from filling cache capacity.

Use `cache off` for unsafe endpoints or reads that must always hit the server.
