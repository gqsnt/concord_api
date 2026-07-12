use crate::body::BodyError;
use crate::body::DynBody;
use crate::stream_body::StreamBody;
use bytes::Bytes;
use futures_core::Stream;
use http::HeaderValue;
use reqwest::multipart::{Form, Part};
use std::error::Error;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FormData;

impl crate::codec::ContentType for FormData {
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
    fn new(kind: MultipartBodyErrorKind) -> Self {
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
                "multipart content type is invalid"
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
            body: RawPartBody::Text(value.into()),
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
    Text(String),
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
    parts: Vec<RawPart>,
}

impl Default for MultipartBody {
    fn default() -> Self {
        Self::new()
    }
}

impl MultipartBody {
    pub fn new() -> Self {
        Self { parts: Vec::new() }
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

    pub(crate) fn into_prepared(self) -> Result<(HeaderValue, DynBody), MultipartBodyError> {
        let form = self.into_form()?;
        let boundary = form.boundary();
        let media_type =
            HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}")).map_err(
                |_| MultipartBodyError::new(MultipartBodyErrorKind::InvalidMultipartContentType),
            )?;

        let stream = FormStream::new(form.into_stream());
        let body = DynBody::from_byte_stream(stream);

        Ok((media_type, body))
    }
}

pub struct MultipartReplayFactory {
    factory: Arc<dyn Fn() -> Result<MultipartBody, MultipartBodyError> + Send + Sync>,
}

impl MultipartReplayFactory {
    pub fn new(
        factory: impl Fn() -> Result<MultipartBody, MultipartBodyError> + Send + Sync + 'static,
    ) -> Self {
        Self {
            factory: Arc::new(factory),
        }
    }

    pub fn into_prepared_body(self) -> Result<crate::io::PreparedBody, MultipartBodyError> {
        // Multipart framing is owned by Reqwest and its aggregate size is not
        // known until a form is produced.  Keep planning metadata unknown and
        // use each produced body's own safe hint for the physical attempt.
        let size_hint = http_body::SizeHint::new();
        let factory = self.factory;
        Ok(crate::io::PreparedBody::replay_factory_with_media(
            size_hint,
            move || {
                let body =
                    factory().map_err(|_| crate::body::BodyError::invalid_configuration())?;
                let (media_type, body) = body
                    .into_prepared()
                    .map_err(|_| crate::body::BodyError::invalid_configuration())?;
                Ok((body, media_type))
            },
        ))
    }
}

struct FormStream<S> {
    inner: S,
}

impl<S> FormStream<S> {
    fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> Stream for FormStream<S>
where
    S: futures_core::Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Bytes, BodyError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(value))) => Poll::Ready(Some(Ok(value))),
            Poll::Ready(Some(Err(_error))) => Poll::Ready(Some(Err(BodyError::input()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl fmt::Debug for MultipartBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MultipartBody")
            .field("parts", &self.parts.len())
            .finish_non_exhaustive()
    }
}

fn build_part(part: RawPart) -> Result<(String, Part), MultipartBodyError> {
    let name = part.name;
    validate_part_name(&name)?;
    let mut body = match part.body {
        RawPartBody::Text(text) => Part::text(text),
        RawPartBody::Bytes(bytes) => Part::stream(reqwest::Body::from(bytes)),
        RawPartBody::Stream(stream) => {
            let source = DynBody::from_stream_body(stream);
            let body = reqwest::Body::wrap(source);
            Part::stream(body)
        }
    };

    if let Some(file_name) = part.file_name {
        validate_part_file_name(&file_name)?;
        body = body.file_name(file_name);
    }

    if let Some(content_type) = part.content_type {
        let content_type = content_type
            .to_str()
            .map_err(|_| MultipartBodyError::new(MultipartBodyErrorKind::InvalidPartContentType))?;
        body = body
            .mime_str(content_type)
            .map_err(|_| MultipartBodyError::new(MultipartBodyErrorKind::InvalidPartContentType))?;
    }

    Ok((name, body))
}

impl MultipartBody {
    fn into_form(self) -> Result<Form, MultipartBodyError> {
        let mut form = Form::new();
        for part in self.parts {
            let (name, part) = build_part(part)?;
            form = form.part(name, part);
        }
        Ok(form)
    }
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
            matches!(byte, 0x20..=0x7E) && !matches!(byte, b'\\' | b'\"' | b'\r' | b'\n')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream_body::StreamBody;
    use crate::stream_body::StreamBodyError;
    use futures_core::Stream;
    use http_body::Body as _;
    use http_body_util::BodyExt;
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};

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
        let (media_type, prepared_body) = body.into_prepared().expect("multipart body");
        let content_type = media_type.to_str().expect("multipart content type");
        let boundary = content_type
            .strip_prefix("multipart/form-data; boundary=")
            .expect("multipart boundary");
        assert!(content_type.starts_with("multipart/form-data; boundary="));

        let out = prepared_body
            .collect()
            .await
            .expect("multipart encoding")
            .to_bytes();
        let rendered = String::from_utf8(out.to_vec()).expect("multipart body should be utf-8");
        assert!(rendered.contains("Content-Disposition: form-data; name=\"title\""));
        assert!(rendered.contains("Content-Disposition: form-data; name=\"file\""));
        assert!(rendered.contains("abc"));
        assert!(rendered.contains("\r\n"));
        assert!(rendered.contains(&format!("--{boundary}")));
        assert!(rendered.ends_with(&format!("--{boundary}--\r\n")));
    }

    #[tokio::test]
    async fn multipart_replay_planning_keeps_size_unknown_and_uses_each_body_hint() {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let factory_calls = calls.clone();
        let mut prepared = MultipartReplayFactory::new(move || {
            factory_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(MultipartBody::new().text("field", "non-empty"))
        })
        .into_prepared_body()
        .expect("replay body");

        assert_eq!(prepared.size_hint().exact(), None);
        let first = prepared.produce_for_attempt().expect("first attempt");
        let first_body = first.into_dyn_body();
        assert_ne!(first_body.size_hint().exact(), Some(0));
        let first_bytes = first_body.collect().await.expect("first body").to_bytes();
        assert!(!first_bytes.is_empty());

        let second = prepared.produce_for_attempt().expect("second attempt");
        let second_body = second.into_dyn_body();
        assert_ne!(second_body.size_hint().exact(), Some(0));
        let second_bytes = second_body.collect().await.expect("second body").to_bytes();
        assert!(!second_bytes.is_empty());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn multipart_invalid_metadata_is_rejected_body_safely() {
        let body = MultipartBody::new().text("bad\r\nname", "value");
        let err = body.into_prepared().expect_err("invalid name should fail");
        assert_eq!(err.kind(), MultipartBodyErrorKind::InvalidPartName);
        assert!(!err.to_string().contains("bad\r\nname"));
    }

    #[tokio::test]
    async fn multipart_file_name_is_preserved_in_encoded_body() {
        let filename = "report.json";
        let body = MultipartBody::new().bytes_with(
            "upload",
            Some(filename.to_string()),
            Some(http::HeaderValue::from_static("application/json")),
            Bytes::from_static(b"{}"),
        );
        let (media_type, prepared_body) = body.into_prepared().expect("multipart body");
        let content_type = media_type.to_str().expect("multipart content type");
        let boundary = content_type
            .strip_prefix("multipart/form-data; boundary=")
            .expect("multipart boundary");
        let out = prepared_body
            .collect()
            .await
            .expect("multipart collection")
            .to_bytes();
        let rendered = String::from_utf8(out.to_vec()).expect("multipart body");
        assert!(rendered.contains(&format!("filename=\"{filename}\"")));
        assert!(rendered.contains("Content-Type: application/json"));
        assert!(rendered.contains(&format!("--{boundary}--")));
    }

    #[test]
    fn multipart_invalid_file_name_is_rejected_body_safely() {
        let err = MultipartBody::new()
            .bytes_with(
                "upload",
                Some("bad\\name".to_string()),
                Some(http::HeaderValue::from_static("application/json")),
                Bytes::from_static(b"{}"),
            )
            .into_prepared()
            .expect_err("invalid file name should fail");
        assert_eq!(err.kind(), MultipartBodyErrorKind::InvalidPartFileName);
        assert!(!err.to_string().contains("bad\\name"));
    }

    #[test]
    fn multipart_invalid_content_type_is_rejected_body_safely() {
        let err = MultipartBody::new()
            .bytes_with(
                "upload",
                None,
                Some(http::HeaderValue::from_static("not a mime")),
                Bytes::from_static(b"{}"),
            )
            .into_prepared()
            .expect_err("invalid part content type should fail");
        assert_eq!(err.kind(), MultipartBodyErrorKind::InvalidPartContentType);
        assert!(!err.to_string().contains("not a mime"));
    }

    struct ErrorStream {
        polled: bool,
    }

    impl Stream for ErrorStream {
        type Item = Result<Bytes, StreamBodyError>;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            if self.polled {
                Poll::Ready(None)
            } else {
                self.polled = true;
                Poll::Ready(Some(Err(StreamBodyError::io(io::Error::other(
                    "multipart stream producer failure",
                )))))
            }
        }
    }

    #[tokio::test]
    async fn multipart_stream_part_error_maps_to_sanitized_body_error() {
        let body = MultipartBody::new().stream(
            "upload",
            StreamBody::from_byte_stream(ErrorStream { polled: false }),
        );
        let (_media_type, prepared_body) = body.into_prepared().expect("multipart body");
        let err = prepared_body
            .collect()
            .await
            .expect_err("multipart stream failure");
        let rendered = format!("{err}");
        assert!(rendered.contains("body"));
        assert!(!rendered.contains("multipart stream producer failure"));
    }
}
