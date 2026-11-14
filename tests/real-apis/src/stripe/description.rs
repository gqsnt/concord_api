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

        /// Every list: at most `limit` objects (1 to 100; Stripe gives 10), after or before an object.
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

        /// Every search: Stripe's query language, in pages by a token.
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
            /// Ids in the answer to give as whole objects: `customer`; in a list, `data.customer`.
            expand?: Vec<String>,
        ) {
            query { "expand[]" = expand; }
        }

        /// Every `POST`: a key, and what it expands, in its form body.
        part write(
            /// The same key gives the same answer for at least 24 hours: send the operation again
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
            /// `None` when the customer was deleted: Stripe answers `200` with `deleted: true`.
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
        /// The bytes are in memory, so another try sends them again, with the same key.
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
