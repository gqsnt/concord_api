concord::api! {
client Tasks(tenant: String) { base "https://a.example"; GET get() { path "{tenant}"; } }
}
fn main() {}
