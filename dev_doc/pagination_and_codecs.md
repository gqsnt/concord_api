# Pagination and codecs

Pagination and codecs are extension points in `concord_core` that generated code wires to endpoint request plans.

## Codecs

`BodyCodec` encodes request bodies. `ResponseCodec` decodes raw responses.

Built-ins:

- `Json<T>`
- `Text<String>`
- `NoContent`

Custom codecs implement the public codec traits and can be used in endpoint signatures or response lines.

The macro syntax:

```text
POST Create(body: Json<Create>)
-> Json<Item>
```

is lowered by codegen into request body encoding and response decode calls.

## Pagination traits

`PageItems` exposes decoded items from a response page.

`HasNextCursor` exposes cursor continuation data for cursor pagination.

Built-in controllers:

- `OffsetLimitPagination`
- `CursorPagination`

Custom pagination implements `PaginationController`.

## Runtime controller model

`PageRequest` represents the next page request mutation.

`PageRequest` is constructed by the runtime, not by user code. Controllers mutate it through query/header helpers. Query keys can be owned or dynamic. Header mutation is fallible and invalid header names must return typed pagination errors rather than panicking.

`PageDecision` tells the runtime whether to continue, stop, or error.

`ProgressKey` protects pagination loops from repeating the same progress state.

Pagination also keeps an always-on logical-request progress guard: each page
builds a request identity from the safe page-shaping state, and a repeated
identity returns a typed pagination error instead of silently reissuing the
same page. The optional controller `ProgressKey` check remains available as an
additional guard.

The macro `paginate` block resolves controller field assignments. Codegen connects those assignments to the runtime controller traits; the runtime drives page requests and decodes each page through the endpoint response codec.

Collection bounds are shape-specific: offset, page-number, and custom pagination collection require `PageItems`; built-in cursor pagination additionally requires `HasNextCursor`. Zero `max_pages` or `max_items` caps are typed pagination errors before the first transport send.

`max_pages` is a hard page-request cap. `max_items` is enforced from the
actual collected items in `collect()` and from item-count hints in
page-by-page processing when hints are available. Cursor pagination with
`stop_when_cursor_missing` stops on missing cursor; continuing without changing
the request identity is a typed non-progress error instead of an infinite loop.
Pagination progress is checked against every logical request identity seen so
far in the run, not just the immediately previous page.
