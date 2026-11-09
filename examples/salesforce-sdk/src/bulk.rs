//! Bulk API 2.0 documents: job creation bodies, job info, the CSV dialect enums, state changes
//! and the parallel result-pages manifest.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use crate::ids::{ApiVersion, IngestJobId, QueryJobId, SObjectName};
use crate::model::DynamicFields;
use crate::soql::Soql;

/// `columnDelimiter`: the six delimiters Bulk API 2.0 supports. The default is `COMMA`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ColumnDelimiter {
    /// `` ` ``
    Backquote,
    /// `^`
    Caret,
    /// `,`
    #[default]
    Comma,
    /// `|`
    Pipe,
    /// `;`
    Semicolon,
    /// tab
    Tab,
}

impl ColumnDelimiter {
    /// The delimiter byte.
    pub fn byte(self) -> u8 {
        match self {
            ColumnDelimiter::Backquote => b'`',
            ColumnDelimiter::Caret => b'^',
            ColumnDelimiter::Comma => b',',
            ColumnDelimiter::Pipe => b'|',
            ColumnDelimiter::Semicolon => b';',
            ColumnDelimiter::Tab => b'\t',
        }
    }

    /// Every supported delimiter.
    pub const ALL: [ColumnDelimiter; 6] = [
        ColumnDelimiter::Backquote,
        ColumnDelimiter::Caret,
        ColumnDelimiter::Comma,
        ColumnDelimiter::Pipe,
        ColumnDelimiter::Semicolon,
        ColumnDelimiter::Tab,
    ];
}

/// `lineEnding`: `LF` (the default) or `CRLF`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LineEnding {
    /// `\n`
    #[default]
    Lf,
    /// `\r\n`
    Crlf,
}

/// The CSV dialect of one Bulk API 2.0 job: chosen when the job is created, echoed by every job
/// document, and used for every CSV body of that job (results, uploads, result files).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct BulkDialect {
    /// `columnDelimiter`.
    pub delimiter: ColumnDelimiter,
    /// `lineEnding`.
    pub line_ending: LineEnding,
}

impl BulkDialect {
    /// A dialect.
    pub const fn new(delimiter: ColumnDelimiter, line_ending: LineEnding) -> Self {
        Self {
            delimiter,
            line_ending,
        }
    }
}

/// `contentType`: only `CSV` exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum JobContentType {
    /// `CSV`
    #[default]
    #[serde(rename = "CSV")]
    Csv,
}

/// A job's `state`. Unknown future states are kept as [`JobState::Other`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum JobState {
    /// `Open` (ingest: data can be uploaded).
    Open,
    /// `UploadComplete` (queued for processing).
    UploadComplete,
    /// `InProgress`.
    InProgress,
    /// `JobComplete`.
    JobComplete,
    /// `Failed`.
    Failed,
    /// `Aborted`.
    Aborted,
    /// A state this SDK does not know.
    Other(String),
}

impl JobState {
    /// Whether the job can no longer change state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobState::JobComplete | JobState::Failed | JobState::Aborted
        )
    }

    /// The wire text.
    pub fn as_str(&self) -> &str {
        match self {
            JobState::Open => "Open",
            JobState::UploadComplete => "UploadComplete",
            JobState::InProgress => "InProgress",
            JobState::JobComplete => "JobComplete",
            JobState::Failed => "Failed",
            JobState::Aborted => "Aborted",
            JobState::Other(s) => s,
        }
    }
}

impl<'de> Deserialize<'de> for JobState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(match text.as_str() {
            "Open" => JobState::Open,
            "UploadComplete" => JobState::UploadComplete,
            "InProgress" => JobState::InProgress,
            "JobComplete" => JobState::JobComplete,
            "Failed" => JobState::Failed,
            "Aborted" => JobState::Aborted,
            _ => JobState::Other(text),
        })
    }
}

/// The body of `PATCH jobs/query/{id}` and `PATCH jobs/ingest/{id}`: `{"state": ..}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct JobStateChange {
    state: RequestedState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
enum RequestedState {
    UploadComplete,
    Aborted,
}

impl JobStateChange {
    /// `{"state": "UploadComplete"}`: closes an ingest job's upload.
    pub const UPLOAD_COMPLETE: JobStateChange = JobStateChange {
        state: RequestedState::UploadComplete,
    };
    /// `{"state": "Aborted"}`: aborts a job.
    pub const ABORTED: JobStateChange = JobStateChange {
        state: RequestedState::Aborted,
    };
}

/// `operation` of a query job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QueryOperation {
    /// `query`: undeleted, unarchived records.
    #[default]
    Query,
    /// `queryAll`: also deleted/archived records.
    QueryAll,
}

/// The body of `POST jobs/query`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateQueryJob {
    operation: QueryOperation,
    query: String,
    content_type: JobContentType,
    column_delimiter: ColumnDelimiter,
    line_ending: LineEnding,
}

impl CreateQueryJob {
    /// A `query` job for `soql` in `dialect`; the dialect is always sent explicitly, never left
    /// to the server's default.
    pub fn new(soql: &Soql, dialect: BulkDialect) -> Self {
        Self {
            operation: QueryOperation::Query,
            query: soql.as_str().to_owned(),
            content_type: JobContentType::Csv,
            column_delimiter: dialect.delimiter,
            line_ending: dialect.line_ending,
        }
    }

    /// `queryAll` instead of `query`.
    #[must_use]
    pub fn query_all(mut self) -> Self {
        self.operation = QueryOperation::QueryAll;
        self
    }

    /// The dialect requested.
    pub fn dialect(&self) -> BulkDialect {
        BulkDialect::new(self.column_delimiter, self.line_ending)
    }
}

impl fmt::Debug for CreateQueryJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CreateQueryJob")
            .field("operation", &self.operation)
            .field("dialect", &self.dialect())
            .finish_non_exhaustive()
    }
}

/// A query job document (create, get, abort).
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(concord::Debug)]
#[sensitive]
pub struct QueryJobInfo {
    /// `id`.
    #[public]
    pub id: QueryJobId,
    /// `operation`.
    pub operation: QueryOperation,
    /// `object`.
    #[serde(default)]
    pub object: Option<String>,
    /// `state`.
    #[public]
    pub state: JobState,
    /// `apiVersion` (number or string on the wire).
    #[public]
    pub api_version: ApiVersion,
    /// `contentType`.
    #[serde(default)]
    pub content_type: Option<JobContentType>,
    /// `columnDelimiter`; documents that omit it (the abort example) leave it `None`.
    #[serde(default)]
    #[public]
    pub column_delimiter: Option<ColumnDelimiter>,
    /// `lineEnding`; `None` when omitted.
    #[serde(default)]
    #[public]
    pub line_ending: Option<LineEnding>,
    /// `numberRecordsProcessed`.
    #[serde(default)]
    #[public]
    pub number_records_processed: Option<u64>,
    /// `retries`.
    #[serde(default)]
    pub retries: Option<u32>,
    /// `totalProcessingTime` (ms).
    #[serde(default)]
    pub total_processing_time: Option<u64>,
    /// `jobType` (`V2Query`).
    #[serde(default)]
    pub job_type: Option<String>,
    /// `errorMessage` (failed jobs).
    #[serde(default)]
    error_message: Option<String>,
    /// Other properties (createdById, createdDate, systemModstamp, concurrencyMode,
    /// isPkChunkingSupported, ...).
    #[serde(flatten)]
    pub extra: DynamicFields,
}

impl QueryJobInfo {
    /// The job's `errorMessage`, **unredacted** (it can quote query text or values).
    pub fn error_message_unredacted(&self) -> Option<&str> {
        self.error_message.as_deref()
    }
}

/// `operation` of an ingest job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IngestOperation {
    /// `insert`
    Insert,
    /// `update`
    Update,
    /// `upsert` (needs an external id field)
    Upsert,
    /// `delete`
    Delete,
    /// `hardDelete`
    HardDelete,
}

/// The body of `POST jobs/ingest`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIngestJob {
    object: SObjectName,
    operation: IngestOperation,
    content_type: JobContentType,
    column_delimiter: ColumnDelimiter,
    line_ending: LineEnding,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_id_field_name: Option<String>,
}

impl CreateIngestJob {
    /// An ingest job for `object` in `dialect` (sent explicitly).
    pub fn new(object: SObjectName, operation: IngestOperation, dialect: BulkDialect) -> Self {
        Self {
            object,
            operation,
            content_type: JobContentType::Csv,
            column_delimiter: dialect.delimiter,
            line_ending: dialect.line_ending,
            external_id_field_name: None,
        }
    }

    /// The external id field of an upsert.
    #[must_use]
    pub fn external_id(mut self, field: crate::ids::FieldName) -> Self {
        self.external_id_field_name = Some(field.as_str().to_owned());
        self
    }

    /// The dialect requested.
    pub fn dialect(&self) -> BulkDialect {
        BulkDialect::new(self.column_delimiter, self.line_ending)
    }
}

/// An ingest job document (create, get, close, abort).
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(concord::Debug)]
#[sensitive]
pub struct IngestJobInfo {
    /// `id`.
    #[public]
    pub id: IngestJobId,
    /// `operation`.
    pub operation: IngestOperation,
    /// `object`.
    #[serde(default)]
    pub object: Option<String>,
    /// `state`.
    #[public]
    pub state: JobState,
    /// `apiVersion`.
    #[public]
    pub api_version: ApiVersion,
    /// `columnDelimiter`.
    #[serde(default)]
    #[public]
    pub column_delimiter: Option<ColumnDelimiter>,
    /// `lineEnding`.
    #[serde(default)]
    #[public]
    pub line_ending: Option<LineEnding>,
    /// `contentUrl`: the upload URL while the job is `Open` (a returned link; the documented
    /// example is relative without a leading slash: `services/data/v68.0/jobs/ingest/{id}/batches`).
    #[serde(default)]
    pub content_url: Option<String>,
    /// `numberRecordsProcessed`.
    #[serde(default)]
    #[public]
    pub number_records_processed: Option<u64>,
    /// `numberRecordsFailed`.
    #[serde(default)]
    #[public]
    pub number_records_failed: Option<u64>,
    /// `errorMessage`.
    #[serde(default)]
    error_message: Option<String>,
    /// Other properties.
    #[serde(flatten)]
    pub extra: DynamicFields,
}

impl IngestJobInfo {
    /// The job's `errorMessage`, **unredacted**.
    pub fn error_message_unredacted(&self) -> Option<&str> {
        self.error_message.as_deref()
    }
}

/// One entry of `resultPages`.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultPageEntry {
    /// `resultUrl`: `/services/data/vXX.X/jobs/query/{id}/results?locator=..` (a returned link).
    pub result_url: String,
}

/// The parallel result manifest (`GET jobs/query/{id}/resultPages`): up to five result URLs and
/// the next manifest's URL.
///
/// The documentation's field table names the next link `nextRecordUrl`, while its example (and
/// prose) use `nextRecordsUrl`; both spellings are read, and [`next_link`](Self::next_link)
/// decides (see `PROVIDER.md`, decision SF-D1).
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultPagesResponse {
    /// `resultPages`.
    #[serde(default)]
    pub result_pages: Vec<ResultPageEntry>,
    /// `nextRecordsUrl` (the example's spelling).
    #[serde(default)]
    pub next_records_url: Option<String>,
    /// `nextRecordUrl` (the field table's spelling).
    #[serde(default)]
    pub next_record_url: Option<String>,
    /// `done`.
    pub done: bool,
}

/// How the two spellings of the next manifest link resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NextLink<'a> {
    /// Neither spelling is present (or both are `null`).
    Absent,
    /// One link (from either spelling, or both spellings with the same text).
    Present(&'a str),
    /// Both spellings are present with different text: refused, never guessed.
    Ambiguous,
}

impl ResultPagesResponse {
    /// The next manifest link under the alias policy: either spelling is accepted; both with the
    /// same value are one link; both with different values are [`NextLink::Ambiguous`].
    pub fn next_link(&self) -> NextLink<'_> {
        match (
            self.next_records_url.as_deref(),
            self.next_record_url.as_deref(),
        ) {
            (None, None) => NextLink::Absent,
            (Some(a), None) | (None, Some(a)) => NextLink::Present(a),
            (Some(a), Some(b)) if a == b => NextLink::Present(a),
            (Some(_), Some(_)) => NextLink::Ambiguous,
        }
    }
}

impl fmt::Debug for ResultPagesResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResultPagesResponse")
            .field("result_pages", &self.result_pages.len())
            .field("next", &!matches!(self.next_link(), NextLink::Absent))
            .field("done", &self.done)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_job_example_parses() {
        // The "Get Information About a Query Job" example (apiVersion as a JSON number).
        let job: QueryJobInfo = serde_json::from_str(
            r#"{"id":"750R0000000zlh9IAA","operation":"query","object":"Account",
            "createdById":"005R0000000GiwjIAC","createdDate":"2018-12-10T17:50:19.000+0000",
            "systemModstamp":"2018-12-10T17:51:27.000+0000","state":"JobComplete",
            "concurrencyMode":"Parallel","contentType":"CSV","apiVersion":46.0,"jobType":"V2Query",
            "lineEnding":"LF","columnDelimiter":"COMMA","numberRecordsProcessed":500,"retries":0,
            "totalProcessingTime":334,"isPkChunkingSupported":true}"#,
        )
        .unwrap();
        assert_eq!(job.state, JobState::JobComplete);
        assert_eq!(job.api_version, ApiVersion::new(46, 0));
        assert_eq!(job.column_delimiter, Some(ColumnDelimiter::Comma));
        assert_eq!(job.line_ending, Some(LineEnding::Lf));
    }

    #[test]
    fn create_body_names_the_dialect() {
        let soql = Soql::select(
            SObjectName::new("Account").unwrap(),
            [crate::soql::FieldPath::parse("Id").unwrap()],
        )
        .build()
        .unwrap();
        let body = serde_json::to_value(CreateQueryJob::new(
            &soql,
            BulkDialect::new(ColumnDelimiter::Caret, LineEnding::Crlf),
        ))
        .unwrap();
        assert_eq!(
            body,
            serde_json::json!({"operation":"query","query":"SELECT Id FROM Account",
                "contentType":"CSV","columnDelimiter":"CARET","lineEnding":"CRLF"})
        );
        assert_eq!(
            serde_json::to_value(JobStateChange::UPLOAD_COMPLETE).unwrap(),
            serde_json::json!({"state":"UploadComplete"})
        );
    }

    #[test]
    fn next_link_alias_policy() {
        let parse = |s: &str| serde_json::from_str::<ResultPagesResponse>(s).unwrap();
        let a = parse(r#"{"resultPages":[],"nextRecordsUrl":"/a","done":false}"#);
        assert_eq!(a.next_link(), NextLink::Present("/a"));
        let b = parse(r#"{"resultPages":[],"nextRecordUrl":"/b","done":false}"#);
        assert_eq!(b.next_link(), NextLink::Present("/b"));
        let same = parse(r#"{"nextRecordsUrl":"/a","nextRecordUrl":"/a","done":false}"#);
        assert_eq!(same.next_link(), NextLink::Present("/a"));
        let both = parse(r#"{"nextRecordsUrl":"/a","nextRecordUrl":"/b","done":false}"#);
        assert_eq!(both.next_link(), NextLink::Ambiguous);
        let none = parse(r#"{"resultPages":[],"nextRecordsUrl":null,"done":true}"#);
        assert_eq!(none.next_link(), NextLink::Absent);
    }
}

#[cfg(test)]
mod sdk_type_canaries {
    use super::*;

    #[test]
    fn ingest_job_documents_withhold_provider_strings() {
        let mut c = concord_check_canary::Canaries::new("sf-ingest-info");
        let object = c.issue();
        let url = c.issue();
        let message = c.issue();
        let extra = c.issue();
        let info: IngestJobInfo = serde_json::from_value(serde_json::json!({
            "id": "750A00000000002AAA", "operation": "insert", "object": object,
            "state": "Open", "apiVersion": 68.0, "contentUrl": url,
            "errorMessage": message, "providerExtra": extra
        }))
        .unwrap();
        assert_eq!(info.object.as_deref(), Some(object.as_str()));
        assert_eq!(info.content_url.as_deref(), Some(url.as_str()));
        assert_eq!(info.error_message.as_deref(), Some(message.as_str()));
        c.assert_debug_absent("IngestJobInfo", &info);
    }
}
