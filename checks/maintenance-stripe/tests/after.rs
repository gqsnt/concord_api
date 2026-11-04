//! The updated consumer ([`after`]) beside the frozen one ([`before`]), against the changed SDK.
//!
//! Evidence kept here: omitted-field wire compatibility (the frozen consumer and the updated one
//! with the new field omitted send the frozen literal bytes), the exact bytes when the nested
//! field and the expansion are requested, the id/object forms of the expansion on two accounts,
//! and validation errors (local value refusals and a Stripe parameter error).
//!
//! Fixtures are synthetic, shaped after Stripe's reference for `2026-10-28.endive` (customer
//! object with `invoice_settings`, <https://docs.stripe.com/api/customers/object>; payment
//! method object, <https://docs.stripe.com/api/payment_methods/object>; both retrieved
//! 2026-11-05). The fake decides expansion from the request (`expand[]` in the query or the
//! form body) and only for the documented spelling of each operation (`data.` on lists).

use concord_check_maintenance_stripe::after::{
    self, BillingProfile, DEFAULT_PAYMENT_METHOD, DefaultPayment,
};
use concord_check_maintenance_stripe::before::{self, CrmContact};
use concord_example_stripe_sdk::ids::IdErrorReason;
use concord_example_stripe_sdk::{
    AccountId, CustomerId, ExpandPath, IdempotencyKey, Limit, PaymentMethodId, SecretKey,
    StripeContext, ValueError,
};
use concord_example_testserver::{Recorded, Reply, Script, TestServer};

const KEY: &str = "sk_test_SYNTHETIC_maintenance";

/// The frozen consumer's create body (copied from `tests/before.rs`, which is frozen).
const ADA_CREATE_BODY: &str = "name=Ada+Lovelace&email=ada%40example.test\
    &address%5Bcity%5D=London&address%5Bcountry%5D=GB&preferred_locales%5B%5D=en-GB\
    &metadata%5Bcrm_id%5D=crm-0001";
const GRACE_CREATE_BODY: &str = "name=Grace+Hopper&email=grace%2Bbilling%40example.test\
    &address%5Bcountry%5D=US&metadata%5Bcrm_id%5D=crm-0002";
/// The same create with the new nested field: one pair more, in declaration order (after
/// `preferred_locales`, before `metadata`), every other byte unchanged.
const ADA_CREATE_WITH_DEFAULT_BODY: &str = "name=Ada+Lovelace&email=ada%40example.test\
    &address%5Bcity%5D=London&address%5Bcountry%5D=GB&preferred_locales%5B%5D=en-GB\
    &invoice_settings%5Bdefault_payment_method%5D=pm_SYNTHGB02\
    &metadata%5Bcrm_id%5D=crm-0001";
const SET_DEFAULT_BODY: &str = "invoice_settings%5Bdefault_payment_method%5D=pm_SYNTHGB02\
    &expand%5B%5D=invoice_settings.default_payment_method";

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

/// A payment method object: `pm_SYNTHGB..` are cards, others SEPA debits.
fn payment_method_json(id: &str, customer: &str) -> String {
    let details = if id.starts_with("pm_SYNTHGB") {
        r#""type":"card","card":{"brand":"visa","checks":null,"country":"GB","display_brand":"visa",
        "exp_month":4,"exp_year":2031,"fingerprint":"SYNTHfp","funding":"credit","last4":"4242",
        "networks":null,"regulated_status":"unregulated","three_d_secure_usage":{"supported":true},"wallet":null}"#
    } else {
        r#""type":"sepa_debit","sepa_debit":{"country":"DE","last4":"3000"}"#
    };
    format!(
        r#"{{"id":"{id}","object":"payment_method","allow_redisplay":"unspecified",
        "billing_details":{{"address":{{"city":null,"country":null,"line1":null,"line2":null,"postal_code":null,"state":null}},"email":null,"name":null,"phone":null}},
        "created":1767225600,"customer":"{customer}","livemode":false,"metadata":{{}},{details}}}"#
    )
}

fn customer_json(account: &str, default_payment_method: &str) -> String {
    let (id, email, crm) = match account {
        "acct_ALPHA" => ("cus_ALPHA0001", "ada@example.test", "crm-0001"),
        _ => ("cus_BETA0001", "grace+billing@example.test", "crm-0002"),
    };
    format!(
        r#"{{"id":"{id}","object":"customer","address":null,"balance":0,"created":1767225600,
        "currency":null,"default_source":null,"delinquent":false,"description":null,"email":"{email}",
        "invoice_prefix":"0759376C",
        "invoice_settings":{{"custom_fields":null,"default_payment_method":{default_payment_method},"footer":null,"rendering_options":null}},
        "livemode":false,"metadata":{{"crm_id":"{crm}"}},"name":null,"next_invoice_sequence":1,
        "phone":null,"preferred_locales":[],"shipping":null,"tax_exempt":"none","test_clock":null}}"#
    )
}

fn fake(request: &Recorded) -> Reply {
    let account = request.header("stripe-account").unwrap_or("platform");
    let customer_id = if account == "acct_ALPHA" {
        "cus_ALPHA0001"
    } else {
        "cus_BETA0001"
    };
    let list = request.method == "GET" && request.path() == "/v1/customers";
    let (expand, chosen) = if request.method == "POST" {
        let pairs = request.form_pairs();
        let value = |name: &str| -> Vec<String> {
            pairs
                .iter()
                .filter(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
                .collect()
        };
        let chosen = value("invoice_settings[default_payment_method]").pop();
        (value("expand[]"), chosen)
    } else {
        (request.query_all("expand[]"), None)
    };
    let method = chosen.unwrap_or_else(|| {
        if account == "acct_ALPHA" {
            "pm_SYNTHGB01".to_owned()
        } else {
            "pm_SYNTHDE01".to_owned()
        }
    });
    let wanted = if list {
        "data.invoice_settings.default_payment_method"
    } else {
        "invoice_settings.default_payment_method"
    };
    let value = if expand.iter().any(|e| e == wanted) {
        payment_method_json(&method, customer_id)
    } else {
        format!("\"{method}\"")
    };
    let customer = customer_json(account, &value);
    let body = if list {
        format!(r#"{{"object":"list","url":"/v1/customers","has_more":false,"data":[{customer}]}}"#)
    } else {
        customer
    };
    Reply::json(200, body).header("request-id", "req_SYNTHafter")
}

#[tokio::test]
async fn omitted_field_keeps_frozen_bytes_and_set_field_adds_one_pair() {
    let server = TestServer::start(fake).await;
    let alpha = context(&server, "acct_ALPHA");
    let beta = context(&server, "acct_BETA");

    // Same logical write through the frozen consumer and the updated one without the new field.
    before::mirror_contact(&alpha, &ada(), key("crm-0001-mirror"))
        .await
        .unwrap();
    let without = after::mirror_contact_with_default(&alpha, &ada(), None, key("crm-0001-mirror"))
        .await
        .unwrap();
    assert_eq!(
        without.default_payment,
        DefaultPayment::IdOnly(PaymentMethodId::new("pm_SYNTHGB01").unwrap())
    );
    let with = after::mirror_contact_with_default(
        &alpha,
        &ada(),
        Some(PaymentMethodId::new("pm_SYNTHGB02").unwrap()),
        key("crm-0001-mirror-with-default"),
    )
    .await
    .unwrap();
    // Not expanded on create: the application gets the id it sent back.
    assert_eq!(
        with.default_payment,
        DefaultPayment::IdOnly(PaymentMethodId::new("pm_SYNTHGB02").unwrap())
    );
    after::mirror_contact_with_default(&beta, &grace(), None, key("crm-0002-mirror"))
        .await
        .unwrap();

    let requests = server.requests();
    let bodies: Vec<String> = requests.iter().map(Recorded::body_text).collect();
    assert_eq!(
        bodies,
        [
            ADA_CREATE_BODY,
            ADA_CREATE_BODY,
            ADA_CREATE_WITH_DEFAULT_BODY,
            GRACE_CREATE_BODY
        ]
    );
    // A replay under the frozen consumer's key with the updated consumer is the same request:
    // identical bytes, identical key.
    assert_eq!(requests[0].body, requests[1].body);
    assert_eq!(
        requests[0].header("idempotency-key"),
        requests[1].header("idempotency-key")
    );
    assert_eq!(
        requests[2]
            .form_pairs()
            .into_iter()
            .filter(|(k, _)| k.starts_with("invoice_settings"))
            .collect::<Vec<_>>(),
        [(
            "invoice_settings[default_payment_method]".to_owned(),
            "pm_SYNTHGB02".to_owned()
        )]
    );
    let accounts: Vec<_> = requests
        .iter()
        .map(|r| r.header("stripe-account"))
        .collect();
    assert_eq!(
        accounts,
        [
            Some("acct_ALPHA"),
            Some("acct_ALPHA"),
            Some("acct_ALPHA"),
            Some("acct_BETA")
        ]
    );
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[tokio::test]
async fn nested_update_and_typed_expansions_on_two_accounts() {
    let server = TestServer::start(fake).await;
    let alpha = context(&server, "acct_ALPHA");
    let beta = context(&server, "acct_BETA");
    let ada_id = CustomerId::new("cus_ALPHA0001").unwrap();

    let updated = after::set_default_payment_method(
        &alpha,
        &ada_id,
        PaymentMethodId::new("pm_SYNTHGB02").unwrap(),
        key("crm-0001-default-pm"),
    )
    .await
    .unwrap();
    assert_eq!(
        updated,
        BillingProfile {
            customer: ada_id.clone(),
            crm_id: Some("crm-0001".to_owned()),
            default_payment: DefaultPayment::Card {
                id: PaymentMethodId::new("pm_SYNTHGB02").unwrap(),
                brand: "visa".to_owned(),
                last4: "4242".to_owned(),
                expires: "04/2031".to_owned(),
            },
        }
    );
    let sepa = after::billing_profile(&beta, &CustomerId::new("cus_BETA0001").unwrap())
        .await
        .unwrap();
    assert_eq!(
        sepa.default_payment,
        DefaultPayment::Other {
            id: PaymentMethodId::new("pm_SYNTHDE01").unwrap(),
            kind: "sepa_debit".to_owned(),
        }
    );
    let listed =
        after::billing_profiles_by_email(&alpha, "ada@example.test", Limit::new(5).unwrap())
            .await
            .unwrap();
    assert!(matches!(
        listed[0].default_payment,
        DefaultPayment::Card { ref last4, .. } if last4 == "4242"
    ));
    // The frozen reader keeps working on the same responses (it ignores the new member).
    let frozen = before::fetch_profile(&alpha, &ada_id).await.unwrap();
    assert_eq!(frozen.crm_id.as_deref(), Some("crm-0001"));

    let requests = server.requests();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].target, "/v1/customers/cus_ALPHA0001");
    assert_eq!(requests[0].body_text(), SET_DEFAULT_BODY);
    assert_eq!(
        requests[0].header("idempotency-key"),
        Some("crm-0001-default-pm")
    );
    assert_eq!(
        requests[1].target,
        "/v1/customers/cus_BETA0001?expand%5B%5D=invoice_settings.default_payment_method"
    );
    assert_eq!(
        requests[2].target,
        "/v1/customers?email=ada%40example.test&limit=5\
         &expand%5B%5D=data.invoice_settings.default_payment_method"
    );
    assert_eq!(requests[3].target, "/v1/customers/cus_ALPHA0001");
    let accounts: Vec<_> = requests
        .iter()
        .map(|r| r.header("stripe-account"))
        .collect();
    assert_eq!(
        accounts,
        [
            Some("acct_ALPHA"),
            Some("acct_BETA"),
            Some("acct_ALPHA"),
            Some("acct_ALPHA")
        ]
    );
    for request in &requests {
        assert_eq!(request.header("stripe-version"), Some("2026-10-28.endive"));
    }
    let cleanup = server.shutdown().await;
    assert_eq!(cleanup.open_after, 0);
}

#[test]
fn local_validation_of_the_new_values() {
    // A legacy card id where a PaymentMethod id is required: refused before any request.
    let legacy = PaymentMethodId::new("card_SYNTHlegacy").unwrap_err();
    assert_eq!(legacy.reason(), IdErrorReason::Prefix("pm_"));
    assert_eq!(
        legacy.to_string(),
        "payment method id must start with `pm_`"
    );
    let spaced = PaymentMethodId::new("pm_with space").unwrap_err();
    assert_eq!(spaced.reason(), IdErrorReason::Character);

    // Five segments: over the documented four-level limit, refused locally.
    let deep =
        ExpandPath::new("data.invoice_settings.default_payment_method.customer.invoice_settings")
            .unwrap_err();
    assert_eq!(deep, ValueError::ExpandPath);
    assert_eq!(
        deep.to_string(),
        "expansion path must have 1-4 non-empty dot-separated segments"
    );

    // Not refused locally: the single-object spelling on a list (Stripe documents that list
    // expansions start with `data`). Nothing ties a path to the operation or to the field it
    // expands; the caller has to know the rule.
    let key = SecretKey::new(KEY).unwrap();
    let stripe = StripeContext::new(concord::reqwest::Client::new(), &key);
    let prepared = stripe
        .api()
        .customers()
        .list()
        .set_expand(vec![ExpandPath::new(DEFAULT_PAYMENT_METHOD).unwrap()])
        .prepare()
        .unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "https://api.stripe.com/v1/customers?expand%5B%5D=invoice_settings.default_payment_method"
    );
}

#[tokio::test]
async fn stripe_parameter_error_names_the_bracketed_wire_key() {
    // Synthetic. `resource_missing` is a documented code; that `param` carries the bracketed
    // form key is an inference from Stripe's form spelling, not shown on the errors page.
    let script = Script::new([Reply::json(
        400,
        r#"{"error":{"type":"invalid_request_error","code":"resource_missing",
        "param":"invoice_settings[default_payment_method]",
        "message":"No such PaymentMethod: 'pm_SYNTHMISSING'"}}"#,
    )
    .header("request-id", "req_SYNTHerr2")]);
    let server = TestServer::start(move |_| script.next()).await;
    let alpha = context(&server, "acct_ALPHA");

    let error = after::set_default_payment_method(
        &alpha,
        &CustomerId::new("cus_ALPHA0001").unwrap(),
        PaymentMethodId::new("pm_SYNTHMISSING").unwrap(),
        key("crm-0001-default-pm-missing"),
    )
    .await
    .unwrap_err();
    let document = error.api().unwrap().error().unwrap();
    assert_eq!(document.code.as_deref(), Some("resource_missing"));
    // To relate this to `InvoiceSettingsParams::default_payment_method`, the caller compares
    // with the encoder's spelling; the SDK offers no mapping back to the Rust field.
    assert_eq!(
        document.param(),
        Some("invoice_settings[default_payment_method]")
    );
    let shown = format!("{error} {error:?}");
    assert!(!shown.contains("pm_SYNTHMISSING"), "{shown}");
    assert_eq!(
        error.to_string(),
        "Stripe.customers.update: Stripe returned HTTP 400 (invalid_request_error/resource_missing), \
         request req_SYNTHerr2"
    );
    server.shutdown().await;
}
