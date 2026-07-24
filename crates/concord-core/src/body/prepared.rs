use std::fmt;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use http::HeaderValue;

use super::spec::{LengthHint, RequestBodyMetadata, RequestContentType};
use super::{BodyError, BodyErrorKind, StreamBody};
#[cfg(feature = "multipart")]
use super::{MultipartBody, MultipartFactory};

/// Structural category of a request body visible to lifecycle observers.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyKind {
    /// No request body.
    Empty,
    /// A complete buffered request body.
    Buffered,
    /// A lazy byte stream.
    Stream,
    /// A request-side multipart recipe.
    Multipart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Replayability {
    Reusable,
    OneShot,
    Reconstructible,
}

/// Creates a fresh complete stream for one Concord attempt.
///
/// `content_type` is captured once when [`RequestBody::factory`] constructs the
/// managed value and must be deterministic and side-effect-light. Metadata
/// computation does not call the factory. `create` is called only after
/// `before_send` approves the attempt. Reqwest-internal retries cannot call
/// this trait. Calls for
/// independent operations may overlap. Bodies and factory state may be
/// sensitive; implementations own their logging and storage and must bound CPU,
/// memory, retries, maps, queues, spawned tasks, metric cardinality, and external
/// I/O. Both callbacks are synchronous and cannot be preempted by Concord; the
/// operation deadline is checked after return, cancellation prevents later work
/// only, and Concord does not automatically bound custom factory state.
pub trait BodyFactory: Send + Sync + 'static {
    /// Returns the stable content type for every body produced by this factory.
    fn content_type(&self) -> Option<HeaderValue> {
        None
    }

    /// Creates one fresh request stream for the current Concord attempt.
    fn create(&self) -> Result<StreamBody, BodyError>;
}

/// Opaque explicit request body used by the shared body architecture.
pub struct RequestBody {
    value: RequestBodyValue,
    content_type: Option<HeaderValue>,
}

enum RequestBodyValue {
    Empty,
    Bytes(Bytes),
    Stream(Mutex<Option<StreamBody>>),
    Factory {
        factory: Arc<dyn BodyFactory>,
        default_content_type: Result<Option<HeaderValue>, BodyErrorKind>,
    },
    #[cfg(feature = "multipart")]
    Multipart(Mutex<Option<MultipartBody>>),
    #[cfg(feature = "multipart")]
    MultipartFactory(Arc<dyn MultipartFactory>),
}

impl RequestBody {
    /// Creates a reusable empty body.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            value: RequestBodyValue::Empty,
            content_type: None,
        }
    }

    /// Creates a reusable body from complete bytes.
    #[must_use]
    pub fn bytes(value: Bytes) -> Self {
        Self {
            value: RequestBodyValue::Bytes(value),
            content_type: None,
        }
    }

    /// Creates a one-shot body from a lazy stream.
    #[must_use]
    pub fn stream(value: StreamBody) -> Self {
        Self {
            value: RequestBodyValue::Stream(Mutex::new(Some(value))),
            content_type: None,
        }
    }

    /// Creates a reconstructible body and captures its structural content type.
    #[must_use]
    pub fn factory<F>(factory: F) -> Self
    where
        F: BodyFactory,
    {
        let default_content_type = factory
            .content_type()
            .map(validate_header_content_type)
            .transpose()
            .map_err(|error| error.kind());
        Self {
            value: RequestBodyValue::Factory {
                factory: Arc::new(factory),
                default_content_type,
            },
            content_type: None,
        }
    }

    /// Creates a multipart body whose replayability follows its parts.
    #[cfg(feature = "multipart")]
    #[must_use]
    pub fn multipart(value: MultipartBody) -> Self {
        Self {
            value: RequestBodyValue::Multipart(Mutex::new(Some(value))),
            content_type: None,
        }
    }

    /// Creates a reconstructible multipart body from a downstream factory.
    #[cfg(feature = "multipart")]
    #[must_use]
    pub fn multipart_factory<F>(factory: F) -> Self
    where
        F: MultipartFactory,
    {
        Self {
            value: RequestBodyValue::MultipartFactory(Arc::new(factory)),
            content_type: None,
        }
    }

    /// Sets a validated top-level media type for a non-multipart body.
    pub fn content_type(mut self, media_type: impl AsRef<str>) -> Result<Self, BodyError> {
        #[cfg(feature = "multipart")]
        if matches!(
            &self.value,
            RequestBodyValue::Multipart(_) | RequestBodyValue::MultipartFactory(_)
        ) {
            return Err(BodyError::new(BodyErrorKind::InvalidContentType));
        }
        self.content_type = Some(validated_content_type(media_type.as_ref())?);
        Ok(self)
    }

    /// Returns the structural body kind without materializing the body.
    #[must_use]
    pub fn kind(&self) -> BodyKind {
        match &self.value {
            RequestBodyValue::Empty => BodyKind::Empty,
            RequestBodyValue::Bytes(_) => BodyKind::Buffered,
            RequestBodyValue::Stream(_) | RequestBodyValue::Factory { .. } => BodyKind::Stream,
            #[cfg(feature = "multipart")]
            RequestBodyValue::Multipart(_) | RequestBodyValue::MultipartFactory(_) => {
                BodyKind::Multipart
            }
        }
    }
}

impl fmt::Debug for RequestBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestBody")
            .field("kind", &self.kind())
            .field(
                "content_type",
                &self.content_type.as_ref().map(|_| "<validated>"),
            )
            .field("value", &"<request body omitted>")
            .finish()
    }
}

impl RequestBody {
    pub(crate) fn structural_metadata(&self) -> Result<RequestBodyMetadata, BodyError> {
        #[cfg(feature = "multipart")]
        if matches!(
            &self.value,
            RequestBodyValue::Multipart(_) | RequestBodyValue::MultipartFactory(_)
        ) {
            let length = match &self.value {
                RequestBodyValue::Multipart(body) => {
                    let body = body
                        .lock()
                        .map_err(|_| BodyError::new(BodyErrorKind::Internal))?;
                    let body = body
                        .as_ref()
                        .ok_or_else(|| BodyError::new(BodyErrorKind::Internal))?;
                    LengthHint::AtLeast(body.metadata()?.lower_bound)
                }
                RequestBodyValue::MultipartFactory(_) => LengthHint::Unknown,
                _ => unreachable!("multipart branch checked above"),
            };
            return Ok(RequestBodyMetadata {
                length,
                content_type: RequestContentType::Generated,
                kind: BodyKind::Multipart,
            });
        }

        let content_type = match &self.content_type {
            Some(value) => Some(value.clone()),
            None => match &self.value {
                RequestBodyValue::Factory {
                    default_content_type,
                    ..
                } => default_content_type.clone().map_err(BodyError::new)?,
                _ => None,
            },
        }
        .map_or(RequestContentType::None, RequestContentType::Exact);

        let (kind, length) = match &self.value {
            RequestBodyValue::Empty => (BodyKind::Empty, LengthHint::Exact(0)),
            RequestBodyValue::Bytes(bytes) => (
                BodyKind::Buffered,
                LengthHint::Exact(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
            ),
            RequestBodyValue::Stream(body) => {
                let body = body
                    .lock()
                    .map_err(|_| BodyError::new(BodyErrorKind::Internal))?;
                let body = body
                    .as_ref()
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Internal))?;
                body.validate_metadata()?;
                let length = match body.exact_length_value() {
                    Some(length) => LengthHint::Exact(length),
                    None => body
                        .lower_bound()
                        .map_or(LengthHint::Unknown, LengthHint::AtLeast),
                };
                (BodyKind::Stream, length)
            }
            RequestBodyValue::Factory { .. } => (BodyKind::Stream, LengthHint::Unknown),
            #[cfg(feature = "multipart")]
            RequestBodyValue::Multipart(_) | RequestBodyValue::MultipartFactory(_) => {
                unreachable!("multipart branch returned above")
            }
        };
        Ok(RequestBodyMetadata {
            length,
            content_type,
            kind,
        })
    }

    pub(crate) fn into_prepared(self, limit: NonZeroUsize) -> Result<PreparedBody, BodyError> {
        match self.value {
            RequestBodyValue::Empty => Ok(PreparedBody::Empty),
            RequestBodyValue::Bytes(bytes) => {
                if bytes.len() > limit.get() {
                    return Err(BodyError::new(BodyErrorKind::LimitExceeded));
                }
                Ok(PreparedBody::ReusableBytes(bytes))
            }
            RequestBodyValue::Stream(body) => {
                let body = body
                    .into_inner()
                    .map_err(|_| BodyError::new(BodyErrorKind::Internal))?
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Internal))?;
                let exact_length = validate_stream_limit(&body, limit)?;
                Ok(PreparedBody::OneShotStream {
                    body: Some(body),
                    exact_length,
                })
            }
            RequestBodyValue::Factory { factory, .. } => {
                let body = factory.create().map_err(|error| {
                    drop(error);
                    BodyError::new(BodyErrorKind::Factory)
                })?;
                let exact_length = validate_stream_limit(&body, limit)?;
                Ok(PreparedBody::StreamFactory {
                    factory,
                    current: Some(body),
                    exact_length,
                })
            }
            #[cfg(feature = "multipart")]
            RequestBodyValue::Multipart(body) => {
                let body = body
                    .into_inner()
                    .map_err(|_| BodyError::new(BodyErrorKind::Internal))?
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Internal))?;
                materialize_multipart(body, limit)
            }
            #[cfg(feature = "multipart")]
            RequestBodyValue::MultipartFactory(factory) => {
                let body = factory.create().map_err(|error| {
                    drop(error);
                    BodyError::new(BodyErrorKind::Factory)
                })?;
                validate_multipart_limit(&body, limit)?;
                Ok(PreparedBody::MultipartFactory {
                    factory,
                    current: Some(body),
                })
            }
        }
    }
}

fn validate_stream_limit(body: &StreamBody, limit: NonZeroUsize) -> Result<Option<u64>, BodyError> {
    body.validate_metadata()?;
    let limit = u64::try_from(limit.get()).unwrap_or(u64::MAX);
    if body
        .exact_length_value()
        .is_some_and(|length| length > limit)
        || body.lower_bound().is_some_and(|length| length > limit)
    {
        return Err(BodyError::new(BodyErrorKind::LimitExceeded));
    }
    Ok(body.exact_length_value())
}

#[cfg(feature = "multipart")]
fn multipart_body_metadata(body: &MultipartBody) -> Result<(Replayability, u64), BodyError> {
    let metadata = body.metadata()?;
    Ok((
        if metadata.reusable {
            Replayability::Reusable
        } else {
            Replayability::OneShot
        },
        metadata.lower_bound,
    ))
}

#[cfg(feature = "multipart")]
fn materialize_multipart(
    body: MultipartBody,
    limit: NonZeroUsize,
) -> Result<PreparedBody, BodyError> {
    let (replayability, _) = validate_multipart_limit(&body, limit)?;
    if replayability == Replayability::Reusable {
        Ok(PreparedBody::ReplayableMultipart { recipe: body })
    } else {
        Ok(PreparedBody::OneShotMultipart { body: Some(body) })
    }
}

#[cfg(feature = "multipart")]
fn validate_multipart_limit(
    body: &MultipartBody,
    limit: NonZeroUsize,
) -> Result<(Replayability, u64), BodyError> {
    let metadata = multipart_body_metadata(body)?;
    if metadata.1 > u64::try_from(limit.get()).unwrap_or(u64::MAX) {
        return Err(BodyError::new(BodyErrorKind::LimitExceeded));
    }
    Ok(metadata)
}

/// The one private materialized application-body representation.
#[doc(hidden)]
pub(crate) enum PreparedBody {
    Empty,
    ReusableBytes(Bytes),
    OneShotStream {
        body: Option<StreamBody>,
        exact_length: Option<u64>,
    },
    StreamFactory {
        factory: Arc<dyn BodyFactory>,
        current: Option<StreamBody>,
        exact_length: Option<u64>,
    },
    #[cfg(feature = "multipart")]
    ReplayableMultipart {
        recipe: MultipartBody,
    },
    #[cfg(feature = "multipart")]
    OneShotMultipart {
        body: Option<MultipartBody>,
    },
    #[cfg(feature = "multipart")]
    MultipartFactory {
        factory: Arc<dyn MultipartFactory>,
        current: Option<MultipartBody>,
    },
}

impl PreparedBody {
    pub(crate) const fn kind(&self) -> BodyKind {
        match self {
            Self::Empty => BodyKind::Empty,
            Self::ReusableBytes(_) => BodyKind::Buffered,
            Self::OneShotStream { .. } | Self::StreamFactory { .. } => BodyKind::Stream,
            #[cfg(feature = "multipart")]
            Self::ReplayableMultipart { .. }
            | Self::OneShotMultipart { .. }
            | Self::MultipartFactory { .. } => BodyKind::Multipart,
        }
    }

    pub(crate) const fn replayability(&self) -> Replayability {
        match self {
            Self::Empty | Self::ReusableBytes(_) => Replayability::Reusable,
            Self::OneShotStream { .. } => Replayability::OneShot,
            Self::StreamFactory { .. } => Replayability::Reconstructible,
            #[cfg(feature = "multipart")]
            Self::ReplayableMultipart { .. } => Replayability::Reusable,
            #[cfg(feature = "multipart")]
            Self::OneShotMultipart { .. } => Replayability::OneShot,
            #[cfg(feature = "multipart")]
            Self::MultipartFactory { .. } => Replayability::Reconstructible,
        }
    }

    pub(crate) fn known_length(&self) -> Option<u64> {
        match self {
            Self::Empty => Some(0),
            Self::ReusableBytes(bytes) => Some(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
            Self::OneShotStream { exact_length, .. } | Self::StreamFactory { exact_length, .. } => {
                *exact_length
            }
            #[cfg(feature = "multipart")]
            Self::ReplayableMultipart { .. }
            | Self::OneShotMultipart { .. }
            | Self::MultipartFactory { .. } => None,
        }
    }

    pub(crate) fn prepare_native(&mut self, limit: NonZeroUsize) -> Result<NativeBody, BodyError> {
        match self {
            Self::Empty => Ok(NativeBody::Empty),
            Self::ReusableBytes(bytes) => Ok(NativeBody::Bytes(bytes.clone())),
            Self::OneShotStream { body, exact_length } => {
                let body = body
                    .take()
                    .ok_or_else(|| BodyError::new(BodyErrorKind::InvalidLength))?;
                Ok(stream_native(body, *exact_length, limit))
            }
            Self::StreamFactory {
                current,
                exact_length,
                ..
            } => {
                let body = current
                    .take()
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Factory))?;
                Ok(stream_native(body, *exact_length, limit))
            }
            #[cfg(feature = "multipart")]
            Self::ReplayableMultipart { recipe } => {
                let body = recipe
                    .clone_reusable()?
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Multipart))?;
                Ok(NativeBody::Multipart {
                    form: body.into_form(limit)?,
                    limit,
                })
            }
            #[cfg(feature = "multipart")]
            Self::OneShotMultipart { body } => {
                let body = body
                    .take()
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Multipart))?;
                Ok(NativeBody::Multipart {
                    form: body.into_form(limit)?,
                    limit,
                })
            }
            #[cfg(feature = "multipart")]
            Self::MultipartFactory { current, .. } => {
                let body = current
                    .take()
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Factory))?;
                Ok(NativeBody::Multipart {
                    form: body.into_form(limit)?,
                    limit,
                })
            }
        }
    }

    pub(crate) fn reconstruct_for_next_attempt(
        &mut self,
        limit: NonZeroUsize,
    ) -> Result<(), BodyError> {
        match self {
            Self::Empty | Self::ReusableBytes(_) => Ok(()),
            Self::OneShotStream { .. } => Err(BodyError::new(BodyErrorKind::InvalidLength)),
            Self::StreamFactory {
                factory,
                current,
                exact_length,
            } => {
                let body = factory.create().map_err(|error| {
                    drop(error);
                    BodyError::new(BodyErrorKind::Factory)
                })?;
                *exact_length = validate_stream_limit(&body, limit)?;
                *current = Some(body);
                Ok(())
            }
            #[cfg(feature = "multipart")]
            Self::ReplayableMultipart { .. } => Ok(()),
            #[cfg(feature = "multipart")]
            Self::OneShotMultipart { .. } => Err(BodyError::new(BodyErrorKind::Multipart)),
            #[cfg(feature = "multipart")]
            Self::MultipartFactory { factory, current } => {
                let body = factory.create().map_err(|error| {
                    drop(error);
                    BodyError::new(BodyErrorKind::Factory)
                })?;
                validate_multipart_limit(&body, limit)?;
                *current = Some(body);
                Ok(())
            }
        }
    }
}

pub(crate) enum NativeBody {
    Empty,
    Bytes(Bytes),
    Stream {
        body: reqwest::Body,
        known_length: Option<u64>,
        failure: BodyFailureState,
    },
    #[cfg(feature = "multipart")]
    Multipart {
        form: reqwest::multipart::Form,
        limit: NonZeroUsize,
    },
}

#[derive(Clone, Default)]
pub(crate) struct BodyFailureState {
    kind: Arc<AtomicU8>,
}

impl BodyFailureState {
    fn record(&self, kind: BodyErrorKind) {
        let _ = self.kind.compare_exchange(
            0,
            body_error_code(kind),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub(crate) fn failed(&self) -> bool {
        self.kind.load(Ordering::Acquire) != 0
    }

    #[cfg(test)]
    pub(crate) fn kind(&self) -> Option<BodyErrorKind> {
        body_error_from_code(self.kind.load(Ordering::Acquire))
    }
}

pub(super) fn stream_native(
    body: StreamBody,
    exact_length: Option<u64>,
    limit: NonZeroUsize,
) -> NativeBody {
    let failure = BodyFailureState::default();
    let stream = ManagedRequestStream {
        source: body.into_source(),
        exact_length,
        limit: u64::try_from(limit.get()).unwrap_or(u64::MAX),
        observed: 0,
        finished: false,
        failure: failure.clone(),
    };
    NativeBody::Stream {
        body: reqwest::Body::wrap_stream(stream),
        known_length: exact_length,
        failure,
    }
}

#[cfg(feature = "multipart")]
pub(crate) fn wrap_multipart_native_body(
    body: reqwest::Body,
    exact_length: Option<u64>,
    limit: NonZeroUsize,
) -> (reqwest::Body, BodyFailureState) {
    let failure = BodyFailureState::default();
    let source = Box::pin(NativeMultipartStream {
        source: Box::pin(body),
        finished: false,
    });
    let stream = ManagedRequestStream {
        source,
        exact_length,
        limit: u64::try_from(limit.get()).unwrap_or(u64::MAX),
        observed: 0,
        finished: false,
        failure: failure.clone(),
    };
    (reqwest::Body::wrap_stream(stream), failure)
}

#[cfg(feature = "multipart")]
struct NativeMultipartStream {
    source: std::pin::Pin<Box<reqwest::Body>>,
    finished: bool,
}

#[cfg(feature = "multipart")]
impl Stream for NativeMultipartStream {
    type Item = Result<Bytes, BodyError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        use http_body::Body as _;

        let this = self.get_mut();
        if this.finished {
            return Poll::Ready(None);
        }
        loop {
            match this.source.as_mut().poll_frame(context) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                    Ok(bytes) => return Poll::Ready(Some(Ok(bytes))),
                    Err(_non_data_frame) => continue,
                },
                Poll::Ready(Some(Err(error))) => {
                    drop(error);
                    this.finished = true;
                    return Poll::Ready(Some(Err(BodyError::new(BodyErrorKind::Multipart))));
                }
                Poll::Ready(None) => {
                    this.finished = true;
                    return Poll::Ready(None);
                }
            }
        }
    }
}

struct ManagedRequestStream {
    source: super::stream::BodyStream,
    exact_length: Option<u64>,
    limit: u64,
    observed: u64,
    finished: bool,
    failure: BodyFailureState,
}

impl ManagedRequestStream {
    fn fail(&mut self, kind: BodyErrorKind) -> Poll<Option<Result<Bytes, BodyError>>> {
        self.finished = true;
        self.failure.record(kind);
        Poll::Ready(Some(Err(BodyError::new(kind))))
    }
}

impl Stream for ManagedRequestStream {
    type Item = Result<Bytes, BodyError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.finished {
            return Poll::Ready(None);
        }
        match this.source.as_mut().poll_next(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(Err(error))) => {
                let kind = error.kind();
                drop(error);
                this.fail(kind)
            }
            Poll::Ready(Some(Ok(bytes))) => {
                let Some(next) = this
                    .observed
                    .checked_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
                else {
                    return this.fail(BodyErrorKind::LimitExceeded);
                };
                if this.exact_length.is_some_and(|exact| next > exact) {
                    return this.fail(BodyErrorKind::InvalidLength);
                }
                if next > this.limit {
                    return this.fail(BodyErrorKind::LimitExceeded);
                }
                this.observed = next;
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(None) => {
                if this
                    .exact_length
                    .is_some_and(|exact| this.observed != exact)
                {
                    return this.fail(BodyErrorKind::InvalidLength);
                }
                this.finished = true;
                Poll::Ready(None)
            }
        }
    }
}

pub(crate) fn validated_content_type(value: &str) -> Result<HeaderValue, BodyError> {
    let parsed = mime::Mime::from_str(value)
        .map_err(|_| BodyError::new(BodyErrorKind::InvalidContentType))?;
    if parsed.type_() == mime::STAR || parsed.subtype() == mime::STAR {
        return Err(BodyError::new(BodyErrorKind::InvalidContentType));
    }
    HeaderValue::from_str(value).map_err(|_| BodyError::new(BodyErrorKind::InvalidContentType))
}

fn validate_header_content_type(value: HeaderValue) -> Result<HeaderValue, BodyError> {
    let text = value
        .to_str()
        .map_err(|_| BodyError::new(BodyErrorKind::InvalidContentType))?;
    validated_content_type(text)?;
    Ok(value)
}

const fn body_error_code(kind: BodyErrorKind) -> u8 {
    match kind {
        BodyErrorKind::InvalidContentType => 1,
        BodyErrorKind::InvalidLength => 2,
        BodyErrorKind::Producer => 3,
        BodyErrorKind::LimitExceeded => 4,
        BodyErrorKind::Factory => 5,
        BodyErrorKind::Multipart => 6,
        BodyErrorKind::Internal => 7,
    }
}

#[cfg(test)]
const fn body_error_from_code(code: u8) -> Option<BodyErrorKind> {
    match code {
        1 => Some(BodyErrorKind::InvalidContentType),
        2 => Some(BodyErrorKind::InvalidLength),
        3 => Some(BodyErrorKind::Producer),
        4 => Some(BodyErrorKind::LimitExceeded),
        5 => Some(BodyErrorKind::Factory),
        6 => Some(BodyErrorKind::Multipart),
        7 => Some(BodyErrorKind::Internal),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
