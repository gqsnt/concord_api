# Pagination

The [DSL specification](spec.md#pagination) owns controller declaration and
binding rules. This file defines execution and controller contracts.

## Controller

`PaginationController` owns typed page state and returns one of three
transitions: continue with updated bindings, finish, or fail with a safe
pagination error.

Built-in controllers cover offset, page-number, and cursor protocols. Custom
controllers implement the same trait and receive no transport or external I/O
capability.

## Request execution

`.paginate(limit).await` executes pages sequentially. Each page uses the
ordinary managed executor with a child operation identity.

The controller updates only declared live bindings. Stable initializer fields
do not change. A paginated request body remains page-stable.

The selected rate-limit policy is recomputed for each page from the updated
public request state. A policy key cannot depend on a pagination-controlled
value. A `host` dimension is also rejected when final host shaping depends on
one.

## Termination

Pagination terminates through controller completion, `PageLimit::Take`, or a
failure.

`PageLimit::Take(0)` performs no controller or page work.
`PageLimit::Take(n)` returns the accumulated prefix after at most `n` pages.
The all-pages mode returns only after controller completion.

Page count, item count, retained identity count, retained identity bytes, and
call-site limits are independent. First excess fails without retaining the
excess value.

## Cycle detection

Each prepared page has a canonical identity derived from endpoint ID, method,
logical URL, and public headers. Repeating an identity fails before another
send.

Private authentication and request-body bytes are excluded. Page-stable bodies
cannot hide a repeated public page request.

## Response requirements

Pagination requires a buffered page-capable response specification. Streaming,
record-stream, discard, and explicit response-bytes terminals are not
page-capable.

Later-page failure returns an error and no accumulated all-pages value.
`PageLimit::Take` is the explicit finite-prefix success mode.

## Related

- [DSL pagination rules](spec.md#pagination)
- [Runtime model](runtime.md)
- [Bodies and responses](bodies_and_responses.md)
