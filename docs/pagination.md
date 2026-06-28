# Pagination

Pagination is opt-in at the endpoint and call site. A paginated endpoint first declares a pagination controller in the DSL, then callers use `.paginate(PaginationTermination::...)` to choose paginated execution and an explicit termination policy. Response types such as `Vec<T>` can implement `PageItems`, but `.paginate(...)` is available only for endpoints that declare pagination. No page or item cap is implicit; loop detection is enabled by default.

The runtime treats pagination as a deterministic page loop:

1. build the logical page request
2. apply pagination mutations for that page
3. validate auth collisions and acquire rate-limit permits
4. send the page request through the normal transport, classification, auth, retry, and decode path
5. use an exact item-count hint, when available, to apply common page-content stop rules before controller advance
6. ask the pagination controller whether to continue or stop only when the runtime has not already stopped
7. derive the next page request or return

Common page-content stop rules are runtime invariants, not controller-specific behavior when the runtime has enough information to decide before advance:

- an empty page stops pagination
- a short page stops pagination when Concord knows the expected page size

The current page is included before stopping. `PageItems::item_count_hint()` is exact when present, and the runtime uses it before calling controller advance. Page types should implement it whenever they can expose the count without consuming themselves. An exact hint alone lets Concord stop before `advance()` for an empty page, hard-item-cap overflow, and provable `TakeItems` completion. Built-in offset, cursor, and page-number pagination provide the expected page size automatically from `limit` or `per_page`. Built-in controllers are `OffsetLimitPagination`, `CursorPagination`, and `PagedPagination`. Custom pagination controllers can call `PageRequest::set_expected_items_per_page(NonZeroUsize)` during `apply()` when they request a specific page size. With both an exact hint and an expected page size, the runtime also owns generic short-page stop before `advance()`. If custom pagination does not set an expected size, `collect()` still remains exact after consuming the page, but Concord cannot generically detect a short page before advance.

Removed controller-local short-page stop fields such as `stop` and `stop_on_short_page` remain unsupported. Runtime-owned short-page stopping is controlled by `PageItems::item_count_hint()` and `PageRequest::set_expected_items_per_page()`.

If a later page request would reuse any previously seen logical request identity, the runtime returns a typed pagination error instead of silently looping. That guard is separate from the explicit termination policy and remains active even when controller loop-key checking is disabled.

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

The runtime keeps request parameters stable while advancing controller fields.

Custom pagination controllers receive a mutable `PageRequest` for the next page. Query mutation accepts borrowed or owned keys, so controllers can compute dynamic query names. Header mutation is fallible: invalid header names return `ApiClientError::Pagination` instead of panicking. Controllers that request a specific page size should call `set_expected_items_per_page(NonZeroUsize)` on each page request. The expected count is per page and does not persist. `PageRequest::new` is an internal runtime construction hook, not a public user construction API.

`PageRequest` mutations are applied to the logical page request before auth collision validation, rate-limit acquisition, and transport materialization. Query mutation is deterministic: `set_query()` removes all prior values for that key and appends the new value at the end of the query list, while `remove_query()` removes every matching value and leaves missing keys unchanged. Header mutation is typed and fallible for invalid names; header values are already represented as `HeaderValue`, so invalid values are rejected before they reach `PageRequest` and controllers usually map `HeaderValue::from_str` failures into typed pagination errors.

Paginated endpoints with request bodies are rejected in v1. Concord does not replay endpoint request bodies across page requests.

Custom pagination cannot override auth-owned query or header material. If a page controller creates a collision with query auth, bearer or Basic `Authorization`, or custom header auth, Concord rejects the request before rate-limit acquisition and transport send.

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

Hard caps fetch until the controller stops, but error if the cap would be exceeded:

- `PaginationTermination::HardPageCap(n)` errors if more than `n` pages would be required.
- `PaginationTermination::HardItemCap(n)` errors if more than `n` items would be collected.

Soft limits stop cleanly:

- `PaginationTermination::TakePages(n)` fetches at most `n` pages and stops even if the controller would continue.
- `PaginationTermination::TakeItems(n)` returns at most `n` items. `collect()` truncates the final page if necessary.

Hard caps must be greater than zero. `HardPageCap(0)` and `HardItemCap(0)` return typed pagination errors before the first page request is sent. `TakePages(0)` and `TakeItems(0)` return an empty collection without transport for `collect()`. `TakePages(0)` is a no-op for `for_each_page()`.

`collect()` supports all four termination modes. Item collection, exact `TakeItems` truncation, and final hard-item-cap validation use the items returned by `PageItems::into_items()`. Pre-advance empty-page stop, hard-item-cap overflow, and provable `TakeItems` completion require an exact `item_count_hint()`, because `into_items()` consumes the page while cursor and custom advance logic may need to borrow it. Generic pre-advance short-page stop also requires an expected page size from built-ins or `PageRequest::set_expected_items_per_page(NonZeroUsize)`. Without a hint, collection and limits remain exact and no extra page is fetched after the exact terminal result is known, but controller advance may already have run. `HardItemCap` never truncates.

`for_each_page()` supports page-based termination exactly. Runtime empty-page and short-page stops use `PageItems::item_count_hint()` because the callback receives whole page responses. If the hint is missing, `for_each_page()` cannot detect empty or short pages generically. `TakeItems` is rejected for `for_each_page()` because the callback receives whole pages; use `collect()` when item-level truncation is required.

Retry and auth refresh preserve the current page state. A retry for page `N` retries page `N`, not page `N + 1`.

Successful page responses with an exact item-count hint are checked for common content termination before the controller can advance. A hinted hard-item-cap overflow or completed `TakeItems` request also prevents advance. For page types without a hint, `collect()` can determine exact item termination only after `into_items()` consumes the page, so controller advance may already have run. Decode failure and retry for a page never advance controller state.

Cursor pagination follows the same per-page runtime order. `stop_when_cursor_missing` stops when a cursor is absent; if pagination continues without changing the next request identity, Concord raises a typed non-progress error rather than reissuing the same page.
