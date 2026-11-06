//! Single-operation wire tests of the SDK against the loopback test server: what each operation
//! puts on the wire (target, headers, body) and how responses, error documents and metadata are
//! decoded. Two accounts are used throughout so that a `Stripe-Account` leak is visible.
//!
//! Response bodies are minimal synthetic objects shaped after the reference examples for API
//! version 2026-10-28.endive (retrieved 2026-11-04); the workflow fixtures with provenance live
//! in `examples/stripe-app/tests/fixtures`.

use concord::ErrorKind;
use concord_example_stripe_sdk::pages::{CursorList, Direction};
use concord_example_stripe_sdk::params::CustomerCreate;
use concord_example_stripe_sdk::send::ApiErrorType;
use concord_example_stripe_sdk::{
    AccountId, AccountOverride, CustomerId, ExpandPath, Expandable, IdempotencyKey, InvoiceId,
    Limit, OnAccount, ProtocolError, SecretKey, StripeContext, Timestamp, send,
};
use concord_example_testserver::{Recorded, Reply, Script, TestServer};

const KEY: &str = "sk_test_SYNTHETIC0000secret";

fn context(server: &TestServer) -> StripeContext {
    let key = SecretKey::new(KEY).unwrap();
    StripeContext::new(concord::reqwest::Client::new(), &key)
        .with_base_url(format!("{}/v1", server.url()))
        .unwrap()
}

fn acct(id: &str) -> AccountId {
    AccountId::new(id).unwrap()
}

fn customer_json(id: &str) -> String {
    format!(
        r#"{{"id":"{id}","object":"customer","email":"{id}@example.test","name":"Synthetic {id}",
        "description":null,"balance":0,"created":1767225600,"currency":"usd","metadata":{{}},
        "livemode":false}}"#
    )
}

fn list_json(items: &[String], has_more: bool, url: &str) -> String {
    format!(
        r#"{{"object":"list","url":"{url}","has_more":{has_more},"data":[{}]}}"#,
        items.join(",")
    )
}

fn header<'a>(request: &'a Recorded, name: &str) -> Option<&'a str> {
    request.header(name)
}

#[tokio::test]
async fn list_query_headers_and_account_scope() {
    let server = TestServer::start(|_| {
        Reply::json(
            200,
            list_json(&[customer_json("cus_one")], false, "/v1/customers"),
        )
        .header("request-id", "req_synthetic1")
    })
    .await;
    let platform = context(&server);
    let alpha = platform.clone().with_account(Some(acct("acct_ALPHA")));
    let beta = platform.clone().with_account(Some(acct("acct_BETA")));

    let list = alpha
        .api()
        .customers()
        .list()
        .set_email("a+b@example.test")
        .set_created_gte(Timestamp(1767225600))
        .set_limit(Limit::new(3).unwrap())
        .set_expand(vec![ExpandPath::new("data.test_clock").unwrap()]);
    let page = send(list.prepare().unwrap()).await.unwrap();
    assert_eq!(page.value.data[0].id.as_str(), "cus_one");
    assert_eq!(page.meta.request_id.as_deref(), Some("req_synthetic1"));

    beta.api().customers().list().send().await.unwrap();
    platform.api().customers().list().send().await.unwrap();
    // Per-request overrides on the alpha context.
    alpha
        .api()
        .customers()
        .list()
        .on_account(&AccountOverride::Platform)
        .send()
        .await
        .unwrap();
    alpha
        .api()
        .customers()
        .list()
        .on_account(&AccountOverride::Account(acct("acct_BETA")))
        .send()
        .await
        .unwrap();
    alpha
        .api()
        .customers()
        .list()
        .on_account(&AccountOverride::Inherit)
        .send()
        .await
        .unwrap();

    let requests = server.requests();
    assert_eq!(
        requests[0].target,
        "/v1/customers?email=a%2Bb%40example.test&created%5Bgte%5D=1767225600&limit=3\
         &expand%5B%5D=data.test_clock"
    );
    assert_eq!(
        requests[0].query_pairs(),
        vec![
            ("email".to_owned(), "a+b@example.test".to_owned()),
            ("created[gte]".to_owned(), "1767225600".to_owned()),
            ("limit".to_owned(), "3".to_owned()),
            ("expand[]".to_owned(), "data.test_clock".to_owned()),
        ]
    );
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert_eq!(
            header(request, "authorization"),
            Some(&*format!("Bearer {KEY}"))
        );
        assert_eq!(header(request, "stripe-version"), Some("2026-10-28.endive"));
        assert_eq!(header(request, "accept"), Some("application/json"));
        assert_eq!(header(request, "idempotency-key"), None);
    }
    let accounts: Vec<Option<&str>> = requests
        .iter()
        .map(|r| header(r, "stripe-account"))
        .collect();
    assert_eq!(
        accounts,
        [
            Some("acct_ALPHA"),
            Some("acct_BETA"),
            None,
            None,
            Some("acct_BETA"),
            Some("acct_ALPHA")
        ]
    );
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn expandable_field_in_both_forms() {
    let invoice = |customer: &str, subscription: &str| {
        format!(
            r#"{{"id":"in_one","object":"invoice","customer":{customer},"currency":"eur",
            "status":"paid","total":1200,"subtotal":1000,"amount_due":1200,"amount_paid":1200,
            "amount_remaining":0,"collection_method":"charge_automatically","created":1767225600,
            "period_start":1767225600,"period_end":1767225600,
            "parent":{{"type":"subscription_details","subscription_details":{{"subscription":{subscription},"metadata":{{}}}}}},
            "lines":{{"object":"list","data":[],"has_more":false,"total_count":0,"url":"/v1/invoices/in_one/lines"}},
            "metadata":null,"livemode":false}}"#
        )
    };
    let subscription = r#"{"id":"sub_one","object":"subscription","customer":"cus_one",
        "status":"active","items":{"object":"list","data":[],"has_more":false,"url":"/v1/subscription_items?subscription=sub_one"},
        "latest_invoice":"in_one","currency":"eur","collection_method":"charge_automatically",
        "created":1767225600,"metadata":{}}"#;
    let script = Script::new([
        Reply::json(200, invoice(r#""cus_one""#, r#""sub_one""#)),
        Reply::json(200, invoice(&customer_json("cus_one"), subscription)),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let stripe = context(&server);
    let id = InvoiceId::new("in_one").unwrap();

    let plain = stripe
        .api()
        .invoices()
        .retrieve(id.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(
        plain.customer,
        Expandable::Id(CustomerId::new("cus_one").unwrap())
    );
    assert!(!plain.subscription().unwrap().is_expanded());

    let expanded = stripe
        .api()
        .invoices()
        .retrieve(id)
        .set_expand(vec![
            ExpandPath::new("customer").unwrap(),
            ExpandPath::new("parent.subscription_details.subscription").unwrap(),
        ])
        .send()
        .await
        .unwrap();
    let customer = expanded.customer.object().unwrap();
    assert_eq!(customer.email.as_deref(), Some("cus_one@example.test"));
    assert_eq!(expanded.customer.id().as_str(), "cus_one");
    assert_eq!(expanded.subscription().unwrap().id().as_str(), "sub_one");
    assert!(expanded.subscription().unwrap().is_expanded());

    let requests = server.requests();
    assert_eq!(requests[0].target, "/v1/invoices/in_one");
    assert_eq!(
        requests[1].query_all("expand[]"),
        ["customer", "parent.subscription_details.subscription"]
    );
    server.shutdown().await;
}

/// `invoice_settings.default_payment_method` (nullable, expandable on the customer object; the
/// payment method object shaped after its reference example, retrieved 2026-11-05).
#[tokio::test]
async fn customer_default_payment_method_in_id_object_and_null_forms() {
    let customer = |default_payment_method: &str| {
        format!(
            r#"{{"id":"cus_pm","object":"customer","email":null,"name":null,"description":null,
            "balance":0,"created":1767225600,"currency":null,
            "invoice_settings":{{"custom_fields":null,"default_payment_method":{default_payment_method},"footer":null,"rendering_options":null}},
            "metadata":{{}},"livemode":false}}"#
        )
    };
    let payment_method = r#"{"id":"pm_one","object":"payment_method","type":"card",
        "billing_details":{"address":{"city":null,"country":null,"line1":null,"line2":null,"postal_code":null,"state":null},"email":null,"name":null,"phone":null},
        "card":{"brand":"visa","last4":"4242","exp_month":12,"exp_year":2030,"funding":"credit"},
        "created":1767225600,"customer":"cus_pm","livemode":false,"metadata":{}}"#;
    let script = Script::new([
        Reply::json(200, customer(r#""pm_one""#)),
        Reply::json(200, customer(payment_method)),
        Reply::json(200, customer("null")),
        Reply::json(
            200,
            list_json(&[customer(payment_method)], false, "/v1/customers"),
        ),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let stripe = context(&server);
    let id = CustomerId::new("cus_pm").unwrap();
    let path = ExpandPath::new("invoice_settings.default_payment_method").unwrap();

    let plain = stripe
        .api()
        .customers()
        .retrieve(id.clone())
        .send()
        .await
        .unwrap();
    let method = plain.default_payment_method().unwrap();
    assert!(!method.is_expanded());
    assert_eq!(method.id().as_str(), "pm_one");

    let expanded = stripe
        .api()
        .customers()
        .retrieve(id.clone())
        .set_expand(vec![path.clone()])
        .send()
        .await
        .unwrap();
    let object = expanded.default_payment_method().unwrap().object().unwrap();
    assert_eq!(object.kind, "card");
    let card = object.card.as_ref().unwrap();
    assert_eq!((card.brand.as_str(), card.last4.as_str()), ("visa", "4242"));
    assert_eq!((card.exp_month, card.exp_year), (12, 2030));
    assert_eq!(object.customer.as_ref().unwrap().id(), &id);

    let none = stripe.api().customers().retrieve(id).send().await.unwrap();
    assert!(none.default_payment_method().is_none());

    let listed = stripe
        .api()
        .customers()
        .list()
        .set_expand(vec![
            ExpandPath::new("data.invoice_settings.default_payment_method").unwrap(),
        ])
        .send()
        .await
        .unwrap();
    assert!(
        listed.data[0]
            .default_payment_method()
            .unwrap()
            .is_expanded()
    );

    let requests = server.requests();
    assert_eq!(requests[0].target, "/v1/customers/cus_pm");
    assert_eq!(
        requests[1].target,
        "/v1/customers/cus_pm?expand%5B%5D=invoice_settings.default_payment_method"
    );
    assert_eq!(
        requests[3].query_all("expand[]"),
        ["data.invoice_settings.default_payment_method"]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn write_carries_form_body_idempotency_key_and_metadata() {
    let server = TestServer::start(|_| {
        Reply::json(200, customer_json("cus_new"))
            .header("request-id", "req_write1")
            .header("idempotent-replayed", "true")
    })
    .await;
    let stripe = context(&server).with_account(Some(acct("acct_ALPHA")));
    let params = CustomerCreate {
        email: Some("new@example.test".to_owned()),
        preferred_locales: vec!["fr-FR".to_owned(), "en".to_owned()],
        metadata: [("crm_id".to_owned(), "42".to_owned())].into(),
        ..CustomerCreate::default()
    };
    let key = IdempotencyKey::new("synthetic-idem-0001").unwrap();
    let create = stripe
        .api()
        .customers()
        .create(params)
        .set_idempotency_key(key);
    let prepared = create.prepare().unwrap();
    let shown = format!("{prepared:?}");
    assert!(!shown.contains("synthetic-idem-0001"), "{shown}");
    assert!(!shown.contains("acct_ALPHA"), "{shown}");
    assert!(!shown.contains(KEY), "{shown}");
    let created = send(prepared).await.unwrap();
    assert_eq!(created.value.id.as_str(), "cus_new");
    assert!(created.meta.idempotent_replayed);
    assert_eq!(created.meta.request_id.as_deref(), Some("req_write1"));

    let request = server.single();
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/v1/customers");
    assert_eq!(
        request.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        request.header("idempotency-key"),
        Some("synthetic-idem-0001")
    );
    assert_eq!(request.header("stripe-account"), Some("acct_ALPHA"));
    assert_eq!(
        request.form_pairs(),
        vec![
            ("email".to_owned(), "new@example.test".to_owned()),
            ("preferred_locales[]".to_owned(), "fr-FR".to_owned()),
            ("preferred_locales[]".to_owned(), "en".to_owned()),
            ("metadata[crm_id]".to_owned(), "42".to_owned()),
        ]
    );
    server.shutdown().await;
}

#[tokio::test]
async fn error_documents_rate_limits_and_lost_responses() {
    let script = Script::new([
        Reply::json(
            400,
            r#"{"error":{"type":"idempotency_error","message":"Keys for idempotent requests can only be used with the same parameters they were first used with. SECRET-PARAM-VALUE","request_log_url":"https://dashboard.stripe.com/logs/req_x"}}"#,
        )
        .header("request-id", "req_err1"),
        Reply::json(
            429,
            r#"{"error":{"type":"invalid_request_error","code":"rate_limit","message":"Too many requests"}}"#,
        )
        .header("request-id", "req_err2")
        .header("stripe-rate-limited-reason", "global-rate")
        .header("stripe-should-retry", "true"),
        Reply::json(
            404,
            r#"{"error":{"type":"invalid_request_error","code":"resource_missing","param":"id","message":"No such customer: 'cus_gone'"}}"#,
        ),
        Reply::text(500, "text/html", "<html>proxy</html>"),
        Reply::hang_up(),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let stripe = context(&server);
    let id = CustomerId::new("cus_gone").unwrap();
    let retrieve = || {
        stripe
            .api()
            .customers()
            .retrieve(id.clone())
            .prepare()
            .unwrap()
    };

    let idem = send(retrieve()).await.unwrap_err();
    assert!(idem.is_idempotency_error());
    let shown = format!("{idem} {idem:?}");
    assert_eq!(
        idem.to_string(),
        "Stripe.customers.retrieve: Stripe returned HTTP 400 (idempotency_error), request req_err1"
    );
    assert!(!shown.contains("SECRET-PARAM-VALUE"), "{shown}");
    let failure = idem.api().unwrap();
    assert_eq!(
        failure.error().unwrap().kind,
        ApiErrorType::IdempotencyError
    );
    assert!(
        failure
            .error()
            .unwrap()
            .message()
            .unwrap()
            .contains("SECRET-PARAM-VALUE")
    );

    let limited = send(retrieve()).await.unwrap_err();
    assert!(limited.is_rate_limited());
    let meta = limited.meta().unwrap();
    assert_eq!(meta.should_retry, Some(true));
    assert_eq!(
        limited.to_string(),
        "Stripe.customers.retrieve: Stripe returned HTTP 429 (invalid_request_error/rate_limit), \
         rate limited: global-rate, request req_err2"
    );

    let missing = send(retrieve()).await.unwrap_err();
    let error = missing.api().unwrap().error().unwrap();
    assert_eq!(error.code.as_deref(), Some("resource_missing"));
    assert!(!format!("{missing:?}").contains("cus_gone"));

    let html = send(retrieve()).await.unwrap_err();
    assert!(html.api().unwrap().error().is_err());

    let lost = send(retrieve()).await.unwrap_err();
    assert!(lost.is_response_lost());
    assert_eq!(lost.concord().unwrap().kind(), ErrorKind::Transport);
    assert!(!format!("{lost} {lost:?}").contains(KEY));
    server.shutdown().await;
}

/// The head's metadata is captured before the body is read and kept when the 2xx body is
/// refused (`Metadata`, unread) or does not decode (`Decode`); no response, no metadata. A
/// non-2xx error document is read from the body whatever its `Content-Type`, within
/// `MAX_ERROR_BODY`; a larger one is reported, not read to its end.
#[tokio::test]
async fn metadata_survives_body_failures_and_error_documents_are_bounded() {
    use concord_example_stripe_sdk::send::{DocumentProblem, MAX_ERROR_BODY};

    let oversized = format!(
        r#"{{"error":{{"type":"api_error","message":"{}"}}}}"#,
        "x".repeat(MAX_ERROR_BODY)
    );
    let script = Script::new([
        Reply::json(
            200,
            r#"{"id":"cus_1","object":"customer","balance":"zero"}"#,
        )
        .header("request-id", "req_decode")
        .header("stripe-version", "2026-10-28.endive"),
        Reply::text(200, "text/html", "<html>proxy</html>").header("request-id", "req_head"),
        Reply::text(
            402,
            "text/plain",
            r#"{"error":{"type":"card_error","code":"card_declined"}}"#,
        )
        .header("request-id", "req_card"),
        Reply::json(500, oversized).header("request-id", "req_big"),
        Reply::hang_up(),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let stripe = context(&server);
    let id = CustomerId::new("cus_1").unwrap();
    let retrieve = || {
        stripe
            .api()
            .customers()
            .retrieve(id.clone())
            .prepare()
            .unwrap()
    };

    let decode = send(retrieve()).await.unwrap_err();
    assert_eq!(decode.concord().unwrap().kind(), ErrorKind::Decode);
    let meta = decode.meta().unwrap();
    assert_eq!(meta.status, 200);
    assert_eq!(meta.request_id.as_deref(), Some("req_decode"));
    assert_eq!(meta.stripe_version.as_deref(), Some("2026-10-28.endive"));
    assert!(!decode.is_response_lost());

    let head = send(retrieve()).await.unwrap_err();
    let concord = head.concord().unwrap();
    assert_eq!(concord.kind(), ErrorKind::Metadata);
    assert!(
        concord.response().is_some(),
        "a refused head keeps the body"
    );
    assert_eq!(head.meta().unwrap().request_id.as_deref(), Some("req_head"));

    let card = send(retrieve()).await.unwrap_err();
    let failure = card.api().unwrap();
    assert_eq!(failure.endpoint(), Some("Stripe.customers.retrieve"));
    assert_eq!(failure.status().as_u16(), 402);
    assert_eq!(failure.meta().request_id.as_deref(), Some("req_card"));
    let document = failure.error().unwrap();
    assert_eq!(document.kind, ApiErrorType::CardError);
    assert_eq!(document.code.as_deref(), Some("card_declined"));

    let big = send(retrieve()).await.unwrap_err();
    let failure = big.api().unwrap();
    assert_eq!(failure.error().unwrap_err(), DocumentProblem::TooLarge);
    assert_eq!(failure.meta().request_id.as_deref(), Some("req_big"));

    let lost = send(retrieve()).await.unwrap_err();
    assert!(lost.is_response_lost());
    assert!(lost.meta().is_none());
    assert_eq!(
        lost.concord().unwrap().endpoint(),
        Some("Stripe.customers.retrieve")
    );
    assert_eq!(server.requests().len(), 5, "one execution each, no retry");
    server.shutdown().await;
}

#[tokio::test]
async fn cursor_list_pages_forward_and_refuses_inconsistent_pages() {
    let script = Script::new([
        Reply::json(
            200,
            list_json(
                &[customer_json("cus_c"), customer_json("cus_b")],
                true,
                "/v1/customers",
            ),
        ),
        Reply::json(
            200,
            list_json(&[customer_json("cus_a")], false, "/v1/customers"),
        ),
        // A non-terminal page with no objects.
        Reply::json(200, list_json(&[], true, "/v1/customers")),
        // A page whose item lacks an id.
        Reply::json(
            200,
            r#"{"object":"list","url":"/v1/customers","has_more":true,"data":[{"object":"customer","created":1}]}"#,
        ),
        // A page repeating the requesting cursor.
        Reply::json(
            200,
            list_json(&[customer_json("cus_b")], true, "/v1/customers"),
        ),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let alpha = context(&server).with_account(Some(acct("acct_ALPHA")));
    let beta = context(&server).with_account(Some(acct("acct_BETA")));
    let template = alpha
        .api()
        .customers()
        .list()
        .set_limit(Limit::new(2).unwrap());
    let list = CursorList::forward(template.clone()).unwrap();

    let first = list.fetch(None).await.unwrap();
    assert_eq!(first.items.len(), 2);
    let next = first.next.clone().unwrap();
    assert_eq!(next.id().as_str(), "cus_b");
    assert!(!format!("{next:?}").contains("cus_b"));
    let second = list.fetch(Some(next.clone())).await.unwrap();
    assert!(second.next.is_none());

    let empty = list.fetch(None).await.unwrap_err();
    assert_eq!(empty.protocol(), Some(ProtocolError::EmptyNonTerminalPage));
    let missing_id = list.fetch(None).await.unwrap_err();
    assert_eq!(missing_id.concord().unwrap().kind(), ErrorKind::Decode);
    let repeated = list.fetch(Some(next.clone())).await.unwrap_err();
    assert_eq!(
        repeated.protocol(),
        Some(ProtocolError::CursorDidNotAdvance)
    );

    // Refused before any request: direction, binding, template cursor.
    let backward = CursorList::new(template.clone(), Direction::Backward).unwrap();
    let wrong_direction = backward.fetch(Some(next.clone())).await.unwrap_err();
    assert_eq!(
        wrong_direction.protocol(),
        Some(ProtocolError::IncompatibleDirection)
    );
    let other_account = CursorList::forward(
        beta.api()
            .customers()
            .list()
            .set_limit(Limit::new(2).unwrap()),
    )
    .unwrap();
    let leaked = other_account.fetch(Some(next.clone())).await.unwrap_err();
    assert_eq!(
        leaked.protocol(),
        Some(ProtocolError::CursorBindingMismatch)
    );
    let preset = CursorList::forward(template.set_ending_before(CustomerId::new("cus_z").unwrap()))
        .unwrap_err();
    assert_eq!(preset.protocol(), Some(ProtocolError::TemplateHasCursor));

    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert_eq!(requests[0].target, "/v1/customers?limit=2");
    assert_eq!(
        requests[1].target,
        "/v1/customers?limit=2&starting_after=cus_b"
    );
    assert!(
        requests
            .iter()
            .all(|r| r.query_all("ending_before").is_empty())
    );
    server.shutdown().await;
}

#[test]
fn header_names_compose() {
    concord_example_stripe_sdk::Stripe::validate_header_names().unwrap();
}

#[test]
fn values_are_validated_and_redacted() {
    assert!(CustomerId::new("in_123").is_err());
    assert!(CustomerId::new("cus_a/b").is_err());
    assert!(Limit::new(0).is_err() && Limit::new(101).is_err());
    assert!(ExpandPath::new("data.a.b.c.d").is_err());
    assert!(ExpandPath::new("data.parent.subscription_details.subscription").is_ok());
    assert!(SecretKey::new("pk_test_x").is_err());
    assert!(IdempotencyKey::new("a".repeat(256)).is_err());
    let key = SecretKey::new(KEY).unwrap();
    assert!(key.is_test_mode());
    assert_eq!(format!("{key:?}"), "SecretKey(<redacted>)");
    assert_eq!(format!("{:?}", acct("acct_ALPHA")), "AccountId(<redacted>)");
    let stripe = StripeContext::new(concord::reqwest::Client::new(), &key)
        .with_account(Some(acct("acct_ALPHA")));
    let shown = format!("{stripe:?} {:?}", stripe.api());
    assert!(
        !shown.contains(KEY) && !shown.contains("acct_ALPHA"),
        "{shown}"
    );
}
