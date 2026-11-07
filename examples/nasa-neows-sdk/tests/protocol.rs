//! Offline wire and failure tests.
use concord::ErrorKind;
use concord::reqwest;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_nasa_neows::{BODY_LIMIT, NeoWs, NeoWsConfig};

fn client(server: &TestServer) -> NeoWs {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    NeoWs::new(
        http,
        NeoWsConfig {
            api_key: "synthetic-secret".into(),
        },
    )
    .with_base_url(format!("{}/neo/rest/v1", server.url()))
    .unwrap()
}

#[tokio::test]
async fn wire_shapes_and_secret_redaction() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/neo/rest/v1/feed" => Reply::json(200, include_str!("../fixtures/feed.json")),
        "/neo/rest/v1/neo/browse" => Reply::json(200, include_str!("../fixtures/browse.json")),
        _ => Reply::json(200, include_str!("../fixtures/lookup-feed.json")),
    })
    .await;
    let api = client(&server);
    let scope = api.neows();
    let op = scope.feed("2015-09-07", "2015-09-08");
    assert!(!format!("{api:?} {scope:?} {op:?}").contains("synthetic-secret"));
    let feed = op.send().await.unwrap();
    assert!(!format!("{feed:?}").contains("synthetic-secret"));
    scope.browse().set_page(0).set_size(1).send().await.unwrap();
    scope.lookup("3542519").send().await.unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests.iter().map(|r| r.path()).collect::<Vec<_>>(),
        [
            "/neo/rest/v1/feed",
            "/neo/rest/v1/neo/browse",
            "/neo/rest/v1/neo/3542519"
        ]
    );
    assert_eq!(requests[0].query_all("start_date"), ["2015-09-07"]);
    assert_eq!(requests[0].query_all("end_date"), ["2015-09-08"]);
    assert_eq!(requests[1].query_all("page"), ["0"]);
    assert_eq!(requests[1].query_all("size"), ["1"]);
    for request in requests {
        assert_eq!(request.method, "GET");
        assert_eq!(request.query_all("api_key"), ["synthetic-secret"]);
        assert_eq!(request.header("accept"), Some("application/json"));
        assert!(request.body.is_empty());
    }
    server.shutdown().await;
}

#[tokio::test]
async fn status_failures_are_classified_without_echoing_secrets() {
    for status in [400, 401, 403, 404, 429, 500] {
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, include_str!("../fixtures/key-error.json"))
        })
        .await;
        let error = client(&server)
            .neows()
            .lookup("3542519")
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Status);
        assert_eq!(error.status.unwrap().as_u16(), status);
        for rendered in [format!("{error}"), format!("{error:?}")] {
            assert!(!rendered.contains("synthetic-secret"));
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn malformed_wrong_media_truncated_and_oversized() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/neo/rest/v1/neo/malformed" => {
            Reply::json(200, include_str!("../fixtures/malformed.json"))
        }
        "/neo/rest/v1/neo/media" => Reply::text(200, "text/html", "synthetic-secret"),
        "/neo/rest/v1/neo/truncated" => Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![b"{\"id\":".as_slice().into()])),
        _ => Reply::json(200, vec![b' '; BODY_LIMIT + 1]),
    })
    .await;
    let api = client(&server);
    for (id, kind) in [
        ("malformed", ErrorKind::Decode),
        ("media", ErrorKind::Metadata),
        ("truncated", ErrorKind::Transport),
        ("large", ErrorKind::Limit),
    ] {
        let error = api.neows().lookup(id).send().await.unwrap_err();
        assert_eq!(error.kind, kind, "{id}");
        assert_eq!(error.status.unwrap().as_u16(), 200);
        assert!(!format!("{error:?} {error}").contains("synthetic-secret"));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn build_and_connection_errors_are_safe() {
    let server = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let api = client(&server);
    let err = api.neows().lookup(".").send().await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::Build);
    assert!(server.requests().is_empty());
    let err = api.neows().lookup("1").send().await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::Transport);
    assert_eq!(err.status, None);
    assert!(!format!("{err:?} {err}").contains("synthetic-secret"));
    server.shutdown().await;
}
