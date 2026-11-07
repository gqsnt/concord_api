//! The response envelope both operations return.
//!
//! The documented schema marks no field of the 200 response as required, so every field is
//! optional or defaults to empty, and unknown fields are ignored. The event-specific object
//! (`data[].attributes.attributes`: monitor, `evt`, priority, status, ...) is outside this SDK
//! slice: it is kept as raw JSON for the caller to interpret.

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::query::Cursor;

/// One page of events: the 200 body of `GET /api/v2/events` and of
/// `POST /api/v2/events/search`.
#[derive(Clone, Debug, Default, Deserialize)]
#[non_exhaustive]
pub struct EventsPage {
    /// The events of this page, in the provider's order.
    #[serde(default)]
    pub data: Vec<Event>,
    /// Links; `next` is informational (see [`Links::next`]).
    #[serde(default)]
    pub links: Option<Links>,
    /// Request metadata: the next cursor, warnings, the request id.
    #[serde(default)]
    pub meta: Option<EventsMeta>,
}

impl EventsPage {
    /// The event of this page whose `id` is `id`, if any.
    pub fn event(&self, id: &str) -> Option<&Event> {
        self.data
            .iter()
            .find(|event| event.id.as_deref() == Some(id))
    }

    /// `meta.page.after`: the cursor of the following page, as sent. `None` when the provider
    /// sent none, which the documentation describes as "no next results".
    pub fn after(&self) -> Option<&str> {
        self.meta.as_ref()?.page.as_ref()?.after.as_deref()
    }

    /// The non-fatal warnings of this response. When present, "partial results might be
    /// returned" (documentation): a page with warnings may be incomplete.
    pub fn warnings(&self) -> &[Warning] {
        self.meta.as_ref().map_or(&[], |meta| &meta.warnings)
    }

    /// `after` as a [`Cursor`] for the next request, `None` when absent. An empty cursor is
    /// returned as is; the pagination adapter refuses it (it would restart the listing).
    pub fn next_cursor(&self) -> Option<Cursor> {
        self.after().map(Cursor::new)
    }
}

/// One event of a page.
#[derive(Clone, Debug, Default, Deserialize)]
#[non_exhaustive]
pub struct Event {
    /// The event's unique id.
    #[serde(default)]
    pub id: Option<String>,
    /// The resource type; the documented (only) value is `event`.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// The event's common attributes.
    #[serde(default)]
    pub attributes: Option<EventAttributes>,
}

impl Event {
    /// The event's message, if present.
    pub fn message(&self) -> Option<&str> {
        self.attributes.as_ref()?.message.as_deref()
    }

    /// The event's tags (`key:value` text as sent), empty when absent.
    pub fn tags(&self) -> &[String] {
        self.attributes.as_ref().map_or(&[], |a| &a.tags)
    }

    /// The event's timestamp as sent (RFC 3339 `date-time`, e.g. `2019-01-02T09:42:36.320Z`).
    pub fn timestamp(&self) -> Option<&str> {
        self.attributes.as_ref()?.timestamp.as_deref()
    }

    /// One field of the event-specific attributes (`title`, `status`, `evt`, `monitor`, ...),
    /// as raw JSON. Their schemas are outside this SDK.
    pub fn attribute(&self, name: &str) -> Option<&Value> {
        self.attributes.as_ref()?.attributes.as_ref()?.get(name)
    }
}

/// `data[].attributes`.
#[derive(Clone, Debug, Default, Deserialize)]
#[non_exhaustive]
pub struct EventAttributes {
    /// The event-specific attributes, kept as raw JSON (outside this SDK's slice).
    #[serde(default)]
    pub attributes: Option<Map<String, Value>>,
    /// The message of the event.
    #[serde(default)]
    pub message: Option<String>,
    /// The tags of the event.
    #[serde(default)]
    pub tags: Vec<String>,
    /// The timestamp of the event, as sent.
    #[serde(default)]
    pub timestamp: Option<String>,
}

/// `links`.
#[derive(Clone, Debug, Default, Deserialize)]
#[non_exhaustive]
pub struct Links {
    /// "Link for the next set of results" (a `GET /api/v2/events` URL). Kept for information:
    /// the SDK continues with `meta.page.after`, the documented cursor, and never requests
    /// this URL (the documented example points at `app.datadoghq.com`, not the API host).
    #[serde(default)]
    pub next: Option<String>,
}

/// `meta`.
#[derive(Clone, Debug, Default, Deserialize)]
#[non_exhaustive]
pub struct EventsMeta {
    /// Time elapsed in milliseconds.
    #[serde(default)]
    pub elapsed: Option<i64>,
    /// Pagination attributes.
    #[serde(default)]
    pub page: Option<PageMeta>,
    /// The identifier of the request.
    #[serde(default)]
    pub request_id: Option<String>,
    /// The request status (`done` in the documented example).
    #[serde(default)]
    pub status: Option<String>,
    /// Non-fatal warnings.
    #[serde(default)]
    pub warnings: Vec<Warning>,
}

/// `meta.page`.
#[derive(Clone, Debug, Default, Deserialize)]
#[non_exhaustive]
pub struct PageMeta {
    /// The cursor for the next results, if any.
    #[serde(default)]
    pub after: Option<String>,
}

/// One entry of `meta.warnings`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Warning {
    /// A unique code for this type of warning (`unknown_index`).
    #[serde(default)]
    pub code: Option<String>,
    /// A detailed explanation.
    #[serde(default)]
    pub detail: Option<String>,
    /// A short summary.
    #[serde(default)]
    pub title: Option<String>,
}
