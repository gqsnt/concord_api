//! W02: one bounded traversal walks both services' pages, returns a continuation at its bound,
//! resumes from it without duplicates or gaps, and keeps partial records and the resumption
//! point when a page fails.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, Ordering};

use concord::{ErrorKind, reqwest};
use concord_example_app::domain::{AccountId, Moment};
use concord_example_app::ledger_repr;
use concord_example_app::traversal::{
    Continuation, Interrupted, Page, Traversal, catalog_items, ledger_entries, traverse,
};
use concord_example_app::workflow::Browser;
use concord_example_fixture::{
    Reply, Server, catalog as catalog_service, ledger as ledger_service, services,
};
use concord_example_sdk::{Catalog, CatalogConfig, Ledger, LedgerConfig};

fn catalog(url: &str) -> Catalog {
    Catalog::new(reqwest::Client::new(), CatalogConfig { key: "k-1".into() })
        .with_base_url(format!("{url}/catalog/v1"))
        .unwrap()
}

fn ledger(url: &str) -> Ledger {
    Ledger::new(
        reqwest::Client::new(),
        LedgerConfig {
            token: "Bearer t".into(),
        },
    )
    .with_base_url(format!("{url}/ledger/v1"))
    .unwrap()
}

const ACCOUNT: AccountId = AccountId::new(42);
const ALL_ITEMS: [&str; 6] = [
    "item-001", "item-002", "item-003", "item-004", "item-005", "item-006",
];
const ALL_ENTRIES: [&str; 6] = [
    "entry-001",
    "entry-002",
    "entry-003",
    "entry-004",
    "entry-005",
    "entry-006",
];

fn pages(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn item_ids(items: &[concord_example_sdk::Item]) -> Vec<&str> {
    items.iter().map(|i| i.id.as_str()).collect()
}

fn entry_ids(entries: &[concord_example_sdk::Entry]) -> Vec<&str> {
    entries.iter().map(|e| e.id.as_str()).collect()
}

#[tokio::test]
async fn w02_complete_walks_over_both_services() {
    let server = services().await;
    let browser = Browser::new(&catalog(&server.url()), "acme");
    let ledger = ledger(&server.url());

    let items = catalog_items(&browser.unfiltered_page().set_limit(4), pages(5), None)
        .await
        .unwrap();
    let template = ledger_repr::entries(&ledger, ACCOUNT, None)
        .unwrap()
        .set_page_size(4);
    let entries = ledger_entries(&template, pages(5), None).await.unwrap();

    let Traversal::Complete { records, pages } = &items else {
        panic!("expected a complete Catalog walk: {items:?}");
    };
    assert_eq!((item_ids(records), *pages), (ALL_ITEMS.to_vec(), 2));
    let Traversal::Complete { records, pages } = &entries else {
        panic!("expected a complete Ledger walk: {entries:?}");
    };
    assert_eq!((entry_ids(records), *pages), (ALL_ENTRIES.to_vec(), 2));

    let queries: Vec<_> = server
        .requests()
        .iter()
        .map(|r| r.query().unwrap_or("").to_owned())
        .collect();
    assert_eq!(
        queries,
        [
            "limit=4",
            "limit=4&cursor=catalog-offset-4",
            "account=acct-002a&page_size=4",
            "account=acct-002a&page_size=4&page_token=ledger-from-4",
        ]
    );
}

#[tokio::test]
async fn w02_a_bounded_walk_returns_a_continuation_and_resumes_without_gaps() {
    let server = services().await;
    let browser = Browser::new(&catalog(&server.url()), "acme");
    let template = browser.unfiltered_page().set_limit(2);

    let first = catalog_items(&template, pages(2), None).await.unwrap();
    let Traversal::Bounded {
        records,
        pages: fetched,
        next,
    } = first
    else {
        panic!("expected a bounded walk: {first:?}");
    };
    assert_eq!(
        item_ids(&records),
        ["item-001", "item-002", "item-003", "item-004"]
    );
    assert_eq!(fetched, 2);
    assert_eq!(next.token(), "catalog-offset-4");
    assert_eq!(server.count(), 2);

    let rest = catalog_items(&template, pages(2), Some(next))
        .await
        .unwrap();
    assert!(matches!(rest, Traversal::Complete { pages: 1, .. }));
    assert_eq!(rest.next(), None);
    assert_eq!(item_ids(rest.records()), ["item-005", "item-006"]);
    let mut all = records;
    all.extend_from_slice(rest.records());
    assert_eq!(item_ids(&all), ALL_ITEMS);
    assert_eq!(server.count(), 3);
    assert_eq!(
        server.requests()[2].query(),
        Some("limit=2&cursor=catalog-offset-4")
    );
}

#[tokio::test]
async fn w02_ledger_resumes_page_by_page_from_a_domain_instant() {
    let server = services().await;
    let ledger = ledger(&server.url());
    // From 2026-01-15T10:00:00Z, two entries per page, one page per call.
    let template = ledger_repr::entries(
        &ledger,
        ACCOUNT,
        Some(Moment::from_unix_seconds(1_768_471_200)),
    )
    .unwrap()
    .set_page_size(2);

    let mut all = Vec::new();
    let mut from = None;
    let mut calls = 0;
    loop {
        calls += 1;
        let walk = ledger_entries(&template, pages(1), from.take())
            .await
            .unwrap();
        all.extend_from_slice(walk.records());
        match walk {
            Traversal::Bounded { next, pages, .. } => {
                assert_eq!(pages, 1);
                from = Some(next);
            }
            Traversal::Complete { pages, .. } => {
                assert_eq!(pages, 1);
                break;
            }
        }
    }
    assert_eq!(calls, 3);
    assert_eq!(entry_ids(&all), ALL_ENTRIES);
    let tokens: Vec<_> = server
        .requests()
        .iter()
        .map(|r| r.query_all("page_token"))
        .collect();
    assert_eq!(
        tokens,
        [
            vec![],
            vec!["ledger-from-2".to_owned()],
            vec!["ledger-from-4".to_owned()]
        ]
    );
    assert!(
        server
            .requests()
            .iter()
            .all(|r| r.query_all("from_ms") == ["1768471200000"])
    );
}

/// The fake services, except that the third page of each listing (`catalog-offset-2` with two
/// items per page, `ledger-from-2` with two entries per page) fails once with a `500`.
async fn services_failing_once_at_page_two() -> Server {
    let catalog = catalog_service("/catalog/v1");
    let ledger = ledger_service("/ledger/v1");
    let catalog_failed = AtomicBool::new(false);
    let ledger_failed = AtomicBool::new(false);
    Server::start(move |request| {
        if request.query_all("cursor") == ["catalog-offset-2"]
            && !catalog_failed.swap(true, Ordering::SeqCst)
        {
            return Reply::problem(500, "INTERNAL", "page lost");
        }
        if request.query_all("page_token") == ["ledger-from-2"]
            && !ledger_failed.swap(true, Ordering::SeqCst)
        {
            return Reply::problem(500, "INTERNAL", "page lost");
        }
        if request.path().starts_with("/ledger/") {
            ledger(request)
        } else {
            catalog(request)
        }
    })
    .await
}

#[tokio::test]
async fn w02_a_failed_page_keeps_partial_records_and_its_resumption_point() {
    let server = services_failing_once_at_page_two().await;
    let browser = Browser::new(&catalog(&server.url()), "acme");
    let catalog_template = browser.unfiltered_page().set_limit(2);

    let Err(Interrupted {
        error,
        records: kept,
        pages: fetched,
        retry_from,
    }) = catalog_items(&catalog_template, pages(10), None).await
    else {
        panic!("the second Catalog page should fail");
    };
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(
        error.status(),
        Some(reqwest::StatusCode::INTERNAL_SERVER_ERROR)
    );
    assert_eq!(item_ids(&kept), ["item-001", "item-002"]);
    assert_eq!(fetched, 1);
    assert_eq!(retry_from, Some(Continuation::new("catalog-offset-2")));
    assert_eq!(server.count(), 2, "the walk stopped at the failed page");

    // Keep the partial records and resume at the failed page.
    let rest = catalog_items(&catalog_template, pages(10), retry_from)
        .await
        .unwrap();
    let mut all = kept;
    all.extend_from_slice(rest.records());
    assert_eq!(item_ids(&all), ALL_ITEMS);
    assert!(matches!(rest, Traversal::Complete { pages: 2, .. }));
    assert_eq!(server.count(), 4);

    // The same policy for Ledger, through the same walk.
    let ledger = ledger(&server.url());
    let ledger_template = ledger_repr::entries(&ledger, ACCOUNT, None)
        .unwrap()
        .set_page_size(2);
    let interrupted = ledger_entries(&ledger_template, pages(10), None)
        .await
        .unwrap_err();
    assert_eq!(entry_ids(&interrupted.records), ["entry-001", "entry-002"]);
    assert_eq!(interrupted.pages, 1);
    assert_eq!(
        interrupted.retry_from.as_ref().map(Continuation::token),
        Some("ledger-from-2")
    );
    assert_eq!(
        interrupted.error.status(),
        Some(reqwest::StatusCode::INTERNAL_SERVER_ERROR)
    );
    let rest = ledger_entries(&ledger_template, pages(10), interrupted.retry_from)
        .await
        .unwrap();
    let mut all = interrupted.records;
    all.extend_from_slice(rest.records());
    assert_eq!(entry_ids(&all), ALL_ENTRIES);
    assert_eq!(server.count(), 4 + 2 + 2);

    let queries: Vec<_> = server.requests()[4..]
        .iter()
        .map(|r| r.query().unwrap_or("").to_owned())
        .collect();
    assert_eq!(
        queries,
        [
            "account=acct-002a&page_size=2",
            "account=acct-002a&page_size=2&page_token=ledger-from-2",
            "account=acct-002a&page_size=2&page_token=ledger-from-2",
            "account=acct-002a&page_size=2&page_token=ledger-from-4",
        ]
    );
}

#[tokio::test]
async fn w02_a_failed_first_page_resumes_from_the_beginning() {
    let mut tokens = Vec::new();
    let result: Result<Traversal<u32>, _> = traverse(pages(3), None, |token| {
        tokens.push(token);
        std::future::ready(Err("unavailable"))
    })
    .await;
    let interrupted = result.unwrap_err();
    assert_eq!(interrupted.error, "unavailable");
    assert!(interrupted.records.is_empty());
    assert_eq!(interrupted.pages, 0);
    assert_eq!(interrupted.retry_from, None);
    assert_eq!(tokens, [None]);
}

#[tokio::test]
async fn w02_the_bound_stops_a_listing_that_never_ends() {
    // A service that always answers the same next token: the bound still ends the walk, and a
    // resumed walk starts at the given continuation.
    let mut tokens = Vec::new();
    let walk = traverse(pages(3), Some(Continuation::new("again")), |token| {
        tokens.push(token);
        std::future::ready(Ok::<_, &str>(Page {
            records: vec![1, 2],
            next: Some("again".to_owned()),
        }))
    })
    .await
    .unwrap();
    assert_eq!(
        walk,
        Traversal::Bounded {
            records: vec![1, 2, 1, 2, 1, 2],
            pages: 3,
            next: Continuation::new("again"),
        }
    );
    assert_eq!(
        tokens,
        [
            Some("again".to_owned()),
            Some("again".to_owned()),
            Some("again".to_owned())
        ]
    );
    // A zero bound cannot be expressed.
    assert_eq!(NonZeroUsize::new(0), None);
}
