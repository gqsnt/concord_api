# What developers can do now (4.4 extensions readout)

Every code block below is copied verbatim from a file in this tree that the maintained gate
compiles and tests (`scripts/verify.sh`); the file is named above each block, and
`readout_check.py` (same directory) verifies that each block still appears in its file.

## 1. Type-level CSV: `Csv<HasHeader, Delimiter>`

Header policy first, delimiter second, an optional advanced profile third; no new DSL grammar.
Direct generic paths, aliases, a provider profile, the media override, finite and streamed
bodies, all in one declaration (`examples/extension/tests/dsl.rs`):

```rust
/// An ordinary Rust alias of a type-level dialect.
pub type UnitSeparated = Csv<NoHeader, Delimiter<0x1f>>;
/// The provider profile of this crate.
pub type CatalogCsv = concord_example_extension::Csv;
/// A one-shot row source with a nameable type, for a declared streaming body.
pub type RowSource = StreamRows<FromIter<std::vec::IntoIter<Result<Row, std::io::Error>>>>;

concord::api! {
    /// A file service whose formats are type-level CSV dialects.
    pub client Files() {
        base "https://files.example.test/v1";

        /// A direct generic format path.
        GET semicolon() -> Vec<Row> { path "semicolon"; response Csv<HasHeader, Semicolon>; }
        /// An alias with a control-byte delimiter.
        GET unit() -> Vec<Row> { path "unit"; response UnitSeparated; }
        /// A provider profile.
        GET catalog() -> Vec<concord_example_extension::CsvRow> { path "catalog"; response CatalogCsv; }
```

A provider dialect is a profile: ordinary Rust returning options, which cannot contradict the
header/delimiter chosen by the type (`examples/extension/src/csv.rs`, the migrated extension
example):

```rust
impl CsvProfile for CatalogExport {
    fn options() -> CsvProfileOptions {
        CsvProfileOptions::default()
            .comment(Some(b'#'))
            .columns(["id", "name"])
    }
}
```

```rust
pub type Csv = concord::csv::Csv<HasHeader, Comma, CatalogExport>;
```

When a job chooses the dialect at run time, the explicit runtime bridge builds validated
readers/writers from `CsvOptions` (written by an independent caller from the public docs,
`checks/dx03-csv-runtime/src/lib.rs`):

```rust
        Ok(CsvOptions::new(mode, bytes[0]))
```

Finite `Decode<Vec<T>>`/`Encode<Vec<T>>`/`Encode<Rows<I>>`, raw records (`Decode<RawTable>`,
`open_raw`), the incremental reader (`CsvReader::open::<T>(response)` → `CsvRecords` with
`async fn next()`) and the lazy writer (`Encode<StreamRows<S>>`, `CsvWriter::upload` → `Upload`
with its matching `Content-Type`) share one framing/mapping implementation
(`crates/concord-extras/src/csv/`).

## 2. Streamed reading and writing

A streamed read opens a native response — status and head checks happen before the body is
polled, and a refusal keeps the response unread — then delivers owned records one at a time. The
Shopify bulk export (`examples/shopify-app/src/bulk_export.rs`):

```rust
    let mut records = match reader.open_for::<BulkRecord>(response, BULK_RESULT_OPERATION) {
        Ok(records) => records,
        Err(error) => return ExportOutcome::DownloadRefused { error },
    };
```

and how it distinguishes a skipped bad line from an error that ended the read:

```rust
            Some(Err(error)) => {
                if !error.is_terminal() && report.skipped_lines < max_skips {
                    report.skipped_lines += 1;
                    continue;
                }
                break ExportOutcome::Interrupted { error };
            }
```

Kubernetes watches use the consecutive-JSON-values reader (no newline required; a number split
across chunks stays one token) (`examples/kubernetes-app/src/list_watch.rs`):

```rust
    let reader = JsonValuesReader::new().max_value_bytes(policy.max_event_bytes);
```

A streamed upload is an ordinary declared body whose one-shot source is polled only when the
transport sends it (`checks/stream-stress/src/lib.rs`):

```rust
        /// A JSON Lines upload of a one-shot generated source.
        POST upload_jsonl(body records: StreamRecords<Generated>) { path "upload"; request JsonLines; }

        /// A CSV upload (`Csv` = `HasHeader`, comma; header inferred from the rows' field names).
        POST upload_csv(body rows: StreamRows<Generated>) { path "upload"; request Csv; }
```

Measured (STRESS.md): 256 MiB and 512 MiB reads keep the reader's own buffer at 127 B (JSON
Lines) / 384 B (CSV) and the whole process at 11–12 MB; in the full-body reads the first record
arrives in under a millisecond; uploads poll their source zero times before sending.

## 3. Cursor traversal

The page factory is an ordinary closure; the traversal owns budgets, checkpoints and the report,
never guesses a cursor and never retries (`examples/stripe-app/src/reconciliation.rs`):

```rust
    let mut pages = sequential(from, budget, |cursor| list.fetch(cursor));
```

## 4. Discovered parallel results

Salesforce Bulk API 2.0: a sequential manifest chain discovers result parts; each job owns its
response, incremental CSV parser and staged sink, and holds its window slot until the sink is
done (`examples/salesforce-app/src/sf_b.rs`):

```rust
        let mut jobs = manifest_jobs(
            from,
            job_options,
            |key| manifests.fetch(key),
            |part| runner.run(part),
        );
        while let Some(outcome) = jobs.next().await {
            match outcome {
                Ok(done) => report.completed.push(done.value),
                Err(JobsError::Job(failed)) => report.failed.push(FailedPart {
                    part: failed.key,
                    error: failed.value.error,
                    rows_discarded: failed.value.rows_discarded,
                }),
                Err(JobsError::Discovery(error)) => report.discovery.error = Some(error),
            }
        }
```

## 5. Nested extraction and item-level resume

GitHub's activity audit walks the parent listing item by item and keeps the checkpoint *before*
each item, so a child failure resumes without losing or duplicating the parent item
(`examples/github-app/src/audit.rs`):

```rust
    let mut items = resume_items(start, budget, |key| {
        let page = pages.fetch(key);
        let log = parent_log.clone();
        async move { record_page(operation::ISSUES, page.await, &log) }
    });
```

```rust
        // The checkpoint *before* this item: a child failure restarts here, re-emitting it.
        let before = items.checkpoint().cloned();
```

## 6. Partial failure and resume

`finish()` returns a report that keeps the failed identity apart from the restart identity:
completed-but-not-yet-emitted results (`retained`), failed parts, cancelled and queued work and the
undiscovered remainder (`discovery.resume`) — the Salesforce export above merges them into its
own report, and `continue_export` resumes only the unfinished parts (tests:
`examples/salesforce-app/tests/sf_b_bulk_csv_round_trip.rs`). An independent caller reproduced the
whole change from sequential collection to bounded fanout with resume from the public API
(`checks/dx03-fanout-sink/`).

## Remaining awkward interfaces (see NEXT_REVIEW_4_4.md)

Keeping response metadata next to a typed value still needs `into_parts` → execute → decode in
every SDK, and converting a known transport failure goes through an async call typed as possibly
successful; generic code over generated operations needs a hand-written trait per SDK; shared
parameter sets and per-request header overrides are redeclared per endpoint; restarting a
discovered traversal composes two traversal types and two report shapes by hand.
