# concord-example-salesforce-app

The external caller of the Salesforce SDK slice (`examples/salesforce-sdk`): the two required
workflows as library functions over a configured `OrgContext`, using only the SDK's public
exports, `concord`, `concord-pagination` and ordinary crates. Provider version, sources and
decisions: [`../salesforce-sdk/PROVIDER.md`](../salesforce-sdk/PROVIDER.md); friction:
[`../salesforce-sdk/FRICTION.md`](../salesforce-sdk/FRICTION.md).

| Workflow | Public functions | Outcome types |
|---|---|---|
| SF-A schema-aware CRM extraction | `sf_a::crm_extraction(&OrgContext, &ExtractionInput)` | `ExtractionReport` (`SchemaProvenance`, `Section<Account>`, `Section<Contact>`, record reads), `Completion::{Complete, Bounded, Interrupted, RestartRequired}`, `QueryResume`, `ExtractionError` |
| SF-B streamed Bulk CSV export | `sf_b::bulk_csv_export(&OrgContext, &ExportInput, wait)`, `sf_b::continue_export(.., ExportRestart, ..)`, `sf_b::PartRunner` (one part job, for custom composition) | `ExportReport` (`completed`, `retained`, `failed`, `cancelled`, `queued`, `DiscoveryState`, `CombinedOutput`), `ExportStatus`, `ExportRestart`, `FailedPart`, `ExportError` |
| SF-B ingest sub-scenario | `ingest::ingest_round_trip(&OrgContext, &IngestInput, rows, wait)` | `IngestReport` (`UploadReceipt`, result rows), `IngestError` |

`wait` is the caller's timer between job polls (`|poll| tokio::time::sleep(..)`); the library
starts no runtime and spawns nothing.

## Commands

```sh
cargo test -p concord-example-salesforce-app --test sf_a_crm_extraction
cargo test -p concord-example-salesforce-app --test sf_b_bulk_csv_round_trip
cargo test -p concord-example-salesforce-app --test sf_b_ingest_round_trip
cargo test -p concord-example-salesforce-app --test generic_streaming_upload_transport   # labelled transport fixture, not Salesforce evidence
cargo test -p concord-example-salesforce-app --test manifest                             # fixture hashes and provenance
cargo test -p concord-example-salesforce-sdk                                             # SDK unit, single-operation wire and result-part tests
```

Live runs (the same functions, another transport target):

```sh
SF_INSTANCE_URL=https://<my-domain>.my.salesforce.com SF_ACCESS_TOKEN=... \
  cargo run -p concord-example-salesforce-app --bin sf_a            # read-only
SF_INSTANCE_URL=... SF_ACCESS_TOKEN=... SF_SANDBOX_JOBS=1 SF_SANDBOX_MUTATIONS=1 \
  cargo run -p concord-example-salesforce-app --bin sf_b            # sandbox only: creates a query job, inserts a Contact
```

Without the variables each binary prints `NOT_RUN` and exits 0. `SF_SANDBOX_JOBS=1` authorizes
Bulk query job creation; `SF_SANDBOX_MUTATIONS=1` authorizes the ingest writes. Neither was run
for this delivery (no org was available): live status `NOT_RUN`.

## Acceptance evidence (contract SF-A / SF-B) → tests

| Requirement | Test(s) |
|---|---|
| SF-A selected describe, record reads, SOQL and query-more through the pagination crate | `sf_a_multi_page_extraction_with_schema_custom_fields_and_record_read` |
| SF-A query construction separate from user-data encoding | `sf_a_user_data_is_a_literal_and_encoded_by_the_transport`, SDK `soql::tests` |
| SF-A multiple pages, records moved from `records`, metadata kept (`totalSize`, `Sforce-Limit-Info`) | `sf_a_multi_page_…` |
| SF-A optional/custom selected fields (value, null, not selected) | `sf_a_multi_page_…`, `sf_a_two_orgs_stay_isolated_and_missing_custom_fields_are_reported` |
| SF-A expiry after earlier success → explicit restart | `sf_a_expired_locator_after_earlier_pages_requires_an_explicit_restart` |
| SF-A failure after earlier success → resume | `sf_a_transient_failure_resumes_from_the_failed_locator_without_duplicates`, `sf_a_resume_points_are_bound_to_their_org_and_statement` |
| SF-A returned URLs validated (origin, version, scope); `totalSize` never makes a locator | `sf_a_returned_links_are_validated_and_counts_never_make_locators`, SDK `context::tests` |
| Two contexts (scope/header leak detection) | `sf_a_two_orgs_…`, `sf_b_two_orgs_export_concurrently_without_scope_or_header_leaks` |
| Redaction (tokens, locators, literals, record values) | `sf_a_diagnostics_never_show_tokens_locators_literals_or_record_values`, SDK `wire::rest_operations_on_the_wire`, `ids::tests` |
| SF-B job creation/status with explicit dialect; version and dialect preserved | `sf_b_discovered_parallel_export_semicolon_crlf`, `sf_b_job_version_and_dialect_are_preserved_and_links_scoped` |
| SF-B discovered batches of result URLs, bounded fanout of CSV bodies | `sf_b_discovered_parallel_export_semicolon_crlf`, `sf_b_permits_are_held_through_stalled_bodies` |
| SF-B failure in a discovery batch | `sf_b_discovery_batch_failure_then_restart_completes` (Continue and Stop) |
| SF-B failure in an active download; partial shard never published | `sf_b_failure_in_an_active_download_discards_its_staged_shard` |
| SF-B completed-but-unemitted shards retained and reused | `sf_b_completed_but_unemitted_shards_are_retained_and_reused` |
| SF-B ordered combination, restart information | `sf_b_discovered_…`, `sf_b_discovery_batch_failure_…`, `sf_b_completed_but_unemitted_…` |
| SF-B sequential locator results (`Sforce-Locator`, `maxRecords`) | `sf_b_sequential_locator_chain_follows_sforce_locator_headers` |
| SF-B non-comma delimiter, LF and CRLF, field-name headers, quoted separators/newlines, escaped quotes | `sf_b_every_bulk_dialect_round_trips_with_quoted_separators_and_newlines`, `sf_b_discovered_…` |
| SF-B empty/null profile behaviour | SDK `csv::tests::ingest_body_uses_the_job_dialect_and_null_marker`, `ingest_spooled_upload_round_trip`, `assert_rows_equal` (empty → `None`) |
| SF-B row-width/schema errors; each part parsed with its own header | `sf_b_width_schema_decode_and_count_errors_fail_only_their_part` |
| SF-B one validated part reader for both routes: a shard is published only after `ResultPartRows::finish` (end of input, declared count equal; early stop, limit or read error is never a completion) | SDK `result_parts::*`, `sf_b_width_schema_decode_and_count_errors_fail_only_their_part`, `sf_b_large_generated_parts_stream_through_bounded_readers` |
| SF-B `nextRecordUrl` vs `nextRecordsUrl` (aliases, ambiguity) | `sf_b_next_manifest_link_alias_policy_on_the_wire`, `sf_b_official_parallel_results_example_agrees_with_the_link_policy`, SDK `bulk::tests::next_link_alias_policy` |
| SF-B Salesforce adapter enforces provider header/dialect rules (CSV item 16) | SDK `csv::tests::headers_follow_salesforce_rules`, `results_are_read_with_the_job_dialect_and_exact_header` |
| SF-B ingest: public encoder, create → upload → close → poll → result files | `ingest_spooled_upload_round_trip` |
| SF-B upload framing: known length (spool) for Salesforce; generic chunked encoder on a labelled fixture | `ingest_spooled_upload_round_trip`, `ingest_encoding_failures_create_no_job`, `generic_one_shot_stream_is_polled_only_by_the_transport_and_sent_chunked`, `generic_stream_source_failure_fails_the_transfer_after_it_started` |
| Scale (generated data above the reader/queue budgets) | `sf_b_large_generated_parts_stream_through_bounded_readers` (prints bytes, rows, latency, peak retained bytes) |
| Fixture provenance and hashes | `manifest` |

## Replay and duplicate implications

A part that fails after rows were parsed discards its staged shard; `FailedPart::rows_discarded`
counts those rows. A re-run re-requests the part's own locator, so published output has no
duplicates and no gaps as long as the job (and so the locator) exists. SF-A's resumed section
re-fetches the failed batch; an expired locator requires a full restart, and a caller keeping both
runs must deduplicate by `Id`. Cancellation does not undo remote requests; it removes only this
crate's staged files.

## Fixtures

`tests/fixtures/` with [`MANIFEST.json`](tests/fixtures/MANIFEST.json): provenance class
(`official_example`, `schema_derived_synthetic`, `fault_injection`), SHA-256, documentation URL and
retrieval date, wire assertions, expected outcome and test command per file; data generated in the
tests is listed under `generated_in_tests`. Result-part CSV is produced by an independent
Salesforce-style writer in `tests/support` (every field quoted), never by the client's encoder. The
fixture directory disables Git line-ending normalization so CRLF bodies stay byte-exact.
