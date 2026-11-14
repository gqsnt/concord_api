concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


GET list(limit?: u32, starting_after?: String) -> List<Customer> {
    query { limit; starting_after; }
    paginate move |call, page| page.value().next_cursor().map(|cursor| call.starting_after(cursor));
}


} }