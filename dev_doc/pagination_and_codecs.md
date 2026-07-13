# Pagination and codecs

Pagination and codecs are extension points in `concord_core` that generated code wires to endpoint request plans.

## Codecs

`BodyCodec` encodes request bodies. `ResponseCodec` decodes raw responses. `ContentType` is the shared wire-content trait used by codec marker types and reserved endpoint I/O formats.

Built-ins:

- `Json<T>`
- `Text<String>`
- `NoContent`

Current marker types:

- `JsonContentType`
- `TextContentType`
- `OctetStream`
- `FormData`

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

The convenience `content_type()` and `accept()` methods remain available for trusted built-in markers and established call sites. Generated planning uses the fallible helpers so invalid user-defined markers return typed errors instead of panicking.

Buffered request-body encode failures are sanitized at the client boundary before they become public `ApiClientError::Codec` values. Public diagnostics keep the generic request-body encoding message and do not render raw codec messages or nested codec source chains. Buffered response read failures become structured response-body errors, while timeout, connect, and request-execution failures remain distinct. Buffered response decode failures become sanitized `ApiClientError::Decode` values. Body-size limit errors remain separate structured errors.

## Pagination traits

`PageItems` exposes decoded items from a response page.

`HasNextCursor` exposes cursor continuation data for cursor pagination.

Built-in controllers:

- `OffsetLimitPagination`
- `CursorPagination<String>`
- `PagedPagination`

Custom pagination controllers implement `Default + EndpointPagination<Page>`.
Generated endpoints implement `PaginateBinding<P>` from the DSL assignment block.
Core runs pagination through `PaginationRuntime` and `PaginationRuntimeAdapter`.

## Runtime controller model

Custom controllers receive bound endpoint fields through generated binding code and mutate their own pagination state during `apply()`. Header mutation and query rendering happen through endpoint planning, not through pagination runtime request mutation. Custom controllers that request a known page size report it through `EndpointPagination::expected_items_per_page()` during `apply()`. The value is per page request and starts as `None` for each page.

`PageDecision` tells the runtime whether to continue, stop, or error.

`ProgressKey` protects pagination loops from repeating the same progress state.

Pagination also keeps an always-on logical-request progress guard: each page
builds a request identity from the safe page-shaping state, and a repeated
identity returns a typed pagination error instead of silently reissuing the
same page. The optional controller `ProgressKey` check remains available as an
additional guard. Public loop diagnostics do not render raw progress-key
contents; they only report safe metadata such as page index and key kind or
length.

The macro `paginate` block resolves controller field assignments. Built-in pagination and custom pagination both use assignment blocks for endpoint fields, and codegen connects those assignments to the pagination runtime path before the endpoint response codec decodes each page.

Empty-page and short-page termination are runtime invariants, not
controller-specific rules. The runtime obtains expected page size from built-in
offset/page/cursor controllers (`limit` or `per_page`) or from custom
`EndpointPagination::expected_items_per_page()`. `PageItems` count hints are exact
when present. An exact hint alone is enough for hinted empty-page stop,
hard-item-cap overflow, and provable `TakeItems` completion before controller
advance. Exact hint plus expected page size is required for generic short-page
stop before controller advance.

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
collected items in `collect()`. Cursor pagination with
`stop_when_cursor_missing` stops on missing cursor; continuing without changing
the request identity is a typed non-progress error instead of an infinite loop.
Pagination progress is checked against every logical request identity seen so
far in the run, not just the immediately previous page.

