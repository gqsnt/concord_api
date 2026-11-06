# Integration record

## Between waves 3-4 (ResponseMeta)

Concord gained `concord::ResponseMeta` (an owned head: status, version, header map, operation;
`Received::meta()`; `Debug` prints header names only; AUTHOR_GUIDE §5.11). Base `92775757`.
Role: implementer, a Claude agent session spawned by the coordinator (model and effort not
re-derived from harness metadata here). Not an independent review.

**Verdict: adopted (re-export).** `src/lib.rs` now has `pub use concord::ResponseMeta;` in place
of its own struct; `send` captures with `received.meta()` instead of a struct literal. Decisive:
the SDK type derived `Debug` over its `HeaderMap`, so `Debug` of `Response`, `PageMeta` and
every `Error` carrying metadata printed every response header value. Concord's type prints
names only, and is otherwise the SDK's three fields plus `version`.

### Lines (non-blank, non-comment, from `git diff`; this file excluded)

| | removed | added |
|---|---|---|
| `src/` | 14 | 5 |
| `tests/` | 38 | 45 |
| `README.md` | 0 | 0 |

`tests/` adds more than it removes because `rustfmt` re-wrapped two assertions that no longer
fit the line width after `()`.

### Caller-visible changes

- Fields become methods, on `Response::meta`, `PageMeta::http` and the `meta` of
  `Error::Http` / `Error::Response`:

  ```rust,ignore
  assert_eq!(response.meta.headers["x-request-id"], "synthetic-request");   // before
  assert_eq!(response.meta.headers()["x-request-id"], "synthetic-request"); // after
  ```

  Likewise `status()` and `operation()`. New: `version()`.
- The type is now `concord::ResponseMeta` (the same type as in the other adopting SDKs). It
  cannot be built by struct literal; `ResponseMeta::new(head, operation)` builds one. No test or
  README built one.
- `Debug` (from the code): `ResponseMeta { status: 200, version: HTTP/1.1, header_names: [..],
  operation: Some("LinearApi.issue") }`; before, the derived form printed
  `headers: {"x-request-id": "synthetic-request", ..}` and `Some(Operation(POST LinearApi.issue))`.
  This reaches `Debug` of `Response<T>`, `PageMeta` and `Error`. `Display` is unchanged.

### Test edits, by kind

1. Field → method, mechanical (38 lines in `tests/first_use.rs`, `tests/protocol.rs`; two
   re-wrapped by `rustfmt`): `.status` → `.status()`, `.headers` → `.headers()`, `.operation` →
   `.operation()`. Expected values unchanged.
2. Added, one assertion (`tests/protocol.rs::lookup_wire_and_partial_diagnostics`): `Debug` of
   `response.meta` does not contain the `x-request-id` value `synthetic-request`.

### Verification (observed)

`CARGO_BUILD_JOBS=6 cargo test --locked -p concord-sdk-linear`, run twice: `first_use` 18,
`protocol` 8, `workflows` 5 passed, 0 failed, 0 doctests (the same counts as at `92775757`).
`cargo clippy --locked --all-targets -- -D warnings`, `cargo fmt --check` and `cargo doc --no-deps`
(no warnings): clean. Offline only.

## Final migration (open issue W3-13)

Open issue W3-13 in `v4/sdk20/STATUS.md`, found by the independent first-use consumer
(`FIRST_USE.md`, trace entries 5-6). Base `47c6c5b0`. Role: implementer, a Claude agent session
spawned by the coordinator (model and effort not re-derived from harness metadata here). Not an
independent review.

**Defect.** The README says a missing connection returns `Error::Page` with the entire response,
diagnostics included. A GraphQL error that nulls the connection (`data.issues: null` with an
error whose `path` is `["issues"]`) instead failed to decode, because `IssuesData::issues` was a
non-optional `Connection`. The caller got `Error::Response` (`ErrorKind::Decode`) with HTTP
metadata only; the GraphQL errors and extensions were lost.

**Fix (follows the README; no new type or variant).** `IssuesData::issues` becomes
`Option<Connection>` with the SDK's existing `required_nullable` deserializer, the same shape as
`IssueData::issue`. `issue_page` returns `Error::Page { reason: "connection is null", .. }` for a
null connection, alongside the existing `"connection data missing"` for absent or null `data`.
The README and the rustdoc of `IssuesData::issues` and `Error::Page` now spell out what a missing
connection is.

| Response (HTTP 200) | Before | After |
|---|---|---|
| `data.issues: null` + error at `["issues"]` | `Error::Response`, `Decode`; diagnostics lost | `Error::Page`, `"connection is null"`; `envelope.data.issues == None`, errors, locations, extensions, headers kept |
| `data.issues: null`, no errors | `Error::Response`, `Decode` | `Error::Page`, `"connection is null"` (still an error, never an empty page) |
| `data: {}` (absent `issues` member) | `Error::Response`, `Decode` | unchanged: not a partial-data shape; an absent selected member is a decode error |
| `data` absent or null; connection with data and errors | `Error::Page`; partial `Page` | unchanged |

Caller-visible: code that reads the connection out of an `Error::Page` response changes from
`data.issues.nodes` to `data.issues.unwrap().nodes` (or a match). `LinearApi::issues` returns the
same `Envelope<IssuesData>`, now with the optional member. `Page`, `PageMeta`, `Pages` and the
success path are unchanged.

### Lines (non-blank, non-comment, from `git diff`; this file excluded)

| | removed | added |
|---|---|---|
| `src/` | 13 | 16 |
| `tests/` | 23 | 127 |
| `README.md` | 1 | 1 |
| `fixtures/` | 0 | 1 (new file) |

### Test edits, by kind

1. **Consumer assertion that pinned the defect, changed to the corrected behaviour**
   (`tests/first_use.rs`). The consumer's `missing_connections_are_decode_failures_without_the_graphql_envelope`
   asserted `Error::Response`/`Decode` for both `first-use-connection-null.json` and its in-memory
   variant without the `issues` member. It is renamed
   `null_connection_keeps_diagnostics_and_absent_connection_is_a_decode_failure`. The null
   connection now asserts the README's `Error::Page` with HTTP status, headers and operation, a
   null `issues`, and the diagnostic's message, path and extension code (the consumer's original
   prediction). The absent-member variant keeps its decode-failure assertions unchanged. The
   fixture bytes are unchanged. `FIRST_USE.md` is not edited, so its reproduction command still
   names the old test.
2. **Mechanical, field became optional** (one line each): `data.unwrap().issues.nodes[0]` →
   `data.unwrap().issues.unwrap().nodes[0]` in `tests/protocol.rs::repeated_cursor_is_refused_with_data`
   and `tests/first_use.rs::invalid_continuations_keep_the_received_page_and_stop`. Expected
   values unchanged.
3. **Added wire test** `tests/protocol.rs::null_connection_is_a_page_error_that_keeps_its_diagnostics`
   with the new author fixture `fixtures/connection-null.json`. Over the loopback server, a
   traversal gets a good first page, then a null connection. It checks the following:
   - `PagesError::Provider` with resume at `cursor-one`, `Error::Page` with reason
     `"connection is null"`, then `Ended` with `Outcome::Failed`.
   - The status, the response header, the error's message, path, location and code, and the
     top-level extensions are kept.
   - A null connection without errors is still `Error::Page`; `{"data":{}}` is still
     `Error::Response` with `ErrorKind::Decode`.
   - The cursor was sent in the second request, and the fixture's location (column 46) points at
     `issues` in the document sent.

No other assertion or expected value changed. `FRICTION.md` and `MAINTENANCE.md` are not edited;
their `src/lib.rs` line references are not refreshed here.

### Verification (observed)

`CARGO_BUILD_JOBS=6 cargo test --locked -p concord-sdk-linear`, run twice: `first_use` 18,
`protocol` 9 (was 8), `workflows` 5 passed, 0 failed, 0 doctests.
`cargo clippy --locked -p concord-sdk-linear --all-targets -- -D warnings`, `cargo fmt --check`
and `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`: clean. The new and changed tests were not
run against the pre-fix SDK, where they do not compile (`issues` was not an `Option`); the
defect itself is the consumer's observation at the base. Offline only.
