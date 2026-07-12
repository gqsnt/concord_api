use concord_core::advanced::{DynBody, RequestExecutionContext};
use concord_core::transport::RequestMeta;

use crate::support;

#[test]
fn runtime_order_harness_records_events() {
    let events = support::EventRecorder::default();
    events.record("request_start");
    events.record("request_end");
    support::assert_event_order(&events.snapshot(), &["request_start", "request_end"]);
}

#[tokio::test]
async fn mock_transport_records_send_events() {
    let transport = support::MockTransport::default();
    transport.push(support::MockResponse::text(200, b"ok".to_vec()));

    let response = concord_core::advanced::Transport::send(&transport, request("Ping"))
        .await
        .expect("mock response");

    assert_eq!(response.status(), http::StatusCode::OK);
    support::assert_event_order(&transport.events.snapshot(), &["transport_send:Ping"]);
}

fn request(endpoint: &'static str) -> http::Request<DynBody> {
    let mut request = http::Request::new(DynBody::empty());
    *request.uri_mut() = "https://example.com/test".parse().expect("valid URI");
    request.extensions_mut().insert(RequestExecutionContext {
        meta: RequestMeta {
            endpoint,
            method: http::Method::GET,
            idempotent: true,
            attempt: 0,
            page_index: 0,
        },
        timeout: None,
    });
    request
}
