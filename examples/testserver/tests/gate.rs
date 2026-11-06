//! `Reply::after(gate)`: a reply held until another request's responder opens the gate, with no
//! sleep deciding the order and no runtime thread blocked.

use std::time::Duration;

use concord_example_testserver::{Gate, Reply, TestServer};

#[tokio::test(flavor = "current_thread")]
async fn a_held_reply_waits_for_the_other_request() {
    let gate = Gate::new();
    let server_gate = gate.clone();
    let server = TestServer::start(move |r| {
        if r.target == "/first" {
            Reply::text(200, "text/plain", "first").after(server_gate.clone())
        } else {
            server_gate.open();
            Reply::text(200, "text/plain", "second")
        }
    })
    .await;
    let client = reqwest::Client::new();
    let first = tokio::spawn({
        let (client, url) = (client.clone(), format!("{}/first", server.url()));
        async move { client.get(url).send().await.unwrap().text().await.unwrap() }
    });
    assert!(
        server
            .wait_until(Duration::from_secs(2), |s| s.requests().len() == 1)
            .await
    );
    // The first request is in flight and its reply is held.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!first.is_finished());
    assert!(!gate.is_open());
    let second = client
        .get(format!("{}/second", server.url()))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(second, "second");
    assert_eq!(first.await.unwrap(), "first");
    assert!(gate.is_open());
}

#[tokio::test]
async fn an_opened_gate_releases_later_waiters_too() {
    let gate = Gate::new();
    gate.open();
    tokio::time::timeout(Duration::from_secs(1), gate.wait())
        .await
        .expect("an open gate does not wait");
}
