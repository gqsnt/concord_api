//! Offline wire contracts and negative cases.
use concord::reqwest;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_world_bank::{Client, Error};

fn client(server: &TestServer) -> Client {
    Client::new(reqwest::Client::builder().no_proxy().build().unwrap())
        .with_base_url(format!("{}/v2", server.url()))
        .unwrap()
}

#[tokio::test]
async fn native_send_keeps_the_declared_success_body_limit() {
    let server =
        TestServer::start(|_| Reply::json(200, vec![b' '; concord_sdk_world_bank::BODY_LIMIT + 1]))
            .await;
    let error = client(&server)
        .indicator("br", "SP.POP.TOTL", "2006")
        .prepare()
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), concord::ErrorKind::Limit);
    assert_eq!(error.head().unwrap().status().as_u16(), 200);
    server.shutdown().await;
}
fn json(status: u16, body: &'static str) -> Reply {
    Reply::new(status)
        .header("content-type", "application/json")
        .header("x-test-id", "fixture")
        .body(body)
}
#[tokio::test]
async fn request_contract_and_encoded_segments() {
    let server =
        TestServer::start(|_: &Recorded| json(200, include_str!("../fixtures/year.json"))).await;
    let result = client(&server)
        .indicator("br/x", "SP.POP.TOTL", "2006:2007")
        .set_page(2)
        .set_per_page(1)
        .send()
        .await
        .unwrap();
    assert_eq!(result.observations[0].value, Some(0.0));
    let req = server.single();
    assert_eq!(req.method, "GET");
    assert_eq!(
        req.target,
        "/v2/country/br%2Fx/indicator/SP.POP.TOTL?date=2006%3A2007&page=2&per_page=1&format=json"
    );
    assert_eq!(req.header("accept"), Some("application/json"));
    assert_eq!(req.header("authorization"), None);
    assert_eq!(req.header("content-type"), None);
    assert_eq!(req.body, b"");
    assert_eq!(req.body_len, 0);
    server.shutdown().await;
}
#[tokio::test]
async fn error_documents_keep_status_and_headers() {
    for status in [200, 400] {
        let server = TestServer::start(move |_: &Recorded| {
            json(status, include_str!("../fixtures/error.json"))
        })
        .await;
        let err = client(&server).year("br", "bad", 2006).await.unwrap_err();
        match err {
            Error::Provider { meta, document } => {
                assert_eq!(meta.status().as_u16(), status);
                assert_eq!(meta.headers()["x-test-id"], "fixture");
                assert_eq!(document.message[0].id, "120");
                assert_eq!(document.message[0].key, "Invalid value");
            }
            other => panic!("unexpected: {other:?}"),
        }
        server.shutdown().await;
    }
}
#[tokio::test]
async fn failures_preserve_head_and_reject_bad_shapes() {
    for (status, content_type, body) in [
        (
            200,
            "application/json",
            include_str!("../fixtures/malformed.json"),
        ),
        (200, "text/html", "not json"),
        (503, "text/plain", "unavailable"),
        (400, "application/json", "[broken"),
        (
            200,
            "application/json",
            "[{\"page\":1,\"pages\":1,\"per_page\":1,\"total\":1},[{\"indicator\":{\"id\":\"x\",\"value\":\"x\"},\"country\":{\"id\":\"x\",\"value\":\"x\"},\"date\":\"2006\"}]]",
        ),
    ] {
        let server = TestServer::start(move |_: &Recorded| {
            Reply::new(status)
                .header("content-type", content_type)
                .header("x-test-id", "bad")
                .body(body)
        })
        .await;
        let error = client(&server).year("br", "x", 2006).await.unwrap_err();
        assert_eq!(error.meta().unwrap().status().as_u16(), status);
        assert_eq!(error.meta().unwrap().headers()["x-test-id"], "bad");
        if status >= 400 {
            assert!(matches!(error, Error::Http { .. }));
        } else {
            assert!(matches!(error, Error::Concord { .. }));
        }
        server.shutdown().await;
    }
}
#[tokio::test]
async fn empty_and_invalid_metadata() {
    let server = TestServer::start(|r: &Recorded| {
        if r.target.contains("page=2") {
            json(
                200,
                "[{\"page\":1,\"pages\":2,\"per_page\":1,\"total\":2},null]",
            )
        } else {
            json(200, include_str!("../fixtures/empty.json"))
        }
    })
    .await;
    let api = client(&server);
    let empty = api.year("br", "x", 2006).await.unwrap();
    assert!(empty.observations.is_empty());
    let mut pages = api
        .range(
            "br",
            "x",
            2006,
            2007,
            1.try_into().unwrap(),
            concord_sdk_world_bank::concord_pagination::Resume::At(2),
        )
        .unwrap();
    assert!(matches!(
        pages.next_page().await.unwrap_err(),
        concord_sdk_world_bank::concord_pagination::PagesError::Provider {
            error: Error::InvalidPage { .. },
            ..
        }
    ));
    assert!(matches!(
        api.range(
            "br",
            "x",
            2007,
            2006,
            1.try_into().unwrap(),
            concord_sdk_world_bank::concord_pagination::Resume::Start
        ),
        Err(Error::InvalidRange)
    ));
    server.shutdown().await;
}

#[tokio::test]
async fn oversized_body_is_rejected_with_head() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::new(200)
            .header("content-type", "application/json")
            .header("x-test-id", "large")
            .body(" ".repeat(concord_sdk_world_bank::BODY_LIMIT + 1))
    })
    .await;
    let error = client(&server).year("br", "x", 2006).await.unwrap_err();
    assert!(
        matches!(&error, Error::Concord { source, .. } if source.kind() == concord::ErrorKind::Limit)
    );
    assert_eq!(error.meta().unwrap().headers()["x-test-id"], "large");
    server.shutdown().await;
}
