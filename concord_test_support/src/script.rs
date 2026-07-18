use std::error::Error;
use std::fmt;

use bytes::Bytes;
use http::{HeaderMap, HeaderName, Method, StatusCode};

use crate::gate::Gate;

/// The isolated managed HTTP channel used by one deterministic call.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Channel {
    Application,
    Provider,
}

/// Safe structural request-body metadata.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyCategory {
    Empty,
    Bytes,
    Streaming,
    Multipart,
    Unknown,
}

/// A centrally sanitized captured header value.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapturedHeaderValue {
    Text(String),
    Redacted,
    NonUtf8,
}

/// One safe captured header entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedHeader {
    name: HeaderName,
    value: CapturedHeaderValue,
}

impl CapturedHeader {
    #[doc(hidden)]
    #[must_use]
    pub fn __new(name: HeaderName, value: CapturedHeaderValue) -> Self {
        Self { name, value }
    }

    #[must_use]
    pub const fn name(&self) -> &HeaderName {
        &self.name
    }

    #[must_use]
    pub const fn value(&self) -> &CapturedHeaderValue {
        &self.value
    }
}

/// An ordinary deterministic capture containing safe metadata only.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedRequest {
    channel: Channel,
    endpoint: &'static str,
    method: Method,
    safe_url: Option<String>,
    headers: Vec<CapturedHeader>,
    body_category: BodyCategory,
    known_body_length: Option<usize>,
}

impl CapturedRequest {
    #[must_use]
    pub const fn channel(&self) -> Channel {
        self.channel
    }

    #[must_use]
    pub const fn endpoint(&self) -> &'static str {
        self.endpoint
    }

    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    #[must_use]
    pub fn safe_url(&self) -> Option<&str> {
        self.safe_url.as_deref()
    }

    #[must_use]
    pub fn headers(&self) -> &[CapturedHeader] {
        &self.headers
    }

    #[must_use]
    pub const fn body_category(&self) -> BodyCategory {
        self.body_category
    }

    #[must_use]
    pub const fn known_body_length(&self) -> Option<usize> {
        self.known_body_length
    }
}

/// Internal input at the final managed execution call.
#[doc(hidden)]
pub struct BoundaryRequest {
    endpoint: &'static str,
    method: Method,
    safe_url: Option<String>,
    headers: Vec<CapturedHeader>,
    body_category: BodyCategory,
    known_body_length: Option<usize>,
    expectation_body: Option<Bytes>,
    expectation_native_url: Option<String>,
    expectation_headers: HeaderMap,
}

impl BoundaryRequest {
    #[doc(hidden)]
    #[must_use]
    pub fn __new(
        endpoint: &'static str,
        method: Method,
        safe_url: Option<String>,
        headers: Vec<CapturedHeader>,
        body_category: BodyCategory,
        known_body_length: Option<usize>,
        expectation_body: Option<Bytes>,
    ) -> Self {
        Self {
            endpoint,
            method,
            safe_url,
            headers,
            body_category,
            known_body_length,
            expectation_body,
            expectation_native_url: None,
            expectation_headers: HeaderMap::new(),
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn __with_native(mut self, native_url: impl Into<String>, headers: &HeaderMap) -> Self {
        self.expectation_native_url = Some(native_url.into());
        self.expectation_headers = headers.clone();
        self
    }

    pub(crate) fn into_capture(self, channel: Channel) -> (CapturedRequest, BoundaryExpectation) {
        (
            CapturedRequest {
                channel,
                endpoint: self.endpoint,
                method: self.method,
                safe_url: self.safe_url,
                headers: self.headers,
                body_category: self.body_category,
                known_body_length: self.known_body_length,
            },
            BoundaryExpectation {
                body: self.expectation_body,
                native_url: self.expectation_native_url,
                headers: self.expectation_headers,
            },
        )
    }
}

pub(crate) struct BoundaryExpectation {
    body: Option<Bytes>,
    native_url: Option<String>,
    headers: HeaderMap,
}

/// Explicit fake-value expectations evaluated only inside the harness.
pub struct ExpectedRequest {
    method: Option<Method>,
    safe_url: Option<String>,
    body: Option<Bytes>,
    native_url: Option<String>,
    headers: HeaderMap,
}

impl ExpectedRequest {
    #[must_use]
    pub fn new() -> Self {
        Self {
            method: None,
            safe_url: None,
            body: None,
            native_url: None,
            headers: HeaderMap::new(),
        }
    }

    #[must_use]
    pub fn method(mut self, method: Method) -> Self {
        self.method = Some(method);
        self
    }

    #[must_use]
    pub fn safe_url(mut self, safe_url: impl Into<String>) -> Self {
        self.safe_url = Some(safe_url.into());
        self
    }

    #[must_use]
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Matches an exact native URL only inside the deterministic boundary.
    /// The value is never copied into captures, formatting, or failures.
    #[must_use]
    pub fn native_url(mut self, native_url: impl Into<String>) -> Self {
        self.native_url = Some(native_url.into());
        self
    }

    /// Matches one exact native header only inside the deterministic boundary.
    /// The value is never copied into captures, formatting, or failures.
    #[must_use]
    pub fn header(mut self, name: HeaderName, value: http::HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    pub(crate) fn matches(
        &self,
        captured: &CapturedRequest,
        expectation: &BoundaryExpectation,
    ) -> bool {
        self.method
            .as_ref()
            .is_none_or(|expected| expected == captured.method())
            && self
                .safe_url
                .as_deref()
                .is_none_or(|expected| Some(expected) == captured.safe_url())
            && self
                .body
                .as_ref()
                .is_none_or(|expected| expectation.body.as_ref() == Some(expected))
            && self
                .native_url
                .as_deref()
                .is_none_or(|expected| expectation.native_url.as_deref() == Some(expected))
            && self
                .headers
                .iter()
                .all(|(name, value)| expectation.headers.get(name) == Some(value))
    }
}

impl Default for ExpectedRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ExpectedRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExpectedRequest")
            .field("method", &self.method.is_some())
            .field("safe_url", &self.safe_url.is_some())
            .field("body", &self.body.is_some())
            .field("native_url", &self.native_url.is_some())
            .field("headers", &self.headers.len())
            .finish()
    }
}

/// A complete scripted response delivered at the managed boundary.
pub struct ScriptedResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
    delivery_gate: Option<Gate>,
    body_polls: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
}

impl ScriptedResponse {
    #[must_use]
    pub fn success(body: impl Into<Bytes>) -> Self {
        Self::status(StatusCode::OK, body)
    }

    #[must_use]
    pub fn status(status: StatusCode, body: impl Into<Bytes>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: body.into(),
            delivery_gate: None,
            body_polls: None,
        }
    }

    #[must_use]
    pub fn headers(mut self, headers: HeaderMap) -> Self {
        self.headers = headers;
        self
    }

    #[must_use]
    pub fn delivery_gate(mut self, gate: Gate) -> Self {
        self.delivery_gate = Some(gate);
        self
    }

    /// Observes native response-body polling without retaining response bytes
    /// in deterministic captures or diagnostics.
    #[must_use]
    pub fn body_poll_counter(
        mut self,
        counter: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> Self {
        self.body_polls = Some(counter);
        self
    }

    pub(crate) fn gate(&self) -> Option<Gate> {
        self.delivery_gate.clone()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn __into_http_response(self) -> http::Response<Bytes> {
        let (status, headers, body, _) = self.__into_parts();
        let mut response = http::Response::new(body);
        *response.status_mut() = status;
        *response.headers_mut() = headers;
        response
    }

    #[doc(hidden)]
    #[must_use]
    pub fn __into_parts(
        self,
    ) -> (
        StatusCode,
        HeaderMap,
        Bytes,
        Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
    ) {
        (self.status, self.headers, self.body, self.body_polls)
    }
}

impl fmt::Debug for ScriptedResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScriptedResponse")
            .field("status", &self.status)
            .field("header_values", &self.headers.len())
            .field("body_len", &self.body.len())
            .field("delivery_gate", &self.delivery_gate.is_some())
            .field("body_poll_counter", &self.body_polls.is_some())
            .finish()
    }
}

/// Safe native-failure classes that may be scripted.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportFailure {
    Timeout,
    Connect,
    RequestBody,
    Transport,
}

/// Safe deterministic execution failure kinds.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionFailureKind {
    Scripted(TransportFailure),
    EmptyQueue,
    Mismatch,
    CancellationPoint,
    Internal,
}

/// A value-free deterministic execution failure.
pub struct ExecutionFailure {
    kind: ExecutionFailureKind,
}

impl ExecutionFailure {
    pub(crate) const fn new(kind: ExecutionFailureKind) -> Self {
        Self { kind }
    }

    #[must_use]
    pub const fn kind(&self) -> ExecutionFailureKind {
        self.kind
    }
}

impl fmt::Display for ExecutionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "deterministic execution failed ({:?})",
            self.kind
        )
    }
}

impl fmt::Debug for ExecutionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionFailure")
            .field("kind", &self.kind)
            .finish()
    }
}

impl Error for ExecutionFailure {}

pub(crate) enum Script {
    Exchange {
        expectation: Option<ExpectedRequest>,
        outcome: Outcome,
    },
    Gate(Gate),
    CancellationPoint(Gate),
}

pub(crate) enum Outcome {
    Response(ScriptedResponse),
    Failure(TransportFailure),
}

impl Script {
    pub(crate) fn delivery_gate(&self) -> Option<Gate> {
        match self {
            Self::Exchange {
                outcome: Outcome::Response(response),
                ..
            } => response.gate(),
            Self::Gate(gate) | Self::CancellationPoint(gate) => Some(gate.clone()),
            Self::Exchange { .. } => None,
        }
    }
}
