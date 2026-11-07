# Fresh consumer: SDK48 Elasticsearch

Date: 2026-11-07. Live provider calls: **NOT_RUN**. Every SDK request in this
exercise must target `examples/testserver` on loopback.

## Predictions

Written before application code and before the first SDK request, using only the
SDK README, generated public rustdoc and the two official reference pages below.

### 1. Bounded bulk indexing and inspection

- Construct `Elasticsearch::new(http, fixture_url, ApiKey::new(encoded_key)?)`.
  Follow the README's transport recipe: no retries, no redirects, finite timeout.
- Build a `BulkRequest::with_limits(BulkLimits::new(5, 16_384)?)` containing explicit
  article IDs: an index, two creates, another index, and a delete. Set
  `Refresh::WaitFor`. Adding a non-object document or exceeding the bounds returns
  `Error::Invalid` before sending and leaves the request unchanged.
- Call `es.bulk(request).await`. Expect `POST /_bulk?refresh=wait_for`,
  `Authorization: ApiKey <encoded_key>`, `Content-Type: application/x-ndjson`, and
  JSON `Accept`. Each action has `_index` and `_id`; index/create have a compact
  source line; delete has none; the final line ends in LF.
- A fixture HTTP 200 with applied, rejected (429), mapping-error (400), conflict
  (409), and missing-delete (404 `not_found`) items returns
  `Error::PartialBulk`, not `Ok`. `is_uncertain()` is false. Inspect *every* item
  in submission order, including position, action, index, id, status, result or
  cause, plus `took` and applied-item version/sequence metadata.
- `report.resubmit_rejected()` contains only the 429 operation, retaining its
  bytes, bounds and refresh. A second explicit bulk call may succeed. Neither the
  transport nor SDK should replay applied operations or retry the 400/409 items.
  `resubmit` must never offer applied operations to its selector.
- Whole-request non-2xx responses return `Error::Provider`, preserving status,
  response headers and error causes. 4xx is a known rejection; 5xx is uncertain.
  Malformed successful JSON returns `Error::Concord` (decode failure, uncertain);
  item count/action/id or `errors` contradictions return `Error::Protocol`
  (uncertain). None may cause automatic retry.
- A successful bulk report means all submitted operations have known applied
  outcomes; there is no pagination. Debug/Display hide API keys, documents and
  provider reasons. Reasons are deliberately readable through `reason()`.

### 2. PIT, incremental `search_after`, and close

- Build `PitSearch::new(vec![Sort::asc("published_at")], 2)?` with an object
  query, `KeepAlive::minutes(NonZeroU32::new(1).unwrap())`, and a mandatory finite
  page cap. No extra Concord imports should be needed for `.items().next()`.
- `es.open_point_in_time("articles-2026.11", keep_alive).await` sends
  `POST /articles-2026.11/_pit?keep_alive=1m`, authentication and JSON `Accept`,
  without a body, and returns `PointInTime`.
- `es.search_after(&pit, &search, cap).items()` fetches lazily. Each request is
  `GET /_search` with no index or query parameters, authentication, JSON `Accept`
  and `Content-Type: application/json`. The JSON body contains size 2, the same
  query/sort, `track_total_hits: false`, and `pit: {id, keep_alive: "1m"}`.
  The provider adds `_shard_doc` sort values; I predict the requested sort contains
  only my explicit `published_at` clause.
- The second request uses the preceding response's `pit_id` and the exact raw
  last-hit sort array as `search_after`, including its tiebreaker and large
  integer/decimal spelling. First-page `search_after` is absent. Consume hits
  individually, inspecting id/index/source/score/sort without unbounded buffering.
- A short nonempty page does not prove completion. An empty page returns
  `Ok(None)`; another `next()` should remain `None` without another request.
  A page cap is an error (`PagesError::Bounded`), not completed traversal.
- Explicit `es.close_point_in_time(pit).await` sends `DELETE /_pit` with JSON
  `{ "id": <latest pit_id> }`, JSON headers and authentication. It returns
  `ClosePitResponse` with optional `succeeded` and `num_freed` fields.
- For failure handling, use ordinary `es.scan(...)` on separate bounded fixture
  runs. Expect it to retain the previously collected prefix and attempt close on
  a provider failure, malformed page, partial-search response or page cap.
  `ScanError.error` holds the primary failure and `.close` its cleanup outcome.
  Close failure after completed search leaves complete hits in `Ok(Scan)` and
  appears in `scan.close`. Invalid input fails before any request.
- Missing `pit_id` or hit sort, nonadvancing cursor and contradictory pages are
  protocol failures. `timed_out`/failed shards return `PartialSearch` and stop.
  Provider and malformed-body errors follow the bulk status/body rules. Debug
  and Display hide PIT ids, sort values, sources, reasons and credentials;
  explicit accessors expose source/sort/id as documented. Dropping a PIT does
  not asynchronously close it.

## Public references read

Read on **2026-11-07**:

- [Bulk index or delete documents](https://www.elastic.co/docs/api/doc/elasticsearch/operation/operation-bulk):
  consulted the NDJSON framing, final newline, per-operation response ordering,
  errors flag, and success/failure examples.
- [Paginate search results](https://www.elastic.co/docs/reference/elasticsearch/rest-apis/paginate-search-results):
  consulted PIT creation, unchanged query/sort, last-hit cursor including the
  tiebreaker, latest PIT id, and explicit deletion.

## Observable preparation

1. Read `examples/elasticsearch-sdk/README.md` and root `Cargo.toml`; the package
   was already a workspace member. Initial `git status --short` was empty.
2. Ran `env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 cargo doc -p
   concord-sdk-elasticsearch --no-deps --offline` without `--locked` as required
   for the first build. It passed; Cargo.toml/Cargo.lock remained unchanged.
3. Read generated public rustdoc for the crate, `Elasticsearch`, bulk request,
   report/item/outcome/result/limits, error/provider/cause/document, PIT/search,
   hit/sort/keep-alive/key, scan results, and generated bulk/open endpoints.
   Did not follow rustdoc source links.
4. Read the SDK and testserver Cargo manifests to identify existing dependencies
   after reading the public SDK documentation. No dependency was added.

## First working call

The first application command was:

```sh
env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-elasticsearch --test first_use --locked --offline
```

It compiled immediately and both tests passed (2 passed, 0 failed). No compiler
correction or SDK implementation lookup was needed. The bulk test's first call
returned the intended `Error::PartialBulk` for HTTP 200; its second call returned
an all-applied report after resending only the 429 operation. The other test
opened a PIT, consumed three hits over two nonempty pages plus an empty terminal
page, and closed the newest PIT id. The original sort decimal spelling and
integer above 2^53 survived on the next request.

What it took: the README's client construction, public types and methods from
rustdoc, and public rustdoc for the loopback server and Concord pagination.
All dependencies were already declared. Only `std`, SDK exports, `serde_json`,
`tokio`, and `concord-example-testserver` were needed. No trait import, advanced
prepared-request API, generated endpoint name, or source assistance was needed.

Additional public rustdoc was generated using `--no-deps --locked --offline`
for `concord-example-testserver`, `concord-pagination`, `concord`, and
`concord-core`. All cargo commands used four jobs and the default worktree target.

## Friction incidents (chronological)

1. Documentation extraction tooling: Python `bs4` is unavailable. Used the
   standard-library HTML parser instead; no dependency was installed.
2. Documentation navigation: guessed `struct.ItemResult.html`; it is an enum.
   Corrected the public-page path. This was a navigation mistake, not an SDK
   diagnostic.
3. Documentation navigation: guessed `enum.ErrorDocument.html`; it is a struct.
   Corrected the public-page path, without looking at source.
4. Documentation extraction tooling: the testserver crate page included a large
   rustdoc tooltip script in the extracted text, truncating that tool output.
   Excluded script/style elements in later extraction. The needed API signatures
   were available in the public item pages.
5. Public pagination documentation has both `Items` (the older generic engine)
   and `PageItems` (returned by this SDK's `Pages::items`). Their `next()` return
   shapes differ. Following the `Pages::items` return-type link resolved this
   before writing code; the SDK README's `next().await?` example was correct.
6. Metadata inspection required additional Concord core rustdoc, which was not
   generated by the SDK's `--no-deps` invocation. Generated it separately and
   read `ResponseMeta`, `Error`, `ErrorKind` and `Delivery`. The documented
   distinction between `status()` (status errors only) and `head_meta()` (also
   decode failures) was sufficient to write the assertions.

No Rust compiler failures, misleading compiler diagnostics, unneeded imports,
unexpected generated endpoint names, completion ambiguity after reading the
docs, or implementation-source lookups occurred. All Rust test invocations
passed. No SDK runtime change is indicated by this exercise. A public-doc
improvement I would suggest for Concord is a prominent cross-link explaining
the `Items` / `PageItems` distinction and their different `next()` signatures;
shared files were left untouched.

## Prediction comparison

**No mismatches in the exercised predictions.** The rows follow the prediction
bullets above. Unexercised details are identified instead of inferred from a
passing suite.

| Prediction | Observed in the fresh caller tests |
|---|---|
| Bulk client/transport | README construction worked immediately; all observed requests were authenticated loopback requests with finite timeout and no retry/redirect policy. |
| Bulk bounds and preflight | Sixth operation and non-object document returned `Invalid`; length/byte counters stayed unchanged. The byte-limit rejection branch was not independently exercised. |
| Bulk wire format | `POST /_bulk?refresh=wait_for`, expected authorization/Accept/content type, nine action/source lines, final LF, and exact document/action values all matched. |
| Bulk partial report | HTTP 200 returned `PartialBulk`, `is_uncertain == false`, two applied and three failed items. Every position, action, index, id, status, result, cause type/reason was inspected. `took`, version, sequence number and primary term matched. Missing delete was applied. |
| Bulk selective resubmission | Selector saw only failed positions 1/2/3. Built-in retry sent only position 1 (429), with its original action/source bytes, refresh and bounds; resulting report was successful. Exactly two requests were observed. |
| Bulk status/body/protocol errors | 403 preserved typed provider error and headers, was a rejection and certain. HTML 503 preserved status/Retry-After despite unreadable error document and was uncertain. Malformed 200 was `Concord(Decode)` with answered delivery and retained response metadata. Wrong id and contradictory `errors` were uncertain `Protocol`. No response head was `Concord(Transport)` with unknown delivery. Exactly one request in every scenario. Count/action mismatches were not independently exercised. |
| Bulk completion/secrets | Successful report contained only the explicit resubmission. Diagnostics hid key/source/reason/cookie; deliberate cause and header access returned unredacted data. No bulk pagination. |
| PIT construction | Explicit sort/query/page size and nonzero cap worked without a trait import. Empty sort, page size zero and array query were refused. |
| PIT open | Bodyless `POST /articles-2026.11/_pit?keep_alive=1m`, authorization and JSON Accept matched. |
| PIT lazy page requests | No request on traversal construction; no read-ahead while a page still held hits. Every page was `GET /_search` without path index/query parameters, with JSON headers and invariant size/query/sort/track_total_hits. Only the explicit sort clause was sent, as predicted. |
| PIT cursor/id progression | First request had no cursor; next requests used the preceding page's PIT id and exact raw last-hit sort text, preserving trailing decimal zeroes and the integer above 2^53. Hit id/index/source/score/sort access worked. |
| PIT completion/cap | Two nonempty pages (second short) needed a third empty page. Repeated `next()` then returned `None` without a request. Progress reported three items and three successful pages. The cap scenario returned scan `Error::PageCap { pages: 1 }` with a prefix, not completion. Direct `PagesError::Bounded` was not separately asserted. |
| PIT close | Explicit DELETE used the empty page's newest PIT id and JSON headers/body; response had `succeeded == Some(true)` and `num_freed == Some(2)`. |
| PIT scan failure/cleanup | Provider 503, malformed page, partial search, missing-sort protocol error and page cap retained the prior one-hit prefix, stopped fetching and closed. Parsed partial/protocol pages updated the id used for close. Failed open had no hits/close attempt. Failed close preserved a completed scan and surfaced its 403 separately. |
| PIT protocol/secrets/lifecycle | Missing hit sort produced `Protocol`; timeout plus failed shard produced `PartialSearch` and emitted no failing-page hits. Diagnostic redaction checks passed. Missing PIT id, nonadvancing cursor and implicit-drop expiry were not independently exercised; they remain documentation-based predictions. |

## Final validation and scope

```sh
rustfmt --edition 2024 --check examples/elasticsearch-sdk/tests/first_use.rs
env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 cargo test -p concord-sdk-elasticsearch --locked --offline
git diff --check
```

All passed. Full package result: **31 integration tests passed, 0 failed**
(2 fresh caller tests, 19 existing protocol tests, 10 existing workflow tests;
0 unit tests and 0 doc tests). Existing tests were executed without reading
their source. The targeted caller tests also passed before and after adding
failure scenarios. Test output reported no ignored tests.

Only `FIRST_USE.md` and `tests/first_use.rs` are new working-tree files.
The workspace member and lock entry were already present; the first unlocked
offline documentation build left `Cargo.toml` and `Cargo.lock` unchanged.
No crate was added and no lockfile package version changed, so no new dependency
provenance entry was needed. `src/` and all pre-existing package files are
unchanged. `git diff --name-only c3fd4f1ee30c1727e26b285270f2cc8d8b694e77 --
crates checks scripts` was empty. No commit was attempted.

## Assistance label

**docs-only**. No SDK source, pre-existing SDK tests, `FRICTION.md` or
`PROVENANCE.md` has been read. No shared crate, check, script or other example
has been edited.
