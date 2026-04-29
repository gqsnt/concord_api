use super::common::*;
use concord_core::internal::PaginationPlan;
use concord_core::prelude::ApiClientError;
use http::StatusCode;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn retry_on_page_n_does_not_advance_page_state() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-me"),
            MockResponse::text(StatusCode::OK, "a,b"),
            MockResponse::text(StatusCode::OK, "c"),
        ],
    );
    let sent = transport.clone();
    let client = client(TestAuthVars::default(), transport);

    let endpoint = ItemsEndpoint {
        policy: retry_policy(2),
        pagination: PaginationPlan::OffsetLimit {
            offset_key: "offset".to_string(),
            limit_key: "limit".to_string(),
            offset: 0,
            limit: 2,
            stop_on_short_page: true,
            stop: Default::default(),
        },
    };

    let items = client
        .request(endpoint)
        .paginate()
        .max_pages(4)
        .collect()
        .await?;

    assert_eq!(
        items,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].meta.page_index, 0);
    assert_eq!(requests[1].meta.page_index, 0);
    assert_eq!(
        query_value(&requests[0].url, "offset"),
        Some("0".to_string())
    );
    assert_eq!(
        query_value(&requests[1].url, "offset"),
        Some("0".to_string())
    );
    assert_eq!(requests[2].meta.page_index, 1);
    assert_eq!(
        query_value(&requests[2].url, "offset"),
        Some("2".to_string())
    );
    Ok(())
}

fn query_value(url: &url::Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}
