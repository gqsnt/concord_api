# 4. Runtime Config

Normal users configure generated clients through the generated surface and the
prelude:

```rust
use concord_core::prelude::*;

let api = runtime_config_api::RuntimeConfigApi::new()
    .with_debug_level(DebugLevel::V)
    .configure(|cfg| {
        cfg.pagination.max_pages = 10;
        cfg.pagination.max_items = 1_000;
    });
```

Use `configure` or `with_configure` when the default client needs runtime
integration. Extension types live in `concord_core::advanced`:

```rust
use concord_core::advanced::*;

let api = runtime_config_api::RuntimeConfigApi::new()
    .with_configure(|cfg| {
        cfg.rate_limiter(std::sync::Arc::new(NoopRateLimiter));
        cfg.cache_store(std::sync::Arc::new(NoopCacheStore));
    });
```

The normal request path remains facade-first:

```rust
let health = api.health().await?;
```

Generated clients also expose `new_with_transport` for tests and non-reqwest
integrations.
