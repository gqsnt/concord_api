//! U14: decoding an API's error document from a non-2xx response.

use concord::{CodecError, Decode, Json, reqwest};
use serde::Deserialize;

use crate::api::{Inventory, Item, Rename};

/// An RFC 9457 problem document (`application/problem+json`), as this API sends it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Problem {
    /// A URI identifying the problem type.
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    /// Short summary.
    pub title: String,
    /// Occurrence-specific explanation.
    #[serde(default)]
    pub detail: Option<String>,
}

/// Why [`rename_item`] failed. Nothing is dropped: the status always accompanies the problem
/// document or the reason it could not be read.
#[derive(Debug, concord::derive::Error)]
#[display(with = display_rename_error)]
pub enum RenameError {
    /// Any failure other than a non-2xx status (build, transport, decoding the 2xx body).
    Concord(concord::Error),
    /// A non-2xx status with a decoded problem document.
    Problem {
        /// The response status.
        status: reqwest::StatusCode,
        /// The decoded document.
        problem: Problem,
    },
    /// A non-2xx status whose body could not be read (the native error, URL removed).
    ProblemUnread {
        /// The response status.
        status: reqwest::StatusCode,
        /// The read failure.
        error: reqwest::Error,
    },
    /// A non-2xx status whose body is not a problem document.
    ProblemUndecodable {
        /// The response status.
        status: reqwest::StatusCode,
        /// The content-free decode error (its source may quote the body).
        error: CodecError,
    },
}

fn display_rename_error(value: &RenameError, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        RenameError::Concord(e) => std::fmt::Display::fmt(e, f),
        RenameError::Problem { status, problem } => write!(f, "{status}: {}", problem.title),
        RenameError::ProblemUnread { status, .. } => write!(f, "{status}: error body unreadable"),
        RenameError::ProblemUndecodable { status, error } => write!(f, "{status}: {error}"),
    }
}

/// Renames an item; on a non-2xx status, returns the status together with the API's problem
/// document.
///
/// This native recipe preserves its public `reqwest::Error` and `CodecError` payloads. For a
/// bounded error document with head checks, use `Received::document` as shown in the quickstart;
/// its failures are `concord::Error`, which borrows rather than yields those inner errors.
///
/// Concord leaves a non-2xx response
/// **unread** inside [`concord::ErrorKind::Status`]: [`concord::Error::into_response`] hands it back,
/// and the public [`Decode::decode_body`] decodes the bytes with any format, regardless of status
/// and without a head check. Only status errors are error documents here: `into_response` also
/// returns the 2xx response of a [`concord::ErrorKind::Metadata`] error, so other kinds are
/// propagated first. The error
/// body is read fully into memory; against an untrusted server, read it with
/// `response.chunk()` up to your own limit instead.
pub async fn rename_item(api: &Inventory, id: u64, name: &str) -> Result<Item, RenameError> {
    let change = Rename {
        name: name.to_owned(),
    };
    let error = match api.rename(id, change).send().await {
        Ok(item) => return Ok(item),
        Err(error) if error.status().is_none() => return Err(error.into()), // not an error document
        Err(error) => error,
    };
    let response = error.into_response()?; // a status error: always its response
    let status = response.status();
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        // The native error's Display would include the URL; strip it like Concord does.
        Err(error) => {
            return Err(RenameError::ProblemUnread {
                status,
                error: error.without_url(),
            });
        }
    };
    match <Json as Decode<Problem>>::decode_body((), bytes) {
        Ok(problem) => Err(RenameError::Problem { status, problem }),
        Err(error) => Err(RenameError::ProblemUndecodable { status, error }),
    }
}
