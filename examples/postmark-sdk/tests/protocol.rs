//! Wire protocol and failure handling.

use concord::ErrorKind;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_postmark::{ERROR_LIMIT, Error, Postmark, PostmarkConfig, RESPONSE_LIMIT};

fn client(server: &TestServer) -> Postmark {
    Postmark::new(
        concord_sdk_postmark::http_client_builder().build().unwrap(),
        PostmarkConfig {
            token: "synthetic-secret-token".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}

#[tokio::test]
async fn all_documented_http_error_classes_and_redaction() {
    for status in [401, 404, 413, 415, 422, 429, 500, 503] {
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, include_str!("../fixtures/error.json")).header("retry-after", "7")
        })
        .await;
        let api = client(&server);
        let prepared = api.outbound().details("id/with space").prepare().unwrap();
        assert!(prepared.request().headers()["x-postmark-server-token"].is_sensitive());
        assert!(!format!("{api:?} {prepared:?}").contains("synthetic-secret-token"));
        let error = api
            .outbound()
            .details("id/with space")
            .send()
            .await
            .unwrap_err();
        assert_eq!(
            server.single().path(),
            "/messages/outbound/id%2Fwith%20space/details"
        );
        assert!(!format!("{error:?} {error}").contains("synthetic-secret-token"));
        match error {
            Error::Api {
                status: actual,
                document,
                retry_after_seconds,
            } => {
                assert_eq!(actual.as_u16(), status);
                assert_eq!(document.unwrap().error_code, 403);
                assert_eq!(retry_after_seconds, Some(7));
            }
            _ => panic!("expected provider error"),
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn bad_success_responses_are_bounded_and_classified() {
    for (case, kind) in [
        (0, ErrorKind::Decode),
        (1, ErrorKind::Transport),
        (2, ErrorKind::Limit),
        (3, ErrorKind::Metadata),
    ] {
        let server = TestServer::start(move |_: &Recorded| match case {
            0 => Reply::json(200, "{bad json"),
            1 => Reply::json(200, "").with_body(Body::FailAfter(vec!["{\"TotalCount\":".into()])),
            2 => Reply::json(200, vec![b'x'; RESPONSE_LIMIT + 1]),
            _ => Reply::text(200, "text/html", "{}"),
        })
        .await;
        let error = client(&server)
            .outbound()
            .list(5, 2)
            .send()
            .await
            .unwrap_err();
        match error {
            Error::Request(e) => assert_eq!(e.kind(), kind),
            _ => panic!("expected request error"),
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn bad_error_bodies_preserve_status_without_disclosing_content() {
    for case in 0..4 {
        let server = TestServer::start(move |_: &Recorded| match case {
            0 => Reply::json(422, "{bad"),
            1 => Reply::json(422, "").with_body(Body::FailAfter(vec!["{\"ErrorCode\":".into()])),
            2 => Reply::json(422, vec![b'x'; ERROR_LIMIT + 1]),
            _ => Reply::text(422, "text/html", include_str!("../fixtures/error.json")),
        })
        .await;
        let error = client(&server)
            .outbound()
            .list(1, 0)
            .send()
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::Api { status, document: None, .. } if status.as_u16() == 422)
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn error_document_message_is_not_formatted() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(
            422,
            r#"{"ErrorCode":403,"Message":"synthetic-secret-token"}"#,
        )
    })
    .await;
    let error = client(&server)
        .outbound()
        .list(1, 0)
        .send()
        .await
        .unwrap_err();
    assert!(!format!("{error:?} {error}").contains("synthetic-secret-token"));
    server.shutdown().await;
}

#[tokio::test]
async fn retry_after_seconds_refuses_signed_repeated_and_overflowing_fields() {
    for (values, expected) in [
        (vec![" 7 "], Some(7)),
        (vec!["+7"], None),
        (vec!["7", "7"], None),
        (vec!["18446744073709551616"], None),
        (vec!["Sat, 07 Nov 2026 10:00:00 GMT"], None),
        (vec![], None),
    ] {
        let server = TestServer::start(move |_: &Recorded| {
            let mut reply = Reply::json(429, include_str!("../fixtures/error.json"));
            for value in &values {
                reply = reply.header("retry-after", *value);
            }
            reply
        })
        .await;
        let error = client(&server)
            .outbound()
            .list(1, 0)
            .send()
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::Api { retry_after_seconds, .. } if retry_after_seconds == expected)
        );
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn a_refused_connection_on_a_send_stays_an_unknown_outcome() {
    let reserved = tokio::net::TcpSocket::new_v4().unwrap();
    reserved.bind(([127, 0, 0, 1], 0).into()).unwrap();
    let origin = format!("http://{}", reserved.local_addr().unwrap());
    // Windows retries a refused connection for about two seconds before reporting it; a shorter
    // client timeout would turn the refusal into a timeout (native Windows CI, run 42338837366).
    let http = concord_sdk_postmark::http_client_builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap();
    let api = Postmark::new(
        http,
        PostmarkConfig {
            token: "synthetic-secret-token".into(),
        },
    )
    .with_base_url(origin)
    .unwrap();
    let error = api.outbound().send_batch(vec![]).send().await.unwrap_err();
    assert!(error.is_unknown_outcome());
    let Error::UnknownOutcome(error) = error else {
        panic!("{error:?}")
    };
    // Concord cannot prove a refused connection sent nothing (a redirect may follow an accepted send).
    assert_eq!(error.delivery(), concord::Delivery::Unknown);
    assert!(error.transport().is_some_and(|e| e.is_connect()));
    drop(reserved);
}
