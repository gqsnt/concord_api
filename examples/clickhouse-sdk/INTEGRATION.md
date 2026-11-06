# Integration record — ClickHouse SDK

## Between waves 4-5 (reader wrapping)

Library refinement R5 (`concord-extras`: `check_head` on every reader, `read_for(source,
operation)`, `impl ByteSource for Incoming`, `ReadError::into_source_unredacted` /
`SourceError::into_source_unredacted`), on top of Concord `27d23b39`. Role: implementer, a Claude
agent session spawned by the coordinator (the session's system prompt names model
`claude-opus-5-5`; effort is not visible to the session; the coordinator records requested vs
observed). Not an independent review: nobody has reviewed this change yet.

**Verdict: adopted.** FRICTION entries 2 and 3 are resolved for this SDK, except one `expect`
that stays (below). No public signature changed and no test was edited.

### What changed (`src/client.rs` `open_rows`, `src/rows.rs`, `src/insert.rs`, `src/error.rs`)

- **JSONEachRow head check.** `JsonLinesReader::new(JsonLinesOptions::ndjson()).check_head(..)`
  replaces `<Ndjson as Incremental<IgnoredAny>>::check_content_type(..)` and its comment; the
  same reader then reads the body. CSVWithNames already used `CsvReader::check_head`.
- **Operation named by the reader.** Both readers read the guarded body with
  `read_for(body, operation)` instead of `read(body)`. `ReadFailure`'s `Display` now prints the
  reader error alone (it starts with the operation); before, the SDK prefixed it.
- **The body stays an `Incoming`.** The guard wraps the `Incoming` itself (`Guarded<Incoming>`,
  the `Body` alias of `src/rows.rs`) instead of `incoming.into_response()`; `head_exception` reads
  the `Incoming` too. The SDK no longer takes a native `reqwest::Response` out of a response.
- **Insert: no stash.** The `Body` adapter of `src/insert.rs` (an `Incoming` byte source that
  kept the `concord::Error` in a field; 26 lines with its doc comment) is gone, with
  `Guarded::into_inner`. The insert loop takes the guard's failure apart by value:

```rust,ignore
let transport = error
    .into_source_unredacted()
    .and_then(|source| source.downcast::<concord::Error>().ok())
    .expect("a guard failure without a block is the body's failure");
return Err(Error::Concord(*transport));
```

  The `expect` stays, with the same message: a `SourceError`'s source is a `Box<dyn Error>`, so
  "the guard failed without a block, therefore its source is Concord's error" is still an
  invariant the types do not carry. What changed is what it rests on: before, the SDK's own
  bookkeeping (the adapter set its `failure` field in the same call that returned the error, and
  the SDK fetched it back through `into_inner()`); now, the guard passing the inner failure
  through unchanged (as before) and the library's documented contract that an `Incoming`'s
  failure carries Concord's `Transport` error.
- **Rows: by reference, deliberately.** `Rows::failure` still finds a `BodyException` with
  `source_unredacted()` + `downcast_ref` and copies its message: when the source is not a block
  the `ReadError` is kept whole in `ReadFailure`, and taking the source by value would consume it.
  A by-value path here would need a test, then a take, then an unreachable branch.

### Lines (`git diff -U0`, this file excluded)

| File | added | removed | of which non-blank, non-comment: added / removed |
|---|---|---|---|
| `src/insert.rs` | 9 | 36 | 7 / 31 |
| `src/client.rs` | 12 | 15 | 10 / 13 |
| `src/guard.rs` | 2 | 7 | 0 / 3 |
| `src/rows.rs` | 4 | 2 | 2 / 2 |
| `src/error.rs` | 2 | 1 | 1 / 1 |
| total | 29 | 61 | 20 / 50 |

Net: −32 lines (−30 of code). Tests: no line changed.

### Behaviour changes

Observed with a throwaway probe through the public API (a test printing, for a body broken after
some rows and for a malformed row in each format, the error's `Display`, the `ReadFailure`'s
`read_error()` kind, operation, codec label and underlying error type), run on the SDK code at
`27d23b39` (with the R5 library) and on this change, then removed:

| Case (both formats) | Before | After |
|---|---|---|
| `Display` of `Error::Read` | `ClickHouseHttp.select: reading failed: reading the response body failed (after 7 records, 501 bytes)` (CSV) | identical |
| `read_error().operation()` | `None` | `Some("ClickHouseHttp.select")` |
| underlying error of a broken body (`read_error().source_unredacted()`) | `reqwest::Error` (URL removed) | `concord::Error`, kind `Transport`, naming the operation (URL removed) |
| a malformed row | `Format`, no underlying error | unchanged, except the operation above |

From the code (not probed): the JSONEachRow head check accepts and refuses exactly as before,
with the same labels (the preset NDJSON reader's `check_head` and the static `Ndjson` format apply
the same policy; `crates/concord-extras/tests/reader_wrapping.rs::the_static_format_and_the_preset_reader_agree`);
a broken insert answer is still `Error::Concord` of kind `Transport`; a late exception (tagged
block, head code) is unchanged.

### Tests

None edited, none added: the existing ones cover the paths that moved (broken bodies in both
formats, late exceptions, the insert's transport failure in
`tests/protocol.rs` and `tests/first_use.rs`).

### Gaps

1. **The insert `expect`** (above): a typed way to take a source error back as one type or keep
   it — `ReadError`/`SourceError` `try_into_source::<E>() -> Result<Box<E>, Self>`, say — would
   let both `Rows::failure` (by value, no copy) and the insert loop drop their invariants. Not
   added: one consumer, and the by-reference copy is of a message of at most 16 KiB.
2. FRICTION entries 4 (an enum of boxed readers for a format chosen at run time) and 5
   (`JsonLinesRecords` accessors bound by `T: DeserializeOwned`) are untouched.

### Verification (observed)

- `CARGO_BUILD_JOBS=6 cargo test --locked -p concord-sdk-clickhouse`: 16 unit + 17 first_use +
  26 protocol + 5 workflows + 5 doctests passed, 0 failed; the same counts as at `27d23b39`.
- `cargo clippy --locked -p concord-sdk-clickhouse --all-targets -- -D warnings`: clean.
  `cargo fmt -p concord-sdk-clickhouse -- --check`: clean. `cargo doc -p concord-sdk-clickhouse
  --no-deps` with `-D warnings`: clean.
- Not run: Windows. Offline only; no ClickHouse server.
