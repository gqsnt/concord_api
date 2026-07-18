use bytes::Bytes;
use concord_test_support::script::{
    BodyCategory, BoundaryRequest, CapturedHeader, CapturedHeaderValue, ExecutionFailureKind,
    ScriptedResponse, TransportFailure,
};
use futures_core::Stream;
use http::{HeaderMap, Method};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use crate::error::ErrorCategory;
use crate::redaction::{SanitizedHeaderValue, SanitizedHeaders};

pub(super) fn boundary_request(
    endpoint: &'static str,
    method: Method,
    safe_url: Option<String>,
    headers: &HeaderMap,
    body_category: BodyCategory,
    known_body_length: Option<usize>,
    expectation_body: Option<Bytes>,
) -> BoundaryRequest {
    let headers = SanitizedHeaders::new(headers)
        .iter()
        .map(|(name, value)| {
            let value = match value {
                SanitizedHeaderValue::Text(text) => CapturedHeaderValue::Text(text.to_owned()),
                SanitizedHeaderValue::Redacted => CapturedHeaderValue::Redacted,
                SanitizedHeaderValue::NonUtf8 => CapturedHeaderValue::NonUtf8,
            };
            CapturedHeader::__new(name.clone(), value)
        })
        .collect();
    BoundaryRequest::__new(
        endpoint,
        method,
        safe_url,
        headers,
        body_category,
        known_body_length,
        expectation_body,
    )
}

pub(super) fn scripted_response(response: ScriptedResponse) -> reqwest::Response {
    let (status, headers, body, polls) = response.__into_parts();
    let body = match polls {
        Some(polls) => reqwest::Body::wrap_stream(ObservedBody {
            body: Some(body),
            polls,
        }),
        None => reqwest::Body::from(body),
    };
    let mut response = http::Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response.into()
}

struct ObservedBody {
    body: Option<Bytes>,
    polls: Arc<AtomicUsize>,
}

impl Stream for ObservedBody {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Poll::Ready(self.body.take().map(Ok))
    }
}

#[cfg(test)]
pub(super) fn minimal_boundary_request(endpoint: &'static str, method: Method) -> BoundaryRequest {
    boundary_request(
        endpoint,
        method,
        None,
        &HeaderMap::new(),
        BodyCategory::Unknown,
        None,
        None,
    )
}

pub(super) fn failure_category(kind: ExecutionFailureKind) -> ErrorCategory {
    match kind {
        ExecutionFailureKind::Scripted(TransportFailure::Timeout) => ErrorCategory::Timeout,
        ExecutionFailureKind::Scripted(TransportFailure::Connect) => ErrorCategory::Connect,
        ExecutionFailureKind::Scripted(TransportFailure::RequestBody) => ErrorCategory::RequestBody,
        ExecutionFailureKind::Scripted(TransportFailure::Transport) => ErrorCategory::Transport,
        ExecutionFailureKind::EmptyQueue
        | ExecutionFailureKind::Mismatch
        | ExecutionFailureKind::CancellationPoint
        | ExecutionFailureKind::Internal
        | _ => ErrorCategory::Internal,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use concord_test_support::executor::DeterministicExecutor;
    use concord_test_support::gate::Gate;
    use concord_test_support::script::{
        BodyCategory, CapturedHeaderValue, ExpectedRequest, ScriptedResponse, TransportFailure,
    };
    use http::{HeaderMap, HeaderValue, Method, StatusCode};

    use super::*;
    use crate::error::ErrorCategory;
    use crate::http::HttpConfigBuilder;
    use crate::http::build::ManagedHttp;
    use crate::runtime::RuntimeConfigBuilder;

    const WAIT: Duration = Duration::from_secs(3);

    fn managed(executor: &DeterministicExecutor) -> ManagedHttp {
        let runtime = RuntimeConfigBuilder::new().resolve();
        ManagedHttp::build_with_development(HttpConfigBuilder::new().resolve(), &runtime, executor)
            .expect("development managed clients should build")
    }

    fn application_request(
        managed: &ManagedHttp,
        endpoint: &'static str,
        method: Method,
        native_url: &str,
    ) -> reqwest::RequestBuilder {
        managed
            .application
            .prepare(endpoint, method, native_url, false)
            .expect("synthetic application request should prepare")
    }

    fn provider_request(managed: &ManagedHttp, method: Method) -> reqwest::RequestBuilder {
        managed
            .provider
            .prepare(method, "https://provider.invalid/token", None)
            .expect("synthetic provider request should prepare")
    }

    #[tokio::test]
    async fn final_call_seam_keeps_channels_independent_and_fifo() {
        let executor = DeterministicExecutor::new();
        let application = executor.application();
        let provider = executor.provider();
        application
            .queue_response(ScriptedResponse::status(
                StatusCode::CREATED,
                "application-one",
            ))
            .expect("first application response should queue");
        application
            .queue_response(ScriptedResponse::status(
                StatusCode::BAD_GATEWAY,
                "application-two",
            ))
            .expect("second application response should queue");
        provider
            .queue_response(ScriptedResponse::status(
                StatusCode::ACCEPTED,
                "provider-one",
            ))
            .expect("provider response should queue");
        application
            .queue_failure(TransportFailure::Timeout)
            .expect("application failure should queue");
        provider
            .queue_failure(TransportFailure::Connect)
            .expect("provider failure should queue");
        let managed = managed(&executor);

        for expected in [StatusCode::CREATED, StatusCode::BAD_GATEWAY] {
            let endpoint = "development.application";
            let method = Method::GET;
            let request = application_request(
                &managed,
                endpoint,
                method.clone(),
                "http://application.invalid/resource",
            );
            let response = managed
                .application
                .execute_at_boundary(
                    endpoint,
                    method.clone(),
                    request,
                    minimal_boundary_request(endpoint, method),
                )
                .await
                .expect("scripted application response should arrive");
            assert_eq!(response.status(), expected);
        }
        assert_eq!(provider.remaining_scripts().expect("provider count"), 2);

        let provider_method = Method::POST;
        let response = managed
            .provider
            .execute_at_boundary(
                provider_request(&managed, provider_method.clone()),
                minimal_boundary_request("development.provider", provider_method),
            )
            .await
            .expect("scripted provider response should arrive");
        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let endpoint = "development.application";
        let method = Method::GET;
        let error = managed
            .application
            .execute_at_boundary(
                endpoint,
                method.clone(),
                application_request(
                    &managed,
                    endpoint,
                    method.clone(),
                    "http://application.invalid/failure",
                ),
                minimal_boundary_request(endpoint, method),
            )
            .await
            .expect_err("scripted application timeout should fail");
        assert_eq!(error.category(), ErrorCategory::Timeout);

        let provider_method = Method::POST;
        let error = managed
            .provider
            .execute_at_boundary(
                provider_request(&managed, provider_method.clone()),
                minimal_boundary_request("development.provider", provider_method),
            )
            .await
            .expect_err("scripted provider connection should fail");
        assert_eq!(error, ErrorCategory::Connect);

        let endpoint = "development.empty";
        let method = Method::DELETE;
        let error = managed
            .application
            .execute_at_boundary(
                endpoint,
                method.clone(),
                application_request(
                    &managed,
                    endpoint,
                    method.clone(),
                    "http://application.invalid/empty",
                ),
                minimal_boundary_request(endpoint, method),
            )
            .await
            .expect_err("an empty queue should fail safely");
        assert_eq!(error.category(), ErrorCategory::Internal);
    }

    #[tokio::test]
    async fn central_capture_and_mismatch_diagnostics_are_redacted() {
        const HEADER_SECRET: &str = "development-header-secret-c233";
        const BODY_SECRET: &str = "development-body-secret-b82a";
        const NATIVE_URL_SECRET: &str = "development-native-url-secret-450f";
        let executor = DeterministicExecutor::new();
        let application = executor.application();
        application
            .queue_expected_response(
                ExpectedRequest::new()
                    .method(Method::POST)
                    .safe_url("https://logical.test/widgets")
                    .body(BODY_SECRET),
                ScriptedResponse::success("response-secret-not-captured"),
            )
            .expect("expected application response should queue");
        let managed = managed(&executor);
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            HeaderValue::from_static(HEADER_SECRET),
        );
        headers.insert("x-public", HeaderValue::from_static("visible"));
        let boundary = boundary_request(
            "development.capture",
            Method::POST,
            Some("https://logical.test/widgets".to_owned()),
            &headers,
            BodyCategory::Bytes,
            Some(BODY_SECRET.len()),
            Some(Bytes::from_static(BODY_SECRET.as_bytes())),
        );
        let native_url = format!(
            "http://user-{NATIVE_URL_SECRET}:password-{NATIVE_URL_SECRET}@application.invalid/private?token={NATIVE_URL_SECRET}"
        );
        let response = managed
            .application
            .execute_at_boundary(
                "development.capture",
                Method::POST,
                application_request(&managed, "development.capture", Method::POST, &native_url),
                boundary,
            )
            .await
            .expect("matching fake values should deliver a response");
        assert_eq!(response.status(), StatusCode::OK);

        let captures = application
            .captures()
            .expect("capture snapshot should work");
        assert_eq!(captures.len(), 1);
        let capture = &captures[0];
        assert_eq!(
            capture.channel(),
            concord_test_support::script::Channel::Application
        );
        assert_eq!(capture.body_category(), BodyCategory::Bytes);
        assert_eq!(capture.known_body_length(), Some(BODY_SECRET.len()));
        assert_eq!(capture.safe_url(), Some("https://logical.test/widgets"));
        assert!(capture.headers().iter().any(|header| {
            header.name() == http::header::AUTHORIZATION
                && header.value() == &CapturedHeaderValue::Redacted
        }));
        assert!(capture.headers().iter().any(|header| {
            header.name() == "x-public"
                && header.value() == &CapturedHeaderValue::Text("visible".to_owned())
        }));
        let formatted = format!("{capture:?}");
        for sentinel in [HEADER_SECRET, BODY_SECRET, NATIVE_URL_SECRET] {
            assert!(!formatted.contains(sentinel));
        }

        const EXPECTED_SECRET: &str = "expected-mismatch-secret-2ebf";
        const ACTUAL_SECRET: &str = "actual-mismatch-secret-36c8";
        application
            .queue_expected_response(
                ExpectedRequest::new().body(EXPECTED_SECRET),
                ScriptedResponse::success("not-delivered"),
            )
            .expect("mismatch response should queue");
        let endpoint = "development.mismatch";
        let method = Method::POST;
        let error = managed
            .application
            .execute_at_boundary(
                endpoint,
                method.clone(),
                application_request(
                    &managed,
                    endpoint,
                    method.clone(),
                    "http://application.invalid/mismatch",
                ),
                boundary_request(
                    endpoint,
                    method,
                    None,
                    &HeaderMap::new(),
                    BodyCategory::Bytes,
                    Some(ACTUAL_SECRET.len()),
                    Some(Bytes::from_static(ACTUAL_SECRET.as_bytes())),
                ),
            )
            .await
            .expect_err("fake-value mismatch should fail safely");
        assert_eq!(error.category(), ErrorCategory::Internal);
        for formatted in [error.to_string(), format!("{error:?}")] {
            assert!(!formatted.contains(EXPECTED_SECRET));
            assert!(!formatted.contains(ACTUAL_SECRET));
        }
    }

    #[tokio::test]
    async fn scoped_gates_and_cancellation_are_deterministic() {
        let executor = DeterministicExecutor::new();
        let application = executor.application();
        let provider = executor.provider();
        let before = Gate::closed();
        application
            .pause_before_execution(before.clone())
            .expect("before-execution gate should install");
        application
            .queue_response(ScriptedResponse::success("after-before-gate"))
            .expect("application response should queue");
        let managed = Arc::new(managed(&executor));
        let task = {
            let managed = Arc::clone(&managed);
            tokio::spawn(async move {
                let endpoint = "development.before";
                let method = Method::GET;
                managed
                    .application
                    .execute_at_boundary(
                        endpoint,
                        method.clone(),
                        application_request(
                            &managed,
                            endpoint,
                            method.clone(),
                            "http://application.invalid/before",
                        ),
                        minimal_boundary_request(endpoint, method),
                    )
                    .await
            })
        };
        tokio::time::timeout(WAIT, before.wait_until_reached())
            .await
            .expect("execution should reach the pre-native gate");
        assert_eq!(application.request_count().expect("request count"), 0);
        before.open();
        tokio::time::timeout(WAIT, task)
            .await
            .expect("application task should finish")
            .expect("application task should not panic")
            .expect("application response should arrive");
        application
            .clear_before_execution()
            .expect("pre-native gate should clear");

        let application_delivery = Gate::closed();
        application
            .queue_response(
                ScriptedResponse::success("application-delivery")
                    .delivery_gate(application_delivery.clone()),
            )
            .expect("application delivery should queue");
        let application_task = {
            let managed = Arc::clone(&managed);
            tokio::spawn(async move {
                let endpoint = "development.application-delivery";
                let method = Method::GET;
                managed
                    .application
                    .execute_at_boundary(
                        endpoint,
                        method.clone(),
                        application_request(
                            &managed,
                            endpoint,
                            method.clone(),
                            "http://application.invalid/delivery",
                        ),
                        minimal_boundary_request(endpoint, method),
                    )
                    .await
            })
        };
        tokio::time::timeout(WAIT, application_delivery.wait_until_reached())
            .await
            .expect("application response delivery should pause");

        let provider_delivery = Gate::closed();
        provider
            .queue_response(
                ScriptedResponse::success("provider-delivery")
                    .delivery_gate(provider_delivery.clone()),
            )
            .expect("provider delivery should queue");
        let provider_task = {
            let managed = Arc::clone(&managed);
            tokio::spawn(async move {
                let method = Method::POST;
                managed
                    .provider
                    .execute_at_boundary(
                        provider_request(&managed, method.clone()),
                        minimal_boundary_request("development.provider-delivery", method),
                    )
                    .await
            })
        };
        tokio::time::timeout(WAIT, provider_delivery.wait_until_reached())
            .await
            .expect("provider response delivery should pause");
        application_delivery.open();
        tokio::time::timeout(WAIT, application_task)
            .await
            .expect("application delivery should finish")
            .expect("application task should not panic")
            .expect("application delivery should succeed");
        assert!(!provider_delivery.is_open());
        provider_delivery.open();
        tokio::time::timeout(WAIT, provider_task)
            .await
            .expect("provider delivery should finish")
            .expect("provider task should not panic")
            .expect("provider delivery should succeed");

        let cancellation = Gate::closed();
        application
            .queue_cancellation_point(cancellation.clone())
            .expect("cancellation point should queue");
        application
            .queue_response(ScriptedResponse::success("after-cancellation"))
            .expect("post-cancellation response should queue");
        provider
            .queue_response(ScriptedResponse::success("unrelated-provider"))
            .expect("unrelated provider response should queue");
        let cancelled = {
            let managed = Arc::clone(&managed);
            tokio::spawn(async move {
                let endpoint = "development.cancel";
                let method = Method::GET;
                managed
                    .application
                    .execute_at_boundary(
                        endpoint,
                        method.clone(),
                        application_request(
                            &managed,
                            endpoint,
                            method.clone(),
                            "http://application.invalid/cancel",
                        ),
                        minimal_boundary_request(endpoint, method),
                    )
                    .await
            })
        };
        tokio::time::timeout(WAIT, cancellation.wait_until_reached())
            .await
            .expect("cancellation point should be reached");
        cancelled.abort();
        assert!(
            cancelled
                .await
                .expect_err("task should be cancelled")
                .is_cancelled()
        );
        assert_eq!(
            application.remaining_scripts().expect("application count"),
            2
        );
        assert_eq!(provider.remaining_scripts().expect("provider count"), 1);

        cancellation.open();
        let endpoint = "development.cancel";
        let method = Method::GET;
        let cancellation_error = managed
            .application
            .execute_at_boundary(
                endpoint,
                method.clone(),
                application_request(
                    &managed,
                    endpoint,
                    method.clone(),
                    "http://application.invalid/cancel-again",
                ),
                minimal_boundary_request(endpoint, method),
            )
            .await
            .expect_err("an explicit cancellation point should fail safely");
        assert_eq!(cancellation_error.category(), ErrorCategory::Internal);
        assert_eq!(
            application.remaining_scripts().expect("application count"),
            1
        );
        assert_eq!(provider.remaining_scripts().expect("provider count"), 1);
    }
}
