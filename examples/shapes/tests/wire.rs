//! The examples on the wire, against a loopback server; assertions mirror the hand-written
//! baselines' tests.

#[path = "../../../crates/concord/tests/support/mod.rs"]
mod support;

use concord::reqwest;
use concord_example_shapes::{s1_simple, s2_scoped, s3_query};
use support::{Reply, Server};

#[tokio::test]
async fn s1_health_and_item() {
    let server = Server::start(|r| {
        if r.path().ends_with("/health") {
            Reply::json(200, r#"{"status":"ok"}"#)
        } else {
            Reply::json(200, r#"{"id":"a/b","name":"slash"}"#)
        }
    })
    .await;
    let api = s1_simple::SimpleApi::new(reqwest::Client::new())
        .with_base_url(server.url() + "/v1")
        .unwrap();
    assert_eq!(api.health().send().await.unwrap().status, "ok");
    assert_eq!(api.item("a/b").send().await.unwrap().name, "slash");
    let requests = server.requests();
    assert_eq!(requests[0].target, "/v1/health");
    assert_eq!(requests[1].target, "/v1/items/a%2Fb");
}

fn regional_client(port: u16) -> reqwest::Client {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    reqwest::Client::builder()
        .resolve("eu.api.example.test", addr)
        .build()
        .unwrap()
}

#[tokio::test]
async fn s2_regional_scopes_headers_and_versions() {
    let server = Server::start(|r| {
        if r.path().ends_with("/status") {
            Reply::json(200, r#"{"healthy":true}"#)
        } else {
            Reply::json(200, r#"{"id":"p-1","title":"Pen"}"#)
        }
    })
    .await;
    let port = server.addr.port();
    let api = s2_scoped::CatalogApi::new(
        regional_client(port),
        s2_scoped::CatalogApiConfig {
            api_key: "k-123".into(),
        },
    )
    .with_base_url(format!("http://api.example.test:{port}/v2"))
    .unwrap();

    let eu = api.regional(s2_scoped::Region::Eu);
    assert!(eu.status().send().await.unwrap().healthy);
    let acme = eu.org("acme");
    acme.product("p-1").send().await.unwrap();
    acme.product("p-1")
        .set_version("2025-01-01")
        .send()
        .await
        .unwrap();
    api.clone()
        .set_api_version("2023-01-01")
        .regional(s2_scoped::Region::Eu)
        .org("acme")
        .product("p-1")
        .send()
        .await
        .unwrap();

    let r = server.requests();
    assert_eq!(r[0].target, "/v2/status");
    assert_eq!(
        r[0].header("host"),
        Some(format!("eu.api.example.test:{port}").as_str())
    );
    assert_eq!(r[1].target, "/v2/orgs/acme/catalog/products/p-1");
    assert_eq!(r[1].header("x-api-key"), Some("k-123"));
    assert_eq!(r[1].header("accept"), Some("application/json"));
    assert_eq!(r[1].header("x-api-version"), Some("2024-06-01"));
    assert_eq!(r[2].header("x-api-version"), Some("2025-01-01"));
    assert_eq!(r[3].header("x-api-version"), Some("2023-01-01"));

    let debug = format!("{api:?}");
    assert!(!debug.contains("k-123"), "{debug}");
}

#[tokio::test]
async fn s3_search_create_delete() {
    let server = Server::start(|r| match r.method.as_str() {
        "GET" => Reply::json(200, r#"{"items":[],"next_cursor":null}"#),
        "POST" => Reply::json(201, r#"{"id":"i-1","name":"pen","price_cents":120}"#),
        _ => Reply::new(204),
    })
    .await;
    let api = s3_query::ShopApi::new(
        reqwest::Client::new(),
        s3_query::ShopApiConfig {
            token: "t0k".into(),
        },
    )
    .with_base_url(server.url() + "/v1")
    .unwrap();

    api.search()
        .set_q("red shoe")
        .set_tags(vec!["a".to_owned(), "b".to_owned()])
        .set_sort(s3_query::Sort::PriceAsc)
        .set_limit(50)
        .send()
        .await
        .unwrap();
    api.search().maybe_cursor(None).send().await.unwrap();
    let created = api
        .create_item(s3_query::NewItem {
            name: "pen".into(),
            price_cents: 120,
            tags: vec![],
        })
        .send()
        .await
        .unwrap();
    api.delete_item(created.id).send().await.unwrap();

    let r = server.requests();
    assert_eq!(
        r[0].target,
        "/v1/items/search?q=red+shoe&tag=a&tag=b&sort=price_asc&limit=50"
    );
    assert_eq!(r[1].target, "/v1/items/search?limit=20");
    assert_eq!(r[2].header("content-type"), Some("application/json"));
    assert_eq!(r[2].body, br#"{"name":"pen","price_cents":120,"tags":[]}"#);
    assert_eq!(
        (r[3].method.as_str(), r[3].target.as_str()),
        ("DELETE", "/v1/items/i-1")
    );
    assert!(
        r.iter()
            .all(|r| r.header("authorization") == Some("Bearer t0k"))
    );
}
