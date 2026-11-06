# concord-pagination

Bounded page and job traversal for [Concord](https://github.com/gqsnt/concord_api) SDKs, over
plain request factories. The crate does not depend on Concord: an SDK adapter supplies a closure
that turns a key into an owned future (a generated operation with the key set, `send()`, and a
projection of the typed response into a `Page`). The crate schedules those futures on the caller's
task, bounds them, orders their outcomes and reports what is left. It spawns no task, needs no
runtime (no Tokio dependency) and never retries.

## Ordinary use: a cursor listing

`Pages<T, K, M, E>` is an owned dependent chain that an SDK can name and return; `items()` gives
`PageItems<T, K, E>` and `into_stream()` an `ItemStream` (a `futures_core::Stream`). The end is
explicit:

- `next_page()` / `next()` return `Ok(None)` **only** once the provider's last page (no next key)
  was read; an empty page with a next key continues.
- A cap is opt-in (`max_pages`, none by default). Reaching it while the provider has a next page
  is `Err(PagesError::Bounded { resume, .. })`; a cap reached exactly on the last page is
  exhaustion.
- A failed page is `Err(PagesError::Provider { error, failed, .. })` with the factory's own error,
  moved. Every call after a `Bounded` or `Provider` error is `Err(PagesError::Ended { .. })`, never
  `Ok(None)`.
- `collect_complete()` returns everything or an `Incomplete` with the collected prefix and the
  error. The stream yields the typed `Err` before it ends, so a `None` after an `Err` is not
  completeness.

The factory and its futures must be `Send + 'static` (the facade erases their types); each page
request costs one allocation for its future.

```rust
use std::num::NonZeroUsize;
use concord_pagination::{Incomplete, Page, Pages, Resume};

#[derive(Debug)]
struct ApiError;

// An SDK adapter: request with the cursor, select the continuation from the full response,
// move the nested items out. The SDK names and returns the traversal.
fn invoices() -> Pages<u32, String, (), ApiError> {
    Pages::new(Resume::Start, |cursor: Option<String>| async move {
        Ok(match cursor.as_deref() {
            None => Page::new(vec![1, 2], Some("c1".into())),
            _ => Page::new(vec![3], None),
        })
    })
}

async fn all_invoices() -> Result<Vec<u32>, Incomplete<u32, ApiError, String>> {
    // `Ok` only after the provider's last page; a cap or a failure keeps the prefix.
    let cap = NonZeroUsize::new(50).unwrap();
    invoices().max_pages(cap).items().collect_complete().await
}
```

## The engine: every traversal shape

| Entry point | Shape | Typical provider |
|---|---|---|
| `sequential(from, budget, fetch)` | dependent chain: the next key comes from the previous page | opaque body cursor, `Link: rel="next"`, next-page URL |
| `Sequential::items()`, `resume_items(at, budget, fetch)` | the chain flattened into items with item-level checkpoints | |
| `independent(keys, options, fetch)` | a known lazy plan of independent keys, bounded window | page numbers or offsets the provider lets you address independently |
| `first_page_then(first, options, fetch, plan)` | the first page fixes the plan (a validated total), then independent keys | `total_pages` envelopes |
| `manifest_jobs(from, options, discover, job)` | a sequential manifest chain discovers descriptors; each job is a whole streamed download (request, parsing, sink through EOF) | result manifests listing result URLs |

Every engine traversal has `async fn next(&mut self)` (cancellation-safe), which returns `None`
when it ended for any reason, `progress()` counters, a `checkpoint()` snapshot and
`finish(self)`, which returns a report whose `Outcome` says how it ended. Engine traversals
require neither `Send` nor `'static`. `JobsReport::into_resume` and
`IndependentReport::into_resume` decompose a report without loss (outcome, counters, retained
results, unemitted errors, classified rerun work, queued or planned work, discovery state).

```rust
use std::num::NonZeroUsize;
use concord_pagination::{sequential, Budget, Outcome, Page, Resume};

async fn list(cursor: Option<String>) -> Result<Page<u32, String>, std::io::Error> {
    Ok(match cursor.as_deref() {
        None => Page::new(vec![1, 2], Some("c1".into())),
        _ => Page::new(vec![3], None),
    })
}

async fn all_items() -> Result<Vec<u32>, std::io::Error> {
    let budget = Budget::new(NonZeroUsize::new(50).unwrap());
    let mut pages = sequential(Resume::Start, budget, list);
    let mut items = Vec::new();
    while let Some(page) = pages.next().await {
        items.extend(page?.items);
    }
    // `None` also ends a bounded chain: the report says which.
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    Ok(items)
}
```

## Guarantees

- **Budgets** count logical factory invocations, charged at admission (failures cannot bypass
  them), including the first page of `first_page_then`; discovery budgets count every manifest
  call, empty ones included. A factory that retries internally makes more HTTP attempts than its
  budget; retry and rate limits are policies you compose in the factory.
- **Window**: futures in flight plus completed results retained for ordered emission never exceed
  it. A job keeps its slot until its whole future (headers, body, sink) completes or is dropped.
  Discovery has one separate slot (`window + 1` owned futures at most); the descriptor backlog is
  bounded. The window is not a byte bound: body and record limits belong to the reader.
- **Errors**: `OnError::Stop` (default) stops admission at the first *observed* failure, even while
  an earlier key is stalled, and drops every other future; `OnError::Continue` returns a keyed
  outcome for every key. `Order::Ordered` (default) or `Order::Completion`. A dependent chain never
  continues after a failure.
- **Reports** distinguish exhaustion, configured bounds, observed failure, the caller's stop and
  incomplete discovery, keep failed keys apart from the restart point, hand back completed results
  that were never emitted, and separate attempted, known-unattempted and undiscovered work.
- **Storage**: besides the window (and the jobs' descriptor backlog), only failed keys are retained,
  bounded by the budget or by `Options::failure_limit`, which ends the traversal with
  `Outcome::FailureLimit`.
- **Cancellation**: dropping a pending `next` (or `next_page`, or a stream's `next()`) keeps the
  request in flight for the following call. Dropping a traversal drops every owned future; nothing
  detached continues. Requests already transmitted may still run on the server; sink output is the
  job's to clean up.
- **Diagnostics**: no `Debug` or `Display` prints a key, cursor, URL, item or metadata value.

Checkpoints belong to the plan they came from (tenant, query, filter, sort, page size, snapshot);
the crate cannot detect a mismatch. Item checkpoints replay the checkpoint page and assume a stable
page. The crate documentation (`cargo doc -p concord-pagination`) has a compiled example per entry
point.

License: MIT OR Apache-2.0.
