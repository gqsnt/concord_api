# Pagination

Pagination is opt-in at the call site. A paginated endpoint first declares a pagination controller in the DSL, then callers use `.paginate()` to choose paginated execution.

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
let items = api
    .list_offset()
    .paginate()
    .max_items(1_000)
    .collect()
    .await?;
```

The runtime keeps request parameters stable while advancing the pagination controller fields.

Custom pagination controllers receive a mutable `PageRequest` for the next page. Query mutation accepts borrowed or owned keys, so controllers can compute dynamic query names. Header mutation is fallible: invalid header names return `ApiClientError::Pagination` instead of panicking. `PageRequest::new` is an internal runtime construction hook, not a public user construction API.

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
    .paginate()
    .for_each_page(|page| async move {
        println!("status={} items={}", page.status(), page.value().len());
        Ok(())
    })
    .await?;
```

## Bounds

Use `max_pages` and `max_items` to cap work.

```rust
let items = api
    .list_offset()
    .paginate()
    .max_pages(10)
    .max_items(500)
    .collect()
    .await?;
```

Caps must be greater than zero. Passing `0` through per-request builders or runtime pagination caps returns a typed pagination error before the first page request is sent.

Retry and auth refresh preserve the current page state. A retry for page `N` retries page `N`, not page `N + 1`.

Successful page responses are decoded and handed to the pagination controller before state advances. Decode failure, stale fallback failure, or retry for a page does not advance the controller state.
