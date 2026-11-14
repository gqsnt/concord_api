pub client Stripe(key: Secret) {
    base "https://api.stripe.com";
    headers { "authorization" = "Bearer {key}"; }
    read Json;

    part page(limit?: u32, starting_after?: String) {
        query { limit; starting_after; }
        paginate by_cursor(starting_after, List::next_cursor);
    }

    scope customers {
        path "v1/customers";
        GET list(email?: String, ..page) -> List<Customer> { query { email; } }
    }

    scope invoices {
        path "v1/invoices";
        GET list(..page) -> List<Invoice>;
    }
}
