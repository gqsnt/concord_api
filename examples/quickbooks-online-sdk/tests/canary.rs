//! Secrets: the OAuth client secret (`OAuthApp::new`, `src/auth.rs:206`; Debug withholds it,
//! `src/auth.rs:320`; sent in the token endpoint's `#[sensitive]` Basic header, `src/lib.rs:137`,
//! checked decoded), the stored refresh token (`RefreshToken`, `src/auth.rs:58`;
//! `OAuthApp::connection`, form body of the refresh grant), the authorization code
//! (`AuthorizationCode`, `src/auth.rs:91`, via `AuthorizationCallback::from_query`; form body of
//! `exchange_code`), the access token and the rotated refresh token (response-only:
//! `TokenResponse`, `src/auth.rs:371`; the access token travels as `Bearer`, the rotated refresh
//! token in the next refresh grant, forced on every call by an `expires_in` inside the 60 s
//! refresh margin), the `#[sensitive]` customer query statement (`src/lib.rs:81`;
//! `CustomerQuery` Debug withholds its `DisplayName` text, `src/query.rs:211`), and a refresh
//! token found in a contract-breaking token response (`TokenFailure::replacement`, stored and
//! sent on the next refresh). Each has its own canary.
//! The server echoes every canary of the test into headers, the `Fault` `Message`/`Detail`
//! (withheld by `FaultError`), the OAuth `error_description` and every malformed, wrong-media
//! and truncated body. Read (`customer(id).read()`) and write (`invoices().create`) cover the
//! seven scenarios, plus a 401 (read refreshed once, then `AccessTokenRejected`) and a `Fault`
//! inside a 200, through the generated `send()` (the connection's policy) and the native
//! `Prepared`; a filtered `customer_pages` traversal runs beside them, and a code exchange
//! succeeds in the success case. A second test drives the token endpoint through the seven
//! scenarios (`Error::Authorization`, `GrantRejected`/`GrantUncertain`). Not covered: native
//! reqwest values; `RefreshToken::expose_secret`, `FaultError::*_unredacted`,
//! `ProviderError::meta().headers()`; `CustomerQuery`'s `Display` (documented as the statement
//! sent); the realm ID, client ID, redirect URI, OAuth `state`, request IDs and entity IDs, which
//! are unmarked identifiers printed by design (fixed public values here). No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{num::NonZeroUsize, time::Duration};

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_example_testserver::refused_url()
}

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(400, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        7 => Reply::json(401, problem),
        8 => Reply::json(200, problem),
        _ => unreachable!(),
    };
    response
        .header("intuit_tid", "synthetic-tid")
        .header("x-request-id", echo)
        .header("set-cookie", echo)
}

async fn receive<T>(c: &mut Canaries, prepared: concord::Prepared<T>) -> Result<T, concord::Error> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error),
    }
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    c.assert_debug_absent("error kind", &error.kind());
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_quickbooks_online::{
    AuthorizationCallback, AuthorizationCode, CustomerQuery, EntityId, Environment, Error,
    NewInvoice, OAuthApp, ProviderError, QuickBooks, RealmId, RefreshToken, SalesItemLine,
    UncertainCause, customer_pages,
};

const REALM: &str = "9130350000000001";
const TOKEN_PATH: &str = "/oauth2/v1/tokens/bearer";

fn provider(c: &mut Canaries, p: &ProviderError) {
    c.assert_debug_absent("provider failure", p);
    c.assert_debug_absent("provider metadata", p.meta());
    c.assert_debug_absent("fault", &p.fault());
    c.assert_debug_absent("intuit_tid", &p.intuit_tid());
    match p.fault() {
        Ok(fault) => c.assert_debug_absent("fault errors", fault.errors()),
        Err(e) => concord_error(c, e),
    }
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error status", &error.status());
    match error {
        Error::Concord(e) => concord_error(c, e),
        Error::Provider(p) => provider(c, p),
        Error::Authorization(a) => {
            c.assert_debug_absent("authorization failure", a);
            c.assert_debug_absent("authorization kind", &a.kind());
            c.assert_debug_absent("oauth error", &a.oauth_error());
            c.assert_debug_absent("token status", &a.token_status());
            c.assert_debug_absent("violation", &a.violation());
            if let Some(e) = a.cause() {
                concord_error(c, e);
            }
            if let Some(p) = a.response() {
                provider(c, p);
            }
        }
        Error::Uncertain(w) => {
            c.assert_debug_absent("uncertain write", w);
            c.assert_debug_absent("uncertain cause", w.cause());
            match w.cause() {
                UncertainCause::NoResponse(e) | UncertainCause::UnreadableSuccess(e) => {
                    concord_error(c, e)
                }
                UncertainCause::ServerError(p) => provider(c, p),
                _ => {}
            }
        }
        _ => {}
    }
}

fn sent<T: std::fmt::Debug>(c: &mut Canaries, label: &str, result: &Result<T, Error>) {
    c.assert_debug_absent(label, result);
    if let Err(e) = result {
        error(c, e);
    }
}

fn oauth_app(c: &mut Canaries, secret: &str, token_base: &str) -> OAuthApp {
    let app = OAuthApp::new(
        Environment::Sandbox,
        "synthetic-public-client-id",
        secret,
        "https://app.example.test/oauth/callback",
    )
    .unwrap()
    .with_token_base_url(token_base)
    .unwrap();
    c.assert_debug_absent("OAuth app", &app);
    app
}

fn callback(c: &mut Canaries, code: &str) -> AuthorizationCallback {
    let query = format!("?code={code}&state=synthetic-state&realmId={REALM}");
    let callback = AuthorizationCallback::from_query(&query).unwrap();
    c.assert_debug_absent("callback", &callback);
    c.assert_debug_absent("authorization code", &AuthorizationCode::new(code).unwrap());
    callback
}

fn token(access: &str, refresh: &str, kind: &str) -> String {
    format!(
        r#"{{"token_type":"{kind}","expires_in":30,"refresh_token":"{refresh}","x_refresh_token_expires_in":8640000,"access_token":"{access}"}}"#
    )
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("quickbooks-online");
    let secret = c.issue();
    let refresh = c.issue();
    let code = c.issue();
    let access = c.issue();
    let rotated = c.issue();
    let name = c.issue();
    let echo = c.issued().join("-");
    let query = CustomerQuery::new()
        .display_name_starts_with(&name)
        .unwrap()
        .max_results(2)
        .unwrap();
    c.assert_debug_absent("customer query", &query);
    let cap = NonZeroUsize::new(3).unwrap();
    for case in 0..9 {
        let (echo, tokens) = (echo.clone(), token(&access, &rotated, "bearer"));
        let server = TestServer::start(move |r: &Recorded| {
            if r.path() == TOKEN_PATH {
                return Reply::json(200, &tokens);
            }
            let path = r.path().trim_start_matches("/v3/company/9130350000000001/");
            let success = match path {
                "customer/29" => include_str!("../fixtures/customer-read.json"),
                "invoice" => include_str!("../fixtures/invoice-created.json"),
                "query" if r.query_all("query")[0].contains("STARTPOSITION 3 ") => {
                    include_str!("../fixtures/customer-query-page2.json")
                }
                "query" => include_str!("../fixtures/customer-query-page1.json"),
                _ => include_str!("../fixtures/invoice-read.json"),
            };
            let problem = format!(
                r#"{{"Fault":{{"Error":[{{"Message":"{echo}","Detail":"{echo}","code":"2050","element":"DisplayName"}}],"type":"ValidationFault"}},"time":"2026-11-07T10:00:00.000-08:00"}}"#
            );
            reply(case, success, &problem, &echo)
        })
        .await;
        let app = oauth_app(&mut c, &secret, &server.url());
        let connection = app.connection(
            RealmId::new(REALM).unwrap(),
            RefreshToken::new(&refresh).unwrap(),
        );
        let base = if case == 6 { refused() } else { server.url() };
        let qbo = QuickBooks::connect(http(), connection.clone())
            .with_base_url(&base)
            .unwrap();
        let customer = EntityId::new("29").unwrap();
        let line = SalesItemLine::new(
            concord_sdk_quickbooks_online::Decimal::from_cents(10_000),
            EntityId::new("1").unwrap(),
        );
        let read = qbo.customer(customer.clone()).read();
        let write = qbo.invoices().create(NewInvoice::new(customer, line));
        let find = qbo.customers().query(query.clone());
        c.assert_debug_absent("connection", &connection);
        c.assert_display_absent("connection", &connection);
        c.assert_debug_absent("refresh token", &connection.refresh_token());
        c.assert_debug_absent("client", &qbo);
        c.assert_debug_absent("customers scope", &qbo.customers());
        c.assert_debug_absent("invoices scope", &qbo.invoices());
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("query endpoint", &find);
        c.assert_debug_absent("write Prepared", &write.clone().prepare().unwrap());
        c.assert_debug_absent("query Prepared", &find.clone().prepare().unwrap());
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("read Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let result = read.send().await;
        sent(&mut c, "read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = &result {
            // Withholding keeps the diagnosis: status, fault type and code, stage, operation.
            match case {
                1 => assert_eq!(
                    e.to_string(),
                    "QuickBooks answered 400 Bad Request with a fault (ValidationFault 2050)"
                ),
                3 => {
                    let Error::Concord(inner) = e else {
                        panic!("{e}")
                    };
                    assert_eq!(inner.kind(), concord::ErrorKind::Decode);
                    assert_eq!(inner.endpoint(), Some("QuickBooks.customer.read"));
                }
                7 => assert!(e.to_string().contains("rejected the access token (401)")),
                _ => {}
            }
        }
        let result = write.send().await;
        sent(&mut c, "write Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(Error::Uncertain(w)) = &result {
            assert_eq!(w.operation(), "QuickBooks.invoices.create");
            assert!(w.request_id().is_some());
        }
        let result = find.send().await;
        sent(&mut c, "query Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        let mut pages = customer_pages(&qbo, query.clone()).max_pages(cap);
        c.assert_debug_absent("traversal", &pages);
        let mut read_pages = 0;
        loop {
            let page = pages.next_page().await;
            c.assert_debug_absent("traversal page Result", &page);
            match page {
                Ok(Some(_)) => read_pages += 1,
                Ok(None) => break,
                Err(e) => {
                    c.assert_display_absent("PagesError", &e);
                    c.assert_error_absent("PagesError and sources", &e);
                    error(&mut c, e.provider_error().unwrap());
                    break;
                }
            }
        }
        assert_eq!(read_pages, if case == 0 { 2 } else { 0 });
        if case == 0 {
            let exchanged = app
                .exchange_code(&http(), callback(&mut c, &code), "synthetic-state")
                .await
                .unwrap();
            c.assert_debug_absent("exchanged connection", &exchanged);
            let qbo = QuickBooks::connect(http(), exchanged)
                .with_base_url(&base)
                .unwrap();
            let invoice = qbo.invoice(EntityId::new("238").unwrap()).read();
            sent(&mut c, "invoice Result", &invoice.send().await);
            c.assert_debug_absent("refresh expiry", &connection.refresh_token_expires_at());
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 300);
    println!(
        "quickbooks-online: {} renderings; positive control passed",
        c.checked()
    );
}

#[tokio::test]
async fn token_endpoint_failures_never_print() {
    let mut c = Canaries::new("quickbooks-online");
    let secret = c.issue();
    let refresh = c.issue();
    let code = c.issue();
    let replacement = c.issue();
    let echo = c.issued().join("-");
    for case in 0..7 {
        let (echo, broken) = (echo.clone(), token("synthetic-access", &replacement, "mac"));
        let server = TestServer::start(move |_: &Recorded| {
            let problem = format!(r#"{{"error":"invalid_grant","error_description":"{echo}"}}"#);
            reply(case, &broken, &problem, &echo)
        })
        .await;
        let token_base = if case == 6 { refused() } else { server.url() };
        let app = oauth_app(&mut c, &secret, &token_base);
        let connection = app.connection(
            RealmId::new(REALM).unwrap(),
            RefreshToken::new(&refresh).unwrap(),
        );
        let qbo = QuickBooks::connect(http(), connection.clone())
            .with_base_url(server.url())
            .unwrap();
        for _ in 0..2 {
            let result = qbo
                .customer(EntityId::new("29").unwrap())
                .read()
                .send()
                .await;
            sent(&mut c, "read Result", &result);
            let a = result.unwrap_err().authorization().map(|a| a.kind());
            assert!(
                a.is_some(),
                "every failure here is an authorization failure"
            );
        }
        c.assert_debug_absent("connection after failure", &connection);
        c.assert_debug_absent("kept refresh token", &connection.refresh_token());
        let result = app
            .exchange_code(&http(), callback(&mut c, &code), "synthetic-state")
            .await;
        sent(&mut c, "exchange Result", &result);
        let e = result.unwrap_err();
        let a = e.authorization().unwrap();
        if case == 1 {
            // Diagnosis survives: kind, status, OAuth code and realm.
            assert!(a.needs_reauthorization());
            assert_eq!(a.token_status().map(|s| s.as_u16()), Some(400));
            assert_eq!(a.oauth_error(), Some("invalid_grant"));
            assert!(e.to_string().contains(REALM));
        }
        if case == 0 {
            assert_eq!(a.violation(), Some("token_type is not bearer"));
            assert_eq!(connection.refresh_token().expose_secret(), replacement);
            assert!(server.requests().iter().all(|r| r.path() == TOKEN_PATH));
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    for bad in [format!("{}\n", c.issue()), String::new()] {
        let e =
            OAuthApp::new(Environment::Production, "id", &bad, "https://a.test/cb").unwrap_err();
        error(&mut c, &e);
    }
    let e = RefreshToken::new(format!("{}\n", c.issue())).unwrap_err();
    error(&mut c, &e);
    let e = AuthorizationCode::new(format!("{}\r", c.issue())).unwrap_err();
    error(&mut c, &e);
    println!(
        "quickbooks-online token endpoint: {} renderings",
        c.checked()
    );
}
