# concord-sdk-ntfy

A small SDK for [ntfy](https://docs.ntfy.sh) declared with `concord::api!`. It publishes a message
to a topic, subscribes to the topic as **Server-Sent Events** or **line-delimited JSON**, and
replays the server's message cache with a `since` cursor. Checked against the documentation of
ntfy **v2.37.2** (see [PROVENANCE.md](PROVENANCE.md)). Each generated `send()` names `Result<T, NtfyError>`.

| Covered | Not covered |
|---|---|
| `POST /<topic>` with the text as body, `X-Title`, `X-Priority`, `X-Tags`, `X-Cache: no` | WebSockets, the raw stream (`/raw`) |
| `GET /<topic>/sse` (SSE subscription) | attachments (a body over 4,096 bytes is refused, see below), actions, icons, e-mail, phone calls |
| `GET /<topic>/json` (JSON subscription) | scheduled delivery (`X-Delay`, `scheduled=1`), filters (`id`, `message`, `title`, `priority`, `tags`) |
| `GET /<topic>/json?poll=1` (cached messages, then the end) | multi-topic subscriptions (`/a,b/sse`), push providers, accounts and reservations |
| `since=` as `all`, `latest`, a message id, a Unix time, or seconds ago | publishing as JSON to the root URL, `Basic` credentials helper (the raw `set_authorization` setter takes any header value) |
| an optional bearer access token | any delivery guarantee beyond the server's own (see [Completion](#completion-and-partial-outcomes)) |

## Construction

```rust,no_run
use concord::reqwest;
use concord_sdk_ntfy::{AccessToken, Ntfy, NtfyTopic, Topic};

fn alerts() -> Result<NtfyTopic, Box<dyn std::error::Error>> {
    // Timeouts, proxies and TLS are your reqwest::Client's. Do not set a total request timeout
    // on a client used for subscriptions: a subscription is meant to stay open.
    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;
    let ntfy = Ntfy::new(http)                            // https://ntfy.sh
        .with_base_url("https://ntfy.example.test")?      // or a self-hosted server
        .with_token(&AccessToken::new("synthetic-token-not-real")?); // optional
    Ok(ntfy.topic(Topic::new("alerts")?))
}
```

- `Ntfy` is the generated client; handles (`Ntfy`, `NtfyTopic`, each request) are cheap
  snapshots. `with_token` sends `Authorization: Bearer <token>` on every request, flagged
  sensitive: no `Debug` of a client, handle, request or error prints it, and `AccessToken` has no
  `Display`.
- `Topic::new` accepts the server's topic pattern (1–64 of `A–Z a–z 0–9 - _`); a name with `/` or
  `,` never reaches a request. `MessageText::new` refuses more than 4,096 bytes (the documented
  limit: ntfy would turn a longer body into an attachment). `Tags::new` refuses empty tags and
  tags containing the `,` separator. `EventId::new` refuses `all`, `latest` and `none`, which
  `since=` would read as keywords.

## Workflow A: subscribe until cancelled

`subscribe_sse().send().await?` returns the open `Events` reader as soon as the response head is
accepted (`text/event-stream`, charset absent or UTF-8), without reading the body. `next()`
returns every event — `open`, `message`, `keepalive`, and kinds the SDK does not know
(`EventKind::Other`) — and is **cancel-safe**, so cancellation is an ordinary `select!`. Dropping
the reader closes the connection.

```rust,no_run
use concord_sdk_ntfy::{EventKind, NtfyTopic, ReplayCursor};
use concord::stream::ReadErrorKind;

async fn follow(
    alerts: NtfyTopic,
    stop: tokio::sync::oneshot::Receiver<()>,
) -> Result<ReplayCursor, Box<dyn std::error::Error>> {
    let mut cursor = ReplayCursor::new();
    tokio::pin!(stop);
    loop {
        // The first connection has no cursor; a reconnect asks for what it missed.
        let mut events = alerts.subscribe_sse().maybe_since(cursor.since()).send().await?;
        let reconnect = loop {
            tokio::select! {
                biased;
                _ = &mut stop => break false,
                event = events.next() => match event {
                    Some(Ok(event)) => {
                        cursor.observe(&event);
                        if event.kind == EventKind::Message {
                            println!("{}", event.message.unwrap_or_default());
                        }
                    }
                    // A lost connection: reconnect from the cursor (bound the attempts yourself).
                    Some(Err(lost)) if lost.kind() == ReadErrorKind::Transport => break true,
                    Some(Err(other)) => return Err(other.into()),
                    None => break true, // the server closed the stream
                },
            }
        };
        drop(events); // closes this connection
        if !reconnect {
            return Ok(cursor);
        }
    }
}
```

`ReplayCursor::since()` is the id of the last `message` received (`since=<id>`), or before any
message the time of the first `open` (`since=<unix time>`), or `None`. The server selects the
replay; callers must allow it to include messages already received. `open` and `keepalive` ids
are per connection and never move it. The loop, its reconnect budget and its backoff are the application's: the SDK never reconnects by itself
(`tests/workflows.rs`, `follow`, adds a bounded budget).

## Workflow B: publish while offline, then replay the cache

```rust,no_run
use std::collections::HashSet;

use concord::stream::Completion;
use concord_sdk_ntfy::{Cache, Event, EventId, MessageText, NtfyTopic, Priority, Since};

async fn catch_up(
    alerts: NtfyTopic,
    last_seen: EventId,
    seen: &mut HashSet<EventId>,
) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
    // Published while the subscriber is away: kept in the server's cache (the answer is the
    // stored message, with its `id`, `time` and `expires`)...
    let _kept: Event = alerts
        .publish(MessageText::new("Disk full")?)
        .set_title("Backup")
        .set_priority(Priority::High)
        .send()
        .await?;
    // ...or delivered to connected subscribers only, never replayed.
    let _live_only = alerts.publish(MessageText::new("ping")?).set_cache(Cache::No).send().await?;

    // Back online: request a JSON poll from the last message seen.
    let mut replay = alerts.poll().set_since(Since::Id(last_seen)).send().await?;
    let mut new = Vec::new();
    while let Some(event) = replay.next().await {
        let event = event?;
        // A replay can repeat a message (a timestamp cursor, the server's own behaviour):
        // skipping it is this caller's decision.
        if seen.insert(event.id.clone()) {
            new.push(event);
        }
    }
    assert_eq!(replay.completion(), Completion::EndOfInput); // the server ended the poll
    if replay.truncated() {
        // X-Messages-Truncated: 1 — older messages were dropped to fit the replay size cap.
    }
    Ok(new)
}
```

`poll()` reads `GET /<topic>/json?poll=1` with Concord's NDJSON reader (`application/x-ndjson`,
blank lines skipped, chunk boundaries anywhere). `subscribe_json()` is the same reader on a live
subscription. Both yield the same `Events` type as SSE after `.send().await?`.

## Errors

| Where | Type | What it keeps |
|---|---|---|
| a domain value | `InvalidTopic`, `InvalidToken`, `MessageTooLong`, `InvalidTag`, `InvalidEventId` | the reason, never the refused value (except the body's length) |
| any `send()` | `NtfyError::Api(ApiError)` | a non-2xx: the status, ntfy's error document (`code` such as `40101`, `40301`, `42901`; `http`; `error`; `link`) when the body was one (read up to 64 KiB, `ERROR_BODY_LIMIT`), one unsigned decimal `Retry-After` field (OWS allowed; signs/repeats/overflow refused), the endpoint |
| any `send()` | `NtfyError::Request(concord::Error)` | Concord's error, whole: not built, no response (`Transport`), a 2xx head refused before its body was read (`Metadata`, e.g. a `/sse` answered as `application/json`; the response kept unread), a publish answer that does not decode |
| reading `Events` | `EventsError` | `kind()` (`Transport`, `Format`, `Decode`, `Limit`), the endpoint, events delivered and bytes read, a static label with `event`/`line`/`column` positions, and the unredacted parser or transport error on request (`source_unredacted`) |

`Display` of every error shows the endpoint, the stage, the status and numeric codes — never the
URL, the token, a header value, the document's `error` text or event content. `ApiError::is_auth()`
(`401`/`403`) and `is_rate_limited()` (`429`) classify common answers.

## Completion and partial outcomes

- **Subscriptions do not end by themselves.** A caller that stops reading (cancellation, a
  timeout) leaves `completion()` at `Reading`; that is not an end of input. `EndOfInput` means the
  server closed the body cleanly at an event boundary.
- **A poll ends.** `EndOfInput` after a poll means every message the server *chose to return* was
  read: what its cache still holds (12 hours by default), minus `Cache: no` messages, minus what
  the per-topic replay cap removed (`truncated()`). It is not complete history.
- **Failures are terminal and never look like an end.** NDJSON accepts a complete final JSON
  value without LF at EOF; an incomplete value fails. A broken connection (`Transport`), a
  body that ends inside an SSE event or JSON value (`Format`), malformed SSE or JSON (`Format`), a
  well-formed value that is not an ntfy message (`Decode`) or an event over 16 MiB
  (`MAX_EVENT_BYTES`, `Limit`) end the reader with `Completion::Failed`. Events already returned
  stay returned; the cursor says where to resume.
- **No deduplication, no exactly-once.** A reconnect from a timestamp, or the server itself, can
  deliver a message again; `Event::id` is the key a caller deduplicates by, if it wants to.
- In SSE, the JSON payload's `event` member decides the kind. ntfy writes an SSE `event:` line
  only for some kinds (not for `message`, `message_delete`, `message_clear`), so the SSE event
  type is not used; `id:` and `retry:` lines are ignored (ntfy sends none; the replay cursor is the
  JSON `id`, sent back as `since=`).

## Limitations

- The SSE framing is Concord's reader (`concord::sse`; this crate decodes each event's `data`, see
  `INTEGRATION.md`). It follows the WHATWG event-stream rules for line ends, a leading byte order
  mark, comments, multi-line `data` and unknown fields, and departs from a browser deliberately:
  invalid UTF-8, a body that ends inside a line or an event, and an event over the bound are
  errors instead of a silent repair or discard.
- The per-event bound is fixed at 16 MiB (the same as Concord's NDJSON default), far above ntfy's
  documented limits; it is not configurable.
- `Since::Ago(n)` is sent as `<n>s`; `Since::Time` is interpreted by the server, whose inclusive
  or exclusive treatment of that second the documentation does not state.
- The 4,096-byte message limit is the documented default; a server configured with a larger limit
  still gets at most 4,096 bytes from this SDK.
- `Retry-After` is read in its delta-seconds form only.

## Tests and fixtures

```sh
cargo test --locked -p concord-sdk-ntfy
```

`tests/protocol.rs` checks the wire (method, target, headers, exact body bytes), the documented
success and error shapes and the negative cases; `tests/workflows.rs` runs both workflows through
the public API against the loopback server of `examples/testserver`, including a gated split SSE
message and delimiter followed by a keepalive and a replay that repeats an already delivered
message. The caller filters that duplicate by ID. The suite also uses a stateful in-memory fake (`tests/common/mod.rs`) with a cache, live SSE subscribers and an auth rule.

**The fixtures are not vendor conformance.** They are hand-authored from the documentation's
examples or synthetic (see `PROVENANCE.md`), the fake server's replay rules are this test suite's
choices, and nothing here ran against a real ntfy server.
