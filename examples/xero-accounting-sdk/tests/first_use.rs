//! Fresh-consumer application tests (see FIRST_USE.md). Written from the README and rustdoc only.
//!
//! Workflow 1: page an authorized tenant's invoices and retrieve a chosen invoice.
//! Workflow 2: create a draft invoice and retrieve its state in the same tenant.
//! Required failure: a rotated refresh token is saved atomically; tokens and invoices never
//! cross tenants. Every exchange goes to the loopback `TestServer`; Xero is never called.

use concord_sdk_xero_accounting::concord_pagination::{PagesError, Resume};
use concord_sdk_xero_accounting::{
    Amount, Class, Error, ErrorKind, FileStore, Grant, LineItem, Tenant, TenantId, Xero,
};
use concord_testserver::{Recorded, Reply, TestServer};
use serde_json::{Value, json};
use std::num::NonZeroUsize;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

const TENANT_A: &str = "45e4708e-d862-4111-ab3a-dd8cd03913e1";
const TENANT_B: &str = "b7c1a7e2-5d1f-4c3e-9a0b-2f6d8e4c1a90";
const CLIENT_ID: &str = "91E5715B1199038080D6D0296EBC1648";
const SECRET: &str = "xero-client-secret-7Hq2";
const SCOPES: &str = "offline_access accounting.invoices";
const CONTACT: &str = "3138017f-8ddc-420e-a159-e7e1cf9e643d";

const INV_1: &str = "220ddca8-3144-4085-9a88-2d72c5133734";
const INV_2: &str = "88192a99-cbc5-4a66-bf1a-2f9fea2d36d0";
const INV_3: &str = "5ceffde5-6786-4987-b03a-bf88d262c286";
const CREATED: &str = "d4b0f3a1-6c2e-4b8f-9e17-0a5c3b2d7e64";

const INVOICES: &str = "/api.xro/2.0/Invoices";
const TOKEN: &str = "/connect/token";

// ---------------------------------------------------------------- application helpers

/// A NEW journal directory beneath an application-private (0700) parent.
fn journal_dir(name: &str) -> PathBuf {
    let parent = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("xero-first-use");
    std::fs::create_dir_all(&parent).unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    parent.join(format!("{name}-{}-{nanos}", std::process::id()))
}

struct App {
    xero: Xero,
    tenant: Tenant,
    store: FileStore,
    dir: PathBuf,
}

/// The ordinary caller setup: grant → journal → client → tenant. Sends nothing.
fn app(
    server: &TestServer,
    name: &str,
    tenant: &str,
    access: &str,
    refresh: &str,
    valid_for: i64,
) -> App {
    let tenant_id = TenantId::new(tenant).unwrap();
    let now = SystemTime::now();
    let expires_at = if valid_for >= 0 {
        now + Duration::from_secs(valid_for as u64)
    } else {
        now - Duration::from_secs(valid_for.unsigned_abs())
    };
    let grant = Grant::new(
        tenant_id.clone(),
        CLIENT_ID,
        access,
        refresh,
        SCOPES,
        expires_at,
    )
    .unwrap();
    let dir = journal_dir(name);
    let store = FileStore::create(&dir, grant).unwrap();
    let xero = Xero::with_origins(store.clone(), SECRET, &server.url(), &server.url()).unwrap();
    let tenant = xero.tenant(&tenant_id).unwrap();
    App {
        xero,
        tenant,
        store,
        dir,
    }
}

fn invoice(id: &str, number: &str, status: &str) -> Value {
    json!({
        "Type": "ACCREC",
        "InvoiceID": id,
        "InvoiceNumber": number,
        "Reference": "PO-7781",
        "Status": status,
        "Contact": { "ContactID": CONTACT, "Name": "Ridgeway University" },
        "DateString": "2026-10-01T00:00:00",
        "Date": "/Date(1759276800000+0000)/",
        "LineAmountTypes": "Exclusive",
        "SubTotal": 1000.00,
        "TotalTax": 150.00,
        "Total": 1150.00,
        "CurrencyCode": "NZD",
        "UpdatedDateUTC": "/Date(1759312345000+0000)/"
    })
}

fn envelope(invoices: Vec<Value>, pagination: Option<(u32, u32, u32, u64)>) -> String {
    let mut body = json!({
        "Id": "8e2d9a8c-1b7c-4b2e-8a43-5d0f2c7b6e11",
        "Status": "OK",
        "ProviderName": "concord-first-use",
        "DateTimeUTC": "/Date(1762490000000)/",
        "Invoices": invoices,
    });
    if let Some((page, page_size, page_count, item_count)) = pagination {
        body["pagination"] = json!({
            "page": page, "pageSize": page_size, "pageCount": page_count, "itemCount": item_count
        });
    }
    body.to_string()
}

fn token_reply(access: &str, refresh: &str) -> Reply {
    Reply::json(
        200,
        json!({
            "id_token": null,
            "access_token": access,
            "expires_in": 1800,
            "token_type": "Bearer",
            "refresh_token": refresh,
            "scope": SCOPES
        })
        .to_string(),
    )
}

fn page_of(req: &Recorded) -> u32 {
    req.query_all("page")[0].parse().unwrap()
}

fn base64(input: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Every byte of every journal file, for black-box "which token is durable" checks.
fn journal_text(dir: &Path) -> String {
    let mut text = String::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            text.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
        }
    }
    text
}

fn journal_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn token_requests(server: &TestServer) -> Vec<Recorded> {
    server
        .requests()
        .into_iter()
        .filter(|r| r.path() == TOKEN)
        .collect()
}

fn accounting_requests(server: &TestServer) -> Vec<Recorded> {
    server
        .requests()
        .into_iter()
        .filter(|r| r.path().starts_with("/api.xro/"))
        .collect()
}

/// Ordinary formatting of an error must not reveal credentials, tenant or document identifiers.
fn assert_quiet(error: &Error, secrets: &[&str]) {
    let text = format!("{error} | {error:?}");
    for secret in secrets {
        assert!(!text.contains(secret), "{secret:?} leaked into {text}");
    }
}

/// Credential-stage failures from a `send()` arrive as `Authentication(inner)`; the journal or
/// storage state is the inner error's kind.
fn credential_kind(error: &Error) -> &ErrorKind {
    match error.kind() {
        ErrorKind::Authentication(inner) => inner.kind(),
        other => other,
    }
}

fn assert_accounting_head(req: &Recorded, bearer: &str, tenant: &str) {
    assert_eq!(
        req.header("authorization"),
        Some(format!("Bearer {bearer}").as_str())
    );
    assert_eq!(req.header("xero-tenant-id"), Some(tenant));
    assert_eq!(req.header("accept"), Some("application/json"));
}

// ---------------------------------------------------------------- workflow 1

/// Page an authorized tenant's invoices (modern metadata), choose one and retrieve it. The access
/// token is already expired, so the first call rotates the refresh token, which must be durable
/// before any Accounting request and must survive a restart.
#[tokio::test]
async fn workflow1_pages_invoices_then_retrieves_the_chosen_one() {
    let server = TestServer::start(|req: &Recorded| match (req.method.as_str(), req.path()) {
        ("POST", TOKEN) => token_reply("at-rotated-1", "rt-rotated-1"),
        ("GET", INVOICES) => match page_of(req) {
            1 => Reply::json(
                200,
                envelope(
                    vec![
                        invoice(INV_1, "INV-0001", "AUTHORISED"),
                        invoice(INV_2, "INV-0002", "PAID"),
                    ],
                    Some((1, 2, 2, 3)),
                ),
            ),
            2 => Reply::json(
                200,
                envelope(
                    vec![invoice(INV_3, "INV-0003", "SUBMITTED")],
                    Some((2, 2, 2, 3)),
                ),
            ),
            _ => Reply::json(404, "{}"),
        },
        ("GET", p) if p == format!("{INVOICES}/{INV_3}") || p == format!("{INVOICES}/{INV_1}") => {
            let id = p.rsplit('/').next().unwrap();
            Reply::json(
                200,
                envelope(vec![invoice(id, "INV-0003", "SUBMITTED")], None),
            )
        }
        _ => Reply::json(404, "{}"),
    })
    .await;
    let a = app(
        &server,
        "w1",
        TENANT_A,
        "at-original-0",
        "rt-original-0",
        -300,
    );

    // Page with an explicit cap well above the expected two pages.
    let mut pages = a
        .tenant
        .pages(None, 2, NonZeroUsize::new(10).unwrap())
        .unwrap();
    let mut seen = Vec::new();
    let mut metadata = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        metadata.push(page.meta.expect("modern pagination metadata"));
        assert_eq!(
            page.next.as_ref().map(|c| c.page()),
            if seen.is_empty() { Some(2) } else { None }
        );
        seen.extend(page.items);
    }
    // Completion is explicit and sticky.
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(seen.len(), 3);
    assert_eq!(
        metadata
            .iter()
            .map(|m| (m.page, m.page_size, m.page_count, m.item_count))
            .collect::<Vec<_>>(),
        [(1, 2, 2, 3), (2, 2, 2, 3)]
    );
    assert_eq!(
        seen.iter().map(|i| i.status.as_str()).collect::<Vec<_>>(),
        ["AUTHORISED", "PAID", "SUBMITTED"]
    );
    for inv in &seen {
        assert_eq!(inv.reference.tenant(), a.tenant.id());
        assert_eq!(inv.invoice_type, "ACCREC");
    }

    // Choose by business number, then retrieve the chosen invoice.
    let chosen = seen
        .iter()
        .find(|i| i.invoice_number.as_deref() == Some("INV-0003"))
        .unwrap();
    assert_eq!(chosen.reference.id(), INV_3);
    let current = a
        .tenant
        .get(&chosen.reference)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(current.reference, chosen.reference);
    assert_eq!(current.status, "SUBMITTED");

    // Wire: exactly one refresh, first, then three Accounting GETs with the rotated access token.
    let all = server.requests();
    assert_eq!(
        all.iter()
            .map(|r| (r.method.as_str(), r.target.as_str()))
            .collect::<Vec<_>>(),
        [
            ("POST", TOKEN),
            ("GET", "/api.xro/2.0/Invoices?page=1&pageSize=2"),
            ("GET", "/api.xro/2.0/Invoices?page=2&pageSize=2"),
            ("GET", format!("{INVOICES}/{INV_3}").as_str()),
        ]
    );
    let refresh = &all[0];
    let basic = format!(
        "Basic {}",
        base64(format!("{CLIENT_ID}:{SECRET}").as_bytes())
    );
    assert_eq!(refresh.header("authorization"), Some(basic.as_str()));
    assert_eq!(
        refresh.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        refresh.header("xero-tenant-id"),
        None,
        "OAuth exchange is not tenant-scoped"
    );
    assert_eq!(refresh.header("accept"), Some("application/json"));
    // As in Xero's documented example, client_id is repeated in the form body beside Basic auth.
    let form = refresh.form_pairs();
    assert_eq!(
        form,
        [
            ("grant_type".to_owned(), "refresh_token".to_owned()),
            ("client_id".to_owned(), CLIENT_ID.to_owned()),
            ("refresh_token".to_owned(), "rt-original-0".to_owned()),
        ]
    );
    assert!(
        !refresh.body_text().contains(SECRET),
        "secret only in Basic auth"
    );
    for req in &all[1..] {
        assert_accounting_head(req, "at-rotated-1", TENANT_A);
        assert!(req.body.is_empty());
    }

    // The whole rotated pair is durable; the consumed refresh token is gone from disk.
    let journal = journal_text(&a.dir);
    assert!(journal.contains("rt-rotated-1") && journal.contains("at-rotated-1"));
    assert!(!journal.contains("rt-original-0") && !journal.contains("at-original-0"));
    #[cfg(unix)]
    {
        let mode = std::fs::metadata(&a.dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    // Restart: reopen the journal (not a new grant). No second refresh; the rotated token is used.
    drop((a.xero, a.tenant, a.store));
    let reopened = FileStore::open(&a.dir, TenantId::new(TENANT_A).unwrap(), CLIENT_ID).unwrap();
    let left = reopened.remaining_lifetime().unwrap();
    assert!(
        left > Duration::from_secs(1700) && left <= Duration::from_secs(1800),
        "{left:?}"
    );
    let xero = Xero::with_origins(reopened, SECRET, &server.url(), &server.url()).unwrap();
    let tenant = xero.tenant(&TenantId::new(TENANT_A).unwrap()).unwrap();
    let again = tenant
        .get(&tenant.invoice(INV_1).unwrap())
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(again.reference.id(), INV_1);
    assert_eq!(token_requests(&server).len(), 1);
    assert_accounting_head(server.requests().last().unwrap(), "at-rotated-1", TENANT_A);
}

/// Partial results, resumption, legacy paging and failing pages for workflow 1. One client; the
/// fixture chooses its behaviour from `pageSize`, which is all a list request carries.
#[tokio::test]
async fn workflow1_partial_results_failures_and_legacy_paging() {
    let server = TestServer::start(|req: &Recorded| {
        if req.path() != INVOICES {
            return Reply::json(404, r#"{"Type":null,"Title":"NotFound","Status":404}"#);
        }
        let page = page_of(req);
        let size: u32 = req.query_all("pageSize")[0].parse().unwrap();
        let two = || {
            vec![
                invoice(INV_1, "INV-0001", "AUTHORISED"),
                invoice(INV_2, "INV-0002", "PAID"),
            ]
        };
        match (size, page) {
            // modern, two pages
            (2, 1) => Reply::json(200, envelope(two(), Some((1, 2, 2, 3)))),
            (2, 2) => Reply::json(
                200,
                envelope(
                    vec![invoice(INV_3, "INV-0003", "DRAFT")],
                    Some((2, 2, 2, 3)),
                ),
            ),
            // legacy (no metadata): full, short nonempty, empty
            (3, 1) => Reply::json(
                200,
                envelope(
                    vec![
                        invoice(INV_1, "INV-0001", "PAID"),
                        invoice(INV_2, "INV-0002", "PAID"),
                        invoice(INV_3, "INV-0003", "PAID"),
                    ],
                    None,
                ),
            ),
            (3, 2) => Reply::json(
                200,
                envelope(vec![invoice(CREATED, "INV-0004", "PAID")], None),
            ),
            (3, 3) => Reply::json(200, envelope(vec![], None)),
            // second page fails with the organisation offline (text body)
            (4, 1) => {
                let mut four = two();
                four.extend([
                    invoice(INV_3, "INV-0003", "PAID"),
                    invoice(CREATED, "INV-0004", "PAID"),
                ]);
                Reply::json(200, envelope(four, Some((1, 4, 2, 6))))
            }
            (4, 2) => Reply::text(503, "text/plain", "The Organisation is offline"),
            // inconsistent metadata: says page 2 for a page-1 request
            (5, 1) => Reply::json(200, envelope(two(), Some((2, 5, 2, 7)))),
            // XML (Xero's default) instead of JSON
            (6, 1) => Reply::text(
                200,
                "text/xml; charset=utf-8",
                "<Response><Invoices/></Response>",
            ),
            // truncated JSON
            (7, 1) => Reply::json(200, r#"{"Invoices":[{"InvoiceID":"#),
            _ => Reply::json(404, r#"{"Type":null,"Title":"NotFound"}"#),
        }
    })
    .await;
    let a = app(
        &server,
        "w1-partial",
        TENANT_A,
        "at-live-1",
        "rt-live-1",
        1200,
    );
    let quiet = ["at-live-1", "rt-live-1", SECRET, TENANT_A, INV_1, INV_2];

    // Local validation happens before I/O (the SDK refuses Xero's silent clamping).
    for (page, size) in [(0, 2), (1, 0), (1, 1001)] {
        let err = a.tenant.list(page, size).unwrap_err();
        assert!(matches!(err.kind(), ErrorKind::Input(_)), "{err:?}");
    }
    assert!(
        a.tenant
            .pages(None, 1001, NonZeroUsize::new(1).unwrap())
            .is_err()
    );
    assert_eq!(server.requests().len(), 0);

    // A cap reached while the provider has a next page is a typed partial result, not completion.
    let incomplete = a
        .tenant
        .pages(None, 2, NonZeroUsize::new(1).unwrap())
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    let cursor = match &incomplete.error {
        PagesError::Bounded {
            resume: Resume::At(cursor),
            ..
        } => cursor.clone(),
        other => panic!("expected Bounded, got {other:?}"),
    };
    assert_eq!(cursor.page(), 2);
    // Resume from the cursor with the same tenant and size: only page 2 is fetched.
    let before = server.requests().len();
    let rest = a
        .tenant
        .pages(Some(cursor.clone()), 2, NonZeroUsize::new(5).unwrap())
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        rest.iter().map(|i| i.reference.id()).collect::<Vec<_>>(),
        [INV_3]
    );
    assert_eq!(server.requests().len(), before + 1);
    assert_eq!(
        server.requests().last().unwrap().target,
        "/api.xro/2.0/Invoices?page=2&pageSize=2"
    );
    // A continuation is bound to its page size.
    let err = a
        .tenant
        .pages(Some(cursor), 3, NonZeroUsize::new(5).unwrap())
        .unwrap_err();
    assert!(matches!(err.kind(), ErrorKind::Input(_)), "{err:?}");

    // InvoicePage::next drives single-page calls just as well.
    let first = a.tenant.list(1, 2).unwrap().send().await.unwrap();
    assert_eq!(first.pagination.map(|p| p.item_count), Some(3));
    assert_eq!(first.next.as_ref().map(|c| c.page()), Some(2));

    // Legacy: without metadata even a short nonempty page continues until an empty page.
    let before = server.requests().len();
    let legacy = a
        .tenant
        .pages(None, 3, NonZeroUsize::new(10).unwrap())
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(legacy.len(), 4);
    assert_eq!(server.requests().len(), before + 3);

    // A failing second page keeps the first page's items and the status/metadata of the failure.
    let incomplete = a
        .tenant
        .pages(None, 4, NonZeroUsize::new(10).unwrap())
        .unwrap()
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 4, "{:?}", incomplete.error);
    let err = incomplete.error.provider_error().expect("provider failure");
    assert_eq!(err.class(), Some(Class::Unavailable));
    assert!(!err.write_uncertain());
    match err.kind() {
        ErrorKind::Provider(failure) => {
            assert_eq!(failure.meta.status().as_u16(), 503);
            assert!(failure.document.is_err(), "text body is not a JSON problem");
        }
        other => panic!("expected Provider, got {other:?}"),
    }
    assert!(matches!(incomplete.error.resume(), Some(Resume::At(c)) if c.page() == 2));
    assert_quiet(err, &quiet);

    // Contract failures on 2xx bodies.
    let err = a.tenant.list(1, 5).unwrap().send().await.unwrap_err();
    assert!(matches!(err.kind(), ErrorKind::Contract(_)), "{err:?}");
    assert_eq!(err.class(), None);
    let err = a.tenant.list(1, 6).unwrap().send().await.unwrap_err();
    assert!(
        matches!(err.kind(), ErrorKind::Concord(_)),
        "XML refused: {err:?}"
    );
    let err = a.tenant.list(1, 7).unwrap().send().await.unwrap_err();
    assert!(
        matches!(err.kind(), ErrorKind::Concord(_)),
        "truncated JSON: {err:?}"
    );
    assert_quiet(&err, &quiet);

    // 404 on a retrieval is classified and not replayed.
    let before = server.requests().len();
    let ghost = a
        .tenant
        .invoice("0f0e0d0c-0b0a-4908-8706-050403020100")
        .unwrap();
    let err = a.tenant.get(&ghost).unwrap().send().await.unwrap_err();
    assert_eq!(err.class(), Some(Class::NotFound));
    assert_eq!(server.requests().len(), before + 1);
    // Every request in this test used the live access token for tenant A; no refresh happened.
    assert!(token_requests(&server).is_empty());
    for req in accounting_requests(&server) {
        assert_accounting_head(&req, "at-live-1", TENANT_A);
    }
}

/// A retrieval whose body names another invoice is refused, and a 429 GET is repeated once only
/// with a valid Retry-After.
#[tokio::test]
async fn workflow1_retrieval_contract_and_rate_limit() {
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let server = TestServer::start(move |req: &Recorded| {
        let id = req.path().rsplit('/').next().unwrap().to_owned();
        if id == INV_1 {
            // wrong invoice in the body
            return Reply::json(
                200,
                envelope(vec![invoice(INV_2, "INV-0002", "PAID")], None),
            );
        }
        if id == INV_2 {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            return if n == 0 {
                Reply::json(429, r#"{"Title":"Too Many Requests"}"#).header("retry-after", "1")
            } else {
                Reply::json(
                    200,
                    envelope(vec![invoice(INV_2, "INV-0002", "PAID")], None),
                )
            };
        }
        Reply::json(429, r#"{"Title":"Too Many Requests"}"#)
            .header("x-rate-limit-problem", "minute")
    })
    .await;
    let a = app(&server, "w1-rate", TENANT_A, "at-live-2", "rt-live-2", 1200);

    let err = a
        .tenant
        .get(&a.tenant.invoice(INV_1).unwrap())
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert!(matches!(err.kind(), ErrorKind::Contract(_)), "{err:?}");
    assert_quiet(&err, &[INV_1, INV_2, "at-live-2"]);

    let started = Instant::now();
    let ok = a
        .tenant
        .get(&a.tenant.invoice(INV_2).unwrap())
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(ok.reference.id(), INV_2);
    assert!(
        started.elapsed() >= Duration::from_secs(1),
        "Retry-After is a minimum"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 2);

    // Without a hint the 429 is returned, once.
    let before = server.requests().len();
    let err = a
        .tenant
        .get(&a.tenant.invoice(INV_3).unwrap())
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert_eq!(err.class(), Some(Class::RateLimited));
    assert_eq!(server.requests().len(), before + 1);
    match err.kind() {
        ErrorKind::Provider(failure) => assert!(matches!(failure.retry_after, Ok(None))),
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------- workflow 2

/// Create a draft sales invoice, then retrieve its current state in the same tenant.
#[tokio::test]
async fn workflow2_creates_a_draft_then_retrieves_its_state() {
    let server = TestServer::start(|req: &Recorded| match (req.method.as_str(), req.path()) {
        ("PUT" | "POST", INVOICES) => {
            let sent: Value = serde_json::from_slice(&req.body).unwrap();
            let lines = sent["Invoices"][0]["LineItems"].clone();
            let mut created = invoice(CREATED, "INV-0042", "DRAFT");
            created["LineItems"] = lines;
            Reply::json(200, envelope(vec![created], None))
        }
        ("GET", p) if p == format!("{INVOICES}/{CREATED}") => Reply::json(
            200,
            envelope(vec![invoice(CREATED, "INV-0042", "DRAFT")], None),
        ),
        _ => Reply::json(404, "{}"),
    })
    .await;
    let a = app(&server, "w2", TENANT_A, "at-live-3", "rt-live-3", 1200);

    let contact = a.tenant.contact(CONTACT).unwrap();
    let lines = vec![
        LineItem::new(
            "Consulting — October",
            3,
            Amount::from_minor_units(12_500).unwrap(),
        )
        .unwrap(),
        LineItem::new("Travel", 1, Amount::from_minor_units(4_205).unwrap()).unwrap(),
    ];
    let draft = a.tenant.draft(contact, lines).unwrap();
    let created = a.tenant.create(draft).unwrap().send().await.unwrap();
    assert_eq!(created.reference.id(), CREATED);
    assert_eq!(created.reference.tenant(), a.tenant.id());
    assert_eq!(
        (created.status.as_str(), created.invoice_type.as_str()),
        ("DRAFT", "ACCREC")
    );
    assert_eq!(created.invoice_number.as_deref(), Some("INV-0042"));

    let state = a
        .tenant
        .get(&created.reference)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(state.reference, created.reference);
    assert_eq!(state.status, "DRAFT");

    let all = server.requests();
    assert_eq!(all.len(), 2);
    let create = &all[0];
    assert_eq!(create.method, "POST");
    assert_eq!(create.target, "/api.xro/2.0/Invoices?summarizeErrors=true");
    assert_accounting_head(create, "at-live-3", TENANT_A);
    assert_eq!(create.header("content-type"), Some("application/json"));
    let body: Value = serde_json::from_slice(&create.body).unwrap();
    let sent = &body["Invoices"][0];
    assert_eq!(sent["Type"], "ACCREC");
    assert_eq!(sent["Status"], "DRAFT");
    assert_eq!(sent["Contact"]["ContactID"], CONTACT);
    assert_eq!(sent["LineItems"][0]["Description"], "Consulting — October");
    assert_eq!(sent["LineItems"][0]["Quantity"], json!(3));
    // Exact two-decimal JSON numbers on the wire (a reparsed Value would print 125.0).
    let text = create.body_text();
    assert!(
        text.contains(r#""UnitAmount":125.00"#) && text.contains(r#""UnitAmount":42.05"#),
        "{text}"
    );
    assert_eq!(
        sent.as_object().unwrap().len(),
        4,
        "only Type, Status, Contact, LineItems: {sent}"
    );
    assert_eq!(all[1].method, "GET");
    assert_eq!(all[1].target, format!("{INVOICES}/{CREATED}"));
    assert_accounting_head(&all[1], "at-live-3", TENANT_A);
}

/// Create failures: which ones are explicit refusals and which leave the write uncertain. The
/// fixture chooses its behaviour from the first line description.
#[tokio::test]
async fn workflow2_create_failures_and_write_uncertainty() {
    let server = TestServer::start(|req: &Recorded| {
        let sent: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
        let case = sent["Invoices"][0]["LineItems"][0]["Description"].as_str().unwrap_or("").to_owned();
        match case.as_str() {
            "case 400" => Reply::json(
                400,
                json!({
                    "ErrorNumber": 10,
                    "Type": "ValidationException",
                    "Message": "A validation exception occurred",
                    "Elements": [{ "ValidationErrors": [{ "Message": "Contact Ridgeway University is archived" }] }]
                })
                .to_string(),
            ),
            "case 200 errors" => {
                let mut inv = invoice("00000000-0000-0000-0000-000000000000", "", "DRAFT");
                inv["HasErrors"] = json!(true);
                inv["StatusAttributeString"] = json!("ERROR");
                inv["ValidationErrors"] = json!([{ "Message": "Account code '' is not a valid code" }]);
                Reply::json(200, envelope(vec![inv], None))
            }
            "case 500" => Reply::json(500, r#"{"ErrorNumber":500,"Type":"UnknownErrorException","Message":"An error occurred in Xero"}"#),
            "case hangup" => Reply::hang_up(),
            "case authorised" => Reply::json(200, envelope(vec![invoice(CREATED, "INV-0043", "AUTHORISED")], None)),
            "case two" => Reply::json(
                200,
                envelope(vec![invoice(CREATED, "INV-0044", "DRAFT"), invoice(INV_1, "INV-0045", "DRAFT")], None),
            ),
            _ => Reply::json(404, "{}"),
        }
    })
    .await;
    let a = app(&server, "w2-fail", TENANT_A, "at-live-4", "rt-live-4", 1200);
    let quiet = ["at-live-4", "rt-live-4", SECRET, TENANT_A, CONTACT, CREATED];
    let attempt = |case: &'static str| {
        let tenant = a.tenant.clone();
        async move {
            let line = LineItem::new(case, 1, Amount::from_minor_units(10_000).unwrap()).unwrap();
            let draft = tenant
                .draft(tenant.contact(CONTACT).unwrap(), vec![line])
                .unwrap();
            tenant.create(draft).unwrap().send().await.unwrap_err()
        }
    };

    let err = attempt("case 400").await;
    assert_eq!(err.class(), Some(Class::Validation));
    assert!(
        !err.write_uncertain(),
        "an explicit 400 refusal is not uncertain"
    );
    match err.kind() {
        ErrorKind::Provider(failure) => {
            let problem = failure.document.as_ref().expect("JSON problem document");
            assert_eq!(problem.number, Some(10));
            assert_eq!(
                problem.exception_type.as_deref(),
                Some("ValidationException")
            );
            assert!(problem.elements.is_some());
        }
        other => panic!("{other:?}"),
    }
    assert_quiet(&err, &quiet);
    assert_quiet(&err, &["archived", "Ridgeway"]);

    let err = attempt("case 200 errors").await;
    assert!(
        matches!(err.kind(), ErrorKind::InvoiceValidation),
        "{err:?}"
    );
    assert!(!err.write_uncertain());

    let err = attempt("case 500").await;
    assert_eq!(err.class(), Some(Class::Internal));
    assert!(err.write_uncertain(), "5xx after a create is uncertain");

    let err = attempt("case hangup").await;
    assert!(matches!(err.kind(), ErrorKind::Concord(_)), "{err:?}");
    assert!(
        err.write_uncertain(),
        "lost response after a create is uncertain"
    );

    let err = attempt("case authorised").await;
    assert!(matches!(err.kind(), ErrorKind::Contract(_)), "{err:?}");
    assert!(err.write_uncertain(), "inconsistent success is uncertain");

    let err = attempt("case two").await;
    assert!(matches!(err.kind(), ErrorKind::Contract(_)), "{err:?}");
    assert!(err.write_uncertain());
    assert_quiet(&err, &quiet);

    // Exactly one wire request per attempt: creates are never replayed.
    assert_eq!(server.requests().len(), 6);
    assert!(
        server
            .requests()
            .iter()
            .all(|r| r.path() == INVOICES && r.method == server.requests()[0].method)
    );

    // Local draft validation, before I/O.
    assert!(matches!(
        LineItem::new("x", 0, Amount::from_minor_units(1).unwrap())
            .unwrap_err()
            .kind(),
        ErrorKind::Input(_)
    ));
    assert!(matches!(
        Amount::from_minor_units(100_000_000_000)
            .unwrap_err()
            .kind(),
        ErrorKind::Input(_)
    ));
    assert!(
        a.tenant
            .draft(a.tenant.contact(CONTACT).unwrap(), vec![])
            .is_err()
    );
    assert!(a.tenant.contact("not-a-uuid").is_err());
    assert_eq!(server.requests().len(), 6);
}

// ---------------------------------------------------------------- required failure: rotation

/// A 401 is returned without replay; the next operation refreshes with the ROTATED refresh token
/// (never the consumed one), and only the newest pair is durable.
#[tokio::test]
async fn rotation_after_unauthorized_uses_the_rotated_refresh_token() {
    let server = TestServer::start(|req: &Recorded| match (req.method.as_str(), req.path()) {
        ("POST", TOKEN) => {
            let form = req.form_pairs();
            let presented = form
                .iter()
                .find(|(k, _)| k == "refresh_token")
                .map(|(_, v)| v.as_str());
            match presented {
                Some("rt-original-1") => token_reply("at-gen-1", "rt-gen-1"),
                Some("rt-gen-1") => token_reply("at-gen-2", "rt-gen-2"),
                _ => Reply::json(400, r#"{"error":"invalid_grant"}"#),
            }
        }
        ("GET", INVOICES) => {
            if req.header("authorization") == Some("Bearer at-gen-1") {
                Reply::json(
                    401,
                    r#"{"Type":null,"Title":"Unauthorized","Status":401,"Detail":"TokenExpired"}"#,
                )
            } else {
                Reply::json(
                    200,
                    envelope(
                        vec![invoice(INV_1, "INV-0001", "PAID")],
                        Some((1, 10, 1, 1)),
                    ),
                )
            }
        }
        _ => Reply::json(404, "{}"),
    })
    .await;
    let a = app(
        &server,
        "rotate-401",
        TENANT_A,
        "at-original-1",
        "rt-original-1",
        10,
    );

    let err = a.tenant.list(1, 10).unwrap().send().await.unwrap_err();
    assert_eq!(err.class(), Some(Class::Unauthorized));
    assert_eq!(
        accounting_requests(&server).len(),
        1,
        "a 401 is not replayed"
    );
    assert_quiet(
        &err,
        &["at-gen-1", "rt-gen-1", "rt-original-1", SECRET, TENANT_A],
    );

    let page = a.tenant.list(1, 10).unwrap().send().await.unwrap();
    assert_eq!(page.invoices.len(), 1);
    assert!(page.next.is_none());

    let presented: Vec<String> = token_requests(&server)
        .iter()
        .map(|r| {
            r.form_pairs()
                .into_iter()
                .find(|(k, _)| k == "refresh_token")
                .unwrap()
                .1
        })
        .collect();
    assert_eq!(presented, ["rt-original-1", "rt-gen-1"]);
    assert_accounting_head(
        accounting_requests(&server).last().unwrap(),
        "at-gen-2",
        TENANT_A,
    );
    let journal = journal_text(&a.dir);
    assert!(journal.contains("rt-gen-2"));
    assert!(!journal.contains("rt-gen-1") && !journal.contains("rt-original-1"));
    // Clones share the cache: no further refresh.
    let clone = a.xero.clone().tenant(a.tenant.id()).unwrap();
    clone.list(1, 10).unwrap().send().await.unwrap();
    assert_eq!(token_requests(&server).len(), 2);
}

/// A journal that cannot be written fails before the OAuth exchange: nothing is sent, so the
/// current refresh token is not consumed and the operation succeeds once storage is fixed.
/// Unix only: the failure is injected with POSIX directory permissions.
#[cfg(unix)]
#[tokio::test]
async fn rotation_storage_failure_before_exchange_sends_nothing() {
    let server = TestServer::start(|req: &Recorded| match req.path() {
        TOKEN => token_reply("at-gen-1", "rt-gen-1"),
        _ => Reply::json(200, envelope(vec![], Some((1, 10, 0, 0)))),
    })
    .await;
    let a = app(
        &server,
        "rotate-ro",
        TENANT_A,
        "at-original-2",
        "rt-original-2",
        -1,
    );

    std::fs::set_permissions(&a.dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    let err = a.tenant.list(1, 10).unwrap().send().await.unwrap_err();
    std::fs::set_permissions(&a.dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        matches!(err.kind(), ErrorKind::Authentication(_)),
        "{err:?}"
    );
    assert!(
        matches!(credential_kind(&err), ErrorKind::Storage),
        "{err:?}"
    );
    assert!(
        server.requests().is_empty(),
        "no OAuth or Accounting request before durable state"
    );
    assert_quiet(
        &err,
        &["rt-original-2", SECRET, TENANT_A, a.dir.to_str().unwrap()],
    );

    let page = a.tenant.list(1, 10).unwrap().send().await.unwrap();
    assert!(page.invoices.is_empty() && page.next.is_none());
    assert_eq!(token_requests(&server).len(), 1);
    assert_eq!(
        token_requests(&server)[0]
            .form_pairs()
            .iter()
            .find(|(k, _)| k == "refresh_token")
            .unwrap()
            .1,
        "rt-original-2"
    );
}

/// Real failed activation rename: during the exchange the fixture replaces `state.json` with a
/// non-empty directory, so `pending.json` cannot be renamed over it. The operation fails without
/// Accounting dispatch; after the operator restores the marker, `recover_pending()` publishes the
/// complete rotated pair with no network I/O and a restarted client uses it.
#[tokio::test]
async fn rotation_failed_activation_is_recovered_without_a_second_exchange() {
    let dir = journal_dir("rotate-rename");
    let fault_dir = dir.clone();
    let server = TestServer::start(move |req: &Recorded| match req.path() {
        TOKEN => {
            let state = fault_dir.join("state.json");
            std::fs::rename(&state, fault_dir.join("state.json.operator-copy")).unwrap();
            std::fs::create_dir(&state).unwrap();
            std::fs::write(state.join("blocker"), b"x").unwrap();
            token_reply("at-gen-1", "rt-gen-1")
        }
        _ => Reply::json(
            200,
            envelope(
                vec![invoice(INV_1, "INV-0001", "PAID")],
                Some((1, 10, 1, 1)),
            ),
        ),
    })
    .await;
    let tenant_id = TenantId::new(TENANT_A).unwrap();
    let grant = Grant::new(
        tenant_id.clone(),
        CLIENT_ID,
        "at-original-3",
        "rt-original-3",
        SCOPES,
        SystemTime::now(),
    )
    .unwrap();
    let store = FileStore::create(&dir, grant).unwrap();
    let xero = Xero::with_origins(store, SECRET, &server.url(), &server.url()).unwrap();
    let tenant = xero.tenant(&tenant_id).unwrap();

    let err = tenant.list(1, 10).unwrap().send().await.unwrap_err();
    assert!(
        matches!(credential_kind(&err), ErrorKind::Storage),
        "{err:?}"
    );
    assert!(
        accounting_requests(&server).is_empty(),
        "no Accounting request before activation"
    );
    assert_eq!(token_requests(&server).len(), 1);
    assert!(
        journal_text(&dir).contains("rt-gen-1"),
        "complete rotated pair is pending on disk"
    );
    assert_quiet(
        &err,
        &["rt-gen-1", "at-gen-1", "rt-original-3", SECRET, TENANT_A],
    );

    // A retry in the same process does not repeat the exchange.
    let err2 = tenant.list(1, 10).unwrap().send().await.unwrap_err();
    assert!(
        matches!(credential_kind(&err2), ErrorKind::RecoveryRequired),
        "{err2:?}"
    );
    assert_eq!(token_requests(&server).len(), 1);
    drop((tenant, xero));

    // Operator repair: remove the obstruction and put the original marker back.
    std::fs::remove_dir_all(dir.join("state.json")).unwrap();
    std::fs::rename(dir.join("state.json.operator-copy"), dir.join("state.json")).unwrap();
    let reopened = FileStore::open(&dir, tenant_id.clone(), CLIENT_ID).unwrap();
    reopened.recover_pending().unwrap();
    assert_eq!(server.requests().len(), 1, "recovery is local");
    assert!(reopened.remaining_lifetime().unwrap() > Duration::from_secs(1700));

    let restarted = Xero::with_origins(reopened, SECRET, &server.url(), &server.url()).unwrap();
    let page = restarted
        .tenant(&tenant_id)
        .unwrap()
        .list(1, 10)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(page.invoices.len(), 1);
    assert_eq!(token_requests(&server).len(), 1, "no second exchange");
    assert_accounting_head(
        accounting_requests(&server).last().unwrap(),
        "at-gen-1",
        TENANT_A,
    );
    let journal = journal_text(&dir);
    assert!(journal.contains("rt-gen-1") && !journal.contains("rt-original-3"));
}

/// The rotated pair is lost before it is journaled (the fixture blocks `pending.json`). The SDK
/// fails closed: the consumed refresh token is never presented again, and the journal requires
/// reauthorization rather than silently reusing the old pair.
#[tokio::test]
async fn rotation_lost_before_journal_fails_closed() {
    let dir = journal_dir("rotate-lost");
    let fault_dir = dir.clone();
    let server = TestServer::start(move |req: &Recorded| match req.path() {
        TOKEN => {
            std::fs::create_dir(fault_dir.join("pending.json")).unwrap();
            std::fs::write(fault_dir.join("pending.json").join("blocker"), b"x").unwrap();
            token_reply("at-gen-1", "rt-gen-1")
        }
        _ => Reply::json(200, envelope(vec![], Some((1, 10, 0, 0)))),
    })
    .await;
    let tenant_id = TenantId::new(TENANT_A).unwrap();
    let grant = Grant::new(
        tenant_id.clone(),
        CLIENT_ID,
        "at-original-4",
        "rt-original-4",
        SCOPES,
        SystemTime::now(),
    )
    .unwrap();
    let store = FileStore::create(&dir, grant).unwrap();
    let xero = Xero::with_origins(store.clone(), SECRET, &server.url(), &server.url()).unwrap();
    let tenant = xero.tenant(&tenant_id).unwrap();

    let err = tenant.list(1, 10).unwrap().send().await.unwrap_err();
    assert!(
        matches!(credential_kind(&err), ErrorKind::Storage),
        "{err:?}"
    );
    assert!(accounting_requests(&server).is_empty());
    std::fs::remove_dir_all(dir.join("pending.json")).unwrap();
    assert!(
        !journal_text(&dir).contains("rt-original-4"),
        "the consumed token is not the saved pair"
    );

    let err = tenant.list(1, 10).unwrap().send().await.unwrap_err();
    assert!(
        matches!(credential_kind(&err), ErrorKind::RecoveryRequired),
        "{err:?}"
    );
    assert_eq!(
        journal_names(&dir),
        ["state.json"],
        "a Refreshing marker, no pending pair, no lock"
    );
    // OBSERVED (FIRST_USE.md F7): recover_pending() reports "no pending pair" as Storage, although
    // the README routes this state to reauthorization; the journal itself says RecoveryRequired.
    let err = store.recover_pending().unwrap_err();
    assert!(matches!(err.kind(), ErrorKind::Storage), "{err:?}");
    let err = store.remaining_lifetime().unwrap_err();
    assert!(matches!(err.kind(), ErrorKind::RecoveryRequired), "{err:?}");
    drop((tenant, xero, store));
    // Restart: the journal reopens, but no client is built over it.
    let reopened = FileStore::open(&dir, tenant_id.clone(), CLIENT_ID).unwrap();
    let err = Xero::with_origins(reopened, SECRET, &server.url(), &server.url()).unwrap_err();
    assert!(matches!(err.kind(), ErrorKind::RecoveryRequired), "{err:?}");
    assert_quiet(
        &err,
        &[
            "rt-original-4",
            "rt-gen-1",
            SECRET,
            TENANT_A,
            dir.to_str().unwrap(),
        ],
    );
    assert_eq!(
        token_requests(&server).len(),
        1,
        "old refresh token never presented twice"
    );
    assert!(accounting_requests(&server).is_empty());
}

// ---------------------------------------------------------------- required failure: tenants

/// Tokens and tenant-bound values never cross tenants; every refusal happens before I/O.
#[tokio::test]
async fn tenant_bound_values_never_cross_tenants() {
    let server = TestServer::start(|req: &Recorded| {
        let tenant = req.header("xero-tenant-id").unwrap_or("").to_owned();
        let id = if tenant == TENANT_A { INV_1 } else { INV_2 };
        Reply::json(
            200,
            envelope(vec![invoice(id, "INV-0001", "PAID")], Some((1, 1, 3, 3))),
        )
    })
    .await;
    let a = app(
        &server,
        "tenant-a",
        TENANT_A,
        "at-tenant-a",
        "rt-tenant-a",
        1200,
    );
    let b = app(
        &server,
        "tenant-b",
        TENANT_B,
        "at-tenant-b",
        "rt-tenant-b",
        1200,
    );
    let id_b = TenantId::new(TENANT_B).unwrap();

    // A credential journal enters only its own tenant.
    let err = a.xero.tenant(&id_b).unwrap_err();
    assert!(matches!(err.kind(), ErrorKind::TenantMismatch), "{err:?}");
    assert_quiet(&err, &[TENANT_A, TENANT_B, "at-tenant-a"]);
    let err = FileStore::open(&a.dir, id_b.clone(), CLIENT_ID).unwrap_err();
    assert!(matches!(err.kind(), ErrorKind::TenantMismatch), "{err:?}");
    let err =
        FileStore::open(&a.dir, TenantId::new(TENANT_A).unwrap(), "another-client").unwrap_err();
    assert!(matches!(err.kind(), ErrorKind::TenantMismatch), "{err:?}");
    assert!(server.requests().is_empty());

    // Values produced in A.
    let page_a = a.tenant.list(1, 1).unwrap().send().await.unwrap();
    let ref_a = page_a.invoices[0].reference.clone();
    let cursor_a = page_a.next.clone().unwrap();
    let contact_a = a.tenant.contact(CONTACT).unwrap();
    let line = LineItem::new("Consulting", 1, Amount::from_minor_units(1234).unwrap()).unwrap();
    let draft_a = a
        .tenant
        .draft(contact_a.clone(), vec![line.clone()])
        .unwrap();
    let imported_a = a.tenant.invoice(INV_1).unwrap();
    assert_eq!(server.requests().len(), 1);

    // ...are refused in B, before I/O.
    let mismatch = |e: Error| assert!(matches!(e.kind(), ErrorKind::TenantMismatch), "{e:?}");
    mismatch(b.tenant.get(&ref_a).unwrap_err());
    mismatch(b.tenant.get(&imported_a).unwrap_err());
    mismatch(
        b.tenant
            .pages(Some(cursor_a), 1, NonZeroUsize::new(3).unwrap())
            .unwrap_err(),
    );
    mismatch(b.tenant.draft(contact_a, vec![line]).unwrap_err());
    mismatch(b.tenant.create(draft_a).unwrap_err());
    assert_eq!(server.requests().len(), 1);

    // Each client sends only its own token to its own tenant.
    let page_b = b.tenant.list(1, 1).unwrap().send().await.unwrap();
    assert_eq!(page_b.invoices[0].reference.tenant(), &id_b);
    assert_ne!(page_b.invoices[0].reference, ref_a);
    let reqs = server.requests();
    assert_accounting_head(&reqs[0], "at-tenant-a", TENANT_A);
    assert_accounting_head(&reqs[1], "at-tenant-b", TENANT_B);

    // Same raw ID imported in two tenants yields two distinct references.
    assert_ne!(
        a.tenant.invoice(INV_1).unwrap(),
        b.tenant.invoice(INV_1).unwrap()
    );
}

/// Grant validation and redaction of the long-lived handles.
#[tokio::test]
async fn grant_scopes_and_redaction() {
    let t = TenantId::new(TENANT_A).unwrap();
    let at = SystemTime::now();
    for scopes in [
        "accounting.invoices",
        "offline_access",
        "offline_access accounting.contacts",
    ] {
        let err = Grant::new(t.clone(), CLIENT_ID, "a", "r", scopes, at).unwrap_err();
        assert!(
            matches!(err.kind(), ErrorKind::Input(_)),
            "{scopes}: {err:?}"
        );
    }
    for scopes in [
        "offline_access accounting.invoices.read",
        "offline_access accounting.transactions",
        "openid profile email accounting.transactions.read offline_access",
    ] {
        Grant::new(t.clone(), CLIENT_ID, "a", "r", scopes, at).unwrap();
    }
    assert!(TenantId::new("not-a-tenant").is_err());
    // OBSERVED GAP (FIRST_USE.md F8): `TenantId: Deserialize` skips the UUID check of
    // `TenantId::new`; such a value reaches the wire unchanged as `xero-tenant-id`.
    let unchecked: TenantId = serde_json::from_str("\"not-a-tenant\"").unwrap();
    let grant = Grant::new(
        unchecked.clone(),
        CLIENT_ID,
        "a",
        "r",
        SCOPES,
        at + Duration::from_secs(1200),
    )
    .unwrap();
    let probe =
        TestServer::start(|_: &Recorded| Reply::json(200, envelope(vec![], Some((1, 10, 0, 0)))))
            .await;
    let store = FileStore::create(journal_dir("unchecked-tenant"), grant).unwrap();
    let xero = Xero::with_origins(store, SECRET, &probe.url(), &probe.url()).unwrap();
    xero.tenant(&unchecked)
        .unwrap()
        .list(1, 10)
        .unwrap()
        .send()
        .await
        .unwrap();
    assert_eq!(
        probe.single().header("xero-tenant-id"),
        Some("not-a-tenant")
    );

    let server = TestServer::start(|_: &Recorded| Reply::json(500, "{}")).await;
    let a = app(
        &server,
        "redact",
        TENANT_A,
        "at-redact-secret",
        "rt-redact-secret",
        1200,
    );
    let grant = Grant::new(
        t.clone(),
        CLIENT_ID,
        "at-redact-secret",
        "rt-redact-secret",
        SCOPES,
        at,
    )
    .unwrap();
    let text = format!(
        "{grant:?} {:?} {:?} {:?} {:?}",
        a.xero,
        a.tenant,
        a.store,
        a.tenant.invoice(INV_1).unwrap()
    );
    for secret in ["at-redact-secret", "rt-redact-secret", SECRET, INV_1] {
        assert!(!text.contains(secret), "{secret} leaked: {text}");
    }

    // A read-only grant: is a create refused locally, or sent?
    let ro_dir = journal_dir("read-only");
    let ro_grant = Grant::new(
        t.clone(),
        CLIENT_ID,
        "at-ro",
        "rt-ro",
        "offline_access accounting.invoices.read",
        at + Duration::from_secs(1200),
    )
    .unwrap();
    let ro = Xero::with_origins(
        FileStore::create(&ro_dir, ro_grant).unwrap(),
        SECRET,
        &server.url(),
        &server.url(),
    )
    .unwrap();
    let ro_tenant = ro.tenant(&t).unwrap();
    let line = LineItem::new("Consulting", 1, Amount::from_minor_units(1).unwrap()).unwrap();
    let draft = ro_tenant
        .draft(ro_tenant.contact(CONTACT).unwrap(), vec![line])
        .unwrap();
    // Observed: the operation builds, and `send()` refuses it locally.
    let op = ro_tenant
        .create(draft)
        .expect("build is not where the scope is checked");
    let err = op.send().await.unwrap_err();
    assert!(matches!(err.kind(), ErrorKind::Input(_)), "{err:?}");
    assert!(!err.write_uncertain());
    assert!(server.requests().is_empty(), "refused before I/O");

    // Origins: HTTPS or loopback HTTP only, without paths.
    let store = FileStore::open(&a.dir, t.clone(), CLIENT_ID).unwrap();
    assert!(
        Xero::with_origins(
            store.clone(),
            SECRET,
            "http://example.com",
            "https://identity.xero.com"
        )
        .is_err()
    );
    assert!(
        Xero::with_origins(
            store.clone(),
            SECRET,
            "https://api.xero.com/api.xro/2.0",
            "https://identity.xero.com"
        )
        .is_err()
    );
    assert!(
        Xero::with_origins(
            store,
            SECRET,
            "https://api.xero.com",
            "https://identity.xero.com"
        )
        .is_ok()
    );
}

/// Every refresh exchange that does not end with a complete, valid, journaled pair fails closed:
/// no Accounting request, the consumed refresh token is erased from disk and never presented
/// again, and the journal then requires reauthorization. This includes an OAuth 503 and a refused
/// connection (FIRST_USE.md F11: Xero's documented 30-minute grace retry is not available).
#[tokio::test]
async fn rotation_refused_or_failed_exchange_fails_closed() {
    let server = TestServer::start(|req: &Recorded| {
        if req.path() != TOKEN {
            return Reply::json(200, envelope(vec![], Some((1, 10, 0, 0))));
        }
        let form = req.form_pairs();
        let presented = form.iter().find(|(k, _)| k == "refresh_token").unwrap().1.clone();
        let pair = |expires_in: u64, token_type: &str, scope: &str| {
            Reply::json(
                200,
                json!({ "access_token": "at-new", "refresh_token": "rt-new", "expires_in": expires_in,
                        "token_type": token_type, "scope": scope })
                .to_string(),
            )
        };
        match presented.as_str() {
            "rt-invalid-grant" => Reply::json(400, r#"{"error":"invalid_grant"}"#),
            "rt-too-long" => pair(3600, "Bearer", SCOPES),
            "rt-not-bearer" => pair(1800, "mac", SCOPES),
            "rt-scope-narrowed" => pair(1800, "Bearer", "offline_access accounting.invoices.read"),
            "rt-no-refresh" => Reply::json(
                200,
                json!({ "access_token": "at-new", "expires_in": 1800, "token_type": "Bearer",
                        "scope": SCOPES })
                .to_string(),
            ),
            "rt-identity-503" => Reply::text(503, "text/html", "<html>Service Unavailable</html>"),
            _ => Reply::json(500, "{}"),
        }
    })
    .await;

    // (refresh token, OAuth status if answered, inner kind label)
    let cases: [(&str, Option<u16>, &str); 6] = [
        ("rt-invalid-grant", Some(400), "Provider"),
        ("rt-too-long", None, "Contract"),
        ("rt-not-bearer", None, "Contract"),
        ("rt-scope-narrowed", None, "Contract"),
        ("rt-no-refresh", None, "Concord"),
        ("rt-identity-503", Some(503), "Provider"),
    ];
    for (n, (refresh, status, inner_label)) in cases.into_iter().enumerate() {
        let a = app(&server, refresh, TENANT_A, "at-expired", refresh, -1);
        let err = a.tenant.list(1, 10).unwrap().send().await.unwrap_err();
        let ErrorKind::Authentication(inner) = err.kind() else {
            panic!("{refresh}: {err:?}")
        };
        // The OAuth HTTP class sits on the inner error; the outer error has none.
        assert_eq!(err.class(), None, "{refresh}");
        match (inner.kind(), status) {
            (ErrorKind::Provider(failure), Some(code)) => {
                assert_eq!(failure.meta.status().as_u16(), code, "{refresh}");
                assert_eq!(inner_label, "Provider");
            }
            (ErrorKind::Contract(_), None) => assert_eq!(inner_label, "Contract", "{refresh}"),
            (ErrorKind::Concord(_), None) => assert_eq!(inner_label, "Concord", "{refresh}"),
            (other, _) => panic!("{refresh}: unexpected inner {other:?}"),
        }
        assert_eq!(
            token_requests(&server).len(),
            n + 1,
            "{refresh}: exactly one exchange"
        );
        assert!(
            accounting_requests(&server).is_empty(),
            "{refresh}: no Accounting dispatch"
        );
        let journal = journal_text(&a.dir);
        assert!(
            !journal.contains(refresh),
            "{refresh}: consumed token erased from disk"
        );
        assert!(
            !journal.contains("rt-new"),
            "{refresh}: no unvalidated pair saved"
        );
        assert_quiet(&err, &[refresh, "rt-new", "at-new", SECRET, TENANT_A]);

        let again = a.tenant.list(1, 10).unwrap().send().await.unwrap_err();
        assert!(
            matches!(credential_kind(&again), ErrorKind::RecoveryRequired),
            "{again:?}"
        );
        assert!(matches!(
            a.store.remaining_lifetime().unwrap_err().kind(),
            ErrorKind::RecoveryRequired
        ));
        assert_eq!(
            token_requests(&server).len(),
            n + 1,
            "{refresh}: never presented twice"
        );
    }

    // A refused connection to the identity origin: no HTTP request reaches any server, yet the
    // old refresh token is gone and the journal needs reauthorization.
    let tenant_id = TenantId::new(TENANT_A).unwrap();
    let grant = Grant::new(
        tenant_id.clone(),
        CLIENT_ID,
        "at-expired",
        "rt-unreachable",
        SCOPES,
        SystemTime::now(),
    )
    .unwrap();
    let dir = journal_dir("rotate-unreachable");
    let store = FileStore::create(&dir, grant).unwrap();
    let xero =
        Xero::with_origins(store.clone(), SECRET, &server.url(), "http://127.0.0.1:9").unwrap();
    let tenant = xero.tenant(&tenant_id).unwrap();
    let before = server.requests().len();
    let err = tenant.list(1, 10).unwrap().send().await.unwrap_err();
    assert!(
        matches!(credential_kind(&err), ErrorKind::Concord(_)),
        "{err:?}"
    );
    let again = tenant.list(1, 10).unwrap().send().await.unwrap_err();
    assert!(
        matches!(credential_kind(&again), ErrorKind::RecoveryRequired),
        "{again:?}"
    );
    assert_eq!(server.requests().len(), before);
    assert!(!journal_text(&dir).contains("rt-unreachable"));
}
