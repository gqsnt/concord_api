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
