//! ST-B end to end against the loopback server and the stateful fake in
//! `support/provisioning_fake.rs`: the wire form of every write, the per-request account
//! override, a response lost after the server accepted a write and its unchanged repeat, and
//! changed parameters under a reused key (prevented by the application's ledger, rejected by the
//! server when the ledger is not shared).
//!
//! Run: `cargo test -p concord-example-stripe-app --test st_b_form_idempotency`.

#[path = "support/provisioning_fake.rs"]
mod provisioning_fake;

use std::collections::BTreeMap;

use concord_example_stripe_app::provisioning::{
    Attempt, IdempotencyPolicy, KeyLedger, PlatformLink, Prevented, PricePlan, ProvisioningPlan,
    Step, StepResult, SubscriptionPlan, provision,
};
use concord_example_stripe_sdk::models::{Address, Interval, MarketingFeature};
use concord_example_stripe_sdk::params::{
    CustomerCreate, CustomerUpdate, ProductCreate, RecurringParams,
};
use concord_example_stripe_sdk::send::ApiErrorType;
use concord_example_stripe_sdk::{
    AccountId, AccountOverride, Currency, CustomerId, ExpandPath, IdempotencyKey, Metadata,
    MetadataPatch, Patch, SecretKey, StripeContext,
};
use concord_example_testserver::{Recorded, TestServer};
use provisioning_fake::FakeStripe;

const KEY: &str = "sk_test_SYNTHETIC_stb_secret";

async fn start() -> (TestServer, FakeStripe) {
    let fake = FakeStripe::new(KEY);
    let responder = fake.clone();
    let server = TestServer::start(move |request| responder.respond(request)).await;
    (server, fake)
}

fn context(server: &TestServer, account: Option<&str>) -> StripeContext {
    StripeContext::new(
        concord::reqwest::Client::new(),
        &SecretKey::new(KEY).unwrap(),
    )
    .with_base_url(format!("{}/v1", server.url()))
    .unwrap()
    .with_account(account.map(|a| AccountId::new(a).unwrap()))
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn plan(target: AccountOverride, unit_amount: i64) -> ProvisioningPlan {
    let mut product = ProductCreate::new("Gold Plan");
    product.marketing_features = vec![
        MarketingFeature {
            name: "Priority support".to_owned(),
        },
        MarketingFeature {
            name: "SSO".to_owned(),
        },
    ];
    product.metadata = Metadata::from([("catalog_ref".to_owned(), "gold-2026".to_owned())]);
    ProvisioningPlan {
        target,
        product,
        price: PricePlan {
            currency: Currency::new("eur").unwrap(),
            unit_amount,
            recurring: Some(RecurringParams {
                interval: Interval::Month,
                interval_count: None,
            }),
            lookup_key: None,
            metadata: Metadata::new(),
        },
        customer: CustomerCreate {
            name: Some("Synthetic Customer".to_owned()),
            email: Some("billing@example.test".to_owned()),
            description: Some("imported from crm".to_owned()),
            address: Some(Address {
                line1: Some("1 Rue Exemple".to_owned()),
                city: Some("Lyon".to_owned()),
                country: Some("FR".to_owned()),
                ..Address::default()
            }),
            preferred_locales: vec!["fr-FR".to_owned(), "en".to_owned()],
            metadata: Metadata::from([
                ("crm_id".to_owned(), "crm-0042".to_owned()),
                ("legacy_id".to_owned(), "L-9".to_owned()),
            ]),
            ..CustomerCreate::default()
        },
        customer_update: CustomerUpdate {
            description: Some(Patch::Unset),
            metadata: Some(MetadataPatch::new().unset("legacy_id").set("plan", "gold")),
            ..CustomerUpdate::default()
        },
        subscription: SubscriptionPlan {
            quantity: Some(2),
            metadata: Metadata::from([("order_id".to_owned(), "6735".to_owned())]),
            default_tax_rates: Vec::new(),
            expand: vec![
                ExpandPath::new("latest_invoice").unwrap(),
                ExpandPath::new("customer").unwrap(),
            ],
        },
        platform_link: Some(PlatformLink {
            customer: CustomerId::new("cus_PLATFORMmirror").unwrap(),
            metadata_key: "connected_customer".to_owned(),
        }),
    }
}

fn accounts(requests: &[Recorded]) -> Vec<Option<&str>> {
    requests
        .iter()
        .map(|r| r.header("stripe-account"))
        .collect()
}

#[tokio::test]
async fn st_b_provisions_with_forms_account_override_and_a_lost_response() {
    let (server, fake) = start().await;
    fake.lose_response_once("/v1/subscriptions");
    // The context acts on acct_DEFAULT; the plan targets acct_TARGET per request.
    let stripe = context(&server, Some("acct_DEFAULT"));
    let target = AccountOverride::Account(AccountId::new("acct_TARGET").unwrap());
    let policy = IdempotencyPolicy::derived("onboarding-7f3a", 1).unwrap();
    let mut ledger = KeyLedger::new();

    let trace = provision(&stripe, &plan(target, 1200), &policy, &mut ledger).await;
    assert!(trace.is_complete(), "{trace:?}");
    assert!(trace.error.is_none());

    // Exactly which writes were attempted and recognized.
    let summary: Vec<(Step, Vec<Attempt>, bool)> = trace
        .writes
        .iter()
        .map(|w| {
            let replayed = matches!(w.result, StepResult::Recognized { replayed: true, .. });
            (w.step, w.attempts.clone(), replayed)
        })
        .collect();
    let accepted = |id: &str| Attempt::Accepted {
        request_id: Some(id.to_owned()),
        replayed: false,
    };
    assert_eq!(
        summary,
        vec![
            (Step::CreateProduct, vec![accepted("req_fake0001")], false),
            (Step::CreatePrice, vec![accepted("req_fake0002")], false),
            (Step::CreateCustomer, vec![accepted("req_fake0003")], false),
            (Step::UpdateCustomer, vec![accepted("req_fake0004")], false),
            (
                Step::CreateSubscription,
                vec![
                    Attempt::ResponseLost,
                    Attempt::Accepted {
                        request_id: Some("req_fake0006".to_owned()),
                        replayed: true,
                    }
                ],
                true
            ),
            (
                Step::LinkPlatformCustomer,
                vec![accepted("req_fake0007")],
                false
            ),
        ]
    );
    let provisioned = &trace.provisioned;
    assert_eq!(provisioned.product.as_ref().unwrap().as_str(), "prod_F0001");
    assert_eq!(provisioned.price.as_ref().unwrap().as_str(), "price_F0002");
    assert_eq!(provisioned.customer.as_ref().unwrap().as_str(), "cus_F0003");
    assert_eq!(
        provisioned.subscription.as_ref().unwrap().as_str(),
        "sub_F0005"
    );
    assert_eq!(
        provisioned.latest_invoice.as_ref().unwrap().as_str(),
        "in_F0005"
    );

    // The server executed one subscription although two requests carried it.
    let executed: Vec<String> = fake.executed().into_iter().map(|e| e.path).collect();
    assert_eq!(
        executed,
        [
            "/v1/products",
            "/v1/prices",
            "/v1/customers",
            "/v1/customers/cus_F0003",
            "/v1/subscriptions",
            "/v1/customers/cus_PLATFORMmirror"
        ]
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 7);
    // Per-request account: the target for steps 1-5 (and the repeat), none for the platform link;
    // the context's acct_DEFAULT never appears.
    assert_eq!(
        accounts(&requests),
        [
            Some("acct_TARGET"),
            Some("acct_TARGET"),
            Some("acct_TARGET"),
            Some("acct_TARGET"),
            Some("acct_TARGET"),
            Some("acct_TARGET"),
            None
        ]
    );
    let keys: Vec<Option<&str>> = requests
        .iter()
        .map(|r| r.header("idempotency-key"))
        .collect();
    assert_eq!(
        keys,
        [
            Some("onboarding-7f3a/create-product"),
            Some("onboarding-7f3a/create-price"),
            Some("onboarding-7f3a/create-customer"),
            Some("onboarding-7f3a/update-customer"),
            Some("onboarding-7f3a/create-subscription"),
            Some("onboarding-7f3a/create-subscription"),
            Some("onboarding-7f3a/link-platform-customer"),
        ]
    );
    for request in &requests {
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(request.header("stripe-version"), Some("2026-10-28.endive"));
    }

    // Nested, indexed and repeated fields, as the reference spells them.
    assert_eq!(
        requests[0].form_pairs(),
        pairs(&[
            ("name", "Gold Plan"),
            ("marketing_features[0][name]", "Priority support"),
            ("marketing_features[1][name]", "SSO"),
            ("metadata[catalog_ref]", "gold-2026"),
        ])
    );
    assert_eq!(
        requests[1].form_pairs(),
        pairs(&[
            ("currency", "eur"),
            ("product", "prod_F0001"),
            ("unit_amount", "1200"),
            ("recurring[interval]", "month"),
        ])
    );
    assert_eq!(
        requests[2].form_pairs(),
        pairs(&[
            ("name", "Synthetic Customer"),
            ("email", "billing@example.test"),
            ("description", "imported from crm"),
            ("address[city]", "Lyon"),
            ("address[country]", "FR"),
            ("address[line1]", "1 Rue Exemple"),
            ("preferred_locales[]", "fr-FR"),
            ("preferred_locales[]", "en"),
            ("metadata[crm_id]", "crm-0042"),
            ("metadata[legacy_id]", "L-9"),
        ])
    );
    // Omitted (name, email: absent) versus empty (description, one metadata key: unset).
    assert_eq!(requests[3].path(), "/v1/customers/cus_F0003");
    assert_eq!(
        requests[3].form_pairs(),
        pairs(&[
            ("description", ""),
            ("metadata[legacy_id]", ""),
            ("metadata[plan]", "gold"),
        ])
    );
    assert_eq!(
        requests[3].body_text(),
        "description=&metadata%5Blegacy_id%5D=&metadata%5Bplan%5D=gold"
    );
    assert_eq!(
        requests[4].form_pairs(),
        pairs(&[
            ("customer", "cus_F0003"),
            ("items[0][price]", "price_F0002"),
            ("items[0][quantity]", "2"),
            ("metadata[order_id]", "6735"),
            ("expand[]", "latest_invoice"),
            ("expand[]", "customer"),
        ])
    );
    // The repeat is the unchanged request: same body bytes, same key, same account.
    assert_eq!(requests[5].body, requests[4].body);
    assert_eq!(
        requests[6].form_pairs(),
        pairs(&[("metadata[connected_customer]", "cus_F0003")])
    );

    // Diagnostics: no key or account text in the trace's Debug.
    let shown = format!("{trace:?} {policy:?} {ledger:?}");
    for secret in ["onboarding-7f3a", "acct_TARGET", "acct_DEFAULT", KEY] {
        assert!(!shown.contains(secret), "{secret} in {shown}");
    }
    assert_eq!(ledger.len(), 6);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn st_b_reused_key_with_changed_parameters_is_prevented_then_rejected() {
    let (server, fake) = start().await;
    let stripe = context(&server, None);
    let target = AccountOverride::Account(AccountId::new("acct_TARGET").unwrap());
    let policy = IdempotencyPolicy::derived("onboarding-b21c", 1).unwrap();
    let mut ledger = KeyLedger::new();
    let first = provision(&stripe, &plan(target.clone(), 1200), &policy, &mut ledger).await;
    assert!(first.is_complete(), "{first:?}");
    let sent_after_first = server.requests().len();

    // Same keys, same ledger, a changed price: the product step is the unchanged request
    // (replayed), the price step is refused before sending.
    let changed = plan(target.clone(), 1500);
    let second = provision(&stripe, &changed, &policy, &mut ledger).await;
    assert_eq!(second.stopped_at, Some(Step::CreatePrice));
    assert_eq!(
        second.write(Step::CreateProduct).unwrap().result,
        StepResult::Recognized {
            id: "prod_F0001".to_owned(),
            replayed: true
        }
    );
    let price = second.write(Step::CreatePrice).unwrap();
    assert!(price.attempts.is_empty());
    assert_eq!(
        price.result,
        StepResult::Prevented(Prevented::KeyReusedWithDifferentRequest)
    );
    assert_eq!(server.requests().len(), sent_after_first + 1);

    // Another process without the ledger: Stripe's own check rejects the changed request.
    let mut fresh = KeyLedger::new();
    let third = provision(&stripe, &changed, &policy, &mut fresh).await;
    assert_eq!(third.stopped_at, Some(Step::CreatePrice));
    let price = third.write(Step::CreatePrice).unwrap();
    assert_eq!(price.result, StepResult::Rejected);
    match &price.attempts[..] {
        [
            Attempt::Rejected {
                status: 400,
                error_type: Some(ApiErrorType::IdempotencyError),
                request_id: Some(_),
                ..
            },
        ] => {}
        other => panic!("unexpected attempts {other:?}"),
    }
    let error = third.error.as_ref().unwrap();
    assert!(error.is_idempotency_error());
    // The server's message quotes the key; the error output does not.
    let shown = format!("{error} {error:?} {third:?}");
    assert!(!shown.contains("onboarding-b21c"), "{shown}");
    assert!(
        error
            .api()
            .unwrap()
            .error()
            .unwrap()
            .message()
            .unwrap()
            .contains("onboarding-b21c/create-price")
    );
    // No second price was ever executed.
    let prices = fake
        .executed()
        .iter()
        .filter(|e| e.path == "/v1/prices")
        .count();
    assert_eq!(prices, 1);
    server.shutdown().await;
}

#[tokio::test]
async fn st_b_missing_key_and_unrepeated_lost_response() {
    let (server, fake) = start().await;
    let stripe = context(&server, Some("acct_BETA"));

    // A policy without a key for the customer: that write is not sent.
    let full = IdempotencyPolicy::derived("onboarding-c0de", 0).unwrap();
    let keys: BTreeMap<Step, IdempotencyKey> = Step::ALL
        .into_iter()
        .filter(|step| *step != Step::CreateCustomer)
        .map(|step| (step, full.key(step).unwrap().clone()))
        .collect();
    let partial = IdempotencyPolicy::new(keys, 0);
    let trace = provision(
        &stripe,
        &plan(AccountOverride::Inherit, 1200),
        &partial,
        &mut KeyLedger::new(),
    )
    .await;
    assert_eq!(trace.stopped_at, Some(Step::CreateCustomer));
    assert_eq!(
        trace.write(Step::CreateCustomer).unwrap().result,
        StepResult::Prevented(Prevented::MissingKey)
    );
    assert_eq!(trace.attempted().count(), 2);
    // Inherit: the context's account on every write.
    assert_eq!(
        accounts(&server.requests()),
        [Some("acct_BETA"), Some("acct_BETA")]
    );

    // No repeat allowed: a lost response leaves the step unresolved, although the server
    // executed it. The trace says the outcome is unknown; nothing claims exactly-once.
    fake.lose_response_once("/v1/subscriptions");
    let trace = provision(
        &stripe,
        &plan(AccountOverride::Inherit, 1200),
        &IdempotencyPolicy::derived("onboarding-d00d", 0).unwrap(),
        &mut KeyLedger::new(),
    )
    .await;
    assert_eq!(trace.stopped_at, Some(Step::CreateSubscription));
    let subscription = trace.write(Step::CreateSubscription).unwrap();
    assert_eq!(subscription.attempts, [Attempt::ResponseLost]);
    assert_eq!(subscription.result, StepResult::Unresolved);
    assert!(trace.error.as_ref().unwrap().is_response_lost());
    assert!(trace.provisioned.subscription.is_none());
    assert!(
        fake.executed()
            .iter()
            .any(|e| e.path == "/v1/subscriptions")
    );
    server.shutdown().await;
}
