# Pagination

The declared endpoint output is one page. A pagination terminal repeatedly uses
that same endpoint contract and flattens its ordered page items into a bounded
aggregate.

## Directional controller bindings

<!-- concord-source: crates/concord-examples/src/pagination/mod.rs#offset_endpoint -->
```rust
    GET offset(offset: u64, limit: std::num::NonZeroU32) -> Vec<u32> using Json<Vec<u32>> {
        query { "offset" = endpoint.offset; "limit" = endpoint.limit; }
        paginate with concord::pagination::OffsetPagination {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
        }
    }
```

An arrow grants the generated adapter live authority to update exactly one
endpoint slot. An equals initializer copies stable controller state without
granting mutation. Controller fields and destinations are unique and reflected
in source order.

Client/scope values, body input, credentials, secrets, and arbitrary request
state cannot be live destinations. A live pagination slot cannot be a rate-key
dimension. Callers may configure optional/defaulted endpoint values before
pagination; the controller takes over bound slots when the terminal begins.

## Page limits

<!-- concord-source: crates/concord-examples/src/pagination/offset.rs#execute_pagination -->
```rust
pub async fn collect(client: &PaginationApi) -> Result<Vec<u32>, concord::ApiError> {
    client
        .offset(
            0,
            std::num::NonZeroU32::new(50).expect("positive page size"),
        )
        .paginate(PageLimit::Prefix(2))
        .await
}
```

`Prefix(0)` returns an empty aggregate without body, provider, limiter, gate,
observer, or HTTP work. `Prefix(n)` returns at most `n` successfully collected
pages even if continuation remains. It never turns an errored page into partial
success.

`CompleteWithin(n)` requires natural completion by page `n`. If continuation
remains, the terminal returns a pagination-limit error with no partial output.

## Runtime guarantees

Pagination is sequential and cycle-detecting. One aggregate deadline covers
controller work, every page, limiter wait, retry/backoff, recovery, response
acquisition, conversion, and completion. A page cannot reset the timeout.

Independent bounds cover page count, item count, cumulative accepted response
bytes, cycle identity count/bytes, controller state and formatting, attempts,
and deadline. A paginated request body must explicitly prove page stability.

Cycle detection uses real credential-free request identity—including the real
values of redacted public fields—while diagnostics expose only redacted forms.
Credentials never participate. Every actual send reacquires rate admission.

See the [pagination grammar](spec.md#pagination-declarations) and
[runtime semantics](runtime.md#pagination).
