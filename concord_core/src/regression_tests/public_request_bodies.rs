use super::common::{MockResponse, NativeMockHarness, ObservationAuthVars, observation_client};
use bytes::Bytes;
#[cfg(feature = "multipart")]
use concord_core::advanced::MultipartBody;
use concord_core::advanced::{
    AdvancedRequestBody, BodyError, CredentialId, PreparedBody, PreparedEndpoint,
    PreparedRequestEntity, RequestAuthentication,
};
use concord_core::prelude::Text;
use http::{Method, StatusCode};
use http_body::SizeHint;
use http_body_util::Full;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::Mutex;

fn public_body_endpoint(body: PreparedBody, authenticated: bool) -> PreparedEndpoint<Text<String>> {
    let endpoint = PreparedEndpoint::new(
        "PublicRequestBody",
        Method::POST,
        "/public-request-body",
        PreparedRequestEntity { body },
    );
    if authenticated {
        endpoint.authentication(RequestAuthentication::bearer(CredentialId::new(
            "test", "token",
        )))
    } else {
        endpoint
    }
}

#[tokio::test]
async fn custom_one_shot_standard_body_is_public_and_not_auth_recoverable() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::UNAUTHORIZED, "challenge")],
    );
    let client = observation_client(
        ObservationAuthVars::bearer_replacing("first", "second", "refresh", events),
        &harness,
    );
    let body = PreparedBody::one_shot(
        AdvancedRequestBody::new(Full::<Bytes>::new(Bytes::from_static(b"one-shot"))),
        Some(http::HeaderValue::from_static("application/octet-stream")),
    );

    let error = public_body_endpoint(body, true)
        .execute(&client)
        .await
        .expect_err("one-shot advanced bodies cannot be reconstructed");
    assert!(matches!(
        error,
        concord_core::prelude::ApiClientError::HttpStatus {
            status: StatusCode::UNAUTHORIZED,
            ..
        }
    ));
    assert_eq!(harness.sent_count().await, 1);
}

#[tokio::test]
async fn complete_advanced_factory_runs_once_per_visible_execution_and_recovers_auth() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "challenge"),
            MockResponse::text(StatusCode::OK, "recovered"),
        ],
    );
    let client = observation_client(
        ObservationAuthVars::bearer_replacing("first", "second", "refresh", events),
        &harness,
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let factory_calls = calls.clone();
    let mut hint = SizeHint::new();
    hint.set_exact(7);
    let body = PreparedBody::factory(hint, None, move || {
        factory_calls.fetch_add(1, Ordering::SeqCst);
        Ok(AdvancedRequestBody::new(Full::<Bytes>::new(
            Bytes::from_static(b"factory"),
        )))
    });

    assert!(body.is_replayable());
    assert_eq!(calls.load(Ordering::SeqCst), 0, "eligibility is structural");
    let value = public_body_endpoint(body, true)
        .execute(&client)
        .await
        .expect("factory reconstructs the auth recovery body");
    assert_eq!(value, "recovered");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(harness.sent_count().await, 2);
    let requests = harness.requests().await;
    assert_eq!(
        requests[0].body.as_bytes(),
        Some(&Bytes::from_static(b"factory"))
    );
    assert_eq!(
        requests[1].body.as_bytes(),
        Some(&Bytes::from_static(b"factory"))
    );
}

#[test]
fn public_factory_accepts_safe_producer_failures_without_invoking_eligibility_checks() {
    let calls = Arc::new(AtomicUsize::new(0));
    let factory_calls = calls.clone();
    let body = PreparedBody::factory(SizeHint::new(), None, move || {
        factory_calls.fetch_add(1, Ordering::SeqCst);
        Err(BodyError::input())
    });
    assert!(body.is_replayable());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
#[cfg(feature = "multipart")]
async fn complete_multipart_factory_creates_a_fresh_reqwest_boundary() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let harness = NativeMockHarness::new(
        events,
        vec![
            MockResponse::text(StatusCode::UNAUTHORIZED, "challenge"),
            MockResponse::text(StatusCode::OK, "recovered"),
        ],
    );
    let client = observation_client(
        ObservationAuthVars::bearer_replacing(
            "first",
            "second",
            "refresh",
            Arc::new(Mutex::new(Vec::new())),
        ),
        &harness,
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let factory_calls = calls.clone();
    let body = PreparedBody::multipart_factory(move || {
        factory_calls.fetch_add(1, Ordering::SeqCst);
        Ok(MultipartBody::new().text("field", "value"))
    });
    assert!(body.is_replayable());
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    public_body_endpoint(body, true)
        .execute(&client)
        .await
        .expect("complete multipart factory recovers authentication");
    let requests = harness.requests().await;
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let first = requests[0]
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("first multipart content type");
    let second = requests[1]
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("second multipart content type");
    assert!(first.starts_with("multipart/form-data; boundary="));
    assert!(second.starts_with("multipart/form-data; boundary="));
    assert_ne!(first, second);
}

const _: fn() = || {
    fn accepts_standard_body<B>(body: B) -> AdvancedRequestBody
    where
        B: http_body::Body<Data = Bytes, Error = Infallible> + Send + 'static,
    {
        AdvancedRequestBody::new(body)
    }
    let _ = accepts_standard_body(Full::<Bytes>::new(Bytes::new()));
};
