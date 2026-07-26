# Pagination

Declare how one bodyless buffered endpoint advances between pages, then call
`.paginate(...)` on its named request. Concord executes pages sequentially
through the ordinary lifecycle and returns one collected value only when the
whole operation succeeds.

## Declare an offset endpoint

The controller binds its state to endpoint values that already shape the
request:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/pagination/mod.rs:offset_endpoint}}
```

`OffsetPagination` owns how the offset advances and how a short page ends the
sequence. The macro does not recognize the controller's type name; ordinary
Rust trait checking verifies that it implements the required pagination
contract for the decoded page.

Execute with a finite caller-selected limit:

```rust,ignore
{{#include ../../crates/concord-examples/src/pagination/offset.rs:execute_pagination}}
```

`PageLimit::Take(2)` accepts at most two pages and succeeds after the second
without asking for another state. `PageLimit::HardCap(n)` permits natural
termination through page `n` but reports a pagination error when the
controller still has a next state at that hard cap. `Take(0)` performs no page
request and returns an empty collection.

## Choose the matching controller

Concord exposes common controllers through `concord::pagination`:

- `OffsetPagination` advances an offset while retaining a fixed positive page
  size;
- `PagePagination` advances a positive page number and retains a fixed
  positive per-page value;
- `CursorPagination<C>` asks the decoded page for its next optional cursor.

The compiled pagination contract contains all three forms:

```rust,ignore
{{#include ../../crates/concord-examples/src/pagination/mod.rs:pagination_contract}}
```

Cursor presence is meaningful. An absent cursor terminates; an empty but
present cursor remains a value unless the response type's cursor contract
decides otherwise. Use the controller that matches the service rather than
translating one remote scheme into another at call sites.

## Operational behavior

Each page is a fresh managed request with the same declared endpoint identity,
authentication, applicable rate-limit plan, hooks, timeout selection, status
handling, and response specification. Pages run sequentially. Direct
completion runs for each page, followed by one aggregate pagination completion
event for the fixed final outcome.

Pagination is all-or-error: if a later page fails, the ordinary terminal does
not return the earlier items as partial success. Controllers are checked for
progress, and repeated canonical state is rejected as a cycle rather than
looping.

Concord enforces independent positive ceilings for pages, retained items, and
the canonical pagination identities used for cycle detection. The requested
`PageLimit` cannot raise those runtime ceilings. Exact defaults and first
excess behavior belong to the [limits reference](../reference/limits.md).

## Pagination-controlled values

Bindings identify exactly which endpoint values the controller may change for
the next page. Other client, scope, and endpoint values remain fixed across
the operation. The controller never receives a native request; it returns the
next semantic state, and Concord shapes the next bounded public request.

Rate-limit partitioning cannot depend on a value that pagination changes. A
host partition is also rejected when final host shaping depends on a
pagination-controlled value. This keeps one logical pagination operation from
silently moving through unrelated quota identities. See [Rate
limiting](rate-limiting.md) for that cross-feature rule.

Runtime-defined endpoints do not have partial runtime pagination. Use a static
declaration when the endpoint needs generated `.paginate(...)`, or write an
application loop with its own explicit bounds when the entire protocol is
truly dynamic.

Implement a custom `PaginationController` only when page/offset/cursor helpers
cannot express the service. Custom state, CPU, allocation, and side effects
remain application-owned bounds.

## Related

- [Rate limiting](rate-limiting.md)
- [Hooks and observability](hooks-and-observability.md)
- [The managed lifecycle](../concepts/managed-lifecycle.md)
- [Limits reference](../reference/limits.md)
- [Pagination controllers](../extending/pagination.md)
