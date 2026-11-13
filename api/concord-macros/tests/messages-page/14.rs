concord::api! {
client Tasks { base "https://a.example"; GET get(#[serde(rename = "id")] item_id: String) { query { item_id; } } }
}
fn main() {}
