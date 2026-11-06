//! Domain values: topics, the access token, publish inputs, the `since` cursor and the JSON
//! message every stream mode carries.

use std::fmt;

use concord::reqwest::header::HeaderValue;
use concord::{CodecError, Encode, reqwest};
use serde::Deserialize;

/// A topic name: 1 to 64 characters of `A–Z a–z 0–9 - _` (the server's topic pattern
/// `^[-_A-Za-z0-9]{1,64}$`).
///
/// The name is one path segment (`/<topic>`, `/<topic>/sse`, `/<topic>/json`); a name with `/`,
/// `,` (a multi-topic subscription, out of scope) or anything outside the pattern is refused here,
/// before any request exists.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Topic(String);

impl Topic {
    /// The longest topic name.
    pub const MAX_LEN: usize = 64;

    /// A checked topic name.
    ///
    /// # Errors
    ///
    /// [`InvalidTopic`] when the name is empty, longer than [`Topic::MAX_LEN`] or contains a
    /// character outside `A–Z a–z 0–9 - _`.
    pub fn new(name: &str) -> Result<Self, InvalidTopic> {
        let valid = !name.is_empty()
            && name.len() <= Self::MAX_LEN
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if valid {
            Ok(Self(name.to_owned()))
        } else {
            Err(InvalidTopic)
        }
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Topic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Topic({:?})", self.0)
    }
}

/// A refused topic name (the name itself is not repeated).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTopic;

impl fmt::Display for InvalidTopic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a topic name is 1 to 64 characters of A-Z, a-z, 0-9, '-' and '_'")
    }
}

impl std::error::Error for InvalidTopic {}

/// An ntfy access token, sent as `Authorization: Bearer <token>` (ntfy's "access tokens").
///
/// It has no `Display`: the only way out is [`AccessToken::header_value`], a header value flagged
/// sensitive. `Debug` prints `AccessToken(<redacted>)`. The token is checked to be a non-empty run
/// of visible ASCII; its `tk_` prefix and length are the server's business, not checked here.
#[derive(Clone, PartialEq, Eq)]
pub struct AccessToken(String);

impl AccessToken {
    /// A checked token.
    ///
    /// # Errors
    ///
    /// [`InvalidToken`] when the token is empty or contains a space, a control character or a
    /// non-ASCII character.
    pub fn new(token: &str) -> Result<Self, InvalidToken> {
        if !token.is_empty() && token.bytes().all(|b| b.is_ascii_graphic()) {
            Ok(Self(token.to_owned()))
        } else {
            Err(InvalidToken)
        }
    }

    /// `Bearer <token>`, flagged sensitive (reqwest and Concord withhold it from `Debug`).
    pub fn header_value(&self) -> HeaderValue {
        let mut value = HeaderValue::try_from(format!("Bearer {}", self.0))
            .expect("visible ASCII is a valid header value");
        value.set_sensitive(true);
        value
    }
}

impl fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccessToken(<redacted>)")
    }
}

/// A refused access token (the value is not repeated).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidToken;

impl fmt::Display for InvalidToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an access token is a non-empty run of visible ASCII characters")
    }
}

impl std::error::Error for InvalidToken {}

/// The body of a published message: UTF-8 text of at most [`MessageText::MAX_BYTES`] bytes.
///
/// ntfy turns a longer body (or one that is not UTF-8) into an **attachment**; attachments are out
/// of this SDK's scope, so a longer text is refused here instead of silently changing meaning. The
/// limit is the documented default (`publish/` "Limitations"); a self-hosted server may be
/// configured otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageText(String);

impl MessageText {
    /// The documented message size limit, in bytes.
    pub const MAX_BYTES: usize = 4096;

    /// A checked message body.
    ///
    /// # Errors
    ///
    /// [`MessageTooLong`] when `text` is longer than [`MessageText::MAX_BYTES`] bytes.
    pub fn new(text: impl Into<String>) -> Result<Self, MessageTooLong> {
        let text = text.into();
        if text.len() <= Self::MAX_BYTES {
            Ok(Self(text))
        } else {
            Err(MessageTooLong { bytes: text.len() })
        }
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A message body over the documented size limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MessageTooLong {
    /// The refused body's length in bytes.
    pub bytes: usize,
}

impl fmt::Display for MessageTooLong {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a message is at most {} bytes (this one has {}); longer bodies become attachments, which this SDK does not send",
            MessageText::MAX_BYTES,
            self.bytes
        )
    }
}

impl std::error::Error for MessageTooLong {}

/// The request format of a published message: the [`MessageText`] bytes as
/// `text/plain; charset=utf-8` (Concord's `Text` encodes only `String`).
pub struct PlainText(());

impl Encode<MessageText> for PlainText {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");

    fn encode(value: MessageText) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value.0))
    }
}

/// Message priority (`X-Priority` when publishing, `priority` in a message). The wire spelling is
/// the number `1`–`5`; ntfy also accepts the names, and `urgent` for `max`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "u8")]
pub enum Priority {
    /// `1` (`min`).
    Min,
    /// `2` (`low`).
    Low,
    /// `3` (`default`): what a message without `priority` means.
    Default,
    /// `4` (`high`).
    High,
    /// `5` (`max`, `urgent`).
    Max,
}

impl Priority {
    /// The wire number, `1`–`5`.
    pub fn number(self) -> u8 {
        match self {
            Self::Min => 1,
            Self::Low => 2,
            Self::Default => 3,
            Self::High => 4,
            Self::Max => 5,
        }
    }
}

impl TryFrom<u8> for Priority {
    type Error = &'static str;

    fn try_from(number: u8) -> Result<Self, Self::Error> {
        Ok(match number {
            1 => Self::Min,
            2 => Self::Low,
            3 => Self::Default,
            4 => Self::High,
            5 => Self::Max,
            _ => return Err("a priority is a number from 1 to 5"),
        })
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.number())
    }
}

/// Tags of a published message (`X-Tags`), sent comma-separated in the given order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tags(Vec<String>);

impl Tags {
    /// Checked tags: each non-empty, without a comma (the separator) and without a control
    /// character. The 512-byte total is the server's limit (answered `400`), not checked here.
    ///
    /// # Errors
    ///
    /// [`InvalidTag`] naming the position of the first refused tag; also for an empty list.
    pub fn new<I, S>(tags: I) -> Result<Self, InvalidTag>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let tags: Vec<String> = tags.into_iter().map(Into::into).collect();
        if tags.is_empty() {
            return Err(InvalidTag { index: 0 });
        }
        for (index, tag) in tags.iter().enumerate() {
            if tag.is_empty() || tag.contains(',') || tag.chars().any(char::is_control) {
                return Err(InvalidTag { index });
            }
        }
        Ok(Self(tags))
    }

    /// The tags, in order.
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

impl fmt::Display for Tags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join(","))
    }
}

/// A refused tag list: the 0-based position of the first bad tag (the tag is not repeated).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTag {
    /// The position of the first refused tag (0 for an empty list).
    pub index: usize,
}

impl fmt::Display for InvalidTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tag {} is empty, contains a comma or a control character (or the list is empty)",
            self.index
        )
    }
}

impl std::error::Error for InvalidTag {}

/// `X-Cache` when publishing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cache {
    /// `no`: delivered to subscribers connected now, never stored, so never returned by a later
    /// `since=` subscription or poll. The published message then has no `expires`.
    No,
}

impl fmt::Display for Cache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::No => "no",
        })
    }
}

/// A message id as the server assigned it (`id`). Ids of `open` and `keepalive` events are made
/// per connection and are not cached messages: only a `message` id is a replay cursor
/// ([`ReplayCursor`](crate::ReplayCursor) applies that rule).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(transparent)]
pub struct EventId(String);

impl EventId {
    /// An id the caller kept (a persisted cursor): 1 to 64 ASCII letters and digits, and not one
    /// of the words `since=` gives another meaning (`all`, `latest`, `none`).
    ///
    /// # Errors
    ///
    /// [`InvalidEventId`] otherwise.
    pub fn new(id: &str) -> Result<Self, InvalidEventId> {
        let valid = !id.is_empty()
            && id.len() <= 64
            && id.bytes().all(|b| b.is_ascii_alphanumeric())
            && !matches!(id, "all" | "latest" | "none");
        if valid {
            Ok(Self(id.to_owned()))
        } else {
            Err(InvalidEventId)
        }
    }

    /// The id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EventId({:?})", self.0)
    }
}

/// A refused message id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidEventId;

impl fmt::Display for InvalidEventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a message id is 1 to 64 ASCII letters and digits (not all, latest or none)")
    }
}

impl std::error::Error for InvalidEventId {}

/// Which cached messages a subscription or poll starts with (`since=`). The server answers from
/// its **cache**, which is bounded by its cache duration (12 hours by default), by `Cache: no` at
/// publish time and by a per-topic replay size cap (the response then carries
/// `X-Messages-Truncated: 1`, see [`Events::truncated`](crate::Events::truncated)): no value
/// promises complete history, and none promises that a message is not delivered again.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Since {
    /// `all`: every cached message (what a poll without `since` means).
    All,
    /// `latest`: the most recent cached message only.
    Latest,
    /// A message id used by the server to select cached messages; replay may include duplicates.
    Id(EventId),
    /// A Unix timestamp in seconds: the cached messages from that time. Messages published within
    /// the same second as an already received one can come again: deduplicate by
    /// [`Event::id`] if that matters.
    Time(u64),
    /// Whole seconds before now, sent as `<n>s` (the server's duration syntax).
    Ago(u64),
}

impl fmt::Display for Since {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::All => f.write_str("all"),
            Self::Latest => f.write_str("latest"),
            Self::Id(id) => f.write_str(id.as_str()),
            Self::Time(seconds) => write!(f, "{seconds}"),
            Self::Ago(seconds) => write!(f, "{seconds}s"),
        }
    }
}

/// The `event` of a message: what the line or SSE event is.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(from = "String")]
pub enum EventKind {
    /// `open`: the subscription is established (not a cached message).
    Open,
    /// `keepalive`: the connection is alive (not a cached message).
    Keepalive,
    /// `message`: a published message.
    Message,
    /// `message_delete`: a published notification is to be deleted (see `sequence_id`).
    MessageDelete,
    /// `message_clear`: a published notification is to be cleared (see `sequence_id`).
    MessageClear,
    /// `poll_request`: a request to poll the topic (sent to push subscribers).
    PollRequest,
    /// Any other value, kept verbatim: a newer server may add event types.
    Other(String),
}

impl From<String> for EventKind {
    fn from(kind: String) -> Self {
        match kind.as_str() {
            "open" => Self::Open,
            "keepalive" => Self::Keepalive,
            "message" => Self::Message,
            "message_delete" => Self::MessageDelete,
            "message_clear" => Self::MessageClear,
            "poll_request" => Self::PollRequest,
            _ => Self::Other(kind),
        }
    }
}

/// One event of a JSON stream line, an SSE `data` payload or a publish response: ntfy's JSON
/// message format. Members the SDK does not model (`actions`, `attachment`, `icon`, any newer
/// member) are ignored, not refused.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Event {
    /// `id`.
    pub id: EventId,
    /// `time`: Unix seconds.
    pub time: u64,
    /// `event`.
    #[serde(rename = "event")]
    pub kind: EventKind,
    /// `topic` (a comma-separated list for an `open`/`keepalive` of a multi-topic subscription).
    pub topic: String,
    /// `expires`: Unix seconds when the server drops the message from its cache; absent when it
    /// was published with `Cache: no`.
    #[serde(default)]
    pub expires: Option<u64>,
    /// `sequence_id`: the notification this message updates, deletes or clears.
    #[serde(default)]
    pub sequence_id: Option<String>,
    /// `message`: the body (always present on `message` events).
    #[serde(default)]
    pub message: Option<String>,
    /// `title`.
    #[serde(default)]
    pub title: Option<String>,
    /// `tags`.
    #[serde(default)]
    pub tags: Vec<String>,
    /// `priority` (absent: [`Priority::Default`]).
    #[serde(default)]
    pub priority: Option<Priority>,
    /// `click`: a URL to open when the notification is tapped.
    #[serde(default)]
    pub click: Option<String>,
}

impl Event {
    /// Whether this is a published `message` (not `open`, `keepalive` or another kind).
    pub fn is_message(&self) -> bool {
        self.kind == EventKind::Message
    }
}

/// ntfy's error document (`{"code":40301,"http":403,"error":"forbidden","link":"…"}`), sent with
/// a non-2xx status.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ErrorDocument {
    /// `code`: ntfy's own error code (`40301`, `42901`, …).
    #[serde(default)]
    pub code: Option<u32>,
    /// `http`: the HTTP status the server meant.
    pub http: u16,
    /// `error`: the server's message (not shown by `Display` of the SDK's errors).
    pub error: String,
    /// `link`: a documentation page about the error.
    #[serde(default)]
    pub link: Option<String>,
}
