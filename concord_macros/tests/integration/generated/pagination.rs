use bytes::Bytes;
use concord_core::error::ErrorCategory;
use concord_core::prelude::*;
use concord_macros::api;
use concord_test_support::{
    DeterministicMock, MockExecutionHandle, RecordedExecution, ScriptedReply, deterministic_mock,
};
use http::{HeaderMap, StatusCode};
use std::sync::{Arc, Mutex};

use self::pagination_helper_api::PaginationHelperApi;

api! {
    client PaginationHelperApi {
        base "https://example.com"
    }

    GET List(filter?: String, tags?: Vec<String>, start: u64 = 0, count: u64 = 2)
        as list
        path ["items"]
        query {
            filter
            tags
            start
            count
        }
        paginate OffsetLimitPagination {
            offset = start,
            limit = count
        }
        -> Json<Vec<String>>
}

#[tokio::test]
async fn generated_pagination_collect_preserves_query_setters_and_caps() {
    let transport = RecordingTransport::new(vec![
        ResponseFixture::json(r#"["a","b"]"#),
        ResponseFixture::json(r#"[]"#),
    ]);
    let sent = transport.clone();
    let api = transport.client();

    let items = api
        .list()
        .filter("ranked".to_string())
        .tags(vec!["first".to_string(), "second".to_string()])
        .count(2)
        .paginate(PaginationTermination::hard_page_cap(3))
        .collect()
        .await
        .expect("pagination collect succeeds");

    assert_eq!(items, vec!["a".to_string(), "b".to_string()]);
    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_query(&requests[0], "filter", "ranked");
    assert_query_values(&requests[0], "tags", &["first", "second"]);
    assert_query(&requests[0], "start", "0");
    assert_query(&requests[0], "count", "2");
    assert_query(&requests[1], "filter", "ranked");
    assert_query_values(&requests[1], "tags", &["first", "second"]);
    assert_query(&requests[1], "start", "2");
    assert_query(&requests[1], "count", "2");
}

#[tokio::test]
async fn generated_pagination_collect_and_max_items_work() {
    let transport = RecordingTransport::new(vec![ResponseFixture::json(r#"["x"]"#)]);
    let api = transport.client();

    let items = api
        .list()
        .count(2)
        .paginate(PaginationTermination::hard_page_cap(3))
        .collect()
        .await
        .expect("collect succeeds");
    assert_eq!(items, vec!["x".to_string()]);

    let transport = RecordingTransport::new(vec![ResponseFixture::json(r#"["a","b"]"#)]);
    let api = transport.client();
    let err = api
        .list()
        .count(2)
        .paginate(PaginationTermination::hard_item_cap(1))
        .collect()
        .await
        .expect_err("hard item cap should fail");
    assert!(err.to_string().contains("hard item cap"));
}

#[tokio::test]
async fn generated_pagination_later_page_failure_is_typed_and_redacted() {
    let sentinel = "SECRET_GENERATED_PAGINATION_SENTINEL_MUST_NOT_APPEAR";
    let transport = RecordingTransport::new(vec![
        ResponseFixture::json(r#"["a","b"]"#),
        ResponseFixture::json_status(StatusCode::INTERNAL_SERVER_ERROR, sentinel),
    ]);
    let api = transport.client();

    let err = api
        .list()
        .count(2)
        .paginate(PaginationTermination::hard_page_cap(3))
        .collect()
        .await
        .expect_err("later page failure should be typed");
    assert!(matches!(err, ApiClientError::HttpStatus { .. }));
    assert_eq!(err.category(), ErrorCategory::HttpStatus);
    assert_eq!(err.context().endpoint, "List");
    assert_eq!(err.context().method, http::Method::GET);
    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    let requests = transport.requests().await;
    assert_eq!(requests.len(), 2);
    assert!(!format!("{err:?}").contains(sentinel));
    assert!(!format!("{err}").contains(sentinel));
}

fn assert_query(request: &RecordedExecution, key: &str, expected: &str) {
    let value = request
        .logical_url
        .query_pairs()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.into_owned());
    assert_eq!(value.as_deref(), Some(expected), "query key `{key}`");
}

#[derive(Clone)]
struct RecordingTransport {
    script: DeterministicMock,
    handle: Arc<Mutex<MockExecutionHandle>>,
}

impl RecordingTransport {
    fn new(responses: Vec<ResponseFixture>) -> Self {
        let replies = responses.into_iter().map(ResponseFixture::into_reply);
        let (script, handle) = deterministic_mock().replies(replies).build();
        Self {
            script,
            handle: Arc::new(Mutex::new(handle)),
        }
    }

    async fn requests(&self) -> Vec<RecordedExecution> {
        self.handle.lock().expect("handle lock").recorded()
    }

    fn client(&self) -> PaginationHelperApi {
        PaginationHelperApi::new_with_safe_reqwest_builder(|builder| {
            self.script.configure_application(builder)
        })
        .expect("deterministic generated pagination client")
    }
}

struct ResponseFixture {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl ResponseFixture {
    fn json(body: &'static str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        Self {
            status: StatusCode::OK,
            headers,
            body: Bytes::from_static(body.as_bytes()),
        }
    }

    fn json_status(status: StatusCode, body: &'static str) -> Self {
        let mut fixture = Self::json(body);
        fixture.status = status;
        fixture
    }

    fn into_reply(self) -> ScriptedReply {
        let mut reply = ScriptedReply::status(self.status).with_body(self.body);
        for (name, value) in self.headers {
            if let Some(name) = name {
                reply = reply.with_header(name, value);
            }
        }
        reply
    }
}

fn assert_query_values(request: &RecordedExecution, key: &str, expected: &[&str]) {
    let values: Vec<String> = request
        .logical_url
        .query_pairs()
        .filter(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.into_owned())
        .collect();
    assert_eq!(values, expected);
}
