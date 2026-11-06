//! Offline HTTP contract tests.
use concord::reqwest;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_pokeapi::{Client, Error, UrlRejection};

fn client(server: &TestServer) -> Client {
    Client::new(reqwest::Client::builder().no_proxy().build().unwrap())
        .with_base_url(format!("http://{}/api/v2", server.addr()))
        .unwrap()
}
fn json(status: u16, body: impl Into<String>) -> Reply {
    Reply::new(status)
        .header("content-type", "application/json")
        .body(body.into())
}

#[tokio::test]
async fn detail_and_list_wire_contract() {
    let server = TestServer::start(|r: &Recorded| {
        if r.path().contains("35") {
            json(200, include_str!("../fixtures/pokemon.json"))
        } else {
            json(
                200,
                include_str!("../fixtures/page-empty.json").replace("HOST", &r.host),
            )
        }
    })
    .await;
    let api = client(&server);
    let pokemon = api.pokemon("35").send().await.unwrap();
    assert_eq!(pokemon.value.name, "clefairy");
    assert_eq!(
        pokemon.value.extra["abilities"][0]["ability"]["name"],
        "friend-guard"
    );
    assert_eq!(pokemon.meta.status, 200);
    let page = api.list().set_limit(5).set_offset(2).send().await.unwrap();
    assert!(page.value.results.is_empty());
    assert!(page.value.next.is_none());
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].target, "/api/v2/pokemon/35/");
    assert_eq!(requests[1].target, "/api/v2/pokemon/?limit=5&offset=2");
    for req in requests {
        assert_eq!(req.method, "GET");
        assert_eq!(req.body, b"");
        assert_eq!(req.body_len, 0);
        assert!(req.header("authorization").is_none());
        assert!(req.header("content-type").is_none());
        assert_eq!(req.header("accept"), Some("application/json"));
        assert_eq!(req.host, server.addr().to_string());
    }
    server.shutdown().await;
}

#[tokio::test]
async fn status_error_keeps_head_and_unread_body() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::new(404)
            .header("content-type", "text/plain")
            .header("x-test-id", "offline-404")
            .body(include_str!("../fixtures/not-found.txt"))
    })
    .await;
    let err = client(&server).pokemon("missing").send().await.unwrap_err();
    match err {
        Error::Concord {
            error,
            meta: Some(meta),
        } => {
            assert_eq!(meta.status, 404);
            assert_eq!(meta.headers["x-test-id"], "offline-404");
            assert_eq!(error.status().unwrap(), 404);
            let response = error.into_response().unwrap();
            assert_eq!(
                response.bytes().await.unwrap().as_ref(),
                include_bytes!("../fixtures/not-found.txt")
            );
        }
        other => panic!("unexpected {other:?}"),
    }
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_json_and_wrong_content_type_retain_head() {
    let server = TestServer::start(|r: &Recorded| {
        if r.query().is_some() {
            Reply::new(200)
                .header("content-type", "text/html")
                .body("not json")
        } else {
            json(200, include_str!("../fixtures/malformed.json"))
        }
    })
    .await;
    let api = client(&server);
    for err in [
        api.list().send().await.unwrap_err(),
        api.list().set_limit(1).send().await.unwrap_err(),
    ] {
        match err {
            Error::Concord {
                meta: Some(meta), ..
            } => assert_eq!(meta.status, 200),
            other => panic!("{other:?}"),
        }
    }
    server.shutdown().await;
}

#[tokio::test]
async fn links_must_be_absolute_same_origin_and_in_scope() {
    let server = TestServer::start(|_: &Recorded| json(200, "{}")).await;
    let api = client(&server);
    let base = format!("http://{}", server.addr());
    for (bad, reason) in [
        (
            "/api/v2/pokemon/?offset=1".to_string(),
            UrlRejection::Unparseable,
        ),
        (
            "http://example.invalid/api/v2/pokemon/".to_string(),
            UrlRejection::ForeignOrigin,
        ),
        (format!("{base}/api/v2/ability/"), UrlRejection::WrongPath),
        (
            format!("{base}/api/v2/pokemon/?offset=1#frag"),
            UrlRejection::Fragment,
        ),
        (
            format!("http://user:secret@{}/api/v2/pokemon/", server.addr()),
            UrlRejection::Userinfo,
        ),
    ] {
        let error = api.follow(&bad).await.unwrap_err();
        assert!(
            matches!(error, Error::InvalidUrl(r) if r == reason),
            "{bad}"
        );
        for shown in [error.to_string(), format!("{error:?}")] {
            assert!(
                !shown.contains("/api/v2/") && !shown.contains("secret"),
                "{shown}"
            );
        }
    }
    for (bad, reason) in [
        (format!("{base}/api/v2/pokemon/"), UrlRejection::WrongPath),
        (
            format!("{base}/api/v2/pokemon/35/encounters"),
            UrlRejection::WrongPath,
        ),
        (
            format!("{base}/api/v2/pokemon/35/?x=1"),
            UrlRejection::Query,
        ),
    ] {
        assert!(
            matches!(api.pokemon_url(&bad).await, Err(Error::InvalidUrl(r)) if r == reason),
            "{bad}"
        );
    }
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn missing_next_is_not_mistaken_for_end() {
    let server =
        TestServer::start(|_: &Recorded| json(200, r#"{"count":0,"previous":null,"results":[]}"#))
            .await;
    let error = client(&server).list().send().await.unwrap_err();
    assert!(matches!(error, Error::Concord { meta: Some(_), .. }));
    server.shutdown().await;
}

#[tokio::test]
async fn failed_body_keeps_head_and_transport_failure_has_no_head() {
    use concord_example_testserver::Body;
    let server = TestServer::start(|_: &Recorded| {
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec!["{\"count\":".into()]))
    })
    .await;
    let api = client(&server);
    let error = api.list().send().await.unwrap_err();
    assert!(matches!(error, Error::Concord { meta: Some(_), .. }));
    let addr = server.addr();
    server.shutdown().await;
    let offline = Client::new(reqwest::Client::builder().no_proxy().build().unwrap())
        .with_base_url(format!("http://{addr}/api/v2"))
        .unwrap();
    assert!(matches!(
        offline.list().send().await.unwrap_err(),
        Error::Concord { meta: None, .. }
    ));
}
