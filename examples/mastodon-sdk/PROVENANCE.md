# Provenance

Every fixture under `fixtures/` and every inline response in `tests/` is either hand-authored from
the two documentation pages below or synthetic. **None was captured from a real Mastodon
server**: no request was made to any Mastodon instance (live provider calls: **NOT_RUN**), and
the tests prove nothing about the real service beyond agreement with the documented shapes.

## Sources read (2026-11-07)

### Maintenance research (2026-11-07)

Read the same two exact pages with the `web.run` tool before implementing hashtags:

- [timelines API methods](https://docs.joinmastodon.org/methods/timelines/), "View hashtag
  timeline": `GET /api/v1/timelines/tag/:hashtag`, a name without `#`; bracketed extra-tag
  arrays, shared flags/id bounds/limit and instance-controlled access. The number of extra tags
  applied is instance-controlled (default four total), so the SDK passes lists without a cap.
- [streaming API methods](https://docs.joinmastodon.org/methods/streaming/), "Hostname
  discovery", "Streaming timelines/categories", "About HTTP server-sent events", "Watch the
  public timeline for a hashtag", "Watch the local timeline for a hashtag" and "Establishing a
  WebSocket connection": use the HTTP hashtag routes with required `tag` for SSE. The
  `stream=hashtag` / `stream=hashtag:local` selectors occur in the WebSocket section. No new
  WebSocket transport or query-selector SSE route is inferred.

SDK decisions: `Hashtag` also rejects empty names, whitespace, controls and dot segments before
construction; other punctuation is encoded safely and subject to server acceptance. No Unicode
normalization, tag-count truncation, reconnect, replay, deduplication or exactly-once promise.

`tests/hashtags.rs` reuses the existing synthetic status/timeline and error fixtures and adds
inline local responses for encoded names, extra tags, comments, multiline data and a broken body
after split UTF-8 / `data:`. `tests/hashtag_use.rs` uses synthetic statuses (`id=7`), advertises a
distinct loopback `ws://` origin, splits an SSE field with a 5 ms fixture delay, and returns
update/edit/delete events followed by EOF. Timings, access refusals and all content are fixture
choices, not claims about a live instance. **Live provider calls: NOT_RUN.**

No dependencies were added; no crate version or lockfile entry changed. The workspace member
and package lock entry were already present. The first maintenance build was offline without
`--locked`; subsequent resolving cargo commands use `--offline --locked`.

### Original implementation research (retained)

Both pages were read on **2026-11-07** with the session's `WebFetch` tool, which returns a model's
summary of the page with short literal quotes, not the raw page. Facts below marked *quoted* come
from those literal quotes; the others from the summary. No other page was read: the brief limits
reading to these two.

| Source | Page date | Used for |
|---|---|---|
| <https://docs.joinmastodon.org/methods/timelines/> ("View public timeline") | "Last updated October 19, 2026" | `GET /api/v1/timelines/public`; returns an array of Status; OAuth public, app token + `read:statuses` if the instance disabled public preview; the `Authorization: Bearer <user_token>` header (*quoted*: "Provide this header with `Bearer <user_token>` to gain authorized access to this API method."); query parameters `local`, `remote`, `only_media` (booleans, default false), `max_id`, `since_id`, `min_id` (strings), `limit` (default 20, max 40); the `200` example (two statuses: ids `103206804533200177` and `103206804086086361`, `created_at` `2019-11-26T23:27:31.000Z` / `…32.000Z`, `visibility` `public`, other members elided); the note that the array may be empty if the feed is disabled; `401` when unauthenticated access is disabled, body `{"error": "The access token is invalid"}` (*quoted*); the `404` body `{"error": "Record not found"}` of the link and list timelines (*quoted*); no `Link` header is mentioned on the page |
| <https://docs.joinmastodon.org/methods/streaming/> ("Hostname discovery", "About HTTP server-sent events", "Watch the federated timeline", "Event types and payloads", "Error responses") | "May 1, 2026" | the streaming API may be served from another hostname, discovered from `configuration.urls.streaming` of the instance endpoint (linked as `../instance`), or by a request to `api/v1/streaming` that redirects or answers Not Found (not implemented); `GET /api/v1/streaming/public` with `only_media`; it returns `update`, `delete` and `status.update`; since 4.2.0 it requires a user token + `read:statuses`; `Authorization: Bearer <user_token>` preferred (`access_token` query "Legacy support only", not used); events as `event:` + `data:` lines; heartbeat comments, lines beginning with `:` to be ignored, example `:thump`; `update` / `status.update` payload a Status, `delete` payload the status id as text (example `data: 107214471804101576`); the federated-timeline examples (see the fixture table); `400` `{"error": "Unknown channel requested"}` for a non-existent streaming endpoint; no interval for heartbeats, no reconnection, replay or `Last-Event-ID` statement on the page |

### Facts the two pages do not state (labelled inferences)

| Fact used by the SDK | Where it comes from |
|---|---|
| `GET /api/v1/statuses/{id}` retrieves one status | prior knowledge of the Mastodon API; the statuses page was not read |
| a missing status answers `404` `{"error": "Record not found"}` | inferred from the documented 404 bodies of the timelines page; not documented for this endpoint on the pages read |
| the instance endpoint is `GET /api/v2/instance` | the streaming page names `configuration.urls.streaming` and links "instance endpoint" to `../instance`; the path is prior knowledge (the instance page was not read) |
| an advertised streaming URL may be `wss://` / `ws://` | prior knowledge (the page does not state the value's scheme); the SDK reads `wss` as `https` and `ws` as `http`, and accepts `https`/`http` too |
| the stream's `Content-Type` is `text/event-stream` | not on the Mastodon page; it is the server-sent events media type, which Concord's `Sse` format requires (exactly one `text/event-stream`, charset absent or `utf-8`) |
| SSE frames are separated by a blank line | the page's examples show each frame in its own code block without blank lines; server-sent events dispatch on a blank line (Concord's reader, which follows the WHATWG rules) |
| visibilities `private` and `direct` | prior knowledge of the Status entity (the pages show `public` and `unlisted`); unknown values are kept as `Visibility::Other` |
| the Status members modelled (`id`, `uri`, `url`, `created_at`, `edited_at`, `account`, `content`, `visibility`, `sensitive`, `spoiler_text`, `language`, `in_reply_to_id`, `in_reply_to_account_id`, `reblog`) and which are required | the members present in the streaming page's examples; required = the ones present and non-null in every example (`id`, `uri`, `created_at`, `account`, `content`, `visibility`) — an SDK decision, not a documented schema |
| the SDK's local rules (`TimelineLimit` 1–40, `StatusId` non-empty without whitespace or controls, `InstanceUrl` an origin, plaintext streaming refused for an `https` instance) | SDK decisions built on the documented maximum of 40, the opaque string ids and the documented absolute paths |

## Fixtures

| File | Source | Transformations | Kind |
|---|---|---|---|
| `public_timeline.json` | timelines page, `200` example (`limit=2`) | the documented ids, `created_at` values (kept as documented: the newer id has the earlier time) and `visibility`; the elided members (`uri`, `url`, `content`, `account`, …) filled with synthetic values on `mastodon.example` / `remote.example`; one line, trailing LF | documented example, completed synthetically |
| `status.json` | the first status of `public_timeline.json` | the same id and members, plus counters present in the streaming examples | synthetic, consistent with the documented page |
| `error_unauthorized.json` | timelines page, `401` example | the three-line rendering collapsed to one line, trailing LF | documented example |
| `error_not_found.json` | timelines page, `404` examples (link and list timelines) | one line, trailing LF; used for `GET /api/v1/statuses/{id}` by inference (above) | documented example, reused by inference |
| `error_unknown_channel.json` | streaming page, "Error responses" (`400`) | one line, trailing LF | documented example |
| `instance.json` | streaming page, "Hostname discovery" (the member path `configuration.urls.streaming`) | synthetic values (`mastodon.example`, `wss://streaming.mastodon.example`); `tests/workflows.rs` replaces the URL with `ws://<loopback streaming server>` | synthetic, documented member path |
| `stream_public_documented.txt` | streaming page, "Watch the federated timeline": the `update` (id `108914354907984653`), `delete` (`107214471804101576`) and `status.update` (id `109348684737626801`) examples, in that order; `:thump` from "About HTTP server-sent events" | each `data:` Status written as one JSON line with the documented `id`, `created_at`, `visibility`, `uri`, `url`, `in_reply_to_*`, `language`, `sensitive`, `spoiler_text`, `edited_at`, `reblog`, `account.id`/`username`/`acct`/`url`, empty `media_attachments`; the `content` (truncated by the tool) replaced by synthetic text containing multi-byte UTF-8 (`これは合成テキストです 🐘`, `SYNTHETIC edited text`); `display_name` synthetic; a blank line after each frame and `:thump` lines added; LF line ends | documented example, completed synthetically |

## Synthetic cases in the tests

- `tests/protocol.rs`: a `429`, a `502` HTML page, an error body over 64 KiB, an error document
  labelled `text/plain`, a non-JSON error body, an empty `503`; `401` from the streaming host
  without a token (the page says the stream requires one, not what the refusal looks like);
  malformed / wrong-shape / empty-id timeline bodies (with a `SECRETPAYLOAD` marker that must not
  appear in any error text); a body cut off mid-document; a 9 MiB generated body; success bodies
  labelled `text/html`, `text/plain` or unlabelled; streams labelled `application/json`,
  `text/plain` or `charset=iso-8859-1`; CRLF line ends, multi-line `data`, an unknown event type;
  undecodable payloads; invalid UTF-8; bodies ending inside an event; a 1 MiB+ event; a
  connection dropped mid-event; a `302` to another origin; the cut positions of the split tests.
- `tests/workflows.rs`: the stand-ins' access rules (the instance refuses its public timeline
  without the exact token, the streaming host requires it), the advertised `ws://` loopback URL,
  the dropped fourth event, and the heartbeat timing (ten `:thump` lines 10 ms apart, then
  silence with the connection open). These model the documented rules; their exact behaviour is
  this suite's choice.
- `tests/common/mod.rs`: `status_json` builds synthetic statuses in the shape of the documented
  examples; `TOKEN` is a synthetic value used to check redaction.
