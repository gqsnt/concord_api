# Friction log — ClickHouse HTTP SDK (author)

Concord frozen at `0983a349`; toolchain 1.99.1. Entries are ordered by how much SDK code they
cost or saved. Each has a reproduction (`file:line` in this crate at the commit that adds this
file) and the cause as I judge it: **Concord**, **docs**, **SDK**, **provider** or
**application**. Nothing under `crates/`, `docs/`, `scripts/` or `checks/` was edited.

Reading done: the root `README.md`; `docs/CHEATSHEET.md`, `docs/CALLER_GUIDE.md`,
`docs/RECIPES.md`, `docs/AUTHOR_GUIDE.md` §2–§4.9 and §5.9–§5.13, `docs/CUSTOM_FORMATS.md` §9a–§10;
`crates/concord-extras/README.md`; the sources of `concord-core` (`response.rs`, `error.rs`,
`prepared.rs`, parts of `codec.rs`) and `concord-extras` (`stream/{mod,source,open,error,upload,records}.rs`,
`csv/{mod,read,write,options}.rs`, `jsonl/{mod,lines,policy,writer}.rs`, `encoded.rs`, `text.rs`);
`examples/testserver/src/lib.rs`. Of the provider SDKs I read only `examples/salesforce-sdk/Cargo.toml`
and the head of its `FRICTION.md`, and the `Cargo.toml` and the heads of `PROVENANCE.md` /
`FRICTION.md` of `examples/ntfy-sdk`, for the shape of these records. No SDK supplied a solution;
the `param_<name>` route below is the documented recipe "Interleaved query parameters".

## 1. Query parameters with per-query names cannot be declared — Concord

- **Provider contract.** A placeholder `{name:Type}` takes its value from the URL parameter
  `param_<name>`; the names are chosen by each query's SQL.
- **What I wrote.** Every query and command goes through
  `prepare()?.into_parts()`, `request.url_mut().query_pairs_mut().append_pair(..)` and
  `decoder.execute(&client, request)` (`src/client.rs:373`–`:384`). The generated `send()` is
  never used for them; the declaration states only the fixed parameters (`src/wire.rs:39`–`:47`).
- **Cost.** Small (12 lines) because RECIPES documents the route, but every caller-visible request
  now leaves Concord's boundary before it is sent: `Prepared`'s redacted `Debug` no longer
  describes the request that goes out (the SDK never logs it, but a caller cannot inspect the
  final request through Concord either), and the appended pairs are invisible to
  `#[sensitive]`.
- **Reproduction.** Try to declare `query("param_{name}") value: String` or a pair list: a key is
  a literal (AUTHOR_GUIDE §4.1–§4.2); there is no placement whose key is a value.
- **Proposal.** A query placement over pairs (`query each_pair params: Vec<(String, String)>`,
  keys validated at build time, optionally `#[sensitive]`), appended after the declared keys.

## 2. A byte-level filter in front of a reader bypasses `open()` and the operation name — Concord

- **Provider contract.** After a 2xx head the body may end with an exception block marked by
  the response's `X-ClickHouse-Exception-Tag`; it must not reach the CSV/JSON Lines parser, and
  it must become a typed late error.
- **What I wrote.** `src/guard.rs` (a `ByteSource` that forwards bytes, holds back a possible
  marker prefix and parses the block; ~230 lines with tests). Because the readers' `open()`
  takes only an `Openable` response, the SDK uses `read(source)` instead, which checks neither
  status nor head and names no operation, and re-does those steps: status
  (`src/client.rs:474`), the CSV head with `CsvReader::check_head` (`src/client.rs:495`), and the
  JSON Lines head with `<Ndjson as Incremental<IgnoredAny>>::check_content_type`
  (`src/client.rs:506`) — `JsonLinesReader` has no `check_head`, unlike `CsvReader`. Reader errors
  carry `operation() == None`; the SDK's error types carry the operation instead.
- **Reproduction.** `JsonLinesReader::check_head` does not exist (`crates/concord-extras/src/jsonl/lines.rs:417`
  lists `new`, `options`, `open`, `open_for`, `read`); `CsvReader::read` / `JsonLinesReader::read`
  take no operation.
- **Proposal.** `open_with(response, |body| wrap(body))` (open, then read through an adapter of
  the body) or `read_for(source, operation)`, and `JsonLinesReader::check_head` for parity.

## 3. A provider-typed source failure comes back only by downcast — Concord

- **What I wrote.** The guard returns `SourceError::new(..).with_source(BodyException)`; the
  reader turns it into `ReadError` of kind `Transport`; the SDK recovers it with
  `source_unredacted()` + `downcast_ref::<BodyException>()` and clones it (`src/rows.rs:93`–`:105`,
  again in `src/client.rs:437` and `src/insert.rs:163`). It works and is tested, but the only
  channel for "the source ended the body for a provider reason" is the accessor meant for
  unredacted diagnostics, and `Transport` is the only kind.
- **Second effect.** A `SourceError`'s source cannot be taken back. On the insert path the SDK
  needs Concord's own transport error, so it wraps `Incoming` in a `ByteSource` that stashes the
  `concord::Error` (`src/insert.rs:198`) and relies on an invariant `expect` to retrieve it
  (`src/insert.rs:174`).
- **Proposal.** A `SourceError::into_source()` (public), or a `ReadErrorKind::Source` /
  `ReadError::source_as::<E>()` for a typed condition raised by a source.

## 4. Runtime-selected formats need an enum of boxed readers — Concord (minor)

- **What I wrote.** `Reader<T> { Csv(Box<CsvRecords<..>>), Json(Box<JsonLinesRecords<..>>) }`
  with a `match` in each accessor (`src/rows.rs:18`–`:22`, `:62`–`:150`). Clippy's
  `large_enum_variant` (CSV ~944 bytes, JSON Lines ~408 bytes) forced both boxes.
- **Why.** `RecordReader` is sealed and returns `impl Future`, so there is no `dyn` reader to
  hold "the reader of the format chosen at run time".
- **Proposal.** None needed; noting that a response whose format is chosen per call always costs
  this enum.

## 5. `JsonLinesRecords` accessors require `T: DeserializeOwned`; `CsvRecords`' do not — Concord (minor)

- **Reproduction.** Move `Rows::outcome` / `position` into `impl<T> Rows<T>`
  (`src/rows.rs:114`–`:116`): E0599, "`completion` exists ... but its trait bounds were not
  satisfied: `T: Deserialize<'de>`". The CSV reader's `completion()` has no bound.
- **Effect.** `Rows`' accessors and its `Debug` carry the bound.

## 6. Display-typed header values need a template — Concord (minor, documented)

- `header("x-clickhouse-format") format: OutputFormat` is not accepted (header values are the
  closed `ToHeaderValue` set, AUTHOR_GUIDE §4.8); the declaration writes
  `headers { "x-clickhouse-format" = "{format}"; }` (`src/wire.rs:46`, `:65`) and likewise for the
  database `Identifier` (`src/wire.rs:36`). One line each, and an optional value in a template
  correctly omits the header when absent.

## 7. What Concord made easy

- **Streamed inserts.** `JsonLinesWriter::upload(StreamRecords::new(rows))` /
  `CsvWriter::upload(StreamRows::new(rows))` with `body rows: Upload` + `request Encoded;`
  (`src/wire.rs:50`–`:56`, `src/insert.rs:94`, `:120`–`:131`): zero polls at prepare time, chunked,
  the media type bound to the bytes. The exact bytes are asserted in `tests/protocol.rs` and
  `tests/workflows.rs`; the documented JSONEachRow rows are reproduced byte for byte.
- **The CSV dialect.** `QuoteStyle::Always` and `LineEnding::Lf` (`src/insert.rs:122`–`:123`)
  covered ClickHouse's input rules (trimmed unquoted fields, single-quote strings) without code.
- **Incremental reading.** Quoted CSV fields, doubled quotes, quoted line breaks and 2- and 4-byte
  UTF-8 characters split across chunks, down to one byte per chunk, needed no SDK code
  (`tests/workflows.rs`, `workflow_a_survives_split_fields_and_rows_then_reports_the_late_exception`).
- **Response boundary.** `-> stream` plus `Received`: status and the whole head (`meta()`) before
  the body; `bytes_limited` for the bounded exception text (`src/client.rs:388`–`:412`);
  `concord::ResponseMeta` wrapped for the `X-ClickHouse-*` accessors (`src/meta.rs`), as AUTHOR_GUIDE
  §5.11 suggests.
- **Redirects and secrets.** AUTHOR_GUIDE §3's warning that reqwest forwards custom credential
  headers across origins led to Basic authentication as the first option and
  `ClickHouse::recommended_http_client()` (`src/client.rs:186`).

## 8. Provider documentation gaps — provider / docs

- The response `Content-Type` of `CSVWithNames` and `JSONEachRow` is not on the format or HTTP
  pages; it was read from the source (PROVENANCE). It happens to equal Concord's `Ndjson` and
  `Csv<HasHeader>` media policies.
- The HTTP page's block layout ends the message line with `\r\n`; the source writes `\n` and
  counts it, and the page's own example (261-byte message, `262`) agrees with the source. The
  parser does not depend on either.
- The output columns of `KILL QUERY` are not documented (only `kill_status`); the SDK reads
  `kill_status` and `query_id` and ignores the rest.
- The version that introduced the tagged block is not stated; the SDK therefore reports
  `Outcome::EndedWithoutMarker`, not `Complete`, whenever the head carries no tag.

## 9. SDK decisions with a cost

- `SELECT` goes over `GET` (read-only on the server) with the SQL in the URL: the URL limit
  (1 MiB by default) bounds query text plus parameters, and the text appears in access logs.
- Identifiers are narrower than ClickHouse's grammar, so the SDK can build `INSERT` statements
  without an escaping rule; a table named outside `[A-Za-z_][A-Za-z0-9_]*` cannot be targeted.
- The placeholder scanner (`src/query.rs`, `scan_placeholders`) is SDK code (~100 lines with
  tests) to fail before sending; the server stays the authority.
