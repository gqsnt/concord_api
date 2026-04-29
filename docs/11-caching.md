# 11. Caching

Cache policy is declared in the DSL and executed by a runtime `CacheStore`.

A fresh cache hit returns before:

- inflight coordination;
- rate-limit acquisition;
- retry;
- transport.

Stale revalidation uses the normal send path.

## Cache profile

```rust
client Api {
    base https "example.com"

    default {
        cache short
    }

    cache short {
        ttl 60 seconds
    }

    cache http_static {
        http
        ttl 1h
        revalidate
        on_error serve_stale
    }
}
```

## Endpoint cache

```rust
GET Cached
    as cached
    path ["cached"]
    -> Json<String>
    cache short
```

## Scope cache

```rust
scope static_data {
    cache http_static

    GET Versions
        as versions
        path ["versions.json"]
        -> Json<Vec<String>>
}
```

## Inline cache

```rust
GET Cached
    as cached
    path ["cached"]
    cache {
        ttl 60 seconds
    }
    -> Json<String>
```

## Turning cache off

```rust
GET Fresh
    as fresh
    path ["fresh"]
    -> Json<String>
    cache off
```

## Cache fields

v5 DSL cache policy is about API semantics:

```rust
cache static_data {
    http
    ttl 1h
    revalidate
    on_error serve_stale
}
```

Supported semantic fields:

| Field | Meaning |
| --- | --- |
| `http` | use HTTP cache semantics |
| `ttl` | default freshness duration |
| `revalidate` | allow conditional revalidation |
| `on_error ignore` | do not serve stale after error |
| `on_error serve_stale` | serve stale after failed revalidation |

Storage details such as capacity, max body size, sharing, and backend choice belong to runtime configuration or the cache store implementation, not the v5 DSL.

## Request cache mode

Per-request cache controls:

```rust
api.static_data()
   .versions()
   .cache_bypass()
   .await?;

api.static_data()
   .versions()
   .cache_refresh()
   .await?;

api.static_data()
   .versions()
   .cache_default()
   .await?;
```

## Cache store extension

`CacheStore` is an advanced extension point.

Use a custom cache store when you need:

- a different backend;
- distributed cache;
- custom keying;
- full HTTP cache semantics;
- custom stale fallback.

The default key should include auth identity so protected responses are isolated per credential.

## Practical guidance

Cache safe reads.

Keep TTLs short unless the upstream API documents long-lived cache behavior.

Use `cache off` for endpoints that must always hit the server.

Use runtime store configuration for deployment concerns.
