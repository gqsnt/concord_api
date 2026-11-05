//! Request bodies of the implemented writes, encoded by [`StripeForm`](crate::form::StripeForm).
//!
//! Conventions (see [`crate::form`] for the wire spelling):
//!
//! - `Option<T>`: `None` omits the parameter.
//! - `Option<Patch<T>>` (update bodies): `None` leaves the field unchanged, `Some(Patch::Unset)`
//!   sends an empty value, which Stripe documents as unsetting it, `Some(Patch::Set(v))` sends
//!   `v`.
//! - `Vec<T>`: an empty vector omits the parameter; a non-empty one is a repeated field
//!   (`expand[]=..`) or an indexed list of objects (`items[0][price]=..`).
//! - Metadata on create bodies is a plain map (`metadata[key]=value`); on update bodies it is a
//!   [`MetadataPatch`] (set keys, unset keys, or clear all).
//!
//! Fields are public so that an application can fill the parts it needs; constructors exist for
//! the required ones. Validation the provider documents for one field alone (id prefixes,
//! currency spelling, expansion depth, metadata key brackets) happens in the value types or the
//! encoder before anything is sent; cross-field rules stay Stripe's to enforce.

use serde::Serialize;

use crate::form::{MetadataPatch, Patch};
use crate::ids::{CustomerId, PaymentMethodId, PriceId, ProductId, SubscriptionItemId, TaxRateId};
use crate::models::{Address, CollectionMethod, Interval, MarketingFeature};
use crate::types::{Currency, ExpandPath, Metadata};

fn is_empty_map(map: &Metadata) -> bool {
    map.is_empty()
}

/// `POST /v1/customers`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CustomerCreate {
    /// Full name or business name.
    pub name: Option<String>,
    /// Email address.
    pub email: Option<String>,
    /// Internal description.
    pub description: Option<String>,
    /// Phone number.
    pub phone: Option<String>,
    /// Address: `address[line1]=..`, `address[country]=..`.
    pub address: Option<Address>,
    /// Preferred locales, a repeated field: `preferred_locales[]=fr-FR&preferred_locales[]=en`.
    pub preferred_locales: Vec<String>,
    /// Default invoice settings: `invoice_settings[default_payment_method]=pm_..`.
    pub invoice_settings: Option<InvoiceSettingsParams>,
    /// Metadata: `metadata[key]=value`.
    #[serde(skip_serializing_if = "is_empty_map")]
    pub metadata: Metadata,
    /// Objects to expand in the response, such as `invoice_settings.default_payment_method`.
    pub expand: Vec<ExpandPath>,
}

/// `invoice_settings` of a customer create or update: `invoice_settings[default_payment_method]`.
///
/// Every field is optional; a value with no field set sends nothing, exactly like `None`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct InvoiceSettingsParams {
    /// The payment method (already attached to the customer) used by default for subscriptions
    /// and invoices. Stripe documents no empty-value unset for this field, so there is no
    /// `Patch` here: `None` leaves it as it is.
    pub default_payment_method: Option<PaymentMethodId>,
}

/// `POST /v1/customers/{id}`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CustomerUpdate {
    /// Name.
    pub name: Option<Patch<String>>,
    /// Email.
    pub email: Option<Patch<String>>,
    /// Description; `Some(Patch::Unset)` sends `description=`.
    pub description: Option<Patch<String>>,
    /// Phone.
    pub phone: Option<Patch<String>>,
    /// Address; `Some(Patch::Unset)` sends `address=`.
    pub address: Option<Patch<Address>>,
    /// Preferred locales; `Some(Patch::Unset)` sends `preferred_locales=`.
    pub preferred_locales: Option<Patch<Vec<String>>>,
    /// Default invoice settings to change: `invoice_settings[default_payment_method]=pm_..`.
    pub invoice_settings: Option<InvoiceSettingsParams>,
    /// Metadata changes.
    pub metadata: Option<MetadataPatch>,
    /// Objects to expand in the response, such as `invoice_settings.default_payment_method`.
    pub expand: Vec<ExpandPath>,
}

/// One item of a new subscription: `items[i][price]`, `items[i][quantity]`,
/// `items[i][metadata][key]`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SubscriptionItemCreate {
    /// The price.
    pub price: PriceId,
    /// Quantity (Stripe's default is 1).
    pub quantity: Option<u64>,
    /// Item metadata.
    #[serde(skip_serializing_if = "is_empty_map")]
    pub metadata: Metadata,
}

impl SubscriptionItemCreate {
    /// One unit of `price`.
    pub fn new(price: PriceId) -> Self {
        Self {
            price,
            quantity: None,
            metadata: Metadata::new(),
        }
    }
}

/// How the first invoice's payment is handled (`payment_behavior`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentBehavior {
    /// `allow_incomplete` (Stripe's default).
    AllowIncomplete,
    /// `default_incomplete`.
    DefaultIncomplete,
    /// `error_if_incomplete`.
    ErrorIfIncomplete,
    /// `pending_if_incomplete` (updates only).
    PendingIfIncomplete,
}

/// How prorations are handled (`proration_behavior`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProrationBehavior {
    /// `always_invoice` (updates only).
    AlwaysInvoice,
    /// `create_prorations` (Stripe's default).
    CreateProrations,
    /// `none`.
    None,
}

/// `POST /v1/subscriptions`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SubscriptionCreate {
    /// The customer.
    pub customer: CustomerId,
    /// Up to 20 items: `items[0][price]=..`.
    pub items: Vec<SubscriptionItemCreate>,
    /// Metadata.
    #[serde(skip_serializing_if = "is_empty_map")]
    pub metadata: Metadata,
    /// Default tax rates, a repeated field: `default_tax_rates[]=txr_..`.
    pub default_tax_rates: Vec<TaxRateId>,
    /// Collection method.
    pub collection_method: Option<CollectionMethod>,
    /// Days to pay, with `send_invoice`.
    pub days_until_due: Option<u32>,
    /// First-invoice payment behaviour.
    pub payment_behavior: Option<PaymentBehavior>,
    /// Trial length in days.
    pub trial_period_days: Option<u32>,
    /// Cancel at the end of the period.
    pub cancel_at_period_end: Option<bool>,
    /// Description.
    pub description: Option<String>,
    /// Objects to expand in the response, such as `latest_invoice`.
    pub expand: Vec<ExpandPath>,
}

impl SubscriptionCreate {
    /// A subscription of `customer` to `items`.
    pub fn new(customer: CustomerId, items: Vec<SubscriptionItemCreate>) -> Self {
        Self {
            customer,
            items,
            metadata: Metadata::new(),
            default_tax_rates: Vec::new(),
            collection_method: None,
            days_until_due: None,
            payment_behavior: None,
            trial_period_days: None,
            cancel_at_period_end: None,
            description: None,
            expand: Vec::new(),
        }
    }
}

/// One item change in a subscription update: `items[i][id]`, `items[i][price]`,
/// `items[i][quantity]`, `items[i][deleted]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SubscriptionItemUpdate {
    /// The existing item to change (absent: add a new item).
    pub id: Option<SubscriptionItemId>,
    /// New price.
    pub price: Option<PriceId>,
    /// New quantity.
    pub quantity: Option<u64>,
    /// Remove the item.
    pub deleted: Option<bool>,
}

/// `POST /v1/subscriptions/{id}`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SubscriptionUpdate {
    /// Item changes.
    pub items: Vec<SubscriptionItemUpdate>,
    /// Metadata changes.
    pub metadata: Option<MetadataPatch>,
    /// Default tax rates; `Some(Patch::Unset)` sends `default_tax_rates=` (documented: "Pass an
    /// empty string to remove previously-defined tax rates").
    pub default_tax_rates: Option<Patch<Vec<TaxRateId>>>,
    /// Cancel at the end of the period.
    pub cancel_at_period_end: Option<bool>,
    /// Proration handling.
    pub proration_behavior: Option<ProrationBehavior>,
    /// Payment handling.
    pub payment_behavior: Option<PaymentBehavior>,
    /// Description.
    pub description: Option<Patch<String>>,
    /// Objects to expand in the response.
    pub expand: Vec<ExpandPath>,
}

/// The recurring part of a new price: `recurring[interval]`, `recurring[interval_count]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RecurringParams {
    /// Interval.
    pub interval: Interval,
    /// Number of intervals between billings.
    pub interval_count: Option<u32>,
}

/// Data of the default price created with a product: `default_price_data[currency]`, ...
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DefaultPriceData {
    /// Currency.
    pub currency: Currency,
    /// Unit amount.
    pub unit_amount: i64,
    /// Recurring part.
    pub recurring: Option<RecurringParams>,
}

/// `POST /v1/products`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProductCreate {
    /// Display name.
    pub name: String,
    /// Whether it can be purchased (Stripe's default: true).
    pub active: Option<bool>,
    /// Description.
    pub description: Option<String>,
    /// Image URLs, a repeated field: `images[]=..`.
    pub images: Vec<String>,
    /// Marketing features: `marketing_features[0][name]=..`.
    pub marketing_features: Vec<MarketingFeature>,
    /// A default price created with it.
    pub default_price_data: Option<DefaultPriceData>,
    /// Metadata.
    #[serde(skip_serializing_if = "is_empty_map")]
    pub metadata: Metadata,
    /// Unit label.
    pub unit_label: Option<String>,
    /// Objects to expand in the response, such as `default_price`.
    pub expand: Vec<ExpandPath>,
}

impl ProductCreate {
    /// A product named `name`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            active: None,
            description: None,
            images: Vec::new(),
            marketing_features: Vec::new(),
            default_price_data: None,
            metadata: Metadata::new(),
            unit_label: None,
            expand: Vec::new(),
        }
    }
}

/// The product of a new price: an existing one (`product=..`) or one created with it
/// (`product_data[name]=..`). Stripe requires exactly one of the two; the enum makes the other
/// combinations unrepresentable.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum PriceProduct {
    /// `product=prod_..`.
    #[serde(rename = "product")]
    Existing(ProductId),
    /// `product_data[name]=..`.
    #[serde(rename = "product_data")]
    New(NewProductData),
}

/// `product_data` of a new price.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NewProductData {
    /// Name.
    pub name: String,
    /// Metadata.
    #[serde(skip_serializing_if = "is_empty_map")]
    pub metadata: Metadata,
}

/// Tax behaviour of a price.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxBehavior {
    /// `exclusive`.
    Exclusive,
    /// `inclusive`.
    Inclusive,
    /// `unspecified`.
    Unspecified,
}

/// `POST /v1/prices`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PriceCreate {
    /// Currency.
    pub currency: Currency,
    /// The product.
    #[serde(flatten)]
    pub product: PriceProduct,
    /// Unit amount in the smallest currency unit.
    pub unit_amount: Option<i64>,
    /// Recurring part (absent: one-time).
    pub recurring: Option<RecurringParams>,
    /// Lookup key.
    pub lookup_key: Option<String>,
    /// Nickname.
    pub nickname: Option<String>,
    /// Whether it can be used for new purchases.
    pub active: Option<bool>,
    /// Tax behaviour.
    pub tax_behavior: Option<TaxBehavior>,
    /// Metadata.
    #[serde(skip_serializing_if = "is_empty_map")]
    pub metadata: Metadata,
    /// Objects to expand in the response, such as `product`.
    pub expand: Vec<ExpandPath>,
}

impl PriceCreate {
    /// A price of `unit_amount` in `currency` for `product`.
    pub fn new(currency: Currency, product: PriceProduct, unit_amount: i64) -> Self {
        Self {
            currency,
            product,
            unit_amount: Some(unit_amount),
            recurring: None,
            lookup_key: None,
            nickname: None,
            active: None,
            tax_behavior: None,
            metadata: Metadata::new(),
            expand: Vec::new(),
        }
    }
}
