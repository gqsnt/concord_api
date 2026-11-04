//! ST-B against a live Stripe **sandbox**, only when explicitly enabled.
//!
//! Requires all of:
//!
//! - `STRIPE_SECRET_KEY`: a test-mode or sandbox key (`sk_test_...` / `rk_test_...`);
//! - `STRIPE_SANDBOX_MUTATIONS=1`: the explicit opt-in to create objects in that sandbox;
//! - optionally `STRIPE_TARGET_ACCOUNT=acct_...`: a connected account of the sandbox to provision
//!   (absent: the sandbox's own account).
//!
//! Otherwise it prints `NOT_RUN` with the reason and exits 0. Read-only smoke authorization is
//! not enough: this binary creates a product, a price, a customer and a subscription.

use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use concord_example_stripe_app::provisioning::{
    IdempotencyPolicy, KeyLedger, PricePlan, ProvisioningPlan, SubscriptionPlan, provision,
};
use concord_example_stripe_sdk::models::Interval;
use concord_example_stripe_sdk::params::{
    CustomerCreate, CustomerUpdate, ProductCreate, RecurringParams,
};
use concord_example_stripe_sdk::{
    AccountId, AccountOverride, Currency, Metadata, MetadataPatch, Patch, SecretKey, StripeContext,
};

fn main() -> ExitCode {
    let key = match std::env::var("STRIPE_SECRET_KEY") {
        Ok(key) => key,
        Err(_) => return not_run("STRIPE_SECRET_KEY is not set"),
    };
    let Ok(key) = SecretKey::new(key) else {
        return not_run("STRIPE_SECRET_KEY is not a Stripe secret key");
    };
    if !key.is_test_mode() {
        return not_run("ST-B creates objects and only runs with a test-mode/sandbox key");
    }
    if std::env::var("STRIPE_SANDBOX_MUTATIONS").as_deref() != Ok("1") {
        return not_run("STRIPE_SANDBOX_MUTATIONS=1 (sandbox mutation opt-in) is not set");
    }
    let target = match std::env::var("STRIPE_TARGET_ACCOUNT") {
        Ok(account) => match AccountId::new(account) {
            Ok(account) => AccountOverride::Account(account),
            Err(error) => {
                eprintln!("FAILED: STRIPE_TARGET_ACCOUNT: {error}");
                return ExitCode::FAILURE;
            }
        },
        Err(_) => AccountOverride::Inherit,
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("FAILED: runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let stripe = StripeContext::new(concord::reqwest::Client::new(), &key);
    // The application's operation id: one run, one set of keys.
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let Ok(policy) = IdempotencyPolicy::derived(&format!("concord-st-b-{started}"), 1) else {
        eprintln!("FAILED: idempotency keys");
        return ExitCode::FAILURE;
    };
    let plan = ProvisioningPlan {
        target,
        product: ProductCreate::new("concord ST-B sandbox product"),
        price: PricePlan {
            currency: Currency::new("eur").expect("valid currency"),
            unit_amount: 1200,
            recurring: Some(RecurringParams {
                interval: Interval::Month,
                interval_count: None,
            }),
            lookup_key: None,
            metadata: Metadata::new(),
        },
        customer: CustomerCreate {
            name: Some("concord ST-B sandbox customer".to_owned()),
            preferred_locales: vec!["fr-FR".to_owned(), "en".to_owned()],
            metadata: Metadata::from([("source".to_owned(), "concord-st-b".to_owned())]),
            ..CustomerCreate::default()
        },
        customer_update: CustomerUpdate {
            description: Some(Patch::Unset),
            metadata: Some(MetadataPatch::new().set("plan", "sandbox")),
            ..CustomerUpdate::default()
        },
        subscription: SubscriptionPlan {
            quantity: Some(1),
            ..SubscriptionPlan::default()
        },
        platform_link: None,
    };
    let trace = runtime.block_on(provision(&stripe, &plan, &policy, &mut KeyLedger::new()));
    println!("{trace:#?}");
    if trace.is_complete() {
        println!(
            "LIVE_PROVIDER_RUN: ST-B completed against the sandbox (version {})",
            stripe.version()
        );
        ExitCode::SUCCESS
    } else {
        println!("FAILED: ST-B stopped at {:?}", trace.stopped_at);
        ExitCode::FAILURE
    }
}

fn not_run(reason: &str) -> ExitCode {
    println!("NOT_RUN: ST-B live sandbox run skipped: {reason}");
    ExitCode::SUCCESS
}
