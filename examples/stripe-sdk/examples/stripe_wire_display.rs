//! Scalar query text shares serde spelling; writes still use StripeForm.

use concord::reqwest;
use concord_example_stripe_sdk::params::CustomerCreate;
use concord_example_stripe_sdk::{
    CollectionMethod, CustomerId, ExpandPath, IdempotencyKey, Limit, SecretKey, StripeContext,
    SubscriptionStatusFilter, Timestamp,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = SecretKey::new("sk_test_synthetic_example")?;
    let stripe = StripeContext::new(reqwest::Client::new(), &key);
    let subscriptions = stripe
        .api()
        .subscriptions()
        .list()
        .set_customer(CustomerId::new("cus_alpha")?)
        .set_status(SubscriptionStatusFilter::IncompleteExpired)
        .set_collection_method(CollectionMethod::SendInvoice)
        .set_created_gte(Timestamp(1767225600))
        .set_limit(Limit::new(25)?)
        .set_expand(vec![ExpandPath::new("data.customer")?])
        .prepare()?;
    assert_eq!(
        subscriptions.request().url().as_str(),
        "https://api.stripe.com/v1/subscriptions?customer=cus_alpha&status=incomplete_expired&collection_method=send_invoice&created%5Bgte%5D=1767225600&limit=25&expand%5B%5D=data.customer",
    );

    let create = stripe
        .api()
        .customers()
        .create(CustomerCreate {
            name: Some("Gold Plan & co".into()),
            ..CustomerCreate::default()
        })
        .set_idempotency_key(IdempotencyKey::new("create-customer-1")?)
        .prepare()?;
    assert_eq!(
        create.request().headers()["idempotency-key"],
        "create-customer-1"
    );
    assert!(create.request().headers()["idempotency-key"].is_sensitive());
    assert_eq!(
        create.request().body().and_then(|body| body.as_bytes()),
        Some(b"name=Gold+Plan+%26+co".as_slice()),
    );
    Ok(())
}
