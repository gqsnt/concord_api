concord::api! {
    client Workshop(region: String) { base "https://regions.test"; GET fetch() { query { region; } } }
}
fn main() {}
