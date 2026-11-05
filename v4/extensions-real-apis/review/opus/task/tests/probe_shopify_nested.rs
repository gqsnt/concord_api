//! Probe (review, not part of the caller task): SH-A when a *nested* variant traversal fails
//! in the middle of a products page. Uses the SH-A fixtures of
//! `examples/shopify-app/tests/fixtures/sh_a` (read only) and a copy of the SH-A test routing.

use std::num::NonZeroUsize;

use concord::reqwest;
use concord_example_shopify_app::catalog_inventory::{ReconcileRequest, Stage, reconcile};
use concord_example_shopify_sdk::{AccessToken, Cursor, ShopDomain, ShopifyContext};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::{Budget, Outcome, Resume};
use serde_json::Value;

const TOKEN_ONE: &str = "shpat_synthetic_shop_one_TOKEN_0001";

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/../../../examples/shopify-app/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap()
}

fn route(op: &str, after: Option<&str>) -> Option<&'static str> {
    Some(match (op, after) {
        ("Locations", None) => "sh_a/one_locations.json",
        ("CatalogProducts", None) => "sh_a/one_products_p1.json",
        ("CatalogProducts", Some("cHJvZC0y")) => "sh_a/one_products_p2.json",
        // Product 101's nested variants, page 2: THROTTLED (fault).
        ("ProductVariants", Some("dmFyLXAxLTI=")) => "sh_a/fault_products_throttled.json",
        ("ProductVariants", Some("dmFyLXAxLTM=")) => "sh_a/one_variants_p101_b.json",
        ("CatalogCollections", None) => "sh_a/one_collections.json",
        ("CollectionProducts", Some("Y29sLTUwMS0x")) => "sh_a/one_collection_products_501.json",
        ("LocationInventory", None) => "sh_a/one_inventory_p1.json",
        ("LocationInventory", Some("aW52LTI=")) => "sh_a/one_inventory_p2.json",
        _ => return None,
    })
}

#[tokio::test]
async fn nested_failure_reports_next_parent_page_as_resume_and_skips_rest_of_page() {
    let server = TestServer::start(|r: &Recorded| {
        let body: Value = serde_json::from_slice(&r.body).unwrap();
        let op = body["operationName"].as_str().unwrap_or_default().to_owned();
        let after = body["variables"]["after"].as_str().map(str::to_owned);
        match route(&op, after.as_deref()) {
            Some(name) => Reply::json(200, fixture(name)).header("x-shopify-api-version", "2026-10"),
            None => Reply::json(500, "{}"),
        }
    })
    .await;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve("shop-one.myshopify.com", server.addr())
        .build()
        .unwrap();
    let sdk = ShopifyContext::new(http)
        .with_base_url(format!("http://myshopify.com:{}", server.addr().port()))
        .unwrap();
    let shop = sdk.shop(
        ShopDomain::new("shop-one").unwrap(),
        &AccessToken::new(TOKEN_ONE).unwrap(),
    );
    let request = ReconcileRequest::new(Budget::new(NonZeroUsize::new(10).unwrap()));
    let report = reconcile(&shop, &request).await;

    let Stage::Ran(products) = &report.products else { panic!("products ran") };
    let resume = products
        .resume
        .as_ref()
        .and_then(Resume::key)
        .map(Cursor::expose)
        .map(str::to_owned);
    println!(
        "products: outcome={:?} pages={} resume={:?} failure={}",
        products.outcome,
        products.pages,
        resume,
        products.failure.is_some()
    );
    println!("counts.products={}", report.counts.products);
    println!(
        "variant_pages.incomplete={:?}",
        report
            .variant_pages
            .incomplete
            .iter()
            .map(|(p, s)| (format!("{p:?}"), s.outcome))
            .collect::<Vec<_>>()
    );
    let products_requests: Vec<_> = server
        .requests()
        .iter()
        .map(|r| {
            let b: Value = serde_json::from_slice(&r.body).unwrap();
            format!(
                "{}({})",
                b["operationName"].as_str().unwrap_or(""),
                b["variables"]["after"].as_str().unwrap_or("-")
            )
        })
        .collect();
    println!("requests: {products_requests:?}");
    // Observed: the products traversal is `Stopped` with the *next* products page as its resume
    // point and no failure of its own, although product 102 (after 101 on page 1) was never
    // processed.
    assert_eq!(products.outcome, Outcome::Stopped);
    assert_eq!(resume.as_deref(), Some("cHJvZC0y"));
    assert!(products.failure.is_none());
    assert_eq!(report.counts.products, 1, "product 102 of page 1 was skipped");
    server.shutdown().await;
}
