concord::api! {
client Tasks { base "https://a.example"; GET get() { status { remove 404 } } }
}
fn main() {}
