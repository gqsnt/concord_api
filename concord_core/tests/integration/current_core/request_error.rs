use super::common::{
    MockOutcome, MockTransport, NativeMockReply, NativeReplyGate, ObservationAuthVars,
    TestAuthVars, client, observation_client, request_plan,
};
use bytes::Bytes;
use concord_core::advanced::{
    AdvancedRequestBody, PreparedBody, PreparedEndpoint, PreparedRequestEntity,
    RequestErrorHookContext, RuntimeHooks,
};
use concord_core::prelude::{ApiClientError, ErrorCategory, RetryMode, Text};
use http::Method;
use http_body::{Body, Frame};
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

#[derive(Default)]
struct CategoryHooks {
    observed: Mutex<Vec<ErrorCategory>>,
}

impl RuntimeHooks for CategoryHooks {
    fn request_error<'a>(
        &'a self,
        ctx: RequestErrorHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        self.observed
            .lock()
            .expect("hook categories")
            .push(ctx.category);
        Box::pin(async {})
    }
}

fn assert_aligned(error: &ApiClientError, hooks: &CategoryHooks) {
    assert_eq!(
        hooks.observed.lock().expect("hook categories").as_slice(),
        &[error.category()]
    );
}

#[tokio::test]
async fn ordinary_request_execution_hook_matches_terminal_error() {
    let transport = MockTransport::with_outcomes(
        Arc::new(tokio::sync::Mutex::new(Vec::new())),
        vec![MockOutcome::DisconnectAfterRequest],
    );
    let mut client = client(TestAuthVars::default(), transport);
    let hooks = Arc::new(CategoryHooks::default());
    client.set_runtime_hooks(hooks.clone());

    let error = client
        .execute_plan::<Text<String>>(request_plan(
            "RequestErrorExecution",
            Method::GET,
            "/request-error",
            Default::default(),
            None,
        ))
        .await
        .expect_err("disconnect is terminal");
    assert_eq!(error.category(), ErrorCategory::RequestExecution);
    assert_aligned(&error, &hooks);
}

#[tokio::test]
async fn timeout_hook_matches_terminal_error() {
    let gate = NativeReplyGate::new();
    let transport = MockTransport::from_native_replies(
        Arc::new(tokio::sync::Mutex::new(Vec::new())),
        [NativeMockReply::ok_text(Bytes::from_static(b"late")).with_gate(gate.clone())],
    );
    let mut client = client(TestAuthVars::default(), transport);
    let hooks = Arc::new(CategoryHooks::default());
    client.set_runtime_hooks(hooks.clone());
    let mut plan = request_plan(
        "RequestErrorTimeout",
        Method::GET,
        "/timeout",
        Default::default(),
        None,
    );
    plan.overrides.timeout = Some(Duration::from_millis(20));

    let error = client
        .execute_plan::<Text<String>>(plan)
        .await
        .expect_err("request timeout is terminal");
    gate.release();
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_aligned(&error, &hooks);
}

#[tokio::test]
async fn connect_hook_matches_terminal_error() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
    let authority = listener.local_addr().expect("loopback address").to_string();
    drop(listener);
    let mut client = concord_core::prelude::ApiClient::<super::common::TestCx>::with_retry_mode(
        (),
        TestAuthVars::default(),
        RetryMode::Disabled,
    )
    .expect("managed client");
    let hooks = Arc::new(CategoryHooks::default());
    client.set_runtime_hooks(hooks.clone());
    let mut plan = request_plan(
        "RequestErrorConnect",
        Method::GET,
        "/connect",
        Default::default(),
        None,
    );
    plan.endpoint.route.scheme = http::uri::Scheme::HTTP;
    plan.endpoint.route.host = authority;
    let error = client
        .execute_plan::<Text<String>>(plan)
        .await
        .expect_err("connection refusal is terminal");
    assert_eq!(error.category(), ErrorCategory::Connect);
    assert_aligned(&error, &hooks);
}

#[derive(Default)]
struct ProducerGate {
    released: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl ProducerGate {
    fn release(&self) {
        self.released.store(true, Ordering::Release);
        if let Some(waker) = self.waker.lock().expect("producer gate").take() {
            waker.wake();
        }
    }
}

struct ProducerFailure(Arc<ProducerGate>);

impl Body for ProducerFailure {
    type Data = Bytes;
    type Error = ProducerSentinel;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        if !self.0.released.load(Ordering::Acquire) {
            *self.0.waker.lock().expect("producer gate") = Some(cx.waker().clone());
            return Poll::Pending;
        }
        Poll::Ready(Some(Err(ProducerSentinel)))
    }

    fn is_end_stream(&self) -> bool {
        false
    }

    fn size_hint(&self) -> http_body::SizeHint {
        http_body::SizeHint::with_exact(1)
    }
}

struct ProducerSentinel;

impl std::fmt::Display for ProducerSentinel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PRODUCER_ERROR_SENTINEL")
    }
}

impl std::fmt::Debug for ProducerSentinel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PRODUCER_ERROR_SENTINEL")
    }
}

impl std::error::Error for ProducerSentinel {}

#[tokio::test]
async fn body_producer_hook_matches_terminal_error_and_chain_is_sanitized() {
    let producer_gate = Arc::new(ProducerGate::default());
    let transport = MockTransport::from_native_replies_with_head_action(
        Arc::new(tokio::sync::Mutex::new(Vec::new())),
        [NativeMockReply::disconnect_after_request().expect_request_body_failure()],
        {
            let producer_gate = producer_gate.clone();
            move || producer_gate.release()
        },
    );
    let mut client = observation_client(
        ObservationAuthVars::bearer(
            "unused",
            "body",
            Arc::new(tokio::sync::Mutex::new(Vec::new())),
        ),
        &transport,
    );
    let hooks = Arc::new(CategoryHooks::default());
    client.set_runtime_hooks(hooks.clone());
    let body = PreparedBody::one_shot(
        AdvancedRequestBody::new(ProducerFailure(producer_gate)),
        None,
    );

    let error = PreparedEndpoint::<Text<String>>::new(
        "RequestErrorBodyProducer",
        Method::POST,
        "/request-error-body",
        PreparedRequestEntity { body },
    )
    .execute(&client)
    .await
    .expect_err("producer failure is terminal");
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_aligned(&error, &hooks);
    crate::support::assert_error_chain_does_not_contain_any(&error, &["PRODUCER_ERROR_SENTINEL"]);
}

const _: fn() = || {
    let _: Result<Bytes, Infallible> = Ok(Bytes::new());
};
