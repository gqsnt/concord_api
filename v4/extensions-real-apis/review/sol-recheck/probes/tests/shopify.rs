use concord::reqwest::Client;
use concord_example_shopify_app::catalog_inventory::{reconcile, ReconcileRequest, Stage};
use concord_example_shopify_sdk::{ShopifyContext, AccessToken, ShopDomain, Cursor, connection_page};
use concord_example_shopify_sdk::ops::catalog::{ProductsQuery, CollectionsQuery};
use concord_example_shopify_sdk::ops::PageSize;
use concord_example_testserver::{TestServer, Reply, Recorded};
use concord_pagination::{Budget, Resume, Outcome, sequential};
use serde_json::{json, Value};
use std::num::NonZeroUsize;

fn fixture(name: &str) -> Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/shopify-app/tests/fixtures/sh_a").join(name);
    serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
}
fn budget() -> Budget { Budget::new(NonZeroUsize::new(20).unwrap()) }
fn conn(nodes: Vec<Value>, more: bool, end: &str) -> Value {
    json!({"nodes":nodes,"pageInfo":{"hasNextPage":more,"endCursor":end}})
}
fn reply(data: Value) -> Reply { Reply::json(200, data.to_string()).header("x-shopify-api-version", "2026-10") }
fn parent_page(kind: &str, page: usize) -> Value {
    let (template, nested, gid) = if kind == "products" {
        (fixture("one_products_p1.json")["data"]["products"]["nodes"][0].clone(), "variants", "Product")
    } else {
        (fixture("one_collections.json")["data"]["collections"]["nodes"][0].clone(), "products", "Collection")
    };
    let nodes = (0..3).map(|i| {
        let id = 100 + page*100 + i;
        let mut node = template.clone();
        node["id"] = json!(format!("gid://shopify/{gid}/{id}"));
        node[nested] = conn(vec![], true, &format!("nested-{id}"));
        node
    }).collect();
    json!({"data":{kind:conn(nodes, page == 0, "parent-page-2")}})
}

#[tokio::test]
async fn nested_failure_matrix_and_actual_parent_replay() {
    let size = PageSize::new(3).unwrap();
    for kind in ["products", "collections"] {
        for failed_page in 0..2 {
            for failed_item in 0..3 {
                let fail_id = 100 + failed_page*100 + failed_item;
                let server = TestServer::start(move |r: &Recorded| {
                    let body: Value = serde_json::from_slice(&r.body).unwrap();
                    let op = body["operationName"].as_str().unwrap();
                    let after = body["variables"]["after"].as_str();
                    match op {
                        "Locations" => reply(fixture("one_locations.json")),
                        "CatalogProducts" if kind == "products" => reply(parent_page(kind, usize::from(after.is_some()))),
                        "CatalogProducts" => reply(json!({"data":{"products":conn(vec![],false,"none")}})),
                        "CatalogCollections" if kind == "collections" => reply(parent_page(kind, usize::from(after.is_some()))),
                        "CatalogCollections" => reply(json!({"data":{"collections":conn(vec![],false,"none")}})),
                        "ProductVariants" | "CollectionProducts" => {
                            let id = body["variables"]["id"].as_str().unwrap();
                            if id.ends_with(&format!("/{fail_id}")) {
                                reply(json!({"errors":[{"message":"synthetic fault","extensions":{"code":"THROTTLED"}}]}))
                            } else if op == "ProductVariants" {
                                reply(json!({"data":{"product":{"id":id,"variants":conn(vec![],false,"done")}}}))
                            } else {
                                reply(json!({"data":{"collection":{"id":id,"products":conn(vec![],false,"done")}}}))
                            }
                        }
                        _ => Reply::new(500),
                    }
                }).await;
                let sdk = ShopifyContext::new(Client::builder().no_proxy().resolve("shop-one.myshopify.com", server.addr()).build().unwrap())
                    .with_base_url(format!("http://myshopify.com:{}",server.addr().port())).unwrap();
                let shop = sdk.shop(ShopDomain::new("shop-one").unwrap(), &AccessToken::new("synthetic").unwrap());
                let report = reconcile(&shop, &ReconcileRequest::new(budget())).await;
                let stage = if kind == "products" { &report.products } else { &report.collections };
                let s = stage.summary().unwrap();
                assert_eq!(s.outcome, Outcome::Stopped, "{kind} {failed_page} {failed_item}: {s:?}");
                assert_eq!(s.pages, (failed_page+1) as u64);
                assert!(s.failure.is_none());
                let expected = if failed_page == 0 { Resume::Start } else { Resume::At(Cursor::new("parent-page-2")) };
                assert_eq!(s.resume.as_ref(), Some(&expected));
                let count = if kind == "products" { report.counts.products } else { report.counts.collections };
                assert_eq!(count, (failed_page*3+failed_item+1) as u64);
                let incomplete = if kind == "products" { report.variant_pages.incomplete.len() } else { report.member_pages.incomplete.len() };
                assert_eq!(incomplete, 1);
                assert!(matches!(report.inventory, Stage::NotRun(_)));
                let nested_op = if kind == "products" { "ProductVariants" } else { "CollectionProducts" };
                let processed: Vec<_> = server.requests().iter().filter_map(|r| {
                    let body: Value = serde_json::from_slice(&r.body).unwrap();
                    (body["operationName"] == nested_op).then(|| body["variables"]["id"].as_str().unwrap().to_owned())
                }).collect();
                assert_eq!(processed.len(), count as usize);

                // The app accepts no resume input. Use its public SDK with exactly the reported
                // parent checkpoint to establish the semantics available to a caller.
                let mut replayed = Vec::new();
                if kind == "products" {
                    let mut pages = sequential(s.resume.clone().unwrap(), budget(), |cursor: Option<Cursor>| {
                        let op = shop.admin().products(ProductsQuery::new(size,size).after(cursor.clone()));
                        async move { connection_page(async { op.send().await.map_err(concord_example_shopify_sdk::RequestError::Concord) }, cursor.as_ref(), |d| Some(d.products)).await }
                    });
                    while let Some(p) = pages.next().await { replayed.extend(p.unwrap().items.into_iter().map(|p| p.id.to_string())); }
                } else {
                    let mut pages = sequential(s.resume.clone().unwrap(), budget(), |cursor: Option<Cursor>| {
                        let op = shop.admin().collections(CollectionsQuery::new(size,size).after(cursor.clone()));
                        async move { connection_page(async { op.send().await.map_err(concord_example_shopify_sdk::RequestError::Concord) }, cursor.as_ref(), |d| Some(d.collections)).await }
                    });
                    while let Some(p) = pages.next().await { replayed.extend(p.unwrap().items.into_iter().map(|p| p.id.to_string())); }
                }
                // No remaining sibling is skipped. All earlier siblings on the interrupted page
                // are replayed; the failed parent is replayed as well.
                let duplicates = processed.iter().filter(|id| replayed.contains(id)).count();
                assert_eq!(duplicates, failed_item+1);
                assert_eq!(replayed.len(), (2-failed_page)*3);
                let successes_repeated = duplicates - 1;
                println!("{kind} page={} failed_item={}: Stopped, correct page; replay repeats {} successful siblings and failed parent; remaining siblings included",failed_page+1, failed_item+1,successes_repeated);
                server.shutdown().await;
            }
        }
    }
}
