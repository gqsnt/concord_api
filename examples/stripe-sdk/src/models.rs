//! Response objects of the implemented operations, as documented for API version
//! [`PINNED_API_VERSION`](crate::PINNED_API_VERSION).
//!
//! Only the fields the SDK and its workflows use are modelled; Stripe sends many more and they
//! are ignored when decoding. Enumerations Stripe documents as growing (statuses, types) keep an
//! `Other` variant so a new provider value does not fail a whole page. Metadata stays a dynamic
//! map.

use serde::Deserialize;

use crate::expandable::{Expandable, List, Object};
use crate::ids::{
    BalanceTransactionId, CustomerId, InvoiceId, InvoiceLineItemId, PaymentMethodId, PriceId,
    ProductId, SubscriptionId, SubscriptionItemId,
};
use crate::types::{Currency, Metadata, Timestamp};

/// A postal address (`address` on a customer).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, serde::Serialize)]
pub struct Address {
    /// City, district, suburb, town or village.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    /// Two-letter country code (ISO 3166-1 alpha-2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// Address line 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line1: Option<String>,
    /// Address line 2.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line2: Option<String>,
    /// ZIP or postal code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<String>,
    /// State, county, province or region.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// A customer (`object: "customer"`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Customer {
    /// `cus_...`.
    pub id: CustomerId,
    /// Email address.
    pub email: Option<String>,
    /// Full name or business name.
    pub name: Option<String>,
    /// Internal description.
    pub description: Option<String>,
    /// Phone number.
    #[serde(default)]
    pub phone: Option<String>,
    /// Address.
    #[serde(default)]
    pub address: Option<Address>,
    /// Current balance in the smallest currency unit (negative: credit).
    #[serde(default)]
    pub balance: i64,
    /// The currency the customer is billed in, once set.
    #[serde(default)]
    pub currency: Option<Currency>,
    /// Creation time.
    pub created: Timestamp,
    /// Preferred locales, in order.
    #[serde(default)]
    pub preferred_locales: Vec<String>,
    /// Default invoice settings (always sent by Stripe; empty when absent from a response).
    #[serde(default)]
    pub invoice_settings: CustomerInvoiceSettings,
    /// Metadata.
    #[serde(default)]
    pub metadata: Metadata,
    /// Live or test mode.
    #[serde(default)]
    pub livemode: bool,
}

impl Customer {
    /// The default payment method for subscriptions and invoices
    /// (`invoice_settings.default_payment_method`), as an id or, when the request named
    /// `invoice_settings.default_payment_method` (`data.invoice_settings.default_payment_method`
    /// on a list) in `expand[]`, as the object.
    pub fn default_payment_method(&self) -> Option<&Expandable<PaymentMethod>> {
        self.invoice_settings.default_payment_method.as_ref()
    }
}

impl Object for Customer {
    type Id = CustomerId;
    fn id(&self) -> &CustomerId {
        &self.id
    }
}

/// `invoice_settings` of a customer. Only the payment method is modelled; `custom_fields`,
/// `footer` and `rendering_options` are ignored.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct CustomerInvoiceSettings {
    /// The default payment method (nullable, expandable:
    /// `invoice_settings.default_payment_method`).
    #[serde(default)]
    pub default_payment_method: Option<Expandable<PaymentMethod>>,
}

/// A payment method (`object: "payment_method"`), as far as this SDK reads it: its type, the
/// card summary of a `card` payment method and the customer it is saved to.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PaymentMethod {
    /// `pm_...`.
    pub id: PaymentMethodId,
    /// The type, such as `card`, `sepa_debit` or `us_bank_account` (a long, growing list kept as
    /// text). A member of the same name holds the type's details; only `card` is modelled.
    #[serde(rename = "type")]
    pub kind: String,
    /// Card details, for a `card` payment method.
    #[serde(default)]
    pub card: Option<PaymentMethodCard>,
    /// The customer it is saved to (expandable), if any.
    #[serde(default)]
    pub customer: Option<Expandable<Customer>>,
    /// Creation time.
    pub created: Timestamp,
    /// Metadata (nullable on payment methods).
    #[serde(default)]
    pub metadata: Option<Metadata>,
    /// Live or test mode.
    #[serde(default)]
    pub livemode: bool,
}

impl Object for PaymentMethod {
    type Id = PaymentMethodId;
    fn id(&self) -> &PaymentMethodId {
        &self.id
    }
}

/// `card` of a payment method: the summary an application may display.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PaymentMethodCard {
    /// Card brand (`visa`, `mastercard`, ..., `unknown`), as sent.
    pub brand: String,
    /// The last four digits.
    pub last4: String,
    /// Expiration month (1-12).
    pub exp_month: u32,
    /// Expiration year (four digits).
    pub exp_year: u32,
}

/// A product (`object: "product"`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Product {
    /// The product id.
    pub id: ProductId,
    /// Display name.
    pub name: String,
    /// Whether it can be purchased.
    pub active: bool,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
    /// The default price (expandable).
    #[serde(default)]
    pub default_price: Option<Expandable<Price>>,
    /// Image URLs.
    #[serde(default)]
    pub images: Vec<String>,
    /// Marketing features.
    #[serde(default)]
    pub marketing_features: Vec<MarketingFeature>,
    /// Metadata.
    #[serde(default)]
    pub metadata: Metadata,
    /// Creation time.
    pub created: Timestamp,
    /// Last update time.
    #[serde(default)]
    pub updated: Option<Timestamp>,
    /// Live or test mode.
    #[serde(default)]
    pub livemode: bool,
}

impl Object for Product {
    type Id = ProductId;
    fn id(&self) -> &ProductId {
        &self.id
    }
}

/// One marketing feature of a product.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
pub struct MarketingFeature {
    /// The feature's name.
    pub name: String,
}

/// Billing interval of a recurring price.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Interval {
    /// Daily.
    Day,
    /// Weekly.
    Week,
    /// Monthly.
    Month,
    /// Yearly.
    Year,
}

/// The recurring part of a price.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Recurring {
    /// The interval.
    pub interval: Interval,
    /// Number of intervals between billings.
    pub interval_count: u32,
}

/// `one_time` or `recurring`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceType {
    /// Charged once.
    OneTime,
    /// Charged per interval.
    Recurring,
    /// A value this SDK does not know.
    #[serde(other)]
    Other,
}

/// A price (`object: "price"`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Price {
    /// `price_...`.
    pub id: PriceId,
    /// The product (expandable).
    pub product: Expandable<Product>,
    /// Whether it can be used for new purchases.
    pub active: bool,
    /// Currency.
    pub currency: Currency,
    /// Unit amount in the smallest currency unit (`None` for tiered or custom prices).
    pub unit_amount: Option<i64>,
    /// The recurring part, for recurring prices.
    #[serde(default)]
    pub recurring: Option<Recurring>,
    /// `one_time` or `recurring`.
    #[serde(rename = "type")]
    pub kind: PriceType,
    /// Lookup key.
    #[serde(default)]
    pub lookup_key: Option<String>,
    /// Nickname.
    #[serde(default)]
    pub nickname: Option<String>,
    /// Metadata.
    #[serde(default)]
    pub metadata: Metadata,
    /// Creation time.
    pub created: Timestamp,
    /// Live or test mode.
    #[serde(default)]
    pub livemode: bool,
}

impl Object for Price {
    type Id = PriceId;
    fn id(&self) -> &PriceId {
        &self.id
    }
}

/// A subscription's status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    /// `active`.
    Active,
    /// `canceled`.
    Canceled,
    /// `incomplete`.
    Incomplete,
    /// `incomplete_expired`.
    IncompleteExpired,
    /// `past_due`.
    PastDue,
    /// `paused`.
    Paused,
    /// `trialing`.
    Trialing,
    /// `unpaid`.
    Unpaid,
    /// A value this SDK does not know.
    #[serde(other)]
    Other,
}

/// How invoices are collected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionMethod {
    /// Charge the default payment method automatically.
    ChargeAutomatically,
    /// Email the invoice with payment instructions.
    SendInvoice,
}

/// One item of a subscription (`object: "subscription_item"`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SubscriptionItem {
    /// `si_...`.
    pub id: SubscriptionItemId,
    /// The item's price.
    pub price: Price,
    /// Quantity.
    #[serde(default)]
    pub quantity: Option<u64>,
    /// Start of the item's current period (on items since the 2025 billing changes).
    #[serde(default)]
    pub current_period_start: Option<Timestamp>,
    /// End of the item's current period.
    #[serde(default)]
    pub current_period_end: Option<Timestamp>,
    /// The subscription.
    pub subscription: SubscriptionId,
    /// Metadata.
    #[serde(default)]
    pub metadata: Metadata,
}

impl Object for SubscriptionItem {
    type Id = SubscriptionItemId;
    fn id(&self) -> &SubscriptionItemId {
        &self.id
    }
}

/// A subscription (`object: "subscription"`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Subscription {
    /// `sub_...`.
    pub id: SubscriptionId,
    /// The customer (expandable).
    pub customer: Expandable<Customer>,
    /// Status.
    pub status: SubscriptionStatus,
    /// The items (an embedded list).
    pub items: List<SubscriptionItem>,
    /// The latest invoice (expandable).
    #[serde(default)]
    pub latest_invoice: Option<Expandable<Invoice>>,
    /// Currency.
    pub currency: Currency,
    /// Collection method.
    pub collection_method: CollectionMethod,
    /// Whether it cancels at the end of the period.
    #[serde(default)]
    pub cancel_at_period_end: bool,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
    /// Creation time.
    pub created: Timestamp,
    /// Start date.
    #[serde(default)]
    pub start_date: Option<Timestamp>,
    /// Metadata.
    #[serde(default)]
    pub metadata: Metadata,
    /// Live or test mode.
    #[serde(default)]
    pub livemode: bool,
}

impl Object for Subscription {
    type Id = SubscriptionId;
    fn id(&self) -> &SubscriptionId {
        &self.id
    }
}

/// An invoice's status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    /// `draft`.
    Draft,
    /// `open`.
    Open,
    /// `paid`.
    Paid,
    /// `uncollectible`.
    Uncollectible,
    /// `void`.
    Void,
}

/// What generated an invoice (`parent`, since the 2025 billing changes; replaces the former
/// top-level `subscription`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct InvoiceParent {
    /// `subscription_details` or `quote_details`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Present when a subscription generated the invoice.
    #[serde(default)]
    pub subscription_details: Option<SubscriptionDetails>,
}

/// `parent.subscription_details` of an invoice.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SubscriptionDetails {
    /// The subscription (expandable: `parent.subscription_details.subscription`).
    pub subscription: Expandable<Subscription>,
    /// Snapshot of the subscription's metadata.
    #[serde(default)]
    pub metadata: Option<Metadata>,
}

/// An invoice (`object: "invoice"`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Invoice {
    /// `in_...`.
    pub id: InvoiceId,
    /// The invoice number, once finalized.
    #[serde(default)]
    pub number: Option<String>,
    /// The customer (expandable).
    pub customer: Expandable<Customer>,
    /// Currency.
    pub currency: Currency,
    /// Status.
    pub status: Option<InvoiceStatus>,
    /// Total after discounts and taxes.
    pub total: i64,
    /// Total before invoice-level discounts and exclusive tax.
    pub subtotal: i64,
    /// Final amount due.
    pub amount_due: i64,
    /// Amount paid.
    pub amount_paid: i64,
    /// `amount_due - amount_paid`.
    pub amount_remaining: i64,
    /// Collection method.
    pub collection_method: CollectionMethod,
    /// Creation time.
    pub created: Timestamp,
    /// Earliest time invoice items can be associated.
    pub period_start: Timestamp,
    /// Latest time invoice items can be associated.
    pub period_end: Timestamp,
    /// What generated it.
    #[serde(default)]
    pub parent: Option<InvoiceParent>,
    /// The first line items (an embedded list; `lines.has_more` says whether
    /// `GET /v1/invoices/{id}/lines` has more).
    pub lines: List<InvoiceLineItem>,
    /// Metadata (nullable on invoices).
    #[serde(default)]
    pub metadata: Option<Metadata>,
    /// Live or test mode.
    #[serde(default)]
    pub livemode: bool,
}

impl Invoice {
    /// The subscription that generated this invoice, if any (`parent.subscription_details`).
    pub fn subscription(&self) -> Option<&Expandable<Subscription>> {
        self.parent
            .as_ref()
            .and_then(|p| p.subscription_details.as_ref())
            .map(|d| &d.subscription)
    }
}

impl Object for Invoice {
    type Id = InvoiceId;
    fn id(&self) -> &InvoiceId {
        &self.id
    }
}

/// The period a line item covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct Period {
    /// Start.
    pub start: Timestamp,
    /// End.
    pub end: Timestamp,
}

/// What generated a line item.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct LineItemParent {
    /// `invoice_item_details` or `subscription_item_details`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Present for invoice items.
    #[serde(default)]
    pub invoice_item_details: Option<InvoiceItemDetails>,
    /// Present for subscription items.
    #[serde(default)]
    pub subscription_item_details: Option<SubscriptionItemDetails>,
}

/// `parent.invoice_item_details` of a line item.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct InvoiceItemDetails {
    /// The invoice item (`ii_...`).
    pub invoice_item: String,
    /// Whether this is a proration.
    #[serde(default)]
    pub proration: bool,
    /// The subscription the invoice item belongs to.
    #[serde(default)]
    pub subscription: Option<SubscriptionId>,
}

/// `parent.subscription_item_details` of a line item.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SubscriptionItemDetails {
    /// The subscription item.
    pub subscription_item: SubscriptionItemId,
    /// The subscription.
    #[serde(default)]
    pub subscription: Option<SubscriptionId>,
    /// Whether this is a proration.
    #[serde(default)]
    pub proration: bool,
}

/// Pricing of a line item.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct LinePricing {
    /// `price_details` (the only documented type).
    #[serde(rename = "type")]
    pub kind: String,
    /// Price and product, for `price_details`.
    #[serde(default)]
    pub price_details: Option<PriceDetails>,
    /// Unit amount as a decimal string.
    #[serde(default)]
    pub unit_amount_decimal: Option<String>,
}

/// `pricing.price_details` of a line item.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PriceDetails {
    /// The price.
    pub price: PriceId,
    /// The product.
    pub product: ProductId,
}

/// An invoice line item (`object: "line_item"`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct InvoiceLineItem {
    /// `il_...`.
    pub id: InvoiceLineItemId,
    /// Amount in the smallest currency unit.
    pub amount: i64,
    /// Currency.
    pub currency: Currency,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
    /// The invoice containing it. Listed as an attribute in the reference but absent from its
    /// example objects, so optional here (see `PROVIDER.md`).
    #[serde(default)]
    pub invoice: Option<InvoiceId>,
    /// What generated it.
    #[serde(default)]
    pub parent: Option<LineItemParent>,
    /// The period covered.
    pub period: Period,
    /// Pricing.
    #[serde(default)]
    pub pricing: Option<LinePricing>,
    /// Quantity.
    #[serde(default)]
    pub quantity: Option<u64>,
    /// Subtotal before discounts and taxes. Listed as an attribute but absent from the
    /// reference's examples, so optional here.
    #[serde(default)]
    pub subtotal: Option<i64>,
    /// Whether discounts apply.
    #[serde(default)]
    pub discountable: bool,
    /// Metadata.
    #[serde(default)]
    pub metadata: Metadata,
    /// Live or test mode.
    #[serde(default)]
    pub livemode: bool,
}

impl InvoiceLineItem {
    /// The subscription this line belongs to, from either parent kind.
    pub fn subscription(&self) -> Option<&SubscriptionId> {
        let parent = self.parent.as_ref()?;
        parent
            .subscription_item_details
            .as_ref()
            .and_then(|d| d.subscription.as_ref())
            .or_else(|| {
                parent
                    .invoice_item_details
                    .as_ref()
                    .and_then(|d| d.subscription.as_ref())
            })
    }

    /// The price, from `pricing.price_details`.
    pub fn price(&self) -> Option<&PriceId> {
        self.pricing
            .as_ref()
            .and_then(|p| p.price_details.as_ref())
            .map(|d| &d.price)
    }
}

impl Object for InvoiceLineItem {
    type Id = InvoiceLineItemId;
    fn id(&self) -> &InvoiceLineItemId {
        &self.id
    }
}

/// Whether a balance transaction's funds are available.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BalanceStatus {
    /// `available`.
    Available,
    /// `pending`.
    Pending,
    /// A value this SDK does not know.
    #[serde(other)]
    Other,
}

/// One fee of a balance transaction.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct FeeDetail {
    /// Amount.
    pub amount: i64,
    /// Currency.
    pub currency: Currency,
    /// `application_fee`, `payment_method_passthrough_fee`, `stripe_fee` or `tax`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
}

/// The object a balance transaction relates to, when expanded (`expand[]=data.source`). It can
/// be any of many object types (charge, refund, transfer, payout, ...), so only its id and
/// `object` name are typed; the rest stays a dynamic JSON object.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct SourceObject {
    /// The object's id.
    pub id: String,
    /// The object's type name, such as `charge`.
    pub object: String,
    /// Every other field, as sent.
    #[serde(flatten)]
    pub fields: serde_json::Map<String, serde_json::Value>,
}

impl Object for SourceObject {
    type Id = String;
    fn id(&self) -> &String {
        &self.id
    }
}

/// A balance transaction (`object: "balance_transaction"`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct BalanceTransaction {
    /// `txn_...`.
    pub id: BalanceTransactionId,
    /// Gross amount.
    pub amount: i64,
    /// When the net funds become available.
    pub available_on: Timestamp,
    /// Creation time.
    pub created: Timestamp,
    /// Currency.
    pub currency: Currency,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
    /// Fees.
    pub fee: i64,
    /// Fee breakdown.
    #[serde(default)]
    pub fee_details: Vec<FeeDetail>,
    /// Net impact (`amount - fee`).
    pub net: i64,
    /// Reporting category.
    pub reporting_category: String,
    /// The related object (expandable; many object types).
    #[serde(default)]
    pub source: Option<Expandable<SourceObject>>,
    /// Available or pending.
    pub status: BalanceStatus,
    /// Transaction type (`charge`, `refund`, `payout`, ...; a long, growing list kept as text).
    #[serde(rename = "type")]
    pub kind: String,
}

impl Object for BalanceTransaction {
    type Id = BalanceTransactionId;
    fn id(&self) -> &BalanceTransactionId {
        &self.id
    }
}
