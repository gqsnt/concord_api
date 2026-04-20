# 17. Cache System Architecture (Implemented + Extensible)

This chapter records the cache model implemented in Concord DSL and `concord_core`.

It stays simple for normal users while keeping clear extension points.

## 1) Design goals

1. Simple defaults: cache behavior should work with very little configuration.
2. Explicit control: user can bypass or refresh cache per request.
3. Correctness first: never return wrong-identity data, never hide unsafe behavior.
4. Extensibility: custom stores should be easy to implement.

## 2) Keep what already works

1. Keep `cache { profile ... default ... }` in client blocks.
2. Keep endpoint/scope `cache <profile>`, inline `cache { ... }`, `cache only`, and `cache off`.
3. Keep runtime order: auth -> cache lookup -> inflight/rate-limit/retry/transport.
4. Keep default key behavior: method + sanitized URL + auth identity partitioning.
5. Keep HTTP semantics in default backend (`MokaCacheStore`).

## 3) Request cache modes

Define only 3 runtime request cache modes:

1. `Default`: current behavior.
2. `Bypass`: skip lookup and skip store/update.
3. `Refresh`: skip lookup, do transport, then store/update.

Generated request API:

```rust
api.request(endpoints::Me::new())
    .cache_default()
    .execute()
    .await?;

api.request(endpoints::Me::new())
    .cache_bypass()
    .execute()
    .await?;

api.request(endpoints::Me::new())
    .cache_refresh()
    .execute()
    .await?;
```

These three modes are explicit and map directly to runtime behavior.

## 4) DSL policy surface

The cache DSL includes:

1. `revalidate true|false`.
2. `on_error ignore|serve_stale` (maps to current core failure mode).

Example:

```rust
cache {
    profile read {
        ttl 60 seconds
        revalidate true
        on_error serve_stale
        max_body 2 mib
        capacity 64 mib
    }
    default read
}
```

No new invalidation DSL is added yet.

Reason: current default invalidation (unsafe success invalidates same-URI GET) is acceptable, and advanced invalidation can remain in custom stores.

## 5) Core runtime shape

### A. Correctness guarantees

1. Fix variant index drift/leak risk in `MokaCacheStore` by pruning index on eviction/invalidation.
2. Fix 304 edge case when cached entry disappears before merge.

Required 304 behavior:

1. If revalidation key still exists: merge and return cached body as today.
2. If key is gone: treat as miss and do one unconditional fetch, then continue normally.

### B. Request cache mode integration

1. Add `CacheRequestMode` to built request metadata.
2. Respect mode in `before_request` and `after_response` integration path.

Semantics:

1. `Default`: unchanged.
2. `Bypass`: no cache lookup, no revalidation, no store, no stale fallback.
3. `Refresh`: no lookup/revalidation; successful response can store/update.

### C. Stable `CacheStore` extension API

Keep existing trait as primary extension API:

1. `key_for/get/put` for simple stores.
2. `before_request/after_response/after_error` for advanced HTTP semantics.

Add only optional extras (default no-op methods), if needed:

1. `clear_all()`
2. `invalidate_key(...)`

This keeps existing custom stores working.

## 6) DX and observability

Current DX improvements are:

1. explicit request-level controls (`cache_default`, `cache_bypass`, `cache_refresh`)
2. explicit profile controls (`revalidate`, `on_error`) without hidden behavior

Optional future extension:

1. add a cache event hook (`cache_event(...)`) for host app telemetry
2. emit compact event names: `hit`, `miss`, `revalidate`, `stored`, `invalidated`, `stale_fallback`, `bypass`, `refresh`

## 7) Validation matrix

Must pass these behaviors:

1. Fresh hit skips inflight/rate-limit/retry/transport.
2. `cache_bypass` always hits transport and does not mutate cache.
3. `cache_refresh` always hits transport and updates cache.
4. `revalidate false` disables conditional requests.
5. `on_error serve_stale` can return stale after failed revalidation.
6. Auth identity still partitions cache entries.
7. Unsafe successful write invalidates same-URI GET cache.
8. 304 with missing prior entry performs unconditional retry once (no empty decode failure).
9. Existing custom stores still compile.

## 8) Implementation status

Implemented:

1. `CacheRequestMode` and generated request helpers (`cache_default`, `cache_bypass`, `cache_refresh`)
2. runtime mode handling in cache lookup/store paths
3. 304 fallback with one unconditional refresh retry when prior cache merge cannot complete
4. Moka variant index cleanup/pruning for safer invalidation and eviction behavior
5. DSL parse/sema/codegen for `revalidate true|false`
6. DSL parse/sema/codegen for `on_error ignore|serve_stale`

Not implemented (optional future):

1. event hook API for host telemetry
2. optional cache admin helpers (`clear_all`, `invalidate_key`) on generated clients

## 9) Why this is the right balance

1. It removes real pain points (runtime cache control, 304 edge case, missing DSL toggles).
2. It avoids introducing a second cache DSL or cache-specific mini-language.
3. It keeps the extension story stable: simple trait for simple stores, lifecycle hooks for advanced stores.
4. It is incremental and low risk to ship.
