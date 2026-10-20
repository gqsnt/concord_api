use std::collections::VecDeque;
use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use http::HeaderValue;

use super::spec::LengthHint;
use super::stream::BodyStream;
use super::{BodyError, BodyErrorKind, RequestStream};

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

/// Attempts made to find a boundary absent from every buffered part.
const BOUNDARY_ATTEMPTS: usize = 4;
/// RFC 2046 maximum boundary length.
const MAX_BOUNDARY_BYTES: usize = 70;

/// Creates a fresh complete multipart recipe for one Concord attempt.
///
/// `create` is called once when the prepared body is materialized (before
/// credential acquisition) and once for each later admitted application
/// attempt. A factory implementation must return a new complete recipe on
/// every call. The multipart boundary is generated once by
/// [`PreparedBody::multipart_factory`](super::PreparedBody::multipart_factory)
/// and reused for every recipe, so the request `Content-Type` never changes
/// between attempts.
///
/// # Extension contract
///
/// * **Ownership.** The factory is moved into the prepared body and shared by
///   its attempts; each returned recipe is owned by core for one attempt.
/// * **Concurrency.** Calls for independent operations may overlap.
/// * **Cancellation.** The synchronous callback cannot be preempted; the
///   operation deadline is checked after it returns and cancellation prevents
///   later work only.
/// * **Input sensitivity.** Multipart values and factory state may be
///   sensitive; implementations own logging and storage. Diagnostics never
///   print part values, names or filenames.
/// * **Errors.** Return a [`BodyError`]; core reduces it to
///   [`BodyErrorKind::Factory`] and fails the operation without sending that
///   attempt.
/// * **Resource obligations.** Implementations must bound CPU, memory,
///   retries, maps, queues, spawned tasks, metric cardinality and external
///   I/O; every recipe is revalidated structurally and against the body
///   limit.
/// * **Negative capabilities.** A factory cannot choose the boundary or the
///   top-level media type, cannot observe credentials or the native request,
///   and cannot grant an extra attempt.
pub trait MultipartFactory: Send + Sync + 'static {
    /// Creates one fresh multipart recipe suitable for a single Concord attempt.
    fn create(&self) -> Result<MultipartBody, BodyError>;
}

/// An owned request-side multipart recipe.
///
/// Serialization is owned by core: once the recipe becomes a
/// [`PreparedBody`](super::PreparedBody), core generates one boundary,
/// validates it and serializes every attempt with that same boundary and the
/// exact `multipart/form-data; boundary=...` media type. Diagnostics never
/// print part values, names or filenames.
pub struct MultipartBody {
    parts: Vec<(String, MultipartPart)>,
    metadata_bytes: usize,
}

impl MultipartBody {
    /// Creates an empty multipart recipe.
    ///
    /// A recipe with no parts still serializes the closing boundary. Its
    /// complete framed length counts against the request-body limit.
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

    /// Computes the exact framed length (when every part length is known), the
    /// framed minimum and the replay capability for one boundary.
    pub(crate) fn framing(
        &self,
        boundary: &MultipartBoundary,
    ) -> Result<MultipartFraming, BodyError> {
        self.validate_structure()?;
        let boundary_len = saturating_u64(boundary.as_str().len());
        let mut exact = Some(0_u64);
        let mut minimum = 0_u64;
        let mut reusable = true;
        for (name, part) in &self.parts {
            let header = part_header(name, part);
            // "--" boundary CRLF header CRLF CRLF value CRLF
            let framing = 2_u64
                .checked_add(boundary_len)
                .and_then(|length| length.checked_add(2))
                .and_then(|length| length.checked_add(saturating_u64(header.len())))
                .and_then(|length| length.checked_add(4))
                .and_then(|length| length.checked_add(2))
                .ok_or_else(limit_exceeded)?;
            let (value_exact, value_minimum) = part.value_length()?;
            reusable &= part.is_reusable();
            minimum = minimum
                .checked_add(framing)
                .and_then(|length| length.checked_add(value_minimum))
                .ok_or_else(limit_exceeded)?;
            exact = match (exact, value_exact) {
                (Some(total), Some(value)) => Some(
                    total
                        .checked_add(framing)
                        .and_then(|length| length.checked_add(value))
                        .ok_or_else(limit_exceeded)?,
                ),
                _ => None,
            };
        }
        // "--" boundary "--" CRLF, including a recipe with no parts.
        let closing = 2_u64
            .checked_add(boundary_len)
            .and_then(|length| length.checked_add(4))
            .ok_or_else(limit_exceeded)?;
        minimum = minimum.checked_add(closing).ok_or_else(limit_exceeded)?;
        exact = exact
            .map(|total| total.checked_add(closing).ok_or_else(limit_exceeded))
            .transpose()?;
        Ok(MultipartFraming {
            exact,
            minimum,
            reusable,
        })
    }

    /// Returns whether any retained buffered part contains the delimiter.
    pub(crate) fn contains_delimiter(&self, boundary: &MultipartBoundary) -> bool {
        let delimiter = format!("--{}", boundary.as_str());
        self.parts.iter().any(|(_, part)| match &part.value {
            MultipartPartValue::Text(value) => contains(value.as_bytes(), delimiter.as_bytes()),
            MultipartPartValue::Bytes(value) => contains(value, delimiter.as_bytes()),
            MultipartPartValue::Stream(_) => false,
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

    /// Serializes this recipe with the retained boundary as one lazy stream.
    ///
    /// Streaming parts are not polled until native transport polls the
    /// returned stream. Every streaming part's own exact length or declared
    /// minimum is enforced at its part boundary.
    pub(crate) fn into_stream(self, boundary: &MultipartBoundary) -> Result<BodyStream, BodyError> {
        self.validate_structure()?;
        let mut segments = VecDeque::with_capacity(self.parts.len().saturating_mul(3) + 1);
        for (name, part) in self.parts {
            let header = part_header(&name, &part);
            let mut prefix = Vec::with_capacity(boundary.as_str().len() + header.len() + 8);
            prefix.extend_from_slice(b"--");
            prefix.extend_from_slice(boundary.as_str().as_bytes());
            prefix.extend_from_slice(b"\r\n");
            prefix.extend_from_slice(&header);
            prefix.extend_from_slice(b"\r\n\r\n");
            segments.push_back(Segment::Static(Bytes::from(prefix)));
            match part.value {
                MultipartPartValue::Text(value) => {
                    segments.push_back(Segment::Static(Bytes::from(value)));
                }
                MultipartPartValue::Bytes(value) => segments.push_back(Segment::Static(value)),
                MultipartPartValue::Stream(value) => {
                    value.validate_metadata()?;
                    let exact = value.exact_length_value();
                    let minimum = value.lower_bound();
                    segments.push_back(Segment::Stream(PartStream {
                        source: value.into_source(),
                        exact,
                        minimum,
                        observed: 0,
                    }));
                }
            }
            segments.push_back(Segment::Static(Bytes::from_static(b"\r\n")));
        }
        let mut closing = Vec::with_capacity(boundary.as_str().len() + 6);
        closing.extend_from_slice(b"--");
        closing.extend_from_slice(boundary.as_str().as_bytes());
        closing.extend_from_slice(b"--\r\n");
        segments.push_back(Segment::Static(Bytes::from(closing)));
        Ok(Box::pin(MultipartStream {
            segments,
            finished: false,
        }))
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

/// Framed length and replay facts of one multipart recipe and boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MultipartFraming {
    exact: Option<u64>,
    minimum: u64,
    reusable: bool,
}

impl MultipartFraming {
    pub(crate) const fn length_hint(self) -> LengthHint {
        match self.exact {
            Some(exact) => LengthHint::Exact(exact),
            None => LengthHint::AtLeast(self.minimum),
        }
    }

    pub(crate) const fn bounds(self) -> (Option<u64>, u64) {
        (self.exact, self.minimum)
    }

    pub(crate) const fn reusable(self) -> bool {
        self.reusable
    }
}

/// One validated multipart boundary generated during preparation.
#[derive(Clone)]
pub(crate) struct MultipartBoundary(Arc<str>);

impl MultipartBoundary {
    /// Generates one fresh random boundary.
    pub(crate) fn generate() -> Self {
        #[cfg(test)]
        GENERATED_BOUNDARIES.with(|count| count.set(count.get() + 1));
        let value = format!("concord-{:016x}{:016x}", random_u64(), random_u64());
        debug_assert!(valid_boundary(&value));
        Self(Arc::from(value))
    }

    /// Generates a boundary absent from every retained buffered part.
    pub(crate) fn generate_for(body: &MultipartBody) -> Result<Self, BodyError> {
        body.validate_structure()?;
        for _ in 0..BOUNDARY_ATTEMPTS {
            let boundary = Self::generate();
            if !body.contains_delimiter(&boundary) {
                return Ok(boundary);
            }
        }
        Err(BodyError::new(BodyErrorKind::Multipart))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the exact generated top-level media type.
    pub(crate) fn content_type(&self) -> Result<HeaderValue, BodyError> {
        if !valid_boundary(self.as_str()) {
            return Err(BodyError::new(BodyErrorKind::Multipart));
        }
        HeaderValue::from_str(&format!("multipart/form-data; boundary={}", self.as_str()))
            .map_err(|_| BodyError::new(BodyErrorKind::Multipart))
    }
}

impl fmt::Debug for MultipartBoundary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MultipartBoundary(<generated>)")
    }
}

#[cfg(test)]
thread_local! {
    static GENERATED_BOUNDARIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Returns how many boundaries the current test thread generated.
#[cfg(test)]
pub(crate) fn generated_boundaries_on_this_thread() -> usize {
    GENERATED_BOUNDARIES.with(std::cell::Cell::get)
}

fn random_u64() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(sequence);
    hasher.finish()
}

fn valid_boundary(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_BOUNDARY_BYTES
        && !value.ends_with(' ')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'\''
                        | b'('
                        | b')'
                        | b'+'
                        | b'_'
                        | b','
                        | b'-'
                        | b'.'
                        | b'/'
                        | b':'
                        | b'='
                        | b'?'
                        | b' '
                )
        })
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
    Stream(RequestStream),
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
    ///
    /// A recipe containing a streaming part is never automatically replayed.
    #[must_use]
    pub const fn stream(value: RequestStream) -> Self {
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
        if let MultipartPartValue::Stream(value) = &self.value {
            value.validate_metadata()?;
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

    /// Returns the exact value length, when known, and the value minimum.
    fn value_length(&self) -> Result<(Option<u64>, u64), BodyError> {
        match &self.value {
            MultipartPartValue::Text(value) => {
                let length = saturating_u64(value.len());
                Ok((Some(length), length))
            }
            MultipartPartValue::Bytes(value) => {
                let length = saturating_u64(value.len());
                Ok((Some(length), length))
            }
            MultipartPartValue::Stream(value) => {
                value.validate_metadata()?;
                Ok((value.exact_length_value(), value.lower_bound().unwrap_or(0)))
            }
        }
    }

    #[cfg(test)]
    fn lower_bound(&self) -> Result<u64, BodyError> {
        self.value_length().map(|(_, minimum)| minimum)
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

/// Encodes one part's header lines (without the terminating blank line).
///
/// Field names and filenames use quoted parameters with escaped backslash
/// and quote characters. Non-ASCII text is UTF-8, as allowed for form-data by
/// RFC 7578 section 5.1. RFC 8187's extended parameter encoding does not apply
/// to multipart/form-data and path-segment escaping cannot preserve names.
/// Metadata was validated to exclude CR, LF, NUL and other controls.
fn part_header(name: &str, part: &MultipartPart) -> Vec<u8> {
    let mut header = Vec::with_capacity(64 + name.len());
    header.extend_from_slice(b"Content-Disposition: form-data; name=");
    quoted_parameter(&mut header, name);
    if let Some(filename) = &part.filename {
        header.extend_from_slice(b"; filename=");
        quoted_parameter(&mut header, filename);
    }
    if let Some(content_type) = &part.content_type {
        header.extend_from_slice(b"\r\nContent-Type: ");
        header.extend_from_slice(content_type.as_bytes());
    }
    header
}

fn quoted_parameter(output: &mut Vec<u8>, value: &str) {
    output.push(b'"');
    for byte in value.bytes() {
        if matches!(byte, b'\\' | b'"') {
            output.push(b'\\');
        }
        output.push(byte);
    }
    output.push(b'"');
}

enum Segment {
    Static(Bytes),
    Stream(PartStream),
}

struct PartStream {
    source: BodyStream,
    exact: Option<u64>,
    minimum: Option<u64>,
    observed: u64,
}

/// Lazy core-owned multipart serialization with one retained boundary.
struct MultipartStream {
    segments: VecDeque<Segment>,
    finished: bool,
}

impl MultipartStream {
    fn fail(&mut self, kind: BodyErrorKind) -> Poll<Option<Result<Bytes, BodyError>>> {
        self.finished = true;
        self.segments.clear();
        Poll::Ready(Some(Err(BodyError::new(kind))))
    }
}

impl Stream for MultipartStream {
    type Item = Result<Bytes, BodyError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.finished {
            return Poll::Ready(None);
        }
        loop {
            match this.segments.front_mut() {
                None => {
                    this.finished = true;
                    return Poll::Ready(None);
                }
                Some(Segment::Static(_)) => {
                    let Some(Segment::Static(bytes)) = this.segments.pop_front() else {
                        unreachable!("the front segment was checked as static");
                    };
                    if !bytes.is_empty() {
                        return Poll::Ready(Some(Ok(bytes)));
                    }
                }
                Some(Segment::Stream(part)) => match part.source.as_mut().poll_next(context) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Some(Ok(bytes))) => {
                        let Some(next) = part.observed.checked_add(saturating_u64(bytes.len()))
                        else {
                            return this.fail(BodyErrorKind::LimitExceeded);
                        };
                        if part.exact.is_some_and(|exact| next > exact) {
                            return this.fail(BodyErrorKind::InvalidLength);
                        }
                        part.observed = next;
                        // Empty chunks pass through to the outer managed stream,
                        // which suppresses and bounds them.
                        return Poll::Ready(Some(Ok(bytes)));
                    }
                    Poll::Ready(Some(Err(error))) => {
                        let kind = error.kind();
                        drop(error);
                        return this.fail(kind);
                    }
                    Poll::Ready(None) => {
                        let observed = part.observed;
                        if part.exact.is_some_and(|exact| observed != exact)
                            || part.minimum.is_some_and(|minimum| observed < minimum)
                        {
                            return this.fail(BodyErrorKind::InvalidLength);
                        }
                        this.segments.pop_front();
                    }
                },
            }
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
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

fn saturating_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn limit_exceeded() -> BodyError {
    BodyError::new(BodyErrorKind::LimitExceeded)
}

#[cfg(test)]
mod tests;
