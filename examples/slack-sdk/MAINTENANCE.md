# SDK07 maintenance: cursor contains reserved characters

Date: 2026-11-06. Starting SDK HEAD: `31cad62d`; frozen Concord baseline: `e04236ca`.
Observed toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, edition 2024.

Ticket: “opaque cursor includes `=` or reserved characters; ensure exactly-once query encoding
and correct termination.” The inherited implementation already satisfied this contract, including
an existing reserved-character wire test. This maintenance adds regression coverage and clarifies
encoding ownership; it does not change executable SDK behavior or require caller changes.

## First attempt and evidence

1. Inspected the SDK README, provenance, source, test support, protocol/workflow tests, and
   first-use test code. Neither `FRICTION.md` nor `FIRST_USE.md` was opened during maintenance.
2. Traced `response_metadata.next_cursor` through the model deserializer, lazy `users` adapter,
   shared `users_page` request builder, and declared `query cursor?: String`. The adapter copies
   the raw string into Concord's query placement. No local URL encoder or decoder is involved.
3. Ran the requested full test command before editing: 31 integration tests and 4 doctests passed.
4. Added two hand-authored response fixtures, four wire tests, and documentation. The first
   expanded full test run compiled and passed: 35 integration tests and 4 doctests. No compiler
   diagnostics, failed tests, generated-name surprises, or corrective implementation pass occurred.

The new tests compare the complete HTTP request target and independently decoded query pairs.
Expected encoded strings are written explicitly, not computed with the production encoder.
They exercise:

- Manual paging with mixed reserved characters, padding-only `==`, whitespace, and literal
  percent-looking text, including lowercase `%2f`, `%2526`, `&cursor=`, and `#`.
- A lazy chain with three consecutive empty continuing pages, followed by each supported
  terminal shape. Repeated reads after exhaustion send no request. A page cap exactly at the
  terminal page still reports exhaustion.
- A cap after an empty continuing page, retaining the exact raw cursor for explicit manual
  continuation. A subsequent traversal read reports `Ended`, not successful exhaustion.
- A typed `InvalidCursor` error after the reserved cursor reaches the server, retaining both
  the collected user prefix and the exact failed cursor.

## Edits by kind

| Kind | Files and result |
| --- | --- |
| Declaration | No change to `src/wire.rs`; the existing sensitive query placement supplies encoding. |
| Adapter | `src/lib.rs`: public `users_page` rustdoc and an inline comment clarify raw input and encoding ownership. No executable change. |
| Model | No change to `src/models.rs`; only an empty string is filtered out, while Serde options handle null/missing values. Nonempty padding and percent text stay opaque. |
| Tests | Added `tests/cursors.rs` with four integration tests, plus `fixtures/users-cursor-padding.json` and `fixtures/users-cursor-percent-text.json`. Existing tests are unchanged. |
| Docs | Updated `README.md` with caller examples of raw versus encoded cursor text, updated `PROVENANCE.md`, and added this record. |

The new fixtures are compact ASCII JSON with a final LF, served directly with `include_bytes!`.
The existing `.gitattributes` rule `fixtures/** -text` remains unchanged and applies to both files.
All requests use the loopback testserver and synthetic auth; these are fixture checks, not live
Slack verification.

## Independent descriptions and locality

Four runtime sites were inspected: the cursor deserializer, the `users` page projection, the
`users_page` raw-string handoff, and the query declaration. **Zero required semantic edits.**
These are distinct responsibilities, not four encoders: encoding has one owner, Concord's
declared query placement; both public paging paths use the same request builder. The lazy
adapter uses the model's optional cursor without repeating its empty-string test.

Two existing caller-facing descriptions of the encoding rule were clarified (`users_page`
rustdoc and the README); one inline handoff comment was added. Each new fixture cursor is also
spelled independently in its test case alongside an expected encoded value, intentionally
providing a wire oracle. No semantic change required coordinated updates to independently
maintained runtime descriptions of the same fact. The work stayed local to `examples/slack-sdk/`;
the library, dependencies, other examples, and existing caller contracts are unchanged.

## Reading and public API findings

Concord material consulted: root `README.md`; relevant value/setter and wire-format sections of
`docs/AUTHOR_GUIDE.md`; pagination/completion guidance in `docs/CALLER_GUIDE.md`; the query-encoding
note in `docs/RECIPES.md`; `crates/concord-core/README.md`, `crates/concord-extras/README.md`, and
`crates/concord-pagination/README.md`. Generated Concord rustdoc was built and its public
`Endpoint` and `Prepared` pages read. The public `PagesError` fields/accessors were also inspected
in pagination source, and testserver `Recorded`/`Script` documentation was read for wire assertions.
No other provider SDK supplied this maintenance solution.

Official sources re-opened on 2026-11-06: Slack's
[pagination guide](https://docs.slack.dev/apis/web-api/pagination/) (cursor encoding, terminal
conditions, and invalid-cursor errors) and
[`users.list` reference](https://docs.slack.dev/reference/methods/users.list/) (cursor argument).
The added adversarial values are invented, not copied vendor examples; provenance is recorded
per fixture in `PROVENANCE.md`.

No new Concord friction or workaround was encountered: no private reach-through, rebuilt
execution, handwritten operation name, native escape, or library patch was necessary.

## Validation

All Cargo commands below used this prefix:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a
```

- `cargo test --locked -p concord-sdk-slack`: passed, 39 tests total (4 new cursor tests,
  6 unchanged first-use tests, 16 unchanged protocol tests, 9 unchanged workflow tests, 4 doctests).
- `cargo doc -p concord --no-deps --features csv,jsonl`: passed.
- `cargo fmt -p concord-sdk-slack -- --check`: passed.
- `git diff --check`: passed. Both new fixture paths report `text: unset` via `git check-attr`.
