//! F2 / X9: a failure after the response head arrived keeps that head (moved out of the native
//! response before the body is read, never cloned), so an SDK keeps status and metadata without
//! capturing them by hand; `decode_with_meta` returns the head beside the value.

use concord::ErrorKind;
use concord_check_core_concepts::{Rate, Rates};
use concord_example_testserver::{Body, Reply, TestServer};

const SECRET: &str = "set-cookie-s3cr3t";

fn rates(server: &TestServer) -> Rates {
    Rates::new(concord::reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap()
}

fn tagged(reply: Reply) -> Reply {
    reply
        .header("x-request-id", "req-42")
        .header("set-cookie", SECRET)
}

#[tokio::test]
async fn success_returns_the_head_beside_the_value() {
    let server = TestServer::start(|_| {
        tagged(Reply::json(
            200,
            br#"{"base":"EUR","quote":"USD","rate":1.08}"#,
        ))
    })
    .await;
    let received = rates(&server)
        .rate("EUR", "USD")
        .prepare()
        .unwrap()
        .receive()
        .await
        .unwrap();
    let (rate, meta) = received.decode_with_meta().await.unwrap();
    assert_eq!(
        rate,
        Rate {
            base: "EUR".into(),
            quote: "USD".into(),
            rate: 1.08
        }
    );
    assert_eq!(meta.status().as_u16(), 200);
    assert_eq!(meta.headers()["x-request-id"], "req-42");
    assert_eq!(meta.operation().unwrap().name(), "Rates.rate");
    assert!(!format!("{meta:?}").contains(SECRET));
}

#[tokio::test]
async fn a_decode_failure_keeps_the_status_and_headers() {
    let server = TestServer::start(|_| tagged(Reply::json(200, br#"{"base":"EUR""#))).await;
    let error = rates(&server).rate("EUR", "USD").send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    let head = error.head().expect("a response arrived");
    assert_eq!(head.status().as_u16(), 200);
    assert_eq!(head.headers()["x-request-id"], "req-42");
    assert_eq!(
        error.status(),
        None,
        "`status` stays the status of a Status error"
    );
    let meta = error.meta().expect("owned copy");
    assert_eq!(meta.operation().unwrap().name(), "Rates.rate");
    for text in [format!("{error}"), format!("{error:?}")] {
        assert!(!text.contains(SECRET) && !text.contains("req-42"), "{text}");
    }
}

#[tokio::test]
async fn a_limit_failure_keeps_the_head() {
    let server = TestServer::start(|_| {
        tagged(Reply::json(
            200,
            br#"{"base":"EUR","quote":"USD","rate":1.08}"#,
        ))
    })
    .await;
    let error = rates(&server)
        .rate("EUR", "USD")
        .prepare()
        .unwrap()
        .receive()
        .await
        .unwrap()
        .decode_limited(8)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    assert_eq!(error.head().unwrap().headers()["x-request-id"], "req-42");
}

#[tokio::test]
async fn a_transport_failure_mid_body_keeps_the_head_but_none_before_it() {
    let server = TestServer::start(|request| {
        if request.path().ends_with("/USD") {
            tagged(
                Reply::json(200, "")
                    .with_body(Body::FailAfter(vec![br#"{"base":"EU"#.as_slice().into()])),
            )
        } else {
            Reply::hang_up()
        }
    })
    .await;
    let mid = rates(&server).rate("EUR", "USD").send().await.unwrap_err();
    assert_eq!(mid.kind(), ErrorKind::Transport);
    assert_eq!(mid.head().unwrap().status().as_u16(), 200);
    assert_eq!(mid.head().unwrap().headers()["x-request-id"], "req-42");

    let before = rates(&server).rate("EUR", "GBP").send().await.unwrap_err();
    assert_eq!(before.kind(), ErrorKind::Transport);
    assert!(before.head().is_none(), "no response arrived");
}

#[tokio::test]
async fn a_drain_failure_keeps_the_head_and_a_drain_returns_it() {
    let server = TestServer::start(|request| {
        if request.path().contains("/broken/") {
            tagged(Reply::new(202).with_body(Body::FailAfter(vec![b"partial".as_slice().into()])))
        } else {
            tagged(Reply::new(202).body("ignored"))
        }
    })
    .await;
    let api = rates(&server);
    let ((), meta) = api
        .purge("ok")
        .prepare()
        .unwrap()
        .receive()
        .await
        .unwrap()
        .decode_with_meta()
        .await
        .unwrap();
    assert_eq!(meta.status().as_u16(), 202);
    let error = api.purge("broken").send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert_eq!(error.head().unwrap().status().as_u16(), 202);
}

#[tokio::test]
async fn status_and_media_errors_keep_their_unread_response_as_before() {
    let server = TestServer::start(|request| {
        if request.path().ends_with("/USD") {
            tagged(Reply::json(404, br#"{"message":"no such pair"}"#))
        } else {
            tagged(Reply::text(200, "text/html", "<html>proxy</html>"))
        }
    })
    .await;
    let status = rates(&server).rate("EUR", "USD").send().await.unwrap_err();
    assert_eq!(status.kind(), ErrorKind::Status);
    assert_eq!(status.status().unwrap().as_u16(), 404);
    assert_eq!(status.head().unwrap().status().as_u16(), 404);
    let body = status.into_response().unwrap().text().await.unwrap();
    assert!(body.contains("no such pair"), "unread until taken");

    let media = rates(&server).rate("EUR", "GBP").send().await.unwrap_err();
    assert_eq!(media.kind(), ErrorKind::Metadata);
    assert_eq!(media.head().unwrap().headers()["content-type"], "text/html");
    assert!(media.response().is_some());
}
