# Stage 1

Completed only Stage 1. This is an independently written caller using public APIs.
I did not read `v4/`, any `FRICTION.md`, `DECISIONS.md`, `NEXT_REVIEW*`, or
`examples/salesforce-app/src/`. Links to those files in permitted documentation were
not followed. No Stage 2 implementation or investigation was started.

## Result and caller contract

`stage1::collect_accounts(&OrgContext, QueryJobId)` returns
`Result<AccountExport, ExportError>`. `AccountExport` contains `Vec<AccountRow>` and
`result_requests: u64`. The job must select `Id, Name, NumberOfEmployees`, in that
order. The row uses `RecordId`, `String`, and `Option<u64>` respectively.

An initial GET of job information obtains the existing job's CSV delimiter and line
ending. That request is excluded from `result_requests`. The caller checks the job
id, context API version, and `JobComplete` state before attaching the job. Missing
dialect metadata is an explicit error, not a guessed default.

The SDK's `ResultSets<AccountRow>` supplies pages to `concord_pagination::sequential`.
Each result body is fully parsed before the next request starts. Rows are appended
in provider order and returned only after `Outcome::Exhausted`. An error discards
partial collected rows and preserves the SDK error inside `ExportError::Salesforce`.
No polling, job creation, ingest, parallel result discovery, retries, or sinks are
part of this stage.

## Documents read and questions answered

| Public document/API | Question | Did it answer? |
| --- | --- | --- |
| `docs/CALLER_GUIDE.md`, especially navigation, sending and errors | How do I navigate a generated SDK handle, send a typed request, and retain a non-2xx body? | Yes. Required arguments, `send()`, and Concord's error stages were explicit. |
| `examples/salesforce-sdk/PROVIDER.md` | What does the provider's sequential results protocol require? Which dialect and header rules should fixtures exercise? | Yes: `Sforce-Locator`, literal `"null"`, opaque locators, job delimiter/line ending, column order, empty-field semantics and the documented reading limitations. The SDK directory had no README, so its crate rustdoc was the entry point. |
| SDK crate rustdoc (`src/lib.rs`) | Which public modules provide jobs, rows, contexts and errors? | Yes. It led directly to `bulk_query`, `csv`, `ids` and `OrgContext`. |
| SDK `bulk_query` module docs and generated rustdoc for `BulkQueryJob` and `ResultSets` | Can I consume an existing job sequentially without implementing an HTTP or CSV adapter? | Mostly. `ResultSets::new` and `fetch` give exactly the page factory needed. `BulkQueryJob::attach` requires a known dialect, which is not supplied by this task's inputs. There was no attach-by-id-and-discover-dialect method in its public API. |
| Generated SDK rustdoc for `SalesforceApiBulkQueryJobInfo`, `QueryJobInfo`, `BulkDialect`, `ColumnDelimiter`, `LineEnding`, `JobState` | How can I discover that dialect using only the job id? | Yes, by using the ordinary generated info endpoint. The dialect fields are optional, so the application still has to choose what to do when they are absent. |
| SDK `csv` module source docs and `SalesforceCsv` public signatures | Should the caller hand-code quote handling, null conversion or a custom profile? | No adapter needed: the SDK already supplies the job dialect and exact selected-column validation. It documents doubled quotes, UTF-8, preserved whitespace, BOM handling and empty optional fields. |
| `crates/concord-extras/src/csv/mod.rs` source docs | What are generic CSV row mapping, empty-input, chunking and memory semantics? | Yes. Serde names map typed fields, header-only means zero rows, readers accept arbitrary byte chunks, and reader bounds do not bound the application's collected `Vec`. |
| `crates/concord-pagination/README.md`, crate source docs, and generated rustdoc for `Budget`, `Sequential`, `SequentialReport`, `Progress` | How do I traverse and know it completed, and where is a request counter? | Yes. The example provides the loop; `finish().outcome` distinguishes exhaustion from bounds; `progress.admitted` counts factory invocations. No `StreamExt` dependency is needed. |
| SDK rustdoc for `SfError`, `ApiFailure`, `ProtocolError`, `ProtocolErrorKind`; stream error source docs | Can errors be asserted by type instead of inspecting strings? | Yes. `SfError::from_concord` reads a status body's provider error; CSV decoding and protocol count errors remain separate variants. `ReadError::kind()` identifies `Decode`. |
| SDK rustdoc for `OrgContext`, `InstanceUrl`, `QueryJobId`, `RecordId`, `MaxRecords` | How do I construct valid inputs and a loopback context? | Yes. The instance parser explicitly permits loopback HTTP, ids validate their lengths, and `MaxRecords` requires a positive value. |
| `examples/testserver` crate docs, public API and generated `Body` rustdoc | Can fixtures inspect actual requests and split a response inside records? Can server tasks be shut down? | Yes. `Script`, `Recorded`, `Body::Generate` and `shutdown()` were sufficient. No existing Salesforce workflow/tests were copied. |

I generated the public API documentation offline using:

```sh
CARGO_TARGET_DIR=checks/dx03-fanout-sink/target cargo doc --offline \
  -p concord-example-salesforce-sdk -p concord-pagination \
  -p concord-example-testserver --no-deps
```

For source documentation, I read module-level docs and extracted doc comments/public
declarations; I did not inspect pagination or CSV implementation bodies. Generated
rustdoc supplied the complete public signatures and types.

## Compiler errors, verbatim

None. The first compilation of the consumer and seven integration tests succeeded;
all seven tests passed. The final test run and clippy also succeeded without compiler
warnings or errors. Consequently there is no compiler diagnostic whose causal
accuracy can be assessed; I did not manufacture a failing compile to create evidence.

There were two Cargo **CLI** failures while looking for a way to keep lockfile writes
inside the permitted package. These were tooling friction, not Concord errors:

```text
$ RUSTC_BOOTSTRAP=1 cargo -Z unstable-options --lockfile-path checks/dx03-fanout-sink/target/Cargo.lock locate-project --workspace
error: unexpected argument '--lockfile-path' found

  tip: a similar argument exists: '--locked'

Usage: cargo [+toolchain] [OPTIONS] [COMMAND]
       cargo [+toolchain] [OPTIONS] -Zscript <MANIFEST_RS> [ARGS]...

For more information, try '--help'.
```

```text
$ RUSTC_BOOTSTRAP=1 cargo test -Z unstable-options --lockfile-path checks/dx03-fanout-sink/target/Cargo.lock --help
error: unexpected argument '--lockfile-path' found

  tip: a similar argument exists: '--locked'
  tip: to pass '--lockfile-path' as a value, use '-- --lockfile-path'

Usage: cargo test -Z <FLAG> --locked [TESTNAME] [-- [ARGS]...]

For more information, try '--help'.
```

Both accurately identified an unsupported flag. Neither suggested a solution to the
actual constraint: `--locked` would prohibit the necessary dependency update, and
passing the flag to the test executable would not relocate Cargo's lockfile.

## Annotations, adapters and workarounds

- `ResultSets::<AccountRow>` explicitly selects the page element type. This was a
  deliberate annotation, not a correction prompted by a compiler error. The closure
  `|key| sets.fetch(key)` bridges the SDK method to the paginator's factory argument;
  no boxing, pinning, manual futures, async-trait machinery or extra runtime is needed.
- `#[serde(rename = "...")]` maps Rust field names to the provider header names.
  `ACCOUNT_COLUMNS` separately declares the exact wire projection. `FieldPath::parse`
  validates those static names, and the `expect`s cover only known-valid constants.
- `MaxRecords::new(10_000)` supplies a positive per-result row request/reader limit.
  This is not a total export-memory limit. Every row is deliberately collected.
- `Budget` has no unlimited constructor in its public API. I used
  `Budget::new(NonZeroUsize::MAX)` because the requested consumer collects all sets,
  without an arbitrary application page cap. A non-exhausted report is still an
  error; it cannot silently turn a bounded traversal into a complete export. This
  consumer assumes the provider eventually terminates its chain; it adds no cycle
  detection or practical request cap.
- The count is the number of admitted `ResultSets::fetch` calls. Each fetch makes
  one results request, with no caller retry or subrequest. The wire tests verify it
  against recorded requests. It is not instrumentation of any redirect/retry policy
  a caller might independently configure on its HTTP client.
- A small `ExportError` wraps the original `SfError`, adds missing-dialect and
  incomplete-traversal cases, and exposes the SDK error through `Error::source`.
  The initial generated endpoint needs an explicit async `SfError::from_concord`
  conversion; later result fetches already return `SfError`.
- Test clients disable proxies for the loopback address and use a five-second
  timeout. Every successful test shuts down and joins the test server.
- Cargo normally updates the root workspace lockfile when adding this package's
  dependencies. To obey the directory restriction, validation used
  `target/validation/` inside this package: a copy of the root workspace manifest
  with only this package as an explicit member, a copied lockfile, and directory
  symlinks to the original `checks`, `crates` and `examples`. The original source
  files are compiled through those links; none are copied or edited. Inherited
  workspace dependency versions, package metadata and lints remain the same.
  Cargo writes its updated lockfile and build outputs only inside this package.
  No unstable flag or bootstrap override was used for the actual validation.

## What was confusing

The largest integration gap was the dialect bootstrap. The job wrapper has the
right invariant (a job owns its dialect), but attaching requires information the
caller does not yet have. I had to drop to the generated info endpoint, handle its
different error type, check id/version/state, extract two optional fields, then
attach. Public documentation made that route possible; it did not offer a single
existing-job entry point for this input shape.

`next().await == None` does not itself mean complete. The paginator docs clearly say
so, but this is an extra obligation in a function promising *all* rows. Likewise,
the useful request counter is named `admitted` and counts factory calls, so its
relationship to HTTP requests has to be established at the SDK boundary.

The type field mapping and the expected header are two representations of the same
schema. They must agree. The SDK requires the selected fields in exact query order,
even though generic named CSV deserialization can accept reordered headers.

“Chunk” can mean a provider result set or an HTTP body fragment. I tested both:
two provider result sets, with the first delivered in seven-byte fragments that
split UTF-8, quoting and line endings. A shared EOF flag proves the next request
was not made as soon as the first set's headers revealed its locator.

The provider adapter documents that read-time line endings are permissive and query
empty strings/nulls are not distinguished. The fixtures assert preservation of
embedded CRLF and conversion of both quoted/unquoted empty numeric fields to
`None`; they do not claim stricter provider validation than the SDK documents.

## What was easy

After obtaining a `BulkQueryJob`, the traversal is a short loop. The SDK adapter
owns cursor parsing, CSV configuration, exact columns, and count validation. The
consumer needed no custom CSV decoder, handwritten URLs, direct reqwest download,
stream combinator crate, or custom pagination adapter.

Typed failures worked immediately: a later HTTP 500 exposes `SfError::Api` with
status and provider code, a non-numeric field exposes `SfError::Read` with
`ReadErrorKind::Decode`, and a lying record count exposes
`ProtocolErrorKind::RecordCountMismatch { declared: 2, parsed: 1 }`.
The failing CSV response advertises another locator; the wire assertion confirms
that it is never fetched. Header-only result sets work both before a continuation
and at final exhaustion.

## Final verification

The requested commands ran from the package-local validation workspace described
above, with `CARGO_TARGET_DIR` set to this package's absolute `target/` path:

```sh
cargo test --offline -p concord-check-dx03-fanout-sink
cargo clippy --offline -p concord-check-dx03-fanout-sink --all-targets -- -D warnings
```

Final test output (build-path lines omitted):

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.52s

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 7 tests
test later_http_failure_preserves_the_typed_salesforce_error ... ok
test later_count_mismatch_is_a_typed_protocol_failure ... ok
test later_csv_decode_failure_is_typed_and_stops_before_its_next_locator ... ok
test header_only_last_set_is_a_complete_empty_export ... ok
test empty_set_with_a_locator_does_not_end_the_chain ... ok
test invalid_job_metadata_is_refused_before_requesting_results ... ok
test collects_two_sets_in_order_using_the_jobs_pipe_crlf_dialect ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

   Doc-tests concord_check_dx03_fanout_sink

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Final clippy output (dependency checking lines omitted; exit status 0):

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.80s
```

Only files in `checks/dx03-fanout-sink/` were changed. Stage 2 remains unstarted.

# Stage 2

Completed the requirement change while keeping `src/stage1.rs` and
`tests/stage1.rs` byte-for-byte unchanged. Validation ran in the real workspace;
the Stage 1 validation mirror was not used. No new dependencies were needed.
I continued to avoid all author-conclusion files and the reference application.

## Result and usage

`stage2::export(ctx, job_id, sink, limits)` reads the existing job's dialect, then
uses the SDK's `ResultManifests::fetch` with `concord_pagination::manifest_jobs`.
The returned `ExportReport` owns the job, sink, completed receipts, known unfinished
descriptors, and any unfinished discovery position. Download/discovery failures
return this report with typed diagnostics; setup failures return `ExportError`.

The caller supplies a `PartSink`. Each part gets a separate `StagedPart` transaction.
The downloader calls `push(&AccountRow)` as rows arrive and calls `publish()` only
after CSV EOF and record-count validation. The slot covers request acquisition,
parsing, row writes and publication. No `Vec<AccountRow>` is accumulated here.

`FileSink` creates a fresh export directory and writes buffered JSON lines to a
`.staging` file per part. After flushing and syncing the file it creates the final
`.jsonl` name with a same-directory hard link. That operation publishes the completed
file atomically and refuses overwrite. A drop guard closes and removes the staging
file on success, read failure, sink failure or cancellation. Unlink is best effort;
a leftover `.staging` file is never considered a completed part.

The scheduler emits completion-order receipts so a slow earlier part does not
retain later successful receipts in the execution window. Stable SDK ordinals go
into a `BTreeMap`; `completed()` iterates them in provider order. After checking
`is_complete()`, `FileSink::copy_completed(report.completed(), writer)` streams the
shards in that order using `io::copy`. Only tests collect the output for assertions.

Calling `report.resume(limits).await` consumes the old report and returns a new one
bound to the same job and sink. It runs known unfinished descriptors through
`independent`, then continues any remaining manifest discovery. Both phases share
one result-request budget and run sequentially, so their concurrency windows do
not add together. Completed parts are reused. A completed report resumes without
network calls. Counters and diagnostics describe the latest pass; completed receipts
cover all passes.

The report separates three questions: which parts published (`completed()`), which
known parts must be redone (`redo()`), and whether more parts remain undiscovered
(`pending_discovery()`). `failures` preserves each failing descriptor and `PartError`;
the latter distinguishes `SfError` from sink `io::Error`.

## Diff statistics

These compare the two implementations, not edits to Stage 1. Counts include docs,
comments and formatting. Both original Stage 1 files remain unchanged.

| Comparison | Stage 1 lines | Stage 2 lines | Insertions | Deletions |
| --- | ---: | ---: | ---: | ---: |
| `src/stage1.rs` → `src/stage2.rs` | 154 | 486 | 422 | 90 |
| `tests/stage1.rs` → `tests/stage2.rs` | 333 | 639 | 556 | 250 |

Measured with `wc -l` and `git diff --no-index --stat`:

```text
 .../dx03-fanout-sink/src/{stage1.rs => stage2.rs}  | 512 +++++++++++++++++----
 1 file changed, 422 insertions(+), 90 deletions(-)

 .../tests/{stage1.rs => stage2.rs}                 | 806 ++++++++++++++-------
 1 file changed, 556 insertions(+), 250 deletions(-)
```

The actual source changes are two added files plus one `pub mod stage2;` line in
`src/lib.rs`; this section is appended to the existing notes. The increase includes
the sink contract, file transaction, receipt ledger, two resume phases, typed sink
errors, and fixtures that observe behavior while downloads are still running.

## Concepts and documentation lookups

I generated public rustdoc offline, using package-local `CARGO_TARGET_DIR`, for the
SDK, pagination, testserver, Concord and concord-extras. I read public signatures,
rustdoc text and source doc comments, not pagination/CSV implementation bodies.
No reference application implementation or existing SDK test was read.

| Lookup | Question and result |
| --- | --- |
| `manifest_jobs`, `JobOptions`, `Jobs` rustdoc | Does the window cover headers only or a whole streamed job? The docs explicitly require the whole request/parse/sink future through EOF. They also explain the separate discovery slot and descriptor backlog. This directly answered the stalled-body requirement. |
| `Options`, `Order`, `OnError` rustdoc | Can independent failures continue, and can successful receipts be emitted immediately? Yes: `OnError::Continue` plus `Order::Completion`. I chose these because staging isolates part failures and final output order can be reconstructed from ordinals. |
| `JobsReport`, `DiscoveryReport`, `JobsCheckpoint`, `Checkpoint`, `Unfinished` rustdoc | What must survive failure or a configured bound? Attempted unfinished jobs, retained successes, queued descriptors and the undiscovered remainder are distinct. A single restart cursor cannot describe all of them. |
| `JobsReport::into_rerun` | How do I avoid downloading already completed retained jobs? It returns retained receipts separately from the unfinished/queued descriptor list. Its docs explicitly direct callers to `independent` and continued `manifest_jobs`. |
| `independent`, `IndependentReport::into_rerun`, `Keyed`, `JobsError` | How do known retries differ from discovery, and what identifies a result? The retry plan is ordinary independent work. `Keyed.index` restarts within each traversal, so the persistent identity must be the SDK's `ResultPart.ordinal`, not that index. Errors retain their typed payloads. |
| SDK `bulk_query` source docs and `ResultManifests`, `ManifestCursor`, `ResultPart` rustdoc | Are parallel results truly independently addressable? Yes, discovery returns validated descriptors. `ManifestCursor.next_ordinal` preserves numbering when discovery resumes. No manual URL validation or cursor arithmetic was needed. |
| SDK `bulk` source docs and `ResultPagesResponse` rustdoc, together with the Stage 1 reading of `PROVIDER.md` | What JSON should fixtures send? `resultPages` contains `resultUrl` entries; `done` and the accepted next-link spellings describe the manifest chain. The fixtures use `nextRecordUrl` and a lowercase returned `resultpages` path. |
| `BulkQueryJob::open_result_set`, `ResultSet`, `ResultLink` | How do I open one independent part without collecting it? The response is left unread; declared count and link `max_records` remain available. Parallel result downloads do not require the sequential `Sforce-Locator` header. |
| `SalesforceCsv::result_reader`, `CsvReader`, `CsvRecords`, `CsvOptions` | Can I retain the SDK dialect and exact header while streaming? Yes. `open_for::<AccountRow>` yields rows with `next()`, and `position()` reports parsed rows/bytes. Cloning `reader.options()` and adding `max_records` preserves its provider settings and exact-column validation. |
| Stream `ReadError`, `Position`, and the existing SDK error docs | Can the test prove rows preceded a transport failure? Yes: `SfError::Read` retains `ReadErrorKind::Transport` and `position().records`. `SfError::from_open` preserves the typed provider failure if opening rejects a non-2xx response. |
| Testserver `Body` rustdoc and the public lifecycle/inspection API | Can failure and stall timing be observed deterministically? `Generate` and `GenerateThenFail` allow gates controlled by the test; `wait_until` observes milestones without fixed-duration sleeps. |

The new concept was a job receipt rather than a page of rows: a successful
future means its sink transaction has published. A failed or dropped future means
the part must restart from its beginning, even if many rows were staged. Successful
completion order and final row order are separate concerns.

## Compiler errors and diagnostic accuracy

There were no Rust type, lifetime or trait-bound compilation errors. The Stage 2
implementation compiled on its first attempt, and all five Stage 2 tests compiled
and passed on their first attempt. The first full clippy invocation failed on this
one style diagnostic, reproduced verbatim:

```text
error: this `if` statement can be collapsed
   --> checks/dx03-fanout-sink/src/stage2.rs:358:5
    |
358 | /     if let Some(declared) = set.declared_records {
359 | |         if declared != position.records {
360 | |             return Err(SfError::Protocol(Box::new(ProtocolError::new(
361 | |                 RESULTS,
...   |
369 | |     }
    | |_____^
    |
    = help: for further information visit https://rust-lang.github.io/rust-clippy/rust-1.99.0/index.html#collapsible_if
    = note: `-D clippy::collapsible-if` implied by `-D warnings`
    = help: to override `-D warnings` add `#[allow(clippy::collapsible_if)]`
help: collapse nested if block
    |
358 ~     if let Some(declared) = set.declared_records
359 ~         && declared != position.records {
360 |             return Err(SfError::Protocol(Box::new(ProtocolError::new(
...
367 |             .into());
368 ~         }
    |

error: could not compile `concord-check-dx03-fanout-sink` (lib test) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `concord-check-dx03-fanout-sink` (lib) due to 1 previous error
```

It pointed at the actual cause: the nested condition violated an enabled style lint
under `-D warnings`. I used the suggested let-chain; no lint was suppressed and no
pagination, CSV or sink behavior changed. The subsequent clippy invocation passed.

## Annotations and adapters

- Reused Stage 1's `AccountRow`, projection constant and setup `ExportError` without
  modifying them. The initial dialect bootstrap/checks are repeated because Stage 1
  must stay frozen and that setup is not a public helper there.
- Named `open_for::<AccountRow>` explicitly. The paginator closure/future types were
  inferred; no explicit `Pin`, boxed future, `'static` bound, `Send` bound, extra
  executor, or stream-combinator dependency was necessary in the consumer.
- Added `PartSink`/`StagedPart` to define the application's publication and rollback
  contract. Neither the CSV reader nor the paginator knows whether a row was durably
  written, so returning a receipt is deliberately after sink publication.
- Added `PartError::{Salesforce, Sink}` and a keyed failure record. Both report types'
  `unemitted_errors` are taken before `into_rerun` consumes the report, so diagnostics
  are not lost. Retained successful receipts are preserved even though completion
  ordering normally emits them immediately.
- Preserved the SDK's runtime reader options, applying the descriptor's optional
  `maxRecords` bound. Manually checked optional `Sforce-NumberOfRecords` at EOF before
  publication. Those checks were already packaged inside `ResultSets` for Stage 1;
  its collected-page return type cannot serve the new incremental sink requirement.
- Added a borrowed `StoredRow` serialization view because Stage 1's row only derives
  `Deserialize`. It serializes one row directly to the bounded file buffer; it does
  not clone strings or construct an intermediate JSON value/table.
- Kept stable discovery ordinals in a `BTreeMap` and continued discovery with the
  SDK cursor unchanged. No conversion from a result count into a locator was used.
- The test sink wraps `FileSink` to count active transactions and rows already
  pushed. This makes incremental delivery and slot occupancy observable independently
  of eventual output equality. Only the test output assertion uses a row vector.

## What was hard

The largest change was application state, not a new request syntax. Stage 1 could
discard everything on failure. Stage 2 must retain completed external effects,
forget failed staged prefixes, distinguish known retries from unknown remaining
discovery, and use ordinals that remain meaningful across several traversals.

`finish()`/`into_rerun()` are useful but not a whole export-resume operation. I still
had to retain the discovery cursor before consuming the report, save diagnostics,
merge retained receipts, run the known retry plan, and then continue discovery.
The result budget must span both phases; otherwise a nominal per-pass limit would
silently double. The bounded-resume fixture exercises that exact boundary.

Streaming moved some provider-level validation back into this caller. The public
pieces are available, but the path is `open_result_set` → `SalesforceCsv` reader →
optional link limit → typed open conversion → row loop → declared-count validation
→ sink publish. I had to make sure an EOF count error did not occur after publication.

Testing only final rows would miss a window that released at headers or a sink that
buffered a whole part. The main fixture therefore waits until two row prefixes have
reached the sink, with both manifests already discovered, then proves part 2 has not
started. It causes part 1's connection to close without completing its body, observes
part 2 publish while part 0 still stalls, and only then releases part 0.

## What was easy

The existing SDK factory plugged directly into `manifest_jobs`. The SDK validated
returned links and kept discovery ordinals stable, including across a failed
manifest. The paginator kept the complete download future in its slot without any
semaphore, spawned task or separate concurrency manager in application code.

The runtime CSV reader retained the Stage 1 provider dialect, exact columns, UTF-8,
embedded CRLF, quotes, whitespace and empty-field behavior. Its async `next()` method
made the incremental sink loop straightforward. Typed read errors exposed exactly
the evidence needed: the failure kind and one already-delivered row.

The public report documentation was unusually useful: it named the important
distinction between retained successful effects and work to rerun, and explicitly
described how to combine independent retry work with continued manifest discovery.
The testserver exposed sufficient control to validate this without a live org.

## Assessment and limits

**Pagination:** the public API fits this requirement well. The whole-job future is
the correct concurrency unit, typed outcomes preserve useful error information,
and `into_rerun` avoids redownloading retained successes. The cost is learning several
report categories and building an application-level resume token. Resume composition
is explicit rather than a single convenience method. My code chooses continued
independent work; it does not introduce retry/backoff policy inside a pass.

**CSV:** incremental `CsvRecords` is enough for a row-at-a-time sink and supplies
bounded parser buffers and useful positions. It does not own sink rollback, which
belongs outside the codec. Reusing configured options to add a descriptor-specific
limit is workable, but less direct than a configured reader setter. The provider's
documented read-time line-ending permissiveness remains unchanged.

**SDK:** parallel discovery is already an effective application-facing adapter;
using its descriptor/cursor types prevented handwritten URL logic. The missing
convenience surfaces for this task are attaching from an id with dialect discovery,
and a streamed-part helper that preserves the SDK's count/limit checks through EOF.
The available public primitives still suffice; no SDK or library changes were made.

**This consumer:** concurrency and per-pass requests/discovery are bounded; row
memory is per active reader/row/file buffer. Receipts and unfinished descriptors
still consume memory proportional to parts across passes. File I/O is synchronous
and can block the executor. Publication assumes a filesystem that supports
same-directory hard links. Ordered assembly streams bytes but is not itself an
atomic whole-export publication.

Resume is deliberately in-process: the owned report is not serialized, and it must
be retained. Dropping the export future cleans up staged parts but loses the token;
process-crash recovery and reconstruction of a durable receipt journal are outside
this implementation. A provider that expires returned result URLs will surface a
typed error on resume; there is no claim that such URLs remain valid indefinitely.

## Final verification

Commands ran from the real workspace root, with `CARGO_TARGET_DIR` inside this
package so build artifacts stay within the permitted directory:

```sh
CARGO_TARGET_DIR=checks/dx03-fanout-sink/target cargo test --offline -p concord-check-dx03-fanout-sink
CARGO_TARGET_DIR=checks/dx03-fanout-sink/target cargo clippy --offline -p concord-check-dx03-fanout-sink --all-targets -- -D warnings
```

Final test output excerpts (exit status 0):

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.67s

running 7 tests
test later_http_failure_preserves_the_typed_salesforce_error ... ok
test later_count_mismatch_is_a_typed_protocol_failure ... ok
test later_csv_decode_failure_is_typed_and_stops_before_its_next_locator ... ok
test header_only_last_set_is_a_complete_empty_export ... ok
test empty_set_with_a_locator_does_not_end_the_chain ... ok
test invalid_job_metadata_is_refused_before_requesting_results ... ok
test collects_two_sets_in_order_using_the_jobs_pipe_crlf_dialect ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

running 5 tests
test a_count_failure_at_eof_discards_all_staged_rows_before_resume ... ok
test sink_failure_does_not_publish_a_prefix_and_can_be_retried ... ok
test bounded_resume_preserves_queued_parts_and_undiscovered_manifests ... ok
test stalled_part_holds_slot_failed_part_is_not_published_and_resume_only_redoes_it ... ok
test discovery_failure_resumes_at_failed_manifest_without_redownloading_completed_parts ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

Library unit tests and doc-tests each report zero tests, with no failures.
Final clippy output (exit status 0):

```text
    Checking concord-check-dx03-fanout-sink v0.0.0 (/home/canarit/projects/concord-ext44/wt-dx03-fanout/checks/dx03-fanout-sink)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.34s
```

`git diff --exit-code -- checks/dx03-fanout-sink/src/stage1.rs checks/dx03-fanout-sink/tests/stage1.rs`
also returned 0. All Stage 2 changes are inside the permitted package.
