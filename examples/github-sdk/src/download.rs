//! Artifact download: GitHub's `302` handled at the native boundary, and the archive streamed into
//! a caller-supplied sink without the token and without buffering the body.
//!
//! GitHub's contract ([artifacts]): `GET /repos/{owner}/{repo}/actions/artifacts/{id}/zip`
//! answers `302 Found` whose `Location` is a download URL that expires after one minute, or
//! `410 Gone`. The signed URL is on another host and is itself the credential.
//!
//! Two steps, each a value the caller holds:
//!
//! 1. [`locate`] sends the **declared** endpoint (`download_artifact`, no typed output) once with
//!    `Prepared::receive`, which returns the response for any status, unread, then reads the head
//!    itself: a `3xx` with a `Location` becomes an [`ArtifactLocation`] (with the API response's
//!    [`ResponseMeta`]: request id, rate limit); `410` becomes [`Located::Gone`]; any other status
//!    goes back to the endpoint's decoder, so it is Concord's `Status` error with the response
//!    unread. A `3xx` is never treated as a decoded success. If the `reqwest::Client` followed the
//!    redirect itself (reqwest's default policy), the response is refused
//!    ([`LocateError::RedirectFollowed`]): the SDK did not choose what was forwarded. Build the
//!    client with [`crate::redirect_policy`]. The head is read on the native response
//!    (`Received::into_parts`): that the client followed a redirect is visible only in the
//!    response URL, which also resolves a relative `Location`.
//! 2. [`stream_to`] requests the location with a **new native request carrying no header at all**
//!    (no `Authorization`, no `X-GitHub-Api-Version`, no `User-Agent`; reqwest adds `Accept` and
//!    `Host`), checks the status (Concord's unlabelled `-> stream` handling, `Decoder::incoming`:
//!    no declaration owns the signed URL) and a declared `Content-Length` against
//!    [`DownloadLimits`], and hands each body chunk to the [`ByteSink`] as it arrives
//!    (`concord::stream::ByteSource` on the response). Nothing is collected. A transport failure,
//!    the byte limit or a sink error stops the transfer and drops the response; the error reports
//!    the bytes received and written, so a partial file is never mistaken for a complete one.
//!
//! The location's scheme must be `https`, unless the configured API base itself is plain `http`
//! (a loopback test configuration); a location with user information is refused.
//!
//! [artifacts]: https://docs.github.com/en/rest/actions/artifacts#download-an-artifact

use std::fmt;
use std::future::Future;

use concord::bytes::Bytes;
use concord::reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, LOCATION};
use concord::reqwest::{self, Method, StatusCode, Url};
use concord::stream::{ByteSource, SourceError};
use concord::{Decoder, Received};

use crate::context::RepoContext;
use crate::error::GitHubError;
use crate::ids::ArtifactId;
use crate::meta::ResponseMeta;
use crate::params::ArchiveFormat;

/// Where GitHub redirected an artifact download. The URL is a short-lived bearer credential:
/// `Debug` shows only its host.
pub struct ArtifactLocation {
    artifact: ArtifactId,
    url: Url,
    meta: ResponseMeta,
}

impl ArtifactLocation {
    /// The artifact.
    pub fn artifact(&self) -> ArtifactId {
        self.artifact
    }

    /// The download host.
    pub fn host(&self) -> &str {
        self.url.host_str().unwrap_or_default()
    }

    /// The signed URL, **unredacted**.
    pub fn raw_url(&self) -> &Url {
        &self.url
    }

    /// The metadata of GitHub's `3xx` response (status, request id, rate limit).
    pub fn meta(&self) -> &ResponseMeta {
        &self.meta
    }
}

impl fmt::Debug for ArtifactLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArtifactLocation")
            .field("artifact", &self.artifact)
            .field("host", &self.host())
            .field("url", &format_args!("<withheld>"))
            .field("meta", &self.meta)
            .finish()
    }
}

/// What [`locate`] found.
#[derive(Debug)]
pub enum Located {
    /// GitHub redirected to a download URL.
    Redirect(ArtifactLocation),
    /// `410 Gone`: the artifact's bytes are no longer available (expired or removed).
    Gone {
        /// The `410` response's metadata.
        meta: ResponseMeta,
    },
}

/// Why [`locate`] produced no location.
#[non_exhaustive]
pub enum LocateError {
    /// The request failed or GitHub answered another status (a Concord error; a status error
    /// keeps its unread response).
    Api(GitHubError),
    /// A redirect status without a usable `Location`.
    MissingLocation {
        /// The response's metadata.
        meta: Box<ResponseMeta>,
    },
    /// The `Location` is not an acceptable download URL (scheme, user information).
    UnsafeLocation {
        /// The response's metadata.
        meta: Box<ResponseMeta>,
    },
    /// The `reqwest::Client` followed the redirect itself; the response was refused unread.
    RedirectFollowed {
        /// The followed response's metadata.
        meta: Box<ResponseMeta>,
    },
    /// GitHub answered `2xx` instead of redirecting.
    NoRedirect {
        /// The response's metadata.
        meta: Box<ResponseMeta>,
    },
}

impl LocateError {
    fn label(&self) -> &'static str {
        match self {
            Self::Api(_) => "request failed",
            Self::MissingLocation { .. } => "redirect without a usable Location",
            Self::UnsafeLocation { .. } => "Location is not an acceptable download URL",
            Self::RedirectFollowed { .. } => {
                "the HTTP client followed the redirect itself; refused (use redirect_policy())"
            }
            Self::NoRedirect { .. } => "GitHub answered 2xx instead of redirecting",
        }
    }

    /// The response metadata, when a response arrived.
    pub fn meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Api(error) => error.meta(),
            Self::MissingLocation { meta }
            | Self::UnsafeLocation { meta }
            | Self::RedirectFollowed { meta }
            | Self::NoRedirect { meta } => Some(meta),
        }
    }
}

impl fmt::Display for LocateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(error) => fmt::Display::fmt(error, f),
            other => write!(f, "GitHub artifact download: {}", other.label()),
        }
    }
}

impl fmt::Debug for LocateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(error) => f.debug_tuple("Api").field(error).finish(),
            other => f
                .debug_struct("LocateError")
                .field("problem", &other.label())
                .field("meta", &other.meta())
                .finish(),
        }
    }
}

impl std::error::Error for LocateError {}

impl From<GitHubError> for LocateError {
    fn from(error: GitHubError) -> Self {
        Self::Api(error)
    }
}

/// Step 1: asks GitHub where to download `artifact` (`zip`). See the
/// [module documentation](self).
///
/// # Errors
///
/// [`LocateError`].
pub async fn locate(repo: &RepoContext, artifact: ArtifactId) -> Result<Located, LocateError> {
    let prepared = repo
        .actions()
        .download_artifact(artifact)
        .set_archive_format(ArchiveFormat::Zip)
        .prepare()
        .map_err(GitHubError::from)?;
    let requested = prepared.request().url().clone();
    let received = prepared
        .receive()
        .await
        .map_err(|error| GitHubError::api(error, None))?;
    // The native response, unread: its URL says whether the client followed the redirect.
    let (response, decoder) = received.into_parts();
    let meta = ResponseMeta::capture(&response);
    let status = response.status();
    if status.is_redirection() {
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| response.url().join(v).ok());
        let Some(url) = location else {
            return Err(LocateError::MissingLocation {
                meta: Box::new(meta),
            });
        };
        let api_is_plain_http = repo.context().base_url().scheme() == "http";
        let scheme_ok = url.scheme() == "https" || (api_is_plain_http && url.scheme() == "http");
        if !scheme_ok || !url.username().is_empty() || url.password().is_some() {
            return Err(LocateError::UnsafeLocation {
                meta: Box::new(meta),
            });
        }
        // The redirect body (if any) is not read; dropping the response closes or reuses the
        // connection as hyper decides.
        return Ok(Located::Redirect(ArtifactLocation {
            artifact,
            url,
            meta,
        }));
    }
    if status == StatusCode::GONE {
        return Ok(Located::Gone { meta });
    }
    if status.is_success() {
        if *response.url() != requested {
            return Err(LocateError::RedirectFollowed {
                meta: Box::new(meta),
            });
        }
        return Err(LocateError::NoRedirect {
            meta: Box::new(meta),
        });
    }
    // Any other status: the endpoint's own handling (a `Status` error, response unread).
    match decoder.decode(response).await {
        Ok(()) => Err(LocateError::NoRedirect {
            meta: Box::new(meta),
        }),
        Err(error) => Err(GitHubError::api(error, Some(meta)).into()),
    }
}

/// A caller-supplied destination for downloaded bytes (a file, a bounded buffer, a hasher).
/// Each chunk is handed over as it arrives; returning an error stops the transfer (that is how a
/// sink cancels).
pub trait ByteSink {
    /// The sink's error.
    type Error;
    /// Accepts the next chunk.
    fn write(&mut self, chunk: Bytes) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// Bounds of one download.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DownloadLimits {
    /// The most body bytes accepted; a larger declared `Content-Length` is refused before the
    /// body is read, and a body that grows past it is cut off.
    pub max_bytes: u64,
}

/// What a completed transfer received.
#[derive(Clone, PartialEq, Eq)]
pub struct StreamReceipt {
    /// The download host.
    pub host: String,
    /// The download response's status.
    pub status: StatusCode,
    /// Its single unsigned decimal `Content-Length`, if valid (surrounding OWS allowed).
    pub content_length: Option<u64>,
    /// Its `Content-Type`, if declared.
    pub content_type: Option<String>,
    /// Body bytes received and written to the sink.
    pub bytes: u64,
    /// Chunks handed to the sink.
    pub chunks: u64,
}

impl fmt::Debug for StreamReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamReceipt")
            .field("host", &self.host)
            .field("status", &self.status.as_u16())
            .field("content_length", &self.content_length)
            .field("content_type", &self.content_type)
            .field("bytes", &self.bytes)
            .field("chunks", &self.chunks)
            .finish()
    }
}

/// Why a transfer did not complete. Bytes already written to the sink stay written: the sink
/// decides what a partial write means (delete, keep for inspection).
pub enum StreamError<E> {
    /// The download host could not be reached, or answered a non-2xx status (Concord's error,
    /// URL removed; a status error keeps the unread response). Unlabeled: no endpoint name.
    Request(concord::Error),
    /// The declared length or the received bytes exceed [`DownloadLimits::max_bytes`].
    TooLarge {
        /// The declared `Content-Length`, if any.
        declared: Option<u64>,
        /// Bytes written to the sink before stopping (the chunk crossing the limit is not).
        written: u64,
        /// The limit.
        limit: u64,
    },
    /// Reading the body failed after `received` bytes (a dropped connection, a timeout).
    Read {
        /// Bytes received (and written) before the failure.
        received: u64,
        /// The source error (its underlying reqwest error has its URL removed).
        error: SourceError,
    },
    /// The sink refused a chunk after `written` bytes had been written.
    Sink {
        /// Bytes accepted by the sink before the refused chunk.
        written: u64,
        /// The sink's error.
        error: E,
    },
}

impl<E> StreamError<E> {
    /// Bytes written to the sink before the transfer stopped.
    pub fn written(&self) -> u64 {
        match self {
            Self::Request(_) => 0,
            Self::TooLarge { written, .. } => *written,
            Self::Read { received, .. } => *received,
            Self::Sink { written, .. } => *written,
        }
    }
}

impl<E> fmt::Display for StreamError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => write!(f, "artifact download: {error}"),
            Self::TooLarge {
                declared: Some(declared),
                limit,
                ..
            } => write!(
                f,
                "artifact download: declared {declared} bytes exceed {limit}"
            ),
            Self::TooLarge { written, limit, .. } => write!(
                f,
                "artifact download: more than {limit} bytes (stopped after writing {written})"
            ),
            Self::Read { received, .. } => {
                write!(
                    f,
                    "artifact download: body read failed after {received} bytes"
                )
            }
            Self::Sink { written, .. } => {
                write!(f, "artifact download: sink stopped after {written} bytes")
            }
        }
    }
}

impl<E> fmt::Debug for StreamError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => f.debug_tuple("Request").field(error).finish(),
            Self::TooLarge {
                declared,
                written,
                limit,
            } => f
                .debug_struct("TooLarge")
                .field("declared", declared)
                .field("written", written)
                .field("limit", limit)
                .finish(),
            Self::Read { received, error } => f
                .debug_struct("Read")
                .field("received", received)
                .field("error", error)
                .finish(),
            Self::Sink { written, .. } => f
                .debug_struct("Sink")
                .field("written", written)
                .field("error", &format_args!("<sink error>"))
                .finish(),
        }
    }
}

impl<E> std::error::Error for StreamError<E> {}

/// Step 2: streams the archive at `location` into `sink`, with `http` (normally
/// [`crate::GitHubContext::http`]). See the [module documentation](self).
///
/// # Errors
///
/// [`StreamError`].
pub async fn stream_to<S: ByteSink>(
    http: &reqwest::Client,
    location: &ArtifactLocation,
    sink: &mut S,
    limits: DownloadLimits,
) -> Result<StreamReceipt, StreamError<S::Error>> {
    // A fresh request: no header of the GitHub API (in particular no Authorization) exists on it.
    let request = reqwest::Request::new(Method::GET, location.url.clone());
    // No declaration owns this URL: the unlabelled `-> stream` handling gives Concord's transport
    // error (URL removed) and, for a non-2xx, its status error (response unread).
    let mut response = Decoder::incoming()
        .execute(http, request)
        .await
        .and_then(Received::into_incoming)
        .map_err(StreamError::Request)?
        .into_response();
    let status = response.status();
    let content_length = concord::field::unsigned(response.headers(), CONTENT_LENGTH)
        .ok()
        .flatten();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    if let Some(declared) = content_length
        && declared > limits.max_bytes
    {
        return Err(StreamError::TooLarge {
            declared: Some(declared),
            written: 0,
            limit: limits.max_bytes,
        });
    }
    let mut bytes = 0u64;
    let mut chunks = 0u64;
    loop {
        let chunk = match response.next_chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                return Err(StreamError::Read {
                    received: bytes,
                    error,
                });
            }
        };
        if bytes + chunk.len() as u64 > limits.max_bytes {
            return Err(StreamError::TooLarge {
                declared: content_length,
                written: bytes,
                limit: limits.max_bytes,
            });
        }
        let len = chunk.len() as u64;
        if let Err(error) = sink.write(chunk).await {
            return Err(StreamError::Sink {
                written: bytes,
                error,
            });
        }
        bytes += len;
        chunks += 1;
    }
    Ok(StreamReceipt {
        host: location.host().to_owned(),
        status,
        content_length,
        content_type,
        bytes,
        chunks,
    })
}
