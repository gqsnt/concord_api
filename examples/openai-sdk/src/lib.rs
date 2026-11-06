//! Offline-testable, bounded OpenAI Responses, Files and Batch client.
//! See the crate README for construction, completion and cancellation semantics.
#![allow(missing_docs)]

use concord::{
    Binary, Endpoint, Json, Received, SendWith,
    reqwest::{
        self,
        header::{AUTHORIZATION, HeaderMap, HeaderValue},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    time::Duration,
};

concord::api! {
    /// Wire declarations for the supported operations. Every `send()` returns
    /// `Result<Response<T>, Error>` (this crate's, through [`SendPolicy`]); [`OpenAi`] adds the
    /// per-operation input rules, the SSE parser and the bounded downloads.
    pub client OpenAiApi(#[sensitive] header(AUTHORIZATION) authorization: HeaderValue) {
        base "https://api.openai.com";
        path "v1";
        send SendPolicy;
        request Json;
        response Json;
        /// Create a response.
        POST response(body request: ResponseRequest) -> ResponseObject { path "responses"; }
        /// Open a response event stream: the unread body, for the SDK's SSE parser.
        POST response_stream(body request: ResponseRequest) -> stream {
            path "responses";
            headers { "accept" = "text/event-stream"; }
        }
        /// Upload batch input.
        POST upload(body bytes: Vec<u8>) -> FileObject {
            path "files";
            request Binary;
            headers { "content-type" = "multipart/form-data; boundary=concord-batch-boundary"; }
        }
        /// Retrieve metadata.
        GET file(id: String) -> FileObject { path "files/{id}"; }
        /// Retrieve file bytes.
        GET content(id: String) -> Vec<u8> { path "files/{id}/content"; response Binary; }
        /// Create a batch.
        POST batch(body request: BatchRequest) -> Batch { path "batches"; }
        /// Retrieve batch status.
        GET batch_status(id: String) -> Batch { path "batches/{id}"; }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResponseRequest {
    pub model: String,
    pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}
impl ResponseRequest {
    pub fn new(model: impl Into<String>, input: impl Into<Value>) -> Self {
        Self {
            model: model.into(),
            input: input.into(),
            stream: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResponseObject {
    pub id: String,
    pub status: Option<String>,
    #[serde(default)]
    pub output: Vec<Value>,
    #[serde(default)]
    pub error: Option<Value>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct FileObject {
    pub id: String,
    pub filename: String,
    pub purpose: String,
    pub bytes: u64,
    #[serde(default)]
    pub status: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct BatchRequest {
    pub input_file_id: String,
    pub endpoint: String,
    pub completion_window: String,
}
impl BatchRequest {
    pub fn responses(input_file_id: impl Into<String>) -> Self {
        Self {
            input_file_id: input_file_id.into(),
            endpoint: "/v1/responses".into(),
            completion_window: "24h".into(),
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct Batch {
    pub id: String,
    pub status: BatchStatus,
    pub output_file_id: Option<String>,
    pub error_file_id: Option<String>,
    #[serde(default)]
    pub errors: Option<Value>,
    #[serde(default)]
    pub request_counts: Option<Value>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    Validating,
    Failed,
    InProgress,
    Finalizing,
    Completed,
    Expired,
    Cancelling,
    Cancelled,
    #[serde(other)]
    Unknown,
}
impl BatchStatus {
    pub fn terminal(&self) -> bool {
        matches!(
            self,
            Self::Failed | Self::Completed | Self::Expired | Self::Cancelled
        )
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct BatchInput {
    pub custom_id: String,
    pub method: String,
    pub url: String,
    pub body: ResponseRequest,
}
impl BatchInput {
    pub fn response(id: impl Into<String>, body: ResponseRequest) -> Self {
        Self {
            custom_id: id.into(),
            method: "POST".into(),
            url: "/v1/responses".into(),
            body,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct BatchItem {
    pub id: String,
    pub custom_id: String,
    pub response: Option<BatchItemResponse>,
    pub error: Option<ItemError>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct BatchItemResponse {
    pub status_code: u16,
    pub request_id: Option<String>,
    pub body: Value,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ItemError {
    pub code: Option<String>,
    pub message: String,
}

/// Builds JSONL while refusing duplicate or empty IDs and streaming requests.
pub fn batch_jsonl(rows: &[BatchInput]) -> Result<Vec<u8>, Error> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for row in rows {
        if row.custom_id.is_empty() || !seen.insert(&row.custom_id) || row.body.stream == Some(true)
        {
            return Err(Error::Invalid(
                "batch IDs must be nonempty and unique; streaming is unsupported",
            ));
        }
        serde_json::to_writer(&mut out, row).map_err(Error::Json)?;
        out.push(b'\n');
    }
    Ok(out)
}
/// Parse each line independently; retain malformed rows and duplicate IDs as errors.
pub fn parse_batch_jsonl(bytes: &[u8]) -> Vec<Result<BatchItem, String>> {
    let mut seen = HashSet::new();
    bytes
        .split(|b| *b == b'\n')
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(n, line)| {
            let item: BatchItem =
                serde_json::from_slice(line).map_err(|e| format!("line {}: {e}", n + 1))?;
            if item.custom_id.is_empty() || !seen.insert(item.custom_id.clone()) {
                return Err(format!("line {}: duplicate or empty custom_id", n + 1));
            }
            Ok(item)
        })
        .collect()
}
/// Correlate rows from both files; duplicates and malformed rows are retained in `problems`.
#[derive(Debug, Default)]
pub struct BatchResults {
    pub items: BTreeMap<String, BatchItem>,
    pub problems: Vec<String>,
    pub file_errors: Vec<(String, Error)>,
}
impl BatchResults {
    pub fn add_file(&mut self, bytes: &[u8]) {
        for row in parse_batch_jsonl(bytes) {
            match row {
                Ok(item) if !self.items.contains_key(&item.custom_id) => {
                    self.items.insert(item.custom_id.clone(), item);
                }
                Ok(item) => self
                    .problems
                    .push(format!("duplicate custom_id: {}", item.custom_id)),
                Err(e) => self.problems.push(e),
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResponseMeta {
    pub status: reqwest::StatusCode,
    pub headers: HeaderMap,
    pub operation: Option<concord::Operation>,
}
impl ResponseMeta {
    fn from<T>(r: &Received<T>) -> Self {
        Self {
            status: r.status(),
            headers: r.head().headers().clone(),
            operation: r.operation(),
        }
    }
}
#[derive(Debug)]
pub struct Response<T> {
    pub value: T,
    pub meta: ResponseMeta,
}
#[derive(Debug, Deserialize)]
pub struct ProviderError {
    pub message: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub param: Option<String>,
    pub code: Option<Value>,
}
#[derive(Debug, Deserialize)]
struct ErrorEnvelope {
    error: ProviderError,
}
#[derive(Debug)]
pub enum Error {
    Concord(concord::Error),
    Provider {
        meta: Box<ResponseMeta>,
        error: Option<Box<ProviderError>>,
    },
    Decode {
        meta: Box<ResponseMeta>,
        error: concord::Error,
    },
    Json(serde_json::Error),
    Invalid(&'static str),
    Stream(String),
    StreamFailed(Box<ResponseObject>),
    StreamEventError {
        message: String,
        code: Option<Value>,
    },
    PollLimit(Box<Batch>),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<concord::Error> for Error {
    fn from(e: concord::Error) -> Self {
        Self::Concord(e)
    }
}
async fn checked<T>(r: Received<T>) -> Result<(Received<T>, ResponseMeta), Error> {
    let meta = ResponseMeta::from(&r);
    if !meta.status.is_success() {
        let error = r
            .bytes_limited(65536)
            .await
            .ok()
            .and_then(|b| serde_json::from_slice::<ErrorEnvelope>(&b).ok())
            .map(|e| Box::new(e.error));
        return Err(Error::Provider {
            meta: Box::new(meta),
            error,
        });
    }
    Ok((r, meta))
}
/// The send policy of [`OpenAiApi`] (`send SendPolicy;`): every generated `send()` returns
/// `Result<Response<T>, Error>`, with OpenAI's error document on a non-2xx status.
#[derive(Default)]
pub struct SendPolicy;

impl<T: Send> SendWith<T> for SendPolicy {
    type Output = Result<Response<T>, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let (r, meta) = checked(endpoint.prepare()?.receive().await?).await?;
        let value = r.decode().await.map_err(|error| Error::Decode {
            meta: Box::new(meta.clone()),
            error,
        })?;
        Ok(Response { value, meta })
    }
}

/// Configured client. Use an application-supplied transport with timeouts and redirect policy.
#[derive(Clone, Debug)]
pub struct OpenAi {
    api: OpenAiApi,
}
impl OpenAi {
    pub fn new(http: reqwest::Client, token: &str) -> Result<Self, Error> {
        let mut authorization = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| Error::Invalid("invalid authorization header"))?;
        authorization.set_sensitive(true);
        Ok(Self {
            api: OpenAiApi::new(http, OpenAiApiConfig { authorization }),
        })
    }
    pub fn with_base_url(mut self, base: &str) -> Result<Self, Error> {
        self.api = self.api.with_base_url(base)?;
        Ok(self)
    }
    pub async fn create_response(
        &self,
        mut request: ResponseRequest,
    ) -> Result<Response<ResponseObject>, Error> {
        request.stream = Some(false);
        self.api.response(request).send().await
    }
    /// Dropping this future or the returned stream cancels local consumption; it does not cancel server-side work.
    pub async fn stream_response(
        &self,
        mut request: ResponseRequest,
    ) -> Result<EventStream, Error> {
        request.stream = Some(true);
        let Response {
            value: incoming,
            meta,
        } = self.api.response_stream(request).send().await?;
        let content_type = meta
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .eq_ignore_ascii_case("text/event-stream")
        {
            return Err(Error::Stream("expected text/event-stream".into()));
        }
        Ok(EventStream {
            body: incoming,
            meta,
            buffer: Vec::new(),
            data: Vec::new(),
            done: false,
        })
    }
    /// Upload buffered JSONL; rejects unsafe filenames and boundary collisions.
    pub async fn upload_batch_file(
        &self,
        filename: &str,
        bytes: &[u8],
    ) -> Result<Response<FileObject>, Error> {
        if filename.is_empty()
            || filename
                .bytes()
                .any(|b| b < 32 || b == 127 || b == b'"' || b == b'\\')
            || bytes.windows(22).any(|w| w == b"concord-batch-boundary")
        {
            return Err(Error::Invalid(
                "unsafe filename or multipart boundary collision",
            ));
        }
        let mut body = format!("--concord-batch-boundary\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\nbatch\r\n--concord-batch-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/jsonl\r\n\r\n").into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n--concord-batch-boundary--\r\n");
        self.api.upload(body).send().await
    }
    pub async fn file(&self, id: &str) -> Result<Response<FileObject>, Error> {
        if id.is_empty() {
            return Err(Error::Invalid("empty file ID"));
        }
        self.api.file(id).send().await
    }
    /// Buffer at most `limit` bytes of file content.
    pub async fn content(&self, id: &str, limit: usize) -> Result<Response<Vec<u8>>, Error> {
        if id.is_empty() {
            return Err(Error::Invalid("empty file ID"));
        }
        let (r, meta) = checked(self.api.content(id).prepare()?.receive().await?).await?;
        let value = r
            .bytes_limited(limit)
            .await
            .map_err(|error| Error::Decode {
                meta: Box::new(meta.clone()),
                error,
            })?
            .to_vec();
        Ok(Response { value, meta })
    }
    pub async fn create_batch(&self, request: BatchRequest) -> Result<Response<Batch>, Error> {
        self.api.batch(request).send().await
    }
    pub async fn batch(&self, id: &str) -> Result<Response<Batch>, Error> {
        if id.is_empty() {
            return Err(Error::Invalid("empty batch ID"));
        }
        self.api.batch_status(id).send().await
    }
    /// Poll at most `attempts` times, returning the last batch on exhaustion. Drop the future to cancel.
    pub async fn poll_batch(
        &self,
        id: &str,
        attempts: usize,
        delay: Duration,
    ) -> Result<Response<Batch>, Error> {
        if attempts == 0 {
            return Err(Error::Invalid("poll attempts must be positive"));
        }
        for i in 0..attempts {
            let result = self.batch(id).await?;
            if result.value.status.terminal() {
                return Ok(result);
            }
            if i + 1 == attempts {
                return Err(Error::PollLimit(Box::new(result.value)));
            }
            tokio::time::sleep(delay).await;
        }
        unreachable!()
    }
    /// Retrieve whichever output and error files are present, retaining rows from both.
    pub async fn batch_results(
        &self,
        batch: &Batch,
        limit_per_file: usize,
    ) -> Result<BatchResults, Error> {
        let mut results = BatchResults::default();
        for id in [&batch.output_file_id, &batch.error_file_id]
            .into_iter()
            .flatten()
        {
            match self.content(id, limit_per_file).await {
                Ok(file) => results.add_file(&file.value),
                Err(error) => results.file_errors.push((id.clone(), error)),
            };
        }
        Ok(results)
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    #[serde(rename = "response.created")]
    Created { response: ResponseObject },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta { delta: String },
    #[serde(rename = "response.completed")]
    Completed { response: ResponseObject },
    #[serde(rename = "response.failed")]
    Failed { response: ResponseObject },
    #[serde(rename = "error")]
    Error {
        message: String,
        #[serde(default)]
        code: Option<Value>,
    },
    #[serde(other)]
    Other,
}
/// Incremental SSE reader. EOF before a completion event is an error. Drop to cancel.
pub struct EventStream {
    body: concord::Incoming,
    pub meta: ResponseMeta,
    buffer: Vec<u8>,
    data: Vec<Vec<u8>>,
    done: bool,
}
impl EventStream {
    pub async fn next(&mut self) -> Result<Option<Event>, Error> {
        if self.done {
            return Ok(None);
        }
        loop {
            if let Some(pos) = self.buffer.iter().position(|b| *b == b'\n') {
                let mut line: Vec<u8> = self.buffer.drain(..=pos).collect();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if line.is_empty() {
                    if self.data.is_empty() {
                        continue;
                    }
                    let data = std::mem::take(&mut self.data).join(&b'\n');
                    let event: Event = serde_json::from_slice(&data).map_err(Error::Json)?;
                    match &event {
                        Event::Completed { .. } => self.done = true,
                        Event::Failed { response } => {
                            self.done = true;
                            return Err(Error::StreamFailed(Box::new(response.clone())));
                        }
                        Event::Error { message, code } => {
                            self.done = true;
                            return Err(Error::StreamEventError {
                                message: message.clone(),
                                code: code.clone(),
                            });
                        }
                        _ => {}
                    }
                    return Ok(Some(event));
                }
                if let Some(data) = line.strip_prefix(b"data:") {
                    self.data
                        .push(data.strip_prefix(b" ").unwrap_or(data).to_vec());
                } else if line == b"data" {
                    self.data.push(Vec::new());
                }
                continue;
            }
            match self.body.chunk().await? {
                Some(chunk) => {
                    if self.buffer.len()
                        + self.data.iter().map(Vec::len).sum::<usize>()
                        + chunk.len()
                        > 1024 * 1024
                    {
                        return Err(Error::Stream("SSE event exceeds 1 MiB".into()));
                    }
                    self.buffer.extend_from_slice(&chunk);
                }
                None => {
                    self.done = true;
                    return Err(Error::Stream("SSE ended before response.completed".into()));
                }
            }
        }
    }
}
