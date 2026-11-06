//! Message request and response values.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A text message from an explicit sender to one recipient.
///
/// No phone-number, sender authorization or provider-specific content validation is done.
/// Optional form values are omitted only for `None`; `Some("")` sends an empty value.
#[derive(Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateMessage {
    /// Recipient, normally an E.164 phone number for this SMS slice.
    pub to: String,
    /// Explicit sender, normally a Twilio number or permitted sender ID.
    pub from: String,
    /// Message text. Form encoding preserves UTF-8, spaces, `&` and `+`.
    pub body: String,
    /// Optional purpose (for example `notifications`), sent as `MessageIntent`.
    ///
    /// The provider determines which intent values it accepts. The fixtures also use a
    /// synthetic Unicode value solely to exercise form encoding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_intent: Option<String>,
    /// Optional URL for provider status notifications, sent as `StatusCallback`.
    ///
    /// Passed through without URL validation or normalization. Form encoding preserves
    /// Unicode and reserved characters; receiving or verifying callbacks is outside this SDK.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_callback: Option<String>,
}

impl CreateMessage {
    /// Creates the required `To`, `From` and `Body` fields, with no optional form values.
    pub fn new(to: impl Into<String>, from: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            from: from.into(),
            body: body.into(),
            message_intent: None,
            status_callback: None,
        }
    }

    /// Adds the optional `MessageIntent` form field to this message only.
    #[must_use]
    pub fn with_message_intent(mut self, intent: impl Into<String>) -> Self {
        self.message_intent = Some(intent.into());
        self
    }

    /// Adds the optional `StatusCallback` form field to this message only.
    ///
    /// Supply the original URL, without pre-encoding it as a form value. As with the other
    /// optional values, an empty string is sent explicitly and left for the provider to validate.
    #[must_use]
    pub fn with_status_callback(mut self, url: impl Into<String>) -> Self {
        self.status_callback = Some(url.into());
        self
    }
}

impl fmt::Debug for CreateMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateMessage").finish_non_exhaustive()
    }
}

/// The resource's current status, not a delivery guarantee.
///
/// Known values from the Message resource are represented even when their workflows are
/// outside this slice. Unknown future values retain their exact spelling.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
#[non_exhaustive]
pub enum MessageStatus {
    /// Accepted by a Messaging Service.
    Accepted,
    /// Scheduled by a Messaging Service.
    Scheduled,
    /// Queued for sending.
    Queued,
    /// Sending to the upstream carrier.
    Sending,
    /// Accepted by the upstream carrier; this does not mean delivered.
    Sent,
    /// Failed to send.
    Failed,
    /// Provider reports delivery confirmation.
    Delivered,
    /// Provider reports non-delivery.
    Undelivered,
    /// Receiving an inbound message.
    Receiving,
    /// Inbound processing completed.
    Received,
    /// Read receipt on a supporting channel (outside this slice).
    Read,
    /// Deprecated provider value.
    PartiallyDelivered,
    /// Scheduled message canceled.
    Canceled,
    /// A value unknown to this SDK; no terminal-state inference is made.
    Unknown(String),
}

impl From<String> for MessageStatus {
    fn from(value: String) -> Self {
        match value.as_str() {
            "accepted" => Self::Accepted,
            "scheduled" => Self::Scheduled,
            "queued" => Self::Queued,
            "sending" => Self::Sending,
            "sent" => Self::Sent,
            "failed" => Self::Failed,
            "delivered" => Self::Delivered,
            "undelivered" => Self::Undelivered,
            "receiving" => Self::Receiving,
            "received" => Self::Received,
            "read" => Self::Read,
            "partially_delivered" => Self::PartiallyDelivered,
            "canceled" => Self::Canceled,
            _ => Self::Unknown(value),
        }
    }
}

/// The relevant fields of a single Message resource. Additional JSON fields are ignored.
///
/// A successful fetch can return `Failed` or `Undelivered`: inspect `error_code` and
/// `error_message`. That is distinct from an HTTP error rejecting the fetch itself.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Message {
    /// Resource SID to pass to [`crate::Twilio::get_message`]. An identifier, shown by `Debug`.
    pub sid: String,
    /// Account owning the message.
    pub account_sid: String,
    /// Observed status at the time of this response.
    pub status: MessageStatus,
    /// Message text; missing or null is retained as `None`.
    pub body: Option<String>,
    /// Sender, when present.
    pub from: Option<String>,
    /// Recipient, when present.
    pub to: Option<String>,
    /// Provider's message processing error code, when present.
    pub error_code: Option<i64>,
    /// Unredacted provider explanation; may contain submitted data.
    pub error_message: Option<String>,
}

/// Shows `sid`, `status` and `error_code`: the message SID is a resource identifier (what a
/// workflow correlates on and what Twilio support asks for), not a credential. Withholds the
/// account SID (the Basic username in account-SID/auth-token authentication), `from`, `to`,
/// `body` and `error_message` (which can quote submitted data).
impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Message")
            .field("sid", &self.sid)
            .field("status", &self.status)
            .field("error_code", &self.error_code)
            .finish_non_exhaustive()
    }
}
