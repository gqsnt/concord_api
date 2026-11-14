concord::api! {
    /// Stripe API v1. Every POST requires a durable, operation-specific key.
    pub client Stripe(
        key: Secret,
        /// Choose a version matching this crate's response models.
        version: String,
        /// Connect account; omitted for requests on the platform account.
        account?: AccountId,
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

            POST create(new: NewCustomer, key: IdempotencyKey, expand?: Vec<String>) -> Customer {
                headers { "Idempotency-Key" = key; }
                body stripe_form(new, expand);
                idempotent;
            }
            /// A deleted customer can be a tombstone; a 404 remains an error.
            GET get(id: CustomerId, expand?: Vec<String>) -> CustomerRecord {
                path "{id}";
                query { "expand[]" = expand; }
            }
            POST update(id: CustomerId, changes: UpdateCustomer, key: IdempotencyKey, expand?: Vec<String>) -> Customer {
                path "{id}";
                headers { "Idempotency-Key" = key; }
                body stripe_form(changes, expand);
                idempotent;
            }
            DELETE delete(id: CustomerId) -> Deleted<CustomerId> { path "{id}"; }
            GET list(email?: String, created_from?: i64, created_before?: i64, limit?: u32, starting_after?: CustomerId, expand?: Vec<String>) -> List<Customer> {
                query { email; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; "expand[]" = expand; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
            /// Search is eventually consistent; do not use it to verify a write.
            GET search(query: String, page?: String, limit?: u32, expand?: Vec<String>) -> SearchPage<Customer> {
                path "search";
                query { query; page; limit; "expand[]" = expand; }
                paginate |call, answer| search_next(&page, call, answer).map_err(concord::mismatch);
            }
        }

        scope payment_intents {
            path "v1/payment_intents";

            POST create(new: NewPaymentIntent, key: IdempotencyKey, expand?: Vec<String>) -> PaymentIntent {
                headers { "Idempotency-Key" = key; }
                body stripe_form(new, expand);
                idempotent;
            }
            GET get(id: PaymentIntentId, expand?: Vec<String>) -> PaymentIntent {
                path "{id}";
                query { "expand[]" = expand; }
            }
            POST update(id: PaymentIntentId, changes: UpdatePaymentIntent, key: IdempotencyKey, expand?: Vec<String>) -> PaymentIntent {
                path "{id}";
                headers { "Idempotency-Key" = key; }
                body stripe_form(changes, expand);
                idempotent;
            }
            /// Success can still require customer action; inspect status/next_action.
            POST confirm(id: PaymentIntentId, options: ConfirmPaymentIntent, key: IdempotencyKey, expand?: Vec<String>) -> PaymentIntent {
                path "{id}/confirm";
                headers { "Idempotency-Key" = key; }
                body stripe_form(options, expand);
                idempotent;
            }
            POST capture(id: PaymentIntentId, options: CapturePaymentIntent, key: IdempotencyKey, expand?: Vec<String>) -> PaymentIntent {
                path "{id}/capture";
                headers { "Idempotency-Key" = key; }
                body stripe_form(options, expand);
                idempotent;
            }
            POST cancel(id: PaymentIntentId, options: CancelPaymentIntent, key: IdempotencyKey, expand?: Vec<String>) -> PaymentIntent {
                path "{id}/cancel";
                headers { "Idempotency-Key" = key; }
                body stripe_form(options, expand);
                idempotent;
            }
            GET list(customer?: CustomerId, created_from?: i64, created_before?: i64, limit?: u32, starting_after?: PaymentIntentId, expand?: Vec<String>) -> List<PaymentIntent> {
                query { customer; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; "expand[]" = expand; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
            /// Search uses page tokens, not PaymentIntent IDs.
            GET search(query: String, page?: String, limit?: u32, expand?: Vec<String>) -> SearchPage<PaymentIntent> {
                path "search";
                query { query; page; limit; "expand[]" = expand; }
                paginate |call, answer| search_next(&page, call, answer).map_err(concord::mismatch);
            }
        }

        scope refunds {
            path "v1/refunds";

            POST create(new: NewRefund, key: IdempotencyKey, expand?: Vec<String>) -> Refund {
                headers { "Idempotency-Key" = key; }
                body stripe_form(new, expand);
                idempotent;
            }
            GET get(id: RefundId, expand?: Vec<String>) -> Refund {
                path "{id}";
                query { "expand[]" = expand; }
            }
            POST update(id: RefundId, changes: UpdateRefund, key: IdempotencyKey, expand?: Vec<String>) -> Refund {
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
            GET list(charge?: ChargeId, payment_intent?: PaymentIntentId, created_from?: i64, created_before?: i64, limit?: u32, starting_after?: RefundId, expand?: Vec<String>) -> List<Refund> {
                query { charge; payment_intent; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; "expand[]" = expand; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
        }

        scope subscriptions {
            path "v1/subscriptions";

            POST create(new: NewSubscription, key: IdempotencyKey, expand?: Vec<String>) -> Subscription {
                headers { "Idempotency-Key" = key; }
                body stripe_form(new, expand);
                idempotent;
            }
            GET get(id: SubscriptionId, expand?: Vec<String>) -> Subscription {
                path "{id}";
                query { "expand[]" = expand; }
            }
            POST update(id: SubscriptionId, changes: UpdateSubscription, key: IdempotencyKey, expand?: Vec<String>) -> Subscription {
                path "{id}";
                headers { "Idempotency-Key" = key; }
                body stripe_form(changes, expand);
                idempotent;
            }
            /// Cancels now. Use update(cancel_at_period_end) to schedule cancellation.
            DELETE cancel(id: SubscriptionId, invoice_now?: bool, prorate?: bool, expand?: Vec<String>) -> Subscription {
                path "{id}";
                query { invoice_now; prorate; "expand[]" = expand; }
            }
            /// None leaves Stripe's default; All and Ended are filter-only values.
            GET list(customer?: CustomerId, status?: SubscriptionStatusFilter, price?: PriceId, created_from?: i64, created_before?: i64, limit?: u32, starting_after?: SubscriptionId, expand?: Vec<String>) -> List<Subscription> {
                query { customer; status; price; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; "expand[]" = expand; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
        }

        scope invoices {
            path "v1/invoices";

            POST create(new: NewInvoice, key: IdempotencyKey, expand?: Vec<String>) -> Invoice {
                headers { "Idempotency-Key" = key; }
                body stripe_form(new, expand);
                idempotent;
            }
            GET get(id: InvoiceId, expand?: Vec<String>) -> Invoice {
                path "{id}";
                query { "expand[]" = expand; }
            }
            POST update(id: InvoiceId, changes: UpdateInvoice, key: IdempotencyKey, expand?: Vec<String>) -> Invoice {
                path "{id}";
                headers { "Idempotency-Key" = key; }
                body stripe_form(changes, expand);
                idempotent;
            }
            /// Only a deletable one-off draft; finalized invoices must be voided.
            DELETE delete(id: InvoiceId) -> Deleted<InvoiceId> { path "{id}"; }
            GET list(customer?: CustomerId, subscription?: SubscriptionId, status?: InvoiceStatusFilter, collection_method?: CollectionMethod, created_from?: i64, created_before?: i64, limit?: u32, starting_after?: InvoiceId, expand?: Vec<String>) -> List<Invoice> {
                query { customer; subscription; status; collection_method; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; "expand[]" = expand; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
            POST finalize(id: InvoiceId, options: FinalizeInvoice, key: IdempotencyKey, expand?: Vec<String>) -> Invoice {
                path "{id}/finalize";
                headers { "Idempotency-Key" = key; }
                body stripe_form(options, expand);
                idempotent;
            }
            POST pay(id: InvoiceId, options: PayInvoice, key: IdempotencyKey, expand?: Vec<String>) -> Invoice {
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

            GET get(id: BalanceTransactionId, expand?: Vec<String>) -> BalanceTransaction {
                path "{id}";
                query { "expand[]" = expand; }
            }
            GET list(currency?: Currency, source?: String, transaction_type?: String, created_from?: i64, created_before?: i64, limit?: u32, starting_after?: BalanceTransactionId, expand?: Vec<String>) -> List<BalanceTransaction> {
                query { currency; source; "type" = transaction_type; "created[gte]" = created_from; "created[lt]" = created_before; limit; starting_after; "expand[]" = expand; }
                paginate |call, answer| list_next(&starting_after, call, answer).map_err(concord::mismatch);
            }
        }

        scope events {
            path "v1/events";

            /// data.object retains its creation-time API version.
            GET get(id: EventId) -> Event { path "{id}"; }
            /// Stripe retains events for retrieval for up to 30 days.
            GET list(event_types?: Vec<String>, delivery_success?: bool, created_from?: i64, created_before?: i64, limit?: u32, starting_after?: EventId) -> List<Event> {
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
