# the Stripe API in Concord

Written by Claude Opus 5.5 (`claude-opus-5-5`, the id the harness gives) from the Concord V5 guide alone (the tour, chapters 3 to 6, the grammar, the rules, the tables of chapter 8), the list of endpoints of [the first version](first/stripe.md), and what I know of Stripe's public documentation, on 2026-11-12. Nothing was compiled, and nothing was sent to Stripe.

## The description

Forty endpoints, on two hosts. Four parts carry what Stripe repeats: the list inputs and their pager, the search inputs and theirs, `expand` in the query, and the idempotency key of every `POST`. The description has no `middleware`: finding 10 says why.

```rust
use concord::{by_cursor, Json, Secret};
use crate::model::*;
use crate::wire::{by_object_id, multipart, new_idempotency_key, stripe_form, API_VERSION};

concord::api! {
    /// Stripe, API v1: customers, payments, refunds, billing, the balance, events, files.
    pub client Stripe(
        /// A secret key (`sk_live_…`, `sk_test_…`) or a restricted one (`rk_…`).
        key: Secret,
        /// A connected account: the request is made as that account (Connect).
        account?: AccountId,
        /// The API version. When it is not given: `API_VERSION`, the one these types were written for.
        version?: String,
    ) {
        base "https://api.stripe.com";
        path "v1";
        headers {
            "authorization" = "Bearer {key}";
            "stripe-version" = version.as_deref().unwrap_or(API_VERSION);
            "stripe-account" = account;
        }
        read Json;
        problem StripeProblem;

        /// Every list: at most `limit` objects, after (or before) the object with a given id.
        part listed(
            /// 1 to 100. Stripe's default is 10.
            limit?: u32,
            /// The id of an object: the page starts after it.
            starting_after?: String,
            /// The id of an object: the page ends before it, and the pages go back in time.
            ending_before?: String,
            /// Created at or after this time (Unix seconds).
            created_gte?: i64,
            /// Created before this time (Unix seconds).
            created_lt?: i64,
        ) {
            query {
                limit; starting_after; ending_before;
                "created[gte]" = created_gte;
                "created[lt]" = created_lt;
            }
            paginate by_object_id(starting_after, ending_before);
        }

        /// Every search: Stripe's query language, in pages by an opaque token.
        /// Not for checking a write: an object is found about a minute after it changed.
        part found(
            /// `email:'ada@example.com'`, `metadata['order']:'7781'`, `status:'succeeded'`.
            query: String,
            /// 1 to 100. Stripe's default is 10.
            limit?: u32,
            /// The `next_page` of the page before.
            page?: String,
        ) {
            query { query; limit; page; }
            paginate by_cursor(page, SearchResult::next_token);
        }

        /// What a `GET` or a `DELETE` expands: in the query.
        part expanded(
            /// Ids in the answer to give as whole objects: `customer`; in a list, `data.customer`.
            expand?: Vec<String>,
        ) {
            query { "expand[]" = expand; }
        }

        /// Every `POST`. A `POST` sends `expand` in its form body, which a part cannot reach:
        /// each node names `expand` in its own `body`.
        part write(
            /// The same key gives the same answer for at least 24 hours. Give one to send an operation
            /// again after a crash; without one, a key is made for the call, and every try sends it.
            idempotency_key?: String,
            /// As in `expanded`, but sent in the body.
            expand?: Vec<String>,
        ) {
            headers { "idempotency-key" = idempotency_key.clone().unwrap_or_else(new_idempotency_key); }
            idempotent;
        }

        scope customers {
            path "customers";

            POST create(customer: NewCustomer, ..write) -> Customer { body stripe_form(customer, expand); }
            /// `None` when the customer was deleted: Stripe answers `200` with `deleted: true`.
            /// An id that never existed is still a `404`.
            GET retrieve(id: CustomerId, ..expanded) -> Option<Customer> {
                path "{id}";
                status { 200 { body } => MaybeDeleted::live(body) }
            }
            POST update(id: CustomerId, changes: UpdateCustomer, ..write) -> Customer {
                path "{id}";
                body stripe_form(changes, expand);
            }
            DELETE delete(id: CustomerId) -> Deleted<CustomerId> { path "{id}"; }
            GET list(email?: String, ..listed, ..expanded) -> List<Customer> { query { email; } }
            GET search(..found, ..expanded) -> SearchResult<Customer> { path "search"; }
        }

        scope payment_intents {
            path "payment_intents";

            POST create(intent: NewPaymentIntent, ..write) -> PaymentIntent { body stripe_form(intent, expand); }
            GET retrieve(id: PaymentIntentId, ..expanded) -> PaymentIntent { path "{id}"; }
            POST update(id: PaymentIntentId, changes: UpdatePaymentIntent, ..write) -> PaymentIntent {
                path "{id}";
                body stripe_form(changes, expand);
            }
            /// `Err` when the card was declined (`402`); the intent is in `error.payment_intent`.
            /// An `Ok` may still wait for the customer: look at `status` and `next_action`.
            POST confirm(id: PaymentIntentId, params?: ConfirmPaymentIntent, ..write) -> Result<PaymentIntent, Declined> {
                path "{id}/confirm";
                body stripe_form(params, expand);
                status { 402 { body } => Err(body) }
            }
            POST capture(id: PaymentIntentId, params?: CapturePaymentIntent, ..write) -> PaymentIntent {
                path "{id}/capture";
                body stripe_form(params, expand);
            }
            POST cancel(id: PaymentIntentId, params?: CancelPaymentIntent, ..write) -> PaymentIntent {
                path "{id}/cancel";
                body stripe_form(params, expand);
            }
            GET list(customer?: CustomerId, ..listed, ..expanded) -> List<PaymentIntent> { query { customer; } }
            GET search(..found, ..expanded) -> SearchResult<PaymentIntent> { path "search"; }
        }

        scope refunds {
            path "refunds";

            /// The body names the payment: `payment_intent` or `charge`.
            POST create(refund: NewRefund, ..write) -> Refund { body stripe_form(refund, expand); }
            GET retrieve(id: RefundId, ..expanded) -> Refund { path "{id}"; }
            POST update(id: RefundId, changes: UpdateRefund, ..write) -> Refund {
                path "{id}";
                body stripe_form(changes, expand);
            }
            /// Only a refund whose status is `requires_action`.
            POST cancel(id: RefundId, ..write) -> Refund {
                path "{id}/cancel";
                body stripe_form(&(), expand);
            }
            GET list(charge?: ChargeId, payment_intent?: PaymentIntentId, ..listed, ..expanded) -> List<Refund> {
                query { charge; payment_intent; }
            }
        }

        scope subscriptions {
            path "subscriptions";

            POST create(subscription: NewSubscription, ..write) -> Subscription { body stripe_form(subscription, expand); }
            GET retrieve(id: SubscriptionId, ..expanded) -> Subscription { path "{id}"; }
            POST update(id: SubscriptionId, changes: UpdateSubscription, ..write) -> Subscription {
                path "{id}";
                body stripe_form(changes, expand);
            }
            /// Cancels now. To cancel at the end of the period: `update` with `cancel_at_period_end`.
            /// The options of a `DELETE` go in the query (unsure).
            DELETE cancel(id: SubscriptionId, invoice_now?: bool, prorate?: bool, ..expanded) -> Subscription {
                path "{id}";
                query { invoice_now; prorate; }
            }
            /// With no `status`, every subscription that is not canceled.
            GET list(customer?: CustomerId, price?: PriceId, status?: SubscriptionFilter, ..listed, ..expanded) -> List<Subscription> {
                query { customer; price; status; }
            }
        }

        scope invoices {
            path "invoices";

            POST create(invoice: NewInvoice, ..write) -> Invoice { body stripe_form(invoice, expand); }
            GET retrieve(id: InvoiceId, ..expanded) -> Invoice { path "{id}"; }
            POST update(id: InvoiceId, changes: UpdateInvoice, ..write) -> Invoice {
                path "{id}";
                body stripe_form(changes, expand);
            }
            /// Only a draft. An invoice that was finalized is voided instead.
            DELETE delete(id: InvoiceId) -> Deleted<InvoiceId> { path "{id}"; }
            GET list(
                customer?: CustomerId,
                subscription?: SubscriptionId,
                status?: InvoiceStatus,
                collection_method?: CollectionMethod,
                ..listed,
                ..expanded
            ) -> List<Invoice> {
                query { customer; subscription; status; collection_method; }
            }
            POST finalize(id: InvoiceId, params?: FinalizeInvoice, ..write) -> Invoice {
                path "{id}/finalize";
                body stripe_form(params, expand);
            }
            /// `Err` when the payment was declined (`402`).
            POST pay(id: InvoiceId, params?: PayInvoice, ..write) -> Result<Invoice, Declined> {
                path "{id}/pay";
                body stripe_form(params, expand);
                status { 402 { body } => Err(body) }
            }
            POST void(id: InvoiceId, ..write) -> Invoice {
                path "{id}/void";
                body stripe_form(&(), expand);
            }
            POST mark_uncollectible(id: InvoiceId, ..write) -> Invoice {
                path "{id}/mark_uncollectible";
                body stripe_form(&(), expand);
            }
            /// Emails the invoice to the customer. (Stripe's libraries call it `send_invoice`,
            /// a name that Concord keeps for itself.)
            POST send(id: InvoiceId, ..write) -> Invoice {
                path "{id}/send";
                body stripe_form(&(), expand);
            }
        }

        /// What the account holds: available and pending, by currency.
        GET balance() -> Balance { path "balance"; }

        scope balance_transactions {
            path "balance_transactions";

            GET retrieve(id: BalanceTransactionId, ..expanded) -> BalanceTransaction { path "{id}"; }
            GET list(
                payout?: PayoutId,
                source?: String,
                /// `charge`, `refund`, `payout`, … (Stripe's `type`).
                kind?: String,
                currency?: Currency,
                ..listed,
                ..expanded
            ) -> List<BalanceTransaction> {
                query { payout; source; "type" = kind; currency; }
            }
        }

        /// Kept 30 days. `data.object` is written in the API version of the time it happened.
        scope events {
            path "events";

            GET retrieve(id: EventId) -> Event { path "{id}"; }
            GET list(
                /// One type, or a family: `invoice.*` (Stripe's `type`).
                kind?: String,
                /// Up to 20 types.
                types?: Vec<String>,
                delivery_success?: bool,
                ..listed
            ) -> List<Event> {
                query { "type" = kind; "types[]" = types; delivery_success; }
            }
        }

        /// A file, on the files host: dispute evidence, an identity document, a logo.
        /// The bytes are held in memory, so a second try sends them again, with the same key.
        POST create_file(purpose: FilePurpose, file: Upload, ..write) -> StripeFile {
            base "https://files.stripe.com";
            path "files";
            headers { override "authorization" = "Bearer {client.key}"; }
            body multipart(purpose, file, expand);
        }
    }
}
```

What the tree gives the user, with the blocks hidden:

```text
client Stripe(key: Secret, account?: AccountId, version?: String)
    scope customers              create retrieve update delete list search
    scope payment_intents        create retrieve update confirm capture cancel list search
    scope refunds                create retrieve update cancel list
    scope subscriptions          create retrieve update cancel list
    scope invoices               create retrieve update delete list finalize pay void mark_uncollectible send
    GET balance() -> Balance
    scope balance_transactions   retrieve list
    scope events                 retrieve list
    POST create_file(purpose: FilePurpose, file: Upload, ..write) -> StripeFile     (files.stripe.com)
```

Choices that are not findings. `path "v1"` stands on the client, so the files host gets `/v1/files` without a word: the nearest `base` starts the URL and every `path` above follows it. Every `POST` is `idempotent` because every `POST` sends a key, made for the call when the caller gave none; `Retry` may then send any of them again, and it is still off until the caller sets it. A `DELETE` takes no key: Stripe ignores one there. Names are Stripe's (`retrieve`, not `get`). Arrays nested in a body are written with indexes (`items[0][price]`), as Stripe's libraries write them (unsure); `expand[]` keeps the empty brackets that Stripe's own examples use.

## Beside the macro

The crate has two modules: `model`, the data types, and `wire`, what the description names. Dependencies beside Concord: `serde`, `serde_json`, `serde_urlencoded`, `uuid`, `bytes`.

**The data types**, by name. All are plain `serde` types; an enum of states that Stripe can extend ends with `#[serde(other)] Unknown`.

| Kind | Types |
|---|---|
| Ids: a `String` each, `#[serde(transparent)]`, `From<&str>`, `as_str()`, `Display` | `AccountId`, `CustomerId`, `PaymentIntentId`, `RefundId`, `SubscriptionId`, `InvoiceId`, `BalanceTransactionId`, `EventId`, `ChargeId`, `PriceId`, `PayoutId` |
| What is sent: `Serialize`, `Default` where Stripe requires nothing; an optional field is an `Option`; `metadata: BTreeMap<String, String>` on most | `NewCustomer`, `UpdateCustomer`, `NewPaymentIntent`, `UpdatePaymentIntent`, `ConfirmPaymentIntent`, `CapturePaymentIntent`, `CancelPaymentIntent`, `NewRefund`, `UpdateRefund`, `NewSubscription` (`items: Vec<NewItem>`), `UpdateSubscription`, `NewInvoice`, `UpdateInvoice`, `FinalizeInvoice`, `PayInvoice` |
| Filters and small values | `Currency` (three lowercase letters), `SubscriptionFilter` (the states, and `all`, `ended`), `InvoiceStatus`, `CollectionMethod`, `FilePurpose` (with `as_str()`), `Upload { name, content_type, bytes: Bytes }` |
| What comes back: `Deserialize` | `Customer`, `PaymentIntent` (`status: PaymentIntentStatus`, `customer: Option<Expandable<Customer>>`, `latest_charge: Option<Expandable<Charge>>`, `next_action: Option<serde_json::Value>`), `Charge`, `Refund`, `Subscription`, `Invoice`, `Balance`, `BalanceTransaction`, `Event` (`data.object: serde_json::Value`), `StripeFile`, `Deleted<I> { id, deleted }`, `MaybeDeleted<T>` (untagged: `Deleted { id, deleted }` or `Live(T)`), `List<T> { data, has_more, url }`, `SearchResult<T> { data, has_more, next_page, total_count }` |
| Refusals | `StripeProblem { error: StripeError }`, `Declined { error: StripeError }`, `ErrorKind` |

Two in full:

```rust
/// The inside of every refusal, `{ "error": { … } }`: of `StripeProblem` and of `Declined`.
#[derive(Debug, Clone, Deserialize)]
pub struct StripeError {
    /// `card_error`, `invalid_request_error`, `idempotency_error`, `api_error`, or `Unknown`.
    #[serde(rename = "type")]
    pub kind: ErrorKind,
    /// `card_declined`, `resource_missing`, … Stripe adds codes, so this is text.
    pub code: Option<String>,
    /// For a card: `insufficient_funds`, `lost_card`, …
    pub decline_code: Option<String>,
    pub message: Option<String>,
    /// The parameter that was wrong, in the form's own spelling: `items[0][price]`.
    pub param: Option<String>,
    pub doc_url: Option<String>,
    /// The intent of a declined payment, as it is now.
    pub payment_intent: Option<Box<PaymentIntent>>,
}

/// A field that `expand` turns from an id into the object. The type after the arrow cannot follow
/// `.expand(…)`, so the field holds both shapes (finding 2).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Expandable<T> {
    Id(String),
    Object(Box<T>),
}

impl<T: Object> Expandable<T> {
    pub fn id(&self) -> &str { match self { Self::Id(id) => id, Self::Object(object) => object.id() } }
    pub fn object(&self) -> Option<&T> { match self { Self::Object(object) => Some(object), Self::Id(_) => None } }
}
```

**The body writers.** The shipped `form` is flat (chapter 5, section 5.8), and Concord ships no multipart writer (chapter 8, section 8.4). Both are functions that give a `concord::Body`.

```rust
use std::error::Error;
use concord::Body;
use serde::Serialize;
use serde_json::Value;

type BoxError = Box<dyn Error + Send + Sync>;

/// The version these types were written for (unsure: the name of a version of late 2026).
pub const API_VERSION: &str = "2026-10-28.endive";

/// A key for a call that was given none. An entry runs once for a call, so every try sends this one.
pub fn new_idempotency_key() -> String { uuid::Uuid::new_v4().to_string() }

/// `body stripe_form(params, expand);`: Stripe's form, with nested keys: `metadata[order]=7781`,
/// `items[0][price]=price_1`, `expand[]=customer`. A `None` is left out; an empty text is sent,
/// which is how Stripe clears a field.
pub fn stripe_form<T: Serialize>(params: &T, expand: &Option<Vec<String>>) -> Result<Body, BoxError> {
    let mut pairs = Vec::new();
    match serde_json::to_value(params)? {
        Value::Null => {}                              // `&()`, or an optional input that was not set
        Value::Object(fields) => for (key, value) in fields { flatten(key, value, &mut pairs) },
        _ => return Err("the parameters of a POST are a struct".into()),
    }
    for path in expand.iter().flatten() {              // here only because a part cannot reach a body
        pairs.push(("expand[]".to_owned(), path.clone()));
    }
    Ok(Body::new(serde_urlencoded::to_string(&pairs)?, "application/x-www-form-urlencoded"))
}

fn flatten(key: String, value: Value, pairs: &mut Vec<(String, String)>) {
    match value {
        Value::Null => {}
        Value::Bool(b) => pairs.push((key, b.to_string())),
        Value::Number(n) => pairs.push((key, n.to_string())),
        Value::String(s) => pairs.push((key, s)),
        Value::Array(items) => {
            for (i, item) in items.into_iter().enumerate() { flatten(format!("{key}[{i}]"), item, pairs) }
        }
        Value::Object(fields) => for (k, v) in fields { flatten(format!("{key}[{k}]"), v, pairs) },
    }
}

/// `body multipart(purpose, file, expand);`: the one `multipart/form-data` body of this API.
/// In memory, so a second try sends the same bytes.
pub fn multipart(purpose: &FilePurpose, file: &Upload, expand: &Option<Vec<String>>) -> Result<Body, BoxError> {
    use std::io::Write;
    if file.name.contains(['"', '\r', '\n']) { return Err("a file name with a quote or a line break".into()); }
    let boundary = format!("concord-{}", uuid::Uuid::new_v4().simple());
    let mut fields = vec![("purpose", purpose.as_str().to_owned())];
    fields.extend(expand.iter().flatten().map(|path| ("expand[]", path.clone())));   // as in `stripe_form`
    let mut out = Vec::new();
    for (name, value) in fields {
        write!(out, "--{boundary}\r\ncontent-disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")?;
    }
    write!(out, "--{boundary}\r\ncontent-disposition: form-data; name=\"file\"; filename=\"{}\"\r\n", file.name)?;
    write!(out, "content-type: {}\r\n\r\n", file.content_type)?;
    out.extend_from_slice(&file.bytes);
    write!(out, "\r\n--{boundary}--\r\n")?;
    // (unsure) that `Body::new` takes bytes and a label that is a `String`: the pages show text and a literal
    Ok(Body::new(out, format!("multipart/form-data; boundary={boundary}")))
}
```

**The pager of the lists.** Stripe's cursor is the id of the last object; with `ending_before` it is the first one, and the pages go back. The shipped `by_cursor` covers the first case only, and cannot refuse a page, so this one is written like the guide's `more`.

```rust
use concord::page::{Call, Slot};
use concord::{Answer, Fail};

/// What the pager needs of an object: its id.
pub trait Object { fn id(&self) -> &str; }
macro_rules! object { ($($t:ty),*) => { $(impl Object for $t { fn id(&self) -> &str { self.id.as_str() } })* } }
object!(Customer, PaymentIntent, Charge, Refund, Subscription, Invoice, BalanceTransaction, Event);

/// `paginate by_object_id(starting_after, ending_before);`. The next page starts after the last
/// object; when the caller went back with `ending_before`, it ends before the first one.
/// `has_more` with no object is not the end of the list: the pages end with a Mismatch.
pub fn by_object_id<C, A, B, T>(after: A, before: B)
    -> impl Fn(C, &Answer<C::Value>) -> Result<Option<C>, Fail<C::Problem>> + Send + 'static
where
    C: Call<Value = List<T>>,
    A: Slot<C, Value = String>,
    B: Slot<C, Value = String>,
    T: Object,
{
    move |call, answer| {
        let page = answer.value();
        if !page.has_more { return Ok(None); }
        let back = before.get(&call).is_some();               // (unsure) the form of `Slot::get`
        let edge = if back { page.data.first() } else { page.data.last() };
        let Some(id) = edge.map(|object| object.id().to_owned()) else {
            return Err(concord::mismatch("has_more, and no object to go on from"));
        };
        Ok(Some(if back { before.set(call, id) } else { after.set(call, id) }))
    }
}

impl<T> SearchResult<T> {
    /// For `by_cursor(page, …)`: the token of the next page, while Stripe says there is one.
    pub fn next_token(&self) -> Option<String> { self.next_page.clone().filter(|_| self.has_more) }
}

impl<T> MaybeDeleted<T> {
    /// For the arm of `customers.retrieve`: the path names the type of `body`.
    pub fn live(self) -> Option<T> { match self { Self::Live(object) => Some(object), Self::Deleted { .. } => None } }
}

// For `.pages().items()`. (unsure) The pages name `IntoItems` and do not show its method.
impl<T> concord::page::IntoItems for List<T> { type Item = T; fn into_items(self) -> Vec<T> { self.data } }
impl<T> concord::page::IntoItems for SearchResult<T> { type Item = T; fn into_items(self) -> Vec<T> { self.data } }
```

**Retry, and telemetry: two values for the caller.** Neither is a fact that every caller must share, so neither is in the description.

```rust
/// Stripe's advice on each answer, `Stripe-Should-Retry`, and a `409` (one key, two requests at once).
/// Every `POST` here is `idempotent`, so each of them may be sent again.
pub fn retry() -> concord::Retry<StripeProblem> {
    concord::Retry::standard()
        .unless(|tried| should_retry(tried.headers()) == Some(false))
        .when(|tried| {
            let asked = should_retry(tried.headers()) == Some(true);
            let busy = tried.status().is_some_and(|s| s == 409);
            (asked || busy).then(|| Duration::from_millis(500))
        })
}

fn should_retry(headers: Option<&concord::Headers>) -> Option<bool> {    // (unsure) the type of `tried.headers()`
    match headers?.get("stripe-should-retry")? { "true" => Some(true), "false" => Some(false), _ => None }
}

/// Stripe's client telemetry, which its own libraries send unless they are told not to: each request
/// says how long the one before took. `Stripe::new(key).with_middleware(Telemetry::default())`.
#[derive(Default)]
pub struct Telemetry { last: std::sync::Mutex<Option<(String, u128)>> }

impl concord::Middleware for Telemetry {
    async fn handle(&self, mut request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        let last = self.last.lock().unwrap().take();
        if let Some((id, ms)) = last {
            let value = format!(r#"{{"last_request_metrics":{{"request_id":"{id}","request_duration_ms":{ms}}}}}"#);
            if let Ok(value) = value.parse() { request.headers_mut().insert("x-stripe-client-telemetry", value); }
        }
        let started = std::time::Instant::now();
        let answer = next.send(request, info).await?;
        if let Some(id) = answer.headers().get("request-id").and_then(|v| v.to_str().ok()) {
            *self.last.lock().unwrap() = Some((id.to_owned(), started.elapsed().as_millis()));
        }
        Ok(answer)
    }
}
```

## Calling it

Six pieces of an `async` function that returns `Result<(), Box<dyn Error + Send + Sync>>`, with `use my_stripe::*;` (the client, the types, `retry`, `Telemetry`). Ids, keys and names are placeholders. Nothing here was run.

**1. A client, and a customer made once.** The key is the order's: after a crash, the second run gets the first customer back.

```rust
let stripe = Stripe::new(std::env::var("STRIPE_SECRET_KEY")?)
    .with_retry(retry())
    .with_timeout(Duration::from_secs(80));

let customer = stripe.customers()
    .create(NewCustomer {
        email: Some("ada@example.com".into()),
        metadata: BTreeMap::from([("order".into(), "7781".into())]),
        ..Default::default()
    })
    .idempotency_key("customer-for-order-7781")
    .await?;
```

**2. Confirm a payment.** A decline is a value of the domain; any other refusal is an error.

```rust
match stripe.payment_intents().confirm(&intent_id)
    .params(ConfirmPaymentIntent { payment_method: Some(card), return_url: Some(back_url), ..Default::default() })
    .idempotency_key(format!("confirm-{order}"))
    .await?
{
    Ok(intent) if intent.status == PaymentIntentStatus::RequiresAction => redirect(intent.next_action),
    Ok(intent) => fulfil(intent),
    Err(declined) => tell_buyer(declined.error.decline_code.as_deref().unwrap_or("declined")),
}
```

**3. Every open invoice of a customer**, all pages, one invoice at a time.

```rust
let mut open = stripe.invoices().list()
    .customer(&customer.id)
    .status(InvoiceStatus::Open)
    .limit(100)
    .pages()
    .items();
while let Some(invoice) = open.try_next().await? {
    println!("{} {} {}", invoice.id, invoice.amount_due, invoice.currency);
}
```

**4. A search, one page on purpose**, from the token that the last run saved.

```rust
let page = stripe.payment_intents()
    .search("status:'succeeded' AND metadata['order']:'7781'")
    .page(saved_token)                                     // an `Option<String>`: `None` is the first page
    .limit(50)
    .send()
    .await?;
saved_token = page.next_token();
```

**5. Expand, and a customer that may be gone.**

```rust
let intent = stripe.payment_intents().retrieve(&intent_id).expand(["customer", "latest_charge"]).await?;
if let Some(Expandable::Object(customer)) = &intent.customer {
    println!("{:?}", customer.email);
}

match stripe.customers().retrieve(&customer_id).await {
    Ok(Some(customer)) => println!("{}", customer.id),
    Ok(None) => println!("deleted"),
    Err(e) if e.is_status(404) => println!("no such customer (a test id with a live key?)"),
    Err(e) => {
        let code = e.problem().and_then(|p| p.error.code.clone());
        tracing::warn!("{e}; code {code:?}");                // `e.headers()` holds `request-id`, for Stripe's support
        return Err(e.into());
    }
}
```

**6. Connect: evidence for a dispute of a connected account**, on the files host, with telemetry on.

```rust
let shop = Stripe::new(platform_key)
    .with_middleware(Telemetry::default())
    .account(AccountId::from("acct_123"));                 // `Stripe-Account` on every request of `shop`

let evidence = shop
    .create_file(FilePurpose::DisputeEvidence, Upload {
        name: "receipt.pdf".into(),
        content_type: "application/pdf".into(),
        bytes: tokio::fs::read("receipt.pdf").await?.into(),
    })
    .with_idle_timeout(Duration::from_secs(120))           // a body in memory goes out in one piece
    .await?;
```

## Findings

| # | Where | What got in the way | What I wrote instead | Kind |
|---|---|---|---|---|
| 1 | `expand` on the 20 `POST`s | `expand` has two places: the query of a `GET` or a `DELETE`, the form body of a `POST`. A part fixes the place, and a part cannot add to a body: `body` is one expression of the node. | Two parts with an `expand` each. `write` declares it and places nothing; each `POST` names it in its own `body stripe_form(x, expand)`, a name that its signature does not show. Four `POST`s with nothing to send write `body stripe_form(&(), expand)` only to carry it. That a part's input may be placed by the node alone I read from rule 25 ("as if it had written them"); no page says it outright. | said with pain |
| 2 | Every answer with an id that `expand` can open | The type after the arrow is fixed: `.expand(["customer"])` cannot make `intent.customer` a `Customer`. | `Expandable<T>`, an id or the object, and a `match` at the call: chapter 8's door. | cannot be said |
| 3 | `version` on the client; `idempotency_key` in `write` | A default is owned by an entry (`unwrap_or(API_VERSION)`, `unwrap_or_else(new_idempotency_key)`), so the signature cannot show it, and the page of an endpoint (chapter 4, section 4.11) would say "not sent unless given" for two headers that are always sent (unsure what the macro writes there). | The defaults in the entries, and again by hand in the `///` of each input. | said with pain |
| 4 | Every form body | The shipped `form` is flat; Stripe nests (`metadata[order]`, `items[0][price]`). | `stripe_form` and `flatten`, 24 lines: the door that section 5.8 names. | small |
| 5 | `create_file` | Concord ships no multipart writer (section 8.4). | `multipart`, 17 lines, in memory so that a second try can send it. | small |
| 6 | `create_file` | The bearer token does not follow the files host. | `override "authorization" = "Bearer {client.key}";`, as the guide shows. Version, account and key follow as they should. | small |
| 7 | `invoices.send`, `balance` | Stripe's libraries say `send_invoice`, which Concord refuses (`send_`). `send` is legal on a handle by the letter of rule 24 (a handle has only `call` and `clone`); I read it twice. The call reads `.send(&id).await`, and its type would be `stripe::invoices::Send`, the name of a marker trait of the prelude; the call type of `balance` would be named like my `Balance` (unsure what the macro does with either). | `send`, and `balance`. | small |
| 8 | `listed` | An input of a part has one type, so the cursor is a `String` on every list while the ids are typed. | `starting_after?: String`; the caller writes `.starting_after(last.id.as_str())`. | small |
| 9 | `by_object_id`, `List` | The pages show `Slot::set` and say that `get` exists, but not its form; they name `IntoItems` but not its method. | Both as I guessed them, marked (unsure). | small |
| 10 | What every request goes through | Stripe has nothing that must: the key is a value. The one process Stripe's libraries run on every request, telemetry, is one that a caller may turn off; a middleware of the description cannot be removed, and its expression sees only the required inputs. | No `middleware` in the description; `Telemetry` beside it, for `with_middleware`. | small |
| 11 | `customers.retrieve` | A deleted customer is a `200` with `deleted: true`, not a `404`. An arm cannot name the type of `body`: it comes from the expression. | `-> Option<Customer>` with `200 { body } => MaybeDeleted::live(body)`: the path of the function gives the type. | small |
| 12 | `events.list`, `balance_transactions.list`, `found` | `type` is a Rust keyword; Stripe's search parameter is `query`. | `kind?: String` with `"type" = kind;`; and `query { query; … }`, legal and odd to read. | small |
| 13 | `write` | Where to make the key: an entry runs once for a call (section 6.5), a middleware once for each try (section 6.2). A key made in a middleware would change between tries, and a payment could go through twice. It took three pages to be sure. | The key is made in the entry. | small |
| 14 | `listed` | A range of dates is a struct, refused in a query (rule 8). | Two flat inputs, `created_gte` and `created_lt`; `gt` and `lte` left out. | small |

What I wished I could write:

- **1:** a way for a part to add to the body of the node that names it, so that `part expanded(expand?: Vec<String>)` serves a `POST` too: `expand` written once, placed once.
- **2:** `.expand_customer()` on the call, giving a `PaymentIntent<Customer>`: a type chosen by an option.
- **3:** `version?: String = API_VERSION` in the signature, shown on the page (chapter 8, section 8.3: it can come later).

## Measures

| | |
|---|---|
| Endpoints | 40: customers 6, payment intents 8, refunds 5, subscriptions 5, invoices 10, balance 1, balance transactions 2, events 2, files 1. |
| Lines of the `api!` block | 258, from `concord::api! {` to its `}`, doc comments and blank lines included. |
| Names written a second time only to place them | 95 (3 in the client, 10 in the four parts, 82 in the endpoints). Of these, 24 are an `{id}` in a path and 20 are `expand` in the body of a `POST` (finding 1). Not counted: the 3 names in `paginate`, which name a cursor's place. |
| Inputs | 75 declared: 3 on the client, 11 in the four parts, 61 on the endpoints. As the callers see them, with each part counted on each node that names it: 160. |
| Rust beside the macro only because the language could not say something | 16 lines: `Expandable<T>` and its two methods (finding 2, 12 lines), and the lines that carry `expand` into the two body writers (finding 1, 4 lines). Not counted, because they are the doors the guide names: the two body writers, the pager, `IntoItems`, `MaybeDeleted::live`, `retry`, `Telemetry`. |

## Scores

- **Writing this description: 7/10.** The tree, the parts and the arms took Stripe almost line for line from its reference; `expand` and the form were the two places where I stopped to think.
- **Reading it back a month later: 7/10.** Each endpoint line is a line of Stripe's reference, but `expand` shows up in bodies from a part, and what a body sends lives in Rust types beside it.
- **The calling code: 8/10.** `.idempotency_key(…)`, `.expand([…])` and `.pages().items()` read like Stripe's own docs; a decline as `Ok(Err(…))` and `Expandable` each cost a `match`.
