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

## Cursor Pagination

Cursor pagination uses a response type that exposes items and a next cursor.

```rust
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CursorPage {
    pub items: Vec<Item>,
    pub next_cursor: Option<String>,
}

impl PageItems for CursorPage {
    type Item = Item;
    type IntoIter = std::vec::IntoIter<Item>;

    fn len(&self) -> usize { self.items.len() }
    fn inner_into_iter(self) -> Self::IntoIter { self.items.into_iter() }
}

impl HasNextCursor for CursorPage {
    type Cursor = String;
    fn next_cursor(&self) -> Option<&Self::Cursor> { self.next_cursor.as_ref() }
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

Retry and auth refresh preserve the current page state. A retry for page `N` retries page `N`, not page `N + 1`.
