# 6. Pagination

Paginated endpoints declare pagination in the endpoint stanza block:

```rust
GET ListEvents(start: u64 = 0, count: u64 = 50, kind?: String)
    as list
    -> Json<Vec<Event>>
{
    query {
        start
        count
        kind
    }
    paginate OffsetLimitPagination {
        offset = start,
        limit = count
    }
}
```

Normal usage is facade-first:

```rust
use concord_core::prelude::*;

let events = api
    .events()
    .list()
    .kind("deploy".to_string())
    .paginate()
    .max_items(500)
    .collect()
    .await?;
```

Supported plan-native controllers include:

- `OffsetLimitPagination`
- `CursorPagination`
- `PagedPagination`

Pagination caps can be configured globally:

```rust
let api = api.with_pagination_caps(
    concord_core::advanced::Caps::default()
        .max_pages(10)
        .max_items(1_000),
);
```
