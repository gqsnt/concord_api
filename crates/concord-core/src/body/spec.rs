use std::marker::PhantomData;
use std::num::NonZeroUsize;

use bytes::Bytes;
use http::{HeaderValue, Method};

use crate::codec::{Binary, EncodeBody, EncodeContext, Text};

#[cfg(feature = "multipart")]
use super::MultipartBody;
use super::prepared::validated_content_type;
use super::{BodyError, BodyErrorKind, BodyKind, RequestBody, RequestStream};

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
pub trait RequestRepresentation<Input>: Send + Sync + 'static
where
    Input: Send + 'static,
{
    /// Computes pre-hook structural facts without materializing the body.
    fn metadata(
        input: &Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError>;

    /// Produces the one managed outgoing request body after hook approval.
    fn prepare(input: Input, context: RequestBodyContext<'_>) -> Result<RequestBody, BodyError>;
}

/// Trusted opt-in for a page-stable repeatable request-body specification.
///
/// Every clone of the exact `Input` type must independently prepare the same
/// request body. Preparing or consuming one clone must not invalidate any other
/// clone. Generated pagination relies on that promise when it clones the
/// original endpoint for each page.
///
/// `Clone` is necessary but does not establish this semantic contract by
/// itself. Specifications backed by shared one-shot state must not implement
/// this trait.
pub trait PageRequestRepresentation<Input>: RequestRepresentation<Input>
where
    Input: Clone + Send + 'static,
{
}

/// Generated-private adapter that gives an authored optional body its outer
/// absence lifecycle without asking the selected representation to interpret
/// `Option`.
#[doc(hidden)]
pub struct OptionalRequestRepresentation<R>(PhantomData<fn() -> R>);

impl<R, Input> RequestRepresentation<Option<Input>> for OptionalRequestRepresentation<R>
where
    R: RequestRepresentation<Input>,
    Input: Send + 'static,
{
    fn metadata(
        input: &Option<Input>,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        input.as_ref().map_or_else(
            || Ok(RequestBodyMetadata::empty()),
            |input| R::metadata(input, context),
        )
    }

    fn prepare(
        input: Option<Input>,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        input.map_or_else(
            || Ok(RequestBody::empty()),
            |input| R::prepare(input, context),
        )
    }
}

impl<R, Input> PageRequestRepresentation<Option<Input>> for OptionalRequestRepresentation<R>
where
    R: PageRequestRepresentation<Input>,
    Input: Clone + Send + 'static,
{
}

impl<C, Input> RequestRepresentation<Input> for C
where
    C: EncodeBody<Input = Input>,
    Input: Send + 'static,
{
    fn metadata(
        _input: &Input,
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

    fn prepare(input: Input, context: RequestBodyContext<'_>) -> Result<RequestBody, BodyError> {
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

impl RequestRepresentation<Bytes> for Binary {
    fn metadata(
        input: &Bytes,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        binary_metadata(input.len())
    }

    fn prepare(input: Bytes, _context: RequestBodyContext<'_>) -> Result<RequestBody, BodyError> {
        RequestBody::bytes(input).content_type("application/octet-stream")
    }
}

impl PageRequestRepresentation<Bytes> for Binary {}

impl RequestRepresentation<Vec<u8>> for Binary {
    fn metadata(
        input: &Vec<u8>,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        binary_metadata(input.len())
    }

    fn prepare(input: Vec<u8>, _context: RequestBodyContext<'_>) -> Result<RequestBody, BodyError> {
        RequestBody::bytes(Bytes::from(input)).content_type("application/octet-stream")
    }
}

impl PageRequestRepresentation<Vec<u8>> for Binary {}

impl PageRequestRepresentation<String> for Text {}

fn binary_metadata(length: usize) -> Result<RequestBodyMetadata, BodyError> {
    Ok(RequestBodyMetadata {
        length: LengthHint::Exact(u64::try_from(length).unwrap_or(u64::MAX)),
        content_type: RequestContentType::Exact(validated_content_type(
            "application/octet-stream",
        )?),
        kind: BodyKind::Buffered,
    })
}

impl RequestRepresentation<RequestStream<Bytes>> for Binary {
    fn metadata(
        input: &RequestStream<Bytes>,
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
            content_type: RequestContentType::Exact(validated_content_type(
                "application/octet-stream",
            )?),
            kind: BodyKind::Stream,
        })
    }

    fn prepare(
        input: RequestStream<Bytes>,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        RequestBody::stream(input).content_type("application/octet-stream")
    }
}

/// Request-only multipart body specification.
#[cfg(feature = "multipart")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Multipart;

#[cfg(feature = "multipart")]
impl RequestRepresentation<MultipartBody> for Multipart {
    fn metadata(
        input: &MultipartBody,
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
        input: MultipartBody,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        Ok(RequestBody::multipart(input))
    }
}

impl RequestRepresentation<RequestBody> for RequestBody {
    fn metadata(
        input: &RequestBody,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        input.structural_metadata()
    }

    fn prepare(
        input: RequestBody,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBody, BodyError> {
        Ok(input)
    }
}

#[doc(hidden)]
pub struct EmptyBody;

impl RequestRepresentation<()> for EmptyBody {
    fn metadata(
        _input: &(),
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        Ok(RequestBodyMetadata::empty())
    }

    fn prepare((): (), _context: RequestBodyContext<'_>) -> Result<RequestBody, BodyError> {
        Ok(RequestBody::empty())
    }
}

impl PageRequestRepresentation<()> for EmptyBody {}

trait ErasedRequestBody: Send {
    fn metadata(&self, context: RequestBodyContext<'_>) -> Result<RequestBodyMetadata, BodyError>;

    fn prepare(self: Box<Self>, context: RequestBodyContext<'_>) -> Result<RequestBody, BodyError>;
}

struct SelectedRequestBody<B, Input>
where
    B: RequestRepresentation<Input>,
    Input: Send + 'static,
{
    input: Input,
    marker: PhantomData<fn() -> (B, Input)>,
}

impl<B, Input> ErasedRequestBody for SelectedRequestBody<B, Input>
where
    B: RequestRepresentation<Input>,
    Input: Send + 'static,
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
    pub(crate) fn new<B, Input>(input: Input) -> Self
    where
        B: RequestRepresentation<Input>,
        Input: Send + 'static,
    {
        Self {
            body: Box::new(SelectedRequestBody::<B, Input> {
                input,
                marker: PhantomData,
            }),
        }
    }
}

/// Core-private request specification used by runtime endpoints.
#[doc(hidden)]
pub struct PreparedBodySpec;

impl RequestRepresentation<PreparedRequestBody> for PreparedBodySpec {
    fn metadata(
        input: &PreparedRequestBody,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        input.body.metadata(context)
    }

    fn prepare(
        input: PreparedRequestBody,
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

    fn context() -> RequestBodyContext<'static> {
        RequestBodyContext {
            endpoint: "stream.metadata",
            method: &Method::POST,
            body_limit: NonZeroUsize::new(1024).expect("positive test body limit"),
        }
    }

    #[test]
    fn exact_media_stream_keeps_its_owned_content_type() {
        let input = RequestStream::from_bytes(Bytes::from_static(b"exact-stream"));
        let metadata =
            <Binary as RequestRepresentation<RequestStream>>::metadata(&input, context())
                .expect("exact stream metadata");
        assert_eq!(
            metadata.content_type,
            RequestContentType::Exact(HeaderValue::from_static("application/octet-stream"))
        );

        let body = <Binary as RequestRepresentation<RequestStream>>::prepare(input, context())
            .expect("exact stream body");
        assert_eq!(
            body.structural_metadata()
                .expect("prepared exact stream metadata")
                .content_type,
            RequestContentType::Exact(HeaderValue::from_static("application/octet-stream"))
        );
    }
}
