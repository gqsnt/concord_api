//! Maintenance regression evidence for the `If-Modified-Since` listing; no live Xero calls.
//!
//! Expected header texts were derived independently of the SDK with Python's
//! `datetime.fromisoformat(..).replace(tzinfo=timezone.utc).timestamp()`, and the spelling
//! `yyyy-mm-ddThh:mm:ss` comes from Xero's Invoices reference (see PROVENANCE.md, key XM).
mod support;
use concord_sdk_xero_accounting::concord_pagination::{PagesError, Resume};
use concord_sdk_xero_accounting::*;
use concord_testserver::{Recorded, Reply, TestServer};
use std::{
    num::NonZeroUsize,
    time::{Duration, UNIX_EPOCH},
};
use support::*;

const IMS: &str = "if-modified-since";
/// Xero's documented example value, as an instant (1257984000 s) plus a sub-second part.
const DOC_EXAMPLE: &str = "2009-11-12T00:00:00";

fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}
fn doc_example(tenant: &Tenant) -> ModifiedSince {
    tenant
        .modified_since(UNIX_EPOCH + Duration::new(1_257_984_000, 999_999_999))
        .unwrap()
}

#[tokio::test]
async fn header_is_the_documented_utc_spelling_truncated_to_the_second() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.target, "/api.xro/2.0/Invoices?page=1&pageSize=1");
        assert_eq!(r.header("xero-tenant-id"), Some(A));
        Reply::json(200, EMPTY)
    })
    .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    let tenant = api.tenant(&id(A)).unwrap();
    let cases = [
        (0, 0, "1970-01-01T00:00:00"),
        (951_868_799, 0, "2000-02-29T23:59:59"),
        (1_257_984_000, 999_999_999, DOC_EXAMPLE),
        (1_735_689_599, 500_000_000, "2024-12-31T23:59:59"),
        (1_735_689_600, 0, "2025-01-01T00:00:00"),
        (4_107_542_400, 0, "2100-03-01T00:00:00"),
        (1_794_028_234, 1, "2026-11-07T05:10:34"),
        (253_402_300_799, 999_999_999, "9999-12-31T23:59:59"),
    ];
    for (seconds, nanos, _) in cases {
        let since = tenant
            .modified_since(UNIX_EPOCH + Duration::new(seconds, nanos))
            .unwrap();
        assert_eq!(since.at(), UNIX_EPOCH + Duration::from_secs(seconds));
        assert_eq!(since.tenant(), &id(A));
        let page = tenant
            .list_modified_since(&since, 1, 1)
            .unwrap()
            .send()
            .await
            .unwrap();
        assert!(page.invoices.is_empty() && page.next.is_none());
    }
    let sent: Vec<Vec<String>> = server
        .requests()
        .iter()
        .map(|r| r.header_all(IMS).into_iter().map(str::to_owned).collect())
        .collect();
    let expected: Vec<Vec<String>> = cases.iter().map(|c| vec![c.2.to_owned()]).collect();
    assert_eq!(sent, expected);
    server.shutdown().await;
}

#[test]
fn instants_outside_the_four_digit_utc_range_are_refused_before_io() {
    let temp = Temp::new();
    let store = FileStore::create(temp.path(), grant(A, false)).unwrap();
    let api = Xero::new(store, "synthetic-client-secret").unwrap();
    let tenant = api.tenant(&id(A)).unwrap();
    // The instant just before the epoch: 100 ns is the finest step `SystemTime` represents on
    // every supported platform (Windows counts 100 ns intervals, so `UNIX_EPOCH - 1 ns` *is* the
    // epoch there); where nanoseconds exist the 1 ns case is checked too.
    let instants = vec![
        UNIX_EPOCH - Duration::from_nanos(100),
        UNIX_EPOCH - Duration::from_secs(86_400),
        UNIX_EPOCH + Duration::from_secs(253_402_300_800),
    ];
    #[cfg(not(windows))]
    let instants = [instants, vec![UNIX_EPOCH - Duration::from_nanos(1)]].concat();
    for at in instants {
        let e = tenant.modified_since(at).unwrap_err();
        assert!(matches!(e.kind(), ErrorKind::Input(_)), "{e:?}");
    }
    let since = doc_example(&tenant);
    assert_eq!(
        format!("{since:?}"),
        format!("ModifiedSince({DOC_EXAMPLE})")
    );
    no_secrets(&since);
    no_secrets(tenant.list_modified_since(&since, 1, 1).unwrap());
    // Paging bounds are the existing listing's, unchanged.
    assert!(tenant.list_modified_since(&since, 0, 1).is_err());
    assert!(tenant.list_modified_since(&since, 1, 0).is_err());
    assert!(tenant.list_modified_since(&since, 1, 1001).is_err());
    assert!(
        tenant
            .pages_modified_since(&since, None, 1001, cap(1))
            .is_err()
    );
}

#[tokio::test]
async fn every_page_and_a_resumed_traversal_send_one_unchanged_timestamp() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.header("xero-tenant-id"), Some(A));
        match r.target.as_str() {
            "/api.xro/2.0/Invoices?page=1&pageSize=1" => Reply::json(200, MODERN1),
            "/api.xro/2.0/Invoices?page=2&pageSize=1" => Reply::json(200, MODERN2),
            _ => panic!("unexpected fixture request"),
        }
    })
    .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    let tenant = api.tenant(&id(A)).unwrap();
    let since = doc_example(&tenant);
    // A cap of one page ends the first traversal with a typed continuation, not completion.
    let mut pages = tenant
        .pages_modified_since(&since, None, 1, cap(1))
        .unwrap();
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 1);
    let PagesError::Bounded {
        resume: Resume::At(cursor),
        ..
    } = pages.next_page().await.unwrap_err()
    else {
        panic!("expected a bounded traversal")
    };
    assert_eq!(cursor.page(), 2);
    // Resuming with a different sync point is refused before I/O; the same one continues.
    let later = tenant
        .modified_since(UNIX_EPOCH + Duration::from_secs(1_257_984_001))
        .unwrap();
    assert!(matches!(
        tenant
            .pages_modified_since(&later, Some(cursor.clone()), 1, cap(5))
            .unwrap_err()
            .kind(),
        ErrorKind::Input(_)
    ));
    assert_eq!(server.requests().len(), 1);
    let mut resumed = tenant
        .pages_modified_since(&since, Some(cursor), 1, cap(5))
        .unwrap();
    assert_eq!(resumed.next_page().await.unwrap().unwrap().items.len(), 1);
    assert!(resumed.next_page().await.unwrap().is_none());
    let sent: Vec<(String, Vec<String>)> = server
        .requests()
        .iter()
        .map(|r| {
            (
                r.target.clone(),
                r.header_all(IMS).into_iter().map(str::to_owned).collect(),
            )
        })
        .collect();
    assert_eq!(
        sent,
        [
            (
                "/api.xro/2.0/Invoices?page=1&pageSize=1".to_owned(),
                vec![DOC_EXAMPLE.to_owned()]
            ),
            (
                "/api.xro/2.0/Invoices?page=2&pageSize=1".to_owned(),
                vec![DOC_EXAMPLE.to_owned()]
            ),
        ]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn a_continuation_resumes_only_the_listing_that_produced_it() {
    let server = TestServer::start(|_: &Recorded| Reply::json(200, LEGACY)).await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    let tenant = api.tenant(&id(A)).unwrap();
    let since = doc_example(&tenant);
    let plain = tenant
        .list(1, 2)
        .unwrap()
        .send()
        .await
        .unwrap()
        .next
        .unwrap();
    let filtered = tenant
        .list_modified_since(&since, 1, 2)
        .unwrap()
        .send()
        .await
        .unwrap()
        .next
        .unwrap();
    assert_ne!(plain, filtered);
    assert_eq!(server.requests().len(), 2);
    let input = |e: Error| assert!(matches!(e.kind(), ErrorKind::Input(_)), "{e:?}");
    // A filtered continuation would silently drop its filter in an unfiltered traversal, and an
    // unfiltered one would add a filter halfway through: both are refused.
    input(tenant.pages(Some(filtered), 2, cap(2)).unwrap_err());
    input(
        tenant
            .pages_modified_since(&since, Some(plain), 2, cap(2))
            .unwrap_err(),
    );
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn a_sync_point_is_never_sent_to_another_tenant() {
    let server = TestServer::start(|_: &Recorded| Reply::json(200, LEGACY)).await;
    let t1 = Temp::new();
    let (a, _) = client(&t1, &server, A, false);
    let a = a.tenant(&id(A)).unwrap();
    let t2 = Temp::new();
    let (b, _) = client(&t2, &server, B, false);
    let b = b.tenant(&id(B)).unwrap();
    let since_a = doc_example(&a);
    let mismatch = |e: Error| assert!(matches!(e.kind(), ErrorKind::TenantMismatch), "{e:?}");
    mismatch(b.list_modified_since(&since_a, 1, 2).unwrap_err());
    mismatch(
        b.pages_modified_since(&since_a, None, 2, cap(2))
            .unwrap_err(),
    );
    // A tenant-A continuation is refused by B even when B supplies its own sync point.
    let cursor_a = a
        .list_modified_since(&since_a, 1, 2)
        .unwrap()
        .send()
        .await
        .unwrap()
        .next
        .unwrap();
    let since_b = b.modified_since(since_a.at()).unwrap();
    assert_ne!(since_a, since_b);
    mismatch(
        b.pages_modified_since(&since_b, Some(cursor_a), 2, cap(2))
            .unwrap_err(),
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].header("xero-tenant-id"), Some(A));
    server.shutdown().await;
}

#[tokio::test]
async fn existing_listing_and_retrieval_send_no_modified_since_header() {
    let server = TestServer::start(|r: &Recorded| match r.path() {
        "/api.xro/2.0/Invoices" => Reply::json(200, EMPTY),
        _ => Reply::json(200, ITEM),
    })
    .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    let tenant = api.tenant(&id(A)).unwrap();
    tenant.list(1, 1).unwrap().send().await.unwrap();
    tenant
        .pages(None, 1, cap(1))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    tenant
        .get(&tenant.invoice(INVOICE).unwrap())
        .unwrap()
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for r in &requests {
        assert!(r.header_all(IMS).is_empty());
    }
    server.shutdown().await;
}

#[tokio::test]
async fn not_modified_is_a_refusal_never_an_empty_completed_sync() {
    // Xero documents a filtered 200 listing; a 304 is not documented for collections, so the
    // SDK must not read it as "no changes" and let a caller advance its sync point.
    let server = TestServer::start(|_: &Recorded| Reply::new(304)).await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    let tenant = api.tenant(&id(A)).unwrap();
    let since = doc_example(&tenant);
    let incomplete = tenant
        .pages_modified_since(&since, None, 1, cap(3))
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(incomplete.collected.is_empty());
    assert!(matches!(incomplete.error.resume(), Some(Resume::Start)));
    let e = incomplete.error.provider_error().unwrap();
    assert_eq!(e.class(), Some(Class::Other));
    let ErrorKind::Provider(p) = e.kind() else {
        panic!("wrong stage")
    };
    assert_eq!(p.meta.status().as_u16(), 304);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}
