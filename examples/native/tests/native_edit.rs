//! U15 / U32 (R07): native request edits finished by the declared decoder.

mod support;

use std::time::Duration;

use concord::ErrorKind;
use concord::reqwest::header::{HeaderName, HeaderValue};
use concord_example_native::native_edit::{Filter, item_with_timeout, search_filters};
use support::{Reply, Server, inventory_at};

fn request_id(value: &'static str) -> (HeaderName, HeaderValue) {
    (
        HeaderName::from_static("x-request-id"),
        HeaderValue::from_static(value),
    )
}

#[tokio::test]
async fn extra_header_arrives_and_declared_decoding_applies() {
    let server = Server::reply(Reply::json(200, r#"{"id":3,"name":"Cup"}"#)).await;
    let api = inventory_at(server.addr, "Bearer t");
    let item = item_with_timeout(&api, 3, Duration::from_secs(5), request_id("req-42"))
        .await
        .unwrap();
    assert_eq!(item.name, "Cup");
    let request = server.single();
    assert_eq!(request.target, "/v1/items/3");
    assert_eq!(request.header("x-request-id"), Some("req-42"));
    assert_eq!(
        request.header("authorization"),
        Some("Bearer t"),
        "declared header kept"
    );
}

#[tokio::test]
async fn native_timeout_becomes_a_concord_transport_error() {
    let server =
        Server::reply(Reply::json(200, r#"{"id":3,"name":"Cup"}"#).delay(Duration::from_secs(3)))
            .await;
    let api = inventory_at(server.addr, "Bearer t");
    let started = std::time::Instant::now();
    let error = item_with_timeout(&api, 3, Duration::from_millis(200), request_id("r"))
        .await
        .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert_eq!(error.endpoint(), Some("Inventory.item"));
    assert!(error.transport().unwrap().is_timeout());
    // Concord's Display stays static: no URL, no port.
    let shown = error.to_string();
    assert!(!shown.contains("127.0.0.1"), "{shown}");
}

#[tokio::test]
async fn non_2xx_after_native_edit_is_still_a_status_error() {
    let server = Server::reply(Reply::new(404)).await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = item_with_timeout(&api, 9, Duration::from_secs(5), request_id("r"))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status().map(|s| s.as_u16()), Some(404));
}

#[tokio::test]
async fn interleaved_repeated_query_keys_are_exact_on_the_wire() {
    let server = Server::reply(Reply::json(200, r#"[{"id":1,"name":"Blue pen"}]"#)).await;
    let api = inventory_at(server.addr, "Bearer t");
    let items = search_filters(
        &api,
        &[
            Filter {
                field: "name",
                op: "eq",
                value: "Blue pen",
            },
            Filter {
                field: "qty",
                op: "gt",
                value: "2",
            },
            Filter {
                field: "name",
                op: "ne",
                value: "a&b=c",
            },
        ],
    )
    .await
    .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        server.single().target,
        "/v1/search?limit=10&f=name&op=eq&v=Blue+pen&f=qty&op=gt&v=2&f=name&op=ne&v=a%26b%3Dc"
    );
}

#[tokio::test]
async fn no_filters_leaves_the_declared_query_untouched() {
    let server = Server::reply(Reply::json(200, "[]")).await;
    let api = inventory_at(server.addr, "Bearer t");
    assert!(search_filters(&api, &[]).await.unwrap().is_empty());
    assert_eq!(server.single().target, "/v1/search?limit=10");
}
