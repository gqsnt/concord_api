concord::api! {
pub client Stripe(key: Secret) {
    base "https://api.stripe.com";
    headers { "authorization" = "Bearer {key}"; }
    read Json;
    problem StripeProblem;

    part page(limit?: u32, starting_after?: String) {
        query { limit; starting_after; }
        paginate by_cursor(starting_after, List::next_cursor);
    }
    part keyed(idempotency_key: String) {
        headers { "idempotency-key" = idempotency_key; }
        idempotent;
    }

    scope customers {
        path "v1/customers";
        GET list(email?: String, ..page) -> List<Customer> { query { email; } }
        POST create(customer: NewCustomer, ..keyed) -> Customer { body form(customer); }
    }
}

}