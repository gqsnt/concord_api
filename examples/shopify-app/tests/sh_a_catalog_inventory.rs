//! SH-A end to end: the reconciliation workflow against two loopback shops serving the
//! schema-derived fixtures of `tests/fixtures/sh_a` (see `MANIFEST.json`).
//!
//! `cargo test -p concord-example-shopify-app --test sh_a_catalog_inventory`

mod support;

use std::num::NonZeroUsize;

use concord_example_shopify_app::catalog_inventory::{
    Discrepancy, NotRun, PartialData, ReconcileRequest, ReconcileStatus, Stage, SyncFailure,
    reconcile,
};
use concord_example_shopify_sdk::connection::{DataShape, Protocol};
use concord_example_shopify_sdk::scalars::{
    CollectionId, InventoryItemId, InventoryLevelId, LocationId, ProductId, ProductVariantId,
};
use concord_example_shopify_sdk::{Cursor, ErrorCode, PageError, RequestError, SearchQuery};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::{Budget, Outcome, Resume};
use support::*;

/// `(shop, operationName, after)` → fixture, for the regular dataset.
fn route(shop: &str, op: &str, after: Option<&str>) -> Option<&'static str> {
    Some(match (shop, op, after) {
        ("one", "Locations", None) => "sh_a/one_locations.json",
        ("one", "CatalogProducts", None) => "sh_a/one_products_p1.json",
        ("one", "CatalogProducts", Some("cHJvZC0y")) => "sh_a/one_products_p2.json",
        ("one", "ProductVariants", Some("dmFyLXAxLTI=")) => "sh_a/one_variants_p101_a.json",
        ("one", "ProductVariants", Some("dmFyLXAxLTM=")) => "sh_a/one_variants_p101_b.json",
        ("one", "CatalogCollections", None) => "sh_a/one_collections.json",
        ("one", "CollectionProducts", Some("Y29sLTUwMS0x")) => {
            "sh_a/one_collection_products_501.json"
        }
        ("one", "LocationInventory", None) => "sh_a/one_inventory_p1.json",
        ("one", "LocationInventory", Some("aW52LTI=")) => "sh_a/one_inventory_p2.json",
        ("two", "Locations", None) => "sh_a/two_locations.json",
        ("two", "CatalogProducts", None) => "sh_a/two_products.json",
        ("two", "CatalogCollections", None) => "sh_a/two_collections.json",
        ("two", "LocationInventory", None) => "sh_a/two_inventory.json",
        _ => return None,
    })
}

type Fault = (
    &'static str,
    &'static str,
    Option<&'static str>,
    &'static str,
);

/// A loopback server for both shops; `faults` replace single answers.
async fn shops(faults: Vec<Fault>) -> TestServer {
    TestServer::start(move |request: &Recorded| {
        let shop = match shop_of(request) {
            Ok(shop) => shop,
            Err(reply) => return reply,
        };
        let op = operation(request);
        let after = variable(request, "after");
        let fault = faults
            .iter()
            .find(|(s, o, a, _)| *s == shop && *o == op && a.map(str::to_owned) == after);
        match fault
            .map(|f| f.3)
            .or_else(|| route(shop, &op, after.as_deref()))
        {
            Some(name) => graphql_fixture(name, None),
            None => Reply::json(500, format!("{{\"unrouted\":\"{op}\"}}")),
        }
    })
    .await
}

fn budget(n: usize) -> Budget {
    Budget::new(NonZeroUsize::new(n).unwrap())
}

fn summary(stage: &Stage) -> &concord_example_shopify_app::catalog_inventory::TraversalSummary {
    stage.summary().expect("the stage ran")
}

fn cursor_of(resume: &Option<Resume<Cursor>>) -> Option<&str> {
    resume.as_ref().and_then(Resume::key).map(Cursor::expose)
}

fn pid(n: u32) -> ProductId {
    ProductId::parse(&format!("gid://shopify/Product/{n}")).unwrap()
}
fn vid(n: u32) -> ProductVariantId {
    ProductVariantId::parse(&format!("gid://shopify/ProductVariant/{n}")).unwrap()
}
fn item(n: u32) -> InventoryItemId {
    InventoryItemId::parse(&format!("gid://shopify/InventoryItem/{n}")).unwrap()
}

#[tokio::test]
async fn sh_a_reconciles_catalog_and_inventory_across_dependent_and_nested_cursors() {
    let server = shops(Vec::new()).await;
    let sdk = sdk(&server, None);
    let shop = shop_one(&sdk);
    let report = reconcile(&shop, &ReconcileRequest::new(budget(10))).await;

    // Wire: one shop, one token, nine requests in dependency order, each next request built from
    // the previous page's endCursor (top-level and nested).
    let requests = server.requests();
    let calls: Vec<(String, Option<String>)> = requests
        .iter()
        .map(|r| (operation(r), variable(r, "after")))
        .collect();
    let expected: Vec<(&str, Option<&str>)> = vec![
        ("Locations", None),
        ("CatalogProducts", None),
        ("ProductVariants", Some("dmFyLXAxLTI=")),
        ("ProductVariants", Some("dmFyLXAxLTM=")),
        ("CatalogProducts", Some("cHJvZC0y")),
        ("CatalogCollections", None),
        ("CollectionProducts", Some("Y29sLTUwMS0x")),
        ("LocationInventory", None),
        ("LocationInventory", Some("aW52LTI=")),
    ];
    let expected: Vec<(String, Option<String>)> = expected
        .into_iter()
        .map(|(o, a)| (o.to_owned(), a.map(str::to_owned)))
        .collect();
    assert_eq!(calls, expected);
    for r in &requests {
        assert_eq!(r.method, "POST");
        assert_eq!(r.path(), "/admin/api/2026-10/graphql.json");
        assert!(r.host.starts_with("shop-one.myshopify.com:"));
        assert_eq!(r.header_all("x-shopify-access-token"), [TOKEN_ONE]);
    }
    // The first page omits `after` (absent, not null); the nested query names its product.
    assert!(body(&requests[1])["variables"].get("after").is_none());
    assert_eq!(
        variable(&requests[2], "id").as_deref(),
        Some("gid://shopify/Product/101")
    );
    let inventory = body(&requests[7]);
    assert_eq!(inventory["variables"]["id"], "gid://shopify/Location/1001");
    assert_eq!(
        inventory["variables"]["names"],
        serde_json::json!(["available", "on_hand"])
    );

    // Traversals: all exhausted, nested ones counted.
    for stage in [
        &report.locations,
        &report.products,
        &report.collections,
        &report.inventory,
    ] {
        let s = summary(stage);
        assert_eq!(s.outcome, Outcome::Exhausted, "{s:?}");
        assert!(s.resume.is_none() && s.failure.is_none());
    }
    assert_eq!(summary(&report.products).pages, 2);
    assert_eq!(
        (report.variant_pages.traversals, report.variant_pages.pages),
        (1, 2)
    );
    assert_eq!(
        (report.member_pages.traversals, report.member_pages.pages),
        (1, 1)
    );
    let (location, node) = report.location.as_ref().unwrap();
    assert_eq!(location.as_str(), "gid://shopify/Location/1001");
    assert_eq!(node.as_ref().unwrap().address.city, None); // nullable, sent as null

    let c = report.counts;
    assert_eq!(
        (
            c.locations,
            c.products,
            c.variants,
            c.collections,
            c.memberships,
            c.levels
        ),
        (1, 3, 6, 2, 2, 5)
    );
    assert_eq!(c.unit_costs_withheld, 2);

    // Partial data accepted: the two per-field ACCESS_DENIED errors are kept with their paths.
    assert_eq!(report.status, ReconcileStatus::CompleteWithErrors);
    assert_eq!(report.accepted_errors.len(), 2);
    for (i, accepted) in report.accepted_errors.iter().enumerate() {
        assert_eq!(accepted.operation, "LocationInventory");
        assert_eq!(accepted.error.code(), Some(&ErrorCode::AccessDenied));
        assert_eq!(
            accepted.error.to_string(),
            format!(
                "GraphQL error ACCESS_DENIED at location.inventoryLevels.nodes.{i}.item.unitCost"
            )
        );
    }

    // Cost metadata of all nine responses.
    assert_eq!(report.cost.responses, 9);
    assert_eq!(
        report.cost.requested,
        4.0 + 52.0 * 4.0 + 102.0 * 2.0 + 104.0 * 2.0
    );
    assert_eq!(
        report.cost.actual,
        3.0 + 14.0 + 4.0 + 4.0 + 8.0 + 9.0 + 3.0 + 12.0 + 14.0
    );
    assert_eq!(report.cost.lowest_available, Some(1929.0));
    assert_eq!(report.cost.last.unwrap().restore_rate, 100.0);

    let level =
        InventoryLevelId::parse("gid://shopify/InventoryLevel/39?inventory_item_id=2999").unwrap();
    assert_eq!(
        report.discrepancies,
        vec![
            Discrepancy::TrackedNotStocked {
                product: pid(101),
                variant: vid(1103),
                item: item(2103)
            },
            Discrepancy::NegativeAvailable {
                item: item(2104),
                available: -1
            },
            Discrepancy::QuantityMismatch {
                variant: vid(1102),
                variant_quantity: 3,
                location_available: 2
            },
            Discrepancy::MissingSku { variant: vid(1201) },
            Discrepancy::DuplicateSku {
                sku: "SYN-TEE-S".into(),
                variants: vec![vid(1101), vid(1104)]
            },
            Discrepancy::LevelWithoutVariant {
                level,
                item: item(2999)
            },
            Discrepancy::ProductInNoCollection { product: pid(102) },
        ]
    );

    // The report's diagnostics never show the token or a cursor.
    let shown = format!("{report:?}");
    for secret in [
        TOKEN_ONE,
        "cHJvZC0y",
        "dmFyLXAxLTI=",
        "aW52LTI=",
        "read_product_costs",
    ] {
        assert!(!shown.contains(secret), "{secret} in {shown}");
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn sh_a_two_shops_reconcile_independently_and_encode_their_filters() {
    let server = shops(Vec::new()).await;
    let sdk = sdk(&server, None);
    let (one, two) = (shop_one(&sdk), shop_two(&sdk));

    let mut filtered = ReconcileRequest::new(budget(10));
    filtered.product_filter = SearchQuery::new()
        .term("vendor", "O'Brien \"Lights\"")
        .term("title", "Lämp ☀");
    filtered.location = Some(LocationId::parse("gid://shopify/Location/2001").unwrap());
    let (a, b) = (
        reconcile(&one, &ReconcileRequest::new(budget(10))).await,
        reconcile(&two, &filtered).await,
    );
    assert_eq!(a.counts.products, 3);
    assert_eq!(b.counts.products, 1);
    assert_eq!(b.status, ReconcileStatus::Complete);
    assert!(b.discrepancies.is_empty(), "{:?}", b.discrepancies);
    assert_eq!(b.shop.handle(), "shop-two");

    // Every request carried exactly its own shop's token (the responder answers 401 otherwise).
    let requests = server.requests();
    assert_eq!(requests.len(), 9 + 4);
    let two_products = requests
        .iter()
        .find(|r| r.host.starts_with("shop-two.") && operation(r) == "CatalogProducts")
        .unwrap();
    assert_eq!(
        two_products.header_all("x-shopify-access-token"),
        [TOKEN_TWO]
    );
    // The search term travels as a JSON string: quotes escaped by the search syntax, then by JSON;
    // non-ASCII as UTF-8.
    assert_eq!(
        variable(two_products, "query").as_deref(),
        Some("vendor:\"O'Brien \\\"Lights\\\"\" title:\"Lämp ☀\"")
    );
    assert!(
        two_products
            .body_text()
            .contains(r#""query":"vendor:\"O'Brien \\\"Lights\\\"\" title:\"Lämp ☀\"""#)
    );
    server.shutdown().await;
}

#[tokio::test]
async fn sh_a_rejected_partial_data_fails_at_that_page_with_its_errors() {
    let server = shops(Vec::new()).await;
    let sdk = sdk(&server, None);
    let mut request = ReconcileRequest::new(budget(10));
    request.partial_data = PartialData::Reject;
    let report = reconcile(&shop_one(&sdk), &request).await;

    let inventory = summary(&report.inventory);
    assert_eq!(inventory.outcome, Outcome::Failed);
    assert_eq!(inventory.pages, 0);
    assert_eq!(inventory.resume, Some(Resume::Start)); // the refused page, not the next one
    let Some(SyncFailure::PartialRejected { errors, cost }) = &inventory.failure else {
        panic!("{inventory:?}");
    };
    assert_eq!(errors.len(), 2);
    assert_eq!(cost.as_ref().unwrap().actual_query_cost, Some(12.0));
    assert!(report.accepted_errors.is_empty());
    assert_eq!(report.status, ReconcileStatus::Incomplete);
    // Without the complete level listing, "tracked but not stocked" is not claimed.
    assert!(
        !report
            .discrepancies
            .iter()
            .any(|d| matches!(d, Discrepancy::TrackedNotStocked { .. }))
    );
    assert_eq!(report.cost.responses, 8);
    server.shutdown().await;
}

#[tokio::test]
async fn sh_a_throttled_page_stops_the_run_with_bucket_state_and_resume_cursor() {
    let server = shops(vec![(
        "one",
        "CatalogProducts",
        Some("cHJvZC0y"),
        "sh_a/fault_products_throttled.json",
    )])
    .await;
    let sdk = sdk(&server, None);
    let report = reconcile(&shop_one(&sdk), &ReconcileRequest::new(budget(10))).await;

    let products = summary(&report.products);
    assert_eq!(products.outcome, Outcome::Failed);
    assert_eq!(products.pages, 1);
    assert_eq!(cursor_of(&products.resume), Some("cHJvZC0y"));
    let failure = products.failure.as_ref().unwrap();
    assert!(failure.is_throttled());
    let bucket = failure.throttle_status().unwrap();
    assert_eq!(
        (bucket.currently_available, bucket.restore_rate),
        (151.0, 100.0)
    );
    assert_eq!(bucket.seconds_until_available(752.0), Some(6.01));
    assert!(matches!(
        failure,
        SyncFailure::Page(PageError::NoConnection {
            data: DataShape::Absent,
            ..
        })
    ));
    assert!(matches!(
        report.collections,
        Stage::NotRun(NotRun::EarlierFailure)
    ));
    assert!(matches!(
        report.inventory,
        Stage::NotRun(NotRun::EarlierFailure)
    ));
    assert_eq!(report.cost.not_executed, 1);
    assert_eq!(report.status, ReconcileStatus::Incomplete);
    let shown = format!("{failure} {failure:?}");
    assert!(
        !shown.contains("cHJvZC0y") && shown.contains("THROTTLED"),
        "{shown}"
    );
    // No retry: the throttled page was requested once and nothing followed it.
    assert_eq!(server.requests().len(), 5);
    server.shutdown().await;
}

/// A nested traversal fails in the middle of a parent page (OPUS-6). Products page 1 holds 101
/// (whose variants continue) and 102; 101's second variant page is THROTTLED. The products
/// summary is `Stopped` with page 1 itself as the resume point (not page 2, which would skip
/// 102), no failure of its own, and 102 is not counted; the failure is the nested one.
/// Collections page 1 (the last page) holds 501 (whose members continue) and 502: a failed
/// member page must not let the collections traversal report `Exhausted`.
#[tokio::test]
async fn sh_a_nested_failure_mid_page_resumes_at_the_interrupted_parent_page() {
    let server = shops(vec![(
        "one",
        "ProductVariants",
        Some("dmFyLXAxLTI="),
        "sh_a/fault_products_throttled.json",
    )])
    .await;
    let client = sdk(&server, None);
    let report = reconcile(&shop_one(&client), &ReconcileRequest::new(budget(10))).await;

    let products = summary(&report.products);
    assert_eq!(products.outcome, Outcome::Stopped);
    assert_eq!(products.pages, 1);
    assert_eq!(products.resume, Some(Resume::Start), "page 1, not page 2");
    assert!(products.failure.is_none());
    assert_eq!(report.counts.products, 1, "102 was not processed");
    let (parent, nested) = &report.variant_pages.incomplete[0];
    assert_eq!(parent, &pid(101));
    assert_eq!(nested.outcome, Outcome::Failed);
    assert_eq!(cursor_of(&nested.resume), Some("dmFyLXAxLTI="));
    assert!(nested.failure.as_ref().unwrap().is_throttled());
    assert!(matches!(
        report.collections,
        Stage::NotRun(NotRun::EarlierFailure)
    ));
    assert_eq!(report.status, ReconcileStatus::Incomplete);
    let operations: Vec<String> = server.requests().iter().map(operation).collect();
    assert_eq!(
        operations,
        ["Locations", "CatalogProducts", "ProductVariants"],
        "nothing after the failed nested page"
    );
    server.shutdown().await;

    let server = shops(vec![(
        "one",
        "CollectionProducts",
        Some("Y29sLTUwMS0x"),
        "sh_a/fault_products_throttled.json",
    )])
    .await;
    let client = sdk(&server, None);
    let report = reconcile(&shop_one(&client), &ReconcileRequest::new(budget(10))).await;
    assert!(summary(&report.products).is_exhausted());
    let collections = summary(&report.collections);
    assert_eq!(collections.outcome, Outcome::Stopped, "not Exhausted");
    assert_eq!(collections.resume, Some(Resume::Start));
    assert!(collections.failure.is_none());
    assert_eq!(report.counts.collections, 1, "502 was not processed");
    let (parent, nested) = &report.member_pages.incomplete[0];
    assert_eq!(
        parent,
        &CollectionId::parse("gid://shopify/Collection/501").unwrap()
    );
    assert_eq!(nested.outcome, Outcome::Failed);
    assert!(matches!(
        report.inventory,
        Stage::NotRun(NotRun::EarlierFailure)
    ));
    server.shutdown().await;
}

#[tokio::test]
async fn sh_a_http_200_errors_without_data_absent_fields_and_bad_page_info_are_failures() {
    type Expect = fn(&SyncFailure) -> bool;
    let cases: [(&str, Expect); 3] = [
        ("sh_a/fault_products_access_denied.json", |f| {
            matches!(f, SyncFailure::Page(PageError::NoConnection { data: DataShape::Null, errors, .. })
                if errors[0].code() == Some(&ErrorCode::AccessDenied))
        }),
        ("sh_a/fault_products_missing_field.json", |f| {
            matches!(f, SyncFailure::Page(PageError::Request(RequestError::Concord(e)))
                if e.kind() == concord::ErrorKind::Decode)
        }),
        ("sh_a/fault_products_next_without_cursor.json", |f| {
            matches!(
                f,
                SyncFailure::Page(PageError::Protocol {
                    violation: Protocol::NextWithoutCursor,
                    ..
                })
            )
        }),
    ];
    for (fixture, expected) in cases {
        let server = shops(vec![("one", "CatalogProducts", None, fixture)]).await;
        let sdk = sdk(&server, None);
        let report = reconcile(&shop_one(&sdk), &ReconcileRequest::new(budget(10))).await;
        let products = summary(&report.products);
        assert_eq!(products.outcome, Outcome::Failed, "{fixture}");
        assert_eq!(products.resume, Some(Resume::Start), "{fixture}");
        let failure = products.failure.as_ref().unwrap();
        assert!(expected(failure), "{fixture}: {failure:?}");
        assert!(matches!(
            report.inventory,
            Stage::NotRun(NotRun::EarlierFailure)
        ));
        server.shutdown().await;
    }
}

#[tokio::test]
async fn sh_a_page_budget_bounds_every_traversal_with_a_resume_cursor() {
    let server = shops(Vec::new()).await;
    let sdk = sdk(&server, None);
    let report = reconcile(&shop_one(&sdk), &ReconcileRequest::new(budget(1))).await;

    let products = summary(&report.products);
    assert_eq!(products.outcome, Outcome::Bounded);
    assert_eq!(cursor_of(&products.resume), Some("cHJvZC0y"));
    let (parent, nested) = &report.variant_pages.incomplete[0];
    assert_eq!(parent, &pid(101));
    assert_eq!(nested.outcome, Outcome::Bounded);
    assert_eq!(cursor_of(&nested.resume), Some("dmFyLXAxLTM="));
    assert_eq!(summary(&report.inventory).outcome, Outcome::Bounded);
    assert_eq!(report.status, ReconcileStatus::Incomplete);
    // Checks that need the complete level listing or catalog are not made from a partial one;
    // the others hold for what was seen (memberships were complete: one page each).
    assert_eq!(
        report.discrepancies,
        vec![
            Discrepancy::QuantityMismatch {
                variant: vid(1102),
                variant_quantity: 3,
                location_available: 2
            },
            Discrepancy::MissingSku { variant: vid(1201) },
            Discrepancy::ProductInNoCollection { product: pid(102) },
        ]
    );
    server.shutdown().await;
}
