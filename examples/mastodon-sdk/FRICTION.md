# Friction log — Mastodon SDK (author)

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

Concord frozen at `c3fd4f1e`; toolchain 1.99.1. No shared crate was edited; each item names the
smallest Concord change that would remove the cost instead. `file:line` references are to this
crate's working tree as delivered. Cause labels: **Concord**, **docs**, **SDK**, **provider**.

## Reading done and examples consulted (example-assisted)

Read: the root `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md` (all sections),
`docs/CHEATSHEET.md`, `docs/RECIPES.md`, `docs/CALLER_GUIDE.md` §6; the READMEs of
`concord-codecs`, `concord-auth`, `concord-retry`, `concord-ratelimit` and the first 140 lines of
`concord-pagination`'s; sources: `crates/concord-codecs/src/sse/{mod,reader}.rs`,
`sse/parse.rs` (lines 1–140), `stream/{mod,error,source}.rs`, `concord-core/src/codec.rs`
(`Incremental`), `concord-core/src/__private.rs` (`Base::parse`), `concord-core/src/response.rs`
(signatures), `concord-macros/src/emit.rs` lines 1380–1500 (what a generated client and scope
expose), `concord-auth/src/credentials.rs` (signatures); `examples/testserver/src/lib.rs` (the
required fixture server).

**Examples consulted — this SDK is example-assisted:**

| File | What I took from it |
|---|---|
| `examples/ntfy-sdk/src/error.rs` | the send-policy shape (`prepare()?.receive()`, non-2xx → an `ApiError` read with `document::<_, _>(64 KiB)`, otherwise `decode()`), `ApiError`'s `Display` without the document's text, the boxed error variant |
| `examples/ntfy-sdk/src/events.rs` | a custom `Incremental` format delegating its head checks to `Sse`; `SseReader::read_for` over a `ByteSource` wrapper of the `Incoming`; taking the source back with `into_source()` after a terminal error; an SDK error type mirroring `ReadError`'s surface (`kind`, `operation`, `position`, `codec`, `transport`, `source_unredacted`) |
| `examples/ntfy-sdk/src/lib.rs`, `Cargo.toml` | the README-as-doctest item, the inherent `with_token` on a generated client, the dependency comments |
| `examples/ntfy-sdk/PROVENANCE.md` (lines 1–80), `FRICTION.md` (lines 1–60) | the layout of these two files |
| `grep` over `examples/*-sdk/src/*.rs` (file names only) | that several SDKs keep the declaration in a private `mod api` |

No other example source was read.

## 1. A run-time host with a credential: the generated client has to be hidden — Concord

- **Evidence.** The generated `with_base_url` replaces only the base and keeps every client
  value (`crates/concord-macros/src/emit.rs:1436`), the `#[sensitive]` `authorization` included.
  A caller holding the generated client could therefore point this instance's token at any host
  with one call. The slice's rule ("a token for one instance must never be sent to another")
  cannot be stated in the declaration.
- **What I wrote by hand.** `src/lib.rs:119`–`200`: a `Mastodon` wrapper that keeps the generated
  `Api` private, five delegating methods returning the generated request values, and four
  `pub use api::{ApiX as XRequest}` aliases (`src/lib.rs:95`). The base itself is a placeholder
  (`src/api.rs`, `base "https://mastodon.invalid"`) that `Mastodon::new` always replaces; since
  `with_base_url` is fallible, the SDK re-implements Concord's base rules (and stricter ones) in
  `InstanceUrl::parse` / `check_origin` (`src/model.rs:16`–`68`, ~50 lines with docs) and calls `unreachable!` on
  the error it can no longer get (`src/lib.rs:132`).
- **The cost in generated docs** (observed in `target/doc/concord_sdk_mastodon/struct.PublicTimelineRequest.html`):
  the re-exported request types say "Request: `GET https://mastodon.invalid/api/v1/timelines/public`
  (the declared base URL; the client's `with_base_url` replaces it)", "the response default of the
  client `Api`" and "impl ApiPublicTimeline" — a placeholder host, a method callers do not have and
  a client they cannot name.
- **Provider semantics vs generic mechanics.** Which host may receive the token (the instance and
  the streaming host it advertises) is Mastodon's rule and stays here (`Mastodon::public_stream`,
  `StreamingHost`). "This client has no compile-time base; it is given one at construction and a
  credential is bound to it" is generic: every self-hosted product (Mastodon, GitLab self-managed,
  Matrix, Gitea) has it.
- **Smallest Concord change.** A declared run-time base (`base;` with no literal): the base
  becomes a constructor argument of a public validated type (say `concord::BaseUrl`, whose
  `parse` is today's `Base::parse`), the generated docs print paths relative to it, and the
  client has no `with_base_url` (a new base means `new` again, re-supplying the credential). The
  SDK could then expose the generated client directly and drop the wrapper, the aliases, the
  placeholder and the `unreachable!`.

## 2. One declaration, two origins: the streaming endpoint lives on a discovered host — Concord (small)

- **Evidence.** `GET /api/v1/streaming/public` is served by the host the instance advertises.
  The SDK clones the client and calls `with_base_url(host)` for every stream request
  (`src/lib.rs:190`); `Prepared::follow` is same-origin only (by design). The declaration cannot
  say "this endpoint is on another origin chosen at run time", so the generated docs of
  `PublicStreamRequest` show the REST placeholder host as well.
- **What it cost.** Three lines plus the trust check — cheap, because the wrapper of item 1
  already exists. Without item 1's wrapper it would also be a token-routing hole.
- **Smallest Concord change.** None needed beyond item 1; a doc note in AUTHOR_GUIDE §3 that a
  second origin is a second client value (`with_base_url` on a clone) would have saved the search.

## 3. SSE: Concord's reader carried the whole framing — Concord (positive), with two gaps

- **Saved.** No framing code in this SDK: split UTF-8 characters and `data:` fields, comments
  (`:thump`), CRLF, multi-line `data`, the 1 MiB event bound, fused terminal errors that name the
  endpoint and drop the URL — all Concord's (`tests/protocol.rs`, the split, one-byte-per-chunk,
  CRLF and oversized tests pass through `PublicStream` unchanged). The SDK's format is a 20-line
  adapter (`src/streaming.rs:193`) plus the payload decoding (`decode`, `src/streaming.rs:330`).
- **Gap a: "the body ended inside an event" is a `Format` error, like malformed bytes.**
  `ReadErrorKind` has no truncation kind (`crates/concord-codecs/src/stream/error.rs`, the enum
  after `Position`), and the labels that tell them apart are `pub(super)` constants
  (`crates/concord-codecs/src/sse/parse.rs:13`–`16`). For an endless stream a truncation is a
  disconnect symptom, not a broken server, so the SDK needs the distinction: it wraps the
  `Incoming` in a `ByteSource` only to note whether the body ended (`struct Body`,
  `src/streaming.rs:363`–`377`) and takes it back with `into_source()` after a `Format` error
  (`src/streaming.rs:283`–`303`), relying on the reader detecting "ends inside" only at the end of
  input. Reproduction: `tests/protocol.rs:692` (`a_body_ending_inside_an_event_is_truncated`;
  removing the `Truncated` arm makes it fail, observed by a mutation run). **Smallest change:** a
  `ReadErrorKind::Truncated` (or `ReadError::at_end_of_input()`) for the "input ends inside a
  line / event / record" faults of every reader.
- **Gap b: heartbeats are invisible.** Comments are consumed silently; a caller's idle rule can
  only watch `position().bytes`, which works because `next()` consumes comment bytes before it
  awaits the next chunk and is cancel-safe (read from `sse/reader.rs`, implied by the `position`
  docs, not stated). `tests/workflows.rs:217` pins that behaviour for this SDK. **Smallest
  change:** state in `SseEvents::position` that it advances over comments and blank lines even
  when no event is returned (including a cancelled `next()`); optionally a comment counter.
- **Provider semantics kept here.** The event-type mapping, the `delete` id, the decision that
  an undecodable payload is skipped rather than terminal, and that a clean end of body is a
  `Closed` error (Concord's `EndOfInput`/`None` means "completed", which an endless stream never
  is: `src/streaming.rs:259`–`268`).

## 4. A stream endpoint's `send()` is documented as `Records<F, I>` — Concord (docs)

- **Evidence.** rustdoc of `PublicStreamRequest::send` reads
  `impl Future<Output = Result<Records<PublicStreamFormat, StreamEvent>, Error>> + Send where
  MastodonPolicy: SendWith<Records<PublicStreamFormat, StreamEvent>, Output = …>`
  (`target/doc/concord_sdk_mastodon/struct.PublicStreamRequest.html`). The concrete
  `PublicStream` never appears; a caller has to open `PublicStreamFormat`'s `Incremental` impl.
- **Workaround.** The SDK's own docs name `PublicStream` everywhere (`src/lib.rs` operation table,
  `src/streaming.rs` `PublicStream` docs).
- **Smallest change.** The generated endpoint docs could add one sentence naming the projection
  ("`Records<F, I>` is `<F as Incremental<I>>::Records`, the format's reader"), or the
  declaration could accept the reader type (`-> stream StreamEvent as PublicStream`) and check it
  against the projection.

## 5. The error-document send policy is the same in every SDK — Concord (small)

- **Evidence.** `MastodonPolicy` and `ApiError::read` (`src/error.rs:31`–`42`, `:169`–`181`) are,
  apart from the document type and the absence of `Retry-After`, ntfy's `NtfyPolicy` and
  `ApiError::read`, which I consulted (and AUTHOR_GUIDE §5.11 shows Frankfurter's). About 25
  lines.
- **Provider semantics.** Mastodon's document (`{"error": ...}`), the 64 KiB bound and strict
  `Json` for the document are this SDK's choices.
- **Smallest change.** A standard policy in `concord` generic over the document format and type
  (`DocumentPolicy<Json, ErrorDocument, LIMIT>` returning `Result<T, ProviderError<D>>` with
  status, operation and `Option<D>`); SDKs needing headers (`Retry-After`) keep their own.

## 6. Small documentation frictions — docs

- Generated docs of a re-exported request show `prepare() -> Result<Prepared<…>, Error>` with
  Concord's `Error` next to `send() -> Result<…, Error>` with the SDK's `Error`: two different
  types rendered with the same name. A crate-qualified rendering (`concord::Error`) in the
  generated signature docs would remove the ambiguity.
- The interaction "serde's derived `Deserialize` accepts a JSON array positionally for a struct"
  bit the instance document: `[]` decoded as an `InstanceInfo` whose members all defaulted
  (observed while writing `tests/protocol.rs:823`). Not Concord's, but `Json` inherits it; the
  SDK made `configuration` required (`src/model.rs:346`). A sentence in AUTHOR_GUIDE §5.1 would
  warn the next author.

## Shared capabilities: used and not used

| Crate / feature | Decision |
|---|---|
| `concord` `json` | every finite document; strict `Content-Type` checks (tested with `text/html`, `text/plain`, unlabelled) |
| `concord` `sse` | the stream's framing and its head check (`Sse`), through a 20-line adapter |
| `concord-auth` `bearer` | the token: validation (empty, spaces, controls, non-ASCII) and the sensitive flag in one call (`src/model.rs:117`), instead of a hand-written check and `set_sensitive`. `TokenCache` not used: a user token is static here (no refresh flow in the slice). |
| `concord-pagination` | not used: each workflow reads one page; the next page is one documented setter (`set_max_id`), and the `Link` header Mastodon servers send is not on the pages read. A timeline walk would fit `Pages` (a dependent `max_id` chain). |
| `concord-retry` | not used: the slice defines no retry; a stream must never be restarted automatically (no replay), and repeating a timeline read is the caller's decision. |
| `concord-ratelimit` | not used: Mastodon's rate-limit headers are not on the pages read. |
