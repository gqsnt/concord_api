concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


part page(limit?: u32, starting_after?: String) {
        query { limit; starting_after; }
        paginate by_cursor(starting_after, List::next_cursor);
    }

GET fixture(..page) -> List<Customer>;
} }