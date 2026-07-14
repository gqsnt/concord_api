use crate::body::BodyError;
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

    fn clone_if_reconstructible(&self) -> Option<Self> {
        match self {
            Self::Text(value) => Some(Self::Text(value.clone())),
            Self::Bytes(value) => Some(Self::Bytes(value.clone())),
            Self::Stream(_) => None,
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

    pub(crate) fn is_reconstructible(&self) -> bool {
        self.parts
            .iter()
            .all(|part| !matches!(&part.body, RawPartBody::Stream(_)))
    }

    pub(crate) fn clone_if_reconstructible(&self) -> Option<Self> {
        let mut parts = Vec::with_capacity(self.parts.len());
        for part in &self.parts {
            parts.push(RawPart {
                name: part.name.clone(),
                file_name: part.file_name.clone(),
                content_type: part.content_type.clone(),
                body: part.body.clone_if_reconstructible()?,
            });
        }
        Some(Self { parts })
    }

    /// Validates recipe metadata without constructing or consuming part bodies.
    pub(crate) fn validate(&self) -> Result<(), MultipartBodyError> {
        for part in &self.parts {
            validate_part_name(&part.name)?;
            if let Some(file_name) = &part.file_name {
                validate_part_file_name(file_name)?;
            }
            if let Some(content_type) = &part.content_type {
                validate_part_content_type(content_type)?;
            }
        }
        Ok(())
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
        let factory = self.factory;
        // Each factory call supplies a wholly fresh recipe, never a flattened
        // form or a partial replacement for a consumed streamed part.
        Ok(crate::io::PreparedBody::multipart_factory(move || {
            factory()
        }))
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
        // Reqwest's `Part::bytes` requires an owned `'static` byte cow and
        // cannot accept `Bytes` without copying. Preserve the existing shared
        // immutable buffer through Reqwest's native Body instead.
        RawPartBody::Bytes(bytes) => Part::stream(reqwest::Body::from(bytes)),
        RawPartBody::Stream(stream) => {
            let exact_length = stream.size_hint().exact();
            let body = reqwest::Body::wrap_stream(MultipartByteStream::new(
                stream.into_byte_stream(),
                exact_length,
            ));
            match exact_length {
                Some(length) => Part::stream_with_length(body, length),
                None => Part::stream(body),
            }
        }
    };

    if let Some(file_name) = part.file_name {
        validate_part_file_name(&file_name)?;
        body = body.file_name(file_name);
    }

    if let Some(content_type) = part.content_type {
        let content_type = validate_part_content_type(&content_type)?;
        body = body
            .mime_str(content_type)
            .map_err(|_| MultipartBodyError::new(MultipartBodyErrorKind::InvalidPartContentType))?;
    }

    Ok((name, body))
}

/// Native byte-stream adapter for ordinary multipart streamed parts.
///
/// This deliberately keeps the source as a native byte stream. It is also the
/// structural exact-length authority for a part:
/// the declared length is only a hint until the producer reaches EOF, and an
/// overrun is rejected before the excess bytes can reach Reqwest.
struct MultipartByteStream {
    inner: crate::stream_body::StreamByteSource,
    expected: Option<u64>,
    seen: u64,
    terminal: bool,
}

impl MultipartByteStream {
    fn new(inner: crate::stream_body::StreamByteSource, expected: Option<u64>) -> Self {
        Self {
            inner,
            expected,
            seen: 0,
            terminal: false,
        }
    }
}

impl Stream for MultipartByteStream {
    type Item = Result<Bytes, BodyError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.terminal {
            return Poll::Ready(None);
        }

        match self.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                let observed = self.seen.saturating_add(len);
                if let Some(expected) = self.expected
                    && observed > expected
                {
                    self.terminal = true;
                    return Poll::Ready(Some(Err(BodyError::exact_length_overflow(
                        expected, observed,
                    ))));
                }
                self.seen = observed;
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(error))) => {
                self.terminal = true;
                Poll::Ready(Some(Err(error.into())))
            }
            Poll::Ready(None) => {
                self.terminal = true;
                if let Some(expected) = self.expected
                    && self.seen != expected
                {
                    return Poll::Ready(Some(Err(BodyError::exact_length_underflow(
                        expected, self.seen,
                    ))));
                }
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Applies Reqwest's multipart MIME acceptance rule without materializing any
/// recipe part or form. The empty probe is discarded immediately; it neither
/// observes nor consumes the caller's body. Keep this as the sole validator so
/// preparation and defensive native construction cannot drift apart.
fn validate_part_content_type(content_type: &HeaderValue) -> Result<&str, MultipartBodyError> {
    let content_type = content_type
        .to_str()
        .map_err(|_| MultipartBodyError::new(MultipartBodyErrorKind::InvalidPartContentType))?;
    Part::text("")
        .mime_str(content_type)
        .map(|_| content_type)
        .map_err(|_| MultipartBodyError::new(MultipartBodyErrorKind::InvalidPartContentType))
}

impl MultipartBody {
    pub(crate) fn into_form(self) -> Result<Form, MultipartBodyError> {
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
    use http_body::SizeHint;
    use std::collections::VecDeque;
    use std::task::{RawWaker, RawWakerVTable, Waker};

    struct TestStream {
        items: VecDeque<Result<Bytes, crate::stream_body::StreamBodyError>>,
    }

    impl Stream for TestStream {
        type Item = Result<Bytes, crate::stream_body::StreamBodyError>;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.items.pop_front())
        }
    }

    fn noop_waker() -> Waker {
        // SAFETY: all vtable functions are no-ops and the data pointer is
        // never dereferenced.
        unsafe fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        unsafe fn wake(_: *const ()) {}
        unsafe fn wake_by_ref(_: *const ()) {}
        unsafe fn drop(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
        // SAFETY: the static vtable and null data pointer satisfy RawWaker's
        // ownership contract for this synchronous test poll.
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
    }

    fn poll_next_now(body: &mut MultipartByteStream) -> Option<Result<Bytes, BodyError>> {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        match Pin::new(body).poll_next(&mut cx) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("test stream unexpectedly pending"),
        }
    }

    fn exact_stream(
        items: Vec<Result<Bytes, crate::stream_body::StreamBodyError>>,
        expected: u64,
    ) -> MultipartByteStream {
        let body = StreamBody::from_byte_stream(TestStream {
            items: items.into(),
        })
        .with_size_hint(SizeHint::with_exact(expected));
        MultipartByteStream::new(body.into_byte_stream(), Some(expected))
    }

    #[test]
    fn direct_stream_adapter_accepts_exact_length() {
        let mut body = exact_stream(vec![Ok(Bytes::from_static(b"ab"))], 2);
        assert_eq!(
            poll_next_now(&mut body).unwrap().unwrap(),
            Bytes::from_static(b"ab")
        );
        assert!(poll_next_now(&mut body).is_none());
    }

    #[test]
    fn direct_stream_adapter_rejects_underflow_and_overflow() {
        let mut underflow = exact_stream(vec![Ok(Bytes::from_static(b"a"))], 2);
        assert!(poll_next_now(&mut underflow).unwrap().is_ok());
        let error = poll_next_now(&mut underflow).unwrap().unwrap_err();
        assert_eq!(
            error.kind(),
            crate::body::BodyErrorKind::ExactLengthUnderflow
        );
        assert!(poll_next_now(&mut underflow).is_none());

        let mut overflow = exact_stream(vec![Ok(Bytes::from_static(b"abc"))], 2);
        let error = poll_next_now(&mut overflow).unwrap().unwrap_err();
        assert_eq!(
            error.kind(),
            crate::body::BodyErrorKind::ExactLengthOverflow
        );
        assert!(poll_next_now(&mut overflow).is_none());
    }

    #[test]
    fn direct_stream_adapter_sanitizes_producer_failures() {
        let mut body = exact_stream(
            vec![Err(crate::stream_body::StreamBodyError::producer(
                std::io::Error::other("multipart producer sentinel"),
            ))],
            0,
        );
        let error = poll_next_now(&mut body).unwrap().unwrap_err();
        assert_eq!(error.kind(), crate::body::BodyErrorKind::Input);
        assert!(!error.to_string().contains("multipart producer sentinel"));
        assert!(poll_next_now(&mut body).is_none());
    }
}
