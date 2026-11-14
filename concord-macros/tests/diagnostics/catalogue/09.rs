concord::api! {
client Tasks { base "https://a.example"; GET list(count: u32); }
}
fn main() {}
