//! Batch sending: one result per message, and whole-batch unknown outcomes.

use concord::ErrorKind;
use concord_example_testserver::{Body, Recorded, Reply, TestServer, paused};
use concord_sdk_postmark::{BatchResult, Email, Error, Postmark, PostmarkConfig, RESPONSE_LIMIT};
use serde_json::{Value, json};
use std::time::Duration;

const TOKEN: &str = "synthetic-batch-token";
const BATCH: &str = include_str!("../fixtures/batch.json");
const ACCEPTED_ID: &str = "b7bc2f4a-e38e-4336-af7d-e6c392c2f817";
const REJECTED_TEXT: &str = "Found inactive addresses: example@example.com.";

fn email(n: usize) -> Email {
    Email {
        from: "sender@example.com".into(),
        to: format!("receiver{n}@example.com"),
        subject: format!("Postmark test #{n}"),
        text_body: Some(format!("Hello {n}")),
        html_body: None,
        message_stream: Some("outbound".into()),
    }
}

fn emails(count: usize) -> Vec<Email> {
    (1..=count).map(email).collect()
}

fn client(server: &TestServer, timeout: Duration) -> Postmark {
    let http = concord_sdk_postmark::http_client_builder()
        .timeout(timeout)
        .build()
        .unwrap();
    Postmark::new(
        http,
        PostmarkConfig {
            token: TOKEN.into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}

fn assert_batch_wire(request: &Recorded, count: usize) {
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/email/batch");
    assert_eq!(request.header_all("x-postmark-server-token"), [TOKEN]);
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.header("accept"), Some("application/json"));
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    let expected: Vec<Value> = (1..=count)
        .map(|n| {
            json!({"From": "sender@example.com", "To": format!("receiver{n}@example.com"),
                "Subject": format!("Postmark test #{n}"), "TextBody": format!("Hello {n}"),
                "MessageStream": "outbound"})
        })
        .collect();
    assert_eq!(body, Value::Array(expected));
}

#[tokio::test]
async fn mixed_batch_reports_each_message_in_submission_order() {
    let server = TestServer::start(|r: &Recorded| match (r.method.as_str(), r.path()) {
        ("POST", "/email/batch") => Reply::json(200, BATCH),
        _ => Reply::new(404),
    })
    .await;
    let results = client(&server, Duration::from_secs(2))
        .outbound()
        .send_batch(emails(2))
        .send()
        .await
        .unwrap();
    assert_eq!(results.len(), 2);
    match &results[0] {
        BatchResult::Accepted(acceptance) => {
            assert_eq!(acceptance.message_id, ACCEPTED_ID);
            assert_eq!(acceptance.to, "receiver1@example.com");
            assert_eq!(acceptance.submitted_at, "2010-11-26T12:01:05.1794748-05:00");
            assert_eq!(acceptance.error_code, 0);
            assert_eq!(acceptance.message, "OK");
        }
        other => panic!("expected acceptance, got {other:?}"),
    }
    match &results[1] {
        BatchResult::Rejected(rejection) => {
            assert_eq!(rejection.error_code, 406);
            assert!(rejection.message.contains(REJECTED_TEXT), "explicit access");
        }
        other => panic!("expected rejection, got {other:?}"),
    }
    let debug = format!("{results:?}");
    assert!(!debug.contains(REJECTED_TEXT), "{debug}");
    assert!(debug.contains("406") && debug.contains(ACCEPTED_ID));
    assert_batch_wire(&server.single(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);

    // A non-zero code is a rejection even when acceptance fields are present.
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(
            200,
            json!([{"ErrorCode": 300, "Message": REJECTED_TEXT, "MessageID": ACCEPTED_ID,
                "To": "receiver1@example.com", "SubmittedAt": "2010-11-26T12:01:05Z"}])
            .to_string(),
        )
    })
    .await;
    let results = client(&server, Duration::from_secs(2))
        .outbound()
        .send_batch(emails(1))
        .send()
        .await
        .unwrap();
    assert!(matches!(&results[..], [BatchResult::Rejected(r)] if r.error_code == 300));
    assert_batch_wire(&server.single(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn lost_or_unreadable_batch_response_is_unknown_and_never_resent() {
    for case in [
        "timeout",
        "stall",
        "disconnect",
        "malformed",
        "wrong-media",
        "unfinished-body",
        "oversized",
        "object",
        "accepted-without-id",
    ] {
        let server = TestServer::start(move |_: &Recorded| match case {
            "timeout" => Reply::json(200, BATCH).delay(Duration::from_secs(2)),
            "stall" => Reply::json(200, BATCH).with_body(Body::Stall),
            "disconnect" => Reply::hang_up(),
            "malformed" => Reply::json(200, &BATCH[..BATCH.len() / 2]),
            "wrong-media" => Reply::text(200, "text/html", BATCH),
            "unfinished-body" => Reply::json(200, "").with_body(Body::FailAfter(vec![
                BATCH.as_bytes()[..BATCH.len() / 2].to_vec().into(),
            ])),
            "oversized" => Reply::json(200, vec![b' '; RESPONSE_LIMIT + 1]),
            "object" => Reply::json(200, include_str!("../fixtures/acceptance.json")),
            _ => Reply::json(
                200,
                json!([{"ErrorCode": 0, "Message": "OK"},
                    {"ErrorCode": 406, "Message": REJECTED_TEXT}])
                .to_string(),
            ),
        })
        .await;
        let timeout = match case {
            "timeout" | "stall" => Duration::from_millis(300),
            _ => Duration::from_secs(5),
        };
        let postmark = client(&server, timeout);
        // Paused clock: the 300 ms timeout fires only once the server holds the batch (timeout)
        // or has written the head of its stalled answer (stall); in the other cases no timeout can
        // fire before the answer ends the send.
        tokio::time::pause();
        let send = postmark.outbound().send_batch(emails(2)).send();
        let error = match case {
            "timeout" => paused::time_out(&server, |_| true, timeout, send).await,
            "stall" => paused::time_out_in_body(&server, |_| true, timeout, send).await,
            _ => paused::drive(send).await,
        }
        .unwrap_err();
        tokio::time::resume();
        assert!(error.is_unknown_outcome(), "{case}: {error:?}");
        let text = format!("{error:?} {error}");
        assert!(
            !text.contains(TOKEN) && !text.contains(REJECTED_TEXT),
            "{case}: {text}"
        );
        match &error {
            Error::UnknownOutcome(cause) => {
                let expected = match case {
                    "timeout" | "stall" | "disconnect" | "unfinished-body" => ErrorKind::Transport,
                    "wrong-media" => ErrorKind::Metadata,
                    "oversized" => ErrorKind::Limit,
                    _ => ErrorKind::Decode,
                };
                assert_eq!(cause.kind(), expected, "{case}");
                assert_eq!(cause.endpoint(), Some("Postmark.outbound.send_batch"));
            }
            other => panic!("{case}: expected unknown outcome, got {other:?}"),
        }
        assert!(
            !server
                .wait_until(Duration::from_millis(200), |s| s.requests().len() > 1)
                .await,
            "{case}: batch resent"
        );
        assert_batch_wire(&server.single(), 2);
        assert_eq!(server.shutdown().await.open_after, 0, "{case}");
    }
}

#[tokio::test]
async fn result_count_unlike_submitted_count_is_unknown_for_the_whole_batch() {
    for returned in [0_usize, 1, 3] {
        let server = TestServer::start(move |_: &Recorded| {
            let mut results: Vec<Value> = serde_json::from_str(BATCH).unwrap();
            results.truncate(returned);
            while results.len() < returned {
                results.push(json!({"ErrorCode": 406, "Message": REJECTED_TEXT}));
            }
            Reply::json(200, Value::Array(results).to_string())
        })
        .await;
        let error = client(&server, Duration::from_secs(2))
            .outbound()
            .send_batch(emails(2))
            .send()
            .await
            .unwrap_err();
        assert!(error.is_unknown_outcome());
        assert_eq!(
            error.to_string(),
            format!(
                "Postmark batch outcome unknown: 2 emails submitted, {returned} results returned"
            )
        );
        assert!(!format!("{error:?}").contains(REJECTED_TEXT));
        match error {
            Error::UnmatchedBatch { submitted, results } => {
                assert_eq!(submitted, 2);
                assert_eq!(results.len(), returned);
                if returned > 0 {
                    // Kept for investigation, never for attribution.
                    assert!(
                        matches!(&results[0], BatchResult::Accepted(a) if a.message_id == ACCEPTED_ID)
                    );
                }
            }
            other => panic!("expected unmatched batch, got {other:?}"),
        }
        assert!(
            !server
                .wait_until(Duration::from_millis(100), |s| s.requests().len() > 1)
                .await
        );
        assert_batch_wire(&server.single(), 2);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn provider_batch_limits_and_rate_limits_stay_provider_errors() {
    // Postmark enforces 500 messages per call (ErrorCode 410, HTTP 422); the SDK sends the input.
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(
            422,
            json!({"ErrorCode": 410, "Message": REJECTED_TEXT}).to_string(),
        )
    })
    .await;
    let error = client(&server, Duration::from_secs(5))
        .outbound()
        .send_batch(emails(501))
        .send()
        .await
        .unwrap_err();
    assert!(!error.is_unknown_outcome());
    assert!(!format!("{error:?} {error}").contains(REJECTED_TEXT));
    assert!(matches!(
        error,
        Error::Api { status, document: Some(ref d), retry_after_seconds: None }
            if status.as_u16() == 422 && d.error_code == 410
    ));
    assert_batch_wire(&server.single(), 501);
    assert_eq!(server.shutdown().await.open_after, 0);

    let server = TestServer::start(|_: &Recorded| {
        Reply::json(429, include_str!("../fixtures/error.json")).header("retry-after", "7")
    })
    .await;
    let error = client(&server, Duration::from_secs(2))
        .outbound()
        .send_batch(emails(2))
        .send()
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Api { status, retry_after_seconds: Some(7), .. } if status.as_u16() == 429
    ));
    assert_batch_wire(&server.single(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}
