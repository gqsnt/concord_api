//! Offline protocol evidence; no live Xero calls.
mod support;
use concord_sdk_xero_accounting::*;
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::{
    fs,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::SystemTime,
};
use support::*;

#[tokio::test]
async fn all_documented_accounting_http_error_classes_preserve_status_and_documents() {
    for (status, class) in [
        (400, Class::Validation),
        (401, Class::Unauthorized),
        (403, Class::Forbidden),
        (404, Class::NotFound),
        (412, Class::Precondition),
        (429, Class::RateLimited),
        (500, Class::Internal),
        (501, Class::NotImplemented),
        (503, Class::Unavailable),
        (418, Class::Other),
    ] {
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, VALIDATION).header("set-cookie", "synthetic-secret-in-body")
        })
        .await;
        let temp = Temp::new();
        let (api, _) = client(&temp, &server, A, false);
        let e = api
            .tenant(&id(A))
            .unwrap()
            .list(1, 1)
            .unwrap()
            .send()
            .await
            .unwrap_err();
        assert_eq!(e.class(), Some(class));
        assert!(!e.write_uncertain());
        let ErrorKind::Provider(p) = e.kind() else {
            panic!("wrong stage")
        };
        assert_eq!(p.meta.status().as_u16(), status);
        assert_eq!(p.document.as_ref().unwrap().number, Some(10));
        assert!(
            p.document
                .as_ref()
                .unwrap()
                .message
                .as_ref()
                .unwrap()
                .contains("synthetic-secret")
        );
        no_secrets(&e);
        no_secrets(e.kind());
        no_secrets(p);
        no_secrets(e.to_string());
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

fn body_case(case: u8, status: u16, limit: usize) -> Reply {
    match case {
        0 => Reply::json(status, MALFORMED),
        1 => Reply::json(status, "{}").with_body(Body::FailAfter(vec![
            concord::bytes::Bytes::from_static(b"{\"Invoices\":"),
        ])),
        2 => Reply::json(status, vec![b' '; limit + 1]),
        3 => Reply::text(
            status,
            "application/xml",
            include_str!("../fixtures/wrong-media.xml"),
        ),
        4 => Reply::new(status).body(EMPTY),
        5 => Reply::json(status, EMPTY).header("content-type", "application/json"),
        6 => Reply::json(status, "{}"),
        _ => unreachable!(),
    }
}
#[tokio::test]
async fn success_malformed_truncated_oversized_wrong_missing_and_repeated_media_are_distinct() {
    for (case, kind) in [
        (0, concord::ErrorKind::Decode),
        (1, concord::ErrorKind::Transport),
        (2, concord::ErrorKind::Limit),
        (3, concord::ErrorKind::Metadata),
        (4, concord::ErrorKind::Metadata),
        (5, concord::ErrorKind::Metadata),
        (6, concord::ErrorKind::Decode),
    ] {
        let server = TestServer::start(move |_: &Recorded| body_case(case, 200, BODY_LIMIT)).await;
        let temp = Temp::new();
        let (api, _) = client(&temp, &server, A, false);
        let e = api
            .tenant(&id(A))
            .unwrap()
            .list(1, 1)
            .unwrap()
            .send()
            .await
            .unwrap_err();
        assert_eq!(concord_kind(&e), kind);
        assert_eq!(
            e.concord_error().unwrap().head().unwrap().status().as_u16(),
            200
        );
        no_secrets(&e);
        no_secrets(e.kind());
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}
#[tokio::test]
async fn error_body_failure_never_hides_http_status() {
    for (case, kind) in [
        (0, concord::ErrorKind::Decode),
        (1, concord::ErrorKind::Transport),
        (2, concord::ErrorKind::Limit),
        (3, concord::ErrorKind::Metadata),
        (4, concord::ErrorKind::Metadata),
        (5, concord::ErrorKind::Metadata),
    ] {
        let server =
            TestServer::start(move |_: &Recorded| body_case(case, 503, ERROR_BODY_LIMIT)).await;
        let temp = Temp::new();
        let (api, _) = client(&temp, &server, A, false);
        let e = api
            .tenant(&id(A))
            .unwrap()
            .list(1, 1)
            .unwrap()
            .send()
            .await
            .unwrap_err();
        let ErrorKind::Provider(p) = e.kind() else {
            panic!("wrong stage")
        };
        assert_eq!(p.class, Class::Unavailable);
        assert_eq!(p.meta.status().as_u16(), 503);
        assert_eq!(p.document.as_ref().unwrap_err().kind(), kind);
        no_secrets(p);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}
#[tokio::test]
async fn oauth_error_and_unreadable_token_responses_leave_a_fail_closed_journal() {
    for case in 0..7 {
        let server = TestServer::start(move |r: &Recorded| {
            assert_eq!(r.path(), "/connect/token");
            if case == 6 {
                Reply::json(400, OAUTH_ERROR)
            } else {
                body_case(case, 200, ERROR_BODY_LIMIT)
            }
        })
        .await;
        let temp = Temp::new();
        let (api, _) = client(&temp, &server, A, true);
        let tenant = api.tenant(&id(A)).unwrap();
        let error = tenant.list(1, 1).unwrap().send().await.unwrap_err();
        let ErrorKind::Authentication(inner) = error.kind() else {
            panic!("wrong stage")
        };
        if case == 6 {
            let ErrorKind::Provider(p) = inner.kind() else {
                panic!("wrong stage")
            };
            assert_eq!(
                p.document.as_ref().unwrap().error.as_deref(),
                Some("invalid_grant")
            );
        } else {
            assert!(inner.concord_error().is_some());
        }
        no_secrets(&error);
        no_secrets(error.kind());
        assert!(tenant.list(1, 1).unwrap().send().await.is_err());
        assert_eq!(server.requests().len(), 1);
        let journal = fs::read_to_string(temp.path().join("state.json")).unwrap();
        assert!(journal.contains("Refreshing") && !journal.contains("synthetic-refresh-old"));
        server.shutdown().await;
    }
}

#[tokio::test]
async fn read_429_retries_once_with_same_tenant_after_valid_hint() {
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let server = TestServer::start(move |r: &Recorded| {
        assert_eq!(r.target, "/api.xro/2.0/Invoices?page=1&pageSize=1");
        assert_eq!(r.header("xero-tenant-id"), Some(A));
        if c.fetch_add(1, Ordering::SeqCst) == 0 {
            Reply::json(429, VALIDATION).header("retry-after", "0")
        } else {
            Reply::json(200, EMPTY)
        }
    })
    .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    assert!(
        api.tenant(&id(A))
            .unwrap()
            .list(1, 1)
            .unwrap()
            .send()
            .await
            .unwrap()
            .invoices
            .is_empty()
    );
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
#[tokio::test]
async fn repeated_or_invalid_retry_after_is_surfaced_without_early_replay() {
    for repeated in [false, true] {
        let server = TestServer::start(move |_: &Recorded| {
            let r = Reply::json(429, VALIDATION)
                .header("retry-after", if repeated { "0" } else { "+1" });
            if repeated {
                r.header("retry-after", "0")
            } else {
                r
            }
        })
        .await;
        let temp = Temp::new();
        let (api, _) = client(&temp, &server, A, false);
        let e = api
            .tenant(&id(A))
            .unwrap()
            .list(1, 1)
            .unwrap()
            .send()
            .await
            .unwrap_err();
        let ErrorKind::Provider(p) = e.kind() else {
            panic!("wrong stage")
        };
        assert!(p.retry_after.is_err());
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}
#[tokio::test]
async fn long_retry_after_is_a_minimum_and_blocks_sibling_requests() {
    let server =
        TestServer::start(|_: &Recorded| Reply::json(429, VALIDATION).header("retry-after", "20"))
            .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    let tenant = api.tenant(&id(A)).unwrap();
    let e = tenant.list(1, 1).unwrap().send().await.unwrap_err();
    assert_eq!(e.class(), Some(Class::RateLimited));
    let e = tenant.list(1, 1).unwrap().send().await.unwrap_err();
    assert!(matches!(e.kind(), ErrorKind::Admission(_)));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}
#[tokio::test]
async fn a_read_retries_at_most_twice() {
    let server =
        TestServer::start(|_: &Recorded| Reply::json(429, VALIDATION).header("retry-after", "0"))
            .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    assert_eq!(
        api.tenant(&id(A))
            .unwrap()
            .list(1, 1)
            .unwrap()
            .send()
            .await
            .unwrap_err()
            .class(),
        Some(Class::RateLimited)
    );
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn create_is_never_automatically_replayed_and_uncertainty_is_explicit() {
    for (case, uncertain) in [(0, false), (1, true), (2, true), (3, true), (4, false)] {
        let server = TestServer::start(move |r: &Recorded| {
            assert_eq!(r.method, "POST");
            match case {
                0 => Reply::json(429, VALIDATION).header("retry-after", "0"),
                1 => Reply::json(503, VALIDATION),
                2 => Reply::hang_up(),
                3 => Reply::json(200, MALFORMED),
                4 => Reply::json(200, BAD200),
                _ => unreachable!(),
            }
        })
        .await;
        let temp = Temp::new();
        let (api, _) = client(&temp, &server, A, false);
        let tenant = api.tenant(&id(A)).unwrap();
        let e = tenant
            .create(draft(&tenant))
            .unwrap()
            .send()
            .await
            .unwrap_err();
        assert_eq!(e.write_uncertain(), uncertain);
        no_secrets(&e);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}
#[tokio::test]
async fn legacy_pages_continue_after_short_pages_and_bounded_resume_is_explicit() {
    let server = TestServer::start(|r: &Recorded| match r.query_all("page")[0].as_str() {
        "1" => Reply::json(200, LEGACY),
        "2" => Reply::json(200, EMPTY),
        _ => panic!("too many pages"),
    })
    .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    let tenant = api.tenant(&id(A)).unwrap();
    let mut pages = tenant
        .pages(None, 2, NonZeroUsize::new(1).unwrap())
        .unwrap();
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 1);
    let key = first.next.unwrap();
    assert_eq!(key.page(), 2);
    assert!(matches!(
        pages.next_page().await.unwrap_err(),
        concord_pagination::PagesError::Bounded { .. }
    ));
    assert!(matches!(
        pages.next_page().await.unwrap_err(),
        concord_pagination::PagesError::Ended { .. }
    ));
    let mut resumed = tenant
        .pages(Some(key), 2, NonZeroUsize::new(1).unwrap())
        .unwrap();
    assert!(resumed.next_page().await.unwrap().unwrap().items.is_empty());
    assert!(resumed.next_page().await.unwrap().is_none());
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
#[tokio::test]
async fn inconsistent_paging_envelopes_and_mismatched_resource_ids_fail() {
    for body in [
        MODERN1.replace("\"page\": 1", "\"page\": 9"),
        MODERN1.replace("\"pageSize\": 1", "\"pageSize\": 2"),
        MODERN1.replace("\"pageCount\": 2", "\"pageCount\": 1"),
        MODERN1.replace("\"itemCount\": 2", "\"itemCount\": 0"),
        MODERN1.replace("\"Status\": \"OK\"", "\"Status\": \"ERROR\""),
        DRAFT.to_owned(),
    ] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, &body)).await;
        let temp = Temp::new();
        let (api, _) = client(&temp, &server, A, false);
        let tenant = api.tenant(&id(A)).unwrap();
        let result = tenant.list(1, 1).unwrap().send().await;
        // DRAFT is a valid legacy page; the get contract must still reject a different ID.
        if result.is_ok() {
            assert!(
                tenant
                    .get(&tenant.invoice(INVOICE).unwrap())
                    .unwrap()
                    .send()
                    .await
                    .is_err()
            );
        } else {
            assert!(matches!(result.unwrap_err().kind(), ErrorKind::Contract(_)));
        }
        server.shutdown().await;
    }
}
#[tokio::test]
async fn read_only_grant_refuses_create_before_accounting_io() {
    let server = TestServer::start(|_: &Recorded| Reply::json(200, EMPTY)).await;
    let temp = Temp::new();
    let store = FileStore::create(
        temp.path(),
        grant_scopes(A, false, "offline_access accounting.invoices.read"),
    )
    .unwrap();
    let api = Xero::with_origins(
        store,
        "synthetic-client-secret",
        &server.url(),
        &server.url(),
    )
    .unwrap();
    let tenant = api.tenant(&id(A)).unwrap();
    assert!(matches!(
        tenant
            .create(draft(&tenant))
            .unwrap()
            .send()
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::Input(_)
    ));
    assert!(server.requests().is_empty());
    server.shutdown().await;
}
#[tokio::test]
async fn redirect_does_not_forward_credentials_or_replay_a_write() {
    let target = TestServer::start(|_: &Recorded| Reply::json(200, DRAFT)).await;
    let redirect = target.url();
    let server = TestServer::start(move |_: &Recorded| {
        Reply::new(307).header("location", format!("{redirect}/api.xro/2.0/Invoices"))
    })
    .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    let tenant = api.tenant(&id(A)).unwrap();
    assert_eq!(
        tenant
            .create(draft(&tenant))
            .unwrap()
            .send()
            .await
            .unwrap_err()
            .class(),
        Some(Class::Other)
    );
    assert_eq!(server.requests().len(), 1);
    assert!(target.requests().is_empty());
    server.shutdown().await;
    target.shutdown().await;
}
#[test]
fn local_inputs_secrets_and_store_bindings_are_checked_without_io() {
    for tenant in [
        "",
        "../secret",
        "x\r\ny",
        "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaag",
    ] {
        assert!(TenantId::new(tenant).is_err());
    }
    assert!(
        Grant::new(
            id(A),
            "client",
            "access",
            "refresh",
            "accounting.invoices",
            SystemTime::now()
        )
        .is_err()
    );
    assert!(
        Grant::new(
            id(A),
            "client",
            "access\r\nsecret",
            "refresh",
            "offline_access accounting.invoices",
            SystemTime::now()
        )
        .is_err()
    );
    assert!(Amount::from_minor_units(100_000_000_000).is_err());
    assert!(LineItem::new("", 1, Amount::from_minor_units(1).unwrap()).is_err());
    let temp = Temp::new();
    let grant = grant(A, false);
    no_secrets(&grant);
    let store = FileStore::create(temp.path(), grant).unwrap();
    no_secrets(&store);
    let api = Xero::new(store.clone(), "synthetic-client-secret").unwrap();
    no_secrets(&api);
    let tenant = api.tenant(&id(A)).unwrap();
    no_secrets(&tenant);
    no_secrets(draft(&tenant));
    no_secrets(tenant.get(&tenant.invoice(INVOICE).unwrap()).unwrap());
    assert!(tenant.list(0, 1).is_err());
    assert!(tenant.list(1, 0).is_err());
    assert!(tenant.list(1, 1001).is_err());
    for origin in [
        "http://example.com",
        "https://user:secret@example.com",
        "https://example.com/path",
        "https://example.com/?q=secret",
    ] {
        let e = Xero::with_origins(store.clone(), "secret", origin, "https://identity.xero.com")
            .unwrap_err();
        no_secrets(&e);
    }
    // A crash-held lock is never stolen based on age; manual reauthorization is explicit.
    fs::write(temp.path().join("lock"), b"synthetic lock").unwrap();
    assert!(matches!(
        FileStore::open(temp.path(), id(A), "synthetic-client")
            .unwrap_err()
            .kind(),
        ErrorKind::StoreBusy
    ));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(temp.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(temp.path().join("state.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[tokio::test]
async fn unauthorized_is_not_replayed_but_next_request_refreshes_the_exact_grant() {
    let n = Arc::new(AtomicUsize::new(0));
    let count = n.clone();
    let server =
        TestServer::start(
            move |r: &Recorded| match count.fetch_add(1, Ordering::SeqCst) {
                0 => {
                    assert_eq!(r.method, "GET");
                    assert_eq!(
                        r.header("authorization"),
                        Some("Bearer synthetic-access-old")
                    );
                    Reply::json(401, VALIDATION)
                }
                1 => {
                    assert_eq!(r.path(), "/connect/token");
                    assert!(r.body_text().contains("synthetic-refresh-old"));
                    Reply::json(200, ROTATED)
                }
                2 => {
                    assert_eq!(
                        r.header("authorization"),
                        Some("Bearer synthetic-access-new")
                    );
                    assert_eq!(r.header("xero-tenant-id"), Some(A));
                    Reply::json(200, EMPTY)
                }
                _ => panic!("unexpected replay"),
            },
        )
        .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    let tenant = api.tenant(&id(A)).unwrap();
    assert_eq!(
        tenant.list(1, 1).unwrap().send().await.unwrap_err().class(),
        Some(Class::Unauthorized)
    );
    assert_eq!(server.requests().len(), 1);
    tenant.list(1, 1).unwrap().send().await.unwrap();
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}
#[tokio::test]
async fn invalid_token_lifetime_scope_and_credentials_are_never_published_or_reused() {
    for body in [
        ROTATED.replace("1800", "0"),
        ROTATED.replace("1800", "18446744073709551615"),
        ROTATED.replace("Bearer", "Basic"),
        ROTATED.replace("offline_access accounting.invoices", "accounting.invoices"),
        ROTATED.replace("synthetic-access-new", "synthetic-access-new with space"),
    ] {
        let server = TestServer::start(move |r: &Recorded| {
            assert_eq!(r.path(), "/connect/token");
            Reply::json(200, &body)
        })
        .await;
        let temp = Temp::new();
        let (api, _) = client(&temp, &server, A, true);
        let tenant = api.tenant(&id(A)).unwrap();
        let e = tenant.list(1, 1).unwrap().send().await.unwrap_err();
        no_secrets(&e);
        assert!(matches!(e.kind(), ErrorKind::Authentication(_)));
        assert!(tenant.list(1, 1).unwrap().send().await.is_err());
        assert_eq!(server.requests().len(), 1);
        assert!(!temp.path().join("pending.json").exists());
        server.shutdown().await;
    }
}
#[tokio::test]
async fn interrupted_traversal_returns_prefix_and_typed_failure() {
    let server = TestServer::start(|r: &Recorded| {
        if r.query_all("page") == ["1"] {
            Reply::json(200, LEGACY)
        } else {
            Reply::json(403, VALIDATION)
        }
    })
    .await;
    let temp = Temp::new();
    let (api, _) = client(&temp, &server, A, false);
    let tenant = api.tenant(&id(A)).unwrap();
    let incomplete = tenant
        .pages(None, 2, NonZeroUsize::new(4).unwrap())
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(
        incomplete.error.provider_error().unwrap().class(),
        Some(Class::Forbidden)
    );
    no_secrets(&incomplete);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
