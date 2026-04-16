# 11. Pagination

Pagination is declared in the endpoint with a `paginate` block, then used at runtime with `.paginate()`.

```rust
let items = api.request(endpoints::List::new())
    .paginate()
    .collect()
    .await?;
```

A paginated endpoint still supports a single-page request with `execute()`.

```rust
let first_page = api.request(endpoints::List::new())
    .execute()
    .await?;
```

## Page items

The decoded response type must implement `PageItems`.

`Vec<T>` already implements `PageItems`, so endpoints returning `Json<Vec<T>>` work directly.

```rust
GET List {
    path["x"]
    paginate PagedPagination {
        page_key = "p".into(),
        per_page_key = "sz".into(),
        page = 1,
        per_page = 20
    }
    -> Json<Vec<String>>;
}
```

For wrapper responses, implement `PageItems` yourself.

```rust
#[derive(serde::Deserialize)]
pub struct Page {
    items: Vec<Item>,
    next: Option<String>,
}

impl PageItems for Page {
    type Item = Item;
    type IntoIter = std::vec::IntoIter<Item>;

    fn len(&self) -> usize {
        self.items.len()
    }

    fn inner_into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}
```

## Offset-limit pagination

Use `OffsetLimitPagination` for APIs that take an offset and limit.

```rust
GET List {
    path["x"]
    params {
        start: u64 = 0,
        count: u64 = 2
    }
    query {
        "start" = start,
        "count" = count
    }
    paginate OffsetLimitPagination {
        offset = start,
        limit = count
    }
    -> Json<Vec<String>>;
}
```

The controller updates `start` by adding `count` after each page.

Requests look like:

```text
/x?start=0&count=2
/x?start=2&count=2
/x?start=4&count=2
```

The default offset query key is `offset` and the default limit key is `limit`, but the macro can bind controller fields to endpoint parameters so the upstream keys remain `start` and `count` as shown above.

The controller stops on an empty page by default. It also stops on a short page when `stop_on_short_page` is true, which is the default.

## Paged pagination

Use `PagedPagination` for APIs that take a page number and page size.

```rust
GET List {
    path["x"]
    params {
        page: u32 = 1,
        page_size: u32 = 2
    }
    query {
        "p" = page,
        "sz" = page_size
    }
    paginate PagedPagination {
        page_key = "p".into(),
        per_page_key = "sz".into(),
        page = page as u64,
        per_page = page_size as u64
    }
    -> Json<Vec<String>>;
}
```

Requests look like:

```text
/x?p=1&sz=2
/x?p=2&sz=2
/x?p=3&sz=2
```

The controller increments `page` by one after each page.

## Cursor pagination

Use `CursorPagination` when the response provides a cursor for the next page.

The response type must implement both `PageItems` and `HasNextCursor`.

```rust
impl HasNextCursor for Page {
    type Cursor = String;

    fn next_cursor(&self) -> Option<&Self::Cursor> {
        self.next.as_ref()
    }
}
```

Endpoint example:

```rust
GET List {
    path["x"]
    params {
        page_cursor?: String,
        page_size: u64 = 2
    }
    query {
        "pageCursor" = page_cursor,
        "pageSize" = page_size
    }
    paginate CursorPagination {
        cursor = page_cursor,
        per_page = page_size
    }
    -> Json<Page>;
}
```

The first request omits `pageCursor` when the cursor is `None`. If the first response has `next = Some("c1")`, the second request sends `pageCursor=c1`.

## Pagination runtime API

Start pagination from a pending request.

```rust
let request = api.request(endpoints::List::new());
let pager = request.paginate();
```

Use `collect()` to collect all items into a `Vec`.

```rust
let items = api.request(endpoints::List::new())
    .paginate()
    .collect()
    .await?;
```

Use `for_each_page` when you need page metadata or streaming-like behavior.

```rust
api.request(endpoints::List::new())
    .paginate()
    .for_each_page(|page| {
        println!("page {} -> {} items", page.meta.page_index, page.value.len());
        Ok(concord_core::internal::Control::Continue)
    })
    .await?;
```

Return `concord_core::internal::Control::Stop` to stop early.

## Caps and safety limits

Pagination has runtime caps.

```rust
let items = api.request(endpoints::List::new())
    .paginate()
    .max_pages(10)
    .max_items(1_000)
    .detect_loops(true)
    .collect()
    .await?;
```

Defaults are defined by `Caps`:

```rust
Caps {
    max_pages: 100,
    max_items: 100_000,
    detect_loops: true,
}
```

Set client defaults with `with_pagination_caps`.

```rust
let api = ApiPaged::new()
    .with_pagination_caps(Caps::default().max_pages(20).max_items(5000));
```

Set per-request caps on `PaginatedRequest` with `.max_pages(...)`, `.max_items(...)`, and `.detect_loops(...)`.

## Limit errors

If pagination reaches `max_pages`, Concord returns `ApiClientError::PaginationLimit`.

If collecting a page would exceed `max_items`, Concord also returns `ApiClientError::PaginationLimit`.

The current page request has already happened when the limit is detected, because the runtime must decode the page to know how many items it contains.

## Loop detection

Controllers can expose a progress key. Built-in offset, page, and cursor controllers do this.

When `detect_loops` is true, Concord errors if it sees the same progress key twice. This catches APIs that keep returning the same cursor or page state.

For cursor pagination, a repeated cursor returns `ApiClientError::Pagination`.

Disable loop detection only when the upstream API is known to repeat progress keys safely:

```rust
api.request(endpoints::List::new())
    .paginate()
    .detect_loops(false)
    .collect()
    .await?;
```

## Interaction with other policies

Each page is a separate request. For each page, Concord applies auth, cache, inflight, rate-limit, retry, transport, response handling, and decoding.

`page_index` in request metadata starts at `0` and increments for each page. Tests assert this metadata on recorded requests.

## Practical guidance

Use default page sizes that match upstream recommendations.

Use `max_pages` and `max_items` in production calls that could otherwise walk unbounded data.

Prefer cursor pagination when the upstream API provides stable cursors. Prefer offset or page pagination when the API only exposes numeric controls.
