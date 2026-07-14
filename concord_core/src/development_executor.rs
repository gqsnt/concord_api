//! Private implementation of the feature-gated deterministic native executor.
//!
//! The narrow public test surface is re-exported only from `__development`.

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use http_body::{Body, Frame, SizeHint};
use std::collections::VecDeque;
use std::fmt;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

/// Identifies the separately owned native execution authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeterministicExecutionKind {
    Application,
    Provider,
}

/// Safe request-body shape recorded by the deterministic executor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapturedBodyCategory {
    Empty,
    Buffered,
    Streaming,
    Multipart,
}

/// A sanitized observation of one native request submission.
///
/// `logical_target` is derived only from Concord's pre-authentication logical
/// URL. Native materialized URLs and request body bytes are never retained.
#[derive(Clone, Debug)]
pub struct CapturedNativeRequest {
    method: http::Method,
    logical_target: url::Url,
    endpoint: &'static str,
    page_index: u32,
    idempotent: bool,
    execution_kind: DeterministicExecutionKind,
    public_headers: HeaderMap,
    protected_header_names: Vec<HeaderName>,
    body_category: CapturedBodyCategory,
    known_body_length: Option<u64>,
    timeout: Option<Duration>,
}

impl CapturedNativeRequest {
    pub fn method(&self) -> &http::Method {
        &self.method
    }

    pub fn logical_target(&self) -> &url::Url {
        &self.logical_target
    }

    pub fn endpoint(&self) -> &'static str {
        self.endpoint
    }

    pub fn page_index(&self) -> u32 {
        self.page_index
    }

    pub fn idempotent(&self) -> bool {
        self.idempotent
    }

    pub fn execution_kind(&self) -> DeterministicExecutionKind {
        self.execution_kind
    }

    pub fn public_headers(&self) -> &HeaderMap {
        &self.public_headers
    }

    pub fn protected_header_names(&self) -> &[HeaderName] {
        &self.protected_header_names
    }

    pub fn body_category(&self) -> CapturedBodyCategory {
        self.body_category
    }

    pub fn known_body_length(&self) -> Option<u64> {
        self.known_body_length
    }

    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }
}

/// Focused execution failures that public Reqwest APIs cannot synthesize.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntheticExecutionFailure {
    Timeout,
    Connect,
    Request,
    Body,
}

/// A deterministic fake value used only by explicit unsafe placement checks.
///
/// Callers must use non-production, deterministic test credentials. The value
/// is intentionally unavailable after construction and is always redacted in
/// diagnostics.
#[derive(Clone, Eq, PartialEq)]
pub struct DeterministicFakeCredential(String);

impl DeterministicFakeCredential {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl fmt::Debug for DeterministicFakeCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeterministicFakeCredential(<redacted>)")
    }
}

impl fmt::Display for DeterministicFakeCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted deterministic fake credential>")
    }
}

#[derive(Clone)]
enum UnsafeCredentialExpectation {
    Header {
        name: HeaderName,
        value: DeterministicFakeCredential,
    },
    Query {
        name: String,
        value: DeterministicFakeCredential,
    },
    BodyCategory(CapturedBodyCategory),
}

/// Explicit, dangerous native credential-placement expectations.
///
/// This object never exposes a native request or captured credential value.
/// Failed expectations become a fixed synthetic request failure with redacted
/// diagnostics.
#[derive(Clone, Default)]
pub struct UnsafeCredentialPlacementExpectations {
    expectations: Vec<UnsafeCredentialExpectation>,
}

impl UnsafeCredentialPlacementExpectations {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn expect_header(mut self, name: HeaderName, value: DeterministicFakeCredential) -> Self {
        self.expectations
            .push(UnsafeCredentialExpectation::Header { name, value });
        self
    }

    pub fn expect_query_pair(
        mut self,
        name: impl Into<String>,
        value: DeterministicFakeCredential,
    ) -> Self {
        self.expectations.push(UnsafeCredentialExpectation::Query {
            name: name.into(),
            value,
        });
        self
    }

    pub fn expect_body_category(mut self, category: CapturedBodyCategory) -> Self {
        self.expectations
            .push(UnsafeCredentialExpectation::BodyCategory(category));
        self
    }

    fn matches(&self, request: &reqwest::Request, body_category: CapturedBodyCategory) -> bool {
        self.expectations
            .iter()
            .all(|expectation| match expectation {
                UnsafeCredentialExpectation::Header { name, value } => request
                    .headers()
                    .get_all(name)
                    .iter()
                    .any(|actual| actual.as_bytes() == value.0.as_bytes()),
                UnsafeCredentialExpectation::Query { name, value } => request
                    .url()
                    .query_pairs()
                    .any(|(actual_name, actual_value)| {
                        actual_name == name.as_str()
                            && actual_value.as_bytes() == value.0.as_bytes()
                    }),
                UnsafeCredentialExpectation::BodyCategory(expected) => *expected == body_category,
            })
    }
}

impl fmt::Debug for UnsafeCredentialPlacementExpectations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnsafeCredentialPlacementExpectations")
            .field("expectation_count", &self.expectations.len())
            .field("values", &"<redacted>")
            .finish()
    }
}

impl fmt::Display for UnsafeCredentialPlacementExpectations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unsafe credential-placement expectations (<values redacted>)")
    }
}

/// A manually released gate for deterministic response-body delivery.
#[derive(Clone, Default)]
pub struct DeterministicBodyGate {
    inner: Arc<BodyGateInner>,
}

#[derive(Default)]
struct BodyGateInner {
    released: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl DeterministicBodyGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn release(&self) {
        self.inner.released.store(true, Ordering::Release);
        if let Some(waker) = lock(&self.inner.waker).take() {
            waker.wake();
        }
    }
}

impl fmt::Debug for DeterministicBodyGate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeterministicBodyGate")
            .field("released", &self.inner.released.load(Ordering::Acquire))
            .finish()
    }
}

enum ScriptedBodyStep {
    Chunk(Bytes),
    Trailers(HeaderMap),
    Failure,
}

/// A scripted successful native response.
///
/// Conversion to `reqwest::Response` occurs only at execution time through
/// `http::Response<reqwest::Body> -> reqwest::Response`.
pub struct ScriptedNativeResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<ScriptedBodyStep>,
    gate: Option<DeterministicBodyGate>,
    unsafe_expectations: UnsafeCredentialPlacementExpectations,
}

impl ScriptedNativeResponse {
    pub fn bytes(status: StatusCode, body: impl Into<Bytes>) -> Self {
        let body = body.into();
        let body = if body.is_empty() {
            Vec::new()
        } else {
            vec![ScriptedBodyStep::Chunk(body)]
        };
        Self {
            status,
            headers: HeaderMap::new(),
            body,
            gate: None,
            unsafe_expectations: UnsafeCredentialPlacementExpectations::new(),
        }
    }

    pub fn chunks(status: StatusCode, chunks: impl IntoIterator<Item = Bytes>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: chunks.into_iter().map(ScriptedBodyStep::Chunk).collect(),
            gate: None,
            unsafe_expectations: UnsafeCredentialPlacementExpectations::new(),
        }
    }

    pub fn with_header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.append(name, value);
        self
    }

    pub fn with_trailers(mut self, trailers: HeaderMap) -> Self {
        self.body.push(ScriptedBodyStep::Trailers(trailers));
        self
    }

    pub fn with_body_failure(mut self) -> Self {
        self.body.push(ScriptedBodyStep::Failure);
        self
    }

    pub fn with_gate(mut self, gate: DeterministicBodyGate) -> Self {
        self.gate = Some(gate);
        self
    }

    pub fn with_unsafe_credential_placement_expectations(
        mut self,
        expectations: UnsafeCredentialPlacementExpectations,
    ) -> Self {
        self.unsafe_expectations = expectations;
        self
    }

    fn into_native(self) -> reqwest::Response {
        let body = reqwest::Body::wrap(ScriptedResponseBody::new(self.body, self.gate));
        let mut response = http::Response::new(body);
        *response.status_mut() = self.status;
        *response.headers_mut() = self.headers;
        reqwest::Response::from(response)
    }
}

impl fmt::Debug for ScriptedNativeResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScriptedNativeResponse")
            .field("status", &self.status)
            .field(
                "headers",
                &crate::debug::SanitizedHeaders::new(&self.headers),
            )
            .field("body", &"<scripted native body>")
            .field("unsafe_expectations", &self.unsafe_expectations)
            .finish()
    }
}

enum ScriptedOutcome {
    Response(ScriptedNativeResponse),
    Failure(SyntheticExecutionFailure),
}

struct ExecutorState {
    kind: DeterministicExecutionKind,
    scripts: Mutex<VecDeque<ScriptedOutcome>>,
    captures: Mutex<Vec<CapturedNativeRequest>>,
}

/// Opaque deterministic executor installed only through `__development`.
#[derive(Clone)]
pub struct DeterministicNativeExecutor {
    state: Arc<ExecutorState>,
}

impl DeterministicNativeExecutor {
    pub fn application() -> Self {
        Self::new(DeterministicExecutionKind::Application)
    }

    pub fn provider() -> Self {
        Self::new(DeterministicExecutionKind::Provider)
    }

    fn new(kind: DeterministicExecutionKind) -> Self {
        Self {
            state: Arc::new(ExecutorState {
                kind,
                scripts: Mutex::new(VecDeque::new()),
                captures: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn kind(&self) -> DeterministicExecutionKind {
        self.state.kind
    }

    pub fn script_response(&self, response: ScriptedNativeResponse) {
        lock(&self.state.scripts).push_back(ScriptedOutcome::Response(response));
    }

    pub fn script_failure(&self, failure: SyntheticExecutionFailure) {
        lock(&self.state.scripts).push_back(ScriptedOutcome::Failure(failure));
    }

    pub fn captures(&self) -> Vec<CapturedNativeRequest> {
        lock(&self.state.captures).clone()
    }

    pub fn remaining_scripts(&self) -> usize {
        lock(&self.state.scripts).len()
    }

    pub(crate) async fn execute_native(
        &self,
        request: reqwest::Request,
        context: Option<&crate::transport::RequestExecutionContext>,
    ) -> Result<reqwest::Response, crate::transport::ReqwestError> {
        let Some(context) = context else {
            return Err(map_failure(SyntheticExecutionFailure::Request));
        };
        let body_category = body_category(&request);
        let capture = sanitize_capture(&request, context, self.state.kind, body_category);
        lock(&self.state.captures).push(capture);

        let Some(outcome) = lock(&self.state.scripts).pop_front() else {
            return Err(map_failure(SyntheticExecutionFailure::Request));
        };
        match outcome {
            ScriptedOutcome::Response(response) => {
                if !response
                    .unsafe_expectations
                    .matches(&request, body_category)
                {
                    return Err(map_failure(SyntheticExecutionFailure::Request));
                }
                Ok(response.into_native())
            }
            ScriptedOutcome::Failure(failure) => Err(map_failure(failure)),
        }
    }
}

impl fmt::Debug for DeterministicNativeExecutor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeterministicNativeExecutor")
            .field("kind", &self.state.kind)
            .field("remaining_scripts", &lock(&self.state.scripts).len())
            .field("capture_count", &lock(&self.state.captures).len())
            .finish()
    }
}

/// Safe installation failure; it retains no executor or request data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeterministicExecutorInstallationError;

impl fmt::Display for DeterministicExecutorInstallationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("deterministic executor execution kind does not match installation channel")
    }
}

impl std::error::Error for DeterministicExecutorInstallationError {}

pub fn install_application_executor<Cx: crate::client::ClientContext>(
    client: &mut crate::client::ApiClient<Cx>,
    executor: DeterministicNativeExecutor,
) -> Result<(), DeterministicExecutorInstallationError> {
    if executor.kind() != DeterministicExecutionKind::Application {
        return Err(DeterministicExecutorInstallationError);
    }
    client.install_development_application_executor(executor);
    Ok(())
}

pub fn install_provider_executor<Cx: crate::client::ClientContext>(
    client: &mut crate::client::ApiClient<Cx>,
    executor: DeterministicNativeExecutor,
) -> Result<(), DeterministicExecutorInstallationError> {
    if executor.kind() != DeterministicExecutionKind::Provider {
        return Err(DeterministicExecutorInstallationError);
    }
    client.install_development_provider_executor(executor);
    Ok(())
}

fn sanitize_capture(
    request: &reqwest::Request,
    context: &crate::transport::RequestExecutionContext,
    execution_kind: DeterministicExecutionKind,
    body_category: CapturedBodyCategory,
) -> CapturedNativeRequest {
    // This target is deliberately derived only from the pre-auth logical URL.
    let mut logical_target = context.logical_url.clone();
    if logical_target.query().is_some() && !context.auth_query_keys.is_empty() {
        let retained = logical_target
            .query_pairs()
            .filter(|(name, _)| {
                !context
                    .auth_query_keys
                    .iter()
                    .any(|protected| name.eq_ignore_ascii_case(protected))
            })
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect::<Vec<_>>();
        logical_target.set_query(None);
        if !retained.is_empty() {
            logical_target.query_pairs_mut().extend_pairs(retained);
        }
    }

    let mut public_headers = HeaderMap::new();
    let mut protected_header_names = context.protected_header_names.clone();
    for (name, value) in request.headers() {
        let protected = protected_header_names.iter().any(|item| item == name)
            || crate::redaction::should_redact_header_name(name);
        if protected {
            if !protected_header_names.iter().any(|item| item == name) {
                protected_header_names.push(name.clone());
            }
        } else {
            public_headers.append(name.clone(), value.clone());
        }
    }
    protected_header_names.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
    protected_header_names.dedup();

    CapturedNativeRequest {
        method: context.meta.method.clone(),
        logical_target,
        endpoint: context.meta.endpoint,
        page_index: context.meta.page_index,
        idempotent: context.meta.idempotent,
        execution_kind,
        public_headers,
        protected_header_names,
        body_category,
        known_body_length: request.body().and_then(|body| body.size_hint().exact()),
        timeout: context.timeout,
    }
}

fn body_category(request: &reqwest::Request) -> CapturedBodyCategory {
    let Some(body) = request.body() else {
        return CapturedBodyCategory::Empty;
    };
    if request
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("multipart/"))
    {
        return CapturedBodyCategory::Multipart;
    }
    if body.as_bytes().is_some() {
        CapturedBodyCategory::Buffered
    } else {
        CapturedBodyCategory::Streaming
    }
}

fn map_failure(failure: SyntheticExecutionFailure) -> crate::transport::ReqwestError {
    use crate::transport::{ReqwestError, ReqwestErrorKind};
    match failure {
        SyntheticExecutionFailure::Timeout => ReqwestError::with_kind(
            ReqwestErrorKind::Timeout,
            OpaqueSyntheticExecutionFailure(failure),
        ),
        SyntheticExecutionFailure::Connect => ReqwestError::with_kind(
            ReqwestErrorKind::Connect,
            OpaqueSyntheticExecutionFailure(failure),
        ),
        SyntheticExecutionFailure::Request => ReqwestError::with_kind(
            ReqwestErrorKind::Request,
            OpaqueSyntheticExecutionFailure(failure),
        ),
        SyntheticExecutionFailure::Body => {
            ReqwestError::with_kind(ReqwestErrorKind::Io, crate::body::BodyError::input())
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct OpaqueSyntheticExecutionFailure(SyntheticExecutionFailure);

impl fmt::Display for OpaqueSyntheticExecutionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "synthetic request execution failure ({:?})", self.0)
    }
}

impl std::error::Error for OpaqueSyntheticExecutionFailure {}

struct ScriptedResponseBody {
    steps: VecDeque<ScriptedBodyStep>,
    gate: Option<DeterministicBodyGate>,
    terminal: bool,
}

impl ScriptedResponseBody {
    fn new(steps: Vec<ScriptedBodyStep>, gate: Option<DeterministicBodyGate>) -> Self {
        Self {
            steps: steps.into(),
            gate,
            terminal: false,
        }
    }
}

#[derive(Debug)]
struct ScriptedResponseBodyFailure;

impl fmt::Display for ScriptedResponseBodyFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("synthetic response body failure")
    }
}

impl std::error::Error for ScriptedResponseBodyFailure {}

impl Body for ScriptedResponseBody {
    type Data = Bytes;
    type Error = ScriptedResponseBodyFailure;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        if self.terminal {
            return Poll::Ready(None);
        }
        if let Some(gate) = &self.gate
            && !gate.inner.released.load(Ordering::Acquire)
        {
            *lock(&gate.inner.waker) = Some(cx.waker().clone());
            if !gate.inner.released.load(Ordering::Acquire) {
                return Poll::Pending;
            }
        }
        self.gate = None;
        match self.steps.pop_front() {
            Some(ScriptedBodyStep::Chunk(bytes)) => Poll::Ready(Some(Ok(Frame::data(bytes)))),
            Some(ScriptedBodyStep::Trailers(trailers)) => {
                Poll::Ready(Some(Ok(Frame::trailers(trailers))))
            }
            Some(ScriptedBodyStep::Failure) => {
                self.terminal = true;
                Poll::Ready(Some(Err(ScriptedResponseBodyFailure)))
            }
            None => {
                self.terminal = true;
                Poll::Ready(None)
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.terminal || self.steps.is_empty()
    }

    fn size_hint(&self) -> SizeHint {
        if self.terminal {
            return SizeHint::with_exact(0);
        }
        let length = self.steps.iter().fold(0_u64, |total, step| {
            total.saturating_add(match step {
                ScriptedBodyStep::Chunk(bytes) => bytes.len() as u64,
                ScriptedBodyStep::Trailers(_) | ScriptedBodyStep::Failure => 0,
            })
        });
        if self
            .steps
            .iter()
            .any(|step| matches!(step, ScriptedBodyStep::Failure))
        {
            let mut hint = SizeHint::new();
            hint.set_upper(length);
            hint
        } else {
            SizeHint::with_exact(length)
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_diagnostics_redact_exact_values() {
        let sentinel = "DETERMINISTIC_FAKE_CREDENTIAL_SENTINEL";
        let credential = DeterministicFakeCredential::new(sentinel);
        let expectations = UnsafeCredentialPlacementExpectations::new()
            .expect_header(http::header::AUTHORIZATION, credential.clone());
        let response = ScriptedNativeResponse::bytes(StatusCode::OK, Bytes::new())
            .with_unsafe_credential_placement_expectations(expectations.clone());
        let executor = DeterministicNativeExecutor::application();
        executor.script_response(response);

        for diagnostic in [
            format!("{credential:?}"),
            credential.to_string(),
            format!("{expectations:?}"),
            expectations.to_string(),
            format!("{executor:?}"),
        ] {
            assert!(!diagnostic.contains(sentinel), "{diagnostic}");
        }
    }

    #[test]
    fn synthetic_success_is_a_native_reqwest_response() {
        let response =
            ScriptedNativeResponse::chunks(StatusCode::ACCEPTED, [Bytes::from_static(b"native")])
                .into_native();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }
}
