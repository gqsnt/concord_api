use concord::{
    CodecError, Decode, Encoded, ResponseHead,
    bytes::Bytes,
    csv::{CsvOptions, CsvWriter, HeaderMode, StreamRows},
    jsonl::JsonLines,
    stream::{Completion, Upload},
};
use concord_pagination::{Page, Pages, Resume};
use futures_util::Stream;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

// The requested feature set does not enable concord::Json. This small format keeps
// the same head-before-body boundary for the two finite JSON responses.
pub struct LedgerJson;
impl<T: DeserializeOwned> Decode<T> for LedgerJson {
    const ACCEPT: Option<&'static str> = Some("application/json");
    type Context = ();
    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        match head.content_type() {
            concord::ContentTypeField::One(value)
                if value
                    .to_str()
                    .ok()
                    .and_then(|v| v.split(';').next())
                    .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json")) =>
            {
                Ok(())
            }
            _ => Err(CodecError::new("expected application/json")),
        }
    }
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body(_: (), body: Bytes) -> Result<T, CodecError> {
        serde_json::from_slice(&body).map_err(|e| CodecError::new("invalid JSON").with_source(e))
    }
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Summary {
    pub balance: i64,
}
#[derive(Debug, Deserialize, PartialEq)]
pub struct Problem {
    pub code: String,
    pub message: String,
}
#[derive(Debug, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub cents: i64,
}
#[derive(Debug, Serialize)]
pub struct Row {
    pub id: u64,
    pub amount: i64,
}
#[derive(Debug, Serialize)]
pub struct ArchiveRow {
    pub id: u64,
    pub amount: String,
}

fn dollars(cents: i64) -> String {
    let magnitude = cents.unsigned_abs();
    format!(
        "{}{}.{:02}",
        if cents < 0 { "-" } else { "" },
        magnitude / 100,
        magnitude % 100
    )
}
#[derive(Debug, Deserialize, PartialEq)]
pub struct Invoice {
    pub id: u64,
}
#[derive(Debug, Deserialize)]
pub struct InvoicePage {
    data: Vec<Invoice>,
    next_cursor: Option<String>,
}

concord::api! {
    pub client Ledger {
        base "http://localhost";
        GET summary(id: String) -> Summary { path "accounts/{id}/summary"; response LedgerJson; }
        POST upload_rows(id: String, body rows: Upload) { path "jobs/{id}/rows"; request Encoded; }
        GET entries() -> stream Entry { path "entries"; response JsonLines; }
        POST archive(body rows: Upload) { path "archive"; request Encoded; }
        GET invoices(query cursor?: String) -> InvoicePage { path "invoices"; response LedgerJson; }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostics {
    pub request_id: Option<String>,
    pub remaining: Option<String>,
}
#[derive(Debug)]
pub enum SummaryFailure {
    Concord(concord::Error),
    Service(Problem),
    InvalidServiceBody(String),
}
#[derive(Debug)]
pub struct SummaryError {
    pub diagnostics: Option<Diagnostics>,
    pub failure: SummaryFailure,
}

pub async fn summary(api: &Ledger, id: &str) -> Result<(Summary, Diagnostics), SummaryError> {
    let prepared = api.summary(id).prepare().map_err(|e| SummaryError {
        diagnostics: None,
        failure: SummaryFailure::Concord(e),
    })?;
    let received = prepared.receive().await.map_err(|e| SummaryError {
        diagnostics: None,
        failure: SummaryFailure::Concord(e),
    })?;
    let h = received.head();
    let get = |name| {
        h.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let diagnostics = Diagnostics {
        request_id: get("x-ledger-request-id"),
        remaining: get("x-ratelimit-remaining"),
    };
    if !received.status().is_success() {
        let response = received.into_response();
        let body = response.bytes().await.map_err(|e| SummaryError {
            diagnostics: Some(diagnostics.clone()),
            failure: SummaryFailure::InvalidServiceBody(e.to_string()),
        })?;
        let problem = serde_json::from_slice(&body).map_err(|e| SummaryError {
            diagnostics: Some(diagnostics.clone()),
            failure: SummaryFailure::InvalidServiceBody(e.to_string()),
        })?;
        return Err(SummaryError {
            diagnostics: Some(diagnostics),
            failure: SummaryFailure::Service(problem),
        });
    }
    received
        .decode()
        .await
        .map(|value| (value, diagnostics.clone()))
        .map_err(|e| SummaryError {
            diagnostics: Some(diagnostics),
            failure: SummaryFailure::Concord(e),
        })
}

#[derive(Clone, Copy)]
pub struct Dialect {
    pub header: bool,
    pub delimiter: u8,
}
pub fn csv_upload<S, T, E>(
    dialect: Dialect,
    rows: S,
) -> Result<Upload, Box<dyn std::error::Error + Send + Sync>>
where
    S: Stream<Item = Result<T, E>> + Send + 'static,
    T: Serialize + 'static,
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let mode = if dialect.header {
        HeaderMode::Present
    } else {
        HeaderMode::Absent
    };
    let writer = CsvWriter::new(CsvOptions::new(mode, dialect.delimiter))?;
    Ok(writer.upload(StreamRows::new(rows).with_columns(["id", "amount"]))?)
}

#[derive(Debug, Clone)]
pub struct ArchiveStatus {
    pub completion: Completion,
    pub download_error: Option<String>,
}
#[derive(Debug)]
pub struct ArchiveError {
    pub status: ArchiveStatus,
    pub message: String,
}

pub async fn archive(api: &Ledger) -> Result<ArchiveStatus, ArchiveError> {
    let state = Arc::new(Mutex::new(ArchiveStatus {
        completion: Completion::Reading,
        download_error: None,
    }));
    let reader = api.entries().send().await.map_err(|e| ArchiveError {
        status: state.lock().unwrap().clone(),
        message: e.to_string(),
    })?;
    let observer = state.clone();
    let source = futures_util::stream::unfold(reader, move |mut reader| {
        let observer = observer.clone();
        async move {
            match reader.next().await {
                Some(Ok(entry)) => Some((
                    Ok(ArchiveRow {
                        id: entry.id,
                        amount: dollars(entry.cents),
                    }),
                    reader,
                )),
                Some(Err(e)) => {
                    let mut status = observer.lock().unwrap();
                    status.completion = reader.completion();
                    status.download_error = Some(e.to_string());
                    drop(status);
                    Some((Err(e), reader))
                }
                None => {
                    observer.lock().unwrap().completion = reader.completion();
                    None
                }
            }
        }
    });
    let upload = csv_upload(
        Dialect {
            header: true,
            delimiter: b',',
        },
        source,
    )
    .map_err(|e| ArchiveError {
        status: state.lock().unwrap().clone(),
        message: e.to_string(),
    })?;
    let result = api.archive(upload).send().await;
    let status = state.lock().unwrap().clone();
    if let Some(message) = status.download_error.clone() {
        return Err(ArchiveError { status, message });
    }
    if let Err(e) = result {
        return Err(ArchiveError {
            status,
            message: e.to_string(),
        });
    }
    Ok(status)
}

pub fn all_invoices(
    api: Ledger,
    max_pages: Option<NonZeroUsize>,
) -> Pages<Invoice, String, (), concord::Error> {
    invoices_from(api, Resume::Start, max_pages)
}

pub fn invoices_from(
    api: Ledger,
    from: Resume<String>,
    max_pages: Option<NonZeroUsize>,
) -> Pages<Invoice, String, (), concord::Error> {
    let pages = Pages::new(from, move |cursor: Option<String>| {
        let api = api.clone();
        async move {
            let page = api.invoices().maybe_cursor(cursor).send().await?;
            Ok(Page::new(page.data, page.next_cursor))
        }
    });
    match max_pages {
        Some(cap) => pages.max_pages(cap),
        None => pages,
    }
}
