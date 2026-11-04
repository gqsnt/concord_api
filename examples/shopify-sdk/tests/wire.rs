//! Single-operation wire tests: what one generated operation puts on the wire (method, host,
//! path, headers, the JSON body with its variables) and how one answer is decoded, against the
//! owned loopback server. Response bodies are small schema-derived literals (pinned 2026-10
//! reference, see `PROVIDER.md`); the workflow fixtures with provenance live in
//! `examples/shopify-app/tests/fixtures`.

use concord::reqwest;
use concord_example_shopify_sdk::graphql::{Data, ErrorCode, Operation};
use concord_example_shopify_sdk::ops::PageSize;
use concord_example_shopify_sdk::ops::catalog::ProductsQuery;
use concord_example_shopify_sdk::ops::shop::ShopQuery;
use concord_example_shopify_sdk::scalars::SignedUrl;
use concord_example_shopify_sdk::{
    AccessToken, ApiVersion, Cursor, RequestError, SearchQuery, ShopDomain, ShopifyContext, Var,
};
use concord_example_testserver::{Recorded, Reply, TestServer};
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
