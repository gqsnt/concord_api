concord::api! { client Leak(key: String) { base "https://api.example.com";
query { "client" = key; }
GET get() { query { "endpoint" = key.len().to_string(); } } } }
fn main() {}
