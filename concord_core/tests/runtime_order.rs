mod support;

use concord_core::advanced::{BuiltRequest, CacheRequestMode, RateLimitPlan};
use concord_core::internal::{CacheSetting, RetrySetting};
use concord_core::transport::RequestMeta;

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

    assert_eq!(response.status, http::StatusCode::OK);
    support::assert_event_order(&transport.events.snapshot(), &["transport_send:Ping"]);
}

fn request(endpoint: &'static str) -> BuiltRequest {
    BuiltRequest {
        meta: RequestMeta {
            endpoint,
            method: http::Method::GET,
            idempotent: true,
            attempt: 0,
            page_index: 0,
        },
        url: "https://example.com/test".parse().expect("valid url"),
        headers: http::HeaderMap::new(),
        body: None,
        timeout: None,
        retry: RetrySetting::Inherit,
        rate_limit: RateLimitPlan::new(),
        cache: CacheSetting::Off,
        cache_mode: CacheRequestMode::Default,
        cache_revalidation: None,
        extensions: Default::default(),
    }
}
