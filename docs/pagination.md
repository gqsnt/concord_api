# Pagination and traversal

Ordinary `.await` on a paginated endpoint returns **one** page. More pages are
always an explicit terminal with an explicit bound, under one aggregate
deadline, and the result never pretends a page bound is exhaustion (§14).

## Declaring a traversal

<!-- concord-source: crates/concord-examples/src/pagination/mod.rs#offset_endpoint -->
<!-- concord-dsl -->
```rust
GET offset(query offset: u64, query limit: NonZeroU32) -> Vec<u32> via Json {
    paginate via OffsetPagination {
        init OffsetInit { offset: offset, page_size: limit };
        cursor -> offset;
    }
}
```

- `paginate via Controller` names a `PaginationController` for the endpoint's
  page type.
- `init InitType { field: value, ... };` constructs the controller's **public**
  initialization type from stable values or references; all required fields
  are supplied, and `..Default::default()` or method calls are not DSL syntax.
  The controller's private state is separate and can evolve without a DSL edit.
- `cursor -> slot;` maps the controller's **entire** `Cursor` value onto the
  endpoint's entire slot (PAGE-01). For `after?: String` the cursor is
  `Option<String>`; a tuple destination `(a, b)` maps a tuple cursor of exactly
  that arity and types. There is no hidden field extraction, `Option` wrapping
  or conversion.
- Destinations are unique, endpoint-owned scalar slots: never bodies, client or
  scope values, credentials, `each` sources, or values in an effective rate key.
  Their sensitivity carries over to continuation requests.

A paginated endpoint must be `effect safe`, use a buffered response
representation, and, when it declares a body, use a `PageRequestRepresentation`
so each page can prepare the same captured body. A `POST` search can qualify by
asserting `effect safe`:

<!-- concord-source: crates/concord-examples/src/pagination/mod.rs#post_search_endpoint -->
<!-- concord-dsl -->
```rust
POST search(
    query offset: u64,
    query limit: NonZeroU32,
    body filter: SearchFilter via Json,
) -> Vec<u32> via Json {
    effect safe;
    path "search";
    paginate via OffsetPagination {
        init OffsetInit { offset: offset, page_size: limit };
        cursor -> offset;
    }
}
```

No `IntoIterator` marker on the page type is required: the controller owns item
extraction, so an envelope decoded by `Json` is pageable (see the
[envelope controller](extensions.md#pagination-controllers)).

## Terminals

```text
request.pages(maximum_pages: u32) -> Pages<Item>
request.collect_prefix(maximum_pages: u32) -> CollectPrefix<Item>    // resolves to Prefix<Item>
request.collect_all(maximum_pages: NonZeroU32) -> CollectAll<Item>   // resolves to Vec<Item>
```

All construction is inert. `CollectPrefix` and `CollectAll` implement
`RequestControls` and the timeout/cancellation controls. `Pages` does not:
apply controls to the request before calling `.pages()`; its captured
cancellation tokens can still be cancelled externally. There is no generic
`paginate(limit)` selector.

### Complete collection

<!-- concord-source: crates/concord-examples/src/pagination/offset.rs#paginate_all -->
```rust
pub async fn collect_all(client: &PaginationApi) -> Result<Vec<u32>, concord::ApiError> {
    client
        .offset(0, NonZeroU32::new(50).expect("positive page size"))
        .collect_all(NonZeroU32::new(100).expect("positive completion limit"))
        .await
}
```

`collect_all` returns a `Vec` only after the controller proved exhaustion
(`Done`). Reaching the bound while the controller still offers continuation is
a pagination completion-limit error carrying the traversal summary, never a
success `Vec` holding an undisclosed prefix.

### Explicit prefix

<!-- concord-source: crates/concord-examples/src/pagination/offset.rs#execute_pagination -->
```rust
pub async fn collect(client: &PaginationApi) -> Result<Prefix<u32>, concord::ApiError> {
    client
        .offset(0, NonZeroU32::new(50).expect("positive page size"))
        .collect_prefix(2)
        .await
}
```

`Prefix<Item>` has `items()`, `into_items()` and `summary()`, so the caller can
tell exhaustion from the page bound.

### Lazy pages

`Pages<Item>::next_page()` resolves to `Result<Option<Page<Item>>, ApiError>`;
`summary()` snapshots the traversal and `finish()` ends it. A `Page<Item>` has
`index()` (zero-based), `items()`, `into_items()` and `metadata()`, a fixed
snapshot as of that page's acquisition. The canonical application search stops
early without claiming the item does not exist:

<!-- concord-source: crates/concord-examples/src/application/search.rs#find_summary -->
```rust
use concord::pagination::{StopReason, TraversalSummary};
use concord::{ApiError, CallOptions};

use crate::contracts::catalog_v3::{ItemSummary, catalog_api};

/// The truthful outcome of a bounded search.
#[derive(Debug)]
pub enum SearchResult<T> {
    /// The item was found; the traversal was stopped by the application.
    Found {
        /// The matching item.
        item: T,
        /// The traversal summary at the application stop.
        traversal: TraversalSummary,
    },
    /// The provider proved exhaustion without a match.
    Exhausted {
        /// The exhausted traversal summary.
        traversal: TraversalSummary,
    },
    /// The traversal stopped for another reason (for example the page bound);
    /// the item may still exist.
    Stopped {
        /// The traversal summary, including its stop reason.
        traversal: TraversalSummary,
    },
}

/// Searches the feed for `wanted` over at most `maximum_pages` pages.
///
/// # Errors
///
/// A page fetch failure (returned once by the traversal).
pub async fn find_summary_within(
    scope: &catalog_api::regional::Regional,
    wanted: &str,
    limits: &CallOptions,
    maximum_pages: u32,
) -> Result<SearchResult<ItemSummary>, ApiError> {
    let mut pages = scope.feed().with_options(limits).pages(maximum_pages);
    while let Some(page) = pages.next_page().await? {
        for item in page.into_items() {
            if item.id == wanted {
                return Ok(SearchResult::Found {
                    item,
                    traversal: pages.finish(),
                });
            }
        }
    }
    let summary = pages.finish();
    if summary.stop_reason() == StopReason::Exhausted {
        Ok(SearchResult::Exhausted { traversal: summary })
    } else {
        Ok(SearchResult::Stopped { traversal: summary })
    }
}

/// The §23.4 search with its canonical 100-page bound.
///
/// # Errors
///
/// A page fetch failure.
pub async fn find_summary(
    scope: &catalog_api::regional::Regional,
    wanted: &str,
    limits: &CallOptions,
) -> Result<SearchResult<ItemSummary>, ApiError> {
    find_summary_within(scope, wanted, limits, 100).await
}
```

Processing time between loop iterations counts against the aggregate deadline,
and stopping early never requires reconstructing auth, quota or retry state.

### Saving and resuming a position

`Pages::checkpoint()` returns `Result<Checkpoint, CheckpointError>`. A checkpoint
describes the next page after the last batch delivered to the application. It
is available before the first fetch, between pages, at a page bound, and after
a fetch failure. Saving performs no HTTP, credential acquisition or quota
admission. It may call the controller's synchronous state methods, shape the
public request binding and prepare a cloned buffered body to fingerprint it.

`Checkpoint::to_token()` produces a portable ASCII token of at most 4,096
characters. `Checkpoint::from_token(&str)` validates and reloads it;
`next_page_index()` reports the next zero-based page index. Recreate the typed
request and call `request.resume_pages(&checkpoint, maximum_pages)` to obtain
`Result<Pages<Item>, CheckpointError>`:

<!-- concord-pseudocode -->
```rust
use concord::pagination::{Checkpoint, ContinuationKnowledge};

let mut pages = client.offset(0, page_size).pages(2);
while let Some(page) = pages.next_page().await? {
    persist_items(page.into_items()).await?;
    if pages.summary().continuation() == ContinuationKnowledge::NextRequestKnown {
        persist_token(pages.checkpoint()?.to_token()).await?;
    }
}

// Later, possibly in another process, recreate the same public inputs.
let checkpoint = Checkpoint::from_token(&load_token().await?)?;
let mut resumed = client
    .offset(0, page_size)
    .resume_pages(&checkpoint, 2)?;
while let Some(page) = resumed.next_page().await? {
    persist_items(page.into_items()).await?;
    if resumed.summary().continuation() == ContinuationKnowledge::NextRequestKnown {
        persist_token(resumed.checkpoint()?.to_token()).await?;
    }
}
```

Save a token only when another request is known; exhaustion returns
`CheckpointErrorKind::Finished`. A checkpoint can also be
saved before starting, when no batch has been delivered. Saving after a failed
fetch retries the failed page; no earlier explicit checkpoint is required.
An exhausted or cancelled traversal cannot create a new checkpoint. A
zero-bound `pages(0)` can save its initial position without fetching; a
zero-bound resumed traversal retains its supplied position without fetching.

Resuming is a new traversal: it allocates a new operation identity and deadline
on its first fetch, and starts fresh item, byte, send, retry and cycle-detection
budgets. The new page bound counts pages fetched by this traversal. Summary
counters describe this traversal only; `Page::index()`, metadata and error page
indices continue from the checkpoint's `next_page_index()`. Resume construction
performs validation and shaping but no managed operation or HTTP request.

Core binds a token to the endpoint, controller type and public non-cursor
request inputs, including the logical origin, path and public query/header
occurrences, plus a fingerprint of the buffered body and its media type. A
different endpoint, controller, public input or body is rejected before
dispatch. Sensitive non-cursor query/header values and credentials are
excluded from this binding; the recreated request supplies their current
values. The cursor slots are restored from the token, so their
starting values do not need to match. Apply new controls to the recreated
request before calling `resume_pages`. Recreate the same semantic request body
and credential configuration yourself; tokens contain neither body bytes nor
credential material and are not an authentication mechanism. A stream,
factory or multipart body cannot be fingerprinted synchronously and returns
`NotResumable` without executing the body source.

Checkpoints require an opted-in `ResumableController`; ordinary custom
controllers continue to work with `.pages()` and report `NotResumable` when
asked for a checkpoint. The built-in offset, page-number, header and link
controllers support checkpoints, as does `CursorPagination<String>` and its
primitive integer variants. See [controller extensions](extensions.md#durable-controller-state).

Tokens encode controller state as plain data. A checksum detects corruption;
it does not prevent a caller from forging a token. Protect stored tokens and
do not log them. A `#[sensitive]` cursor destination or controller-initialization input is
rejected with `SensitiveCursor`, even before the first fetch. `Checkpoint` debug output and
checkpoint errors omit state and input values. Malformed, unsupported-version
and oversized tokens produce typed, value-free errors.

Checkpointing does not commit application work or provide exactly-once
delivery. Store processed items and their checkpoint with the consistency your
application needs; replaying a saved token can deliver those items again, and
provider data may change between traversals.

## Summary facts

`TraversalSummary` is owned and cloneable: `operation_id()`, `stop_reason()`,
`continuation()`, `requested_page_bound()`, `effective_page_bound()`,
`pages_received()`, `pages_delivered()`, `items_delivered()`,
`application_attempts()`, `automatic_replays()`, `quota_admissions()`,
`acquired_response_bytes()`, `retained_identity_bytes()` and
`dispatch_knowledge()`.

| `StopReason` | Meaning |
|---|---|
| `NotStarted` | nothing ran |
| `Active` | the traversal can continue |
| `Exhausted` | the controller returned `Done` (provider-contract proof of exhaustion) |
| `PageBound` | the page bound was reached |
| `Application` | the application stopped (`finish()`) |
| `Cancelled` | a started fetch was dropped or a token fired |
| `Failed` | a fetch failed; the error was returned once |

`ContinuationKnowledge` is `Unknown`, `NextRequestKnown` (a valid next request
can be constructed; not a promise that another item exists) or `Exhausted`. A
`Done` on the last permitted page yields `Exhausted` without an extra fetch;
reaching the bound while continuation is offered yields
`PageBound`/`NextRequestKnown`.

`collect_prefix(0)` and `pages(0)` perform no shaping, body work, controller
start, credentials, quota or transport and allocate no operation; their summary
is `PageBound`/`Unknown` with no operation ID. An unstarted nonzero
`pages(n).finish()` records `Application`/`Unknown` (PAGE-02).

## Page atomicity and progress

The first polled fetch starts one aggregate deadline and finite budgets covering
controller start, all pages and attempts, and time spent in application code
between polls; there is no per-page reset and no background prefetch
(PAGE-03). At most one page fetch is active.

Before the first dispatch, core checks that the controller's initial cursor,
applied to a copy of the selected slots, produces the same public request as the
current request (`InvalidInitialCursor` otherwise). For each accepted page, core
completely acquires and decodes it, calls `advance`, enforces item and byte
bounds and validates the proposed next cursor and progress identity **before**
delivering the current page. A controller error or an invalid or cyclic
continuation withholds the current batch but still records that the page was
received. Eager collectors fail as a whole.

The progress identity covers the method, logical origin and path, ordered
public query/header occurrences and the body contract, never credential bytes.
Immediate repeats and longer cycles are rejected, and every next cursor passes
the same public shaping and ownership rules as the initial request, so a
controller cannot mutate auth, origins, rate dimensions, effect, status,
timeouts or unmapped inputs.

## Resources and cancellation

| Traversal limit | Default | Runtime setter |
|---|---|---|
| Pages (intersected with the requested bound) | 1,000 | `pagination_max_pages(u32)` |
| Delivered items | 100,000 | `pagination_max_items(usize)` |
| Acquired response bytes, all attempts | 64 MiB | `pagination_total_response_body_limit(usize)` |
| Retained progress identity | 4 MiB | `pagination_identity_limit(usize)` |
| Application handoffs | 2,000 (intersected with `cap_sends`) | `aggregate_max_sends(u32)` |

Acquired bytes count every attempt in the traversal, including rejected
statuses, retry responses and failed partial acquisitions (PAGE-04). The item
ceiling applies to the whole current batch; no page is silently truncated.

Dropping an **unpolled** `next_page()` future changes nothing. Dropping a
**started** one cancels the whole traversal and records `Cancelled`; that
traversal cannot continue. An earlier saved checkpoint can start a new
traversal. A fetch error is returned once, then `next_page()` returns `Ok(None)`
with `Failed` retained. `finish()` preserves an already terminal state and
otherwise records `Application`. Dropping a started traversal records
cancellation, never EOF or rollback. (Stream reads differ: see
[Streams](streams.md).)

## Built-in controllers

<!-- concord-source: crates/concord-examples/src/pagination/mod.rs#page_endpoint -->
<!-- concord-dsl -->
```rust
GET page(query page: NonZeroU64, query per_page: NonZeroU32) -> Vec<u32> via Json {
    paginate via PagePagination {
        init PageInit { page: page, page_size: per_page };
        cursor -> page;
    }
}
```

<!-- concord-source: crates/concord-examples/src/pagination/mod.rs#cursor_endpoints -->
<!-- concord-dsl -->
```rust
GET cursor(query cursor?: String) -> ExamplePage via ExamplePageCodec {
    paginate via CursorPagination<String> {
        init CursorInit { cursor: cursor };
        cursor -> cursor;
    }
}

GET custom(query cursor?: String) -> ExamplePage via ExamplePageCodec {
    paginate via ExampleController {
        init ExampleStart { cursor: cursor };
        cursor -> cursor;
    }
}
```

| Controller | Init and Cursor | Continuation |
|---|---|---|
| `OffsetPagination` | `OffsetInit { offset: u64, page_size: NonZeroU32 }`; `u64` | a short page is `Done`; otherwise offset + page size (checked) |
| `PagePagination` | `PageInit { page: NonZeroU64, page_size: NonZeroU32 }`; `NonZeroU64` | a short page is `Done`; otherwise page + 1 (checked) |
| `CursorPagination<C>` | `CursorInit<C> { cursor: Option<C> }`; `Option<C>` | page implements `HasNextCursor<C>`; an absent cursor is `Done` |
| `NextHeaderPagination` | `CursorInit<String>`; `Option<String>` | reads `X-Next-Cursor`; absent is `Done`; duplicates or invalid text are `InvalidContinuation` |
| `LinkCursorPagination` | `CursorInit<String>`; `Option<String>` | one `rel=next` link on the same origin and path with exactly one `cursor` query value and no other change |

Link parsing handles commas inside angle brackets and quotes. A next link that
changes the path, carries userinfo or a fragment, or changes any non-cursor
query occurrence is rejected: the controller owns only the cursor slot and never
uses a link as an arbitrary request URL. Controllers may declare at most 16
header interests with 64 KiB of retained evidence. For a provider-specific
contract, write a narrow custom controller ([Extensions](extensions.md)).
