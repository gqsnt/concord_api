use crate::codec::CodecError;
use crate::codec::ContentType;
use crate::stream_body::StreamBody;
use crate::stream_body::StreamByteSource;
use bytes::Bytes;
use futures_core::Stream;
use http::HeaderValue;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

static MULTIPART_BOUNDARY_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FormData;

impl ContentType for FormData {
    const CONTENT_TYPE: &'static str = "multipart/form-data";
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MultipartBodyErrorKind {
    InvalidPartName,
    InvalidPartFileName,
    InvalidPartContentType,
    InvalidMultipartContentType,
}

pub struct MultipartBodyError {
    kind: MultipartBodyErrorKind,
}

impl MultipartBodyError {
    pub(crate) fn new(kind: MultipartBodyErrorKind) -> Self {
        Self { kind }
    }

    pub fn kind(&self) -> MultipartBodyErrorKind {
        self.kind
    }
}

impl fmt::Debug for MultipartBodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MultipartBodyError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for MultipartBodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.kind {
            MultipartBodyErrorKind::InvalidPartName => "multipart part name is invalid",
            MultipartBodyErrorKind::InvalidPartFileName => "multipart part file name is invalid",
            MultipartBodyErrorKind::InvalidPartContentType => {
                "multipart part content type is invalid"
            }
            MultipartBodyErrorKind::InvalidMultipartContentType => {
                "multipart request content type is invalid"
            }
        })
    }
}

impl Error for MultipartBodyError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

pub struct RawPart {
    name: String,
    file_name: Option<String>,
    content_type: Option<HeaderValue>,
    body: RawPartBody,
}

impl RawPart {
    pub fn text(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            file_name: None,
            content_type: None,
            body: RawPartBody::Text(Bytes::from(value.into())),
        }
    }

    pub fn bytes(name: impl Into<String>, bytes: Bytes) -> Self {
        Self {
            name: name.into(),
            file_name: None,
            content_type: None,
            body: RawPartBody::Bytes(bytes),
        }
    }

    pub fn bytes_with(
        name: impl Into<String>,
        file_name: Option<String>,
        content_type: Option<HeaderValue>,
        bytes: Bytes,
    ) -> Self {
        Self {
            name: name.into(),
            file_name,
            content_type,
            body: RawPartBody::Bytes(bytes),
        }
    }

    pub fn stream(name: impl Into<String>, stream: StreamBody) -> Self {
        Self {
            name: name.into(),
            file_name: None,
            content_type: None,
            body: RawPartBody::Stream(stream),
        }
    }

    pub fn stream_with(
        name: impl Into<String>,
        file_name: Option<String>,
        content_type: Option<HeaderValue>,
        stream: StreamBody,
    ) -> Self {
        Self {
            name: name.into(),
            file_name,
            content_type,
            body: RawPartBody::Stream(stream),
        }
    }
}

impl fmt::Debug for RawPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawPart")
            .field("kind", &self.body.kind_name())
            .field("has_file_name", &self.file_name.is_some())
            .field("has_content_type", &self.content_type.is_some())
            .finish_non_exhaustive()
    }
}

enum RawPartBody {
    Text(Bytes),
    Bytes(Bytes),
    Stream(StreamBody),
}

impl RawPartBody {
    fn kind_name(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Bytes(_) => "bytes",
            Self::Stream(_) => "stream",
        }
    }
}

pub struct MultipartBody {
    boundary: String,
    parts: Vec<RawPart>,
}

impl Default for MultipartBody {
    fn default() -> Self {
        Self::new()
    }
}

impl MultipartBody {
    pub fn new() -> Self {
        let boundary_id = MULTIPART_BOUNDARY_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            boundary: format!("concord-multipart-{boundary_id}"),
            parts: Vec::new(),
        }
    }

    pub fn text(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.part(RawPart::text(name, value))
    }

    pub fn bytes(self, name: impl Into<String>, bytes: Bytes) -> Self {
        self.part(RawPart::bytes(name, bytes))
    }

    pub fn bytes_with(
        self,
        name: impl Into<String>,
        file_name: Option<String>,
        content_type: Option<HeaderValue>,
        bytes: Bytes,
    ) -> Self {
        self.part(RawPart::bytes_with(name, file_name, content_type, bytes))
    }

    pub fn stream(self, name: impl Into<String>, stream: StreamBody) -> Self {
        self.part(RawPart::stream(name, stream))
    }

    pub fn stream_with(
        self,
        name: impl Into<String>,
        file_name: Option<String>,
        content_type: Option<HeaderValue>,
        stream: StreamBody,
    ) -> Self {
        self.part(RawPart::stream_with(name, file_name, content_type, stream))
    }

    pub fn part(mut self, part: RawPart) -> Self {
        self.parts.push(part);
        self
    }

    pub fn content_type(&self) -> HeaderValue {
        HeaderValue::from_str(&format!("multipart/form-data; boundary={}", self.boundary))
            .expect("generated multipart boundary must be valid")
    }

    pub fn try_content_type(&self) -> Result<HeaderValue, http::header::InvalidHeaderValue> {
        HeaderValue::from_str(&format!("multipart/form-data; boundary={}", self.boundary))
    }

    pub(crate) fn into_dyn_body(self) -> Result<crate::body::DynBody, MultipartBodyError> {
        let MultipartBody { boundary, parts } = self;
        let prepared = PreparedMultipartBody::from_parts(boundary, parts)?;
        Ok(crate::body::DynBody::from_byte_stream(
            MultipartEncodeStream::new(prepared),
        ))
    }
}

impl fmt::Debug for MultipartBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MultipartBody")
            .field("parts", &self.parts.len())
            .finish_non_exhaustive()
    }
}

struct PreparedMultipartBody {
    boundary: String,
    parts: Vec<PreparedPart>,
}

impl PreparedMultipartBody {
    fn from_parts(boundary: String, parts: Vec<RawPart>) -> Result<Self, MultipartBodyError> {
        let mut prepared = Vec::with_capacity(parts.len());
        for part in parts {
            validate_part_name(&part.name)?;
            if let Some(file_name) = &part.file_name {
                validate_part_file_name(file_name)?;
            }
            let content_type = match part.content_type {
                Some(content_type) => Some(
                    content_type
                        .to_str()
                        .map_err(|_| {
                            MultipartBodyError::new(MultipartBodyErrorKind::InvalidPartContentType)
                        })?
                        .to_string(),
                ),
                None => None,
            };
            let body = match part.body {
                RawPartBody::Text(text) => PreparedPartBody::Text(Some(text)),
                RawPartBody::Bytes(bytes) => PreparedPartBody::Bytes(Some(bytes)),
                RawPartBody::Stream(stream) => {
                    PreparedPartBody::Stream(Some(stream.into_byte_stream()))
                }
            };
            prepared.push(PreparedPart {
                name: part.name,
                file_name: part.file_name,
                content_type,
                body,
            });
        }
        Ok(Self {
            boundary,
            parts: prepared,
        })
    }
}

struct PreparedPart {
    name: String,
    file_name: Option<String>,
    content_type: Option<String>,
    body: PreparedPartBody,
}

enum PreparedPartBody {
    Text(Option<Bytes>),
    Bytes(Option<Bytes>),
    Stream(Option<StreamByteSource>),
}

struct MultipartEncodeStream {
    boundary: String,
    parts: VecDeque<PreparedPart>,
    current: Option<ActivePart>,
    closing_emitted: bool,
}

impl MultipartEncodeStream {
    fn new(parts: PreparedMultipartBody) -> Self {
        Self {
            boundary: parts.boundary,
            parts: parts.parts.into(),
            current: None,
            closing_emitted: false,
        }
    }
}

struct ActivePart {
    header: Option<Bytes>,
    body: ActiveBody,
    trailer_pending: bool,
}

enum ActiveBody {
    Text(Option<Bytes>),
    Bytes(Option<Bytes>),
    Stream {
        stream: StreamByteSource,
        done: bool,
    },
}

impl ActivePart {
    fn new(boundary: &str, part: PreparedPart) -> Result<Self, CodecError> {
        Ok(Self {
            header: Some(build_part_header(boundary, &part)?),
            body: match part.body {
                PreparedPartBody::Text(bytes) => ActiveBody::Text(bytes),
                PreparedPartBody::Bytes(bytes) => ActiveBody::Bytes(bytes),
                PreparedPartBody::Stream(stream) => ActiveBody::Stream {
                    stream: stream
                        .ok_or_else(|| CodecError::new("multipart request encoding failed"))?,
                    done: false,
                },
            },
            trailer_pending: false,
        })
    }
}

impl Stream for MultipartEncodeStream {
    type Item = Result<Bytes, CodecError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(mut active) = this.current.take() {
                if let Some(header) = active.header.take() {
                    active.trailer_pending = true;
                    this.current = Some(active);
                    return Poll::Ready(Some(Ok(header)));
                }

                let result = match &mut active.body {
                    ActiveBody::Text(bytes) | ActiveBody::Bytes(bytes) => bytes.take().map(Ok),
                    ActiveBody::Stream { stream, done } => {
                        if *done {
                            None
                        } else {
                            match Pin::new(stream).poll_next(cx) {
                                Poll::Pending => {
                                    this.current = Some(active);
                                    return Poll::Pending;
                                }
                                Poll::Ready(Some(Ok(bytes))) => Some(Ok(bytes)),
                                Poll::Ready(Some(Err(_error))) => {
                                    Some(Err(CodecError::new("multipart request encoding failed")))
                                }
                                Poll::Ready(None) => {
                                    *done = true;
                                    None
                                }
                            }
                        }
                    }
                };

                if let Some(result) = result {
                    match result {
                        Ok(bytes) => {
                            this.current = Some(active);
                            return Poll::Ready(Some(Ok(bytes)));
                        }
                        Err(error) => return Poll::Ready(Some(Err(error))),
                    }
                }

                if active.trailer_pending {
                    active.trailer_pending = false;
                    this.current = None;
                    return Poll::Ready(Some(Ok(Bytes::from_static(b"\r\n"))));
                }

                continue;
            }

            if let Some(part) = this.parts.pop_front() {
                match ActivePart::new(&this.boundary, part) {
                    Ok(active) => {
                        this.current = Some(active);
                        continue;
                    }
                    Err(error) => return Poll::Ready(Some(Err(error))),
                }
            }

            if !this.closing_emitted {
                this.closing_emitted = true;
                let closing = Bytes::from(format!("--{}--\r\n", this.boundary));
                return Poll::Ready(Some(Ok(closing)));
            }
            return Poll::Ready(None);
        }
    }
}

fn build_part_header(boundary: &str, part: &PreparedPart) -> Result<Bytes, CodecError> {
    let mut out = String::new();
    write!(&mut out, "--{boundary}\r\n").expect("writing to string cannot fail");
    write!(
        &mut out,
        "Content-Disposition: form-data; name=\"{}\"",
        part.name
    )
    .expect("writing to string cannot fail");
    if let Some(file_name) = &part.file_name {
        write!(&mut out, "; filename=\"{}\"", file_name).expect("writing to string cannot fail");
    }
    out.push_str("\r\n");
    if let Some(content_type) = &part.content_type {
        write!(&mut out, "Content-Type: {content_type}\r\n")
            .expect("writing to string cannot fail");
    }
    out.push_str("\r\n");
    Ok(Bytes::from(out))
}

fn validate_part_name(value: &str) -> Result<(), MultipartBodyError> {
    if is_valid_disposition_value(value) {
        Ok(())
    } else {
        Err(MultipartBodyError::new(
            MultipartBodyErrorKind::InvalidPartName,
        ))
    }
}

fn validate_part_file_name(value: &str) -> Result<(), MultipartBodyError> {
    if is_valid_disposition_value(value) {
        Ok(())
    } else {
        Err(MultipartBodyError::new(
            MultipartBodyErrorKind::InvalidPartFileName,
        ))
    }
}

fn is_valid_disposition_value(value: &str) -> bool {
    !value.is_empty()
        && value.is_ascii()
        && value.bytes().all(|byte| {
            matches!(byte, 0x20..=0x7E) && !matches!(byte, b'\\' | b'"' | b'\r' | b'\n')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    #[test]
    fn multipart_body_debug_is_body_free() {
        let body = MultipartBody::new()
            .text("title", "SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR")
            .bytes("file", Bytes::from_static(b"abc"));
        let rendered = format!("{body:?}");
        assert!(!rendered.contains("SECRET_MULTIPART_SENTINEL_MUST_NOT_APPEAR"));
        assert!(rendered.contains("MultipartBody"));
    }

    #[tokio::test]
    async fn multipart_form_data_body_encodes_headers_and_streams_bytes() {
        let body = MultipartBody::new()
            .text("title", "hello")
            .bytes("file", Bytes::from_static(b"abc"));
        let content_type = body.content_type();
        assert!(
            content_type
                .to_str()
                .expect("multipart content type")
                .starts_with("multipart/form-data; boundary=")
        );

        let out = body
            .into_dyn_body()
            .expect("multipart body")
            .collect()
            .await
            .expect("multipart encoding")
            .to_bytes();
        let rendered = String::from_utf8(out.to_vec()).expect("multipart body should be utf-8");
        assert!(rendered.contains("Content-Disposition: form-data; name=\"title\""));
        assert!(rendered.contains("Content-Disposition: form-data; name=\"file\""));
        assert!(rendered.contains("abc"));
        assert!(rendered.contains("\r\n"));
        assert!(rendered.ends_with("\r\n"));
        assert!(rendered.contains("--concord-multipart-"));
    }

    #[test]
    fn multipart_invalid_metadata_is_rejected_body_safely() {
        let body = MultipartBody::new().text("bad\r\nname", "value");
        let err = body.into_dyn_body().expect_err("invalid name should fail");
        assert_eq!(err.kind(), MultipartBodyErrorKind::InvalidPartName);
        assert!(!err.to_string().contains("bad\r\nname"));
    }
}
