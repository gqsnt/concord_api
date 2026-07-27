# Pagination controllers

Implement a pagination controller when the next request state cannot be
expressed by Concord's offset, page-number, or cursor helpers. A controller is
operation-local synchronous state. It reads each decoded page and produces the
optional state for the next sequential request.

Use the exact Rustdoc trait
`concord::pagination::PaginationController<Page>`.

## Complete example

This compiled example defines a page codec, custom cursor state, controller,
and bounded generated request:

```rust,ignore
{{#include ../../crates/concord-examples/src/pagination/custom.rs:pagination_controller_extension}}
```

The declaration binds the controller's `cursor` field to the endpoint's public
query value:

```rust,ignore
{{#include ../../crates/concord-examples/src/pagination/mod.rs:cursor_endpoints}}
```

The response codec's output implements `IntoIterator<Item = u32>`, so generated
pagination can accumulate its items.

## Controller state and bindings

The `paginate Type { field: bind ep.value }` block tells generated code how a
controller state becomes the next endpoint snapshot. The initial request
arguments create the first controller. Every later controller produced by
`next` is rebound through the same declaration.

Keep controller state minimal and typed. A continuation may be a cursor, an
offset, several public query values, or a small enum describing a documented
transition. Do not store decoded pages, credentials, response bodies, clients,
or an unbounded request history in it.

Each pagination operation owns its controller value. Separate operations may
run concurrently, so the type must be `Send` but need not be `Sync`.

## The three callbacks

`validate(&self)` runs before a page request is prepared. Use it for
state-local invariants such as a positive page size, a cursor length bound, or
a valid enum combination. It has no request, transport, or external context.

`next(self, page: &Page)` consumes the current state and returns:

- `Ok(Some(next))` to request another page;
- `Ok(None)` for natural completion;
- `Err(PaginationError)` for a safe controller failure.

The decoded page is borrowed and may contain sensitive application data.
Extract only the bounded continuation needed for the next state.

`short_page_size(&self)` optionally selects exact decoded-item-count
termination for the current state. Return a positive value only when the
remote contract guarantees that fewer items means the sequence ended. Cursor
protocols normally decide termination in `next` instead.

All callbacks are synchronous, non-preemptible, and execute outside the
individual page's operation deadline. They must perform no external I/O and
must bound CPU, allocation, parsing, internal retries, and metric cardinality.
Cancellation prevents later pages but cannot interrupt a callback already
running.

## Sequential flow and identity

Concord executes one page at a time:

```text
validate state
→ bind and prepare request
→ execute the managed page lifecycle
→ decode page and accumulate items
→ derive next state
→ repeat or complete
```

The controller does not author a cycle-detection key. Core derives a complete
canonical identity from the shaped public request, including stable endpoint
and method identity. The controller is responsible for producing state whose
bindings make real forward progress. If two states produce the same canonical
request, Concord detects the repetition and fails instead of looping.

Keep credentials and protected provider state out of controller values and
public bindings. Pagination-controlled values cannot be rate-limit dimensions
or indirectly control a host-partition dimension; this keeps page identity
and quota identity coherent.

Canonical identity storage has a separate finite byte ceiling, and page count
has a runtime ceiling. Those limits bound core retention but do not bound
controller-owned heap data.

## Completion and page limits

The call site selects `PageLimit::Take(n)` or `PageLimit::HardCap(n)`.
`Take` intentionally succeeds with a prefix after at most `n` pages.
`HardCap` succeeds only when natural termination occurs within the cap.

The runtime's maximum pages and items remain additional ceilings. A call-site
value can reduce work but cannot raise runtime configuration. Any page,
controller, identity, or item-limit failure fails the aggregate request;
accumulated items are not returned as implicit partial success.

The response specification must implement
`concord::response::PageResponseSpec`, and its output must implement
`IntoIterator`. Buffered codecs gain the marker automatically when their
output is iterable. `BufferedRecords<D>` and direct `Csv<T, H, D>` /
`NdJson<T>` responses implement the marker explicitly and yield their decoded
records as page items. Byte streams, `Records<D>` streams, discard responses,
and the explicit response-bytes acquisition path are not pagination page
outputs.

## Safe errors

Use `PaginationError::new(PaginationErrorKind::...)` for structural failures.
`with_safe_message` accepts only reviewed constant or bounded diagnostic text.
Do not include cursors, request identities, page items, credentials, bodies,
or native transport details.

The exact error item paths are
`concord::pagination::PaginationError` and
`concord::pagination::PaginationErrorKind`.

## Verification

Drive the controller through a generated endpoint and deterministic page
responses. Cover:

- natural completion on the first and later pages;
- state validation failure before transport;
- repeated shaped identity;
- exact `Take`, exact naturally terminating `HardCap`, and first excess;
- page and item runtime ceilings;
- cursor absent versus present-empty where the protocol distinguishes them;
- a decoding or transport failure after earlier pages;
- concurrent pagination operations using independent state;
- sentinels absent from controller errors, hooks, and completion events.

Assert each received request in order. A direct unit test of `next` is useful,
but it does not verify binding, identity, or aggregate completion.

## Related

- [Pagination](../features/pagination.md)
- [Paginate all results](../recipes/paginate-all.md)
- [Rate limiters and observers](rate-limiting.md)
- [Hooks, trusted work, and bounds](hooks-and-trusted-work.md)
- [Limits reference](../reference/limits.md)
- Rustdoc: `concord::pagination::PaginationController`
