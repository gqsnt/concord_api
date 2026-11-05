//! The SDK's declared behaviour on the wire, against the fake services.

use concord::reqwest;
use concord_example_fixture::services;
use concord_example_sdk::{Attachment, Catalog, CatalogConfig, Ledger, LedgerConfig};

fn catalog(url: &str) -> Catalog {
    Catalog::new(reqwest::Client::new(), CatalogConfig { key: "k-1".into() })
        .with_base_url(format!("{url}/catalog/v1"))
        .unwrap()
}

#[tokio::test]
async fn listing_defaults_are_inherited_replaced_or_cleared_per_call() {
    let server = services().await;
    let featured = catalog(&server.url())
        .set_region("south")
        .tenant("acme")
        .listing()
        .set_default_tags(vec!["featured".to_owned()]);

    let inherited = featured.list().send().await.unwrap();
    let cleared = featured.list().set_tags(Vec::new()).send().await.unwrap();
    let replaced = featured
        .list()
        .set_tags(vec!["office".to_owned()])
        .send()
        .await
        .unwrap();
    let no_region = featured.list().set_region(None).send().await.unwrap();

    assert_eq!(inherited.items.len(), 4);
    assert_eq!(cleared.items.len(), 6);
    assert_eq!(replaced.items.len(), 2);
    assert_eq!(no_region.items.len(), 4);
    let seen = server.requests();
    assert_eq!(seen[0].query(), Some("tag=featured&limit=20"));
    assert_eq!(seen[1].query(), Some("limit=20"));
    assert_eq!(seen[2].query(), Some("tag=office&limit=20"));
    assert_eq!(seen[0].header("x-region"), Some("south"));
    assert_eq!(seen[3].header("x-region"), None);
    assert_eq!(seen[0].header("x-client"), Some("concord-example-sdk"));
}

#[tokio::test]
async fn an_attachment_is_moved_into_one_body() {
    let server = services().await;
    let tenant = catalog(&server.url()).tenant("acme");
    let receipt = tenant
        .attach("item-001", Attachment::new(&b"twelve bytes"[..]))
        .send()
        .await
        .unwrap();
    assert_eq!(receipt.bytes_received, 12);
    let seen = server.single();
    assert_eq!(
        seen.header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(seen.body, b"twelve bytes");
}

#[tokio::test]
async fn ledger_pages_decode_its_vendor_json() {
    let server = services().await;
    let ledger = Ledger::new(
        reqwest::Client::new(),
        LedgerConfig {
            token: "Bearer t".into(),
        },
    )
    .with_base_url(format!("{}/ledger/v1", server.url()))
    .unwrap();
    let page = ledger
        .entries("acct-002a")
        .set_page_size(4)
        .send()
        .await
        .unwrap();
    assert_eq!(page.data.len(), 4);
    assert_eq!(page.paging.next.as_deref(), Some("ledger-from-4"));
    assert_eq!(
        server.single().query(),
        Some("account=acct-002a&page_size=4")
    );
}
