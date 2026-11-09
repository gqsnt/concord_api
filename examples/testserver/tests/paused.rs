//! The paused-clock helpers: a short client timeout fires only where the test puts it.

use std::time::Duration;

use concord_example_testserver::paused::{drive, time_out, time_out_in_body};
use concord_example_testserver::{Body, Reply, TestServer};

const TIMEOUT: Duration = Duration::from_millis(50);

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(TIMEOUT)
        .build()
        .unwrap()
}

/// A token-like request answered at once and the late request share one 50 ms client timeout:
/// the first completes in frozen time, the second times out only after the server received it.
#[tokio::test(start_paused = true)]
async fn only_the_late_request_times_out_and_only_after_it_arrived() {
    let server = TestServer::start(|r| match r.path() {
        "/late" => Reply::new(200).delay(Duration::from_secs(5)),
        _ => Reply::text(200, "text/plain", "token"),
    })
    .await;
    let http = client();
    let start = tokio::time::Instant::now();
    let call = async {
        let token = http.get(format!("{}/token", server.url())).send().await;
        let late = http.get(format!("{}/late", server.url())).send().await;
        (token, late)
    };
    let (token, late) = time_out(&server, |r| r.path() == "/late", TIMEOUT, call).await;
    assert_eq!(token.unwrap().status(), 200);
    assert!(late.unwrap_err().is_timeout());
    let paths: Vec<_> = server
        .requests()
        .iter()
        .map(|r| r.path().to_owned())
        .collect();
    assert_eq!(paths, ["/token", "/late"]);
    // Virtual time moved once, just past the timeout.
    assert!(start.elapsed() > TIMEOUT && start.elapsed() < 2 * TIMEOUT);
    // A follow-up call completes while time stands still.
    let again = drive(http.get(format!("{}/token", server.url())).send()).await;
    assert_eq!(again.unwrap().status(), 200);
    server.shutdown().await;
}

/// The head arrives, the body stalls: the timeout ends the body, never the wait for the head.
#[tokio::test(start_paused = true)]
async fn a_stalled_body_times_out_after_the_head_arrived() {
    let server = TestServer::start(|_| Reply::new(200).with_body(Body::Stall)).await;
    let http = client();
    let call = async {
        let response = match http.get(server.url()).send().await {
            Ok(response) => response,
            Err(error) => return Err(format!("no head: {error:?}")),
        };
        let status = response.status();
        match response.bytes().await {
            Ok(_) => Err("the stalled body completed".to_owned()),
            Err(error) => Ok((status, error)),
        }
    };
    let (status, error) = time_out_in_body(&server, |_| true, TIMEOUT, call)
        .await
        .unwrap();
    assert_eq!(status, 200);
    assert!(error.is_timeout(), "{error:?}");
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

/// A reply the server does not hold is never timed out: the helper only moves time while the
/// server holds the late request's answer.
#[tokio::test(start_paused = true)]
async fn a_reply_the_server_answers_at_once_is_not_timed_out() {
    let server = TestServer::start(|_| Reply::text(200, "text/plain", "at once")).await;
    let http = client();
    let response = time_out(&server, |_| true, TIMEOUT, http.get(server.url()).send()).await;
    assert_eq!(response.unwrap().status(), 200);
    assert_eq!(server.held_replies(), 0);
    server.shutdown().await;
}
