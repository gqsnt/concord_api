//! Offline workflows evidence; no live Xero calls.
mod support;
use concord_sdk_xero_accounting::*;
use concord_testserver::{Gate, Recorded, Reply, TestServer};
use std::{
    fs,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use support::*;

#[tokio::test]
async fn page_then_retrieve_chosen_invoice_in_authorized_tenant() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.method, "GET");
        assert_eq!(r.header("xero-tenant-id"), Some(A));
        assert_eq!(
            r.header("authorization"),
            Some("Bearer synthetic-access-old")
        );
        assert_eq!(r.header("accept"), Some("application/json"));
        match r.target.as_str() {
            "/api.xro/2.0/Invoices?page=1&pageSize=1" => Reply::json(200, MODERN1),
            "/api.xro/2.0/Invoices?page=2&pageSize=1" => Reply::json(200, MODERN2),
            "/api.xro/2.0/Invoices/11111111-1111-4111-8111-111111111111" => Reply::json(200, ITEM),
            _ => panic!("unexpected fixture request"),
        }
    })
    .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    let tenant = api.tenant(&id(A)).unwrap();
    let invoices = tenant
        .pages(None, 1, NonZeroUsize::new(5).unwrap())
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(invoices.len(), 2);
    assert_eq!(invoices[0].reference.tenant(), &id(A));
    let chosen = tenant
        .get(&invoices[0].reference)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(chosen.reference.id(), INVOICE);
    assert_eq!(chosen.status, "AUTHORISED");
    assert_eq!(server.requests().len(), 3);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn refresh_create_draft_and_retrieve_its_state() {
    let server = TestServer::start(|r: &Recorded| {
        if r.path() == "/connect/token" {
            assert_eq!(r.method, "POST");
            assert_eq!(
                r.header("authorization"),
                Some("Basic c3ludGhldGljLWNsaWVudDpzeW50aGV0aWMtY2xpZW50LXNlY3JldA==")
            );
            assert_eq!(
                r.header("content-type"),
                Some("application/x-www-form-urlencoded")
            );
            assert_eq!(r.header("accept"), Some("application/json"));
            assert_eq!(r.header("xero-tenant-id"), None);
            assert_eq!(
                r.form_pairs(),
                vec![
                    ("grant_type".into(), "refresh_token".into()),
                    ("client_id".into(), "synthetic-client".into()),
                    ("refresh_token".into(), "synthetic-refresh-old".into())
                ]
            );
            return Reply::json(200, ROTATED);
        }
        assert_eq!(r.header("xero-tenant-id"), Some(A));
        assert_eq!(
            r.header("authorization"),
            Some("Bearer synthetic-access-new")
        );
        if r.method == "POST" {
            assert_eq!(r.target, "/api.xro/2.0/Invoices?summarizeErrors=true");
            assert_eq!(r.header("content-type"), Some("application/json"));
            let expected: serde_json::Value =
                serde_json::from_str(include_str!("../fixtures/create-request.json")).unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&r.body).unwrap(),
                expected
            );
            // Exact decimal token, no binary-float rounding/quoted amount.
            assert!(r.body_text().contains("\"UnitAmount\":12.34"));
        } else {
            assert_eq!(r.method, "GET");
            assert_eq!(r.target, format!("/api.xro/2.0/Invoices/{DRAFT_ID}"));
        }
        Reply::json(200, DRAFT)
    })
    .await;
    let temp = Temp::new();
    let (api, store) = client(&temp, &server, A, true);
    let tenant = api.tenant(&id(A)).unwrap();
    let created = tenant.create(draft(&tenant)).unwrap().send().await.unwrap();
    assert_eq!(created.status, "DRAFT");
    assert_eq!(created.invoice_type, "ACCREC");
    let retrieved = tenant
        .get(&created.reference)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(retrieved.reference, created.reference);
    assert!(store.remaining_lifetime().unwrap() > Duration::from_secs(1700));
    let durable = fs::read_to_string(temp.path().join("state.json")).unwrap();
    assert!(durable.contains("synthetic-access-new") && durable.contains("synthetic-refresh-new"));
    assert!(!durable.contains("synthetic-refresh-old"));
    assert!(!temp.path().join("pending.json").exists());
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn rotated_pair_survives_failed_atomic_publication_and_can_be_recovered() {
    let temp = Temp::new();
    let path = temp.path();
    let fixture_path = path.clone();
    let server = TestServer::start(move |r: &Recorded| {
        if r.path() == "/connect/token" {
            // Simulated storage activation failure AFTER exchange. Pending file can still be fsynced.
            fs::remove_file(fixture_path.join("state.json")).unwrap();
            fs::create_dir(fixture_path.join("state.json")).unwrap();
            Reply::json(200, ROTATED)
        } else {
            assert_eq!(
                r.header("authorization"),
                Some("Bearer synthetic-access-new")
            );
            assert_eq!(r.header("xero-tenant-id"), Some(A));
            Reply::json(200, DRAFT)
        }
    })
    .await;
    let (api, store) = client(&temp, &server, A, true);
    let tenant = api.tenant(&id(A)).unwrap();
    let error = tenant
        .create(draft(&tenant))
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error.kind(), ErrorKind::Authentication(_)));
    assert!(!error.write_uncertain());
    assert_eq!(server.requests().len(), 1); // never use an uncommitted access token for Accounting
    let pending = fs::read_to_string(path.join("pending.json")).unwrap();
    assert!(pending.contains("synthetic-access-new") && pending.contains("synthetic-refresh-new"));
    assert!(!pending.contains("synthetic-refresh-old"));
    no_secrets(&error);
    // A second attempt neither reuses the old refresh token nor repeats the remote exchange.
    assert!(tenant.list(1, 1).unwrap().send().await.is_err());
    assert_eq!(server.requests().len(), 1);
    fs::remove_dir(path.join("state.json")).unwrap();
    store.recover_pending().unwrap();
    let reopened = FileStore::open(&path, id(A), "synthetic-client").unwrap();
    let restarted = Xero::with_origins(
        reopened,
        "synthetic-client-secret",
        &server.url(),
        &server.url(),
    )
    .unwrap();
    assert_eq!(
        restarted
            .tenant(&id(A))
            .unwrap()
            .create(draft(&tenant))
            .unwrap()
            .send()
            .await
            .unwrap()
            .status,
        "DRAFT"
    );
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn concurrent_waiters_share_one_refresh_even_when_starter_is_cancelled() {
    let gate = Gate::new();
    let response_gate = gate.clone();
    let token_calls = Arc::new(AtomicUsize::new(0));
    let count = token_calls.clone();
    let server = TestServer::start(move |r: &Recorded| {
        if r.path() == "/connect/token" {
            count.fetch_add(1, Ordering::SeqCst);
            Reply::json(200, ROTATED).after(response_gate.clone())
        } else {
            assert_eq!(
                r.header("authorization"),
                Some("Bearer synthetic-access-new")
            );
            Reply::json(200, ITEM)
        }
    })
    .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, true);
    let tenant = api.tenant(&id(A)).unwrap();
    let op = tenant.get(&tenant.invoice(INVOICE).unwrap()).unwrap();
    let starter = tokio::spawn(op.clone().send());
    assert!(
        server
            .wait_until(Duration::from_secs(2), |s| s.requests().len() == 1)
            .await
    );
    let waiter = tokio::spawn(op.send());
    starter.abort();
    gate.open();
    assert_eq!(waiter.await.unwrap().unwrap().reference.id(), INVOICE);
    assert_eq!(token_calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn separate_sessions_and_process_lock_cannot_refresh_one_grant_concurrently() {
    let gate = Gate::new();
    let g = gate.clone();
    let server = TestServer::start(move |r: &Recorded| {
        if r.path() == "/connect/token" {
            Reply::json(200, ROTATED).after(g.clone())
        } else {
            Reply::json(200, EMPTY)
        }
    })
    .await;
    let temp = Temp::new();
    let (api, store) = client(&temp, &server, A, true);
    let second = Xero::with_origins(
        store,
        "synthetic-client-secret",
        &server.url(),
        &server.url(),
    )
    .unwrap();
    let tenant = api.tenant(&id(A)).unwrap();
    let other = second.tenant(&id(A)).unwrap();
    let first = tokio::spawn(tenant.list(1, 1).unwrap().send());
    assert!(
        server
            .wait_until(Duration::from_secs(2), |s| s.requests().len() == 1)
            .await
    );
    let error = other.list(1, 1).unwrap().send().await.unwrap_err();
    let ErrorKind::Authentication(inner) = error.kind() else {
        panic!("unexpected stage")
    };
    assert!(matches!(inner.kind(), ErrorKind::StoreBusy));
    gate.open();
    first.await.unwrap().unwrap();
    other.list(1, 1).unwrap().send().await.unwrap();
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.path() == "/connect/token")
            .count(),
        1
    );
    server.shutdown().await;
}

#[tokio::test]
async fn lost_refresh_response_and_restart_fail_closed_without_replaying_old_token() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.path(), "/connect/token");
        Reply::hang_up()
    })
    .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, true);
    let tenant = api.tenant(&id(A)).unwrap();
    assert!(tenant.list(1, 1).unwrap().send().await.is_err());
    let record = fs::read_to_string(temp.path().join("state.json")).unwrap();
    assert!(record.contains("Refreshing"));
    assert!(!record.contains("synthetic-refresh-old"));
    assert!(tenant.list(1, 1).unwrap().send().await.is_err());
    let store = FileStore::open(temp.path(), id(A), "synthetic-client").unwrap();
    assert!(matches!(
        Xero::with_origins(
            store,
            "synthetic-client-secret",
            &server.url(),
            &server.url()
        )
        .unwrap_err()
        .kind(),
        ErrorKind::RecoveryRequired
    ));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn credentials_contacts_drafts_invoices_and_continuations_stay_in_their_tenant() {
    let server = TestServer::start(|_: &Recorded| Reply::json(200, LEGACY)).await;
    let t1 = Temp::new();
    let (a, _) = client(&t1, &server, A, false);
    let a = a.tenant(&id(A)).unwrap();
    let t2 = Temp::new();
    let (b, _) = client(&t2, &server, B, false);
    assert!(matches!(
        b.tenant(&id(A)).unwrap_err().kind(),
        ErrorKind::TenantMismatch
    ));
    let b = b.tenant(&id(B)).unwrap();
    assert!(matches!(
        b.get(&a.invoice(INVOICE).unwrap()).unwrap_err().kind(),
        ErrorKind::TenantMismatch
    ));
    assert!(matches!(
        b.draft(a.contact(CONTACT).unwrap(), vec![])
            .unwrap_err()
            .kind(),
        ErrorKind::TenantMismatch
    ));
    assert!(matches!(
        b.create(draft(&a)).unwrap_err().kind(),
        ErrorKind::TenantMismatch
    ));
    assert!(FileStore::open(t1.path(), id(B), "synthetic-client").is_err());
    assert!(FileStore::open(t1.path(), id(A), "another-client").is_err());
    assert!(server.requests().is_empty());
    let cursor = a.list(1, 2).unwrap().send().await.unwrap().next.unwrap();
    assert!(
        b.pages(Some(cursor.clone()), 2, NonZeroUsize::new(2).unwrap())
            .is_err()
    );
    assert!(
        a.pages(Some(cursor), 1, NonZeroUsize::new(2).unwrap())
            .is_err()
    );
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn a_delayed_old_401_cannot_invalidate_the_committed_new_generation() {
    let gate = Gate::new();
    let held = gate.clone();
    let first_gate = Gate::new();
    let first_reply_gate = first_gate.clone();
    let n = Arc::new(AtomicUsize::new(0));
    let count = n.clone();
    let server =
        TestServer::start(
            move |r: &Recorded| match count.fetch_add(1, Ordering::SeqCst) {
                0 => Reply::json(401, VALIDATION).after(first_reply_gate.clone()),
                1 => {
                    first_reply_gate.open();
                    Reply::json(401, VALIDATION).after(held.clone())
                }
                2 => {
                    assert_eq!(r.path(), "/connect/token");
                    Reply::json(200, ROTATED)
                }
                3 | 4 => {
                    assert_eq!(
                        r.header("authorization"),
                        Some("Bearer synthetic-access-new")
                    );
                    Reply::json(200, EMPTY)
                }
                _ => panic!("unexpected extra refresh"),
            },
        )
        .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    let tenant = api.tenant(&id(A)).unwrap();
    // Gate the first head until a second operation has dispatched with the old snapshot.
    let first = tenant.list(1, 1).unwrap();
    let second = tenant.list(1, 1).unwrap();
    let first_handle = tokio::spawn(first.send());
    assert!(
        server
            .wait_until(Duration::from_secs(2), |s| s.requests().len() == 1)
            .await
    );
    let second_handle = tokio::spawn(second.send());
    assert!(
        server
            .wait_until(Duration::from_secs(2), |s| s.requests().len() >= 2)
            .await
    );
    assert_eq!(
        first_handle.await.unwrap().unwrap_err().class(),
        Some(Class::Unauthorized)
    );
    tenant.list(1, 1).unwrap().send().await.unwrap();
    gate.open();
    assert_eq!(
        second_handle.await.unwrap().unwrap_err().class(),
        Some(Class::Unauthorized)
    );
    tenant.list(1, 1).unwrap().send().await.unwrap();
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.path() == "/connect/token")
            .count(),
        1
    );
    server.shutdown().await;
}
