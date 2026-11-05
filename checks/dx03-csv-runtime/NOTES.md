# Caller notes

## Documentation consulted

I read these files or the relevant sections of them (source files below for their public documentation and signatures):

- `docs/QUICKSTART.md`: answered how to declare an endpoint, configure a base URL, and retrieve an unread error response; it does not explain runtime CSV.
- `docs/CALLER_GUIDE.md`: answered the difference between `send`, `send_raw`, `into_parts`, and Concord's error stages. It does not show the runtime CSV reader.
- `docs/CHEATSHEET.md`: confirmed formats can be type paths and summarized prepared requests; it did not give the runtime CSV workflow.
- `docs/RECIPES.md`: explained capturing metadata before decoding and editing a prepared native request. The general streaming recipe describes reading raw chunks, not the CSV reader, so I needed the CSV module docs as well.
- `crates/concord/src/lib.rs`: confirmed the facade, feature-gated modules, generated request methods, and native-request escape hatch.
- `crates/concord/Cargo.toml`: identified `csv` and the features it enables.
- `crates/concord-extras/src/csv/mod.rs`: answered the fixed policy syntax, default media checks, row mapping, fixed versus incremental routes, and the CSV limitations. This was the most useful overview.
- `crates/concord-extras/src/csv/options.rs`: answered how to construct runtime options, choose a header mode, specify column mapping, and validate a dialect.
- `crates/concord-extras/src/csv/read.rs`: answered how `open_for` checks a response without consuming refusals, and how `next` reads incrementally. Its `Chunks` example was useful.
- `crates/concord-extras/src/csv/write.rs`: answered how to build a runtime writer, use explicit columns, and create an upload; it also explains late errors.
- `crates/concord-extras/src/stream/mod.rs`: explained the open/read split, typed error categories, positions, and upload pairing.
- `crates/concord-extras/src/stream/error.rs`: answered exactly how to inspect/take the unread response and find the count of delivered records.
- `crates/concord-extras/src/stream/upload.rs`: answered how `Upload::apply` attaches both body and header, and how `from_iter` supplies a source without another dependency.
- `crates/concord-extras/src/stream/source.rs`: answered how to test a reader with chunk boundaries and an injected failure without a socket.
- `examples/testserver/src/lib.rs`: answered how to start/shut down the server, observe requests, send delayed chunks, and interrupt a response body.

## Compiler diagnostics and adaptations

There were **no compiler errors**. The first integration-test compilation emitted the following warning, which correctly identified the missing crate-level test documentation; I added a crate doc comment:

```text
warning: missing documentation for the crate
   --> checks/dx03-csv-runtime/tests/exports.rs:1:1
    |
  1 | / use std::time::Duration;
  2 | |
  3 | | use concord::{
  4 | |     ErrorKind,
...   |
189 | |     assert_eq!(server.shutdown().await.open_after, 0);
190 | | }
    | |_^
    |
    = note: requested on the command line with `-W missing-docs`
```

`ReportCsv` is an alias for the declaration's generic CSV policy. For a job without headers, I had to translate the descriptor's column names into `ReadMapping::columns`; for a job with headers I instead use `options.columns` to validate the header. A descriptor delimiter arrives as a string, so the client checks it is exactly one ASCII byte and lets `CsvReader`/`CsvWriter` validate dialect conflicts. `from_iter` adapts the owned rows to the writer's one-shot stream, and `Ok::<_, Infallible>` supplies its otherwise ambiguous source error type. The upload requires `prepare().into_parts()`, applying the paired body/header to the native request, then `decoder.finish` for Concord's response/error handling.

## Observations and limitations

- The CSV module documents that a contradictory `header=absent` is a metadata refusal, with the response unread. The integration test asserts that result and reads the original body, but the server could not start in this environment, so this is not an observed wire result here.
- `CsvRecords` owns the response. I copy status and selected header values before opening it; a failed open instead exposes the entire unread response through `OpenError`. A later `ReadError` contains its kind and delivered-row position, while the independently stored metadata remains available on `ExportDownload`. The consumed body cannot be recovered as unread after a read failure. No response metadata exists to copy when transport fails before the response head. Only the selected headers are retained once reading starts.
- Runtime uploads cannot use a static format's single Content-Type. `Upload::apply` is a convenient way to avoid setting a header independently of the body. A lazy upload can fail after sending starts; `decoder.finish` reports the request failure as a Concord error, rather than directly returning an `UploadError` at the top level. Its transport details are available through Concord's explicit accessor; this example does not recover a typed late upload error.
- Explicit writer columns validate named field order rather than rearranging struct fields. A descriptor with a different order cannot be uploaded using this `ReportRow` as-is; the error may arise when the lazy body is polled. Headerless reads can map reordered descriptor columns successfully.
- The default writer uses CRLF and the media type describes header presence, not the delimiter. The descriptor supplies the delimiter to the peer separately.
- Opening/reading and preserving refusals required more steps and two distinct error types compared with a simple `.send()`, but the APIs and documentation made the ownership boundary explicit. Static CSV declaration, option construction, and the byte-source test were straightforward. The CSV overview is much more directly useful for this task than the general streaming recipe.

## Verification in this environment

The socket-free library tests pass and check runtime mapping across chunks, an injected failure after two delivered rows, and finite writer bytes/media types for both header modes. The integration tests compile, but this environment denies the test server's loopback bind (`Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }`); all six therefore fail at server startup, before testing HTTP. Consequently the HTTP metadata, refusal, streaming timing, and upload wire assertions remain unverified here. Run the requested full test command in an environment that permits loopback sockets.

## Second round

The coordinator's loopback run passed five integration tests but failed `read_failures_report_delivered_rows`. This was my test fixture's routing error: the client requests `/jobs/broken/export`, so `ends_with("broken")` was false and the server returned the bad-row fixture (`id|amount\n1|35\n2|bad\n`) instead of the interrupted body. The reported decode error at record 2, field 2, after one delivered row matches `bad` in that fixture; the test was still expecting two valid rows in the transport-failure case. I changed the responder to match both complete paths explicitly and reject unexpected paths. This evidence points to the test, not the caller or a library defect. The corrected HTTP test still needs a loopback-enabled run for confirmation.
