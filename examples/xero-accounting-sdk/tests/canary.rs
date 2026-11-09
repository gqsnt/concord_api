//! Secret inputs, each with its own canary (all of them travel; no wire exemption):
//! - OAuth client ID, the Basic username (Grant::new, FileStore::open: src/store.rs:41,162):
//!   Basic credential of /connect/token (src/auth.rs:40,71; decoded by the helper), refresh form.
//! - OAuth client secret (Xero::new/with_origins: src/lib.rs:72,84): Basic credential.
//! - access and refresh tokens (Grant::new: src/store.rs:42-43): Bearer and refresh form.
//! - rotated access and refresh tokens, response-only secrets of /connect/token
//!   (src/auth.rs:24-26): they reach the decoder in every token scenario, then travel as the
//!   Bearer and, after a 401 forces another rotation, in the next refresh form.
//! - draft line description (LineItem, Debug withheld: src/models.rs:194,212): create body.
//!
//! UUID-validated values cannot hold a canary (TenantId::new, Tenant::invoice/contact refuse
//! anything else): the #[sensitive] tenant header and invoice ID (src/lib.rs:43,49), the
//! withheld contact ID and the response-only created invoice ID are fixed UUIDs. `Probe`
//! asserts them absent (case-insensitively) from every rendering the canaries are checked in;
//! the three request-side ones are asserted on the wire explicitly. Every secret is echoed
//! into response headers, the withheld Problem fields (Message, error_description, Elements),
//! the withheld InvoiceNumber and the malformed, text/html and truncated bodies.
//!
//! Scenarios: get (read), create (write), a modified-since list and a page traversal across
//! 2xx, 400, 503, malformed, text/html, truncated and refused; the token refresh across the
//! same seven; a 401 that forces a second rotation. Not covered: the generated Accounting
//! client and its Prepared are private (`mod wire`), so no native Concord handle is reachable;
//! native reqwest values; the journal files, which hold the token pair in clear by design;
//! Problem and Invoice fields read by deliberate access (documented as returned in clear).

use concord_check_canary::Canaries;
use concord_sdk_xero_accounting::{
    Amount, Class, Error, ErrorKind, FileStore, Grant, LineItem, TenantId, Xero,
};
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use std::{fs, num::NonZeroUsize};

const TENANT: &str = "45e4708e-d862-4111-ab3a-dd8cd03913e1";
const CONTACT: &str = "3138017f-8ddc-420e-a159-e7e1cf9e643d";
const INVOICE: &str = "220ddca8-3144-4085-9a88-2d72c5133734";
const CREATED: &str = "d4b0f3a1-6c2e-4b8f-9e17-0a5c3b2d7e64";
const SCOPES: &str = "offline_access accounting.invoices";

#[derive(Clone)]
struct Secrets {
    client_id: String,
    secret: String,
    access: String,
    refresh: String,
    line: String,
    new_access: String,
    new_refresh: String,
}

/// The canary helper plus the UUID-shaped secrets that cannot carry a canary.
struct Probe(Canaries);

impl Probe {
    fn text(&mut self, label: &str, rendered: &str) {
        self.0.assert_absent(label, rendered);
        let lower = rendered.to_ascii_lowercase();
        for id in [TENANT, CONTACT, INVOICE, CREATED] {
            assert!(
                !lower.contains(id),
                "identifier leak: {id} in {label}: {rendered}"
            );
        }
    }
    fn debug<T: Debug + ?Sized>(&mut self, label: &str, value: &T) {
        self.text(label, &format!("{value:?}"));
        self.text(label, &format!("{value:#?}"));
    }
    fn chain(&mut self, label: &str, error: &(dyn std::error::Error + 'static)) {
        let mut current = Some(error);
        while let Some(e) = current {
            self.text(&format!("{label} (Display)"), &e.to_string());
            self.debug(&format!("{label} (Debug)"), e);
            current = e.source();
        }
    }
    fn concord(&mut self, e: &concord::Error) {
        self.chain("concord::Error", e);
        self.debug("concord error kind", &e.kind());
        self.debug("concord error head", &e.head());
        self.debug("concord error metadata", &e.head_meta());
    }
    fn error(&mut self, e: &Error) {
        self.chain("SDK error", e);
        self.debug("SDK error kind", e.kind());
        self.debug("SDK error class", &e.class());
        if let Some(inner) = e.concord_error() {
            self.concord(inner);
        }
        match e.kind() {
            ErrorKind::Authentication(inner) => self.error(inner),
            ErrorKind::Provider(p) => {
                self.debug("provider failure", p);
                self.debug("provider metadata", &p.meta);
                self.debug("provider retry-after", &p.retry_after);
                match &p.document {
                    Ok(problem) => self.debug("Problem", problem),
                    Err(inner) => self.concord(inner),
                }
            }
            _ => {}
        }
    }
    fn result<T: Debug>(&mut self, label: &str, result: &Result<T, Error>) {
        self.debug(label, result);
        if let Err(e) = result {
            self.error(e);
        }
    }
}

fn refused() -> String {
    concord_testserver::refused_url()
}

/// A new journal directory under <worktree>/target/tmp (never the system temp directory).
fn journal(name: &str) -> PathBuf {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp/xero-canary");
    fs::create_dir_all(&parent).unwrap();
    let nanos = SystemTime::UNIX_EPOCH.elapsed().unwrap().as_nanos();
    parent.join(format!("{name}-{}-{nanos}", std::process::id()))
}

fn grant(s: &Secrets, access: &str, expires_in: u64) -> Result<Grant, Error> {
    let expires = SystemTime::now() + Duration::from_secs(expires_in);
    let tenant = TenantId::new(TENANT).unwrap();
    Grant::new(tenant, &s.client_id, access, &s.refresh, SCOPES, expires)
}

fn client(p: &mut Probe, s: &Secrets, dir: &Path, url: &str, expired: bool) -> Xero {
    let grant = grant(s, &s.access, if expired { 0 } else { 1800 }).unwrap();
    p.debug("grant", &grant);
    p.debug("created journal", &FileStore::create(dir, grant).unwrap());
    let store = FileStore::open(dir, TenantId::new(TENANT).unwrap(), &s.client_id).unwrap();
    p.debug("reopened journal", &store);
    p.debug("journal tenant", store.tenant());
    p.debug("remaining lifetime", &store.remaining_lifetime());
    Xero::with_origins(store, &s.secret, url, url).unwrap()
}

fn invoice(id: &str, status: &str, echo: &str) -> String {
    format!(
        r#"{{"Status":"OK","Invoices":[{{"InvoiceID":"{id}","Type":"ACCREC","Status":"{status}","InvoiceNumber":"{echo}"}}]}}"#
    )
}

fn problem(echo: &str) -> String {
    format!(
        r#"{{"ErrorNumber":10,"Type":"ValidationException","Message":"{echo}","error":"invalid_grant","error_description":"{echo}","Elements":[{{"ValidationErrors":[{{"Message":"{echo}"}}]}}]}}"#
    )
}

fn token(s: &Secrets) -> String {
    format!(
        r#"{{"access_token":"{}","refresh_token":"{}","token_type":"Bearer","expires_in":1800,"scope":"{SCOPES}"}}"#,
        s.new_access, s.new_refresh
    )
}

/// Malformed and truncated bodies keep the success document's secrets so they reach the decoder.
fn reply(case: usize, success: String, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(400, problem(echo)),
        2 => Reply::json(503, problem(echo)),
        3 => Reply::json(200, format!("{success} {echo}")),
        4 => Reply::text(200, "text/html", format!("{success} {echo}")),
        5 => Reply::json(200, "").with_body(Body::FailAfter(vec![
            format!("{}{echo}", &success[..success.len() / 2]).into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("xero-accounting");
    let s = Secrets {
        client_id: c.issue(),
        secret: c.issue(),
        access: c.issue(),
        refresh: c.issue(),
        line: c.issue(),
        new_access: c.issue(),
        new_refresh: c.issue(),
    };
    let echo = format!(
        "{} {TENANT} {CONTACT} {INVOICE} {CREATED}",
        c.issued().join(" ")
    );
    let mut p = Probe(c);
    let mut wire: Vec<Recorded> = Vec::new();
    let tenant_id = TenantId::new(TENANT).unwrap();
    let bearer = |token: &str| format!("Bearer {token}");

    // Accounting: a valid grant, so the original access token is the Bearer.
    for case in 0..7 {
        let (srv, echo) = (s.clone(), echo.clone());
        let server = TestServer::start(move |r: &Recorded| {
            let success = match (r.method.as_str(), r.path()) {
                (_, "/connect/token") => token(&srv),
                ("POST", _) => invoice(CREATED, "DRAFT", &echo),
                _ => invoice(INVOICE, "AUTHORISED", &echo),
            };
            reply(case, success, &echo)
        })
        .await;
        let dir = journal("accounting");
        let url = if case == 6 { refused() } else { server.url() };
        let xero = client(&mut p, &s, &dir, &url, false);
        let tenant = xero.tenant(&tenant_id).unwrap();
        let reference = tenant.invoice(INVOICE).unwrap();
        let contact = tenant.contact(CONTACT).unwrap();
        let amount = Amount::from_minor_units(1234).unwrap();
        let line = LineItem::new(&s.line, 1, amount).unwrap();
        let draft = tenant.draft(contact.clone(), vec![line.clone()]).unwrap();
        let since = tenant.modified_since(SystemTime::now()).unwrap();
        p.debug("client", &xero);
        p.debug("tenant scope", &tenant);
        p.debug("tenant ID", tenant.id());
        p.debug("invoice reference", &reference);
        p.debug("contact reference", &contact);
        p.debug("amount", &amount);
        p.debug("line item", &line);
        p.debug("draft", &draft);
        p.debug("modified-since", &since);
        let get = tenant.get(&reference).unwrap();
        let create = tenant.create(draft).unwrap();
        let list = tenant.list_modified_since(&since, 1, 10).unwrap();
        p.debug("get endpoint", &get);
        p.debug("create endpoint", &create);
        p.debug("list endpoint", &list);
        let read = get.send().await;
        p.result("get Result", &read);
        let write = create.send().await;
        p.result("create Result", &write);
        let listed = list.send().await;
        p.result("list Result", &listed);
        assert_eq!((read.is_ok(), write.is_ok()), (case == 0, case == 0));
        assert_eq!(listed.is_ok(), case == 0);
        if let Ok(found) = &read {
            p.debug("invoice reference (returned)", &found.reference);
            p.debug("invoice tenant", found.reference.tenant());
            assert_eq!(found.reference.id(), INVOICE);
            // The withheld InvoiceNumber did carry the echo (deliberate access only).
            assert!(found.invoice_number.as_ref().unwrap().contains(&s.access));
        }
        if let Ok(page) = &listed {
            p.debug("page cursor", &page.next);
            p.debug("pagination", &page.pagination);
        }
        if let Err(e) = &write {
            // Safe diagnosis: stage, class, status and write certainty stay reachable.
            assert_eq!(e.write_uncertain(), case != 1);
            match case {
                1 | 2 => {
                    let ErrorKind::Provider(failure) = e.kind() else {
                        panic!("expected a provider failure, got {e:?}")
                    };
                    assert_eq!(e.to_string(), "provider refused the request");
                    let (class, status) = if case == 1 {
                        (Class::Validation, 400)
                    } else {
                        (Class::Unavailable, 503)
                    };
                    assert_eq!(
                        (e.class(), failure.meta.status().as_u16()),
                        (Some(class), status)
                    );
                    // The withheld Problem did carry the echo (deliberate access only).
                    let message = failure.document.as_ref().unwrap().message.as_ref();
                    assert!(message.unwrap().contains(&s.secret));
                }
                _ => {
                    assert_eq!(e.to_string(), "invoice request or response failed");
                    let kind = e.concord_error().unwrap().kind();
                    if case == 6 {
                        assert_eq!(kind, concord::ErrorKind::Transport);
                    }
                }
            }
        }
        let mut walk = tenant
            .pages(None, 10, NonZeroUsize::new(1).unwrap())
            .unwrap();
        p.debug("traversal", &walk);
        let page = walk.next_page().await;
        p.debug("traversal page Result", &page);
        if let Err(e) = &page {
            p.chain("PagesError", e);
            if let Some(e) = e.provider_error() {
                p.error(e);
            }
        }
        p.debug("traversal checkpoint", &walk.checkpoint());
        p.debug("traversal progress", &walk.progress());
        p.debug("traversal report", &walk.finish());
        let requests = server.requests();
        if case == 0 {
            assert_eq!(requests.len(), 4);
            for r in &requests {
                assert_eq!(r.header("authorization"), Some(bearer(&s.access).as_str()));
                assert_eq!(r.header("xero-tenant-id"), Some(TENANT));
            }
            assert!(requests.iter().any(|r| r.path().ends_with(INVOICE)));
            let posted = requests.iter().find(|r| r.method == "POST").unwrap();
            assert!(posted.body_text().contains(CONTACT));
            wire.extend(requests);
        } else if case == 6 {
            assert!(requests.is_empty());
        }
        server.shutdown().await;
        fs::remove_dir_all(&dir).unwrap();
    }

    // Token refresh: an expired grant, so every read first rotates the pair.
    for case in 0..7 {
        let (srv, echo) = (s.clone(), echo.clone());
        let server = TestServer::start(move |r: &Recorded| match (r.method.as_str(), r.path()) {
            (_, "/connect/token") => reply(case, token(&srv), &echo),
            // A 401 invalidates the rotated token and forces a second rotation.
            ("POST", _) => Reply::json(401, problem(&echo)),
            _ => Reply::json(200, invoice(INVOICE, "AUTHORISED", &echo)),
        })
        .await;
        let dir = journal("token");
        let url = if case == 6 { refused() } else { server.url() };
        let xero = client(&mut p, &s, &dir, &url, true);
        let tenant = xero.tenant(&tenant_id).unwrap();
        let read = tenant
            .get(&tenant.invoice(INVOICE).unwrap())
            .unwrap()
            .send()
            .await;
        p.result("refreshing get Result", &read);
        assert_eq!(read.is_ok(), case == 0);
        if let Err(e) = &read {
            assert_eq!(e.to_string(), "credential acquisition failed");
            let ErrorKind::Authentication(inner) = e.kind() else {
                panic!("expected an authentication failure, got {e:?}")
            };
            match case {
                1 => assert_eq!(inner.class(), Some(Class::Validation)),
                2 => assert_eq!(inner.class(), Some(Class::Unavailable)),
                _ => assert!(inner.concord_error().is_some()),
            }
        }
        if case == 0 {
            let line = LineItem::new(&s.line, 2, Amount::from_minor_units(1).unwrap()).unwrap();
            let draft = tenant
                .draft(tenant.contact(CONTACT).unwrap(), vec![line])
                .unwrap();
            let write = tenant.create(draft).unwrap().send().await;
            p.result("unauthorized create Result", &write);
            let e = write.unwrap_err();
            assert_eq!(
                (e.class(), e.write_uncertain()),
                (Some(Class::Unauthorized), false)
            );
            let listed = tenant.list(1, 10).unwrap().send().await;
            p.result("re-rotated list Result", &listed);
            assert!(listed.is_ok());
            let requests = server.requests();
            let paths: Vec<_> = requests.iter().map(|r| r.path().to_owned()).collect();
            assert_eq!(paths[0], "/connect/token");
            assert_eq!(paths[3], "/connect/token");
            assert_eq!(paths.len(), 5);
            for (r, refresh) in [(&requests[0], &s.refresh), (&requests[3], &s.new_refresh)] {
                let form = r.form_pairs();
                assert!(form.contains(&("refresh_token".into(), refresh.clone())));
                assert!(form.contains(&("client_id".into(), s.client_id.clone())));
                assert!(r.header("authorization").unwrap().starts_with("Basic "));
            }
            for r in [&requests[1], &requests[2], &requests[4]] {
                assert_eq!(
                    r.header("authorization"),
                    Some(bearer(&s.new_access).as_str())
                );
            }
            wire.extend(requests);
        } else if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
        fs::remove_dir_all(&dir).unwrap();
    }
    p.0.assert_on_wire(&wire);

    // Local refusals that receive a secret must not echo it (`error` covers Debug and Display).
    let dir = journal("local");
    let store = FileStore::create(&dir, grant(&s, &s.access, 1800).unwrap()).unwrap();
    let loopback = "http://127.0.0.1:9";
    let xero = Xero::with_origins(store.clone(), &s.secret, loopback, loopback).unwrap();
    let tenant = xero.tenant(&tenant_id).unwrap();
    let origin = format!("http://{}:{}@127.0.0.1:9/", s.client_id, s.secret);
    let wrong_client = format!("{}X", s.client_id);
    p.error(&TenantId::new(s.access.as_str()).unwrap_err());
    p.error(&tenant.invoice(s.line.as_str()).unwrap_err());
    p.error(&tenant.contact(s.secret.as_str()).unwrap_err());
    p.error(&grant(&s, &format!("{} {}", s.access, s.refresh), 1800).unwrap_err());
    p.error(&FileStore::open(&dir, tenant_id.clone(), wrong_client).unwrap_err());
    p.error(&Xero::with_origins(store, &s.secret, &origin, &origin).unwrap_err());
    fs::remove_dir_all(&dir).unwrap();

    let checked = p.0.checked();
    assert!(checked >= 1000, "only {checked} renderings checked");
    println!("xero-accounting: {checked} renderings; positive control passed (no exemption)");
}
