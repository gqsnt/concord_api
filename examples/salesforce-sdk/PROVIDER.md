# Salesforce: pinned version, sources, scope and decisions

A bounded slice of the Salesforce REST API and Bulk API 2.0, written against the documentation,
not against a live org. **No live org was used**: every behaviour below is documented or an
explicit decision, and every test runs against the loopback server (`OFFLINE_VERIFIED`); live
runs are `NOT_RUN` (no credentials, no sandbox).

## Version

- **Pinned: API `68.0`** (`ApiVersion::PINNED`), sent in every path as `/services/data/v68.0/`.
  It is the version the REST and Bulk API 2.0 documentation shows in its examples on the
  retrieval date (`/services/data/v68.0/query/...`, `"apiVersion" : 68.0`, `contentUrl`
  `services/data/v68.0/...`).
- **Minimum accepted by the context: `58.0`** (`ApiVersion::MINIMUM`): "Get Parallel Results for
  a Query Job … is available in API version 58.0 and later". The other operations are older
  (query jobs 47.0, ingest 41.0).
- The org context fixes the version for every request; Bulk job documents are checked against it
  ("use the same API version that you used to create the query. Otherwise, the call returns a 409
  error").
- Retrieval: **2026-11-04**. `developer.salesforce.com` refused direct HTTP fetches (403), so the
  pages were read through a browser session, including their "View as Markdown" form. The
  release-numbered archive URLs (`atlas.en-us.258.0…`, `…264.0…`) served the same text as the
  current pages, so they are not a version-frozen snapshot; the evidence is the current
  documentation, which shows 68.0.

## Sources

REST API Developer Guide (`https://developer.salesforce.com/docs/platform/api-rest/guide/`):
`resources-query.html` (Query), `resources-query-more-results.html` (Query More Results, [F1]),
`dome-query.html` (Execute a SOQL Query), `headers-queryoptions.html` (`Sforce-Query-Options`
batchSize 200–2,000), `headers-api-usage.html` (`Sforce-Limit-Info`),
`resources-sobject-describe.html` and `dome-sobject-describe.html` (sObject Describe),
`resources-sobject-retrieve-get.html` (sObject Rows GET, `fields`), `errorcodes.html` (status
codes and error documents).

SOAP API Developer Guide: `sforce_api_calls_concepts_core_data_objects.htm` (`INVALID_QUERY_LOCATOR`:
"The query locator in the call is invalid or doesn't exist"), `sforce_api_calls_querymore.htm`.

Bulk API 2.0 Developer Guide (`https://developer.salesforce.com/docs/platform/api-asynch/guide/`):
`query-create-job.html` [F3], `query-get-one-job.html`, `query-abort-job.html`,
`query-get-job-results.html` [F2], `query-get-parallel-job-results.html` [F4], `queries.html`,
`query-compression.html`, `create-job.html`, `upload-job-data.html`, `close-job.html`,
`get-job-info.html`, `abort-job.html`, `get-job-successful-results.html`,
`get-job-failed-results.html`, `get-job-unprocessed-results.html`, `datafiles-prepare-csv.html`
[F5], `datafiles-csv-samples.html`, `bulk-api-2-0-line-ending-header.html`,
`bulk-api-2-0-content-type-header.html`, `bulk-api-2-0-errors.html`.

## Operations

24 declared endpoints (`src/api.rs`) covering 16 provider operations.

| Family | Operation | Declared endpoint(s) | Typed wrapper |
|---|---|---|---|
| Metadata | sObject Describe | `accounts().describe()`, `contacts().describe()`, `sobject(name).describe()` | — (used by SF-A) |
| Records | sObject Rows GET (`?fields=`) | `accounts().get(id, fields)`, `contacts().get(..)`, `sobject(name).get(id)` | — |
| SOQL | Query (`GET query?q=`) | `query().accounts(q)`, `.contacts(q)`, `.dynamic(q)` | `query::QueryPages` |
| SOQL | Query More (`GET query/{locator}`) | `query().more_accounts(l)`, `.more_contacts(l)`, `.more_dynamic(l)` | `query::QueryPages` |
| Bulk query | Create (`POST jobs/query`) | `bulk_query().create(body)` | `BulkQueryJob::create` |
| Bulk query | Job info | `bulk_query().job(id).info()` | `BulkQueryJob::info`, `wait_complete` |
| Bulk query | Abort (`PATCH {"state":"Aborted"}`) | `bulk_query().job(id).abort(change)` | `BulkQueryJob::abort` |
| Bulk query | Parallel result pages | `bulk_query().job(id).result_pages()` | `ResultManifests::fetch` (manifest factory) |
| Bulk query | Results (`locator`, `maxRecords`) | `bulk_query().job(id).results()` (`-> stream`: the unread body) | `ResultPartReader::open` (one validated part), `ResultSets` (sequential chain, same reader) |
| Ingest | Create (`POST jobs/ingest`) | `ingest().create(body)` | `IngestJob::create` |
| Ingest | Job info | `ingest().job(id).info()` | `IngestJob::info`, `wait_finished` |
| Ingest | Upload complete / abort (`PATCH`) | `ingest().job(id).set_state(change)` | `IngestJob::close`, `abort` |
| Ingest | Upload job data (`PUT .../batches`, `text/csv`) | `ingest().job(id).upload(bytes)` | `IngestJob::upload`, `upload_spooled` |
| Ingest | Successful / failed / unprocessed results | `ingest().job(id).successful_results()`, `.failed_results()`, `.unprocessed_records()` (`-> stream`) | `IngestJob::result_rows` |

**Deliberately unsupported** (a bounded slice, not a complete SDK): authentication flows (the
caller supplies an access token), REST record writes, `queryAll` over REST, SOSL, composite and
batch resources, Bulk API 1.0, the multipart create-and-upload ingest request, query/ingest job
delete and list-all, PK-chunking headers, platform-event subscriptions, `Sforce-Call-Options`,
response compression (`Accept-Encoding: gzip` needs reqwest's `gzip` feature, not enabled here:
results arrive uncompressed), relationship sub-queries.

## Documented discrepancies and decisions

**SF-D1 `nextRecordUrl` vs `nextRecordsUrl` (parallel result pages).** The field table of "Get
Parallel Results for a Query Job" names the next link `nextRecordUrl`; its example response and
the prose under it use `nextRecordsUrl` (the same in the 258.0 and 264.0 archive pages). The SOAP
`INVALID_QUERY_LOCATOR` entry also says "the `nextRecordUrl` call (REST API)", while every REST
query page and example uses `nextRecordsUrl`. Decision: for result pages, **both spellings are
read**; one present → used; both present with the same value → one link; both present with
different values → `ProtocolErrorKind::AmbiguousNextLink` (never guessed). REST query envelopes
read only `nextRecordsUrl` (consistent in its own documentation; an unexpected spelling then means
`done: false` without a link → `MissingNextLink`, fail closed). Tests:
`bulk::tests::next_link_alias_policy`, `sf_b_next_manifest_link_alias_policy_on_the_wire`,
`sf_b_official_parallel_results_example_agrees_with_the_link_policy`.

**SF-D2 `resultPages` vs `resultpages`.** The documented URI is `.../resultPages`; the example's
next link is `.../resultpages?locator=YcApWm4N`. The link is accepted with either spelling (ASCII
case-insensitive); the request is the declared `resultPages` endpoint with `locator=`. The
`?locator=` request form of the manifest resource is inferred from the documented example link,
not stated by the field table.

**SF-D3 `apiVersion` type.** Tables type it as a string, examples send a number (`46.0`, `68.0`);
both are accepted (`ApiVersion` deserializer).

**SF-D4 `contentUrl` form.** The documented example is relative without a leading slash
(`services/data/v68.0/jobs/ingest/{id}/batches`); both that and the leading-slash form are
accepted, validated against the org, version and job, and the upload uses the declared endpoint.

**SF-D5 Null vs empty in query results.** Not documented. Empty fields (quoted `""` or unquoted)
are `None` in typed rows; raw records keep bytes. Exception: the Contact export's
`HasOptedOutOfEmail` (a standard, non-nillable boolean, "Email Opt Out" in the Object Reference;
that page was not retrievable over HTTP either, 403) is a `bool`: `true`/`false`, and an empty or
other value is a decode error of its result set. For ingest the documentation is explicit: an
empty field is ignored on update, `#N/A` sets null (`IngestValue::{Keep, Null}`); a literal
`#N/A` text value is refused as ambiguous (`FieldText`).

**SF-D6 Upload framing.** The upload page states a limit ("does not in total exceed 150 MB of
base64 encoded content … upload data that does not exceed 100 MB") and `Content-Type: text/csv`,
and says nothing about chunked transfer encoding. The SDK therefore sends **one body of known
length** (`Content-Length`), produced by the public encoder into a bounded in-memory spool
(`SalesforceCsv::spool` / `spool_stream`, default and maximum 100 MB = `MAX_UPLOAD_BYTES`). Cost:
the whole encoded body is buffered before the request starts. Live acceptance of either framing
is unverified. The generic chunked route is tested only against a labelled transport fixture.

**SF-D7 `Sforce-NumberOfRecords` on parallel result URLs.** Documented for the results resource,
which the result URLs address. When present it must equal the parsed row count at the parsed end
of input (`RecordCountMismatch`, after the part's rows were delivered); when absent the check is
skipped and the completion says so (`declared_records: None`). Both routes read parts through one
adapter, `bulk_query::ResultPartReader`: only `ResultPartRows::finish` after that validated end
returns a `PartComplete`; an early stop, a limit (`maxRecords`, a byte limit) or a read error is
`SfError::Incomplete`, never a completion. `Sforce-Locator` is required only on the sequential
chain (`ResultSets`); `"null"` (the string) ends it.

**SF-D8 Ingest result file dialect.** Not stated explicitly; read with the job's delimiter and
line ending (the files echo "field data … provided in the original job data upload"). Columns are
mapped by name (`sf__Id`, `sf__Created`, `sf__Error`, then the uploaded fields), not by position.

**SF-D9 Locators.** Opaque: never derived from `totalSize`, a count or an offset. Query locators
allow `[A-Za-z0-9-]`, result locators `[A-Za-z0-9_.~=-]` (documented examples: `MTAwMDA`,
`aBcDeFg4N`, `01gD0000002HU6KIAW-2000`); anything else in a returned link is refused, not
re-encoded. An expired query locator (`INVALID_QUERY_LOCATOR`) ends a section with an explicit
restart requirement; its lifetime is not stated on the pages read and is not assumed.

**SF-D10 Envelope contradictions.** `done: false` without a next link and `done: true` with one
are protocol errors (fail closed), for REST queries and result pages alike.

**SF-D11 Returned links.** Validated against the instance origin (scheme, host, port), the
context's API version, the expected resource and job, canonical form (no dot segments,
backslashes, whitespace, user information, fragments) and the documented query parameters; the
SDK then rebuilds the declared operation from the extracted parts (`context.rs`).

**SF-D12 CSV header order.** "In API version 50.0 and later, the order of the columns returned by
the query is the same as the order you requested them": the typed result reader requires the
header to be exactly the selected fields in order.

**SF-D13 Line endings on read.** The job's `lineEnding` is sent at creation, checked on every job
document and used when writing; the codec's reader accepts `\n`, `\r\n` and `\r` together and
cannot be restricted to `\r\n`, so a result body with the other ending is parsed rather than
refused (FRICTION F5).

## Evidence labels

| Workflow | Label | Scope |
|---|---|---|
| SF-A | `OFFLINE_VERIFIED` | `cargo test -p concord-example-salesforce-app --test sf_a_crm_extraction` against the loopback server with documentation-derived fixtures |
| SF-A live | `NOT_RUN` | `cargo run -p concord-example-salesforce-app --bin sf_a` prints `NOT_RUN` without `SF_INSTANCE_URL`/`SF_ACCESS_TOKEN` |
| SF-B export + ingest | `OFFLINE_VERIFIED` | `--test sf_b_bulk_csv_round_trip`, `--test sf_b_ingest_round_trip` |
| SF-B live | `NOT_RUN` | `--bin sf_b`: needs credentials plus `SF_SANDBOX_JOBS=1` (query job creation) and `SF_SANDBOX_MUTATIONS=1` (ingest writes) |

### Operations and paging (Concord Next)

SOQL nextRecordsUrl is validated into a typed locator and sent with a separately declared query-more operation; it may be relative. Bulk manifests similarly validate job/version/locator; result URLs use a per-job CSV dialect. An absolute same-endpoint next-URL declaration would lose distinct operation identities and runtime body contracts.
