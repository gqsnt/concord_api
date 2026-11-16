concord::api! {
pub client Search {
        base "https://search.example.com";
        GET find() { query { "page" = 1; } }
    }
}
