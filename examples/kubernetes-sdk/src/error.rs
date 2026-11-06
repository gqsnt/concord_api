//! Errors: Concord's error kept whole, and the API's `Status` document read from a status
//! error.
//!
//! Kubernetes answers failures with a `Status` document (`reason`, `code`, `message`). A non-2xx
//! response reaches the SDK as a Concord `Status` error that keeps the **unread** response;
//! [`ApiError::read`] takes it, reads at most [`STATUS_BODY_LIMIT`] bytes and decodes the
//! document with `Json`'s body-only decoder (the error document's head is not judged). Display and
//! Debug name the endpoint, the HTTP status and the reason — never the message (it can quote object
//! names), a token or a URL.

use std::fmt;

use concord::bytes::{Bytes, BytesMut};
use concord::reqwest::{self, StatusCode};
use concord::{Decode, Json};

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
    /// Reads the `Status` document of a Concord `Status` error (a non-2xx answer). Any other
    /// error (build, transport, a 2xx refused by the format, decode) is returned unchanged.
    ///
    /// # Errors
    ///
    /// The original error when it carries no non-2xx response.
    pub async fn read(error: concord::Error) -> Result<Self, concord::Error> {
        if error.status().is_none() {
            return Err(error);
        }
        let endpoint = error.endpoint();
        let response = error.into_response()?;
        Ok(Self::from_response(endpoint, response).await)
    }

    /// Reads the `Status` document of a non-2xx response obtained another way (the unread
    /// response of a reader's `Status` refusal, as in [`crate::watch::open`]). The body is read up
    /// to [`STATUS_BODY_LIMIT`]; a body that is larger, not JSON or not a `Status` leaves
    /// [`status`](Self::status) empty.
    pub async fn from_response(
        endpoint: Option<&'static str>,
        response: reqwest::Response,
    ) -> Self {
        let http_status = response.status();
        let status = read_bounded(response)
            .await
            .and_then(|body| <Json as Decode<Status>>::decode_body((), body).ok())
            .map(Box::new);
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

async fn read_bounded(mut response: reqwest::Response) -> Option<Bytes> {
    let mut body = BytesMut::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if body.len() + chunk.len() > STATUS_BODY_LIMIT {
                    return None;
                }
                body.extend_from_slice(&chunk);
            }
            Ok(None) => return Some(body.freeze()),
            Err(_) => return None,
        }
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
pub enum KubeError {
    /// Concord's error: the request was not built or sent, the 2xx head was refused, or the body
    /// did not decode. Kept whole (redacted by Concord).
    Request(concord::Error),
    /// The server answered non-2xx.
    Api(ApiError),
    /// A bearer token that cannot be a header value.
    InvalidToken,
}

impl KubeError {
    /// Classifies a Concord error, reading the `Status` document of a non-2xx answer.
    pub async fn from_concord(error: concord::Error) -> Self {
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

impl fmt::Display for KubeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => error.fmt(f),
            Self::Api(error) => error.fmt(f),
            Self::InvalidToken => crate::ids::InvalidToken.fmt(f),
        }
    }
}

impl std::error::Error for KubeError {}
