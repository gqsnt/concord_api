concord::api! {
client Tasks { base "https://a.example"; GET get(id: String) { path "{idd}"; } }
}
fn main() {}
