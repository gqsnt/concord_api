//! Errors retain Concord failures and the API's declared `Status` document.
//!
//! Kubernetes answers failures with a `Status` document (`reason`, `code`, `message`). A non-2xx
//! reaches the SDK as `concord::Error<Status>`, with its bounded provider answer already read.
//! [`ApiError::read`] exposes it. Native responses use [`ApiError::from_response`].
//! Display and Debug name the endpoint, HTTP status and reason, withholding messages, names,
//! credentials and URLs.

use std::fmt;

use concord::reqwest::{self, StatusCode};
use concord::{Decoder, IgnoreContentType, Json};

use crate::model::Status;

/// The most bytes read from an error response body before giving up on decoding it.
pub const STATUS_BODY_LIMIT: usize = 64 * 1024;

/// A non-2xx answer of the API server, with its `Status` document when one could be read.
pub struct ApiError {
    endpoint: Option<&'static str>,
    http_status: StatusCode,
    status: Option<Box<Status>>,
}

impl ApiError {
    /// Takes the declared `Status` document of a Concord non-2xx error. Any other
    /// error (build, transport, a 2xx refused by the format, decode) is returned unchanged.
    ///
    /// # Errors
    ///
    /// The original error when it carries no non-2xx response.
    pub async fn read(error: concord::Error<Status>) -> Result<Self, concord::Error> {
        error.into_provider().map(Self::from)
    }

    /// Reads the `Status` document of a non-2xx response obtained another way (the unread
    /// response of a reader's `Status` refusal, as in [`crate::watch::open`]). The body is read up
    /// to [`STATUS_BODY_LIMIT`]; a body that is larger, not JSON or not a `Status` leaves
    /// [`status`](Self::status) empty.
    pub async fn from_response(
        endpoint: Option<&'static str>,
        response: reqwest::Response,
    ) -> Self {
        let (meta, document) = Decoder::drain()
            .receive(response)
            .document::<IgnoreContentType<Json>, Status>(STATUS_BODY_LIMIT)
            .await;
        let http_status = meta.status();
        let status = document.ok().map(Box::new);
        Self {
            endpoint,
            http_status,
            status,
        }
    }

    /// The endpoint, such as `Kube.core_v1.namespaced.pods`.
    pub fn endpoint(&self) -> Option<&'static str> {
        self.endpoint
    }

    /// The HTTP status.
    pub fn http_status(&self) -> StatusCode {
        self.http_status
    }

    /// The decoded `Status` document, if the body was one.
    pub fn status(&self) -> Option<&Status> {
        self.status.as_deref()
    }

    /// The `Status` reason (`NotFound`, `Forbidden`, `Expired`, ...).
    pub fn reason(&self) -> Option<&str> {
        self.status.as_ref().and_then(|s| s.reason.as_deref())
    }

    /// `410 Gone` (reason `Expired` or `Gone`): the requested resource version or continue
    /// token is no longer available; a consistent read must start again.
    pub fn is_expired(&self) -> bool {
        self.http_status == StatusCode::GONE
    }

    /// Whether the expired-continue answer carried a token for an **inconsistent** continuation
    /// (`Status.metadata.continue`). This SDK never uses it: an inconsistent list would mix
    /// snapshots, which the inventory contract forbids.
    pub fn offers_inconsistent_continue(&self) -> bool {
        self.status
            .as_ref()
            .is_some_and(|s| s.metadata.next_token().is_some())
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(endpoint) = self.endpoint {
            write!(f, "{endpoint}: ")?;
        }
        write!(f, "Kubernetes API answered {}", self.http_status.as_u16())?;
        match self.reason() {
            Some(reason) => write!(f, " (reason {reason})"),
            None => Ok(()),
        }
    }
}

impl fmt::Debug for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiError")
            .field("endpoint", &self.endpoint)
            .field("http_status", &self.http_status.as_u16())
            .field("status", &self.status)
            .finish()
    }
}

impl std::error::Error for ApiError {}

/// A failed single operation.
#[derive(Debug)]
#[non_exhaustive]
#[derive(concord::derive::Error)]
#[display(with = display_kube_error)]
pub enum KubeError {
    /// Concord's error: the request was not built or sent, the 2xx head was refused, or the body
    /// did not decode. Kept whole (redacted by Concord).
    Request(concord::Error),
    /// The server answered non-2xx.
    #[from(concord::Error<Status>)]
    Api(ApiError),
    /// A bearer token that cannot be a header value.
    InvalidToken,
}

impl KubeError {
    /// Classifies a Concord error, reading the `Status` document of a non-2xx answer.
    pub async fn from_concord(error: concord::Error<Status>) -> Self {
        match ApiError::read(error).await {
            Ok(api) => Self::Api(api),
            Err(error) => Self::Request(error),
        }
    }

    /// The API error, for a non-2xx answer.
    pub fn api(&self) -> Option<&ApiError> {
        match self {
            Self::Api(api) => Some(api),
            _ => None,
        }
    }
}

fn display_kube_error(value: &KubeError, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        KubeError::Request(error) => std::fmt::Display::fmt(error, f),
        KubeError::Api(error) => std::fmt::Display::fmt(error, f),
        KubeError::InvalidToken => std::fmt::Display::fmt(&crate::ids::InvalidToken, f),
    }
}

impl From<concord::Failure<Status>> for ApiError {
    fn from(failure: concord::Failure<Status>) -> Self {
        let endpoint = failure
            .operation()
            .filter(concord::Operation::is_endpoint)
            .map(|operation| operation.name());
        let http_status = failure.status();
        let (_, document) = failure.into_document();
        Self {
            endpoint,
            http_status,
            status: document.ok().map(Box::new),
        }
    }
}
