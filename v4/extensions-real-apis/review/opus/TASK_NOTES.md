# OPUS caller task notes (written before reading any author conclusions)

Reviewer: Claude Opus 5.5 subagent (strand OPUS, did not author any reviewed code).
Checkout: `/home/canarit/projects/concord-ext44/rv44-opus`, detached at `0dd65501`.
Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, `cargo 1.99.1`. `CARGO_BUILD_JOBS=6`,
`CARGO_TARGET_DIR=review/opus/target`, `cargo test --offline` inside `review/opus/task`.

## Task chosen

Workflow: **GH-A** (GitHub repository activity audit, `examples/github-app/src/audit.rs`).
Changed requirement, implemented through public APIs only (`concord-example-github-sdk` public
exports, `concord-pagination`, `concord`) in `review/opus/task/src/lib.rs`:

1. Incremental processing: each issue / pull request goes to an `AuditSink` as soon as its child
   collections were walked; child pages are streamed and counted, nothing is collected.
2. A durable item position after every processed item (`StoredPosition`: page URL string, skip,
   last number), resumable in a new run with a new context.
3. Continue after an independent child failure (comments, PR details, reviews) with a persistable
   failed key and restart page (`StoredFailure`); a parent page failure still stops;
   `max_child_failures` bounds a run.
4. A repair pass (`repair`) that re-runs only the failed children from their restart pages,
   serially, continuing past failures, reporting new restart points.
5. Staleness check on resume: the replayed page must still have the last processed item at
   `skip - 1`.
6. Alternative attempt of the same traversal with `manifest_jobs` (`via_jobs`).

Outcome: **done**, 9 loopback tests pass (`review/opus/task/tests/keep_going.rs`, output in
`review/opus/task-test-output.txt`), using the GH-A synthetic fixtures read from
`examples/github-app/tests/fixtures/synthetic/` (not modified) and
`concord-example-testserver`.

| Test | What it shows |
|---|---|
| `child_failure_does_not_stop_and_repair_fetches_only_the_failed_page` | PR #12 details 500 and #13 comments page 2 500: the run processes 11, 12, 13, ends `CompleteWithChildFailures` with two persisted failed keys; the repair pass sends exactly 4 requests (details, reviews p1, reviews p2, comments p2 of #13) |
| `repair_that_fails_again_reports_the_new_restart_point` | the repair's still-failed entry carries the *new* restart (`reviews?page=2`); a second repair sends one request |
| `stop_after_one_item_then_resume_from_the_persisted_position` | stop after #11 → `{page_url: None, skip: 1, last: 11}`; new context resumes with 12, 13 (page 1 replayed, #11 children not refetched); stop after the last item of a page stores the next page with `skip 0` |
| `changed_listing_is_refused_on_resume` | page 1 now starts with #13 → `Stale { expected: 11, found: Some(13) }`, position unchanged, nothing processed |
| `parent_failure_stops_with_a_page_position_and_resumes` | issues page 2 500 → `Failed` with page-2 position; resume processes #13 only |
| `child_failure_limit_stops_after_the_item_that_reached_it` | two failures, limit 2 → `Stopped` after #12 |
| `persisted_child_link_restored_into_another_parent_is_not_refused` | probe (see surprises #3) |
| `manifest_jobs_variant_overlaps_discovery_with_jobs` | probe (see surprises #5) |
| `hand_written_traversal_is_serial` | baseline: with a 40 ms reply delay no two requests of the hand-written traversal overlap |

## What I read (in this order)

- `crates/concord-pagination/README.md`, `src/lib.rs` (crate docs), `src/types.rs`,
  `src/sequential.rs`, `src/window.rs`, `src/independent.rs`, `src/jobs.rs` (docs and source).
- `examples/github-app/README.md`, `src/lib.rs`, `src/audit.rs` (the workflow being changed),
  `src/requests.rs`, `tests/support/mod.rs`, first 200 lines of `tests/gh_a_activity_audit.rs`
  (the stand-in pattern).
- `examples/github-sdk/src/lib.rs`, `src/paging.rs`, head of `src/pages.rs`, `src/filters.rs`,
  the public-function list of `src/context.rs`, `src/meta.rs`, `src/error.rs`, the issue models in
  `src/models.rs`.
- `examples/testserver/src/lib.rs` (docs and `Reply`).
- `docs/RECIPES.md` ("Application policies"), the pagination lines of `README.md`.
- Not read before finishing the task: any `FRICTION.md`, `MAINTENANCE.md`, `NOTES.md`,
  `PROVENANCE.md`, `DECISIONS.md`, `NEXT_REVIEW*`, `v4/extensions-real-apis/*.md`.

## Compiler errors

None. The library compiled on the first `cargo build --offline` (14 s cold, shared target). The
only failing test on the first run was my own wrong guess of a fixture value
(`merged: Some(true)`; the fixture says `false`), corrected in the test.

## Workarounds and surprises

1. **Resume staleness check has to live in the factory.** `resume_items` drops the first `skip`
   items of the replayed page internally (`crates/concord-pagination/src/sequential.rs:416-421`);
   the caller never sees them, so it cannot check that the item at `skip - 1` is still the last
   processed one. Workaround: the factory closure holds an `Option<(skip, expected)>` that it
   `take()`s on its first call and checks inside the returned future
   (`review/opus/task/src/lib.rs`, `keep_going_audit`). It works, but the check is coupled to
   "first factory call" rather than to "the replayed page". GH-A itself only checks the item
   *after* the skip (when a child restart is pending), not the skipped one.
2. **`ItemCheckpoint<NextLink>` is not persistable as such.** `NextLink` has no
   serialization; the persistable form is `raw_url()` + `skip`, and restoring needs
   `Url::parse` (`concord::reqwest::Url`) and `LinkPages::restore` on a factory built for the same
   listing, with a URL parse error type of my own (`AuditError::BadStoredUrl`). Fine once known;
   nothing in the pagination crate or SDK docs shows the round trip.
3. **Persisted child continuations lose their binding.** `LinkPages::restore`
   (`examples/github-sdk/src/paging.rs:245-253`) attaches the *factory's* binding to any URL on
   the API origin. A persisted `#13 comments?page=2` URL restored into `#11`'s factory is sent and
   its comment is attributed to #11 (test
   `persisted_child_link_restored_into_another_parent_is_not_refused`, observed: 1 request to
   `/repositories/101/issues/13/comments?page=2`, record `Issue { number: 11, comments: 1 }`).
   In-memory `NextLink`s are refused (`ContinuationMismatch`), persisted ones are not.
4. **Repair with `independent` + `OnError::Continue`: the failed *key* is the old restart.** A
   child job has internal progress (pages); `IndependentReport::failed()` / `into_rerun()` return
   the key the job was started with, so a rerun from them refetches pages already processed.
   The new restart point has to travel in the error value (`Keyed::value`), which I did. Also
   `Options::new(Budget)` needs a non-zero budget, so an empty failure list is a special case.
5. **`manifest_jobs` does not fit a serial provider.** The GH-A shape (a dependent parent chain
   discovering per-item child work) is exactly `manifest_jobs` (discovery = issue pages, job =
   child walk), and it works functionally (records 11, 12, 13, failed `[(13, false)]`,
   `ExhaustedWithFailures`). But discovery runs beside the job window (`window + 1`, documented):
   with window 1 and a 40 ms reply delay, the issue-page request overlapped PR #12's details and
   reviews requests (two overlaps of ~0.06–0.17 ms arrival gap in 10 requests). GitHub asks for
   serial requests, so the hand-written loop is required. Its checkpoint after the first record is
   `admitted=1, queued=[(12, true)], discovery=At(page 2)`: no single persistable item position;
   restarting needs `independent(queued)` followed by `manifest_jobs(discovery)`.
6. **Page metadata is not visible through `Items`** (`sequential.rs:410-423` drops `page.meta`).
   My task did not need it; GH-A works around it with a shared `Arc<Mutex<Vec<_>>>` log written
   from inside each page future (`examples/github-app/src/requests.rs:35-37, 81-92`).
7. Things that worked as documented and saved code: page-boundary checkpoints after the last item
   of a page (`skip 0`, next page), empty pages with `next` passing through without an item, the
   failed page as the resume point of a dependent chain, `Items::finish` after an early stop, the
   factory's `Option<K>` key with `None` = first page, no `Send`/`'static` bounds (closures
   borrow `&RepoContext` freely).

## Size

`review/opus/task/src/lib.rs`: about 560 lines including the `manifest_jobs` alternative and
docs; the traversal itself (`keep_going_audit`, `walk`, children, `repair`) about 300 lines.
