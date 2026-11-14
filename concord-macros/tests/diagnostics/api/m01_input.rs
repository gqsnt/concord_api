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
            GET list(customer?: CustomerId, price?: PriceId, status?: SubscriptionFilter, customer?: CustomerId, ..listed, ..expanded) -> List<Subscription> {
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
fn main() {}

include!("../../support/models.rs");
