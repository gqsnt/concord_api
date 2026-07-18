use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_core::Stream;
use http::{HeaderValue, Method};

use crate::codec::{EncodeBody, EncodeContext, MediaType};
use crate::error::{ApiError, ErrorCategory};

use super::{BodyError, BodyErrorKind, StreamBody};
#[cfg(feature = "multipart")]
use super::{MultipartBody, MultipartFactory};

pub(super) mod sealed {
    pub trait Body {}
}

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

/// Generated-private safe request-body facts available before materialization.
#[doc(hidden)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyMetadata {
    kind: BodyKind,
    replayability: Replayability,
    exact_length: Option<u64>,
    lower_bound: Option<u64>,
    content_type: Option<HeaderValue>,
}

impl BodyMetadata {
    pub(crate) fn empty() -> Self {
        Self {
            kind: BodyKind::Empty,
            replayability: Replayability::Reusable,
            exact_length: Some(0),
            lower_bound: Some(0),
            content_type: None,
        }
    }

    pub(crate) fn buffered(content_type: Option<HeaderValue>) -> Self {
        Self {
            kind: BodyKind::Buffered,
            replayability: Replayability::Reusable,
            exact_length: None,
            lower_bound: None,
            content_type,
        }
    }

    pub(crate) fn stream(
        stream: &StreamBody,
        replayability: Replayability,
        content_type: Option<HeaderValue>,
    ) -> Result<Self, BodyError> {
        stream.validate_metadata()?;
        Ok(Self {
            kind: BodyKind::Stream,
            replayability,
            exact_length: stream.exact_length_value(),
            lower_bound: stream.lower_bound(),
            content_type,
        })
    }

    #[cfg(feature = "multipart")]
    pub(crate) const fn multipart(replayability: Replayability, lower_bound: Option<u64>) -> Self {
        Self {
            kind: BodyKind::Multipart,
            replayability,
            exact_length: None,
            lower_bound,
            content_type: None,
        }
    }

    #[doc(hidden)]
    pub const fn kind(&self) -> BodyKind {
        self.kind
    }

    #[doc(hidden)]
    pub fn known_length(&self) -> Option<u64> {
        self.exact_length.or(self.lower_bound)
    }

    pub(crate) const fn exact_length(&self) -> Option<u64> {
        self.exact_length
    }

    #[cfg(test)]
    pub(crate) const fn replayability(&self) -> Replayability {
        self.replayability
    }

    pub(crate) const fn content_type(&self) -> Option<&HeaderValue> {
        self.content_type.as_ref()
    }

    pub(crate) const fn owns_content_type(&self) -> bool {
        self.content_type.is_some() || matches!(self.kind, BodyKind::Multipart)
    }

    pub(crate) fn with_known_length(mut self, known_length: Option<u64>) -> Self {
        self.exact_length = known_length;
        self.lower_bound = known_length;
        self
    }

    pub(crate) fn validate_limit(&self, limit: NonZeroUsize) -> Result<(), BodyError> {
        if self
            .exact_length
            .zip(self.lower_bound)
            .is_some_and(|(exact, lower)| lower > exact)
        {
            return Err(BodyError::new(BodyErrorKind::InvalidLength));
        }
        let limit = u64::try_from(limit.get()).unwrap_or(u64::MAX);
        if self.exact_length.is_some_and(|length| length > limit)
            || self.lower_bound.is_some_and(|length| length > limit)
        {
            return Err(BodyError::new(BodyErrorKind::LimitExceeded));
        }
        Ok(())
    }
}

/// The one generated-private request-body integration contract.
#[doc(hidden)]
pub trait BodySpec: sealed::Body + Send + 'static {
    fn metadata(&self, endpoint: &'static str, method: &Method) -> Result<BodyMetadata, ApiError>;

    fn materialize(
        self,
        endpoint: &'static str,
        method: &Method,
        limit: NonZeroUsize,
    ) -> Result<PreparedBody, ApiError>;
}

/// Empty request-body value used by bodyless generated endpoints.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default)]
pub struct EmptyBody;

impl EmptyBody {
    #[doc(hidden)]
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    #[doc(hidden)]
    #[must_use]
    pub fn metadata() -> BodyMetadata {
        BodyMetadata::empty()
    }
}

impl BodySpec for EmptyBody {
    fn metadata(
        &self,
        _endpoint: &'static str,
        _method: &Method,
    ) -> Result<BodyMetadata, ApiError> {
        Ok(BodyMetadata::empty())
    }

    fn materialize(
        self,
        _endpoint: &'static str,
        _method: &Method,
        _limit: NonZeroUsize,
    ) -> Result<PreparedBody, ApiError> {
        Ok(PreparedBody::Empty)
    }
}

impl sealed::Body for EmptyBody {}

/// Generated-private request body backed by one public buffered codec.
#[doc(hidden)]
pub struct CodecBody<C>
where
    C: EncodeBody,
{
    input: C::Input,
    marker: PhantomData<C>,
}

impl<C> CodecBody<C>
where
    C: EncodeBody,
{
    #[doc(hidden)]
    #[must_use]
    pub fn new(input: C::Input) -> Self {
        Self {
            input,
            marker: PhantomData,
        }
    }

    pub(crate) fn metadata_for(
        endpoint: &'static str,
        method: &Method,
    ) -> Result<BodyMetadata, ApiError> {
        let content_type = C::CONTENT_TYPE
            .map(validated_content_type)
            .transpose()
            .map_err(|error| body_api_error(endpoint, method, error))?;
        Ok(BodyMetadata::buffered(content_type))
    }
}

impl<C> BodySpec for CodecBody<C>
where
    C: EncodeBody,
{
    fn metadata(&self, endpoint: &'static str, method: &Method) -> Result<BodyMetadata, ApiError> {
        Self::metadata_for(endpoint, method)
    }

    fn materialize(
        self,
        endpoint: &'static str,
        method: &Method,
        limit: NonZeroUsize,
    ) -> Result<PreparedBody, ApiError> {
        let bytes = C::encode(
            self.input,
            EncodeContext {
                endpoint,
                method,
                body_limit: Some(limit.get()),
            },
        )
        .map_err(|error| {
            drop(error);
            ApiError::classified(endpoint, method.clone(), ErrorCategory::RequestEncoding)
        })?;
        if bytes.len() > limit.get() {
            return Err(ApiError::classified(
                endpoint,
                method.clone(),
                ErrorCategory::RequestBody,
            ));
        }
        Ok(PreparedBody::ReusableBytes(bytes))
    }
}

impl<C> sealed::Body for CodecBody<C> where C: EncodeBody {}

/// Generated-private typed request-stream adapter.
#[doc(hidden)]
pub struct StreamRequestBody<M> {
    body: StreamBody,
    marker: PhantomData<fn() -> M>,
}

impl<M> StreamRequestBody<M> {
    #[doc(hidden)]
    #[must_use]
    pub fn new(body: StreamBody) -> Self {
        Self {
            body,
            marker: PhantomData,
        }
    }
}

impl<M> BodySpec for StreamRequestBody<M>
where
    M: MediaType,
{
    fn metadata(&self, endpoint: &'static str, method: &Method) -> Result<BodyMetadata, ApiError> {
        let content_type = validated_content_type(M::MEDIA_TYPE)
            .map_err(|error| body_api_error(endpoint, method, error))?;
        BodyMetadata::stream(&self.body, Replayability::OneShot, Some(content_type))
            .map_err(|error| body_api_error(endpoint, method, error))
    }

    fn materialize(
        self,
        endpoint: &'static str,
        method: &Method,
        limit: NonZeroUsize,
    ) -> Result<PreparedBody, ApiError> {
        let metadata = BodyMetadata::stream(&self.body, Replayability::OneShot, None)
            .and_then(|metadata| {
                metadata.validate_limit(limit)?;
                Ok(metadata)
            })
            .map_err(|error| body_api_error(endpoint, method, error))?;
        Ok(PreparedBody::OneShotStream {
            body: Some(self.body),
            exact_length: metadata.exact_length(),
        })
    }
}

impl<M> sealed::Body for StreamRequestBody<M> where M: MediaType {}

/// Creates a fresh complete stream for one Concord attempt.
///
/// `content_type` is metadata inspection and must be deterministic and
/// side-effect-light. `create` is called only after `before_send` approves the
/// attempt. Reqwest-internal retries cannot call this trait.
pub trait BodyFactory: Send + Sync + 'static {
    fn content_type(&self) -> Option<HeaderValue> {
        None
    }

    fn create(&self) -> Result<StreamBody, BodyError>;
}

/// Opaque advanced request body used by the shared body architecture.
pub struct RequestBody {
    value: RequestBodyValue,
    content_type: Option<HeaderValue>,
}

enum RequestBodyValue {
    Empty,
    Bytes(Bytes),
    Stream(StreamBody),
    Factory(Arc<dyn BodyFactory>),
    #[cfg(feature = "multipart")]
    Multipart(MultipartBody),
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
            value: RequestBodyValue::Stream(value),
            content_type: None,
        }
    }

    /// Creates a reconstructible body from a downstream factory.
    #[must_use]
    pub fn factory<F>(factory: F) -> Self
    where
        F: BodyFactory,
    {
        Self {
            value: RequestBodyValue::Factory(Arc::new(factory)),
            content_type: None,
        }
    }

    /// Creates a multipart body whose replayability follows its parts.
    #[cfg(feature = "multipart")]
    #[must_use]
    pub fn multipart(value: MultipartBody) -> Self {
        Self {
            value: RequestBodyValue::Multipart(value),
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
            self.value,
            RequestBodyValue::Multipart(_) | RequestBodyValue::MultipartFactory(_)
        ) {
            return Err(BodyError::new(BodyErrorKind::InvalidContentType));
        }
        self.content_type = Some(validated_content_type(media_type.as_ref())?);
        Ok(self)
    }

    /// Returns the structural body kind without materializing the body.
    #[must_use]
    pub const fn kind(&self) -> BodyKind {
        match self.value {
            RequestBodyValue::Empty => BodyKind::Empty,
            RequestBodyValue::Bytes(_) => BodyKind::Buffered,
            RequestBodyValue::Stream(_) | RequestBodyValue::Factory(_) => BodyKind::Stream,
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

impl BodySpec for RequestBody {
    fn metadata(&self, endpoint: &'static str, method: &Method) -> Result<BodyMetadata, ApiError> {
        let content_type = match &self.content_type {
            Some(value) => Some(value.clone()),
            None => match &self.value {
                RequestBodyValue::Factory(factory) => factory
                    .content_type()
                    .map(validate_header_content_type)
                    .transpose()
                    .map_err(|error| body_api_error(endpoint, method, error))?,
                #[cfg(feature = "multipart")]
                RequestBodyValue::Multipart(_) | RequestBodyValue::MultipartFactory(_) => None,
                _ => None,
            },
        };
        let metadata = match &self.value {
            RequestBodyValue::Empty => {
                let mut metadata = BodyMetadata::empty();
                metadata.content_type = content_type;
                metadata
            }
            RequestBodyValue::Bytes(bytes) => BodyMetadata {
                kind: BodyKind::Buffered,
                replayability: Replayability::Reusable,
                exact_length: Some(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
                lower_bound: Some(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
                content_type,
            },
            RequestBodyValue::Stream(stream) => {
                BodyMetadata::stream(stream, Replayability::OneShot, content_type)
                    .map_err(|error| body_api_error(endpoint, method, error))?
            }
            RequestBodyValue::Factory(_) => BodyMetadata {
                kind: BodyKind::Stream,
                replayability: Replayability::Reconstructible,
                exact_length: None,
                lower_bound: None,
                content_type,
            },
            #[cfg(feature = "multipart")]
            RequestBodyValue::Multipart(body) => multipart_body_metadata(body)
                .map_err(|error| body_api_error(endpoint, method, error))?,
            #[cfg(feature = "multipart")]
            RequestBodyValue::MultipartFactory(_) => {
                BodyMetadata::multipart(Replayability::Reconstructible, None)
            }
        };
        Ok(metadata)
    }

    fn materialize(
        self,
        endpoint: &'static str,
        method: &Method,
        limit: NonZeroUsize,
    ) -> Result<PreparedBody, ApiError> {
        match self.value {
            RequestBodyValue::Empty => Ok(PreparedBody::Empty),
            RequestBodyValue::Bytes(bytes) => {
                if bytes.len() > limit.get() {
                    return Err(ApiError::classified(
                        endpoint,
                        method.clone(),
                        ErrorCategory::RequestBody,
                    ));
                }
                Ok(PreparedBody::ReusableBytes(bytes))
            }
            RequestBodyValue::Stream(body) => {
                let metadata = BodyMetadata::stream(&body, Replayability::OneShot, None)
                    .and_then(|metadata| {
                        metadata.validate_limit(limit)?;
                        Ok(metadata)
                    })
                    .map_err(|error| body_api_error(endpoint, method, error))?;
                Ok(PreparedBody::OneShotStream {
                    body: Some(body),
                    exact_length: metadata.exact_length(),
                })
            }
            RequestBodyValue::Factory(factory) => {
                let body = factory.create().map_err(|error| {
                    drop(error);
                    ApiError::classified(endpoint, method.clone(), ErrorCategory::RequestBody)
                })?;
                let metadata = BodyMetadata::stream(&body, Replayability::OneShot, None)
                    .and_then(|metadata| {
                        metadata.validate_limit(limit)?;
                        Ok(metadata)
                    })
                    .map_err(|error| body_api_error(endpoint, method, error))?;
                Ok(PreparedBody::StreamFactory {
                    factory,
                    current: Some(body),
                    exact_length: metadata.exact_length(),
                })
            }
            #[cfg(feature = "multipart")]
            RequestBodyValue::Multipart(body) => {
                materialize_multipart(body, endpoint, method, limit)
            }
            #[cfg(feature = "multipart")]
            RequestBodyValue::MultipartFactory(factory) => {
                let body = factory.create().map_err(|error| {
                    drop(error);
                    ApiError::classified(endpoint, method.clone(), ErrorCategory::RequestBody)
                })?;
                multipart_body_metadata(&body)
                    .and_then(|metadata| {
                        metadata.validate_limit(limit)?;
                        Ok(metadata)
                    })
                    .map_err(|error| body_api_error(endpoint, method, error))?;
                Ok(PreparedBody::MultipartFactory {
                    factory,
                    current: Some(body),
                })
            }
        }
    }
}

impl sealed::Body for RequestBody {}

#[cfg(feature = "multipart")]
impl BodySpec for MultipartBody {
    fn metadata(&self, endpoint: &'static str, method: &Method) -> Result<BodyMetadata, ApiError> {
        multipart_body_metadata(self).map_err(|error| body_api_error(endpoint, method, error))
    }

    fn materialize(
        self,
        endpoint: &'static str,
        method: &Method,
        limit: NonZeroUsize,
    ) -> Result<PreparedBody, ApiError> {
        materialize_multipart(self, endpoint, method, limit)
    }
}

#[cfg(feature = "multipart")]
impl sealed::Body for MultipartBody {}

#[cfg(feature = "multipart")]
fn multipart_body_metadata(body: &MultipartBody) -> Result<BodyMetadata, BodyError> {
    let metadata = body.metadata()?;
    Ok(BodyMetadata::multipart(
        if metadata.reusable {
            Replayability::Reusable
        } else {
            Replayability::OneShot
        },
        Some(metadata.lower_bound),
    ))
}

#[cfg(feature = "multipart")]
fn materialize_multipart(
    body: MultipartBody,
    endpoint: &'static str,
    method: &Method,
    limit: NonZeroUsize,
) -> Result<PreparedBody, ApiError> {
    let metadata = multipart_body_metadata(&body)
        .and_then(|metadata| {
            metadata.validate_limit(limit)?;
            Ok(metadata)
        })
        .map_err(|error| body_api_error(endpoint, method, error))?;
    if metadata.replayability == Replayability::Reusable {
        Ok(PreparedBody::ReplayableMultipart { recipe: body })
    } else {
        Ok(PreparedBody::OneShotMultipart { body: Some(body) })
    }
}

/// The one private materialized application-body representation.
#[doc(hidden)]
pub enum PreparedBody {
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
                    .clone_reusable()
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
                let metadata = BodyMetadata::stream(&body, Replayability::OneShot, None)?;
                metadata.validate_limit(limit)?;
                *exact_length = metadata.exact_length();
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
                let metadata = multipart_body_metadata(&body)?;
                metadata.validate_limit(limit)?;
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

fn body_api_error(endpoint: &'static str, method: &Method, error: BodyError) -> ApiError {
    drop(error);
    ApiError::classified(endpoint, method.clone(), ErrorCategory::RequestBody)
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
mod tests {
    use std::collections::VecDeque;
    use std::future::poll_fn;
    use std::io;
    use std::pin::Pin;
    use std::sync::atomic::AtomicUsize;
    use std::task::{Context, Poll};

    use super::*;
    use http_body::Body as _;

    struct Factory {
        calls: Arc<AtomicUsize>,
    }

    #[cfg(feature = "multipart")]
    struct MultipartFactoryProbe {
        calls: Arc<AtomicUsize>,
    }

    impl BodyFactory for Factory {
        fn content_type(&self) -> Option<HeaderValue> {
            Some(HeaderValue::from_static("application/x-factory"))
        }

        fn create(&self) -> Result<StreamBody, BodyError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(StreamBody::from_bytes(Bytes::from_static(b"factory")))
        }
    }

    #[cfg(feature = "multipart")]
    impl MultipartFactory for MultipartFactoryProbe {
        fn create(&self) -> Result<MultipartBody, BodyError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            MultipartBody::new().part(
                "factory-field",
                crate::body::MultipartPart::text("factory-value"),
            )
        }
    }

    fn metadata(body: &RequestBody) -> BodyMetadata {
        body.metadata("BodyApi::upload", &Method::POST)
            .expect("body metadata")
    }

    #[test]
    fn request_body_kinds_and_replayability_are_inferred() {
        let empty = RequestBody::empty();
        assert_eq!(empty.kind(), BodyKind::Empty);
        assert_eq!(metadata(&empty).replayability(), Replayability::Reusable);

        let bytes = RequestBody::bytes(Bytes::from_static(b"bytes"));
        assert_eq!(bytes.kind(), BodyKind::Buffered);
        assert_eq!(metadata(&bytes).replayability(), Replayability::Reusable);
        assert_eq!(metadata(&bytes).known_length(), Some(5));

        let stream = RequestBody::stream(StreamBody::from_bytes(Bytes::from_static(b"bytes")));
        assert_eq!(stream.kind(), BodyKind::Stream);
        assert_eq!(metadata(&stream).replayability(), Replayability::OneShot);

        let calls = Arc::new(AtomicUsize::new(0));
        let factory = RequestBody::factory(Factory {
            calls: Arc::clone(&calls),
        });
        assert_eq!(
            metadata(&factory).replayability(),
            Replayability::Reconstructible
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            metadata(&factory).content_type(),
            Some(&HeaderValue::from_static("application/x-factory"))
        );
    }

    #[test]
    fn content_type_is_validated_and_factory_default_can_be_overridden() {
        let calls = Arc::new(AtomicUsize::new(0));
        let body = RequestBody::factory(Factory { calls })
            .content_type("application/x-override; version=1")
            .expect("valid media type");
        assert_eq!(
            metadata(&body).content_type(),
            Some(&HeaderValue::from_static(
                "application/x-override; version=1"
            ))
        );
        assert_eq!(
            RequestBody::empty()
                .content_type("not a media type")
                .expect_err("invalid media")
                .kind(),
            BodyErrorKind::InvalidContentType
        );
    }

    #[cfg(feature = "multipart")]
    #[test]
    fn multipart_replayability_and_factory_freshness_are_internal() {
        let reusable = RequestBody::multipart(
            MultipartBody::new()
                .part(
                    "text",
                    crate::body::MultipartPart::text("multipart-secret-text"),
                )
                .expect("text part")
                .part(
                    "bytes",
                    crate::body::MultipartPart::bytes(Bytes::from_static(
                        b"multipart-secret-bytes",
                    )),
                )
                .expect("bytes part"),
        );
        assert_eq!(reusable.kind(), BodyKind::Multipart);
        assert_eq!(metadata(&reusable).replayability(), Replayability::Reusable);
        assert!(
            reusable
                .content_type("multipart/form-data; boundary=application-owned")
                .is_err()
        );

        let mut reusable_prepared = RequestBody::multipart(
            MultipartBody::new()
                .part("text", crate::body::MultipartPart::text("fresh recipe"))
                .expect("text part"),
        )
        .materialize(
            "BodyApi::upload",
            &Method::POST,
            NonZeroUsize::new(4096).expect("positive limit"),
        )
        .expect("reusable multipart materialization");
        assert_eq!(reusable_prepared.replayability(), Replayability::Reusable);
        let _ = reusable_prepared
            .prepare_native(NonZeroUsize::new(4096).expect("positive limit"))
            .expect("first reusable native recipe");
        reusable_prepared
            .reconstruct_for_next_attempt(NonZeroUsize::new(4096).expect("positive limit"))
            .expect("reusable next attempt");
        let _ = reusable_prepared
            .prepare_native(NonZeroUsize::new(4096).expect("positive limit"))
            .expect("second reusable native recipe");

        let one_shot = RequestBody::multipart(
            MultipartBody::new()
                .part(
                    "stream",
                    crate::body::MultipartPart::stream(StreamBody::from_bytes(Bytes::from_static(
                        b"multipart-stream-secret",
                    ))),
                )
                .expect("stream part"),
        );
        assert_eq!(metadata(&one_shot).replayability(), Replayability::OneShot);
        let mut one_shot = one_shot
            .materialize(
                "BodyApi::upload",
                &Method::POST,
                NonZeroUsize::new(4096).expect("positive limit"),
            )
            .expect("one-shot multipart materialization");
        assert!(
            one_shot
                .reconstruct_for_next_attempt(NonZeroUsize::new(4096).expect("positive limit"))
                .is_err()
        );

        let calls = Arc::new(AtomicUsize::new(0));
        let factory = RequestBody::multipart_factory(MultipartFactoryProbe {
            calls: Arc::clone(&calls),
        });
        assert_eq!(
            metadata(&factory).replayability(),
            Replayability::Reconstructible
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let mut prepared = factory
            .materialize(
                "BodyApi::upload",
                &Method::POST,
                NonZeroUsize::new(4096).expect("positive limit"),
            )
            .expect("multipart factory materialization");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(prepared.replayability(), Replayability::Reconstructible);
        let _ = prepared
            .prepare_native(NonZeroUsize::new(4096).expect("positive limit"))
            .expect("current multipart attempt");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        prepared
            .reconstruct_for_next_attempt(NonZeroUsize::new(4096).expect("positive limit"))
            .expect("next multipart attempt");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[cfg(feature = "multipart")]
    #[tokio::test]
    async fn reqwest_multipart_serialization_uses_the_managed_aggregate_counter() {
        let first_polls = Arc::new(AtomicUsize::new(0));
        let form = MultipartBody::new()
            .part(
                "field",
                crate::body::MultipartPart::stream(stream([Ok(&b"x"[..])], &first_polls)),
            )
            .expect("multipart recipe")
            .into_form(NonZeroUsize::new(4096).expect("positive limit"))
            .expect("native multipart recipe");
        let mut serialized = Box::pin(form.into_stream());
        let mut serialized_frames = Vec::new();
        loop {
            match poll_fn(|context| serialized.as_mut().poll_next(context)).await {
                Some(Ok(bytes)) => serialized_frames.push(bytes),
                Some(Err(error)) => {
                    drop(error);
                    panic!("reference serialization failed")
                }
                None => break,
            }
        }
        assert!(serialized_frames.len() >= 2);
        let crossing_limit = serialized_frames[0]
            .len()
            .checked_add(serialized_frames[1].len())
            .and_then(|length| length.checked_sub(1))
            .expect("multipart frame lengths");
        let limit = NonZeroUsize::new(crossing_limit).expect("positive crossing limit");

        let second_polls = Arc::new(AtomicUsize::new(0));
        let form = MultipartBody::new()
            .part(
                "field",
                crate::body::MultipartPart::stream(stream([Ok(&b"x"[..])], &second_polls)),
            )
            .expect("multipart recipe")
            .into_form(limit)
            .expect("native multipart recipe");
        let serialized = reqwest::Body::wrap_stream(form.into_stream());
        let (mut managed, failure) = wrap_multipart_native_body(serialized, None, limit);
        let mut accepted = Vec::new();
        let native_error = loop {
            match poll_fn(|context| Pin::new(&mut managed).poll_frame(context)).await {
                Some(Ok(frame)) => {
                    if let Ok(bytes) = frame.into_data() {
                        accepted.extend_from_slice(&bytes);
                    }
                }
                Some(Err(error)) => {
                    drop(error);
                    break true;
                }
                None => break false,
            }
        };
        assert!(
            native_error,
            "multipart framing must exceed the small limit"
        );
        assert_eq!(accepted.len(), serialized_frames[0].len());
        assert!(accepted.len() < limit.get());
        assert_eq!(failure.kind(), Some(BodyErrorKind::LimitExceeded));
        assert!(1 < limit.get(), "the one-byte payload itself fits");
    }

    #[test]
    fn factory_materialization_and_explicit_reconstruction_are_attempt_scoped() {
        let calls = Arc::new(AtomicUsize::new(0));
        let body = RequestBody::factory(Factory {
            calls: Arc::clone(&calls),
        });
        let mut prepared = body
            .materialize(
                "BodyApi::upload",
                &Method::POST,
                NonZeroUsize::new(64).unwrap(),
            )
            .expect("initial factory body");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(prepared.replayability(), Replayability::Reconstructible);
        let _ = prepared
            .prepare_native(NonZeroUsize::new(64).unwrap())
            .expect("first native body");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        prepared
            .reconstruct_for_next_attempt(NonZeroUsize::new(64).unwrap())
            .expect("next Concord-attempt body");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    struct FrameStream {
        frames: VecDeque<Result<Bytes, io::Error>>,
        polls: Arc<AtomicUsize>,
    }

    impl Stream for FrameStream {
        type Item = Result<Bytes, io::Error>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(self.frames.pop_front())
        }
    }

    fn stream(
        frames: impl IntoIterator<Item = Result<&'static [u8], io::Error>>,
        polls: &Arc<AtomicUsize>,
    ) -> StreamBody {
        StreamBody::from_stream(FrameStream {
            frames: frames
                .into_iter()
                .map(|frame| frame.map(Bytes::from_static))
                .collect(),
            polls: Arc::clone(polls),
        })
    }

    async fn consume(
        body: StreamBody,
        exact_length: Option<u64>,
        limit: usize,
    ) -> (Vec<Bytes>, Option<BodyErrorKind>) {
        let NativeBody::Stream {
            mut body, failure, ..
        } = stream_native(
            body,
            exact_length,
            NonZeroUsize::new(limit).expect("positive limit"),
        )
        else {
            panic!("stream native body expected");
        };
        let mut accepted = Vec::new();
        loop {
            match poll_fn(|context| Pin::new(&mut body).poll_frame(context)).await {
                Some(Ok(frame)) => {
                    if let Ok(bytes) = frame.into_data() {
                        accepted.push(bytes);
                    }
                }
                Some(Err(error)) => {
                    drop(error);
                    return (accepted, failure.kind());
                }
                None => return (accepted, failure.kind()),
            }
        }
    }

    #[tokio::test]
    async fn managed_stream_withholds_exact_and_limit_excess_frames() {
        let success_polls = Arc::new(AtomicUsize::new(0));
        let (accepted, failure) = consume(
            stream([Ok(&b"ab"[..]), Ok(&b"cd"[..])], &success_polls),
            Some(4),
            4,
        )
        .await;
        assert_eq!(accepted.concat(), b"abcd");
        assert_eq!(failure, None);
        assert_eq!(success_polls.load(Ordering::SeqCst), 3);

        let overflow_polls = Arc::new(AtomicUsize::new(0));
        let (accepted, failure) = consume(
            stream([Ok(&b"abc"[..]), Ok(&b"de"[..])], &overflow_polls),
            Some(4),
            64,
        )
        .await;
        assert_eq!(accepted.concat(), b"abc");
        assert_eq!(failure, Some(BodyErrorKind::InvalidLength));
        assert_eq!(overflow_polls.load(Ordering::SeqCst), 2);

        let limit_polls = Arc::new(AtomicUsize::new(0));
        let (accepted, failure) = consume(
            stream([Ok(&b"abc"[..]), Ok(&b"de"[..])], &limit_polls),
            None,
            4,
        )
        .await;
        assert_eq!(accepted.concat(), b"abc");
        assert_eq!(failure, Some(BodyErrorKind::LimitExceeded));
        assert_eq!(limit_polls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn managed_stream_detects_underflow_and_sanitizes_producer_failure() {
        let underflow_polls = Arc::new(AtomicUsize::new(0));
        let (accepted, failure) =
            consume(stream([Ok(&b"abc"[..])], &underflow_polls), Some(4), 64).await;
        assert_eq!(accepted.concat(), b"abc");
        assert_eq!(failure, Some(BodyErrorKind::InvalidLength));
        assert_eq!(underflow_polls.load(Ordering::SeqCst), 2);

        let producer_polls = Arc::new(AtomicUsize::new(0));
        let (accepted, failure) = consume(
            stream(
                [
                    Ok(&b"accepted"[..]),
                    Err(io::Error::other("producer-secret-881d")),
                ],
                &producer_polls,
            ),
            None,
            64,
        )
        .await;
        assert_eq!(accepted.concat(), b"accepted");
        assert_eq!(failure, Some(BodyErrorKind::Producer));
        assert_eq!(producer_polls.load(Ordering::SeqCst), 2);
    }

    struct PendingStream {
        polls: Arc<AtomicUsize>,
        dropped: Arc<std::sync::atomic::AtomicBool>,
        reached: Arc<tokio::sync::Notify>,
    }

    impl Stream for PendingStream {
        type Item = Result<Bytes, io::Error>;

        fn poll_next(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            self.reached.notify_one();
            Poll::Pending
        }
    }

    impl Drop for PendingStream {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn dropping_native_consumption_drops_source_and_stops_polling() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reached = Arc::new(tokio::sync::Notify::new());
        let body = StreamBody::from_stream(PendingStream {
            polls: Arc::clone(&polls),
            dropped: Arc::clone(&dropped),
            reached: Arc::clone(&reached),
        });
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        let NativeBody::Stream { mut body, .. } =
            stream_native(body, None, NonZeroUsize::new(64).expect("positive limit"))
        else {
            panic!("stream native body expected");
        };
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        let task = tokio::spawn(async move {
            let _ = poll_fn(|context| Pin::new(&mut body).poll_frame(context)).await;
        });
        reached.notified().await;
        let polls_at_cancellation = polls.load(Ordering::SeqCst);
        task.abort();
        let join = task.await.expect_err("native body task must be cancelled");
        assert!(join.is_cancelled());
        assert!(dropped.load(Ordering::SeqCst));
        tokio::task::yield_now().await;
        assert_eq!(polls.load(Ordering::SeqCst), polls_at_cancellation);
    }
}
