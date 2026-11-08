//! Between waves 2 and 3, candidate R: what a failed request proves about delivery. Baseline:
//! Discord and Zendesk classify `transport().is_connect()` as "not sent", Postmark, HubSpot and
//! Mailgun treat every failed write as uncertain (HubSpot also its build errors), each by hand.
//! A connect error is not proof of non-delivery (redirects, nested sources), so `delivery()`
//! claims `NotSent` only for requests Concord refused before sending.

use std::time::Duration;

use concord::{Delivery, ErrorKind, Json};
use concord_example_testserver::{Body, Reply, TestServer, paused};
use serde::{Deserialize, Serialize};

/// A created item.
#[derive(Debug, Deserialize, Serialize)]
pub struct Item {
    /// Its id.
    pub id: u32,
}

concord::api! {
    /// A store accepting writes.
    pub client Store() {
        base "http://store.invalid";
        format Json;
        /// Creates an item.
        POST create(header("x-note") note: String, body item: Item) -> Item { path "items"; }
    }
}

fn store(base: &str, timeout: Duration) -> Store {
    let http = concord::reqwest::Client::builder()
        .timeout(timeout)
        .retry(concord::reqwest::retry::never())
        .build()
        .unwrap();
    Store::new(http).with_base_url(base).unwrap()
}

async fn delivery(store: &Store) -> (Delivery, ErrorKind) {
    let error = store.create("n", Item { id: 1 }).send().await.unwrap_err();
    (error.delivery(), error.kind())
}

#[tokio::test]
async fn only_a_request_concord_refused_is_proven_not_sent() {
    // Not built: a header value Concord refuses before any I/O.
    let error = store("http://127.0.0.1:9", Duration::from_secs(5))
        .create("a\nb", Item { id: 1 })
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        (error.delivery(), error.kind()),
        (Delivery::NotSent, ErrorKind::Build)
    );

    // A refused connection is not proof: below, the same final error follows a performed write.
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let closed = format!("http://{}", socket.local_addr().unwrap());
    assert_eq!(
        delivery(&store(&closed, Duration::from_secs(5))).await,
        (Delivery::Unknown, ErrorKind::Transport)
    );

    // Review b23-review-sol, finding 1: the server performs the POST and answers 303 to a closed
    // port; reqwest follows it and returns the connect error. Claiming `NotSent` would be wrong.
    let target = format!("{closed}/elsewhere");
    let server =
        TestServer::start(move |_| Reply::new(303).header("location", target.clone())).await;
    let error = store(&server.url(), Duration::from_secs(5))
        .create("n", Item { id: 1 })
        .send()
        .await
        .unwrap_err();
    assert!(error.transport().is_some_and(|e| e.is_connect()));
    assert_eq!(error.delivery(), Delivery::Unknown);
    assert_eq!(server.requests().len(), 1, "the write reached the server");
    drop(socket);
}

#[tokio::test]
async fn a_request_without_an_answer_may_have_been_performed() {
    // The server reads the whole request, then answers too late.
    let server =
        TestServer::start(|_| Reply::json(201, r#"{"id":1}"#).delay(Duration::from_secs(5))).await;
    let timeout = Duration::from_millis(300);
    let late = store(&server.url(), timeout);
    // Paused clock: the 300 ms timeout fires only once the server has read the whole request.
    tokio::time::pause();
    let outcome = paused::time_out(&server, |_| true, timeout, delivery(&late)).await;
    tokio::time::resume();
    assert_eq!(outcome, (Delivery::Unknown, ErrorKind::Transport));
    assert_eq!(server.requests().len(), 1, "the write reached the server");

    // The server reads the request and closes the connection without a response.
    let server = TestServer::start(|_| Reply::hang_up()).await;
    assert_eq!(
        delivery(&store(&server.url(), Duration::from_secs(5))).await,
        (Delivery::Unknown, ErrorKind::Transport)
    );
}

#[tokio::test]
async fn any_response_head_is_an_answer_even_when_its_body_fails() {
    // `Reply` is not `Clone`: each case is a constructor.
    let cases: [(fn() -> Reply, ErrorKind); 4] = [
        (|| Reply::json(500, r#"{"error":"x"}"#), ErrorKind::Status),
        (|| Reply::text(201, "text/html", "<p>"), ErrorKind::Metadata),
        (|| Reply::json(201, "{"), ErrorKind::Decode),
        (
            || {
                Reply::new(201)
                    .header("content-type", "application/json")
                    .with_body(Body::FailAfter(vec![r#"{"id""#.into()]))
            },
            ErrorKind::Transport,
        ),
    ];
    for (reply, kind) in cases {
        let server = TestServer::start(move |_| reply()).await;
        let (delivery, observed) = delivery(&store(&server.url(), Duration::from_secs(5))).await;
        assert_eq!((delivery, observed), (Delivery::Answered, kind));
    }
}

// Review b23-review3-sol, finding 1: with a client that follows redirects, a status answers a later
// request. The POST is performed and answered 303; the target answers 429. The policy sees 429 for
// the target's URL: "refused" would be wrong. With redirects off it sees the 303 itself.
#[tokio::test]
async fn a_followed_redirect_answers_for_a_later_request() {
    let server = TestServer::start(|r| {
        if r.method == "POST" {
            Reply::new(303).header("location", "/later")
        } else {
            Reply::json(429, "{}")
        }
    })
    .await;
    let prepared = store(&server.url(), Duration::from_secs(5))
        .create("n", Item { id: 1 })
        .prepare()
        .unwrap();
    let sent = prepared.request().url().clone();
    let received = prepared.receive().await.unwrap();
    assert_eq!(received.status(), 429);
    assert_ne!(received.url(), &sent, "the answer is the redirect target's");
    let methods: Vec<String> = server.requests().iter().map(|r| r.method.clone()).collect();
    assert_eq!(methods, ["POST", "GET"], "the write was performed first");

    let http = concord::reqwest::Client::builder()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .build()
        .unwrap();
    let store = Store::new(http).with_base_url(server.url()).unwrap();
    let received = store
        .create("n", Item { id: 1 })
        .prepare()
        .unwrap()
        .receive()
        .await
        .unwrap();
    assert_eq!(received.status(), 303);
    server.shutdown().await;
}
