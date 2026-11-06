# First use of `concord-sdk-dropbox` (independent consumer)

Consumer: Claude Opus subagent (requested runner per the SDK11 capsule; model/effort as
recorded by the harness, not self-reported here). I did not write this SDK and did not read
`FRICTION.md`. Library frozen at `6d6d66aa`; SDK as committed in `69692433`.

Business outcomes: **A** list one team member's folder in an explicit root; **B** list another
member/root concurrently on the same client.

## Trace (ordered)

1. Entry point: `examples/dropbox-sdk/README.md` (crate docs are the same text).
2. `examples/dropbox-sdk/PROVENANCE.md`, because the README points to it for sources and
   fixtures. It describes the SDK's own fixture files but the README does not point me at
   them for reuse, so I author my own fake responses from the Dropbox documentation.
3. Rustdoc: `cargo doc -p concord-sdk-dropbox -p concord-example-testserver --no-deps`, then
   `cargo doc -p concord-pagination -p concord --no-deps --features concord/csv,concord/jsonl`.
   Read (as text extracted from the HTML) every public item of the SDK, the testserver's
   `TestServer`/`Reply`/`Body`/`Recorded`/`Script`, and pagination's `Pages`, `PageItems`,
   `ItemStream`, `PagesError`, `Incomplete`, `Page`, `Resume`.
4. `crates/concord-pagination/README.md`; `docs/CALLER_GUIDE.md` sections 6 (errors) and 6a
   (streams and pages).
5. Early documentation friction noticed while reading (before writing code): see F1 below.

## Prediction before writing outcome A

Request (what I expect on the wire, from the Dropbox docs plus the README):

- `Client::new(reqwest_client, ClientConfig { token })`, then
  `.with_base_url(format!("{}/2", server.url()))` because the README says the base includes `/2`.
- `client.list_folder_pages(RequestContext::new("dbmid:…", PathRoot::root("100")?)?,
  ListFolderOptions::new("/Projects"))` makes no request until polled.
- First poll: `POST /2/files/list_folder`, `authorization: Bearer <token>`,
  `content-type: application/json`, `dropbox-api-select-user: dbmid:…`,
  `dropbox-api-path-root: {".tag": "root", "root": "100"}` (exact spacing unknown; I will assert
  the parsed JSON first and then pin the exact text I observe), and a JSON body with `path`
  plus the explicit Boolean defaults `recursive=false`, `include_deleted=false`,
  `include_mounted_folders=true`, `include_non_downloadable_files=true`, and no `limit`.
- Following polls: `POST /2/files/list_folder/continue` with body `{"cursor": "<exact cursor>"}`
  only, and the *same* select-user and path-root headers as the first request (the cursor carries
  its context).

Return value:

- `next_page()` → `Ok(Some(Page { items: Vec<Entry>, next: Option<Cursor>, meta: FolderPageMeta }))`;
  `meta.http.request_id()` reads Dropbox's `X-Dropbox-Request-Id` header (guess: that is the
  header name it reads); `meta.cursor` is present on the last page too.
- `Entry::File(FileMetadata)`, `Entry::Folder(FolderMetadata)`, `Entry::Deleted(EntryPath)` from
  the `.tag` field.

Completion and failure:

- `Ok(None)` only after a page with `has_more: false`, and again on later calls. An empty
  page with `has_more: true` continues.
- A failed page → `Err(PagesError::Provider { error: Error::Http(..), failed, .. })`;
  `failed` is `Resume::Start` for the first page and `Resume::At(cursor)` for a continuation;
  the next call is `Err(PagesError::Ended { .. })`.
- A 409 error document decodes to `ProviderError { kind: Tagged(..), summary, .. }`
  (`path/not_found/..` → `Tagged(Path { path: NotFound })`, `reset/..` → `Tagged(Reset)`).
- A body cut off mid-page → `Error::Concord { meta: Some(..), .. }` (transport or decode kind);
  `items().collect_complete()` returns `Incomplete` with the previous pages' items.
- 429: `Error::Http` with `meta.retry_after() == RetryAfter::Seconds(..)`, and exactly one request
  (no retry).
- `max_pages(n)` reached while `has_more: true` → `PagesError::Bounded`.

## Outcome A: what happened

6. Wrote the fake provider (`FakeDropbox` in `tests/first_use.rs`) from the documentation:
   member+path lookup, 401 `invalid_select_user`, 409 `path/not_found` (with `user_message`),
   409 `reset` for an unknown cursor, 422 `invalid_root`/`no_permission` for a path-root
   mismatch, and a loud non-Dropbox 400 if a cursor is continued with another member/root or
   extra body fields. Cursors contain `+ / = %2F`, a quote and `é`, and are matched byte-exact.
7. Wrote 13 outcome-A tests. Compiler diagnostics on the first build: **no errors**; three
   warnings only (an unused `AtomicBool` import and unused `MEMBER_B`/`MEMBER_C`, reserved for B).
   No SDK `src/` or test reads, no native escape.
8. First run: 13/13 passed. Because nothing failed, I added a throwaway `--nocapture` test (then
   removed it) to look at the actual wire and diagnostics instead of trusting the asserts alone.
   Observed:
   - `dropbox-api-path-root` is compact JSON: `{".tag":"root","root":"1001"}`,
     `{".tag":"home"}`, `{".tag":"namespace_id","namespace_id":"2002"}`.
   - The first-request body is exactly
     `{"path":…,"recursive":false,"include_deleted":false,"include_mounted_folders":true,"include_non_downloadable_files":true}`
     (no `limit` unless set); the continue body is `{"cursor":…}` with the cursor bytes unchanged.
   - Headers also include `accept: application/json`; no query string.
   - `meta.http.request_id()` reads `x-dropbox-request-id` (my guess held).
   - Operation names: `DropboxApi.selected.list_folder` and
     `DropboxApi.selected.list_folder_continue` (method `POST`).
   - `Debug` of `Cursor` and `RequestContext` is `<redacted>`; `Debug` of `HttpError` shows status,
     operation and header *names* only; `Display` of the SDK error is
     `Dropbox HTTP failure (409 Conflict)`.
   - `Display` of `PagesError::Provider` is `a page request failed (after 0 pages emitted)`;
     it does not contain the SDK error's text (see F2).

Prediction check for A: every wire and completion prediction held. A body cut off after the
head is `Error::Concord` with `ErrorKind::Transport` and the page's metadata (as predicted). An
unknown entry tag and a missing `has_more` are `ErrorKind::Decode`; an empty terminal cursor is
`Error::EmptyCursor`; a `text/html` 200 is `ErrorKind::Metadata`; all keep the 200 metadata and
end the traversal (`Ended`). The default cap stops a never-ending listing at exactly 1000
requests as `Bounded`.

## Prediction before writing outcome B

Request:

- One `Client` (no clone needed: `list_folder_pages(&self, ..)` returns an owned traversal).
  Member A with `PathRoot::root("1001")`, member B with `PathRoot::namespace("2002")`. Each
  traversal's every request carries its own `dropbox-api-select-user` and path-root header, the
  continuations included, exactly as for A; nothing is shared between the two except the token.
- Reassigning the local `selected` context variable after starting A (README pattern) changes
  nothing about A's traversal.

Return value:

- `tokio::join!(a.items().collect_complete(), b.items().collect_complete())` gives
  `(Result<Vec<Entry>, Incomplete<Entry, Error, Cursor>>, …)`, one per member.

Completion, failure and concurrency:

- Both chains progress on the one task: B can finish its whole listing while A's first page body
  is still arriving (I will gate A's first body on the arrival of B's *continuation* request).
- A failure of B (422 `no_permission` for an inaccessible namespace, 422 `invalid_root` with the
  member's actual root, 401 `invalid_select_user`) is B's own `Err(Incomplete)`; A is still `Ok`
  with every entry. The SDK does not switch roots or retry: exactly one request for B.
- Dropping B's traversal while its page is stalled stops B (no more requests) and A completes.

## Outcome B: what happened

9. To prove concurrency with the testserver (whose responder is a synchronous
   `Fn(&Recorded) -> Reply`), I gated A's first page: its reply becomes a `Body::Generate` that
   sends a space every 5 ms (insignificant JSON whitespace) and the real page only once B's
   *continuation* request has reached the fake (bounded at about 10 s, which sets a
   `timed_out` flag). `Bytes` came from `concord::bytes` (found in Concord's re-export list).
   Docs used: testserver rustdoc for `Body::Generate`/`Generator`; no testserver source read.
10. Wrote 5 outcome-B tests. First build: no errors, no warnings. First run: 18/18 passed.
11. Negative control (temporary test, removed): the same concurrency test with A awaited to
    completion before B started **failed** after 12 s on `B progressed while A's first page was
    arriving`, so the gate does detect a serialized listing.
12. `rustfmt` (repo `rustfmt.toml`) on my file; `cargo clippy -p concord-sdk-dropbox --test
    first_use -- -D warnings` clean; suite run 5 times, 18/18 each (about 0.5 s).

Prediction check for B: held. Every request of each member carried that member's select-user
header and its own exact path-root text (`{".tag":"root","root":"1001"}` vs
`{".tag":"namespace_id","namespace_id":"2002"}`, `{".tag":"home"}` for C); each continuation sent
its own cursor; B's continuation arrived before A's. `no_permission` (namespace 3003),
`invalid_root` (with `RootInfo { kind: "team", root_namespace_id: "2002", .. }`) and
`invalid_select_user` were each the failing member's own `Err(Incomplete)` with exactly one
request; the other member completed. After an `invalid_root` the application re-selected the
reported root explicitly and listed successfully. Dropping B's stalled traversal produced no
further B request while A completed.

## Interventions

None. I did not read the SDK's `src/` or tests, nor `FRICTION.md`, and used no native escape
(no raw reqwest calls to the provider, no private reach-through). Non-SDK reading: `PROVENANCE.md`
(linked from the README), testserver and Concord rustdoc, the pagination README and the caller
guide. Two throwaway tests (wire/diagnostic observation; sequential negative control) were run
and removed; they are debugging steps, not changes to the SDK.

## Working path

```rust
let client = Client::new(http, ClientConfig { token }).with_base_url(base)?;
let a = RequestContext::new(member_a, PathRoot::root("1001")?)?;
let b = RequestContext::new(member_b, PathRoot::namespace("2002")?)?;
let (a, b) = tokio::join!(
    client.list_folder_pages(a, ListFolderOptions::new("/Projects")).items().collect_complete(),
    client.list_folder_pages(b, ListFolderOptions::new("/Projects")).items().collect_complete(),
);
// Ok(entries) only after has_more: false; Err(Incomplete { collected, error }) otherwise.
// Typed provider failure: PagesError::Provider { error, failed, .. } -> error.provider()?.kind
```

Not covered here (not needed for A/B as a first user): Retry-After duplicate/invalid/overflow,
the 16 MiB / 64 KiB body caps, header-injection inputs, `PagesError` resume into a new
traversal.

## Friction (concrete)

F1. **`ClientConfig` rustdoc names items that do not exist for a caller.** Its description is
    "The required values of [DropboxApi], passed to [DropboxApi::new]. Optional and initialized
    values are set afterwards with the client's set_* and maybe_* methods", and its trait impls
    are headed `impl Clone for DropboxApiConfig`. Neither `DropboxApi` nor `DropboxApiConfig`
    appears in the crate's item list (`all.html`), the links are unresolved brackets, and
    `Client` has no `set_*`/`maybe_*` methods. It reads as generated text for the hidden
    `wire` declaration leaking through the re-export. Reproduction:
    `cargo doc -p concord-sdk-dropbox --no-deps`, open `struct.ClientConfig.html`.

F2. **`PagesError`'s `Display` does not add the provider error's `Display`, contrary to its
    rustdoc** ("Display adds the provider error's own Display"). Observed for a 409 first page:
    `format!("{e}")` is `a page request failed (after 0 pages emitted)`, while the SDK error's own
    text is `Dropbox HTTP failure (409 Conflict)`. An application that logs `{e}` of a traversal
    error loses the status unless it walks `source()`. Reproduction: answer the first
    `list_folder` with any 409 and print `client.list_folder_pages(..).next_page().await
    .unwrap_err()`. The test
    `a_provider_error_document_on_the_first_page_is_typed_and_ends_the_traversal` asserts only
    what an application relies on (`source()` is the SDK error, whose text has the status), not
    the generic text. This is concord-pagination behavior/doc, not this SDK.

F3. **Reaching the Dropbox error tag from a traversal error is a three-level unwrap.** Every
    "is this `reset` / `invalid_root` / `no_permission`?" check needs
    `PagesError::Provider { error, .. }` → `error.provider()` → `.kind` →
    `ProviderErrorKind::Tagged(tag)`. I wrote two helpers for it (`provider_error`, `tagged` in
    `tests/first_use.rs`); every application doing B (several members, each with its own
    recoverable errors) will write the same. A `Error::tag() -> Option<&TaggedError>` would
    remove one level; the `PagesError` level is generic and expected.

F4. **A failed first page carries no context, so a fan-out must label results itself.** For B
    the error's `failed` is `Resume::Start` and all diagnostics redact the member/root (by
    design), so only the application's own pairing (tuple position in `join!`) says which member
    failed. Continuation failures do carry it (`Resume::At(cursor)` → `cursor.context()`). Not a
    defect; worth one README sentence for multi-member use.

F5. **Proving concurrency needed an invented gate.** The testserver responder is synchronous and
    has no "hold this reply until request X arrives" primitive; the exported `BoxFuture` alias
    ("for responders that compute replies asynchronously") has no documented entry point that
    takes one. The whitespace-padded `Body::Generate` gate works and is bounded, but it relies on
    the JSON parser accepting leading whitespace. Testserver (frozen) friction, not this SDK.

F6. Minor rustdoc readability: with `--no-deps`, `Error::Concord { error: Error, .. }` and
    `impl From<Error> for Error` show two unlinked types both named `Error`; I inferred from
    "Original Concord failure" that it is `concord::Error` (confirmed when `error.kind()` returned
    `concord::ErrorKind`). The generated names `DropboxApi.selected.list_folder` (operation
    identity) and `Client { api: DropboxApi { .. } }` (`Debug`) refer to the same hidden
    declaration as F1.

Things that worked without friction: the README's API shapes behaved as described and my
first build had no compiler errors; the `concord_sdk_dropbox::pagination` re-export meant no extra
dependency for `PagesError`/`Resume`; `error.meta().map(|m| m.retry_after())` as documented;
the SDK README's "default cap 1000" (SDK policy) is consistent with pagination's "no cap unless
set" and was verified at exactly 1000 requests.
