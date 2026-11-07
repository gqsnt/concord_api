# Friction

What writing this SDK cost on top of Concord, with evidence, and the smallest Concord change that
would remove each cost. "Provider" marks rules that belong to Anthropic and stay in the SDK;
"generic" marks mechanics another SDK would write the same way. Line numbers refer to the files
as delivered.

## What needed no hand-written code

Recorded so the costs below are read against it (all observed on this slice):

- The declaration (`src/lib.rs`, client → scope `messages` → `create`, `stream`) compiled on the
  first build; `validate_header_names()` passes.
- The send policy (`SendPolicy`, 16 lines) gives every `send()` this SDK's result. Non-2xx error
  documents are read with `received.document::<Json, ErrorDocument>(64 KiB)`; the status stays
  authoritative for HTML, unlabelled, malformed, oversized and truncated documents with no extra
  code (`unreadable_error_documents_keep_the_status`).
- `limit MAX_MESSAGE_BYTES;` on `create` bounds the message body (`Limit`); a scope-level `limit`
  above the `-> stream` sibling also compiled (checked, then reverted to the endpoint).
- `-> stream StreamEvent` with an SDK format (`MessageEvents: Incremental<StreamEvent>`) makes the
  generated `send()` return the typed `MessageStream` after the head check, body unread. Concord's
  SSE reader handled chunk cuts inside lines and UTF-8 characters, invalid UTF-8, bodies ending
  inside an event, the 1 MiB event bound and cancel-safety without SDK code.
- `#[sensitive]` on the key: client, `Prepared` and error output withhold it, including a `Build`
  error for a key with a line break.

## F1. The stream's reader type is hidden in the generated docs (generic)

Evidence: `target/doc/concord_sdk_anthropic_messages/struct.AnthropicMessagesStream.html`
documents `send() -> impl Future<Output = Result<Response<Records<MessageEvents, StreamEvent>>, Error>>`
with a `where SendPolicy: SendWith<Records<MessageEvents, StreamEvent>, Output = ...>` bound. A
caller learns that the value is a `MessageStream` only by opening `MessageEvents`'s
`Incremental` implementation.

Written by hand: the endpoint's `///` doc names `MessageStream`.

Smallest change: the macro's generated doc for a `-> stream T` endpoint with `response F;` adds a
line linking `<F as Incremental<T>>::Records` ("the reader: `F`'s `Incremental<T>::Records`");
rustdoc cannot resolve the projection, the text can point at it.

## F2. A format's reader cannot take client configuration (generic)

Evidence: `Incremental::open(context, body)` is an associated function; its only inputs are the
head-derived `Context` and the body. `MAX_EVENT_BYTES` (`src/stream.rs`) is therefore a crate
constant (Concord's 1 MiB default), not a per-client setting. The alternative, `-> stream` plus
an SDK method that opens the `Incoming` with a configured `SseReader`, would move the typed stream
off the generated `send()`.

Written by hand: nothing beyond the constant; the slice does not need a configurable bound.

Smallest change: none proposed now. If an SDK needs it, a way for a declared value to reach the
format (an `Incremental` context fed from client values) would be the change; it is not cheap and
this slice does not justify it.

## F3. Recovering Concord's transport error from a `ReadError` (generic)

Evidence: `read_failure` (`src/stream.rs`, 22 lines): for a `Transport` `ReadError`, test
`source_unredacted().is::<concord::Error>()`, then `into_source_unredacted()` and
`downcast::<concord::Error>()`, with two `expect`s, because taking the source consumes the error
(the openai SDK writes the same dance).

Smallest change: `ReadError::into_concord_error(self) -> Result<concord_core::Error, ReadError>`
in `concord-codecs` (it already depends on `concord-core`), returning the error unchanged when its
source is not Concord's.

## F4. The provider's credential header follows redirects (generic mechanics, provider header)

Evidence: `the_documented_transport_does_not_carry_the_key_through_a_redirect` — with
`reqwest::Client::new()`, a 307 to another origin delivers `x-api-key` to the second server;
reqwest drops only `Authorization`-like headers. Concord's `#[sensitive]` redacts output, it does
not control the transport (by design; the author guide says so).

Written by hand: `http_client()` (no redirects, no reqwest retries, 6 lines), its docs, and the
two-server test with the control.

Smallest change: none in core (the transport boundary is deliberate). The author guide's FRED
excerpt (§3) and this SDK write the same builder; a documented helper (in `concord-auth`, say:
`strict_transport() -> reqwest::ClientBuilder`) would remove the copy, not the decision.

## F5. A credential newtype cannot be placed as a header (generic)

Evidence: `ToHeaderValue` is sealed to `HeaderValue`, text, integers and `bool`. A validated
`ApiKey` type cannot be the client value, so the key is a `#[sensitive] String`; a key with a
line break is refused only when a request is prepared (`Build`, `Delivery::NotSent`, tested), not
when the client is constructed. Declaring it `HeaderValue` instead would make every caller build
and flag a header value.

Smallest change: in `concord-auth`, an `api_key(raw) -> Result<HeaderValue, CredentialError>`
next to `bearer` (same token rule, sensitive), so an SDK can declare `HeaderValue` and offer one
validated constructor. Not adopted here: the crate has no such constructor today (F6).

## F6. Shared capabilities that did not fit

- `concord-auth`: `bearer`, `basic` and `authorization(scheme, token)` all build an
  `Authorization`-style value; Anthropic's raw `x-api-key` has none, and there is no token to
  acquire or refresh. Not used.
- `concord-retry`: **used for `http::retry_after` only** (`ApiError::retry_after(now)`, the strict
  parser: repeated and invalid fields are errors, a spend-cap 429 without the header is
  `Ok(None)`). The attempt engine is not adopted: the slice forbids restarting a stream, and
  re-sending a `create` is a new, billed, non-deterministic generation, so repetition stays the
  application's decision. Cost: the SDK's accessor takes `now: SystemTime` because HTTP dates need
  a wall clock.
- `concord-pagination`: no listing in the slice.
- `concord-ratelimit`: process-local admission is an application policy; the cited pages give no
  rate-limit headers to observe.

## F7. Completion is a provider sentinel, not the end of the body (provider)

Evidence: Concord's SSE reader reports a body that ends at an event boundary as a clean end
(`Completion::EndOfInput`). For this provider that is **not** completion: only `message_stop` is.
`MessageStream::next` maps a clean end without `message_stop` to `IncompleteCause::EndOfStream`
and never answers `Ok(None)` after a failure (`AlreadyFailed`). The documented flow (SSE name =
data `type`, block order, `message_delta` before `message_stop`, cumulative usage) is
`PartialMessage::apply` (75 lines) and `decode` (55 lines) plus the wire structs.

Smallest change: none. This is the provider's protocol; Concord is right to call a body that ends
inside an event malformed and to leave the sentinel to the SDK.

## F8. One body flag per endpoint means two body types (provider)

Evidence: the streaming endpoint needs `"stream": true` in the same JSON body. A declaration
cannot add a constant field to a body, so `StreamRequest` wraps `MessageRequest` and serializes it
flattened with `stream: true` (`src/model.rs`, a 16-line `Serialize` and a 20-line type).

Smallest change: none. Bodies are opaque to Concord by design, and the two types make a streaming
body impossible to send to `create` — a gain, not only a cost.

## F9. Request ids from the head (generic, small)

Evidence: `request_id` (`src/error.rs`, 6 lines) reads `request-id` with `field::single` for
responses, Concord errors (`error.head()`) and the stream context. No change proposed:
`field::single` already gives absent/repeated; the SDK chose to treat both as "no id".

## Process notes (observed)

- First build, first test run: all tests passed. A mutation run (17 mutations of `src/`, copies
  under `target/tmp/mutation`) then found two rules no test exercised (a delta for a closed block,
  content in `message_start`); tests were added and all 17 mutations are now detected.
- No shared crate, check, script or other example was edited.
