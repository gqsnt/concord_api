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
  moved. Every call after an error is `Err(PagesError::Ended { .. })`, never `Ok(None)`.
- `reject_repeated_keys()` is opt-in: before requesting a continuation key already used in this
  traversal, it returns `PagesError::RepeatedKey { resume, progress }` with value-free formatting.
  The successful prefix includes the page that supplied that key; no duplicate request is made
  or charged, and no provider failure is counted. `finish()` reports `Outcome::RepeatedKey`.
- `collect_complete()` returns everything or an `Incomplete` with the collected prefix and the
  error. The stream yields the typed `Err` before it ends, so a `None` after an `Err` is not
  completeness.

The factory and its futures must be `Send + 'static` (the facade erases their types); each page
request costs one allocation for its future.

The repeated-key guard requires `Eq + Hash` in addition to the existing `Clone` key bound. It
stores one cloned key per distinct keyed page requested in a `HashSet` (O(pages) memory,
expected O(1) lookup). Without opting in there is no history allocation, hashing or extra key
clone. Enable it before reading; only later admissions are remembered, and enabling it again
keeps that history. `max_pages` takes precedence if the cap and a repeated key both prevent the
next admission. A cap still bounds a chain of distinct keys. `Resume::At(key)` records its key
on first admission; `Resume::Start` has no key. A new traversal from either starts fresh: a
`Resume` carries no history, so resuming after refusal may reread pages from the earlier prefix.
It never proves a snapshot or deduplicates records. Key identity belongs to the adapter: aliases
for the same URL, empty cursors and contradictory envelopes still require provider validation.

The guard carries through `items()`, `into_stream()` and `item_events()`. `PageItems`, the
`Sequential` engine and its `Items`/`resume_items` traversal also expose
`reject_repeated_keys()`; engine consumers inspect `Outcome::RepeatedKey` in the report after
`next()` returns `None`. A stream emits the typed refusal before ending, and events never emit
`Complete` after refusal.

`Jobs::reject_repeated_keys()` applies the same opt-in to the dependent manifest chain. A
refusal emits the value-free `JobsError::RepeatedKey` once, leaves `discovery.resume` at that
key and counts no manifest call or provider failure. As with discovery failures,
`OnError::Stop` cancels active jobs; `OnError::Continue` finishes known queued and admitted
jobs. The settled report is `Outcome::DiscoveryRepeatedKey`, with counters and undiscovered
resume state retained. Finishing before that known work settles reports `Outcome::Stopped`.
Discovery and job budgets take precedence over a refused admission. Independent job descriptors
are not deduplicated.

When item consumers also need metadata, `Pages::item_events()` returns an item for each record
and moves metadata once after each page's items, including empty pages. `PageItemEvent::Page`
marks an intermediate page; `PageItemEvent::Complete` carries the final page's metadata only
after all its items were delivered. A bound or failure returns a `PagesError`, and stopping
early has no completion event. `finish()` reports the outcome, item checkpoint and remainder.
A completion token in metadata is not a next-page key. Start the event traversal before reading
the final page; it cannot recover metadata from pages already consumed. Plain `items()` is
unchanged. Access a typed factory error with `provider_error()` or move it with
`into_provider_error()`, which also returns its failed position and progress (and returns the
original error for non-provider endings). Neither accessor changes redacted formatting.

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
    invoices().reject_repeated_keys().max_pages(cap).items().collect_complete().await
}
```

The factory and its futures are `Send + 'static`: move a cloned client into the closure. A
helper method that builds the page future from `&self` and returns `impl Future + 'static` still
captures the borrow under edition 2024; write `-> impl Future<Output = ..> + use<..>` naming only
the type parameters it needs (or return an `async move` block from a clone).

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
- **Storage**: besides the window (and the jobs' descriptor backlog), failed keys are retained,
  bounded by the budget or by `Options::failure_limit`, which ends the traversal with
  `Outcome::FailureLimit`. Opting into the repeated-key guard additionally retains one key per
  keyed page requested in that dependent page traversal.
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
