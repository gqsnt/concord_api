concord::api! {
client Tasks { base "https://a.example"; GET get(id?: String) { path "{id}"; } }
}
fn main() {}
