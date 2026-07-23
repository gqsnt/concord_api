# Custom pagination, limiters, observers, and explicit keys

> Audience: Advanced  
> Covers: `PaginationController`, `RateLimiter`, `RateLimitObserver`, and prepared explicit rate-limit plans

Application-defined controllers and limiters extend semantic behavior without replacing the page loop or request executor. Explicit rate-limit keys are available only at the advanced prepared/custom boundary.

## Custom pagination controllers

A custom controller is an ordinary Rust struct implementing `PaginationController<Page>`.

```rust
pub trait PaginationController<Page>: Sized + Send + 'static {
    fn validate(&self) -> Result<(), PaginationError> { Ok(()) }
    fn next(self, page: &Page) -> Result<Option<Self>, PaginationError>;
    fn short_page_size(&self) -> Option<NonZeroUsize> { None }
}
```

DSL construction and bindings:

```rust
paginate crate::VendorCursor {
    cursor: bind ep.cursor,
    region: scope.region,
    empty_pages: 0,
    max_empty_pages: vars.max_empty_pages,
}
```

### Field semantics

- Bound fields initialize from endpoint/scope state once and are written into every fresh page endpoint clone.
- Unbound fields initialize controller-local stable state once.
- Controller fields must be visible to generated code and bound values must be clonable where endpoint snapshots require it.
- The controller itself does not need `Clone` or `Sync`; it is moved from page to page.

### Method semantics

- `validate` runs before every attempted page.
- `next` consumes the current controller after a decoded page.
- `short_page_size = Some(n)` ends naturally when decoded item count is less than `n`.
- `None` means item count does not independently terminate.
- `Some(1)` gives empty-page termination.

### Error precedence

The runtime computes the next transition, accepts items with finite checks, and applies count termination before propagating a retained next-state error. The next controller is committed only after successful item acceptance.

### Constraints

Methods are synchronous, deterministic, non-blocking, and externally side-effect-free. They run outside core locks and outside the per-page operation deadline, so the application must bound their work.

Controllers cannot mutate query/header state directly. They influence requests only through declared bound endpoint/scope fields and ordinary request shaping.

## Custom rate limiters

Implement `RateLimiter` when process-local pacing is insufficient or an application needs externally authoritative policy.

```rust
pub trait RateLimiter: Send + Sync + 'static {
    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>>;

    fn observe(&self, response: RateLimitResponse<'_>) { }
}
```

### Acquisition context

`RateLimitRequest` exposes semantic, body-free information such as:

- endpoint and method;
- logical host;
- operation and parent operation identity;
- page index and recovery index;
- immutable `RateLimitPlan`.

It does not expose credentials, auth material, request/response bodies, native URL, native request, or transport.

### Plans

A plan contains ordered `RateLimitRule` values. Each rule has:

- `RateLimitId`;
- `RateLimitKey`;
- positive cost;
- one or more positive `RateLimitWindow` values.

Normal generated rules use global keys. Runtime endpoints can append explicit keys constructed from bounded `RateLimitKeyPart` values.

### Permit semantics

Return `RateLimitPermit::granted()` when authority is acquired. Dropping the acquisition future cancels waiting. Concord assumes a successful permit represents an external effect and does not refund it after later failure.

### Observation

The selected limiter owns its own `observe` behavior. Observer policy is
configured with the limiter instance rather than embedded in the remote
contract declaration.

### Key limits and redaction

Explicit key parts are intentionally visible to the selected limiter. They remain absent from built-in errors, hooks, completion, debug, and Rustdoc. The default aggregate key bound is 1024 bytes, and the governor revalidates before insertion.

### No-op

`NoopRateLimiter` deliberately grants every plan. Use it only when disabling enforcement is an explicit application decision. It is not selected silently for nonempty plans.

### Distributed enforcement

A custom limiter can call an external service, but Concord does not automatically bound custom state or external I/O beyond dropping the future at cancellation/deadline. Implement strict idempotency, timeout, retry, and cardinality controls in the limiter.

## Rate-limit observers

A `RateLimitObserver` interprets a final response head and optionally returns a future-call cooldown for the built-in governor.

```rust
pub trait RateLimitObserver: Send + Sync + 'static {
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown>;
}
```

Install one on the extras governor, then select that limiter on the runtime
builder:

```rust
let limiter = GovernorRateLimiter::new()
    .observer(Arc::new(ProviderObserver));
let client = Api::builder()
    .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
    .build()?;
```

Limiter and observer selection are runtime policy and have no DSL clauses. An
explicitly supplied limiter always wins over generated defaults.

### Response input

The observer receives body-free response information and the effective semantic rate-limit plan. Observation happens after the response-head hook and before body polling.

### Cooldowns

A `Cooldown` contains a duration and scope:

- current buckets;
- logical host;
- client.

The runtime cap limits server-directed cooldown duration. Observation affects future acquisitions only and cannot alter the current response.

### Default observer

The extras-owned `DefaultRateLimitObserver` handles a final `429` with
positive `Retry-After` delta seconds or HTTP date. It targets current buckets,
or the host when the effective plan is empty. Malformed, past, or zero values
are ignored.

### Ownership

The façade installs an extras-owned `GovernorRateLimiter` when needed and no
explicit limiter was supplied. Installing an explicit limiter means that limiter owns
observation; the DSL observer is not constructed or called.

### Responsibility

Observer code is synchronous. Keep it bounded and do not include sensitive headers or application data in logs. Automatic headers are sanitized, but the extension owns its own storage and metrics cardinality.
