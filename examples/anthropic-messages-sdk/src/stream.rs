//! The streaming endpoint's response: typed events over Concord's server-sent events reader,
//! and the message they assemble.
//!
//! The documented flow (streaming page, "Event types"): one `message_start` (a `Message` with
//! empty `content`); content blocks, each `content_block_start`, `content_block_delta`s and
//! `content_block_stop`, whose `index` is the block's position in the final `content`; one or more
//! `message_delta`; a final `message_stop`. `ping` events may appear anywhere, an `error` event
//! may arrive after the 200 response, and unknown event types must be tolerated.
//!
//! [`MessageStream`] checks that flow while it assembles the message. Only `message_stop` makes
//! the message final: every other end — an `error` event, the end of the body, a broken
//! connection, a malformed or oversized event, an order the flow does not allow — is
//! [`Error::Incomplete`], with what arrived kept as a [`PartialMessage`]. Nothing is re-sent.

use std::fmt;

use concord::sse::{self, SseEvents, SseOptions, SseReader};
use concord::stream::{ReadError, ReadErrorKind};
use concord::{CodecError, Incoming, Incremental, ResponseHead};
use serde::Deserialize;
use serde_json::Value;

use crate::error::{Error, Incomplete, IncompleteCause, ProviderError, request_id};
use crate::model::{ContentBlock, Message, StopReason, Usage, text_of};

/// The largest single server-sent event this SDK assembles: Concord's default, 1 MiB (an event's
/// `data` lines, its type and the line being received). A larger event ends the stream as
/// [`IncompleteCause::EventTooLarge`]; it is never truncated.
pub const MAX_EVENT_BYTES: usize = sse::DEFAULT_MAX_EVENT_BYTES;

/// The response format of the streaming endpoint (`response MessageEvents;`): Concord's
/// `text/event-stream` head check before the body is read (exactly one `Content-Type`,
/// `text/event-stream`, charset absent or UTF-8; anything else is a `Metadata` error with the
/// response unread), then a [`MessageStream`] over the unread body. It suggests
/// `Accept: text/event-stream`.
pub struct MessageEvents(());

impl Incremental<StreamEvent> for MessageEvents {
    type Records = MessageStream;
    /// The `request-id` of the response.
    type Context = Option<String>;
    const ACCEPT: Option<&'static str> = Some("text/event-stream");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        reader().check_head(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<Option<String>, CodecError> {
        Ok(request_id(head.headers()).map(str::to_owned))
    }

    fn open(request_id: Option<String>, body: Incoming) -> MessageStream {
        let operation = body
            .operation()
            .map_or("Anthropic.messages.stream", |operation| operation.name());
        // The `Incoming` itself is the byte source: a broken body is Concord's own `Transport`
        // error (endpoint named, URL removed), handed back by value.
        let events = reader().read_for(body, operation);
        MessageStream::new(events, request_id)
    }
}

fn reader() -> SseReader {
    SseReader::new(SseOptions::new().max_event_bytes(MAX_EVENT_BYTES))
}

/// One event of a message stream, decoded from its `data`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StreamEvent {
    /// `message_start`: the message, with empty `content`.
    MessageStart(Message),
    /// `content_block_start`: block `index` begins with this value.
    ContentBlockStart {
        /// The block's position in the message's `content`.
        index: usize,
        /// The block as it starts (`{"type": "text", "text": ""}`).
        block: ContentBlock,
    },
    /// `content_block_delta`: a change to block `index`.
    ContentBlockDelta {
        /// The block's position.
        index: usize,
        /// The change.
        delta: Delta,
    },
    /// `content_block_stop`: block `index` is complete.
    ContentBlockStop {
        /// The block's position.
        index: usize,
    },
    /// `message_delta`: top-level changes (stop reason, cumulative usage).
    MessageDelta {
        /// The stop reason, as of this event.
        stop_reason: Option<StopReason>,
        /// The stop sequence, as of this event.
        stop_sequence: Option<String>,
        /// Cumulative token counts, if sent.
        usage: Option<Usage>,
    },
    /// `message_stop`: the message is complete.
    MessageStop,
    /// `ping`.
    Ping,
    /// An event type this SDK does not know, passed through (the documentation asks clients to
    /// tolerate new types). It does not change the assembled message.
    Unknown {
        /// The event type.
        event_type: String,
        /// The event's data.
        data: Value,
    },
}

impl StreamEvent {
    /// The event type (`message_start`, ..., or an unknown type).
    pub fn event_type(&self) -> &str {
        match self {
            Self::MessageStart(_) => "message_start",
            Self::ContentBlockStart { .. } => "content_block_start",
            Self::ContentBlockDelta { .. } => "content_block_delta",
            Self::ContentBlockStop { .. } => "content_block_stop",
            Self::MessageDelta { .. } => "message_delta",
            Self::MessageStop => "message_stop",
            Self::Ping => "ping",
            Self::Unknown { event_type, .. } => event_type,
        }
    }

    /// The text of a `text_delta`, for printing text as it arrives.
    pub fn text_delta(&self) -> Option<&str> {
        match self {
            Self::ContentBlockDelta {
                delta: Delta::Text(text),
                ..
            } => Some(text),
            _ => None,
        }
    }
}

/// The `delta` of a `content_block_delta`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Delta {
    /// `text_delta`: text appended to a `text` block.
    Text(String),
    /// Another delta type (`input_json_delta`, `thinking_delta`, `signature_delta`, ...), kept
    /// uninterpreted: it is recorded on the block it targets
    /// ([`OtherBlock::deltas`](crate::OtherBlock::deltas)).
    Other {
        /// The delta's `type`.
        delta_type: String,
        /// The delta object.
        raw: Value,
    },
}

/// The state of a [`MessageStream`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StreamState {
    /// Events may follow.
    Reading,
    /// `message_stop` arrived: the message is final.
    Complete,
    /// The stream ended before `message_stop`; the failure was returned by `next()`.
    Failed,
}

/// The events of one streaming response, read as they arrive, and the message they assemble.
///
/// `next()` returns `Ok(None)` **only** after `message_stop`; every other end is an
/// [`Error::Incomplete`], also on later calls (never `Ok(None)` after a failure). Nothing runs
/// ahead of the caller: one `next()` reads only what one event needs.
///
/// `next()` is cancel-safe: a pending call dropped by a timeout or a `select!` loses no byte (the
/// partly received event stays buffered), so a per-event deadline is
/// `tokio::time::timeout(d, stream.next())`. Dropping the stream closes the connection; whether
/// the server then stops generating is not stated on the cited pages.
///
/// `Debug` shows the state and counts, never content.
pub struct MessageStream {
    events: SseEvents<Incoming>,
    request_id: Option<String>,
    state: StreamState,
    partial: PartialMessage,
    received: u64,
}

impl MessageStream {
    fn new(events: SseEvents<Incoming>, request_id: Option<String>) -> Self {
        Self {
            events,
            request_id,
            state: StreamState::Reading,
            partial: PartialMessage::default(),
            received: 0,
        }
    }

    /// The next event; `Ok(None)` once `message_stop` was received.
    ///
    /// # Errors
    ///
    /// [`Error::Incomplete`] when the stream ends in any other way, with the cause and the
    /// partial message; on every later call, cause [`IncompleteCause::AlreadyFailed`].
    pub async fn next(&mut self) -> Result<Option<StreamEvent>, Error> {
        match self.state {
            StreamState::Complete => return Ok(None),
            StreamState::Failed => return Err(self.fail(IncompleteCause::AlreadyFailed)),
            StreamState::Reading => {}
        }
        let frame = match self.events.next().await {
            Some(Ok(frame)) => frame,
            Some(Err(error)) => return Err(self.fail(read_failure(error))),
            None => return Err(self.fail(IncompleteCause::EndOfStream)),
        };
        let event = match decode(&frame) {
            Ok(event) => event,
            Err(cause) => return Err(self.fail(cause)),
        };
        if let Err(rule) = self.partial.apply(&event) {
            return Err(self.fail(IncompleteCause::Protocol(rule)));
        }
        self.received += 1;
        if event == StreamEvent::MessageStop {
            self.state = StreamState::Complete;
        }
        Ok(Some(event))
    }

    /// Reads the remaining events and returns the final message.
    ///
    /// # Errors
    ///
    /// [`Error::Incomplete`], as [`next`](Self::next): a stream that ends before
    /// `message_stop` has no final message.
    pub async fn final_message(mut self) -> Result<Message, Error> {
        while self.next().await?.is_some() {}
        Ok(self
            .partial
            .message
            .take()
            .expect("message_stop is accepted only after message_start"))
    }

    /// Where the stream stands.
    pub fn state(&self) -> StreamState {
        self.state
    }

    /// What has been assembled so far (final once [`state`](Self::state) is `Complete`).
    pub fn partial(&self) -> &PartialMessage {
        &self.partial
    }

    /// Events received and accepted so far, `ping` and unknown types included.
    pub fn events_received(&self) -> u64 {
        self.received
    }

    /// The `request-id` of the response.
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    fn fail(&mut self, cause: IncompleteCause) -> Error {
        self.state = StreamState::Failed;
        Error::Incomplete(Box::new(Incomplete::new(
            cause,
            self.partial.clone(),
            self.received,
            self.request_id.clone(),
        )))
    }
}

impl fmt::Debug for MessageStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MessageStream")
            .field("state", &self.state)
            .field("events_received", &self.received)
            .field("request_id", &self.request_id)
            .field("reader", &self.events)
            .finish_non_exhaustive()
    }
}

/// What a stream assembled before it ended: the message of `message_start` with the content
/// received so far. It is not a [`Message`] on purpose: without `message_stop` it is not final.
///
/// `Debug` shows counts, never content.
#[derive(Clone, Default, PartialEq)]
pub struct PartialMessage {
    message: Option<Message>,
    open: Vec<bool>,
    message_deltas: u64,
}

impl PartialMessage {
    /// Whether `message_start` arrived.
    pub fn started(&self) -> bool {
        self.message.is_some()
    }

    /// The message id, once `message_start` arrived.
    pub fn id(&self) -> Option<&str> {
        self.message.as_ref().map(|message| message.id.as_str())
    }

    /// The model, once `message_start` arrived.
    pub fn model(&self) -> Option<&str> {
        self.message.as_ref().map(|message| message.model.as_str())
    }

    /// The content blocks received so far; a block whose `content_block_stop` has not arrived
    /// is unfinished ([`open_blocks`](Self::open_blocks)).
    pub fn content(&self) -> &[ContentBlock] {
        self.message
            .as_ref()
            .map_or(&[], |message| message.content.as_slice())
    }

    /// The text received so far, of every `text` block in order.
    pub fn text(&self) -> String {
        text_of(self.content())
    }

    /// The indexes of blocks that started but did not stop.
    pub fn open_blocks(&self) -> Vec<usize> {
        self.open
            .iter()
            .enumerate()
            .filter_map(|(index, open)| open.then_some(index))
            .collect()
    }

    /// The stop reason of the latest `message_delta`, if one arrived.
    pub fn stop_reason(&self) -> Option<&StopReason> {
        self.message.as_ref()?.stop_reason.as_ref()
    }

    /// The token counts as of the latest event that carried them.
    pub fn usage(&self) -> Option<&Usage> {
        self.message.as_ref()?.usage.as_ref()
    }

    /// Checks `event` against the documented flow and applies it.
    fn apply(&mut self, event: &StreamEvent) -> Result<(), &'static str> {
        let message = match (&mut self.message, event) {
            (_, StreamEvent::Ping | StreamEvent::Unknown { .. }) => return Ok(()),
            (None, StreamEvent::MessageStart(start)) => {
                if !start.content.is_empty() {
                    return Err("message_start carries content");
                }
                self.message = Some(start.clone());
                return Ok(());
            }
            (Some(_), StreamEvent::MessageStart(_)) => return Err("a second message_start"),
            (None, _) => return Err("an event before message_start"),
            (Some(message), _) => message,
        };
        match event {
            StreamEvent::ContentBlockStart { index, block } => {
                if *index != message.content.len() {
                    return Err("content_block_start out of order");
                }
                message.content.push(block.clone());
                self.open.push(true);
            }
            StreamEvent::ContentBlockDelta { index, delta } => {
                if !self.open.get(*index).copied().unwrap_or(false) {
                    return Err("content_block_delta for a block that is not open");
                }
                match (&mut message.content[*index], delta) {
                    (ContentBlock::Text { text }, Delta::Text(more)) => text.push_str(more),
                    (ContentBlock::Other(block), Delta::Other { raw, .. }) => {
                        block.push_delta(raw.clone());
                    }
                    (ContentBlock::Text { .. }, Delta::Other { .. }) => {
                        return Err("a delta other than text_delta for a text block");
                    }
                    (ContentBlock::Other(_), Delta::Text(_)) => {
                        return Err("text_delta for a block that is not text");
                    }
                }
            }
            StreamEvent::ContentBlockStop { index } => match self.open.get_mut(*index) {
                Some(open @ true) => *open = false,
                _ => return Err("content_block_stop for a block that is not open"),
            },
            StreamEvent::MessageDelta {
                stop_reason,
                stop_sequence,
                usage,
            } => {
                if self.open.contains(&true) {
                    return Err("message_delta while a content block is open");
                }
                message.stop_reason.clone_from(stop_reason);
                message.stop_sequence.clone_from(stop_sequence);
                if let Some(later) = usage {
                    message
                        .usage
                        .get_or_insert_with(Usage::default)
                        .update(later);
                }
                self.message_deltas += 1;
            }
            StreamEvent::MessageStop => {
                if self.open.contains(&true) {
                    return Err("message_stop while a content block is open");
                }
                if self.message_deltas == 0 {
                    return Err("message_stop before any message_delta");
                }
            }
            StreamEvent::MessageStart(_) | StreamEvent::Ping | StreamEvent::Unknown { .. } => {
                unreachable!("handled above")
            }
        }
        Ok(())
    }
}

impl fmt::Debug for PartialMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PartialMessage")
            .field("started", &self.started())
            .field("blocks", &self.content().len())
            .field("open_blocks", &self.open_blocks())
            .field("message_deltas", &self.message_deltas)
            .finish()
    }
}

/// A reader failure as an incomplete cause.
fn read_failure(error: ReadError) -> IncompleteCause {
    match error.kind() {
        ReadErrorKind::Limit => IncompleteCause::EventTooLarge(error),
        ReadErrorKind::Transport => {
            // Read through the `Incoming`, a broken body's source is Concord's own error.
            if error
                .source_unredacted()
                .is_some_and(|source| source.is::<concord::Error>())
            {
                let source = error
                    .into_source_unredacted()
                    .expect("checked above")
                    .downcast::<concord::Error>()
                    .expect("checked above");
                IncompleteCause::Transport(*source)
            } else {
                IncompleteCause::MalformedStream(error)
            }
        }
        _ => IncompleteCause::MalformedStream(error),
    }
}

/// Decodes one server-sent event into a [`StreamEvent`]; an `error` event is a cause.
fn decode(frame: &sse::Event) -> Result<StreamEvent, IncompleteCause> {
    let invalid = |source| IncompleteCause::InvalidEvent {
        event: frame.event().to_owned(),
        source,
    };
    let data: Value = serde_json::from_str(frame.data()).map_err(invalid)?;
    let Some(event_type) = data.get("type").and_then(Value::as_str) else {
        return Err(IncompleteCause::Protocol(
            "event data without a string type",
        ));
    };
    // "Each event uses an SSE event name ... and includes the matching event `type` in its data."
    if event_type != frame.event() {
        return Err(IncompleteCause::Protocol(
            "the SSE event name differs from the data's type",
        ));
    }
    let event = match frame.event() {
        "error" => {
            let wire = ErrorEvent::deserialize(data).map_err(invalid)?;
            return Err(IncompleteCause::ErrorEvent(wire.error));
        }
        "message_start" => {
            MessageStart::deserialize(data).map(|start| StreamEvent::MessageStart(start.message))
        }
        "content_block_start" => {
            BlockStart::deserialize(data).map(|start| StreamEvent::ContentBlockStart {
                index: start.index,
                block: start.content_block,
            })
        }
        "content_block_delta" => BlockDelta::deserialize(data).and_then(|wire| {
            Ok(StreamEvent::ContentBlockDelta {
                index: wire.index,
                delta: delta(wire.delta)?,
            })
        }),
        "content_block_stop" => BlockStop::deserialize(data)
            .map(|stop| StreamEvent::ContentBlockStop { index: stop.index }),
        "message_delta" => {
            MessageDeltaWire::deserialize(data).map(|wire| StreamEvent::MessageDelta {
                stop_reason: wire.delta.stop_reason,
                stop_sequence: wire.delta.stop_sequence,
                usage: wire.usage,
            })
        }
        "message_stop" => Ok(StreamEvent::MessageStop),
        "ping" => Ok(StreamEvent::Ping),
        other => Ok(StreamEvent::Unknown {
            event_type: other.to_owned(),
            data,
        }),
    };
    event.map_err(invalid)
}

/// A delta object: a `text_delta` is interpreted, every other type kept as received.
fn delta(raw: Value) -> Result<Delta, serde_json::Error> {
    use serde::de::Error as _;
    let Some(delta_type) = raw.get("type").and_then(Value::as_str) else {
        return Err(serde_json::Error::custom("a delta has no string `type`"));
    };
    if delta_type == "text_delta" {
        let text = raw
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| serde_json::Error::custom("a text_delta has no string `text`"))?;
        return Ok(Delta::Text(text.to_owned()));
    }
    Ok(Delta::Other {
        delta_type: delta_type.to_owned(),
        raw,
    })
}

#[derive(Deserialize)]
struct MessageStart {
    message: Message,
}

#[derive(Deserialize)]
struct BlockStart {
    index: usize,
    content_block: ContentBlock,
}

#[derive(Deserialize)]
struct BlockDelta {
    index: usize,
    delta: Value,
}

#[derive(Deserialize)]
struct BlockStop {
    index: usize,
}

#[derive(Deserialize)]
struct MessageDeltaWire {
    delta: MessageDeltaFields,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct MessageDeltaFields {
    #[serde(default)]
    stop_reason: Option<StopReason>,
    #[serde(default)]
    stop_sequence: Option<String>,
}

#[derive(Deserialize)]
struct ErrorEvent {
    error: ProviderError,
}
