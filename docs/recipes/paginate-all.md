# Paginate all results

## Goal

Follow an offset-paginated endpoint until the service naturally terminates,
accumulate every item, and fail closed if the service never reaches that
termination within a reviewed finite cap.

## Complete code

The endpoint binds its offset and fixed page size to the built-in controller:

```rust,ignore
{{#include ../../crates/concord-examples/src/pagination/mod.rs:offset_endpoint}}
```

The application requests natural termination through at most 100 pages:

```rust,ignore
{{#include ../../crates/concord-examples/src/pagination/offset.rs:paginate_all}}
```

The canonical local runtime test scripts a two-item first page and a short
second page, then verifies that the second request advances from offset `0` to
offset `2`. No external service is involved.

## Why these declarations

`offset` and `limit` already shape the public query. The `paginate` block
declares which endpoint values the controller owns. `OffsetPagination`
increments the offset by the decoded item count while keeping the positive
page size fixed.

`.paginate(PageLimit::HardCap(100))` means “succeed only if the controller
naturally stops on or before page 100.” If a 100th full page still produces a
next state, Concord returns a pagination error instead of silently presenting
the capped prefix as all results.

That differs from `PageLimit::Take(100)`, which deliberately succeeds after
at most 100 pages without asking whether another state exists. Use `Take` for
“give me up to this many pages” and `HardCap` for “collect the complete
sequence, but never exceed this safety boundary.”

Each page runs through the same authentication, limiter, hooks, timeout,
status, buffering, decoding, and completion path as a direct request. Pages
are sequential, and the final `Vec<u32>` appears only after the aggregate
operation succeeds.

## Adaptations

For page-number APIs, bind a positive page and page-size value to
`PagePagination`. For cursor APIs, decode a page type implementing
`HasNextCursor<C>` and bind an optional cursor to `CursorPagination<C>`.
Absent and present-empty cursor states remain distinct.

Choose a page size supported by the service and a hard cap supported by
product requirements. The runtime's configured maximum pages and items remain
additional ceilings; the call-site cap cannot raise them.

If the caller needs only a prefix, use `Take`. If it needs per-page streaming
or resumable partial results, the built-in accumulated terminal is not that
contract. Write an explicitly bounded application workflow or a dedicated
response design instead of interpreting a pagination error as partial
success.

Implement a custom controller only when the service cannot be represented by
offset, page, or cursor progression. It still must provide deterministic
progress and bounded canonical identity.

## Production notes

Accumulated application items consume memory beyond the buffered bytes used
for each individual page. Set both page and item ceilings from payload
evidence and consider whether “all” is suitable for the endpoint's real
cardinality.

Concord detects repeated canonical pagination state and fails instead of
looping. Keep controller identity complete enough to distinguish states but
free of credentials. Pagination-controlled values cannot partition rate-limit
buckets or indirectly control a host-partition dimension.

Treat any page failure as failure of the whole collection. Use aggregate and
page operation IDs in bounded telemetry, and avoid recording raw cursor values
or unbounded page-specific labels.

## Related

- [Pagination](../features/pagination.md)
- [Rate limiting](../features/rate-limiting.md)
- [Hooks and observability](../features/hooks-and-observability.md)
- [Limits reference](../reference/limits.md)
- [Error reference](../reference/errors.md)
