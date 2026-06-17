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

`PageDecision` tells the runtime whether to continue, stop, or error.

`ProgressKey` protects pagination loops from repeating the same progress state.

The macro `paginate` block resolves controller field assignments. Codegen connects those assignments to the runtime controller traits; the runtime drives page requests and decodes each page through the endpoint response codec.
