use std::error::Error;
use std::fmt;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use http::header::{CONTENT_TYPE, RETRY_AFTER};
use http::{HeaderMap, Method, StatusCode};

use crate::codec::{DecodeBody, DecodeContext};
use crate::endpoint::EndpointId;
use crate::redaction::SanitizedHeaderMap;

/// The structural result of finite terminal status-body capture.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusBodyState {
    /// Capture was explicitly disabled.
    Disabled,
    /// The complete body contained no bytes.
    Empty,
    /// The complete body was retained within the finite limit.
    Complete,
    /// A trusted known length exceeded the finite limit before polling.
    OmittedKnownTooLarge,
    /// Unknown-length capture crossed the finite limit and discarded its prefix.
    OmittedLimitExceeded,
    /// Reading the body failed after the response head was known.
    UnavailableReadFailure,
    /// The operation deadline expired during capture.
    UnavailableDeadline,
}

enum StatusBodyInner {
    Disabled,
    Empty,
    Complete(Bytes),
    OmittedKnownTooLarge,
    OmittedLimitExceeded,
    UnavailableReadFailure,
    UnavailableDeadline,
}

/// An opaque finite complete-only terminal HTTP status body.
///
/// Capture suppresses empty transport chunks and applies the response-wide
/// empty-chunk ceiling before later status-body processing.
pub struct StatusBody {
    inner: StatusBodyInner,
}

impl StatusBody {
    pub(crate) const fn disabled() -> Self {
        Self {
            inner: StatusBodyInner::Disabled,
        }
    }

    pub(crate) const fn empty() -> Self {
        Self {
            inner: StatusBodyInner::Empty,
        }
    }

    pub(crate) fn complete(bytes: Bytes) -> Self {
        if bytes.is_empty() {
            Self::empty()
        } else {
            Self {
                inner: StatusBodyInner::Complete(bytes),
            }
        }
    }

    pub(crate) const fn omitted_known_too_large() -> Self {
        Self {
            inner: StatusBodyInner::OmittedKnownTooLarge,
        }
    }

    pub(crate) const fn omitted_limit_exceeded() -> Self {
        Self {
            inner: StatusBodyInner::OmittedLimitExceeded,
        }
    }

    pub(crate) const fn unavailable_read_failure() -> Self {
        Self {
            inner: StatusBodyInner::UnavailableReadFailure,
        }
    }

    pub(crate) const fn unavailable_deadline() -> Self {
        Self {
            inner: StatusBodyInner::UnavailableDeadline,
        }
    }

    /// Returns the structural capture state.
    #[must_use]
    pub const fn state(&self) -> StatusBodyState {
        match self.inner {
            StatusBodyInner::Disabled => StatusBodyState::Disabled,
            StatusBodyInner::Empty => StatusBodyState::Empty,
            StatusBodyInner::Complete(_) => StatusBodyState::Complete,
            StatusBodyInner::OmittedKnownTooLarge => StatusBodyState::OmittedKnownTooLarge,
            StatusBodyInner::OmittedLimitExceeded => StatusBodyState::OmittedLimitExceeded,
            StatusBodyInner::UnavailableReadFailure => StatusBodyState::UnavailableReadFailure,
            StatusBodyInner::UnavailableDeadline => StatusBodyState::UnavailableDeadline,
        }
    }

    /// Returns the complete body length when it is known and available.
    #[must_use]
    #[allow(
        clippy::len_without_is_empty,
        reason = "the normative API represents unknown length as Option<usize>"
    )]
    pub fn len(&self) -> Option<usize> {
        match &self.inner {
            StatusBodyInner::Empty => Some(0),
            StatusBodyInner::Complete(bytes) => Some(bytes.len()),
            _ => None,
        }
    }

    /// Returns retained bytes only for [`StatusBodyState::Complete`].
    #[must_use]
    pub fn bytes(&self) -> Option<&Bytes> {
        match &self.inner {
            StatusBodyInner::Complete(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// Consumes this value and returns retained bytes only for
    /// [`StatusBodyState::Complete`].
    #[must_use]
    pub fn into_bytes(self) -> Option<Bytes> {
        match self.inner {
            StatusBodyInner::Complete(bytes) => Some(bytes),
            _ => None,
        }
    }
}

impl fmt::Display for StatusBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "terminal status body ({:?})", self.state())
    }
}

impl fmt::Debug for StatusBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StatusBody")
            .field("state", &self.state())
            .field("len", &self.len())
            .finish()
    }
}

/// A body-free failure from explicit terminal status-body decoding.
pub struct StatusBodyDecodeError {
    kind: StatusBodyDecodeErrorKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StatusBodyDecodeErrorKind {
    Unavailable(StatusBodyState),
    DecodeFailed,
}

impl StatusBodyDecodeError {
    fn unavailable(state: StatusBodyState) -> Self {
        Self {
            kind: StatusBodyDecodeErrorKind::Unavailable(state),
        }
    }

    fn decode_failed() -> Self {
        Self {
            kind: StatusBodyDecodeErrorKind::DecodeFailed,
        }
    }
}

impl fmt::Display for StatusBodyDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            StatusBodyDecodeErrorKind::Unavailable(state) => {
                write!(
                    formatter,
                    "terminal status body is not complete ({state:?})"
                )
            }
            StatusBodyDecodeErrorKind::DecodeFailed => {
                formatter.write_str("terminal status body decoding failed")
            }
        }
    }
}

impl fmt::Debug for StatusBodyDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StatusBodyDecodeError")
            .field("kind", &self.kind)
            .finish()
    }
}

impl Error for StatusBodyDecodeError {}

/// Safe metadata and an opaque complete-only body for a terminal HTTP status.
pub struct HttpResponseFailure {
    endpoint: EndpointId,
    method: Method,
    status: StatusCode,
    headers: SanitizedHeaderMap,
    body: StatusBody,
    content_type: Option<String>,
    retry_after: Option<Duration>,
}

impl HttpResponseFailure {
    #[cfg(test)]
    pub(crate) fn new(
        endpoint: EndpointId,
        method: Method,
        status: StatusCode,
        headers: HeaderMap,
        body: StatusBody,
        retry_after_cap: Duration,
    ) -> Self {
        let retry_after = parse_retry_after(&headers, retry_after_cap, SystemTime::now());
        Self::new_with_retry_after(
            endpoint,
            method,
            status,
            headers,
            body,
            retry_after_cap,
            retry_after,
        )
    }

    pub(crate) fn new_with_retry_after(
        endpoint: EndpointId,
        method: Method,
        status: StatusCode,
        headers: HeaderMap,
        body: StatusBody,
        retry_after_cap: Duration,
        retry_after: Option<Duration>,
    ) -> Self {
        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let headers = SanitizedHeaderMap::from_response_headers(&headers);
        Self {
            endpoint,
            method,
            status,
            headers,
            body,
            content_type,
            retry_after: retry_after.map(|delay| delay.min(retry_after_cap)),
        }
    }

    #[cfg(test)]
    fn new_at(
        endpoint: EndpointId,
        method: Method,
        status: StatusCode,
        headers: HeaderMap,
        body: StatusBody,
        retry_after_cap: Duration,
        now: SystemTime,
    ) -> Self {
        let content_type = headers
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let retry_after = parse_retry_after(&headers, retry_after_cap, now);
        let headers = SanitizedHeaderMap::from_response_headers(&headers);
        Self {
            endpoint,
            method,
            status,
            headers,
            body,
            content_type,
            retry_after,
        }
    }

    /// Returns the stable generated endpoint identity.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        self.endpoint.as_str()
    }

    /// Returns the typed stable endpoint identity.
    #[must_use]
    pub const fn endpoint_id(&self) -> &EndpointId {
        &self.endpoint
    }

    /// Returns the application HTTP method.
    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    /// Returns the terminal application HTTP status.
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns a bounded structural response-header view.
    ///
    /// Names may be present, but every remote value is redacted. Use a
    /// successful explicit [`crate::response::ApiResponse`] when caller-owned
    /// access to actual response values is required.
    #[must_use]
    pub const fn headers(&self) -> &SanitizedHeaderMap {
        &self.headers
    }

    /// Returns the explicit opaque terminal body value.
    #[must_use]
    pub const fn body(&self) -> &StatusBody {
        &self.body
    }

    /// Consumes this response failure and returns its opaque terminal body.
    #[must_use]
    pub fn into_body(self) -> StatusBody {
        self.body
    }

    /// Returns parsed non-negative `Retry-After` metadata clamped to the
    /// configured cooldown cap.
    #[must_use]
    pub const fn retry_after(&self) -> Option<Duration> {
        self.retry_after
    }

    /// Explicitly decodes a retained complete body with the final public
    /// response-decoder contract.
    pub fn decode_body<C>(&self) -> Result<C::Output, StatusBodyDecodeError>
    where
        C: DecodeBody,
    {
        let Some(body) = self.body.bytes() else {
            return Err(StatusBodyDecodeError::unavailable(self.body.state()));
        };
        C::validate_content_type(self.content_type.as_deref())
            .map_err(|_| StatusBodyDecodeError::decode_failed())?;
        C::decode(
            body.clone(),
            DecodeContext {
                endpoint: self.endpoint.as_str(),
                method: &self.method,
                status: self.status,
                content_type: self.content_type.as_deref(),
            },
        )
        .map_err(|_| StatusBodyDecodeError::decode_failed())
    }
}

impl fmt::Display for HttpResponseFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "terminal HTTP status {} for {} {}",
            self.status, self.method, self.endpoint
        )
    }
}

impl fmt::Debug for HttpResponseFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpResponseFailure")
            .field("endpoint", &self.endpoint)
            .field("method", &self.method)
            .field("status", &self.status)
            .field("header_values", &self.headers.len())
            .field("body", &self.body)
            .field("retry_after", &self.retry_after)
            .finish()
    }
}

pub(crate) fn parse_retry_after(
    headers: &HeaderMap,
    cap: Duration,
    now: SystemTime,
) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        let seconds = value.bytes().try_fold(0_u64, |seconds, byte| {
            seconds.checked_mul(10)?.checked_add(u64::from(byte - b'0'))
        });
        return Some(seconds.map_or(cap, |seconds| Duration::from_secs(seconds).min(cap)));
    }

    let date = httpdate::parse_http_date(value).ok()?;
    Some(date.duration_since(now).ok()?.min(cap))
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;
    use std::time::UNIX_EPOCH;

    use http::HeaderValue;

    use super::*;
    use crate::codec::{CodecError, CodecErrorKind};

    const BODY_SENTINEL: &str = "terminal-body-sentinel-7d5921";

    fn response(body: StatusBody) -> HttpResponseFailure {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/test"));
        HttpResponseFailure::new_at(
            EndpointId::from_static("widgets.fetch"),
            Method::GET,
            StatusCode::BAD_GATEWAY,
            headers,
            body,
            Duration::from_secs(60),
            UNIX_EPOCH,
        )
    }

    #[test]
    fn complete_body_requires_explicit_access_and_owning_extraction() {
        let failure = response(StatusBody::complete(Bytes::from_static(
            BODY_SENTINEL.as_bytes(),
        )));
        assert_eq!(failure.body().state(), StatusBodyState::Complete);
        assert_eq!(failure.body().len(), Some(BODY_SENTINEL.len()));
        assert_eq!(
            failure.body().bytes().map(Bytes::as_ref),
            Some(BODY_SENTINEL.as_bytes())
        );
        for formatted in [
            failure.to_string(),
            format!("{failure:?}"),
            failure.body().to_string(),
            format!("{:?}", failure.body()),
        ] {
            assert!(!formatted.contains(BODY_SENTINEL));
        }
        assert_eq!(
            failure.into_body().into_bytes().as_deref(),
            Some(BODY_SENTINEL.as_bytes())
        );
    }

    #[test]
    fn unknown_response_header_values_never_enter_error_metadata_or_formatting() {
        const UNKNOWN_SENTINEL: &str = "unknown-error-header-value-b8d41c";
        let mut headers = HeaderMap::new();
        let name = http::HeaderName::from_static("x-unknown-error-metadata");
        headers.insert(&name, HeaderValue::from_static(UNKNOWN_SENTINEL));
        let failure = HttpResponseFailure::new_at(
            EndpointId::from_static("widgets.fetch"),
            Method::GET,
            StatusCode::BAD_GATEWAY,
            headers,
            StatusBody::empty(),
            Duration::from_secs(60),
            UNIX_EPOCH,
        );

        assert_eq!(
            failure.headers().get(&name),
            Some(crate::redaction::SanitizedHeaderValue::Redacted)
        );
        assert!(!failure.to_string().contains(UNKNOWN_SENTINEL));
        assert!(!format!("{failure:?}").contains(UNKNOWN_SENTINEL));
    }

    #[test]
    fn every_non_complete_state_exposes_no_bytes() {
        let bodies = [
            StatusBody::disabled(),
            StatusBody::empty(),
            StatusBody::omitted_known_too_large(),
            StatusBody::omitted_limit_exceeded(),
            StatusBody::unavailable_read_failure(),
            StatusBody::unavailable_deadline(),
        ];
        let expected = [
            StatusBodyState::Disabled,
            StatusBodyState::Empty,
            StatusBodyState::OmittedKnownTooLarge,
            StatusBodyState::OmittedLimitExceeded,
            StatusBodyState::UnavailableReadFailure,
            StatusBodyState::UnavailableDeadline,
        ];
        for (body, expected) in bodies.into_iter().zip(expected) {
            assert_eq!(body.state(), expected);
            assert_eq!(body.bytes(), None);
            assert_eq!(body.into_bytes(), None);
        }
    }

    struct Echo;

    impl DecodeBody for Echo {
        type Output = Bytes;

        const ACCEPT: Option<&'static str> = Some("application/test");

        fn validate_content_type(content_type: Option<&str>) -> Result<(), CodecError> {
            assert_eq!(content_type, Some("application/test"));
            Ok(())
        }

        fn decode(body: Bytes, context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
            assert_eq!(context.endpoint, "widgets.fetch");
            assert_eq!(context.method, Method::GET);
            assert_eq!(context.status, StatusCode::BAD_GATEWAY);
            assert_eq!(context.content_type, Some("application/test"));
            Ok(body)
        }
    }

    struct Reject;

    impl DecodeBody for Reject {
        type Output = ();

        const ACCEPT: Option<&'static str> = None;

        fn decode(_body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
            Err(CodecError::new(CodecErrorKind::Decode))
        }
    }

    #[test]
    fn custom_decoder_receives_only_explicit_complete_bytes() {
        let failure = response(StatusBody::complete(Bytes::from_static(
            BODY_SENTINEL.as_bytes(),
        )));
        assert_eq!(
            failure
                .decode_body::<Echo>()
                .expect("decode should succeed"),
            Bytes::from_static(BODY_SENTINEL.as_bytes())
        );
    }

    #[test]
    fn decoder_refuses_every_unavailable_state() {
        let bodies = [
            StatusBody::disabled(),
            StatusBody::empty(),
            StatusBody::omitted_known_too_large(),
            StatusBody::omitted_limit_exceeded(),
            StatusBody::unavailable_read_failure(),
            StatusBody::unavailable_deadline(),
        ];
        for body in bodies {
            let error = response(body)
                .decode_body::<Echo>()
                .expect_err("unavailable bodies must not decode");
            assert!(error.source().is_none());
            assert!(!error.to_string().contains(BODY_SENTINEL));
            assert!(!format!("{error:?}").contains(BODY_SENTINEL));
        }
    }

    #[test]
    fn decoder_failure_drops_application_message_and_body() {
        let failure = response(StatusBody::complete(Bytes::from_static(
            BODY_SENTINEL.as_bytes(),
        )));
        let error = failure
            .decode_body::<Reject>()
            .expect_err("decoder should reject");
        assert!(error.source().is_none());
        assert!(!error.to_string().contains(BODY_SENTINEL));
        assert!(!format!("{error:?}").contains(BODY_SENTINEL));
    }

    fn retry_headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_str(value).expect("test value should be a header"),
        );
        headers
    }

    #[test]
    fn retry_after_parses_delta_dates_past_and_cap() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let cap = Duration::from_secs(60);
        assert_eq!(
            parse_retry_after(&retry_headers("12"), cap, now),
            Some(Duration::from_secs(12))
        );
        assert_eq!(
            parse_retry_after(&retry_headers("999999999999999999999999"), cap, now),
            Some(cap)
        );

        let future = httpdate::fmt_http_date(now + Duration::from_secs(30));
        assert_eq!(
            parse_retry_after(&retry_headers(&future), cap, now),
            Some(Duration::from_secs(30))
        );
        let far_future = httpdate::fmt_http_date(now + Duration::from_secs(120));
        assert_eq!(
            parse_retry_after(&retry_headers(&far_future), cap, now),
            Some(cap)
        );
        let past = httpdate::fmt_http_date(now - Duration::from_secs(1));
        assert_eq!(parse_retry_after(&retry_headers(&past), cap, now), None);
    }

    #[test]
    fn retry_after_rejects_malformed_values() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let cap = Duration::from_secs(60);
        for value in ["", "-1", "+1", "1.5", "tomorrow", "12 seconds"] {
            assert_eq!(parse_retry_after(&retry_headers(value), cap, now), None);
        }
        assert_eq!(parse_retry_after(&HeaderMap::new(), cap, now), None);
    }
}
