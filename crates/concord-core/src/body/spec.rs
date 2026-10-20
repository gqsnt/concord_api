use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::time::Duration;

use bytes::Bytes;
use http::Method;

use crate::codec::{Binary, CodecErrorKind, EncodeBody, EncodeContext, Text};
use crate::endpoint::EndpointId;

#[cfg(feature = "multipart")]
use super::MultipartBody;
use super::{BodyError, BodyErrorKind, BodyKind, PreparedBody, RequestStream};

/// Maximum byte length of one static [`BodyDescription`] media hint.
pub const MAX_BODY_MEDIA_HINT_BYTES: usize = 256;

/// Safe structural context supplied to [`RequestRepresentation::prepare`].
///
/// It carries the validated endpoint identity, the application method, the
/// positive configured request-body limit and the remaining operation time at
/// preparation. It intentionally excludes the URL, headers, credentials,
/// limiter state, the client, the native request and transport state.
///
/// The context grants no authority: constructing one (for example in a
/// downstream unit test through [`RequestBodyContext::new`]) cannot send,
/// authenticate or bypass any limit enforced by core after `prepare` returns.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct RequestBodyContext<'a> {
    /// Validated operational endpoint ID (an explicit `id` pin or navigation-derived).
    pub endpoint: &'a EndpointId,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Positive finite aggregate request-body limit in bytes.
    pub body_limit: NonZeroUsize,
    /// Remaining operation time when preparation started, or `None` when the
    /// operation has no deadline. Core rechecks the deadline after `prepare`
    /// returns.
    pub remaining_time: Option<Duration>,
}

impl<'a> RequestBodyContext<'a> {
    /// Creates a context with explicit facts.
    ///
    /// Core constructs the context for every managed preparation; this
    /// constructor exists so downstream representations can be exercised in
    /// isolation.
    #[must_use]
    pub const fn new(
        endpoint: &'a EndpointId,
        method: &'a Method,
        body_limit: NonZeroUsize,
        remaining_time: Option<Duration>,
    ) -> Self {
        Self {
            endpoint,
            method,
            body_limit,
            remaining_time,
        }
    }
}

/// Trustworthy request-body length information.
///
/// Core validates every hint against observed production. Producing more
/// bytes than an exact hint, fewer bytes than an exact or minimum hint, or
/// more bytes than the configured limit fails with a [`BodyError`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LengthHint {
    /// No trustworthy length information is available.
    Unknown,
    /// The produced body is at least this many bytes.
    AtLeast(u64),
    /// The produced body has exactly this many bytes.
    Exact(u64),
}

/// Static, bounded explanation metadata for one request representation.
///
/// A description names the known body family and an optional media-family
/// hint. It is read by descriptions and plans without evaluating input or
/// calling a representation, and it is **not** an exact header promise: the
/// actual `Content-Type` is owned solely by the [`PreparedBody`] returned from
/// [`RequestRepresentation::prepare`]. Plans therefore label the exact
/// `Content-Type` and body length as deferred to preparation, and a multipart
/// family is described as "multipart/form-data; boundary generated during
/// execution".
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BodyDescription {
    family: Option<BodyKind>,
    media_hint: Option<&'static str>,
}

impl BodyDescription {
    /// No static body facts are known.
    pub const UNKNOWN: Self = Self {
        family: None,
        media_hint: None,
    };

    /// An explicitly empty body.
    pub const EMPTY: Self = Self {
        family: Some(BodyKind::Empty),
        media_hint: None,
    };

    /// A multipart body whose boundary is generated during execution.
    pub const MULTIPART: Self = Self {
        family: Some(BodyKind::Multipart),
        media_hint: Some("multipart/form-data"),
    };

    /// Describes a known body family without a media hint.
    #[must_use]
    pub const fn new(family: BodyKind) -> Self {
        Self {
            family: Some(family),
            media_hint: None,
        }
    }

    /// Describes a complete buffered body with an optional media hint.
    ///
    /// A hint that is not a valid bounded media hint (see
    /// [`BodyDescription::with_media_hint`]) is omitted rather than rejected,
    /// because a description is explanation metadata, never the header.
    #[must_use]
    pub const fn buffered(media_hint: Option<&'static str>) -> Self {
        let description = Self::new(BodyKind::Buffered);
        match media_hint {
            Some(hint) if valid_media_hint(hint) => Self {
                family: description.family,
                media_hint: Some(hint),
            },
            _ => description,
        }
    }

    /// Adds a static media-family hint.
    ///
    /// # Panics
    ///
    /// Panics when `hint` is empty, longer than
    /// [`MAX_BODY_MEDIA_HINT_BYTES`] or contains a byte outside visible ASCII
    /// and space. In a `const` item (the intended use, for
    /// [`RequestRepresentation::DESCRIPTION`]) the panic is a compile-time
    /// error.
    #[must_use]
    pub const fn with_media_hint(self, hint: &'static str) -> Self {
        assert!(
            valid_media_hint(hint),
            "a body description media hint must be 1..=256 visible ASCII bytes"
        );
        Self {
            family: self.family,
            media_hint: Some(hint),
        }
    }

    /// Returns the known body family, if any.
    #[must_use]
    pub const fn family(self) -> Option<BodyKind> {
        self.family
    }

    /// Returns the static media-family hint, if any.
    #[must_use]
    pub const fn media_hint(self) -> Option<&'static str> {
        self.media_hint
    }

    /// Returns whether a boundary is generated during execution rather than
    /// being statically known.
    #[must_use]
    pub const fn boundary_generated_during_execution(self) -> bool {
        matches!(self.family, Some(BodyKind::Multipart))
    }
}

impl fmt::Display for BodyDescription {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.family, self.media_hint) {
            (Some(BodyKind::Multipart), hint) => write!(
                formatter,
                "{}; boundary generated during execution",
                hint.unwrap_or("multipart/form-data")
            ),
            (Some(BodyKind::Empty), _) => formatter.write_str("empty body"),
            (Some(family), Some(hint)) => write!(
                formatter,
                "{} body, media hint {hint}; exact Content-Type deferred to preparation",
                family_label(family)
            ),
            (Some(family), None) => write!(
                formatter,
                "{} body; exact Content-Type deferred to preparation",
                family_label(family)
            ),
            (None, Some(hint)) => write!(
                formatter,
                "media hint {hint}; body family and exact Content-Type deferred to preparation"
            ),
            (None, None) => {
                formatter.write_str("body family and exact Content-Type deferred to preparation")
            }
        }
    }
}

const fn family_label(family: BodyKind) -> &'static str {
    match family {
        BodyKind::Empty => "empty",
        BodyKind::Buffered => "buffered",
        BodyKind::Stream => "streaming",
        BodyKind::Multipart => "multipart",
    }
}

const fn valid_media_hint(hint: &str) -> bool {
    let bytes = hint.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_BODY_MEDIA_HINT_BYTES {
        return false;
    }
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte < 0x20 || byte > 0x7e {
            return false;
        }
        index += 1;
    }
    true
}

/// Defines one request input and its single managed prepared body.
///
/// `prepare` is the **sole** authority for the request body: it returns one
/// [`PreparedBody`] whose core constructors own the actual `Content-Type`,
/// trustworthy length information and mechanical replay capability. There is
/// no second metadata method whose answer could disagree with the prepared
/// value. [`RequestRepresentation::DESCRIPTION`] is static explanation data
/// only.
///
/// Ordinary complete buffered encoders can implement
/// [`EncodeBody`](crate::codec::EncodeBody) instead; a blanket implementation
/// bridges it to this trait with one media source.
///
/// # Lifecycle
///
/// Core calls `prepare` at most once per direct operation or pagination page,
/// after public head selection and before credential acquisition and rate
/// admission. An outer-absent optional body never reaches the representation.
/// Automatic attempts within the same page reuse or reconstruct the prepared
/// body through its mechanical replay capability; `prepare` is not called
/// again because a retry was allowed. Core validates the final media ownership
/// and known length against the configured limit before credentials or
/// dispatch, and checks the operation deadline before and after the call.
///
/// # Extension contract
///
/// * **Ownership.** `prepare` consumes the exact body input; the returned
///   prepared body is owned by core for the operation or page.
/// * **Concurrency.** Calls for independent operations may overlap.
/// * **Cancellation.** The call is synchronous and cannot be preempted;
///   dropping the operation afterwards cannot undo work already performed.
/// * **Input sensitivity.** Inputs may be sensitive; implementations own
///   their logging, storage and metrics. Core never formats the input or body
///   bytes.
/// * **Errors.** Return a [`BodyError`]; its optional message must already be
///   safe. Core maps it to a request-body or request-encoding category.
/// * **Resource obligations.** Implementations should respect
///   [`RequestBodyContext::body_limit`] while building bytes and stop at the
///   first excess rather than serialize without bound and truncate.
/// * **Negative capabilities.** A representation cannot read authentication,
///   rate-limit or native request state, cannot supply a conflicting
///   transport header and cannot grant a safe effect or extra attempt.
#[diagnostic::on_unimplemented(
    message = "[C3-REP] `{Self}` is not a request representation for body type `{Input}`",
    label = "selected as the request representation of a `{Input}` body",
    note = "a request body needs `RequestRepresentation<{Input}>` or a complete buffered `EncodeBody` encoder; a response-only representation cannot encode a body",
    note = "write `body name: {Input} via R` with a representation for this input, or select a direction default with `request R;` on an owner"
)]
pub trait RequestRepresentation<Input>: Send + Sync + 'static
where
    Input: Send + 'static,
{
    /// Static bounded explanation metadata; never an exact header promise.
    const DESCRIPTION: BodyDescription = BodyDescription::UNKNOWN;

    /// Produces the one managed prepared body for this operation or page.
    fn prepare(input: Input, context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError>;
}

/// Trusted opt-in for a page-stable repeatable request-body specification.
///
/// Every clone of the exact `Input` type must independently prepare the same
/// semantic body with the same exact content type, independent of page-live
/// values. Preparing or consuming one clone must not invalidate any other
/// clone. Generated pagination relies on that promise when it prepares the
/// selected body for each new page. `Clone` is necessary but does not
/// establish this semantic contract by itself.
///
/// # Extension contract
///
/// * **Ownership.** A traversal keeps one page-stable clone of the captured
///   input and moves a fresh clone into
///   [`RequestRepresentation::prepare`] for each page.
/// * **Concurrency.** The trait adds no callback of its own; clones may be
///   prepared for independent traversals concurrently.
/// * **Cancellation.** Dropping a traversal drops its retained clone; the
///   cancellation obligations of [`RequestRepresentation`] apply per page.
/// * **Input sensitivity.** The retained clone may be sensitive and lives for
///   the whole traversal; core never formats it.
/// * **Errors.** None of its own; a `prepare` failure on a later page fails
///   the traversal at that page.
/// * **Resource obligations.** Cloning the input must be finite and must not
///   share one-shot state; the traversal holds one extra clone at a time.
/// * **Negative capabilities.** The opt-in cannot grant a safe effect, native
///   replay or an extra attempt. Specifications backed by shared one-shot
///   state must not implement this trait.
#[diagnostic::on_unimplemented(
    message = "[C3-PAGE] `{Self}` does not promise page-stable request bodies for `{Input}`",
    label = "a paginated endpoint prepares this body again for every page",
    note = "pagination requires the explicit `PageRequestRepresentation<{Input}>` opt-in; `Clone` alone does not establish the page-body promise",
    note = "select a page-capable representation with `via R` (such as `Json`, `Text` or `Binary`), implement `PageRequestRepresentation<{Input}>` when every clone prepares the same body, or remove `paginate`"
)]
pub trait PageRequestRepresentation<Input>: RequestRepresentation<Input>
where
    Input: Clone + Send + 'static,
{
}

/// Generated-private adapter that gives an authored optional body its outer
/// absence lifecycle without asking the selected representation to interpret
/// `Option`.
///
/// An outer-absent body (`None`) never calls the selected representation,
/// never polls a producer, sends no body bytes and places no body-owned media
/// header.
#[doc(hidden)]
pub struct OptionalRequestRepresentation<R>(PhantomData<fn() -> R>);

impl<R, Input> RequestRepresentation<Option<Input>> for OptionalRequestRepresentation<R>
where
    R: RequestRepresentation<Input>,
    Input: Send + 'static,
{
    const DESCRIPTION: BodyDescription = R::DESCRIPTION;

    fn prepare(
        input: Option<Input>,
        context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        input.map_or_else(
            || Ok(PreparedBody::empty()),
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

/// Maps one category-only codec failure to a body failure.
pub(crate) const fn codec_body_error_kind(kind: CodecErrorKind) -> BodyErrorKind {
    match kind {
        CodecErrorKind::Limit => BodyErrorKind::LimitExceeded,
        CodecErrorKind::Encode | CodecErrorKind::Decode => BodyErrorKind::Producer,
    }
}

impl<C, Input> RequestRepresentation<Input> for C
where
    C: EncodeBody<Input = Input>,
    Input: Send + 'static,
{
    const DESCRIPTION: BodyDescription = BodyDescription::buffered(C::CONTENT_TYPE);

    fn prepare(input: Input, context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        let limit = context.body_limit.get();
        let bytes = C::encode(
            input,
            EncodeContext {
                endpoint: context.endpoint.as_str(),
                method: context.method,
                body_limit: Some(limit),
            },
        )
        .map_err(|error| BodyError::new(codec_body_error_kind(error.kind())))?;
        if bytes.len() > limit {
            return Err(BodyError::new(BodyErrorKind::LimitExceeded));
        }
        let body = PreparedBody::bytes(bytes);
        match C::CONTENT_TYPE {
            Some(content_type) => body.content_type(content_type),
            None => Ok(body),
        }
    }
}

const OCTET_STREAM: &str = "application/octet-stream";

impl RequestRepresentation<Bytes> for Binary {
    const DESCRIPTION: BodyDescription = BodyDescription::buffered(Some(OCTET_STREAM));

    fn prepare(input: Bytes, _context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        PreparedBody::bytes(input).content_type(OCTET_STREAM)
    }
}

impl PageRequestRepresentation<Bytes> for Binary {}

impl RequestRepresentation<Vec<u8>> for Binary {
    const DESCRIPTION: BodyDescription = BodyDescription::buffered(Some(OCTET_STREAM));

    fn prepare(
        input: Vec<u8>,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        PreparedBody::bytes(Bytes::from(input)).content_type(OCTET_STREAM)
    }
}

impl PageRequestRepresentation<Vec<u8>> for Binary {}

impl PageRequestRepresentation<String> for Text {}

impl RequestRepresentation<RequestStream<Bytes>> for Binary {
    const DESCRIPTION: BodyDescription =
        BodyDescription::new(BodyKind::Stream).with_media_hint(OCTET_STREAM);

    fn prepare(
        input: RequestStream<Bytes>,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        input.validate_metadata()?;
        PreparedBody::stream(input).content_type(OCTET_STREAM)
    }
}

/// Request-only multipart body representation.
///
/// Preparation generates and validates one boundary, records the exact
/// `multipart/form-data; boundary=...` media type and retains both with the
/// prepared body. Planning describes the body as "multipart/form-data;
/// boundary generated during execution" without preparing it.
#[cfg(feature = "multipart")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Multipart;

#[cfg(feature = "multipart")]
impl RequestRepresentation<MultipartBody> for Multipart {
    const DESCRIPTION: BodyDescription = BodyDescription::MULTIPART;

    fn prepare(
        input: MultipartBody,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        PreparedBody::multipart(input)
    }
}

impl RequestRepresentation<PreparedBody> for PreparedBody {
    fn prepare(
        input: PreparedBody,
        _context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        Ok(input)
    }
}

/// Generated-private representation of a bodyless endpoint.
#[doc(hidden)]
pub struct EmptyBody;

impl RequestRepresentation<()> for EmptyBody {
    const DESCRIPTION: BodyDescription = BodyDescription::EMPTY;

    fn prepare((): (), _context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        Ok(PreparedBody::empty())
    }
}

impl PageRequestRepresentation<()> for EmptyBody {}

trait ErasedRequestBody: Send {
    fn prepare(self: Box<Self>, context: RequestBodyContext<'_>)
    -> Result<PreparedBody, BodyError>;
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
    fn prepare(
        self: Box<Self>,
        context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        B::prepare(self.input, context)
    }
}

/// Core-private owned request-representation selection for runtime endpoints.
#[doc(hidden)]
pub struct PreparedRequestBody {
    body: Box<dyn ErasedRequestBody>,
    description: BodyDescription,
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
            description: B::DESCRIPTION,
        }
    }

    pub(crate) const fn description(&self) -> BodyDescription {
        self.description
    }
}

/// Core-private request representation used by runtime endpoints.
#[doc(hidden)]
pub struct PreparedBodySpec;

impl RequestRepresentation<PreparedRequestBody> for PreparedBodySpec {
    fn prepare(
        input: PreparedRequestBody,
        context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        input.body.prepare(context)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::CodecError;

    static ENDPOINT: EndpointId = EndpointId::from_static("stream.metadata");
    static POST: Method = Method::POST;

    fn context() -> RequestBodyContext<'static> {
        RequestBodyContext::new(
            &ENDPOINT,
            &POST,
            NonZeroUsize::new(1024).expect("positive test body limit"),
            None,
        )
    }

    #[test]
    fn exact_media_stream_keeps_its_owned_content_type() {
        let input = RequestStream::from_bytes(Bytes::from_static(b"exact-stream"));
        let body = <Binary as RequestRepresentation<RequestStream>>::prepare(input, context())
            .expect("exact stream body");
        assert_eq!(body.media_type(), Some("application/octet-stream"));
        assert_eq!(body.kind(), BodyKind::Stream);
        assert_eq!(body.length_hint(), LengthHint::Exact(12));
        assert!(!body.is_replayable());
    }

    #[test]
    fn optional_absent_body_never_calls_the_representation() {
        struct Panicking;
        impl RequestRepresentation<u8> for Panicking {
            fn prepare(
                _input: u8,
                _context: RequestBodyContext<'_>,
            ) -> Result<PreparedBody, BodyError> {
                panic!("an outer-absent body must never reach its representation")
            }
        }
        let body = <OptionalRequestRepresentation<Panicking> as RequestRepresentation<
            Option<u8>,
        >>::prepare(None, context())
        .expect("absent body");
        assert_eq!(body.kind(), BodyKind::Empty);
        assert_eq!(body.media_type(), None);
        assert_eq!(body.length_hint(), LengthHint::Exact(0));
    }

    #[test]
    fn present_zero_byte_payload_remains_a_present_body() {
        let body = <Text as RequestRepresentation<String>>::prepare(String::new(), context())
            .expect("empty text body");
        assert_eq!(body.kind(), BodyKind::Buffered);
        assert_eq!(body.media_type(), Some("text/plain; charset=utf-8"));
        assert_eq!(body.length_hint(), LengthHint::Exact(0));
    }

    struct LimitedCodec;

    impl EncodeBody for LimitedCodec {
        type Input = usize;
        const CONTENT_TYPE: Option<&'static str> = Some("application/x-limited");

        fn encode(input: usize, context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
            if context.body_limit.is_some_and(|limit| input > limit) {
                return Err(CodecError::new(CodecErrorKind::Limit));
            }
            Ok(Bytes::from(vec![b'x'; input]))
        }
    }

    #[test]
    fn encode_body_bridge_uses_one_media_source_and_maps_codec_categories() {
        assert_eq!(
            <LimitedCodec as RequestRepresentation<usize>>::DESCRIPTION.media_hint(),
            Some("application/x-limited")
        );
        let body = <LimitedCodec as RequestRepresentation<usize>>::prepare(4, context())
            .expect("encoded body");
        assert_eq!(body.media_type(), Some("application/x-limited"));
        assert_eq!(body.length_hint(), LengthHint::Exact(4));
        let error = <LimitedCodec as RequestRepresentation<usize>>::prepare(2048, context())
            .expect_err("codec limit");
        assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    }

    #[test]
    fn descriptions_are_static_bounded_and_render_deferred_facts() {
        const HINTED: BodyDescription = BodyDescription::buffered(Some("application/json"));
        assert_eq!(HINTED.family(), Some(BodyKind::Buffered));
        assert_eq!(HINTED.media_hint(), Some("application/json"));
        assert!(HINTED.to_string().contains("deferred to preparation"));
        assert_eq!(
            BodyDescription::MULTIPART.to_string(),
            "multipart/form-data; boundary generated during execution"
        );
        assert!(BodyDescription::MULTIPART.boundary_generated_during_execution());
        assert_eq!(BodyDescription::UNKNOWN.family(), None);
        assert!(!valid_media_hint(""));
        assert!(!valid_media_hint("bad\r\nhint"));
        assert!(!valid_media_hint(
            &"a".repeat(MAX_BODY_MEDIA_HINT_BYTES + 1)
        ));
        assert!(valid_media_hint(&"a".repeat(MAX_BODY_MEDIA_HINT_BYTES)));
    }
}
