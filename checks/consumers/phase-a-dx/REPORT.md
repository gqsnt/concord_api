# First-use consumer report

## 1. Account summary — prediction before implementation

`GET /accounts/{id}/summary`, `Accept: application/json`, no body. I expect a decoded summary and both diagnostic headers on success, and an error carrying both headers on malformed JSON, wrong media type, or non-2xx; the latter should also expose the typed error document.

## 2. Runtime CSV — prediction before implementation

`POST /jobs/{id}/rows` with `Content-Type: text/csv; header=present` or `text/csv; header=absent`, and rows serialized using the descriptor's delimiter and header setting. I expect the source to be polled only as the request is sent, with success or a transport/encoding error returned by the operation.

## 3. Stream and archive — prediction before implementation

`GET /entries` with a JSON Lines accept header, then `POST /archive` with a CSV content type and transformed rows as a streamed body. I expect a malformed entry to identify the download operation, and successful completion to require observing the download's true EOF, not merely starting or stopping the upload.

## 4. Invoices — prediction before implementation

`GET /invoices`, followed by `GET /invoices?cursor=...` as directed by each JSON page, with `Accept: application/json`. I expect a traversal that distinguishes provider exhaustion, a page cap with a resume cursor, and a typed page failure while preserving received invoices.

## Trace and findings

The predictions above were written before implementation. I read the README, `docs/QUICKSTART.md`, `docs/CALLER_GUIDE.md`, `docs/RECIPES.md`, the pagination README, and generated crate rustdoc. I generated rustdoc with `CARGO_BUILD_JOBS=6 cargo doc -p concord -p concord-pagination --no-deps`, then with `-p concord --no-default-features --features csv,jsonl --no-deps`. I read no implementation source or example source. The manifest shape came from `checks/consumers/streaming/Cargo.toml`, as requested. The guides include excerpts from examples and tests; I read those excerpts as documentation, not the linked files.

### 1. Summary: actual, trace, friction

1. The first public entry point was the README's generated endpoint `.send()`. The README and quick start explain typed decoding and status errors; the caller guide describes status, metadata, and decoding errors.
2. The recipes' “Response metadata with the typed value” describes the `prepare`/`into_parts` native route. The rustdoc for `Prepared::receive` and `Received` showed a shorter route: `prepare()?.receive().await`, copy headers from `head()`, then consume the response. The copied head survives a decoding failure.
3. Rustdoc under the requested features showed that `Json` is not exported: `csv` and `jsonl` do not enable the separate `json` feature. The `Decode` and `CodecError` rustdocs supplied the contract for `LedgerJson`, which checks for a single `application/json` content type and uses `serde_json`.
4. On non-2xx, `Received::into_response` exposes the unread native reqwest response; its bytes are decoded as `Problem`. This is the only native reqwest escape used. On 2xx, `Received::decode` preserves the declared operation and error category. There were no compiler errors; the initial check warned about an unused `StreamExt` import, which I removed.

The predicted request was observed, including the encoded path segment and `Accept`. Success returns `(Summary, Diagnostics)`; bad JSON yields `Decode`, wrong content type yields `Metadata`, and non-2xx yields the typed service problem, all with copied headers. A failure before a head arrives has no diagnostics; a malformed service error body has an `InvalidServiceBody` error with headers.

The metadata recipe emphasizes native disassembly, while `receive` makes this common path simpler. Put the `receive` pattern first in that recipe. The specified CSV/JSONL feature selection excludes the finite JSON format, so consumers combining those with ordinary JSON must implement or enable another format. Document that feature combination prominently; this local decoder intentionally accepts only `application/json`, unlike the standard format's broader JSON media types. A documented typed status-body helper retaining the head would also reduce adapter code.

### 2. Runtime CSV: actual, trace, friction

1. First public entry point: `CsvWriter::new` in CSV rustdoc, after the caller guide's streams section. `CsvOptions::new` takes a runtime header mode and delimiter.
2. `CsvWriter::upload` wraps `StreamRows`; `stream::from_iter` supplies the one-shot iterator. `StreamRows::with_columns` makes the header explicit, including for an empty source. The `Upload` and `Encoded` rustdocs show `body rows: Upload` and `request Encoded`, keeping the media type paired with the body.
3. `prepare` left the source counter at zero; sending consumed it once. No native request escape, compiler error, source or example intervention. The only compiler warning in the overall run was the unused import described above.

Semicolon/header-present and pipe/header-absent requests had exactly the predicted content type and matching bytes. The writer validates dialect configuration and late serialization/source failures surface during the transfer; bytes already sent are not rolled back.

The writer rustdoc still says the SDK must apply the runtime content type to a native request, whereas `Upload` and `Encoded` describe the declarative route. Update `CsvWriter` to lead to `request Encoded` and reserve `Upload::apply` for native requests. Explicit columns are easy to overlook when headers are required; add an end-to-end runtime-dialect example alongside the writer.

### 3. Stream and archive: actual, trace, friction

1. First public entry point: `-> stream Entry` and `.send()` from caller guide §6a. The `JsonLines` and `JsonLinesRecords` rustdocs specify media checking, lazy reading, operation-labelled errors, and completion.
2. The `RecordStream` rustdoc describes mapping into `StreamRows`. I used an `unfold` adapter around the reader so the upload polls one download record at a time, converts cents to a decimal dollar string, and records the reader's completion and error in shared state. `CsvWriter` and `Encoded` produce the POST without collecting the download.
3. A malformed line and a response closed before its declared content length both propagated a `Ledger.entries` read failure and completion other than `EndOfInput`. Clean end returned `EndOfInput`; the caller can also observe `Reading` if the upload ends before EOF. No native request escape, compiler error, source or example intervention.
4. The first test run failed because my fixture unwrapped a send to a dropped observation channel; the server died before writing responses. Ignoring that send error fixed the fixture; it was not a library failure.

The requests used `Accept: application/jsonl` and `Content-Type: text/csv; header=present`. The CSV had converted values `1.20` and `-0.08`. `ArchiveStatus` exposes completion; errors retain it and the operation-labelled download failure. Upload success alone does not prove download EOF.

Moving a reader into an upload source moves its directly observable completion with it. Preserving completion and the original read error across a failed HTTP body requires the shared-state adapter rather than just the documented `map`. Add a documented end-to-end adapter or upload result exposing the source's final completion and error. A partially transmitted archive may already exist after failure; retry and cleanup are application policy.

### 4. Invoices: actual, trace, friction

1. First public entry point: `Pages::new` in the pagination README, plus `PagesError`, `Incomplete`, and `Resume` rustdoc. The adapter clones the client, sets `maybe_cursor`, sends the generated operation, and projects the envelope into `Page::new`.
2. `items().collect_complete()` returns the whole list only on provider exhaustion; its incomplete result owns the prefix and either `Bounded { resume }` or `Provider { error, failed }`. `invoices_from` accepts the returned `Resume` for continuation.
3. Tests exercised two pages, a cap and continuation, and a 503 on page two. The 503 remains a typed `concord::Error` with status; the prefix and failed cursor are retained. No native escape, compiler error, source or example intervention.

Requests and outcomes matched the prediction. A cap after the first page returned cursor `next`, and continuing fetched the remaining invoice. Provider failure returned the previously received invoice, failed cursor `bad`, and a status error. An exact cap at the last page is documented as exhaustion rather than a bound.

The projection closure and resume-entry wrapper are application code that each SDK must write, and `Incomplete` is easier to discover in the pagination README than in general recipes. Add a linked SDK example with a generated request, optional cap, typed provider failure and a resume call.

## Verification

`cd checks/consumers/phase-a-dx && CARGO_BUILD_JOBS=6 cargo test` passed **9 tests**: 0 library unit tests and 9 integration tests. It also ran **0 doctests**. An offline run used a lockfile seeded from the repository lockfile and passed. All HTTP interactions use the local Tokio HTTP/1.1 fixture, not a live service.
