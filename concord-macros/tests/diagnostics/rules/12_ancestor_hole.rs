concord::api! {
    client Workshop(region: String) { base "https://regions.test"; GET fetch() { path "{region}"; } }
}
fn main() {}
