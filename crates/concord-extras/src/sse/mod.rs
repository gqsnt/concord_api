//! Server-sent events (`text/event-stream`) read as a response arrives (feature `sse`).
//!
//! | Use | API |
//! |---|---|
//! | Events of an endpoint declared `-> stream Event` | `response Sse;` ([`Sse`]): `send()` returns [`SseEvents`] (head checked, body unread), default options |
//! | Events with other limits, or of a `-> stream` body | [`SseReader::open`] → [`SseEvents`] |
//! | Events from any byte source (no status or head check) | [`SseReader::read`] |
//!
//! The reader frames events; it does not decode them. Each [`Event`] carries its type, its data
//! and the last event ID as text: an SDK parses `data` itself (JSON, say), so that a malformed
//! payload and a payload of the wrong shape stay its own distinction.
//!
//! # Interpretation
//!
//! As the WHATWG HTML standard, "Interpreting an event stream":
//!
//! - one leading UTF-8 byte order mark is skipped (also when split across chunks); a later one is
//!   content;
//! - lines end with CR LF, LF or CR; a CR at the end of one chunk and an LF at the start of the
//!   next are one line end;
//! - a line starting with `:` is a comment, ignored;
//! - `name: value` is a field; one space after the colon is removed; a line without a colon is a
//!   field with an empty value; unknown fields are ignored; field names are case-sensitive;
//! - `event` sets the event type; `data` appends its value and a line feed to the data buffer;
//!   `id` sets the last event ID buffer unless the value contains NUL; `retry` sets the
//!   reconnection time if the value is ASCII digits (at least one), otherwise it is ignored;
//! - a blank line dispatches: the last event ID string takes the buffer's value; if the data
//!   buffer is empty nothing is dispatched; otherwise its final line feed is removed and an event
//!   of type `message` (or the event type) is delivered; the data and event type buffers are reset
//!   either way (the last event ID is not).
//!
//! # Where Concord is stricter than the standard
//!
//! A browser repairs or discards silently; a reader of a provider's stream must not lose events
//! unnoticed. Every departure is a terminal [`ReadError`](crate::stream::ReadError) (the reader is
//! then fused):
//!
//! | Input | Standard | This reader |
//! |---|---|---|
//! | invalid UTF-8 (anywhere: data, comments, unknown fields) | replacement characters | [`Format`](crate::stream::ReadErrorKind::Format), with `line` and `column` |
//! | input ending inside a line | the line is discarded | `Format`: "the input ends inside a line" |
//! | input ending after fields without the blank line that dispatches them | the event is discarded | `Format`: "the input ends inside an event, ..." |
//! | an event larger than `max_event_bytes` (default 1 MiB) | no bound | [`Limit`](crate::stream::ReadErrorKind::Limit), before the buffer grows; never truncated |
//! | more events than `max_events` (if set) | no bound | `Limit` |
//! | a failing source (a connection lost mid-event) | reconnect | [`Transport`](crate::stream::ReadErrorKind::Transport), naming the operation, URL removed |
//! | a `retry` value past `u64` milliseconds | an integer | saturates at `u64::MAX` |
//!
//! Only a body that ends exactly at an event boundary — after a blank line, or after nothing but
//! comments and blank lines (an empty body, a lone byte order mark) — ends cleanly
//! ([`Completion::EndOfInput`](crate::stream::Completion::EndOfInput)). Any field line counts,
//! including `id`, `retry` and unknown fields: they belong to the event their blank line closes.
//!
//! The response head check (for [`Sse`] and [`SseReader::open`]) requires a 2xx status, exactly
//! one `Content-Type`, `text/event-stream` (case-insensitive), and a `charset`, if any, of `utf-8`
//! (or its subset `us-ascii`) declared once; a refusal keeps the response unread. There is no
//! media policy to relax it: read a mislabelled stream with [`SseReader::read`], which checks
//! neither status nor head.
//!
//! # Errors and diagnostics
//!
//! Failures carry static labels and numbers (`line`, `column`, `limit`), the operation name and
//! the position (events delivered, bytes consumed), never event content. A transport failure's
//! underlying error (its URL removed) is reachable only through
//! [`ReadError::source_unredacted`](crate::stream::ReadError::source_unredacted).
//!
//! # Resources
//!
//! A reader retains the current transport chunk (as delivered by the source; reqwest delivers
//! decompressed bytes, which is what the limits and positions count), the event being assembled
//! (at most `max_event_bytes`) and the last event ID (one line at most, outside that bound). It
//! never queues events: one `next()` produces at most one. A `next()` interprets at most 64 KiB of
//! input between two yields to the executor and yields after every 64 empty chunks from the
//! source. No task is spawned; dropping the reader drops the response. It does not reconnect.

mod parse;
mod reader;

use concord_core::{CodecError, Incoming, Incremental, ResponseHead};

pub use reader::{DEFAULT_MAX_EVENT_BYTES, Event, SseEvents, SseOptions, SseReader};

/// The `text/event-stream` format (feature `sse`): an endpoint declared `-> stream Event` with
/// `response Sse;` returns [`SseEvents`] from `send()`, after the [module](self) head check and
/// before any of the body is read, with the default [`SseOptions`] (1 MiB per event, no event
/// total). It suggests `Accept: text/event-stream`.
///
/// For other limits, declare `-> stream` and open the [`Incoming`] body with a configured
/// [`SseReader`]. It has no finite decoding: an event stream is not one document.
pub struct Sse(());

impl Incremental<Event> for Sse {
    type Records = SseEvents;
    type Context = ();
    const ACCEPT: Option<&'static str> = Some("text/event-stream");

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        reader::check_head(head)
    }

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn open((): (), body: Incoming) -> SseEvents {
        SseReader::new(SseOptions::new()).open_checked(body)
    }
}
