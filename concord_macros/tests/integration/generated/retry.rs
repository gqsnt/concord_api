use bytes::Bytes;
use concord_macros::api;
use concord_test_support::{MockHandle, MockReply, MockServer, RecordedRequest, mock};
use http::{HeaderMap, StatusCode};
use std::sync::{Arc, Mutex as StdMutex};

mod retry_helper_contract {
    #![allow(unused_imports)]
    use super::*;
    use concord_core::prelude::Json;

    api! {
        client RetryHelperApi {
            base "https://example.com"

            retry read {
                max_attempts 2
                methods [GET]
                on [500]
            }
        }

        GET Fetch
            as fetch
            path ["retry"]
            retry read
            -> Json<String>
    }

    pub(super) use retry_helper_api::RetryHelperApi;
}

use retry_helper_contract::RetryHelperApi;

#[derive(Clone)]
struct RecordingTransport {
    server: MockServer,
    handle: Arc<StdMutex<MockHandle>>,
}

#[derive(Clone)]
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

    fn into_reply(self) -> MockReply {
        let mut reply = MockReply::status(self.status).with_body(self.body);
        for (name, value) in self.headers {
            if let Some(name) = name {
                reply = reply.with_header(name, value);
            }
        }
        reply
    }
}

impl RecordingTransport {
    fn new(responses: Vec<ResponseFixture>) -> Self {
        let (server, handle) = mock()
            .replies(responses.into_iter().map(ResponseFixture::into_reply))
            .build();
        Self {
            server,
            handle: Arc::new(StdMutex::new(handle)),
        }
    }

    async fn requests(&self) -> Vec<RecordedRequest> {
        self.handle.lock().expect("handle lock").recorded()
    }

    fn configure_reqwest(
        &self,
        builder: concord_core::advanced::SafeReqwestBuilder,
    ) -> concord_core::advanced::SafeReqwestBuilder {
        self.server.configure_reqwest(builder)
    }
}

#[tokio::test]
async fn generated_retry_retries_then_succeeds_and_preserves_metadata() {
    let transport = RecordingTransport::new(vec![
        ResponseFixture::json_status(StatusCode::INTERNAL_SERVER_ERROR, r#""retry""#),
        ResponseFixture::json(r#""ok""#),
    ]);
    let sent = transport.clone();
    let api = RetryHelperApi::new_with_safe_reqwest_builder(|builder| {
        transport.configure_reqwest(builder)
    })
    .expect("mock client");

    let value = api
        .fetch()
        .execute()
        .await
        .expect("retry request should eventually succeed");
    assert_eq!(value, "ok");

    let requests = sent.requests().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method, requests[1].method);
    assert_eq!(requests[0].url.path(), "/retry");
    assert_eq!(requests[1].url.path(), "/retry");
}
