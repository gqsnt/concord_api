//! The updated consumer against loopback fixtures (stage 3), plus the compatibility evidence
//! across the SDK change: the frozen consumer against answers that carry the new member, the
//! updated consumer against an answer to the older document, and the frozen bulk reader against
//! a result that contains metafield lines.
//!
//! Same setup as `tests/before.rs` (its helpers are repeated here because that file is frozen):
//! one loopback server for two shops with per-shop synthetic tokens, a second one for the
//! storage host; schema-derived synthetic answers; every server is shut down.

use std::num::NonZeroUsize;

use concord::reqwest;
use concord::stream::ReadErrorKind;
use concord_check_maintenance_shopify::after::{
    AttributeLine, AttributeState, FEED_NAMESPACE, UpdatedFeedError, attribute_export,
    list_published_products, stream_attribute_export, submit_attribute_export,
};
use concord_check_maintenance_shopify::before::{
    FeedError, FeedLine, list_active_products, stream_feed_export,
};
use concord_example_shopify_sdk::ops::PageSize;
use concord_example_shopify_sdk::ops::bulk::{BulkOperationQuery, RunBulkQuery};
use concord_example_shopify_sdk::ops::catalog::ProductsQuery;
use concord_example_shopify_sdk::scalars::{BulkOperationId, Gid, GidKind};
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
    "/bulk/9002.jsonl?X-Goog-Algorithm=GOOG4-RSA-SHA256&X-Goog-Signature=SYNTHETIC_SIG_9002";

fn graphql(body: Value) -> Reply {
    Reply::json(200, body.to_string()).header("x-shopify-api-version", "2026-10")
}

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

fn gid<K: GidKind>(raw: &str) -> Gid<K> {
    Gid::parse(raw).unwrap()
}

fn expected_body<O: Operation>(variables: &str) -> String {
    format!(
        r#"{{"query":{},"operationName":"{}","variables":{variables}}}"#,
        serde_json::to_string(O::DOCUMENT).unwrap(),
        O::NAME
    )
}

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

/// A `CatalogProducts` product node; `link`: `None` leaves `onlineStoreUrl` out of the answer
/// (what Shopify sends when it was not requested), `Some(v)` puts it in (`null` or a URL).
fn product(id: u64, title: &str, link: Option<Value>) -> Value {
    let mut node = json!({
        "id": format!("gid://shopify/Product/{id}"),
        "title": title,
        "handle": title.to_lowercase().replace(' ', "-"),
        "status": "ACTIVE",
        "vendor": "Synthetic Pottery",
        "productType": "Mugs",
        "tracksInventory": true,
        "totalInventory": 7,
        "updatedAt": "2026-10-30T08:00:00Z",
        "variants": {"nodes": [{
            "id": format!("gid://shopify/ProductVariant/{id}1"),
            "title": "Default Title",
            "sku": format!("SKU-{id}"),
            "price": "12.50",
            "compareAtPrice": null,
            "inventoryQuantity": 7,
            "position": 1,
            "inventoryItem": {"id": format!("gid://shopify/InventoryItem/{id}1"), "tracked": true}
        }], "pageInfo": {"hasNextPage": false, "endCursor": null}}
    });
    if let Some(link) = link {
        node["onlineStoreUrl"] = link;
    }
    node
}

fn link(handle: &str) -> Value {
    json!(format!(
        "https://synthetic-feed-shop.example/products/{handle}"
    ))
}

fn products_page(nodes: Vec<Value>, next: Option<&str>) -> Reply {
    graphql(json!({"data": {"products": {"nodes": nodes,
        "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next.unwrap_or("Y3Vyc29yLWVuZA==")}}}}))
}

fn budget() -> Budget {
    Budget::new(NonZeroUsize::new(5).unwrap())
}

#[tokio::test]
async fn published_listing_requests_the_link_and_sets_unpublished_products_aside() {
    // Shopify's behaviour: the member only when `withOnlineStoreUrl` is true.
    let server = TestServer::start(|request| {
        if let Err(reply) = shop_of(request) {
            return reply;
        }
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let wanted = body["variables"]["withOnlineStoreUrl"] == json!(true);
        let member = |value: Value| wanted.then_some(value);
        match body["variables"]["after"].as_str() {
            None => products_page(
                vec![
                    product(11, "Blue Mug", member(link("blue-mug"))),
                    product(12, "Red Mug", member(Value::Null)),
                ],
                Some("Y3Vyc29yLTE="),
            ),
            Some(_) => products_page(
                vec![product(13, "Café Mug", member(link("café-mug")))],
                None,
            ),
        }
    })
    .await;
    let sdk = sdk(&server, None);
    let one = shop(&sdk, "shop-one", TOKEN_ONE);

    let listing = list_published_products(&one, PageSize::new(2).unwrap(), budget())
        .await
        .unwrap();

    assert_eq!(listing.outcome, Outcome::Exhausted);
    let listed: Vec<(&str, &str)> = listing
        .listed
        .iter()
        .map(|p| (p.product.title.as_str(), p.link.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            (
                "Blue Mug",
                "https://synthetic-feed-shop.example/products/blue-mug"
            ),
            (
                "Café Mug",
                "https://synthetic-feed-shop.example/products/café-mug"
            ),
        ]
    );
    // `null` is "not published to the online store": set aside, not an error, not a row.
    assert_eq!(listing.unpublished, [gid("gid://shopify/Product/12")]);
    // The frozen row type is reused unchanged (offer from the first variant).
    assert_eq!(
        listing.listed[0]
            .product
            .offer
            .as_ref()
            .unwrap()
            .sku
            .as_deref(),
        Some("SKU-11")
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_graphql_request(request, "shop-one", TOKEN_ONE);
    }
    assert_eq!(
        requests[0].body_text(),
        expected_body::<ProductsQuery>(
            r#"{"first":2,"query":"status:active","variantsFirst":1,"withOnlineStoreUrl":true}"#
        )
    );
    assert_eq!(
        requests[1].body_text(),
        expected_body::<ProductsQuery>(
            r#"{"first":2,"after":"Y3Vyc29yLTE=","query":"status:active","variantsFirst":1,"withOnlineStoreUrl":true}"#
        )
    );
    server.shutdown().await;
}

#[tokio::test]
async fn the_link_member_across_old_and_new_answers() {
    // (a) An answer shaped for the older document (no `onlineStoreUrl`) although the updated
    // consumer asked for it — a replayed recording, a stale fixture: refused, never "unpublished".
    let stale = TestServer::start(|request| match shop_of(request) {
        Ok(_) => products_page(vec![product(21, "Old Mug", None)], None),
        Err(reply) => reply,
    })
    .await;
    let sdk_stale = sdk(&stale, None);
    let two = shop(&sdk_stale, "shop-two", TOKEN_TWO);
    let error = list_published_products(&two, PageSize::new(2).unwrap(), budget())
        .await
        .unwrap_err();
    let UpdatedFeedError::LinkNotIncluded { product: missing } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(missing.as_str(), "gid://shopify/Product/21");
    stale.shutdown().await;

    // (b) The frozen consumer, compiled against the changed SDK, meets answers that carry the
    // member (value and `null`) although it never asks for it: the extra member is decoded into
    // the SDK model and ignored by the consumer; rows are the same as without it.
    let carrying = TestServer::start(|request| match shop_of(request) {
        Ok(_) => products_page(
            vec![
                product(31, "Blue Mug", Some(link("blue-mug"))),
                product(32, "Red Mug", Some(Value::Null)),
            ],
            None,
        ),
        Err(reply) => reply,
    })
    .await;
    let plain = TestServer::start(|request| match shop_of(request) {
        Ok(_) => products_page(
            vec![product(31, "Blue Mug", None), product(32, "Red Mug", None)],
            None,
        ),
        Err(reply) => reply,
    })
    .await;
    let sdk_carrying = sdk(&carrying, None);
    let sdk_plain = sdk(&plain, None);
    let with_member = list_active_products(
        &shop(&sdk_carrying, "shop-one", TOKEN_ONE),
        PageSize::new(2).unwrap(),
        budget(),
    )
    .await
    .unwrap();
    let without_member = list_active_products(
        &shop(&sdk_plain, "shop-one", TOKEN_ONE),
        PageSize::new(2).unwrap(),
        budget(),
    )
    .await
    .unwrap();
    assert_eq!(with_member.products, without_member.products);
    assert_eq!(with_member.products.len(), 2);
    // The frozen consumer's request still omits the flag: its variables are the stage-1 bytes.
    assert_eq!(
        carrying.single().body_text(),
        expected_body::<ProductsQuery>(r#"{"first":2,"query":"status:active","variantsFirst":1}"#)
    );
    carrying.shutdown().await;
    plain.shutdown().await;
}

/// The attribute export's bulk document, written out.
const ATTRIBUTE_EXPORT_DOCUMENT: &str = "{ products(query: \"status:active\") { edges { node { id \
     title handle status vendor updatedAt variants { edges { node { id title sku price \
     inventoryQuantity inventoryItem { id tracked } } } } metafields(namespace: \
     \"mm-google-shopping\") { edges { node { id namespace key value type } } } } } } }";

/// Two products, their variants and their feed metafields; children interleaved (only "after
/// their parents" is documented).
const ATTRIBUTE_EXPORT: &str = concat!(
    r#"{"id":"gid://shopify/Product/21","title":"Café Mug","handle":"cafe-mug","status":"ACTIVE","vendor":"Synthetic Pottery","updatedAt":"2026-10-30T08:00:00Z"}"#,
    "\n",
    r#"{"id":"gid://shopify/ProductVariant/211","title":"S","sku":"CAFE-S","price":"9.90","inventoryQuantity":3,"inventoryItem":{"id":"gid://shopify/InventoryItem/211","tracked":true},"__parentId":"gid://shopify/Product/21"}"#,
    "\n",
    r#"{"id":"gid://shopify/Metafield/7001","namespace":"mm-google-shopping","key":"gender","value":"unisex","type":"single_line_text_field","__parentId":"gid://shopify/Product/21"}"#,
    "\n",
    r#"{"id":"gid://shopify/Product/22","title":"日本の湯呑","handle":"yunomi","status":"ACTIVE","vendor":"Synthetic Pottery","updatedAt":"2026-10-30T08:00:00Z"}"#,
    "\n",
    r#"{"id":"gid://shopify/Metafield/7002","namespace":"mm-google-shopping","key":"google_product_category","value":"Home & Garden > Kitchen & Dining > Tableware","type":"single_line_text_field","__parentId":"gid://shopify/Product/22"}"#,
    "\n",
    r#"{"id":"gid://shopify/Metafield/7003","namespace":"mm-google-shopping","key":"age_group","value":"adult","type":"single_line_text_field","__parentId":"gid://shopify/Product/21"}"#,
    "\n",
    r#"{"id":"gid://shopify/ProductVariant/221","title":"Default Title","sku":null,"price":"24.00","inventoryQuantity":null,"inventoryItem":{"id":"gid://shopify/InventoryItem/221","tracked":false},"__parentId":"gid://shopify/Product/22"}"#,
    "\n",
);

fn bulk_operation(status: &str, url: Option<String>) -> Value {
    json!({
        "id": "gid://shopify/BulkOperation/9002",
        "status": status,
        "errorCode": null,
        "createdAt": "2026-11-01T10:00:00Z",
        "completedAt": if status == "COMPLETED" { json!("2026-11-01T10:02:00Z") } else { Value::Null },
        "objectCount": "7",
        "rootObjectCount": "2",
        "fileSize": if url.is_some() { json!("2048") } else { Value::Null },
        "url": url,
        "partialDataUrl": null,
        "type": "QUERY"
    })
}

/// A shops server answering the bulk submission and then the status check, and a storage
/// server holding `file`.
async fn bulk_servers(file: &'static str, submit: bool) -> (TestServer, TestServer) {
    let storage = TestServer::start(move |_| Reply::text(200, "application/jsonl", file)).await;
    let url = format!("http://{STORAGE}:{}{SIGNED_PATH}", storage.addr().port());
    let mut replies = Vec::new();
    if submit {
        replies.push(graphql(json!({"data": {"bulkOperationRunQuery": {
            "bulkOperation": bulk_operation("CREATED", None), "userErrors": []}}})));
    }
    replies.push(graphql(
        json!({"data": {"bulkOperation": bulk_operation("COMPLETED", Some(url))}}),
    ));
    let script = Script::new(replies);
    let shops = TestServer::start(move |request| match shop_of(request) {
        Ok(_) => script.next(),
        Err(reply) => reply,
    })
    .await;
    (shops, storage)
}

#[tokio::test]
async fn attribute_export_streams_products_offers_and_metafields_by_parent() {
    let (shops, storage) = bulk_servers(ATTRIBUTE_EXPORT, true).await;
    let sdk = sdk(&shops, Some(&storage));
    let two = shop(&sdk, "shop-two.myshopify.com", TOKEN_TWO);

    let id = submit_attribute_export(&two).await.unwrap();
    let mut lines = Vec::new();
    let state = stream_attribute_export(&two, &id, |line| lines.push(line))
        .await
        .unwrap();

    let AttributeState::Streamed(report) = state else {
        panic!("{state:?}");
    };
    assert_eq!(
        (
            report.feed.products,
            report.feed.offers,
            report.attributes,
            report.unknown
        ),
        (2, 2, 3, 0)
    );
    assert_eq!(report.feed.position.records, 7);
    assert_eq!(report.feed.position.bytes, ATTRIBUTE_EXPORT.len() as u64);
    // Product and offer lines arrive as the frozen consumer's `FeedLine`s.
    assert!(
        matches!(&lines[0], AttributeLine::Feed(FeedLine::Product { title, .. }) if title == "Café Mug")
    );
    assert!(matches!(
        &lines[6],
        AttributeLine::Feed(FeedLine::Offer { sku: None, .. })
    ));
    // Each attribute names its product by `__parentId`, whatever line came just before it.
    let attributes: Vec<(&str, &str, &str)> = lines
        .iter()
        .filter_map(|line| match line {
            AttributeLine::Attribute {
                product_id,
                key,
                value,
                value_type,
            } => {
                assert_eq!(value_type, "single_line_text_field");
                Some((product_id.tail(), key.as_str(), value.as_str()))
            }
            AttributeLine::Feed(_) => None,
        })
        .collect();
    assert_eq!(
        attributes,
        [
            ("21", "gender", "unisex"),
            (
                "22",
                "google_product_category",
                "Home & Garden > Kitchen & Dining > Tableware"
            ),
            ("21", "age_group", "adult"),
        ]
    );

    let requests = shops.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_graphql_request(request, "shop-two", TOKEN_TWO);
    }
    assert_eq!(attribute_export().document(), ATTRIBUTE_EXPORT_DOCUMENT);
    assert!(ATTRIBUTE_EXPORT_DOCUMENT.contains(FEED_NAMESPACE));
    assert_eq!(
        requests[0].body_text(),
        expected_body::<RunBulkQuery>(&json!({"query": ATTRIBUTE_EXPORT_DOCUMENT}).to_string())
    );
    assert_eq!(
        requests[1].body_text(),
        expected_body::<BulkOperationQuery>(r#"{"id":"gid://shopify/BulkOperation/9002"}"#)
    );
    let download = storage.single();
    assert_eq!(download.target, SIGNED_PATH);
    assert!(download.header_all("x-shopify-access-token").is_empty());
    storage.shutdown().await;
    shops.shutdown().await;
}

#[tokio::test]
async fn the_frozen_reader_refuses_a_metafield_line_at_its_position() {
    // The frozen consumer attached to an attribute export (a selection it did not build) reads
    // the file with `BulkRecord`: the product and the offer before the first metafield line are
    // delivered, then that line is a Decode error of a line of an unselected type.
    let (shops, storage) = bulk_servers(ATTRIBUTE_EXPORT, false).await;
    let sdk = sdk(&shops, Some(&storage));
    let one = shop(&sdk, "shop-one", TOKEN_ONE);
    let id: BulkOperationId = gid("gid://shopify/BulkOperation/9002");

    let mut lines = Vec::new();
    let error = stream_feed_export(&one, &id, |line| lines.push(line))
        .await
        .unwrap_err();
    let FeedError::Read(read) = &error else {
        panic!("{error:?}");
    };
    assert_eq!(read.kind(), ReadErrorKind::Decode);
    assert_eq!(read.position().records, 2);
    assert_eq!(lines.len(), 2);
    assert_eq!(
        read.source_unredacted().unwrap().to_string(),
        "a line of an object type the selection does not contain"
    );
    let shown = format!("{error} {error:?}");
    assert!(
        !shown.contains("SYNTHETIC_SIG") && !shown.contains("unisex"),
        "{shown}"
    );
    storage.shutdown().await;
    shops.shutdown().await;
}
