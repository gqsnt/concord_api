//! Wire-level request shapes, every documented error class, malformed / truncated / oversized /
//! wrong-media bodies, and redaction of secrets in `Debug` and `Display`.

mod support;

use std::time::Duration;

use concord_example_testserver::{Body, Recorded, Reply, TestServer, generated_records};
use concord_sdk_quickbooks_online::concord::bytes::Bytes;
use concord_sdk_quickbooks_online::concord::{ErrorKind, reqwest};
use concord_sdk_quickbooks_online::{
    Activity, AuthorizationCallback, AuthorizationKind, CallbackError, Connection, CustomerQuery,
    Decimal, ENTITY_BODY_LIMIT, ERROR_BODY_LIMIT, EntityId, Environment, Error, NewInvoice,
    OAuthApp, QuickBooks, RealmId, RefreshToken, RequestId, SalesItemLine, UncertainCause,
};
use serde_json::Value;
use support::*;

/// The access token in `token-issued.json`.
const ACCESS: &str = "synthetic-access-token-2";
const STORED_REFRESH: &str = "synthetic-refresh-stored-0";

/// A server whose token endpoint issues `token-issued.json` and whose Accounting API answers
/// with `accounting`.
async fn serve(accounting: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> TestServer {
    serve_tokens(|_| json_fixture(200, "token-issued.json"), accounting).await
}

async fn serve_tokens(
    token: impl Fn(&Recorded) -> Reply + Send + Sync + 'static,
    accounting: impl Fn(&Recorded) -> Reply + Send + Sync + 'static,
) -> TestServer {
    TestServer::start(move |r| {
        if r.path() == TOKEN_PATH {
            token(r)
        } else {
            accounting(r)
        }
    })
    .await
}

fn client(server: &TestServer) -> (QuickBooks, Connection) {
    let connection = connection(server, REALM_A, STORED_REFRESH);
    (qbo(server, connection.clone()), connection)
}

fn customer_29() -> EntityId {
    EntityId::new("29").unwrap()
}

fn invoice() -> NewInvoice {
    NewInvoice::new(
        customer_29(),
        SalesItemLine::new(Decimal::from_cents(10000), EntityId::new("1").unwrap()),
    )
}

async fn read_error(reply: impl Fn() -> Reply + Send + Sync + 'static) -> Error {
    let server = serve(move |_| reply()).await;
    let (qbo, _) = client(&server);
    qbo.customer(customer_29()).read().send().await.unwrap_err()
}

async fn create_error(reply: impl Fn() -> Reply + Send + Sync + 'static) -> (Error, TestServer) {
    let server = serve(move |_| reply()).await;
    let (qbo, _) = client(&server);
    let error = qbo.invoices().create(invoice()).send().await.unwrap_err();
    (error, server)
}

fn concord_kind(error: &Error) -> ErrorKind {
    match error {
        Error::Concord(e) => e.kind(),
        other => panic!("not a Concord error: {other:?}"),
    }
}

// ---------------------------------------------------------------- request shapes

#[tokio::test]
async fn the_refresh_and_the_query_go_on_the_wire_as_documented() {
    let server = serve(|_| json_fixture(200, "customer-query-page1.json")).await;
    let (qbo, _) = client(&server);
    let query = CustomerQuery::new()
        .activity(Activity::Any)
        .display_name("Amy's")
        .unwrap();
    let page = qbo.customers().query(query).send().await.unwrap().page;
    assert_eq!(page.customers.len(), 2);
    assert_eq!(page.start_position, Some(1));

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let token = &requests[0];
    assert_eq!(token.method, "POST");
    assert_eq!(token.target, TOKEN_PATH);
    assert_eq!(token.header("authorization"), Some(BASIC));
    assert_eq!(
        token.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(token.header("accept"), Some("application/json"));
    assert_eq!(
        token.form_pairs(),
        [
            ("grant_type".to_owned(), "refresh_token".to_owned()),
            ("refresh_token".to_owned(), STORED_REFRESH.to_owned()),
        ]
    );

    let api = &requests[1];
    assert_eq!(api.method, "GET");
    // application/x-www-form-urlencoded: space is `+`; `'` `=` `(` `,` `)` `\` are escaped.
    assert_eq!(
        api.target,
        format!(
            "/v3/company/{REALM_A}/query?minorversion=75&query=SELECT+*+FROM+Customer+WHERE+\
             Active+IN+%28true%2C+false%29+AND+DisplayName+%3D+%27Amy%5C%27s%27+\
             STARTPOSITION+1+MAXRESULTS+100"
        )
    );
    assert_eq!(
        api.header("authorization"),
        Some(format!("Bearer {ACCESS}").as_str())
    );
    assert_eq!(api.header("accept"), Some("application/json"));
    assert_eq!(api.header("content-type"), None);
    assert!(api.body.is_empty());
}

#[tokio::test]
async fn reads_target_one_entity_and_keep_decimals_exact() {
    let server = serve(|r| {
        if r.path().ends_with("/invoice/130") {
            json_fixture(200, "invoice-read.json")
        } else {
            json_fixture(200, "customer-read.json")
        }
    })
    .await;
    let (qbo, _) = client(&server);
    let customer = qbo
        .customer(customer_29())
        .read()
        .send()
        .await
        .unwrap()
        .customer;
    assert_eq!(
        customer.display_name.as_deref(),
        Some("Synthetic Consulting")
    );
    assert_eq!(
        customer
            .primary_email_addr
            .and_then(|e| e.address)
            .as_deref(),
        Some("consulting@example.test")
    );
    let invoice = qbo
        .invoice(EntityId::new("130").unwrap())
        .read()
        .send()
        .await
        .unwrap()
        .invoice;
    assert_eq!(
        invoice.total_amt.as_ref().map(Decimal::as_str),
        Some("362.07")
    );
    let detail = invoice.line[1].sales_item_line_detail.as_ref().unwrap();
    assert_eq!(detail.qty.as_ref().map(Decimal::as_str), Some("5"));
    assert_eq!(detail.unit_price.as_ref().map(Decimal::as_str), Some("9.5"));
    assert_eq!(invoice.line[2].detail_type, "SubTotalLineDetail");

    let targets: Vec<_> = api_requests(&server)
        .into_iter()
        .map(|r| r.target)
        .collect();
    assert_eq!(
        targets,
        [
            format!("/v3/company/{REALM_A}/customer/29?minorversion=75"),
            format!("/v3/company/{REALM_A}/invoice/130?minorversion=75"),
        ]
    );
}

#[tokio::test]
async fn a_create_sends_the_documented_minimum_body_with_its_request_id() {
    let server = serve(|_| json_fixture(200, "invoice-created.json")).await;
    let (qbo, _) = client(&server);
    let id = RequestId::new("order-2026-0042").unwrap();
    let created = qbo
        .invoices()
        .create(invoice())
        .set_request_id(id)
        .send()
        .await
        .unwrap();
    assert_eq!(created.invoice.id.as_str(), "238");

    let create = api_requests(&server).remove(0);
    assert_eq!(create.method, "POST");
    assert_eq!(
        create.target,
        format!("/v3/company/{REALM_A}/invoice?minorversion=75&requestid=order-2026-0042")
    );
    assert_eq!(create.header("content-type"), Some("application/json"));
    assert_eq!(create.header("accept"), Some("application/json"));
    let body: Value = serde_json::from_slice(&create.body).unwrap();
    assert_eq!(body, fixture_json("invoice-create-request.json"));
}

#[test]
fn optional_invoice_fields_have_their_documented_names() {
    let line = SalesItemLine::new(Decimal::parse("47.5").unwrap(), EntityId::new("3").unwrap())
        .description("Concrete")
        .quantity(Decimal::parse("5").unwrap(), Decimal::parse("9.5").unwrap());
    let invoice = NewInvoice::new(customer_29(), line)
        .doc_number("SYN-0001")
        .unwrap();
    let qbo = QuickBooks::connect(
        reqwest::Client::new(),
        app_offline().connection(realm(REALM_A), RefreshToken::new("r").unwrap()),
    );
    let prepared = qbo.invoices().create(invoice).prepare().unwrap();
    let body = prepared
        .request()
        .body()
        .and_then(|b| b.as_bytes())
        .unwrap();
    assert_eq!(
        std::str::from_utf8(body).unwrap(),
        r#"{"Line":[{"DetailType":"SalesItemLineDetail","Amount":47.5,"Description":"Concrete","SalesItemLineDetail":{"ItemRef":{"value":"3"},"Qty":5,"UnitPrice":9.5}}],"CustomerRef":{"value":"29"},"DocNumber":"SYN-0001"}"#
    );
}

fn app_offline() -> OAuthApp {
    OAuthApp::new(Environment::Sandbox, CLIENT_ID, CLIENT_SECRET, REDIRECT).unwrap()
}

#[test]
fn each_environment_has_its_documented_base_and_prepare_adds_no_credential() {
    for (environment, host) in [
        (Environment::Sandbox, "sandbox-quickbooks.api.intuit.com"),
        (Environment::Production, "quickbooks.api.intuit.com"),
    ] {
        let app = OAuthApp::new(environment, CLIENT_ID, CLIENT_SECRET, REDIRECT).unwrap();
        let qbo = QuickBooks::connect(
            reqwest::Client::new(),
            app.connection(realm(REALM_A), RefreshToken::new("r").unwrap()),
        );
        let prepared = qbo.customer(customer_29()).read().prepare().unwrap();
        assert_eq!(
            prepared.request().url().as_str(),
            format!("https://{host}/v3/company/{REALM_A}/customer/29?minorversion=75")
        );
        assert!(prepared.request().headers().get("authorization").is_none());
    }
}

#[test]
fn the_authorization_url_carries_the_documented_parameters() {
    let url = app_offline().authorization_url("synthetic-state").unwrap();
    assert_eq!(url.scheme(), "https");
    assert_eq!(url.host_str(), Some("appcenter.intuit.com"));
    assert_eq!(url.path(), "/connect/oauth2");
    let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    let expect = |k: &str, v: &str| (k.to_owned(), v.to_owned());
    assert_eq!(
        pairs,
        [
            expect("client_id", CLIENT_ID),
            expect("response_type", "code"),
            expect("scope", "com.intuit.quickbooks.accounting"),
            expect("redirect_uri", REDIRECT),
            expect("state", "synthetic-state"),
        ]
    );
    assert!(app_offline().authorization_url("").is_err());
}

#[tokio::test]
async fn the_code_exchange_is_one_documented_form_request() {
    let server = serve_tokens(
        |_| json_fixture(200, "token-issued.json"),
        |_| Reply::new(404),
    )
    .await;
    let callback = AuthorizationCallback::from_query(&format!(
        "code=synthetic-code&state=s1&realmId={REALM_B}"
    ))
    .unwrap();
    let connection = app(&server)
        .exchange_code(&http(Duration::from_secs(5)), callback, "s1")
        .await
        .unwrap();
    assert_eq!(connection.realm().as_str(), REALM_B);
    assert_eq!(
        connection.refresh_token().expose_secret(),
        "synthetic-refresh-rotated-2"
    );
    let exchange = server.single();
    assert_eq!(exchange.header("authorization"), Some(BASIC));
    assert_eq!(
        exchange.form_pairs(),
        [
            ("grant_type".to_owned(), "authorization_code".to_owned()),
            ("code".to_owned(), "synthetic-code".to_owned()),
            ("redirect_uri".to_owned(), REDIRECT.to_owned()),
        ]
    );
}

// ---------------------------------------------------------------- documented error classes

#[tokio::test]
async fn a_validation_fault_keeps_its_codes_elements_and_transaction_id() {
    let error = read_error(|| {
        json_fixture(400, "fault-validation.json").header("intuit_tid", "1-synthetic-tid")
    })
    .await;
    let provider = error.provider().unwrap();
    assert_eq!(provider.status().as_u16(), 400);
    assert_eq!(provider.intuit_tid(), Some("1-synthetic-tid"));
    let fault = provider.fault().unwrap();
    assert_eq!(fault.kind(), Some("ValidationFault"));
    let codes: Vec<_> = fault
        .errors()
        .iter()
        .map(|e| (e.code(), e.element()))
        .collect();
    assert_eq!(
        codes,
        [
            (Some("2050"), Some("firstname")),
            (Some("2080"), Some("postalcode"))
        ]
    );
    assert_eq!(
        error.to_string(),
        "QuickBooks answered 400 Bad Request with a fault (ValidationFault 2050)"
    );
}

#[tokio::test]
async fn each_documented_status_is_a_provider_error_with_its_fault() {
    for (status, fixture, kind, code) in [
        (400, "fault-object-not-found.json", "ValidationFault", "610"),
        (
            403,
            "fault-authorization.json",
            "AuthorizationFault",
            "5020",
        ),
        (500, "fault-system.json", "SystemFault", "10000"),
        (503, "fault-system.json", "SystemFault", "10000"),
    ] {
        let error = read_error(move || json_fixture(status, fixture)).await;
        let provider = error
            .provider()
            .unwrap_or_else(|| panic!("{status}: {error:?}"));
        assert_eq!(provider.status().as_u16(), status);
        let fault = provider.fault().unwrap();
        assert_eq!((fault.kind(), fault.code()), (Some(kind), Some(code)));
    }
    // A 404 or 429 without a fault document is still the status, the document reported apart.
    for status in [404, 429] {
        let error = read_error(move || Reply::text(status, "text/html", "<html>no</html>")).await;
        let provider = error.provider().unwrap();
        assert_eq!(provider.status().as_u16(), status);
        assert_eq!(provider.fault().unwrap_err().kind(), ErrorKind::Metadata);
    }
}

#[tokio::test]
async fn a_fault_inside_a_successful_response_is_a_provider_error() {
    let error = read_error(|| json_fixture(200, "fault-in-success.json")).await;
    let provider = error.provider().unwrap();
    assert_eq!(provider.status().as_u16(), 200);
    assert_eq!(provider.fault().unwrap().code(), Some("6000"));

    let server = serve(|_| json_fixture(200, "query-fault-in-success.json")).await;
    let (qbo, _) = client(&server);
    let error = qbo
        .customers()
        .query(CustomerQuery::new())
        .send()
        .await
        .unwrap_err();
    let fault = error.provider().unwrap().fault().unwrap();
    assert_eq!(
        (fault.kind(), fault.code()),
        (Some("Validation"), Some("100"))
    );
}

#[tokio::test]
async fn a_401_on_a_read_is_refreshed_once_then_reported() {
    let server = serve(|_| json_fixture(401, "fault-authentication.json")).await;
    let (qbo, _) = client(&server);
    let error = qbo.customer(customer_29()).read().send().await.unwrap_err();
    let auth = error.authorization().unwrap();
    assert_eq!(auth.kind(), AuthorizationKind::AccessTokenRejected);
    assert_eq!(auth.realm().as_str(), REALM_A);
    assert!(!auth.needs_reauthorization());
    let response = auth.response().unwrap();
    assert_eq!(response.fault().unwrap().code(), Some("3200"));
    assert_eq!(api_requests(&server).len(), 2);
    assert_eq!(token_requests(&server).len(), 2);
}

#[tokio::test]
async fn write_failures_are_definite_or_uncertain_never_resent() {
    let (error, server) = create_error(|| json_fixture(400, "fault-validation.json")).await;
    assert_eq!(error.provider().unwrap().status().as_u16(), 400);
    assert_eq!(api_requests(&server).len(), 1);

    let (error, server) = create_error(|| json_fixture(200, "fault-in-success.json")).await;
    assert_eq!(
        error.provider().unwrap().status().as_u16(),
        200,
        "a fault: not created"
    );
    assert_eq!(api_requests(&server).len(), 1);

    let (error, server) = create_error(|| json_fixture(500, "fault-system.json")).await;
    let uncertain = error.uncertain().unwrap();
    assert!(matches!(uncertain.cause(), UncertainCause::ServerError(_)));
    assert_eq!(uncertain.request_id().unwrap().as_str().len(), 32);
    assert_eq!(api_requests(&server).len(), 1);

    let (error, server) = create_error(|| json_fixture(200, "customer-malformed.json")).await;
    let uncertain = error.uncertain().unwrap();
    assert!(
        matches!(uncertain.cause(), UncertainCause::UnreadableSuccess(e) if e.kind() == ErrorKind::Decode)
    );
    assert_eq!(api_requests(&server).len(), 1);

    let (error, server) = create_error(Reply::hang_up).await;
    assert!(matches!(
        error.uncertain().unwrap().cause(),
        UncertainCause::NoResponse(_)
    ));
    assert_eq!(api_requests(&server).len(), 1);

    let (error, server) = create_error(|| json_fixture(429, "fault-system.json")).await;
    assert_eq!(error.provider().unwrap().status().as_u16(), 429);
    assert_eq!(api_requests(&server).len(), 1);
}

// ---------------------------------------------------------------- bodies

#[tokio::test]
async fn unusable_success_bodies_are_concord_errors_of_their_stage() {
    let error = read_error(|| json_fixture(200, "customer-malformed.json")).await;
    assert_eq!(concord_kind(&error), ErrorKind::Decode);

    let error = read_error(|| json_fixture(200, "customer-wrong-shape.json")).await;
    assert_eq!(concord_kind(&error), ErrorKind::Decode);

    let error = read_error(|| Reply::text(200, "text/html", fixture("customer-read.json"))).await;
    assert_eq!(concord_kind(&error), ErrorKind::Metadata);

    let error = read_error(|| Reply::new(200).body(fixture("customer-read.json"))).await;
    assert_eq!(concord_kind(&error), ErrorKind::Metadata, "no Content-Type");

    let error = read_error(|| {
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![Bytes::from_static(
                b"{\"Customer\": {\"Id\"",
            )]))
    })
    .await;
    assert_eq!(concord_kind(&error), ErrorKind::Transport, "truncated");

    let error = read_error(|| {
        let chunk = ENTITY_BODY_LIMIT / 64;
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Generate(
                generated_records(65, chunk, move |_| vec![b' '; chunk]),
                None,
            ))
    })
    .await;
    assert_eq!(concord_kind(&error), ErrorKind::Limit, "oversized");
}

#[tokio::test]
async fn unusable_error_bodies_keep_the_status() {
    let error = read_error(|| Reply::json(400, "{\"Fault\": ")).await;
    let provider = error.provider().unwrap();
    assert_eq!(provider.status().as_u16(), 400);
    assert_eq!(provider.fault().unwrap_err().kind(), ErrorKind::Decode);

    let error = read_error(|| Reply::json(400, vec![b' '; ERROR_BODY_LIMIT + 1])).await;
    assert_eq!(
        error.provider().unwrap().fault().unwrap_err().kind(),
        ErrorKind::Limit
    );
}

#[tokio::test]
async fn an_empty_query_result_is_an_empty_page() {
    let server = serve(|_| json_fixture(200, "customer-query-empty.json")).await;
    let (qbo, _) = client(&server);
    let page = qbo
        .customers()
        .query(CustomerQuery::new())
        .send()
        .await
        .unwrap()
        .page;
    assert!(page.customers.is_empty());
    assert_eq!(page.start_position, None);
}

#[tokio::test]
async fn a_page_that_breaks_the_offset_rule_stops_the_walk() {
    // Asked for 1 per page from position 1; the server reports position 3.
    let server = serve(|_| json_fixture(200, "customer-query-page2.json")).await;
    let (qbo, _) = client(&server);
    let query = CustomerQuery::new().max_results(1).unwrap();
    let result = concord_sdk_quickbooks_online::customer_pages(&qbo, query)
        .items()
        .collect_complete()
        .await;
    let incomplete = result.unwrap_err();
    assert!(matches!(
        incomplete.error.provider_error(),
        Some(Error::Pagination(
            concord_sdk_quickbooks_online::PaginationError::StartPositionMismatch
        ))
    ));
    // Two customers where one was asked for.
    let server = serve(|_| json_fixture(200, "customer-query-page1.json")).await;
    let (qbo, _) = client(&server);
    let query = CustomerQuery::new().max_results(1).unwrap();
    let result = concord_sdk_quickbooks_online::customer_pages(&qbo, query)
        .items()
        .collect_complete()
        .await;
    assert!(matches!(
        result.unwrap_err().error.provider_error(),
        Some(Error::Pagination(
            concord_sdk_quickbooks_online::PaginationError::Offset(_)
        ))
    ));
}

// ---------------------------------------------------------------- token endpoint classes

async fn token_failure(
    reply: impl Fn() -> Reply + Send + Sync + 'static,
) -> (Error, Connection, TestServer) {
    let server = serve_tokens(
        move |_| reply(),
        |_| json_fixture(200, "customer-read.json"),
    )
    .await;
    let (qbo, connection) = client(&server);
    let error = qbo.customer(customer_29()).read().send().await.unwrap_err();
    assert!(
        api_requests(&server).is_empty(),
        "no API request without a token"
    );
    (error, connection, server)
}

/// A token endpoint reply, the kind it maps to, the OAuth `error`, and whether it is terminal.
type TokenCase = (fn() -> Reply, AuthorizationKind, Option<&'static str>, bool);

#[tokio::test]
async fn token_endpoint_refusals_are_classified() {
    let cases: [TokenCase; 7] = [
        (
            || json_fixture(400, "oauth-invalid-grant.json"),
            AuthorizationKind::GrantRejected,
            Some("invalid_grant"),
            true,
        ),
        (
            || json_fixture(401, "oauth-invalid-client.json"),
            AuthorizationKind::GrantRejected,
            Some("invalid_client"),
            true,
        ),
        (
            || Reply::text(400, "text/html", "<html/>"),
            AuthorizationKind::GrantRejected,
            None,
            true,
        ),
        (
            || Reply::json(429, "{}"),
            AuthorizationKind::TokenServiceUnavailable,
            None,
            false,
        ),
        (
            || Reply::json(503, "{}"),
            AuthorizationKind::GrantUncertain,
            None,
            false,
        ),
        (
            Reply::hang_up,
            AuthorizationKind::GrantUncertain,
            None,
            false,
        ),
        (
            || Reply::json(200, "{\"token_type\": \"bearer\""),
            AuthorizationKind::GrantUncertain,
            None,
            false,
        ),
    ];
    for (reply, kind, oauth_error, terminal) in cases {
        let (error, connection, server) = token_failure(reply).await;
        let auth = error.authorization().unwrap_or_else(|| panic!("{error:?}"));
        assert_eq!(auth.kind(), kind, "{error:?}");
        assert_eq!(auth.oauth_error(), oauth_error);
        assert_eq!(auth.realm().as_str(), REALM_A);
        assert_eq!(connection.needs_reauthorization(), terminal);
        // A rejection is remembered; anything else is tried again by the next call.
        let qbo = qbo(&server, connection.clone());
        let _ = qbo.customer(customer_29()).read().send().await;
        assert_eq!(token_requests(&server).len(), if terminal { 1 } else { 2 });
        assert_eq!(connection.refresh_token().expose_secret(), STORED_REFRESH);
    }
}

#[tokio::test]
async fn a_token_response_that_breaks_the_contract_still_stores_the_new_refresh_token() {
    let (error, connection, _) = token_failure(|| json_fixture(200, "token-wrong-type.json")).await;
    let auth = error.authorization().unwrap();
    assert_eq!(auth.kind(), AuthorizationKind::GrantUncertain);
    assert_eq!(auth.violation(), Some("token_type is not bearer"));
    assert_eq!(
        connection.refresh_token().expose_secret(),
        "synthetic-refresh-after-mac"
    );

    let zero = r#"{"token_type":"bearer","expires_in":0,"refresh_token":"r2","access_token":"a"}"#;
    let (error, _, _) = token_failure(move || Reply::json(200, zero)).await;
    assert_eq!(
        error.authorization().unwrap().violation(),
        Some("expires_in is zero")
    );
}

// ---------------------------------------------------------------- inputs and callbacks

#[test]
fn inputs_outside_their_documented_range_are_refused() {
    assert!(RealmId::new("").is_err());
    assert!(RealmId::new("12a4").is_err());
    assert!(RealmId::new("1".repeat(33)).is_err());
    assert!(EntityId::new("../1").is_err());
    assert!(RequestId::new("x".repeat(51)).is_err());
    assert!(RequestId::new("has space").is_err());
    assert!(RequestId::new("x".repeat(50)).is_ok());
    assert!(Decimal::parse("1e3").is_err());
    assert!(Decimal::parse("+1").is_err());
    assert!(Decimal::parse("1.").is_err());
    assert_eq!(Decimal::from_cents(-5).as_str(), "-0.05");
    assert!(invoice().doc_number(&"9".repeat(22)).is_err());
    assert!(invoice().doc_number("").is_err());
    assert!(CustomerQuery::new().display_name("100%").is_err());
    assert!(CustomerQuery::new().display_name("a\\b").is_err());
    assert!(CustomerQuery::new().display_name("line\nbreak").is_err());
    assert!(CustomerQuery::new().max_results(0).is_err());
    assert!(CustomerQuery::new().max_results(1001).is_err());
    assert!(CustomerQuery::new().max_results(1000).is_ok());
    assert!(RefreshToken::new("").is_err());
    assert!(OAuthApp::new(Environment::Sandbox, "", "s", REDIRECT).is_err());
    assert!(OAuthApp::new(Environment::Sandbox, "a:b", "s", REDIRECT).is_err());
    assert!(OAuthApp::new(Environment::Sandbox, "a", "s", "ftp://x").is_err());
}

#[test]
fn callbacks_are_parsed_strictly() {
    let ok =
        AuthorizationCallback::from_query(&format!("code=c&state=s&realmId={REALM_A}")).unwrap();
    assert_eq!((ok.realm().as_str(), ok.state()), (REALM_A, "s"));
    let denied = AuthorizationCallback::from_query("error=access_denied&state=s").unwrap_err();
    assert!(matches!(denied, Error::Callback(CallbackError::Denied(e)) if e == "access_denied"));
    let missing = AuthorizationCallback::from_query("code=c&state=s").unwrap_err();
    assert!(matches!(
        missing,
        Error::Callback(CallbackError::Missing("realmId"))
    ));
    let repeated =
        AuthorizationCallback::from_query(&format!("code=c&code=d&state=s&realmId={REALM_A}"))
            .unwrap_err();
    assert!(matches!(
        repeated,
        Error::Callback(CallbackError::Missing("code"))
    ));
    let empty = AuthorizationCallback::from_query(&format!("code=&state=s&realmId={REALM_A}"));
    assert!(empty.is_err());
    assert!(AuthorizationCallback::from_query("code=c&state=s&realmId=abc").is_err());
}

// ---------------------------------------------------------------- redaction

#[tokio::test]
async fn no_secret_reaches_debug_or_display() {
    let secrets = [
        CLIENT_SECRET,
        &BASIC["Basic ".len()..],
        STORED_REFRESH,
        ACCESS,
        "synthetic-refresh-rotated-2",
        "synthetic-code-secret",
        "synthetic detail that must never be printed",
        "Length of the field exceeds",
        "Secret Customer Name",
    ];
    let mut shown = Vec::new();

    let server = serve(|r| {
        if r.path().ends_with("/query") {
            json_fixture(200, "fault-in-success.json")
        } else {
            json_fixture(400, "fault-validation.json")
        }
    })
    .await;
    let (qbo, connection) = client(&server);
    let query = CustomerQuery::new()
        .display_name("Secret Customer Name")
        .unwrap();
    let prepared = qbo.customers().query(query.clone()).prepare().unwrap();
    let prepared_debug = format!("{prepared:?}");
    assert!(prepared_debug.contains("<redacted>"));
    shown.push(prepared_debug);
    shown.push(format!("{query:?}"));
    shown.push(format!("{qbo:?}"));
    shown.push(format!("{connection:?}"));
    shown.push(format!("{:?}", connection.refresh_token()));
    shown.push(format!("{:?}", app(&server)));
    let callback = AuthorizationCallback::from_query(&format!(
        "code=synthetic-code-secret&state=s&realmId={REALM_A}"
    ))
    .unwrap();
    shown.push(format!("{callback:?}"));
    for error in [
        qbo.customers().query(query).send().await.unwrap_err(),
        qbo.customer(customer_29()).read().send().await.unwrap_err(),
        qbo.invoices().create(invoice()).send().await.unwrap_err(),
    ] {
        shown.push(format!("{error} {error:?}"));
    }
    // An authorization error and an uncertain write.
    let (error, _, _) = token_failure(|| json_fixture(400, "oauth-invalid-grant.json")).await;
    shown.push(format!("{error} {error:?}"));
    let (error, _) = create_error(Reply::hang_up).await;
    shown.push(format!("{error} {error:?}"));

    for text in &shown {
        for secret in secrets {
            assert!(!text.contains(secret), "{secret:?} in {text}");
        }
    }
}

#[test]
fn fixture_constructor_preserves_the_environment_boundary() {
    let app = OAuthApp::new(Environment::Sandbox, CLIENT_ID, CLIENT_SECRET, REDIRECT).unwrap();
    let connection = app.connection(realm(REALM_A), RefreshToken::new(STORED_REFRESH).unwrap());
    for bad in [
        "https://127.0.0.1:1234",
        "http://192.0.2.1:1234",
        "http://localhost:1234",
        "http://127.0.0.1:1234/path",
        "http://127.0.0.1:1234?x=1",
    ] {
        assert!(
            QuickBooks::for_fixture(http(Duration::from_secs(1)), connection.clone(), bad).is_err()
        );
    }
    for good in ["http://127.0.0.1:1234", "http://[::1]:1234"] {
        let api = QuickBooks::for_fixture(http(Duration::from_secs(1)), connection.clone(), good)
            .unwrap();
        let prepared = api.customer(customer_29()).read().prepare().unwrap();
        assert_eq!(
            prepared.request().url().host_str(),
            reqwest::Url::parse(good).unwrap().host_str()
        );
        assert!(!prepared.request().headers().contains_key("authorization"));
    }
}
