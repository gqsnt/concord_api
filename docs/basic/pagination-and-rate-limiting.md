# Pagination and rate limiting

> Audience: Basic  
> Covers: offset/page/cursor pagination, lexical quotas, the local governor, and observer behavior

Pagination and rate limiting both compose with the ordinary request lifecycle. Pagination owns evolving request-local state; generated rate limits are owned lexically by clients, scopes, and endpoints.

## Pagination

A paginated endpoint is bodyless, returns a buffered page type whose output implements `IntoIterator`, and declares a typed controller with explicit live bindings.

```rust
GET items(offset: u64, limit: NonZeroU32) -> Json<Vec<Item>> {
    query { "offset" = ep.offset; "limit" = ep.limit; }
    paginate concord::pagination::OffsetPagination {
        offset: bind ep.offset,
        page_size: bind ep.limit,
    }
}
```

Application use:

```rust
use concord::PageLimit;

let items = client
    .items(0, NonZeroU32::new(50).unwrap())
    .paginate(PageLimit::Take(4))
    .await?;
```

### Page limits

- `PageLimit::Take(n)` collects at most `n` pages and succeeds after the final allowed page without requesting another continuation. `Take(0)` performs no controller or request work.
- `PageLimit::HardCap(n)` allows natural completion through page `n` and returns `HardCapExceeded` when another continuation still exists at the cap.
- `u32` converts to `PageLimit::Take`.

### Built-in extras controllers

#### Offset

`concord::pagination::OffsetPagination` adds the requested page size using checked arithmetic. A decoded page shorter than the page size terminates naturally.

#### Page number

`concord::pagination::PagePagination` increments a one-based page number with checked arithmetic. Short-page behavior matches offset pagination.

#### Cursor

`concord::pagination::CursorPagination<C>` asks the decoded page for `concord::pagination::HasNextCursor<C>`. It does not stop merely because an item page is empty; metadata can still contain a next cursor.

### Bindings

`field: bind ep.name` or `field: bind scope.name` creates a live round-trip binding:

1. the initial endpoint/scope value initializes the controller field once;
2. before every page, the current controller field is copied into a fresh endpoint clone;
3. the controller remains the only authority for evolving state.

Unbound fields initialize stable controller-local configuration and are never copied back.

### Runtime behavior

- Pages execute sequentially; there is one active page request.
- Every page uses a fresh operation deadline and child operation identity.
- Authentication recovery occurs within one page and reacquires the same lexical limiter plan.
- Results are all-or-error; no partial vector is returned.
- Full canonical request identity prevents cycles before repeated send.
- Page, item, and identity-storage ceilings are finite.
- Cancellation prevents later pages but does not roll back completed page effects.
- Page completion and aggregate pagination completion are separate events.

### Unsupported pagination forms

There is no prepared pagination, page stream, prefetch, parallel execution, partial success, or separate `.collect()` terminal. See [Unsupported behavior and alternatives](../concepts/security-bounds-and-limitations.md#unsupported-behavior-and-alternatives).

For custom controllers, see [Custom pagination controllers](../advanced/pagination-and-rate-limits.md#custom-pagination-controllers).

### Complete example

See [`pagination.rs`](../../concord_examples/examples/pagination.rs) for offset, page-number, cursor, and application-defined pagination.

## Rate limiting

Normal generated rate limits are declared directly at the lexical layer they own.

```rust
api! {
    client Api {
        base "https://example.test/";
        rate_limit {
            500 / 10s;
            30000 / 10m;
        }
    }

    scope regions(region: String) {
        rate_limit { 20 / 1s; }

        GET read(id: String) -> Json<Item> {
            rate_limit {
                cost 2;
                100 / 1m;
            }
        }
    }
}
```

### Lexical composition

An endpoint receives, in exact order:

1. optional client rule;
2. optional outer-to-inner scope rules;
3. optional endpoint rule.

Children add rules. They cannot replace, disable, or modify ancestor rules.

### Bucket semantics

Normal generated rules use:

- one deterministic identity per lexical declaration;
- `RateLimitKey::global()`;
- static cost;
- static windows.

Runtime values do not partition the bucket. Different region parameter values share the same scope declaration bucket; different endpoint arguments and pagination continuations share the same endpoint declaration bucket.

This simplicity is intentional. Per-user, per-item, per-region-value, dynamic-host, or discontiguous cross-branch generated buckets are not supported by the normal DSL.

### Observer

Observer policy is runtime configuration:

```rust
let limiter = GovernorRateLimiter::new()
    .observer(Arc::new(ProviderObserver));
let client = Api::builder()
    .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
    .build()?;
```

Limiter and observer selection are runtime policy and have no DSL clauses. An
explicitly supplied custom limiter owns its observation policy and always wins
over the generated default.

The default observer interprets bounded future-only `Retry-After` information. Observation cannot replace, retry, suppress, or otherwise alter the current response.

### Governor behavior

The built-in governor is a bounded, best-effort, process-local pacing mechanism. It provides atomic reservation across all rules and windows, cancellation-safe waiting, bounded quota/cooldown maps, idle pruning, and deterministic eviction. It is not a distributed or strict authoritative quota service.

### Feature-disabled behavior

Without `rate-limit-governor`:

- a generated client with an empty plan can build normally;
- a generated client with a nonempty plan requires an explicit functional limiter or deliberate `NoopRateLimiter`;
- a prepared nonempty plan on the fallback fails before send.

### Advanced keys

Runtime endpoints and custom integrations can construct explicit bounded `RateLimitKey` values. Normal generated DSL cannot. See [Custom rate limiters](../advanced/pagination-and-rate-limits.md#custom-rate-limiters).

### Complete example

See [`rate_limiting.rs`](../../concord_examples/examples/rate_limiting.rs) for lexical client/scope/endpoint rules, observation, and advanced prepared keys.
