# Provenance

Where every provider fact, fixture and dependency of this SDK comes from. Live provider calls:
**NOT_RUN** — the real API was never called; every test runs against `examples/testserver` on
127.0.0.1.

## Documentation pages

Both pages were retrieved on **2026-11-07 (06:47 UTC)** with `curl`, as published HTML and as the
site's Markdown rendering of the same page (`<url>.md`, `text/markdown`); the Markdown copies were
read. Copies are kept outside the package (`target/tmp/docs/`, not committed). SHA-256 of the
Markdown copies as retrieved:

- streaming: `6b399a4915b5327366f18511c0498a34c2b4c1c83b83845adfcb61a8d33e27ee`
- errors: `7866969b68a2b5e93f90c4e6727be59f9a9e874fa6035f5cb8fbb95a35a815ed`

Page content is referred to below by section and identifier, not reproduced.

| Page | Sections used | Facts taken |
|---|---|---|
| [Streaming messages](https://platform.claude.com/docs/en/build-with-claude/streaming) | opening paragraph; "Event types" (flow, "Ping events", "Error events", "Other events"); "Content block delta types"; "Full HTTP stream response" ("Basic streaming request" curl and Response, "Streaming request with tool use" Response, "Streaming request with thinking" Response); "Error recovery" | `"stream": true` selects SSE; the event flow `message_start` (a `Message` with empty `content`) → content blocks (`content_block_start`, deltas, `content_block_stop`, `index` = position in the final `content`) → one or more `message_delta` → final `message_stop`; `ping` anywhere; the SSE event name matches the data's `type`; `message_delta` usage is cumulative; `error` events after the 200, with the `{"type":"error","error":{"type":"overloaded_error",...}}` shape; unknown event types must be tolerated; delta types `text_delta`, `input_json_delta`, `thinking_delta`, `signature_delta`; request headers `x-api-key`, `anthropic-version: 2026-10-21`, `content-type: application/json`; request body `model`, `messages`, `max_tokens`, `stream`; recovery after an interruption is a new request built by the client |
| [Claude API errors](https://platform.claude.com/docs/en/api/errors) | "HTTP errors"; "Request size limits"; "Error shapes"; "Request ID"; "Long requests" | the status → `error.type` list (400 `invalid_request_error`, 401 `authentication_error`, 402 `billing_error`, 403 `permission_error`, 404 `not_found_error`, 409 `conflict_error`, 413 `request_too_large`, 429 `rate_limit_error`, 500 `api_error`, 504 `timeout_error` — also with a 500 —, 529 `overloaded_error`); a spend-cap 429 has no `retry-after`; the official SDKs retry and honour `retry-after` (this SDK deliberately does not retry); errors after a 200 on a stream follow the stream's error events; the error document shape (`type`, `error.type`, `error.message`, `request_id`) and that `type` values may grow; the `request-id` header on every response; a stream-assembled message is "identical" to a non-streaming one; the non-streaming curl example (no `stream` field) |

Observations about the pages, used by the tests:

- The tool-use, thinking and web-search Response blocks end at their last `data:` line, without
  the blank line that dispatches an SSE event (the basic Response block has it). Under Concord's
  reader such a body is incomplete; the fixtures add the final blank line, and
  `a_documented_stream_without_its_final_blank_line_is_incomplete` shows the verbatim form fails.
- The thinking stream has no `usage` in `message_start` or `message_delta`: `usage` and its counts
  are optional in this SDK.
- The web-search Response contains a `...` elision and jumps from block index 3 to 17: not used.
- The "Input JSON delta" snippet has one closing brace too many (not valid JSON): not used.
- Model names (`claude-opus-5-5`) and the version `2026-10-21` are copied from the pages' examples;
  neither is validated by the SDK.

## Fixtures (`fixtures/`)

| File | Derivation |
|---|---|
| `stream-basic.sse` | **Verbatim**: streaming page, "Basic streaming request", the `sse Response` block (8 events, ends with its blank line). |
| `stream-tool-use.sse` | **Verbatim + one line feed**: streaming page, "Streaming request with tool use", Response block; the final blank line added (see above). |
| `stream-thinking.sse` | **Verbatim + one line feed**: streaming page, "Streaming request with thinking", Response block; the final blank line added. |
| `error-event.sse` | **Verbatim + one line feed**: streaming page, "Error events", the `sse Example error` block; the final blank line added. |
| `error-not-found.json` | **Verbatim**: errors page, "Error shapes", the JSON example. |
| `message.json` | **Derived by hand**: the non-streaming message the basic stream describes — `message_start`'s message, `content` = the text block started empty plus the deltas `"Hello"` and `"!"`, `stop_reason`/`stop_sequence` from `message_delta`, `usage` = `input_tokens` 25 (message_start) and the cumulative `output_tokens` 15 (message_delta). Written before the assembly code ran; the errors page states the two are identical. |
| `message-tool-use.json` | **Derived by hand**: the non-streaming message of the tool-use stream; the `tool_use` block's `input` is the concatenation of its `partial_json` deltas, parsed. |

Bodies built in the tests (each labelled where it is built):

- error documents for every documented class: `error-not-found.json` with `error.type`,
  `error.message` and `request_id` replaced (synthetic messages and ids `req_head_<status>`,
  `req_body_<status>`);
- streams with one event replaced, removed, repeated or inserted (synthetic events: a renamed
  `ping`, an unnamed event, a skipped index, a `tool_use` start, an event without `type`, an
  unknown `future_event`, an oversized `text_delta`, invalid UTF-8, non-JSON data, a wrong-typed
  `index`);
- prefixes of `stream-basic.sse` (cut at an event boundary, inside a data line, or followed by a
  dropped connection) and the basic prefix followed by `error-event.sse`;
- unreadable bodies: an HTML page, unlabelled JSON, `{oops`, truncated documents, bodies one byte
  past `ERROR_BODY_LIMIT` / `MAX_MESSAGE_BYTES`, a 307 redirect to a second local server;
- the key `sk-ant-synthetic-0000-key` and every `request-id` value are synthetic.

## Dependencies

No new crate: `Cargo.lock` gains only this package's own entry; no other package changed version.

| Crate | Source | Why |
|---|---|---|
| `concord` (features `sse`, default `json`) | workspace | the declaration, `Json`, the SSE reader (`concord::sse`, `concord::stream`) |
| `concord-retry` (no features) | workspace | `http::retry_after`, the strict `retry-after` parser behind `ApiError::retry_after` |
| `serde`, `serde_json` | already in `Cargo.lock` (workspace versions) | models, event `data` decoding |
| dev: `concord-example-testserver`, `tokio` | workspace / already locked | the local server, the async test runtime |

## Examples and sources consulted

Concord material read: `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
`docs/CHEATSHEET.md`, `docs/RECIPES.md`, `docs/CALLER_GUIDE.md` (headings and §8), the READMEs of
`concord-codecs`, `concord-auth`, `concord-retry`, `concord-ratelimit`, `concord-pagination`, and
the sources of `concord-codecs/src/sse/{mod,reader}.rs`, `concord-codecs/src/stream/{mod,error,source}.rs`,
`concord-core/src/{response,error,field,value,header}.rs` and the trait definitions in
`concord-core/src/{codec,prepared}.rs`, `concord-auth/src/credentials.rs` (signatures),
`concord-retry/src/http.rs` and `Cargo.toml`, `examples/testserver/src/lib.rs` (the test server's
API). This SDK is **example-assisted**:

| Example file | What was taken |
|---|---|
| `examples/openai-sdk/src/lib.rs` | the shape of a send policy that reads the provider's error document with `received.document::<_, _>(limit)` and returns `Response { value, meta }`; reading an `Incoming` with an SSE reader so that a broken body is Concord's own transport error, recovered with `into_source_unredacted().downcast()`. This SDK differs: a custom `Incremental` format so the generated `send()` returns the typed stream, strict `Json` for error documents, and an event-flow state machine with a sentinel rule. |
| `examples/openai-sdk/Cargo.toml` | the dependency layout (`concord` with `sse`, testserver as dev-dependency). |
| `examples/openai-sdk/.gitattributes` | `fixtures/** -text`, so fixture bytes are not converted on checkout. |

The file lists (names only) of `examples/openai-sdk` and `examples/ntfy-sdk` were listed to find
which examples use SSE; no other file of theirs was opened.
