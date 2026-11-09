//! A bounded Postmark transactional-email and outbound-message SDK.
//!
//! The generated `send()` performs one Concord execution. Configure the supplied HTTP client
//! with a timeout and without application-level retries. Acceptance is not delivery; a send
//! whose response cannot be read has an unknown outcome and must not be blindly repeated.
//! A batch send returns one [`BatchResult`] per submitted message, in submission order; a batch
//! whose results cannot be read or attributed has an unknown outcome as a whole.

use concord::{Endpoint, Json, SendWith};
use serde::{Deserialize, Serialize};

/// HTTP client builder with redirects and transport retries disabled. Set a timeout before building.
/// Re-enabling either can cause an email to be submitted more than once.
pub fn http_client_builder() -> concord::reqwest::ClientBuilder {
    concord::reqwest::Client::builder()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
}

/// A single email. Postmark validates addresses, sender signature, size, and body requirements.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Email {
    /// Registered sender address.
    pub from: String,
    /// Comma-separated recipients.
    pub to: String,
    /// Email subject.
    pub subject: String,
    /// Plain-text body (or supply HTML instead).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_body: Option<String>,
    /// HTML body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html_body: Option<String>,
    /// Transactional message stream; absent uses Postmark's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_stream: Option<String>,
}

/// Postmark's send acceptance; this is not a delivery receipt.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Acceptance {
    /// Recipient as returned by Postmark.
    pub to: String,
    /// Submission timestamp.
    pub submitted_at: String,
    /// Message identifier for the details endpoint.
    #[serde(rename = "MessageID")]
    pub message_id: String,
    /// Provider error code; zero in the documented acceptance.
    pub error_code: u32,
    /// Provider's response message.
    pub message: String,
}

/// One outbound message, with other documented and future fields retained verbatim.
#[derive(Debug, Deserialize)]
pub struct Message {
    /// Message identifier.
    #[serde(rename = "MessageID")]
    pub message_id: String,
    /// Activity status, such as `Queued` or `Sent`.
    #[serde(rename = "Status")]
    pub status: String,
    /// All remaining fields, including message events for a details response.
    #[serde(flatten)]
    pub fields: serde_json::Map<String, serde_json::Value>,
}

/// A single search page.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessagePage {
    /// Number of matching messages.
    pub total_count: u64,
    /// Messages in this page.
    pub messages: Vec<Message>,
}

/// A Postmark error document, or a rejected batch entry ([`BatchResult::Rejected`]). Message text
/// may contain user data, such as recipient addresses; it is omitted from Debug.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
#[derive(concord::Debug)]
pub struct ErrorDocument {
    /// Postmark's numeric error code.
    pub error_code: u32,
    /// Provider-supplied detail; treat as potentially sensitive.
    #[sensitive]
    pub message: String,
}

/// Postmark's verdict on one message of a batch send. A successful batch response holds one per
/// submitted message, in submission order: the `n`th result belongs to the `n`th email.
#[derive(Debug, Deserialize)]
#[serde(try_from = "BatchEntry")]
pub enum BatchResult {
    /// `ErrorCode` 0: accepted for sending (not delivered), with the ID for `details`.
    Accepted(Acceptance),
    /// Any other `ErrorCode`: Postmark rejected this message, for example an inactive recipient
    /// (406). Its provider text is omitted from Debug.
    Rejected(ErrorDocument),
}

/// One documented batch response element. Postmark omits `MessageID`, `SubmittedAt` and `To`
/// from a rejected entry; an entry claiming acceptance without them cannot be attributed, so the
/// batch response fails to decode and its outcome is unknown.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BatchEntry {
    error_code: u32,
    message: String,
    #[serde(rename = "MessageID")]
    message_id: Option<String>,
    to: Option<String>,
    submitted_at: Option<String>,
}

impl TryFrom<BatchEntry> for BatchResult {
    type Error = &'static str;

    fn try_from(entry: BatchEntry) -> Result<Self, Self::Error> {
        if entry.error_code != 0 {
            return Ok(Self::Rejected(ErrorDocument {
                error_code: entry.error_code,
                message: entry.message,
            }));
        }
        match (entry.message_id, entry.to, entry.submitted_at) {
            (Some(message_id), Some(to), Some(submitted_at)) => Ok(Self::Accepted(Acceptance {
                to,
                submitted_at,
                message_id,
                error_code: 0,
                message: entry.message,
            })),
            _ => Err("accepted batch entry lacks MessageID, To or SubmittedAt"),
        }
    }
}

/// Largest successful response read, in bytes (details may include bodies).
pub const RESPONSE_LIMIT: usize = 16 * 1024 * 1024;
/// Largest error document read, in bytes.
pub const ERROR_LIMIT: usize = 64 * 1024;

/// Failure of an SDK operation. Formatting never includes provider error text or credentials.
#[derive(Debug)]
#[non_exhaustive]
#[derive(concord::derive::Error)]
#[display(with = display_error)]
pub enum Error {
    /// Request building or a read operation failed.
    #[from(skip)]
    Request(concord::Error),
    /// A send may have taken effect but its response could not be received or decoded.
    #[from(skip)]
    UnknownOutcome(concord::Error),
    /// A non-success status, with a document if the bounded JSON read succeeded.
    Api {
        /// HTTP status.
        status: concord::reqwest::StatusCode,
        /// Parsed provider error, if available.
        document: Option<ErrorDocument>,
        /// Retry-After seconds from exactly one unsigned decimal field; otherwise absent.
        retry_after_seconds: Option<u64>,
    },
    /// A batch was answered with HTTP 2xx, but the number of results differs from the number of
    /// emails submitted, so no result can be attributed to an email. As with
    /// [`UnknownOutcome`](Self::UnknownOutcome), any email may have been accepted: do not resend
    /// the batch; the accepted entries' IDs are kept for investigation.
    UnmatchedBatch {
        /// Emails in the request.
        submitted: usize,
        /// Results as returned, in response order.
        results: Vec<BatchResult>,
    },
}
impl Error {
    /// A send was executed without a result that can be attributed to each submitted email
    /// ([`UnknownOutcome`](Self::UnknownOutcome) or [`UnmatchedBatch`](Self::UnmatchedBatch)).
    /// Investigate before resubmitting any of its emails.
    pub fn is_unknown_outcome(&self) -> bool {
        matches!(self, Self::UnknownOutcome(_) | Self::UnmatchedBatch { .. })
    }
}
fn display_error(value: &Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        Error::Request(e) => write!(f, "Postmark request failed: {e}"),
        Error::UnknownOutcome(e) => write!(f, "Postmark send outcome unknown: {e}"),
        Error::Api {
            status, document, ..
        } => {
            write!(f, "Postmark returned HTTP {}", status.as_u16())?;
            if let Some(d) = document {
                write!(f, " (code {})", d.error_code)?;
            }
            Ok(())
        }
        Error::UnmatchedBatch { submitted, results } => write!(
            f,
            "Postmark batch outcome unknown: {submitted} emails submitted, {} results returned",
            results.len()
        ),
    }
}

mod reply {
    /// A rule the policy applies to a decoded 2xx output of one operation.
    pub trait Reply: Sized {
        /// Results a 2xx response to `request` must hold, for an output with one result per
        /// submitted item; `None` for every other output.
        fn expected(_request: &concord::reqwest::Request) -> Option<usize> {
            None
        }
        /// Accepts the decoded output, given what [`expected`](Self::expected) returned.
        fn check(self, _expected: Option<usize>) -> Result<Self, super::Error> {
            Ok(self)
        }
    }
}
use reply::Reply;
impl Reply for Acceptance {}
impl Reply for MessagePage {}
impl Reply for Message {}
impl Reply for Vec<BatchResult> {
    fn expected(request: &concord::reqwest::Request) -> Option<usize> {
        // The body is this SDK's own JSON array of emails, buffered by Concord: count it again.
        let body = request.body()?.as_bytes()?;
        let emails = serde_json::from_slice::<Vec<serde::de::IgnoredAny>>(body).ok()?;
        Some(emails.len())
    }
    fn check(self, expected: Option<usize>) -> Result<Self, Error> {
        match expected {
            Some(submitted) if self.len() != submitted => Err(Error::UnmatchedBatch {
                submitted,
                results: self,
            }),
            _ => Ok(self),
        }
    }
}

/// Policy for generated calls: one execution, no retry, bounded provider error decoding, and a
/// batch's results attributed to its emails only when there is exactly one result per email.
#[derive(Clone, Copy, Debug, Default)]
pub struct PostmarkPolicy;
impl<T: Send + Reply> SendWith<T, crate::ErrorDocument> for PostmarkPolicy {
    type Output = Result<T, Error>;
    async fn send<E: Endpoint<T, crate::ErrorDocument> + Send>(&self, endpoint: E) -> Self::Output {
        let prepared = endpoint.prepare().map_err(Error::Request)?;
        let sending = prepared.request().method() == concord::reqwest::Method::POST;
        let expected = T::expected(prepared.request());
        let failure = |e: concord::Error| {
            if sending {
                Error::UnknownOutcome(e)
            } else {
                Error::Request(e)
            }
        };
        let received = prepared.receive().await.map_err(failure)?;
        if !received.status().is_success() {
            let status = received.status();
            let retry_after_seconds =
                concord::field::unsigned(received.head().headers(), "retry-after")
                    .ok()
                    .flatten();
            let (_, document) = received.failure(ERROR_LIMIT).await.into_document();
            return Err(Error::Api {
                status,
                document: document.ok(),
                retry_after_seconds,
            });
        }
        let output = received
            .into_success()
            .map_err(failure)?
            .decode()
            .await
            .map_err(failure)?;
        output.check(expected)
    }
}

concord::api! {
    /// Postmark server-scoped client. Use a server token, not an account token.
    pub client Postmark(#[sensitive] mut token: String) {
        base "https://api.postmarkapp.com";
        error crate::ErrorDocument in concord::Json;
        headers { "X-Postmark-Server-Token" = "{token}"; }
        format Json;
        limit 16777216;
        send PostmarkPolicy -> Result<_, Error>;

        /// Sending and inspecting outbound messages.
        scope outbound() {
            /// Submit one transactional email.
            POST send_email(body email: Email) -> Acceptance { path "email"; }
            /// Submit several transactional emails in one request: one result per email, in
            /// order. Postmark accepts up to 500 messages and 50 MB per call and rejects more.
            POST send_batch(body emails: Vec<Email>) -> Vec<BatchResult> { path "email/batch"; }
            /// Search one page. Postmark validates count and offset.
            GET list(query count: u32, query offset: u32) -> MessagePage { path "messages/outbound"; }
            /// Inspect a single outbound message by its ID.
            GET details(id: String) -> Message { path "messages/outbound/{id}/details"; }
        }
    }
}

/// Compile the public README example.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
