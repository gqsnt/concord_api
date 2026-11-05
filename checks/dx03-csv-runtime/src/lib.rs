//! Client for a fictional reports and job exports service.

use std::{convert::Infallible, error::Error as StdError, fmt};

use concord::{
    csv::{
        Csv, CsvConfigError, CsvOptions, CsvReader, CsvRecords, CsvWriter, HasHeader, HeaderMode,
        ReadMapping, Semicolon, StreamRows,
    },
    reqwest::{self, StatusCode, header::HeaderValue},
    stream::{OpenError, from_iter},
};
use serde::{Deserialize, Serialize};

/// One report or export row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportRow {
    /// Row identifier.
    pub id: u64,
    /// Amount in minor currency units.
    pub amount: u64,
}

/// The fixed report dialect.
pub type ReportCsv = Csv<HasHeader, Semicolon>;

concord::api! {
    /// Reports and exports API.
    pub client Exports {
        base "https://exports.example.test";

        /// A fixed-dialect report.
        GET report(id: String) -> Vec<ReportRow> {
            path "reports/{id}";
            response ReportCsv;
        }
        /// Download a job export; opened with runtime options by `open_export`.
        GET download(id: String) { path "jobs/{id}/export"; }
        /// Upload a job export; the runtime writer fills the request body.
        POST upload(id: String) { path "jobs/{id}/export"; }
    }
}

/// Dialect and column order returned by the service for a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDescriptor {
    /// Whether the first record is a header.
    pub header: bool,
    /// A single ASCII delimiter byte, expressed as a string in JSON.
    pub delimiter: String,
    /// Column names in wire order.
    pub columns: Vec<String>,
}

impl JobDescriptor {
    fn options(&self) -> Result<CsvOptions, ExportError> {
        let bytes = self.delimiter.as_bytes();
        if bytes.len() != 1 || !bytes[0].is_ascii() {
            return Err(ExportError::Delimiter);
        }
        let mode = if self.header {
            HeaderMode::Present
        } else {
            HeaderMode::Absent
        };
        Ok(CsvOptions::new(mode, bytes[0]))
    }

    fn reader(&self) -> Result<CsvReader, ExportError> {
        let options = self.options()?;
        // Headerless named rows need the descriptor's mapping; with a header, validate it.
        let options = if self.header {
            options.columns(self.columns.clone())
        } else {
            options.read_mapping(ReadMapping::columns(self.columns.clone()))
        };
        CsvReader::new(options).map_err(ExportError::Config)
    }
}

/// Selected response metadata copied before the reader owns the response.
#[derive(Debug, Clone)]
pub struct ExportMetadata {
    /// HTTP status.
    pub status: StatusCode,
    /// Service export identifier, when supplied.
    pub export_id: Option<HeaderValue>,
    /// Service request identifier, when supplied.
    pub request_id: Option<HeaderValue>,
}

/// An open export. Call `rows.next().await` to consume one row at a time.
pub struct ExportDownload {
    /// The response head, retained after reading starts.
    pub metadata: ExportMetadata,
    /// Incremental rows and their typed read errors, including delivered-row count.
    pub rows: CsvRecords<ReportRow>,
}

/// A failure before an export reader is returned or while preparing/sending an upload.
#[derive(Debug)]
pub enum ExportError {
    /// The descriptor does not provide exactly one ASCII byte.
    Delimiter,
    /// Concord refused the runtime CSV configuration.
    Config(CsvConfigError),
    /// Concord could not build or send the request, or finish an upload response.
    Request(concord::Error),
    /// The response was refused before reading; it remains available in the error.
    Open(OpenError),
    /// The upload could not be constructed.
    Encode(concord::CodecError),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Delimiter => f.write_str("delimiter must be one ASCII byte"),
            Self::Config(e) => write!(f, "invalid CSV options: {e}"),
            Self::Request(e) => write!(f, "request failed: {e}"),
            Self::Open(e) => write!(f, "export refused: {e}"),
            Self::Encode(e) => write!(f, "upload encoding failed: {e}"),
        }
    }
}
impl StdError for ExportError {}

/// Open a job export without buffering the body. On refusal, `ExportError::Open`
/// retains the complete unread response. Subsequent errors are `ReadError`s from `rows.next()`.
pub async fn open_export(
    api: &Exports,
    id: &str,
    job: &JobDescriptor,
) -> Result<ExportDownload, ExportError> {
    let reader = job.reader()?;
    let response = api
        .download(id)
        .prepare()
        .map_err(ExportError::Request)?
        .send_raw()
        .await
        .map_err(ExportError::Request)?;
    let metadata = ExportMetadata {
        status: response.status(),
        export_id: response.headers().get("x-export-id").cloned(),
        request_id: response.headers().get("x-request-id").cloned(),
    };
    let rows = reader
        .open_for(response, "Exports.download")
        .map_err(ExportError::Open)?;
    Ok(ExportDownload { metadata, rows })
}

/// Upload one job's rows using its dialect and the matching runtime Content-Type.
pub async fn upload_export(
    api: &Exports,
    id: &str,
    job: &JobDescriptor,
    rows: Vec<ReportRow>,
) -> Result<(), ExportError> {
    let writer = CsvWriter::new(job.options()?).map_err(ExportError::Config)?;
    let source = from_iter(rows.into_iter().map(Ok::<_, Infallible>));
    let upload = writer
        .upload(StreamRows::new(source).with_columns(job.columns.clone()))
        .map_err(ExportError::Encode)?;
    let (client, mut request, decoder) = api
        .upload(id)
        .prepare()
        .map_err(ExportError::Request)?
        .into_parts();
    upload.apply(&mut request);
    decoder
        .finish(client.execute(request).await)
        .await
        .map_err(ExportError::Request)
}

/// Build a client using the supplied HTTP transport and service base URL.
pub fn client(http: reqwest::Client, base: &str) -> Result<Exports, concord::Error> {
    Exports::new(http).with_base_url(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord::{
        csv::Rows,
        stream::{Chunks, ReadErrorKind},
    };

    #[tokio::test]
    async fn runtime_reader_maps_columns_across_chunks_and_reports_progress() {
        let job = JobDescriptor {
            header: false,
            delimiter: "|".into(),
            columns: vec!["amount".into(), "id".into()],
        };
        let reader = job.reader().unwrap();
        let mut rows = reader
            .read::<ReportRow, _>(Chunks::new(["35|", "1\n90|2\n"]).then_fail("source failed"));
        assert_eq!(
            rows.next().await.unwrap().unwrap(),
            ReportRow { id: 1, amount: 35 }
        );
        assert_eq!(
            rows.next().await.unwrap().unwrap(),
            ReportRow { id: 2, amount: 90 }
        );
        let error = rows.next().await.unwrap().unwrap_err();
        assert_eq!(error.kind(), ReadErrorKind::Transport);
        assert_eq!(error.position().records, 2);
        assert!(rows.next().await.is_none());
    }

    #[test]
    fn runtime_writer_encodes_both_header_modes() {
        for (header, expected_type, expected_body) in [
            (false, "text/csv; header=absent", &b"1|35\r\n"[..]),
            (
                true,
                "text/csv; header=present",
                &b"id|amount\r\n1|35\r\n"[..],
            ),
        ] {
            let job = JobDescriptor {
                header,
                delimiter: "|".into(),
                columns: vec!["id".into(), "amount".into()],
            };
            let writer = CsvWriter::new(job.options().unwrap()).unwrap();
            assert_eq!(writer.content_type(), expected_type);
            assert_eq!(
                writer
                    .to_vec(Rows::new([ReportRow { id: 1, amount: 35 }]).with_columns(job.columns))
                    .unwrap(),
                expected_body
            );
        }
    }
}
