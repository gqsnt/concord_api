//! Proposed I1 regression tests. Keep the existing form, wire, pages and canary suites intact.

use std::fmt::Display;

use concord_example_stripe_sdk::ids::*;
use concord_example_stripe_sdk::{
    ApiVersion, CollectionMethod, Currency, ExpandPath, IdempotencyKey, InvoiceStatus, Limit,
    PriceTypeFilter, SubscriptionStatusFilter, Timestamp,
};

fn text(value: impl Display, expected: &str) {
    assert_eq!(value.to_string(), expected);
}

#[test]
fn query_enums_follow_the_existing_serde_spellings() {
    text(SubscriptionStatusFilter::Active, "active");
    text(SubscriptionStatusFilter::All, "all");
    text(SubscriptionStatusFilter::Canceled, "canceled");
    text(SubscriptionStatusFilter::Ended, "ended");
    text(SubscriptionStatusFilter::Incomplete, "incomplete");
    text(
        SubscriptionStatusFilter::IncompleteExpired,
        "incomplete_expired",
    );
    text(SubscriptionStatusFilter::PastDue, "past_due");
    text(SubscriptionStatusFilter::Paused, "paused");
    text(SubscriptionStatusFilter::Trialing, "trialing");
    text(SubscriptionStatusFilter::Unpaid, "unpaid");
    text(PriceTypeFilter::OneTime, "one_time");
    text(PriceTypeFilter::Recurring, "recurring");
    text(
        CollectionMethod::ChargeAutomatically,
        "charge_automatically",
    );
    text(CollectionMethod::SendInvoice, "send_invoice");
    text(InvoiceStatus::Draft, "draft");
    text(InvoiceStatus::Open, "open");
    text(InvoiceStatus::Paid, "paid");
    text(InvoiceStatus::Uncollectible, "uncollectible");
    text(InvoiceStatus::Void, "void");
}

#[test]
fn ids_keep_validation_and_response_prefix_rules() {
    text(CustomerId::new("cus_one").unwrap(), "cus_one");
    text(SubscriptionId::new("sub_one").unwrap(), "sub_one");
    text(SubscriptionItemId::new("si_one").unwrap(), "si_one");
    text(InvoiceId::new("in_one").unwrap(), "in_one");
    text(InvoiceLineItemId::new("il_tmp_one").unwrap(), "il_tmp_one");
    text(ProductId::new("custom-product").unwrap(), "custom-product");
    text(PriceId::new("price_one").unwrap(), "price_one");
    text(BalanceTransactionId::new("txn_one").unwrap(), "txn_one");
    text(TaxRateId::new("txr_one").unwrap(), "txr_one");
    text(PaymentMethodId::new("pm_one").unwrap(), "pm_one");
    let account = AccountId::new("acct_SYNTHETIC").unwrap();
    text(&account, "acct_SYNTHETIC");
    assert_eq!(format!("{account:?}"), "AccountId(<redacted>)");
    assert_eq!(
        serde_json::to_string(&account).unwrap(),
        "\"acct_SYNTHETIC\""
    );
    let preview: InvoiceId = serde_json::from_str("\"upcoming_in_preview\"").unwrap();
    text(&preview, "upcoming_in_preview");
    assert!(InvoiceId::new("upcoming_in_preview").is_err());
    assert!(serde_json::from_str::<InvoiceId>("\"in_unsafe/path\"").is_err());
    assert!(serde_json::from_str::<Currency>("\"USD\"").is_err());
}

#[test]
fn scalars_are_raw_text_and_epoch_seconds_stay_numbers() {
    text(Currency::new("usd").unwrap(), "usd");
    text(ExpandPath::new("data.customer").unwrap(), "data.customer");
    text(ApiVersion::pinned(), "2026-10-28.endive");
    text(Limit::new(100).unwrap(), "100");
    assert_eq!(
        serde_json::to_string(&Limit::new(100).unwrap()).unwrap(),
        "100"
    );
    for (seconds, spelling) in [
        (0, "0"),
        (-1, "-1"),
        (1767225600, "1767225600"),
        (i64::MIN, "-9223372036854775808"),
        (i64::MAX, "9223372036854775807"),
    ] {
        text(Timestamp(seconds), spelling);
        assert_eq!(
            serde_json::to_string(&Timestamp(seconds)).unwrap(),
            spelling
        );
    }
    let idempotency = IdempotencyKey::new("explicit-application-key").unwrap();
    text(&idempotency, "explicit-application-key");
    assert_eq!(format!("{idempotency:?}"), "IdempotencyKey(<redacted>)");
}
