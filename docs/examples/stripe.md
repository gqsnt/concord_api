# Stripe

*[Concord](../../README.md) · [docs](../README.md) · examples · previous: [an object store like Amazon S3](s3.md) · next: [Kubernetes](kubernetes.md)*

This page is for a Rust developer who calls the Stripe API. It gives a client with 38 endpoints that you can copy, and shows form bodies with nested keys, `expand`, an idempotency key on every `POST`, and a card decline as a value.

Here is Stripe as Concord sees it. There is one host, `api.stripe.com`, under `/v1`; files go to `files.stripe.com`. Authentication is a secret key, `authorization: Bearer sk_…`; the API version goes in `stripe-version`, and with Connect, the account goes in `stripe-account`. A `POST` sends a form with nested keys (`metadata[order]=7781`, `items[0][price]=…`): a body of entries, `body stripe_form { … }`, with the author's own writer that flattens them, and a part that adds `expand` to every one of these bodies. Every `POST` carries an idempotency key, so each one may be retried. Lists are paginated by the id of their last object, and searches by a token. Stripe rejects a request with `{"error": {"type", "code", "message", …}}`, read into `StripeProblem`; a declined card is a `402`, which the description turns into an `Err` in the endpoint's own return type. Stripe's own reference is [docs.stripe.com/api](https://docs.stripe.com/api). `(unsure)` marks a detail about Stripe that has not been checked.

## The description

```rust
use concord::{by_cursor, multipart, Json, Secret};

concord::api! {
    /// Stripe, API v1: customers, payments, refunds, subscriptions, invoices, the balance, events, files.
    pub client Stripe(
        /// A secret key (`sk_live_…`, `sk_test_…`) or a restricted one (`rk_…`).
        key: Secret,
        /// A connected account: the request is made as that account (Connect).
        account?: AccountId,
        /// The API version. Not set: `API_VERSION`, the one these types were written for.
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

        /// Every list: at most `limit` objects (1 to 100; Stripe returns 10), after or before an object.
        part listed(
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

        /// Every search: Stripe's query language, paginated by a token.
        /// Not to check a write: an object is found about a minute after it changed.
        part found(
            /// `email:'ada@example.com'`, `metadata['order']:'7781'`, `status:'succeeded'`.
            query: String,
            limit?: u32,
            /// The `next_page` of the page before.
            page?: String,
        ) {
            query { query; limit; page; }
            paginate by_cursor(page, SearchResult::next_token);
        }

        /// What a `GET` or a `DELETE` expands: in the query.
        part expanded(
            /// Ids in the response to return as whole objects: `customer`; in a list, `data.customer`.
            expand?: Vec<String>,
        ) {
            query { "expand[]" = expand; }
        }

        /// Every `POST`: a key, and what it expands, in its form body.
        part write(
            /// The same key returns the same response for at least 24 hours: send the operation again
            /// after a crash, and it is done once.
            idempotency_key: String,
            expand?: Vec<String>,
        ) {
            headers { "idempotency-key" = idempotency_key; }
            body stripe_form { expand; }
            idempotent;
        }

        scope customers {
            path "customers";

            POST create(
                email?: String, name?: String, description?: String,
                payment_method?: String, metadata?: Metadata, ..write,
            ) -> Customer {
                body stripe_form { email; name; description; payment_method; metadata; }
            }
            /// `None` when the customer was deleted: Stripe responds with `200` and `deleted: true`.
            /// An id that never existed is still a `404`.
            GET retrieve(id: CustomerId, ..expanded) -> Option<Customer> {
                path "{id}";
                status { 200 { body } => MaybeDeleted::live(body) }
            }
            POST update(
                id: CustomerId, email?: String, name?: String, description?: String,
                metadata?: Metadata, ..write,
            ) -> Customer {
                path "{id}";
                body stripe_form { email; name; description; metadata; }
            }
            DELETE delete(id: CustomerId) -> Deleted<CustomerId> { path "{id}"; }
            GET list(email?: String, ..listed, ..expanded) -> List<Customer> { query { email; } }
            GET search(..found, ..expanded) -> SearchResult<Customer> { path "search"; }
        }

        scope payment_intents {
            path "payment_intents";

            POST create(
                amount: u64, currency: Currency, customer?: CustomerId,
                payment_method?: String, confirm?: bool, metadata?: Metadata, ..write,
            ) -> PaymentIntent {
                body stripe_form { amount; currency; customer; payment_method; confirm; metadata; }
            }
            GET retrieve(id: PaymentIntentId, ..expanded) -> PaymentIntent { path "{id}"; }
            POST update(id: PaymentIntentId, amount?: u64, metadata?: Metadata, ..write) -> PaymentIntent {
                path "{id}";
                body stripe_form { amount; metadata; }
            }
            /// `Err` when the card was declined (`402`); the intent is in `error.payment_intent`.
            /// An `Ok` may still wait for the customer: look at `status` and `next_action`.
            POST confirm(
                id: PaymentIntentId, payment_method?: String, return_url?: String, ..write,
            ) -> Result<PaymentIntent, Declined> {
                path "{id}/confirm";
                body stripe_form { payment_method; return_url; }
                status { 402 { body } => Err(body) }
            }
            POST capture(id: PaymentIntentId, amount_to_capture?: u64, ..write) -> PaymentIntent {
                path "{id}/capture";
                body stripe_form { amount_to_capture; }
            }
            POST cancel(id: PaymentIntentId, cancellation_reason?: String, ..write) -> PaymentIntent {
                path "{id}/cancel";
                body stripe_form { cancellation_reason; }
            }
            GET list(customer?: CustomerId, ..listed, ..expanded) -> List<PaymentIntent> {
                query { customer; }
            }
            GET search(..found, ..expanded) -> SearchResult<PaymentIntent> { path "search"; }
        }

        scope refunds {
            path "refunds";

            /// Name the payment: `.payment_intent(…)` or `.charge(…)`.
            POST create(
                payment_intent?: PaymentIntentId, charge?: ChargeId, amount?: u64,
                reason?: String, metadata?: Metadata, ..write,
            ) -> Refund {
                body stripe_form { payment_intent; charge; amount; reason; metadata; }
            }
            GET retrieve(id: RefundId, ..expanded) -> Refund { path "{id}"; }
            /// Only a refund whose status is `requires_action`.
            POST cancel(id: RefundId, ..write) -> Refund { path "{id}/cancel"; }
            GET list(
                charge?: ChargeId, payment_intent?: PaymentIntentId, ..listed, ..expanded,
            ) -> List<Refund> {
                query { charge; payment_intent; }
            }
        }

        scope subscriptions {
            path "subscriptions";

            POST create(
                customer: CustomerId, items: Vec<NewItem>, default_payment_method?: String,
                cancel_at_period_end?: bool, metadata?: Metadata, ..write,
            ) -> Subscription {
                body stripe_form { customer; items; default_payment_method; cancel_at_period_end; metadata; }
            }
            GET retrieve(id: SubscriptionId, ..expanded) -> Subscription { path "{id}"; }
            POST update(
                id: SubscriptionId, items?: Vec<NewItem>, cancel_at_period_end?: bool,
                proration_behavior?: String, metadata?: Metadata, ..write,
            ) -> Subscription {
                path "{id}";
                body stripe_form { items; cancel_at_period_end; proration_behavior; metadata; }
            }
            /// Cancels now. To cancel at the end of the period: `update` with `cancel_at_period_end`.
            DELETE cancel(id: SubscriptionId, invoice_now?: bool, prorate?: bool, ..expanded) -> Subscription {
                path "{id}";
                query { invoice_now; prorate; }
            }
            /// With no `status`, every subscription that is not canceled.
            GET list(
                customer?: CustomerId, price?: PriceId, status?: SubscriptionFilter,
                ..listed, ..expanded,
            ) -> List<Subscription> {
                query { customer; price; status; }
            }
        }

        scope invoices {
            path "invoices";

            POST create(
                customer: CustomerId, subscription?: SubscriptionId, auto_advance?: bool,
                collection_method?: CollectionMethod, days_until_due?: u32, metadata?: Metadata,
                ..write,
            ) -> Invoice {
                body stripe_form {
                    customer; subscription; auto_advance; collection_method; days_until_due; metadata;
                }
            }
            GET retrieve(id: InvoiceId, ..expanded) -> Invoice { path "{id}"; }
            /// Only a draft. An invoice that was finalized is voided instead.
            DELETE delete(id: InvoiceId) -> Deleted<InvoiceId> { path "{id}"; }
            GET list(
                customer?: CustomerId, subscription?: SubscriptionId, status?: InvoiceStatus,
                collection_method?: CollectionMethod, ..listed, ..expanded,
            ) -> List<Invoice> {
                query { customer; subscription; status; collection_method; }
            }
            POST finalize(id: InvoiceId, auto_advance?: bool, ..write) -> Invoice {
                path "{id}/finalize";
                body stripe_form { auto_advance; }
            }
            /// `Err` when the payment was declined (`402`).
            POST pay(id: InvoiceId, payment_method?: String, ..write) -> Result<Invoice, Declined> {
                path "{id}/pay";
                body stripe_form { payment_method; }
                status { 402 { body } => Err(body) }
            }
            POST void(id: InvoiceId, ..write) -> Invoice { path "{id}/void"; }
            POST mark_uncollectible(id: InvoiceId, ..write) -> Invoice { path "{id}/mark_uncollectible"; }
            /// Emails the invoice to the customer.
            POST send(id: InvoiceId, ..write) -> Invoice { path "{id}/send"; }
        }

        /// What the account holds: available and pending, by currency.
        GET balance() -> Balance { path "balance"; }

        scope balance_transactions {
            path "balance_transactions";

            GET retrieve(id: BalanceTransactionId, ..expanded) -> BalanceTransaction { path "{id}"; }
            GET list(
                payout?: PayoutId, source?: String,
                /// `charge`, `refund`, `payout`, … (Stripe's `type`).
                kind?: String,
                currency?: Currency, ..listed, ..expanded,
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
                ..listed,
            ) -> List<Event> {
                query { "type" = kind; "types[]" = types; delivery_success; }
            }
        }

        /// A file, on the files host: dispute evidence, an identity document, a logo.
        /// The bytes are in memory, so a retry sends them again, with the same key.
        POST create_file(purpose: FilePurpose, file: Upload, idempotency_key: String) -> StripeFile {
            base "https://files.stripe.com";
            path "files";
            headers {
                override "authorization" = "Bearer {client.key}";
                "idempotency-key" = idempotency_key;
            }
            body multipart()
                .text("purpose", purpose.as_str())
                .file("file", &file.name, &file.content_type, file.bytes.clone())
                .finish();
            idempotent;
        }
    }
}
```

`path "v1"` is on the client, so the files host gets `/v1/files` too: the nearest enclosing `base` starts the URL, and every `path` above the endpoint follows it. A `DELETE` takes no idempotency key, because Stripe ignores one there. The method names are Stripe's (`retrieve`, not `get`).

## Beside the description

The data types are plain `serde` types, named here but not shown. Ids, each a `#[serde(transparent)]` `String` with `From<&str>` and `Display`: `AccountId`, `CustomerId`, `PaymentIntentId`, `RefundId`, `SubscriptionId`, `InvoiceId`, `BalanceTransactionId`, `EventId`, `ChargeId`, `PriceId`, `PayoutId`. The response types: `Customer`, `PaymentIntent` (its `customer` is an `Expandable<Customer>`), `Refund`, `Subscription`, `Invoice`, `Balance`, `BalanceTransaction`, `Event`, `StripeFile`, `Deleted<I>`. The request types and the filters: `Metadata` (a `BTreeMap<String, String>`), `NewItem`, `Currency`, `SubscriptionFilter`, `InvoiceStatus`, `CollectionMethod`, `FilePurpose`, `Upload { name, content_type, bytes }`. Errors: `StripeProblem` and `Declined`, each `{ error: StripeError }`.

The form writer, the pages, `expand`, a deleted customer, and Stripe's retry policy:

```rust
use concord::page::{Call, Slot};
use concord::{Answer, Body, Fail, Headers};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// The version these types were written for.
pub const API_VERSION: &str = "2026-10-28.endive";

/// `body stripe_form { … }`: Stripe's form, with nested keys: `metadata[order]=7781`,
/// `items[0][price]=price_1`, `expand[0]=customer`. A `None` is left out.
pub fn stripe_form<T: Serialize>(params: T) -> Result<Body, BoxError> {
    let mut pairs = Vec::new();
    if let Value::Object(fields) = serde_json::to_value(params)? {
        for (key, value) in fields {
            flatten(key, value, &mut pairs);
        }
    }
    Ok(concord::form(pairs)?)
}

fn flatten(key: String, value: Value, pairs: &mut Vec<(String, String)>) {
    match value {
        Value::Null => {}
        Value::Bool(b) => pairs.push((key, b.to_string())),
        Value::Number(n) => pairs.push((key, n.to_string())),
        Value::String(s) => pairs.push((key, s)),
        Value::Array(items) => {
            for (i, item) in items.into_iter().enumerate() {
                flatten(format!("{key}[{i}]"), item, pairs);
            }
        }
        Value::Object(fields) => {
            for (k, v) in fields {
                flatten(format!("{key}[{k}]"), v, pairs);
            }
        }
    }
}

/// A page of a list.
#[derive(Debug, Deserialize)]
pub struct List<T> {
    pub data: Vec<T>,
    pub has_more: bool,
}

/// A page of a search.
#[derive(Debug, Deserialize)]
pub struct SearchResult<T> {
    pub data: Vec<T>,
    pub has_more: bool,
    pub next_page: Option<String>,
}

impl<T> SearchResult<T> {
    /// For `by_cursor(page, …)`: the token of the next page, while Stripe says there is one.
    pub fn next_token(&self) -> Option<String> {
        self.next_page.clone().filter(|_| self.has_more)
    }
}

impl<T: Send> concord::page::IntoItems for List<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> {
        self.data
    }
    fn count(&self) -> usize {
        self.data.len()
    }
}

impl<T: Send> concord::page::IntoItems for SearchResult<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> {
        self.data
    }
    fn count(&self) -> usize {
        self.data.len()
    }
}

/// What the pager of the lists needs of an object: its id.
pub trait Object {
    fn id(&self) -> &str;
}

/// The next call, none at the end, or a Mismatch.
pub type NextPage<C> = Result<Option<C>, Fail<<C as Call>::Problem>>;

/// `paginate by_object_id(starting_after, ending_before);`. The next page starts after the last
/// object; when the caller went back with `ending_before`, it ends before the first one.
/// `has_more` with no object is not the end: the pages end with a Mismatch.
pub fn by_object_id<C, A, B, T>(after: A, before: B)
    -> impl Fn(C, &Answer<C::Value>) -> NextPage<C> + Send + 'static
where
    C: Call<Value = List<T>>,
    A: Slot<C, Value = String>,
    B: Slot<C, Value = String>,
    T: Object,
{
    move |call, answer| {
        let page = answer.value();
        if !page.has_more {
            return Ok(None);
        }
        let back = before.get(&call).is_some();
        let edge = if back { page.data.first() } else { page.data.last() };
        let Some(id) = edge.map(|object| object.id().to_owned()) else {
            return Err(concord::mismatch("has_more, and no object to go on from"));
        };
        Ok(Some(if back { before.set(call, id) } else { after.set(call, id) }))
    }
}

/// A field that `expand` turns from an id into the object: the return type does not
/// change with `.expand(…)`, so the field holds both shapes.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Expandable<T> {
    Id(String),
    Object(Box<T>),
}

/// A customer that was deleted is still found by its id: `{"id": …, "deleted": true}`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MaybeDeleted<T> {
    Deleted { id: String, deleted: bool },
    Live(T),
}

impl<T> MaybeDeleted<T> {
    /// For the arm of `customers.retrieve`: its path determines the type of `body`.
    pub fn live(self) -> Option<T> {
        match self {
            MaybeDeleted::Live(object) => Some(object),
            MaybeDeleted::Deleted { .. } => None,
        }
    }
}

/// Stripe's policy: its `Stripe-Should-Retry` header decides, and a `409` (one key, two
/// requests at the same time) is retried. Every `POST` here has a key, so each may be retried.
pub fn retry() -> stripe::Retry {
    stripe::Retry::standard()
        .unless(|tried| should_retry(tried.headers()) == Some(false))
        .when(|tried| {
            let asked = should_retry(tried.headers()) == Some(true);
            let busy = tried.status().is_some_and(|status| status == 409);
            (asked || busy).then(|| Duration::from_millis(500))
        })
}

fn should_retry(headers: Option<&Headers>) -> Option<bool> {
    match headers?.get("stripe-should-retry")? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}
```

`stripe_form` writes a list with indexes (`items[0][price]`, `expand[0]`), as Stripe's own libraries do (unsure: Stripe's examples also show `expand[]`).

## Calling it

```rust
use std::collections::BTreeMap;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A client, and a customer made once: the key is the order's, so after a crash the second
/// run gets the first customer back.
pub async fn customer_for_order() -> Result<Customer, BoxError> {
    let stripe = Stripe::new(std::env::var("STRIPE_SECRET_KEY")?)
        .with_retry(retry())
        .with_timeout(Duration::from_secs(80));
    let customer = stripe.customers().create("customer-for-order-7781")
        .email("ada@example.com")
        .metadata(BTreeMap::from([("order".to_owned(), "7781".to_owned())]))
        .await?;
    Ok(customer)
}

/// Confirm a payment. A decline is a value; any other rejection is an error.
pub async fn confirm(stripe: &Stripe, intent: &PaymentIntentId, card: &str) -> stripe::Result<()> {
    match stripe.payment_intents().confirm(intent, format!("confirm-{intent}"))
        .payment_method(card)
        .return_url("https://shop.example/back")
        .await?
    {
        Ok(intent) if intent.status == "requires_action" => println!("the customer must act"),
        Ok(intent) => println!("{} paid", intent.id),
        Err(declined) => println!("declined: {:?}", declined.error.decline_code),
    }
    Ok(())
}

/// Every open invoice of a customer, all pages, one invoice at a time.
pub async fn open_invoices(stripe: &Stripe, customer: &CustomerId) -> stripe::Result<()> {
    let mut open = stripe.invoices().list()
        .customer(customer)
        .status(InvoiceStatus::Open)
        .limit(100)
        .pages()
        .items();
    while let Some(invoice) = open.try_next().await? {
        println!("{} {} {}", invoice.id, invoice.amount_due, invoice.currency);
    }
    Ok(())
}

/// One page of a search, on purpose, from the token that the last run kept.
pub async fn one_page(stripe: &Stripe, token: Option<String>) -> stripe::Result<Option<String>> {
    let page = stripe.payment_intents()
        .search("status:'succeeded' AND metadata['order']:'7781'")
        .page(token)                                        // `None`: the first page
        .limit(50)
        .send()
        .await?;
    Ok(page.next_token())
}

/// Expand the customer of a payment; and a customer that may have been deleted.
pub async fn expand(stripe: &Stripe, intent: &PaymentIntentId, customer: &CustomerId) -> stripe::Result<()> {
    let intent = stripe.payment_intents().retrieve(intent).expand(["customer"]).await?;
    if let Some(Expandable::Object(customer)) = &intent.customer {
        println!("{:?}", customer.email);
    }
    match stripe.customers().retrieve(customer).await {
        Ok(Some(customer)) => println!("{}", customer.id),
        Ok(None) => println!("deleted"),
        Err(e) => {
            let code = e.problem().and_then(|p| p.error.code.clone());
            eprintln!("{e}; code {code:?}");               // `e.headers()` holds `request-id`
            return Err(e);
        }
    }
    Ok(())
}

/// A file for a dispute of a connected account, on the files host.
pub async fn evidence(platform_key: String, pdf: Vec<u8>) -> stripe::Result<StripeFile> {
    let shop = Stripe::new(platform_key).account(AccountId::from("acct_123"));
    let file = Upload { name: "receipt.pdf".into(), content_type: "application/pdf".into(), bytes: pdf.into() };
    shop.create_file(FilePurpose::DisputeEvidence, file, "evidence-for-dispute-42")
        .with_idle_timeout(Duration::from_secs(120))        // a body in memory is sent as one chunk
        .await
}
```

## What does not fit well, and the workaround

| Stripe wants | What to write |
|---|---|
| `expand`, which changes the shape of the response: an id becomes the object. The return type is fixed. | A field that holds both shapes, `Expandable<T>`, and a `match` at the call. |
| A form with nested keys. The `form` that Concord provides is flat. | Your own writer that flattens the value, `stripe_form`, named before a body of entries: the entries of the parts and those of the node are merged. |
| An API version that is always sent, with a default. A signature cannot declare a default. | The default in the entry, `version.as_deref().unwrap_or(API_VERSION)`, stated in the doc comment of the input. |
| A deleted customer: a `200` with `deleted: true`, not a `404`. | `-> Option<Customer>` with an arm that reads both shapes: `200 { body } => MaybeDeleted::live(body)`. |
| `type` and `query` as names: `type` is a Rust keyword. | `kind?: String` with `"type" = kind;`. `query` is an ordinary name: `query { query; }` is legal. |
| A date range, `created[gte]` and `created[lt]`: a struct is rejected in a query. | Two flat inputs, `created_gte` and `created_lt`. |
| A cursor typed as the id of each list. But an input of a part has one type for every node. | `starting_after?: String`; the caller writes `.starting_after(last.id.0.as_str())`. |
| Telemetry that Stripe's libraries send on each request, and that a caller may turn off. A middleware layer declared in the description cannot be removed. | Your own middleware layer, added by the caller with `with_middleware(…)` (see [When Concord does not cover what you need](../guide/extending.md)). |
