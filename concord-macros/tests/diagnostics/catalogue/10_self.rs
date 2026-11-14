concord::api! {
client Tasks { base "https://a.example"; read Json; GET list() -> Vec<Self>; }
}
fn main() {}
