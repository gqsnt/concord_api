use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, Method, StatusCode, Version};
use url::Url;

use crate::body::Stream;
use crate::codec::{DecodeBody, DecodeContext, MediaType};
use crate::endpoint::ResponseAcquisition;
use crate::hooks::{BoxFuture, CompletionOutcome};

use super::handler::ResponseInput;
use super::{ByteStream, ExecutionMeta, ResponseBodyError, ResponseBodyErrorKind};

pub(crate) mod private {
    use crate::error::ApiError;
    use crate::hooks::{BoxFuture, CompletionOutcome};

    use super::{ResponseInput, ResponseMode};

    pub trait Sealed: Send + Sync + 'static {
        const COMPLETION_OUTCOME: CompletionOutcome;

        fn preflight(input: &ResponseInput) -> Result<(), ApiError>;

        fn acquire(
            input: ResponseInput,
        ) -> BoxFuture<'static, Result<<Self as ResponseMode>::Body, ApiError>>
        where
            Self: ResponseMode;
    }
}

/// Complete bounded response acquisition before specification conversion.
///
/// Empty transport chunks are suppressed and bounded independently of the
/// configured nonempty-byte limit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Buffered;

/// Lazy bounded response acquisition as one media-independent [`ByteStream`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Streaming;

/// Successful response-body discard without polling the native body.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Discard;

/// A core-owned accepted-response acquisition mode.
///
/// Applications may select one of Concord's public marker types but cannot
/// implement another mode or obtain a native or unbounded response body.
pub trait ResponseMode: private::Sealed + Send + Sync + 'static {
    /// Semantic body value delivered after core-owned acquisition.
    ///
    /// This associated type is the input to [`ResponseBodySpec::from_body`];
    /// it is not a public `ResponseBody` trait or a native transport response.
    type Body: Send + 'static;

    /// Safe structural acquisition category used by endpoint descriptions and
    /// planning without invoking a response specification.
    const ACQUISITION: ResponseAcquisition;
}

impl ResponseMode for Buffered {
    type Body = Bytes;
    const ACQUISITION: ResponseAcquisition = ResponseAcquisition::Buffered;
}

impl ResponseMode for Streaming {
    type Body = ByteStream;
    const ACQUISITION: ResponseAcquisition = ResponseAcquisition::Streaming;
}

impl ResponseMode for Discard {
    type Body = ();
    const ACQUISITION: ResponseAcquisition = ResponseAcquisition::Discard;
}

impl private::Sealed for Buffered {
    const COMPLETION_OUTCOME: CompletionOutcome = CompletionOutcome::Success;

    fn preflight(input: &ResponseInput) -> Result<(), crate::error::ApiError> {
        super::handler::preflight_buffered(input)
    }

    fn acquire(
        input: ResponseInput,
    ) -> BoxFuture<'static, Result<<Self as ResponseMode>::Body, crate::error::ApiError>> {
        Box::pin(super::handler::acquire_buffered(input))
    }
}

impl private::Sealed for Streaming {
    const COMPLETION_OUTCOME: CompletionOutcome = CompletionOutcome::StreamOpened;

    fn preflight(input: &ResponseInput) -> Result<(), crate::error::ApiError> {
        super::handler::preflight_streaming(input)
    }

    fn acquire(
        input: ResponseInput,
    ) -> BoxFuture<'static, Result<<Self as ResponseMode>::Body, crate::error::ApiError>> {
        Box::pin(super::handler::acquire_streaming(input))
    }
}

impl private::Sealed for Discard {
    const COMPLETION_OUTCOME: CompletionOutcome = CompletionOutcome::Success;

    fn preflight(input: &ResponseInput) -> Result<(), crate::error::ApiError> {
        super::handler::preflight_discard(input)
    }

    fn acquire(
        input: ResponseInput,
    ) -> BoxFuture<'static, Result<<Self as ResponseMode>::Body, crate::error::ApiError>> {
        Box::pin(super::handler::acquire_discard(input))
    }
}

/// Sealed response-acquisition capability accepted by
/// [`Request::response_bytes`](crate::Request::response_bytes).
///
/// Concord implements this trait only for [`Buffered`] and [`Discard`].
/// Applications cannot implement new response modes because [`ResponseMode`]
/// is itself sealed.
pub trait ResponseBytesMode: ResponseMode {}

impl ResponseBytesMode for Buffered {}

impl ResponseBytesMode for Discard {}

/// Borrowed accepted response-head facts supplied to a response extension.
///
/// `url` is the credential-sanitized logical application URL and never the
/// native authenticated URL. Public path/query values may still contain
/// operational identifiers or personally identifiable information. Headers
/// are the actual accepted response headers and may contain sensitive values.
/// The selected extension intentionally receives these documented inputs and
/// owns their logging and storage behavior. No native response or transport
/// handle is exposed.
pub struct ResponseHead<'a> {
    /// Stable generated or runtime endpoint identity.
    pub endpoint: &'a str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Credential-sanitized logical application URL. Public values may still
    /// contain operational identifiers or personally identifiable information.
    pub url: &'a Url,
    /// Successful HTTP status.
    pub status: StatusCode,
    /// Observed HTTP version.
    pub version: Version,
    /// Actual accepted response headers.
    pub headers: &'a HeaderMap,
    /// Known post-decompression content length, when available.
    pub content_length: Option<u64>,
    /// Safe structural execution metadata.
    pub metadata: &'a ExecutionMeta,
}

/// Selects status acceptance and converts one core-owned acquisition mode into an
/// application output.
///
/// Concord first performs limiter and hook observation and authoritative
/// authentication challenge handling. It then calls `accepts_status`, whose
/// default accepts 2xx, and checks the operation deadline immediately after the
/// synchronous callback returns. A rejected status stays on the bounded status
/// error path.
///
/// For an accepted status, Concord performs the selected mode's known-length
/// preflight. `validate` runs next, before the first body poll. Concord alone
/// acquires the complete bounded bytes, lazy bounded byte stream, or discard
/// value, and only then calls `from_body`. The validation and conversion
/// callbacks are also synchronous and followed by deadline checks. These
/// callbacks cannot replace challenge ordering, limits, completion, native-body
/// ownership, or the three sealed modes. No native response or transport handle
/// is supplied to a callback.
///
/// Calls for independent responses may overlap. The accepted head and mode
/// body can contain sensitive application data; implementations own their
/// logging, storage, CPU, memory, metric cardinality, and external I/O behavior.
/// Concord's acquisition limit bounds retained wire bytes, not the heap size of
/// the output value produced by `from_body`. Every core-owned body-polling mode
/// suppresses empty transport chunks and applies the same finite response-wide
/// empty-chunk ceiling before extension conversion.
pub trait ResponseBodySpec: Send + Sync + 'static {
    /// Accepted semantic output.
    type Output: Send + 'static;
    /// Core-owned acquisition mode selected by this specification.
    type Mode: ResponseMode;

    /// Default validated `Accept` value inserted only when the request has none.
    fn accept() -> Option<&'static str> {
        None
    }

    /// Returns whether this specification accepts the received status.
    ///
    /// Authentication challenge handling remains authoritative and runs before
    /// this callback. The default preserves the ordinary 2xx-only contract.
    /// This synchronous callback is trusted application work; Concord checks
    /// the operation deadline immediately after it returns.
    fn accepts_status(status: StatusCode) -> bool {
        status.is_success()
    }

    /// Validates the accepted response head before the first body poll.
    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }

    /// Converts the fully acquired mode body into the semantic output.
    fn from_body(
        body: <Self::Mode as ResponseMode>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError>;
}

pub(crate) trait ResponseSelection<R>
where
    R: ResponseBodySpec,
{
    type Output: Send + 'static;
    type Mode: ResponseMode;

    fn accepts_status(status: StatusCode) -> bool;

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError>;

    fn from_body(
        body: <Self::Mode as ResponseMode>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError>;
}

pub(crate) struct DeclaredResponse<R>(std::marker::PhantomData<fn() -> R>);

impl<R> ResponseSelection<R> for DeclaredResponse<R>
where
    R: ResponseBodySpec,
{
    type Output = R::Output;
    type Mode = R::Mode;

    fn accepts_status(status: StatusCode) -> bool {
        R::accepts_status(status)
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        R::validate(head)
    }

    fn from_body(
        body: <Self::Mode as ResponseMode>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        R::from_body(body, head)
    }
}

pub(crate) struct ResponseBytesSelection<R>(std::marker::PhantomData<fn() -> R>);

impl<R> ResponseSelection<R> for ResponseBytesSelection<R>
where
    R: ResponseBodySpec,
{
    type Output = Bytes;
    type Mode = Buffered;

    fn accepts_status(status: StatusCode) -> bool {
        R::accepts_status(status)
    }

    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }

    fn from_body(
        body: <Self::Mode as ResponseMode>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        Ok(body)
    }
}

impl<C> ResponseBodySpec for C
where
    C: DecodeBody,
{
    type Output = C::Output;
    type Mode = Buffered;

    fn accept() -> Option<&'static str> {
        C::ACCEPT
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        let content_type = head
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        C::validate_content_type(content_type)
            .map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::InvalidMediaType))
    }

    fn from_body(
        body: <Self::Mode as ResponseMode>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        let content_type = head
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        C::decode(
            body,
            DecodeContext {
                endpoint: head.endpoint,
                method: head.method,
                status: head.status,
                content_type,
            },
        )
        .map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::Decode))
    }
}

impl ResponseBodySpec for Bytes {
    type Output = Bytes;
    type Mode = Buffered;

    fn from_body(
        body: <Self::Mode as ResponseMode>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        Ok(body)
    }
}

/// An accepted response whose body is discarded without polling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NoContent;

impl ResponseBodySpec for NoContent {
    type Output = ();
    type Mode = Discard;

    fn from_body(
        (): <Self::Mode as ResponseMode>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        Ok(())
    }
}

impl<M> ResponseBodySpec for Stream<M>
where
    M: MediaType,
{
    type Output = ByteStream;
    type Mode = Streaming;

    fn accept() -> Option<&'static str> {
        Some(M::MEDIA_TYPE)
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        let required = parse_exact_media_type(M::MEDIA_TYPE)?;
        let actual = head
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<mime::Mime>().ok());
        if actual.is_some_and(|actual| exact_media_matches(&required, &actual)) {
            Ok(())
        } else {
            Err(ResponseBodyError::new(
                ResponseBodyErrorKind::InvalidMediaType,
            ))
        }
    }

    fn from_body(
        body: <Self::Mode as ResponseMode>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        Ok(body)
    }
}

/// Explicit capability for buffered response specifications that can be
/// consumed by generated pagination.
///
/// Downstream buffered specifications opt in explicitly. Complete codecs whose
/// output implements [`IntoIterator`] receive the capability automatically.
/// Bytes, discard, byte streams, and record streams do not acquire it merely by
/// implementing [`ResponseBodySpec`].
pub trait PageResponseSpec: ResponseBodySpec<Mode = Buffered> {}

impl<C> PageResponseSpec for C
where
    C: DecodeBody,
    C::Output: IntoIterator,
{
}

pub(crate) fn completion_outcome<R, S>() -> CompletionOutcome
where
    R: ResponseBodySpec,
    S: ResponseSelection<R>,
{
    <S::Mode as private::Sealed>::COMPLETION_OUTCOME
}

pub(crate) fn validate_accept(value: &str) -> Result<(), ResponseBodyError> {
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;
    let bytes = value.as_bytes();
    for (index, byte) in bytes.iter().copied().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' if quoted => escaped = true,
            b'"' => quoted = !quoted,
            b',' if !quoted => {
                validate_media_range(&value[start..index])?;
                start = index + 1;
            }
            _ => {}
        }
    }
    if quoted || escaped {
        return Err(ResponseBodyError::new(
            ResponseBodyErrorKind::InvalidMediaType,
        ));
    }
    validate_media_range(&value[start..])
}

fn validate_media_range(value: &str) -> Result<(), ResponseBodyError> {
    let media = value
        .trim()
        .parse::<mime::Mime>()
        .map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::InvalidMediaType))?;
    if media.type_() == mime::STAR && media.subtype() != mime::STAR {
        return Err(ResponseBodyError::new(
            ResponseBodyErrorKind::InvalidMediaType,
        ));
    }
    Ok(())
}

fn parse_exact_media_type(value: &str) -> Result<mime::Mime, ResponseBodyError> {
    let parsed = value
        .parse::<mime::Mime>()
        .map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::InvalidMediaType))?;
    if parsed.type_() == mime::STAR || parsed.subtype() == mime::STAR {
        return Err(ResponseBodyError::new(
            ResponseBodyErrorKind::InvalidMediaType,
        ));
    }
    Ok(parsed)
}

fn exact_media_matches(required: &mime::Mime, actual: &mime::Mime) -> bool {
    required.type_() == actual.type_()
        && required.subtype() == actual.subtype()
        && required
            .params()
            .all(|(name, value)| actual.get_param(name) == Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_validation_supports_lists_parameters_and_quoted_commas() {
        for value in [
            "application/json",
            "text/plain; charset=utf-8, application/vnd.example+json; version=2",
            "text/csv; note=\"comma,inside\", application/*; q=0.5",
            "*/*",
        ] {
            validate_accept(value).unwrap_or_else(|error| {
                panic!("valid Accept value {value:?} failed as {:?}", error.kind())
            });
        }

        for value in [
            "",
            ",application/json",
            "application/json,",
            "*/json",
            "not a media range",
            "text/csv; note=\"unterminated",
        ] {
            assert_eq!(
                validate_accept(value)
                    .expect_err("invalid Accept value")
                    .kind(),
                ResponseBodyErrorKind::InvalidMediaType
            );
        }
    }

    #[test]
    fn exact_stream_media_requires_type_subtype_and_declared_parameters() {
        let required = parse_exact_media_type("application/vnd.example.stream; version=1")
            .expect("exact media type");
        let with_extra = "application/vnd.example.stream; charset=utf-8; version=1"
            .parse::<mime::Mime>()
            .expect("actual media type");
        let wrong_parameter = "application/vnd.example.stream; version=2"
            .parse::<mime::Mime>()
            .expect("actual media type");
        assert!(exact_media_matches(&required, &with_extra));
        assert!(!exact_media_matches(&required, &wrong_parameter));
        assert!(parse_exact_media_type("application/*").is_err());
    }
}
