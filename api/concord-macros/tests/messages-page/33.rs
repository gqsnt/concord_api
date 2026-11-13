concord::api! {
client Tasks { base "https://a.example"; GET list() -> Vec<Task>; }
}
fn main() {}
