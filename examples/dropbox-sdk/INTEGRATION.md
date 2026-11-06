# Integration record

## Between waves 3-4 (ResponseMeta)

Concord gained `concord::ResponseMeta` (an owned head: status, version, header map, operation;
`Received::meta()`; `Debug` prints header names only; AUTHOR_GUIDE §5.11). Base `92775757`.
Role: implementer, a Claude agent session spawned by the coordinator (model and effort not
re-derived from harness metadata here). Not an independent review.

**Verdict: adopted as a thin wrapper.** `src/error.rs` now has
`pub struct ResponseMeta(pub concord::ResponseMeta)` with `Deref<Target = concord::ResponseMeta>`
and a `Debug` that delegates; `request_id()` and `retry_after()` stay inherent methods, reading
`self.headers()`. `DropboxPolicy` captures with `ResponseMeta(received.meta())`. Decisive: the
SDK adds two accessors that callers use throughout (`page.meta.http.request_id()` in the README,
`error.meta().map(|m| m.retry_after())`). A plain re-export cannot carry them (no inherent
methods on another crate's type), and an extension trait would make every caller import it. The
wrapper keeps both calls unchanged and takes Concord's storage, capture and `Debug`.

Alternatives weighed:

- Not adopting: the SDK's `Debug` already printed header names only, so there was no leak to
  fix. Rejected: the wrapper is 10 lines shorter, adds `version()`, and `&ResponseMeta` now
  coerces to `&concord::ResponseMeta` for code written against Concord's type.
- Explicit delegating methods instead of `Deref` (`status()`, `version()`, `headers()`,
  `operation()`, three lines each): 6 more lines than the `Deref` impl (net -4 instead of
  -10). `Deref` on a non-pointer is against the API guidelines' C-DEREF; it is accepted here
  because the wrapper adds methods and no invariant, so reaching the inner type's methods
  cannot bypass anything.

### Lines (non-blank, non-comment, from `git diff`; this file excluded)

| | removed | added |
|---|---|---|
| `src/` | 24 | 14 |
| `tests/` | 30 | 31 |
| `README.md` | 0 | 0 |

`src/` additions: the one-field struct, the `Deref` impl, the one-line delegating `Debug`
body, two import lines, `ResponseMeta(received.meta())`, and four field → method edits in the
SDK's own reads (`self.headers()`, `meta.status()`).

### Caller-visible changes

- Fields become methods (through `Deref`), on `Response::meta`, `FolderPageMeta::http`,
  `HttpError::meta` and `Error::meta()`:

  ```rust,ignore
  assert_eq!(first.meta.status, reqwest::StatusCode::OK);   // before
  assert_eq!(first.meta.status(), reqwest::StatusCode::OK); // after
  ```

  Likewise `headers()` and `operation()`. New: `version()`. Unchanged: `request_id()`,
  `retry_after()`.
- `meta.0` is the `concord::ResponseMeta`; `&meta` coerces to `&concord::ResponseMeta`. The
  wrapper is built as `ResponseMeta(concord::ResponseMeta::new(head, operation))`; the former
  struct literal no longer compiles. No test or README built one.
- `Debug` (from the code): Concord's form, `ResponseMeta { status: 200, version: HTTP/1.1,
  header_names: [..], operation: Some("DropboxApi.selected.list_folder") }`; before, no
  `version`, and the operation as `Some(Operation(POST DropboxApi.selected.list_folder))`.
  Header names only, as before. `Display` is unchanged.

### Test edits, by kind

1. Field → method, mechanical (30 lines in `tests/first_use.rs`, `tests/maintenance.rs`,
   `tests/protocol.rs`, `tests/workflows.rs`): `.status` → `.status()`, `.operation` →
   `.operation()`. Expected values unchanged; the `request_id()` / `retry_after()` assertions
   needed no edit.
2. Added, one assertion
   (`tests/protocol.rs::root_listing_and_continuation_preserve_exact_wire_and_metadata`): `Debug`
   of `first.meta` does not contain the `x-dropbox-request-id` value `synthetic-a-first`.

### Verification (observed)

`CARGO_BUILD_JOBS=6 cargo test --locked -p concord-sdk-dropbox`, run twice: `first_use` 18,
`maintenance` 5, `protocol` 18, `workflows` 7, doctests 2 passed, 0 failed (the same counts as at
`92775757`). `cargo clippy --locked --all-targets -- -D warnings`, `cargo fmt --check` and
`cargo doc --no-deps` (no warnings): clean. Offline only.
