# the Stripe API in Concord
Written by GPT-6 (Codex), from the supplied Concord guide and Stripe's public API documentation, on 2026-11-12.

## The description

This is an API v1 client with 40 endpoints. Required keys identify operations, not attempts: persist a key with the operation and reuse it with identical parameters. Stripe can prune keys after at least 24 hours; stop retrying an unresolved operation before that window ends. All POSTs below require a key, including uploads. [Stripe idempotency](https://docs.stripe.com/api/idempotent_requests).

The version is deliberately required. The examples use `2026-10-28.endive`, shown in the [current customer reference](https://docs.stripe.com/api/customers/retrieve); the named response models must match the version selected by the application. Lists expose forward pagination; backward pagination is outside this subset. All snippets are paper Rust, with the phase-C sheet taking precedence over the older pages.

```rust
use concord::{Json, Secret};
use crate::model::*;
use crate::wire::{list_next, search_next, stripe_form, stripe_multipart};

concord::api! {
    /// Stripe API v1. Every POST requires a durable, operation-specific key.
    pub client Stripe(
        key: Secret,
        /// Choose a version matching this crate's response models.
        version: String,
        /// Connect account; omitted for requests on the platform account.
        account: Option<AccountId>,
    ) {
        base "https://api.stripe.com";
        headers {
            "authorization" = "Bearer {key}";
            "Stripe-Version" = version;
            "Stripe-Account" = account;
        }
        read Json;
        problem StripeProblem;

        scope customers {
            path "v1/customers";

            POST create(new: NewCustomer, key: IdempotencyKey, expand: Option<Vec<String>>) -> Customer {
                headers { "Idempotency-Key" = key; }
                body stripe_form(new, expand);
                idempotent;
            }
            /// A deleted customer can be a tombstone; a 404 remains an error.
            GET get(id: CustomerId, expand: Option<Vec<String>>) -> CustomerRecord {
                path "{id}";
                query { "expand[]" = expand; }
            }
            POST update(id: CustomerId, changes: UpdateCustomer, key: IdempotencyKey, expand: Option<Vec<String>>) -> Customer {
                path "{id}";
                headers { "Idempotency-Key" = key; }
                body stripe_form(changes, expand);
                idempotent;
            }
            DELETE delete(id: CustomerId) -> Deleted<CustomerId> { path "{id}"; }
            GET list(email: Option<String>, created_from: Option<i64>, created_before: Option<i64>, limit: Option<u32>, starting_after: Option<CustomerId>, expand: Option<Vec<String>>) -> List<Customer> {
                query { email; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; "expand[]" = expand; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
            /// Search is eventually consistent; do not use it to verify a write.
            GET search(query: String, page: Option<String>, limit: Option<u32>, expand: Option<Vec<String>>) -> SearchPage<Customer> {
                path "search";
                query { query; page; limit; "expand[]" = expand; }
                paginate |call, answer| search_next(&page, call, answer).map_err(concord::mismatch);
            }
        }

        scope payment_intents {
            path "v1/payment_intents";

            POST create(new: NewPaymentIntent, key: IdempotencyKey, expand: Option<Vec<String>>) -> PaymentIntent {
                headers { "Idempotency-Key" = key; }
                body stripe_form(new, expand);
                idempotent;
            }
            GET get(id: PaymentIntentId, expand: Option<Vec<String>>) -> PaymentIntent {
                path "{id}";
                query { "expand[]" = expand; }
            }
            POST update(id: PaymentIntentId, changes: UpdatePaymentIntent, key: IdempotencyKey, expand: Option<Vec<String>>) -> PaymentIntent {
                path "{id}";
                headers { "Idempotency-Key" = key; }
                body stripe_form(changes, expand);
                idempotent;
            }
            /// Success can still require customer action; inspect status/next_action.
            POST confirm(id: PaymentIntentId, options: ConfirmPaymentIntent, key: IdempotencyKey, expand: Option<Vec<String>>) -> PaymentIntent {
                path "{id}/confirm";
                headers { "Idempotency-Key" = key; }
                body stripe_form(options, expand);
                idempotent;
            }
            POST capture(id: PaymentIntentId, options: CapturePaymentIntent, key: IdempotencyKey, expand: Option<Vec<String>>) -> PaymentIntent {
                path "{id}/capture";
                headers { "Idempotency-Key" = key; }
                body stripe_form(options, expand);
                idempotent;
            }
            POST cancel(id: PaymentIntentId, options: CancelPaymentIntent, key: IdempotencyKey, expand: Option<Vec<String>>) -> PaymentIntent {
                path "{id}/cancel";
                headers { "Idempotency-Key" = key; }
                body stripe_form(options, expand);
                idempotent;
            }
            GET list(customer: Option<CustomerId>, created_from: Option<i64>, created_before: Option<i64>, limit: Option<u32>, starting_after: Option<PaymentIntentId>, expand: Option<Vec<String>>) -> List<PaymentIntent> {
                query { customer; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; "expand[]" = expand; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
            /// Search uses page tokens, not PaymentIntent IDs.
            GET search(query: String, page: Option<String>, limit: Option<u32>, expand: Option<Vec<String>>) -> SearchPage<PaymentIntent> {
                path "search";
                query { query; page; limit; "expand[]" = expand; }
                paginate |call, answer| search_next(&page, call, answer).map_err(concord::mismatch);
            }
        }

        scope refunds {
            path "v1/refunds";

            POST create(new: NewRefund, key: IdempotencyKey, expand: Option<Vec<String>>) -> Refund {
                headers { "Idempotency-Key" = key; }
                body stripe_form(new, expand);
                idempotent;
            }
            GET get(id: RefundId, expand: Option<Vec<String>>) -> Refund {
                path "{id}";
                query { "expand[]" = expand; }
            }
            POST update(id: RefundId, changes: UpdateRefund, key: IdempotencyKey, expand: Option<Vec<String>>) -> Refund {
                path "{id}";
                headers { "Idempotency-Key" = key; }
                body stripe_form(changes, expand);
                idempotent;
            }
            /// Stripe permits this only for refunds in requires_action.
            POST cancel(id: RefundId, key: IdempotencyKey) -> Refund {
                path "{id}/cancel";
                headers { "Idempotency-Key" = key; }
                idempotent;
            }
            GET list(charge: Option<ChargeId>, payment_intent: Option<PaymentIntentId>, created_from: Option<i64>, created_before: Option<i64>, limit: Option<u32>, starting_after: Option<RefundId>, expand: Option<Vec<String>>) -> List<Refund> {
                query { charge; payment_intent; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; "expand[]" = expand; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
        }

        scope subscriptions {
            path "v1/subscriptions";

            POST create(new: NewSubscription, key: IdempotencyKey, expand: Option<Vec<String>>) -> Subscription {
                headers { "Idempotency-Key" = key; }
                body stripe_form(new, expand);
                idempotent;
            }
            GET get(id: SubscriptionId, expand: Option<Vec<String>>) -> Subscription {
                path "{id}";
                query { "expand[]" = expand; }
            }
            POST update(id: SubscriptionId, changes: UpdateSubscription, key: IdempotencyKey, expand: Option<Vec<String>>) -> Subscription {
                path "{id}";
                headers { "Idempotency-Key" = key; }
                body stripe_form(changes, expand);
                idempotent;
            }
            /// Cancels now. Use update(cancel_at_period_end) to schedule cancellation.
            DELETE cancel(id: SubscriptionId, invoice_now: Option<bool>, prorate: Option<bool>, expand: Option<Vec<String>>) -> Subscription {
                path "{id}";
                query { invoice_now; prorate; "expand[]" = expand; }
            }
            /// None leaves Stripe's default; All and Ended are filter-only values.
            GET list(customer: Option<CustomerId>, status: Option<SubscriptionStatusFilter>, price: Option<PriceId>, created_from: Option<i64>, created_before: Option<i64>, limit: Option<u32>, starting_after: Option<SubscriptionId>, expand: Option<Vec<String>>) -> List<Subscription> {
                query { customer; status; price; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; "expand[]" = expand; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
        }

        scope invoices {
            path "v1/invoices";

            POST create(new: NewInvoice, key: IdempotencyKey, expand: Option<Vec<String>>) -> Invoice {
                headers { "Idempotency-Key" = key; }
                body stripe_form(new, expand);
                idempotent;
            }
            GET get(id: InvoiceId, expand: Option<Vec<String>>) -> Invoice {
                path "{id}";
                query { "expand[]" = expand; }
            }
            POST update(id: InvoiceId, changes: UpdateInvoice, key: IdempotencyKey, expand: Option<Vec<String>>) -> Invoice {
                path "{id}";
                headers { "Idempotency-Key" = key; }
                body stripe_form(changes, expand);
                idempotent;
            }
            /// Only a deletable one-off draft; finalized invoices must be voided.
            DELETE delete(id: InvoiceId) -> Deleted<InvoiceId> { path "{id}"; }
            GET list(customer: Option<CustomerId>, subscription: Option<SubscriptionId>, status: Option<InvoiceStatusFilter>, collection_method: Option<CollectionMethod>, created_from: Option<i64>, created_before: Option<i64>, limit: Option<u32>, starting_after: Option<InvoiceId>, expand: Option<Vec<String>>) -> List<Invoice> {
                query { customer; subscription; status; collection_method; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; "expand[]" = expand; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
            POST finalize(id: InvoiceId, options: FinalizeInvoice, key: IdempotencyKey, expand: Option<Vec<String>>) -> Invoice {
                path "{id}/finalize";
                headers { "Idempotency-Key" = key; }
                body stripe_form(options, expand);
                idempotent;
            }
            POST pay(id: InvoiceId, options: PayInvoice, key: IdempotencyKey, expand: Option<Vec<String>>) -> Invoice {
                path "{id}/pay";
                headers { "Idempotency-Key" = key; }
                body stripe_form(options, expand);
                idempotent;
            }
            POST void(id: InvoiceId, key: IdempotencyKey) -> Invoice {
                path "{id}/void";
                headers { "Idempotency-Key" = key; }
                idempotent;
            }
            POST mark_uncollectible(id: InvoiceId, key: IdempotencyKey) -> Invoice {
                path "{id}/mark_uncollectible";
                headers { "Idempotency-Key" = key; }
                idempotent;
            }
            POST send_invoice(id: InvoiceId, key: IdempotencyKey) -> Invoice {
                path "{id}/send";
                headers { "Idempotency-Key" = key; }
                idempotent;
            }
        }

        GET balance() -> Balance { path "v1/balance"; }

        scope balance_transactions {
            path "v1/balance_transactions";

            GET get(id: BalanceTransactionId, expand: Option<Vec<String>>) -> BalanceTransaction {
                path "{id}";
                query { "expand[]" = expand; }
            }
            GET list(currency: Option<Currency>, source: Option<String>, transaction_type: Option<String>, created_from: Option<i64>, created_before: Option<i64>, limit: Option<u32>, starting_after: Option<BalanceTransactionId>, expand: Option<Vec<String>>) -> List<BalanceTransaction> {
                query { currency; source; "type" = transaction_type; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; "expand[]" = expand; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
        }

        scope events {
            path "v1/events";

            /// data.object retains its creation-time API version.
            GET get(id: EventId) -> Event { path "{id}"; }
            /// Stripe retains events for retrieval for up to 30 days.
            GET list(event_types: Option<Vec<String>>, delivery_success: Option<bool>, created_from: Option<i64>, created_before: Option<i64>, limit: Option<u32>, starting_after: Option<EventId>) -> List<Event> {
                query { "types[]" = event_types; delivery_success; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
        }

        scope files {
            base "https://files.stripe.com";
            path "v1/files";

            /// Buffered multipart upload; this helper accepts at most 8 MiB.
            POST upload(purpose: FilePurpose, file: Upload, key: IdempotencyKey) -> StripeFile {
                headers { "Idempotency-Key" = key; }
                body stripe_multipart(purpose, file);
                idempotent;
            }
        }
    }
}
```

There is no root `path "v1"`: a nearer `base` replaces the base, not inherited paths. Both hosts intentionally inherit the bearer token, version and account. Upload uses Stripe's documented host and multipart fields. [File upload](https://docs.stripe.com/api/files/create).

The subscription filter is separate from an object's status: it includes `all` and `ended`, while an omitted filter retains Stripe's default. A refund cancellation is limited to `requires_action`. [Subscriptions](https://docs.stripe.com/api/subscriptions/list), [refund cancellation](https://docs.stripe.com/api/refunds/cancel).

## Beside the macro

`model` holds the following plain Rust types; `wire` holds the functions below. Dependencies beside Concord are `serde`, `serde_json`, `serde_urlencoded` and `bytes`. The reader is the shipped `Json`; the transport is the shipped `concord::Http`. Stripe needs no custom reader or transport here, so there is no author-owned implementation of either to hide.

Two data types in full:

```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Default, Serialize)]
pub struct NewCustomer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

/// Expansion changes the wire value, not this Rust type.
#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub enum Expandable<I, T> {
    Id(I),
    Object(Box<T>),
}
```

The rest are named by their signatures below, rather than reproduced field by field as SDK source. Fields listed are public except the validated key/currency wrappers. All created timestamps and date filters are Unix seconds. All inputs own their data and implement `Clone`; request structs implement `Serialize`, response structs `Deserialize`. Optional request fields use `skip_serializing_if = "Option::is_none"`; otherwise this writer rejects JSON null. Optional response fields accept absence and null. Enums use `#[serde(rename_all = "snake_case")]`; response status enums add a unit `#[serde(other)] Unknown`, while input enums contain known values only. Unknown response fields are ignored.

| Type signature | Fields or wire contract |
|---|---|
| `AccountId`, `CustomerId`, `PaymentIntentId`, `RefundId`, `SubscriptionId`, `InvoiceId`, `BalanceTransactionId`, `EventId`, `PriceId`, `PaymentMethodId`, `ChargeId`, `FileId` | Distinct transparent String newtypes; serde string; `From<&str>`; no implicit Concord conversion from text to an ID. |
| `IdempotencyKey(String)` | Private field; transparent `Serialize`, `Clone`; only the validated constructor below is public. |
| `Currency(String)` | Transparent String with a validated input constructor, `Clone + Serialize + Deserialize`; `Currency::new(&str) -> Result<Self, std::io::Error>`. No float amounts. |
| `List<T>` | `data: Vec<T>, has_more: bool, url: String`; serde object, no default for data or has_more. |
| `SearchPage<T>` | `data: Vec<T>, has_more: bool, next_page: Option<String>, total_count: Option<u64>`; no default for data or has_more. |
| `StripeProblem` | `error: StripeError`; this is the outer error envelope, not the inner object. |
| `StripeError` | `kind: String` renamed from `type`; `code, decline_code, message, param: Option<String>`; `payment_intent: Option<PaymentIntent>`; arbitrary future type/code strings remain readable. |
| `Customer` | `id: CustomerId, email: Option<String>, name: Option<String>, metadata: BTreeMap<String,String>`; richer fields may be added. |
| `Deleted<I>` | `id: I, deleted: bool`; derived Deserialize requires the deleted field to be present. |
| `CustomerRecord` | Untagged `Deleted(Deleted<CustomerId>)` or `Live(Customer)`; tombstone variant first. |
| `PaymentIntent` | `id, amount: i64, currency, status: PaymentIntentStatus, customer: Option<Expandable<CustomerId,CustomerRecord>>, payment_method: Option<Expandable<PaymentMethodId,PaymentMethod>>, latest_charge: Option<Expandable<ChargeId,Charge>>, next_action: Option<serde_json::Value>`; id is PaymentIntentId. |
| `PaymentMethod`, `Charge` | Typed ID and provider's object fields; these are named supporting response models. |
| `Refund` | `id: RefundId, amount: i64, currency, status: RefundStatus, payment_intent: Option<Expandable<PaymentIntentId,PaymentIntent>>`. |
| `Subscription` | `id: SubscriptionId, customer: Expandable<CustomerId,CustomerRecord>, status: SubscriptionStatus, latest_invoice: Option<Expandable<InvoiceId,Invoice>>`. |
| `Invoice` | `id: InvoiceId, customer: Option<Expandable<CustomerId,CustomerRecord>>, status: Option<InvoiceStatus>, amount_due: i64, currency`. |
| `Balance` | `available, pending: Vec<BalanceAmount>`; `BalanceAmount` has `amount: i64, currency: Currency`. |
| `BalanceTransaction` | `id: BalanceTransactionId, amount: i64, fee: i64, net: i64, currency, created: i64, kind: String` renamed from `type`. |
| `Event` | `id: EventId, api_version: Option<String>, kind: String` renamed from `type`, `created: i64, data: EventData`; `EventData` has `object: serde_json::Value`. |
| `StripeFile` | `id: FileId, filename: Option<String>, purpose: String, size: u64, url: Option<String>`. |
| `UpdateCustomer` | Optional `email, name: String`, `metadata: BTreeMap<String,String>`; Default. |
| `NewPaymentIntent` | Required `amount: i64, currency: Currency`; optional `customer: CustomerId, payment_method: PaymentMethodId, confirm: bool, capture_method: CaptureMethod, metadata: BTreeMap<String,String>`. |
| `UpdatePaymentIntent` | Optional `amount: i64, customer: CustomerId, payment_method: PaymentMethodId, metadata: BTreeMap<String,String>`; Default. |
| `ConfirmPaymentIntent` | Optional `payment_method: PaymentMethodId, return_url: String, off_session: bool`; Default. |
| `CapturePaymentIntent` | Optional `amount_to_capture: i64, final_capture: bool`; Default. |
| `CancelPaymentIntent` | Optional `cancellation_reason: CancellationReason`; Default. |
| `NewRefund` | Required `target: RefundTarget`; optional `amount: i64, reason: RefundReason, metadata: BTreeMap<String,String>`; target is serde-flattened. |
| `RefundTarget` | Externally tagged `PaymentIntent(PaymentIntentId)` or `Charge(ChargeId)`, snake-case: exactly one top-level payment_intent or charge pair. |
| `UpdateRefund` | Optional `metadata: BTreeMap<String,String>`; Default. |
| `NewSubscription` | `customer: CustomerId, items: Vec<NewSubscriptionItem>`; optional `payment_behavior: PaymentBehavior, metadata: BTreeMap<String,String>`. |
| `NewSubscriptionItem` | `price: PriceId, quantity: Option<u64>`; serializes under items[index]. |
| `UpdateSubscription` | Optional `items: Vec<UpdateSubscriptionItem>, cancel_at_period_end: bool, proration_behavior: ProrationBehavior, metadata: BTreeMap<String,String>`; Default. |
| `UpdateSubscriptionItem` | Required `id: String`; optional `price: PriceId, quantity: u64, deleted: bool`. |
| `NewInvoice` | `customer: CustomerId`; optional `collection_method: CollectionMethod, days_until_due: u32, auto_advance: bool, metadata: BTreeMap<String,String>`. |
| `UpdateInvoice` | Optional `description: String, auto_advance: bool, metadata: BTreeMap<String,String>`; Default. |
| `FinalizeInvoice` | Optional `auto_advance: bool`; Default. |
| `PayInvoice` | Optional `payment_method: PaymentMethodId, paid_out_of_band: bool, off_session: bool`; Default. |
| `Upload` | `filename: String, content_type: String, bytes: bytes::Bytes`; Clone. |
| `SubscriptionStatusFilter` | `Incomplete, IncompleteExpired, Trialing, Active, PastDue, Canceled, Unpaid, Paused, All, Ended`; Serialize, snake_case. |
| `SubscriptionStatus` | Same actual states, without All/Ended, plus Unknown; Deserialize. |
| `InvoiceStatusFilter` | `Draft, Open, Paid, Uncollectible, Void`; Serialize. |
| `InvoiceStatus` | Same five actual states, plus Unknown; Deserialize. |
| `PaymentIntentStatus`, `RefundStatus` | Provider's documented object statuses, plus Unknown; Deserialize. |
| `CollectionMethod` | `ChargeAutomatically, SendInvoice`; Serialize. |
| `CaptureMethod` | `Automatic, Manual`; Serialize; a deliberate subset. |
| `CancellationReason` | `Duplicate, Fraudulent, RequestedByCustomer, Abandoned`; Serialize. |
| `RefundReason` | `Duplicate, Fraudulent, RequestedByCustomer`; Serialize. |
| `PaymentBehavior` | `DefaultIncomplete, AllowIncomplete, ErrorIfIncomplete`; Serialize; a create subset. |
| `ProrationBehavior` | `CreateProrations, None, AlwaysInvoice`; Serialize. |
| `FilePurpose` | `DisputeEvidence, IdentityDocument, BusinessLogo`; Serialize, snake_case; a deliberate subset. |

The error's type and code remain strings because Stripe can add values. No 402 arm consumes the problem: callers inspect `e.problem()` while retaining HTTP evidence. An expanded customer can itself be deleted. No endpoint maps 404 to absence. The successful customer tombstone is documented in [customer retrieval](https://docs.stripe.com/api/customers/retrieve).

The two small validated constructors live inside `model`; their entire behavior follows:

```rust
impl IdempotencyKey {
    pub fn new(value: impl Into<String>) -> std::io::Result<Self> {
        let value = value.into();
        if value.is_empty() || value.len() > 255
            || !value.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(std::io::Error::other("key must be 1..255 visible ASCII bytes"));
        }
        Ok(Self(value))
    }
}
impl Currency {
    pub fn new(value: &str) -> std::io::Result<Self> {
        if value.len() != 3 || !value.bytes().all(|b| b.is_ascii_lowercase()) {
            return Err(std::io::Error::other("currency must be three lowercase ASCII letters"));
        }
        Ok(Self(value.to_owned()))
    }
}
```

The nested form writer is deliberately separate from Concord's flat `form`. It emits scalar arrays as `expand[]=customer`, object arrays as `items[0][price]=price_…`, and maps as `metadata[order]=…`. The encoder escapes keys and values once; it does not pre-escape brackets. Empty strings survive, so `metadata[order]=` can clear that key. Empty containers emit nothing; this subset does not expose clearing the entire metadata map. Fields in request types never include their own `expand` field. Brackets are rejected inside individual map keys, matching [Stripe's metadata key restriction](https://docs.stripe.com/api/metadata).

```rust
use concord::{Answer, Body};
use concord::page::{Call, Slot};
use serde::Serialize;
use serde_json::Value;
use std::io;
use crate::model::*;

fn bad(message: &'static str) -> io::Error { io::Error::other(message) }

pub fn stripe_form<T: Serialize>(value: &T, expand: &Option<Vec<String>>) -> io::Result<Body> {
    let root = serde_json::to_value(value).map_err(io::Error::other)?;
    let Value::Object(fields) = root else { return Err(bad("form root must be an object")); };
    let mut pairs = Vec::new();
    for (key, value) in fields {
        key_part(&key)?;
        flatten(&key, &value, &mut pairs)?;
    }
    if let Some(paths) = expand {
        for path in paths { pairs.push(("expand[]".to_owned(), path.clone())); }
    }
    let encoded = serde_urlencoded::to_string(&pairs).map_err(io::Error::other)?;
    Ok(Body::new(encoded, "application/x-www-form-urlencoded"))
}

fn key_part(key: &str) -> io::Result<()> {
    if key.is_empty() || key.contains('[') || key.contains(']') {
        return Err(bad("form key components must be nonempty and contain no brackets"));
    }
    Ok(())
}

fn flatten(key: &str, value: &Value, out: &mut Vec<(String, String)>) -> io::Result<()> {
    match value {
        Value::Null => return Err(bad("omit absent form fields; JSON null has no Stripe form meaning")),
        Value::Bool(v) => out.push((key.to_owned(), v.to_string())),
        Value::Number(v) => out.push((key.to_owned(), v.to_string())),
        Value::String(v) => out.push((key.to_owned(), v.clone())),
        Value::Object(fields) => {
            for (part, child) in fields {
                key_part(part)?;
                flatten(&format!("{key}[{part}]"), child, out)?;
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                let child_key = if child.is_object() || child.is_array() {
                    format!("{key}[{index}]")
                } else { format!("{key}[]") };
                flatten(&child_key, child, out)?;
            }
        }
    }
    Ok(())
}
```

List paging uses `has_more` and the last object's ID, never page length. Search paging uses `has_more` and `next_page`; an empty search page can still advance. Both reject evidence that cannot support another request. The endpoint closures translate an author's error into `concord::mismatch`, avoiding an undocumented concrete return type for that function. The engine supplies the repeated-request guard. [Stripe search pagination](https://docs.stripe.com/api/pagination/search).

```rust
pub trait Identified {
    type Id: Clone;
    fn id(&self) -> &Self::Id;
}
impl Identified for Customer { type Id = CustomerId; fn id(&self) -> &Self::Id { &self.id } }
impl Identified for PaymentIntent { type Id = PaymentIntentId; fn id(&self) -> &Self::Id { &self.id } }
impl Identified for Refund { type Id = RefundId; fn id(&self) -> &Self::Id { &self.id } }
impl Identified for Subscription { type Id = SubscriptionId; fn id(&self) -> &Self::Id { &self.id } }
impl Identified for Invoice { type Id = InvoiceId; fn id(&self) -> &Self::Id { &self.id } }
impl Identified for BalanceTransaction { type Id = BalanceTransactionId; fn id(&self) -> &Self::Id { &self.id } }
impl Identified for Event { type Id = EventId; fn id(&self) -> &Self::Id { &self.id } }

pub fn list_next<C, S, T>(after: &S, call: C, answer: &Answer<List<T>>) -> Result<Option<C>, &'static str>
where C: Call<Value = List<T>>, T: Identified, S: Slot<C, Value = T::Id> {
    let page = answer.value();
    if !page.has_more { return Ok(None); }
    let last = page.data.last().ok_or("list has_more without a last object")?;
    Ok(Some(after.set(call, last.id().clone())))
}

pub fn search_next<C, S, T>(slot: &S, call: C, answer: &Answer<SearchPage<T>>) -> Result<Option<C>, &'static str>
where C: Call<Value = SearchPage<T>>, S: Slot<C, Value = String> {
    let page = answer.value();
    if !page.has_more { return Ok(None); }
    let token = page.next_page.as_ref().filter(|s| !s.is_empty())
        .ok_or("search has_more without next_page")?;
    Ok(Some(slot.set(call, token.clone())))
}

impl<T> IntoIterator for List<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter { self.data.into_iter() }
}
impl<'a, T> IntoIterator for &'a List<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter { self.data.iter() }
}
impl<T> IntoIterator for SearchPage<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter { self.data.into_iter() }
}
impl<'a, T> IntoIterator for &'a SearchPage<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter { self.data.iter() }
}
```

Multipart is only planned as a shipped helper. Here is the whole replacement, using the existing `Body::new` door. It buffers bytes for replay and imposes an author-selected 8 MiB limit, not a claim about Stripe's purpose-dependent limits. The fixed, checked boundary also avoids assuming an undocumented owned-string signature for `Body::new`'s content type. The filename policy is deliberately narrow. Stripe still validates file format and purpose requirements. Exact purpose-dependent limits are (unsure), so this description makes no promise that every file under the local cap is accepted. A memory body does not get the sheet's stream-progress resets of idle_timeout.

```rust
pub fn stripe_multipart(purpose: &FilePurpose, file: &Upload) -> io::Result<Body> {
    const BOUNDARY: &str = "concord-stripe-v5-20261112";
    const CONTENT_TYPE: &str = "multipart/form-data; boundary=concord-stripe-v5-20261112";
    if file.bytes.len() > 8 * 1024 * 1024 { return Err(bad("buffered upload exceeds 8 MiB")); }
    if file.filename.is_empty() || !file.filename.bytes().all(|b|
        b.is_ascii_graphic() && b != b'"' && b != b'\\') {
        return Err(bad("filename must be visible ASCII without quotes or backslashes"));
    }
    if file.content_type.is_empty() || !file.content_type.bytes().all(|b|
        (0x20..=0x7e).contains(&b)) {
        return Err(bad("content type must be printable ASCII"));
    }
    if file.bytes.windows(BOUNDARY.len()).any(|part| part == BOUNDARY.as_bytes()) {
        return Err(bad("upload contains this encoder's boundary"));
    }
    let purpose_json = serde_json::to_value(purpose).map_err(io::Error::other)?;
    let purpose = purpose_json.as_str().ok_or_else(|| bad("purpose must serialize as text"))?;
    let head = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\n{purpose}\r\n\
         --{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n\
         Content-Type: {}\r\n\r\n", file.filename, file.content_type);
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(&file.bytes);
    bytes.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    Ok(Body::new(bytes, CONTENT_TYPE))
}
```

An optional Stripe retry policy follows. The header veto overrides the usual retry table; a positive header, a 409, or a server error can request another attempt. Idempotency and replayability remain separate engine gates. This follows the provider SDK's header handling; retry is still an application choice. [Stripe's retry implementation](https://github.com/stripe/stripe-php/blob/master/lib/HttpClient/CurlClient.php).

```rust
pub fn retry() -> concord::Retry<StripeProblem> {
    use std::time::Duration;
    concord::Retry::standard()
        .unless(|tried| tried.headers().get("stripe-should-retry") == Some("false"))
        .when(|tried| {
            let hinted = tried.headers().get("stripe-should-retry") == Some("true");
            let transient = tried.status().is_some_and(|s| s == 409 || s.is_server_error());
            (hinted || transient).then_some(Duration::from_secs(1))
        })
}
```

`Event.data.object` intentionally stays JSON: changing `Stripe-Version` does not upgrade stored event payloads. A version-aware event decoder belongs beside this client; this subset does not pretend that every historical event is today's `PaymentIntent`. [Event retrieval and versioning](https://docs.stripe.com/api/events/list).

## Calling it

These are six independent async function bodies returning `Result<(), Box<dyn std::error::Error + Send + Sync>>`. IDs, files and operation identifiers shown are placeholders. None of this calling code was executed. Each body has the macro's imports plus the model names in scope; `wire::retry` is the helper above.

1. Create a customer with nested metadata, on a Connect account. `operation_id` is a durable application identifier, and the key is stored with the operation before sending.

```rust
use std::collections::BTreeMap;
let secret = std::env::var("STRIPE_SECRET_KEY")?;
let stripe = Stripe::new(secret.as_str(), "2026-10-28.endive")
    .account(AccountId::from("acct_example"))
    .with_retry(wire::retry())
    .with_timeout(std::time::Duration::from_secs(60));
let key = IdempotencyKey::new(format!("{operation_id}:customer"))?;
let customer = stripe.customers().create(NewCustomer {
    email: Some("buyer@example.com".into()),
    metadata: BTreeMap::from([("order".into(), operation_id.to_string())]),
    ..Default::default()
}, key).await?;
println!("{}", customer.id.0);
Ok(())
```

2. Confirm a payment; handle a refusal separately from successful customer-action requirements. `stripe`, `intent_id` and the persisted `confirm_key` are inputs.

```rust
match stripe.payment_intents().confirm(intent_id, ConfirmPaymentIntent {
    payment_method: Some(PaymentMethodId::from("pm_example")),
    return_url: Some("https://shop.example/complete".into()),
    ..Default::default()
}, confirm_key).await {
    Ok(intent) => { /* dispatch on intent.status and intent.next_action */ }
    Err(e) if e.is_status(402) => {
        if let Some(p) = e.problem() {
            eprintln!("payment refused: type={}, code={:?}", p.error.kind, p.error.code);
        } else { return Err(e.into()); }
    }
    Err(e) => return Err(e.into()),
}
Ok(())
```

3. Audit subscriptions and unpaid invoices. `.status(...)` is an API filter. Invoice filters use the documented customer/subscription/status and creation-time keys. [Invoice list](https://docs.stripe.com/api/invoices/list).

```rust
let mut subscriptions = stripe.subscriptions().list()
    .customer(&customer_id).status(SubscriptionStatusFilter::All).limit(100).items();
while let Some(sub) = subscriptions.try_next().await? { println!("{}", sub.id.0); }
let mut invoices = stripe.invoices().list().customer(customer_id)
    .status(InvoiceStatusFilter::Open).created_from(period_start).created_before(period_end)
    .limit(100).items();
while let Some(invoice) = invoices.try_next().await? { println!("{}", invoice.id.0); }
Ok(())
```

4. Search an indexed metadata value, keeping page tokens for a later resume. A saved token is only advanced after processing the page succeeds.

```rust
let mut pages = stripe.customers().search("metadata['order']:'order_42'")
    .page(saved_page_token).limit(100).pages();
while let Some(page) = pages.try_next().await? {
    for customer in &page { println!("{}", customer.id.0); }
    // Persist page.next_page with your processing checkpoint, not before processing.
    // has_more=false means completion; do not restart at page=None as a resume.
}
Ok(())
```

5. Expand a payment's customer. Expansion names start with `data.` on list responses; a single-object expansion is just `customer`. The caller still checks the enum. [Expansion rules](https://docs.stripe.com/api/expanding_objects).

```rust
let intent = stripe.payment_intents().get(PaymentIntentId::from("pi_example"))
    .expand(["customer".to_owned()]).await?;
match intent.customer {
    Some(Expandable::Object(customer)) => match *customer {
        CustomerRecord::Live(c) => println!("customer email: {:?}", c.email),
        CustomerRecord::Deleted(_) => println!("customer was deleted"),
    },
    Some(Expandable::Id(id)) => println!("only an ID returned: {}", id.0),
    None => println!("no customer attached"),
}
Ok(())
```

6. Upload dispute evidence to the file host with the same bearer/account settings. Bytes are buffered, so Concord can replay the complete multipart body with the same key. Keep `upload_key` with the upload operation.

```rust
let file = stripe.files().upload(FilePurpose::DisputeEvidence, Upload {
    filename: "receipt.pdf".into(),
    content_type: "application/pdf".into(),
    bytes: std::fs::read("receipt.pdf")?.into(),
}, upload_key).with_retry(wire::retry())
    .with_idle_timeout(std::time::Duration::from_secs(60))
    .with_timeout(std::time::Duration::from_secs(120)).await?;
println!("{}", file.id.0);
Ok(())
```

## Findings

| # | where | what got in the way | what you wrote instead | kind |
|---|---|---|---|---|
| 1 | Every form POST | Shipped form is flat; metadata maps and indexed subscription items do not serialize as Stripe's nested keys. The actual body schema disappears behind opaque Rust. | One complete flattening writer plus explicit serde omission rules. | said with pain |
| 2 | expand | An optional builder input cannot change the static arrow type or guarantee that customer is an object. | One fixed Expandable enum and a caller-side match, including deleted customers. | cannot be said |
| 3 | File multipart | The guide calls multipart planned; using that helper as if shipped would invent availability. Even an owned content-type argument is unspecified. | A buffered MIME encoder with a fixed checked boundary, filename restrictions and a local size limit. | said with pain |
| 4 | Large retryable uploads | Body::replayable is planned; a stream body gets one try even with a key and idempotent. | Restrict this endpoint's helper to buffered uploads. | cannot be said |
| 5 | Every POST | A required key, its header and idempotent are repeated at every leaf; none of body/idempotent is inheritable. | Repeat the declaration, with an operation key rather than a client-wide key. | said with pain |
| 6 | Every list | Limit, cursor, expansion and date inputs recur; parked parts cannot share those signatures. | Repeat inputs and wire keys, share only the pager function. | said with pain |
| 7 | Nested list filters | A created-range struct is not a scalar query value, and query has no spread/map entry. | Separate created_from/created_before inputs, placed at fixed bracketed keys. | said with pain |
| 8 | Fallible custom pagers | The guide shows concord::mismatch but never names its concrete return type, making a named fallible factory hard to sign. | Helpers return static-text errors; each endpoint closure maps them through mismatch. | said with pain |
| 9 | Stripe-Version | A signature has no syntax for a pinned but overridable default; an entry expression would hide that choice. Arbitrary versions also imply model compatibility work. | Require the version at construction and document the model contract. | small |
| 10 | Subscription status | Older arm prose suggests status is implicitly reserved; I reread it before trusting a filter called status. | Use status normally; the sheet requires explicit arm bindings and this endpoint has no arm. | small |
| 11 | Customer deletion | The tempting Option<Customer> loses Stripe's successful deleted-object response; 404 is not a tombstone. | CustomerRecord for retrieval and Deleted for deletion, retaining normal 404 errors. | small |
| 12 | Event payload versions | The request's version header cannot make historical data.object match one current resource type. | Keep EventData.object as JSON; version-specific decoding is outside this subset. | said with pain |
| 13 | Idempotency retention | idempotent is a fact or an input expression; there is no per-try expiry tied to the original operation's first attempt across calls. | Document the 24-hour retry window, persist keys, bound individual calls, leave cross-call reconciliation to the application. | cannot be said |
| 14 | Stripe retry hints | Concord's standard table is not Stripe's header-aware retry policy, and policies are opt-in. | Publish the complete retry helper and select it explicitly in calling code. | small |

Wishes for every substantial finding (intent, not proposed legal syntax):

- **1:** I wished for a shipped nested-form writer whose Stripe bracket convention could be selected once.
- **2:** I wished that choosing an expansion could produce a typed call promising the corresponding object field.
- **3:** I wished for an available multipart value with safe boundary and filename handling.
- **4:** I wished for the planned replayable-body factory, so a large file could reopen for each attempt.
- **5:** I wished for reusable POST request options that require a distinct key per operation without repeating the wire declaration.
- **6:** I wished for reusable list inputs that preserve the same flat caller methods.
- **7:** I wished for a typed query-range writer that expands a range into bracketed keys.
- **8:** I wished for the public return type of mismatch and a complete signature for a fallible pager.
- **12:** I wished for a documented version-aware serde recipe for event payloads, beside the ordinary reader door.
- **13:** I wished for a retry deadline tied to an operation's first attempt, preserved across resumed calls.

## Read twice

| Question | Answer and where |
|---|---|
| Which arm spelling is current? | The phase-C sheet, item 1: explicit body/headers/status pattern; the response chapter and grammar still show the removed spelling. No custom arms were needed here. |
| Is an input named status legal? | Sheet item 1 removes implicit arm variables; language §3.7 restricts actual method-name collisions, not this filter. The older response §5.5 explanation misled me initially. |
| Can one scope make all POSTs idempotent? | Grammar's position table: endpoint only. Sheet item 2 adds a bool expression but does not change that position. |
| Should a retry create another key? | Behaviour §6.3: build once, copy request per try. Keys are inputs; transport must not regenerate them. Stripe's retention window is an additional provider constraint. |
| Can form handle metadata and items? | Response §5.8 explicitly says form is flat; use an author's writer. |
| Is multipart actually available? | Response §5.8 and scope §8.4: planned. Use Body::new, not an assumed concord::multipart function. |
| Does changing base erase an inherited path? | Rules 5 and 10: nearest base, then every inherited path. Keep v1 in resource scopes. |
| Can arbitrary query-map entries be spread from one input? | Language §3.7 permits scalar/list values only; scope §8.3 parks a map of pairs. Fixed bracketed keys are legal. |
| Are slots optional values or values inside Option? | Behaviour §6.4's Slot<Value=u32> example: Value is the inner type; get can return absence. In paginate the input names its place. |
| What exact error type does a fallible pager return? | Not found in the permitted guide. §6.4 demonstrates inference through concord::mismatch; the closures here retain that inference. |
| Which iteration does items require? | Usage §4.9 and behaviour §6.4 say IntoIterator without a full adapter signature. Both owned and borrowed iteration are supplied. |
| Does every short list end paging? | Behaviour §6.4: only offset_limit uses length; a cursor pager trusts the provider's continuation evidence. |
| Must Stripe get a custom transport or reader? | Tour §8 and usage §4.8: static bearer placement is enough; response §5.3: Json reads ordinary JSON. No invented Stripe transport. |
| Which HTTP constructor is current? | Sheet item 5: concord::Http, not the older client_builder examples. |
| Can a retryable file be streamed now? | Response §5.8 and scope §8.4: no; streams are one-shot, replayable factories are planned. Sheet item 7 changes upload progress accounting, not replayability. |
| Can the signature declare a default version and an override option? | Scope §8.3 parks written defaults. An entry expression could compute one, but this description requires the version explicitly. |
| Does a reader add Accept or require a JSON label? | Sheet's kept rules and response §5.3: neither by default. This API needs no extra Accept entry. |
| Is Body::new's content type borrowed, owned, or parsed? | Not found beyond literal examples in response §5.8. The multipart helper uses a literal and a checked fixed boundary. |

## Scores

- **Writing this description: 6/10.** The resource tree is straightforward, but repeated signatures and author-owned wire encoders carry much of Stripe's identity outside it.
- **Reading it back a month later: 7/10.** Paths and operation names remain visible; understanding body semantics requires returning to the helpers and serde contracts.
- **The calling code: 8/10.** Filters and pagination read naturally, while explicit keys and expansion matching are useful but persistent ceremony.
