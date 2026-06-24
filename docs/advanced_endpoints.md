# Advanced Endpoints

The facade-first client is the normal API. Advanced endpoint structs are available under `endpoints::*` for focused tests, reusable endpoint values, and explicit request construction.

```rust
let endpoint = example_api::endpoints::GetUser::new(42);
let user = api.request(endpoint).execute().await?;
```

Root endpoints live directly under `endpoints::*`. Scoped endpoints are nested
under their scope module path:

```rust
let endpoint = minimal_api::endpoints::users::GetUser::new(42);
let user = api.request(endpoint).execute().await?;
```

Endpoint setters are available on explicit endpoint values too.

```rust
use concord_core::prelude::PaginationTermination;

let endpoint = example_api::endpoints::ListItems::new()
    .count(50)
    .count_opt(Some(100))
    .clear_count();

let items = api
    .request(endpoint)
    .paginate(PaginationTermination::hard_page_cap(100))
    .collect()
    .await?;
```

The `.paginate(...)` builder is available only for endpoint structs generated
from DSL endpoints that declare `paginate ...`, and it requires an explicit
`PaginationTermination`.

Use `.execute_raw()` when a test or diagnostic needs the classified raw response before endpoint decoding. `execute_raw()` bypasses endpoint cache entirely: it does not read from cache, does not serve stale cache, and does not populate cache because raw execution skips endpoint decode/map and cannot prove endpoint success.

```rust
let raw = api
    .request(example_api::endpoints::GetUser::new(42))
    .execute_raw()
    .await?;
```

Normal application code should prefer facade methods because they preserve the intended high-level API shape.
