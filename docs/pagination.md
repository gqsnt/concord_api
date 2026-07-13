# Pagination

Pagination is opt-in at the endpoint and call site. A paginated endpoint declares a pagination controller type in the DSL, then callers use `.paginate(PaginationTermination::...)` to choose paginated execution and an explicit termination policy. Response types such as `Vec<T>` can implement `PageItems`, but `.paginate(...)` is available only for endpoints that declare pagination. Pagination is collect-only: callers drive it through `.paginate(...).collect().await`, and `collect()` materializes the accumulated results in memory. No page or item cap is implicit; loop detection is enabled by default.

The runtime treats pagination as a deterministic page loop:

1. build the logical page request
2. apply pagination for that page
3. validate auth collisions and acquire rate-limit permits
4. perform one visible execution, with at most one authentication recovery, then decode
5. use the exact item count to apply common page-content stop rules before controller advance
6. ask the pagination controller whether to continue or stop only when the runtime has not already stopped
7. derive the next page request or return

Common page-content stop rules are runtime invariants, not controller-specific behavior when the runtime has enough information to decide before advance:

- an empty page stops pagination
- a short page stops pagination when Concord knows the expected page size

Pagination state is per request. The current page is included before stopping. `PageItems::item_count()` returns the exact number of items, and the runtime uses it before calling controller advance. Built-in offset, cursor, and page-number pagination provide the expected page size automatically from `limit` or `per_page`. Built-in controllers are just core-provided Rust types: `OffsetLimitPagination`, `CursorPagination<String>`, and `PagedPagination`. Custom pagination controllers expose their expected page size through `EndpointPagination::expected_items_per_page()`. With both an exact item count and an expected page size, the runtime also owns generic short-page stop before `advance()`.

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

The runtime keeps endpoint fields stable while advancing controller state.

Custom pagination uses generated `PaginateBinding` to synchronize endpoint fields with controller state before planning. The generated endpoint type implements `PaginatedEndpoint<Cx> { type Pagination = Type; }`, and `EndpointPlan.pagination` is only a `PaginationMarker` presence flag. Core owns the runtime loop through `PaginationRuntime` and `PaginationRuntimeAdapter`. Endpoint-bound assignments load from endpoint fields and store back after the page advances. Literal or config assignments initialize pagination fields during load and are not stored back to endpoint fields. Planning remains the only place that renders query, header, path, or body output. Custom controllers that request a specific page size should implement `EndpointPagination::expected_items_per_page()`. The expected count is per page and does not persist.

Paginated endpoints with request bodies are rejected in v1. Concord does not replay endpoint request bodies across page requests.

Pagination cannot override auth-owned query or header material. If a controller creates a collision with query auth, bearer or Basic `Authorization`, or custom header auth, Concord rejects the request before rate-limit acquisition and transport send.

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

    fn item_count(&self) -> usize { self.items.len() }
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
    paginate CursorPagination<String> {
        cursor = cursor,
        per_page = count
    }
    -> Json<CursorPage>
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

Hard caps must be greater than zero. `HardPageCap(0)` and `HardItemCap(0)` return typed pagination errors before the first page request is sent. `TakePages(0)` and `TakeItems(0)` return an empty collection without transport for `collect()`.

Structured pagination failures use `ApiClientError::Pagination` or `PaginationLimit` with a `PaginationErrorKind`. The kind is stable for machine handling, and the message is safe metadata only.

`collect()` supports all four termination modes. Item collection, exact `TakeItems` truncation, and final hard-item-cap validation use the items returned by `PageItems::into_items()`. Pre-advance empty-page stop, hard-item-cap overflow, and provable `TakeItems` completion use the exact item count, because `into_items()` consumes the page while cursor and custom advance logic may need to borrow it. Generic pre-advance short-page stop also requires an expected page size from built-ins or `EndpointPagination::expected_items_per_page()`. `HardItemCap` never truncates.

Reqwest applies the selected client retry mode independently to each page's
visible execution. Any hidden resend remains part of page `N`. Authentication
recovery also reconstructs page `N` and never advances page state.

Successful page responses are checked for common content termination before the controller can advance. A hard-item-cap overflow or completed `TakeItems` request also prevents advance. Decode failure never advances controller state.

Cursor pagination follows the same per-page runtime order. `stop_when_cursor_missing` stops when a cursor is absent; if pagination continues without changing the next request identity, Concord raises a typed non-progress error rather than reissuing the same page.


