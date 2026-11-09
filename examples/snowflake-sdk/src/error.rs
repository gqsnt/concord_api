//! Typed errors retain response evidence without printing provider payloads.
use crate::ServerMessage;
use concord::ResponseMeta;

/// Why a response cannot safely be used as a statement result or partition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    /// A response omitted or changed the statement handle.
    HandleMismatch,
    /// A returned status/partition link is malformed, ambiguous or not for this query/account.
    InvalidLink,
    /// The status is not documented for this operation.
    UnexpectedStatus,
    /// Metadata is absent, inconsistent, unsupported or not partition zero, including an absent
    /// or empty `partitionInfo`. A 200 body that does not decode as a result document (invalid
    /// JSON, or a missing required member such as `resultSetMetaData`, `format` or `data`) is
    /// instead [`Error::Concord`] with a decode error.
    InvalidManifest,
    /// A complete first partition is not present (including code 391908).
    IncompleteFirstPartition,
    /// Partition row count or row width differs from the first response's metadata.
    InvalidRows,
    /// Partition index is outside the plan or zero (already available).
    InvalidPartition,
    /// The cancel endpoint did not return the documented cancellation status.
    UnexpectedCancellation,
}

/// A non-success HTTP response, including failures whose body could not be decoded.
pub struct ServerError {
    /// Original status, headers and generated operation identity.
    pub meta: ResponseMeta,
    /// Typed document, when valid.
    pub document: Option<ServerMessage>,
    /// Body bytes if fully read within the configured limit. May contain SQL.
    pub body: concord::bytes::Bytes,
    /// Read/limit error, if the body could not be fully read.
    pub read_error: Option<concord::Error>,
    /// JSON diagnostic, if a bounded body was not a valid server message.
    pub decode_error: Option<serde_json::Error>,
}
impl ServerError {
    /// Cancellation is recognized by the documented code AND SQLSTATE, never HTTP alone.
    pub fn is_cancelled(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|d| d.code == "000604" && d.sql_state.as_deref() == Some("57014"))
    }
}
impl std::fmt::Debug for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerError")
            .field("meta", &self.meta)
            .field("document", &self.document)
            .field("body_bytes", &self.body.len())
            .field("read_failed", &self.read_error.is_some())
            .field("decode_failed", &self.decode_error.is_some())
            .finish()
    }
}

/// A partition failed after its bounded wire bytes were received.
#[derive(Debug, concord::derive::Error)]
#[display(with = display_partition_body_error)]
pub enum PartitionBodyError {
    /// Inflated output exceeded the configured maximum. At most limit + 1 bytes are collected.
    Limit {
        /// Configured maximum uncompressed bytes.
        limit: usize,
    },
    /// Invalid/truncated gzip, bad checksum, or invalid trailing data.
    #[from(skip)]
    Gzip(std::io::Error),
    /// The complete uncompressed bytes were not a valid partition JSON document.
    #[from(skip)]
    Json(serde_json::Error),
}
fn display_partition_body_error(
    value: &PartitionBodyError,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    f.write_str(match value {
        PartitionBodyError::Limit { .. } => "inflated partition exceeds byte limit",
        PartitionBodyError::Gzip(_) => "invalid gzip partition",
        PartitionBodyError::Json(_) => "invalid partition JSON",
    })
}

/// SDK errors distinguish provider rejection, protocol validation and transport/codec failures.
#[derive(Debug, concord::derive::Error)]
#[display(with = display_error)]
pub enum Error {
    /// Invalid local configuration; no request was sent.
    #[from(skip)]
    Config(&'static str),
    /// Concord build, HTTP, body-limit or successful-body decode error.
    Concord {
        /// Response metadata when headers arrived.
        meta: Option<Box<ResponseMeta>>,
        /// Original operation-aware Concord diagnostic.
        source: concord::Error,
    },
    /// SDK-owned gzip or JSON decoding failed; HTTP execution was performed by Concord.
    PartitionBody {
        /// Original wire status, headers and generated operation identity.
        meta: Box<ResponseMeta>,
        /// Typed limit, gzip or JSON diagnostic; payloads are not printed by Display.
        source: PartitionBodyError,
    },
    /// A non-success status, with bounded error body and metadata.
    #[from(skip)]
    Server(Box<ServerError>),
    /// Provider data did not meet the scoped protocol contract.
    Protocol {
        /// Structured reason.
        kind: ProtocolError,
        /// Response head when there was one.
        meta: Option<Box<ResponseMeta>>,
    },
}
impl Error {
    pub(crate) fn protocol(kind: ProtocolError, meta: Option<&ResponseMeta>) -> Self {
        Self::Protocol {
            kind,
            meta: meta.cloned().map(Box::new),
        }
    }
    /// Response metadata on every error that had received a head.
    pub fn response_meta(&self) -> Option<&ResponseMeta> {
        match self {
            Self::Concord { meta, .. } | Self::Protocol { meta, .. } => meta.as_deref(),
            Self::Server(e) => Some(&e.meta),
            Self::PartitionBody { meta, .. } => Some(meta),
            Self::Config(_) => None,
        }
    }
}
impl From<concord::Error> for Error {
    fn from(source: concord::Error) -> Self {
        let meta = source.head_meta().map(Box::new);
        Self::Concord { meta, source }
    }
}
fn display_error(value: &Error, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match value {
        Error::Config(s) => write!(f, "Snowflake configuration: {s}"),
        Error::Concord { source, .. } => std::fmt::Display::fmt(source, f),
        Error::PartitionBody { source, .. } => write!(f, "Snowflake partition: {source}"),
        Error::Server(e) => write!(f, "Snowflake HTTP {}", e.meta.status()),
        Error::Protocol { kind, .. } => write!(f, "Snowflake protocol: {kind:?}"),
    }
}

/// Snowflake's server-message parse outcome, preserving the SDK's concrete JSON diagnostic.
pub type ErrorDocument = Result<ServerMessage, serde_json::Error>;
/// The SQL API error document is JSON regardless of its advertised media type.
pub struct ServerErrorJson;
impl concord::Decode<ErrorDocument> for ServerErrorJson {
    type Context = ();
    fn context(_: &concord::ResponseHead<'_>) -> Result<(), concord::CodecError> {
        Ok(())
    }
    fn decode_body(
        (): (),
        body: concord::bytes::Bytes,
    ) -> Result<ErrorDocument, concord::CodecError> {
        Ok(serde_json::from_slice(&body))
    }
}
