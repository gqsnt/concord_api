# concord-example-shopify-app

The external caller of [`concord-example-shopify-sdk`](../shopify-sdk) (Shopify GraphQL Admin
API `2026-10`, a bounded slice): the two required workflows as library functions. It depends only
on the SDK's public exports, `concord`, `concord-pagination` and `tokio` (for the live binaries).

| Workflow | Function | Inputs | Outcome |
|---|---|---|---|
| SH-A catalog and inventory reconciliation | `catalog_inventory::reconcile(&ShopContext, &ReconcileRequest)` | location (or the only active one), product filter, page sizes, page budget per traversal, nested-traversal bound, partial-data policy | `ReconcileReport`: per-traversal outcome and resume cursor, nested traversals, counts, discrepancies, accepted GraphQL errors, cost ledger, status |
| SH-B bulk catalog export | `bulk_export::export_catalog(&ShopContext, &ExportRequest, &mut impl RecordSink, wait)` | submit (with `JobCreationOptIn`) or attach, poll bound, line limit, malformed-line policy, a caller's bounded sink, a caller's wait future | `ExportReport` and normalized `ExportRecord`s delivered one at a time to the sink |

## Commands

```sh
cargo test -p concord-example-shopify-app --test sh_a_catalog_inventory
cargo test -p concord-example-shopify-app --test sh_b_bulk_export
cargo test -p concord-example-shopify-app --test manifest     # fixture digests and provenance
cargo test --locked -p concord-example-shopify-app            # all of the above
```

The tests run each workflow end to end against `concord-example-testserver`: one loopback server
answers for two shops (`shop-one.myshopify.com`, `shop-two.myshopify.com`, mapped with
`reqwest::ClientBuilder::resolve`; each request must carry exactly its shop's synthetic token),
and SH-B adds a second server for the signed-URL storage host. Every test shuts its servers down.

## Live mode

The binaries run the same functions against a real shop; without credentials they print
`NOT_RUN` and exit 0.

| Binary | Needs | Does |
|---|---|---|
| `cargo run -p concord-example-shopify-app --bin sh_a` | `SHOPIFY_SHOP`, `SHOPIFY_ADMIN_TOKEN`; optional `SHOPIFY_LOCATION_ID` | read-only queries, 3 pages per traversal, 10 nested traversals |
| `cargo run -p concord-example-shopify-app --bin sh_b` | the same, plus **`SHOPIFY_DEV_SHOP_BULK_OPT_IN=create-job`** (a development shop whose owner agreed) to submit a bulk query, or `SHOPIFY_BULK_OPERATION_ID` to attach to an existing one (read-only) | poll every 2 s (60 polls at most), stream into a 1000-record bounded sink, print the report |

Live runs were **not** performed for this delivery (no credentials): both workflows are
`OFFLINE_VERIFIED` only; see the SDK's `PROVIDER.md`.

## Policies the workflows own

- **SH-A partial data:** `PartialData::Accept` keeps a page's data and records its errors
  (`accepted_errors`, with paths); `Reject` fails the traversal at that page, keeping the errors
  and resuming there. A page without its connection is always a failure.
- **SH-A failures:** the first failed traversal ends the run (later stages `NotRun`); nothing is
  retried; a `THROTTLED` failure keeps the bucket state and the resume cursor. A failed nested
  traversal stops its parent traversal mid-page: the parent summary is `Stopped`, resuming at the
  interrupted page (its later parents were not processed). The cursor names a page, not a
  position in it: resuming there processes that page's earlier parents again, so a caller
  discards or deduplicates them (at least once, not exactly once).
- **SH-A completeness:** each discrepancy kind runs only when its inputs are complete (see
  `Discrepancy`); `ReconcileStatus::Complete` needs every enabled traversal exhausted without
  accepted errors.
- **SH-B output:** records are normalized per line (a variant names its product), so nothing is
  grouped or collected; the workflow holds at most one record plus the reader's line buffer.
- **SH-B malformed lines:** stop, or skip up to a bound; only a bad line ended by a line feed is
  skipped (`ReadError::is_terminal` is false). A bad final line without a line feed, an over-long
  line, a transport failure or a byte-order mark is terminal: it always ends the stream and is
  reported as `Interrupted`, never as complete.

## Test map (acceptance → test)

| Contract item | Test |
|---|---|
| SH-A dependent end cursors, nested connections, typed report | `sh_a_reconciles_catalog_and_inventory_across_dependent_and_nested_cursors` |
| SH-A null fields per schema | same (`address.city`, `sku`, `inventoryQuantity`, `productsCount` null), `sh_a_http_200_errors_without_data_absent_fields_and_bad_page_info_are_failures` (absent selected field) |
| SH-A HTTP 200 containing errors | `sh_a_http_200_errors_without_data_absent_fields_and_bad_page_info_are_failures` |
| SH-A partial data with errors, policy | `sh_a_reconciles_…` (accept), `sh_a_rejected_partial_data_fails_at_that_page_with_its_errors` |
| SH-A throttling information | `sh_a_throttled_page_stops_the_run_with_bucket_state_and_resume_cursor` |
| SH-A nested failure mid-page (resume at the interrupted parent page) | `sh_a_nested_failure_mid_page_resumes_at_the_interrupted_parent_page` |
| SH-A variables requiring encoding, two shops | `sh_a_two_shops_reconcile_independently_and_encode_their_filters` |
| SH-A budgets | `sh_a_page_budget_bounds_every_traversal_with_a_resume_cursor` |
| SH-B submit, poll, completion, signed download on another host without the token | `sh_b_completed_export_streams_normalized_records_from_arbitrary_chunks` |
| SH-B arbitrarily split JSONL / UTF-8 chunks, `__parentId` | same (chunk sizes 1, 2, 3, 5, 7, 64, whole body) |
| SH-B empty successful completion, two shops | `sh_b_empty_completion_downloads_nothing_and_shops_stay_separate` |
| SH-B failed job with partial output | `sh_b_failed_job_streams_partial_output_and_says_it_is_partial` |
| SH-B expired signed URL | `sh_b_expired_signed_url_is_refused_before_its_body_is_read` |
| SH-B malformed records | `sh_b_malformed_lines_stop_the_export_or_are_skipped_within_a_bound`, `sh_b_a_skipped_error_right_before_the_end_is_never_reported_as_complete` |
| SH-B oversized record | `sh_b_an_oversized_line_ends_the_stream_with_a_limit_error` |
| SH-B consumer cancellation | `sh_b_consumer_cancellation_stops_reading_and_closes_the_download` |
| SH-B transport error after records | `sh_b_a_transport_error_after_records_is_reported_with_its_position` |
| SH-B job creation refused / poll bound / throttled poll | `sh_b_a_rejected_submission_creates_no_job_and_polls_nothing`, `sh_b_polling_is_bounded_and_a_throttled_poll_is_a_reported_failure` |
| SH-B streaming before the body ends, bounded line buffer | `sh_b_a_large_generated_export_is_streamed_with_a_bounded_line_buffer` |

Fixtures: `tests/fixtures/` (`MANIFEST.json`: provenance class, sources, sha256, wire assertions,
expected outcome and test command per file; `generate.py` and `manifest.py` regenerate them
deterministically). All data is synthetic; no fixture is a capture of a real shop.
