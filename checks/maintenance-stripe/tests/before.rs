//! The frozen initial consumer ([`before`]) against a loopback fake, with exact wire assertions.
//! Frozen at stage 1 of the exercise: do not edit. At later commits it runs unchanged against
//! the changed SDK, so the literal form bodies below are the omitted-field compatibility check
//! (a consumer that never sets a new field must send the same bytes).
//!
//! Response objects are synthetic, shaped after the customer object example of Stripe's API
//! reference for version `2026-10-28.endive` (<https://docs.stripe.com/api/customers/object>,
//! retrieved 2026-11-05), including its `invoice_settings` member, which the SDK did not model
//! at stage 1. Account, customer and payment method ids are synthetic; two connected accounts
//! make a `Stripe-Account` leak visible.

use concord_check_maintenance_stripe::before::{
    self, ContactProfile, Correction, CrmContact, Written,
};
use concord_example_stripe_sdk::{
    AccountId, CustomerId, IdempotencyKey, Limit, SecretKey, StripeContext,
};
use concord_example_testserver::{Recorded, Reply, Script, TestServer};

const KEY: &str = "sk_test_SYNTHETIC_maintenance";

fn context(server: &TestServer, account: &str) -> StripeContext {
    let key = SecretKey::new(KEY).unwrap();
    StripeContext::new(concord::reqwest::Client::new(), &key)
        .with_base_url(format!("{}/v1", server.url()))
        .unwrap()
        .with_account(Some(AccountId::new(account).unwrap()))
}

fn key(text: &str) -> IdempotencyKey {
    IdempotencyKey::new(text).unwrap()
}

/// A customer object as Stripe documents it (every member of the reference example present).
fn customer_json(account: &str) -> String {
    let (id, name, email, city, country, locale, crm) = match account {
        "acct_ALPHA" => (
            "cus_ALPHA0001",
            "Ada Lovelace",
            "ada@example.test",
            r#""London""#,
            "GB",
            r#"["en-GB"]"#,
            "crm-0001",
        ),
        _ => (
            "cus_BETA0001",
            "Grace Hopper",
            "grace+billing@example.test",
            "null",
            "US",
            "[]",
            "crm-0002",
        ),
    };
    format!(
        r#"{{"id":"{id}","object":"customer",
        "address":{{"city":{city},"country":"{country}","line1":null,"line2":null,"postal_code":null,"state":null}},
        "balance":0,"created":1767225600,"currency":null,"default_source":null,"delinquent":false,
        "description":null,"email":"{email}","invoice_prefix":"0759376C",
        "invoice_settings":{{"custom_fields":null,"default_payment_method":"pm_SYNTH{country}01","footer":null,"rendering_options":null}},
        "livemode":false,"metadata":{{"crm_id":"{crm}"}},"name":"{name}","next_invoice_sequence":1,
        "phone":null,"preferred_locales":{locale},"shipping":null,"tax_exempt":"none","test_clock":null}}"#
    )
}

/// Answers every customer operation with the customer of the request's `Stripe-Account`.
fn fake(request: &Recorded) -> Reply {
    let account = request.header("stripe-account").unwrap_or("platform");
    let customer = customer_json(account);
    let body = if request.method == "GET" && request.path() == "/v1/customers" {
        format!(r#"{{"object":"list","url":"/v1/customers","has_more":false,"data":[{customer}]}}"#)
    } else {
        customer
    };
    let reply =
        Reply::json(200, body).header("request-id", format!("req_SYNTH{}", request.body.len()));
    if request.header("idempotency-key") == Some("crm-0001-mirror-retry") {
        reply.header("idempotent-replayed", "true")
    } else {
        reply
    }
}

fn ada() -> CrmContact {
    CrmContact {
        crm_id: "crm-0001".to_owned(),
        name: "Ada Lovelace".to_owned(),
        email: "ada@example.test".to_owned(),
        country: "GB".to_owned(),
        city: Some("London".to_owned()),
        locale: Some("en-GB".to_owned()),
    }
}

fn grace() -> CrmContact {
    CrmContact {
        crm_id: "crm-0002".to_owned(),
        name: "Grace Hopper".to_owned(),
        email: "grace+billing@example.test".to_owned(),
        country: "US".to_owned(),
        city: None,
        locale: None,
    }
}

/// The exact `POST /v1/customers` body of [`ada`].
const ADA_CREATE_BODY: &str = "name=Ada+Lovelace&email=ada%40example.test\
    &address%5Bcity%5D=London&address%5Bcountry%5D=GB&preferred_locales%5B%5D=en-GB\
    &metadata%5Bcrm_id%5D=crm-0001";
/// The exact `POST /v1/customers` body of [`grace`].
const GRACE_CREATE_BODY: &str = "name=Grace+Hopper&email=grace%2Bbilling%40example.test\
    &address%5Bcountry%5D=US&metadata%5Bcrm_id%5D=crm-0002";
/// The exact `POST /v1/customers/{id}` body of the correction below.
const ADA_CORRECTION_BODY: &str =
    "name=Ada+King&phone=&metadata%5Bimport_batch%5D=&metadata%5Blegacy_ref%5D=";

fn assert_common_headers(request: &Recorded, account: &str) {
    assert_eq!(
        request.header("authorization"),
        Some(&*format!("Bearer {KEY}"))
    );
    assert_eq!(request.header("stripe-version"), Some("2026-10-28.endive"));
    assert_eq!(request.header("stripe-account"), Some(account));
    assert_eq!(request.header("accept"), Some("application/json"));
}

#[tokio::test]
async fn frozen_writes_send_exact_form_bytes_per_account() {
    let server = TestServer::start(fake).await;
    let alpha = context(&server, "acct_ALPHA");
    let beta = context(&server, "acct_BETA");

    let created = before::mirror_contact(&alpha, &ada(), key("crm-0001-mirror"))
        .await
        .unwrap();
    assert_eq!(created.profile.customer.as_str(), "cus_ALPHA0001");
    assert_eq!(created.profile.crm_id.as_deref(), Some("crm-0001"));
    assert!(!created.replayed);
    let replayed = before::mirror_contact(&alpha, &ada(), key("crm-0001-mirror-retry"))
        .await
        .unwrap();
    assert!(replayed.replayed);

    let correction = Correction {
        name: "Ada King".to_owned(),
        drop_phone: true,
        retire_keys: vec!["legacy_ref".to_owned(), "import_batch".to_owned()],
    };
    let corrected: Written = before::correct_contact(
        &alpha,
        &created.profile.customer,
        &correction,
        key("crm-0001-correct"),
    )
    .await
    .unwrap();
    assert_eq!(corrected.profile.customer.as_str(), "cus_ALPHA0001");

    let other = before::mirror_contact(&beta, &grace(), key("crm-0002-mirror"))
        .await
        .unwrap();
    assert_eq!(other.profile.customer.as_str(), "cus_BETA0001");
    assert_eq!(other.profile.country.as_deref(), Some("US"));

    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    let expected = [
        (
            "/v1/customers",
            "acct_ALPHA",
            "crm-0001-mirror",
            ADA_CREATE_BODY,
        ),
        (
            "/v1/customers",
            "acct_ALPHA",
            "crm-0001-mirror-retry",
            ADA_CREATE_BODY,
        ),
        (
            "/v1/customers/cus_ALPHA0001",
            "acct_ALPHA",
            "crm-0001-correct",
            ADA_CORRECTION_BODY,
        ),
        (
            "/v1/customers",
            "acct_BETA",
            "crm-0002-mirror",
            GRACE_CREATE_BODY,
        ),
    ];
    for (request, (target, account, idem, body)) in requests.iter().zip(expected) {
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, target);
        assert_common_headers(request, account);
        assert_eq!(request.header("idempotency-key"), Some(idem));
        assert_eq!(
            request.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(request.body_text(), body, "exact form bytes of {target}");
    }
    // The decoded view of the same bytes, as Stripe documents the spelling.
    assert_eq!(
        requests[0].form_pairs(),
        [
            ("name", "Ada Lovelace"),
            ("email", "ada@example.test"),
            ("address[city]", "London"),
            ("address[country]", "GB"),
            ("preferred_locales[]", "en-GB"),
            ("metadata[crm_id]", "crm-0001"),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
    );
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn frozen_reads_send_no_body_and_ignore_unmodelled_members() {
    let server = TestServer::start(fake).await;
    let alpha = context(&server, "acct_ALPHA");
    let beta = context(&server, "acct_BETA");

    let profile = before::fetch_profile(&alpha, &CustomerId::new("cus_ALPHA0001").unwrap())
        .await
        .unwrap();
    assert_eq!(
        profile,
        ContactProfile {
            customer: CustomerId::new("cus_ALPHA0001").unwrap(),
            name: Some("Ada Lovelace".to_owned()),
            email: Some("ada@example.test".to_owned()),
            country: Some("GB".to_owned()),
            crm_id: Some("crm-0001".to_owned()),
        }
    );
    let found = before::find_by_email(&beta, "grace+billing@example.test", Limit::new(5).unwrap())
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].customer.as_str(), "cus_BETA0001");

    let requests = server.requests();
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].target, "/v1/customers/cus_ALPHA0001");
    assert_common_headers(&requests[0], "acct_ALPHA");
    assert_eq!(
        requests[1].target,
        "/v1/customers?email=grace%2Bbilling%40example.test&limit=5"
    );
    assert_common_headers(&requests[1], "acct_BETA");
    for request in &requests {
        assert!(request.body.is_empty());
        assert_eq!(request.header("idempotency-key"), None);
        assert_eq!(request.header("content-type"), None);
    }
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn frozen_write_sees_stripe_parameter_errors_by_wire_name() {
    // A synthetic invalid_request_error shaped after https://docs.stripe.com/api/errors
    // (`type`, `code`, `param`, `message`); `email_invalid` is a documented error code.
    let script = Script::new([Reply::json(
        400,
        r#"{"error":{"type":"invalid_request_error","code":"email_invalid","param":"email",
        "message":"Invalid email address: SYNTHETIC-SUBMITTED-VALUE"}}"#,
    )
    .header("request-id", "req_SYNTHerr1")]);
    let server = TestServer::start(move |_| script.next()).await;
    let alpha = context(&server, "acct_ALPHA");
    let mut contact = ada();
    contact.email = "not-an-email".to_owned();

    let error = before::mirror_contact(&alpha, &contact, key("crm-0001-bad"))
        .await
        .unwrap_err();
    let document = error.api().unwrap().error().unwrap();
    assert_eq!(document.code.as_deref(), Some("email_invalid"));
    assert_eq!(document.param(), Some("email"));
    assert_eq!(
        error.to_string(),
        "Stripe.customers.create: Stripe returned HTTP 400 (invalid_request_error/email_invalid), \
         request req_SYNTHerr1"
    );
    assert!(!format!("{error:?}").contains("SYNTHETIC-SUBMITTED-VALUE"));
    assert_eq!(
        server.single().body_text().split('&').nth(1),
        Some("email=not-an-email")
    );
    server.shutdown().await;
}
