//! The frozen initial consumer against loopback fixtures (stage 1; not edited afterwards).
//!
//! One loopback server answers for two shops (each request must carry exactly its shop's
//! synthetic token, so a scope leak gets a `401`); a second one is the signed-URL storage host.
//! The JSON answers are schema-derived synthetic fixtures (field types and nullability from the
//! `2026-10` reference pages, see the SDK's `PROVIDER.md`); every server is shut down.

use std::num::NonZeroUsize;

use concord::reqwest;
use concord::stream::ReadErrorKind;
use concord_check_maintenance_shopify::before::{
    ExportState, FeedError, FeedLine, FeedOffer, FeedProduct, feed_export, list_active_products,
    stream_feed_export, submit_feed_export,
};
use concord_example_shopify_sdk::ops::PageSize;
use concord_example_shopify_sdk::ops::bulk::{
    BulkOperationQuery, BulkOperationStatus, RunBulkQuery,
};
use concord_example_shopify_sdk::ops::catalog::{ProductStatus, ProductsQuery};
use concord_example_shopify_sdk::scalars::BulkOperationId;
use concord_example_shopify_sdk::{
    AccessToken, Operation, ShopContext, ShopDomain, ShopifyContext,
};
use concord_example_testserver::{Recorded, Reply, Script, TestServer};
use concord_pagination::{Budget, Outcome};
use serde_json::{Value, json};

const TOKEN_ONE: &str = "shpat_synthetic_feed_shop_one_0001";
const TOKEN_TWO: &str = "shpat_synthetic_feed_shop_two_0002";
const STORAGE: &str = "storage.example.test";
const SIGNED_PATH: &str =
    "/bulk/9001.jsonl?X-Goog-Algorithm=GOOG4-RSA-SHA256&X-Goog-Signature=SYNTHETIC_SIG_9001";

/// A GraphQL answer: 200, JSON, served under the pinned version.
fn graphql(body: Value) -> Reply {
    Reply::json(200, body.to_string()).header("x-shopify-api-version", "2026-10")
}

/// The shop a request was sent to, after checking it carries exactly that shop's token.
fn shop_of(request: &Recorded) -> Result<&'static str, Reply> {
    let (shop, token) = if request.host.starts_with("shop-one.myshopify.com") {
        ("one", TOKEN_ONE)
    } else if request.host.starts_with("shop-two.myshopify.com") {
        ("two", TOKEN_TWO)
    } else {
        return Err(Reply::json(404, "{}"));
    };
    if request.header_all("x-shopify-access-token") != [token] {
        return Err(Reply::json(401, r#"{"errors":"[API] Invalid API key"}"#));
    }
    Ok(shop)
}

fn sdk(shops: &TestServer, storage: Option<&TestServer>) -> ShopifyContext {
    let mut http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve("shop-one.myshopify.com", shops.addr())
        .resolve("shop-two.myshopify.com", shops.addr());
    if let Some(storage) = storage {
        http = http.resolve(STORAGE, storage.addr());
    }
    ShopifyContext::new(http.build().unwrap())
        .with_base_url(format!("http://myshopify.com:{}", shops.addr().port()))
        .unwrap()
}

fn shop(sdk: &ShopifyContext, handle: &str, token: &str) -> ShopContext {
    sdk.shop(
        ShopDomain::new(handle).unwrap(),
        &AccessToken::new(token).unwrap(),
    )
}

/// The exact body the SDK sends for operation `O` with these variables (raw JSON text).
fn expected_body<O: Operation>(variables: &str) -> String {
    format!(
        r#"{{"query":{},"operationName":"{}","variables":{variables}}}"#,
        serde_json::to_string(O::DOCUMENT).unwrap(),
        O::NAME
    )
}

/// Wire checks every GraphQL request shares.
fn assert_graphql_request(request: &Recorded, shop: &str, token: &str) {
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/admin/api/2026-10/graphql.json");
    assert!(
        request.host.starts_with(&format!("{shop}.myshopify.com:")),
        "{}",
        request.host
    );
    assert_eq!(request.header_all("x-shopify-access-token"), [token]);
    assert_eq!(request.header_all("content-type"), ["application/json"]);
}

fn cost() -> Value {
    json!({"cost": {"requestedQueryCost": 12, "actualQueryCost": 6,
        "throttleStatus": {"maximumAvailable": 2000.0, "currentlyAvailable": 1994, "restoreRate": 100.0}}})
}

/// A `CatalogProducts` product node with its first variant (or none).
fn product(id: u64, title: &str, vendor: &str, variant: Option<Value>) -> Value {
    let nodes: Vec<Value> = variant.into_iter().collect();
    json!({
        "id": format!("gid://shopify/Product/{id}"),
        "title": title,
        "handle": title.to_lowercase().replace(' ', "-"),
        "status": "ACTIVE",
        "vendor": vendor,
        "productType": "Mugs",
        "tracksInventory": true,
        "totalInventory": 7,
        "updatedAt": "2026-10-30T08:00:00Z",
        "variants": {"nodes": nodes, "pageInfo": {"hasNextPage": false, "endCursor": null}}
    })
}

fn variant(id: u64, sku: Option<&str>, price: &str) -> Value {
    json!({
        "id": format!("gid://shopify/ProductVariant/{id}"),
        "title": "Default Title",
        "sku": sku,
        "price": price,
        "compareAtPrice": null,
        "inventoryQuantity": 7,
        "position": 1,
        "inventoryItem": {"id": format!("gid://shopify/InventoryItem/{id}"), "tracked": true}
    })
}

#[tokio::test]
async fn feed_listing_pages_active_products_with_exact_requests() {
    let server = TestServer::start(|request| {
        if let Err(reply) = shop_of(request) {
            return reply;
        }
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let after = body["variables"]["after"].as_str().map(str::to_owned);
        let (nodes, page_info) = match after.as_deref() {
            None => (
                vec![
                    product(
                        11,
                        "Blue Mug",
                        "Synthetic Pottery",
                        Some(variant(111, Some("MUG-B"), "12.50")),
                    ),
                    product(
                        12,
                        "Red Mug",
                        "Ünïcødé \"Kiln\" Co",
                        Some(variant(121, None, "13.00")),
                    ),
                ],
                json!({"hasNextPage": true, "endCursor": "Y3Vyc29yLTE="}),
            ),
            Some("Y3Vyc29yLTE=") => (
                vec![product(13, "Gift Card", "Synthetic Pottery", None)],
                json!({"hasNextPage": false, "endCursor": "Y3Vyc29yLTI="}),
            ),
            Some(_) => return Reply::json(400, "{}"),
        };
        graphql(
            json!({"data": {"products": {"nodes": nodes, "pageInfo": page_info}},
            "extensions": cost()}),
        )
    })
    .await;
    let sdk = sdk(&server, None);
    let one = shop(&sdk, "shop-one", TOKEN_ONE);

    let listing = list_active_products(
        &one,
        PageSize::new(2).unwrap(),
        Budget::new(NonZeroUsize::new(5).unwrap()),
    )
    .await
    .unwrap();

    assert_eq!(listing.outcome, Outcome::Exhausted);
    assert!(listing.resume.is_none());
    let ids: Vec<&str> = listing.products.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "gid://shopify/Product/11",
            "gid://shopify/Product/12",
            "gid://shopify/Product/13"
        ]
    );
    assert_eq!(
        listing.products[1],
        FeedProduct {
            id: "gid://shopify/Product/12".parse_gid(),
            title: "Red Mug".into(),
            handle: "red-mug".into(),
            vendor: "Ünïcødé \"Kiln\" Co".into(),
            product_type: "Mugs".into(),
            offer: Some(FeedOffer {
                variant_id: "gid://shopify/ProductVariant/121".parse_gid(),
                sku: None,
                price: serde_json::from_value(json!("13.00")).unwrap(),
            }),
        }
    );
    assert_eq!(listing.products[2].offer, None);

    // The exact requests: one per page, the cursor of page 1 sent back verbatim as `after`.
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_graphql_request(request, "shop-one", TOKEN_ONE);
    }
    assert_eq!(
        requests[0].body_text(),
        expected_body::<ProductsQuery>(r#"{"first":2,"query":"status:active","variantsFirst":1}"#)
    );
    assert_eq!(
        requests[1].body_text(),
        expected_body::<ProductsQuery>(
            r#"{"first":2,"after":"Y3Vyc29yLTE=","query":"status:active","variantsFirst":1}"#
        )
    );
    server.shutdown().await;
}

/// The bulk document the feed export submits, written out: the consumer pins the selection it
/// chose (filter + variants).
const FEED_EXPORT_DOCUMENT: &str = "{ products(query: \"status:active\") { edges { node { id title \
     handle status vendor updatedAt variants { edges { node { id title sku price inventoryQuantity \
     inventoryItem { id tracked } } } } } } } }";

fn bulk_operation(status: &str, url: Option<String>) -> Value {
    json!({
        "id": "gid://shopify/BulkOperation/9001",
        "status": status,
        "errorCode": null,
        "createdAt": "2026-11-01T10:00:00Z",
        "completedAt": if status == "COMPLETED" { json!("2026-11-01T10:02:00Z") } else { Value::Null },
        "objectCount": "5",
        "rootObjectCount": "2",
        "fileSize": if url.is_some() { json!("1024") } else { Value::Null },
        "url": url,
        "partialDataUrl": null,
        "type": "QUERY"
    })
}

const EXPORT: &str = concat!(
    r#"{"id":"gid://shopify/Product/21","title":"Café Mug","handle":"cafe-mug","status":"ACTIVE","vendor":"Synthetic Pottery","updatedAt":"2026-10-30T08:00:00Z"}"#,
    "\n",
    r#"{"id":"gid://shopify/Product/22","title":"日本の湯呑","handle":"yunomi","status":"ACTIVE","vendor":"Synthetic Pottery","updatedAt":"2026-10-30T08:00:00Z"}"#,
    "\n",
    r#"{"id":"gid://shopify/ProductVariant/211","title":"S","sku":"CAFE-S","price":"9.90","inventoryQuantity":3,"inventoryItem":{"id":"gid://shopify/InventoryItem/211","tracked":true},"__parentId":"gid://shopify/Product/21"}"#,
    "\n",
    r#"{"id":"gid://shopify/ProductVariant/221","title":"Default Title","sku":null,"price":"24.00","inventoryQuantity":null,"inventoryItem":{"id":"gid://shopify/InventoryItem/221","tracked":false},"__parentId":"gid://shopify/Product/22"}"#,
    "\n",
    r#"{"id":"gid://shopify/ProductVariant/212","title":"L","sku":"CAFE-L","price":"11.90","inventoryQuantity":0,"inventoryItem":{"id":"gid://shopify/InventoryItem/212","tracked":true},"__parentId":"gid://shopify/Product/21"}"#,
    "\n",
);

#[tokio::test]
async fn feed_export_submits_checks_and_streams_products_and_offers() {
    let storage = TestServer::start(|_| Reply::text(200, "application/jsonl", EXPORT)).await;
    let url = format!("http://{STORAGE}:{}{SIGNED_PATH}", storage.addr().port());
    let script = Script::new([
        graphql(json!({"data": {"bulkOperationRunQuery": {
            "bulkOperation": bulk_operation("CREATED", None), "userErrors": []}},
            "extensions": cost()})),
        graphql(
            json!({"data": {"bulkOperation": bulk_operation("COMPLETED", Some(url))},
            "extensions": cost()}),
        ),
    ]);
    let shops = TestServer::start(move |request| match shop_of(request) {
        Ok(_) => script.next(),
        Err(reply) => reply,
    })
    .await;
    let sdk = sdk(&shops, Some(&storage));
    let two = shop(&sdk, "shop-two.myshopify.com", TOKEN_TWO);

    let id = submit_feed_export(&two).await.unwrap();
    assert_eq!(id.as_str(), "gid://shopify/BulkOperation/9001");
    let mut lines = Vec::new();
    let state = stream_feed_export(&two, &id, |line| lines.push(line))
        .await
        .unwrap();

    let ExportState::Streamed(report) = state else {
        panic!("{state:?}");
    };
    assert_eq!((report.products, report.offers), (2, 3));
    assert_eq!(report.position.records, 5);
    assert_eq!(report.position.bytes, EXPORT.len() as u64);
    // Records arrive one per line, in file order; an offer names its product (`__parentId`)
    // even when it is not adjacent to it.
    assert!(
        matches!(&lines[0], FeedLine::Product { title, status: ProductStatus::Active, .. } if title == "Café Mug")
    );
    assert!(matches!(&lines[1], FeedLine::Product { title, .. } if title == "日本の湯呑"));
    assert_eq!(
        lines[4],
        FeedLine::Offer {
            variant_id: "gid://shopify/ProductVariant/212".parse_gid(),
            product_id: "gid://shopify/Product/21".parse_gid(),
            sku: Some("CAFE-L".into()),
            price: serde_json::from_value(json!("11.90")).unwrap(),
            quantity: Some(0),
        }
    );
    assert!(matches!(
        &lines[3],
        FeedLine::Offer {
            sku: None,
            quantity: None,
            ..
        }
    ));

    // The exact GraphQL requests: the submission carries the bulk document as its `query`
    // variable, the status check the operation id.
    let requests = shops.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_graphql_request(request, "shop-two", TOKEN_TWO);
    }
    assert_eq!(feed_export().document(), FEED_EXPORT_DOCUMENT);
    assert_eq!(
        requests[0].body_text(),
        expected_body::<RunBulkQuery>(&json!({"query": FEED_EXPORT_DOCUMENT}).to_string())
    );
    assert_eq!(
        requests[1].body_text(),
        expected_body::<BulkOperationQuery>(r#"{"id":"gid://shopify/BulkOperation/9001"}"#)
    );
    // The download: the signed URL verbatim, on the storage host, without the shop's token.
    let download = storage.single();
    assert_eq!(download.method, "GET");
    assert_eq!(download.target, SIGNED_PATH);
    assert!(download.host.starts_with(STORAGE), "{}", download.host);
    assert!(download.header_all("x-shopify-access-token").is_empty());
    storage.shutdown().await;
    shops.shutdown().await;
}

#[tokio::test]
async fn feed_export_not_ready_downloads_nothing_and_a_bad_line_stops_the_stream() {
    let storage = TestServer::start(|_| {
        // The second line is an offer without its price: valid JSON, not a record of the selection.
        Reply::text(200, "application/jsonl", concat!(
            r#"{"id":"gid://shopify/Product/21","title":"Café Mug","handle":"cafe-mug","status":"ACTIVE","vendor":"Synthetic Pottery","updatedAt":"2026-10-30T08:00:00Z"}"#,
            "\n",
            r#"{"id":"gid://shopify/ProductVariant/211","title":"S","sku":"CAFE-S","inventoryQuantity":3,"inventoryItem":{"id":"gid://shopify/InventoryItem/211","tracked":true},"__parentId":"gid://shopify/Product/21"}"#,
            "\n",
            r#"{"id":"gid://shopify/Product/22","title":"never read","handle":"x","status":"ACTIVE","vendor":"x","updatedAt":"2026-10-30T08:00:00Z"}"#,
            "\n",
        ))
    })
    .await;
    let url = format!("http://{STORAGE}:{}{SIGNED_PATH}", storage.addr().port());
    let script = Script::new([
        graphql(json!({"data": {"bulkOperation": bulk_operation("RUNNING", None)}})),
        graphql(json!({"data": {"bulkOperation": bulk_operation("COMPLETED", Some(url))}})),
    ]);
    let shops = TestServer::start(move |request| match shop_of(request) {
        Ok(_) => script.next(),
        Err(reply) => reply,
    })
    .await;
    let sdk = sdk(&shops, Some(&storage));
    let two = shop(&sdk, "shop-two", TOKEN_TWO);
    let id: BulkOperationId = "gid://shopify/BulkOperation/9001".parse_gid();

    let mut lines = Vec::new();
    let state = stream_feed_export(&two, &id, |line| lines.push(line))
        .await
        .unwrap();
    assert_eq!(state, ExportState::NotReady(BulkOperationStatus::Running));
    assert_eq!(
        storage.requests().len(),
        0,
        "nothing downloaded before completion"
    );

    let error = stream_feed_export(&two, &id, |line| lines.push(line))
        .await
        .unwrap_err();
    let FeedError::Read(read) = &error else {
        panic!("{error:?}");
    };
    assert_eq!(read.kind(), ReadErrorKind::Decode);
    assert_eq!(
        read.position().records,
        1,
        "the product before it was delivered"
    );
    assert_eq!(lines.len(), 1);
    // Diagnostics name the failure, never the signed URL or a token.
    let shown = format!("{error} {error:?}");
    assert!(
        !shown.contains("SYNTHETIC_SIG") && !shown.contains("shpat_"),
        "{shown}"
    );
    storage.shutdown().await;
    shops.shutdown().await;
}

/// Global ids as the tests write them.
trait ParseGid {
    fn parse_gid<K: concord_example_shopify_sdk::scalars::GidKind>(
        &self,
    ) -> concord_example_shopify_sdk::scalars::Gid<K>;
}

impl ParseGid for str {
    fn parse_gid<K: concord_example_shopify_sdk::scalars::GidKind>(
        &self,
    ) -> concord_example_shopify_sdk::scalars::Gid<K> {
        concord_example_shopify_sdk::scalars::Gid::parse(self).unwrap()
    }
}
