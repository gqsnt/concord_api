concord::api! {
client Tasks { base "https://a.example"; GET get() { path "a/../b"; } }
}
fn main() {}
