use std::fmt;
use std::num::NonZeroUsize;

use bytes::Bytes;

use super::prepared::{NativeBody, stream_native};
use super::{BodyError, BodyErrorKind, StreamBody};

/// Maximum number of parts retained by one multipart recipe.
pub const MAX_MULTIPART_PARTS: usize = 128;
/// Maximum byte length of one multipart part name.
pub const MAX_MULTIPART_NAME_BYTES: usize = 256;
/// Maximum byte length of one multipart filename.
pub const MAX_MULTIPART_FILENAME_BYTES: usize = 1_024;
/// Maximum byte length of one multipart part content type.
pub const MAX_MULTIPART_CONTENT_TYPE_BYTES: usize = 256;
/// Maximum aggregate byte length of names, filenames, and content types.
pub const MAX_MULTIPART_METADATA_BYTES: usize = 65_536;
/// Maximum byte length of one inline multipart text value.
pub const MAX_MULTIPART_INLINE_TEXT_BYTES: usize = 16_777_216;

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
    metadata_bytes: usize,
}

impl MultipartBody {
    /// Creates an empty multipart recipe.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            parts: Vec::new(),
            metadata_bytes: 0,
        }
    }

    /// Adds one validated named part.
    pub fn part(mut self, name: impl AsRef<str>, part: MultipartPart) -> Result<Self, BodyError> {
        self.push_part(name.as_ref(), part)?;
        Ok(self)
    }

    fn push_part(&mut self, name: &str, part: MultipartPart) -> Result<(), BodyError> {
        if self.parts.len() >= MAX_MULTIPART_PARTS {
            return Err(limit_exceeded());
        }
        validate_metadata_value(name, MAX_MULTIPART_NAME_BYTES)?;
        part.validate_structure()?;
        let metadata_bytes = self
            .metadata_bytes
            .checked_add(name.len())
            .and_then(|length| length.checked_add(part.metadata_bytes()))
            .ok_or_else(limit_exceeded)?;
        if metadata_bytes > MAX_MULTIPART_METADATA_BYTES {
            return Err(limit_exceeded());
        }
        self.parts.push((name.to_owned(), part));
        self.metadata_bytes = metadata_bytes;
        Ok(())
    }

    fn validate_structure(&self) -> Result<(), BodyError> {
        if self.parts.len() > MAX_MULTIPART_PARTS {
            return Err(limit_exceeded());
        }
        let mut metadata_bytes = 0_usize;
        for (name, part) in &self.parts {
            validate_metadata_value(name, MAX_MULTIPART_NAME_BYTES)?;
            part.validate_structure()?;
            metadata_bytes = metadata_bytes
                .checked_add(name.len())
                .and_then(|length| length.checked_add(part.metadata_bytes()))
                .ok_or_else(limit_exceeded)?;
            if metadata_bytes > MAX_MULTIPART_METADATA_BYTES {
                return Err(limit_exceeded());
            }
        }
        if metadata_bytes != self.metadata_bytes {
            return Err(BodyError::new(BodyErrorKind::Internal));
        }
        Ok(())
    }

    pub(crate) fn metadata(&self) -> Result<MultipartMetadata, BodyError> {
        self.validate_structure()?;
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

    pub(crate) fn clone_reusable(&self) -> Result<Option<Self>, BodyError> {
        self.validate_structure()?;
        Ok(self
            .parts
            .iter()
            .map(|(name, part)| Some((name.clone(), part.clone_reusable()?)))
            .collect::<Option<Vec<_>>>()
            .map(|parts| Self {
                parts,
                metadata_bytes: self.metadata_bytes,
            }))
    }

    pub(crate) fn into_form(
        self,
        limit: NonZeroUsize,
    ) -> Result<reqwest::multipart::Form, BodyError> {
        self.validate_structure()?;
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
    pub fn text(value: impl AsRef<str>) -> Result<Self, BodyError> {
        let value = value.as_ref();
        if value.len() > MAX_MULTIPART_INLINE_TEXT_BYTES {
            return Err(limit_exceeded());
        }
        Ok(Self {
            value: MultipartPartValue::Text(value.to_owned()),
            filename: None,
            content_type: None,
        })
    }

    /// Creates a reusable, already-owned bytes part.
    ///
    /// The active request-body limit is checked when the complete recipe is
    /// prepared. This constructor does not copy the supplied bytes.
    pub fn bytes(value: Bytes) -> Result<Self, BodyError> {
        u64::try_from(value.len()).map_err(|_| limit_exceeded())?;
        Ok(Self {
            value: MultipartPartValue::Bytes(value),
            filename: None,
            content_type: None,
        })
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
    pub fn file_name(mut self, filename: impl AsRef<str>) -> Result<Self, BodyError> {
        let filename = filename.as_ref();
        validate_metadata_value(filename, MAX_MULTIPART_FILENAME_BYTES)?;
        self.filename = Some(filename.to_owned());
        Ok(self)
    }

    /// Attaches a validated concrete media type to this part.
    pub fn content_type(mut self, media_type: impl AsRef<str>) -> Result<Self, BodyError> {
        let media_type = media_type.as_ref();
        if media_type.len() > MAX_MULTIPART_CONTENT_TYPE_BYTES {
            return Err(limit_exceeded());
        }
        super::prepared::validated_content_type(media_type)?;
        self.content_type = Some(media_type.to_owned());
        Ok(self)
    }

    fn metadata_bytes(&self) -> usize {
        self.filename.as_ref().map_or(0, String::len)
            + self.content_type.as_ref().map_or(0, String::len)
    }

    fn validate_structure(&self) -> Result<(), BodyError> {
        if let MultipartPartValue::Text(value) = &self.value {
            if value.len() > MAX_MULTIPART_INLINE_TEXT_BYTES {
                return Err(limit_exceeded());
            }
        }
        if let Some(filename) = &self.filename {
            validate_metadata_value(filename, MAX_MULTIPART_FILENAME_BYTES)?;
        }
        if let Some(content_type) = &self.content_type {
            if content_type.len() > MAX_MULTIPART_CONTENT_TYPE_BYTES {
                return Err(limit_exceeded());
            }
            super::prepared::validated_content_type(content_type)?;
        }
        Ok(())
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

fn validate_metadata_value(value: &str, limit: usize) -> Result<(), BodyError> {
    if value.len() > limit {
        return Err(limit_exceeded());
    }
    if value.is_empty()
        || value.chars().any(|character| {
            character == '\r' || character == '\n' || character == '\0' || character.is_control()
        })
    {
        return Err(BodyError::new(BodyErrorKind::Multipart));
    }
    Ok(())
}

fn limit_exceeded() -> BodyError {
    BodyError::new(BodyErrorKind::LimitExceeded)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct InvalidMultipartFactory;

    impl MultipartFactory for InvalidMultipartFactory {
        fn create(&self) -> Result<MultipartBody, BodyError> {
            let name = format!(
                "{}factory-name-sentinel",
                "n".repeat(MAX_MULTIPART_NAME_BYTES)
            );
            Ok(MultipartBody {
                metadata_bytes: name.len(),
                parts: vec![(
                    name,
                    MultipartPart::text("factory-value").expect("bounded factory text"),
                )],
            })
        }
    }

    fn text(value: &str) -> MultipartPart {
        MultipartPart::text(value).expect("bounded multipart text")
    }

    #[test]
    fn validation_and_debug_are_structural_and_redacted() {
        let secret = "multipart-value-sentinel";
        let filename = "filename-sentinel.txt";
        let part = MultipartPart::text(secret)
            .expect("bounded text")
            .file_name(filename)
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

        assert!(MultipartBody::new().part("bad\r\nname", text("x")).is_err());
        assert!(
            MultipartPart::bytes(Bytes::new())
                .expect("bounded bytes")
                .file_name("bad\nfile")
                .is_err()
        );
        assert!(
            MultipartPart::bytes(Bytes::new())
                .expect("bounded bytes")
                .content_type("*/json")
                .is_err()
        );
    }

    #[test]
    fn exact_part_count_and_first_excess_part_is_not_retained() {
        let mut body = MultipartBody::new();
        for _ in 0..MAX_MULTIPART_PARTS {
            body.push_part("p", text("x")).expect("part-count limit");
        }
        assert_eq!(body.parts.len(), MAX_MULTIPART_PARTS);

        let error = body
            .push_part("first-excess-part-sentinel", text("not-retained"))
            .expect_err("first excess part");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
        assert_eq!(body.parts.len(), MAX_MULTIPART_PARTS);
        assert!(!format!("{error:?}").contains("first-excess-part-sentinel"));
    }

    #[test]
    fn name_filename_and_content_type_have_exact_borrowed_limits() {
        let exact_name = "n".repeat(MAX_MULTIPART_NAME_BYTES);
        MultipartBody::new()
            .part(exact_name.as_str(), text("x"))
            .expect("exact name limit");
        let excessive_name = format!("{exact_name}name-sentinel");
        let error = MultipartBody::new()
            .part(excessive_name.as_str(), text("x"))
            .expect_err("name first excess");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
        assert!(!format!("{error:?}").contains("name-sentinel"));

        let exact_filename = "f".repeat(MAX_MULTIPART_FILENAME_BYTES);
        text("x")
            .file_name(exact_filename.as_str())
            .expect("exact filename limit");
        let excessive_filename = format!("{exact_filename}filename-sentinel");
        let error = text("x")
            .file_name(excessive_filename.as_str())
            .expect_err("filename first excess");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
        assert!(!format!("{error:?}").contains("filename-sentinel"));

        let exact_content_type = format!(
            "application/{}",
            "a".repeat(MAX_MULTIPART_CONTENT_TYPE_BYTES - "application/".len())
        );
        assert_eq!(exact_content_type.len(), MAX_MULTIPART_CONTENT_TYPE_BYTES);
        text("x")
            .content_type(exact_content_type.as_str())
            .expect("exact content-type limit");
        let excessive_content_type = format!("{exact_content_type}a");
        let error = text("x")
            .content_type(excessive_content_type.as_str())
            .expect_err("content-type first excess");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    }

    #[test]
    fn aggregate_metadata_exact_limit_and_first_excess_are_incremental() {
        let name = "n".repeat(MAX_MULTIPART_NAME_BYTES);
        let filename = "f".repeat(MAX_MULTIPART_FILENAME_BYTES);
        let mut body = MultipartBody::new();
        for _ in 0..51 {
            body.push_part(
                name.as_str(),
                text("").file_name(filename.as_str()).expect("filename"),
            )
            .expect("aggregate metadata below limit");
        }
        body.push_part(name.as_str(), text(""))
            .expect("exact aggregate metadata limit");
        assert_eq!(body.metadata_bytes, MAX_MULTIPART_METADATA_BYTES);
        let part_count = body.parts.len();

        let error = body
            .push_part("x", text("first-excess-metadata-sentinel"))
            .expect_err("aggregate metadata first excess");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
        assert_eq!(body.metadata_bytes, MAX_MULTIPART_METADATA_BYTES);
        assert_eq!(body.parts.len(), part_count);
        assert!(!format!("{error:?}").contains("first-excess-metadata-sentinel"));
    }

    #[test]
    fn inline_text_exact_limit_and_first_excess_are_redacted() {
        let exact = "x".repeat(MAX_MULTIPART_INLINE_TEXT_BYTES);
        let part = MultipartPart::text(exact.as_str()).expect("exact inline-text limit");
        assert_eq!(
            part.lower_bound().expect("text lower bound"),
            MAX_MULTIPART_INLINE_TEXT_BYTES as u64
        );
        drop(part);
        drop(exact);

        let excessive = "x".repeat(MAX_MULTIPART_INLINE_TEXT_BYTES + 1);
        let error = MultipartPart::text(excessive.as_str()).expect_err("inline-text first excess");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
        assert!(!format!("{error:?}").contains("xxxx"));
    }

    #[test]
    fn factory_output_is_revalidated_before_native_construction() {
        let error = super::super::RequestBody::multipart_factory(InvalidMultipartFactory)
            .into_prepared(NonZeroUsize::new(1_024).expect("positive limit"))
            .err()
            .expect("invalid factory recipe");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
        assert!(!format!("{error:?}").contains("factory-name-sentinel"));
    }

    #[test]
    fn replayability_and_payload_lower_bound_follow_parts() {
        let buffered = MultipartBody::new()
            .part("text", text("four"))
            .expect("part")
            .part(
                "bytes",
                MultipartPart::bytes(Bytes::from_static(b"three")).expect("bounded bytes"),
            )
            .expect("part");
        let metadata = buffered.metadata().expect("metadata");
        assert!(metadata.reusable);
        assert_eq!(metadata.lower_bound, 9);
        assert!(
            buffered
                .clone_reusable()
                .expect("valid reusable recipe")
                .is_some()
        );

        let streamed = MultipartBody::new()
            .part(
                "stream",
                MultipartPart::stream(StreamBody::from_bytes(Bytes::from_static(b"data"))),
            )
            .expect("part");
        let metadata = streamed.metadata().expect("metadata");
        assert!(!metadata.reusable);
        assert_eq!(metadata.lower_bound, 4);
        assert!(
            streamed
                .clone_reusable()
                .expect("valid streaming recipe")
                .is_none()
        );
    }
}
