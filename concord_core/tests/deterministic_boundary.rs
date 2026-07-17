use std::time::Duration;

use concord_test_support::executor::DeterministicExecutor;
use concord_test_support::gate::Gate;
use concord_test_support::script::{
    BodyCategory, BoundaryRequest, ExecutionFailureKind, ScriptedResponse, TransportFailure,
};
use http::{Method, StatusCode};

const WAIT: Duration = Duration::from_secs(3);

fn request(endpoint: &'static str) -> BoundaryRequest {
    BoundaryRequest::__new(
        endpoint,
        Method::GET,
        Some(format!("https://logical.test/{endpoint}")),
        Vec::new(),
        BodyCategory::Empty,
        Some(0),
        None,
    )
}

#[tokio::test]
async fn downstream_harness_channels_are_independent_and_fifo() {
    let executor = DeterministicExecutor::new();
    let application = executor.application();
    let provider = executor.provider();
    application
        .queue_response(ScriptedResponse::status(StatusCode::CREATED, "one"))
        .expect("first application response should queue");
    application
        .queue_response(ScriptedResponse::status(StatusCode::NO_CONTENT, ""))
        .expect("second application response should queue");
    provider
        .queue_response(ScriptedResponse::status(StatusCode::ACCEPTED, "provider"))
        .expect("provider response should queue");

    for expected in [StatusCode::CREATED, StatusCode::NO_CONTENT] {
        let response = application
            .__execute(request("application"))
            .await
            .expect("application response should arrive");
        assert_eq!(response.__into_http_response().status(), expected);
    }
    assert_eq!(provider.remaining_scripts().expect("provider count"), 1);
    let response = provider
        .__execute(request("provider"))
        .await
        .expect("provider response should arrive");
    assert_eq!(
        response.__into_http_response().status(),
        StatusCode::ACCEPTED
    );
}

#[tokio::test]
async fn empty_and_scripted_failures_are_structural() {
    let application = DeterministicExecutor::new().application();
    let empty = application
        .__execute(request("empty"))
        .await
        .expect_err("empty queue should fail");
    assert_eq!(empty.kind(), ExecutionFailureKind::EmptyQueue);
    application
        .queue_failure(TransportFailure::Timeout)
        .expect("failure should queue");
    let failure = application
        .__execute(request("timeout"))
        .await
        .expect_err("scripted timeout should fail");
    assert_eq!(
        failure.kind(),
        ExecutionFailureKind::Scripted(TransportFailure::Timeout)
    );
}

#[tokio::test]
async fn cancellation_preserves_the_reserved_script() {
    let application = DeterministicExecutor::new().application();
    let delivery = Gate::closed();
    application
        .queue_response(ScriptedResponse::success("delayed").delivery_gate(delivery.clone()))
        .expect("delayed response should queue");
    let task = {
        let application = application.clone();
        tokio::spawn(async move { application.__execute(request("cancelled")).await })
    };
    tokio::time::timeout(WAIT, delivery.wait_until_reached())
        .await
        .expect("delivery gate should be reached");
    task.abort();
    assert!(task.await.expect_err("task should cancel").is_cancelled());
    assert_eq!(application.remaining_scripts().expect("script count"), 1);
    delivery.open();
    application
        .__execute(request("retried"))
        .await
        .expect("reserved response should be requeued");
}
