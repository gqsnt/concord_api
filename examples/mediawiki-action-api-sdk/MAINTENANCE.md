# MAINTENANCE — batch content fetch (SDK39, 2026-11-07)

Role: independent maintainer, after the author (commit `5e02cfab`) and the fresh consumer
(commit `4726c2ae`). Requested and observed model/effort are recorded by the coordinator from
harness metadata, not here. Live provider calls: **NOT_RUN**. Every new test runs against
`examples/testserver` on loopback. No shared crate, check, script or other example was edited.
No dependency was added, and root `Cargo.toml` and `Cargo.lock` are unchanged.

## Prior exposure (what I read, in order)

1. Harness context: the global and project `CLAUDE.md` files and the memory index were in my
   context. They contain coordinator instructions, not SDK content. Then the role prompt
   `work/roles/w4-sdk39-maintainer-opus/prompt.md`.
2. Workspace metadata: `git status`, `git log --oneline -3`, and `git show --stat HEAD HEAD~1`.
   **Disclosure:** the consumer commit message summarises finding F1. It says that a page
   answered with only `pageid` + `missing` decodes as an error instead of `ContentUnavailable`.
   I therefore knew F1 before I read `FIRST_USE.md`.
3. The SDK as shipped: `Cargo.toml`, `src/{lib,models,error,pagination,policy}.rs`,
   `tests/{protocol,workflows}.rs`, `tests/support/mod.rs`, all `fixtures/`, `README.md`,
   `VALIDATION.md`, `PROVENANCE.md`. From `tests/first_use.rs` I read the header, the content
   fixtures, the content assertions and the ignored F1 test, because the change must keep that
   file passing.
4. Concord, read-only: `docs/CHEATSHEET.md`; `docs/AUTHOR_GUIDE.md` §4.1a (`query(..)`), §5.10a
   (`limit`), §5.13 (send policy) and §6 (Display newtypes); the `decode_with_meta` part of
   `crates/concord-core/src/response.rs` (it uses the endpoint's declared limit); the
   signatures in `examples/testserver/src/lib.rs`; root `Cargo.toml` lints.
5. Official documentation. I fetched each page once with `curl` on 2026-11-07 at 06:36 UTC and
   saved it under the worktree's `target/tmp/docs/`:
   - https://www.mediawiki.org/wiki/API:Query (`oldid=7933095`)
   - https://www.mediawiki.org/wiki/API:Revisions (`oldid=8361673`)
   - https://www.mediawiki.org/wiki/API:Continue/en (`oldid=8588859`)
6. **After** the implementation, the tests and the mutation runs were finished (06:50 UTC):
   `FRICTION.md`, then all of `FIRST_USE.md`. They led to no code change; they are only
   cross-referenced below.

## What changed for callers

New, additive API:

- `MediaWikiQuery::fetch_pages(&PageIds) -> Result<Vec<PageAnswer>, Error>`. It returns one
  `PageAnswer { pageid, content: Result<PageContent, Unavailable> }` per requested ID, in the
  caller's order. `Unavailable` is `Missing`, `Invalid` or `NoContent`.
- `PageIds::new(ids, PageIdLimit) -> Result<PageIds, TooManyPageIds>`. `PageIdLimit::Standard`
  allows 50 IDs and `HighLimits` allows 500 (API:Query: "Maximum number of values is 50 (500 for
  clients that are allowed higher limits)"). The check happens before anything is sent. A repeated
  ID is sent once and answered at each position, and the limit counts the IDs the caller passed.
  An empty list answers nothing and makes no request.
- The generated endpoint `query.contents(PageIds)` (`MediaWikiQueryContents`) returns one raw
  response, with `continuation?`. It sends `prop=revisions&rvprop=ids|content&rvslots=main` and
  no `rvlimit`. API:Revisions: rvlimit "May only be used with a single page (mode #2)", and
  multiple pages with rvlimit give `rvmultpages`/`invalidparammix`.
- `BATCH_BODY_LIMIT` (16 MiB) bounds `contents` bodies. The other endpoints keep 1 MiB.
- `PageContent` now derives `Clone`.

Continuation semantics follow API:Continue: "the same set of pages is returned but with the
next set of properties". So a page listed **without** a revision in a response that carries
`continue` is not answered yet. It is answered by a later response, or as `NoContent` by the
final one. The whole `continue` object is sent again on each request (the existing
`Continuation`). Each continued response must answer at least one new ID, so a batch makes at
most one request per distinct ID and cannot loop. Once every ID is answered, `fetch_pages`
stops. With one latest revision per page, no further answer is possible.

Fail-closed rules: every one of these is `Error::InvalidResponse`, and no new `Error` variant
was added:
- a row without a page ID, or for an ID that was not requested;
- a repeated row in one response;
- a second revision for an ID that is already answered (a replay);
- two revisions in one row, or content without a title;
- a continued response that answers nothing new;
- an ID that is still unanswered when the provider stops continuing.

Any `send()` failure in any response fails the whole batch (`Http`, `Api`, `Warnings`,
`Concord`). There are no partial batch results.

Existing callers: `search` and `category_members`/`category_pages` are untouched. `content` has
the same wire shape. The behaviour of `fetch_page` changed only for bodies containing a page row
without `title`. All of those used to fail decoding:

| `fetch_page` body | Before | Now |
|---|---|---|
| `{"pageid":N,"missing":true}` (no `ns`/`title`) | `Error::Concord` (`Decode`) | `Error::ContentUnavailable`, as the README already promised (consumer finding F1) |
| matching page with content but no `title` | `Error::Concord` (`Decode`) | `Error::InvalidResponse` |
| any other row without `title` | `Error::Concord` (`Decode`) | the same outcome as the row with a title |

Every outcome for rows with a `title` is unchanged. The existing protocol and first-use assertions
pass unmodified. The single-page and batch fetch now share one row rule
(`ContentPage::project`). Without that, the same provider row would be a decode error through one
call and a typed answer through the other. The consumer's F1 test
(`missing_page_id_without_title_is_content_unavailable`) now passes, so I removed its
`#[ignore]`. Its assertion is identical. This is a strengthening, not a weakening.

## Decisions and alternatives rejected

- **A new endpoint instead of widening `content`.** `content` sends `rvlimit=1`, which is valid
  only for a single page. Changing its parameter type or its query would change existing callers.
- **The limit as a checked input type, not an `Error` variant.** `Error` is an exhaustive public
  enum. A new variant would break exhaustive `match`es in existing callers. `TooManyPageIds` is
  returned by `PageIds::new` instead.
- **No `concord-pagination::Pages`.** `Pages` streams item pages under a resumable key. A batch
  is a fixed key set whose answers are merged across responses, with a progress rule and an end
  condition ("all answered") that `Pages` does not express. Its `PagesError` would also expose
  resume positions that cannot restore the merge state. The loop and the merge rule take about
  60 lines in `src/batch.rs`.
- **The batch body bound.** Up to 500 pages of content arrive in one response until the wiki's
  result-size limit continues it, and 50 encyclopedia articles commonly exceed 1 MiB. The cited
  pages do not state the wiki's limit (MediaWiki's default is 8 MiB: inferred, not verified).
  16 MiB leaves room for JSON escaping. The bound is per endpoint (`limit` nearest wins,
  §5.10a), so other endpoints keep 1 MiB.
- **No partial results** on a failed later response. Partial results would need a second outcome
  shape. They are recorded as a possible extension, not built.

## Every file and location edited

| File | Edit |
|---|---|
| `src/batch.rs` (new) | `PageIdLimit`, `TooManyPageIds`, `PageIds` (wire `Display`), `Unavailable`, `PageAnswer`, `fetch_pages` and the `merge` rule. |
| `src/lib.rs` | `mod batch` and its re-exports; `BATCH_BODY_LIMIT`; the `contents` declaration in `scope query`; `fetch_page` now uses `ContentPage::project`. |
| `src/models.rs` | `ContentPage.title` is now `Option<String>`; new crate-private `Row` and `ContentPage::project` (the single row rule); `Clone` on `PageContent`; `ContentResults` docs. |
| `src/error.rs` | `InvalidResponse` doc mentions batch responses. |
| `tests/batch.rs` (new) | 9 regression/protocol tests (below). |
| `tests/batch_consumer.rs` (new) | 2 consumer tests: an application that fetches search hits plus bookmarks in one continued batch, and chunking by the limit. |
| `tests/support/mod.rs` | `BATCH_FIRST`, `BATCH_LAST`, `BATCH_WIRE_IDS`, `batch_service` oracle. |
| `tests/first_use.rs` | Removed `#[ignore]` from the F1 test and replaced one doc sentence. Assertions untouched. |
| `fixtures/batch-first.json`, `fixtures/batch-last.json` (new) | Synthetic continued batch. |
| `README.md` | Overview line; "Fetching several pages at once" section with a compiled `no_run` example; failure-table rows `InvalidResponse`/`ContentUnavailable`; body-bound sentence; evidence paragraph. |
| `PROVENANCE.md` | Appended a maintenance section: the three pages with revisions, inferred facts, fixtures. |
| `MAINTENANCE.md` (new) | This file. |

### Facts that had to be written in several places

1. The revision-content query (`prop=revisions`, `rvprop=ids|content`, `rvslots=main`) is now
   declared twice, in `content` and `contents`. Only `rvlimit` differs. A shared scope would
   change navigation, and a parameter group would make the values caller-settable.
2. The fixed-key reservation: `Continuation`'s reserved list in `src/models.rs` must contain every
   fixed key of every endpoint that accepts a continuation. `contents` introduced no new key
   (`pageids`, `prop`, `rvprop`, `rvslots` were already reserved). I verified this by hand.
3. The 50/500 limit is written in `PageIdLimit::max` and stated again in the README and the tests.
4. The body bounds: `BODY_LIMIT` on the client and `BATCH_BODY_LIMIT` on the endpoint, each stated
   in the README.
5. The batch rules (pending until a revision arrives, progress, fail-closed cases) appear three
   times: the code, the `fetch_pages` rustdoc and the README (batch section and failure table).
6. The page-row rule used to live only in `fetch_page`. It is now in one place
   (`ContentPage::project`), which removed a duplication the batch would otherwise have added.

### Did the compiler or the tests catch an omitted edit?

I omitted or mutated one edit at a time. Copies were kept under `target/tmp/mutations/` and
the source was restored and checked by `sha256sum` after every run. Each run was
`cargo test --locked --offline -p concord-sdk-mediawiki-action-api --tests --no-fail-fast`.

| Omission / mutation | Caught by |
|---|---|
| O1 re-export of the batch types omitted | **compiler** (`E0432` unresolved import) |
| O4 `Clone` on `PageContent` omitted | **compiler** (`E0277`) |
| O3 a new fixed key (`rvsection`) added to `contents` and **not** reserved in `Continuation` | only the exact-wire test fails, because the wire changed. **Nothing ties the key to the reserved list**: the collision test passes. |
| M14 `pageids` removed from the reserved list | test `a_continuation_cannot_overwrite_the_batch_inputs` (it lists the keys explicitly) |
| M1 a pending page answered as `NoContent` | regression test + consumer test + 3 others |
| M2 continuation dropped on the follow-up | regression test + consumer test + 2 others |
| M3 progress check removed | stalled-continuation test. The fixture answers a third request with an API error, so a loop fails instead of hanging. |
| M4 duplicate-row check removed | invalid-rows test. Before the first mutation run, I had already changed its duplicate case on inspection: the first draft duplicated a row with a revision, which the replay check rejects before the duplicate check is reached. |
| M5 unrequested rows ignored | invalid-rows test |
| M6 replayed revision accepted | **initially not caught**: the progress check also rejected the replay fixture. I added a case where the replay also answers the pending page; now caught. |
| M7 limit off by one, M8 wire IDs not deduplicated, M13 answers in wire order | limit test / wire, regression and limit tests / regression test |
| M9 batch body limit removed, M10 `rvlimit` sent on the batch | body-bound test / wire test |
| M11 untitled content accepted, M12 `missing` flag ignored | invalid-rows + single-page tests / regression + consumer tests |
| M15 `title` required again (the pre-change model) | 9 tests, including the now-unignored first-use F1 test |

The compiler catches structural omissions such as exports, trait bounds and model types. The
provider rules are caught only by tests. The fixed-key reservation is caught only for keys a test
names explicitly.

## Tests

`tests/batch.rs`:

- `batch_sends_distinct_ids_without_rvlimit_and_replays_the_whole_continuation`: the exact pairs
  of both requests; no `rvlimit`; `pageids=12|404|7|31|5|8`; `rvcontinue` plus generic
  `continue` on the follow-up.
- `a_page_listed_without_revision_in_a_continued_response_is_answered_later`: the focused
  regression. It asserts the precondition on the fixture, then the 7 answers in caller order:
  content, `Missing` (title-less), content for the page answered only by the follow-up, the
  duplicate, `NoContent` (`texthidden`), `Invalid`, `NoContent` (no revision at the end). The
  batch takes 2 requests.
- `replayed_or_stalled_continuation_fails_closed_after_one_follow_up`: replay, replay with
  progress, and a stalled continuation. Each is `InvalidResponse` after exactly 2 requests.
- `rows_that_cannot_belong_to_the_batch_are_invalid_responses`: 6 shapes, 1 request each.
- `whole_response_failures_fail_the_batch_and_warnings_are_not_followed`: warnings on the
  continued response (1 request), API error and missing query on the follow-up (2 requests).
- `page_id_limits_are_checked_before_anything_is_sent`: 50/51, 500/501, duplicates count, the
  wire spelling, an empty batch with no request.
- `batch_bodies_have_their_own_bound_and_single_page_bodies_keep_theirs`: the declared decoder
  limits; a 2 MiB content body decodes through `fetch_pages` and is still `Limit` through
  `fetch_page`.
- `a_continuation_cannot_overwrite_the_batch_inputs`: `pageids`, `prop`, `rvprop`, `rvslots`,
  `rvlimit`.
- `single_page_fetch_shares_the_row_rule`: the two changed `fetch_page` shapes.

`tests/batch_consumer.rs`:

- `reading_pack_fetches_hits_and_bookmarks_in_one_continued_batch`: search → 3 hits plus
  bookmarks `[999 (deleted), 102 (repeat)]`. One `PageIds`, one `fetch_pages`, rendered lines in
  order, 3 requests in total.
- `long_bookmark_lists_are_chunked_by_the_documented_limit`: 60 IDs are refused before sending.
  Chunked by `PageIdLimit::Standard.max()` they make 2 requests (50 + 10), all answered.

Final gates, run serially from the worktree root with
`CARGO_TARGET_DIR=<worktree>/target CARGO_BUILD_JOBS=4`:

| Command | Result |
|---|---|
| `cargo test --locked -p concord-sdk-mediawiki-action-api` | exit 0. batch 9, batch_consumer 2, first_use **6** (was 5 + 1 ignored), protocol 11, workflows 6; doctests 2 (README example and new batch example, compile-only). 0 ignored, 0 failed. |
| `cargo clippy --locked -p concord-sdk-mediawiki-action-api --all-targets -- -D warnings` | exit 0 |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-mediawiki-action-api --no-deps` | exit 0 |
| `rustfmt --edition 2024 --check` on the package's `src/` and `tests/` | exit 0 |
| `git diff --check`; `git diff --quiet HEAD -- Cargo.toml Cargo.lock` | exit 0; root manifest and lock unchanged |

Baseline before any edit: 22 passed, 1 ignored, 1 doctest.

## Provider-specific responsibilities (stay in the SDK)

- The page-ID limits and the per-ID answer model (`missing`, `invalid`, `texthidden`, no
  revision).
- `rvlimit` is single-page only, so the batch gets its own endpoint.
- The property-continuation merge: pending versus answered, the "next set of properties" rule
  and when to stop.
- Attribution of rows to requested IDs, and replay or stall detection.
- The pipe-joined `pageids` spelling, and sending each distinct ID once.
- The batch body bound, chosen against the wiki's result-size behaviour.

## Remaining generic duplication

- The revision-content query block appears twice (fact 1 above).
- The `Continuation` reserved-key list re-states every endpoint's fixed keys (fact 2; O3 shows
  that nothing enforces it).
- Rustdoc of the generated `contents` `send()` again names
  `<QueryPolicy as SendWith<Envelope<ContentResults>>>::Output`, the author's friction F3. The
  checked signature is visible only on the hand-written `fetch_pages`.
- As with `fetch_page`, the batch returns no `ResponseMeta` on success (consumer friction 6). A
  caller who needs headers must use `contents(..).send()`, whose `ContentResults` fields are
  private.

## Open provider risks (inferred, not verified; live NOT_RUN)

1. **Truncation warnings.** From my knowledge of MediaWiki's result handling, not from the cited
   pages: when a result reaches the wiki's size limit, MediaWiki may add a "result truncated"
   warning to the response that carries the continuation. `QueryPolicy` refuses every warnings
   envelope, so such a batch would fail closed with `Error::Warnings` instead of continuing. The
   test `whole_response_failures...` pins this. Accepting that one warning (the `result` module)
   would change the SDK's existing warnings policy, which is the owner's decision, so I did not
   make it. It interacts with consumer friction 8: `Warnings` does not expose the warning.
2. A page ID outside the wiki's ID range might be dropped silently instead of being listed as
   missing. The batch would then be `InvalidResponse` (never answered) rather than a per-ID
   `Missing`.
3. The `invalid` flag on a row that carries a `pageid` is documented only for titles. The
   fixture shape is synthetic.
4. `PageIdLimit::HighLimits` needs the `apihighlimits` right, but this client sends no
   credentials. On a wiki that does not grant it, more than 50 IDs probably produce a warning
   (refused) or an unanswered ID (`InvalidResponse`), never a silent truncation.

## Candidate Concord improvements (recorded only; shared code untouched)

1. **Declared-key collision for `query(..)` values.** Concord knows each endpoint's declared
   query keys. It could refuse a runtime pair that repeats one (`ErrorKind::Build`), or expose
   the keys (for example `Endpoint::declared_query_keys()`). Either would remove the SDK's
   hand-maintained reserved list. Evidence: experiment O3 and fact 2.
2. **A shared fixed query block for a subset of sibling endpoints**, without adding a navigation
   level: a literal-only group spliced like `..Name`, but not settable by callers. Evidence:
   fact 1. The benefit is small with two endpoints, so record it rather than build it.
3. **concord-pagination: a merge-until-answered driver** for property continuation over a fixed
   key set. It would take a step function, return a progress count, and enforce a "must
   progress" bound. Evidence: `src/batch.rs`. This is one SDK only, so it is a candidate to
   compare against other providers' batch APIs before deciding.
4. Already recorded by the author and still observed: F3 (associated-type `send()` signature in
   rustdoc).

Not warranted: a pipe-separated list placement. `PageIds` needs a newtype anyway for the limit
and deduplication, and the documented `Display` newtype (§6) fits.

Naming note for the owner: the scope now has `content` (single ID, plural wire name `pageids`)
and `contents` (batch). This is clear in rustdoc, but the names are only one letter apart.
Callers are steered to `fetch_page` / `fetch_pages`.
