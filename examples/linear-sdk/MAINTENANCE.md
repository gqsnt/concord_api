# Nullable field and partial-data error maintenance

Ticket: add a nullable selected field plus a partial-data error in the fixture, and verify the
diagnostics are kept. The maintainer did not write this SDK. The requested runner was a Claude
Opus subagent. This file does not attest to the runner model; the coordinator records that from
harness metadata.

The new field is Linear's `Issue.dueDate: TimelessDate`, which is nullable in Linear's public SDK
schema (source and caveats in `PROVENANCE.md`). It is appended after `description` in both
documents and modelled as `Issue::due_date: Option<String>`. The SDK passes the value through as
returned and does not parse it. Like `description`, it must be present in the response, even when
null. An absent key is a decode error, not a null.

## Edits by kind

- **Concord declaration (`concord::api!`):** none. The endpoint, request and response codecs and
  the generated methods are unchanged.
- **GraphQL documents (SDK constants):** `LIST` and `ISSUE` in `src/lib.rs` both select `dueDate`.
  This changes what callers see on the wire (see "Caller-visible changes" below).
- **Adapter:** none. `send`, `issue_page` and `issues` are unchanged. The existing partial-page
  path already puts `errors` and `extensions` in `PageMeta` and keeps following the cursor.
- **Model:** `Issue::due_date` uses `#[serde(rename = "dueDate", deserialize_with =
  "required_nullable")]` and reuses the existing deserializer. The struct rustdoc now says that
  only an error `path` tells a failed field apart from a null value.
- **Fixtures:** two are new, `page-due-date-partial.json` and `issue-due-date-partial.json`. Ten
  existing fixtures gained a `dueDate` member after `description`: four author fixtures (`page-one`,
  `page-partial`, `issue`, `issue-partial`, with date values) and six first-use fixtures (with
  `null`). No other byte changed, and `.gitattributes` is unchanged (`fixtures/** -text` covers the
  new files).
- **Tests:** two new wire tests.
  - `tests/workflows.rs::due_date_field_error_is_attributed_by_path_and_traversal_continues` runs
    a non-terminal partial page with three issues: a due date, a legitimate null, and a null with a
    field error at `["issues","nodes",2,"dueDate"]`. It checks the following:
    - The message, the full path with index, the location, the extension code, an extra error
      member and the top-level extensions are all kept.
    - Only the path identifies which null failed.
    - The opaque cursor `due/cursor+=?&` is sent unchanged in the next request's variables.
    - The second page keeps its own `description` diagnostic.
    - The fixture locations point at `dueDate` in the documents actually sent.
  - `tests/protocol.rs::due_date_field_error_keeps_data_diagnostics_and_presence_rule` runs a
    lookup with `dueDate: null`, an error at `["issue","dueDate"]` and the description present. It
    checks the following:
    - `is_success()` is false.
    - The data, the path, the location, the code, the response header and the extensions are kept.
    - The exact request bytes match.
    - A response that omits `dueDate` gives `Error::Response` with `ErrorKind::Decode`.

  In existing tests, four expected request documents changed (two in `protocol.rs`, two in the
  first-use `wire` helpers), and `fetch_issue_selected_fields` gained one `due_date` assertion. No
  other existing assertion changed.
- **Docs:**
  - `README.md`: the selection sentence; the rule that an absent field is a decode error; how to
    attribute a null through the error path; a partial page keeps its cursor; and a link to this
    file.
  - `PROVENANCE.md`: two rows and a `dueDate` source paragraph.
  - `FRICTION.md`: thirteen `file:line` references moved by this edit, and new entry 6.
  - `FIRST_USE.md`: one labelled note appended. The consumer's trace is left as written.

## Caller-visible changes and why

GraphQL returns only the fields a request selects, so delivering `dueDate` requires a changed
request document. Any fake that pins request documents must change with it: the author's tests
compare exact bytes, and the consumer's tests compare the document after the operation name,
ignoring whitespace. A GraphQL response contains every selected field (null on a field error), so
the old fixtures are no longer valid responses to the new request. The SDK rejects an absent
selected field.

Making the field `#[serde(default)]` was the alternative. It would have kept the old fixtures
decoding unchanged, but an absent field would then read the same as null. That contradicts the
README's presence rule, and it contradicts the consumer's expectation that a missing selected
field fails decoding. It was rejected.

`Issue` is a public struct with all fields public and without `#[non_exhaustive]`. Adding a field
therefore breaks callers that build it with a struct literal or destructure it exhaustively. No
caller in this repository does either. Making the struct `#[non_exhaustive]` would be an API
design change and was not made.

## What happened, in order

1. **Source only** (two documents and the model): compiled with no warnings. 24 of 29 tests failed;
   the 5 that passed decode no issue object and check no request document. The failures:
   - 8 first-use tests failed at the `wire` document comparison (`tests/first_use.rs:86`).
   - 16 tests failed by decoding `missing field dueDate` from old fixtures: 9 first-use, 3
     protocol, 4 workflows. These failures came before the two exact-body assertions in
     `protocol.rs`, so those two assertions were hidden until the fixtures were fixed.
2. **Ten fixtures and four expected documents:** 29 of 29 passed.
3. **New fixtures and the two tests:** 31 of 31 passed.
4. **Two temporary mutations of `src/lib.rs`**, each reverted and checked byte-equal afterwards.
   - `#[serde(default)]` in place of `required_nullable`: only the new presence assertion failed.
   - Removing `dueDate` from `LIST` only: the code compiled and every fixture still decoded,
     because the fixtures contain the key. 11 tests failed, all on request-side checks: 9 at the
     first-use `wire` comparison (`tests/first_use.rs:86`), the exact body (`tests/protocol.rs:86`),
     and the location check in the new workflow test (`tests/workflows.rs:200`).
5. `cargo fmt` rewrapped two of the new assertions. `cargo clippy --all-targets -- -D warnings` is
   clean.

## How many places encode the same fact

The fact is "the issue selection is `id title description dueDate`, and `dueDate` is a nullable
field that must be present". It had to change in **20 independently maintained places, across 15
existing files**:

- **3 production encodings with no compiler link between them:** the `LIST` document, the `ISSUE`
  document and the `Issue` model. Only tests that pin the request side catch a disagreement
  (mutation 2 above).
- **3 prose descriptions:** the README selection sentence, the `Issue` rustdoc and the
  `PROVENANCE.md` scope.
- **4 request expectations in tests,** written by two roles in two styles: the author's exact
  bytes, and the consumer's whitespace-insensitive document tail.
- **10 response fixtures,** each a model of the response to the selection.

The two new fixtures add a hidden fifth kind of encoding: GraphQL `locations` are column offsets
into the sent document (114 in `LIST`, 67 in `ISSUE`). The new tests assert that these offsets
point at `dueDate`. The existing locations are synthetic and nothing checks them; for example,
`first-use-page-partial.json` says column 95, but `description` is at column 102. Appending the
field after `description` kept every existing offset unchanged. Inserting it earlier would have
silently moved them, without any test failing.

## What had to be read to find them

- **The SDK:** `src/lib.rs`, `README.md`, `PROVENANCE.md`, `Cargo.toml`, `.gitattributes`, the
  three test files and all 17 fixtures. A text search for `description` across the crate found
  every encoding listed above.
- **Format reference:** `examples/gitlab-sdk/MAINTENANCE.md`. It supplied no part of the solution.
- **Linear's schema:** the public SDK schema, through five WebFetch reads at different offsets of
  the 1.4 MB file. This was the only network access, and no Linear API call was made.
- **Not needed:** Concord's docs and rustdoc, because no Concord surface was touched.

`FRICTION.md` and `FIRST_USE.md` were read only after the first attempt passed. One exception
happened earlier: a workspace-wide text search, meant to exclude this crate, printed 12 lines of
`FIRST_USE.md`. Those lines showed the consumer's predicted selections and its finding about
missing-connection diagnostics. That finding is also stated in the `tests/first_use.rs` comment
read before the search. The exposure did not change the approach.

## Generated-name or compile surprise

None. The `concord::api!` declaration and the generated names were not involved, and the first
build had no errors or warnings. The surprises were in testing, not compiling:

- Agreement between the documents and the model is enforced only by request-pinning tests.
- The location offsets in fixtures are a hidden dependency on the order of the selection.

## Did it feel local?

At the crate level, yes. The change needed nothing from Concord, the adapter or the declaration.
Within the crate, no. One field added to a selection touched 20 places and broke 24 of 29
existing tests, including 17 of the 18 first-use consumer tests. The breakage was loud and easy to
classify (two failure kinds), and no test passed silently.

Two options would reduce this next time. Both are owner decisions and neither was adopted:

- A single `macro_rules!` field list shared through `concat!` by both documents would turn three
  production encodings into two.
- `#[non_exhaustive]` on `Issue` would make future field additions non-breaking for callers.

## Left as is

The consumer's finding still stands: a partial-data error that nulls the connection itself
(`data.issues: null`) is a decode error, and its diagnostics are lost. A leaf such as `dueDate` is
not affected, because its error and the surrounding data are both kept. Fixing the connection case
would change `Error::Page` semantics and is outside this ticket.
`first_use.rs::missing_connections_are_decode_failures_without_the_graphql_envelope` still
reproduces it.

## Verification

`CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=…/work/target-b cargo test --locked -p concord-sdk-linear`
gives 18 first-use, 8 protocol and 5 workflow tests: **31 passed, 0 failed**. No live provider
calls were made and only synthetic credentials were used.
