//! Private implementation of the feature-gated deterministic native executor.
//!
//! The narrow public test surface is re-exported only from `__development`.

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use http_body::{Body, Frame, SizeHint};
use std::collections::VecDeque;
use std::fmt;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
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
    sequence: u64,
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
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

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

/// The single terminal observation for request-body handling in one native
/// execution. No request bytes are exposed by these lifecycle events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestBodyTerminalObservation {
    Completed,
    Failed,
    CancelledOrDropped,
    NeverPolled,
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

/// Explicitly unsafe deterministic fake request bytes used only by an
/// in-executor equality expectation.
///
/// The bytes are never returned, captured, formatted, or included in mismatch
/// diagnostics. Callers must never construct this value from production data.
#[derive(Clone, Eq, PartialEq)]
pub struct UnsafeDeterministicFakeBody(Bytes);

impl UnsafeDeterministicFakeBody {
    pub fn new(value: impl Into<Bytes>) -> Self {
        Self(value.into())
    }
}

impl fmt::Debug for UnsafeDeterministicFakeBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("UnsafeDeterministicFakeBody(<redacted>)")
    }
}

impl fmt::Display for UnsafeDeterministicFakeBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted deterministic fake request body>")
    }
}

/// Dangerous request-body expectations evaluated without retaining request
/// bytes. Equality is checked incrementally while the native body is polled.
#[derive(Clone, Default)]
pub struct UnsafeRequestBodyExpectations {
    exact: Option<UnsafeDeterministicFakeBody>,
}

impl UnsafeRequestBodyExpectations {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn expect_exact(mut self, value: UnsafeDeterministicFakeBody) -> Self {
        self.exact = Some(value);
        self
    }
}

impl fmt::Debug for UnsafeRequestBodyExpectations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnsafeRequestBodyExpectations")
            .field("exact", &self.exact.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl fmt::Display for UnsafeRequestBodyExpectations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unsafe request-body expectations (<values redacted>)")
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
    state: Mutex<BodyGateState>,
    entered_condvar: Condvar,
}

#[derive(Default)]
struct BodyGateState {
    entered: bool,
    released: bool,
    waker: Option<Waker>,
}

impl DeterministicBodyGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn release(&self) {
        let waker = {
            let mut state = lock(&self.inner.state);
            state.released = true;
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    pub fn wait_until_entered(&self, timeout: Duration) -> bool {
        let state = lock(&self.inner.state);
        if state.entered {
            return true;
        }
        let (state, _result) = self
            .inner
            .entered_condvar
            .wait_timeout_while(state, timeout, |state| !state.entered)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.entered
    }

    fn poll_ready(&self, waker: &Waker) -> bool {
        let (released, entered_now) = {
            let mut state = lock(&self.inner.state);
            let entered_now = !state.entered;
            state.entered = true;
            if !state.released
                && state
                    .waker
                    .as_ref()
                    .is_none_or(|registered| !registered.will_wake(waker))
            {
                state.waker = Some(waker.clone());
            }
            (state.released, entered_now)
        };
        if entered_now {
            self.inner.entered_condvar.notify_all();
        }
        released
    }

    #[cfg(test)]
    fn is_entered(&self) -> bool {
        lock(&self.inner.state).entered
    }
}

impl fmt::Debug for DeterministicBodyGate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = lock(&self.inner.state);
        f.debug_struct("DeterministicBodyGate")
            .field("released", &state.released)
            .field("entered", &state.entered)
            .finish()
    }
}

#[derive(Clone)]
enum ScriptedBodyStep {
    Chunk(Bytes),
    Trailers(HeaderMap),
    Gate(DeterministicBodyGate),
    Failure,
}

/// Ordered response-body actions for focused streaming tests.
#[derive(Clone)]
pub enum ScriptedResponseBodyStep {
    Chunk(Bytes),
    Gate(DeterministicBodyGate),
    Failure,
}

impl fmt::Debug for ScriptedResponseBodyStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Chunk(bytes) => f
                .debug_tuple("Chunk")
                .field(&format_args!("<{} bytes>", bytes.len()))
                .finish(),
            Self::Gate(gate) => f.debug_tuple("Gate").field(gate).finish(),
            Self::Failure => f.write_str("Failure"),
        }
    }
}

/// A scripted successful native response.
///
/// Conversion to `reqwest::Response` occurs only at execution time through
/// `http::Response<reqwest::Body> -> reqwest::Response`.
#[derive(Clone)]
pub struct ScriptedNativeResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<ScriptedBodyStep>,
    gate: Option<DeterministicBodyGate>,
    unsafe_expectations: UnsafeCredentialPlacementExpectations,
    unsafe_body_expectations: UnsafeRequestBodyExpectations,
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
            unsafe_body_expectations: UnsafeRequestBodyExpectations::new(),
        }
    }

    pub fn chunks(status: StatusCode, chunks: impl IntoIterator<Item = Bytes>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: chunks.into_iter().map(ScriptedBodyStep::Chunk).collect(),
            gate: None,
            unsafe_expectations: UnsafeCredentialPlacementExpectations::new(),
            unsafe_body_expectations: UnsafeRequestBodyExpectations::new(),
        }
    }

    pub fn body_steps(
        status: StatusCode,
        steps: impl IntoIterator<Item = ScriptedResponseBodyStep>,
    ) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: steps
                .into_iter()
                .map(|step| match step {
                    ScriptedResponseBodyStep::Chunk(bytes) => ScriptedBodyStep::Chunk(bytes),
                    ScriptedResponseBodyStep::Gate(gate) => ScriptedBodyStep::Gate(gate),
                    ScriptedResponseBodyStep::Failure => ScriptedBodyStep::Failure,
                })
                .collect(),
            gate: None,
            unsafe_expectations: UnsafeCredentialPlacementExpectations::new(),
            unsafe_body_expectations: UnsafeRequestBodyExpectations::new(),
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

    pub fn with_unsafe_request_body_expectations(
        mut self,
        expectations: UnsafeRequestBodyExpectations,
    ) -> Self {
        self.unsafe_body_expectations = expectations;
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
            .field("unsafe_body_expectations", &self.unsafe_body_expectations)
            .finish()
    }
}

#[derive(Clone)]
enum ScriptedOutcome {
    Response(ScriptedNativeResponse),
    Failure {
        failure: SyntheticExecutionFailure,
        after_request_body: bool,
        unsafe_expectations: UnsafeCredentialPlacementExpectations,
        unsafe_body_expectations: UnsafeRequestBodyExpectations,
    },
}

type RequestObserver = Arc<dyn Fn() + Send + Sync>;
type RequestBodyObserver = Arc<dyn Fn(RequestBodyTerminalObservation) + Send + Sync>;

struct ExecutorState {
    kind: DeterministicExecutionKind,
    scripts: Mutex<VecDeque<ScriptedOutcome>>,
    captures: Mutex<Vec<CapturedNativeRequest>>,
    repeating: Mutex<Option<ScriptedOutcome>>,
    unexpected_executions: Mutex<usize>,
    request_head_observer: Mutex<Option<RequestObserver>>,
    request_body_terminal_observer: Mutex<Option<RequestBodyObserver>>,
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
                repeating: Mutex::new(None),
                unexpected_executions: Mutex::new(0),
                request_head_observer: Mutex::new(None),
                request_body_terminal_observer: Mutex::new(None),
            }),
        }
    }

    pub fn kind(&self) -> DeterministicExecutionKind {
        self.state.kind
    }

    pub fn script_response(&self, response: ScriptedNativeResponse) {
        lock(&self.state.scripts).push_back(ScriptedOutcome::Response(response));
    }

    pub fn script_repeating_response(&self, response: ScriptedNativeResponse) {
        *lock(&self.state.repeating) = Some(ScriptedOutcome::Response(response));
    }

    pub fn script_failure(&self, failure: SyntheticExecutionFailure) {
        lock(&self.state.scripts).push_back(ScriptedOutcome::Failure {
            failure,
            after_request_body: false,
            unsafe_expectations: UnsafeCredentialPlacementExpectations::new(),
            unsafe_body_expectations: UnsafeRequestBodyExpectations::new(),
        });
    }

    pub fn script_failure_after_request_body(
        &self,
        failure: SyntheticExecutionFailure,
        unsafe_expectations: UnsafeCredentialPlacementExpectations,
        unsafe_body_expectations: UnsafeRequestBodyExpectations,
    ) {
        lock(&self.state.scripts).push_back(ScriptedOutcome::Failure {
            failure,
            after_request_body: true,
            unsafe_expectations,
            unsafe_body_expectations,
        });
    }

    pub fn script_repeating_failure_after_request_body(&self, failure: SyntheticExecutionFailure) {
        *lock(&self.state.repeating) = Some(ScriptedOutcome::Failure {
            failure,
            after_request_body: true,
            unsafe_expectations: UnsafeCredentialPlacementExpectations::new(),
            unsafe_body_expectations: UnsafeRequestBodyExpectations::new(),
        });
    }

    pub fn script_repeating_failure(&self, failure: SyntheticExecutionFailure) {
        *lock(&self.state.repeating) = Some(ScriptedOutcome::Failure {
            failure,
            after_request_body: false,
            unsafe_expectations: UnsafeCredentialPlacementExpectations::new(),
            unsafe_body_expectations: UnsafeRequestBodyExpectations::new(),
        });
    }

    pub fn set_request_head_observer(&self, observer: impl Fn() + Send + Sync + 'static) {
        *lock(&self.state.request_head_observer) = Some(Arc::new(observer));
    }

    pub fn set_request_body_terminal_observer(
        &self,
        observer: impl Fn(RequestBodyTerminalObservation) + Send + Sync + 'static,
    ) {
        *lock(&self.state.request_body_terminal_observer) = Some(Arc::new(observer));
    }

    pub fn captures(&self) -> Vec<CapturedNativeRequest> {
        lock(&self.state.captures).clone()
    }

    pub fn remaining_scripts(&self) -> usize {
        lock(&self.state.scripts).len()
    }

    pub fn unexpected_execution_count(&self) -> usize {
        *lock(&self.state.unexpected_executions)
    }

    pub(crate) async fn execute_native(
        &self,
        mut request: reqwest::Request,
        context: Option<&crate::transport::RequestExecutionContext>,
    ) -> Result<reqwest::Response, crate::transport::ReqwestError> {
        let mut body_terminal =
            RequestBodyTerminalGuard::new(lock(&self.state.request_body_terminal_observer).clone());
        let Some(context) = context else {
            return Err(map_failure(SyntheticExecutionFailure::Request));
        };
        let body_category = body_category(&request);
        let capture = sanitize_capture(&request, context, self.state.kind, body_category);
        lock(&self.state.captures).push(capture);

        if let Some(observer) = lock(&self.state.request_head_observer).clone() {
            observer();
        }
        let outcome = lock(&self.state.scripts)
            .pop_front()
            .or_else(|| lock(&self.state.repeating).clone());
        let Some(outcome) = outcome else {
            *lock(&self.state.unexpected_executions) += 1;
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
                drain_request_body(
                    &mut request,
                    context,
                    &response.unsafe_body_expectations,
                    &mut body_terminal,
                )
                .await?;
                Ok(response.into_native())
            }
            ScriptedOutcome::Failure {
                failure,
                after_request_body,
                unsafe_expectations,
                unsafe_body_expectations,
            } => {
                if !unsafe_expectations.matches(&request, body_category) {
                    return Err(map_failure(SyntheticExecutionFailure::Request));
                }
                if after_request_body {
                    drain_request_body(
                        &mut request,
                        context,
                        &unsafe_body_expectations,
                        &mut body_terminal,
                    )
                    .await?;
                }
                Err(map_failure(failure))
            }
        }
    }
}

impl fmt::Debug for DeterministicNativeExecutor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeterministicNativeExecutor")
            .field("kind", &self.state.kind)
            .field("remaining_scripts", &lock(&self.state.scripts).len())
            .field("capture_count", &lock(&self.state.captures).len())
            .field(
                "unexpected_execution_count",
                &*lock(&self.state.unexpected_executions),
            )
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

pub fn configure_application_executor(
    builder: crate::transport::SafeReqwestBuilder,
    executor: DeterministicNativeExecutor,
) -> Result<crate::transport::SafeReqwestBuilder, DeterministicExecutorInstallationError> {
    if executor.kind() != DeterministicExecutionKind::Application {
        return Err(DeterministicExecutorInstallationError);
    }
    Ok(builder.with_development_application_executor(executor))
}

pub fn configure_provider_executor(
    builder: crate::transport::SafeReqwestBuilder,
    executor: DeterministicNativeExecutor,
) -> Result<crate::transport::SafeReqwestBuilder, DeterministicExecutorInstallationError> {
    if executor.kind() != DeterministicExecutionKind::Provider {
        return Err(DeterministicExecutorInstallationError);
    }
    Ok(builder.with_development_provider_executor(executor))
}

#[derive(Clone, Copy)]
enum RequestBodyProgress {
    NeverPolled,
    Polling,
}

struct RequestBodyTerminalGuard {
    observer: Option<RequestBodyObserver>,
    progress: RequestBodyProgress,
}

impl RequestBodyTerminalGuard {
    fn new(observer: Option<RequestBodyObserver>) -> Self {
        Self {
            observer,
            progress: RequestBodyProgress::NeverPolled,
        }
    }

    fn begin_polling(&mut self) {
        self.progress = RequestBodyProgress::Polling;
    }

    fn finish(&mut self, observation: RequestBodyTerminalObservation) {
        if let Some(observer) = self.observer.take() {
            observer(observation);
        }
    }
}

impl Drop for RequestBodyTerminalGuard {
    fn drop(&mut self) {
        let observation = match self.progress {
            RequestBodyProgress::NeverPolled => RequestBodyTerminalObservation::NeverPolled,
            RequestBodyProgress::Polling => RequestBodyTerminalObservation::CancelledOrDropped,
        };
        if let Some(observer) = self.observer.take() {
            observer(observation);
        }
    }
}

async fn drain_request_body(
    request: &mut reqwest::Request,
    context: &crate::transport::RequestExecutionContext,
    expectations: &UnsafeRequestBodyExpectations,
    terminal: &mut RequestBodyTerminalGuard,
) -> Result<(), crate::transport::ReqwestError> {
    terminal.begin_polling();
    let Some(mut body) = request.body_mut().take() else {
        let result = if expectations
            .exact
            .as_ref()
            .is_none_or(|expected| expected.0.is_empty())
        {
            Ok(())
        } else {
            Err(map_failure(SyntheticExecutionFailure::Request))
        };
        terminal.finish(if result.is_ok() {
            RequestBodyTerminalObservation::Completed
        } else {
            RequestBodyTerminalObservation::Failed
        });
        return result;
    };
    let mut offset = 0_usize;
    loop {
        let frame = std::future::poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await;
        let Some(frame) = frame else {
            break;
        };
        let frame = match frame.map_err(|error| {
            context
                .body_errors
                .get()
                .map(crate::transport::ReqwestError::from)
                .unwrap_or_else(|| crate::transport::ReqwestError::from(error))
        }) {
            Ok(frame) => frame,
            Err(error) => {
                terminal.finish(RequestBodyTerminalObservation::Failed);
                return Err(error);
            }
        };
        let Ok(data) = frame.into_data() else {
            continue;
        };
        if let Some(expected) = &expectations.exact {
            let end = offset.saturating_add(data.len());
            if expected.0.get(offset..end) != Some(data.as_ref()) {
                terminal.finish(RequestBodyTerminalObservation::Failed);
                return Err(map_failure(SyntheticExecutionFailure::Request));
            }
            offset = end;
        }
    }
    if expectations
        .exact
        .as_ref()
        .is_some_and(|expected| offset != expected.0.len())
    {
        terminal.finish(RequestBodyTerminalObservation::Failed);
        return Err(map_failure(SyntheticExecutionFailure::Request));
    }
    terminal.finish(RequestBodyTerminalObservation::Completed);
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
        sequence: CAPTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
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

static CAPTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
            && !gate.poll_ready(cx.waker())
        {
            return Poll::Pending;
        }
        self.gate = None;
        loop {
            match self.steps.pop_front() {
                Some(ScriptedBodyStep::Chunk(bytes)) => {
                    return Poll::Ready(Some(Ok(Frame::data(bytes))));
                }
                Some(ScriptedBodyStep::Trailers(trailers)) => {
                    return Poll::Ready(Some(Ok(Frame::trailers(trailers))));
                }
                Some(ScriptedBodyStep::Gate(gate)) => {
                    if gate.poll_ready(cx.waker()) {
                        continue;
                    }
                    self.steps.push_front(ScriptedBodyStep::Gate(gate));
                    return Poll::Pending;
                }
                Some(ScriptedBodyStep::Failure) => {
                    self.terminal = true;
                    return Poll::Ready(Some(Err(ScriptedResponseBodyFailure)));
                }
                None => {
                    self.terminal = true;
                    return Poll::Ready(None);
                }
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.terminal || (self.gate.is_none() && self.steps.is_empty())
    }

    fn size_hint(&self) -> SizeHint {
        if self.terminal {
            return SizeHint::with_exact(0);
        }
        let length = self.steps.iter().fold(0_u64, |total, step| {
            total.saturating_add(match step {
                ScriptedBodyStep::Chunk(bytes) => bytes.len() as u64,
                ScriptedBodyStep::Trailers(_)
                | ScriptedBodyStep::Gate(_)
                | ScriptedBodyStep::Failure => 0,
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
    use std::future::poll_fn;
    use std::sync::atomic::AtomicBool;

    #[derive(Debug)]
    struct TestBodyFailure;

    impl fmt::Display for TestBodyFailure {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("test body failure")
        }
    }

    impl std::error::Error for TestBodyFailure {}

    struct FramesBody {
        frames: VecDeque<Result<Frame<Bytes>, TestBodyFailure>>,
        pending: Option<Arc<AtomicBool>>,
    }

    impl Body for FramesBody {
        type Data = Bytes;
        type Error = TestBodyFailure;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            if let Some(polled) = &self.pending {
                polled.store(true, Ordering::Release);
                return Poll::Pending;
            }
            Poll::Ready(self.frames.pop_front())
        }
    }

    fn execution_context() -> crate::transport::RequestExecutionContext {
        crate::transport::RequestExecutionContext {
            meta: crate::execution_meta::RequestExecutionMeta {
                endpoint: "BodyPhaseMatrix",
                method: http::Method::POST,
                idempotent: false,
                page_index: 0,
            },
            logical_url: "https://example.test/body".parse().expect("logical URL"),
            timeout: None,
            body_errors: Default::default(),
            auth_query_keys: Vec::new(),
            protected_header_names: Vec::new(),
        }
    }

    fn native_request(body: Option<reqwest::Body>) -> reqwest::Request {
        let mut request = reqwest::Request::new(
            http::Method::POST,
            "https://example.test/body".parse().expect("native URL"),
        );
        *request.body_mut() = body;
        request
    }

    fn observed_executor() -> (
        DeterministicNativeExecutor,
        Arc<Mutex<Vec<RequestBodyTerminalObservation>>>,
    ) {
        let executor = DeterministicNativeExecutor::application();
        let observations = Arc::new(Mutex::new(Vec::new()));
        let captured = observations.clone();
        executor.set_request_body_terminal_observer(move |observation| {
            lock(&captured).push(observation);
        });
        (executor, observations)
    }

    fn streaming_body(
        frames: impl IntoIterator<Item = Result<Frame<Bytes>, TestBodyFailure>>,
    ) -> reqwest::Body {
        reqwest::Body::wrap(FramesBody {
            frames: frames.into_iter().collect(),
            pending: None,
        })
    }

    #[derive(Clone, Copy, Debug)]
    enum GatePlacement {
        Initial,
        Ordered,
    }

    fn gated_response_body(
        placement: GatePlacement,
        gate: DeterministicBodyGate,
        chunk: Option<Bytes>,
    ) -> ScriptedResponseBody {
        let mut steps = Vec::new();
        let initial_gate = match placement {
            GatePlacement::Initial => Some(gate),
            GatePlacement::Ordered => {
                steps.push(ScriptedBodyStep::Gate(gate));
                None
            }
        };
        if let Some(chunk) = chunk {
            steps.push(ScriptedBodyStep::Chunk(chunk));
        }
        ScriptedResponseBody::new(steps, initial_gate)
    }

    async fn collect_gated_body(mut body: ScriptedResponseBody) -> Bytes {
        let mut collected = Vec::new();
        loop {
            match poll_fn(|cx| Pin::new(&mut body).poll_frame(cx)).await {
                Some(Ok(frame)) => {
                    let bytes = frame.into_data().expect("gate test only scripts data");
                    collected.extend_from_slice(&bytes);
                }
                Some(Err(error)) => panic!("unexpected scripted body failure: {error}"),
                None => break,
            }
        }
        assert!(
            poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                .await
                .is_none(),
            "terminal EOF must remain terminal"
        );
        Bytes::from(collected)
    }

    async fn bounded_body(body: ScriptedResponseBody) -> Bytes {
        tokio::time::timeout(Duration::from_secs(2), collect_gated_body(body))
            .await
            .expect("gated body completed within the bound")
    }

    fn poll_body_once(body: &mut ScriptedResponseBody) -> PollBodyResult {
        let mut context = Context::from_waker(Waker::noop());
        match Pin::new(body).poll_frame(&mut context) {
            Poll::Pending => PollBodyResult::Pending,
            Poll::Ready(None) => PollBodyResult::Eof,
            Poll::Ready(Some(Ok(frame))) => PollBodyResult::Data(
                frame
                    .into_data()
                    .expect("gate lifecycle test only scripts data"),
            ),
            Poll::Ready(Some(Err(error))) => panic!("unexpected body failure: {error}"),
        }
    }

    #[derive(Debug, Eq, PartialEq)]
    enum PollBodyResult {
        Pending,
        Data(Bytes),
        Eof,
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn deterministic_body_gate_release_timing_matrix_is_bounded() {
        const CHUNK: &[u8] = b"after-gate";

        for placement in [GatePlacement::Initial, GatePlacement::Ordered] {
            let gate = DeterministicBodyGate::new();
            gate.release();
            gate.release();
            assert_eq!(
                bounded_body(gated_response_body(
                    placement,
                    gate,
                    Some(Bytes::from_static(CHUNK)),
                ))
                .await,
                CHUNK,
                "release before first poll and repeated release: {placement:?}"
            );

            let gate = DeterministicBodyGate::new();
            let body =
                gated_response_body(placement, gate.clone(), Some(Bytes::from_static(CHUNK)));
            let consumer = tokio::spawn(collect_gated_body(body));
            tokio::time::timeout(Duration::from_secs(2), async {
                while !gate.is_entered() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("first poll entered the gate");
            assert!(!consumer.is_finished(), "unreleased gate must stay pending");
            gate.release();
            gate.release();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), consumer)
                    .await
                    .expect("release after pending woke the body")
                    .expect("body task succeeded"),
                CHUNK,
                "release after first pending poll: {placement:?}"
            );

            let gate = DeterministicBodyGate::new();
            let body =
                gated_response_body(placement, gate.clone(), Some(Bytes::from_static(CHUNK)));
            let consumer = tokio::spawn(collect_gated_body(body));
            let waiter_gate = gate.clone();
            assert!(
                tokio::task::spawn_blocking(move || {
                    waiter_gate.wait_until_entered(Duration::from_secs(2))
                })
                .await
                .expect("entered waiter joined"),
                "wait_until_entered observed the poll: {placement:?}"
            );
            gate.release();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), consumer)
                    .await
                    .expect("release after entered wait woke the body")
                    .expect("body task succeeded"),
                CHUNK,
                "release after wait_until_entered: {placement:?}"
            );

            let gate = DeterministicBodyGate::new();
            let body =
                gated_response_body(placement, gate.clone(), Some(Bytes::from_static(CHUNK)));
            let barrier = Arc::new(tokio::sync::Barrier::new(3));
            let consumer_barrier = barrier.clone();
            let consumer = tokio::spawn(async move {
                consumer_barrier.wait().await;
                collect_gated_body(body).await
            });
            let release_barrier = barrier.clone();
            let releaser = tokio::spawn(async move {
                release_barrier.wait().await;
                gate.release();
            });
            barrier.wait().await;
            releaser.await.expect("concurrent releaser succeeded");
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), consumer)
                    .await
                    .expect("concurrent release did not lose the wake")
                    .expect("body task succeeded"),
                CHUNK,
                "release concurrent with first-poll registration: {placement:?}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn deterministic_body_gate_release_registration_stress_has_no_lost_wake() {
        const ITERATIONS: usize = 500;
        const CHUNK: &[u8] = b"race";

        tokio::time::timeout(Duration::from_secs(20), async {
            for placement in [GatePlacement::Initial, GatePlacement::Ordered] {
                for _ in 0..ITERATIONS {
                    let executor = DeterministicNativeExecutor::application();
                    let gate = DeterministicBodyGate::new();
                    let response = match placement {
                        GatePlacement::Initial => ScriptedNativeResponse::body_steps(
                            StatusCode::OK,
                            [ScriptedResponseBodyStep::Chunk(Bytes::from_static(CHUNK))],
                        )
                        .with_gate(gate.clone()),
                        GatePlacement::Ordered => ScriptedNativeResponse::body_steps(
                            StatusCode::OK,
                            [
                                ScriptedResponseBodyStep::Gate(gate.clone()),
                                ScriptedResponseBodyStep::Chunk(Bytes::from_static(CHUNK)),
                            ],
                        ),
                    };
                    executor.script_response(response);
                    let response = executor
                        .execute_native(native_request(None), Some(&execution_context()))
                        .await
                        .expect("scripted execution succeeded");
                    assert_eq!(executor.remaining_scripts(), 0);

                    let barrier = Arc::new(tokio::sync::Barrier::new(3));
                    let consumer_barrier = barrier.clone();
                    let consumer = tokio::spawn(async move {
                        consumer_barrier.wait().await;
                        response.bytes().await
                    });
                    let release_barrier = barrier.clone();
                    let releaser = tokio::spawn(async move {
                        release_barrier.wait().await;
                        gate.release();
                        gate.release();
                    });
                    barrier.wait().await;
                    releaser.await.expect("stress releaser succeeded");
                    let bytes = consumer
                        .await
                        .expect("stress consumer joined")
                        .expect("stress response body succeeded");
                    assert_eq!(bytes, CHUNK, "one exact chunk: {placement:?}");
                    assert_eq!(executor.unexpected_execution_count(), 0);
                }
            }
        })
        .await
        .expect("release-registration stress completed within the bound");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn deterministic_body_gate_entered_wait_uses_synchronized_predicate() {
        let never_entered = DeterministicBodyGate::new();
        assert!(!never_entered.wait_until_entered(Duration::from_millis(1)));

        for placement in [GatePlacement::Initial, GatePlacement::Ordered] {
            let gate = DeterministicBodyGate::new();
            let waiter_gate = gate.clone();
            let waiter = tokio::task::spawn_blocking(move || {
                waiter_gate.wait_until_entered(Duration::from_secs(2))
            });
            let mut body = gated_response_body(placement, gate.clone(), None);
            assert_eq!(poll_body_once(&mut body), PollBodyResult::Pending);
            assert!(waiter.await.expect("entered waiter joined"));
            assert!(gate.wait_until_entered(Duration::ZERO));
            gate.release();
            assert_eq!(poll_body_once(&mut body), PollBodyResult::Eof);
        }
    }

    #[test]
    fn response_gate_end_stream_tracks_initial_and_ordered_gate_lifecycle() {
        for placement in [GatePlacement::Initial, GatePlacement::Ordered] {
            let gate = DeterministicBodyGate::new();
            let mut body = gated_response_body(placement, gate.clone(), None);
            assert!(!body.is_end_stream(), "unpolled gate: {placement:?}");
            assert_eq!(poll_body_once(&mut body), PollBodyResult::Pending);
            assert!(!body.is_end_stream(), "pending gate: {placement:?}");
            gate.release();
            assert!(
                !body.is_end_stream(),
                "released but unpolled gate: {placement:?}"
            );
            assert_eq!(poll_body_once(&mut body), PollBodyResult::Eof);
            assert!(body.is_end_stream(), "polled EOF: {placement:?}");
        }

        let gate = DeterministicBodyGate::new();
        let mut dropped = gated_response_body(GatePlacement::Ordered, gate.clone(), None);
        assert_eq!(poll_body_once(&mut dropped), PollBodyResult::Pending);
        drop(dropped);
        gate.release();
        gate.release();
    }

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

    #[tokio::test]
    async fn protected_credential_mismatch_is_generic_and_redacted() {
        const EXPECTED: &str = "F07_EXPECTED_CREDENTIAL_SENTINEL";
        const OBSERVED: &str = "F07_OBSERVED_CREDENTIAL_SENTINEL";
        let executor = DeterministicNativeExecutor::application();
        executor.script_response(
            ScriptedNativeResponse::bytes(StatusCode::OK, Bytes::new())
                .with_unsafe_credential_placement_expectations(
                    UnsafeCredentialPlacementExpectations::new().expect_header(
                        http::header::AUTHORIZATION,
                        DeterministicFakeCredential::new(EXPECTED),
                    ),
                ),
        );
        let mut request = native_request(None);
        request.headers_mut().insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static(OBSERVED),
        );
        let error = executor
            .execute_native(request, Some(&execution_context()))
            .await
            .expect_err("wrong protected credential must fail");
        for diagnostic in [
            format!("{error}"),
            format!("{error:?}"),
            format!("{executor:?}"),
        ] {
            assert!(!diagnostic.contains(EXPECTED), "{diagnostic}");
            assert!(!diagnostic.contains(OBSERVED), "{diagnostic}");
        }
        let capture = executor.captures().pop().expect("sanitized capture");
        let recorded = format!("{capture:?}");
        assert!(!recorded.contains(EXPECTED), "{recorded}");
        assert!(!recorded.contains(OBSERVED), "{recorded}");
    }

    #[test]
    fn synthetic_success_is_a_native_reqwest_response() {
        let response =
            ScriptedNativeResponse::chunks(StatusCode::ACCEPTED, [Bytes::from_static(b"native")])
                .into_native();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn request_body_terminal_phase_matrix_is_exact_and_single() {
        use RequestBodyTerminalObservation::{Completed, Failed, NeverPolled};

        let cases = [
            ("empty body success", native_request(None), None, Completed),
            (
                "buffered body EOF",
                native_request(Some(reqwest::Body::from(Bytes::from_static(b"buffered")))),
                None,
                Completed,
            ),
            (
                "streaming body EOF",
                native_request(Some(streaming_body([
                    Ok(Frame::data(Bytes::from_static(b"stream"))),
                    Ok(Frame::data(Bytes::from_static(b"ing"))),
                ]))),
                None,
                Completed,
            ),
            (
                "producer failure",
                native_request(Some(streaming_body([Err(TestBodyFailure)]))),
                None,
                Failed,
            ),
            (
                "exact-length underflow",
                native_request(Some(reqwest::Body::from(Bytes::from_static(b"abc")))),
                Some(UnsafeDeterministicFakeBody::new(Bytes::from_static(
                    b"abcde",
                ))),
                Failed,
            ),
            (
                "exact-length overflow",
                native_request(Some(reqwest::Body::from(Bytes::from_static(b"abcdef")))),
                Some(UnsafeDeterministicFakeBody::new(Bytes::from_static(
                    b"abcde",
                ))),
                Failed,
            ),
        ];

        for (label, request, expected, observation) in cases {
            let (executor, observations) = observed_executor();
            let mut response = ScriptedNativeResponse::bytes(StatusCode::OK, Bytes::new());
            if let Some(expected) = expected {
                response = response.with_unsafe_request_body_expectations(
                    UnsafeRequestBodyExpectations::new().expect_exact(expected),
                );
            }
            executor.script_response(response);
            let _ = executor
                .execute_native(request, Some(&execution_context()))
                .await;
            assert_eq!(*lock(&observations), [observation], "{label}");
        }

        for (label, failure) in [
            (
                "connect failure before polling",
                SyntheticExecutionFailure::Connect,
            ),
            (
                "timeout failure before polling",
                SyntheticExecutionFailure::Timeout,
            ),
        ] {
            let (executor, observations) = observed_executor();
            executor.script_failure(failure);
            let _ = executor
                .execute_native(
                    native_request(Some(reqwest::Body::from(Bytes::from_static(b"unpolled")))),
                    Some(&execution_context()),
                )
                .await;
            assert_eq!(*lock(&observations), [NeverPolled], "{label}");
        }

        let (executor, observations) = observed_executor();
        executor.script_failure_after_request_body(
            SyntheticExecutionFailure::Request,
            UnsafeCredentialPlacementExpectations::new(),
            UnsafeRequestBodyExpectations::new(),
        );
        let result = executor
            .execute_native(
                native_request(Some(reqwest::Body::from(Bytes::from_static(b"complete")))),
                Some(&execution_context()),
            )
            .await;
        assert!(result.is_err());
        assert_eq!(*lock(&observations), [Completed], "failure after full body");

        let (executor, observations) = observed_executor();
        let _ = executor
            .execute_native(native_request(None), Some(&execution_context()))
            .await;
        assert_eq!(*lock(&observations), [NeverPolled], "missing script");

        let (executor, observations) = observed_executor();
        executor.script_response(
            ScriptedNativeResponse::bytes(StatusCode::OK, Bytes::new())
                .with_unsafe_credential_placement_expectations(
                    UnsafeCredentialPlacementExpectations::new().expect_header(
                        http::header::AUTHORIZATION,
                        DeterministicFakeCredential::new("deterministic-fake"),
                    ),
                ),
        );
        let _ = executor
            .execute_native(native_request(None), Some(&execution_context()))
            .await;
        assert_eq!(
            *lock(&observations),
            [NeverPolled],
            "credential mismatch before polling"
        );
    }

    #[tokio::test]
    async fn pending_request_body_cancellation_is_not_completion() {
        let (executor, observations) = observed_executor();
        executor.script_response(ScriptedNativeResponse::bytes(StatusCode::OK, Bytes::new()));
        let polled = Arc::new(AtomicBool::new(false));
        let request = native_request(Some(reqwest::Body::wrap(FramesBody {
            frames: VecDeque::new(),
            pending: Some(polled.clone()),
        })));
        let task = tokio::spawn(async move {
            executor
                .execute_native(request, Some(&execution_context()))
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            while !polled.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("pending body was polled");
        task.abort();
        assert!(task.await.expect_err("task cancellation").is_cancelled());
        assert_eq!(
            *lock(&observations),
            [RequestBodyTerminalObservation::CancelledOrDropped]
        );
    }
}
