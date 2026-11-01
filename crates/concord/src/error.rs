//! The single error type returned by generated code.

use std::error::Error as StdError;
use std::fmt;

use crate::__private::EndpointMeta;
use crate::repr::BodyError;

/// Why a Concord call failed.
///
/// `Display` and `Debug` contain only static information — the endpoint, the HTTP method, the
/// failing placement or the status code — never request values, URLs, header values or body
/// content. See the crate documentation for the exact scope of this guarantee.
pub struct Error {
    inner: Box<Inner>,
}

struct Inner {
    endpoint: Option<&'static EndpointMeta>,
    repr: Repr,
}

enum Repr {
    Build {
        what: &'static str,
        reason: &'static str,
    },
    Encode(BodyError),
    Transport(reqwest::Error),
    Status(reqwest::Response),
    Decode(BodyError),
}

/// The stage at which a call failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The request could not be built from the selected values (invalid path segment, host
    /// label, header value, base URL or body). Nothing was sent.
    Build,
    /// reqwest failed to send the request or to read the response.
    Transport,
    /// The server answered with a non-2xx status.
    Status,
    /// A 2xx response body could not be decoded with the declared representation.
    Decode,
}

impl Error {
    pub(crate) fn build(
        endpoint: Option<&'static EndpointMeta>,
        what: &'static str,
        reason: &'static str,
    ) -> Self {
        Self::new(endpoint, Repr::Build { what, reason })
    }

    pub(crate) fn from_encode(endpoint: &'static EndpointMeta, error: BodyError) -> Self {
        Self::new(Some(endpoint), Repr::Encode(error))
    }

    pub(crate) fn from_transport(endpoint: &'static EndpointMeta, error: reqwest::Error) -> Self {
        Self::new(Some(endpoint), Repr::Transport(error.without_url()))
    }

    pub(crate) fn from_status(
        endpoint: &'static EndpointMeta,
        response: reqwest::Response,
    ) -> Self {
        Self::new(Some(endpoint), Repr::Status(response))
    }

    pub(crate) fn from_decode(endpoint: &'static EndpointMeta, error: BodyError) -> Self {
        Self::new(Some(endpoint), Repr::Decode(error))
    }

    fn new(endpoint: Option<&'static EndpointMeta>, repr: Repr) -> Self {
        Self {
            inner: Box::new(Inner { endpoint, repr }),
        }
    }

    /// The stage at which the call failed.
    pub fn kind(&self) -> ErrorKind {
        match self.inner.repr {
            Repr::Build { .. } | Repr::Encode(_) => ErrorKind::Build,
            Repr::Transport(_) => ErrorKind::Transport,
            Repr::Status(_) => ErrorKind::Status,
            Repr::Decode(_) => ErrorKind::Decode,
        }
    }

    /// The endpoint's declared name, such as `GitHub.repo.issue`.
    pub fn endpoint(&self) -> Option<&'static str> {
        self.inner.endpoint.map(|m| m.name)
    }

    /// The response status, for [`ErrorKind::Status`].
    pub fn status(&self) -> Option<reqwest::StatusCode> {
        match &self.inner.repr {
            Repr::Status(response) => Some(response.status()),
            _ => None,
        }
    }

    /// The unread non-2xx response, for [`ErrorKind::Status`]. Concord never reads or prints its
    /// body; use it to decode an API error document.
    pub fn into_response(self) -> Option<reqwest::Response> {
        match self.inner.repr {
            Repr::Status(response) => Some(response),
            _ => None,
        }
    }

    /// The underlying reqwest error (with its URL removed), for [`ErrorKind::Transport`].
    ///
    /// **Not redacted below the top level**, which is why it is not this error's `source()`: its
    /// own chain comes from reqwest, hyper, TLS and DNS layers that Concord does not control and
    /// can contain URLs (a redirect error's source holds the redirect target, query included) and
    /// host names (a certificate mismatch), including sensitive values.
    pub fn transport(&self) -> Option<&reqwest::Error> {
        match &self.inner.repr {
            Repr::Transport(error) => Some(error),
            _ => None,
        }
    }

    /// The underlying encoder/decoder error, if the representation supplied one.
    ///
    /// Unlike everything else on this type, it **may quote body content** (serde_json does), so it
    /// is not exposed through [`std::error::Error::source`].
    pub fn body_source(&self) -> Option<&(dyn StdError + Send + Sync + 'static)> {
        match &self.inner.repr {
            Repr::Encode(e) | Repr::Decode(e) => e.source_ref(),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(meta) = self.inner.endpoint {
            write!(f, "{} {}: ", meta.method, meta.name)?;
        }
        match &self.inner.repr {
            Repr::Build { what, reason } => write!(f, "request not built: {what} {reason}"),
            Repr::Encode(e) => write!(f, "request not built: body: {}", e.description()),
            Repr::Transport(e) => {
                f.write_str("transport error")?;
                let class = if e.is_timeout() {
                    Some("timeout")
                } else if e.is_connect() {
                    Some("connect")
                } else if e.is_body() || e.is_decode() {
                    Some("reading the response body")
                } else if e.is_redirect() {
                    Some("redirect")
                } else {
                    None
                };
                match class {
                    Some(class) => write!(f, " ({class})"),
                    None => Ok(()),
                }
            }
            Repr::Status(response) => {
                let status = response.status();
                write!(f, "HTTP status {}", status.as_u16())?;
                match status.canonical_reason() {
                    Some(reason) => write!(f, " {reason}"),
                    None => Ok(()),
                }
            }
            Repr::Decode(e) => write!(f, "response not decoded: {}", e.description()),
        }
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("Error");
        s.field("kind", &self.kind());
        s.field("endpoint", &self.endpoint());
        if let Some(status) = self.status() {
            s.field("status", &status.as_u16());
        }
        s.field("message", &format_args!("{self}"));
        s.finish()
    }
}

/// Concord errors expose no `source()` chain. Underlying errors are reachable only through the
/// explicit accessors [`Error::transport`] and [`Error::body_source`], because their text is not
/// under Concord's control: reqwest nests errors that keep URLs (a refused redirect keeps its
/// target, which a server may build from any request value, headers included), TLS errors print
/// host names and decoders quote response bodies.
impl StdError for Error {}
