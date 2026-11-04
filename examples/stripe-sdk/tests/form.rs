//! Stripe form encoding, checked against the request spellings in Stripe's API reference
//! (curl examples retrieved 2026-11-04, recorded in `PROVIDER.md`):
//!
//! - `-d "items[0][price]={{PRICE_ID}}"` (Create a subscription)
//! - `-d "metadata[order_id]=6735"` (Update a subscription, Metadata)
//! - `-d "recurring[interval]=month" -d "product_data[name]=Gold Plan"` (Create a price)
//! - `-d "expand[]"=customer -d "expand[]"="payment_intent.customer"` (Expanding responses)
//! - "Individual keys can be unset by posting an empty value to them. All keys can be unset by
//!   posting an empty value to `metadata`." / "Pass an empty string to remove previously-defined
//!   tax rates." (Update a subscription)
//!
//! The expectations are written as literal pairs from those examples, not derived from the
//! encoder.

use std::collections::BTreeMap;

use concord_example_stripe_sdk::form::{FormError, encode_pairs, to_pairs};
use concord_example_stripe_sdk::models::{Address, Interval};
use concord_example_stripe_sdk::params::{
    CustomerUpdate, NewProductData, PriceCreate, PriceProduct, RecurringParams, SubscriptionCreate,
    SubscriptionItemCreate, SubscriptionUpdate,
};
use concord_example_stripe_sdk::{
    CollectionMethod, Currency, CustomerId, ExpandPath, Metadata, MetadataPatch, Patch, PriceId,
    ProductId, TaxRateId,
};
use serde::Serialize;

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn nested_indexed_and_repeated_fields_follow_the_documented_spelling() {
    let mut item = SubscriptionItemCreate::new(PriceId::new("price_gold").unwrap());
    item.quantity = Some(2);
    let mut params = SubscriptionCreate::new(
        CustomerId::new("cus_alpha").unwrap(),
        vec![
            item,
            SubscriptionItemCreate::new(PriceId::new("price_seat").unwrap()),
        ],
    );
    params.metadata = Metadata::from([("order_id".to_owned(), "6735".to_owned())]);
    params.default_tax_rates = vec![
        TaxRateId::new("txr_one").unwrap(),
        TaxRateId::new("txr_two").unwrap(),
    ];
    params.collection_method = Some(CollectionMethod::SendInvoice);
    params.days_until_due = Some(30);
    params.expand = vec![
        ExpandPath::new("latest_invoice").unwrap(),
        ExpandPath::new("customer").unwrap(),
    ];

    assert_eq!(
        to_pairs(&params).unwrap(),
        pairs(&[
            ("customer", "cus_alpha"),
            ("items[0][price]", "price_gold"),
            ("items[0][quantity]", "2"),
            ("items[1][price]", "price_seat"),
            ("metadata[order_id]", "6735"),
            ("default_tax_rates[]", "txr_one"),
            ("default_tax_rates[]", "txr_two"),
            ("collection_method", "send_invoice"),
            ("days_until_due", "30"),
            ("expand[]", "latest_invoice"),
            ("expand[]", "customer"),
        ])
    );
}

#[test]
fn omitted_and_empty_values_are_distinct() {
    let update = CustomerUpdate {
        name: None,                      // omitted: unchanged
        description: Some(Patch::Unset), // empty: unset
        email: Some(Patch::Set("ops@example.test".to_owned())),
        address: Some(Patch::Set(Address {
            line1: Some("1 Rue Exemple".to_owned()),
            country: Some("FR".to_owned()),
            ..Address::default()
        })),
        preferred_locales: Some(Patch::Unset),
        metadata: Some(MetadataPatch::new().set("tier", "gold").unset("legacy_id")),
        ..CustomerUpdate::default()
    };
    let encoded = to_pairs(&update).unwrap();
    assert_eq!(
        encoded,
        pairs(&[
            ("email", "ops@example.test"),
            ("description", ""),
            ("address[country]", "FR"),
            ("address[line1]", "1 Rue Exemple"),
            ("preferred_locales", ""),
            ("metadata[legacy_id]", ""),
            ("metadata[tier]", "gold"),
        ])
    );
    assert!(!encoded.iter().any(|(k, _)| k == "name" || k == "phone"));

    // Clearing every metadata key and every default tax rate.
    let clear = SubscriptionUpdate {
        metadata: Some(MetadataPatch::ClearAll),
        default_tax_rates: Some(Patch::Unset),
        ..SubscriptionUpdate::default()
    };
    assert_eq!(
        to_pairs(&clear).unwrap(),
        pairs(&[("metadata", ""), ("default_tax_rates", "")])
    );
    // Nothing set: an empty body.
    assert!(to_pairs(&SubscriptionUpdate::default()).unwrap().is_empty());
}

#[test]
fn price_product_is_exactly_one_of_product_or_product_data() {
    let mut new_product = PriceCreate::new(
        Currency::new("usd").unwrap(),
        PriceProduct::New(NewProductData {
            name: "Gold Plan".to_owned(),
            metadata: Metadata::new(),
        }),
        1000,
    );
    new_product.recurring = Some(RecurringParams {
        interval: Interval::Month,
        interval_count: None,
    });
    assert_eq!(
        to_pairs(&new_product).unwrap(),
        pairs(&[
            ("currency", "usd"),
            ("product_data[name]", "Gold Plan"),
            ("unit_amount", "1000"),
            ("recurring[interval]", "month"),
        ])
    );
    let existing = PriceCreate::new(
        Currency::new("eur").unwrap(),
        PriceProduct::Existing(ProductId::new("prod_gold").unwrap()),
        900,
    );
    assert_eq!(
        to_pairs(&existing).unwrap(),
        pairs(&[
            ("currency", "eur"),
            ("product", "prod_gold"),
            ("unit_amount", "900")
        ])
    );
}

#[test]
fn bytes_are_form_urlencoded() {
    let body = encode_pairs(&pairs(&[
        ("metadata[order_id]", "6735"),
        ("name", "Gold Plan & co = 100%"),
        ("description", "café"),
        ("expand[]", "data.customer"),
        ("empty", ""),
    ]));
    assert_eq!(
        body,
        "metadata%5Border_id%5D=6735&name=Gold+Plan+%26+co+%3D+100%25\
         &description=caf%C3%A9&expand%5B%5D=data.customer&empty="
    );
}

#[test]
fn values_without_a_form_spelling_are_refused() {
    #[derive(Serialize)]
    struct WithMap {
        metadata: BTreeMap<String, String>,
    }
    let bracket = WithMap {
        metadata: BTreeMap::from([("a[b]".to_owned(), "x".to_owned())]),
    };
    assert_eq!(to_pairs(&bracket).unwrap_err(), FormError::Key);
    let empty_key = WithMap {
        metadata: BTreeMap::from([(String::new(), "x".to_owned())]),
    };
    assert_eq!(to_pairs(&empty_key).unwrap_err(), FormError::Key);

    assert_eq!(to_pairs(&"text").unwrap_err(), FormError::TopLevel);
    assert_eq!(to_pairs(&vec![1, 2]).unwrap_err(), FormError::TopLevel);

    #[derive(Serialize)]
    struct Float {
        rate: f64,
    }
    assert_eq!(
        to_pairs(&Float { rate: f64::NAN }).unwrap_err(),
        FormError::NonFinite
    );
    assert_eq!(
        to_pairs(&Float { rate: 2.5 }).unwrap(),
        pairs(&[("rate", "2.5")])
    );

    #[derive(Serialize)]
    enum Shape {
        Pair(u8, u8),
    }
    #[derive(Serialize)]
    struct WithShape {
        shape: Shape,
    }
    assert_eq!(
        to_pairs(&WithShape {
            shape: Shape::Pair(1, 2)
        })
        .unwrap_err(),
        FormError::Unsupported("tuple variant")
    );
    // Display names the rule only.
    assert_eq!(
        FormError::Key.to_string(),
        "a form key must be non-empty and contain no `[` or `]`"
    );
}

#[test]
fn nested_sequences_inside_objects_keep_indices() {
    #[derive(Serialize)]
    struct Line {
        tax_rates: Vec<&'static str>,
        price: &'static str,
    }
    #[derive(Serialize)]
    struct Body {
        lines: Vec<Line>,
        flags: Vec<bool>,
    }
    let body = Body {
        lines: vec![
            Line {
                tax_rates: vec!["txr_a", "txr_b"],
                price: "price_a",
            },
            Line {
                tax_rates: vec![],
                price: "price_b",
            },
        ],
        flags: vec![true, false],
    };
    assert_eq!(
        to_pairs(&body).unwrap(),
        pairs(&[
            ("lines[0][tax_rates][]", "txr_a"),
            ("lines[0][tax_rates][]", "txr_b"),
            ("lines[0][price]", "price_a"),
            ("lines[1][price]", "price_b"),
            ("flags[]", "true"),
            ("flags[]", "false"),
        ])
    );
}
