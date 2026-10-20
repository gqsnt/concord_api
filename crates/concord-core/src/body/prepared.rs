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

#[cfg(feature = "multipart")]
use super::multipart::{MultipartBoundary, MultipartFraming};
use super::spec::LengthHint;
use super::{BodyError, BodyErrorKind, RequestStream};
#[cfg(feature = "multipart")]
use super::{MultipartBody, MultipartFactory};
use crate::limits::{MAX_EMPTY_REQUEST_CHUNKS, MAX_EMPTY_REQUEST_CHUNKS_PER_POLL};

/// Maximum byte length of one validated request `Content-Type` value.
pub const MAX_REQUEST_CONTENT_TYPE_BYTES: usize = 1_024;

/// Structural category of a request body visible to lifecycle observers.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

/// Creates a fresh complete stream for one Concord application attempt.
///
/// A factory is the core-supported replayable stream source: the prepared body
/// is mechanically reconstructible because every attempt obtains a new finite
/// stream from `create`. The first stream is created while the body is
/// prepared, before credential acquisition; later streams are created only
/// when core has already admitted another application attempt (policy retry
/// or authentication recovery). Transport-private retries never call this
/// trait.
///
/// # Extension contract
///
/// * **Ownership.** The factory is moved into the prepared body and shared by
///   its attempts; each returned stream is owned by core for one attempt.
/// * **Concurrency.** Calls for independent operations may overlap.
/// * **Cancellation.** `create` is synchronous and cannot be preempted; the
///   operation deadline is checked after it returns and cancellation only
///   prevents later Concord work.
/// * **Input sensitivity.** Bodies and factory state may be sensitive;
///   implementations own logging and storage.
/// * **Errors.** Return a [`BodyError`]; core reduces it to
///   [`BodyErrorKind::Factory`] and never copies it into diagnostics.
/// * **Resource obligations.** Implementations must bound CPU, memory,
///   retries, spawned tasks, metric cardinality and external I/O. Every
///   returned stream crosses the same core-owned empty-chunk, byte-limit and
///   length-hint boundary as a direct [`RequestStream`].
/// * **Negative capabilities.** A factory cannot choose the request
///   `Content-Type` (use [`PreparedBody::content_type`]), cannot observe
///   credentials, headers or the native request, and cannot grant an extra
///   attempt or a safe effect.
pub trait BodyFactory: Send + Sync + 'static {
    /// Creates one fresh request stream for the current Concord attempt.
    fn create(&self) -> Result<RequestStream, BodyError>;
}

/// The one core-owned managed request body.
///
/// `PreparedBody` is the sole authority for a request body's source, its exact
/// `Content-Type` (if any), its trustworthy length information and its
/// mechanical replay capability. A
/// [`RequestRepresentation`](super::RequestRepresentation) returns exactly one
/// value from `prepare`; there is no second metadata callback that could
/// disagree with it.
///
/// * [`PreparedBody::empty`] and [`PreparedBody::bytes`] are mechanically
///   replayable.
/// * [`PreparedBody::stream`] is one-shot: it is never automatically replayed.
/// * [`PreparedBody::factory`] is reconstructible through a
///   [`BodyFactory`].
/// * Multipart bodies (feature `multipart`) generate and validate their
///   boundary once, during preparation, and retain it for every replay.
///
/// Mechanical replayability never makes a POST safe or idempotent; effect and
/// dispatch gates are evaluated separately by the executor. Length hints
/// ([`LengthHint`]) are validated against observed production: an excess,
/// truncated or otherwise dishonest body fails with a [`BodyError`] and the
/// operation error records truthful dispatch knowledge. Known lengths are
/// checked against the configured request-body limit before credentials,
/// rate admission or dispatch.
///
/// Automatic `Debug` output is structural and never prints body bytes, stream
/// state, filenames or part values.
pub struct PreparedBody {
    source: BodySource,
    content_type: Option<HeaderValue>,
}

enum BodySource {
    Empty,
    Bytes(Bytes),
    Stream(Mutex<Option<RequestStream>>),
    Factory(Arc<dyn BodyFactory>),
    #[cfg(feature = "multipart")]
    Multipart {
        recipe: Mutex<Option<MultipartBody>>,
        boundary: MultipartBoundary,
        framing: MultipartFraming,
    },
    #[cfg(feature = "multipart")]
    MultipartFactory {
        factory: Arc<dyn MultipartFactory>,
        boundary: MultipartBoundary,
    },
}

impl PreparedBody {
    /// Creates an explicitly empty, mechanically replayable body.
    ///
    /// An empty prepared body produces zero bytes and owns no `Content-Type`
    /// unless [`PreparedBody::content_type`] adds one.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            source: BodySource::Empty,
            content_type: None,
        }
    }

    /// Creates a mechanically replayable body from complete bytes.
    #[must_use]
    pub fn bytes(bytes: Bytes) -> Self {
        Self {
            source: BodySource::Bytes(bytes),
            content_type: None,
        }
    }

    /// Creates a one-shot body from a lazy, empty-chunk-bounded stream.
    ///
    /// The stream is not polled until native transport begins. It is never
    /// replayed automatically: policy retries and authentication recovery are
    /// refused for a one-shot body.
    #[must_use]
    pub fn stream(input: RequestStream<Bytes>) -> Self {
        Self {
            source: BodySource::Stream(Mutex::new(Some(input))),
            content_type: None,
        }
    }

    /// Creates a reconstructible stream body from a core-supported factory.
    ///
    /// The factory is not called by this constructor. Core calls it once while
    /// materializing the prepared body for the first attempt and once more for
    /// each later admitted application attempt.
    #[must_use]
    pub fn factory<F>(factory: F) -> Self
    where
        F: BodyFactory,
    {
        Self {
            source: BodySource::Factory(Arc::new(factory)),
            content_type: None,
        }
    }

    /// Creates a multipart body and generates its boundary exactly once.
    ///
    /// The recipe is structurally validated, a fresh boundary is generated
    /// and validated against the retained buffered parts, and the exact
    /// `multipart/form-data; boundary=...` media type is recorded. The same
    /// boundary is used for the header and for every serialized byte of every
    /// replay. A recipe containing a streaming part is one-shot; a recipe of
    /// text/bytes parts is mechanically replayable.
    #[cfg(feature = "multipart")]
    pub fn multipart(input: MultipartBody) -> Result<Self, BodyError> {
        let boundary = MultipartBoundary::generate_for(&input)?;
        let framing = input.framing(&boundary)?;
        let content_type = boundary.content_type()?;
        Ok(Self {
            source: BodySource::Multipart {
                recipe: Mutex::new(Some(input)),
                boundary,
                framing,
            },
            content_type: Some(content_type),
        })
    }

    /// Creates a reconstructible multipart body from a downstream factory.
    ///
    /// The boundary and exact media type are generated once by this
    /// constructor and retained for every recipe the factory later creates.
    /// The factory itself is first called while the body is materialized.
    #[cfg(feature = "multipart")]
    pub fn multipart_factory<F>(factory: F) -> Result<Self, BodyError>
    where
        F: MultipartFactory,
    {
        let boundary = MultipartBoundary::generate();
        let content_type = boundary.content_type()?;
        Ok(Self {
            source: BodySource::MultipartFactory {
                factory: Arc::new(factory),
                boundary,
            },
            content_type: Some(content_type),
        })
    }

    /// Declares the body's exact `Content-Type` once.
    ///
    /// The value must be a concrete, syntactically valid media type (no `*`
    /// wildcards, no control characters) of at most
    /// [`MAX_REQUEST_CONTENT_TYPE_BYTES`] bytes. Repeating an identical value
    /// is harmless; a conflicting value — including any value other than the
    /// generated one on a multipart body — fails with
    /// [`BodyErrorKind::InvalidContentType`] and never overwrites the
    /// existing declaration.
    pub fn content_type(mut self, value: &str) -> Result<Self, BodyError> {
        let validated = validated_content_type(value)?;
        match &self.content_type {
            Some(existing) if *existing == validated => Ok(self),
            Some(_) => Err(BodyError::new(BodyErrorKind::InvalidContentType)),
            None => {
                self.content_type = Some(validated);
                Ok(self)
            }
        }
    }

    /// Returns the structural body kind without materializing the body.
    #[must_use]
    pub fn kind(&self) -> BodyKind {
        match &self.source {
            BodySource::Empty => BodyKind::Empty,
            BodySource::Bytes(_) => BodyKind::Buffered,
            BodySource::Stream(_) | BodySource::Factory(_) => BodyKind::Stream,
            #[cfg(feature = "multipart")]
            BodySource::Multipart { .. } | BodySource::MultipartFactory { .. } => {
                BodyKind::Multipart
            }
        }
    }

    /// Returns the exact validated `Content-Type` this body owns, if any.
    #[must_use]
    pub fn media_type(&self) -> Option<&str> {
        self.content_type
            .as_ref()
            .and_then(|value| value.to_str().ok())
    }

    /// Returns trustworthy length information without polling or creating
    /// any stream.
    #[must_use]
    pub fn length_hint(&self) -> LengthHint {
        match &self.source {
            BodySource::Empty => LengthHint::Exact(0),
            BodySource::Bytes(bytes) => LengthHint::Exact(saturating_u64(bytes.len())),
            BodySource::Stream(stream) => stream
                .lock()
                .ok()
                .and_then(|stream| stream.as_ref().map(RequestStream::length_hint))
                .unwrap_or(LengthHint::Unknown),
            BodySource::Factory(_) => LengthHint::Unknown,
            #[cfg(feature = "multipart")]
            BodySource::Multipart { framing, .. } => framing.length_hint(),
            #[cfg(feature = "multipart")]
            BodySource::MultipartFactory { .. } => LengthHint::Unknown,
        }
    }

    /// Returns whether core can mechanically replay or reconstruct this body
    /// for another admitted application attempt.
    ///
    /// This is a mechanical capability only; it never grants a retry, a safe
    /// effect or an additional attempt by itself.
    #[must_use]
    pub fn is_replayable(&self) -> bool {
        match &self.source {
            BodySource::Empty | BodySource::Bytes(_) | BodySource::Factory(_) => true,
            BodySource::Stream(_) => false,
            #[cfg(feature = "multipart")]
            BodySource::Multipart { framing, .. } => framing.reusable(),
            #[cfg(feature = "multipart")]
            BodySource::MultipartFactory { .. } => true,
        }
    }

    /// Materializes the one managed body for this operation or page.
    ///
    /// Known length information is checked against the configured positive
    /// request-body limit here, before credentials, rate admission or
    /// dispatch. The first factory stream or recipe is created here.
    pub(crate) fn into_managed(self, limit: NonZeroUsize) -> Result<ManagedBody, BodyError> {
        let hint = self.length_hint();
        validate_hint_limit(hint, limit)?;
        let content_type = self.content_type;
        let source = match self.source {
            BodySource::Empty => ManagedSource::Empty,
            BodySource::Bytes(bytes) => ManagedSource::Bytes(bytes),
            BodySource::Stream(stream) => {
                let stream = stream
                    .into_inner()
                    .map_err(|_| BodyError::new(BodyErrorKind::Internal))?
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Internal))?;
                let length = StreamLength::validated(&stream, limit)?;
                ManagedSource::OneShotStream {
                    body: Some(stream),
                    length,
                }
            }
            BodySource::Factory(factory) => {
                let stream = create_factory_stream(factory.as_ref())?;
                let length = StreamLength::validated(&stream, limit)?;
                ManagedSource::StreamFactory {
                    factory,
                    current: Some(stream),
                    length,
                }
            }
            #[cfg(feature = "multipart")]
            BodySource::Multipart {
                recipe,
                boundary,
                framing,
            } => {
                let recipe = recipe
                    .into_inner()
                    .map_err(|_| BodyError::new(BodyErrorKind::Internal))?
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Internal))?;
                if framing.reusable() {
                    ManagedSource::ReplayableMultipart {
                        recipe,
                        boundary,
                        framing,
                    }
                } else {
                    ManagedSource::OneShotMultipart {
                        body: Some(recipe),
                        boundary,
                        framing,
                    }
                }
            }
            #[cfg(feature = "multipart")]
            BodySource::MultipartFactory { factory, boundary } => {
                let (recipe, framing) = create_factory_recipe(factory.as_ref(), &boundary, limit)?;
                ManagedSource::MultipartFactory {
                    factory,
                    current: Some(recipe),
                    boundary,
                    framing,
                }
            }
        };
        Ok(ManagedBody {
            source,
            content_type,
        })
    }
}

impl fmt::Debug for PreparedBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedBody")
            .field("kind", &self.kind())
            .field("has_content_type", &self.content_type.is_some())
            .field("length_hint", &self.length_hint())
            .field("replayable", &self.is_replayable())
            .field("value", &"<request body omitted>")
            .finish()
    }
}

fn validate_hint_limit(hint: LengthHint, limit: NonZeroUsize) -> Result<(), BodyError> {
    let limit = saturating_u64(limit.get());
    match hint {
        LengthHint::Exact(length) | LengthHint::AtLeast(length) if length > limit => {
            Err(BodyError::new(BodyErrorKind::LimitExceeded))
        }
        _ => Ok(()),
    }
}

fn create_factory_stream(factory: &dyn BodyFactory) -> Result<RequestStream, BodyError> {
    factory.create().map_err(|error| {
        drop(error);
        BodyError::new(BodyErrorKind::Factory)
    })
}

#[cfg(feature = "multipart")]
fn create_factory_recipe(
    factory: &dyn MultipartFactory,
    boundary: &MultipartBoundary,
    limit: NonZeroUsize,
) -> Result<(MultipartBody, MultipartFraming), BodyError> {
    let recipe = factory.create().map_err(|error| {
        drop(error);
        BodyError::new(BodyErrorKind::Factory)
    })?;
    let framing = recipe.framing(boundary)?;
    if recipe.contains_delimiter(boundary) {
        return Err(BodyError::new(BodyErrorKind::Multipart));
    }
    validate_hint_limit(framing.length_hint(), limit)?;
    Ok((recipe, framing))
}

/// Validated exact and minimum length facts of one stream source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StreamLength {
    exact: Option<u64>,
    minimum: Option<u64>,
}

impl StreamLength {
    fn validated(stream: &RequestStream, limit: NonZeroUsize) -> Result<Self, BodyError> {
        stream.validate_metadata()?;
        validate_hint_limit(stream.length_hint(), limit)?;
        Ok(Self {
            exact: stream.exact_length_value(),
            minimum: stream.lower_bound(),
        })
    }

    fn hint(self) -> LengthHint {
        match (self.exact, self.minimum) {
            (Some(exact), _) => LengthHint::Exact(exact),
            (None, Some(minimum)) => LengthHint::AtLeast(minimum),
            (None, None) => LengthHint::Unknown,
        }
    }
}

/// The one private materialized application body of an operation or page.
pub(crate) struct ManagedBody {
    source: ManagedSource,
    content_type: Option<HeaderValue>,
}

enum ManagedSource {
    Empty,
    Bytes(Bytes),
    OneShotStream {
        body: Option<RequestStream>,
        length: StreamLength,
    },
    StreamFactory {
        factory: Arc<dyn BodyFactory>,
        current: Option<RequestStream>,
        length: StreamLength,
    },
    #[cfg(feature = "multipart")]
    ReplayableMultipart {
        recipe: MultipartBody,
        boundary: MultipartBoundary,
        framing: MultipartFraming,
    },
    #[cfg(feature = "multipart")]
    OneShotMultipart {
        body: Option<MultipartBody>,
        boundary: MultipartBoundary,
        framing: MultipartFraming,
    },
    #[cfg(feature = "multipart")]
    MultipartFactory {
        factory: Arc<dyn MultipartFactory>,
        current: Option<MultipartBody>,
        boundary: MultipartBoundary,
        framing: MultipartFraming,
    },
}

impl ManagedBody {
    pub(crate) const fn kind(&self) -> BodyKind {
        match &self.source {
            ManagedSource::Empty => BodyKind::Empty,
            ManagedSource::Bytes(_) => BodyKind::Buffered,
            ManagedSource::OneShotStream { .. } | ManagedSource::StreamFactory { .. } => {
                BodyKind::Stream
            }
            #[cfg(feature = "multipart")]
            ManagedSource::ReplayableMultipart { .. }
            | ManagedSource::OneShotMultipart { .. }
            | ManagedSource::MultipartFactory { .. } => BodyKind::Multipart,
        }
    }

    /// Returns the exact body-owned `Content-Type`, if any.
    pub(crate) const fn content_type(&self) -> Option<&HeaderValue> {
        self.content_type.as_ref()
    }

    pub(crate) const fn replayability(&self) -> Replayability {
        match &self.source {
            ManagedSource::Empty | ManagedSource::Bytes(_) => Replayability::Reusable,
            ManagedSource::OneShotStream { .. } => Replayability::OneShot,
            ManagedSource::StreamFactory { .. } => Replayability::Reconstructible,
            #[cfg(feature = "multipart")]
            ManagedSource::ReplayableMultipart { .. } => Replayability::Reusable,
            #[cfg(feature = "multipart")]
            ManagedSource::OneShotMultipart { .. } => Replayability::OneShot,
            #[cfg(feature = "multipart")]
            ManagedSource::MultipartFactory { .. } => Replayability::Reconstructible,
        }
    }

    /// Returns the length facts of the body the next attempt will send.
    ///
    /// A reconstructible body whose current stream or recipe was already
    /// consumed reports `Unknown` until the factory creates the next one, so
    /// observers never see a previous attempt's length as a promise.
    pub(crate) fn length_hint(&self) -> LengthHint {
        match &self.source {
            ManagedSource::Empty => LengthHint::Exact(0),
            ManagedSource::Bytes(bytes) => LengthHint::Exact(saturating_u64(bytes.len())),
            ManagedSource::OneShotStream { length, .. } => length.hint(),
            ManagedSource::StreamFactory {
                current: Some(_),
                length,
                ..
            } => length.hint(),
            ManagedSource::StreamFactory { current: None, .. } => LengthHint::Unknown,
            #[cfg(feature = "multipart")]
            ManagedSource::ReplayableMultipart { framing, .. }
            | ManagedSource::OneShotMultipart { framing, .. }
            | ManagedSource::MultipartFactory {
                current: Some(_),
                framing,
                ..
            } => framing.length_hint(),
            #[cfg(feature = "multipart")]
            ManagedSource::MultipartFactory { current: None, .. } => LengthHint::Unknown,
        }
    }

    pub(crate) fn known_length(&self) -> Option<u64> {
        match self.length_hint() {
            LengthHint::Exact(length) => Some(length),
            LengthHint::AtLeast(_) | LengthHint::Unknown => None,
        }
    }

    pub(crate) fn prepare_native(&mut self, limit: NonZeroUsize) -> Result<NativeBody, BodyError> {
        match &mut self.source {
            ManagedSource::Empty => Ok(NativeBody::Empty),
            ManagedSource::Bytes(bytes) => Ok(NativeBody::Bytes(bytes.clone())),
            ManagedSource::OneShotStream { body, length } => {
                let body = body
                    .take()
                    .ok_or_else(|| BodyError::new(BodyErrorKind::InvalidLength))?;
                Ok(stream_native_with_length(body, *length, limit))
            }
            ManagedSource::StreamFactory {
                current, length, ..
            } => {
                let body = current
                    .take()
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Factory))?;
                Ok(stream_native_with_length(body, *length, limit))
            }
            #[cfg(feature = "multipart")]
            ManagedSource::ReplayableMultipart {
                recipe,
                boundary,
                framing,
            } => {
                let body = recipe
                    .clone_reusable()?
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Multipart))?;
                multipart_native(body, boundary, *framing, limit)
            }
            #[cfg(feature = "multipart")]
            ManagedSource::OneShotMultipart {
                body,
                boundary,
                framing,
            } => {
                let body = body
                    .take()
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Multipart))?;
                multipart_native(body, boundary, *framing, limit)
            }
            #[cfg(feature = "multipart")]
            ManagedSource::MultipartFactory {
                current,
                boundary,
                framing,
                ..
            } => {
                let body = current
                    .take()
                    .ok_or_else(|| BodyError::new(BodyErrorKind::Factory))?;
                multipart_native(body, boundary, *framing, limit)
            }
        }
    }

    /// Reconstructs the body for another admitted application attempt
    /// through its mechanical replay capability. The request representation
    /// is never called again.
    pub(crate) fn reconstruct_for_next_attempt(
        &mut self,
        limit: NonZeroUsize,
    ) -> Result<(), BodyError> {
        match &mut self.source {
            ManagedSource::Empty | ManagedSource::Bytes(_) => Ok(()),
            ManagedSource::OneShotStream { .. } => {
                Err(BodyError::new(BodyErrorKind::InvalidLength))
            }
            ManagedSource::StreamFactory {
                factory,
                current,
                length,
            } => {
                let body = create_factory_stream(factory.as_ref())?;
                *length = StreamLength::validated(&body, limit)?;
                *current = Some(body);
                Ok(())
            }
            #[cfg(feature = "multipart")]
            ManagedSource::ReplayableMultipart { .. } => Ok(()),
            #[cfg(feature = "multipart")]
            ManagedSource::OneShotMultipart { .. } => Err(BodyError::new(BodyErrorKind::Multipart)),
            #[cfg(feature = "multipart")]
            ManagedSource::MultipartFactory {
                factory,
                current,
                boundary,
                framing,
            } => {
                let (recipe, next_framing) =
                    create_factory_recipe(factory.as_ref(), boundary, limit)?;
                *framing = next_framing;
                *current = Some(recipe);
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

/// Wraps one request stream in the managed native body with an explicit exact
/// length; the stream's own declared minimum is also enforced.
#[cfg(test)]
fn stream_native(
    body: RequestStream,
    exact_length: Option<u64>,
    limit: NonZeroUsize,
) -> NativeBody {
    let minimum = body.lower_bound();
    stream_native_with_length(
        body,
        StreamLength {
            exact: exact_length,
            minimum,
        },
        limit,
    )
}

fn stream_native_with_length(
    body: RequestStream,
    length: StreamLength,
    limit: NonZeroUsize,
) -> NativeBody {
    native_from_source(body.into_source(), length, limit)
}

fn native_from_source(
    source: super::stream::BodyStream,
    length: StreamLength,
    limit: NonZeroUsize,
) -> NativeBody {
    let failure = BodyFailureState::default();
    let stream = managed_request_stream_with_minimum(
        source,
        length.exact,
        length.minimum,
        limit,
        failure.clone(),
    );
    NativeBody::Stream {
        body: reqwest::Body::wrap_stream(stream),
        known_length: length.exact,
        failure,
    }
}

#[cfg(feature = "multipart")]
fn multipart_native(
    body: MultipartBody,
    boundary: &MultipartBoundary,
    framing: MultipartFraming,
    limit: NonZeroUsize,
) -> Result<NativeBody, BodyError> {
    let source = body.into_stream(boundary)?;
    let (exact, minimum) = framing.bounds();
    Ok(native_from_source(
        source,
        StreamLength {
            exact,
            minimum: Some(minimum),
        },
        limit,
    ))
}

#[cfg(test)]
fn managed_request_stream(
    source: super::stream::BodyStream,
    exact_length: Option<u64>,
    limit: NonZeroUsize,
    failure: BodyFailureState,
) -> ManagedRequestStream {
    managed_request_stream_with_minimum(source, exact_length, None, limit, failure)
}

fn managed_request_stream_with_minimum(
    source: super::stream::BodyStream,
    exact_length: Option<u64>,
    minimum_length: Option<u64>,
    limit: NonZeroUsize,
    failure: BodyFailureState,
) -> ManagedRequestStream {
    ManagedRequestStream {
        source: Some(source),
        exact_length,
        minimum_length,
        limit: saturating_u64(limit.get()),
        observed: 0,
        empty_chunks_seen: 0,
        final_chunk: None,
        finished: false,
        failure,
    }
}

struct ManagedRequestStream {
    source: Option<super::stream::BodyStream>,
    exact_length: Option<u64>,
    minimum_length: Option<u64>,
    limit: u64,
    observed: u64,
    empty_chunks_seen: usize,
    // Native Content-Length framing may stop polling after the declared
    // bytes. Retain at most the chunk reaching that boundary until source EOF
    // proves the exact length; no extra buffer or eager source polling is
    // needed. Empty-chunk and operation limits still govern that final read.
    final_chunk: Option<Bytes>,
    finished: bool,
    failure: BodyFailureState,
}

impl ManagedRequestStream {
    fn fail(&mut self, kind: BodyErrorKind) -> Poll<Option<Result<Bytes, BodyError>>> {
        self.finished = true;
        self.source = None;
        self.final_chunk = None;
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

        let mut empty_chunks_this_poll = 0_usize;
        loop {
            let polled = this
                .source
                .as_mut()
                .expect("an unfinished managed request stream retains its source")
                .as_mut()
                .poll_next(context);
            match polled {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Err(error))) => {
                    let kind = error.kind();
                    drop(error);
                    return this.fail(kind);
                }
                Poll::Ready(Some(Ok(bytes))) if bytes.is_empty() => {
                    let Some(next) = this.empty_chunks_seen.checked_add(1) else {
                        return this.fail(BodyErrorKind::LimitExceeded);
                    };
                    if next > MAX_EMPTY_REQUEST_CHUNKS {
                        return this.fail(BodyErrorKind::LimitExceeded);
                    }
                    this.empty_chunks_seen = next;
                    empty_chunks_this_poll += 1;
                    if empty_chunks_this_poll == MAX_EMPTY_REQUEST_CHUNKS_PER_POLL {
                        context.waker().wake_by_ref();
                        return Poll::Pending;
                    }
                }
                Poll::Ready(Some(Ok(bytes))) => {
                    let Some(next) = this.observed.checked_add(saturating_u64(bytes.len())) else {
                        return this.fail(BodyErrorKind::LimitExceeded);
                    };
                    if this.exact_length.is_some_and(|exact| next > exact) {
                        return this.fail(BodyErrorKind::InvalidLength);
                    }
                    if next > this.limit {
                        return this.fail(BodyErrorKind::LimitExceeded);
                    }
                    this.observed = next;
                    if this.exact_length == Some(next) {
                        this.final_chunk = Some(bytes);
                        continue;
                    }
                    return Poll::Ready(Some(Ok(bytes)));
                }
                Poll::Ready(None) => {
                    if this
                        .exact_length
                        .is_some_and(|exact| this.observed != exact)
                        || this
                            .minimum_length
                            .is_some_and(|minimum| this.observed < minimum)
                    {
                        return this.fail(BodyErrorKind::InvalidLength);
                    }
                    this.finished = true;
                    this.source = None;
                    return Poll::Ready(this.final_chunk.take().map(Ok));
                }
            }
        }
    }
}

/// Validates one concrete request media type and returns its header value.
pub(crate) fn validated_content_type(value: &str) -> Result<HeaderValue, BodyError> {
    if value.is_empty() || value.len() > MAX_REQUEST_CONTENT_TYPE_BYTES {
        return Err(BodyError::new(BodyErrorKind::InvalidContentType));
    }
    let parsed = mime::Mime::from_str(value)
        .map_err(|_| BodyError::new(BodyErrorKind::InvalidContentType))?;
    if parsed.type_() == mime::STAR || parsed.subtype() == mime::STAR {
        return Err(BodyError::new(BodyErrorKind::InvalidContentType));
    }
    HeaderValue::from_str(value).map_err(|_| BodyError::new(BodyErrorKind::InvalidContentType))
}

fn saturating_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
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
