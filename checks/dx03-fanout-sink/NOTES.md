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
