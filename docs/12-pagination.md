# 12. Pagination

Pagination is declared on an endpoint and used at runtime with `.paginate()`.

A paginated endpoint can still be executed as a single request.

## Runtime usage

Facade:

```rust
let items = api
    .regional(region)
    .match_v5_matches()
    .get_match_ids_by_puuid(puuid)
    .count(100)
    .paginate()
    .max_items(10_000)
    .collect()
    .await?;
```

Explicit endpoint:

```rust
let items = api
    .request(endpoints::List::new())
    .paginate()
    .collect()
    .await?;
```

## Page items

The response type must expose items.

`Vec<T>` works directly.

Wrapper response types can implement `PageItems`.

## Offset-limit pagination

Use this when the API accepts an offset/start and a limit/count.

```rust
GET List(start: u64 = 0, count: u64 = 20)
    path ["items"]
    query {
        "start" = start
        "count" = count
    }
    paginate OffsetLimitPagination {
        offset = start
        limit = count
    }
    -> Json<Vec<String>>
```

Requests:

```text
/items?start=0&count=20
/items?start=20&count=20
/items?start=40&count=20
```

## Paged pagination

Use this when the API accepts a page number and page size.

```rust
GET List(page: u32 = 1, page_size: u32 = 20)
    path ["items"]
    query {
        "page" = page
        "pageSize" = page_size
    }
    paginate PagedPagination {
        page = page as u64
        per_page = page_size as u64
    }
    -> Json<Vec<String>>
```

## Cursor pagination

Use this when the response returns a cursor.

```rust
GET List(page_cursor?: String, page_size: u64 = 20)
    path ["items"]
    query {
        "pageCursor" = page_cursor
        "pageSize" = page_size
    }
    paginate CursorPagination {
        cursor = page_cursor
        per_page = page_size
    }
    -> Json<Page>
```

The first request omits the cursor when it is `None`.

The response type must implement the cursor trait used by the runtime.

## Caps

Pagination supports safety caps:

```rust
let items = api.items()
    .list()
    .paginate()
    .max_pages(10)
    .max_items(1_000)
    .detect_loops(true)
    .collect()
    .await?;
```

Defaults are provided by `Caps`.

## Loop detection

Built-in pagination engines expose progress keys.

If loop detection sees the same key twice, Concord returns a pagination error. This protects against APIs that keep returning the same cursor or page.

## Per-page metadata

Each page request has request metadata with a page index starting at `0`.

This is useful for tests and debug hooks.

## Practical guidance

Use upstream-recommended page sizes.

Always set `max_pages` or `max_items` for unbounded production walks.

Prefer cursor pagination when the upstream API supports it.
