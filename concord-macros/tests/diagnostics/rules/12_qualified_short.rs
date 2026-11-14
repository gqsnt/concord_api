concord::api! {
    client Workshop(region: String) { base "https://regions.test"; GET fetch() { query { client.region; } } }
}
fn main() {}
