# concord-sdk-mastodon

The declaration owns the provider error document: `error crate::ErrorDocument in concord::Json;`. Native prepared/decoded
paths carry that document as `D` in `Prepared<T, D>` and `concord::Error<D>`; SDK status routing
and validation remain in the policy. Concord withholds document contents in formatting.

A bounded [Mastodon](https://docs.joinmastodon.org) SDK slice built on Concord's `concord::api!`.
It covers two connected workflows against the caller's instance:

1. **Read the public timeline, then retrieve one status** (`GET /api/v1/timelines/public`,
   `GET /api/v1/statuses/{id}`).
2. **Discover the streaming host, then consume the public timeline as server-sent events**
   (`GET /api/v2/instance` → `configuration.urls.streaming`, then
   `GET /api/v1/streaming/public` on that host).

Hashtag timelines and both federated and local hashtag SSE streams are also supported (see
"Hashtags" below). They use the same status model, instance binding and stream reader.

Every test runs against local fixtures (`examples/testserver`). **Live provider calls: NOT_RUN** —
no request was ever made to a real Mastodon instance; the sources and every fixture are listed in
[`PROVENANCE.md`](PROVENANCE.md).

**Excluded:** posting and every other write, WebSocket streaming, other timelines and streams
(home, standalone local, list, user), federation semantics, OAuth flows (obtaining a token),
rate-limit headers, `Link` header pagination, and any exactly-once, replay or resumption
guarantee for streamed events.

## Calling it

The instance is chosen at run time. A client is bound to it: its token goes to that instance and
to the streaming host that instance advertises, nowhere else.

```rust
use concord_sdk_mastodon::concord::reqwest;
use concord_sdk_mastodon::{AccessToken, InstanceUrl, Mastodon, StatusId, TimelineLimit};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Transport settings (timeouts, proxies, TLS) belong to your reqwest::Client.
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let instance = InstanceUrl::parse("https://mastodon.example")?;
    let mastodon = Mastodon::new(http, instance)
        .with_token(&AccessToken::new("synthetic-token-not-real")?);

    // The generated request values: setters, then `send()` (or `prepare()`, shown here).
    let page = mastodon
        .public_timeline()
        .set_only_media(true)
        .set_max_id(StatusId::new("103206804533200177")?)
        .set_limit(TimelineLimit::new(40)?)
        .prepare()?;
    assert_eq!(
        page.request().url().as_str(),
        "https://mastodon.example/api/v1/timelines/public\
         ?only_media=true&max_id=103206804533200177&limit=40"
    );
    let status = mastodon.status(StatusId::new("103206804533200177")?).prepare()?;
    assert_eq!(status.request().url().path(), "/api/v1/statuses/103206804533200177");
    Ok(())
}
```

| Method | Request | Success value |
|---|---|---|
| `public_timeline()` + `set_local`, `set_remote`, `set_only_media`, `set_max_id`, `set_since_id`, `set_min_id`, `set_limit` | `GET /api/v1/timelines/public` | `Vec<Status>` |
| `hashtag_timeline(tag)` + `set_any`, `set_all`, `set_none` and all public timeline setters | `GET /api/v1/timelines/tag/{hashtag}` | `Vec<Status>` |
| `status(id)` | `GET /api/v1/statuses/{id}` | `Status` |
| `instance()` | `GET /api/v2/instance` | `InstanceInfo` |
| `discover_streaming()` (async) | `GET /api/v2/instance` | `StreamingHost` |
| `public_stream(&host)?` + `set_only_media` | `GET /api/v1/streaming/public` on the streaming host | an open `PublicStream` |
| `hashtag_stream(&host, tag)?` | `GET /api/v1/streaming/hashtag?tag=…` on the streaming host | an open `PublicStream` |
| `local_hashtag_stream(&host, tag)?` | `GET /api/v1/streaming/hashtag/local?tag=…` on the streaming host | an open `PublicStream` |

- **Values.** `InstanceUrl::parse` accepts an `http`/`https` origin only (no path, query,
  fragment or user information; the input is never echoed in its error). `AccessToken::new` uses
  `concord_auth::bearer` (non-empty visible ASCII, no spaces; the header is flagged sensitive).
  `StatusId` is an opaque string, non-empty, without whitespace or control characters.
  `TimelineLimit` is 1 to 40 (the documented maximum); 0 or 41 is refused before anything is
  sent. Unset timeline parameters are not sent: the server's defaults apply.
- **Paging** is the documented id bounds: the next older page is
  `.set_max_id(last.id.clone())`. The `Link` header is not read.
- **An empty page** is a page: the documentation notes that an instance that disabled this feed
  answers an empty array.

### Hashtags

```rust,no_run
use concord_sdk_mastodon::{Hashtag, Mastodon, TimelineLimit};

async fn follow_tag(mastodon: Mastodon) -> Result<(), Box<dyn std::error::Error>> {
    let tag = Hashtag::new("café")?; // name only, without #
    let page = mastodon.hashtag_timeline(tag.clone())
        .set_any(vec![Hashtag::new("rust")?, Hashtag::new("sdk")?])
        .set_all(vec![Hashtag::new("code")?])
        .set_none(vec![Hashtag::new("spoiler")?])
        .set_local(true)
        .set_limit(TimelineLimit::new(20)?)
        .send().await?;
    if let Some(first) = page.first() {
        let status = mastodon.status(first.id.clone()).send().await?;
        println!("{}", status.content);
    }
    let host = mastodon.discover_streaming().await?;
    let mut stream = mastodon.hashtag_stream(&host, tag.clone())?.send().await?;
    while let Some(event) = stream.next().await {
        println!("{event:?}"); // a terminal error is returned once, then None
    }
    // An explicit, separate subscription limited to statuses originating on this instance.
    let mut local = mastodon.local_hashtag_stream(&host, tag)?.send().await?;
    let _first_local_event = local.next().await;
    Ok(())
}
```

`Hashtag::new` rejects empty names, `#`, whitespace, controls and `.` / `..` before a request can
be built. Case and Unicode are preserved; other punctuation is safely encoded rather than
treated as a path or query delimiter. The instance still decides whether a name is acceptable.
Extra tags use `Vec<Hashtag>` and repeated `any[]`, `all[]`, `none[]` query keys. Unset or empty
lists send no pairs. The instance controls how many extra tags it applies (documented default:
four total); the SDK does not truncate lists.

The timelines page specifies the hashtag path, filters, paging and instance-dependent access.
The streaming page names the categories `hashtag` and `hashtag:local`; their HTTP SSE routes are
the two paths above. The `stream=hashtag&tag=…` selector is described under WebSocket connections
on that page; this SDK uses its documented HTTP SSE endpoints. Sources read **2026-11-07**:
[timelines API methods](https://docs.joinmastodon.org/methods/timelines/#view-hashtag-timeline),
[streaming API methods](https://docs.joinmastodon.org/methods/streaming/#watch-the-public-timeline-for-a-hashtag).
Both hashtag streams share `PublicStream`'s framing, terminal errors and no-replay contract.

### The streaming host

```rust,no_run
use concord_sdk_mastodon::{Mastodon, StreamEvent};

async fn follow(mastodon: Mastodon) -> Result<(), Box<dyn std::error::Error>> {
    let host = mastodon.discover_streaming().await?; // reads configuration.urls.streaming
    let mut stream = mastodon.public_stream(&host)?.send().await?;
    while let Some(item) = stream.next().await {
        match item {
            Ok(StreamEvent::Update(status)) => println!("new {}", status.id),
            Ok(StreamEvent::StatusUpdate(status)) => println!("edited {}", status.id),
            Ok(StreamEvent::Delete(id)) => println!("deleted {id}"),
            Ok(_) => {} // another event type, kept as text
            Err(error) if !error.is_terminal() => eprintln!("skipped: {error}"),
            Err(error) => eprintln!("stopped: {error}"), // the loop ends after this
        }
    }
    Ok(())
}
```

- `discover_streaming` reads `configuration.urls.streaming` from the instance document and
  applies `StreamingHost::advertised_by`: `wss://` is read as `https://` and `ws://` as `http://`
  (the stream is HTTP server-sent events), only an origin is accepted, and a plaintext host
  advertised by an `https` instance is refused (`DiscoveryError::Insecure`): the token would
  travel in clear. An instance that advertises nothing is `DiscoveryError::NotAdvertised`;
  nothing is guessed. `StreamingHost::instance_host(&instance)` is the explicit choice for an
  instance known to stream on its own host.
- A `StreamingHost` remembers the instance that advertised it. Every stream method on a client
  of another instance is `DiscoveryError::OtherInstance`, and nothing is sent.
- These streams require a user token (documented since Mastodon 4.2.0): without one, the server's
  `401` is `Error::Api`.

## Failure semantics

Every `send()` (and `discover_streaming`) returns `Result<_, concord_sdk_mastodon::Error>`:

| Variant | When | What is kept |
|---|---|---|
| `Error::Api(ApiError)` | any non-2xx answer (`401` invalid or missing token, `404` no such status, `400` unknown stream, `429`, a proxy's `5xx`, ...) | the status, the endpoint, and Mastodon's error document `{"error": ...}` when the body was one (bounded to 64 KiB, labelled JSON); otherwise `document()` is `None` and the status still stands |
| `Error::Request(concord::Error)` | `Build` (a value that cannot form the request, nothing sent), `Transport` (no answer, or the body broke off), `Metadata` (a 2xx not labelled JSON, a stream not labelled `text/event-stream`; the response kept unread), `Decode` (a 2xx body that is not the documented shape), `Limit` (a document over 8 MiB) | Concord's error, whole and redacted |
| `Error::Discovery(DiscoveryError)` | no usable streaming host, or a host of another instance | the rule that failed |

`Display` never shows the token, a header value, the URL or the error document's text
(`ApiError::message()` reads it deliberately).

### The stream

Concord's SSE reader frames the body: an event, a `data:` field or a UTF-8 character split across
chunks is reassembled; comments (the documented `:thump` heartbeats) are not events. The SDK
decodes `data` by event type (`update`, `status.update`: a status; `delete`: a status id; any
other type is `StreamEvent::Other`, undecoded).

Public and hashtag streams have no end of their own, so **every way they stop is an error**, returned once;
`next()` then returns `None`:

| `StreamErrorKind` | Meaning | Terminal |
|---|---|---|
| `Disconnected` | the connection broke mid-body (`transport()` gives Concord's error, no URL) | yes |
| `Closed` | the server ended the body at an event boundary: a disconnect, not a completion | yes |
| `Truncated` | the body ended inside a line or an event: that event is lost | yes |
| `Malformed` | invalid UTF-8 | yes |
| `TooLarge` | an event over 1 MiB (never truncated) | yes |
| `Payload` | one event's `data` is not a status (or not an id): that event is skipped, reading goes on | no |

**No replay.** The SDK never reconnects. The Mastodon documentation describes no event ids or
`Last-Event-ID` resumption for this stream, so events sent while no connection was open are not
recoverable from the stream, and no exactly-once claim is made. Reconnecting, and deciding what
a gap means for the application, is the caller's policy.

**Liveness.** Heartbeats are not events, but they advance `PublicStream::position().bytes`, and
`next()` is cancel-safe. An idle rule is therefore caller code: wrap `next()` in a timeout; when
it fires, the stream is quiet if the position moved, dead if it did not
(`tests/workflows.rs`, `heartbeats_let_a_caller_tell_a_quiet_stream_from_a_dead_one`). The
heartbeat interval is not documented.

## Token destinations

- The token is a sensitive `Authorization: Bearer` header, sent to the client's instance and to
  the streaming host discovered from it. Separate generated REST and streaming handles each
  declare `base origin;`; neither can move its credential to another host. The wrapper checks
  that the streaming host belongs to this instance before constructing the second handle.
- reqwest's default redirect policy drops `Authorization` on a cross-origin hop (tested); a
  redirect policy of yours must keep that rule.
- `prepare()` returns the native request with the header: if you redirect that request
  elsewhere, that is your decision.

## Running the tests

```sh
CARGO_BUILD_JOBS=4 cargo test --offline --locked -p concord-sdk-mastodon
CARGO_BUILD_JOBS=4 cargo clippy --offline --locked -p concord-sdk-mastodon --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --offline --locked -p concord-sdk-mastodon --no-deps
```

`tests/protocol.rs` covers the wire shapes, every documented error class, malformed, truncated,
oversized and wrong-media bodies, the stream's framing and payload rules, the token's
destinations and redaction; `tests/workflows.rs` runs both workflows end to end, including the
required failure (a UTF-8 character and a `data:` field split across chunks, then a dropped
connection, surfaced, with one request and no reconnect).
`tests/hashtags.rs` adds hashtag validation, encoding, bracketed extra-tag filters, stream host
binding, access errors and split UTF-8/fields followed by disconnect for both hashtag categories.
`tests/hashtag_use.rs` is a consumer of the new timeline → status → discovery → hashtag SSE flow;
the original `tests/first_use.rs` remains unchanged.
