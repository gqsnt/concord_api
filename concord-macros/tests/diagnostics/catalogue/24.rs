concord::api! {
client Tasks { base "https://a.example"; GET get() { override path "items"; } }
}
fn main() {}
