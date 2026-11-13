concord::api! { client Leak(key: String) { base "https://api.example.com";
headers { "client" = key; }
GET get() { headers { "endpoint" = key.len().to_string(); } } } }
fn main() {}
