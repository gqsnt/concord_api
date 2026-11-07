//! Both connected workflows end to end against the stateful fake, and the required failure: a
//! replaced or expired refresh token is a recoverable authorization error of its own realm.

mod support;

use std::time::Duration;

use concord_example_testserver::Gate;
use concord_sdk_quickbooks_online::{
    Activity, AuthorizationCallback, AuthorizationCode, AuthorizationKind, CallbackError,
    CustomerQuery, Decimal, EntityId, Error, NewInvoice, RefreshToken, SalesItemLine,
    UncertainCause, customer_pages,
};
use support::*;

fn invoice_for(customer: &EntityId) -> NewInvoice {
    let item = EntityId::new("1").unwrap();
    NewInvoice::new(
        customer.clone(),
        SalesItemLine::new(Decimal::from_cents(10000), item),
    )
}

/// Workflow 1: query the realm's customers page by page, then read the selected one.
#[tokio::test]
async fn query_customers_then_read_the_selected_one() {
    let fake = Fake::new();
    fake.grant_refresh(REALM_A, "synthetic-refresh-a-0");
    fake.add_customers(
        REALM_A,
        &["customer-query-page1.json", "customer-query-page2.json"],
    );
    let server = fake.start().await;
    let qbo = qbo(
        &server,
        connection(&server, REALM_A, "synthetic-refresh-a-0"),
    );

    let query = CustomerQuery::new()
        .activity(Activity::Active)
        .order_by_display_name()
        .max_results(2)
        .unwrap();
    let customers = customer_pages(&qbo, query)
        .max_pages(std::num::NonZeroUsize::new(5).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    let names: Vec<_> = customers
        .iter()
        .map(|c| c.display_name.as_deref())
        .collect();
    assert_eq!(
        names,
        [
            Some("Synthetic Bird Supply"),
            Some("Synthetic Consulting"),
            Some("Synthetic Windsurf Shop")
        ]
    );

    let selected = &customers[1];
    let read = qbo
        .customer(selected.id.clone())
        .read()
        .send()
        .await
        .unwrap();
    assert_eq!(read.customer.id, selected.id);
    assert_eq!(
        read.customer.balance.as_ref().map(Decimal::as_str),
        Some("390.0")
    );

    // Pages 1 and 2 (full), then the short page 3 ends the walk; one token for all four calls.
    let api = api_requests(&server);
    let starts: Vec<_> = api
        .iter()
        .filter(|r| r.path().ends_with("/query"))
        .map(|r| r.query_all("query").pop().unwrap())
        .collect();
    assert_eq!(
        starts,
        [
            "SELECT * FROM Customer WHERE Active = true ORDERBY DisplayName STARTPOSITION 1 MAXRESULTS 2",
            "SELECT * FROM Customer WHERE Active = true ORDERBY DisplayName STARTPOSITION 3 MAXRESULTS 2",
        ]
    );
    assert_eq!(api.len(), 3);
    assert_eq!(api[2].path(), format!("/v3/company/{REALM_A}/customer/29"));
    assert_eq!(token_requests(&server).len(), 1);
    for request in &api {
        assert_eq!(path_realm(request).as_deref(), Some(REALM_A));
        assert_eq!(
            bearer(request).as_deref(),
            Some(format!("synthetic-access-{REALM_A}-1").as_str())
        );
    }
}

/// Workflow 2: create a sandbox invoice for a customer (sent once, with a request ID), then
/// retrieve it.
#[tokio::test]
async fn create_an_invoice_for_a_customer_then_retrieve_it() {
    let fake = Fake::new();
    fake.grant_refresh(REALM_A, "synthetic-refresh-a-0");
    fake.add_customers(REALM_A, &["customer-query-page1.json"]);
    let server = fake.start().await;
    let qbo = qbo(
        &server,
        connection(&server, REALM_A, "synthetic-refresh-a-0"),
    );

    let customer = qbo
        .customers()
        .query(
            CustomerQuery::new()
                .display_name("Synthetic Consulting")
                .unwrap(),
        )
        .send()
        .await
        .unwrap()
        .page
        .customers
        .remove(0);
    assert_eq!(
        customer.display_name.as_deref(),
        Some("Synthetic Consulting")
    );
    let created = qbo
        .invoices()
        .create(invoice_for(&customer.id))
        .send()
        .await
        .unwrap()
        .invoice;
    assert_eq!(created.customer_ref.value, customer.id.as_str());
    assert_eq!(
        created.total_amt.as_ref().map(Decimal::as_str),
        Some("100.0")
    );

    let fetched = qbo
        .invoice(created.id.clone())
        .read()
        .send()
        .await
        .unwrap()
        .invoice;
    assert_eq!(fetched, created);

    let api = api_requests(&server);
    let create = &api[1];
    assert_eq!(create.method, "POST");
    assert_eq!(create.path(), format!("/v3/company/{REALM_A}/invoice"));
    let request_id = create.query_all("requestid").pop().unwrap();
    assert_eq!(request_id.len(), 32);
    assert!(request_id.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(
        api[2].path(),
        format!("/v3/company/{REALM_A}/invoice/{}", created.id)
    );
    assert_eq!(fake.creates_received(), 1);
    assert_eq!(fake.invoices(REALM_A), 1);
    assert_eq!(token_requests(&server).len(), 1);
}

/// The required failure. Realm A's refresh token was replaced (it is no longer accepted); realm
/// B's is fine. A's calls fail with a recoverable authorization error naming A, no Accounting
/// request is sent for A, and nothing of B's is used; B keeps working. A recovers with a new
/// authorization of the same realm, and a callback for another realm is refused unsent.
#[tokio::test]
async fn a_replaced_refresh_token_is_a_recoverable_error_of_its_own_realm() {
    let fake = Fake::new();
    fake.grant_refresh(REALM_B, "synthetic-refresh-b-0");
    fake.add_customers(REALM_A, &["customer-query-page1.json"]);
    fake.add_customers(REALM_B, &["customer-query-page2.json"]);
    let server = fake.start().await;
    // A's stored token was replaced elsewhere: the fake does not know it any more.
    let a = connection(&server, REALM_A, "synthetic-refresh-a-replaced");
    let b = connection(&server, REALM_B, "synthetic-refresh-b-0");
    let qbo_a = qbo(&server, a.clone());
    let qbo_b = qbo(&server, b.clone());

    let error = qbo_a
        .customers()
        .query(CustomerQuery::new())
        .send()
        .await
        .unwrap_err();
    let auth = error.authorization().expect("an authorization error");
    assert_eq!(auth.kind(), AuthorizationKind::GrantRejected);
    assert!(auth.needs_reauthorization());
    assert_eq!(auth.realm().as_str(), REALM_A);
    assert_eq!(auth.oauth_error(), Some("invalid_grant"));
    assert_eq!(auth.token_status().map(|s| s.as_u16()), Some(400));
    assert!(a.needs_reauthorization());
    assert!(!b.needs_reauthorization());
    assert!(
        api_requests(&server).is_empty(),
        "nothing was sent to the Accounting API"
    );

    // Realm B is unaffected: its own token, its own path.
    let page = qbo_b
        .customers()
        .query(CustomerQuery::new())
        .send()
        .await
        .unwrap()
        .page;
    assert_eq!(page.customers.len(), 1);

    // A's next call fails the same way without asking the token endpoint again.
    let tokens_before = token_requests(&server).len();
    let again = qbo_a
        .customer(EntityId::new("1").unwrap())
        .read()
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        again.authorization().unwrap().kind(),
        AuthorizationKind::GrantRejected
    );
    assert_eq!(token_requests(&server).len(), tokens_before);

    // A callback for realm B cannot repair A: refused before any request.
    fake.grant_code(REALM_B, "synthetic-code-b");
    let wrong = AuthorizationCallback::new(
        AuthorizationCode::new("synthetic-code-b").unwrap(),
        "synthetic-state",
        realm(REALM_B),
    );
    let refused = a
        .reauthorize(&http(Duration::from_secs(5)), wrong, "synthetic-state")
        .await;
    assert!(matches!(
        refused,
        Err(Error::Callback(CallbackError::RealmMismatch))
    ));
    assert_eq!(token_requests(&server).len(), tokens_before);

    // The user reauthorizes realm A: every handle of the connection works again.
    fake.grant_code(REALM_A, "synthetic-code-a");
    let callback = AuthorizationCallback::from_query(&format!(
        "code=synthetic-code-a&state=synthetic-state&realmId={REALM_A}"
    ))
    .unwrap();
    a.reauthorize(&http(Duration::from_secs(5)), callback, "synthetic-state")
        .await
        .unwrap();
    assert!(!a.needs_reauthorization());
    let page = qbo_a
        .customers()
        .query(CustomerQuery::new())
        .send()
        .await
        .unwrap()
        .page;
    assert_eq!(page.customers.len(), 2);

    // Every Accounting request carried a token the fake issued for the realm in its path.
    for request in api_requests(&server) {
        let realm = path_realm(&request).unwrap();
        let token = bearer(&request).unwrap();
        assert!(
            token.starts_with(&format!("synthetic-access-{realm}-")),
            "{token} on {realm}"
        );
    }
    // The code exchange's access token served A's first call: no refresh after it.
    let grants: Vec<_> = token_requests(&server)
        .iter()
        .map(|r| r.form_pairs()[0].1.clone())
        .collect();
    assert_eq!(
        grants,
        ["refresh_token", "refresh_token", "authorization_code"]
    );
}

/// An access token the service expires is discarded; the refresh uses the **rotated** refresh
/// token, the read is sent once more, and the replaced refresh token is no longer accepted.
#[tokio::test]
async fn an_expired_access_token_is_refreshed_with_the_rotated_refresh_token() {
    let fake = Fake::new();
    fake.grant_refresh(REALM_A, "synthetic-refresh-a-0");
    fake.add_customers(REALM_A, &["customer-query-page1.json"]);
    let server = fake.start().await;
    let a = connection(&server, REALM_A, "synthetic-refresh-a-0");
    let qbo = qbo(&server, a.clone());

    qbo.customer(EntityId::new("1").unwrap())
        .read()
        .send()
        .await
        .unwrap();
    let rotated = a.refresh_token();
    assert_eq!(
        rotated.expose_secret(),
        format!("synthetic-refresh-{REALM_A}-1")
    );
    assert!(a.refresh_token_expires_at().is_some());
    assert!(
        !fake.refresh_valid("synthetic-refresh-a-0"),
        "the old token was replaced"
    );

    fake.expire_access(&format!("synthetic-access-{REALM_A}-1"));
    let customer = qbo
        .customer(EntityId::new("29").unwrap())
        .read()
        .send()
        .await
        .unwrap();
    assert_eq!(customer.customer.id.as_str(), "29");

    let tokens = token_requests(&server);
    assert_eq!(tokens.len(), 2);
    assert!(
        tokens[1]
            .body_text()
            .contains(&format!("refresh_token=synthetic-refresh-{REALM_A}-1"))
    );
    let api = api_requests(&server);
    assert_eq!(api.len(), 3, "first read, rejected read, the one repeat");
    assert_eq!(api[1].target, api[2].target);
    assert_eq!(
        bearer(&api[2]).unwrap(),
        format!("synthetic-access-{REALM_A}-2")
    );

    // The replaced refresh token, presented again (a stale copy), is a recoverable rejection.
    a.replace_refresh_token(RefreshToken::new("synthetic-refresh-a-0").unwrap());
    let error = qbo
        .customer(EntityId::new("1").unwrap())
        .read()
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        error.authorization().unwrap().kind(),
        AuthorizationKind::GrantRejected
    );
    assert_eq!(api_requests(&server).len(), 3);
}

/// A create whose response never arrives was committed: it is uncertain, carries its request
/// ID, is never resent by the SDK, and a deliberate replay with the same ID returns the original
/// invoice instead of a second one.
#[tokio::test]
async fn a_timed_out_create_is_uncertain_and_replays_only_deliberately() {
    let fake = Fake::new();
    fake.grant_refresh(REALM_A, "synthetic-refresh-a-0");
    let server = fake.start().await;
    let a = connection(&server, REALM_A, "synthetic-refresh-a-0");
    let customer = EntityId::new("29").unwrap();

    let impatient = qbo_with(&server, a.clone(), Duration::from_millis(300));
    fake.delay_next_create(Duration::from_secs(2));
    let error = impatient
        .invoices()
        .create(invoice_for(&customer))
        .send()
        .await
        .unwrap_err();
    let uncertain = error.uncertain().expect("an uncertain write");
    assert!(matches!(uncertain.cause(), UncertainCause::NoResponse(_)));
    assert_eq!(uncertain.operation(), "QuickBooks.invoices.create");
    assert_eq!(uncertain.realm().as_str(), REALM_A);
    let request_id = uncertain.request_id().cloned().unwrap();
    assert_eq!(fake.creates_received(), 1, "sent once");
    assert_eq!(fake.invoices(REALM_A), 1, "and committed");

    let patient = qbo(&server, a);
    let replayed = patient
        .invoices()
        .create(invoice_for(&customer))
        .set_request_id(request_id.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(fake.creates_received(), 2);
    assert_eq!(fake.invoices(REALM_A), 1, "no duplicate invoice");
    assert_eq!(replayed.invoice.id.as_str(), "238");
    let creates: Vec<_> = api_requests(&server)
        .into_iter()
        .filter(|r| r.method == "POST")
        .map(|r| r.query_all("requestid"))
        .collect();
    assert_eq!(
        creates,
        [[request_id.to_string()], [request_id.to_string()]]
    );
}

/// A 5xx after a committed create is uncertain too, and a 401 on a create is returned without
/// sending it again; the next call acquires a new token.
#[tokio::test]
async fn a_failed_create_is_never_resent() {
    let fake = Fake::new();
    fake.grant_refresh(REALM_A, "synthetic-refresh-a-0");
    let server = fake.start().await;
    let a = connection(&server, REALM_A, "synthetic-refresh-a-0");
    let qbo = qbo(&server, a);
    let customer = EntityId::new("29").unwrap();

    fake.fail_next_create(503);
    let error = qbo
        .invoices()
        .create(invoice_for(&customer))
        .send()
        .await
        .unwrap_err();
    let uncertain = error.uncertain().unwrap();
    assert!(matches!(uncertain.cause(), UncertainCause::ServerError(_)));
    assert_eq!(error.status().map(|s| s.as_u16()), Some(503));
    assert_eq!(fake.creates_received(), 1);

    fake.expire_access(&format!("synthetic-access-{REALM_A}-1"));
    let error = qbo
        .invoices()
        .create(invoice_for(&customer))
        .send()
        .await
        .unwrap_err();
    let auth = error.authorization().unwrap();
    assert_eq!(auth.kind(), AuthorizationKind::AccessTokenRejected);
    assert_eq!(auth.response().map(|r| r.status().as_u16()), Some(401));
    let posts = || {
        api_requests(&server)
            .iter()
            .filter(|r| r.method == "POST")
            .count()
    };
    assert_eq!(posts(), 2, "the rejected create was not resent");
    assert_eq!(
        fake.creates_received(),
        1,
        "the service refused it before processing"
    );
    assert_eq!(token_requests(&server).len(), 1);

    qbo.invoices()
        .create(invoice_for(&customer))
        .send()
        .await
        .unwrap();
    assert_eq!(
        token_requests(&server).len(),
        2,
        "the rejected token was discarded"
    );
    assert_eq!(fake.invoices(REALM_A), 2);
}

/// Concurrent calls on one connection share a single refresh.
#[tokio::test]
async fn concurrent_calls_share_one_refresh() {
    let fake = Fake::new();
    fake.grant_refresh(REALM_A, "synthetic-refresh-a-0");
    fake.add_customers(REALM_A, &["customer-query-page1.json"]);
    let gate = Gate::new();
    fake.hold_tokens(gate.clone());
    let server = fake.start().await;
    let qbo = qbo(
        &server,
        connection(&server, REALM_A, "synthetic-refresh-a-0"),
    );

    let first = qbo.customer(EntityId::new("1").unwrap()).read().send();
    let second = qbo.customer(EntityId::new("29").unwrap()).read().send();
    let release = async {
        assert!(
            server
                .wait_until(Duration::from_secs(5), |s| !token_requests(s).is_empty())
                .await
        );
        // Both reads are waiting on the one refresh in flight; neither has been sent.
        assert!(api_requests(&server).is_empty());
        gate.open();
    };
    let (first, second, ()) = tokio::join!(first, second, release);
    assert_eq!(first.unwrap().customer.id.as_str(), "1");
    assert_eq!(second.unwrap().customer.id.as_str(), "29");
    assert_eq!(token_requests(&server).len(), 1);
}

/// The authorization-code flow: the exchange's access token serves the first call.
#[tokio::test]
async fn an_authorization_code_connects_a_realm() {
    let fake = Fake::new();
    fake.grant_code(REALM_B, "synthetic-code-b");
    fake.add_customers(REALM_B, &["customer-query-page2.json"]);
    let server = fake.start().await;
    let app = app(&server);
    let url = app.authorization_url("synthetic-state").unwrap();
    assert_eq!(url.host_str(), Some("appcenter.intuit.com"));

    let callback = AuthorizationCallback::from_query(&format!(
        "?code=synthetic-code-b&state=synthetic-state&realmId={REALM_B}"
    ))
    .unwrap();
    let mismatch = app
        .exchange_code(
            &http(Duration::from_secs(5)),
            callback.clone(),
            "another-state",
        )
        .await;
    assert!(matches!(
        mismatch,
        Err(Error::Callback(CallbackError::StateMismatch))
    ));
    assert!(token_requests(&server).is_empty());

    let b = app
        .exchange_code(&http(Duration::from_secs(5)), callback, "synthetic-state")
        .await
        .unwrap();
    assert_eq!(b.realm().as_str(), REALM_B);
    let qbo = qbo(&server, b.clone());
    let page = qbo
        .customers()
        .query(CustomerQuery::new())
        .send()
        .await
        .unwrap()
        .page;
    assert_eq!(page.customers[0].id.as_str(), "58");
    assert_eq!(token_requests(&server).len(), 1, "the exchange only");
    assert_eq!(
        b.refresh_token().expose_secret(),
        format!("synthetic-refresh-{REALM_B}-1")
    );

    // The code is spent: exchanging it again is a rejected grant, sent once.
    let again = AuthorizationCallback::from_query(&format!(
        "code=synthetic-code-b&state=s&realmId={REALM_B}"
    ))
    .unwrap();
    let error = app
        .exchange_code(&http(Duration::from_secs(5)), again, "s")
        .await
        .unwrap_err();
    assert_eq!(
        error.authorization().unwrap().kind(),
        AuthorizationKind::GrantRejected
    );
    assert_eq!(token_requests(&server).len(), 2);
}
