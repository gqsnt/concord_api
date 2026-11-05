# Friction log: the Salesforce SDK and its caller on Concord 4.4 extensions

Written while implementing the SDK (`examples/salesforce-sdk`) and the SF-A/SF-B workflows
(`examples/salesforce-app`) against the public APIs of `concord` (DSL, `Prepared`, `Decoder`),
`concord::csv` (CSV codec, branch `ext44/csv` as merged at `e0950f3`) and `concord-pagination`
(`366aa3e`). Each entry: the provider contract, the public path used, the extra code, and whether
it is intentional flexibility or friction. Library changes are proposals only; nothing in
`crates/` was edited.

Summary: the DSL declared all 16 provider operations without escape hatches beyond the
documented native route; the CSV codec's runtime options and `columns` fit the Bulk dialect well;
`manifest_jobs` gave the discovered-fanout workflow its bounds, keyed outcomes and retained
receipts with little glue. The real friction is concentrated in four places: no generic endpoint
output (F1), no bounded known-length encode route (F4), the reader cannot enforce one line ending
(F5), and restart composition across two traversal types (F9).

## F1 — One output type per endpoint: query endpoints declared three times (friction)

- **Provider contract.** `GET query?q=` and `GET query/{locator}` return records of whatever
  object the SOQL selects.
- **Public path.** One declared endpoint per record type, plus a trait mapping the record type to
  its endpoints:

  ```rust
  GET accounts(query q: Soql) -> QueryResponse<Account>;
  GET contacts(query q: Soql) -> QueryResponse<Contact>;
  GET dynamic(query q: Soql) -> QueryResponse<DynamicRecord>;
  GET more_accounts(#[sensitive] locator: QueryLocator) -> QueryResponse<Account> { path "{locator}"; }
  // ... more_contacts, more_dynamic; likewise accounts().get / contacts().get / sobject(name).get

  impl QueryRecord for Account {
      fn first(q: &SalesforceApiQuery, soql: Soql) -> Result<Prepared<QueryResponse<Self>>, concord::Error> {
          q.accounts(soql).prepare()
      }
      fn more(q: &SalesforceApiQuery, l: QueryLocator) -> Result<Prepared<QueryResponse<Self>>, concord::Error> {
          q.more_accounts(l).prepare()
      }
      // ...
  }
  ```
- **Extra code.** 6 extra endpoint declarations and three trait impls (~60 lines). A caller's
  own record type needs an SDK change.
- **Alternative not taken.** Declare one endpoint without output and finish with
  `Decoder::<QueryResponse<T>>::new::<Json>()` after `into_parts()` (documented native route): it
  works for any `T`, but loses the generated `send()`/typed `Prepared<T>` and the endpoint name in
  errors.
- **Proposal.** Document the per-type pattern next to the native-decoder route as the two answers
  to "generic output", or allow an endpoint output type parameter (larger change; not required).

## F2 — Runtime CSV dialects bypass typed outputs (intentional flexibility)

- **Contract.** Result sets and ingest result files are CSV in the dialect chosen per job.
- **Public path.** CSV endpoints have no `->`; they are sent with `prepare()?.send_raw()` and read
  with `CsvReader::new(SalesforceCsv::new(dialect).options()...)?.open_for::<Row>(response, label)`.
  A non-2xx refusal is an `OpenError` that keeps the response, which the SDK reads as an error
  document: `SfError::from_open(error).await` (10 lines).
- **Assessment.** This is the plan's intended design (static `Csv<H, D, P>` cannot carry a
  per-job delimiter). The status/head checks move from `Decoder` to the reader; the two error
  types (`concord::Error` with endpoint name vs `OpenError` with an SDK label) have to be unified
  in the SDK's error type.

## F3 — Writer media type `text/csv; header=present` vs the provider's `text/csv` (friction, minor)

- **Contract.** Upload Job Data: "Formats: text/csv"; the examples send `Content-Type: text/csv`.
- **Public path.** `CsvWriter::upload` / `Upload::apply` set `text/csv; header=present`, the
  codec's media type. Whether Salesforce accepts the `header` parameter is unknown, so the
  Salesforce path does not use `Upload` at all: the spool's bytes go through the declared
  `PUT upload(body csv: Bytes) { request Binary "text/csv"; }`. A caller using `Upload::apply`
  would have to overwrite the header afterwards (`request.headers_mut().insert(CONTENT_TYPE, ..)`).
- **Proposal.** A writer media option (e.g. `MediaPolicy`-like `write_media(..)` with "no header
  parameter"), or `Upload::with_content_type(HeaderValue)`.

## F4 — No bounded, known-length encode route (friction)

- **Contract.** Upload bodies are limited (100 MB of CSV); chunked uploads are not documented, so
  the SDK sends `Content-Length` bodies (PROVIDER SF-D6).
- **Public path.** A spool built from repeated finite encodes: the header from a `HasHeader`
  writer with the profile's `columns` and an empty iterator, then batches of 512 rows through a
  second, **headerless** writer with the same dialect and columns, checking the size after each
  batch:

  ```rust
  let header = writer.to_vec(Rows::new(std::iter::empty::<RawRow>()))?;   // header only
  let rows_writer = CsvWriter::new(
      CsvOptions::with_profile(HeaderMode::Absent, delimiter, options.profile().clone())
          .columns(columns.names().iter().cloned()),
  )?;
  // per batch:
  let bytes = rows_writer.to_vec(Rows::new(batch.drain(..)))?;
  if body.len() + bytes.len() > limit { return Err(CsvError::UploadTooLarge { limit }); }
  ```
- **Extra code.** ~70 lines (`Spool`, `spool`, `spool_stream` with a manual `poll_fn` over the
  one-shot `Stream`). Spooling the lazy `Upload` body itself would need `http-body` and a manual
  `poll_frame` loop, a dependency outside the SDK brief.
- **Proposal.** `CsvWriter::write_to(&mut impl io::Write, rows)` plus a writer-side
  `max_total_bytes`, or an async `Upload::collect_bounded(max) -> Result<Bytes, UploadError>`.
  Either would make "encode with the public encoder, then send with a known length" a one-liner
  and keep the header/schema checks in one writer.

## F5 — The reader cannot enforce the job's line ending (friction)

- **Contract.** "specify a line ending that matches the line ending used to create the CSV file
  using the `lineEnding` request field" (LF or CRLF).
- **Public path.** `RecordTerminator::{Standard, Byte(u8)}`: `Standard` accepts `\n`, `\r\n` and
  `\r`; `Byte(b'\n')` for an LF job would silently keep a stray `\r` at the end of the last field
  of a CRLF body. The adapter uses `Standard` and documents that a mismatched body is parsed, not
  refused.
- **Proposal.** `RecordTerminator::CrLf` (only `\r\n`, a lone `\r` or `\n` is a `Format` error)
  and a strict LF mode that refuses `\r\n`.

## F6 — Exact header matching via profile `columns` (positive)

- `CsvOptions::columns([...])` makes header-reading readers require the wire header to be exactly
  the selected fields, in order (CSV-04): precisely the Bulk API 50.0+ rule. Width, decode and
  limit errors arrive as typed `ReadError` kinds with positions and no row values. Per-part
  headers are parsed by each part's own reader, so repeated headers never leak into records.

## F7 — `CodecError` is large, so wrapping enums box it (friction, minor)

- `clippy::result_large_err` (with `-D warnings`) refused `Result<_, PartFailure>` and
  `Result<_, SfError>` until the CSV error and a few SDK variants were boxed. Measured with
  `size_of` on this toolchain: `concord::Error` is 8 bytes (already one pointer), `CodecError`
  112 bytes, the SDK's `CsvError` (which holds a `CodecError`) 120 bytes; `SfError` is 16 bytes
  after boxing.
- **Proposal.** Box `CodecError`'s payload internally, as `concord::Error`, `OpenError` and
  `ReadError` already do, so every wrapper does not have to.

## F8 — Job factories do not receive their discovery index (friction, minor)

- **Contract.** Result parts must be combined in discovery order and staged under stable names
  that survive a restart.
- **Public path.** `manifest_jobs`' job factory receives only the descriptor (`FnMut(D)`), while
  `Keyed::index` is known only to the caller of `next()`. The SDK's manifest factory numbers the
  descriptors itself and carries the next ordinal in the manifest key so a resumed discovery
  numbers parts identically:

  ```rust
  pub struct ManifestCursor { pub locator: ResultLocator, pub next_ordinal: u64 }
  pub struct ResultPart { pub ordinal: u64, pub link: ResultLink }
  ```
- **Assessment.** Small and arguably better (the ordinal survives across traversals, which
  `Keyed::index` would not: it restarts at 0 in each traversal). Proposal: document the pattern.

## F9 — Restart composes two traversal types with different report shapes (friction)

- **Contract.** After a failure, published shards are reused, failed/cancelled/queued parts run
  again, and discovery continues from the failed manifest.
- **Public path.** `JobsReport::into_rerun()`/fields → run the re-runs with `independent(..)`,
  then continue discovery with another `manifest_jobs(discovery.resume, ..)`. The two reports
  differ (`IndependentReport::rest: Option<Peekable<..>>` vs `JobsReport::queued: Vec<D>`), so
  `sf_b::run` merges completed/retained/failed/cancelled/queued twice (~80 lines).
- **Proposal.** Let `manifest_jobs` take seed descriptors to run before discovery (or a
  `JobsReport::resume(..)` constructor), so one traversal and one report cover a restart.

## F10 — Sequential failure classification needs the emitted error (observation)

- `Sequential::finish()` reports the failed key, not the error; the error is moved out by
  `next()`. SF-A keeps the last error to decide between `Interrupted` (resumable) and
  `RestartRequired` (`INVALID_QUERY_LOCATOR`). Natural with the move-only design; no change
  proposed.

## F11 — Status error documents consume the Concord error (observation)

- `concord::Error::into_response()` consumes the error, so the SDK keeps the endpoint name and
  status alongside the decoded Salesforce error document (`ApiFailure`) instead of the original
  `concord::Error`. Reads are bounded (`MAX_ERROR_BODY` = 64 KiB) with `chunk()`.

## F12 — Returned links: validate and rebuild, never follow (intentional)

- **Contract.** `nextRecordsUrl`, `resultUrl`, the result-pages next link, `contentUrl` and
  `attributes.url` are returned to be followed.
- **Public path.** No Concord route follows a URL "with an endpoint's decoder", and the recipes
  advise against rewriting a seed request's URL. The SDK validates each link against the org and
  extracts typed parts (`OrgContext::query_link`, `result_link`, `result_pages_link`,
  `content_url`, `record_link`, ~200 lines with tests), then sends the declared operation. One
  consequence is SF-D2: the server's `resultpages` link is requested as the declared
  `resultPages`.
- **Assessment.** Intentional: it keeps base, version, headers, sensitivity and decoder from the
  declaration and makes origin/version/job checks explicit.

## F13 — DSL details that worked without friction (positive)

- `path "services/data/v{version}"` (a literal prefix and a value in one segment), an optional
  scope value feeding a header template (`scope query(batch_size?: BatchSize) { headers {
  "sforce-query-options" = "batchSize={batch_size}"; } }`, absent ⇒ no header), `#[sensitive]`
  on locators (withheld from `Prepared`/handle `Debug`), trailing-slash endpoint paths
  (`successfulResults/`), `request Binary "text/csv"`, and endpoint-level `ACCEPT = "text/csv"`.

## F14 — Test infrastructure (friction outside the library)

- The shared test server has no routing helper; this crate's tests carry a ~70-line `Router`
  (host + method + exact target, then path-only fallback; reply queues for stateful flows such as
  polls and a failure followed by success). Other SDK tests likely duplicate it.

## F15 — Sync file writes inside job futures (observation)

- The examples' allowed dependencies have no async file API, so staged shards are written with
  `std::fs` inside the job futures (one encoded batch at a time). Acceptable for an example; a
  production sink would use a blocking pool or async files. Not a library issue.
