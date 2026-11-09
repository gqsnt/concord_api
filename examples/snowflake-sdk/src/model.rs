//! SQL API v2 documents; SQL values remain strings or nulls.
use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A validated UUID-shaped identifier. No generation or implicit retry is performed.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RequestId(String);
impl RequestId {
    /// Accepts the documented 8-4-4-4-12 hexadecimal UUID spelling.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if !uuid_shape(&value) {
            return Err(Error::Config("requestId must be a UUID"));
        }
        Ok(Self(value))
    }
    /// The exact request ID supplied by the caller.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for RequestId {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Self::new(value)
    }
}
impl From<RequestId> for String {
    fn from(value: RequestId) -> Self {
        value.0
    }
}
impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
fn uuid_shape(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Server statement identity; never interpreted as a URL or SQL.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct StatementHandle(String);
impl StatementHandle {
    /// Parses a server statement UUID.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if !uuid_shape(&value) {
            return Err(Error::Config("statementHandle must be a UUID"));
        }
        Ok(Self(value))
    }
    /// Exact server identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for StatementHandle {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Self::new(value)
    }
}
impl From<StatementHandle> for String {
    fn from(value: StatementHandle) -> Self {
        value.0
    }
}
impl std::fmt::Display for StatementHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Immutable correlation for a submission and all work derived from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestContext {
    /// Stable ID of the original POST. GETs omit optional requestId; cancel takes its own ID.
    pub submission_id: RequestId,
}

/// Case-sensitive execution defaults. Omitted fields use Snowflake user defaults.
#[derive(Clone, Debug, Default, Serialize)]
pub struct SessionContext {
    /// Database name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// Schema name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Warehouse name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warehouse: Option<String>,
    /// Role name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// One statement. SQL semantics and multi-statement execution are outside this SDK.
#[derive(Clone, Serialize)]
pub struct Statement {
    /// SQL supplied verbatim, with JSON escaping supplied by serde.
    pub statement: String,
    /// Server execution timeout in seconds, distinct from HTTP/poll timeouts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
    /// Execution context copied into the submission body.
    #[serde(flatten)]
    pub session: SessionContext,
}
impl Statement {
    /// Creates a single statement using server session defaults.
    pub fn new(sql: impl Into<String>) -> Self {
        Self {
            statement: sql.into(),
            timeout: None,
            session: SessionContext::default(),
        }
    }
}

/// One row. NULL is distinct from the literal string "null".
pub type Row = Vec<Option<String>>;

/// Server message used for pending, cancellation and error documents.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(concord::Debug)]
#[sensitive]
pub struct ServerMessage {
    /// Provider-specific code (kept as a string, including leading zeroes).
    #[public]
    pub code: String,
    /// Provider text; inspect deliberately, as it may echo SQL.
    pub message: String,
    /// SQLSTATE, when provided.
    #[public]
    pub sql_state: Option<String>,
    /// Server identity, when provided (generic HTTP errors may omit it).
    pub statement_handle: Option<StatementHandle>,
    /// Exact server status link. Correlated observations update the Query's next status target.
    pub statement_status_url: Option<String>,
    /// Additional server fields, without loss.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// A server-determined partition descriptor. Array position is its request index.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionInfo {
    /// Exact number of rows in this partition.
    pub row_count: u64,
    /// Server-reported uncompressed size; not used as a local body limit.
    pub uncompressed_size: u64,
    /// Server compressed size if supplied.
    pub compressed_size: Option<u64>,
    /// Unrecognized descriptor fields retained verbatim.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Metadata retained from the first successful response for every later partition.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultMetadata {
    /// Total rows across the descriptors.
    pub num_rows: u64,
    /// Supported format is jsonv2.
    pub format: String,
    /// Column descriptions retained without imposing SQL type conversions.
    pub row_type: Vec<serde_json::Value>,
    /// Ordered descriptors, including index zero already received. An absent member reads as
    /// empty; discovery rejects both as [`ProtocolError::InvalidManifest`](crate::ProtocolError::InvalidManifest).
    #[serde(default)]
    pub partition_info: Vec<PartitionInfo>,
    /// Additional metadata retained verbatim.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// First result document. Later partitions need only their data array.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultSet {
    /// Result status fields and any extensions.
    #[serde(flatten)]
    pub status: ServerMessage,
    /// Complete partition and column metadata.
    pub result_set_meta_data: ResultMetadata,
    /// Rows of partition zero, reused without a GET for partition zero.
    pub data: Vec<Row>,
}
impl std::fmt::Debug for ResultSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResultSet")
            .field("rows", &self.data.len())
            .finish_non_exhaustive()
    }
}

/// Metadata-free partition wire document. An envelope may echo identity; it is checked if present.
#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum PartitionBody {
    Envelope {
        data: Vec<Row>,
        #[serde(rename = "statementHandle")]
        statement_handle: Option<StatementHandle>,
    },
    Rows(Vec<Row>),
}
