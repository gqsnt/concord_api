concord::api! {
client Tasks { base "https://a.example"; GET get(id: &str) { path "{id}"; } }
}
fn main() {}
