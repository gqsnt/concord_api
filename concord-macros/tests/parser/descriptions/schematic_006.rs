pub client Stripe(key: Secret) {
    …
    part page(limit?: u32, starting_after?: String) {
        query { limit; starting_after; }
        paginate cursor(starting_after, List::next_cursor);
    }

    scope customers {
        path "v1/customers";
        GET list(email?: String, ..page) -> List<Customer> { query { email; } }
    }
}
