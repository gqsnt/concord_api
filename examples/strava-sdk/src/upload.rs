//! One-shot streaming multipart encoding and bounded processing polls.
use crate::{Error, StravaUploads};
use concord::{CodecError, Encode, EncodedBody, reqwest};
use serde::Deserialize;
use std::{fmt, num::NonZeroUsize, time::Duration};

/// Supported provider file encodings.
#[derive(Clone, Copy, Debug)]
pub enum DataType {
    /// FIT.
    Fit,
    /// Gzipped FIT.
    FitGz,
    /// TCX.
    Tcx,
    /// Gzipped TCX.
    TcxGz,
    /// GPX.
    Gpx,
    /// Gzipped GPX.
    GpxGz,
    /// Strava's limited strength-training JSON.
    Json,
}
impl DataType {
    fn wire(self) -> &'static str {
        match self {
            Self::Fit => "fit",
            Self::FitGz => "fit.gz",
            Self::Tcx => "tcx",
            Self::TcxGz => "tcx.gz",
            Self::Gpx => "gpx",
            Self::GpxGz => "gpx.gz",
            Self::Json => "json",
        }
    }
}
/// A non-cloneable multipart activity submission. The source is moved and never buffered for replay.
pub struct UploadFile {
    form: reqwest::multipart::Form,
}
impl UploadFile {
    /// Accept a native one-shot body (for example `Body::wrap_stream` over an open file).
    /// Filename controls, quotes and backslashes are refused before the source is polled.
    pub fn new(filename: &str, data_type: DataType, source: reqwest::Body) -> Result<Self, Error> {
        if filename.is_empty()
            || filename
                .chars()
                .any(|c| c.is_control() || matches!(c, '"' | '\\'))
        {
            return Err(Error::Invalid("invalid upload filename"));
        }
        let part = reqwest::multipart::Part::stream(source).file_name(filename.to_owned());
        Ok(Self {
            form: reqwest::multipart::Form::new()
                .percent_encode_attr_chars()
                .text("data_type", data_type.wire())
                .part("file", part),
        })
    }
    /// Add a provider-visible external identifier. It is a label, not an idempotency guarantee.
    pub fn external_id(mut self, id: String) -> Self {
        self.form = self.form.text("external_id", id);
        self
    }
    /// Optional activity title.
    pub fn name(mut self, name: String) -> Self {
        self.form = self.form.text("name", name);
        self
    }
    /// Optional case-sensitive sport override.
    pub fn sport_type(mut self, sport: String) -> Self {
        self.form = self.form.text("sport_type", sport);
        self
    }
}
impl fmt::Debug for UploadFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UploadFile(<one-shot>)")
    }
}
/// `Encode::CONTENT_TYPE` is `None`; `encode_body` binds
/// `content-type: multipart/form-data; boundary=<boundary>` from reqwest's `Form::boundary()`.
///
/// Runtime multipart format using reqwest's streaming encoder through Concord's ordinary hook.
/// Concord's built-in multipart format requires in-memory parts and cannot carry this source.
pub struct StreamMultipart;
impl Encode<UploadFile> for StreamMultipart {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(_: UploadFile) -> Result<reqwest::Body, CodecError> {
        Err(CodecError::new("multipart requires its runtime media type"))
    }
    fn encode_body(value: UploadFile) -> Result<EncodedBody, CodecError> {
        let media = reqwest::header::HeaderValue::from_str(&format!(
            "multipart/form-data; boundary={}",
            value.form.boundary()
        ))
        .map_err(|_| CodecError::new("invalid multipart media type"))?;
        Ok(EncodedBody::with_content_type(
            reqwest::Body::wrap_stream(value.form.into_stream()),
            media,
        ))
    }
}
/// Upload receipt / processing result. Debug withholds all provider strings, including HTML errors.
#[derive(Clone, Deserialize)]
pub struct UploadStatus {
    /// 64-bit upload identifier (distinct from the activity identifier).
    pub id: u64,
    /// String rendering for consumers unable to represent 64-bit integers.
    pub id_str: Option<String>,
    /// Caller-visible filename / external label.
    pub external_id: Option<String>,
    /// Non-null means processing failed; never render as trusted HTML.
    pub error: Option<String>,
    /// Human-readable status, deliberately not parsed as an enum.
    pub status: String,
    /// Present when processing completes successfully.
    pub activity_id: Option<u64>,
}
impl fmt::Debug for UploadStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UploadStatus")
            .field("id", &self.id)
            .field("failed", &self.error.is_some())
            .field("activity_id", &self.activity_id)
            .finish_non_exhaustive()
    }
}
/// Finite upload processing policy. Production polls wait at least one second.
#[derive(Clone, Copy, Debug)]
pub struct PollPolicy {
    /// Maximum status GETs; submission is separate.
    pub max_polls: NonZeroUsize,
    /// Delay before each poll, including the first.
    pub interval: Duration,
}
impl StravaUploads {
    /// Poll an existing receipt; never re-upload. Returns the activity ID on completion.
    /// Bound/failure errors retain the upload receipt for explicit inspection or resumption.
    pub async fn poll(
        &self,
        receipt: UploadStatus,
        policy: PollPolicy,
    ) -> Result<UploadStatus, Error> {
        if policy.interval < Duration::from_secs(1) {
            return Err(Error::Invalid("poll interval below one second"));
        }
        let id = receipt.id;
        if id == 0 {
            return Err(Error::Invalid("zero upload identifier"));
        }
        let mut last = receipt;
        for _ in 0..policy.max_polls.get() {
            if last.error.is_some() {
                return Err(Error::Processing(Box::new(last)));
            }
            if last.activity_id.is_some() {
                return Ok(last);
            }
            tokio::time::sleep(policy.interval).await;
            let response = self.status(id).send().await?;
            if response.value.id != id {
                return Err(Error::Invalid("upload identifier changed while polling"));
            }
            if response.value.error.is_some() {
                return Err(Error::Processing(Box::new(response.value)));
            }
            if response.value.activity_id.is_some() {
                return Ok(response.value);
            }
            last = response.value;
        }
        Err(Error::PollBounded(Box::new(last)))
    }
    /// Submit a one-shot file and poll its returned ID until success, failure or the poll cap.
    pub async fn upload_and_poll(
        &self,
        file: UploadFile,
        policy: PollPolicy,
    ) -> Result<UploadStatus, Error> {
        if policy.interval < Duration::from_secs(1) {
            return Err(Error::Invalid("poll interval below one second"));
        }
        let response = self.create(file).send().await?;
        if response.value.error.is_some() {
            return Err(Error::Processing(Box::new(response.value)));
        }
        if response.value.activity_id.is_some() {
            return Ok(response.value);
        }
        self.poll(response.value, policy).await
    }
}
