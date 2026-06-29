# Pagination and codecs

Pagination and codecs are extension points in `concord_core` that generated code wires to endpoint request plans.

## Codecs

`BodyCodec` encodes request bodies. `ResponseCodec` decodes raw responses. `ContentType` is the shared wire-content marker trait used by codec marker types and reserved endpoint I/O formats.

Built-ins:

- `Json<T>`
- `Text<String>`
- `NoContent`

Current marker types:

- `JsonContentType`
- `TextContentType`
- `EventStream`
- `OctetStream`
- `NdJson`
- `FormData`
- `Mixed`

Custom codecs implement the public codec traits and can be used in endpoint signatures or response lines.

The macro syntax:

```text
POST Create(body: Json<Create>)
-> Json<Item>
```

is lowered by codegen into request body encoding and response decode calls.

Codec helpers use the fallible header conversion path:

- `BodyCodec::try_content_type()`
- `ResponseCodec::try_accept()`

The convenience `content_type()` and `accept()` methods remain available for trusted built-in markers and compatibility call sites. Generated planning uses the fallible helpers so invalid user-defined markers return typed errors instead of panicking.

## Pagination traits

`PageItems` exposes decoded items from a response page.

`HasNextCursor` exposes cursor continuation data for cursor pagination.

Built-in controllers:

- `OffsetLimitPagination`
- `CursorPagination`

Custom pagination implements `PaginationController`.

## Runtime controller model

`PageRequest` represents the next page request mutation.

`PageRequest` is constructed by the runtime, not by user code. Controllers mutate it through query/header helpers. Query keys can be owned or dynamic. Header mutation is fallible and invalid header names must return typed pagination errors rather than panicking. Custom controllers that request a known page size set it through `PageRequest::set_expected_items_per_page(NonZeroUsize)` during `apply()`. The value is per page request and starts as `None` for each page.

`PageDecision` tells the runtime whether to continue, stop, or error.

`ProgressKey` protects pagination loops from repeating the same progress state.

Pagination also keeps an always-on logical-request progress guard: each page
builds a request identity from the safe page-shaping state, and a repeated
identity returns a typed pagination error instead of silently reissuing the
same page. The optional controller `ProgressKey` check remains available as an
additional guard.

The macro `paginate` block resolves controller field assignments. Codegen connects those assignments to the runtime controller traits; the runtime drives page requests and decodes each page through the endpoint response codec.

Empty-page and short-page termination are runtime invariants, not
controller-specific rules. The runtime obtains expected page size from built-in
offset/page/cursor controllers (`limit` or `per_page`) or from custom
`PageRequest::set_expected_items_per_page(NonZeroUsize)`. `PageItems` count
hints are exact when present. An exact hint alone is enough for hinted
empty-page stop, hard-item-cap overflow, and provable `TakeItems` completion
before controller advance. Exact hint plus expected page size is required for
generic short-page stop before controller advance.

`collect()` still validates actual items and applies exact `TakeItems`
truncation after `into_items()`. Because that method consumes the page while
cursor/custom advance can require a page reference, a page without an item
count hint may be advanced before exact post-consumption termination or a hard
item-cap error is known. No additional request is sent after the exact result is
known. This limitation is part of the v1 `PageItems` contract. Without an
expected page size, Concord cannot generically detect a short page before
advance.

Collection bounds are shape-specific: offset, page-number, and custom pagination collection require `PageItems`; built-in cursor pagination additionally requires `HasNextCursor`. There is no implicit page or item cap after `.paginate(...)`; callers must pass an explicit `PaginationTermination`.

`HardPageCap(n)` and `HardItemCap(n)` are hard safety caps and zero values are
typed pagination configuration errors before the first transport send.
`TakePages(n)` and `TakeItems(n)` are soft limits and zero values return an
empty/no-op result without transport. Item limits are enforced from the actual
collected items in `collect()`; `for_each_page()` supports page-based
termination exactly and rejects `TakeItems` because it cannot truncate whole
page responses. `for_each_page()` can apply runtime empty/short-page stops only
when `PageItems::item_count_hint()` is present. Cursor pagination with
`stop_when_cursor_missing` stops on missing cursor; continuing without changing
the request identity is a typed non-progress error instead of an infinite loop.
Pagination progress is checked against every logical request identity seen so
far in the run, not just the immediately previous page.
