# Pagination

Pagination is an explicit safe aggregate operation, not syntactic sugar for an unbounded loop.

## Declaration

```concord-pseudocode
GET item_summaries(
    offset: u64 = 0,
    count: PageSize = DEFAULT_PAGE_SIZE,
) -> ItemPage {
    effect safe;
    paginate via OffsetPages {
        offset -> endpoint.offset;
        page_size -> endpoint.count;
    }
}
```

The one-page endpoint output remains `ItemPage`. The pagination controller owns extraction of aggregate items and continuation.

## Capability gates

Generate `.paginate(...)` only when:

- pagination is declared;
- endpoint effect is `safe`;
- response output is complete, not `ByteStream` or `ItemStream<T>`;
- representation/output pair is page-capable;
- body is page-stable when present;
- controller and bindings satisfy generated Rust obligations.

An idempotent mutating operation is not enough. An all-or-error convenience loop must not hide partial mutation across pages.

## Bindings

Live destinations are endpoint-owned ordinary scalar values only.

Reject:

- client or scope values;
- body inputs;
- secrets/credentials;
- timeout/effect/policy controls;
- repeated/multi-value destinations;
- duplicate fields/destinations;
- values used in an effective rate-key dimension.

Each page is a fresh immutable endpoint/request snapshot. The original request remains reusable.

## Controller contract

Conceptually:

```rust
pub trait PaginationController<Page>: Sized + Send + 'static {
    type Item: Send + 'static;

    const DESCRIPTION: PaginationDescription;

    fn validate(&self) -> Result<(), PaginationError>;

    fn advance(
        self,
        page: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError>;
}

pub enum PageStep<C, T> {
    Done { items: Vec<T> },
    Continue { items: Vec<T>, next: C },
}
```

The exact signature is prototype-gated. The authority is fixed:

- controller owns item extraction, completion, and next controller state;
- item and continuation return atomically;
- controller sees no client, credentials, limiter, native request/response, or arbitrary mutation;
- core owns sequence, bindings, request execution, auth/retry/rate/deadline, limits, identity, aggregation, and errors.

Returning a concrete `Vec<T>` bounds/eagerly materializes the controller contribution and prevents effectful/infinite iterators. Core still checks count before retaining.

## Bounded response metadata

`PaginationDescription` declares:

- a finite static set of response header names;
- whether current public page URI is required for resolving relative links;
- any structural page identity contribution.

`PageResponse<Page>` contains:

- exact complete decoded page value;
- accepted status;
- only requested bounded header values;
- current public logical page URI when requested;
- page/operation correlation.

No native response, unrestricted header map, cookies, private auth, or credentials.

For `Link` pagination, relative links resolve against the current logical page URL; cross-origin continuation is rejected before dispatch.

## Page body

Baseline:

```text
prepare once per page
replay prepared state only within that page’s attempts
```

`PageStableInput` allows independent page preparation after page-bound values change. It does not imply one prepared body can live across the entire aggregate. Aggregate reusable prepared bodies remain deferred pending a stronger contract and benchmark.

## Caller limits

```rust
PageLimit::Prefix(n)
PageLimit::CompleteWithin(non_zero_n)
```

- `Prefix(n)` intentionally returns at most `n` accepted pages even when continuation exists;
- `Prefix(0)` performs no operation allocation, shaping, body/provider/limiter/controller/observer/HTTP work;
- `CompleteWithin(n)` fails if natural completion needs more pages.

Do not accept a bare integer that hides truncation semantics.

## Aggregate lifecycle

One pagination terminal has:

- one aggregate operation ID;
- one absolute deadline;
- one shared retry-load budget;
- page operation IDs;
- per-page visible attempt ceilings;
- aggregate send ceiling;
- page/item/accepted-page-byte/controller-identity/cycle/progress bounds;
- one final aggregate result and event.

No page receives a fresh deadline or shared retry budget.

## Cycle and progress identity

Use actual canonical credential-free public request semantics and declared controller contributions. Sensitive values participate internally but are redacted in diagnostics. Credentials and credential versions never create progress.

Reject repeated identity, oversized identity, and non-progress under finite history bounds.

## Output and partiality

Ordinary `.paginate()` is all-or-error locally. A failed page or completeness failure returns no partial aggregate collection. Remote earlier pages may already have been observed; the endpoint must be safe so those observations are not hidden mutation.

Partial/resumable or page-stream output is a separate deferred capability with explicit partiality, cancellation, resume, and resource semantics.

## Observation

Public operation observation distinguishes aggregate, page, and visible attempts. It may emit bounded `PageFinished` events linked to the aggregate. It does not grant a result-determining page callback.
