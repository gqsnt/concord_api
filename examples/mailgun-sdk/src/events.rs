use crate::{Error, InputError, LinkError, MailgunDomainEvents, MessageId, Metadata, SendPolicy};
use concord::{
    QueryPairs, QueryPairsWriter,
    reqwest::{Url, header::HeaderValue},
};
use concord_pagination::{Page, Pages, Resume};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    num::NonZeroUsize,
};

/// Validated ASCII DNS sending domain, represented as one path segment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Domain(String);
impl Domain {
    /// Accept ordinary DNS labels; IDNs must be supplied in ASCII form.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if value.len() > 253
            || value.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err(Error::Input(InputError::Domain));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }
}
impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Finite ascending search interval in Unix seconds. No implicit "now".
#[derive(Clone, Copy, Debug)]
pub struct EventRange {
    begin: u64,
    end: u64,
}
impl EventRange {
    /// Refuse a reversed interval.
    pub fn new(begin: u64, end: u64) -> Result<Self, Error> {
        if begin > end {
            return Err(Error::Input(InputError::Search));
        }
        Ok(Self { begin, end })
    }
}
impl QueryPairs for EventRange {
    fn query_pairs(&self, out: &mut QueryPairsWriter<'_>) {
        out.pair("begin", self.begin);
        out.pair("end", self.end);
        out.pair("ascending", "yes");
    }
}

/// Mailgun event page size, 1–300. The default is 100.
#[derive(Clone, Copy, Debug)]
pub struct PageSize(u16);
impl PageSize {
    /// Validate the provider's upper bound and the SDK's nonzero lower bound.
    pub fn new(size: u16) -> Result<Self, Error> {
        if !(1..=300).contains(&size) {
            return Err(Error::Input(InputError::Search));
        }
        Ok(Self(size))
    }
}
impl Default for PageSize {
    fn default() -> Self {
        Self(100)
    }
}
impl fmt::Display for PageSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// An events response; unknown additive fields are tolerated.
#[derive(Deserialize)]
pub struct EventPage {
    /// Records returned on this page.
    pub items: Vec<Event>,
    /// Provider continuation URLs. Empty items terminate even when these links are present.
    pub paging: Paging,
}
/// Provider page URLs; Debug never renders them.
#[derive(Deserialize)]
pub struct Paging {
    /// Next page URL, opaque to the caller.
    pub next: String,
    /// Previous page URL. Retained, but not used by the forward traversal.
    pub previous: String,
}
/// Common Mailgun event fields. Event kinds and severities remain forward compatible strings.
#[derive(Deserialize)]
pub struct Event {
    /// Event ID (unique within a day, not globally).
    pub id: String,
    /// Event kind, e.g. accepted, delivered or failed.
    pub event: String,
    /// Provider timestamp without unit conversion; the events manual describes Unix seconds.
    pub timestamp: f64,
    /// Recipient, when present for this event type.
    pub recipient: Option<String>,
    /// Message headers, when present.
    pub message: Option<EventMessage>,
    /// Failure severity (temporary or permanent); absent/unknown stays indeterminate.
    pub severity: Option<String>,
    /// Includes the provider's test-mode flag, which prevents a real-delivery conclusion.
    pub flags: Option<EventFlags>,
    /// User-defined tags; empty when absent. The provider compares tags case insensitively.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Custom JSON data sent with `v:<name>`; empty when absent.
    #[serde(default, rename = "user-variables")]
    pub user_variables: BTreeMap<String, serde_json::Value>,
}
/// Flags relevant to interpreting delivery evidence.
#[derive(Deserialize)]
pub struct EventFlags {
    /// A simulated delivery that stopped before sending to the recipient's provider.
    #[serde(rename = "is-test-mode")]
    pub is_test_mode: Option<bool>,
}
/// Message fields needed to correlate an event.
#[derive(Deserialize)]
pub struct EventMessage {
    /// Headers may be absent for event types unrelated to a sent message.
    pub headers: Option<MessageHeaders>,
}
/// Message ID from the event's message headers.
#[derive(Deserialize)]
pub struct MessageHeaders {
    /// Provider message identifier, often without surrounding angle brackets.
    #[serde(rename = "message-id")]
    pub message_id: Option<String>,
}

macro_rules! redacted_debug {
    ($($name:ident),+ $(,)?) => { $(
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
    )+ };
}
redacted_debug!(
    EventPage,
    Paging,
    Event,
    EventFlags,
    EventMessage,
    MessageHeaders,
    EventCursor
);

/// Latest delivery-relevant observation for one message and one recipient in the searched window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryStatus {
    /// No matching delivery-relevant event; not evidence of non-delivery.
    Unknown,
    /// Queued by Mailgun; not delivered.
    Accepted,
    /// Accepted by the recipient's mail server; not proof of inbox placement or reading.
    Delivered,
    /// A delivered event explicitly marked test mode; no actual delivery is established.
    TestMode,
    /// Provider will retry delivery; the SDK does not resubmit the email.
    TemporaryFailure,
    /// Provider has stopped retrying delivery.
    PermanentFailure,
    /// Failed event without a recognized severity.
    FailedUnknownSeverity,
    /// Provider rejected the message.
    Rejected,
    /// Conflicting observations at the same latest timestamp; no order is invented.
    Conflicting,
}

/// Inspect records already retrieved, independently checking message ID and recipient.
/// Pass a completed search for a conclusion over its whole window; a partial prefix is partial evidence.
pub fn delivery_status(id: &MessageId, recipient: &str, events: &[Event]) -> DeliveryStatus {
    let mut latest = None;
    let mut status = DeliveryStatus::Unknown;
    for event in events {
        let event_id = event
            .message
            .as_ref()
            .and_then(|m| m.headers.as_ref())
            .and_then(|h| h.message_id.as_deref());
        if !event_id.is_some_and(|value| id.matches(value))
            || event.recipient.as_deref() != Some(recipient)
            || !event.timestamp.is_finite()
        {
            continue;
        }
        let found = match event.event.as_str() {
            "accepted" => DeliveryStatus::Accepted,
            "delivered" if event.flags.as_ref().and_then(|f| f.is_test_mode) == Some(true) => {
                DeliveryStatus::TestMode
            }
            "delivered" => DeliveryStatus::Delivered,
            "rejected" => DeliveryStatus::Rejected,
            "failed" => match event.severity.as_deref() {
                Some("temporary") => DeliveryStatus::TemporaryFailure,
                Some("permanent") => DeliveryStatus::PermanentFailure,
                _ => DeliveryStatus::FailedUnknownSeverity,
            },
            _ => continue,
        };
        match latest {
            Some(time) if event.timestamp < time => {}
            Some(time) if event.timestamp == time => {
                if found != status {
                    status = DeliveryStatus::Conflicting;
                }
            }
            _ => {
                latest = Some(event.timestamp);
                status = found;
            }
        }
    }
    status
}

/// Opaque, redacted continuation bound to the original request and credential.
#[derive(Clone)]
pub struct EventCursor {
    url: String,
    original: Url,
    credential: Option<HeaderValue>,
}

/// Bounded event chain using the shared pagination engine; failed/bounded reads keep their prefix.
pub type EventPages = Pages<Event, EventCursor, Metadata, Error>;

impl MailgunDomainEvents {
    /// Start a forward traversal with a mandatory page bound, counting the empty terminal page.
    pub fn pages(self, max_pages: NonZeroUsize) -> Result<EventPages, Error> {
        self.pages_from(Resume::Start, max_pages)
    }

    /// Resume only the exact original search (origin, domain, query and credential).
    /// The provider dataset may change between requests; no snapshot or deduplication is claimed.
    pub fn pages_from(
        self,
        from: Resume<EventCursor>,
        max_pages: NonZeroUsize,
    ) -> Result<EventPages, Error> {
        let prepared = self.clone().prepare()?;
        let original = prepared.request().url().clone();
        let credential = prepared.request().headers().get("authorization").cloned();
        if from
            .key()
            .is_some_and(|key| key.original != original || key.credential != credential)
        {
            return Err(Error::Link(LinkError::Context));
        }
        let mut seen = HashSet::new();
        Ok(Pages::new(from, move |cursor: Option<EventCursor>| {
            let op = self.clone();
            let original = original.clone();
            let credential = credential.clone();
            let target = cursor
                .as_ref()
                .map_or_else(|| original.to_string(), |c| c.url.clone());
            let fresh = seen.insert(target.clone());
            async move {
                if !fresh {
                    return Err(Error::Link(LinkError::Cycle));
                }
                let response = match cursor {
                    None => op.send().await?,
                    Some(_) => {
                        trusted_link(&original, &target)?;
                        SendPolicy::execute(op.prepare()?.follow(&target)?).await?
                    }
                };
                let EventPage { items, paging } = response.value;
                let next = if items.is_empty() {
                    None
                } else {
                    let url = trusted_link(&original, &paging.next)?;
                    Some(EventCursor {
                        url: url.to_string(),
                        original,
                        credential,
                    })
                };
                Ok(Page {
                    items,
                    next,
                    meta: response.meta,
                })
            }
        })
        .max_pages(max_pages))
    }
}

fn trusted_link(original: &Url, text: &str) -> Result<Url, Error> {
    let refuse = || Error::Link(LinkError::Untrusted);
    if text.len() > 16 * 1024 || text.chars().any(char::is_control) || text.contains('\\') {
        return Err(refuse());
    }
    let url = Url::parse(text).map_err(|_| refuse())?;
    let collection = original.path();
    let path_ok = url.path() == collection
        || url
            .path()
            .strip_prefix(collection)
            .and_then(|suffix| suffix.strip_prefix('/'))
            .is_some_and(|token| {
                !token.is_empty() && !token.contains(['/', '%']) && token != "." && token != ".."
            });
    if url.origin() != original.origin()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || !path_ok
    {
        return Err(refuse());
    }
    Ok(url)
}
