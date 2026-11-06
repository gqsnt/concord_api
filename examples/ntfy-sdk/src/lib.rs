//! A small [ntfy](https://docs.ntfy.sh) SDK slice declared with `concord::api!`: publish a message
//! to a topic, subscribe to the topic as Server-Sent Events or line-delimited JSON, and replay
//! the server's message cache with a `since` cursor.
//!
//! Checked against the ntfy documentation of release **v2.37.2** (see `PROVENANCE.md`). Not
//! covered: WebSockets, the raw stream, attachments, actions, scheduled delivery, filters,
//! multi-topic subscriptions, push providers, accounts. No delivery guarantee beyond the server's
//! own is implied: a replay is bounded by the server's cache and policy, and a message can be
//! delivered more than once.
//!
//! # Construction
//!
//! [`Ntfy::new`] takes your `reqwest::Client` (timeouts, proxies and TLS are configured there).
//! The base URL is `https://ntfy.sh`; [`Ntfy::with_base_url`] points the client at a self-hosted
//! server. Protected topics need an [`AccessToken`] ([`Ntfy::with_token`], sent as
//! `Authorization: Bearer …`, flagged sensitive and withheld from every `Debug`).
//!
//! ```
//! use concord::reqwest;
//! use concord_sdk_ntfy::{AccessToken, Ntfy, Since, Topic};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let ntfy = Ntfy::new(reqwest::Client::new())
//!     .with_base_url("https://ntfy.example.test")?
//!     .with_token(&AccessToken::new("synthetic-token-not-real")?);
//! let alerts = ntfy.topic(Topic::new("alerts")?);
//!
//! let replay = alerts.poll().set_since(Since::Latest).prepare()?;
//! assert_eq!(
//!     replay.request().url().as_str(),
//!     "https://ntfy.example.test/alerts/json?since=latest&poll=1"
//! );
//! assert!(replay.request().headers()["authorization"].is_sensitive());
//! assert!(!format!("{replay:?} {ntfy:?}").contains("synthetic-token"));
//! # Ok(()) }
//! ```
//!
//! # Operations
//!
//! On a topic handle ([`Ntfy::topic`]):
//!
//! | Operation | Request | `send()` returns |
//! |---|---|---|
//! | `publish(text)` + `set_title`, `set_priority`, `set_tags`, `set_cache` | `POST /<topic>`, the text as body | the published [`Event`] |
//! | `subscribe_sse()` + `set_since` | `GET /<topic>/sse` | open [`Events`] (SSE) |
//! | `subscribe_json()` + `set_since` | `GET /<topic>/json` | open [`Events`] (NDJSON) |
//! | `poll()` + `set_since` | `GET /<topic>/json?poll=1` | open [`Events`] that end with the cached messages |
//!
//! Every `send()` returns `Result<_, NtfyError>` ([`NtfyPolicy`]): a non-2xx answer is
//! [`NtfyError::Api`] with ntfy's error document; every other failure is Concord's error, kept
//! whole. `prepare()` builds the native request without sending it.
//!
//! # Example: subscribe until cancelled, then resume
//!
//! ```no_run
//! use concord_sdk_ntfy::{NtfyTopic, ReplayCursor};
//!
//! # async fn run(alerts: NtfyTopic, stop: tokio::sync::oneshot::Receiver<()>) -> Result<(), Box<dyn std::error::Error>> {
//! let mut cursor = ReplayCursor::new();
//! let mut events = alerts.subscribe_sse().send().await?;
//! tokio::pin!(stop);
//! loop {
//!     tokio::select! {
//!         _ = &mut stop => break, // the caller's cancellation: `next()` is cancel-safe
//!         event = events.next() => match event {
//!             Some(Ok(event)) => {
//!                 cursor.observe(&event);
//!                 if event.is_message() { /* deliver */ }
//!             }
//!             Some(Err(lost)) => { eprintln!("{lost}"); break } // e.g. the connection broke
//!             None => break, // the server ended the stream
//!         },
//!     }
//! }
//! drop(events); // closes the connection
//! // Later: resume from the last message (or from the first `open` time).
//! let resumed = alerts.subscribe_sse().maybe_since(cursor.since()).send().await?;
//! # let _ = resumed; Ok(()) }
//! ```

mod error;
pub mod events;
mod model;
mod sse;

use concord::Json;
use concord::reqwest::header::{AUTHORIZATION, HeaderValue};

pub use error::{ApiError, ERROR_BODY_LIMIT, NtfyError, NtfyPolicy};
pub use events::{
    Events, EventsError, MAX_EVENT_BYTES, Mode, NdjsonFormat, ReplayCursor, SseFormat,
};
pub use model::{
    AccessToken, Cache, ErrorDocument, Event, EventId, EventKind, InvalidEventId, InvalidTag,
    InvalidToken, InvalidTopic, MessageText, MessageTooLong, PlainText, Priority, Since, Tags,
    Topic,
};

/// The ntfy release whose documentation this slice was checked against.
pub const DOCUMENTED_VERSION: &str = "v2.37.2";

concord::api! {
    /// An ntfy server: `https://ntfy.sh` unless [`with_base_url`](Ntfy::with_base_url) points it
    /// at a self-hosted one. Every `send()` returns `Result<_, NtfyError>`.
    pub client Ntfy(
        /// `Authorization`: absent for open topics; [`Ntfy::with_token`] sets a bearer token.
        #[sensitive] header(AUTHORIZATION) authorization?: HeaderValue,
    ) {
        base "https://ntfy.sh";
        send NtfyPolicy;

        /// One topic (`/<topic>`).
        scope topic(topic: Topic) {
            path "{topic}";

            /// Publishes a message (`POST /<topic>`, the text as body). The answer is the message
            /// as stored, with its `id` and `time`.
            POST publish(
                body message: MessageText,
                header("x-title") title?: String,
                priority?: Priority,
                tags?: Tags,
                cache?: Cache,
            ) -> Event {
                headers {
                    "x-priority" = "{priority}";
                    "x-tags" = "{tags}";
                    "x-cache" = "{cache}";
                }
                request PlainText;
                response Json;
            }

            /// Subscribes as Server-Sent Events (`GET /<topic>/sse`): an `open` event, the cached
            /// messages `since` selects, live messages and `keepalive`s, until the caller drops
            /// the reader.
            GET subscribe_sse(query since?: Since) -> stream Event {
                path "sse";
                response SseFormat;
            }

            /// Subscribes as line-delimited JSON (`GET /<topic>/json`): the same events as
            /// `subscribe_sse`, one JSON message per line.
            GET subscribe_json(query since?: Since) -> stream Event {
                path "json";
                response NdjsonFormat;
            }

            /// Polls the cache (`GET /<topic>/json?poll=1`): the cached messages selected by
            /// `since` (all of them without it), then the end of the body. No `open`, no
            /// `keepalive`.
            GET poll(query since?: Since) -> stream Event {
                path "json";
                query { "poll" = "1"; }
                response NdjsonFormat;
            }
        }
    }
}

/// The README's code blocks, compiled as doctests so they cannot drift from the API.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

impl Ntfy {
    /// The same client sending `Authorization: Bearer <token>` (flagged sensitive) on every
    /// request. Handles created from the earlier client keep its credentials.
    #[must_use]
    pub fn with_token(self, token: &AccessToken) -> Self {
        self.set_authorization(token.header_value())
    }
}
