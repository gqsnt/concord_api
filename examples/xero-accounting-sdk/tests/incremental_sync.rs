//! Maintainer-written consumer test (public API and README only) for incremental invoice sync.
//! Not a fresh-consumer test: the maintainer wrote the feature. Xero is never called.
//!
//! The application keeps one sync point per tenant: it records `SystemTime::now()` before a
//! sync's first request, and advances to it only when that sync completes. An interrupted sync
//! resumes with the old sync point; a restart persists the sync point as Unix seconds.

use concord_sdk_xero_accounting::concord_pagination::Resume;
use concord_sdk_xero_accounting::{
    Class, ErrorKind, FileStore, Grant, ModifiedSince, Tenant, TenantId, Xero,
};
use concord_testserver::{Recorded, Reply, TestServer};
use serde_json::{Value, json};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TENANT_A: &str = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
const TENANT_B: &str = "5f4e3d2c-1b0a-4c9d-8e7f-6a5b4c3d2e1f";
const CLIENT_ID: &str = "SYNTHETICCLIENTID000000000000001";
const SECRET: &str = "synthetic-sync-secret";
const SCOPES: &str = "offline_access accounting.invoices.read";
const INV: [&str; 4] = [
    "aaaa0001-0000-4000-8000-000000000001",
    "aaaa0002-0000-4000-8000-000000000002",
    "aaaa0003-0000-4000-8000-000000000003",
    "aaaa0004-0000-4000-8000-000000000004",
];

fn cap() -> NonZeroUsize {
    NonZeroUsize::new(10).unwrap()
}

fn journal_dir(name: &str) -> PathBuf {
    let parent = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("xero-incremental-sync");
    std::fs::create_dir_all(&parent).unwrap();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    parent.join(format!("{name}-{}-{nanos}", std::process::id()))
}

fn open(server: &TestServer, dir: &PathBuf, tenant: &str) -> Tenant {
    let id = TenantId::new(tenant).unwrap();
    let store = FileStore::open(dir, id.clone(), CLIENT_ID).unwrap();
    let xero = Xero::with_origins(store, SECRET, &server.url(), &server.url()).unwrap();
    xero.tenant(&id).unwrap()
}

fn connect(server: &TestServer, name: &str, tenant: &str) -> (Tenant, PathBuf) {
    let dir = journal_dir(name);
    let grant = Grant::new(
        TenantId::new(tenant).unwrap(),
        CLIENT_ID,
        format!("at-{name}"),
        format!("rt-{name}"),
        SCOPES,
        SystemTime::now() + Duration::from_secs(1200),
    )
    .unwrap();
    drop(FileStore::create(&dir, grant).unwrap());
    (open(server, &dir, tenant), dir)
}

fn envelope(ids: &[&str], (page, size, count, items): (u32, u32, u32, u64)) -> String {
    let invoices: Vec<Value> = ids
        .iter()
        .map(|id| json!({"InvoiceID": id, "Type": "ACCREC", "Status": "AUTHORISED"}))
        .collect();
    json!({
        "Status": "OK",
        "Invoices": invoices,
        "pagination": {"page": page, "pageSize": size, "pageCount": count, "itemCount": items},
    })
    .to_string()
}

/// Independent oracle: Xero's documented `yyyy-mm-ddThh:mm:ss` (UTC) back to Unix seconds, by
/// counting calendar days, so the SDK's formatter is not used to check itself.
fn utc_seconds(text: &str) -> u64 {
    let b = text.as_bytes();
    assert_eq!(b.len(), 19, "{text}");
    for (i, c) in [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')] {
        assert_eq!(b[i], c, "{text}");
    }
    let n = |r: std::ops::Range<usize>| -> u64 { text[r].parse().unwrap() };
    let (year, month, day) = (n(0..4), n(5..7), n(8..10));
    let (hour, minute, second) = (n(11..13), n(14..16), n(17..19));
    let leap = |y: u64| (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400);
    let lengths = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    assert!((1..=12).contains(&month) && day >= 1 && day <= lengths[month as usize - 1]);
    assert!(hour < 24 && minute < 60 && second < 60);
    let days: u64 = (1970..year)
        .map(|y| if leap(y) { 366 } else { 365 })
        .sum::<u64>()
        + lengths[..month as usize - 1].iter().sum::<u64>()
        + (day - 1);
    days * 86_400 + hour * 3600 + minute * 60 + second
}

fn whole_seconds(at: SystemTime) -> u64 {
    at.duration_since(UNIX_EPOCH).unwrap().as_secs()
}

/// Full sync, interrupted-and-resumed incremental sync across a restart, then an empty one.
#[tokio::test]
async fn incremental_sync_resumes_with_its_sync_point_and_advances_only_on_completion() {
    // Filtered requests in arrival order: 0..=2 belong to the second sync, 3 to the third. (Two
    // syncs may start within one second, so the header value alone cannot tell them apart.)
    let filtered = Arc::new(AtomicUsize::new(0));
    let counter = filtered.clone();
    let server = TestServer::start(move |r: &Recorded| {
        assert_eq!(r.method, "GET");
        assert_eq!(r.path(), "/api.xro/2.0/Invoices");
        assert_eq!(r.header("xero-tenant-id"), Some(TENANT_A));
        let page: u32 = r.query_all("page")[0].parse().unwrap();
        let headers = r.header_all("if-modified-since");
        if headers.is_empty() {
            // First sync: the whole listing.
            return Reply::json(
                200,
                match page {
                    1 => envelope(&INV[..2], (1, 2, 2, 3)),
                    _ => envelope(&INV[2..3], (2, 2, 2, 3)),
                },
            );
        }
        assert_eq!(headers.len(), 1);
        match (counter.fetch_add(1, Ordering::SeqCst), page) {
            (0, 1) => Reply::json(200, envelope(&INV[1..3], (1, 2, 2, 3))),
            (1, 2) => Reply::text(503, "text/plain", "The Organisation is offline"),
            (2, 2) => Reply::json(200, envelope(&INV[3..], (2, 2, 2, 3))),
            (3, 1) => Reply::json(200, envelope(&[], (1, 2, 0, 0))),
            other => panic!("unexpected sync request {other:?}"),
        }
    })
    .await;
    let (tenant, dir) = connect(&server, "sync-a", TENANT_A);

    // Sync 1: no sync point yet, so list everything; record the start before the first request.
    let started_1 = SystemTime::now();
    let all = tenant
        .pages(None, 2, cap())
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(all.len(), 3);
    let sync_point: ModifiedSince = tenant.modified_since(started_1).unwrap();
    let saved = whole_seconds(sync_point.at()); // what the application persists

    // Restart: reopen the journal and rebind the persisted sync point to the same tenant.
    drop(tenant);
    let tenant = open(&server, &dir, TENANT_A);
    let since = tenant
        .modified_since(UNIX_EPOCH + Duration::from_secs(saved))
        .unwrap();
    assert_eq!(since, sync_point);

    // Sync 2 fails on page 2: a typed partial result. The sync point must not advance.
    let started_2 = SystemTime::now();
    let incomplete = tenant
        .pages_modified_since(&since, None, 2, cap())
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    let failure = incomplete.error.provider_error().unwrap();
    assert_eq!(failure.class(), Some(Class::Unavailable));
    let Some(Resume::At(cursor)) = incomplete.error.resume() else {
        panic!("expected a continuation")
    };
    assert_eq!(cursor.page(), 2);
    // Resume the same traversal: same sync point, same page size, only page 2 is fetched again.
    let rest = tenant
        .pages_modified_since(&since, Some(cursor.clone()), 2, cap())
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        rest.iter().map(|i| i.reference.id()).collect::<Vec<_>>(),
        [INV[3]]
    );
    // Completed: advance to the time sync 2 started (not the time it was resumed).
    let next = tenant.modified_since(started_2).unwrap();

    // Sync 3: nothing changed.
    let unchanged = tenant
        .pages_modified_since(&next, None, 2, cap())
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert!(unchanged.is_empty());

    // Wire: sync 1 unfiltered; every sync-2 request (including the retried page) carries the
    // sync-1 start; sync 3 carries the sync-2 start. Both are valid UTC, truncated to the second.
    let requests = server.requests();
    let header = |r: &Recorded| r.header("if-modified-since").map(str::to_owned);
    let pages: Vec<(Option<String>, String)> = requests
        .iter()
        .map(|r| (header(r), r.query_all("page")[0].clone()))
        .collect();
    assert_eq!(requests.len(), 6);
    let s1 = header(&requests[2]).unwrap();
    let s2 = header(&requests[5]).unwrap();
    assert_eq!(
        pages,
        [
            (None, "1".to_owned()),
            (None, "2".to_owned()),
            (Some(s1.clone()), "1".to_owned()),
            (Some(s1.clone()), "2".to_owned()),
            (Some(s1.clone()), "2".to_owned()),
            (Some(s2.clone()), "1".to_owned()),
        ]
    );
    assert_eq!(utc_seconds(&s1), whole_seconds(started_1));
    assert_eq!(utc_seconds(&s2), whole_seconds(started_2));
    assert_eq!(filtered.load(Ordering::SeqCst), 4);
    server.shutdown().await;
}

/// One tenant's sync point and continuation are refused by another tenant before any request.
#[tokio::test]
async fn sync_points_stay_with_their_tenant() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.header("xero-tenant-id"), Some(TENANT_A));
        Reply::json(200, envelope(&INV[..1], (1, 1, 2, 2)))
    })
    .await;
    let (a, _) = connect(&server, "sync-ta", TENANT_A);
    let (b, _) = connect(&server, "sync-tb", TENANT_B);
    let since_a = a.modified_since(SystemTime::now()).unwrap();
    assert_eq!(since_a.tenant(), a.id());
    let page = a
        .list_modified_since(&since_a, 1, 1)
        .unwrap()
        .send()
        .await
        .unwrap();
    let cursor_a = page.next.unwrap();
    for e in [
        b.list_modified_since(&since_a, 1, 1).unwrap_err(),
        b.pages_modified_since(&since_a, None, 1, cap())
            .unwrap_err(),
        b.pages_modified_since(
            &b.modified_since(since_a.at()).unwrap(),
            Some(cursor_a),
            1,
            cap(),
        )
        .unwrap_err(),
    ] {
        assert!(matches!(e.kind(), ErrorKind::TenantMismatch), "{e:?}");
        let shown = format!("{e} {e:?}");
        assert!(!shown.contains(TENANT_A) && !shown.contains(TENANT_B));
    }
    assert!(!format!("{since_a:?}").contains(TENANT_A));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}
