# Provenance

Every fixture under `fixtures/` and every inline response in `tests/` is either hand-authored from
the sources below or synthetic. **None was captured from a real ntfy server**, no request was made
to ntfy.sh or any other ntfy instance, and the tests prove nothing about the real service beyond
agreement with the documented shapes.

## Sources checked (2026-11-06)

All pages were read on **2026-11-06** with the session's `WebFetch` tool, which returns a model's
summary of the page with short literal quotes, not the raw bytes. Example lines below were taken
from those literal quotes; where the tool said the rendered page drops blank lines, that is noted.

| Source | Version / date | Used for |
|---|---|---|
| <https://docs.ntfy.sh/subscribe/api/> | no version or date on the page; the docs site at the access date lists **v2.37.2** (released 2026-11-03) as the latest release | the `/json` and `/sse` stream examples, `poll=1` (defaults to `since=all`), the `since` forms (duration, Unix timestamp, message id, `all`, `latest`), the JSON message format (`id`, `time`, `event`, `topic` required; `expires`, `sequence_id`, `message`, `title`, `tags`, `priority` 1–5, `click`, `actions`, `attachment`), the event types (`open`, `keepalive`, `message`, `message_delete`, `message_clear`, `poll_request`), the replay cap (10 MB per topic, `X-Messages-Truncated: 1`, HTTP 429 / code 42905), content types `application/x-ndjson` and `text/event-stream`, `expires` absent with `Cache: no` |
| <https://docs.ntfy.sh/publish/> | as above | publishing with `PUT`/`POST /<topic>` and the body as message; `X-Title`, `X-Priority` (`min`=1 … `max`/`urgent`=5), `X-Tags` (comma-separated) and their aliases; access tokens as `Authorization: Bearer …`; `X-Cache: no`; the 12-hour default cache; the publish answer example; Limitations (4,096-byte messages, longer ones become attachments; title 1 KB and tags 512 bytes, else HTTP 400) |
| <https://docs.ntfy.sh/releases/> | top released entry **v2.37.2**, 2026-11-03 (v2.38.0 listed as unreleased) | the version pin (`DOCUMENTED_VERSION`) |
| <https://raw.githubusercontent.com/binwiederhier/ntfy/main/server/errors.go> | `main` branch at the access date; the tool shows no commit id, so none is recorded | the error document shape (`code` omitted when zero, `http`, `error`, `link` omitted when empty) and the values of `40101 unauthorized`, `40301 forbidden`, `42901 limit reached: too many requests` with their links. **Not in the two docs pages.** |
| <https://raw.githubusercontent.com/binwiederhier/ntfy/main/server/server.go> | as above | facts the docs pages do not state: the response `Content-Type` is the mode's type plus `; charset=utf-8`; the SSE writer sends `event: <kind>` before `data:` except for `message`, `message_delete` and `message_clear`; an error answer is `application/json` with the document and a newline; a poll sends no `open` event; `since` is parsed as `all`/`latest`/`none`, then a message id, then an integer timestamp, then a Go duration; the topic pattern `^[-_A-Za-z0-9]{1,64}$`; `X-Messages-Truncated` is set when the replay is capped; `X-Cache` is a boolean parameter |
| <https://html.spec.whatwg.org/multipage/server-sent-events.html> | "Last Updated 5 November 2026" | the SSE framing rules (§9.2.5–9.2.6): CRLF/LF/CR line ends, one leading BOM stripped, `:` comments, field splitting with one leading space removed, `event`/`data`/`id`/`retry`, dispatch on an empty line (nothing when the data buffer is empty, one trailing LF removed), a pending event discarded at end of file |

A Debian source mirror of `errors.go` (`sources.debian.org/src/ntfy/2.11.0-3/...`) was tried and
answered 404; it was not used.

## Fixtures

| File | Source | Transformations | Kind |
|---|---|---|---|
| `sse_documented.txt` | subscribe/api, "HTTP stream" `/sse` curl output: the five `event:`/`data:` lines | an empty line added after each event (the rendered page shows none; SSE needs them, and the server writes `data: <json>` followed by its JSON encoder's newline and its own: inferred from the source quote, the encoder itself was not read); the trailing `...` line removed; LF line ends. The keepalive's `"topic":"test"` is kept as documented | documented example |
| `json_documented.ndjson` | subscribe/api, "HTTP stream" `/json` curl output: three lines | the trailing `...` line removed; LF after each line | documented example |
| `publish_response.json` | publish page, the JSON returned by a publish (`"message":"Downloading file..."`) | a trailing LF added (inferred from the server's JSON encoder, not observed) | documented example |
| `error_unauthorized.json`, `error_forbidden.json`, `error_rate_limited.json` | `server/errors.go` (struct tags and the three variables) | hand-assembled into the JSON the struct serializes to, field order as declared, trailing LF (the server's error writer) | hand-authored from source, not from the docs |
| `sse_perturbed.txt` | layout as ntfy writes it (above) | synthetic ids (`syn…`) and times (2026); an unknown member `unknown_future_field`, a 4-byte UTF-8 character, `title`, `tags`, `priority`, `expires` = `time` + 12 h | synthetic |
| `maintenance_replay.jsonl` | ntfy subscription documentation JSON message shape | hand-authored synthetic replay: repeats the message from `sse_perturbed.txt`, including its unknown optional member, and adds a second message; LF line ends. The repeated ID is a test scenario, not an observed server response | synthetic |
| `sse_crlf_bom.txt` | WHATWG SSE rules | synthetic: a leading BOM before a `data:` line, CRLF and lone-CR line ends, a comment, `id:`/`retry:`/unknown fields, one event in two `data:` lines. ntfy sends none of these; they exercise the framer | synthetic |

`.gitattributes` marks `fixtures/** -text` so a checkout keeps these bytes (CR, BOM) unchanged.

## Synthetic cases in the tests

- `tests/protocol.rs`: a `message_delete` without an SSE `event:` line and an unknown
  `future_event` kind; malformed JSON, a value of the wrong shape and invalid UTF-8 as `data`
  (with a `SECRETPAYLOAD` marker that must not appear in any error text); a lazily generated
  event over 16 MiB; bodies cut inside an event or dropped mid-body; `Content-Type` variants;
  a poll with `X-Messages-Truncated: 1`, a blank NDJSON line and an unknown member; a `429` with
  `Retry-After: 30` (the source mentions `Retry-After` for 429 decisions; the value is synthetic);
  a `502` HTML page; an error body over 64 KiB; a connection closed without an answer.
- `tests/workflows.rs` and `tests/common/mod.rs`: keepalive generators (every 10–20 ms; the real
  default interval is not documented on these pages) and the stateful fake server. Its clock, ids,
  auth rule (exact bearer value, else `401`/`403` with the documents above) and replay rules are
  this suite's choices. In particular **`since=<id>` returns the messages after that id and
  `since=<time>` the messages at or after that second**: the documentation does not state either
  boundary, so the duplicate the workflow deduplicates comes from the fake's inclusive choice.
- That `open` and `keepalive` ids are not cached messages (so `ReplayCursor` ignores them) is an
  inference from the documentation (only messages are cached; the examples show fresh ids on each
  `open`/`keepalive`) and from the source (they are created per subscription), not a documented
  rule.
