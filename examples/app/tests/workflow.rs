//! W01: the named listing workflow returns generated operations and leaves the shared handle's
//! defaults intact for sibling calls.

use concord::reqwest;
use concord_example_app::workflow::{Browser, FEATURED_TAG};
use concord_example_fixture::services;
use concord_example_sdk::{Catalog, CatalogConfig, CatalogTenantListingList};

fn catalog(url: &str) -> Catalog {
    Catalog::new(reqwest::Client::new(), CatalogConfig { key: "k-1".into() })
        .with_base_url(format!("{url}/catalog/v1"))
        .unwrap()
}

#[tokio::test]
async fn w01_unfiltered_and_featured_pages_leave_sibling_defaults_intact() {
    let server = services().await;
    let browser = Browser::new(&catalog(&server.url()), "acme");

    // The names return the generated operation; nothing is sent until the caller sends it.
    let unfiltered: CatalogTenantListingList = browser.unfiltered_page();
    let featured: CatalogTenantListingList = browser.featured_page();
    assert_eq!(server.count(), 0);

    let unfiltered = unfiltered.send().await.unwrap();
    let featured = featured.send().await.unwrap();
    // A sibling ordinary call through the SDK, after the unfiltered one.
    let sibling = browser.listing().list().set_limit(3).send().await.unwrap();

    assert_eq!(unfiltered.items.len(), 6);
    assert_eq!(featured.items.len(), 4);
    assert!(
        featured
            .items
            .iter()
            .all(|item| item.tags.iter().any(|t| t == FEATURED_TAG))
    );
    let ids: Vec<_> = sibling.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["item-001", "item-002", "item-004"]);

    let seen = server.requests();
    assert_eq!(seen.len(), 3);
    assert!(
        seen.iter()
            .all(|r| r.path() == "/catalog/v1/tenants/acme/items")
    );
    assert_eq!(seen[0].query(), Some("limit=20"));
    assert_eq!(seen[1].query(), Some("tag=featured&limit=20"));
    assert_eq!(seen[2].query(), Some("tag=featured&limit=3"));
}

#[tokio::test]
async fn w01_callers_keep_chaining_setters_on_named_operations() {
    let server = services().await;
    let browser = Browser::new(&catalog(&server.url()), "acme");

    let page = browser
        .unfiltered_page()
        .set_q("a")
        .set_limit(2)
        .set_region(Some("north".to_owned()))
        .send()
        .await
        .unwrap();
    let ids: Vec<_> = page.items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, ["item-003", "item-004"]);
    assert_eq!(page.next_cursor.as_deref(), Some("catalog-offset-2"));

    // `prepare()` works on a named operation too.
    let prepared = browser
        .featured_page()
        .set_cursor("catalog-offset-2")
        .prepare()
        .unwrap();
    assert_eq!(
        prepared.request().url().query(),
        Some("tag=featured&limit=20&cursor=catalog-offset-2")
    );

    let seen = server.single();
    assert_eq!(seen.query(), Some("q=a&limit=2"));
    assert_eq!(seen.header("x-region"), Some("north"));
}
