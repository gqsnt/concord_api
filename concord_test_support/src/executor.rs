use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::gate::Gate;
use crate::script::{
    BoundaryRequest, CapturedRequest, Channel, ExecutionFailure, ExecutionFailureKind,
    ExpectedRequest, Outcome, Script, ScriptedResponse, TransportFailure,
};

macro_rules! channel_methods {
    () => {
        pub fn queue_response(&self, response: ScriptedResponse) -> Result<(), ExecutionFailure> {
            self.0.queue(Script::Exchange {
                expectation: None,
                outcome: Outcome::Response(response),
            })
        }

        pub fn queue_failure(&self, failure: TransportFailure) -> Result<(), ExecutionFailure> {
            self.0.queue(Script::Exchange {
                expectation: None,
                outcome: Outcome::Failure(failure),
            })
        }

        pub fn queue_expected_response(
            &self,
            expectation: ExpectedRequest,
            response: ScriptedResponse,
        ) -> Result<(), ExecutionFailure> {
            self.0.queue(Script::Exchange {
                expectation: Some(expectation),
                outcome: Outcome::Response(response),
            })
        }

        pub fn queue_expected_failure(
            &self,
            expectation: ExpectedRequest,
            failure: TransportFailure,
        ) -> Result<(), ExecutionFailure> {
            self.0.queue(Script::Exchange {
                expectation: Some(expectation),
                outcome: Outcome::Failure(failure),
            })
        }

        pub fn queue_gate(&self, gate: Gate) -> Result<(), ExecutionFailure> {
            self.0.queue(Script::Gate(gate))
        }

        pub fn queue_cancellation_point(&self, gate: Gate) -> Result<(), ExecutionFailure> {
            self.0.queue(Script::CancellationPoint(gate))
        }

        pub fn pause_before_execution(&self, gate: Gate) -> Result<(), ExecutionFailure> {
            self.0.pause_before_execution(gate)
        }

        pub fn clear_before_execution(&self) -> Result<(), ExecutionFailure> {
            self.0.clear_before_execution()
        }

        pub fn captures(&self) -> Result<Vec<CapturedRequest>, ExecutionFailure> {
            self.0.captures()
        }

        pub fn clear_captures(&self) -> Result<(), ExecutionFailure> {
            self.0.clear_captures()
        }

        pub fn request_count(&self) -> Result<usize, ExecutionFailure> {
            self.0.request_count()
        }

        pub fn remaining_scripts(&self) -> Result<usize, ExecutionFailure> {
            self.0.remaining_scripts()
        }
    };
}

/// A scoped deterministic executor with isolated application/provider state.
#[derive(Clone)]
pub struct DeterministicExecutor {
    application: ApplicationChannel,
    provider: ProviderChannel,
}

impl DeterministicExecutor {
    #[must_use]
    pub fn new() -> Self {
        Self {
            application: ApplicationChannel::new(),
            provider: ProviderChannel::new(),
        }
    }

    #[must_use]
    pub fn application(&self) -> ApplicationChannel {
        self.application.clone()
    }

    #[must_use]
    pub fn provider(&self) -> ProviderChannel {
        self.provider.clone()
    }
}

impl Default for DeterministicExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for DeterministicExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeterministicExecutor")
            .field("application", &self.application)
            .field("provider", &self.provider)
            .finish()
    }
}

/// The independently scripted application channel.
#[derive(Clone)]
pub struct ApplicationChannel(ChannelHandle);

impl ApplicationChannel {
    fn new() -> Self {
        Self(ChannelHandle::new(Channel::Application))
    }

    channel_methods!();

    #[doc(hidden)]
    pub async fn __execute(
        &self,
        request: BoundaryRequest,
    ) -> Result<ScriptedResponse, ExecutionFailure> {
        self.0.execute(request).await
    }
}

impl fmt::Debug for ApplicationChannel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt_named("ApplicationChannel", formatter)
    }
}

/// The independently scripted provider channel.
#[derive(Clone)]
pub struct ProviderChannel(ChannelHandle);

impl ProviderChannel {
    fn new() -> Self {
        Self(ChannelHandle::new(Channel::Provider))
    }

    channel_methods!();

    #[doc(hidden)]
    pub async fn __execute(
        &self,
        request: BoundaryRequest,
    ) -> Result<ScriptedResponse, ExecutionFailure> {
        self.0.execute(request).await
    }
}

impl fmt::Debug for ProviderChannel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt_named("ProviderChannel", formatter)
    }
}

#[derive(Clone)]
struct ChannelHandle {
    state: Arc<ChannelState>,
}

struct ChannelState {
    channel: Channel,
    scripts: Mutex<VecDeque<Script>>,
    captures: Mutex<Vec<CapturedRequest>>,
    before_execution: Mutex<Option<Gate>>,
}

impl ChannelHandle {
    fn new(channel: Channel) -> Self {
        Self {
            state: Arc::new(ChannelState {
                channel,
                scripts: Mutex::new(VecDeque::new()),
                captures: Mutex::new(Vec::new()),
                before_execution: Mutex::new(None),
            }),
        }
    }

    fn queue(&self, script: Script) -> Result<(), ExecutionFailure> {
        lock(&self.state.scripts)?.push_back(script);
        Ok(())
    }

    fn pause_before_execution(&self, gate: Gate) -> Result<(), ExecutionFailure> {
        *lock(&self.state.before_execution)? = Some(gate);
        Ok(())
    }

    fn clear_before_execution(&self) -> Result<(), ExecutionFailure> {
        *lock(&self.state.before_execution)? = None;
        Ok(())
    }

    fn captures(&self) -> Result<Vec<CapturedRequest>, ExecutionFailure> {
        Ok(lock(&self.state.captures)?.clone())
    }

    fn clear_captures(&self) -> Result<(), ExecutionFailure> {
        lock(&self.state.captures)?.clear();
        Ok(())
    }

    fn request_count(&self) -> Result<usize, ExecutionFailure> {
        Ok(lock(&self.state.captures)?.len())
    }

    fn remaining_scripts(&self) -> Result<usize, ExecutionFailure> {
        Ok(lock(&self.state.scripts)?.len())
    }

    async fn execute(
        &self,
        request: BoundaryRequest,
    ) -> Result<ScriptedResponse, ExecutionFailure> {
        let before_execution = lock(&self.state.before_execution)?.clone();
        if let Some(gate) = before_execution {
            gate.wait().await;
        }

        let (captured, boundary_expectation) = request.into_capture(self.state.channel);
        lock(&self.state.captures)?.push(captured.clone());

        loop {
            let reservation = ScriptReservation::reserve(Arc::clone(&self.state))?
                .ok_or_else(|| ExecutionFailure::new(ExecutionFailureKind::EmptyQueue))?;
            if let Some(gate) = reservation.delivery_gate()? {
                gate.wait().await;
            }
            let script = reservation.commit()?;
            match script {
                Script::Gate(_) => continue,
                Script::CancellationPoint(_) => {
                    return Err(ExecutionFailure::new(
                        ExecutionFailureKind::CancellationPoint,
                    ));
                }
                Script::Exchange {
                    expectation,
                    outcome,
                } => {
                    if expectation.as_ref().is_some_and(|expectation| {
                        !expectation.matches(&captured, &boundary_expectation)
                    }) {
                        return Err(ExecutionFailure::new(ExecutionFailureKind::Mismatch));
                    }
                    return match outcome {
                        Outcome::Response(response) => Ok(response),
                        Outcome::Failure(failure) => Err(ExecutionFailure::new(
                            ExecutionFailureKind::Scripted(failure),
                        )),
                    };
                }
            }
        }
    }

    fn fmt_named(&self, name: &str, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct(name)
            .field("channel", &self.state.channel)
            .finish_non_exhaustive()
    }
}

struct ScriptReservation {
    state: Arc<ChannelState>,
    script: Option<Script>,
}

impl ScriptReservation {
    fn reserve(state: Arc<ChannelState>) -> Result<Option<Self>, ExecutionFailure> {
        let script = lock(&state.scripts)?.pop_front();
        Ok(script.map(|script| Self {
            state,
            script: Some(script),
        }))
    }

    fn delivery_gate(&self) -> Result<Option<Gate>, ExecutionFailure> {
        self.script
            .as_ref()
            .map(Script::delivery_gate)
            .ok_or_else(|| ExecutionFailure::new(ExecutionFailureKind::Internal))
    }

    fn commit(mut self) -> Result<Script, ExecutionFailure> {
        self.script
            .take()
            .ok_or_else(|| ExecutionFailure::new(ExecutionFailureKind::Internal))
    }
}

impl Drop for ScriptReservation {
    fn drop(&mut self) {
        let Some(script) = self.script.take() else {
            return;
        };
        if let Ok(mut scripts) = self.state.scripts.lock() {
            scripts.push_front(script);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, ExecutionFailure> {
    mutex
        .lock()
        .map_err(|_| ExecutionFailure::new(ExecutionFailureKind::Internal))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;
    use http::{Method, StatusCode};

    use super::*;
    use crate::script::{BodyCategory, BoundaryRequest};

    fn request(body: Option<Bytes>) -> BoundaryRequest {
        BoundaryRequest::__new(
            "test.endpoint",
            Method::GET,
            Some("https://safe.test/resource".to_owned()),
            Vec::new(),
            BodyCategory::Bytes,
            body.as_ref().map(Bytes::len),
            body,
        )
    }

    #[tokio::test]
    async fn cancellation_requeues_reserved_script_without_touching_other_channel() {
        let executor = DeterministicExecutor::new();
        let application = executor.application();
        let provider = executor.provider();
        let gate = Gate::closed();
        application
            .queue_response(
                ScriptedResponse::status(StatusCode::CREATED, Bytes::new())
                    .delivery_gate(gate.clone()),
            )
            .expect("application script should queue");
        provider
            .queue_response(ScriptedResponse::success(Bytes::new()))
            .expect("provider script should queue");

        let cancelled = tokio::time::timeout(
            Duration::from_millis(30),
            application.__execute(request(None)),
        )
        .await;
        assert!(cancelled.is_err());
        assert_eq!(application.remaining_scripts().expect("count"), 1);
        assert_eq!(provider.remaining_scripts().expect("count"), 1);

        gate.open();
        let response = application
            .__execute(request(None))
            .await
            .expect("requeued response should arrive");
        assert_eq!(
            response.__into_http_response().status(),
            StatusCode::CREATED
        );
    }

    #[tokio::test]
    async fn mismatch_and_script_debug_never_reproduce_fake_values() {
        const EXPECTED: &str = "expected-body-secret-9e1c";
        const ACTUAL: &str = "actual-body-secret-52aa";
        const RESPONSE: &str = "response-body-secret-77fb";
        const NATIVE: &str = "native-auth-secret-4f21";
        let application = DeterministicExecutor::new().application();
        let response = ScriptedResponse::success(Bytes::from_static(RESPONSE.as_bytes()));
        assert!(!format!("{response:?}").contains(RESPONSE));
        application
            .queue_expected_response(
                ExpectedRequest::new()
                    .body(Bytes::from_static(EXPECTED.as_bytes()))
                    .native_url(format!("https://native.invalid/?token={NATIVE}"))
                    .header(
                        http::header::AUTHORIZATION,
                        http::HeaderValue::from_str(NATIVE).expect("header"),
                    ),
                response,
            )
            .expect("expected response should queue");
        let error = application
            .__execute(
                request(Some(Bytes::from_static(ACTUAL.as_bytes()))).__with_native(
                    "https://native.invalid/?token=wrong",
                    &http::HeaderMap::new(),
                ),
            )
            .await
            .expect_err("body mismatch should fail safely");
        assert_eq!(error.kind(), ExecutionFailureKind::Mismatch);
        for formatted in [error.to_string(), format!("{error:?}")] {
            assert!(!formatted.contains(EXPECTED));
            assert!(!formatted.contains(ACTUAL));
            assert!(!formatted.contains(RESPONSE));
            assert!(!formatted.contains(NATIVE));
        }
    }
}
