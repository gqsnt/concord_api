use crate::body::DynBody;
use crate::stream_body::StreamBody;
use bytes::Bytes;
use http::HeaderValue;
use reqwest::multipart::{Form, Part};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

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
        let content_type = validate_part_content_type(&content_type)?;
        body = body
            .mime_str(content_type)
            .map_err(|_| MultipartBodyError::new(MultipartBodyErrorKind::InvalidPartContentType))?;
    }

    Ok((name, body))
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
