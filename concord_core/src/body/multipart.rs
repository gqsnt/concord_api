use std::fmt;
use std::num::NonZeroUsize;

use bytes::Bytes;

use super::prepared::{NativeBody, stream_native};
use super::{BodyError, BodyErrorKind, StreamBody};

/// Creates a fresh complete multipart recipe for one Concord attempt.
///
/// `create` is called only after `before_send` approves the attempt. A factory
/// implementation must return a new complete recipe on every call.
///
/// Calls for independent operations may overlap and occur only after the
/// before-send hook approves an attempt. Multipart values and factory state may
/// be sensitive; implementations own logging and storage and must bound CPU,
/// memory, retries, maps, queues, spawned tasks, metric cardinality, and external
/// I/O. The synchronous callback cannot be preempted by Concord; the operation
/// deadline is checked after return, cancellation prevents later work only, and
/// Concord does not automatically bound custom factory state.
pub trait MultipartFactory: Send + Sync + 'static {
    /// Creates one fresh multipart recipe suitable for a single Concord attempt.
    fn create(&self) -> Result<MultipartBody, BodyError>;
}

/// An owned request-side multipart recipe.
///
/// Serialization and boundary generation are deferred until native request
/// construction and are owned by the private HTTP implementation.
pub struct MultipartBody {
    parts: Vec<(String, MultipartPart)>,
}

impl MultipartBody {
    /// Creates an empty multipart recipe.
    #[must_use]
    pub const fn new() -> Self {
        Self { parts: Vec::new() }
    }

    /// Adds one validated named part.
    pub fn part(mut self, name: impl Into<String>, part: MultipartPart) -> Result<Self, BodyError> {
        let name = name.into();
        validate_disposition_value(&name)?;
        self.parts.push((name, part));
        Ok(self)
    }

    pub(crate) fn metadata(&self) -> Result<MultipartMetadata, BodyError> {
        let mut lower_bound = 0_u64;
        let mut reusable = true;
        for (_, part) in &self.parts {
            reusable &= part.is_reusable();
            lower_bound = lower_bound
                .checked_add(part.lower_bound()?)
                .ok_or_else(|| BodyError::new(BodyErrorKind::LimitExceeded))?;
        }
        Ok(MultipartMetadata {
            reusable,
            lower_bound,
        })
    }

    pub(crate) fn clone_reusable(&self) -> Option<Self> {
        self.parts
            .iter()
            .map(|(name, part)| Some((name.clone(), part.clone_reusable()?)))
            .collect::<Option<Vec<_>>>()
            .map(|parts| Self { parts })
    }

    pub(crate) fn into_form(
        self,
        limit: NonZeroUsize,
    ) -> Result<reqwest::multipart::Form, BodyError> {
        let mut form = reqwest::multipart::Form::new();
        for (name, part) in self.parts {
            form = form.part(name, part.into_native(limit)?);
        }
        Ok(form)
    }
}

impl Default for MultipartBody {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MultipartBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MultipartBody")
            .field("part_count", &self.parts.len())
            .field("parts", &"<multipart values omitted>")
            .finish()
    }
}

/// One opaque part in a request-side multipart recipe.
pub struct MultipartPart {
    value: MultipartPartValue,
    filename: Option<String>,
    content_type: Option<String>,
}

enum MultipartPartValue {
    Text(String),
    Bytes(Bytes),
    Stream(StreamBody),
}

impl MultipartPart {
    /// Creates a reusable text part.
    #[must_use]
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            value: MultipartPartValue::Text(value.into()),
            filename: None,
            content_type: None,
        }
    }

    /// Creates a reusable bytes part.
    #[must_use]
    pub const fn bytes(value: Bytes) -> Self {
        Self {
            value: MultipartPartValue::Bytes(value),
            filename: None,
            content_type: None,
        }
    }

    /// Creates a one-shot streaming part.
    #[must_use]
    pub const fn stream(value: StreamBody) -> Self {
        Self {
            value: MultipartPartValue::Stream(value),
            filename: None,
            content_type: None,
        }
    }

    /// Attaches a validated filename without exposing it through diagnostics.
    pub fn filename(mut self, filename: impl Into<String>) -> Result<Self, BodyError> {
        let filename = filename.into();
        validate_disposition_value(&filename)?;
        self.filename = Some(filename);
        Ok(self)
    }

    /// Attaches a validated concrete media type to this part.
    pub fn content_type(mut self, media_type: impl Into<String>) -> Result<Self, BodyError> {
        let media_type = media_type.into();
        super::prepared::validated_content_type(&media_type)?;
        self.content_type = Some(media_type);
        Ok(self)
    }

    fn is_reusable(&self) -> bool {
        !matches!(self.value, MultipartPartValue::Stream(_))
    }

    fn lower_bound(&self) -> Result<u64, BodyError> {
        match &self.value {
            MultipartPartValue::Text(value) => Ok(u64::try_from(value.len()).unwrap_or(u64::MAX)),
            MultipartPartValue::Bytes(value) => Ok(u64::try_from(value.len()).unwrap_or(u64::MAX)),
            MultipartPartValue::Stream(value) => {
                value.validate_metadata()?;
                Ok(value.lower_bound().unwrap_or(0))
            }
        }
    }

    fn clone_reusable(&self) -> Option<Self> {
        let value = match &self.value {
            MultipartPartValue::Text(value) => MultipartPartValue::Text(value.clone()),
            MultipartPartValue::Bytes(value) => MultipartPartValue::Bytes(value.clone()),
            MultipartPartValue::Stream(_) => return None,
        };
        Some(Self {
            value,
            filename: self.filename.clone(),
            content_type: self.content_type.clone(),
        })
    }

    fn into_native(self, limit: NonZeroUsize) -> Result<reqwest::multipart::Part, BodyError> {
        let mut part = match self.value {
            MultipartPartValue::Text(value) => reqwest::multipart::Part::text(value),
            MultipartPartValue::Bytes(value) => {
                let length = u64::try_from(value.len()).unwrap_or(u64::MAX);
                reqwest::multipart::Part::stream_with_length(reqwest::Body::from(value), length)
            }
            MultipartPartValue::Stream(value) => {
                value.validate_metadata()?;
                let exact_length = value.exact_length_value();
                let NativeBody::Stream {
                    body, known_length, ..
                } = stream_native(value, exact_length, limit)
                else {
                    unreachable!("stream materialization always creates a native stream")
                };
                match known_length {
                    Some(length) => reqwest::multipart::Part::stream_with_length(body, length),
                    None => reqwest::multipart::Part::stream(body),
                }
            }
        };
        if let Some(filename) = self.filename {
            part = part.file_name(filename);
        }
        if let Some(content_type) = self.content_type {
            part = part.mime_str(&content_type).map_err(|error| {
                drop(error);
                BodyError::new(BodyErrorKind::InvalidContentType)
            })?;
        }
        Ok(part)
    }
}

impl fmt::Debug for MultipartPart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.value {
            MultipartPartValue::Text(_) => "text",
            MultipartPartValue::Bytes(_) => "bytes",
            MultipartPartValue::Stream(_) => "stream",
        };
        formatter
            .debug_struct("MultipartPart")
            .field("kind", &kind)
            .field("has_filename", &self.filename.is_some())
            .field("has_content_type", &self.content_type.is_some())
            .field("value", &"<multipart value omitted>")
            .finish()
    }
}

pub(crate) struct MultipartMetadata {
    pub(crate) reusable: bool,
    pub(crate) lower_bound: u64,
}

fn validate_disposition_value(value: &str) -> Result<(), BodyError> {
    if value.is_empty()
        || value.chars().any(|character| {
            character == '\r' || character == '\n' || character == '\0' || character.is_control()
        })
    {
        return Err(BodyError::new(BodyErrorKind::Multipart));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_and_debug_are_structural_and_redacted() {
        let secret = "multipart-value-sentinel";
        let filename = "filename-sentinel.txt";
        let part = MultipartPart::text(secret)
            .filename(filename)
            .expect("valid filename")
            .content_type("text/plain; charset=utf-8")
            .expect("valid media type");
        let part_debug = format!("{part:?}");
        assert!(!part_debug.contains(secret));
        assert!(!part_debug.contains(filename));

        let body = MultipartBody::new()
            .part("field", part)
            .expect("valid part");
        let body_debug = format!("{body:?}");
        assert!(!body_debug.contains(secret));
        assert!(!body_debug.contains(filename));

        assert!(
            MultipartBody::new()
                .part("bad\r\nname", MultipartPart::text("x"))
                .is_err()
        );
        assert!(
            MultipartPart::bytes(Bytes::new())
                .filename("bad\nfile")
                .is_err()
        );
        assert!(
            MultipartPart::bytes(Bytes::new())
                .content_type("*/json")
                .is_err()
        );
    }

    #[test]
    fn replayability_and_payload_lower_bound_follow_parts() {
        let buffered = MultipartBody::new()
            .part("text", MultipartPart::text("four"))
            .expect("part")
            .part("bytes", MultipartPart::bytes(Bytes::from_static(b"three")))
            .expect("part");
        let metadata = buffered.metadata().expect("metadata");
        assert!(metadata.reusable);
        assert_eq!(metadata.lower_bound, 9);
        assert!(buffered.clone_reusable().is_some());

        let streamed = MultipartBody::new()
            .part(
                "stream",
                MultipartPart::stream(StreamBody::from_bytes(Bytes::from_static(b"data"))),
            )
            .expect("part");
        let metadata = streamed.metadata().expect("metadata");
        assert!(!metadata.reusable);
        assert_eq!(metadata.lower_bound, 4);
        assert!(streamed.clone_reusable().is_none());
    }
}
