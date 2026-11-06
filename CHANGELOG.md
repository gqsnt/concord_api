# Changelog

## Unreleased — interface consolidation (candidate)

Not released: the version is unchanged (4.2.0) and no release has been decided. Records:
`v4/sdk20/`. Source-compatible for callers of the existing API except where noted.

### Changed

- **An empty declared body is sent with `Content-Length: 0`.** reqwest omits the field for an
  empty buffered body, so an empty PUT/POST looked like a request without content (RFC 9110
  §8.6; Google Drive's resumable-upload status probe documents `Content-Length: 0`). A request
  without a declared body and streamed bodies are unchanged.

### Added

- **Owned response metadata.** `concord::ResponseMeta` (status, version, header map, operation;
  `Debug` prints header names, never values), from `Received::meta()` and `Incoming::meta()`:
  the value SDKs kept beside a decoded value or provider error after the body is consumed.
- **One response boundary.** `Prepared::receive()` executes once and returns `Received<T>`: any
  status, body unread, the declared handling and the endpoint identity (`Operation`). Read the head
  (`head()`), then consume once: `decode()` (what `send()` does), `into_incoming()` (an incremental
  reader chosen at run time) or `into_response()` (native). `Decoder::execute`, `Decoder::receive`
  and `Decoder::transport_error` serve natively edited requests without reconstructing execution.
- **Truthful streaming outputs.** `-> stream T` (format `Incremental<T>`) makes `send()` return the
  format's reader without reading the body; `-> stream` returns the unread `Incoming` body.
  `Incremental` is implemented by `Csv<..>`, `JsonLines`, `Ndjson`, the new `jsonl::JsonStream`
  (consecutive JSON values) and `IgnoreContentType<F>`.
- **Readers open declared responses.** `CsvReader::open/open_raw`, `JsonLinesReader::open` and
  `JsonValuesReader::open` accept `Incoming`, `Received<T>` or a native response
  (`stream::Openable`); the endpoint's identity reaches every `ReadError` without `open_for`.
  `completion()` distinguishes a clean end of input from a failure or an early stop.
  `into_stream()` returns an owning standard `Stream` (`stream::RecordStream`).
- **Runtime body media.** `Encode::encode_body` (defaulted) returns an `EncodedBody` whose media
  type is bound to its bytes; contradictions with a declared media type or `content-type` header
  are `Build` errors before I/O. The `Encoded` format sends an `EncodedBody` or `stream::Upload`
  through a declared body.
- **Ordinary pagination.** `concord_pagination::Pages`/`PageItems`/`ItemStream`: nameable,
  owned traversals whose `Ok(None)` means provider exhaustion; caps and failures are typed
  `PagesError`s; `collect_complete` returns `Incomplete` with the collected prefix.
  `JobsReport::into_resume` / `IndependentReport::into_resume` decompose a report losslessly.

- **One SDK adapter for all operations.** Every generated endpoint implements `concord::Endpoint<T>`
  (`prepare()`, `operation()`; the output `T` is a parameter), so an SDK writes one generic adapter (its own error and
  metadata) instead of a wrapper method per operation. `Prepared::operation()`.
- **The SDK's own `send()`.** A client statement `send Policy;` makes every generated `send()`
  return `<Policy as SendWith<T>>::Output`: an SDK's result type and error, metadata, or a signed
  request, from one policy type (`concord::SendWith<T>`), without wrapper methods or imports.
- **Following server links.** `Prepared::follow(url)` replaces the URL of a prepared request with
  one the server returned (`Link: rel="next"`, a `next` field), keeping the endpoint's headers,
  handling and identity; another origin is a `Build` error, and `Debug` withholds the URL.
- **Bounded body reads.** `Received::bytes_limited(n)` reads a body (any status) up to `n` bytes,
  `ErrorKind::Limit` beyond: for provider error documents. `Received::decode_limited(n)` decodes a
  finite output with the same bound.

### Changed

- `Error`'s identity is an `Operation`; `Display` of a labelled native response starts with the
  label. `Error::endpoint()` is unchanged for declared endpoints.
- The reader `open` methods take `impl Openable` instead of `reqwest::Response` (a native
  response still works unchanged).
- `IgnoreContentType<F>` forwards `encode_body`, so a runtime media type of `F` is kept.

## Unreleased — 4.4 extensions (candidate)

Additive, not released: the version is unchanged (4.2.0) and no release or version has been
decided. Records: `v4/extensions-real-apis/`.

### Added

- **`concord-pagination`**, a new workspace crate (not published): bounded page and job
  traversal over plain request factories — dependent chains with item checkpoints, independent
  page plans, first-page plans and manifest-discovered streamed jobs, with budgets, a bounded
  window, cancellation by drop and reports. It does not depend on Concord, and core Concord still
  performs no pagination ([README](crates/concord-pagination/README.md)).
- **Incremental bodies in `concord-extras`**, re-exported by the `concord` facade under the same
  feature names:
  - `stream` (`concord::stream`): response opening with unread refusals, byte sources, safe read
    errors and lazily produced upload bodies;
  - `csv` (`concord::csv`): `Csv<HasHeader, Delimiter>`, finite and incremental;
  - `jsonl` (`concord::jsonl`): JSON Lines / NDJSON, finite and incremental, and consecutive JSON
    values.
- `ReadError::is_terminal`: whether a read error ended the read. Only a bad line ended by a line
  feed, reported by a JSON Lines reader in `RecordErrors::Yield` mode, is not terminal; the reader
  then goes on with the next record.
- Five provider SDK examples, each with an external caller example and loopback tests:
  `examples/{github,stripe,shopify,salesforce,kubernetes}-sdk` and `-app`.

## Unreleased — V4.3 candidate (documentation and examples)

No public API, behaviour or version change: the four crates are still 4.2.0 and their code is
unchanged. Records: `v4/dx-4.3/`.

### Documentation

- **Request streams are formats.** The format guide (§1) and the author guide (§5.7) said a
  streamed request body is "not a format". An `Encode<T>` takes its payload by value and may return
  a streaming `reqwest::Body`; only a per-body media type (a multipart boundary) needs the native
  route or a `content-type` header the SDK controls, and only an incrementally read *response*
  needs `send_raw`. The guides now say so and link a tested example.
- The caller guide (§7) points at `decoder.finish(Err(e))` for converting a transport failure when
  you execute a request yourself: reqwest's own error prints the full URL.

### Examples and recipes

- `examples/native`: a non-`Clone` one-shot request stream (`upload.rs`), and response metadata
  together with the typed value after a native edit (`metadata.rs`), with what each failure stage
  keeps.
- `examples/sdk`, `examples/app`, `examples/extension`: a two-service SDK; domain adapters that
  return generated operations, independent per-service outcomes, a named workflow and a bounded
  traversal with a continuation; and an independent crate adding an operation the SDK does not
  declare through an SDK-owned context. `examples/fixture` is their loopback test server.
- `docs/RECIPES.md` indexes the new recipes.

## 4.2.0

A focused maintenance release: compiler correctness and exact advice text, fair benchmarks and
trustworthy maintained checks, and corrected caller documentation. The statement DSL, the four
crates, the codec contracts and metadata policies, request types, `IntoValue` borrowing and the
Rust 1.99.1 minimum and pin are unchanged; no edit is needed except where an expression fragment
forwarded by your own `macro_rules!` into a type or initializer now means what Rust says
([migration notes](docs/MIGRATION.md#from-41-to-42)). Not published on crates.io; the four crates
share one version and require each other exactly (`=4.2.0`).

### Fixes

- **Conditional `Clone` keeps every bound.** Types that two crates' macros write with the same
  tokens (`$crate::Payload`) are different types; the bounds of an endpoint request's conditional
  `Clone` were deduplicated by their printed text, so the bound of a non-`Clone` one could be
  dropped and the declaration failed with E0277 without any `.clone()`. Each value and body keeps
  its own bound: such declarations compile, and cloning a request that holds a non-`Clone` value is
  still rejected.
- **Type macros are output types.** `-> payload!()`, `-> m::payload![]` and `-> ::c::payload! {}` are
  ordinary payload types; they were read as the removed in-place format. The removed spellings
  around a type macro still report their replacement, and a braced type macro that takes the
  endpoint's block gets a direct hint.
- **Exact advice and generated documentation.** Types in diagnostics and generated rustdoc are
  rendered token by token: literal contents are kept (`Payload<{ count("a :: b") }>` was suggested
  as `"a::b"`, changing a six-byte length to four), a raw name stays raw (`body r#type?: String`,
  not `body type?`), punctuation written apart stays apart and a macro's input is shown token for
  token, so applied advice selects the same macro arm. A payload holding a `macro_rules!` fragment
  is named `T` instead of spelled.
- **Forwarded `macro_rules!` fragments keep their grouping.** Value, body and output types and
  value initializers that contain an expression fragment (`Payload<{ $e * 2 }>`, `= $e * 2`) mean
  what Rust says (6 for `$e = 1 + 2`, where 4.1 computed 5, or named different types for the body's
  format and the signature); a forwarded `dyn A + B` behind `&` is accepted.

### Maintenance

- **Fair benchmarks.** The S3 construction pair compared unequal work (only Concord prepared
  `Accept: application/json`); the manual side now prepares it too. Every promised equal-work pair
  is registered with its measured stage and compared before timing by an ordinary test (47
  comparisons, with negative controls for a changed or missing `Accept`, repeated values and their
  order, URL, method and body). The 4.1 S3 ratios stay in `benches/criterion/README.md`, marked as
  unequal work; no corrected figure is derived from them.
- **Examples declare their own features.** `examples/renamed` enables `extras`; the gate tests each
  example package alone (`test-examples-isolated`), so workspace feature unification cannot hide a
  missing feature, and checks that the no-extras consumer really has none.
- **Workflow guard inspects structure.** `scripts/check-ci.py --workflow` parses `ci.yml` (a
  documented YAML subset) and counts only an enabled, blocking gate step on Linux and Windows; a
  commented, echoed, disabled or failure-swallowing invocation no longer satisfies it. It is static
  inspection; CI logs remain the evidence that the gate ran.
- **Fresh package extraction.** `scripts/package-check.sh` and `scripts/consumers.sh --packaged`
  recreate only their own marked output, refuse other directories, never build from stale extracted
  sources, and build one packaged consumer from a deep, non-ASCII path.
- **Retired mutation helper.** The historical mutation helper under `tests-gen/` refuses to run:
  it had no maintained caller and could report a build failure as a kill.
- **Metadata stage tested.** `examples/native` asserts that a 2xx head the format refuses is a
  `Metadata` error before its withheld body is read, with the response recoverable; removing the
  head check fails this test. Lifecycle (cancellation, drop, one request per send) and
  multi-owner composition controls were added.
- **Documentation.** Header text accepts horizontal tab (as `http` does); `into_response()` returns
  the response of a `Status` or a `Metadata` error; runtime bases keep percent escapes and whole
  segments may not come out empty, `.` or `..`; `Text`'s `Content-Type` policy; compiler-dependent
  diagnostics and recursion limits; `&HeaderValue` among the `IntoValue` conversions; short task
  notes (Config, format imports, `Accept`, inference, no output vs `-> ()`, scope reuse, copying
  the Riot example) and a compiled example of moving a default into a scope.
- **Performance experiments (nothing adopted).** An allocation-free long-chain header planner and
  a selective response media scanner were measured and independently reviewed; both are deferred
  until workloads show the cases matter. Records: `v4/release-4.2/PERF.md`.


## 4.1.0

The modular V4: four crates, formats as ordinary Rust types, and response metadata checked before
a body is read. Not published on crates.io; depend on a checkout or a git revision. Rust 1.99.1 is
both the minimum and the pinned toolchain. Upgrade steps are in
[`docs/MIGRATION.md`](docs/MIGRATION.md) (its sections on format types and the codec contract).

### Crates

- `concord`: the facade you depend on; `api!`, re-exports, features `json` and
  `reqwest-defaults` (default), `form`, `extras`.
- `concord-core`: the runtime and the open `Encode`/`Decode` contracts.
- `concord-macros`: the declaration macro. It knows no format: it resolves each format as a type
  path, with the same rules as any other path.
- `concord-extras`: the standard formats (`Json`, `Form`, `Text`, `Binary`) and
  `IgnoreContentType<F>`, written against the public traits only.

The four crates share one version and require each other exactly (`=4.1.0`); update them together.

### Changes

- **Formats are Rust paths.** `format Json;`, `response my::Csv;`: no format words, no privileged
  formats. A format of yours is named and resolved exactly like `Json`.
- **Strict `Json`.** A response must declare `application/json` or an `application/*+json` type,
  with no charset other than `utf-8`/`us-ascii`, or the call fails with `ErrorKind::Metadata`.
  For a server that mislabels its responses, `IgnoreContentType<Json>` skips only that check,
  explicitly, per endpoint or scope.
- **`Accept` is a fallback.** A typed endpoint whose format suggests a media type (`Json`, `Form`)
  sends it only when the declared headers leave no `accept` value. An `Accept` you declare always
  wins; a client-wide `Accept` configured on your `reqwest::Client` does not (declare it in
  `headers` instead). When Concord sends none, reqwest's own default goes out. To stop only the
  suggestion, wrap the format so it forwards every decoding step and suggests nothing
  ([format guide, section 8](docs/CUSTOM_FORMATS.md#8-the-accept-suggestion)).
- **Metadata errors keep the response unread.** A refusal by `check_content_type` or `context`
  happens before Concord reads the body; `Error::into_response()` returns the response with its
  body still available.
- **`Client::validate_header_names()`** checks every native header name an endpoint could send,
  without building a request or any value. Its guarantee is limited: a reported endpoint cannot
  build any request, but `prepare()` returns an earlier failure first (body encoding, host, path or
  query formatting), and a passing check says nothing about values.
- **Decompression stays reqwest's.** Enabling reqwest's `gzip` (or other) feature in your
  application works with every format: the format sees the head reqwest exposes and the unpacked
  body. Concord enables no compression feature itself.

### Windows

No library behaviour differs on Windows. The verification scripts did: `scripts/consumers.sh`
passed about 50 KB of `cargo metadata` output as one command-line argument, beyond Windows'
32,767-character limit, so the gate failed there while passing on Linux. It now passes the data as
a file, and every gate step runs `python`, `cargo` and `tar` behind a wrapper that measures each
command line as Windows would quote it (UTF-16 units) and refuses one over the limit on all
platforms. CI now runs the same complete inventory (packages and benchmarks included) on Linux and
Windows; doing so exposed two more Windows-only script defects, also fixed: GNU tar read the
`D:\...` archive path as a remote `host:path`, and a carriage return from Windows Python ended a
path read with `read`.

### Tests and checks added in this release

- `checks/consumers/facade-gzip`: an application with reqwest's gzip support, against compressed
  fixtures, a withheld body and a decompression-disabled control.
- `crates/concord/tests/accept.rs::no_suggestion_keeps_the_metadata_checks`.
- `scripts/argv-guard.sh` and `scripts/argv-length.py` (with a self-test gate step), installed by
  `scripts/verify.sh`; `scripts/check-ci.py` refuses platform-selected gates and `tar -f NAME`.
