//! Response metadata kept with results and errors.

use std::fmt;

use concord::reqwest::StatusCode;
use concord::reqwest::header::{CONTENT_LENGTH, HeaderMap};
use concord::{Operation, Received};

use crate::digest::Digest;
use crate::error::ProtocolProblem;

/// `Docker-Distribution-API-Version`.
pub const API_VERSION_HEADER: &str = "docker-distribution-api-version";
/// `Docker-Content-Digest`.
pub const CONTENT_DIGEST_HEADER: &str = "docker-content-digest";
/// `Docker-Upload-UUID`.
pub const UPLOAD_UUID_HEADER: &str = "docker-upload-uuid";

/// The head of a registry response: status, operation and header fields, owned. `Debug` prints
/// the status, the header **names** and the operation, never a value (a `Location` can be an
/// upload capability).
#[derive(Clone)]
pub struct ResponseMeta(concord::ResponseMeta);

impl ResponseMeta {
    pub(crate) fn capture<T>(received: &Received<T>) -> Self {
        Self(received.meta())
    }

    pub(crate) fn from_concord(meta: concord::ResponseMeta) -> Self {
        Self(meta)
    }

    /// The status.
    pub fn status(&self) -> StatusCode {
        self.0.status()
    }

    /// The operation that produced the response.
    pub fn operation(&self) -> Option<Operation> {
        self.0.operation()
    }

    /// `Docker-Distribution-API-Version` (`registry/2.0` on a V2 registry).
    pub fn api_version(&self) -> Option<&str> {
        text(self.0.headers(), API_VERSION_HEADER)
    }

    /// `Docker-Content-Digest`, as sent.
    pub fn content_digest(&self) -> Option<&str> {
        text(self.0.headers(), CONTENT_DIGEST_HEADER)
    }

    /// `Docker-Upload-UUID`, as sent. Correlation only: the SDK never builds a URL from it.
    pub fn upload_uuid(&self) -> Option<&str> {
        text(self.0.headers(), UPLOAD_UUID_HEADER)
    }

    /// Every header field, unredacted (a `Location` may be an upload capability: do not log it).
    pub fn headers(&self) -> &HeaderMap {
        self.0.headers()
    }

    /// Concord's copy of the head.
    pub fn raw(&self) -> &concord::ResponseMeta {
        &self.0
    }

    /// `Docker-Content-Digest`, parsed (`Ok(None)` when absent).
    pub(crate) fn digest_header(&self) -> Result<Option<Digest>, ProtocolProblem> {
        let Some(value) = concord::field::single(self.0.headers(), CONTENT_DIGEST_HEADER)
            .map_err(|_| ProtocolProblem::MalformedDigestHeader)?
        else {
            return Ok(None);
        };
        let text = value
            .to_str()
            .map_err(|_| ProtocolProblem::MalformedDigestHeader)?;
        Digest::parse(text.trim())
            .map(Some)
            .map_err(|_| ProtocolProblem::MalformedDigestHeader)
    }

    /// One unsigned decimal `Content-Length`, with OWS allowed (`Ok(None)` when absent).
    pub(crate) fn content_length(&self) -> Result<Option<u64>, ProtocolProblem> {
        concord::field::unsigned(self.0.headers(), CONTENT_LENGTH)
            .map_err(|_| ProtocolProblem::MalformedLength)
    }
}

fn text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
}

impl fmt::Debug for ResponseMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}
