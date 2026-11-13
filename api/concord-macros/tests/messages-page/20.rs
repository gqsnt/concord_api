concord::api! {
client Tasks { base "https://a.example"; read Json; scope items { problem E; GET list(); } }
}
fn main() {}
