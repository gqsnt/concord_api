use bytes::Bytes;
use concord_test_support::executor::DeterministicExecutor;
use concord_test_support::script::{
    BodyCategory, BoundaryRequest, ExecutionFailureKind, ExpectedRequest, ScriptedResponse,
};
use http::Method;

#[tokio::test]
async fn fake_value_mismatch_diagnostics_are_value_free() {
    const EXPECTED_BODY: &str = "expected-fake-body-secret-1c62";
    const ACTUAL_BODY: &str = "actual-fake-body-secret-d84f";
    const EXPECTED_URL: &str = "https://expected.test/path-secret-afe4";
    const ACTUAL_URL: &str = "https://actual.test/path-secret-b9c3";
    const RESPONSE_BODY: &str = "fake-response-secret-33ea";
    let application = DeterministicExecutor::new().application();
    let response = ScriptedResponse::success(Bytes::from_static(RESPONSE_BODY.as_bytes()));
    assert!(!format!("{response:?}").contains(RESPONSE_BODY));
    application
        .queue_expected_response(
            ExpectedRequest::new()
                .safe_url(EXPECTED_URL)
                .body(Bytes::from_static(EXPECTED_BODY.as_bytes())),
            response,
        )
        .expect("expectation should queue");
    let request = BoundaryRequest::__new(
        "redaction.mismatch",
        Method::POST,
        Some(ACTUAL_URL.to_owned()),
        Vec::new(),
        BodyCategory::Bytes,
        Some(ACTUAL_BODY.len()),
        Some(Bytes::from_static(ACTUAL_BODY.as_bytes())),
    );
    let error = application
        .__execute(request)
        .await
        .expect_err("fake values should mismatch");
    assert_eq!(error.kind(), ExecutionFailureKind::Mismatch);
    for formatted in [error.to_string(), format!("{error:?}")] {
        for sentinel in [
            EXPECTED_BODY,
            ACTUAL_BODY,
            EXPECTED_URL,
            ACTUAL_URL,
            RESPONSE_BODY,
        ] {
            assert!(!formatted.contains(sentinel));
        }
    }
}
