//! Single-operation wire tests: what one generated operation puts on the wire (method, host,
//! path, headers, the JSON body with its variables) and how one answer is decoded, against the
//! owned loopback server. Response bodies are small schema-derived literals (pinned 2026-10
//! reference, see `PROVIDER.md`); the workflow fixtures with provenance live in
//! `examples/shopify-app/tests/fixtures`.

use std::time::Duration;

use concord::reqwest;
use concord_example_shopify_sdk::graphql::{Data, ErrorCode, Operation};
use concord_example_shopify_sdk::ops::PageSize;
use concord_example_shopify_sdk::ops::catalog::ProductsQuery;
use concord_example_shopify_sdk::ops::shop::ShopQuery;
use concord_example_shopify_sdk::scalars::SignedUrl;
use concord_example_shopify_sdk::{
    AccessToken, ApiVersion, Cursor, Included, RequestError, SearchQuery, ShopDomain,
    ShopifyContext, Var,
};
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use serde_json::{Value, json};

const TOKEN_ONE: &str = "shpat_synthetic_one_0001";
const TOKEN_TWO: &str = "shpat_synthetic_two_0002";

fn sdk(server: &TestServer) -> ShopifyContext {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve("shop-one.myshopify.com", server.addr())
        .resolve("shop-two.myshopify.com", server.addr())
        .build()
        .unwrap();
    ShopifyContext::new(http)
        .with_base_url(format!("http://myshopify.com:{}", server.addr().port()))
        .unwrap()
}

fn token(t: &str) -> AccessToken {
    AccessToken::new(t).unwrap()
}

fn shop_reply(request: &Recorded) -> Reply {
    let (name, token) = if request.host.starts_with("shop-one.") {
        ("shop-one", TOKEN_ONE)
    } else {
        ("shop-two", TOKEN_TWO)
    };
    if request.header("x-shopify-access-token") != Some(token) {
        return Reply::json(401, r#"{"errors":"[API] Invalid API key or access token"}"#);
    }
    let body = json!({
        "data": {"shop": {
            "id": format!("gid://shopify/Shop/{}", if name == "shop-one" { 1 } else { 2 }),
            "name": name,
            "myshopifyDomain": format!("{name}.myshopify.com"),
            "currencyCode": "EUR"
        }},
        "extensions": {"cost": {"requestedQueryCost": 1, "actualQueryCost": 1,
            "throttleStatus": {"maximumAvailable": 2000.0, "currentlyAvailable": 1999, "restoreRate": 100.0}}}
    });
    graphql(body)
}

fn graphql(body: Value) -> Reply {
    Reply::json(200, body.to_string()).header("x-shopify-api-version", "2026-10")
}

fn body_json(request: &Recorded) -> Value {
    serde_json::from_slice(&request.body).expect("the request body is JSON")
}

#[tokio::test]
async fn one_operation_on_the_wire() {
    let server = TestServer::start(shop_reply).await;
    let sdk = sdk(&server);
    let shop = sdk.shop(ShopDomain::new("shop-one").unwrap(), &token(TOKEN_ONE));

    let answer = shop
        .send(shop.admin().shop_info(ShopQuery {}).prepare())
        .await
        .unwrap();
    let data = answer.data.present().unwrap();
    assert_eq!(data.shop.myshopify_domain, "shop-one.myshopify.com");

    let request = server.single();
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/admin/api/2026-10/graphql.json");
    assert_eq!(
        request.host,
        format!("shop-one.myshopify.com:{}", server.addr().port())
    );
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("x-shopify-access-token"), Some(TOKEN_ONE));
    assert_eq!(
        body_json(&request),
        json!({"query": ShopQuery::DOCUMENT, "operationName": "ShopIdentity", "variables": {}})
    );
    assert!(ShopQuery::DOCUMENT.starts_with("query ShopIdentity "));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn two_shops_never_share_a_host_or_a_token() {
    let server = TestServer::start(shop_reply).await;
    let sdk = sdk(&server);
    let one = sdk.shop(ShopDomain::new("shop-one").unwrap(), &token(TOKEN_ONE));
    let two = sdk.shop(
        ShopDomain::new("SHOP-TWO.myshopify.com").unwrap(),
        &token(TOKEN_TWO),
    );

    for _ in 0..2 {
        let a = one
            .send(one.admin().shop_info(ShopQuery {}).prepare())
            .await
            .unwrap();
        let b = two
            .send(two.admin().shop_info(ShopQuery {}).prepare())
            .await
            .unwrap();
        assert_eq!(a.data.present().unwrap().shop.name, "shop-one");
        assert_eq!(b.data.present().unwrap().shop.name, "shop-two");
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    for request in &requests {
        let expected = if request.host.starts_with("shop-one.") {
            TOKEN_ONE
        } else {
            assert!(request.host.starts_with("shop-two."));
            TOKEN_TWO
        };
        assert_eq!(request.header_all("x-shopify-access-token"), [expected]);
    }

    // A shop handle swapped onto the other token is refused by the server: the scope carries both.
    let crossed = sdk.shop(ShopDomain::new("shop-one").unwrap(), &token(TOKEN_TWO));
    let error = crossed
        .send(crossed.admin().shop_info(ShopQuery {}).prepare())
        .await
        .unwrap_err();
    let concord = error.concord().unwrap();
    assert_eq!(concord.status(), Some(reqwest::StatusCode::UNAUTHORIZED));
    let shown = format!("{error} {error:?} {one:?} {sdk:?}");
    assert!(
        !shown.contains(TOKEN_ONE) && !shown.contains(TOKEN_TWO),
        "{shown}"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn variables_are_json_encoded_with_null_and_absent_distinct() {
    let server = TestServer::start(|_| {
        graphql(json!({"data": {"products": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}}}}))
    })
    .await;
    let sdk = sdk(&server);
    let shop = sdk.shop(ShopDomain::new("shop-one").unwrap(), &token(TOKEN_ONE));
    let filter = SearchQuery::new()
        .term("vendor", "Ünïcødé \"Quoted\" \\ Co")
        .term("title", "line\nbreak\ttab")
        .not_term("status", "archived");
    let ten = PageSize::new(10).unwrap();

    // First page: `after` absent, filter present.
    let first = ProductsQuery::new(ten, PageSize::new(3).unwrap()).filter(&filter);
    shop.send(shop.admin().products(first).prepare())
        .await
        .unwrap();
    // Explicit null `after`.
    let mut null = ProductsQuery::new(ten, ten);
    null.after = Var::Null;
    shop.send(shop.admin().products(null).prepare())
        .await
        .unwrap();
    // A cursor with JSON-significant characters is sent verbatim.
    let cursor = Cursor::new("eyJsYXN0X2lkIjo0Mn0=\"\\/");
    let next = ProductsQuery::new(ten, ten).after(Some(cursor.clone()));
    shop.send(shop.admin().products(next).prepare())
        .await
        .unwrap();

    let requests = server.requests();
    let vars: Vec<Value> = requests
        .iter()
        .map(|r| body_json(r)["variables"].clone())
        .collect();
    assert_eq!(
        vars[0],
        json!({
            "first": 10,
            "query": "vendor:\"Ünïcødé \\\"Quoted\\\" \\\\ Co\" title:\"line\nbreak\ttab\" -status:archived",
            "variantsFirst": 3
        })
    );
    assert!(vars[0].get("after").is_none(), "absent is omitted");
    assert_eq!(
        vars[1],
        json!({"first": 10, "after": null, "variantsFirst": 10})
    );
    assert_eq!(vars[2]["after"], json!(cursor.expose()));
    // The raw body carries the non-ASCII text as UTF-8, not as \u escapes, and escapes controls.
    let raw = requests[0].body_text();
    assert!(raw.contains("Ünïcødé"), "{raw}");
    assert!(raw.contains(r"line\nbreak\ttab"), "{raw}");
    assert_eq!(body_json(&requests[0])["operationName"], "CatalogProducts");
    server.shutdown().await;
}

#[tokio::test]
async fn http_200_with_errors_and_throttle_metadata_is_an_envelope_not_an_error() {
    let throttled = json!({
        "errors": [{"message": "Throttled", "extensions": {"code": "THROTTLED",
            "documentation": "https://shopify.dev/api/usage/limits#rate-limits"}}],
        "extensions": {"cost": {"requestedQueryCost": 752, "actualQueryCost": null,
            "throttleStatus": {"maximumAvailable": 2000.0, "currentlyAvailable": 300, "restoreRate": 100.0}}}
    });
    let server = TestServer::start(move |_| graphql(throttled.clone())).await;
    let sdk = sdk(&server);
    let shop = sdk.shop(ShopDomain::new("shop-one").unwrap(), &token(TOKEN_ONE));
    let query = ProductsQuery::new(PageSize::MAX, PageSize::new(2).unwrap());
    let answer = shop
        .send(shop.admin().products(query).prepare())
        .await
        .unwrap();

    assert!(matches!(answer.data, Data::Absent));
    assert!(answer.is_throttled());
    assert_eq!(answer.errors[0].code(), Some(&ErrorCode::Throttled));
    let cost = answer.cost().unwrap();
    assert_eq!(cost.requested_query_cost, 752.0);
    assert_eq!(cost.actual_query_cost, None);
    assert_eq!(
        cost.throttle_status.seconds_until_available(752.0),
        Some(4.52)
    );
    // Diagnostics show the code, never the message text.
    let shown = format!("{answer:?}");
    assert!(
        shown.contains("Throttled") && !shown.contains("documentation"),
        "{shown}"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn the_served_version_must_be_the_pinned_one() {
    let server = TestServer::start(|request: &Recorded| {
        let ok = json!({"data": {"shop": {"id": "gid://shopify/Shop/1", "name": "n",
            "myshopifyDomain": "shop-one.myshopify.com", "currencyCode": "EUR"}}});
        match request.path() {
            // Shopify falls forward from a retired version and says so in the header.
            "/admin/api/2026-01/graphql.json" => {
                Reply::json(200, ok.to_string()).header("x-shopify-api-version", "2026-04")
            }
            _ => Reply::json(200, ok.to_string()),
        }
    })
    .await;
    let sdk = sdk(&server);
    let shop = sdk.shop(ShopDomain::new("shop-one").unwrap(), &token(TOKEN_ONE));
    let error = shop
        .send(shop.admin().shop_info(ShopQuery {}).prepare())
        .await
        .unwrap_err();
    assert!(
        matches!(error, RequestError::VersionUnconfirmed),
        "{error:?}"
    );

    let old = sdk.clone().with_version(ApiVersion::new(2026, 1).unwrap());
    let shop = old.shop(ShopDomain::new("shop-one").unwrap(), &token(TOKEN_ONE));
    let error = shop
        .send(shop.admin().shop_info(ShopQuery {}).prepare())
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "requested API version 2026-01, served 2026-04"
    );
    assert_eq!(
        server.requests()[1].path(),
        "/admin/api/2026-01/graphql.json"
    );
    server.shutdown().await;
}

/// `ShopContext::send` under `version`, which must fail; bounded so that a guard that read the
/// (stalled) body would fail the test instead of hanging it.
async fn refused(sdk: &ShopifyContext, version: ApiVersion) -> RequestError {
    let pinned = sdk.clone().with_version(version);
    let shop = pinned.shop(ShopDomain::new("shop-one").unwrap(), &token(TOKEN_ONE));
    let sent = tokio::time::timeout(
        Duration::from_secs(5),
        shop.send(shop.admin().shop_info(ShopQuery {}).prepare()),
    )
    .await
    .expect("refused on the head, without waiting for the body");
    match sent {
        Ok(_) => panic!("{version} was accepted"),
        Err(error) => error,
    }
}

/// The served-version guard runs on the head of a 2xx answer, before its body is read: a body
/// that never arrives does not delay the refusal (another version, an unparseable one, none). A
/// non-2xx answer is not version-checked: it is Concord's status error, its response unread.
#[tokio::test]
async fn the_version_guard_refuses_on_the_head_without_reading_the_body() {
    let server = TestServer::start(|request: &Recorded| {
        let served = match request.path() {
            "/admin/api/2026-01/graphql.json" => Some("2026-04"),
            "/admin/api/2026-04/graphql.json" => Some("2026-4"),
            "/admin/api/2026-07/graphql.json" => None,
            _ => return Reply::json(503, r#"{"errors":"unavailable"}"#),
        };
        let reply = Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Stall);
        match served {
            Some(version) => reply.header("x-shopify-api-version", version),
            None => reply,
        }
    })
    .await;
    let sdk = sdk(&server);

    let error = refused(&sdk, ApiVersion::new(2026, 1).unwrap()).await;
    assert_eq!(
        error.to_string(),
        "requested API version 2026-01, served 2026-04"
    );
    let error = refused(&sdk, ApiVersion::new(2026, 4).unwrap()).await;
    assert!(
        matches!(error, RequestError::VersionMismatch { served: None, .. }),
        "{error:?}"
    );
    let error = refused(&sdk, ApiVersion::new(2026, 7).unwrap()).await;
    assert!(
        matches!(error, RequestError::VersionUnconfirmed),
        "{error:?}"
    );

    let RequestError::Concord(error) = refused(&sdk, ApiVersion::PINNED).await else {
        panic!("a non-2xx answer is Concord's status error")
    };
    assert_eq!(error.kind(), concord::ErrorKind::Status);
    assert_eq!(error.status().map(|s| s.as_u16()), Some(503));
    let body = error.into_response().unwrap().text().await.unwrap();
    assert_eq!(body, r#"{"errors":"unavailable"}"#);
    assert_eq!(server.requests().len(), 4, "one execution each");
    server.shutdown().await;
}

#[tokio::test]
async fn the_result_download_goes_to_its_own_host_without_the_token() {
    let storage = TestServer::start(|_| {
        Reply::text(
            200,
            "application/jsonl",
            "{\"id\":\"gid://shopify/Product/1\"}\n",
        )
    })
    .await;
    let http = reqwest::Client::builder()
        .resolve("storage.example.test", storage.addr())
        .build()
        .unwrap();
    let sdk = ShopifyContext::new(http);
    let shop = sdk.shop(ShopDomain::new("shop-one").unwrap(), &token(TOKEN_ONE));
    let secret = "X-Goog-Signature=synthetic0123456789abcdef";
    let url = SignedUrl::parse(&format!(
        "http://storage.example.test:{}/bulk/1.jsonl?X-Goog-Expires=604800&{secret}",
        storage.addr().port()
    ))
    .unwrap();
    assert_eq!(
        format!("{url:?}"),
        r#"SignedUrl { host: "storage.example.test", .. }"#
    );

    let response = shop
        .http()
        .execute(shop.result_download(&url))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let request = storage.single();
    assert_eq!(request.method, "GET");
    assert!(
        request.target.ends_with(secret),
        "the signed query is sent verbatim"
    );
    assert_eq!(request.header("x-shopify-access-token"), None);
    assert_eq!(request.header("authorization"), None);
    drop(response);
    storage.shutdown().await;
}

#[tokio::test]
async fn product_by_id_and_bulk_cancel_on_the_wire() {
    use concord_example_shopify_sdk::ops::bulk::{BulkOperationStatus, CancelBulkOperation};
    use concord_example_shopify_sdk::ops::catalog::ProductQuery;
    use concord_example_shopify_sdk::scalars::{BulkOperationId, ProductId};

    let server = TestServer::start(|request: &Recorded| {
        let body = body_json(request);
        match body["operationName"].as_str() {
            Some("CatalogProduct") => graphql(json!({"data": {"product": {
                "id": body["variables"]["id"], "title": "Tee", "handle": "tee", "status": "UNLISTED",
                "vendor": "V", "productType": "", "tags": ["a", "b"], "totalInventory": 0,
                "variantsCount": null,
                "variants": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}},
                "collections": {"nodes": [{"id": "gid://shopify/Collection/5", "title": "C", "handle": "c"}],
                                "pageInfo": {"hasNextPage": true, "endCursor": "Y29s"}}}}})),
            Some("BulkOperationCancel") => graphql(json!({"data": {"bulkOperationCancel": {
                "bulkOperation": {"id": body["variables"]["id"], "status": "CANCELING", "errorCode": null,
                    "createdAt": "2026-10-22T10:00:00Z", "completedAt": null, "objectCount": "12",
                    "rootObjectCount": "4", "fileSize": null, "url": null, "partialDataUrl": null, "type": "QUERY"},
                "userErrors": []}}})),
            _ => Reply::json(500, "{}"),
        }
    })
    .await;
    let sdk = sdk(&server);
    let shop = sdk.shop(ShopDomain::new("shop-one").unwrap(), &token(TOKEN_ONE));

    let id = ProductId::parse("gid://shopify/Product/101").unwrap();
    let query = ProductQuery {
        id: id.clone(),
        variants_first: PageSize::new(5).unwrap(),
        collections_first: PageSize::new(2).unwrap(),
    };
    let product = shop
        .send(shop.admin().product(query).prepare())
        .await
        .unwrap()
        .data
        .present()
        .unwrap()
        .product
        .unwrap();
    assert_eq!(product.id, id);
    assert_eq!(product.variants_count, None);
    assert_eq!(product.tags, ["a", "b"]);
    assert!(product.collections.page_info.has_next_page);

    let op = BulkOperationId::parse("gid://shopify/BulkOperation/9001").unwrap();
    let cancel = shop
        .send(
            shop.admin()
                .cancel_bulk_operation(CancelBulkOperation { id: op.clone() })
                .prepare(),
        )
        .await
        .unwrap();
    let payload = cancel
        .data
        .present()
        .unwrap()
        .bulk_operation_cancel
        .unwrap();
    assert!(payload.user_errors.is_empty());
    assert_eq!(
        payload.bulk_operation.unwrap().status,
        BulkOperationStatus::Canceling
    );

    let requests = server.requests();
    assert_eq!(
        body_json(&requests[0])["variables"],
        json!({"id": "gid://shopify/Product/101", "variantsFirst": 5, "collectionsFirst": 2})
    );
    let cancel_body = body_json(&requests[1]);
    assert_eq!(
        cancel_body["variables"],
        json!({"id": "gid://shopify/BulkOperation/9001"})
    );
    assert!(
        cancel_body["query"]
            .as_str()
            .unwrap()
            .starts_with("mutation BulkOperationCancel(")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn online_store_url_is_selected_only_when_requested() {
    let product = |url: Option<Value>| {
        let mut node = json!({"id": "gid://shopify/Product/1", "title": "Tee", "handle": "tee",
            "status": "ACTIVE", "vendor": "V", "productType": "", "tracksInventory": true,
            "totalInventory": 1, "updatedAt": "2026-10-01T00:00:00Z",
            "variants": {"nodes": [], "pageInfo": {"hasNextPage": false, "endCursor": null}}});
        if let Some(url) = url {
            node["onlineStoreUrl"] = url;
        }
        node
    };
    // What Shopify answers: the member only when `withOnlineStoreUrl` is true.
    let server = TestServer::start(move |request| {
        let nodes = match body_json(request)["variables"]["withOnlineStoreUrl"].as_bool() {
            Some(true) => json!([
                product(Some(json!("https://shop-one.example/products/tee"))),
                product(Some(Value::Null))
            ]),
            _ => json!([product(None)]),
        };
        graphql(json!({"data": {"products": {"nodes": nodes,
            "pageInfo": {"hasNextPage": false, "endCursor": null}}}}))
    })
    .await;
    let sdk = sdk(&server);
    let shop = sdk.shop(ShopDomain::new("shop-one").unwrap(), &token(TOKEN_ONE));
    let ten = PageSize::new(10).unwrap();

    let plain = shop
        .send(
            shop.admin()
                .products(ProductsQuery::new(ten, ten))
                .prepare(),
        )
        .await
        .unwrap();
    let with_url = ProductsQuery::new(ten, ten).with_online_store_url();
    let requested = shop
        .send(shop.admin().products(with_url).prepare())
        .await
        .unwrap();

    let plain = plain.data.present().unwrap().products.nodes;
    assert_eq!(plain[0].online_store_url, Included::NotIncluded);
    let requested = requested.data.present().unwrap().products.nodes;
    assert_eq!(
        requested[0].online_store_url.value().map(String::as_str),
        Some("https://shop-one.example/products/tee")
    );
    // `null`: not published to the online store; distinct from not requested.
    assert_eq!(requested[1].online_store_url, Included::Null);
    assert!(requested[1].online_store_url.is_included());

    let requests = server.requests();
    // Without the flag the variable is omitted (the document's default, `false`).
    assert_eq!(
        body_json(&requests[0])["variables"],
        json!({"first": 10, "variantsFirst": 10})
    );
    assert_eq!(
        body_json(&requests[1])["variables"],
        json!({"first": 10, "variantsFirst": 10, "withOnlineStoreUrl": true})
    );
    assert!(ProductsQuery::DOCUMENT.contains("$withOnlineStoreUrl: Boolean! = false"));
    assert!(ProductsQuery::DOCUMENT.contains("onlineStoreUrl @include(if: $withOnlineStoreUrl)"));
    server.shutdown().await;
}
