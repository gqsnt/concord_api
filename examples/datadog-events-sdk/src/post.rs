//! The v1 posting payload and acceptance envelope; event schemas remain opaque.

use concord::{CodecError, Encode, Json, reqwest};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::InputError;

/// The seven alert types documented for `POST /api/v1/events`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    /// Error.
    Error,
    /// Warning.
    Warning,
    /// Informational.
    Info,
    /// Success.
    Success,
    /// User update.
    UserUpdate,
    /// Recommendation.
    Recommendation,
    /// Snapshot.
    Snapshot,
}

/// A validated v1 event to post. Private fields and no deserializer prevent bypassing the
/// documented limits: text at most 4,000 characters, aggregation key at most 100.
/// Characters are counted as Unicode scalar values, rather than UTF-8 bytes.
///
/// The reference specifies no title, source or tag limits for v1, and explicitly shows a
/// title longer than 100 characters. Source values are provider-defined strings.
/// Only the ticket's six fields are supported; dates and category-specific schemas are omitted.
#[derive(Clone, Debug, Serialize)]
pub struct NewEvent {
    title: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alert_type: Option<AlertType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    aggregation_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_type_name: Option<String>,
}

impl NewEvent {
    /// The required title and text, checked before a request can be built.
    ///
    /// # Errors
    /// [`InputError::EventTextTooLong`] if `text` exceeds 4,000 characters.
    pub fn new(title: impl Into<String>, text: impl Into<String>) -> Result<Self, InputError> {
        let text = text.into();
        if text.chars().count() > 4_000 {
            return Err(InputError::EventTextTooLong);
        }
        Ok(Self {
            title: title.into(),
            text,
            tags: None,
            alert_type: None,
            aggregation_key: None,
            source_type_name: None,
        })
    }

    /// Tags to apply to the event (replaces any earlier tags).
    pub fn tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags = Some(tags.into_iter().map(Into::into).collect());
        self
    }

    /// The event's alert type.
    pub fn alert_type(mut self, alert_type: AlertType) -> Self {
        self.alert_type = Some(alert_type);
        self
    }

    /// Groups events using the same key in the Event Stream. This is not an idempotency key:
    /// it does not make retrying an uncertain post safe.
    ///
    /// # Errors
    /// [`InputError::AggregationKeyTooLong`] if `key` exceeds 100 characters.
    pub fn aggregation_key(mut self, key: impl Into<String>) -> Result<Self, InputError> {
        let key = key.into();
        if key.chars().count() > 100 {
            return Err(InputError::AggregationKeyTooLong);
        }
        self.aggregation_key = Some(key);
        Ok(self)
    }

    /// The source, sent as `source_type_name` (for example, `jenkins` or `git`).
    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.source_type_name = Some(source.into());
        self
    }
}

/// The provider's HTTP 202 acceptance. This says nothing about search visibility or indexing.
/// No follow-up read is issued. The optional event object stays raw JSON: the v1 posting ID
/// is not promised to be a v2 search ID, and event schemas are outside this SDK's slice.
#[derive(Clone, Debug, Deserialize)]
#[non_exhaustive]
pub struct EventAcceptance {
    /// The provider's status string, if returned; not a search-visibility guarantee.
    pub status: Option<String>,
    /// The provider's event object, including `id_str` when returned. Preserve string IDs to
    /// avoid loss of precision; the documentation's example uses placeholder numeric fields.
    pub event: Option<Map<String, Value>>,
}

/// JSON encoded in a non-replayable reqwest body. Even a caller-supplied retry policy cannot
/// clone this body to send the post again. This also applies to prepared requests.
pub(crate) struct PostJson;

impl Encode<NewEvent> for PostJson {
    const CONTENT_TYPE: Option<&'static str> = <Json as Encode<NewEvent>>::CONTENT_TYPE;

    fn encode(value: NewEvent) -> Result<reqwest::Body, CodecError> {
        <Json as Encode<NewEvent>>::encode(value).map(reqwest::Body::wrap)
    }
}
