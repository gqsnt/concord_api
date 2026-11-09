//! A one-line redacted view of a call: [`Summary`].

use std::fmt;

use reqwest::Url;

use crate::__private::{EndpointMeta, redacted_url};
use crate::error::ErrorKind;
use crate::response::Operation;

/// One line for a log: `OPERATION: METHOD TARGET`, then ` -> STATUS`, ` -> KIND` or
/// ` -> STATUS (KIND)` once known. Formatting it reads and sends nothing.
///
/// The target of a [`Prepared`](crate::Prepared) is its URL rendered like its `Debug` (sensitive
/// placements `<redacted>`, the query of a `query(..)` endpoint `?<withheld>`; ordinary values
/// print). Every other view — a [`Decoder`](crate::Decoder) after `into_parts`, a
/// [`Received`](crate::Received), an [`Error`](crate::Error), a followed URL — prints
/// `<withheld>`. `Display` and `Debug` print the same text.
#[derive(Clone, Copy)]
pub struct Summary<'a> {
    operation: Option<Operation>,
    method: &'a str,
    url: Option<(&'a Url, &'static EndpointMeta)>,
    status: Option<u16>,
    kind: Option<ErrorKind>,
}

impl<'a> Summary<'a> {
    /// A request built by an endpoint: its URL with the endpoint's redaction.
    pub(crate) fn request(
        operation: Option<Operation>,
        method: &'a str,
        url: Option<(&'a Url, &'static EndpointMeta)>,
    ) -> Self {
        Self {
            operation,
            method,
            url,
            status: None,
            kind: None,
        }
    }

    /// The declared operation only, target withheld.
    pub(crate) fn operation(operation: Option<Operation>) -> Self {
        Self::request(
            operation,
            operation.and_then(|op| op.method()).unwrap_or("HTTP"),
            None,
        )
    }

    pub(crate) fn with_status(mut self, status: Option<u16>) -> Self {
        self.status = status;
        self
    }

    pub(crate) fn with_kind(mut self, kind: ErrorKind) -> Self {
        self.kind = Some(kind);
        self
    }
}

fn kind_text(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::Build => "build",
        ErrorKind::Transport => "transport",
        ErrorKind::Status => "status",
        ErrorKind::Metadata => "metadata",
        ErrorKind::Decode => "decode",
        ErrorKind::Limit => "limit",
    }
}

impl fmt::Display for Summary<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self.operation.map_or("unnamed", |op| op.name());
        write!(f, "{name}: {} ", self.method)?;
        match self.url {
            Some((url, meta)) => f.write_str(&redacted_url(url, meta))?,
            None => f.write_str("<withheld>")?,
        }
        match (self.status, self.kind.map(kind_text)) {
            (Some(status), Some(kind)) => write!(f, " -> {status} ({kind})"),
            (Some(status), None) => write!(f, " -> {status}"),
            (None, Some(kind)) => write!(f, " -> {kind}"),
            (None, None) => Ok(()),
        }
    }
}

impl fmt::Debug for Summary<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
