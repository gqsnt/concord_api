# Pagination

- **Audience:** endpoint authors, callers, and controller authors
- **Purpose:** end-to-end concept guide
- **Normative reference:** [DSL pagination rules](spec.md#pagination)

Pagination is a controlled sequence of ordinary endpoint executions. Initial
pagination values remain caller-visible request inputs; explicit bindings give
a controller exclusive authority to advance selected fields.

## Declaration

<!-- concord-source: crates/concord-examples/src/pagination/mod.rs#offset_endpoint -->
```concord
    GET offset(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u32>> {
        query { "offset" = ep.offset; "limit" = ep.limit; }
        paginate concord::pagination::OffsetPagination {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }
```

Binding is explicit because APIs use many naming conventions and because field
names should not silently grant runtime mutation rights. Fields and
destinations are unique. A body is not a binding destination. The controller
type is opaque to the macro; Rust checks its fields and trait obligations.

## One page and finite prefix

Directly awaiting the endpoint performs one ordinary request. A finite prefix
uses `PageLimit::Take`:

<!-- concord-source: crates/concord-examples/src/pagination/offset.rs#execute_pagination -->
```rust
pub async fn collect(client: &PaginationApi) -> Result<Vec<u32>, concord::ApiError> {
    client
        .offset(
            0,
            std::num::NonZeroU32::new(50).expect("positive page size"),
        )
        .paginate(PageLimit::Take(2))
        .await
}
```

`Take(0)` performs no controller or page work. `Take(n)` succeeds with the
accumulated prefix after at most `n` pages without requiring natural
termination.

## Natural completion with a hard cap

<!-- concord-source: crates/concord-examples/src/pagination/offset.rs#paginate_all -->
```rust
pub async fn collect_all(client: &PaginationApi) -> Result<Vec<u32>, concord::ApiError> {
    client
        .offset(
            0,
            std::num::NonZeroU32::new(50).expect("positive page size"),
        )
        .paginate(PageLimit::HardCap(
            std::num::NonZeroU32::new(100).expect("positive hard cap"),
        ))
        .await
}
```

`HardCap(n)` permits natural termination through page `n` and fails when the
controller still has another state. It prevents an incomplete prefix from
being silently labeled complete.

There is no unbounded public all-pages variant. Natural completion is requested
through a finite hard cap.

## Page execution

Each page:

1. clones the original endpoint snapshot;
2. writes current controller fields into declared bindings;
3. prepares the ordinary public request head;
4. constructs canonical credential-free page identity;
5. checks cycles and finite state;
6. executes through the same hooks, credentials, limiter, transport, response,
   error, and completion path as a direct request;
7. decodes one complete page;
8. asks the controller for continue, finish, or failure.

## Cycle detection and bounds

Canonical identity uses endpoint ID, method, logical URL, and public headers.
Credential material and body bytes are excluded. Page-stable bodies therefore
cannot hide a repeated public page request.

Page count, item count, retained identity count, retained identity bytes,
response/body limits, and call-site limit are independent. First excess fails
without retaining the excess value.

## Body and response requirements

A bodyless endpoint is page-stable. A body-bearing endpoint requires
`PageRequestBodySpec`, promising independent preparation of the same body for
every cloned page.

Pagination requires a buffered page-capable response specification. Streaming,
record-stream, discard, and explicit response-bytes terminals are not
page-capable.

## Rate limits and authentication

Policy is recomputed for each updated page. A rate key cannot depend on a
pagination-controlled value; `host` is also rejected when host shaping depends
on one. Every page and admitted retry performs ordinary limiter admission and
private authentication placement.

## Result semantics

Later-page failure returns one error and no accumulated complete value.
`PageLimit::Take` is the explicit finite-prefix success mode. Hard-cap or
controller completion preserves the all-or-error meaning of the returned
collection.

## Extension boundary

`PaginationController<Page>` owns typed state and deterministic continuation.
It receives no transport or external I/O authority and can update only
compiler-declared bindings.

## Related

- [Request bodies](request_bodies.md)
- [Responses](responses.md)
- [Rate limits](rate_limits.md)
- [Execution policy](execution_policy.md)
