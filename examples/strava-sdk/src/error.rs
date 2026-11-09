//! Value-free diagnostics with explicit access to provider payloads.
use crate::{RateLimits, StravaOperation, UploadStatus};
use serde::Deserialize;
use std::fmt;

/// A provider error document. Debug withholds every untrusted string.
#[derive(Deserialize, concord::Debug)]
#[sensitive]
pub struct Problem {
    /// Human-readable provider message; may contain HTML or sensitive content.
    pub message: String,
    /// Structured field failures.
    #[serde(default)]
    pub errors: Vec<ProblemField>,
}
/// One field failure from a provider error document.
#[derive(Deserialize, concord::Debug)]
#[sensitive]
pub struct ProblemField {
    /// Provider resource name.
    pub resource: String,
    /// Rejected field.
    pub field: String,
    /// Provider error code.
    pub code: String,
}

/// Error schemas documented for general API failures and activity uploads.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ErrorDocument {
    /// General API/OAuth rejection.
    Problem(Problem),
    /// Upload rejection; the provider can return this schema on HTTP 400 too.
    Upload(UploadStatus),
}

/// An HTTP failure retains its status even if its error body cannot be read.
#[derive(Debug)]
pub struct HttpFailure {
    /// HTTP head with safe Debug formatting.
    pub meta: concord::ResponseMeta,
    /// Typed error or its exact body failure class (including metadata/limit/transport).
    pub document: Result<ErrorDocument, concord::Error>,
    /// All available usage windows, including on 429.
    pub rates: RateLimits,
    /// An upload or activity-update server error can follow a committed write.
    pub ambiguous: bool,
}

/// SDK failure. Debug and Display do not render credentials or provider body strings.
#[derive(Debug)]
pub enum Error {
    /// Static input or protocol refusal.
    Invalid(&'static str),
    /// Preparation, transport, media, decode or body-bound failure.
    Concord(concord::Error),
    /// An upload or activity update may have been applied; never replay it automatically.
    /// After an uncertain update, re-read the activity (`ActivityUpdate::is_reflected_in`).
    Ambiguous(concord::Error),
    /// Provider HTTP rejection, with an independently fallible error document.
    Http(Box<HttpFailure>),
    /// Shared credential acquisition failure; explicit typed inspection remains possible.
    Auth(concord_auth::RefreshError<Error>),
    /// Local refusal based on previously observed provider windows.
    Admission(concord_ratelimit::Error),
    /// The upload was accepted but processing failed (duplicate or malformed activity).
    Processing(Box<UploadStatus>),
    /// Poll budget ended; the last upload identifier and status are retained for resumption.
    PollBounded(Box<UploadStatus>),
    /// The stored granted scopes lack this permission. Nothing was sent and no token was
    /// refreshed; re-authorize the athlete requesting it.
    MissingScope(&'static str),
}
impl Error {
    pub(crate) fn from_call(error: concord::Error, uncertain: bool) -> Self {
        if uncertain && error.delivery() != concord::Delivery::NotSent {
            Self::Ambiguous(error)
        } else {
            Self::Concord(error)
        }
    }
    /// Whether an upload or activity update may have been applied but cannot be confirmed.
    pub fn is_ambiguous(&self) -> bool {
        matches!(self, Self::Ambiguous(_)) || matches!(self, Self::Http(e) if e.ambiguous)
    }
    /// HTTP status when a head arrived, including body failures and auth-service failures.
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Http(e) => Some(e.meta.status().as_u16()),
            Self::Concord(e) | Self::Ambiguous(e) => e.head().map(|h| h.status().as_u16()),
            Self::Auth(concord_auth::RefreshError::Acquire(e)) => e.status(),
            _ => None,
        }
    }
}
impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Self::Concord(error)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid(reason) => reason,
            Self::Concord(_) => "Strava request failed",
            // No wildcard arm: a new operation chooses its wording here (E0004 otherwise). Reads
            // are never uncertain; an error naming no Strava operation keeps the upload wording.
            Self::Ambiguous(e) => match e.operation().and_then(StravaOperation::from_operation) {
                Some(StravaOperation::ActivitiesUpdate) => {
                    "activity update outcome is uncertain; re-read the activity, it is never resent"
                }
                Some(
                    StravaOperation::UploadsCreate
                    | StravaOperation::AthleteActivities
                    | StravaOperation::ActivitiesGet
                    | StravaOperation::UploadsStatus,
                )
                | None => "upload outcome is ambiguous; automatic replay is forbidden",
            },
            Self::Http(_) => "Strava HTTP rejection",
            Self::Auth(_) => "Strava token acquisition failed",
            Self::Admission(_) => "Strava quota admission refused",
            Self::Processing(_) => "Strava upload processing failed",
            Self::PollBounded(_) => "Strava upload poll budget exhausted",
            Self::MissingScope(_) => "Strava token lacks a required scope",
        })
    }
}
// Untrusted errors are deliberately reachable only through explicit variants/accessors.
impl std::error::Error for Error {}

#[cfg(test)]
mod sdk_type_canaries {
    use super::*;

    #[test]
    fn nested_problem_fields_withhold_each_provider_string() {
        let mut c = concord_check_canary::Canaries::new("strava-problem-field");
        let field = ProblemField {
            resource: c.issue(),
            field: c.issue(),
            code: c.issue(),
        };
        assert!(field.resource.starts_with("CANARY"));
        c.assert_debug_absent("ProblemField", &field);
    }
}
