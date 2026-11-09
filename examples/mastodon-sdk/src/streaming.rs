//! The streaming API: where it is served ([`StreamingHost`]) and public and hashtag streams
//! ([`PublicStream`], [`StreamEvent`], [`StreamError`]).
//!
//! # The streaming host
//!
//! An instance may serve its streaming API from another host than its REST API, and advertises
//! it as `configuration.urls.streaming` in its instance document.
//! [`Mastodon::discover_streaming`](crate::Mastodon::discover_streaming) reads it and applies
//! [`StreamingHost::advertised_by`]: a `wss` URL is read as `https` and `ws` as `http` (the
//! stream is HTTP server-sent events, not a WebSocket); only an origin is accepted; a plaintext
//! host advertised by an `https` instance is refused, since the token would travel in clear. The
//! result is bound to the instance it came from: a [`Mastodon`](crate::Mastodon) client for
//! another instance refuses it before sending anything.
//!
//! # The stream
//!
//! The body is `text/event-stream`. Concord's SSE reader frames it: an event, a `data:` field or
//! a UTF-8 character split across chunks is reassembled; comment lines (the `:thump` heartbeats)
//! and blank lines are not events. This module decodes each event's `data`:
//!
//! | SSE `event` | [`StreamEvent`] | `data` |
//! |---|---|---|
//! | `update` | [`Update`](StreamEvent::Update) | a status (JSON) |
//! | `status.update` | [`StatusUpdate`](StreamEvent::StatusUpdate) | the edited status (JSON) |
//! | `delete` | [`Delete`](StreamEvent::Delete) | the id of the deleted status (plain text) |
//! | anything else | [`Other`](StreamEvent::Other) | kept as text |
//!
//! # What a stream proves, and what it does not
//!
//! The stream has no end of its own: every way it stops is a [`StreamError`], returned once,
//! after which [`PublicStream::next`] returns `None`. A lost connection is
//! [`Disconnected`](StreamErrorKind::Disconnected); a body the server ended at an event boundary
//! is [`Closed`](StreamErrorKind::Closed) — a disconnect too, not a completion. Nothing is
//! replayed: the SDK never reconnects, Mastodon documents no event ids or `Last-Event-ID`
//! resumption for this stream, and events sent while no connection was open are not recoverable
//! from the stream. Whether and when to reconnect is the caller's decision. A payload that does
//! not decode is reported ([`Payload`](StreamErrorKind::Payload)) and reading goes on: the
//! framing is intact, only that event is lost.

use std::fmt;

use concord::bytes::Bytes;
use concord::reqwest::Url;
use concord::sse::{DEFAULT_MAX_EVENT_BYTES, Sse, SseEvents, SseOptions, SseReader};
use concord::stream::{ByteSource, Position, ReadError, ReadErrorKind, SourceError};
use concord::{CodecError, Incoming, Incremental, ResponseHead};

use crate::model::{InstanceUrl, Status, StatusId, check_origin};

/// The largest event the stream reader assembles (its `data` and event type): Concord's
/// default, 1 MiB. A larger event ends the stream ([`StreamErrorKind::TooLarge`]).
pub const MAX_EVENT_BYTES: usize = DEFAULT_MAX_EVENT_BYTES;

/// Where an instance serves its streaming API: an `http`/`https` origin, bound to the instance
/// that advertised it (see the [module](self)).
#[derive(Clone, PartialEq, Eq)]
pub struct StreamingHost {
    instance: InstanceUrl,
    base: Url,
}

impl StreamingHost {
    /// The streaming host `instance` advertises as `advertised` (its
    /// `configuration.urls.streaming`): `wss` is read as `https`, `ws` as `http`; `https` and
    /// `http` are kept. Use it only with a value read from that instance:
    /// [`Mastodon::discover_streaming`](crate::Mastodon::discover_streaming) does.
    ///
    /// # Errors
    ///
    /// [`DiscoveryError::NotAdvertised`] for an empty value; [`DiscoveryError::Unsupported`] for
    /// another scheme, or anything but an origin (a path, a query, user information);
    /// [`DiscoveryError::Insecure`] for a plaintext host advertised by an `https` instance.
    pub fn advertised_by(instance: &InstanceUrl, advertised: &str) -> Result<Self, DiscoveryError> {
        if advertised.is_empty() {
            return Err(DiscoveryError::NotAdvertised);
        }
        let mut url = Url::parse(advertised)
            .map_err(|_| DiscoveryError::Unsupported("is not an absolute URL"))?;
        let scheme = match url.scheme() {
            "wss" | "https" => "https",
            "ws" | "http" => "http",
            _ => {
                return Err(DiscoveryError::Unsupported(
                    "is not a ws, wss, http or https URL",
                ));
            }
        };
        // ws/wss and http/https are all special schemes: the switch cannot fail.
        if url.set_scheme(scheme).is_err() {
            return Err(DiscoveryError::Unsupported(
                "is not a ws, wss, http or https URL",
            ));
        }
        check_origin(&url).map_err(DiscoveryError::Unsupported)?;
        if instance.url().scheme() == "https" && url.scheme() == "http" {
            return Err(DiscoveryError::Insecure);
        }
        Ok(Self {
            instance: instance.clone(),
            base: url,
        })
    }

    /// The instance's own host, for an instance known to serve streaming there (one that does
    /// not advertise a streaming URL, say).
    pub fn instance_host(instance: &InstanceUrl) -> Self {
        Self {
            instance: instance.clone(),
            base: instance.url().clone(),
        }
    }

    /// The streaming origin, as the SDK connects to it (`https://streaming.example.social/`).
    pub fn url(&self) -> &str {
        self.base.as_str()
    }

    /// The instance that advertised it.
    pub fn instance(&self) -> &InstanceUrl {
        &self.instance
    }

    /// Whether streaming is served by the instance's own origin.
    pub fn is_instance_host(&self) -> bool {
        self.base.origin() == self.instance.url().origin()
    }
}

impl fmt::Debug for StreamingHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamingHost")
            .field("instance", &self.instance.as_str())
            .field("url", &self.base.as_str())
            .finish()
    }
}

/// Why no streaming host is available.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiscoveryError {
    /// The instance document has no (or an empty) `configuration.urls.streaming`.
    NotAdvertised,
    /// The advertised value is not a `ws`, `wss`, `http` or `https` origin: the rule it broke.
    Unsupported(&'static str),
    /// An `https` instance advertised a plaintext (`ws` or `http`) streaming host.
    Insecure,
    /// The host was advertised by another instance than the client's: nothing was sent.
    OtherInstance,
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAdvertised => f.write_str("the instance advertises no streaming URL"),
            Self::Unsupported(rule) => write!(f, "the advertised streaming URL {rule}"),
            Self::Insecure => {
                f.write_str("an https instance advertised a plaintext streaming URL (refused)")
            }
            Self::OtherInstance => {
                f.write_str("the streaming host belongs to another instance (nothing was sent)")
            }
        }
    }
}

impl std::error::Error for DiscoveryError {}

/// One event of a public or hashtag stream (see the [module](self) for the mapping).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum StreamEvent {
    /// A new status (`update`).
    Update(Box<Status>),
    /// A status was edited (`status.update`): its new version.
    StatusUpdate(Box<Status>),
    /// A status was deleted (`delete`): its id.
    Delete(StatusId),
    /// An event type this SDK does not decode, kept as sent.
    Other {
        /// The SSE event type.
        event: String,
        /// The SSE data, undecoded.
        data: String,
    },
}

/// The shared format of public and hashtag streams: Concord's `text/event-stream` head check
/// (exactly one `Content-Type`, charset absent or `utf-8`) before the body is read, then
/// [`PublicStream`] over Concord's SSE reader. It suggests `Accept: text/event-stream`.
pub struct PublicStreamFormat(());

impl Incremental<StreamEvent> for PublicStreamFormat {
    type Records = PublicStream;
    type Context = ();
    const ACCEPT: Option<&'static str> = <Sse as Incremental<concord::sse::Event>>::ACCEPT;

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Sse as Incremental<concord::sse::Event>>::check_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        <Sse as Incremental<concord::sse::Event>>::context(head)
    }

    fn open((): (), body: Incoming) -> PublicStream {
        PublicStream::new(body)
    }
}

/// A public or hashtag stream, read as the body arrives: what the generated `send()` of
/// [`PublicStreamRequest`](crate::PublicStreamRequest),
/// [`HashtagStreamRequest`](crate::HashtagStreamRequest) or
/// [`LocalHashtagStreamRequest`](crate::LocalHashtagStreamRequest) returns on success once the response head was
/// accepted (nothing of the body is read before the first [`next`](Self::next)).
///
/// - [`next`](Self::next) returns the events in order; heartbeats and comments are skipped. Every
///   end is an error (the stream has none of its own), then `None`.
/// - It is **cancel-safe**: dropping a pending `next()` (a `tokio::select!` branch, a timeout)
///   loses nothing; a partly received event stays buffered for the next call. Nothing runs ahead
///   of the caller and no task is spawned.
/// - [`position`](Self::position) advances with every byte read, heartbeats included: a caller's
///   idle timeout can tell "no events" (the position moved) from "nothing at all".
/// - Dropping it drops the response: the connection is closed.
///
/// `Debug` shows the position and the state, never event content.
pub struct PublicStream {
    events: Option<SseEvents<Body>>,
    operation: Option<&'static str>,
    /// Where reading stopped, once it did.
    stopped: Position,
}

impl PublicStream {
    fn new(body: Incoming) -> Self {
        let operation = body.operation().map(|op| op.name());
        let reader = SseReader::new(SseOptions::new().max_event_bytes(MAX_EVENT_BYTES));
        let body = Body {
            incoming: body,
            ended: false,
        };
        // `PublicStreamFormat::check_content_type` checked the head: `read_for` checks nothing.
        let events = match operation {
            Some(operation) => reader.read_for(body, operation),
            None => reader.read(body),
        };
        Self {
            events: Some(events),
            operation,
            stopped: Position::default(),
        }
    }

    /// The next event; an error when the stream stopped (once), then `None`. A
    /// [`Payload`](StreamErrorKind::Payload) error is not an end: the next call goes on.
    pub async fn next(&mut self) -> Option<Result<StreamEvent, StreamError>> {
        let events = self.events.as_mut()?;
        let event = match events.next().await {
            Some(Ok(event)) => event,
            Some(Err(error)) => return Some(Err(self.stop(error))),
            None => {
                // A body that ended at an event boundary: the server closed the stream.
                self.stopped = events.position();
                self.events = None;
                return Some(Err(StreamError::new(
                    StreamErrorKind::Closed,
                    self.operation,
                    self.stopped,
                    Detail::Closed,
                )));
            }
        };
        let position = events.position();
        Some(decode(event, position).map_err(|codec| {
            StreamError::new(
                StreamErrorKind::Payload,
                self.operation,
                position,
                Detail::Payload(codec),
            )
        }))
    }

    /// A terminal reader error: classify it, keep the position, drop the connection.
    fn stop(&mut self, error: ReadError) -> StreamError {
        let ended = self
            .events
            .take()
            .is_some_and(|events| events.into_source().ended);
        let kind = match error.kind() {
            ReadErrorKind::Transport => StreamErrorKind::Disconnected,
            ReadErrorKind::Limit => StreamErrorKind::TooLarge,
            // The reader reports a body that ended inside a line or an event as a format error;
            // it can only find that at the end of the body.
            ReadErrorKind::Format if ended => StreamErrorKind::Truncated,
            _ => StreamErrorKind::Malformed,
        };
        self.stopped = error.position();
        StreamError::new(
            kind,
            error.operation().or(self.operation),
            self.stopped,
            Detail::Read(error),
        )
    }

    /// SSE events framed and body bytes read so far (heartbeats and comments count as bytes, not
    /// events; an event whose payload did not decode counts as an event).
    pub fn position(&self) -> Position {
        self.events
            .as_ref()
            .map_or(self.stopped, SseEvents::position)
    }

    /// Whether the stream stopped (its error was returned): `next()` returns `None` from now on.
    pub fn is_finished(&self) -> bool {
        self.events.is_none()
    }
}

impl fmt::Debug for PublicStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PublicStream")
            .field("operation", &self.operation)
            .field("position", &self.position())
            .field("finished", &self.is_finished())
            .finish()
    }
}

/// One framed event's `data`, decoded by its type.
fn decode(event: concord::sse::Event, position: Position) -> Result<StreamEvent, CodecError> {
    let status = |data: &str| -> Result<Box<Status>, CodecError> {
        serde_json::from_str::<Status>(data)
            .map(Box::new)
            .map_err(|error| {
                let label = match error.classify() {
                    serde_json::error::Category::Data => "an event's data is not a Mastodon status",
                    _ => "an event's data is not one JSON value",
                };
                CodecError::new(label)
                    .at("event", position.records)
                    .at("column", error.column() as u64)
                    .with_source(error)
            })
    };
    match event.event() {
        "update" => Ok(StreamEvent::Update(status(event.data())?)),
        "status.update" => Ok(StreamEvent::StatusUpdate(status(event.data())?)),
        "delete" => StatusId::new(event.into_data())
            .map(StreamEvent::Delete)
            .map_err(|_| {
                CodecError::new("a delete event's data is not a status id")
                    .at("event", position.records)
            }),
        other => Ok(StreamEvent::Other {
            event: other.to_owned(),
            data: event.into_data(),
        }),
    }
}

/// The body as the reader's byte source: the `Incoming` (whose failure carries Concord's own
/// `Transport` error, naming the endpoint, URL removed), noting whether the body ended.
struct Body {
    incoming: Incoming,
    ended: bool,
}

impl ByteSource for Body {
    async fn next_chunk(&mut self) -> Result<Option<Bytes>, SourceError> {
        // Cancel-safe: nothing changes until `Incoming`'s returns, and it loses no byte.
        let next = self.incoming.next_chunk().await?;
        if next.is_none() {
            self.ended = true;
        }
        Ok(next)
    }
}

/// How a [`PublicStream`] stopped, or which event's payload did not decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StreamErrorKind {
    /// The connection broke mid-body (terminal).
    Disconnected,
    /// The server ended the body at an event boundary (terminal): a disconnect, not a completion.
    Closed,
    /// The body ended inside a line or an event (terminal): that event is lost.
    Truncated,
    /// The body is not a valid event stream: invalid UTF-8 (terminal).
    Malformed,
    /// An event larger than [`MAX_EVENT_BYTES`] (terminal; it is never truncated).
    TooLarge,
    /// One event's `data` does not decode as its type requires: that event is skipped and the
    /// stream goes on (not terminal).
    Payload,
}

/// A [`PublicStream`] stopped, or one event's payload did not decode
/// ([`is_terminal`](Self::is_terminal) says which).
///
/// `Display` and `Debug` show the endpoint, the kind, a static label and numeric positions —
/// never event content, a header value or the URL. The underlying error (a serde error quoting
/// the payload, the transport error) is reachable deliberately through
/// [`source_unredacted`](Self::source_unredacted).
pub struct StreamError {
    inner: Box<Inner>,
}

struct Inner {
    kind: StreamErrorKind,
    operation: Option<&'static str>,
    position: Position,
    detail: Detail,
}

enum Detail {
    /// Concord's reader's error.
    Read(ReadError),
    /// An event's `data` did not decode.
    Payload(CodecError),
    /// The body ended at an event boundary.
    Closed,
}

impl StreamError {
    fn new(
        kind: StreamErrorKind,
        operation: Option<&'static str>,
        position: Position,
        detail: Detail,
    ) -> Self {
        Self {
            inner: Box::new(Inner {
                kind,
                operation,
                position,
                detail,
            }),
        }
    }

    /// What happened.
    pub fn kind(&self) -> StreamErrorKind {
        self.inner.kind
    }

    /// Whether the stream stopped (every kind but [`Payload`](StreamErrorKind::Payload)).
    pub fn is_terminal(&self) -> bool {
        self.inner.kind != StreamErrorKind::Payload
    }

    /// The declared endpoint (`Api.public_stream`).
    pub fn operation(&self) -> Option<&'static str> {
        self.inner.operation
    }

    /// Events framed and bytes read when it happened.
    pub fn position(&self) -> Position {
        self.inner.position
    }

    /// The static label and numeric positions (`line`, `column`, `event`), if any.
    pub fn codec(&self) -> Option<&CodecError> {
        match &self.inner.detail {
            Detail::Read(error) => error.codec(),
            Detail::Payload(codec) => Some(codec),
            Detail::Closed => None,
        }
    }

    /// Concord's reader's own error, for a terminal error other than `Closed`.
    pub fn read_error(&self) -> Option<&ReadError> {
        match &self.inner.detail {
            Detail::Read(error) => Some(error),
            _ => None,
        }
    }

    /// Concord's `Transport` error, for [`Disconnected`](StreamErrorKind::Disconnected): it names
    /// the endpoint and has no URL.
    pub fn transport(&self) -> Option<&concord::Error> {
        self.read_error()
            .filter(|error| error.kind() == ReadErrorKind::Transport)?
            .source_unredacted()?
            .downcast_ref::<concord::Error>()
    }

    /// The underlying error, **unredacted**: a serde error quoting the payload, a transport
    /// error.
    pub fn source_unredacted(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        match &self.inner.detail {
            Detail::Read(error) => error.source_unredacted(),
            Detail::Payload(codec) => codec.source_unredacted(),
            Detail::Closed => None,
        }
    }
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(operation) = self.inner.operation {
            write!(f, "{operation}: ")?;
        }
        f.write_str(match self.inner.kind {
            StreamErrorKind::Disconnected => "the stream was disconnected",
            StreamErrorKind::Closed => "the server closed the stream",
            StreamErrorKind::Truncated => "the stream ended inside an event",
            StreamErrorKind::Malformed => "the stream is not a valid event stream",
            StreamErrorKind::TooLarge => "an event is too large",
            StreamErrorKind::Payload => "an event does not decode",
        })?;
        if let Some(codec) = self.codec() {
            write!(f, ": {codec}")?;
        }
        write!(
            f,
            " (after {} events, {} bytes)",
            self.inner.position.records, self.inner.position.bytes
        )
    }
}

impl fmt::Debug for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamError")
            .field("kind", &self.inner.kind)
            .field("operation", &self.inner.operation)
            .field("position", &self.inner.position)
            .field("codec", &self.codec())
            .finish()
    }
}

/// No `source()`: the underlying error may quote content.
impl std::error::Error for StreamError {}
