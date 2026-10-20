use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, Method, StatusCode, Version};
use url::Url;

use crate::codec::{Binary, DecodeBody, DecodeContext};
use crate::endpoint::ResponseAcquisition;
use crate::hooks::{BoxFuture, CompletionOutcome};

use super::absence::ExpectedAbsence;
use super::handler::ResponseInput;
use super::{BodyDisposition, ByteStream, ExecutionMeta, ResponseBodyError, ResponseBodyErrorKind};

pub(crate) mod private {
    use crate::error::ApiError;
    use crate::hooks::{BoxFuture, CompletionOutcome};

    use super::{ResponseDriver, ResponseInput};

    pub trait Sealed: Send + Sync + 'static {
        const COMPLETION_OUTCOME: CompletionOutcome;
        const BODY_DISPOSITION: super::BodyDisposition;

        fn preflight(input: &ResponseInput) -> Result<(), ApiError>;

        fn acquired_body_bytes(body: &<Self as ResponseDriver>::Body) -> usize
        where
            Self: ResponseDriver;

        fn acquire(
            input: ResponseInput,
        ) -> BoxFuture<'static, Result<<Self as ResponseDriver>::Body, ApiError>>
        where
            Self: ResponseDriver;
    }
}

/// Complete bounded response acquisition before specification conversion.
///
/// Empty transport chunks are suppressed and bounded independently of the
/// configured nonempty-byte limit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Buffered;

/// Lazy bounded response acquisition as one media-independent managed
/// [`ByteStream`].
///
/// The endpoint await returns after accepted-head validation and opening the
/// stream (completion `StreamOpened`, body disposition
/// [`BodyDisposition::StreamOpened`]); consumption then runs under the
/// stream's finite lease, idle, byte, record and frame limits. See
/// [`ByteStream`] for the acquisition-versus-consumption contract.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Streaming;

/// Accepted response-body discard without polling the native body.
///
/// The native body is dropped unread: it is never polled and never drained,
/// so no body byte limit applies and the outcome reports
/// [`BodyDisposition::DiscardedUnread`]. This is not complete consumption of
/// the body.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiscardDriver;

/// A core-owned accepted-response acquisition mode.
///
/// The trait is sealed: its only modes are [`Buffered`] (complete bounded
/// `Bytes` before conversion), [`Streaming`] (a managed [`ByteStream`] with
/// its own finite post-acquisition contract) and [`DiscardDriver`] (the
/// accepted native body is dropped without polling).
///
/// # Extension contract
///
/// * **Ownership.** Core owns the native response body in every mode and
///   hands a representation only the mode's [`Body`](Self::Body) value.
/// * **Concurrency.** The markers are stateless; independent responses are
///   acquired concurrently without shared driver state.
/// * **Cancellation.** Acquisition runs under the operation deadline (and,
///   for a stream, its lease and idle timeouts); dropping the operation or
///   stream stops acquisition without polling further.
/// * **Input sensitivity.** The acquired body may be sensitive; no mode
///   formats body bytes into diagnostics.
/// * **Errors.** Acquisition failures (limits, transport errors, timeouts)
///   are value-free core errors reported before any representation
///   conversion runs.
/// * **Resource obligations.** Every mode is bounded by core: the configured
///   response or stream byte limits, the empty-chunk ceiling and finite time
///   leases.
/// * **Negative capabilities.** Applications may select one of these markers
///   but cannot implement another mode or obtain a native or unbounded
///   response body.
pub trait ResponseDriver: private::Sealed + Send + Sync + 'static {
    /// Semantic body value delivered after core-owned acquisition.
    ///
    /// This associated type is the input to [`ResponseRepresentation::from_body`];
    /// it is not a public `ResponseBody` trait or a native transport response.
    type Body: Send + 'static;

    /// Safe structural acquisition category used by endpoint descriptions and
    /// planning without invoking a response specification.
    const ACQUISITION: ResponseAcquisition;
}

impl ResponseDriver for Buffered {
    type Body = Bytes;
    const ACQUISITION: ResponseAcquisition = ResponseAcquisition::Buffered;
}

impl ResponseDriver for Streaming {
    type Body = ByteStream;
    const ACQUISITION: ResponseAcquisition = ResponseAcquisition::Streaming;
}

impl ResponseDriver for DiscardDriver {
    type Body = ();
    const ACQUISITION: ResponseAcquisition = ResponseAcquisition::Discard;
}

impl private::Sealed for Buffered {
    const COMPLETION_OUTCOME: CompletionOutcome = CompletionOutcome::Success;
    const BODY_DISPOSITION: BodyDisposition = BodyDisposition::Complete;

    fn preflight(input: &ResponseInput) -> Result<(), crate::error::ApiError> {
        super::handler::preflight_buffered(input)
    }

    fn acquired_body_bytes(body: &<Self as ResponseDriver>::Body) -> usize {
        body.len()
    }

    fn acquire(
        input: ResponseInput,
    ) -> BoxFuture<'static, Result<<Self as ResponseDriver>::Body, crate::error::ApiError>> {
        Box::pin(super::handler::acquire_buffered(input))
    }
}

impl private::Sealed for Streaming {
    const COMPLETION_OUTCOME: CompletionOutcome = CompletionOutcome::StreamOpened;
    const BODY_DISPOSITION: BodyDisposition = BodyDisposition::StreamOpened;

    fn preflight(input: &ResponseInput) -> Result<(), crate::error::ApiError> {
        super::handler::preflight_streaming(input)
    }

    fn acquired_body_bytes(_body: &<Self as ResponseDriver>::Body) -> usize {
        0
    }

    fn acquire(
        input: ResponseInput,
    ) -> BoxFuture<'static, Result<<Self as ResponseDriver>::Body, crate::error::ApiError>> {
        Box::pin(super::handler::acquire_streaming(input))
    }
}

impl private::Sealed for DiscardDriver {
    const COMPLETION_OUTCOME: CompletionOutcome = CompletionOutcome::Success;
    const BODY_DISPOSITION: BodyDisposition = BodyDisposition::DiscardedUnread;

    fn preflight(input: &ResponseInput) -> Result<(), crate::error::ApiError> {
        super::handler::preflight_discard(input)
    }

    fn acquired_body_bytes(_body: &<Self as ResponseDriver>::Body) -> usize {
        0
    }

    fn acquire(
        input: ResponseInput,
    ) -> BoxFuture<'static, Result<<Self as ResponseDriver>::Body, crate::error::ApiError>> {
        Box::pin(super::handler::acquire_discard(input))
    }
}

/// Sealed response-acquisition capability accepted by
/// endpoint-declared accepted-byte terminals.
///
/// Concord implements this trait only for [`Buffered`] and [`DiscardDriver`].
///
/// # Extension contract
///
/// * **Ownership.** Core keeps ownership of the acquired bytes and returns
///   them, bounded, through the accepted-bytes terminal.
/// * **Concurrency.** The markers are stateless and carry no shared state.
/// * **Cancellation.** The acquisition obligations of [`ResponseDriver`]
///   apply; dropping the terminal stops acquisition.
/// * **Input sensitivity.** Accepted bytes may be sensitive and are never
///   formatted into diagnostics.
/// * **Errors.** Acquisition failures are value-free core errors; the
///   capability adds none of its own.
/// * **Resource obligations.** The configured response byte limit bounds the
///   retained bytes.
/// * **Negative capabilities.** Applications cannot implement new response
///   modes because [`ResponseDriver`] is itself sealed, so no terminal can
///   expose a streaming or native body as accepted bytes.
pub trait AcceptedBytesDriver: ResponseDriver {}

impl AcceptedBytesDriver for Buffered {}

impl AcceptedBytesDriver for DiscardDriver {}

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
    /// Operational endpoint ID: an explicit `id` pin or a navigation-derived ID
    /// for a generated endpoint, the validated ID of a runtime endpoint.
    pub endpoint: &'a str,
    /// Application HTTP method.
    pub method: &'a Method,
    /// Credential-sanitized logical application URL. Public values may still
    /// contain operational identifiers or personally identifiable information.
    pub url: &'a Url,
    /// Accepted HTTP status.
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

/// Converts one core-owned acquisition mode into an application output.
///
/// Concord first performs limiter observation, authoritative authentication
/// challenge handling, head-only expected absence (an endpoint's `absent on`
/// statuses never reach the representation) and endpoint-owned status
/// acceptance. A rejected status stays on the bounded status error path and
/// never enters this extension, except through the explicit application call
/// `HttpResponseFailure::decode_status_body`, which runs the same callbacks on
/// retained complete bytes without network work.
///
/// Negotiation is the static [`ResponseRepresentation::ACCEPT`] constant; no
/// user function runs merely to discover the `Accept` header.
///
/// For an accepted status, Concord performs the selected mode's known-length
/// preflight. `validate` runs next, before the first body poll. Concord alone
/// acquires the complete bounded bytes, lazy bounded byte stream, or discard
/// value, and only then calls `from_body`. The validation and conversion
/// callbacks are also synchronous and followed by deadline checks. These
/// callbacks cannot replace challenge ordering, limits, completion, native-body
/// ownership, or the three sealed modes.
///
/// For pagination, core additionally charges the exact acquired buffered body
/// length to the operation's aggregate response-byte budget before retaining
/// decoded page items. The item-count limit independently bounds collection
/// structure.
///
/// # Extension contract
///
/// * **Ownership.** `from_body` takes ownership of the mode's acquired body
///   and moves the output to the caller; the response head is borrowed.
///   Representations are marker types that Concord never constructs.
/// * **Concurrency.** Calls for independent responses may overlap.
/// * **Cancellation.** The callbacks are synchronous and cannot be
///   preempted; core checks the operation deadline after each one, and
///   cancellation prevents only later Concord work.
/// * **Input sensitivity.** The accepted head and mode body can contain
///   sensitive application data; implementations own their logging, storage
///   and metrics.
/// * **Errors.** Return a [`ResponseBodyError`]; core keeps its structural
///   kind, discards application text before automatic diagnostics, and
///   treats the failure as terminal (never retried).
/// * **Resource obligations.** Implementations own their CPU, memory, metric
///   cardinality and external I/O behavior. Concord's acquisition limit
///   bounds retained wire bytes, not the heap size of the output value
///   produced by `from_body`; a custom decoder that expands a small body into
///   disproportionately large application-owned state remains trusted
///   extension code. Every core-owned body-polling mode suppresses empty
///   transport chunks and applies the same finite response-wide empty-chunk
///   ceiling before extension conversion.
/// * **Negative capabilities.** A representation cannot change the accepted
///   status, contact evidence or absence rule, cannot obtain a native
///   response or transport handle, and cannot grant itself a new acquisition
///   mode.
#[diagnostic::on_unimplemented(
    message = "[C3-REP] `{Self}` is not a response representation for result type `{Output}`",
    label = "selected as the response representation of a `{Output}` result",
    note = "a result needs `ResponseRepresentation<{Output}>` or a complete buffered `DecodeBody` decoder whose `Output` is `{Output}`; a request-only representation cannot decode a response, and with `absent on` the result is `Option<T>` decoded by a buffered representation of `T`",
    note = "write `-> {Output} via R` with a representation producing this result, or select a direction default with `response R;` on an owner"
)]
pub trait ResponseRepresentation<Output>: Send + Sync + 'static
where
    Output: Send + 'static,
{
    /// Core-owned acquisition mode selected by this specification.
    type Driver: ResponseDriver;

    /// Static `Accept` value negotiated for this representation.
    ///
    /// Inspection, descriptions and plans read this constant without running
    /// user code. It is validated as an HTTP `Accept` value (no injected line
    /// breaks) once, at compile time through the generated-code bridge or at
    /// runtime endpoint build, and it is independent of response
    /// `Content-Type` validation performed by [`Self::validate`].
    const ACCEPT: Option<&'static str> = None;

    /// Validates the accepted response head before the first body poll.
    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }

    /// Converts the fully acquired mode body into the semantic output.
    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Output, ResponseBodyError>;
}

pub(crate) trait ResponseSelection<R, DeclaredOutput>
where
    R: ResponseRepresentation<DeclaredOutput>,
    DeclaredOutput: Send + 'static,
{
    type Output: Send + 'static;
    type Driver: ResponseDriver;

    /// Whether this selection can complete a declared head-only absence.
    const SUPPORTS_ABSENCE: bool;

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError>;

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError>;

    /// Constructs the absent outcome without invoking the representation.
    fn absent_output(absence: &ExpectedAbsence<DeclaredOutput, R>) -> Option<Self::Output>;
}

pub(crate) struct DeclaredResponse<R>(std::marker::PhantomData<fn() -> R>);

impl<R, Output> ResponseSelection<R, Output> for DeclaredResponse<R>
where
    R: ResponseRepresentation<Output>,
    Output: Send + 'static,
{
    type Output = Output;
    type Driver = R::Driver;

    const SUPPORTS_ABSENCE: bool = true;

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        R::validate(head)
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        R::from_body(body, head)
    }

    fn absent_output(absence: &ExpectedAbsence<Output, R>) -> Option<Self::Output> {
        Some(absence.absent_value())
    }
}

pub(crate) struct AcceptedBytesSelection<R>(std::marker::PhantomData<fn() -> R>);

impl<R, Output> ResponseSelection<R, Output> for AcceptedBytesSelection<R>
where
    R: ResponseRepresentation<Output>,
    Output: Send + 'static,
{
    type Output = Bytes;
    type Driver = Buffered;

    const SUPPORTS_ABSENCE: bool = false;

    fn validate(_head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<Self::Output, ResponseBodyError> {
        Ok(body)
    }

    fn absent_output(_absence: &ExpectedAbsence<Output, R>) -> Option<Self::Output> {
        None
    }
}

impl<C> ResponseRepresentation<C::Output> for C
where
    C: DecodeBody,
{
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = C::ACCEPT;

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        let content_type = head
            .headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        C::validate_content_type(content_type)
            .map_err(|_| ResponseBodyError::new(ResponseBodyErrorKind::InvalidMediaType))
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        head: &ResponseHead<'_>,
    ) -> Result<C::Output, ResponseBodyError> {
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
        .map_err(|error| ResponseBodyError::new(codec_response_error_kind(error.kind())))
    }
}

/// Maps one category-only codec failure to an accepted-response failure.
pub(crate) const fn codec_response_error_kind(
    kind: crate::codec::CodecErrorKind,
) -> ResponseBodyErrorKind {
    match kind {
        crate::codec::CodecErrorKind::Decode => ResponseBodyErrorKind::Decode,
        crate::codec::CodecErrorKind::Limit => ResponseBodyErrorKind::InvalidOutput,
        crate::codec::CodecErrorKind::Encode => ResponseBodyErrorKind::Internal,
    }
}

impl ResponseRepresentation<Bytes> for Binary {
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = Some("application/octet-stream");

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_exact_media(head, "application/octet-stream")
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<Bytes, ResponseBodyError> {
        Ok(body)
    }
}

impl ResponseRepresentation<Vec<u8>> for Binary {
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = Some("application/octet-stream");

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_exact_media(head, "application/octet-stream")
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<Vec<u8>, ResponseBodyError> {
        Ok(body.to_vec())
    }
}

/// An accepted response that must contain zero decoded body bytes.
///
/// The body is acquired completely under the buffered limit; a nonempty body
/// is a conversion error, never silently discarded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Empty;

impl ResponseRepresentation<()> for Empty {
    type Driver = Buffered;

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<(), ResponseBodyError> {
        if body.is_empty() {
            Ok(())
        } else {
            Err(ResponseBodyError::new(ResponseBodyErrorKind::Decode))
        }
    }
}

/// An accepted response whose native body is dropped without being read.
///
/// `Discard` selects [`DiscardDriver`]: the body is never polled or drained,
/// and the response reports [`BodyDisposition::DiscardedUnread`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Discard;

impl ResponseRepresentation<()> for Discard {
    type Driver = DiscardDriver;

    fn from_body(
        (): <Self::Driver as ResponseDriver>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<(), ResponseBodyError> {
        Ok(())
    }
}

impl ResponseRepresentation<ByteStream> for Binary {
    type Driver = Streaming;

    const ACCEPT: Option<&'static str> = Some("application/octet-stream");

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_exact_media(head, "application/octet-stream")
    }

    fn from_body(
        body: <Self::Driver as ResponseDriver>::Body,
        _head: &ResponseHead<'_>,
    ) -> Result<ByteStream, ResponseBodyError> {
        Ok(body)
    }
}

fn validate_exact_media(
    head: &ResponseHead<'_>,
    expected: &'static str,
) -> Result<(), ResponseBodyError> {
    let required = parse_exact_media_type(expected)?;
    let Some(actual) = head.headers.get(CONTENT_TYPE) else {
        return Ok(());
    };
    let actual = actual
        .to_str()
        .ok()
        .and_then(|value| value.parse::<mime::Mime>().ok());
    if actual.is_some_and(|actual| exact_media_matches(&required, &actual)) {
        Ok(())
    } else {
        Err(ResponseBodyError::new(
            ResponseBodyErrorKind::InvalidMediaType,
        ))
    }
}

pub(crate) fn completion_outcome<R, Output, S>() -> CompletionOutcome
where
    R: ResponseRepresentation<Output>,
    Output: Send + 'static,
    S: ResponseSelection<R, Output>,
{
    <S::Driver as private::Sealed>::COMPLETION_OUTCOME
}

pub(crate) fn body_disposition<R, Output, S>() -> BodyDisposition
where
    R: ResponseRepresentation<Output>,
    Output: Send + 'static,
    S: ResponseSelection<R, Output>,
{
    <S::Driver as private::Sealed>::BODY_DISPOSITION
}

/// Maximum byte length of one static representation `Accept` value.
pub const MAX_ACCEPT_BYTES: usize = 4_096;

pub(crate) fn validate_accept(value: &str) -> Result<(), ResponseBodyError> {
    if accept_is_valid(value) {
        Ok(())
    } else {
        Err(ResponseBodyError::new(
            ResponseBodyErrorKind::InvalidMediaType,
        ))
    }
}

/// Validates one HTTP `Accept` field value in constant context.
///
/// The grammar is RFC 9110 `#( media-range [ weight ] )` without empty list
/// elements: each element is `type "/" subtype` (`*/*`, `type/*` or concrete),
/// followed by `;`-separated `token "=" ( token / quoted-string )`
/// parameters. Controls, line breaks and a `*/subtype` range are rejected.
pub(crate) const fn accept_is_valid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_ACCEPT_BYTES {
        return false;
    }
    let mut index = 0;
    loop {
        index = skip_ows(bytes, index);
        let type_start = index;
        index = skip_token(bytes, index);
        if index == type_start || index >= bytes.len() || bytes[index] != b'/' {
            return false;
        }
        let type_is_star = index - type_start == 1 && bytes[type_start] == b'*';
        index += 1;
        let subtype_start = index;
        index = skip_token(bytes, index);
        if index == subtype_start {
            return false;
        }
        let subtype_is_star = index - subtype_start == 1 && bytes[subtype_start] == b'*';
        if type_is_star && !subtype_is_star {
            return false;
        }
        loop {
            let after_ows = skip_ows(bytes, index);
            if after_ows >= bytes.len() || bytes[after_ows] != b';' {
                index = after_ows;
                break;
            }
            index = skip_ows(bytes, after_ows + 1);
            if index >= bytes.len() || bytes[index] == b',' || bytes[index] == b';' {
                // The grammar permits an empty parameter after a semicolon.
                continue;
            }
            let name_start = index;
            index = skip_token(bytes, index);
            if index == name_start || index >= bytes.len() || bytes[index] != b'=' {
                return false;
            }
            index += 1;
            if index < bytes.len() && bytes[index] == b'"' {
                index += 1;
                let mut closed = false;
                while index < bytes.len() {
                    let byte = bytes[index];
                    if byte == b'"' {
                        closed = true;
                        index += 1;
                        break;
                    } else if byte == b'\\' {
                        if index + 1 >= bytes.len() || !quoted_pair_byte(bytes[index + 1]) {
                            return false;
                        }
                        index += 2;
                    } else if qdtext_byte(byte) {
                        index += 1;
                    } else {
                        return false;
                    }
                }
                if !closed {
                    return false;
                }
            } else {
                let value_start = index;
                index = skip_token(bytes, index);
                if index == value_start {
                    return false;
                }
            }
        }
        if index == bytes.len() {
            return true;
        }
        if bytes[index] != b',' {
            return false;
        }
        index += 1;
        if skip_ows(bytes, index) == bytes.len() {
            return false;
        }
    }
}

const fn skip_ows(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && (bytes[index] == b' ' || bytes[index] == b'\t') {
        index += 1;
    }
    index
}

const fn skip_token(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && tchar(bytes[index]) {
        index += 1;
    }
    index
}

const fn tchar(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

const fn qdtext_byte(byte: u8) -> bool {
    byte == b'\t'
        || byte == b' '
        || byte == 0x21
        || (byte >= 0x23 && byte <= 0x5b)
        || (byte >= 0x5d && byte <= 0x7e)
        || byte >= 0x80
}

const fn quoted_pair_byte(byte: u8) -> bool {
    byte == b'\t' || byte == b' ' || (byte >= 0x21 && byte <= 0x7e) || byte >= 0x80
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
