concord::api! { client D { base "https://d.example"; GET g() { query { "e" = Self::__ENDPOINT; } } } }
fn main() {}
