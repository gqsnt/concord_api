concord::api! { client Leak(key:String) { base "https://api.example.com"; headers { "client"=key; } POST get() { body concord::json { "endpoint"=key.len().to_string(); } } } }
fn main() {}
