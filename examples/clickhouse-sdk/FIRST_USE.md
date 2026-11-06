# Independent first use: ClickHouse HTTP

Baseline: SDK author commit `66f3cf00`, Concord library `0983a349`, Rust 1.99.1,
edition 2024. Consumer test file did not exist; it starts empty in this role.
All requests target a loopback `concord_example_testserver`. Authentication uses
explicitly synthetic values. No ClickHouse instance or credentials are used.

## Prediction A (recorded before writing outcome A)

Use `ClickHouse::new`, `with_base_url`, `Query::new(...).bind(...)`, then
`select(query, runtime_format).query_id(...).rows::<StockRow>().await`.
Expect one GET `/`, no body, literal SQL in `query`, separate `param_tenant`
and `param_minimum` parameters, and `X-ClickHouse-Format` selecting
`CSVWithNames` or `JSONEachRow`. Tabs, backslashes and newlines in a bound
string need ClickHouse escaping in addition to URL encoding. Context adds
database/quota, and selected options add wait/progress flags.

Expect typed rows independent of HTTP chunk boundaries, including UTF-8,
quoted CSV commas, quotes and embedded LF. `next()` yields `Some(Ok(row))`,
then `None`; only `Outcome::Complete` proves completion when the response
announced its exception tag. No tag means `EndedWithoutMarker`, and stopping
before EOF remains `Reading`. A non-2xx exception should produce `Error::Server`
with status/code/name/message and metadata. A tagged in-body exception should
preserve earlier rows, yield one terminal `LateException`, and set `Failed`.
Broken HTTP framing should yield a transport `Read` failure. Malformed or
undecodable rows must also fail. A format mismatch should refuse the head
without waiting for a stalled body. Missing binds should fail before I/O.

## Prediction B (recorded before writing outcome B)

Use `insert(Table::new(identifier)).query_id(...).context(&context)` with a
one-shot `stream::from_iter` source, followed by `json_each_row` or
`csv_with_names`. Expect POST `/`, SQL generated as
``INSERT INTO `inventory_load` FORMAT JSONEachRow`` (or `CSVWithNames`)
in the URL, chunked data in the body, the documented content type, and exact
LF-delimited JSON or fully quoted CSV with a names row. A clean empty response
returns `InsertReceipt`, optional query ID and string-valued summary counters.
Query back through the SELECT API using a separately bound table identifier;
the fake must have received the correct insert before returning stored bytes.

The source must be consumed during send, not collected into an application
buffer. A source error, broken receipt body, or late server exception must
return an error and indicate an unknown insert outcome; do not retry or query
back as though the insert completed. A provider rejection must keep its typed
exception and metadata. Missing summary is not a zero-row receipt.

## Ordered trace

1. First entry point: SDK `README.md`, construction and workflows A/B. Read the
   manifest and `.gitattributes`; no SDK implementation or existing tests read.
2. Read Concord `README.md`, caller guide stream/completion guidance and
   `concord-extras/README.md`. Built both public rustdocs with the prescribed
   cache/jobs prefix: SDK `cargo doc --locked -p concord-sdk-clickhouse --no-deps`
   and facade `cargo doc --locked -p concord --no-deps --features csv,jsonl`.
   Both succeeded. Read rendered rustdoc for `Rows`, `Outcome`, `Select`,
   `Insert`, receipt, metadata and typed errors; did not follow Source links.
3. Read `examples/testserver/src/lib.rs` for its public fake-server API, chunked
   response, deliberate framing failure, wire capture and owned shutdown.
   This is test infrastructure, not SDK source. No other provider SDK supplied
   the solution. Did not read `FRICTION.md` or author notes.
4. Consulted official HTTP, JSONEachRow, CSVWithNames and 24.11 release pages
   below. Predictions A/B above precede all consumer test implementation.
5. Wrote 17 application tests and four independent fixtures. First requested
   test command failed at compilation with `E0308`: `with_base_url(server.url())`
   expected `&str`, found `String`. Applied the diagnostic's suggestion to borrow
   `&server.url()`. This was a caller mistake; the README example uses a string
   literal. The compiler displayed the public SDK signature, but no SDK source
   file was opened. No other distinct compiler diagnostic on this attempt.
6. Second requested test run compiled: 15 passed, 2 failed. The provider-error
   assertion showed that `ServerException::message()` trims the trailing LF
   from the original bytes. Adjusted the expectation to compare the trimmed
   text; the fixture bytes remain exact. The broken-insert-response test
   panicked when unwrapping `error.meta()`: it was `None` despite a scripted
   `X-ClickHouse-Query-Id`. Replaced that unwrap with explicit checks of the
   observed error variant/kind and missing metadata, retaining a regression
   reproduction of the limitation. No SDK source read or native escape used.
7. Third requested test run: all 17 passed, none ignored. Formatted only the new
   test file with Rust 2024 rustfmt. Independently decoded JSON and CSV fixtures
   with Python's standard library and checked equal rows, the 16-byte exception
   tag and the exception message byte count. Git attributes report `text: unset`
   for all four fixtures. Existing tracked files are unchanged.
8. Retained and asserted the outbound query ID in the broken-insert reproduction
   so reconciliation does not depend on missing response metadata. Final locked
   run after formatting: 17 passed, 0 failed, 0 ignored. Rustfmt check and
   `git diff --check` passed; git status contains only this log, the new test
   and the four new fixture files.

## Fixture provenance

New `fixtures/first-use/` files are independently hand-authored synthetic data
following the documented shapes, not downloaded provider examples or captures.
No existing SDK fixture is reused. JSON objects have LF record ends; CSV has
quoted fields, a names row, LF ends and an embedded LF inside a quoted field.
The exception fixture uses CRLF delimiters, a synthetic 16-byte tag and an LF
message included in its byte count, following the README's documented caveat.
Media types follow the SDK README (which attributes them to provider source).
`fixtures/** -text` was already present and is preserved.

Sources consulted:

- [HTTP interface](https://clickhouse.com/docs/interfaces/http): GET/POST,
  parameter escaping, auth, context, summary/progress, late exception framing.
- [JSONEachRow](https://clickhouse.com/docs/interfaces/formats/JSONEachRow):
  one JSON object per line, input and output.
- [CSVWithNames](https://clickhouse.com/docs/interfaces/formats/CSVWithNames):
  names row, input and output.
- [Release 24.11](https://clickhouse.com/blog/clickhouse-release-24-11):
  broken HTTP framing after an execution error.

## Results and concrete friction

The working paths use the README's public builders directly. Outcome A runs in
both runtime formats. Outcome B streams both formats, validates chunked request
bytes and context against a stateful fake, and queries the stored body back.
Failure tests retain delivered rows and verify terminal outcomes, metadata,
typed provider errors, optional receipt counters, head refusal, unknown insert
outcomes and absence of automatic retries. Every fake is explicitly shut down.

First intervention: **none**. No SDK source or pre-existing test was opened;
no private reach-through, native execution/response escape, manually written
Concord operation name, library change or added dependency was needed. The
ordinary reqwest client builder configures only loopback transport policy.

- **Broken insert response loses metadata (observed SDK surface limitation).**
  Reproduction: `insert_broken_receipt_body_reports_unknown_outcome` scripts
  HTTP 200 with `X-ClickHouse-Query-Id: synthetic-broken-receipt` and
  `Body::FailAfter(vec![])` after receiving the entire JSON upload. The result
  is `Error::Concord` / `concord::ErrorKind::Transport`, unknown outcome, and
  `error.meta() == None`. No claim is made that the client exposed the head
  before the close. A SELECT whose body breaks after delivered rows instead
  retains metadata in `Error::Read`. The reproduction explicitly asserts the
  observed limitation; it does not reconstruct lost metadata or patch the SDK.
  Applications must retain their outbound query ID for failure reconciliation.
- **Successful collection requires an explicit completion decision (documented
  provider constraint).** Reproduction:
  `eof_without_marker_is_distinct_from_complete_including_empty_results` returns
  clean, valid JSON/CSV without an exception tag. `collect().await` is `Ok`, but
  the outcome is `EndedWithoutMarker`, including zero rows. Using only `?`
  cannot establish complete success. Successful workflow assertions explicitly
  require `Complete`; stopping after a prefix is checked as `Reading`.
- **Partial rows require `next()` (documented SDK trade-off).** `collect()`
  documents that it drops prior rows on failure. The late-exception and broken
  body cases use `next()` and retain both delivered rows before asserting
  `Failed`; this is application control flow, not a native escape.
- **Exception text is normalized (minor documentation detail).** The provider
  error fixture ends in LF; `message()` returns the text without it. The test
  compares trimmed text while preserving the original fixture bytes. The
  README/rustdoc do not promise byte-for-byte exception text.

Validation command (same prefix for every cargo invocation):

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a cargo test --locked -p concord-sdk-clickhouse --test first_use
```

Result: **17 passed, 0 failed, 0 ignored**. These are offline self-authored
fixture tests, not a live provider verification. The frozen library, SDK
implementation, README, manifest and pre-existing tests remain unchanged.
