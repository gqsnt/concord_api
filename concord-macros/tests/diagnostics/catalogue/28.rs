concord::api! {
client Tasks { base "https://a.example"; GET get() { path "items?count=5"; } }
}
fn main() {}
