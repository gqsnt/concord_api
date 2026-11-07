//! A bounded Postmark transactional-email and outbound-message SDK.
//!
//! The generated `send()` performs one Concord execution. Configure the supplied HTTP client
//! with a timeout and without application-level retries. Acceptance is not delivery; a send
//! whose response cannot be read has an unknown outcome and must not be blindly repeated.

use concord::{Endpoint, Json, SendWith};
use serde::{Deserialize, Serialize};
use std::fmt;

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

/// A Postmark error document. Message text may contain user data; it is omitted from Debug.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ErrorDocument {
    /// Postmark's numeric error code.
    pub error_code: u32,
    /// Provider-supplied detail; treat as potentially sensitive.
    pub message: String,
}
impl fmt::Debug for ErrorDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorDocument")
            .field("error_code", &self.error_code)
            .field("message", &"<redacted>")
            .finish()
    }
}

/// Largest successful response read, in bytes (details may include bodies).
pub const RESPONSE_LIMIT: usize = 16 * 1024 * 1024;
/// Largest error document read, in bytes.
pub const ERROR_LIMIT: usize = 64 * 1024;

/// Failure of an SDK operation. Formatting never includes provider error text or credentials.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Request building or a read operation failed.
    Request(concord::Error),
    /// A send was executed but its response could not be received or decoded.
    UnknownOutcome(concord::Error),
    /// A non-success status, with a document if the bounded JSON read succeeded.
    Api {
        /// HTTP status.
        status: concord::reqwest::StatusCode,
        /// Parsed provider error, if available.
        document: Option<ErrorDocument>,
        /// Retry-After value when it is an integer number of seconds.
        retry_after_seconds: Option<u64>,
    },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(e) => write!(f, "Postmark request failed: {e}"),
            Self::UnknownOutcome(e) => write!(f, "Postmark send outcome unknown: {e}"),
            Self::Api {
                status, document, ..
            } => {
                write!(f, "Postmark returned HTTP {}", status.as_u16())?;
                if let Some(d) = document {
                    write!(f, " (code {})", d.error_code)?;
                }
                Ok(())
            }
        }
    }
}
impl std::error::Error for Error {}

/// Policy for generated calls: one execution, no retry, bounded provider error decoding.
#[derive(Clone, Copy, Debug, Default)]
pub struct PostmarkPolicy;
impl<T: Send> SendWith<T> for PostmarkPolicy {
    type Output = Result<T, Error>;
    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let prepared = endpoint.prepare().map_err(Error::Request)?;
        let sending = prepared.request().method() == concord::reqwest::Method::POST;
        let received = prepared.receive().await.map_err(|e| {
            if sending {
                Error::UnknownOutcome(e)
            } else {
                Error::Request(e)
            }
        })?;
        if !received.status().is_success() {
            let status = received.status();
            let retry_after_seconds = received
                .head()
                .headers()
                .get(concord::reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse().ok());
            let (_, document) = received.document::<Json, ErrorDocument>(ERROR_LIMIT).await;
            return Err(Error::Api {
                status,
                document: document.ok(),
                retry_after_seconds,
            });
        }
        received.decode().await.map_err(|e| {
            if sending {
                Error::UnknownOutcome(e)
            } else {
                Error::Request(e)
            }
        })
    }
}

concord::api! {
    /// Postmark server-scoped client. Use a server token, not an account token.
    pub client Postmark(#[sensitive] token: String) {
        base "https://api.postmarkapp.com";
        headers { "X-Postmark-Server-Token" = "{token}"; }
        format Json;
        limit 16777216;
        send PostmarkPolicy;

        /// Sending and inspecting outbound messages.
        scope outbound() {
            /// Submit one transactional email.
            POST send_email(body email: Email) -> Acceptance { path "email"; }
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
