concord::api! {
client Tasks { base "https://a.example"; GET list(r#type: String) { query { r#type; } } }
}
fn main() {}
