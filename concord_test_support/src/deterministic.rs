#![allow(dead_code)]

use bytes::Bytes;
use concord_core::__development::{
    CapturedBodyCategory, CapturedNativeRequest, DeterministicBodyGate, DeterministicExecutionKind,
    DeterministicFakeCredential, DeterministicNativeExecutor, RequestBodyTerminalObservation,
    ScriptedNativeResponse, ScriptedResponseBodyStep, SyntheticExecutionFailure,
    UnsafeCredentialPlacementExpectations, UnsafeDeterministicFakeBody,
    UnsafeRequestBodyExpectations, configure_application_executor, configure_provider_executor,
};
use concord_core::advanced::SafeReqwestBuilder;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// A sanitized record of one deterministic native execution.
#[derive(Clone)]
pub struct RecordedExecution {
    sequence: u64,
    pub method: Method,
    pub logical_url: url::Url,
    pub headers: HeaderMap,
    pub protected_header_names: Vec<HeaderName>,
    pub body_category: CapturedBodyCategory,
    pub known_body_length: Option<u64>,
    pub endpoint: Option<String>,
    pub page_index: Option<u32>,
    pub timeout: Option<Duration>,
}

impl RecordedExecution {
    pub fn body_present(&self) -> bool {
        self.body_category != CapturedBodyCategory::Empty
    }
}

impl fmt::Debug for RecordedExecution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecordedExecution")
            .field("method", &self.method)
            .field("logical_url", &self.logical_url)
            .field(
                "headers",
                &concord_core::advanced::SanitizedHeaders::new(&self.headers),
            )
            .field("protected_header_names", &self.protected_header_names)
            .field("body_category", &self.body_category)
            .field("known_body_length", &self.known_body_length)
            .field("endpoint", &self.endpoint)
            .field("page_index", &self.page_index)
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[derive(Clone)]
pub enum ScriptedResponseStep {
    Chunk(Bytes),
    Gate(ResponseGate),
    Failure,
}

impl fmt::Debug for ScriptedResponseStep {
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

#[derive(Clone, Default)]
pub struct ResponseGate(DeterministicBodyGate);

impl ResponseGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn wait_until_entered(&self, timeout: Duration) {
        assert!(
            self.0.wait_until_entered(timeout),
            "deterministic response gate was not entered"
        );
    }

    pub fn release(&self) {
        self.0.release();
    }
}

impl fmt::Debug for ResponseGate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone)]
pub struct ScriptedReply {
    execution_kind: DeterministicExecutionKind,
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
    response_steps: Option<Vec<ScriptedResponseStep>>,
    gate: Option<ResponseGate>,
    failure: Option<(SyntheticExecutionFailure, RequestBodyFailurePhase)>,
    expected_credentials: UnsafeCredentialPlacementExpectations,
    expected_body: UnsafeRequestBodyExpectations,
}

#[derive(Clone, Copy, Debug)]
enum RequestBodyFailurePhase {
    BeforePolling,
    AfterCompletion,
}

impl ScriptedReply {
    pub fn ok_json(body: Bytes) -> Self {
        Self::status(StatusCode::OK)
            .with_header(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )
            .with_body(body)
    }

    pub fn ok_text(body: Bytes) -> Self {
        Self::status(StatusCode::OK)
            .with_header(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain"),
            )
            .with_body(body)
    }

    pub fn status(status: StatusCode) -> Self {
        Self {
            execution_kind: DeterministicExecutionKind::Application,
            status,
            headers: HeaderMap::new(),
            body: Bytes::new(),
            response_steps: None,
            gate: None,
            failure: None,
            expected_credentials: UnsafeCredentialPlacementExpectations::new(),
            expected_body: UnsafeRequestBodyExpectations::new(),
        }
    }

    pub fn provider(mut self) -> Self {
        self.execution_kind = DeterministicExecutionKind::Provider;
        self
    }

    pub fn failure_before_request_body(failure: SyntheticExecutionFailure) -> Self {
        Self {
            failure: Some((failure, RequestBodyFailurePhase::BeforePolling)),
            ..Self::status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }

    pub fn failure_after_request_body(failure: SyntheticExecutionFailure) -> Self {
        Self {
            failure: Some((failure, RequestBodyFailurePhase::AfterCompletion)),
            ..Self::status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }

    pub fn timeout_before_request_body() -> Self {
        Self::failure_before_request_body(SyntheticExecutionFailure::Timeout)
    }

    pub fn disconnect_after_request_body() -> Self {
        Self::failure_after_request_body(SyntheticExecutionFailure::Request)
    }

    pub fn with_header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.append(name, value);
        self
    }

    pub fn with_body(mut self, body: Bytes) -> Self {
        self.body = body;
        self
    }

    pub fn with_chunks(mut self, chunks: impl IntoIterator<Item = Bytes>) -> Self {
        self.response_steps = Some(
            chunks
                .into_iter()
                .map(ScriptedResponseStep::Chunk)
                .collect(),
        );
        self
    }

    pub fn with_response_steps(
        mut self,
        steps: impl IntoIterator<Item = ScriptedResponseStep>,
    ) -> Self {
        self.response_steps = Some(steps.into_iter().collect());
        self
    }

    pub fn with_gate(mut self, gate: ResponseGate) -> Self {
        self.gate = Some(gate);
        self
    }

    pub fn expect_query_pair(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.expected_credentials = self
            .expected_credentials
            .expect_query_pair(name, DeterministicFakeCredential::new(value.into()));
        self
    }

    pub fn expect_header(mut self, name: HeaderName, value: impl Into<String>) -> Self {
        self.expected_credentials = self
            .expected_credentials
            .expect_header(name, DeterministicFakeCredential::new(value.into()));
        self
    }

    pub fn expect_body(mut self, value: impl Into<Bytes>) -> Self {
        self.expected_body = UnsafeRequestBodyExpectations::new()
            .expect_exact(UnsafeDeterministicFakeBody::new(value));
        self
    }

    fn response(&self) -> ScriptedNativeResponse {
        let mut response = if let Some(steps) = &self.response_steps {
            ScriptedNativeResponse::body_steps(
                self.status,
                steps.iter().cloned().map(|step| match step {
                    ScriptedResponseStep::Chunk(bytes) => ScriptedResponseBodyStep::Chunk(bytes),
                    ScriptedResponseStep::Gate(gate) => ScriptedResponseBodyStep::Gate(gate.0),
                    ScriptedResponseStep::Failure => ScriptedResponseBodyStep::Failure,
                }),
            )
        } else {
            ScriptedNativeResponse::bytes(self.status, self.body.clone())
        };
        for (name, value) in &self.headers {
            response = response.with_header(name.clone(), value.clone());
        }
        if self.response_steps.is_none() && !self.headers.contains_key(http::header::CONTENT_LENGTH)
        {
            response = response.with_header(
                http::header::CONTENT_LENGTH,
                HeaderValue::from_str(&self.body.len().to_string()).expect("response length"),
            );
        }
        if let Some(gate) = &self.gate {
            response = response.with_gate(gate.0.clone());
        }
        response
            .with_unsafe_credential_placement_expectations(self.expected_credentials.clone())
            .with_unsafe_request_body_expectations(self.expected_body.clone())
    }

    fn install(&self, executor: &DeterministicNativeExecutor, repeating: bool) {
        if let Some((failure, phase)) = self.failure {
            match (repeating, phase) {
                (true, RequestBodyFailurePhase::BeforePolling) => {
                    executor.script_repeating_failure(failure);
                }
                (true, RequestBodyFailurePhase::AfterCompletion) => {
                    executor.script_repeating_failure_after_request_body(failure);
                }
                (false, RequestBodyFailurePhase::BeforePolling) => {
                    executor.script_failure(failure);
                }
                (false, RequestBodyFailurePhase::AfterCompletion) => {
                    executor.script_failure_after_request_body(
                        failure,
                        self.expected_credentials.clone(),
                        self.expected_body.clone(),
                    );
                }
            }
        } else if repeating {
            executor.script_repeating_response(self.response());
        } else {
            executor.script_response(self.response());
        }
    }
}

impl fmt::Debug for ScriptedReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScriptedReply")
            .field("status", &self.status)
            .field(
                "headers",
                &concord_core::advanced::SanitizedHeaders::new(&self.headers),
            )
            .field(
                "body",
                &format_args!("<{} response bytes>", self.body.len()),
            )
            .field("response_steps", &self.response_steps)
            .field("gate", &self.gate)
            .field("failure", &self.failure)
            .field("expected_credentials", &self.expected_credentials)
            .field("expected_body", &self.expected_body)
            .finish()
    }
}

#[derive(Clone)]
pub struct DeterministicMock {
    application: DeterministicNativeExecutor,
    provider: DeterministicNativeExecutor,
    gates: Arc<Vec<ResponseGate>>,
}

impl DeterministicMock {
    pub fn configure_application(&self, builder: SafeReqwestBuilder) -> SafeReqwestBuilder {
        configure_application_executor(builder, self.application.clone())
            .expect("application deterministic executor configuration")
    }

    pub fn configure_provider(&self, builder: SafeReqwestBuilder) -> SafeReqwestBuilder {
        configure_provider_executor(builder, self.provider.clone())
            .expect("provider deterministic executor configuration")
    }

    pub fn configure_both(&self, builder: SafeReqwestBuilder) -> SafeReqwestBuilder {
        self.configure_provider(self.configure_application(builder))
    }
}

impl Drop for DeterministicMock {
    fn drop(&mut self) {
        if Arc::strong_count(&self.gates) == 1 {
            for gate in self.gates.iter() {
                gate.release();
            }
        }
    }
}

pub struct MockExecutionHandle {
    application: DeterministicNativeExecutor,
    provider: DeterministicNativeExecutor,
    completed: Arc<AtomicUsize>,
    body_observations: Arc<std::sync::Mutex<Vec<RequestBodyTerminalObservation>>>,
    finished: bool,
}

impl MockExecutionHandle {
    pub fn recorded(&self) -> Vec<RecordedExecution> {
        let mut captures = self.application.captures();
        captures.extend(self.provider.captures());
        captures.sort_unstable_by_key(CapturedNativeRequest::sequence);
        captures.into_iter().map(recorded_execution).collect()
    }

    pub fn recorded_len(&self) -> usize {
        self.application.captures().len() + self.provider.captures().len()
    }

    pub fn completed_len(&self) -> usize {
        self.completed.load(Ordering::Acquire)
    }

    pub fn body_observations(&self) -> Vec<RequestBodyTerminalObservation> {
        self.body_observations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn assert_recorded_len(&self, expected: usize) {
        assert_eq!(self.recorded_len(), expected, "recorded execution count");
    }

    pub fn finish(mut self) {
        self.assert_clean();
        self.finished = true;
    }

    fn assert_clean(&self) {
        assert_eq!(
            self.application.remaining_scripts() + self.provider.remaining_scripts(),
            0,
            "deterministic scripted replies remain unused"
        );
        assert_eq!(
            self.application.unexpected_execution_count()
                + self.provider.unexpected_execution_count(),
            0,
            "unexpected deterministic executions exceeded scripted replies"
        );
    }
}

impl Drop for MockExecutionHandle {
    fn drop(&mut self) {
        if !self.finished && !std::thread::panicking() {
            self.assert_clean();
        }
    }
}

type RequestObserver = Arc<dyn Fn() + Send + Sync>;
type RequestBodyObserver = Arc<dyn Fn(RequestBodyTerminalObservation) + Send + Sync>;

#[derive(Default)]
pub struct DeterministicMockBuilder {
    replies: Vec<ScriptedReply>,
    repeating: Option<ScriptedReply>,
    kind: Option<DeterministicExecutionKind>,
    request_head_observer: Option<RequestObserver>,
    request_body_complete_observer: Option<RequestObserver>,
    request_body_terminal_observer: Option<RequestBodyObserver>,
}

impl DeterministicMockBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn provider(mut self) -> Self {
        self.kind = Some(DeterministicExecutionKind::Provider);
        self
    }

    pub fn reply(mut self, reply: ScriptedReply) -> Self {
        self.replies.push(reply);
        self
    }

    pub fn replies(mut self, replies: impl IntoIterator<Item = ScriptedReply>) -> Self {
        self.replies.extend(replies);
        self
    }

    pub fn repeating(mut self, reply: ScriptedReply) -> Self {
        self.repeating = Some(reply);
        self
    }

    pub fn on_request_head(mut self, observer: impl Fn() + Send + Sync + 'static) -> Self {
        self.request_head_observer = Some(Arc::new(observer));
        self
    }

    pub fn on_request_body_complete(mut self, observer: impl Fn() + Send + Sync + 'static) -> Self {
        self.request_body_complete_observer = Some(Arc::new(observer));
        self
    }

    pub fn on_request_body_terminal(
        mut self,
        observer: impl Fn(RequestBodyTerminalObservation) + Send + Sync + 'static,
    ) -> Self {
        self.request_body_terminal_observer = Some(Arc::new(observer));
        self
    }

    pub fn build(self) -> (DeterministicMock, MockExecutionHandle) {
        let application = DeterministicNativeExecutor::application();
        let provider = DeterministicNativeExecutor::provider();
        let default_kind = self.kind.unwrap_or(DeterministicExecutionKind::Application);
        for reply in &self.replies {
            let executor = match if self.kind.is_some() {
                default_kind
            } else {
                reply.execution_kind
            } {
                DeterministicExecutionKind::Application => &application,
                DeterministicExecutionKind::Provider => &provider,
            };
            reply.install(executor, false);
        }
        if let Some(repeating) = &self.repeating {
            let executor = match default_kind {
                DeterministicExecutionKind::Application => &application,
                DeterministicExecutionKind::Provider => &provider,
            };
            repeating.install(executor, true);
        }
        if let Some(observer) = self.request_head_observer {
            let provider_observer = observer.clone();
            application.set_request_head_observer(move || observer());
            provider.set_request_head_observer(move || provider_observer());
        }
        let completed = Arc::new(AtomicUsize::new(0));
        let body_observations = Arc::new(std::sync::Mutex::new(Vec::new()));
        let completed_observer = completed.clone();
        let observer = self.request_body_complete_observer;
        let terminal_observer = self.request_body_terminal_observer;
        let provider_completed = completed_observer.clone();
        let provider_observer = observer.clone();
        let application_observations = body_observations.clone();
        let provider_observations = body_observations.clone();
        let provider_terminal_observer = terminal_observer.clone();
        application.set_request_body_terminal_observer(move |observation| {
            application_observations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(observation);
            if observation == RequestBodyTerminalObservation::Completed {
                completed_observer.fetch_add(1, Ordering::Release);
                if let Some(observer) = &observer {
                    observer();
                }
            }
            if let Some(observer) = &terminal_observer {
                observer(observation);
            }
        });
        provider.set_request_body_terminal_observer(move |observation| {
            provider_observations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(observation);
            if observation == RequestBodyTerminalObservation::Completed {
                provider_completed.fetch_add(1, Ordering::Release);
                if let Some(observer) = &provider_observer {
                    observer();
                }
            }
            if let Some(observer) = &provider_terminal_observer {
                observer(observation);
            }
        });
        let gates = self
            .replies
            .iter()
            .chain(self.repeating.iter())
            .flat_map(reply_gates)
            .collect::<Vec<_>>();
        (
            DeterministicMock {
                application: application.clone(),
                provider: provider.clone(),
                gates: Arc::new(gates),
            },
            MockExecutionHandle {
                application,
                provider,
                completed,
                body_observations,
                finished: false,
            },
        )
    }
}

pub fn deterministic_mock() -> DeterministicMockBuilder {
    DeterministicMockBuilder::new()
}

fn reply_gates(reply: &ScriptedReply) -> Vec<ResponseGate> {
    reply
        .gate
        .iter()
        .cloned()
        .chain(
            reply
                .response_steps
                .iter()
                .flatten()
                .filter_map(|step| match step {
                    ScriptedResponseStep::Gate(gate) => Some(gate.clone()),
                    ScriptedResponseStep::Chunk(_) | ScriptedResponseStep::Failure => None,
                }),
        )
        .collect()
}

fn recorded_execution(request: CapturedNativeRequest) -> RecordedExecution {
    RecordedExecution {
        sequence: request.sequence(),
        method: request.method().clone(),
        logical_url: request.logical_target().clone(),
        headers: request.public_headers().clone(),
        protected_header_names: request.protected_header_names().to_vec(),
        body_category: request.body_category(),
        known_body_length: request.known_body_length(),
        endpoint: Some(request.endpoint().to_string()),
        page_index: Some(request.page_index()),
        timeout: request.timeout(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "deterministic scripted replies remain unused")]
    fn finish_rejects_unused_scripted_replies() {
        let (_mock, handle) = deterministic_mock()
            .reply(ScriptedReply::status(StatusCode::OK))
            .build();
        handle.finish();
    }

    #[test]
    fn unsafe_expectation_diagnostics_are_redacted() {
        const SENTINEL: &str = "DETERMINISTIC_SUPPORT_UNSAFE_SENTINEL";
        let reply = ScriptedReply::status(StatusCode::OK)
            .expect_query_pair("fake", SENTINEL)
            .expect_body(Bytes::from_static(SENTINEL.as_bytes()));
        let rendered = format!("{reply:?}");
        assert!(!rendered.contains(SENTINEL), "{rendered}");
    }
}
