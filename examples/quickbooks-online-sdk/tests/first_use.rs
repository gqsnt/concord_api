//! Fresh caller capsule: public SDK API and local fixtures only.

use std::num::{NonZeroU32, NonZeroUsize};

use concord::ErrorKind;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_pagination::{Outcome, PagesError, Resume};
use concord_sdk_quickbooks_online::{
    Activity, AuthorizationCallback, AuthorizationKind, CallbackError, Connection, CustomerQuery,
    Decimal, EntityId, Environment, Error, NewInvoice, OAuthApp, QuickBooks, RealmId, RefreshToken,
    RequestId, SalesItemLine, UncertainCause, customer_pages, http_client,
};
use serde_json::{Value, json};

const REALM: &str = "9341456292345678";
const CUSTOMER: &str = "58";
const TIME: &str = "2026-11-07T09:30:00-08:00";
const TOKEN_PATH: &str = "/oauth2/v1/tokens/bearer";
const REQUEST_ID: &str = "billing-2026-0042";
const BASIC: &str = "Basic Zml4dHVyZS1jbGllbnQ6Zml4dHVyZS1zZWNyZXQ=";
const OTHER_REALM: &str = "9341456292345699";

fn customer() -> Value {
    json!({
        "Id": CUSTOMER, "SyncToken": "3", "DisplayName": "Amy's Bird Sanctuary",
        "CompanyName": "Amy's Bird Sanctuary", "Active": true, "Balance": 125.50,
        "PrimaryEmailAddr": {"Address": "amy@example.test"},
        "MetaData": {"CreateTime": "2025-03-01T10:00:00-08:00", "LastUpdatedTime": TIME}
    })
}

fn invoice() -> Value {
    json!({
        "Id": "145", "SyncToken": "0", "DocNumber": "INV-2026-0042",
        "TxnDate": "2026-11-07", "DueDate": "2026-12-07",
        "CustomerRef": {"value": CUSTOMER, "name": "Amy's Bird Sanctuary"},
        "Line": [{"Id": "1", "Amount": 250.00, "DetailType": "SalesItemLineDetail",
            "Description": "Habitat consultation",
            "SalesItemLineDetail": {"ItemRef": {"value": "1"}, "Qty": 2, "UnitPrice": 125.00}}],
        "TotalAmt": 250.00, "Balance": 250.00,
        "MetaData": {"CreateTime": TIME, "LastUpdatedTime": TIME}
    })
}

fn tokens(access: &str, refresh: &str) -> Reply {
    Reply::json(
        200,
        json!({
            "access_token": access, "refresh_token": refresh, "token_type": "bearer",
            "expires_in": 3600, "x_refresh_token_expires_in": 8640000
        })
        .to_string(),
    )
}

fn successful_fixture(request: &Recorded) -> Reply {
    if request.path() == TOKEN_PATH {
        return tokens("fixture-access-a", "fixture-refresh-a-rotated");
    }
    let base = format!("/v3/company/{REALM}");
    if request.path() == format!("{base}/query") {
        Reply::json(
            200,
            json!({"QueryResponse": {
            "Customer": [customer()], "startPosition": 1, "maxResults": 1
        }, "time": TIME})
            .to_string(),
        )
    } else if request.path() == format!("{base}/customer/{CUSTOMER}") {
        Reply::json(
            200,
            json!({"Customer": customer(), "time": TIME}).to_string(),
        )
    } else if request.path() == format!("{base}/invoice")
        || request.path() == format!("{base}/invoice/145")
    {
        Reply::json(200, json!({"Invoice": invoice(), "time": TIME}).to_string())
    } else {
        Reply::json(
            404,
            r#"{"Fault":{"type":"ValidationFault","Error":[{"code":"610"}]}}"#,
        )
    }
}

fn app(server: &TestServer) -> OAuthApp {
    OAuthApp::new(
        Environment::Sandbox,
        "fixture-client",
        "fixture-secret",
        "https://billing.example.test/oauth/callback",
    )
    .unwrap()
    .with_token_base_url(&server.url())
    .unwrap()
}

async fn authorized(server: &TestServer) -> (QuickBooks, Connection) {
    let http = http_client().unwrap();
    let callback = AuthorizationCallback::from_query(&format!(
        "code=fixture-code-a&state=fixture-state&realmId={REALM}"
    ))
    .unwrap();
    let connection = app(server)
        .exchange_code(&http, callback, "fixture-state")
        .await
        .unwrap();
    let qbo = QuickBooks::for_fixture(http, connection.clone(), &server.url()).unwrap();
    (qbo, connection)
}

fn query() -> CustomerQuery {
    CustomerQuery::new()
        .activity(Activity::Active)
        .order_by_display_name()
        .max_results(2)
        .unwrap()
}

fn new_invoice(customer: EntityId) -> NewInvoice {
    let line = SalesItemLine::new(Decimal::from_cents(25_000), EntityId::new("1").unwrap())
        .description("Habitat consultation")
        .quantity(Decimal::parse("2").unwrap(), Decimal::from_cents(12_500));
    NewInvoice::new(customer, line)
        .doc_number("INV-2026-0042")
        .unwrap()
}

fn accounting_wire(request: &Recorded, method: &str, path: &str, access: &str) {
    assert_eq!(request.method, method);
    assert_eq!(request.path(), format!("/v3/company/{REALM}/{path}"));
    assert_eq!(request.query_all("minorversion"), ["75"]);
    assert_eq!(request.header("authorization"), Some(access));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert!(request.host.starts_with("127.0.0.1:"));
    if method == "GET" {
        assert!(request.body.is_empty());
    }
}

#[tokio::test]
async fn query_then_fetch_customer_in_one_realm() {
    let server = TestServer::start(successful_fixture).await;
    let (qbo, connection) = authorized(&server).await;
    let response = qbo.customers().query(query()).send().await.unwrap();
    assert_eq!(response.time.as_deref(), Some(TIME));
    assert_eq!(response.page.start_position, Some(1));
    assert_eq!(response.page.max_results, Some(1));
    let selected = response.page.customers.first().unwrap();
    let fetched = qbo
        .customer(selected.id.clone())
        .read()
        .send()
        .await
        .unwrap();
    assert_eq!(fetched.customer.id, selected.id);
    assert_eq!(
        fetched.customer.display_name.as_deref(),
        Some("Amy's Bird Sanctuary")
    );
    assert_eq!(fetched.customer.balance.as_ref().unwrap().as_str(), "125.5");
    assert_eq!(fetched.customer.sync_token, "3");
    assert_eq!(
        fetched
            .customer
            .meta_data
            .unwrap()
            .last_updated_time
            .as_deref(),
        Some(TIME)
    );
    assert_eq!(connection.realm().as_str(), REALM);
    assert_eq!(
        connection.refresh_token().expose_secret(),
        "fixture-refresh-a-rotated"
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path(), TOKEN_PATH);
    assert_eq!(requests[0].header("authorization"), Some(BASIC));
    assert_eq!(requests[0].header("accept"), Some("application/json"));
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        requests[0].form_pairs(),
        [
            ("grant_type".into(), "authorization_code".into()),
            ("code".into(), "fixture-code-a".into()),
            (
                "redirect_uri".into(),
                "https://billing.example.test/oauth/callback".into()
            ),
        ]
    );
    accounting_wire(&requests[1], "GET", "query", "Bearer fixture-access-a");
    assert_eq!(
        requests[1].query_all("query"),
        [
            "SELECT * FROM Customer WHERE Active = true ORDERBY DisplayName STARTPOSITION 1 MAXRESULTS 2"
        ]
    );
    accounting_wire(
        &requests[2],
        "GET",
        "customer/58",
        "Bearer fixture-access-a",
    );
    assert_eq!(server.shutdown().await.open_after, 0);

    pagination_completion_and_partial_results().await;
    rejected_refresh_recovers_without_crossing_realms().await;
    read_failures_keep_status_metadata_and_secrets().await;
    reads_refresh_at_most_once().await;
}

#[tokio::test]
async fn create_then_retrieve_sandbox_invoice() {
    let server = TestServer::start(successful_fixture).await;
    let (qbo, _) = authorized(&server).await;
    let selected = qbo
        .customer(EntityId::new(CUSTOMER).unwrap())
        .read()
        .send()
        .await
        .unwrap();
    let created = qbo
        .invoices()
        .create(new_invoice(selected.customer.id.clone()))
        .set_request_id(RequestId::new(REQUEST_ID).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(
        created.invoice.customer_ref.value,
        selected.customer.id.as_str()
    );
    assert_eq!(
        created.invoice.total_amt.as_ref().unwrap().as_str(),
        "250.0"
    );
    assert_eq!(created.time.as_deref(), Some(TIME));
    let fetched = qbo
        .invoice(created.invoice.id.clone())
        .read()
        .send()
        .await
        .unwrap();
    assert_eq!(fetched.invoice.id, created.invoice.id);
    assert_eq!(fetched.invoice.doc_number.as_deref(), Some("INV-2026-0042"));
    assert_eq!(fetched.invoice.sync_token, "0");
    assert_eq!(
        fetched.invoice.meta_data.unwrap().create_time.as_deref(),
        Some(TIME)
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    accounting_wire(&requests[2], "POST", "invoice", "Bearer fixture-access-a");
    assert_eq!(requests[2].query_all("requestid"), [REQUEST_ID]);
    assert_eq!(requests[2].header("content-type"), Some("application/json"));
    let body: Value = serde_json::from_slice(&requests[2].body).unwrap();
    assert_eq!(body["CustomerRef"]["value"], CUSTOMER);
    assert_eq!(body["DocNumber"], "INV-2026-0042");
    assert_eq!(body["Line"][0]["Amount"], json!(250.00));
    assert_eq!(body["Line"][0]["DetailType"], "SalesItemLineDetail");
    assert_eq!(
        body["Line"][0]["SalesItemLineDetail"]["ItemRef"]["value"],
        "1"
    );
    assert_eq!(body["Line"][0]["SalesItemLineDetail"]["Qty"], 2);
    accounting_wire(
        &requests[3],
        "GET",
        "invoice/145",
        "Bearer fixture-access-a",
    );
    assert_eq!(server.shutdown().await.open_after, 0);

    creates_are_never_automatically_replayed().await;
    failed_retrieval_keeps_the_created_invoice().await;
}

fn fault(status: u16) -> Reply {
    Reply::json(
        status,
        json!({"Fault": {"type": "ValidationFault", "Error": [{
        "code": "610", "element": "CustomerRef",
        "Message": "fixture-private-provider-message",
        "Detail": "fixture-private-provider-detail"
    }]}, "time": TIME})
        .to_string(),
    )
    .header("intuit_tid", "fixture-private-trace-id")
    .header("retry-after", "7")
}

fn redacted(error: &Error) {
    let mut source: Option<&dyn std::error::Error> = Some(error);
    while let Some(current) = source {
        let output = format!("{current} {current:?}");
        for secret in [
            "fixture-secret",
            "fixture-code-a",
            "fixture-access-a",
            "fixture-access-b",
            "fixture-refresh",
            "fixture-private",
            "Amy's",
            "http://127.0.0.1",
        ] {
            assert!(
                !output.contains(secret),
                "error formatting exposed {secret}"
            );
        }
        source = current.source();
    }
}

fn provider_fault(error: &Error, status: u16, operation: &str) {
    let provider = error.provider().unwrap();
    assert_eq!(error.status().unwrap().as_u16(), status);
    assert_eq!(provider.status().as_u16(), status);
    assert_eq!(provider.intuit_tid(), Some("fixture-private-trace-id"));
    assert_eq!(provider.meta().status().as_u16(), status);
    assert_eq!(provider.meta().operation().unwrap().name(), operation);
    assert_eq!(provider.meta().headers()["retry-after"], "7");
    assert!(!format!("{:?}", provider.meta()).contains("fixture-private"));
    let fault = provider.fault().unwrap();
    assert_eq!(fault.kind(), Some("ValidationFault"));
    assert_eq!(fault.code(), Some("610"));
    assert_eq!(fault.errors()[0].element(), Some("CustomerRef"));
    assert_eq!(
        fault.errors()[0].message_unredacted(),
        Some("fixture-private-provider-message")
    );
    assert_eq!(
        fault.errors()[0].detail_unredacted(),
        Some("fixture-private-provider-detail")
    );
    redacted(error);
}

async fn pagination_completion_and_partial_results() {
    // Normal exhaustion, a cap, a late provider fault, and a late decode error.
    for mode in 0..4 {
        let server = TestServer::start(move |request: &Recorded| {
            if request.path().ends_with("/query") {
                let statement = request.query_all("query").pop().unwrap();
                if statement.contains("STARTPOSITION 1 ") {
                    let mut second = customer();
                    second["Id"] = json!("59");
                    second["DisplayName"] = json!("Bill's Windsurf Shop");
                    return Reply::json(
                        200,
                        json!({"QueryResponse": {
                        "Customer": [customer(), second], "startPosition": 1, "maxResults": 2
                    }, "time": TIME})
                        .to_string(),
                    );
                }
                return match mode {
                    2 => fault(429),
                    3 => Reply::json(200, r#"{"QueryResponse":"fixture-private-malformed"}"#)
                        .header("intuit_tid", "fixture-private-trace-id"),
                    _ => Reply::json(200, json!({"QueryResponse": {}, "time": TIME}).to_string()),
                };
            }
            successful_fixture(request)
        })
        .await;
        let (qbo, _) = authorized(&server).await;
        let pages = customer_pages(&qbo, query());
        assert_eq!(
            server.requests().len(),
            1,
            "constructing traversal must be lazy"
        );
        if mode == 1 || mode == 2 {
            let pages = if mode == 1 {
                pages.max_pages(NonZeroUsize::new(1).unwrap())
            } else {
                pages
            };
            let partial = pages.items().collect_complete().await.unwrap_err();
            assert_eq!(
                partial
                    .collected
                    .iter()
                    .map(|c| c.id.as_str())
                    .collect::<Vec<_>>(),
                ["58", "59"]
            );
            assert_eq!(
                partial.error.resume(),
                Some(&Resume::At(NonZeroU32::new(3).unwrap()))
            );
            assert_eq!(partial.error.progress().succeeded, 1);
            if mode == 1 {
                assert!(matches!(partial.error, PagesError::Bounded { .. }));
                assert_eq!(partial.error.outcome(), Outcome::Bounded);
                assert_eq!(partial.error.progress().admitted, 1);
            } else {
                assert_eq!(partial.error.outcome(), Outcome::Failed);
                assert_eq!(partial.error.progress().admitted, 2);
                assert_eq!(partial.error.progress().failed, 1);
                provider_fault(
                    partial.error.provider_error().unwrap(),
                    429,
                    "QuickBooks.customers.query",
                );
            }
        } else {
            let mut pages = pages;
            let first = pages.next_page().await.unwrap().unwrap();
            assert_eq!(first.items.len(), 2);
            assert_eq!(
                pages.checkpoint(),
                Some(&Resume::At(NonZeroU32::new(3).unwrap()))
            );
            if mode == 0 {
                assert!(pages.next_page().await.unwrap().unwrap().items.is_empty());
                assert!(pages.next_page().await.unwrap().is_none());
                assert!(pages.next_page().await.unwrap().is_none());
                let report = pages.finish();
                assert_eq!(report.outcome, Outcome::Exhausted);
                assert!(report.outcome.is_success());
                assert_eq!(report.progress.admitted, 2);
                assert_eq!(report.progress.succeeded, 2);
                assert!(report.resume.is_none());
            } else {
                let error = pages.next_page().await.unwrap_err();
                let sdk_error = error.provider_error().unwrap();
                let Error::Concord(decode) = sdk_error else {
                    panic!("expected decode error")
                };
                assert_eq!(decode.kind(), ErrorKind::Decode);
                assert_eq!(decode.head().unwrap().status().as_u16(), 200);
                assert_eq!(
                    decode.head_meta().unwrap().headers()["intuit_tid"],
                    "fixture-private-trace-id"
                );
                redacted(sdk_error);
                assert_eq!(
                    first.items[0].id.as_str(),
                    CUSTOMER,
                    "the caller keeps its prefix"
                );
                assert!(matches!(
                    pages.next_page().await,
                    Err(PagesError::Ended {
                        outcome: Outcome::Failed,
                        ..
                    })
                ));
                let report = pages.finish();
                assert_eq!(report.outcome, Outcome::Failed);
                assert!(!report.outcome.is_success());
                assert_eq!(report.progress.failed, 1);
                assert_eq!(report.failed, Some(Resume::At(NonZeroU32::new(3).unwrap())));
            }
        }
        let requests = server.requests();
        assert_eq!(requests.len(), if mode == 1 { 2 } else { 3 });
        for (index, request) in requests.iter().skip(1).enumerate() {
            accounting_wire(request, "GET", "query", "Bearer fixture-access-a");
            assert_eq!(
                request.query_all("query"),
                [format!(
                    "SELECT * FROM Customer WHERE Active = true ORDERBY DisplayName STARTPOSITION {} MAXRESULTS 2",
                    1 + index * 2
                )]
            );
        }
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

async fn rejected_refresh_recovers_without_crossing_realms() {
    let server = TestServer::start(|request: &Recorded| {
        if request.path() == TOKEN_PATH {
            let token = request.form_pairs().into_iter().find(|(key, _)| key == "refresh_token").unwrap().1;
            return match token.as_str() {
                "fixture-refresh-expired-a" => Reply::json(400,
                    r#"{"error":"invalid_grant","error_description":"fixture-private-expired-token"}"#),
                "fixture-refresh-b" => tokens("fixture-access-b", "fixture-refresh-b-rotated"),
                "fixture-refresh-recovered-a" => tokens("fixture-access-a-recovered", "fixture-refresh-a-latest"),
                _ => Reply::json(400, r#"{"error":"invalid_grant"}"#),
            };
        }
        // Deliberately allow both realms here; assertions below must detect a wrong route/token.
        if request.path().ends_with("/query") {
            return Reply::json(200, json!({"QueryResponse": {"Customer": [customer()]}, "time": TIME}).to_string());
        }
        successful_fixture(request)
    }).await;
    let oauth = app(&server);
    let a = oauth.connection(
        RealmId::new(REALM).unwrap(),
        RefreshToken::new("fixture-refresh-expired-a").unwrap(),
    );
    let b = oauth.connection(
        RealmId::new(OTHER_REALM).unwrap(),
        RefreshToken::new("fixture-refresh-b").unwrap(),
    );
    let qbo = QuickBooks::for_fixture(http_client().unwrap(), a.clone(), &server.url()).unwrap();
    let clone = qbo.clone();
    let other = QuickBooks::for_fixture(http_client().unwrap(), b.clone(), &server.url()).unwrap();
    let error = qbo.customers().query(query()).send().await.unwrap_err();
    let auth = error.authorization().unwrap();
    assert_eq!(auth.kind(), AuthorizationKind::GrantRejected);
    assert_eq!(auth.realm().as_str(), REALM);
    assert_eq!(auth.token_status().unwrap().as_u16(), 400);
    assert_eq!(auth.oauth_error(), Some("invalid_grant"));
    assert!(auth.needs_reauthorization());
    assert!(a.needs_reauthorization());
    redacted(&error);
    let repeated = clone
        .customer(EntityId::new(CUSTOMER).unwrap())
        .read()
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        repeated.authorization().unwrap().kind(),
        AuthorizationKind::GrantRejected
    );
    assert_eq!(
        server.requests().len(),
        1,
        "no API request or repeated token request after rejection"
    );

    assert_eq!(
        other
            .customers()
            .query(query())
            .send()
            .await
            .unwrap()
            .page
            .customers
            .len(),
        1
    );
    assert!(!b.needs_reauthorization());
    assert_eq!(
        b.refresh_token().expose_secret(),
        "fixture-refresh-b-rotated"
    );
    let wrong_realm = AuthorizationCallback::from_query(&format!(
        "code=fixture-code-b&state=fixture-state&realmId={OTHER_REALM}"
    ))
    .unwrap();
    let mismatch = a
        .reauthorize(&http_client().unwrap(), wrong_realm, "fixture-state")
        .await
        .unwrap_err();
    assert!(matches!(
        mismatch,
        Error::Callback(CallbackError::RealmMismatch)
    ));
    let wrong_state = AuthorizationCallback::from_query(&format!(
        "code=fixture-code-a&state=wrong-state&realmId={REALM}"
    ))
    .unwrap();
    assert!(matches!(
        oauth
            .exchange_code(&http_client().unwrap(), wrong_state, "fixture-state")
            .await,
        Err(Error::Callback(CallbackError::StateMismatch))
    ));
    assert_eq!(
        server.requests().len(),
        3,
        "invalid callbacks must be unsent"
    );
    assert!(a.needs_reauthorization());
    a.replace_refresh_token(RefreshToken::new("fixture-refresh-recovered-a").unwrap());
    assert!(!a.needs_reauthorization());
    assert_eq!(
        clone
            .customers()
            .query(query())
            .send()
            .await
            .unwrap()
            .page
            .customers
            .len(),
        1
    );
    qbo.customer(EntityId::new(CUSTOMER).unwrap())
        .read()
        .send()
        .await
        .unwrap();
    assert_eq!(
        a.refresh_token().expose_secret(),
        "fixture-refresh-a-latest"
    );
    assert!(a.refresh_token_expires_at().is_some());
    assert_eq!(
        b.refresh_token().expose_secret(),
        "fixture-refresh-b-rotated"
    );
    let debug = format!("{a:?} {qbo:?} {:?} {:?}", a.refresh_token(), query());
    assert!(!debug.contains("fixture-refresh"));
    assert!(!debug.contains("fixture-access"));
    let private_query = CustomerQuery::new()
        .display_name("Amy's Bird Sanctuary")
        .unwrap();
    assert!(!format!("{private_query:?}").contains("Amy's"));

    let requests = server.requests();
    assert_eq!(requests.len(), 6);
    for (index, token) in [
        (0, "fixture-refresh-expired-a"),
        (1, "fixture-refresh-b"),
        (3, "fixture-refresh-recovered-a"),
    ] {
        let request = &requests[index];
        assert_eq!(request.method, "POST");
        assert_eq!(request.path(), TOKEN_PATH);
        assert_eq!(request.header("authorization"), Some(BASIC));
        assert_eq!(request.header("accept"), Some("application/json"));
        assert_eq!(
            request.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(
            request.form_pairs(),
            [
                ("grant_type".into(), "refresh_token".into()),
                ("refresh_token".into(), token.into())
            ]
        );
    }
    assert_eq!(
        requests[2].path(),
        format!("/v3/company/{OTHER_REALM}/query")
    );
    assert_eq!(
        requests[2].header("authorization"),
        Some("Bearer fixture-access-b")
    );
    accounting_wire(
        &requests[4],
        "GET",
        "query",
        "Bearer fixture-access-a-recovered",
    );
    accounting_wire(
        &requests[5],
        "GET",
        "customer/58",
        "Bearer fixture-access-a-recovered",
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// Maintenance consumer: create an invoice, record a partial payment, then read its balance.
#[tokio::test]
async fn record_a_partial_payment_then_read_the_invoice_balance() {
    use concord_sdk_quickbooks_online::NewPayment;
    use std::sync::atomic::{AtomicBool, Ordering};

    let paid = AtomicBool::new(false);
    let server = TestServer::start(move |request: &Recorded| {
        if request.path() == format!("/v3/company/{REALM}/payment") {
            assert_eq!(request.method, "POST");
            assert_eq!(request.query_all("requestid"), ["receipt-2026-0042"]);
            assert_eq!(request.body_text(),
                r#"{"CustomerRef":{"value":"58"},"TotalAmt":25.1300,"Line":[{"Amount":25.1300,"LinkedTxn":[{"TxnId":"145","TxnType":"Invoice"}]}]}"#);
            assert!(!paid.swap(true, Ordering::SeqCst), "payment sent once");
            Reply::json(200, include_str!("../fixtures/payment-created.json"))
        } else if request.path() == format!("/v3/company/{REALM}/invoice/145")
            && paid.load(Ordering::SeqCst)
        {
            Reply::json(200, include_str!("../fixtures/invoice-after-payment.json"))
        } else {
            successful_fixture(request)
        }
    }).await;
    let (qbo, connection) = authorized(&server).await;
    let selected = qbo
        .customer(EntityId::new(CUSTOMER).unwrap())
        .read()
        .send()
        .await
        .unwrap()
        .customer;
    let created = qbo
        .invoices()
        .create(new_invoice(selected.id.clone()))
        .send()
        .await
        .unwrap()
        .invoice;
    assert_eq!(created.balance.as_ref().unwrap().as_str(), "250.0");

    let payment = NewPayment::new(
        selected.id.clone(),
        created.id.clone(),
        Decimal::parse("25.1300").unwrap(),
    )
    .unwrap();
    let recorded = qbo
        .payments()
        .create(payment)
        .set_request_id(RequestId::new("receipt-2026-0042").unwrap())
        .send()
        .await
        .unwrap()
        .payment;
    assert_eq!(recorded.id.as_str(), "146");
    assert_eq!(recorded.customer_ref.value, selected.id.as_str());
    assert_eq!(recorded.total_amt.as_str(), "25.1300");
    assert_eq!(recorded.unapplied_amt.as_ref().unwrap().as_str(), "0.00");
    assert_eq!(recorded.line.len(), 1);
    assert_eq!(recorded.line[0].amount.as_str(), "25.1300");
    assert_eq!(recorded.line[0].linked_txn.len(), 1);
    assert_eq!(recorded.line[0].linked_txn[0].txn_id, created.id);
    assert_eq!(recorded.line[0].linked_txn[0].txn_type, "Invoice");

    let fetched = qbo
        .invoice(created.id.clone())
        .read()
        .send()
        .await
        .unwrap()
        .invoice;
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.customer_ref.value, selected.id.as_str());
    assert_eq!(fetched.total_amt.as_ref().unwrap().as_str(), "250.00");
    assert_eq!(fetched.balance.as_ref().unwrap().as_str(), "224.8700");
    assert_eq!(connection.realm().as_str(), REALM);
    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    accounting_wire(&requests[3], "POST", "payment", "Bearer fixture-access-a");
    assert_eq!(requests[3].header("content-type"), Some("application/json"));
    assert_eq!(requests[3].query_all("minorversion"), ["75"]);
    accounting_wire(
        &requests[4],
        "GET",
        "invoice/145",
        "Bearer fixture-access-a",
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

async fn read_failures_keep_status_metadata_and_secrets() {
    for mode in 0..5 {
        let server = TestServer::start(move |request: &Recorded| {
            if request.path() == TOKEN_PATH {
                return successful_fixture(request);
            }
            match mode {
                0 => fault(400),
                1 => fault(200),
                2 => Reply::text(429, "text/html", "fixture-private-proxy-error")
                    .header("intuit_tid", "fixture-private-trace-id"),
                3 => Reply::json(200, r#"{"Customer":"fixture-private-malformed"}"#)
                    .header("intuit_tid", "fixture-private-trace-id"),
                _ => Reply::text(200, "text/html", "fixture-private-wrong-media")
                    .header("intuit_tid", "fixture-private-trace-id"),
            }
        })
        .await;
        let (qbo, _) = authorized(&server).await;
        let error = qbo
            .customer(EntityId::new(CUSTOMER).unwrap())
            .read()
            .send()
            .await
            .unwrap_err();
        redacted(&error);
        match mode {
            0 => provider_fault(&error, 400, "QuickBooks.customer.read"),
            1 => provider_fault(&error, 200, "QuickBooks.customer.read"),
            2 => {
                let provider = error.provider().unwrap();
                assert_eq!(provider.status().as_u16(), 429);
                assert_eq!(provider.intuit_tid(), Some("fixture-private-trace-id"));
                assert!(provider.fault().is_err());
            }
            _ => {
                let Error::Concord(ref underlying) = error else {
                    panic!("expected Concord error")
                };
                assert_eq!(
                    underlying.kind(),
                    if mode == 3 {
                        ErrorKind::Decode
                    } else {
                        ErrorKind::Metadata
                    }
                );
                assert_eq!(underlying.head().unwrap().status().as_u16(), 200);
                assert_eq!(
                    underlying.head_meta().unwrap().headers()["intuit_tid"],
                    "fixture-private-trace-id"
                );
            }
        }
        assert_eq!(server.requests().len(), 2);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

async fn reads_refresh_at_most_once() {
    for refuse_fresh_token in [false, true] {
        let server = TestServer::start(move |request: &Recorded| {
            if request.path() == TOKEN_PATH {
                if request
                    .form_pairs()
                    .iter()
                    .any(|(key, value)| key == "grant_type" && value == "refresh_token")
                {
                    return tokens("fixture-access-a-fresh", "fixture-refresh-a-latest");
                }
                return successful_fixture(request);
            }
            if refuse_fresh_token
                || request.header("authorization") == Some("Bearer fixture-access-a")
            {
                return fault(401);
            }
            successful_fixture(request)
        })
        .await;
        let (qbo, connection) = authorized(&server).await;
        let result = qbo
            .customer(EntityId::new(CUSTOMER).unwrap())
            .read()
            .send()
            .await;
        if refuse_fresh_token {
            let error = result.unwrap_err();
            let auth = error.authorization().unwrap();
            assert_eq!(auth.kind(), AuthorizationKind::AccessTokenRejected);
            assert_eq!(auth.realm().as_str(), REALM);
            assert!(!auth.needs_reauthorization());
            assert_eq!(auth.response().unwrap().status().as_u16(), 401);
            assert_eq!(
                auth.response().unwrap().intuit_tid(),
                Some("fixture-private-trace-id")
            );
            redacted(&error);
        } else {
            assert_eq!(result.unwrap().customer.id.as_str(), CUSTOMER);
        }
        assert_eq!(
            connection.refresh_token().expose_secret(),
            "fixture-refresh-a-latest"
        );
        let requests = server.requests();
        assert_eq!(
            requests.len(),
            4,
            "one code exchange, two reads and one refresh"
        );
        accounting_wire(
            &requests[1],
            "GET",
            "customer/58",
            "Bearer fixture-access-a",
        );
        assert_eq!(requests[2].path(), TOKEN_PATH);
        assert_eq!(
            requests[2].form_pairs(),
            [
                ("grant_type".into(), "refresh_token".into()),
                ("refresh_token".into(), "fixture-refresh-a-rotated".into())
            ]
        );
        accounting_wire(
            &requests[3],
            "GET",
            "customer/58",
            "Bearer fixture-access-a-fresh",
        );
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

async fn creates_are_never_automatically_replayed() {
    for mode in 0..6 {
        let server = TestServer::start(move |request: &Recorded| {
            if request.path() == TOKEN_PATH {
                return successful_fixture(request);
            }
            match mode {
                0 => fault(400),
                1 => fault(200),
                2 => fault(500),
                3 => Reply::json(200, r#"{"Invoice":"fixture-private-malformed"}"#),
                4 => fault(401),
                _ => Reply::hang_up(),
            }
        })
        .await;
        let (qbo, connection) = authorized(&server).await;
        let before = server.requests().len();
        assert!(matches!(
            CustomerQuery::new().max_results(1001),
            Err(Error::InvalidInput { .. })
        ));
        assert!(matches!(
            EntityId::new("customer-58"),
            Err(Error::InvalidInput { .. })
        ));
        assert!(matches!(
            RequestId::new("not a request id"),
            Err(Error::InvalidInput { .. })
        ));
        assert!(matches!(
            new_invoice(EntityId::new(CUSTOMER).unwrap())
                .doc_number("INV-DOCUMENT-NUMBER-TOO-LONG"),
            Err(Error::InvalidInput { .. })
        ));
        assert_eq!(
            server.requests().len(),
            before,
            "validation must not dispatch"
        );
        let error = qbo
            .invoices()
            .create(new_invoice(EntityId::new(CUSTOMER).unwrap()))
            .set_request_id(RequestId::new(REQUEST_ID).unwrap())
            .send()
            .await
            .unwrap_err();
        redacted(&error);
        match mode {
            0 => provider_fault(&error, 400, "QuickBooks.invoices.create"),
            1 => provider_fault(&error, 200, "QuickBooks.invoices.create"),
            4 => {
                let auth = error.authorization().unwrap();
                assert_eq!(auth.kind(), AuthorizationKind::AccessTokenRejected);
                assert_eq!(auth.realm().as_str(), REALM);
                assert_eq!(auth.response().unwrap().status().as_u16(), 401);
                assert!(!connection.needs_reauthorization());
            }
            _ => {
                let uncertain = error.uncertain().unwrap();
                assert_eq!(uncertain.request_id().unwrap().as_str(), REQUEST_ID);
                assert_eq!(uncertain.realm().as_str(), REALM);
                assert_eq!(uncertain.operation(), "QuickBooks.invoices.create");
                match (mode, uncertain.cause()) {
                    (2, UncertainCause::ServerError(provider)) => {
                        assert_eq!(provider.status().as_u16(), 500);
                        assert_eq!(provider.intuit_tid(), Some("fixture-private-trace-id"));
                        assert_eq!(error.status().unwrap().as_u16(), 500);
                    }
                    (3, UncertainCause::UnreadableSuccess(decode)) => {
                        assert_eq!(decode.kind(), ErrorKind::Decode)
                    }
                    (5, UncertainCause::NoResponse(transport)) => {
                        assert_eq!(transport.kind(), ErrorKind::Transport)
                    }
                    _ => panic!("unexpected uncertain-write cause"),
                }
            }
        }
        let requests = server.requests();
        assert_eq!(
            requests.len(),
            2,
            "the create is sent exactly once, including after 401"
        );
        accounting_wire(&requests[1], "POST", "invoice", "Bearer fixture-access-a");
        assert_eq!(requests[1].query_all("requestid"), [REQUEST_ID]);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

async fn failed_retrieval_keeps_the_created_invoice() {
    let server = TestServer::start(|request: &Recorded| {
        if request.path().ends_with("/invoice/145") {
            return Reply::json(200, r#"{"Invoice":"fixture-private-malformed"}"#)
                .header("intuit_tid", "fixture-private-trace-id");
        }
        successful_fixture(request)
    })
    .await;
    let (qbo, _) = authorized(&server).await;
    let created = qbo
        .invoices()
        .create(new_invoice(EntityId::new(CUSTOMER).unwrap()))
        .set_request_id(RequestId::new(REQUEST_ID).unwrap())
        .send()
        .await
        .unwrap();
    let error = qbo
        .invoice(created.invoice.id.clone())
        .read()
        .send()
        .await
        .unwrap_err();
    let Error::Concord(ref decode) = error else {
        panic!("expected invoice-read decode failure")
    };
    assert_eq!(decode.kind(), ErrorKind::Decode);
    assert_eq!(decode.endpoint(), Some("QuickBooks.invoice.read"));
    assert_eq!(decode.head().unwrap().status().as_u16(), 200);
    assert_eq!(
        decode.head_meta().unwrap().headers()["intuit_tid"],
        "fixture-private-trace-id"
    );
    redacted(&error);
    assert_eq!(created.invoice.id.as_str(), "145");
    assert_eq!(created.invoice.customer_ref.value, CUSTOMER);
    assert_eq!(created.time.as_deref(), Some(TIME));
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.path().ends_with("/invoice"))
            .count(),
        1
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}
