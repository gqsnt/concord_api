# Pagination

Pagination is opt-in at the endpoint and call site. A paginated endpoint first
declares a pagination controller in the DSL, then callers use
`.paginate(PaginationTermination::...)` to choose paginated execution and an
explicit termination policy. Response types such as `Vec<T>` can implement
`PageItems`, but `.paginate(...)` is available only for endpoints that declare
pagination. No page or item cap is implicit; loop detection is enabled by
default.

The runtime treats pagination as a deterministic page loop:

1. build the logical page request
2. apply pagination mutations for that page
3. execute the page request through the normal cache/rate-limit/retry/auth pipeline
4. decode the response
5. ask the pagination controller whether to continue or stop
6. derive the next page request or return

If a later page request would reuse any previously seen logical request
identity, the runtime returns a typed pagination error instead of silently
looping. That guard is separate from the explicit termination policy and remains
active even when controller loop-key checking is disabled.

## Offset Pagination

```rust
GET ListOffset(start: u64 = 0, count: u64 = 20)
    as list_offset
    path ["items"]
    query {
        start
        count
    }
    paginate OffsetLimitPagination {
        offset = start,
        limit = count
    }
    -> Json<Vec<Item>>
```

Collect all items with `.collect()`.

```rust
use concord_core::prelude::PaginationTermination as PageUntil;

let items = api
    .list_offset()
    .paginate(PageUntil::hard_item_cap(1_000))
    .collect()
    .await?;
```

The runtime keeps request parameters stable while advancing the pagination controller fields.

Custom pagination controllers receive a mutable `PageRequest` for the next page. Query mutation accepts borrowed or owned keys, so controllers can compute dynamic query names. Header mutation is fallible: invalid header names return `ApiClientError::Pagination` instead of panicking. `PageRequest::new` is an internal runtime construction hook, not a public user construction API.

Paginated endpoints with request bodies are rejected in v1. Concord does not
reuse or replay endpoint request bodies across page requests.

## Cursor Pagination

Cursor pagination uses a response type that exposes items and a next cursor. Offset, page-number, and custom pagination collection only require `PageItems`; built-in cursor pagination additionally requires `HasNextCursor`.

```rust
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CursorPage {
    pub items: Vec<Item>,
    pub next_cursor: Option<String>,
}

impl PageItems for CursorPage {
    type Item = Item;

    fn item_count_hint(&self) -> Option<usize> { Some(self.items.len()) }
    fn into_items(self) -> Vec<Self::Item> { self.items }
}

impl HasNextCursor for CursorPage {
    type Cursor = String;
    fn next_cursor(&self) -> Option<Self::Cursor> { self.next_cursor.clone() }
}
```

```rust
GET ListCursor(cursor?: String, count: u64 = 20)
    as list_cursor
    path ["cursor-items"]
    query { cursor, count }
    paginate CursorPagination {
        cursor = cursor,
        per_page = count
    }
    -> Json<CursorPage>
```

## Processing Page By Page

Use `for_each_page` when pages should be processed without collecting every item into one vector.

```rust
api.list_cursor()
    .paginate(PageUntil::hard_page_cap(100))
    .for_each_page(|page| async move {
        println!("status={} items={}", page.status(), page.value().len());
        Ok(())
    })
    .await?;
```

## Termination

Pagination requires an explicit termination policy.

```rust
let items = api
    .list_offset()
    .paginate(PageUntil::take_items(500))
    .collect()
    .await?;
```

Hard caps fetch until the controller stops, but error if the cap would be
exceeded:

- `PaginationTermination::HardPageCap(n)` errors if more than `n` pages would
  be required.
- `PaginationTermination::HardItemCap(n)` errors if more than `n` items would
  be collected.

Soft limits stop cleanly:

- `PaginationTermination::TakePages(n)` fetches at most `n` pages and stops even
  if the controller would continue.
- `PaginationTermination::TakeItems(n)` returns at most `n` items. `collect()`
  truncates the final page if necessary.

Hard caps must be greater than zero. `HardPageCap(0)` and `HardItemCap(0)`
return typed pagination errors before the first page request is sent.
`TakePages(0)` and `TakeItems(0)` return an empty collection without transport
for `collect()`. `TakePages(0)` is a no-op for `for_each_page()`.

`collect()` supports all four termination modes. `for_each_page()` supports
page-based termination exactly. `TakeItems` is rejected for `for_each_page()`
because the callback receives whole pages; use `collect()` when item-level
truncation is required.

Retry and auth refresh preserve the current page state. A retry for page `N` retries page `N`, not page `N + 1`.

Successful page responses are decoded and handed to the pagination controller before state advances. Decode failure, stale fallback failure, or retry for a page does not advance the controller state.

Cursor pagination follows the same per-page runtime order. `stop_when_cursor_missing` still stops when a cursor is absent; if pagination continues without changing the next request identity, Concord raises a typed non-progress error rather than reissuing the same page forever.
