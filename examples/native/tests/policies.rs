//! U30 / U31 (R10–R12): application-owned cursor loop, bounded retry, admission and
//! caller-updated authorization, with zero Concord changes.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord::ErrorKind;
use concord_example_native::policies::{Session, collect_items, item_with_retry, items_admitted};
use support::{Reply, Server, inventory_at};

fn pages() -> impl Fn(&support::Recorded) -> Reply + Send + Sync + 'static {
    |r| match r.query() {
        Some(q) if q.ends_with("cursor=c2") => {
            Reply::json(200, r#"{"items":[{"id":3,"name":"c"}],"next":"c+3/x"}"#)
        }
        Some(q) if q.ends_with("cursor=c%2B3%2Fx") => {
            Reply::json(200, r#"{"items":[{"id":4,"name":"d"}],"next":null}"#)
        }
        _ => Reply::json(
            200,
            r#"{"items":[{"id":1,"name":"a"},{"id":2,"name":"b"}],"next":"c2"}"#,
        ),
    }
}

#[tokio::test]
async fn cursor_loop_follows_exact_query_strings() {
    let server = Server::start(pages()).await;
    let api = inventory_at(server.addr, "Bearer t");
    let collected = collect_items(&api, 10).await.unwrap();
    let ids: Vec<u64> = collected.items.iter().map(|i| i.id).collect();
    assert_eq!(ids, [1, 2, 3, 4]);
    assert_eq!(collected.next, None);
    let targets: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(
        targets,
        [
            "/v1/items?limit=2",
            "/v1/items?limit=2&cursor=c2",
            "/v1/items?limit=2&cursor=c%2B3%2Fx",
        ]
    );
}

#[tokio::test]
async fn cursor_loop_bound_returns_the_unused_cursor() {
    let server = Server::start(pages()).await;
    let api = inventory_at(server.addr, "Bearer t");
    let collected = collect_items(&api, 2).await.unwrap();
    assert_eq!(collected.items.len(), 3);
    assert_eq!(collected.next.as_deref(), Some("c+3/x"));
    assert_eq!(server.requests().len(), 2);
}

/// Hangs up on the first `failures` requests, then answers.
async fn flaky(failures: usize) -> Server {
    let seen = AtomicUsize::new(0);
    Server::start(move |_| {
        if seen.fetch_add(1, Ordering::SeqCst) < failures {
            Reply::hang_up()
        } else {
            Reply::json(200, r#"{"id":5,"name":"Pen"}"#)
        }
    })
    .await
}

#[tokio::test]
async fn bounded_retry_recovers_from_hang_ups() {
    let server = flaky(2).await;
    let api = inventory_at(server.addr, "Bearer t");
    let item = item_with_retry(&api, 5, 3).await.unwrap();
    assert_eq!(item.name, "Pen");
    assert_eq!(server.requests().len(), 3);
}

#[tokio::test]
async fn bounded_retry_stops_at_the_bound() {
    let server = flaky(5).await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = item_with_retry(&api, 5, 2).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn one_send_is_one_execute() {
    // R11: Concord itself never retries.
    let server = flaky(5).await;
    let api = inventory_at(server.addr, "Bearer t");
    assert_eq!(
        api.item(5).send().await.unwrap_err().kind(),
        ErrorKind::Transport
    );
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn status_errors_are_not_retried() {
    let server = Server::reply(Reply::new(503)).await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = item_with_retry(&api, 5, 3).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn timeouts_are_not_retried_by_this_predicate() {
    let server =
        Server::reply(Reply::json(200, r#"{"id":5,"name":"Pen"}"#).delay(Duration::from_secs(3)))
            .await;
    let http = concord::reqwest::Client::builder()
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();
    let api = concord_example_native::api::Inventory::new(
        http,
        concord_example_native::api::InventoryConfig {
            key: "Bearer t".into(),
        },
    )
    .with_base_url(format!("http://{}/v1", server.addr))
    .unwrap();
    let error = item_with_retry(&api, 5, 3).await.unwrap_err();
    assert!(error.transport().unwrap().is_timeout());
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn admission_bounds_requests_in_flight() {
    let server = Server::start(|r| {
        let id = r.path().rsplit('/').next().unwrap().to_owned();
        Reply::json(200, &format!(r#"{{"id":{id},"name":"n{id}"}}"#))
            .delay(Duration::from_millis(60))
    })
    .await;
    let api = inventory_at(server.addr, "Bearer t");
    let admission = Arc::new(tokio::sync::Semaphore::new(2));
    let ids: Vec<u64> = (1..=8).collect();
    let results = items_admitted(&api, &ids, Arc::clone(&admission)).await;
    let got: Vec<u64> = results.into_iter().map(|r| r.unwrap().id).collect();
    assert_eq!(got, ids, "results in input order");
    assert_eq!(server.requests().len(), 8);
    let peak = server.peak_in_flight();
    assert!((1..=2).contains(&peak), "peak {peak}");
    assert_eq!(admission.available_permits(), 2);

    // Control: with enough permits for everyone, the same fixture does see more than two.
    let wide = Arc::new(tokio::sync::Semaphore::new(8));
    let before = server.requests().len();
    for result in items_admitted(&api, &ids, wide).await {
        result.unwrap();
    }
    assert_eq!(server.requests().len(), before + 8);
    assert!(
        server.peak_in_flight() > 2,
        "control peak {}",
        server.peak_in_flight()
    );
}

#[tokio::test]
async fn building_requests_performs_no_io() {
    // R10: `prepare` builds a native request; nothing reaches the server until `send`.
    let server = Server::reply(Reply::json(200, r#"{"id":1,"name":"a"}"#)).await;
    let api = inventory_at(server.addr, "Bearer t");
    let prepared: Vec<_> = (0..5).map(|id| api.item(id).prepare().unwrap()).collect();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(server.connections(), 0);
    assert!(server.requests().is_empty());
    drop(prepared);
}

#[tokio::test]
async fn caller_updated_authorization_spares_existing_requests() {
    let server = Server::start(|r| match r.header("authorization") {
        Some("Bearer new") => Reply::json(200, r#"{"id":1,"name":"Pen"}"#),
        _ => Reply::json(401, r#"{"title":"token expired"}"#),
    })
    .await;
    let session = Session::new(inventory_at(server.addr, "Bearer old"));

    // Captured before the update: an endpoint request and a whole handle.
    let queued = session.api().item(1);
    let old_handle = session.api();

    let error = session.api().item(1).send().await.unwrap_err();
    assert_eq!(error.status().map(|s| s.as_u16()), Some(401));

    session.set_key("Bearer new"); // obtained by the application's own means
    assert_eq!(session.api().item(1).send().await.unwrap().name, "Pen");

    assert_eq!(
        queued
            .send()
            .await
            .unwrap_err()
            .status()
            .map(|s| s.as_u16()),
        Some(401)
    );
    assert_eq!(
        old_handle
            .item(1)
            .send()
            .await
            .unwrap_err()
            .status()
            .map(|s| s.as_u16()),
        Some(401)
    );

    let seen: Vec<String> = server
        .requests()
        .iter()
        .map(|r| r.header("authorization").unwrap().to_owned())
        .collect();
    assert_eq!(
        seen,
        ["Bearer old", "Bearer new", "Bearer old", "Bearer old"]
    );
    // The handle's Debug never shows the key.
    let shown = format!("{:?}", session.api());
    assert!(!shown.contains("Bearer new"), "{shown}");
}
