//! Fresh-consumer workflows. Every request uses a loopback fixture and a fake token.

use concord::ErrorKind;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_postmark::{
    BatchResult, Email, Error, Message, Postmark, PostmarkConfig, http_client_builder,
};
use serde_json::{Value, json};
use std::time::Duration;

const TOKEN: &str = "fixture-server-token-not-a-real-credential";
const ID: &str = "a63a9307-e9b7-4bd4-923b-7cfe98f2c301";
const SECOND_ID: &str = "b74b0418-fac8-4ce5-a34c-8daf09a3d412";
const THIRD_ID: &str = "c85c1529-abd9-4df6-b45d-9ebf10b4e523";
const SUBMITTED: &str = "2026-11-07T09:15:30.1234567Z";
const PRIVATE_DETAIL: &str = "fixture-private-provider-detail alex@example.net";

fn client(server: &TestServer, timeout: Duration) -> Postmark {
    let http = http_client_builder()
        .no_proxy()
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

fn receipt() -> Email {
    Email {
        from: "Northwind Receipts <receipts@example.com>".into(),
        to: "alex@example.net".into(),
        subject: "Receipt for order NW-1042".into(),
        text_body: Some("Thank you. Your order NW-1042 totals EUR 42.50.".into()),
        html_body: None,
        message_stream: Some("outbound".into()),
    }
}

fn acceptance() -> Value {
    json!({"To": "alex@example.net", "SubmittedAt": SUBMITTED,
        "MessageID": ID, "ErrorCode": 0, "Message": "OK"})
}

fn message(id: &str, status: &str) -> Value {
    json!({"MessageID": id, "Status": status, "MessageStream": "outbound",
        "From": "Northwind Receipts <receipts@example.com>",
        "To": [{"Email": "alex@example.net", "Name": "Alex"}],
        "Recipients": ["alex@example.net"], "Subject": "Receipt for order NW-1042",
        "ReceivedAt": SUBMITTED, "Metadata": {"order": "NW-1042"},
        "MessageEvents": [], "Sandboxed": true})
}

fn json_reply(value: Value) -> Reply {
    Reply::json(200, value.to_string()).header("X-Fixture-Trace", "postmark-first-use")
}

fn assert_common_wire(request: &Recorded) {
    assert_eq!(request.header_all("X-Postmark-Server-Token"), [TOKEN]);
    assert_eq!(request.header("Accept"), Some("application/json"));
    assert_eq!(request.header("Authorization"), None);
    assert!(request.host.starts_with("127.0.0.1:"));
    assert!(!request.target.contains(TOKEN));
}

fn assert_send_wire(request: &Recorded) {
    assert_common_wire(request);
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/email");
    assert_eq!(request.header("Content-Type"), Some("application/json"));
    assert_eq!(
        serde_json::from_slice::<Value>(&request.body).unwrap(),
        json!({
            "From": "Northwind Receipts <receipts@example.com>", "To": "alex@example.net",
            "Subject": "Receipt for order NW-1042",
            "TextBody": "Thank you. Your order NW-1042 totals EUR 42.50.",
            "MessageStream": "outbound"
        })
    );
}

fn assert_read_wire(request: &Recorded, path: &str) {
    assert_common_wire(request);
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), path);
    assert!(request.body.is_empty());
    assert_eq!(request.header("Content-Type"), None);
}

fn assert_redacted(value: &str) {
    assert!(!value.contains(TOKEN));
    assert!(!value.contains(PRIVATE_DETAIL));
}

fn provider_error(status: u16, code: u32) -> Reply {
    Reply::json(
        status,
        json!({"ErrorCode": code,
        "Message": format!("{PRIVATE_DETAIL} {TOKEN}")})
        .to_string(),
    )
}

fn assert_api_error(error: &Error, expected_status: u16, code: Option<u32>, retry: Option<u64>) {
    assert_redacted(&format!("{error:?} {error}"));
    match error {
        Error::Api {
            status,
            document,
            retry_after_seconds,
        } => {
            assert_eq!(status.as_u16(), expected_status);
            assert_eq!(*retry_after_seconds, retry);
            assert_eq!(document.as_ref().map(|doc| doc.error_code), code);
            if let Some(document) = document {
                assert_redacted(&format!("{document:?}"));
                assert!(document.message.contains(PRIVATE_DETAIL));
                assert!(
                    document.message.contains(TOKEN),
                    "explicit access is unredacted"
                );
            }
        }
        other => panic!("expected provider error, got {other:?}"),
    }
}

// The application owns pagination and its partial-result report; the SDK fetches one page.
#[derive(Debug)]
enum ListingStop {
    TotalReached,
    EmptyPage,
    Failed(Error),
}

#[derive(Debug)]
struct Listing {
    messages: Vec<Message>,
    total_count: Option<u64>,
    stop: ListingStop,
}

async fn collect_outbound(api: &Postmark) -> Listing {
    let outbound = api.outbound();
    let mut messages = Vec::new();
    let mut total_count = None;
    let mut offset = 0_u32;
    let stop = loop {
        match outbound.list(2, offset).send().await {
            Err(error) => break ListingStop::Failed(error),
            Ok(page) => {
                total_count = Some(page.total_count);
                if page.messages.is_empty() {
                    break ListingStop::EmptyPage;
                }
                offset += u32::try_from(page.messages.len()).unwrap();
                messages.extend(page.messages);
                if u64::from(offset) >= page.total_count {
                    break ListingStop::TotalReached;
                }
            }
        }
    };
    Listing {
        messages,
        total_count,
        stop,
    }
}

#[tokio::test]
async fn send_transactional_email_then_inspect_returned_id() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match (request.method.as_str(), request.path()) {
            ("POST", "/email") => json_reply(acceptance()),
            ("GET", path) if path == format!("/messages/outbound/{ID}/details") => {
                json_reply(message(ID, "Queued"))
            }
            _ => Reply::new(500),
        }
    })
    .await;
    let api = client(&server, Duration::from_secs(2));
    let outbound = api.outbound();
    assert_redacted(&format!(
        "{api:?} {:?} {:?} {:?}",
        PostmarkConfig {
            token: TOKEN.into()
        },
        outbound,
        outbound.send_email(receipt())
    ));
    let accepted = outbound.send_email(receipt()).send().await.unwrap();
    assert_eq!(accepted.message_id, ID);
    assert_eq!(accepted.to, "alex@example.net");
    assert_eq!(accepted.submitted_at, SUBMITTED);
    assert_eq!(accepted.error_code, 0);
    assert_eq!(accepted.message, "OK");
    let details = outbound.details(&accepted.message_id).send().await.unwrap();
    assert_eq!(details.message_id, accepted.message_id);
    assert_eq!(details.status, "Queued");
    assert_eq!(details.fields["MessageEvents"], json!([]));
    assert_eq!(details.fields["Metadata"]["order"], "NW-1042");
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_send_wire(&requests[0]);
    assert_read_wire(&requests[1], &format!("/messages/outbound/{ID}/details"));
    assert_eq!(requests[1].query(), None);
    assert_eq!(server.shutdown().await.open_after, 0);

    // Receipt accepted, but inspection fails: preserve the ID instead of sending again.
    let server = TestServer::start(|request: &Recorded| -> Reply {
        if request.path() == "/email" {
            json_reply(acceptance())
        } else {
            provider_error(422, 701)
        }
    })
    .await;
    let api = client(&server, Duration::from_secs(2));
    let outbound = api.outbound();
    let accepted = outbound.send_email(receipt()).send().await.unwrap();
    let inspection = outbound
        .details(&accepted.message_id)
        .send()
        .await
        .unwrap_err();
    assert_api_error(&inspection, 422, Some(701), None);
    assert_eq!(accepted.message_id, ID, "partial result retained");
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.method == "POST")
            .count(),
        1
    );
    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);

    // Required failure: server read the entire email and sent headers, then stalled.
    let server = TestServer::start(|_: &Recorded| -> Reply {
        json_reply(acceptance()).with_body(Body::Stall)
    })
    .await;
    let api = client(&server, Duration::from_millis(150));
    let error = api
        .outbound()
        .send_email(receipt())
        .send()
        .await
        .unwrap_err();
    assert_redacted(&format!("{error:?} {error}"));
    match error {
        Error::UnknownOutcome(cause) => {
            assert_eq!(cause.kind(), ErrorKind::Transport);
            assert!(cause.transport().unwrap().is_timeout());
            assert_eq!(cause.endpoint(), Some("Postmark.outbound.send_email"));
            let head = cause.head().unwrap();
            assert_eq!(head.status().as_u16(), 200);
            assert_eq!(head.headers()["x-fixture-trace"], "postmark-first-use");
        }
        other => panic!("expected unknown send outcome, got {other:?}"),
    }
    assert!(
        !server
            .wait_until(Duration::from_millis(200), |s| s.requests().len() > 1)
            .await
    );
    assert_send_wire(&server.single());
    assert_eq!(server.shutdown().await.open_after, 0);

    for case in ["malformed", "wrong-media", "disconnect", "unfinished-body"] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            match case {
                "malformed" => Reply::json(200, "{broken"),
                "wrong-media" => Reply::text(200, "text/html", "<html>proxy</html>"),
                "disconnect" => Reply::hang_up(),
                _ => json_reply(acceptance())
                    .with_body(Body::FailAfter(vec![acceptance().to_string().into()])),
            }
        })
        .await;
        let error = client(&server, Duration::from_secs(2))
            .outbound()
            .send_email(receipt())
            .send()
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::UnknownOutcome(_)),
            "{case}: {error:?}"
        );
        assert_redacted(&format!("{error:?} {error}"));
        assert_send_wire(&server.single());
        assert_eq!(server.shutdown().await.open_after, 0);
    }

    for (status, code) in [(401, 10), (422, 400), (429, 429), (503, 100)] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            provider_error(status, code).header("Retry-After", "7")
        })
        .await;
        let error = client(&server, Duration::from_secs(2))
            .outbound()
            .send_email(receipt())
            .send()
            .await
            .unwrap_err();
        assert_api_error(&error, status, Some(code), Some(7));
        assert_send_wire(&server.single());
        assert_eq!(server.shutdown().await.open_after, 0);
    }

    // An invalid secret header fails before sending, so it is not an unknown outcome.
    let server = TestServer::start(|_: &Recorded| -> Reply { Reply::new(500) }).await;
    let api = client(&server, Duration::from_secs(2)).set_token(format!("{TOKEN}\ninvalid"));
    let error = api
        .outbound()
        .send_email(receipt())
        .send()
        .await
        .unwrap_err();
    assert_redacted(&format!("{error:?} {error}"));
    assert!(matches!(error, Error::Request(ref cause) if cause.kind() == ErrorKind::Build));
    assert!(server.requests().is_empty());
    assert_eq!(server.shutdown().await.open_after, 0);

    // Adversarial response, not a documented successful Postmark acceptance.
    let server = TestServer::start(|_: &Recorded| -> Reply {
        let mut body = acceptance();
        body["ErrorCode"] = json!(400);
        body["Message"] = json!("Synthetic provider failure in a 2xx envelope");
        json_reply(body)
    })
    .await;
    let accepted = client(&server, Duration::from_secs(2))
        .outbound()
        .send_email(receipt())
        .send()
        .await
        .unwrap();
    assert_eq!(
        accepted.error_code, 400,
        "caller sees provider code unchanged"
    );
    assert_send_wire(&server.single());
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn list_outbound_by_count_and_offset_then_retrieve_an_item() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match request.path() {
            "/messages/outbound" => match request.query_all("offset").as_slice() {
                [offset] if offset == "0" => json_reply(json!({"TotalCount": 3,
                    "Messages": [message(ID, "Queued"), message(SECOND_ID, "Sent")]})),
                [offset] if offset == "2" => json_reply(json!({"TotalCount": 3,
                    "Messages": [message(THIRD_ID, "Sent")]})),
                _ => Reply::new(500),
            },
            path if path == format!("/messages/outbound/{ID}/details") => {
                json_reply(message(ID, "Queued"))
            }
            _ => Reply::new(500),
        }
    })
    .await;
    let api = client(&server, Duration::from_secs(2));
    let outbound = api.outbound();
    let first = outbound.list(2, 0).send().await.unwrap();
    assert_eq!(first.total_count, 3);
    assert_eq!(first.messages.len(), 2);
    assert_eq!(server.requests().len(), 1, "list fetches one page only");
    let second = outbound.list(2, 2).send().await.unwrap();
    let mut messages = first.messages;
    messages.extend(second.messages);
    assert_eq!(
        messages.len() as u64,
        second.total_count,
        "stable fixture complete"
    );
    assert_eq!(
        messages
            .iter()
            .map(|m| m.message_id.as_str())
            .collect::<Vec<_>>(),
        [ID, SECOND_ID, THIRD_ID]
    );
    let details = outbound
        .details(&messages[0].message_id)
        .send()
        .await
        .unwrap();
    assert_eq!(details.message_id, messages[0].message_id);
    assert_eq!(details.fields["Metadata"]["order"], "NW-1042");
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for (request, offset) in requests[..2].iter().zip(["0", "2"]) {
        assert_read_wire(request, "/messages/outbound");
        let mut query = request.query_pairs();
        query.sort();
        assert_eq!(
            query,
            [
                ("count".into(), "2".into()),
                ("offset".into(), offset.into())
            ]
        );
    }
    assert_read_wire(&requests[2], &format!("/messages/outbound/{ID}/details"));
    assert_eq!(requests[2].query(), None);
    assert_eq!(server.shutdown().await.open_after, 0);

    // Use a small application-owned report for completion and interrupted traversal.
    for case in [
        "complete",
        "empty",
        "rate-limited",
        "malformed",
        "detail-fails",
    ] {
        let server = TestServer::start(move |request: &Recorded| -> Reply {
            if request.path() == format!("/messages/outbound/{ID}/details") {
                if case == "detail-fails" {
                    return provider_error(422, 701);
                }
                return json_reply(message(ID, "Queued"));
            }
            if request.query_all("offset") == ["0"] {
                return json_reply(json!({"TotalCount": 3,
                    "Messages": [message(ID, "Queued"), message(SECOND_ID, "Sent")]}));
            }
            match case {
                "complete" | "detail-fails" => json_reply(json!({"TotalCount": 3,
                    "Messages": [message(THIRD_ID, "Sent")]})),
                "empty" => json_reply(json!({"TotalCount": 3, "Messages": []})),
                "rate-limited" => provider_error(429, 429).header("Retry-After", "3"),
                _ => Reply::json(
                    200,
                    format!(
                        "{{\"TotalCount\":3,\"Messages\":[{},",
                        message(THIRD_ID, "Sent")
                    ),
                )
                .header("X-Fixture-Trace", "malformed-page"),
            }
        })
        .await;
        let api = client(&server, Duration::from_secs(2));
        let listing = collect_outbound(&api).await;
        assert_eq!(listing.total_count, Some(3));
        assert_eq!(
            listing.messages.len(),
            if matches!(case, "complete" | "detail-fails") {
                3
            } else {
                2
            }
        );
        assert_eq!(listing.messages[0].message_id, ID);
        match (&listing.stop, case) {
            (ListingStop::TotalReached, "complete" | "detail-fails")
            | (ListingStop::EmptyPage, "empty") => {}
            (ListingStop::Failed(error), "rate-limited") => {
                assert_api_error(error, 429, Some(429), Some(3));
            }
            (ListingStop::Failed(Error::Request(cause)), "malformed") => {
                assert_eq!(cause.kind(), ErrorKind::Decode);
                assert_eq!(cause.head().unwrap().status().as_u16(), 200);
                assert_eq!(
                    cause.head().unwrap().headers()["x-fixture-trace"],
                    "malformed-page"
                );
            }
            _ => panic!("unexpected completion for {case}: {:?}", listing.stop),
        }
        // A retained item is still usable, even if a later page failed.
        let details = api
            .outbound()
            .details(&listing.messages[0].message_id)
            .send()
            .await;
        if case == "detail-fails" {
            assert_api_error(&details.unwrap_err(), 422, Some(701), None);
            assert_eq!(
                listing.messages.len(),
                3,
                "list survives failed item inspection"
            );
            assert_eq!(listing.messages[0].message_id, ID);
        } else {
            assert_eq!(details.unwrap().message_id, ID);
        }
        let requests = server.requests();
        assert_eq!(requests.len(), 3, "no pagination past the observed stop");
        for (request, offset) in requests[..2].iter().zip(["0", "2"]) {
            assert_read_wire(request, "/messages/outbound");
            assert_eq!(request.query_all("count"), ["2"]);
            assert_eq!(request.query_all("offset"), [offset]);
        }
        assert_read_wire(&requests[2], &format!("/messages/outbound/{ID}/details"));
        assert_eq!(server.shutdown().await.open_after, 0);
    }

    // Initially empty search does not invent an item to retrieve.
    let server = TestServer::start(|_: &Recorded| -> Reply {
        json_reply(json!({"TotalCount": 0, "Messages": []}))
    })
    .await;
    let listing = collect_outbound(&client(&server, Duration::from_secs(2))).await;
    assert!(matches!(listing.stop, ListingStop::EmptyPage));
    assert_eq!(listing.total_count, Some(0));
    assert!(listing.messages.is_empty());
    assert_read_wire(&server.single(), "/messages/outbound");
    assert_eq!(server.shutdown().await.open_after, 0);

    // Page bounds remain provider validation; integers are sent unchanged.
    let server = TestServer::start(|_: &Recorded| -> Reply { provider_error(422, 700) }).await;
    let error = client(&server, Duration::from_secs(2))
        .outbound()
        .list(501, 9_900)
        .send()
        .await
        .unwrap_err();
    assert_api_error(&error, 422, Some(700), None);
    let request = server.single();
    assert_read_wire(&request, "/messages/outbound");
    assert_eq!(request.query_all("count"), ["501"]);
    assert_eq!(request.query_all("offset"), ["9900"]);
    assert_eq!(server.shutdown().await.open_after, 0);

    for case in ["wrong-media", "malformed-error", "html-error", "http-date"] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            match case {
                "wrong-media" => Reply::text(200, "text/html", "<html>proxy</html>"),
                "malformed-error" => Reply::json(502, "{broken"),
                "html-error" => Reply::text(502, "text/html", "<html>proxy error</html>"),
                _ => {
                    provider_error(429, 429).header("Retry-After", "Sat, 07 Nov 2026 10:00:00 GMT")
                }
            }
        })
        .await;
        let error = client(&server, Duration::from_secs(2))
            .outbound()
            .list(2, 0)
            .send()
            .await
            .unwrap_err();
        if case == "wrong-media" {
            assert!(
                matches!(error, Error::Request(ref cause) if cause.kind() == ErrorKind::Metadata)
            );
        } else {
            assert_api_error(
                &error,
                if case == "http-date" { 429 } else { 502 },
                if case == "http-date" { Some(429) } else { None },
                None,
            );
        }
        assert_read_wire(&server.single(), "/messages/outbound");
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

// Maintenance (batch sending): the application keeps one outcome per receipt, in its own order.
#[derive(Debug, PartialEq)]
enum ReceiptOutcome {
    Accepted(String),
    Rejected(u32),
    // Any receipt of the batch may have been accepted: never resend it automatically.
    Unknown,
}

fn order_receipt(order: &str, to: &str) -> Email {
    Email {
        to: to.into(),
        subject: format!("Receipt for order {order}"),
        text_body: Some(format!("Thank you. Your order {order} is confirmed.")),
        ..receipt()
    }
}

async fn send_receipts(api: &Postmark, receipts: Vec<Email>) -> Result<Vec<ReceiptOutcome>, Error> {
    let submitted = receipts.len();
    match api.outbound().send_batch(receipts).send().await {
        Ok(results) => {
            assert_redacted(&format!("{results:?}"));
            Ok(results
                .into_iter()
                .map(|result| match result {
                    BatchResult::Accepted(acceptance) => {
                        ReceiptOutcome::Accepted(acceptance.message_id)
                    }
                    BatchResult::Rejected(rejection) => {
                        ReceiptOutcome::Rejected(rejection.error_code)
                    }
                })
                .collect())
        }
        Err(error) if error.is_unknown_outcome() => {
            assert_redacted(&format!("{error:?} {error}"));
            Ok((0..submitted).map(|_| ReceiptOutcome::Unknown).collect())
        }
        // The batch as a whole was refused (or never sent): the application may try again later.
        Err(error) => Err(error),
    }
}

#[tokio::test]
async fn send_receipts_in_one_batch_then_inspect_each_accepted_id() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match (request.method.as_str(), request.path()) {
            ("POST", "/email/batch") => json_reply(json!([
                acceptance(),
                {"ErrorCode": 406, "Message": format!("{PRIVATE_DETAIL} {TOKEN}")},
                {"To": "sam@example.net", "SubmittedAt": SUBMITTED,
                    "MessageID": SECOND_ID, "ErrorCode": 0, "Message": "OK"},
            ])),
            ("GET", path) if path == format!("/messages/outbound/{ID}/details") => {
                json_reply(message(ID, "Queued"))
            }
            ("GET", path) if path == format!("/messages/outbound/{SECOND_ID}/details") => {
                json_reply(message(SECOND_ID, "Sent"))
            }
            _ => Reply::new(500),
        }
    })
    .await;
    let api = client(&server, Duration::from_secs(2));
    let receipts = vec![
        receipt(),
        order_receipt("NW-1043", "inactive@example.net"),
        order_receipt("NW-1044", "sam@example.net"),
    ];
    let outcomes = send_receipts(&api, receipts).await.unwrap();
    assert_eq!(
        outcomes,
        [
            ReceiptOutcome::Accepted(ID.into()),
            ReceiptOutcome::Rejected(406),
            ReceiptOutcome::Accepted(SECOND_ID.into()),
        ]
    );
    for (outcome, status) in [(&outcomes[0], "Queued"), (&outcomes[2], "Sent")] {
        let ReceiptOutcome::Accepted(id) = outcome else {
            unreachable!()
        };
        let details = api.outbound().details(id).send().await.unwrap();
        assert_eq!(&details.message_id, id);
        assert_eq!(details.status, status);
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_common_wire(&requests[0]);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].target, "/email/batch");
    assert_eq!(requests[0].header("Content-Type"), Some("application/json"));
    let body: Vec<Value> = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body.len(), 3);
    assert_eq!(
        body[0],
        json!({
            "From": "Northwind Receipts <receipts@example.com>", "To": "alex@example.net",
            "Subject": "Receipt for order NW-1042",
            "TextBody": "Thank you. Your order NW-1042 totals EUR 42.50.",
            "MessageStream": "outbound"
        })
    );
    assert_eq!(body[1]["To"], "inactive@example.net");
    assert_eq!(body[2]["Subject"], "Receipt for order NW-1044");
    assert_read_wire(&requests[1], &format!("/messages/outbound/{ID}/details"));
    assert_read_wire(
        &requests[2],
        &format!("/messages/outbound/{SECOND_ID}/details"),
    );
    assert_eq!(server.shutdown().await.open_after, 0);

    // Required failure, batch form: the batch was read and headers sent, then the body stalled.
    // Also a 2xx answer that cannot be attributed: one result for two receipts.
    for stall in [true, false] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            let reply = json_reply(json!([acceptance()]));
            if stall {
                reply.with_body(Body::Stall)
            } else {
                reply
            }
        })
        .await;
        let api = client(&server, Duration::from_millis(150));
        let receipts = vec![receipt(), order_receipt("NW-1043", "sam@example.net")];
        let outcomes = send_receipts(&api, receipts).await.unwrap();
        assert_eq!(outcomes, [ReceiptOutcome::Unknown, ReceiptOutcome::Unknown]);
        assert!(
            !server
                .wait_until(Duration::from_millis(200), |s| s.requests().len() > 1)
                .await,
            "batch never resent"
        );
        let request = server.single();
        assert_common_wire(&request);
        assert_eq!(request.target, "/email/batch");
        assert_eq!(server.shutdown().await.open_after, 0);
    }

    // A refused batch is a provider error for the whole request, not a per-receipt outcome.
    let server = TestServer::start(|_: &Recorded| -> Reply {
        provider_error(429, 429).header("Retry-After", "7")
    })
    .await;
    let error = send_receipts(&client(&server, Duration::from_secs(2)), vec![receipt()])
        .await
        .unwrap_err();
    assert_api_error(&error, 429, Some(429), Some(7));
    assert_eq!(server.single().target, "/email/batch");
    assert_eq!(server.shutdown().await.open_after, 0);
}
