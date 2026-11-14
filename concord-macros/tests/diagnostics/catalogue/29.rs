concord::api! {
client Tasks { base "https://a.example"; GET get(key: String) { path "files/{*key}.json"; } }
}
fn main() {}
