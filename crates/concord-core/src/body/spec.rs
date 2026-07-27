use std::marker::PhantomData;
use std::num::NonZeroUsize;

use bytes::Bytes;
use http::{HeaderValue, Method};

use crate::codec::{AnyMedia, EncodeBody, EncodeContext, MediaType, Text};

#[cfg(feature = "multipart")]
use super::MultipartBody;
use super::prepared::validated_content_type;
use super::{BodyError, BodyErrorKind, BodyKind, RequestBody, StreamBody};

/// Structural context supplied to a request-body specification.
///
/// It intentionally excludes the URL, headers, credentials, authentication,
/// limiter state, native request, Reqwest types, and transport state.
pub struct RequestBodyContext<'a> {
    /// Stable generated or runtime endpoint identity.
    pub endpoint: &'a str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Positive finite aggregate request-body limit.
    pub body_limit: NonZeroUsize,
}

/// Structural request-body length information available before hooks.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LengthHint {
    /// No trustworthy length information is available.
    Unknown,
    /// The produced body is at least this many bytes.
    AtLeast(u64),
    /// The produced body has exactly this many bytes.
    Exact(u64),
}

/// Request `Content-Type` ownership available before hooks.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestContentType {
    /// The specification does not own `Content-Type`.
    None,
    /// The specification owns one exact validated value.
    Exact(HeaderValue),
    /// Native serialization owns generation of the value.
    Generated,
}

/// Structural request-body facts computed before the before-send hook.
///
/// These facts describe the eventual managed body without encoding a codec,
/// creating a body or multipart factory result, polling a stream, opening a
/// file, or constructing a native body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestBodyMetadata {
    /// Trustworthy length information that requires no materialization.
    pub length: LengthHint,
    /// Direction-specific request `Content-Type` ownership.
    pub content_type: RequestContentType,
    /// Structural managed-body category.
    pub kind: BodyKind,
}

impl RequestBodyMetadata {
    pub(crate) const fn empty() -> Self {
        Self {
            length: LengthHint::Exact(0),
            content_type: RequestContentType::None,
            kind: BodyKind::Empty,
        }
    }

    pub(crate) const fn known_length(&self) -> Option<u64> {
        match self.length {
            LengthHint::Unknown => None,
            LengthHint::AtLeast(value) | LengthHint::Exact(value) => Some(value),
        }
    }

    pub(crate) fn with_known_length(mut self, length: Option<u64>) -> Self {
        if let Some(length) = length {
            self.length = LengthHint::Exact(length);
        }
        self
    }

    pub(crate) fn validate_limit(&self, limit: NonZeroUsize) -> Result<(), BodyError> {
        let limit = u64::try_from(limit.get()).unwrap_or(u64::MAX);
        if self.known_length().is_some_and(|length| length > limit) {
            Err(BodyError::new(BodyErrorKind::LimitExceeded))
        } else {
            Ok(())
        }
    }
}

/// Defines one request input and its single managed outgoing-body lifecycle.
///
/// `metadata` runs before the before-send hook and must be structural: it must
/// not encode data, create a body or multipart factory result, poll a stream,
/// open a file, or access request, credential, limiter, Reqwest, or transport
/// state. A hook veto therefore prevents `prepare`.
///
/// `prepare` runs synchronously at most once for one pending request execution,
/// after hook approval. It must return the one public [`RequestBody`]; Concord
/// derives replayability from that value, validates it against the earlier
/// metadata, and checks the operation deadline immediately after the callback.
/// Factories stored inside the returned body remain attempt-lazy. Calls for
/// independent requests may overlap. Inputs may be sensitive; implementations
/// own their logging, storage, CPU, memory, metric cardinality, and external
/// I/O behavior.
///
/// A custom specification may return a managed stream whose source returns
/// `Pending`, performs trusted application work, or reports a safe producer
/// error. Core suppresses and finitely bounds successful empty chunks before
/// transport; extensions may not use them as an unbounded progress mechanism.
///
/// Ordinary complete buffered encoders should implement
/// [`EncodeBody`](crate::codec::EncodeBody) instead; Concord supplies this
/// specification through a blanket implementation.
pub trait RequestBodySpec: Send + Sync + 'static {
    /// Application input stored by a pending request.
    type Input: Send + 'static;

    /// Computes pre-hook structural facts without materializing the body.
    fn metadata(
        input: &Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError>;

    /// Produces the one managed outgoing request body after hook approval.
    fn prepare(
        input: Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError>;
}

/// Trusted opt-in for a page-stable repeatable request-body specification.
///
/// Every clone of [`RequestBodySpec::Input`] must independently prepare the
/// same request body. Preparing or consuming one clone must not invalidate any
/// other clone. Generated pagination relies on that promise when it clones the
/// original endpoint for each page.
///
/// `Clone` is necessary but does not establish this semantic contract by
/// itself. Specifications backed by shared one-shot state must not implement
/// this trait.
pub trait PageRequestBodySpec: RequestBodySpec
where
    Self::Input: Clone,
{
}

impl<C> RequestBodySpec for C
where
    C: EncodeBody,
{
    type Input = C::Input;

    fn metadata(
        _input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        let content_type = C::CONTENT_TYPE
            .map(validated_content_type)
            .transpose()?
            .map_or(RequestContentType::None, RequestContentType::Exact);
        Ok(RequestBodyMetadata {
            length: LengthHint::Unknown,
            content_type,
            kind: BodyKind::Buffered,
        })
    }

    fn prepare(
        input: Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        let bytes = C::encode(
            input,
            EncodeContext {
                endpoint: context.endpoint,
                method: context.method,
                body_limit: Some(context.body_limit.get()),
            },
        )
        .map_err(|_| BodyError::new(BodyErrorKind::Producer))?;
        if bytes.len() > context.body_limit.get() {
            return Err(BodyError::new(BodyErrorKind::LimitExceeded));
        }
        let mut body = RequestBody::bytes(bytes);
        if let Some(content_type) = C::CONTENT_TYPE {
            body = body.content_type(content_type)?;
        }
        Ok(body)
    }
}

impl RequestBodySpec for Bytes {
    type Input = Bytes;

    fn metadata(
        input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        Ok(RequestBodyMetadata {
            length: LengthHint::Exact(u64::try_from(input.len()).unwrap_or(u64::MAX)),
            content_type: RequestContentType::None,
            kind: BodyKind::Buffered,
        })
    }

    fn prepare(
        input: Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        Ok(RequestBody::bytes(input))
    }
}

impl PageRequestBodySpec for Bytes {}

impl PageRequestBodySpec for Text {}

/// Bounded streaming request and response specification.
///
/// Bare `Stream` is media-unconstrained. `Stream<M>` for a [`MediaType`]
/// marker owns one exact request `Content-Type` and response media assertion.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stream<M = AnyMedia>(PhantomData<M>);

impl RequestBodySpec for Stream<AnyMedia> {
    type Input = StreamBody;

    fn metadata(
        input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        input.validate_metadata()?;
        let length = match input.exact_length_value() {
            Some(length) => LengthHint::Exact(length),
            None => input
                .lower_bound()
                .map_or(LengthHint::Unknown, LengthHint::AtLeast),
        };
        Ok(RequestBodyMetadata {
            length,
            content_type: RequestContentType::None,
            kind: BodyKind::Stream,
        })
    }

    fn prepare(
        input: Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        Ok(RequestBody::stream(input))
    }
}

impl<M> RequestBodySpec for Stream<M>
where
    M: MediaType,
{
    type Input = StreamBody;

    fn metadata(
        input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        input.validate_metadata()?;
        let length = match input.exact_length_value() {
            Some(length) => LengthHint::Exact(length),
            None => input
                .lower_bound()
                .map_or(LengthHint::Unknown, LengthHint::AtLeast),
        };
        Ok(RequestBodyMetadata {
            length,
            content_type: RequestContentType::Exact(validated_content_type(M::MEDIA_TYPE)?),
            kind: BodyKind::Stream,
        })
    }

    fn prepare(
        input: Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        RequestBody::stream(input).content_type(M::MEDIA_TYPE)
    }
}

/// Request-only multipart body specification.
#[cfg(feature = "multipart")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Multipart;

#[cfg(feature = "multipart")]
impl RequestBodySpec for Multipart {
    type Input = MultipartBody;

    fn metadata(
        input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        let metadata = input.metadata()?;
        Ok(RequestBodyMetadata {
            length: LengthHint::AtLeast(metadata.lower_bound),
            content_type: RequestContentType::Generated,
            kind: BodyKind::Multipart,
        })
    }

    fn prepare(
        input: Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        Ok(RequestBody::multipart(input))
    }
}

impl RequestBodySpec for RequestBody {
    type Input = RequestBody;

    fn metadata(
        input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        input.structural_metadata()
    }

    fn prepare(
        input: Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        Ok(input)
    }
}

#[doc(hidden)]
pub struct EmptyBody;

impl RequestBodySpec for EmptyBody {
    type Input = ();

    fn metadata(
        _input: &Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        Ok(RequestBodyMetadata::empty())
    }

    fn prepare(
        (): Self::Input,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        Ok(RequestBody::empty())
    }
}

impl PageRequestBodySpec for EmptyBody {}

trait ErasedRequestBody: Send {
    fn metadata(&self, context: RequestBodyContext<'_>) -> Result<RequestBodyMetadata, BodyError>;

    fn prepare(self: Box<Self>, context: RequestBodyContext<'_>) -> Result<RequestBody, BodyError>;
}

struct SelectedRequestBody<B>
where
    B: RequestBodySpec,
{
    input: B::Input,
    marker: PhantomData<fn() -> B>,
}

impl<B> ErasedRequestBody for SelectedRequestBody<B>
where
    B: RequestBodySpec,
{
    fn metadata(&self, context: RequestBodyContext<'_>) -> Result<RequestBodyMetadata, BodyError> {
        B::metadata(&self.input, context)
    }

    fn prepare(self: Box<Self>, context: RequestBodyContext<'_>) -> Result<RequestBody, BodyError> {
        B::prepare(self.input, context)
    }
}

/// Core-private owned request-specification selection for runtime endpoints.
#[doc(hidden)]
pub struct PreparedRequestBody {
    body: Box<dyn ErasedRequestBody>,
}

impl PreparedRequestBody {
    pub(crate) fn new<B>(input: B::Input) -> Self
    where
        B: RequestBodySpec,
    {
        Self {
            body: Box::new(SelectedRequestBody::<B> {
                input,
                marker: PhantomData,
            }),
        }
    }
}

/// Core-private request specification used by runtime endpoints.
#[doc(hidden)]
pub struct PreparedBodySpec;

impl RequestBodySpec for PreparedBodySpec {
    type Input = PreparedRequestBody;

    fn metadata(
        input: &Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        input.body.metadata(context)
    }

    fn prepare(
        input: Self::Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        input.body.prepare(context)
    }
}

pub(crate) fn validate_produced_body(
    expected: &RequestBodyMetadata,
    body: &RequestBody,
) -> Result<(), BodyError> {
    let actual = body.structural_metadata()?;
    if expected.kind != actual.kind {
        return Err(BodyError::new(BodyErrorKind::Internal));
    }
    match expected.length {
        LengthHint::Unknown => {}
        LengthHint::Exact(expected) => {
            if actual.length != LengthHint::Exact(expected) {
                return Err(BodyError::new(BodyErrorKind::InvalidLength));
            }
        }
        LengthHint::AtLeast(expected) => match actual.length {
            LengthHint::Exact(actual) | LengthHint::AtLeast(actual) if actual >= expected => {}
            _ => return Err(BodyError::new(BodyErrorKind::InvalidLength)),
        },
    }
    if expected.content_type != actual.content_type {
        return Err(BodyError::new(BodyErrorKind::InvalidContentType));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::OctetStream;

    fn context() -> RequestBodyContext<'static> {
        RequestBodyContext {
            endpoint: "stream.metadata",
            method: &Method::POST,
            body_limit: NonZeroUsize::new(1024).expect("positive test body limit"),
        }
    }

    #[test]
    fn media_unconstrained_stream_owns_no_content_type() {
        let input = StreamBody::from_bytes(Bytes::from_static(b"bare-stream"));
        let metadata =
            <Stream as RequestBodySpec>::metadata(&input, context()).expect("bare stream metadata");
        assert_eq!(metadata.length, LengthHint::Exact(11));
        assert_eq!(metadata.content_type, RequestContentType::None);
        assert_eq!(metadata.kind, BodyKind::Stream);

        let body =
            <Stream as RequestBodySpec>::prepare(input, context()).expect("bare stream body");
        assert_eq!(
            body.structural_metadata()
                .expect("prepared bare stream metadata")
                .content_type,
            RequestContentType::None
        );
    }

    #[test]
    fn exact_media_stream_keeps_its_owned_content_type() {
        let input = StreamBody::from_bytes(Bytes::from_static(b"exact-stream"));
        let metadata = <Stream<OctetStream> as RequestBodySpec>::metadata(&input, context())
            .expect("exact stream metadata");
        assert_eq!(
            metadata.content_type,
            RequestContentType::Exact(HeaderValue::from_static("application/octet-stream"))
        );

        let body = <Stream<OctetStream> as RequestBodySpec>::prepare(input, context())
            .expect("exact stream body");
        assert_eq!(
            body.structural_metadata()
                .expect("prepared exact stream metadata")
                .content_type,
            RequestContentType::Exact(HeaderValue::from_static("application/octet-stream"))
        );
    }
}
