use crate::Metadata;
use std::fmt;

/// A refused local input. No supplied values are retained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputError {
    /// Empty or invalid Basic API key.
    Credential,
    /// Invalid sending domain.
    Domain,
    /// Fixture origin is not an HTTP loopback address.
    BaseUrl,
    /// A timeout must be nonzero.
    Timeout,
    /// Native transport construction failed.
    TransportConfig,
    /// Missing or invalid sender, recipients, subject or text.
    Message,
    /// Empty or malformed message identifier.
    MessageId,
    /// More than three tag fields on one message.
    TagLimit,
    /// Empty, non-ASCII, control-containing or over-128-byte tag.
    Tag,
    /// Variable name is not a nonempty ASCII alphanumeric, underscore or hyphen name.
    VariableName,
    /// Unsafe filename or invalid media type.
    Attachment,
    /// The file could not be read.
    File,
    /// Local attachment or text size bound exceeded.
    Size,
    /// Invalid time range or page size.
    Search,
}

/// Refused event continuation. Contains no URL or cursor bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkError {
    /// Invalid URL, credentials, fragment, origin or domain collection.
    Untrusted,
    /// Cursor belongs to another search, domain, origin or credential.
    Context,
    /// A URL already fetched in this traversal was encountered again.
    Cycle,
}

/// HTTP classes documented by Mailgun, with an explicit fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpClass {
    /// HTTP 400.
    BadRequest,
    /// HTTP 401.
    Unauthorized,
    /// HTTP 403.
    Forbidden,
    /// HTTP 404.
    NotFound,
    /// HTTP 429; hints do not grant send replay permission.
    RateLimited,
    /// HTTP 5xx.
    Server,
    /// Any other non-success, including an unfollowed redirect.
    Unexpected,
}

/// Mailgun's JSON error document. Inspect its untrusted message explicitly.
#[derive(serde::Deserialize, concord::Debug)]
#[sensitive]
pub struct Problem {
    /// Provider text; excluded from Debug and Display.
    pub message: String,
}

/// A non-success status survives every error-document decoding failure.
#[derive(Debug)]
pub struct ProviderError {
    /// Response head with value-free Debug.
    pub meta: Metadata,
    /// A typed document, or the exact bounded-read/media/decoding failure.
    pub document: Result<Problem, Box<concord::Error>>,
}

impl ProviderError {
    /// Classify the authoritative HTTP status, independently of the body.
    pub fn class(&self) -> HttpClass {
        match self.meta.status().as_u16() {
            400 => HttpClass::BadRequest,
            401 => HttpClass::Unauthorized,
            403 => HttpClass::Forbidden,
            404 => HttpClass::NotFound,
            429 => HttpClass::RateLimited,
            500..=599 => HttpClass::Server,
            _ => HttpClass::Unexpected,
        }
    }
}

/// SDK failures; formatting never prints provider values or native error sources.
#[derive(Debug)]
pub enum Error {
    /// Invalid input before a request starts.
    Input(InputError),
    /// Build failure or event-read failure, preserving Concord's stage and head.
    Concord(Box<concord::Error>),
    /// A send may have taken effect but no usable acknowledgement was obtained.
    /// Never automatically replay, even with a buffered attachment.
    AmbiguousSend(Box<concord::Error>),
    /// Non-success status, including an unreadable error document.
    Provider(Box<ProviderError>),
    /// Event continuation rejected before dispatch.
    Link(LinkError),
}

impl Error {
    /// Concord diagnostic underlying a build, transport or successful-body failure.
    pub fn concord_error(&self) -> Option<&concord::Error> {
        match self {
            Self::Concord(error) | Self::AmbiguousSend(error) => Some(error),
            _ => None,
        }
    }
}

impl From<concord::Error> for Error {
    fn from(value: concord::Error) -> Self {
        Self::Concord(Box::new(value))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(kind) => write!(f, "invalid Mailgun input: {kind:?}"),
            Self::Link(kind) => write!(f, "Mailgun continuation refused: {kind:?}"),
            Self::Concord(error) => write!(f, "Mailgun request failed: {:?}", error.kind()),
            Self::AmbiguousSend(_) => {
                f.write_str("Mailgun send outcome unknown; do not automatically replay")
            }
            Self::Provider(error) => write!(
                f,
                "Mailgun HTTP {} ({:?})",
                error.meta.status().as_u16(),
                error.class()
            ),
        }
    }
}

// Deliberately no source chain: native sources may contain secrets.
impl std::error::Error for Error {}
