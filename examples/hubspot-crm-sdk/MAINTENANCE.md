# Maintenance record: contact batch read

Date: 2026-11-07. SDK commit `a171f3779a7d139204a8bf61d3341df43edff0de` on the frozen library
baseline `8c27cd1a`. Role: independent maintainer, not the author or first consumer. Live provider
calls: **NOT_RUN**; every request goes to `examples/testserver` on loopback. This record lists
observable actions, files read and decisions. It contains no private reasoning transcript.

## Prior exposure

Read **before and during** the change:

- The maintenance capsule (ticket and rules), plus the user/project instruction files.
- This package: `Cargo.toml`, `README.md`, `PROVENANCE.md`, `src/lib.rs`, every fixture,
  `tests/workflows.rs`, `tests/protocol.rs`, and the existing consumer `tests/first_use.rs`
  (read in order to keep it and every assertion in it).
- Concord: `docs/CHEATSHEET.md` (all of it) and `docs/CALLER_GUIDE.md` §5–6 (sending, errors).
  I also grepped `docs/*.md` for `207|multi-status|batch|partial` and found no batch guidance.
- `examples/testserver/src/lib.rs`: the public signatures, and the `Reply`/`Script` definitions.
- HubSpot documentation, read-only. The pages are listed under Provider grounding below.

Read **only after** the implementation and all four gates passed: `FRICTION.md`, `FIRST_USE.md`.
Not read at any point: other example SDKs, `checks/maintenance-*` records, `docs/AUTHOR_GUIDE.md`,
and shared-crate sources apart from the testserver items above.

## Provider grounding (all read 2026-11-07)

| Source | Fact used |
| --- | --- |
| [Retrieve a batch of contacts, legacy v3 reference](https://developers.hubspot.com/docs/api-reference/legacy/crm/objects/contacts/batch/get-contacts.md) | `POST /crm/v3/objects/contacts/batch/read`. `inputs[].id`. `properties` and `propertiesWithHistory` are both *required* arrays. `idProperty` is omitted for record IDs. The `archived` query defaults to false. `200` returns `results`; `207` returns `results` plus `errors[]` and `numErrors`. `StandardError` has `category`, `context` (a map of string arrays), and an `id` described as *the error instance's* ID, not a contact ID. |
| [2026-03 reference](https://developers.hubspot.com/docs/api-reference/2026-03/crm/objects/contacts/batch/get-contacts.md) | Same schema under a versioned path. Not adopted, so the routes stay consistent with the SDK's other legacy routes. |
| [Contacts guide, legacy](https://developers.hubspot.com/docs/api-reference/legacy/crm/objects/contacts/guide) | Record IDs need no `idProperty`. "Batch operations are limited to 100 records" (general limits). |
| [Error handling, legacy](https://developers.hubspot.com/docs/api-reference/legacy/error-handling.md) | The multi-status envelope example: `status`, `results`, `numErrors`, and `errors[]` items with `status`, `category`, `message`, `context`. All error fields may be absent. HubSpot recommends retrying 429/502/503/504; this SDK deliberately leaves that to the caller. |
| HubSpot Community thread "Question about batch read API", known only through a web-search summary (the page answered 403 to the research tool) | **Secondary, not official.** It reports `OBJECT_NOT_FOUND` errors that name IDs in `context.ids`, and a merged contact returned under a different ID. |

The requested reference path `.../contacts/batch/read-contacts` returned 404. The working URL came
from search.

## What changed for callers

- New `HubSpot::batch_read(BatchReadRequest { ids, properties }) -> Result<BatchRead, Error>`.
  The generated `CrmApi::contacts().batch_read(..)` (`CrmApiContactsBatchRead`) inherits the
  client's base, bearer header, `Json` format and 1 MiB limit, and the scope's path. The
  declaration added only three lines.
- **Input** is refused before any OAuth or CRM request (`Error::Invalid`) when the list is empty,
  has more than `BATCH_READ_MAX` (100) IDs, or contains an empty or duplicate ID. The body is
  always `{"inputs":[{"id"}..],"properties":[..],"propertiesWithHistory":[]}`.
- **Per-ID result.** `BatchRead.items` has exactly one `BatchItem { id, result }` per requested
  ID, in request order (provider order is not documented). The result is one of:
  - `Ok(Contact)`;
  - `Err(ItemError::Provider { category })`, when an item error's `context.ids` names the ID. If
    several errors name it, the first category wins;
  - `Err(ItemError::Unreported)`, when HubSpot named the ID nowhere. This includes an error that
    carries the ID only in its error-instance `id`.
- **The rest of the response is kept, but not attributed.** Results whose ID was not requested go
  to `unmatched` (for example the reported merged-contact case). Categories of errors that name
  no requested ID go to `unattributed`.
- **Contradictions** fail the whole call with `Error::Invalid`: a requested ID returned twice, or
  reported as both found and failed.
- **Redaction.** Item error messages, links, sub-categories and other context are never
  deserialized. `ItemError` `Display`/`Debug` show only the category, as the ticket requires.
- **No retry.** The request is sent once. A status error (429 with `Retry-After` included),
  timeout, disconnect or decode failure fails the whole call as `Error::Concord`. Repeating it is
  safe, but it is the caller's decision. A 401 is not refreshed and replayed.
- **OAuth.** Batch read uses the existing `self.api()` → `token()` path, so it joins the shared
  single-flight refresh and observes the replacement refresh token.
- **Unchanged.** Search, get, create, `Error` and the existing models are untouched.
  `tests/first_use.rs` and `tests/workflows.rs` are byte-identical. In `tests/protocol.rs` only
  the `use` lines changed and tests were appended; no existing assertion changed.

## Every location edited

| File | Location | Edit |
| --- | --- | --- |
| `src/lib.rs` | `use` lines | `Deserializer`, `Serializer`, `BTreeSet` |
| `src/lib.rs` | `concord::api!` `scope contacts` | the `POST batch_read(body request: BatchReadRequest) -> BatchReadResponse { path "batch/read"; }` endpoint |
| `src/lib.rs` | after `Next` | `BATCH_READ_MAX`; `BatchReadRequest` with its `validate` and a hand-written `Serialize`; the wire types `BatchReadResponse` and `BatchError` (with `context_ids`); the caller types `ItemError`, `BatchItem` and `BatchRead` (with `get`); the private `attribute` |
| `src/lib.rs` | `impl HubSpot` after `get` | `batch_read`, with a compiled `no_run` rustdoc example |
| `src/lib.rs` | `mod tests` | `batch_read_default_destination_and_body` (default host, method, path, body) |
| `tests/protocol.rs` | appended | `batch_read_refuses_invalid_input_before_any_request`, `batch_read_attributes_each_requested_id`, `batch_read_failures_are_sent_once` (401/429/500/502/503/504, timeout, hang-up) |
| `tests/batch_read.rs` | new consumer test | search → batch read of the chosen IDs plus a stored stale ID (207, then 200); batch read joining a held refresh with a concurrent `get`; caller-decided repeat after a 429 |
| `fixtures/` | new | `batch-read-complete.json` (200), `batch-read-partial.json` (207) |
| `README.md` | scope sentence, "sent once" sentence, new batch paragraph | the caller contract above |
| `PROVENANCE.md` | sources, fixture table | the citations above and fixture derivations |

No shared crate, other example, manifest or `Cargo.lock` changed. No dependency was added.

## Facts that had to be written in several places

- **Batch size 100.** It appears in `BATCH_READ_MAX`, its rustdoc, the README, and the validation
  message text. `Error::Invalid` takes a `&'static str`, so the message names the constant
  instead of formatting the number. Tests use the constant.
- **Wire body shape.** The `Serialize` impl defines it, and it is asserted in three places: the
  unit test, `protocol.rs` and `batch_read.rs`. Deliberate: these are independent checks.
- **Route `/crm/v3/objects/contacts/batch/read`.** The declaration holds only the
  `"batch/read"` suffix; base and scope path are inherited. The full path is repeated in the unit
  test, in a `BATCH` constant in each of the two test files, and in the README text.
- **Attribution and no-retry rules.** These live in `attribute`/`batch_read` code, in the
  `batch_read` rustdoc, in the `BatchRead` field docs, and in the README paragraph and its
  "sent once" sentence.
- **Fixture provenance.** The fixture files and their rows in the `PROVENANCE.md` table.

## Did the compiler or the tests catch an omitted edit?

The experiments were temporary mutations of `src/lib.rs`, each restored from a copy (verified
with `cmp`):

| Omission or mutation | Caught by |
| --- | --- |
| `impl Serialize for BatchReadRequest` disabled | **Compiler**, E0277 at the `body request: BatchReadRequest` span in the declaration (`lib.rs:37:43`): "required for `Json` to implement `Encode<BatchReadRequest>`". The note also names an internal path, `concord::__macro::concord_core::__private::BodyFormat::<T>::new`. |
| `#[serde(default)]` on `errors` removed | **Tests**: 3 consumer tests and 1 protocol test fail, because a `200` body has no `errors`. |
| `propertiesWithHistory` skipped | **Tests**: unit, protocol and consumer. My first attempt at this mutation was a no-op and passed. I rewrote it, and it was then caught. |
| `validate()` call removed | **Tests**: `batch_read_refuses_invalid_input_before_any_request` |
| found-and-failed check removed; duplicate-result check removed; outcomes in provider order; unattributed categories dropped (each run separately) | **Tests**: `batch_read_attributes_each_requested_id` |
| SDK repeats the batch after a failure | **Tests**: `caller_decides_whether_to_repeat_a_failed_batch` and `batch_read_failures_are_sent_once` |
| a later error's category overwrites the first | **Not caught** at first. I then added a third error naming ID `2` to the 207 case, and it is now caught. |
| README / PROVENANCE text not updated | **Nothing**. The README code block is not compiled (the crate does not include the README). The `batch_read` rustdoc example is compiled as `no_run`. |

## Remaining generic duplication

- The "redirects off, retries off, finite timeout" `reqwest::Client` builder now appears five
  times in this package's tests: `first_use.rs`, `workflows.rs`, `protocol.rs` (`http()` and the
  new `batch_hub()`), and `batch_read.rs`. I added `batch_hub` instead of changing the existing
  `http()` helper, so existing tests stayed untouched.
- Most test servers repeat the same OAuth route stub.
- `self.api().await?.contacts().X(..).send().await?` is repeated for each operation. It is small,
  and `concord::Endpoint<T>` could absorb it, but four call sites do not justify an adapter.
- The bookkeeping in `attribute`, which is mostly provider-independent: request-order output,
  duplicate and contradiction detection, the unmatched and unreported sets.
- `testserver::Reply` is not `Clone`, so a reply chosen per scenario needed a flag inside the
  closure.

## Provider-specific responsibilities (stay in this SDK)

- **Where failing IDs are named.** The SDK reads `context.ids`, never the error-instance `id`.
  This rests on a secondary source. An ID reported another way surfaces as `Unreported`, never as
  found.
- **The 100-record cap, and the required-but-empty `propertiesWithHistory`.** `idProperty`,
  `archived` and property history are excluded.
- **Merged contacts returned under another ID.** These land in `unmatched` and are not guessed
  back to a requested ID.
- **Category vocabulary** (`OBJECT_NOT_FOUND`, ...). Categories are passed through unvalidated.
- **Retry policy.** HubSpot recommends retrying 429/5xx; the ticket forbids SDK retries, so the
  caller decides.
- **The body `status` field** (`COMPLETE`/`PENDING`/`PROCESSING`/`CANCELED`) is not interpreted.
  IDs missing from a non-complete batch show up as `Unreported`.
- **200 versus 207** is not exposed separately. The per-ID results already carry the difference.
  If a caller needs the status, `prepare()?.receive()` with `decode_with_meta()` is the documented
  route.

## Candidate Concord improvements (not applied; not demonstrated as necessary)

1. **Macro diagnostics.** The missing-`Serialize` error already points at the right span and
   names `Encode<BatchReadRequest>`. A `#[diagnostic::on_unimplemented]` on `Encode`/`Decode`
   could drop the `__private::BodyFormat` note from the user-facing output.
2. **Test transport.** The strict test `reqwest::Client` builder is repeated across tests. This
   is a second observation of FRICTION item 5, a "strict transport" builder. It belongs in the
   testserver or a documented helper, not in core.
3. **Keyed batch outcomes.** A small helper for "requested keys → found / failed(category) /
   unreported, plus unmatched" could serve other batch APIs. The rule that attributes an error to
   a key (here `context.ids`) is provider-specific and must remain SDK code. I would not add this
   without a second SDK needing it.
4. **testserver.** Make `Reply` `Clone` for `Full` bodies.

No change to core, macros or the client → scope → endpoint hierarchy is indicated. The endpoint
needed only its own suffix, request and response. Everything else was inherited.

## Evidence limits and open questions

- The `context.ids` key and the merged-ID behavior are not in the official reference and were not
  validated live.
- Decoding is strict. A missing `category`, or non-string `context.ids`, fails the whole call as a
  `Decode` error, as the schema marks `category` required. A `context` that is absent or `null`
  is accepted.
- Category text is printed as given. The SDK does not check that it is a short token.
- A 100-ID batch whose body exceeds 1 MiB fails with `Limit`. Not tested with large bodies.
- Concurrency evidence covers clones of one instance only, as in the existing tests.

## Commands and results

All runs used `CARGO_TARGET_DIR=<worktree>/target CARGO_BUILD_JOBS=4`, one cargo command at a time:

- `cargo test --locked -p concord-sdk-hubspot-crm`: **21 passed, 0 failed**. That is 2 unit tests
  (1 new), 3 new consumer tests in `batch_read.rs`, the 2 unchanged `first_use.rs` tests,
  9 protocol tests (3 new), the 4 unchanged workflow tests, and 1 compiled doctest (new).
- `cargo clippy --locked -p concord-sdk-hubspot-crm --all-targets -- -D warnings`: clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-hubspot-crm --no-deps`: clean.
- `cargo fmt --all -- --check`: clean, after `cargo fmt -p concord-sdk-hubspot-crm` on my own files.
